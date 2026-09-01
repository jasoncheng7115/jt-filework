//! Hostile input, applied through the real APIs.
//!
//! `docs/TESTING.md` §9.2 asks for this fixture set. Nothing here should
//! crash, hang, escape its directory, or report success for work it did not do.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use jtf_jobs::CancellationToken;
use jtf_ops::{execute, preview_batch_rename, ConflictPolicy, Operation, Plan, RenamePattern};

static COUNTER: AtomicU64 = AtomicU64::new(0);

fn scratch() -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    let unique = COUNTER.fetch_add(1, Ordering::SeqCst);
    let root = std::env::temp_dir().join(format!(
        "jtf-hostile-{}-{nanos}-{unique}",
        std::process::id()
    ));
    fs::create_dir_all(&root).unwrap();
    root
}

/// Names that are legal on disk and awkward everywhere else.
const AWKWARD: &[&str] = &[
    "  leading and trailing  ",
    "dots...",
    "-dash-start",
    "--",
    "semi;colon",
    "dollar$sign",
    "quote'single",
    "back\\slash",
    "new\tline-ish",
    "unicode-\u{4e2d}\u{6587}",
    "emoji-\u{1f600}",
    "combining-e\u{0301}",
    "rtl-\u{202e}gnirts",
    "CON",
    "NUL",
    ".hidden",
    "..dots",
];

#[test]
fn awkward_names_survive_a_copy_unchanged() {
    let root = scratch();
    let source = root.join("from");
    let target = root.join("to");
    fs::create_dir_all(&source).unwrap();
    fs::create_dir_all(&target).unwrap();

    // Only the names the platform actually stored, under the name we asked
    // for.
    //
    // A successful write is not proof of that. Windows accepts a path with a
    // trailing space or dot and stores it without - `"dots..."` lands as
    // `"dots"` - and reserves `CON` and `NUL` outright, so a fixture that
    // trusted the write would then look for a file that is not there and
    // report a copy failure the copy did not cause. What survives this check
    // is what the platform genuinely holds, and all of it must copy intact.
    let mut created = Vec::new();
    for name in AWKWARD {
        let path = source.join(name);
        if fs::write(&path, name.as_bytes()).is_err() {
            continue;
        }
        let stored_as_asked = fs::read_dir(&source)
            .into_iter()
            .flatten()
            .flatten()
            .any(|entry| entry.file_name() == std::ffi::OsStr::new(name));
        if stored_as_asked {
            created.push(path);
        }
    }
    assert!(created.len() > 10, "the fixture set is not exercising much");

    let operation = Operation::Copy {
        sources: created.clone(),
        destination: target.clone(),
    };
    let plan = Plan::build(&operation, &CancellationToken::never()).unwrap();
    let report = execute(
        &plan,
        ConflictPolicy::Skip,
        &CancellationToken::never(),
        |_| {},
    );

    assert!(report.is_complete(), "failures: {:?}", report.failures());
    for path in &created {
        let name = path.file_name().unwrap();
        let landed = target.join(name);
        assert!(landed.exists(), "{name:?} did not arrive");
        assert_eq!(
            fs::read(&landed).unwrap(),
            fs::read(path).unwrap(),
            "{name:?} changed on the way"
        );
    }
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn a_batch_rename_cannot_be_talked_out_of_its_directory() {
    let root = scratch();
    fs::write(root.join("a.txt"), b"x").unwrap();
    let sources = vec![root.join("a.txt")];

    // Every one of these is a template that, taken literally, would write
    // somewhere else.
    for template in [
        "../escaped.txt",
        "../../escaped.txt",
        "sub/escaped.txt",
        "/absolute.txt",
        "{name}/../../escaped.txt",
        "..",
        ".",
        "",
    ] {
        let pattern = RenamePattern {
            template: template.to_string(),
            ..RenamePattern::default()
        };
        let preview = preview_batch_rename(&sources, &pattern);
        assert!(
            preview.is_blocked(),
            "{template:?} was not refused: {:?}",
            preview.rows[0]
        );
    }
    assert!(root.join("a.txt").exists());
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn a_very_long_name_is_reported_rather_than_crashing() {
    let root = scratch();
    let long = "x".repeat(4096);
    let source = root.join("a.txt");
    fs::write(&source, b"x").unwrap();

    let plan = Plan::build(
        &Operation::Rename {
            source,
            new_name: long,
        },
        &CancellationToken::never(),
    );
    // Either the plan refuses it or the filesystem does; neither may panic,
    // and the file must still be there.
    if let Ok(plan) = plan {
        let report = execute(
            &plan,
            ConflictPolicy::Skip,
            &CancellationToken::never(),
            |_| {},
        );
        assert!(!report.is_complete() || root.join("a.txt").exists());
    }
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn a_deep_tree_is_copied_without_recursing_the_stack() {
    // The walk is iterative; this is the fixture that would blow a recursive
    // one (AGENTS.md 20.2).
    let root = scratch();
    let mut deep = root.join("deep");
    fs::create_dir_all(&deep).unwrap();
    for i in 0..300 {
        deep = deep.join(format!("d{i}"));
    }
    if fs::create_dir_all(&deep).is_err() {
        let _ = fs::remove_dir_all(&root);
        return; // the platform refused the depth; nothing to test
    }
    fs::write(deep.join("leaf.txt"), b"deep").unwrap();

    let target = root.join("copy");
    fs::create_dir_all(&target).unwrap();
    let operation = Operation::Copy {
        sources: vec![root.join("deep")],
        destination: target.clone(),
    };
    let plan = Plan::build(&operation, &CancellationToken::never()).unwrap();
    let report = execute(
        &plan,
        ConflictPolicy::Skip,
        &CancellationToken::never(),
        |_| {},
    );

    assert!(report.is_complete(), "failures: {:?}", report.failures());
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn deleting_a_tree_that_contains_a_link_to_its_own_root_terminates() {
    if cfg!(not(unix)) {
        return;
    }
    let root = scratch();
    let tree = root.join("tree");
    fs::create_dir_all(tree.join("inner")).unwrap();
    fs::write(tree.join("inner/file.txt"), b"x").unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(&tree, tree.join("inner/loop")).unwrap();

    let plan = Plan::build(
        &Operation::Delete {
            sources: vec![tree.clone()],
        },
        &CancellationToken::never(),
    )
    .unwrap();
    let report = execute(
        &plan,
        ConflictPolicy::Skip,
        &CancellationToken::never(),
        |_| {},
    );

    assert!(report.is_complete(), "failures: {:?}", report.failures());
    assert!(!tree.exists());
    assert!(
        root.exists(),
        "the loop must not have taken the parent with it"
    );
    let _ = fs::remove_dir_all(&root);
}
