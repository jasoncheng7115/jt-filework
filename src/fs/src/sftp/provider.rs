//! The `Provider` implementation for SFTP.
//!
//! Stage one of ADR-0004: listing and navigating. Nothing here writes.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, UNIX_EPOCH};

use jtf_core::{
    Error, ErrorCode, FileEntry, FileKind, Location, PermissionsSummary, RawName, Result,
    Timestamps,
};
use jtf_jobs::CancellationToken;

use super::connect::{Connection, Endpoint, RemoteEntry, UnknownHostPolicy};
use crate::provider::{Batch, EnumerationHandle, Provider};

/// How many rows to send at once.
///
/// Larger than the local provider's: every batch crosses a channel, but a
/// remote listing arrives all at once from the server anyway, so batching is
/// about not flooding the UI rather than about latency.
const BATCH: usize = 256;

/// Lists directories on hosts reached over SFTP.
///
/// Connections are kept and reused: opening one costs a TCP handshake, a key
/// exchange and an authentication round trip, and doing that per directory
/// would make walking a tree unusable.
#[derive(Default)]
/// Cloning shares the pool.
///
/// A clone is another handle on the same connections, accepted hosts and
/// pending passwords - which is the point: a background job that needs to list
/// a server has to reuse the session the pane already signed in on, not open a
/// second one and ask for the password again.
#[derive(Clone)]
pub struct SftpProvider {
    /// Shared with the worker threads that open and use the connections.
    ///
    /// Behind an `Arc` because connecting happens on a worker - it takes as
    /// long as the network takes, and doing it on the calling thread froze the
    /// window - and that worker has to reach the same pool, the same accepted
    /// hosts and the same pending password as the provider it came from.
    state: Arc<ProviderState>,
}

#[derive(Default)]
struct ProviderState {
    connections: Mutex<HashMap<Endpoint, Arc<Connection>>>,
    /// Hosts the user has agreed to trust in this session. Not persisted;
    /// `known_hosts` is where an answer that outlives the session goes.
    accepted: Mutex<Vec<Endpoint>>,
    /// A password given for connecting to an endpoint, held in memory for as
    /// long as this process runs and dropped when the user disconnects.
    ///
    /// Not part of the saved connection and never written anywhere
    /// (`docs/adr/0004-sftp.md`).
    pending_password: Mutex<HashMap<Endpoint, String>>,
}

impl std::fmt::Debug for SftpProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SftpProvider")
    }
}

impl SftpProvider {
    /// A provider with no open connections.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Hold a password for the next connection to `endpoint`, and no longer.
    ///
    /// Taken out of the map when it is used, so the program stops holding it
    /// the moment it has served its purpose.
    pub fn set_password(&self, endpoint: Endpoint, password: String) {
        if let Ok(mut pending) = self.state.pending_password.lock() {
            pending.insert(endpoint, password);
        }
    }

    /// Record that the user accepted this host's key, so the next attempt
    /// writes it to `known_hosts` rather than refusing again.
    pub fn accept_host(&self, endpoint: Endpoint) {
        if let Ok(mut accepted) = self.state.accepted.lock() {
            if !accepted.contains(&endpoint) {
                accepted.push(endpoint);
            }
        }
    }

    /// Whether there is a live session for this endpoint.
    ///
    /// "Live" means a connection object exists, which is what the sidebar
    /// needs to know: whether clicking it will be instant or will open a
    /// connection. It is not a health check - a server that has gone away
    /// still looks connected until something is asked of it, and finding that
    /// out costs a round trip nobody asked for.
    pub fn is_connected(&self, endpoint: &Endpoint) -> bool {
        self.state
            .connections
            .lock()
            .is_ok_and(|open| open.contains_key(endpoint))
    }

    /// Close one connection, and forget how it signed in.
    pub fn disconnect(&self, endpoint: &Endpoint) {
        // The password is kept for reconnects, so disconnecting on purpose is
        // what ends it: "log me out" has to mean the secret is gone too.
        if let Ok(mut pending) = self.state.pending_password.lock() {
            pending.remove(endpoint);
        }
        if let Ok(mut open) = self.state.connections.lock() {
            open.remove(endpoint);
        }
    }

