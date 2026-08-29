//! Searching a real tree.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use jtf_core::{FileEntry, Location};
use jtf_search::{parse, search, SearchUpdate};

static COUNTER: AtomicU64 = AtomicU64::new(0);

struct Tree {
    root: PathBuf,
}

impl Tree {
    fn new() -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        let unique = COUNTER.fetch_add(1, Ordering::SeqCst);
        let root = std::env::temp_dir().join(format!(
            "jtf-search-{}-{nanos}-{unique}",
            std::process::id()
        ));
        fs::create_dir_all(&root).unwrap();
        Self { root }
    }

    fn file(&self, relative: &str, bytes: &[u8]) -> &Self {
        let path = self.root.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, bytes).unwrap();
        self
    }

    fn run(&self, query: &str) -> Vec<String> {
        let parsed = parse(query).expect("query parses");
        let handle = search(&Location::local(&self.root), parsed).expect("search starts");

        let mut names = Vec::new();
        while let Some(update) = handle.recv() {
            match update {
                SearchUpdate::Matches(entries) => {
                    names.extend(entries.iter().map(FileEntry::display_name));
                }
                SearchUpdate::Done { .. } => break,
                SearchUpdate::Failed(error) => panic!("search failed: {error}"),
                SearchUpdate::Progress { .. } => {}
            }
        }
        names.sort();
        names
    }
}

