//! Opening an SFTP session, and the decisions that come with it.
//!
//! Everything here runs on a worker thread with its own Tokio runtime. The
//! network never reaches the UI thread (`AGENTS.md` §3), and a server that
//! stops answering stalls one worker rather than the program.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use jtf_core::{Error, ErrorCode, Result};
use russh::client;
use russh::keys::{ssh_key, PrivateKeyWithHashAlg};
use russh_sftp::client::SftpSession;

use super::host_keys::{self, HostKeyVerdict};

/// How long to wait for a server that is not answering.
///
/// Long enough for a slow link, short enough that a wrong address reports
/// rather than appearing to hang.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(20);

/// How long any single request may take before it is given up on.
///
/// A file manager may not hang. Whatever the cause - a server that stops
/// answering, a stalled link, or the transfer defect described on
/// [`Connection::upload`] - the user gets an error and their window back
/// rather than a spinner that never stops.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// How much of a file to move at a time.
///
/// Large enough that the per-packet overhead does not dominate, small enough
/// that progress moves and a cancellation is noticed promptly.
const TRANSFER_CHUNK: usize = 16 * 1024;

/// Which host, as which user. Not a credential — see the module docs.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Endpoint {
    /// Hostname or address as the user typed it.
    pub host: String,
    /// Port, normally 22.
    pub port: u16,
    /// Account on that host.
    pub user: String,
}

impl Endpoint {
    /// The endpoint a remote location names, if it is one.
    #[must_use]
    pub fn of(location: &jtf_core::Location) -> Option<Self> {
        match location {
            jtf_core::Location::Remote {
                host, port, user, ..
            } => Some(Self {
                host: host.clone(),
                port: *port,
                user: user.clone(),
            }),
            _ => None,
        }
    }
}

/// What to do about a host key this program has not seen before.
///
/// The decision belongs to the user, so it is passed in rather than taken
/// here. `Refuse` is the default a caller gets by doing nothing, which is the
/// safe direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnknownHostPolicy {
    /// Refuse, and report the fingerprint so the caller can ask.
    Refuse,
    /// Accept and append to `known_hosts`. Only after the user has said yes.
    AcceptAndRemember,
}

/// The verdict a connection attempt reached about the server's identity.
///
/// Kept so the caller can tell "wrong password" from "this is not the machine
/// you connected to last time", which are not the same problem at all.
#[derive(Debug, Clone)]
pub struct HostKeyOutcome {
    /// What the check concluded.
    pub verdict: HostKeyVerdict,
    /// The key's algorithm, for writing a `known_hosts` line.
    pub algorithm: String,
    /// The key itself, base64 as `known_hosts` stores it.
    pub key_base64: String,
}

/// Verifies the server against `known_hosts` and records what it found.
struct Verifier {
    endpoint: Endpoint,
    policy: UnknownHostPolicy,
    outcome: Arc<Mutex<Option<HostKeyOutcome>>>,
}

impl client::Handler for Verifier {
    type Error = russh::Error;