    /// Close every connection. Used when the user disconnects, and on quit.
    pub fn disconnect_all(&self) {
        if let Ok(mut pending) = self.state.pending_password.lock() {
            pending.clear();
        }
        if let Ok(mut open) = self.state.connections.lock() {
            open.clear();
        }
    }

    /// An open connection for `endpoint`, opening one if needed.
    fn connection(&self, endpoint: &Endpoint) -> Result<Arc<Connection>> {
        Self::connect(&self.state, endpoint)
    }

    /// The same, over the shared state alone, so a worker thread can call it.
    fn connect(state: &ProviderState, endpoint: &Endpoint) -> Result<Arc<Connection>> {
        if let Ok(open) = state.connections.lock() {
            if let Some(existing) = open.get(endpoint) {
                return Ok(Arc::clone(existing));
            }
        }
        // A poisoned lock means refuse: the safe direction when we cannot
        // tell whether the user has agreed to this host.
        let policy = state
            .accepted
            .lock()
            .map_or(UnknownHostPolicy::Refuse, |accepted| {
                if accepted.contains(endpoint) {
                    UnknownHostPolicy::AcceptAndRemember
                } else {
                    UnknownHostPolicy::Refuse
                }
            });

        // Read, and kept for as long as this process runs.
        //
        // It was taken on first use, which made a password server work exactly
        // once. A dropped connection, a `disconnect`, or simply the next
        // launch restoring a remote tab all landed on a `connection()` with no
        // password, the server refused it, and the refusal arrived as
        // `PermissionDenied` - so the pane said 「你沒有執行這項操作的權限」 for a
        // folder the account could read perfectly well.
        //
        // Held in memory only, for this process, and never written: the
        // session file has no field for it and `disconnect` drops it. That is
        // a deliberate change to what ADR-0004 promised - "used for that one
        // connection" - because one connection is not enough to browse with,
        // and re-typing it per reconnect is not a security property, only an
        // annoyance. What has not changed is that it never reaches the disk.
        let password = state
            .pending_password
            .lock()
            .ok()
            .and_then(|pending| pending.get(endpoint).cloned());
        let opened = Arc::new(Connection::open(
            endpoint.clone(),
            policy,
            password.as_deref(),
        )?);
        if let Ok(mut open) = state.connections.lock() {
            open.insert(endpoint.clone(), Arc::clone(&opened));
        }
        Ok(opened)
    }
}

/// Turn what the server said into what the rest of the program understands.
fn to_entry(parent: &Location, row: &RemoteEntry) -> FileEntry {
    let (host, port, user, base) = match parent {
        Location::Remote {
            host,
            port,
            user,
            path,
        } => (host.clone(), *port, user.clone(), path.clone()),
        _ => (String::new(), 22, String::new(), String::from("/")),
    };
    let joined = if base.ends_with('/') {
        format!("{base}{}", row.name)
    } else {
        format!("{base}/{}", row.name)
    };

    // A symlink is reported as a symlink even when it points at a directory:
    // the same rule the local provider follows, and the reason a recursive
    // delete never walks through one.
    let kind = if row.is_symlink {
        FileKind::Symlink
    } else if row.is_dir {
        FileKind::Directory
    } else {
        FileKind::File
    };

    let mut entry = FileEntry::new(
        Location::remote(host, port, user, joined),
        RawName::new(row.name.clone()),
        kind,
    )
    .with_size(row.size);

    if let Some(seconds) = row.modified {
        entry = entry.with_timestamps(Timestamps {
            modified: UNIX_EPOCH.checked_add(Duration::from_secs(seconds)),
            ..Timestamps::default()
        });
    }
    if let Some(mode) = row.permissions {
        entry = entry.with_permissions(PermissionsSummary {
            readable: mode & 0o400 != 0,
            writable: mode & 0o200 != 0,
            executable: mode & 0o100 != 0,
        });
    }
    entry
}

impl Provider for SftpProvider {
    fn handles(&self, location: &Location) -> bool {
        matches!(location, Location::Remote { .. })
    }

