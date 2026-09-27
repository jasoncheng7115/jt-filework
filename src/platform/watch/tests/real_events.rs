//! The watcher against the real backend of whichever platform runs the tests:
//! FSEvents, inotify or `ReadDirectoryChangesW`. Each change is made in a
//! fresh temporary folder and waited for, with a deadline generous enough for
//! a loaded CI machine - FSEvents alone delivers tens of milliseconds late by
//! design.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use jtf_platform_watch::{Change, Standing, Watcher};

const DEADLINE: Duration = Duration::from_secs(10);

/// A folder of its own under the system temporary directory - never under a
/// synced folder, where a sync client would add changes of its own.
fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("jtf-watch-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Drain until `wanted` says yes, or fail after the deadline.
fn wait_for(watcher: &mut Watcher, what: &str, mut wanted: impl FnMut(&[Change]) -> bool) {
    let start = Instant::now();
    while start.elapsed() < DEADLINE {
        let drained = watcher.drain();
        if wanted(&drained.changes) {
            return;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    panic!("no {what} within {DEADLINE:?}");
}

fn watching(folder: &Path) -> Watcher {
    let mut watcher = Watcher::start();
    watcher.set_folders([folder.to_path_buf()]);
    let start = Instant::now();
    while watcher.standing(folder).is_none() && start.elapsed() < DEADLINE {
        let _ = watcher.drain();
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(
        watcher.standing(folder),
        Some(Standing::Watched),
        "{} was not watched",
        folder.display()
    );
    // FSEvents can report the folder's own creation a moment after the stream
    // starts; let it pass so the next event is the one under test.
    std::thread::sleep(Duration::from_millis(300));
    let _ = watcher.drain();
    watcher
}

fn listed(folder: &Path) -> impl Fn(&[Change]) -> bool + '_ {
    move |changes| changes.contains(&Change::Listing(folder.to_path_buf()))
}

#[test]
fn a_file_appearing_is_heard() {
    let folder = scratch("create");
    let mut watcher = watching(&folder);
    std::fs::write(folder.join("new.txt"), b"hello").unwrap();
    wait_for(
        &mut watcher,
        "listing change after a create",
        listed(&folder),
    );
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn a_file_going_is_heard() {
    let folder = scratch("remove");
    std::fs::write(folder.join("old.txt"), b"bye").unwrap();
    let mut watcher = watching(&folder);
    std::fs::remove_file(folder.join("old.txt")).unwrap();
    wait_for(
        &mut watcher,
        "listing change after a remove",
        listed(&folder),
    );
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn a_rename_is_heard() {
    let folder = scratch("rename");
    std::fs::write(folder.join("a.txt"), b"x").unwrap();
    let mut watcher = watching(&folder);
    std::fs::rename(folder.join("a.txt"), folder.join("b.txt")).unwrap();
    wait_for(
        &mut watcher,
        "listing change after a rename",
        listed(&folder),
    );
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn a_file_growing_is_heard_as_that_file_or_the_folder() {
    let folder = scratch("grow");
    let file = folder.join("log.txt");
    std::fs::write(&file, b"one\n").unwrap();
    let mut watcher = watching(&folder);
    {
        use std::io::Write as _;
        let mut log = std::fs::OpenOptions::new()
            .append(true)
            .open(&file)
            .unwrap();
        log.write_all(b"two\n").unwrap();
        log.sync_all().unwrap();
    }
    // FSEvents may still carry the creation flag for a file this young, and
    // then the folder is re-read instead; either is correct.
    let entry = Change::Entry(file.clone());
    let listing = Change::Listing(folder.clone());
    wait_for(&mut watcher, "a change after an append", |changes| {
        changes.contains(&entry) || changes.contains(&listing)
    });
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn a_folder_no_longer_shown_is_let_go() {
    let folder = scratch("letgo");
    let mut watcher = watching(&folder);
    watcher.set_folders(Vec::new());
    assert_eq!(watcher.standing(&folder), None);
    std::fs::write(folder.join("after.txt"), b"x").unwrap();
    std::thread::sleep(Duration::from_millis(500));
    assert!(
        watcher.drain().changes.is_empty(),
        "a folder no pane shows still reported a change"
    );
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn the_temporary_directory_is_not_a_network_filesystem() {
    // If it were, the tests above would have failed at `watching`, which
    // asserts Watched; this names the reason in case they do.
    let folder = scratch("local");
    let watcher = watching(&folder);
    assert_eq!(watcher.standing(&folder), Some(Standing::Watched));
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn a_folder_let_go_and_asked_for_again_is_reported_again() {
    let folder = scratch("again");
    let mut watcher = watching(&folder);
    watcher.set_folders(Vec::new());
    watcher.set_folders([folder.clone()]);
    assert_eq!(watcher.standing(&folder), None);
    let start = Instant::now();
    while watcher.standing(&folder).is_none() && start.elapsed() < DEADLINE {
        let _ = watcher.drain();
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(watcher.standing(&folder), Some(Standing::Watched));
    let _ = std::fs::remove_dir_all(&folder);
}

/// A FUSE mount - sshfs, rclone, the desktop's own portal - is polled, not
/// watched: inotify on it hears only what this machine did (ADR-0007). Uses
/// whichever FUSE mount the machine already has, and says so and passes when
/// it has none, which is most CI runners.
#[cfg(target_os = "linux")]
#[test]
fn a_fuse_mount_is_polled_rather_than_watched() {
    use jtf_platform_watch::Polled;
    let mounts = std::fs::read_to_string("/proc/self/mounts").unwrap_or_default();
    let Some(folder) = mounts
        .lines()
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            let point = fields.next().and(fields.next())?;
            let kind = fields.next()?;
            (kind == "fuse" || kind.starts_with("fuse.")).then(|| PathBuf::from(point))
        })
        .find(|point| std::fs::read_dir(point).is_ok())
    else {
        eprintln!("no readable FUSE mount here; nothing to check");
        return;
    };
    let mut watcher = Watcher::start();
    watcher.set_folders([folder.clone()]);
    let start = Instant::now();
    while watcher.standing(&folder).is_none() && start.elapsed() < DEADLINE {
        let _ = watcher.drain();
        std::thread::sleep(Duration::from_millis(10));
    }
    // Either answer leaves it to the poll: FUSE by its magic number, or - as
    // the desktop's document portal does - by refusing to say at all.
    assert!(
        matches!(
            watcher.standing(&folder),
            Some(Standing::Polled(
                Polled::NetworkFilesystem | Polled::UnknownFilesystem
            ))
        ),
        "{} is FUSE and was not left to the poll: {:?}",
        folder.display(),
        watcher.standing(&folder)
    );
}
