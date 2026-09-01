//! A real SFTP session against a real server.
//!
//! Skipped unless `JTF_SFTP_HOST` is set, because a test that needs a network
//! and an account cannot run in a checkout that has neither. `docs/TESTING.md`
//! records how to run it.
//!
//! `expect` is right here: a test that cannot reach the server it was told to
//! reach has failed, and the panic message names which step gave up.
//!
//! What this proves that the unit tests cannot: that the key exchange, the
//! authentication, the subsystem request and the directory listing all work
//! against a server we did not write.

#![allow(clippy::expect_used, reason = "a test that cannot connect has failed")]

use jtf_fs::sftp::{Connection, Endpoint, UnknownHostPolicy};

/// The endpoint under test, or `None` when the environment does not describe
/// one.
fn endpoint() -> Option<(Endpoint, Option<String>)> {
    let host = std::env::var("JTF_SFTP_HOST").ok()?;
    let user = std::env::var("JTF_SFTP_USER").unwrap_or_else(|_| "root".to_string());
    let port = std::env::var("JTF_SFTP_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(22);
    let password = std::env::var("JTF_SFTP_PASSWORD").ok();
    Some((Endpoint { host, port, user }, password))
}

#[test]
fn a_real_server_lists_a_real_directory() {
    let Some((endpoint, password)) = endpoint() else {
        eprintln!("skipped: set JTF_SFTP_HOST to run this");
        return;
    };

    let connection = Connection::open(
        endpoint.clone(),
        // The test host is one the operator gave us on purpose; accepting its
        // key on first sight is what a person would do here, and it is
        // written to known_hosts so the next run is a plain match.
        UnknownHostPolicy::AcceptAndRemember,
        password.as_deref(),
    )
    .expect("the server accepted the connection");

    let path = std::env::var("JTF_SFTP_PATH").unwrap_or_else(|_| "/".to_string());
    let rows = connection.read_dir(&path).expect("listed the directory");
    assert!(
        !rows.is_empty(),
        "a real server's {path} should not be empty"
    );

    // The shape of what came back, not just that something did.
    let named: Vec<&str> = rows.iter().map(|r| r.name.as_str()).collect();
    assert!(
        !named.iter().any(|n| n.is_empty()),
        "every row has a name: {named:?}"
    );
    assert!(
        rows.iter().any(|r| r.is_dir),
        "the root of a unix host has directories in it: {named:?}"
    );

    // And that the same connection can be used again - the pooling in
    // SftpProvider depends on it.
    let again = connection.read_dir(&path).expect("listed a second time");
    assert_eq!(
        again.len(),
        rows.len(),
        "the same directory listed twice on one connection"
    );
}

#[test]
fn canonicalize_resolves_a_relative_path_on_the_server() {
    let Some((endpoint, password)) = endpoint() else {
        eprintln!("skipped: set JTF_SFTP_HOST to run this");
        return;
    };
    let connection = Connection::open(
        endpoint,
        UnknownHostPolicy::AcceptAndRemember,
        password.as_deref(),
    )
    .expect("connected");

    // `.` is the login directory, and the server is the only thing that knows
    // where that is - which is why this is asked rather than assumed.
    let home = connection.canonicalize(".").expect("resolved");
    assert!(
        home.starts_with('/'),
        "the server returns an absolute path, got {home:?}"
    );
}

#[test]
fn what_the_server_returned_is_printed_for_a_human_to_recognise() {
    let Some((endpoint, password)) = endpoint() else {
        eprintln!("skipped: set JTF_SFTP_HOST to run this");
        return;
    };
    let connection = Connection::open(
        endpoint,
        UnknownHostPolicy::AcceptAndRemember,
        password.as_deref(),
    )
    .expect("connected");
    let rows = connection.read_dir("/").expect("listed /");
    for row in rows.iter().take(20) {
        eprintln!(
            "{:<24} dir={} link={} size={:>10} mode={:o}",
            row.name,
            u8::from(row.is_dir),
            u8::from(row.is_symlink),
            row.size,
            row.permissions.unwrap_or(0)
        );
    }
}

/// A directory of our own on the server, removed however the test ends.
///
/// Every write test works inside one of these. Nothing outside it is created,
/// renamed or removed: a test suite that can damage the machine it is pointed
/// at is one nobody will run against anything real.
struct Scratch {
    connection: Connection,
    path: String,
}

impl Scratch {
    fn open() -> Option<Self> {
        // Per instance, not per process: these tests run in parallel and a
        // name built from the pid alone had them all claiming one directory -
        // the first won, the rest failed to create it, and whichever finished
        // first deleted it out from under the others.
        static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let serial = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let (endpoint, password) = endpoint()?;
        let connection = Connection::open(
            endpoint,
            UnknownHostPolicy::AcceptAndRemember,
            password.as_deref(),
        )
        .expect("connected");

        // Under the login directory, so the account is certain to be allowed
        // to write, and named after the process so two runs cannot collide.
        let home = connection.canonicalize(".").expect("resolved home");
        let base = home.trim_end_matches('/').to_string();
        let path = format!("{base}/jtf-selftest-{}-{serial}", std::process::id());
        connection.create_dir(&path).expect("made a scratch folder");
        Some(Self { connection, path })
    }

    fn join(&self, name: &str) -> String {
        format!("{}/{}", self.path, name)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        // Best effort, and only inside our own directory.
        if let Ok(rows) = self.connection.read_dir(&self.path) {
            for row in rows {
                let _ = self.connection.remove_file(&self.join(&row.name));
            }
        }
        let _ = self.connection.remove_dir(&self.path);
    }
}

#[test]
fn a_file_uploads_downloads_renames_and_deletes() {
    let Some(scratch) = Scratch::open() else {
        eprintln!("skipped: set JTF_SFTP_HOST to run this");
        return;
    };
    let connection = &scratch.connection;

    // Several chunks' worth, so the loop, the progress callback and the
    // server's own buffering are all exercised rather than a single write
    // that happens to fit.
    let body: Vec<u8> = (0..500_000u32).map(|i| (i % 251) as u8).collect();
    let local = std::env::temp_dir().join(format!("jtf-upload-{}", std::process::id()));
    std::fs::write(&local, &body).expect("wrote the local source");

    let remote = scratch.join("payload.bin");
    let mut seen = Vec::new();
    let sent = connection
        .upload(&local, &remote, |so_far| {
            seen.push(so_far);
            true
        })
        .expect("uploaded");
    assert_eq!(sent, body.len() as u64);
    assert!(
        seen.windows(2).all(|w| w[0] < w[1]),
        "progress only ever moves forward: {seen:?}"
    );

    // The server agrees about the size.
    let listed = connection.read_dir(&scratch.path).expect("listed");
    let row = listed
        .iter()
        .find(|r| r.name == "payload.bin")
        .expect("the uploaded file is there");
    assert_eq!(row.size, body.len() as u64);
    assert!(!row.is_dir);

    // And what comes back is byte for byte what went up.
    let back = std::env::temp_dir().join(format!("jtf-download-{}", std::process::id()));
    let got = connection
        .download(&remote, &back, |_| true)
        .expect("downloaded");
    assert_eq!(got, body.len() as u64);
    assert_eq!(
        std::fs::read(&back).expect("read the download"),
        body,
        "a round trip changed the bytes"
    );

    // Rename, and the old name is gone rather than both existing.
    let renamed = scratch.join("renamed.bin");
    connection.rename(&remote, &renamed).expect("renamed");
    let after = connection.read_dir(&scratch.path).expect("listed again");
    let names: Vec<&str> = after.iter().map(|r| r.name.as_str()).collect();
    assert!(names.contains(&"renamed.bin"), "{names:?}");
    assert!(!names.contains(&"payload.bin"), "{names:?}");

    connection.remove_file(&renamed).expect("removed");
    let empty = connection
        .read_dir(&scratch.path)
        .expect("listed once more");
    assert!(empty.is_empty(), "the folder is empty again: {empty:?}");

    let _ = std::fs::remove_file(&local);
    let _ = std::fs::remove_file(&back);
}

#[test]
fn a_cancelled_upload_leaves_nothing_behind() {
    let Some(scratch) = Scratch::open() else {
        eprintln!("skipped: set JTF_SFTP_HOST to run this");
        return;
    };
    let connection = &scratch.connection;

    let body = vec![7u8; 500_000];
    let local = std::env::temp_dir().join(format!("jtf-cancel-{}", std::process::id()));
    std::fs::write(&local, &body).expect("wrote the local source");

    let remote = scratch.join("cancelled.bin");
    // Stop after the first chunk. A partial file with the right name is worse
    // than no file: it looks like the transfer worked.
    let result = connection.upload(&local, &remote, |_| false);
    assert!(result.is_err(), "a stopped upload reports that it stopped");

    let listed = connection.read_dir(&scratch.path).expect("listed");
    assert!(
        !listed.iter().any(|r| r.name == "cancelled.bin"),
        "the partial file was removed: {:?}",
        listed.iter().map(|r| &r.name).collect::<Vec<_>>()
    );

    let _ = std::fs::remove_file(&local);
}

#[test]
fn a_directory_is_created_and_removed_but_never_recursively() {
    let Some(scratch) = Scratch::open() else {
        eprintln!("skipped: set JTF_SFTP_HOST to run this");
        return;
    };
    let connection = &scratch.connection;

    let dir = scratch.join("sub");
    connection.create_dir(&dir).expect("created");
    let inner = format!("{dir}/inside.txt");
    let local = std::env::temp_dir().join(format!("jtf-inner-{}", std::process::id()));
    std::fs::write(&local, b"x").expect("wrote");
    connection
        .upload(&local, &inner, |_| true)
        .expect("uploaded");

    // A non-empty directory must not be removed. SFTP has no recursive
    // remove and this layer does not invent one: walking the tree is the
    // caller's job, where it can be planned and shown.
    assert!(
        connection.remove_dir(&dir).is_err(),
        "removing a non-empty directory is refused"
    );

    connection.remove_file(&inner).expect("removed the file");
    connection.remove_dir(&dir).expect("now the directory goes");

    let _ = std::fs::remove_file(&local);
}

#[test]
fn a_run_of_transfers_finishes_rather_than_hanging() {
    // Written while a faulty network cable was stalling every transfer past
    // ~64 KB, to prove the timeout turned that into a reported error instead
    // of a hung window. The cable is replaced and these all pass now, which
    // is why the assertion is that the run *ends* and that any failure is
    // reported - it holds whether the network is healthy or not, and it is
    // the property that actually matters.
    let Some(scratch) = Scratch::open() else {
        eprintln!("skipped: set JTF_SFTP_HOST to run this");
        return;
    };
    let connection = &scratch.connection;
    let local = std::env::temp_dir().join(format!("jtf-stall-{}", std::process::id()));
    std::fs::write(&local, vec![9u8; 200_000]).expect("wrote");

    let started = std::time::Instant::now();
    let mut failures = 0;
    for i in 0..5 {
        let remote = scratch.join(&format!("stall{i}.bin"));
        match connection.upload(&local, &remote, |_| true) {
            Ok(sent) => assert_eq!(sent, 200_000),
            Err(error) => {
                failures += 1;
                eprintln!("upload {i} reported: {error}");
            }
        }
        let _ = connection.remove_file(&remote);
    }

    assert!(
        started.elapsed() < std::time::Duration::from_secs(180),
        "five small uploads finished, one way or the other, in {:?}",
        started.elapsed()
    );
    eprintln!("{failures} of 5 uploads reported an error");
    let _ = std::fs::remove_file(&local);
}

/// A host already in `known_hosts` connects under the policy the application
/// actually uses.
///
/// Every other test here connects with `AcceptAndRemember`, which succeeds
/// whether or not the key was already known - so the path the program takes
/// when the user has *not* ticked "trust this host" was never exercised. That
/// is the path that reports 「你沒有執行這項操作的權限」, because a refused host
/// key is reported as `PermissionDenied`.
#[test]
fn a_known_host_connects_without_being_accepted_again() {
    let Some((endpoint, password)) = endpoint() else {
        eprintln!("skipped: set JTF_SFTP_HOST to run this");
        return;
    };

    let connection = Connection::open(endpoint, UnknownHostPolicy::Refuse, password.as_deref())
        .expect("a host already in known_hosts connects without being accepted again");
    let path = std::env::var("JTF_SFTP_PATH").unwrap_or_else(|_| "/".to_string());
    let rows = connection.read_dir(&path).expect("listed the directory");
    assert!(!rows.is_empty(), "{path} should not be empty");
}
