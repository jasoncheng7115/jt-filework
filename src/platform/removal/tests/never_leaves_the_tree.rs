//! Deleting must never reach outside what was selected.
//!
//! The interesting cases are not "does it delete the files" - every
//! implementation does that. They are the ones where the tree is not what it
//! appeared to be a moment ago.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::{Path, PathBuf};

use jtf_platform_removal::remove_tree;

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("jtf-rm-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("temp dir");
    dir
}

fn write(path: &Path, body: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("parent");
    }
    fs::write(path, body).expect("write");
}

#[test]
fn removes_a_tree_and_everything_in_it() {
    let root = temp_dir("plain");
    write(&root.join("tree/a.txt"), "a");
    write(&root.join("tree/deep/b.txt"), "b");
    write(&root.join("tree/deep/deeper/c.txt"), "c");

    remove_tree(&root.join("tree")).expect("removed");
    assert!(!root.join("tree").exists());
    // And nothing above it.
    assert!(root.exists());
}

#[test]
fn a_file_is_removed_as_a_file() {
    let root = temp_dir("file");
    write(&root.join("one.txt"), "x");
    remove_tree(&root.join("one.txt")).expect("removed");
    assert!(!root.join("one.txt").exists());
}

/// The whole point. A link inside the tree is unlinked; what it points at is
/// not touched.
#[cfg(unix)]
#[test]
fn a_symlink_inside_the_tree_is_unlinked_not_followed() {
    let root = temp_dir("link");
    write(&root.join("keep/precious.txt"), "do not delete me");
    fs::create_dir_all(root.join("tree")).unwrap();
    std::os::unix::fs::symlink(root.join("keep"), root.join("tree/pointer")).unwrap();

    remove_tree(&root.join("tree")).expect("removed");

    assert!(!root.join("tree").exists(), "the tree went");
    assert!(
        root.join("keep/precious.txt").exists(),
        "a link was followed out of the selected tree"
    );
}

/// And a link *at the top*, which is the case where the user selected the link
/// itself. Removing what it points at would be answering a question nobody
/// asked.
#[cfg(unix)]
#[test]
fn a_symlink_at_the_top_removes_only_the_link() {
    let root = temp_dir("toplink");
    write(&root.join("keep/precious.txt"), "still here");
    std::os::unix::fs::symlink(root.join("keep"), root.join("pointer")).unwrap();

    remove_tree(&root.join("pointer")).expect("removed");

    assert!(!root.join("pointer").exists());
    assert!(root.join("keep/precious.txt").exists());
}

/// A name that is *already* a link when the walk reaches it.
///
/// This is the symlink rule rather than the race: the old path-based walk
/// passed it too. It is kept because the rule is worth a test of its own, and
/// named for what it actually checks.
#[cfg(unix)]
#[test]
fn a_directory_replaced_by_a_link_before_the_walk_is_not_a_door() {
    let root = temp_dir("swap");
    write(
        &root.join("outside/treasure.txt"),
        "not part of the selection",
    );
    write(&root.join("tree/inner/ordinary.txt"), "ordinary");

    // What an attacker with write access to `tree` achieves: `inner` is no
    // longer a directory of ours, it is a pointer somewhere else.
    fs::remove_dir_all(root.join("tree/inner")).unwrap();
    std::os::unix::fs::symlink(root.join("outside"), root.join("tree/inner")).unwrap();

    remove_tree(&root.join("tree")).expect("removed");

    assert!(!root.join("tree").exists(), "the selected tree went");
    assert!(
        root.join("outside/treasure.txt").exists(),
        "the delete walked through a swapped directory and out of the tree"
    );
}

/// A tree deeper than the bound is refused rather than recursed into.
#[test]
fn a_tree_deeper_than_the_bound_is_refused() {
    let root = temp_dir("deep");
    let mut path = root.join("tree");
    for _ in 0..(jtf_platform_removal::MAX_DEPTH + 4) {
        path = path.join("d");
    }
    fs::create_dir_all(&path).expect("deep tree");

    let outcome = remove_tree(&root.join("tree"));
    assert!(
        outcome.is_err(),
        "a tree past the depth bound must be refused, not walked"
    );
}

/// The guarantee is different on Windows, and says so rather than pretending.
#[test]
fn the_build_says_which_guarantee_it_offers() {
    assert_eq!(jtf_platform_removal::is_race_free(), cfg!(unix));
}

/// The race itself.
///
/// One thread deletes; another spends the whole time replacing a subdirectory
/// with a symlink pointing outside the selection, over and over. This is what
/// an attacker with write access to one directory inside the tree can do, and
/// it is the reason a path is never resolved twice.
///
/// A racing test can only ever say "it did not escape this time", so it is run
/// many times and the assertion is about the file outside, never about whether
/// the delete succeeded - losing the race legitimately produces an error, and
/// an error is a fine outcome. Escaping is not.
///
/// Against the path-based walk this eventually deletes `treasure.txt`. Against
/// the descriptor walk there is no window in which to do it: the directory is
/// open before it is read, and the descriptor keeps pointing at the directory
/// that was opened however the name is rebound afterwards.
#[cfg(unix)]
#[test]
fn swapping_a_directory_for_a_link_while_the_delete_runs_never_escapes() {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    for attempt in 0..40 {
        let root = temp_dir(&format!("race{attempt}"));
        write(
            &root.join("outside/treasure.txt"),
            "not part of the selection",
        );

        let tree = root.join("tree");
        // Enough entries that the walk takes long enough to have a window at
        // all; with three files it can finish between two swaps.
        for i in 0..200 {
            write(&tree.join(format!("filler/{i}.txt")), "x");
        }
        fs::create_dir_all(tree.join("target")).unwrap();
        write(&tree.join("target/inner.txt"), "inner");

        let stop = Arc::new(AtomicBool::new(false));
        let swapper = {
            let stop = Arc::clone(&stop);
            let target = tree.join("target");
            let outside = root.join("outside");
            std::thread::spawn(move || {
                while !stop.load(Ordering::Relaxed) {
                    let _ = fs::remove_dir_all(&target);
                    let _ = std::os::unix::fs::symlink(&outside, &target);
                    let _ = fs::remove_file(&target);
                    let _ = fs::create_dir_all(&target);
                }
            })
        };

        let _ = remove_tree(&tree); // may fail; that is a fine outcome
        stop.store(true, Ordering::Relaxed);
        swapper.join().unwrap();

        assert!(
            root.join("outside/treasure.txt").exists(),
            "attempt {attempt}: the delete followed a swapped directory out of \
             the tree it was given"
        );
        let _ = fs::remove_dir_all(&root);
    }
}
