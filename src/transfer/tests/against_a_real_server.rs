//! The whole scenario, against a real SFTP server.
//!
//! Everything else in this crate is checked against values, which proves the
//! arithmetic and not the thing. These move real bytes over a real connection:
//! download, upload, recursion into folders, the conflict policies, a move
//! that is a copy and a delete, a move within one server that is a rename, a
//! recursive remote delete, and what is left behind when a transfer stops.
//!
//! Skipped unless `JTF_SFTP_TEST` names a server, because a test that needs
//! one would otherwise fail on every machine that has not got one:
//!
//! ```text
//! JTF_SFTP_TEST=user@host:22 cargo test -p jtf-transfer --test against_a_real_server
//! ```
//!
//! The account must be reachable by key — from the agent or `~/.ssh` — since
//! nothing here can answer a password prompt. It writes only under a
//! directory it creates itself and removes that directory afterwards.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::{Path, PathBuf};

use jtf_fs::sftp::{Endpoint, SftpProvider};
use jtf_jobs::CancellationToken;
use jtf_transfer::run::{Policy, Silent};
use jtf_transfer::{Item, Kind, Plan, Side};

/// The server under test, or `None` to skip.
fn endpoint() -> Option<Endpoint> {
    let spec = std::env::var("JTF_SFTP_TEST").ok()?;
    let (user, rest) = spec.split_once('@')?;
    let (host, port) = rest
        .split_once(':')
        .map_or((rest, 22), |(h, p)| (h, p.parse().unwrap_or(22)));
    Some(Endpoint {
        host: host.to_string(),
        port,
        user: user.to_string(),
    })
}

struct Fixture {
    sftp: SftpProvider,
    endpoint: Endpoint,
    /// A directory on the server, made for this run.
    remote_root: String,
    /// A directory here, made for this run.
    local_root: PathBuf,
}