    // Not async in body: the check is a file read and a comparison. The
    // trait's signature is async, so the attribute says so rather than
    // pretending there is work to await.
    #[allow(
        clippy::unused_async_trait_impl,
        reason = "the trait declares it async; the check is a file read and a comparison"
    )]
    async fn check_server_key(
        &mut self,
        offered: &russh::keys::PublicKeyOrCertificate,
    ) -> std::result::Result<bool, Self::Error> {
        // A certificate is a different trust model - it is vouched for by a
        // CA rather than pinned in known_hosts - and this program has no way
        // to check that chain. Refusing is the honest answer; pretending to
        // have verified it would be worse than not offering the feature.
        let russh::keys::PublicKeyOrCertificate::PublicKey { key, .. } = offered else {
            return Ok(false);
        };
        let algorithm = key.algorithm().to_string();
        let key_base64 = key.to_openssh().unwrap_or_default();
        // `to_openssh` gives "algo base64 comment"; known_hosts wants the
        // middle field on its own.
        let key_base64 = key_base64
            .split_whitespace()
            .nth(1)
            .unwrap_or_default()
            .to_string();

        let contents = host_keys::known_hosts_path()
            .and_then(|path| std::fs::read_to_string(path).ok())
            .unwrap_or_default();
        let verdict = host_keys::verify(
            &contents,
            &self.endpoint.host,
            self.endpoint.port,
            &key_base64,
        );

        // The two refusals below say `false` for opposite reasons and are
        // deliberately not merged: "this is not the machine you trusted" can
        // never be waved through, while "I have not met this host" is a
        // question the user may answer yes to. Collapsing them would make the
        // first one look like a policy choice.
        #[allow(
            clippy::match_same_arms,
            reason = "distinct reasons, coincidentally the same answer"
        )]
        let accept = match (&verdict, self.policy) {
            (HostKeyVerdict::Known, _) => true,
            (HostKeyVerdict::Changed { .. }, _) => false,
            (HostKeyVerdict::Unknown { .. }, UnknownHostPolicy::AcceptAndRemember) => {
                remember(&self.endpoint, &algorithm, &key_base64);
                true
            }
            (HostKeyVerdict::Unknown { .. }, UnknownHostPolicy::Refuse) => false,
        };

        if let Ok(mut slot) = self.outcome.lock() {
            *slot = Some(HostKeyOutcome {
                verdict,
                algorithm,
                key_base64,
            });
        }
        Ok(accept)
    }
}

/// Append an accepted host to `known_hosts`, creating it if needed.
fn remember(endpoint: &Endpoint, algorithm: &str, key_base64: &str) {
    use std::io::Write;

    let Some(path) = host_keys::known_hosts_path() else {
        return;
    };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    {
        let _ = file.write_all(
            host_keys::entry_line(&endpoint.host, endpoint.port, algorithm, key_base64).as_bytes(),
        );
    }
}

/// The private keys to try, in the order `ssh` would.
fn candidate_keys() -> Vec<PathBuf> {
    let Some(home) = dirs::home_dir() else {
        return Vec::new();
    };
    let ssh = home.join(".ssh");
    ["id_ed25519", "id_ecdsa", "id_rsa"]
        .iter()
        .map(|name| ssh.join(name))
        .filter(|path| path.exists())
        .collect()
}

/// An open SFTP session, plus the runtime it lives on.
///
/// The runtime is owned here so that dropping the connection shuts down its
/// threads: a session nobody is using must not keep a socket open.
pub struct Connection {
    runtime: tokio::runtime::Runtime,
    sftp: SftpSession,
    /// The SSH connection under the SFTP session. Kept for its own sake -
    /// dropping it closes the transport out from under the channel - and
    /// used to open a channel of its own for each transfer.
    handle: client::Handle<Verifier>,
    endpoint: Endpoint,
}

impl std::fmt::Debug for Connection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Connection")
            .field("endpoint", &self.endpoint)
            .finish_non_exhaustive()
    }
}

impl Connection {
    /// Which host this is.
    #[must_use]
    pub const fn endpoint(&self) -> &Endpoint {
        &self.endpoint
    }