impl Drop for Tree {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[test]
fn finds_by_name_anywhere_in_the_tree() {
    let tree = Tree::new();
    tree.file("a/report.txt", b"x")
        .file("b/c/report-2.txt", b"x")
        .file("b/other.txt", b"x");

    assert_eq!(tree.run("report"), vec!["report-2.txt", "report.txt"]);
}

#[test]
fn glob_and_extension_agree() {
    let tree = Tree::new();
    tree.file("one.log", b"x")
        .file("deep/two.log", b"x")
        .file("three.txt", b"x");

    assert_eq!(tree.run("glob:*.log"), vec!["one.log", "two.log"]);
    assert_eq!(tree.run("ext:log"), vec!["one.log", "two.log"]);
    assert_eq!(
        tree.run("ext:.log"),
        vec!["one.log", "two.log"],
        "a leading dot is tolerated"
    );
}

#[test]
fn regex_matches_the_name() {
    let tree = Tree::new();
    tree.file("log-2024.txt", b"x").file("log-abcd.txt", b"x");
    assert_eq!(tree.run(r"re:^log-\d+\.txt$"), vec!["log-2024.txt"]);
}

#[test]
fn size_narrows_by_bytes() {
    let tree = Tree::new();
    tree.file("small.bin", &[0u8; 100])
        .file("large.bin", &[0u8; 4096]);

    assert_eq!(tree.run("size:>1k"), vec!["large.bin"]);
    assert_eq!(tree.run("size:<1k"), vec!["small.bin"]);
}

#[test]
fn kind_narrows_to_folders_or_files() {
    let tree = Tree::new();
    tree.file("folder/inside.txt", b"x").file("loose.txt", b"x");

    assert_eq!(tree.run("kind:dir"), vec!["folder"]);
    assert_eq!(tree.run("kind:file"), vec!["inside.txt", "loose.txt"]);
}

#[test]
fn terms_combine_with_and() {
    let tree = Tree::new();
    tree.file("a/report.log", &[0u8; 4096])
        .file("a/report.txt", &[0u8; 4096])
        .file("a/small.log", b"x");

    assert_eq!(tree.run("report glob:*.log size:>1k"), vec!["report.log"]);
}

#[test]
fn negation_excludes() {
    let tree = Tree::new();
    tree.file("keep.txt", b"x").file("cache/drop.txt", b"x");

    assert_eq!(tree.run("glob:*.txt -path:cache"), vec!["keep.txt"]);
    assert_eq!(tree.run("glob:*.txt NOT path:cache"), vec!["keep.txt"]);
}

#[test]
fn an_empty_query_matches_everything_it_walks() {
    let tree = Tree::new();
    tree.file("one.txt", b"x").file("two.txt", b"x");
    assert_eq!(tree.run(""), vec!["one.txt", "two.txt"]);
}

#[test]
fn results_arrive_before_the_walk_finishes() {
    // docs/SEARCH_AI.md 2.3: a search over a large tree is usable before it
    // completes.
    let tree = Tree::new();
    for i in 0..500 {
        tree.file(&format!("dir{}/file{i:04}.txt", i % 20), b"x");
    }

    let handle = search(&Location::local(&tree.root), parse("file").unwrap()).unwrap();
    let mut batches = 0;
    while let Some(update) = handle.recv() {
        match update {
            SearchUpdate::Matches(_) => batches += 1,
            SearchUpdate::Done { matches, .. } => {
                assert_eq!(matches, 500);
                break;
            }
            _ => {}
        }
    }
    assert!(batches > 1, "matches must arrive incrementally");
}

#[test]
fn a_cancelled_search_stops_and_reports_nothing_further() {
    let tree = Tree::new();
    for i in 0..2000 {
        tree.file(&format!("dir{}/file{i:05}.txt", i % 50), b"x");
    }

    let handle = search(&Location::local(&tree.root), parse("file").unwrap()).unwrap();

    // Cancel once the walk has demonstrably started, not immediately.
    //
    // Cancelling straight after `search` raced the worker: on a loaded
    // machine the walk could finish before the cancel landed, and reporting
    // completion is then correct - the search really had completed. The test
    // failed intermittently for that reason and not because anything was
    // wrong. Waiting for the first result proves the walk is still running,
    // so the cancellation has something to interrupt.
    let first = handle.recv();
    let Some(first) = first else {
        panic!("the search produced nothing at all");
    };
    if matches!(first, SearchUpdate::Done { .. }) {
        // Finished before we could interrupt it. There is no cancellation to
        // observe, so there is nothing here to assert.
        return;
    }

    handle.cancel();
    let mut saw_done = false;
    while let Some(update) = handle.recv() {
        if matches!(update, SearchUpdate::Done { .. }) {
            saw_done = true;
        }
    }
    assert!(
        !saw_done,
        "a search cancelled while it was still walking must not go on to \
         report completion"
    );
}

#[test]
fn dropping_the_handle_stops_the_walk() {
    let tree = Tree::new();
    for i in 0..2000 {
        tree.file(&format!("dir{}/file{i:05}.txt", i % 50), b"x");
    }
    let handle = search(&Location::local(&tree.root), parse("file").unwrap()).unwrap();
    drop(handle); // must return promptly rather than after the whole walk
}

#[test]
fn an_unreadable_subtree_does_not_end_the_search() {
    let tree = Tree::new();
    tree.file("readable/one.txt", b"x");
    let locked = tree.root.join("locked");
    fs::create_dir_all(&locked).unwrap();
    fs::write(locked.join("hidden.txt"), b"x").unwrap();

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&locked, fs::Permissions::from_mode(0o000));
    }

    let found = tree.run("one");
    assert_eq!(
        found,
        vec!["one.txt"],
        "the readable part is still searched"
    );

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&locked, fs::Permissions::from_mode(0o755));
    }
}

#[test]
fn a_symlink_loop_cannot_make_the_walk_run_forever() {
    // The walk never follows a symlink, so a cycle is not entered at all; the
    // depth bound is the second layer (docs/SECURITY.md 3.1, AGENTS.md 20.2).
    if cfg!(not(unix)) {
        return;
    }
    let tree = Tree::new();
    tree.file("a/one.txt", b"x");
    #[cfg(unix)]
    std::os::unix::fs::symlink(&tree.root, tree.root.join("a/loop")).unwrap();

    assert_eq!(tree.run("one"), vec!["one.txt"]);
}