    fn list(&self, location: &Location, cancel: &CancellationToken) -> Result<Vec<FileEntry>> {
        if cancel.is_cancelled() {
            return Err(Error::bare(ErrorCode::Cancelled));
        }
        let Some(endpoint) = Endpoint::of(location) else {
            return Err(Error::new(ErrorCode::Unsupported, "not a remote location"));
        };
        let path = location.remote_path().unwrap_or("/").to_string();
        let connection = self.connection(&endpoint)?;
        let rows = connection.read_dir(&path)?;
        Ok(rows.iter().map(|row| to_entry(location, row)).collect())
    }

    fn enumerate_async(&self, location: &Location) -> Result<EnumerationHandle> {
        let Some(endpoint) = Endpoint::of(location) else {
            return Err(Error::new(ErrorCode::Unsupported, "not a remote location"));
        };
        // Connecting happens on the worker, not here.
        //
        // It used to happen here, so that a refused host key or a bad address
        // came back as an error immediately rather than as a `Batch::Failed`
        // the caller had to wait for. But "here" is the UI thread, and opening
        // a connection takes as long as it takes - up to the 20-second connect
        // timeout when the host is simply down. Clicking a saved server froze
        // the whole window until it gave up, which is the thing
        // `AGENTS.md` §3 exists to forbid; the error arriving a moment later
        // through the channel that already carries failures is a much smaller
        // price than a window that stops repainting.
        let state = Arc::clone(&self.state);
        let path = location.remote_path().unwrap_or("/").to_string();
        let here = location.clone();

        let (token, canceller) = CancellationToken::new();
        let (sender, receiver) = std::sync::mpsc::channel();
        let join = std::thread::Builder::new()
            .name("jtf-sftp".to_string())
            .spawn(move || {
                if token.is_cancelled() {
                    return;
                }
                let connection = match Self::connect(&state, &endpoint) {
                    Ok(connection) => connection,
                    Err(error) => {
                        let _ = sender.send(Batch::Failed(error));
                        return;
                    }
                };
                match connection.read_dir(&path) {
                    Err(error) => {
                        let _ = sender.send(Batch::Failed(error));
                    }
                    Ok(rows) => {
                        let mut sent = 0usize;
                        for chunk in rows.chunks(BATCH) {
                            if token.is_cancelled() {
                                return;
                            }
                            let batch: Vec<FileEntry> =
                                chunk.iter().map(|row| to_entry(&here, row)).collect();
                            sent += batch.len();
                            if sender.send(Batch::Rows(batch)).is_err() {
                                return; // nobody is listening any more
                            }
                        }
                        let _ = sender.send(Batch::Done { total: sent });
                    }
                }
            })
            .map_err(|e| Error::new(ErrorCode::Io, format!("spawn: {e}")))?;

        Ok(EnumerationHandle::new(canceller, receiver, join))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(name: &str, dir: bool) -> RemoteEntry {
        RemoteEntry {
            name: name.to_string(),
            is_dir: dir,
            is_symlink: false,
            size: 12,
            modified: Some(1_700_000_000),
            permissions: Some(0o644),
        }
    }

    /// A failed connection must not eat the password.
    ///
    /// There is no server here, so `connection` cannot succeed - which is
    /// exactly the case that used to consume the password anyway. Every later
    /// attempt then went out with none, the server refused it, and the refusal
    /// surfaced as "you do not have permission" for a folder the user could
    /// read.
    #[test]
    fn a_password_survives_a_connection_that_failed() {
        let provider = SftpProvider::new();
        // Port 1 with nothing listening: the attempt fails immediately.
        let endpoint = Endpoint {
            host: "127.0.0.1".to_string(),
            port: 1,
            user: "nobody".to_string(),
        };
        provider.set_password(endpoint.clone(), "hunter2".to_string());

        assert!(
            provider.connection(&endpoint).is_err(),
            "nothing is listening on port 1; this attempt must fail"
        );

        let still_there = provider
            .state
            .pending_password
            .lock()
            .is_ok_and(|pending| pending.contains_key(&endpoint));
        assert!(
            still_there,
            "the password is for the next attempt, not spent on the one that failed"
        );
    }

    /// Asking to list a remote folder must return at once.
    ///
    /// Connecting is what takes time - up to the connect timeout when the host
    /// is simply down - and it used to happen on the calling thread, which is
    /// the UI thread. Clicking a saved server whose machine was off froze the
    /// window for twenty seconds and then showed the error. The failure now
    /// arrives through the channel that already carries failures, and the call
    /// itself is immediate.
    ///
    /// Port 1 on localhost: nothing listens there, so the attempt fails
    /// without needing a network or a server.
    #[test]
    fn listing_a_remote_folder_does_not_block_the_caller() {
        let provider = SftpProvider::new();
        let location = Location::remote("127.0.0.1", 1, "nobody", "/");

        let started = std::time::Instant::now();
        let handle = provider.enumerate_async(&location);
        let elapsed = started.elapsed();

        assert!(
            handle.is_ok(),
            "the request is accepted; whether it connects is the worker's news"
        );
        assert!(
            elapsed < std::time::Duration::from_millis(500),
            "enumerate_async returned in {elapsed:?}; it must not wait for the connection"
        );
    }

    /// Disconnecting forgets the password, so "log out" means it.
    ///
    /// The secret is kept across reconnects - a password server is unusable
    /// otherwise - which makes the deliberate disconnect the point where it
    /// has to go.
    #[test]
    fn disconnecting_forgets_the_password() {
        let provider = SftpProvider::new();
        let endpoint = Endpoint {
            host: "127.0.0.1".to_string(),
            port: 1,
            user: "nobody".to_string(),
        };
        provider.set_password(endpoint.clone(), "hunter2".to_string());

        provider.disconnect(&endpoint);

        let gone = provider
            .state
            .pending_password
            .lock()
            .is_ok_and(|pending| !pending.contains_key(&endpoint));
        assert!(gone, "disconnecting drops the secret with the session");
    }

    #[test]
    fn disconnecting_everything_forgets_every_password() {
        let provider = SftpProvider::new();
        let endpoint = Endpoint {
            host: "127.0.0.1".to_string(),
            port: 1,
            user: "nobody".to_string(),
        };
        provider.set_password(endpoint.clone(), "hunter2".to_string());

        provider.disconnect_all();

        let empty = provider
            .state
            .pending_password
            .lock()
            .is_ok_and(|pending| pending.is_empty());
        assert!(empty);
    }

    #[test]
    fn a_row_becomes_an_entry_under_the_folder_it_came_from() {
        let here = Location::remote("host", 22, "jt", "/srv/data");
        let entry = to_entry(&here, &row("report.pdf", false));
        assert_eq!(
            entry.location(),
            &Location::remote("host", 22, "jt", "/srv/data/report.pdf")
        );
        assert_eq!(entry.kind(), FileKind::File);
        assert_eq!(entry.size(), Some(12));
    }

    #[test]
    fn the_remote_root_does_not_produce_a_double_slash() {
        // `/` + `/etc` would be `//etc`, which some servers accept and some
        // do not - and which no user would recognise as the path they are in.
        let root = Location::remote("host", 22, "jt", "/");
        let entry = to_entry(&root, &row("etc", true));
        assert_eq!(
            entry.location(),
            &Location::remote("host", 22, "jt", "/etc")
        );
    }

    #[test]
    fn a_symlink_stays_a_symlink_even_when_it_points_at_a_directory() {
        let here = Location::remote("host", 22, "jt", "/srv");
        let mut link = row("current", true);
        link.is_symlink = true;
        assert_eq!(to_entry(&here, &link).kind(), FileKind::Symlink);
    }

    #[test]
    fn the_provider_claims_remote_locations_and_nothing_else() {
        let provider = SftpProvider::new();
        assert!(provider.handles(&Location::remote("host", 22, "jt", "/")));
        assert!(!provider.handles(&Location::local("/tmp")));
        assert!(!provider.handles(&Location::virtual_location("search", "1")));
    }
}