    /// Open a session, authenticating by agent, key file, or a password the
    /// caller was given for this one attempt.
    ///
    /// The password is borrowed, used, and never stored: it is not written to
    /// the session file, not kept on the connection, and not logged. A server
    /// that only accepts passwords is common enough that refusing to talk to
    /// one would make the feature useless, but remembering the password is a
    /// different decision and this program does not make it.
    ///
    /// # Errors
    ///
    /// [`ErrorCode::PermissionDenied`] when the server refuses every key, and
    /// [`ErrorCode::Io`] for anything that stopped the connection being made
    /// — including a host key that is not the one on file, which is reported
    /// with both fingerprints rather than as a generic failure.
    pub fn open(
        endpoint: Endpoint,
        policy: UnknownHostPolicy,
        password: Option<&str>,
    ) -> Result<Self> {
        // Two worker threads, not a current-thread runtime. The SSH session
        // and the SFTP subsystem each keep a task alive that has to service
        // the socket; on a current-thread runtime those only run while
        // something is inside `block_on`, and a transfer that awaits a reply
        // the reader task was supposed to deliver waits for a task that
        // cannot be scheduled. A one-chunk write got away with it; the second
        // chunk hung.
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .thread_name("jtf-sftp-io")
            .build()
            .map_err(|e| Error::new(ErrorCode::Io, format!("runtime: {e}")))?;

        let outcome = Arc::new(Mutex::new(None));
        let verifier = Verifier {
            endpoint: endpoint.clone(),
            policy,
            outcome: Arc::clone(&outcome),
        };

        let config = Arc::new(client::Config::default());
        let address = (endpoint.host.clone(), endpoint.port);
        let user = endpoint.user.clone();
        let password = password.map(str::to_owned);

        let (handle, sftp) = runtime
            .block_on(async move {
                let mut handle = tokio::time::timeout(
                    CONNECT_TIMEOUT,
                    client::connect(config, address, verifier),
                )
                .await
                .map_err(|_| Error::new(ErrorCode::Io, "timed out connecting"))?
                .map_err(|e| Error::new(ErrorCode::Io, format!("connect: {e}")))?;

                authenticate(&mut handle, &user, password.as_deref()).await?;

                let channel = handle
                    .channel_open_session()
                    .await
                    .map_err(|e| Error::new(ErrorCode::Io, format!("open session: {e}")))?;
                channel
                    .request_subsystem(true, "sftp")
                    .await
                    .map_err(|e| Error::new(ErrorCode::Io, format!("request sftp: {e}")))?;
                let sftp = SftpSession::new(channel.into_stream())
                    .await
                    .map_err(|e| Error::new(ErrorCode::Io, format!("sftp: {e}")))?;
                Ok::<_, Error>((handle, sftp))
            })
            .map_err(|error| enrich(error, &outcome))?;

        Ok(Self {
            runtime,
            sftp,
            handle,
            endpoint,
        })
    }

    /// List a directory.
    ///
    /// # Errors
    ///
    /// Whatever the server reports, mapped to an [`ErrorCode`].
    pub fn read_dir(&self, path: &str) -> Result<Vec<RemoteEntry>> {
        self.runtime.block_on(async {
            let dir = self
                .sftp
                .read_dir(path.to_string())
                .await
                .map_err(|e| map_sftp(&e))?;
            let mut rows = Vec::new();
            for entry in dir {
                let metadata = entry.metadata();
                rows.push(RemoteEntry {
                    name: entry.file_name(),
                    is_dir: metadata.is_dir(),
                    is_symlink: metadata.is_symlink(),
                    size: metadata.size.unwrap_or(0),
                    modified: metadata.mtime.map(u64::from),
                    permissions: metadata.permissions,
                });
            }
            Ok(rows)
        })
    }

    /// Create a directory on the server.
    ///
    /// # Errors
    ///
    /// Whatever the server reports — most often that it already exists, or
    /// that the account may not write there.
    pub fn create_dir(&self, path: &str) -> Result<()> {
        self.runtime
            .block_on(async { self.sftp.create_dir(path.to_string()).await })
            .map_err(|e| map_sftp(&e))
    }

    /// Remove a file. Never a directory: a call that silently did both is how
    /// a mistyped path takes a tree with it.
    ///
    /// # Errors
    ///
    /// Whatever the server reports.
    pub fn remove_file(&self, path: &str) -> Result<()> {
        self.runtime
            .block_on(async { self.sftp.remove_file(path.to_string()).await })
            .map_err(|e| map_sftp(&e))
    }

    /// Remove an empty directory.
    ///
    /// Only an empty one — SFTP has no recursive remove, and building one
    /// here would mean this layer deciding to delete things it was not asked
    /// about. Walking the tree is the caller's job, where it can be planned,
    /// shown and cancelled like any other operation.
    ///
    /// # Errors
    ///
    /// Whatever the server reports, including that it is not empty.
    pub fn remove_dir(&self, path: &str) -> Result<()> {
        self.runtime
            .block_on(async { self.sftp.remove_dir(path.to_string()).await })
            .map_err(|e| map_sftp(&e))
    }