impl Fixture {
    fn new(name: &str) -> Option<Self> {
        let endpoint = endpoint()?;
        let sftp = SftpProvider::new();
        // The host is trusted for this run only; nothing is written to
        // known_hosts by saying so here.
        sftp.accept_host(endpoint.clone());

        let connection = sftp
            .connection_for(&endpoint)
            .expect("could not reach the server named by JTF_SFTP_TEST");

        let remote_root = format!("/tmp/jtf-transfer-{}-{name}", std::process::id());
        let _ = connection.remove_dir(&remote_root);
        connection
            .create_dir(&remote_root)
            .expect("could not create the remote scratch directory");

        let local_root =
            std::env::temp_dir().join(format!("jtf-transfer-local-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&local_root);
        std::fs::create_dir_all(&local_root).unwrap();

        Some(Self {
            sftp,
            endpoint,
            remote_root,
            local_root,
        })
    }

    fn remote(&self, rest: &str) -> Side {
        Side::Remote {
            endpoint: self.endpoint.clone(),
            path: if rest.is_empty() {
                self.remote_root.clone()
            } else {
                format!("{}/{rest}", self.remote_root)
            },
        }
    }

    fn local(&self, rest: &str) -> Side {
        Side::Local(if rest.is_empty() {
            self.local_root.clone()
        } else {
            self.local_root.join(rest)
        })
    }

    /// Put a file on the server by writing it here and uploading it.
    fn put(&self, rest: &str, bytes: &[u8]) {
        let staging = self.local_root.join("__staging");
        std::fs::write(&staging, bytes).unwrap();
        let connection = self.sftp.connection_for(&self.endpoint).unwrap();
        connection
            .upload(&staging, &format!("{}/{rest}", self.remote_root), |_| true)
            .unwrap_or_else(|e| panic!("upload {rest}: {e}"));
        std::fs::remove_file(&staging).unwrap();
    }

    fn mkdir(&self, rest: &str) {
        let connection = self.sftp.connection_for(&self.endpoint).unwrap();
        connection
            .create_dir(&format!("{}/{rest}", self.remote_root))
            .unwrap_or_else(|e| panic!("mkdir {rest}: {e}"));
    }

    fn remote_names(&self, rest: &str) -> Vec<String> {
        let connection = self.sftp.connection_for(&self.endpoint).unwrap();
        let path = if rest.is_empty() {
            self.remote_root.clone()
        } else {
            format!("{}/{rest}", self.remote_root)
        };
        let mut names: Vec<String> = connection
            .read_dir(&path)
            .map(|entries| entries.into_iter().map(|e| e.name).collect())
            .unwrap_or_default();
        names.sort();
        names
    }

    fn run(&self, plan: &Plan, policy: Policy) -> jtf_transfer::Report {
        jtf_transfer::run(
            plan,
            &self.sftp,
            policy,
            &mut Silent,
            &CancellationToken::never(),
        )
        .expect("the transfer could not start")
    }

    fn item(&self, source: Side, bytes: u64, is_directory: bool) -> Item {
        Item {
            source,
            destination: None,
            bytes,
            is_directory,
            is_symlink: false,
        }
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        // Best effort: a leftover scratch directory on somebody's server is
        // rude, and a panicking Drop would hide the real failure.
        if let Ok(connection) = self.sftp.connection_for(&self.endpoint) {
            let plan = Plan::build(
                Kind::Delete,
                vec![Item {
                    source: self.remote(""),
                    destination: None,
                    bytes: 0,
                    is_directory: true,
                    is_symlink: false,
                }],
                None,
            );
            if let Ok(plan) = plan {
                let _ = jtf_transfer::run(
                    &plan,
                    &self.sftp,
                    Policy::Skip,
                    &mut Silent,
                    &CancellationToken::never(),
                );
            }
            let _ = connection.remove_dir(&self.remote_root);
        }
        let _ = std::fs::remove_dir_all(&self.local_root);
    }
}

/// Skip with a note rather than passing silently, so a run with no server
/// cannot be mistaken for a run that proved something.
macro_rules! fixture {
    ($name:expr) => {
        match Fixture::new($name) {
            Some(fixture) => fixture,
            None => {
                eprintln!("skipped: set JTF_SFTP_TEST=user@host:port to run this");
                return;
            }
        }
    };
}

fn local_bytes(path: &Path) -> Vec<u8> {
    std::fs::read(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

#[test]
fn a_file_comes_down_with_its_bytes_intact() {
    let f = fixture!("download");
    let body: Vec<u8> = (0..40_000u32).map(|i| (i % 251) as u8).collect();
    f.put("payload.bin", &body);

    let plan = Plan::build(
        Kind::Copy,
        vec![f.item(f.remote("payload.bin"), body.len() as u64, false)],
        Some(f.local("")),
    )
    .unwrap();
    let report = f.run(&plan, Policy::Skip);

    assert_eq!(report.failed(), 0, "{:?}", report.outcomes);
    assert_eq!(report.succeeded(), 1);
    assert_eq!(local_bytes(&f.local_root.join("payload.bin")), body);
    assert!(
        f.remote_names("").contains(&"payload.bin".to_string()),
        "a copy removed the source"
    );
}

#[test]
fn nothing_is_left_under_the_temporary_name() {
    let f = fixture!("partial");
    f.put("thing.bin", b"content");
    let plan = Plan::build(
        Kind::Copy,
        vec![f.item(f.remote("thing.bin"), 7, false)],
        Some(f.local("")),
    )
    .unwrap();
    f.run(&plan, Policy::Skip);

    let strays: Vec<String> = std::fs::read_dir(&f.local_root)
        .unwrap()
        .filter_map(Result::ok)
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.contains("jtf-part"))
        .collect();
    assert!(strays.is_empty(), "left behind: {strays:?}");
}

#[test]
fn a_folder_comes_down_whole() {
    let f = fixture!("recurse");
    f.mkdir("tree");
    f.mkdir("tree/inner");
    f.put("tree/one.txt", b"one");
    f.put("tree/inner/two.txt", b"two");

    let plan = Plan::build(
        Kind::Copy,
        vec![f.item(f.remote("tree"), 0, true)],
        Some(f.local("")),
    )
    .unwrap();
    let report = f.run(&plan, Policy::Skip);

    assert_eq!(report.failed(), 0, "{:?}", report.outcomes);
    assert_eq!(local_bytes(&f.local_root.join("tree/one.txt")), b"one");
    assert_eq!(
        local_bytes(&f.local_root.join("tree/inner/two.txt")),
        b"two",
        "the walk did not reach the second level"
    );
}

#[test]
fn a_name_already_taken_is_skipped_or_kept_beside_it() {
    let f = fixture!("conflict");
    f.put("same.txt", b"from the server");
    std::fs::write(f.local_root.join("same.txt"), b"already here").unwrap();

    let plan = Plan::build(
        Kind::Copy,
        vec![f.item(f.remote("same.txt"), 15, false)],
        Some(f.local("")),
    )
    .unwrap();

    let report = f.run(&plan, Policy::Skip);
    assert_eq!(report.skipped(), 1);
    assert_eq!(
        local_bytes(&f.local_root.join("same.txt")),
        b"already here",
        "skip overwrote the file it was meant to leave alone"
    );

    let report = f.run(&plan, Policy::KeepBoth);
    assert_eq!(report.succeeded(), 1, "{:?}", report.outcomes);
    assert_eq!(
        local_bytes(&f.local_root.join("same 2.txt")),
        b"from the server",
        "keep-both did not write beside it"
    );
    assert_eq!(local_bytes(&f.local_root.join("same.txt")), b"already here");

    let report = f.run(&plan, Policy::Overwrite);
    assert_eq!(report.succeeded(), 1, "{:?}", report.outcomes);
    assert_eq!(
        local_bytes(&f.local_root.join("same.txt")),
        b"from the server",
        "overwrite left the old bytes"
    );
}

#[test]
fn a_move_off_the_server_takes_the_source_away_but_only_after_it_arrives() {
    let f = fixture!("move-down");
    f.put("going.txt", b"travelling");

    let plan = Plan::build(
        Kind::Move,
        vec![f.item(f.remote("going.txt"), 10, false)],
        Some(f.local("")),
    )
    .unwrap();
    let report = f.run(&plan, Policy::Skip);

    assert_eq!(report.failed(), 0, "{:?}", report.outcomes);
    assert_eq!(local_bytes(&f.local_root.join("going.txt")), b"travelling");
    assert!(
        !f.remote_names("").contains(&"going.txt".to_string()),
        "the source survived a move"
    );
}

#[test]
fn a_file_goes_up_and_can_be_moved_up() {
    let f = fixture!("upload");
    std::fs::write(f.local_root.join("rising.txt"), b"upwards").unwrap();
    std::fs::write(f.local_root.join("leaving.txt"), b"gone from here").unwrap();

    let copy = Plan::build(
        Kind::Copy,
        vec![f.item(f.local("rising.txt"), 7, false)],
        Some(f.remote("")),
    )
    .unwrap();
    let report = f.run(&copy, Policy::Skip);
    assert_eq!(report.failed(), 0, "{:?}", report.outcomes);
    assert!(f.remote_names("").contains(&"rising.txt".to_string()));
    assert!(
        f.local_root.join("rising.txt").exists(),
        "a copy up removed the local file"
    );

    let moved = Plan::build(
        Kind::Move,
        vec![f.item(f.local("leaving.txt"), 14, false)],
        Some(f.remote("")),
    )
    .unwrap();
    let report = f.run(&moved, Policy::Skip);
    assert_eq!(report.failed(), 0, "{:?}", report.outcomes);
    assert!(f.remote_names("").contains(&"leaving.txt".to_string()));
    assert!(
        !f.local_root.join("leaving.txt").exists(),
        "the local source survived a move up"
    );
}

#[test]
fn a_move_within_one_server_is_a_rename_and_moves_no_bytes() {
    let f = fixture!("rename");
    f.mkdir("over-there");
    f.put("staying.txt", b"same server");

    let plan = Plan::build(
        Kind::Move,
        vec![f.item(f.remote("staying.txt"), 11, false)],
        Some(f.remote("over-there")),
    )
    .unwrap();
    assert!(
        plan.is_same_server_rename(),
        "this should be recognised as a rename"
    );

    let report = f.run(&plan, Policy::Skip);
    assert_eq!(report.failed(), 0, "{:?}", report.outcomes);
    assert!(!f.remote_names("").contains(&"staying.txt".to_string()));
    assert!(f
        .remote_names("over-there")
        .contains(&"staying.txt".to_string()));
}

#[test]
fn deleting_on_the_server_removes_a_whole_tree() {
    let f = fixture!("delete");
    f.mkdir("doomed");
    f.mkdir("doomed/inner");
    f.put("doomed/a.txt", b"a");
    f.put("doomed/inner/b.txt", b"b");

    let plan = Plan::build(
        Kind::Delete,
        vec![f.item(f.remote("doomed"), 0, true)],
        None,
    )
    .unwrap();
    assert!(
        plan.deletes_on_a_server(),
        "the caller must be told this is the permanent kind"
    );

    let report = f.run(&plan, Policy::Skip);
    assert_eq!(report.failed(), 0, "{:?}", report.outcomes);
    assert!(
        !f.remote_names("").contains(&"doomed".to_string()),
        "the tree survived"
    );
}

#[test]
fn a_symbolic_link_is_refused_rather_than_followed() {
    let f = fixture!("symlink");
    let mut item = f.item(f.remote("a-link"), 0, false);
    item.is_symlink = true;

    let plan = Plan::build(Kind::Copy, vec![item], Some(f.local(""))).unwrap();
    let report = f.run(&plan, Policy::Skip);

    assert_eq!(report.succeeded(), 0);
    assert_eq!(report.failed(), 1, "a link was followed");
    assert!(
        !f.local_root.join("a-link").exists(),
        "something was written for a link"
    );
}

#[test]
fn a_cancelled_transfer_stops_and_leaves_no_half_file() {
    let f = fixture!("cancel");
    let big: Vec<u8> = (0..4_000_000u32).map(|i| (i % 251) as u8).collect();
    f.put("large.bin", &big);

    let plan = Plan::build(
        Kind::Copy,
        vec![f.item(f.remote("large.bin"), big.len() as u64, false)],
        Some(f.local("")),
    )
    .unwrap();

    // Cancelled before it starts, which is the case the window produces when
    // someone changes their mind at the confirmation.
    let report = jtf_transfer::run(
        &plan,
        &f.sftp,
        Policy::Skip,
        &mut Silent,
        &CancellationToken::cancelled(),
    )
    .unwrap();

    assert!(report.cancelled);
    assert!(
        !f.local_root.join("large.bin").exists(),
        "a cancelled transfer wrote the file anyway"
    );
    let strays: Vec<String> = std::fs::read_dir(&f.local_root)
        .unwrap()
        .filter_map(Result::ok)
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.contains("jtf-part"))
        .collect();
    assert!(strays.is_empty(), "left behind: {strays:?}");
}