    /// Rename, which on one server is also how a file is moved.
    ///
    /// # Errors
    ///
    /// Whatever the server reports. Many refuse when the destination exists,
    /// which is the safe behaviour and is left as it is.
    pub fn rename(&self, from: &str, to: &str) -> Result<()> {
        self.runtime
            .block_on(async { self.sftp.rename(from.to_string(), to.to_string()).await })
            .map_err(|e| map_sftp(&e))
    }

    /// Read a whole remote file.
    ///
    /// For small files — a preview, a text view. A transfer that needs
    /// progress and cancellation streams instead; see `download`.
    ///
    /// # Errors
    ///
    /// Whatever the server reports.
    pub fn read_file(&self, path: &str) -> Result<Vec<u8>> {
        use tokio::io::AsyncReadExt;
        self.runtime.block_on(async {
            let mut file = self
                .sftp
                .open(path.to_string())
                .await
                .map_err(|e| map_sftp(&e))?;
            let mut bytes = Vec::new();
            file.read_to_end(&mut bytes)
                .await
                .map_err(|e| Error::new(ErrorCode::Io, format!("read: {e}")))?;
            Ok(bytes)
        })
    }

    /// A fresh SFTP subsystem on the same SSH connection.
    ///
    /// Transfers get their own. The long-lived session is fine for listing
    /// and for single-packet writes, but a *second* multi-packet file on it
    /// hangs waiting for an acknowledgement that never arrives: the first
    /// such file goes through, the next one stops. A channel of its own per
    /// transfer keeps that state from carrying over, and costs one channel
    /// open rather than a whole connection.
    async fn transfer_session(&self) -> Result<SftpSession> {
        let channel = self
            .handle
            .channel_open_session()
            .await
            .map_err(|e| Error::new(ErrorCode::Io, format!("open session: {e}")))?;
        channel
            .request_subsystem(true, "sftp")
            .await
            .map_err(|e| Error::new(ErrorCode::Io, format!("request sftp: {e}")))?;
        SftpSession::new(channel.into_stream())
            .await
            .map_err(|e| Error::new(ErrorCode::Io, format!("sftp: {e}")))
    }

    /// Copy a remote file to a local path, reporting progress as it goes.
    ///
    /// `progress` is called with the bytes written so far; returning `false`
    /// from it stops the transfer, and the partial file is removed — a
    /// cancelled download must not leave something that looks complete.
    ///
    /// # Errors
    ///
    /// Whatever the server or the local filesystem reports.
    pub fn download(
        &self,
        remote: &str,
        local: &std::path::Path,
        mut progress: impl FnMut(u64) -> bool,
    ) -> Result<u64> {
        use std::io::Write;
        use tokio::io::AsyncReadExt;

        let mut sink = std::fs::File::create(local)
            .map_err(|e| Error::new(ErrorCode::Io, format!("create {}: {e}", local.display())))?;

        let outcome = self.runtime.block_on(async {
            tokio::time::timeout(REQUEST_TIMEOUT, async {
                let session = self.transfer_session().await?;
                let mut file = session
                    .open(remote.to_string())
                    .await
                    .map_err(|e| map_sftp(&e))?;
                let mut buffer = vec![0u8; TRANSFER_CHUNK];
                let mut written = 0u64;
                loop {
                    let read = file
                        .read(&mut buffer)
                        .await
                        .map_err(|e| Error::new(ErrorCode::Io, format!("read: {e}")))?;
                    if read == 0 {
                        break;
                    }
                    sink.write_all(&buffer[..read])
                        .map_err(|e| Error::new(ErrorCode::Io, format!("write: {e}")))?;
                    written += read as u64;
                    if !progress(written) {
                        return Err(Error::bare(ErrorCode::Cancelled));
                    }
                }
                drop(file);
                let _ = session.close().await;
                Ok(written)
            })
            .await
            .unwrap_or_else(|_| Err(Error::new(ErrorCode::Io, "the server stopped answering")))
        });

        match outcome {
            Ok(written) => Ok(written),
            Err(error) => {
                // A half-written file with the right name is worse than none:
                // it looks like the transfer succeeded.
                drop(sink);
                let _ = std::fs::remove_file(local);
                Err(error)
            }
        }
    }

    /// Copy a local file to the server, reporting progress as it goes.
    ///
    /// Same contract as [`download`](Self::download), including removing the
    /// partial file on the far side when it is stopped.
    ///
    /// # Known defect
    ///
    /// Against the server this was developed on (a Windows SFTP server),
    /// transfers stop responding once roughly 64 KB has been written across
    /// the connection: three 20 KB uploads succeed and the fourth stops. It
    /// reproduces exactly, and it is not the chunk size, the runtime, the
    /// number of writes in flight, or reusing one SFTP channel - a fresh
    /// channel per transfer behaves the same, which is what rules out the
    /// per-channel window on its own. It is recorded here rather than worked
    /// around by guessing, and [`REQUEST_TIMEOUT`] makes it an error rather
    /// than a hang. `docs/TESTING.md` §5.3 has the reproduction.
    ///
    /// # Errors
    ///
    /// Whatever the server or the local filesystem reports.
    pub fn upload(
        &self,
        local: &std::path::Path,
        remote: &str,
        mut progress: impl FnMut(u64) -> bool,
    ) -> Result<u64> {
        use tokio::io::AsyncWriteExt;

        let source = std::fs::read(local)
            .map_err(|e| Error::new(ErrorCode::Io, format!("read {}: {e}", local.display())))?;

        let outcome = self.runtime.block_on(async {
            tokio::time::timeout(REQUEST_TIMEOUT, async {
                let session = self.transfer_session().await?;
                let mut file = session
                    .create(remote.to_string())
                    .await
                    .map_err(|e| map_sftp(&e))?;
                let mut written = 0u64;
                for chunk in source.chunks(TRANSFER_CHUNK) {
                    file.write_all(chunk)
                        .await
                        .map_err(|e| Error::new(ErrorCode::Io, format!("write: {e}")))?;
                    // Waited for here, chunk by chunk, rather than letting the
                    // acknowledgements pile up to be drained at the end.
                    //
                    // `poll_write` in russh-sftp queues a write and returns
                    // immediately, keeping a receiver for the server's reply; the
                    // close then waits for every one of them. Against the server
                    // this was tested on, a single outstanding write is answered
                    // and two are not - so a 32 KB file closed fine and a 40 KB
                    // file hung forever on the close. One at a time is slower and
                    // it finishes, which is the trade a file manager should make.
                    // It also makes the progress figure honest: it now counts
                    // bytes the server has confirmed, not bytes handed to a queue.
                    file.flush()
                        .await
                        .map_err(|e| Error::new(ErrorCode::Io, format!("flush: {e}")))?;
                    written += chunk.len() as u64;
                    if !progress(written) {
                        // Cleaned up here, on the session that created it and
                        // while that session is still open. Removing it after
                        // `block_on` has returned uses the long-lived session
                        // instead, and races the close of a handle the transfer
                        // session has not finished letting go of - which left the
                        // partial file sitting there.
                        let _ = file.shutdown().await;
                        drop(file);
                        let _ = session.remove_file(remote.to_string()).await;
                        return Err(Error::bare(ErrorCode::Cancelled));
                    }
                }
                file.shutdown()
                    .await
                    .map_err(|e| Error::new(ErrorCode::Io, format!("close: {e}")))?;
                drop(file);
                let _ = session.close().await;
                Ok(written)
            })
            .await
            .unwrap_or_else(|_| Err(Error::new(ErrorCode::Io, "the server stopped answering")))
        });

        match outcome {
            Ok(written) => Ok(written),
            Err(error) => {
                // A failure that was not a cancellation may also have left
                // something behind; the cancellation path has already
                // cleaned up, and removing an absent file is harmless.
                let _ = self.remove_file(remote);
                Err(error)
            }
        }
    }

    /// Resolve a path to its absolute form on the server.
    ///
    /// # Errors
    ///
    /// Whatever the server reports.
    pub fn canonicalize(&self, path: &str) -> Result<String> {
        self.runtime
            .block_on(async { self.sftp.canonicalize(path.to_string()).await })
            .map_err(|e| map_sftp(&e))
    }
}

/// A row as the server describes it, before it becomes a `FileEntry`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteEntry {
    /// The name, which is the server's and may be anything.
    pub name: String,
    /// Whether it is a directory.
    pub is_dir: bool,
    /// Whether it is a symbolic link.
    pub is_symlink: bool,
    /// Size in bytes, or 0 when the server did not say.
    pub size: u64,
    /// Modification time as a Unix timestamp, when the server gave one.
    pub modified: Option<u64>,
    /// POSIX mode bits, when the server gave them.
    pub permissions: Option<u32>,
}

/// Try the agent first, then key files, then the password if one was given.
///
/// That order on purpose: the agent and a key file prove who you are without
/// the secret ever crossing the wire, and a password should be the last
/// resort rather than the first thing offered.
async fn authenticate(
    handle: &mut client::Handle<Verifier>,
    user: &str,
    password: Option<&str>,
) -> Result<()> {
    // The agent is what a user who is already working over SSH has running,
    // and it is the only path that never touches a key file this program
    // would then be holding in memory.
    //
    // Unix only. `connect_env` reads `SSH_AUTH_SOCK` and opens a Unix socket;
    // Windows keeps its agent behind a named pipe and russh offers no
    // equivalent, so a Windows build falls through to the key files below
    // rather than pretending to have looked. Wiring up the named pipe is its
    // own piece of work and belongs with the Windows platform adapter.
    #[cfg(unix)]
    if let Ok(mut agent) = russh::keys::agent::client::AgentClient::connect_env().await {
        if let Ok(identities) = agent.request_identities().await {
            for identity in identities {
                // Only plain keys: a certificate from the agent would be
                // offered to a server this program cannot verify a CA for.
                let russh::keys::agent::AgentIdentity::PublicKey { key, .. } = identity else {
                    continue;
                };
                let attempt = handle
                    .authenticate_publickey_with(
                        user,
                        key,
                        Some(ssh_key::HashAlg::Sha256),
                        &mut agent,
                    )
                    .await;
                if matches!(attempt, Ok(ref result) if result.success()) {
                    return Ok(());
                }
            }
        }
    }

    for path in candidate_keys() {
        // An encrypted key without a passphrase simply fails here and the
        // next candidate is tried; this program does not prompt for a
        // passphrase, because that is what the agent is for.
        let Ok(key) = russh::keys::load_secret_key(&path, None) else {
            continue;
        };
        let attempt = handle
            .authenticate_publickey(
                user,
                PrivateKeyWithHashAlg::new(Arc::new(key), Some(ssh_key::HashAlg::Sha256)),
            )
            .await;
        if matches!(attempt, Ok(ref result) if result.success()) {
            return Ok(());
        }
    }

    if let Some(secret) = password {
        let attempt = handle.authenticate_password(user, secret).await;
        if matches!(attempt, Ok(ref result) if result.success()) {
            return Ok(());
        }
        return Err(Error::new(
            ErrorCode::PermissionDenied,
            "the server refused that password",
        ));
    }

    Err(Error::new(
        ErrorCode::PermissionDenied,
        "no key was accepted; add one to your agent or to ~/.ssh, or give a password",
    ))
}

/// Turn a connection failure into one that says *why* where we know.
fn enrich(error: Error, outcome: &Arc<Mutex<Option<HostKeyOutcome>>>) -> Error {
    let Ok(slot) = outcome.lock() else {
        return error;
    };
    match slot.as_ref().map(|o| &o.verdict) {
        Some(HostKeyVerdict::Changed { expected, offered }) => Error::new(
            ErrorCode::PermissionDenied,
            format!(
                "the host key changed: expected {expected}, the server offered {offered}. \
                 Not connecting"
            ),
        ),
        Some(HostKeyVerdict::Unknown { fingerprint }) => Error::new(
            ErrorCode::PermissionDenied,
            format!("unknown host key {fingerprint}"),
        ),
        _ => error,
    }
}

fn map_sftp(error: &russh_sftp::client::error::Error) -> Error {
    let text = error.to_string();
    let code = if text.contains("permission") || text.contains("Permission") {
        ErrorCode::PermissionDenied
    } else if text.contains("no such") || text.contains("No such") {
        ErrorCode::NotFound
    } else {
        ErrorCode::Io
    };
    Error::new(code, text)
}
