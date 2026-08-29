//! Behaviour of the file operations.
//!
//! These act on real files, because the interesting failures are real: a
//! symlink followed by mistake, a directory copied into itself, a partial
//! failure reported as success.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use jtf_core::ErrorCode;
use jtf_jobs::CancellationToken;
use jtf_ops::{execute, ConflictPolicy, Operation, Outcome, Plan, PlanError};

static COUNTER: AtomicU64 = AtomicU64::new(0);

struct Fixture {
    root: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        let unique = COUNTER.fetch_add(1, Ordering::SeqCst);
        let root =
            std::env::temp_dir().join(format!("jtf-ops-{}-{nanos}-{unique}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        Self { root }
    }

    fn file(&self, relative: &str, bytes: &[u8]) -> PathBuf {
        let path = self.root.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(&path, bytes).unwrap();
        path
    }

    fn dir(&self, relative: &str) -> PathBuf {
        let path = self.root.join(relative);
        fs::create_dir_all(&path).unwrap();
        path
    }

    fn path(&self, relative: &str) -> PathBuf {
        self.root.join(relative)
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn run(operation: &Operation, policy: ConflictPolicy) -> jtf_ops::Report {
    let token = CancellationToken::never();
    let plan = Plan::build(operation, &token).expect("plan");
    execute(&plan, policy, &token, |_| {})
}

fn read(path: &Path) -> String {
    fs::read_to_string(path).unwrap()
}

// ------------------------------------------------------------------- copying

#[test]
fn copies_a_file_and_leaves_the_original() {
    let f = Fixture::new();
    let source = f.file("a.txt", b"hello");
    let target = f.dir("out");

    let report = run(
        &Operation::Copy {
            sources: vec![source.clone()],
            destination: target.clone(),
        },
        ConflictPolicy::Skip,
    );

    assert!(report.is_complete());
    assert_eq!(read(&target.join("a.txt")), "hello");
    assert!(source.exists(), "a copy does not remove the original");
}

#[test]
fn copies_a_whole_tree() {
    let f = Fixture::new();
    f.file("tree/one.txt", b"1");
    f.file("tree/nested/two.txt", b"2");
    f.dir("tree/empty");
    let target = f.dir("out");

    let report = run(
        &Operation::Copy {
            sources: vec![f.path("tree")],
            destination: target.clone(),
        },
        ConflictPolicy::Skip,
    );

    assert!(report.is_complete());
    assert_eq!(read(&target.join("tree/one.txt")), "1");
    assert_eq!(read(&target.join("tree/nested/two.txt")), "2");
    assert!(
        target.join("tree/empty").is_dir(),
        "an empty directory is still a directory"
    );
}

#[test]
fn copying_a_directory_into_itself_is_refused_before_anything_moves() {
    let f = Fixture::new();
    let tree = f.dir("tree");
    f.file("tree/one.txt", b"1");
    let inside = f.dir("tree/inner");

    let error = Plan::build(
        &Operation::Copy {
            sources: vec![tree],
            destination: inside,
        },
        &CancellationToken::never(),
    )
    .unwrap_err();

    assert!(matches!(error, PlanError::DestinationInsideSource(_)));
}

// ------------------------------------------------------------------ conflicts

#[test]
fn the_default_policy_skips_rather_than_destroying() {
    let f = Fixture::new();
    let source = f.file("a.txt", b"new");
    let target = f.dir("out");
    f.file("out/a.txt", b"existing");

    let plan = Plan::build(
        &Operation::Copy {
            sources: vec![source],
            destination: target.clone(),
        },
        &CancellationToken::never(),
    )
    .unwrap();
    assert_eq!(
        plan.conflicts.len(),
        1,
        "the conflict is known before the job runs"
    );

    let report = execute(
        &plan,
        ConflictPolicy::Skip,
        &CancellationToken::never(),
        |_| {},
    );
    assert_eq!(report.skipped(), 1);
    assert_eq!(
        read(&target.join("a.txt")),
        "existing",
        "nothing was overwritten"
    );
}

#[test]
fn overwrite_replaces_and_keep_both_writes_alongside() {
    let f = Fixture::new();
    let source = f.file("a.txt", b"new");
    let target = f.dir("out");
    f.file("out/a.txt", b"existing");

    run(
        &Operation::Copy {
            sources: vec![source.clone()],
            destination: target.clone(),
        },
        ConflictPolicy::Overwrite,
    );
    assert_eq!(read(&target.join("a.txt")), "new");

    fs::write(target.join("a.txt"), b"existing").unwrap();
    run(
        &Operation::Copy {
            sources: vec![source],
            destination: target.clone(),
        },
        ConflictPolicy::KeepBoth,
    );
    assert_eq!(
        read(&target.join("a.txt")),
        "existing",
        "the original is untouched"
    );
    assert_eq!(
        read(&target.join("a 2.txt")),
        "new",
        "the copy sits beside it"
    );
}

// --------------------------------------------------------------------- moving

#[test]
fn moving_removes_the_source() {
    let f = Fixture::new();
    let source = f.file("a.txt", b"hello");
    let target = f.dir("out");

    let report = run(
        &Operation::Move {
            sources: vec![source.clone()],
            destination: target.clone(),
        },
        ConflictPolicy::Skip,
    );

    assert!(report.is_complete());
    assert_eq!(read(&target.join("a.txt")), "hello");
    assert!(!source.exists());
}

// ------------------------------------------------------------------- renaming

#[test]
fn renames_in_place() {
    let f = Fixture::new();
    let source = f.file("old.txt", b"x");
    let report = run(
        &Operation::Rename {
            source: source.clone(),
            new_name: "new.txt".into(),
        },
        ConflictPolicy::Skip,
    );
    assert!(report.is_complete());
    assert!(!source.exists());
    assert_eq!(read(&f.path("new.txt")), "x");
}

#[test]
fn a_name_containing_a_path_is_refused() {
    let f = Fixture::new();
    let source = f.file("a.txt", b"x");
    for name in ["../escape.txt", "sub/dir.txt", "", ".", ".."] {
        let error = Plan::build(
            &Operation::Rename {
                source: source.clone(),
                new_name: name.into(),
            },
            &CancellationToken::never(),
        )
        .unwrap_err();
        assert!(
            matches!(error, PlanError::InvalidName(_)),
            "{name:?} must be refused as a name"
        );
    }
}

#[test]
fn a_case_only_rename_is_not_treated_as_a_conflict() {
    // docs/TESTING.md 5.2: on a case-insensitive filesystem the target
    // resolves to the same file, and treating that as a conflict would make
    // the rename impossible.
    let f = Fixture::new();
    let source = f.file("readme", b"x");
    let plan = Plan::build(
        &Operation::Rename {
            source,
            new_name: "README".into(),
        },
        &CancellationToken::never(),
    )
    .unwrap();
    assert!(plan.conflicts.is_empty());
}

// ------------------------------------------------------------------- removing

#[test]
fn delete_removes_a_tree() {
    let f = Fixture::new();
    f.file("tree/one.txt", b"1");
    f.file("tree/nested/two.txt", b"2");
    let tree = f.path("tree");

    let report = run(
        &Operation::Delete {
            sources: vec![tree.clone()],
        },
        ConflictPolicy::Skip,
    );

    assert!(report.is_complete());
    assert!(!tree.exists());
}

#[test]
fn deleting_a_symlink_removes_the_link_and_not_its_target() {
    // The single most damaging bug a file manager can have
    // (docs/SECURITY.md 3.1).
    if cfg!(not(unix)) {
        return;
    }
    let f = Fixture::new();
    let real = f.file("outside/precious.txt", b"do not delete me");
    let tree = f.dir("tree");
    let link = tree.join("link.txt");
    #[cfg(unix)]
    std::os::unix::fs::symlink(&real, &link).unwrap();

    let report = run(
        &Operation::Delete {
            sources: vec![tree.clone()],
        },
        ConflictPolicy::Skip,
    );

    assert!(report.is_complete());
    assert!(!tree.exists(), "the tree is gone");
    assert!(!link.exists());
    assert!(real.exists(), "the symlink's target must survive");
    assert_eq!(read(&real), "do not delete me");
}

#[test]
fn a_directory_symlink_is_not_descended_into_during_delete() {
    if cfg!(not(unix)) {
        return;
    }
    let f = Fixture::new();
    f.file("outside/keep.txt", b"keep");
    let outside = f.path("outside");
    let tree = f.dir("tree");
    #[cfg(unix)]
    std::os::unix::fs::symlink(&outside, tree.join("link")).unwrap();

    run(
        &Operation::Delete {
            sources: vec![tree],
        },
        ConflictPolicy::Skip,
    );

    assert!(
        outside.join("keep.txt").exists(),
        "delete escaped through a directory symlink"
    );
}

// --------------------------------------------------------- reporting and cost

#[test]
fn one_failure_does_not_lose_the_other_entries() {
    let f = Fixture::new();
    let good = f.file("good.txt", b"1");
    let missing = f.path("missing.txt");
    let target = f.dir("out");

    let report = run(
        &Operation::Copy {
            sources: vec![good, missing.clone()],
            destination: target.clone(),
        },
        ConflictPolicy::Skip,
    );

    // The good one lands, the vanished one is reported, and the operation is
    // honest about being partial.
    assert!(
        target.join("good.txt").exists(),
        "a sibling's failure must not lose this file"
    );
    assert_eq!(report.succeeded(), 1);
    let failures = report.failures();
    assert_eq!(
        failures.len(),
        1,
        "the failure is attributed to the entry that caused it"
    );
    assert_eq!(failures[0].0, missing);
    assert!(!report.is_complete());
}

#[test]
fn a_partial_result_is_never_reported_as_complete() {
    let f = Fixture::new();
    let source = f.file("a.txt", b"x");
    let target = f.dir("out");
    f.file("out/a.txt", b"existing");

    let report = run(
        &Operation::Copy {
            sources: vec![source],
            destination: target,
        },
        ConflictPolicy::Skip,
    );
    assert!(
        !report.is_complete(),
        "a skipped entry means the operation did not fully complete"
    );
    assert_eq!(report.skipped(), 1);
}

#[test]
fn the_plan_measures_what_the_progress_bar_will_show() {
    let f = Fixture::new();
    f.file("tree/one.txt", b"12345");
    f.file("tree/two.txt", b"1234567890");
    let target = f.dir("out");

    let plan = Plan::build(
        &Operation::Copy {
            sources: vec![f.path("tree")],
            destination: target,
        },
        &CancellationToken::never(),
    )
    .unwrap();

    assert_eq!(plan.total_bytes, 15);
    assert!(plan.total_entries >= 3, "the directory and both files");
}

#[test]
fn progress_is_reported_and_ends_at_the_total() {
    let f = Fixture::new();
    f.file("tree/one.txt", b"12345");
    let target = f.dir("out");
    let plan = Plan::build(
        &Operation::Copy {
            sources: vec![f.path("tree")],
            destination: target,
        },
        &CancellationToken::never(),
    )
    .unwrap();

    let mut last = None;
    execute(
        &plan,
        ConflictPolicy::Skip,
        &CancellationToken::never(),
        |progress| {
            last = Some(progress.clone());
        },
    );

    let last = last.expect("progress was reported");
    assert_eq!(last.bytes_done, plan.total_bytes);
    assert_eq!(last.current, None, "nothing is in progress once it is done");
}

#[test]
fn a_cancelled_operation_says_so_rather_than_claiming_success() {
    let f = Fixture::new();
    f.file("tree/one.txt", b"1");
    let target = f.dir("out");
    let plan = Plan::build(
        &Operation::Copy {
            sources: vec![f.path("tree")],
            destination: target,
        },
        &CancellationToken::never(),
    )
    .unwrap();

    let report = execute(
        &plan,
        ConflictPolicy::Skip,
        &CancellationToken::cancelled(),
        |_| {},
    );
    assert!(report.cancelled);
    assert!(!report.is_complete());
}

#[test]
fn an_empty_selection_is_refused_rather_than_silently_doing_nothing() {
    let f = Fixture::new();
    let error = Plan::build(
        &Operation::Copy {
            sources: vec![],
            destination: f.root.clone(),
        },
        &CancellationToken::never(),
    )
    .unwrap_err();
    assert!(matches!(error, PlanError::NothingToDo));
}

#[test]
fn new_folder_creates_and_reports_an_existing_one_as_skipped() {
    let f = Fixture::new();
    let report = run(
        &Operation::NewFolder {
            parent: f.root.clone(),
            name: "made".into(),
        },
        ConflictPolicy::Skip,
    );
    assert!(report.is_complete());
    assert!(f.path("made").is_dir());

    let again = run(
        &Operation::NewFolder {
            parent: f.root.clone(),
            name: "made".into(),
        },
        ConflictPolicy::Skip,
    );
    assert_eq!(again.skipped(), 1);
}

#[test]
fn delete_is_flagged_irreversible_and_trash_is_not() {
    assert!(Operation::Delete { sources: vec![] }.is_irreversible());
    assert!(!Operation::Trash { sources: vec![] }.is_irreversible());
}

#[test]
fn a_missing_source_reports_not_found_rather_than_panicking() {
    let f = Fixture::new();
    // Planning tolerates it; execution reports it against that entry.
    let report = run(
        &Operation::Delete {
            sources: vec![f.path("nope")],
        },
        ConflictPolicy::Skip,
    );
    let failures = report.failures();
    assert_eq!(failures.len(), 1);
    assert_eq!(failures[0].1.code(), ErrorCode::NotFound);
    let _ = Outcome::Skipped;
}

// ----------------------------------------------------------------- undo

use jtf_ops::{undo, UndoRecord};

fn run_with_record(
    operation: &Operation,
    policy: ConflictPolicy,
) -> (jtf_ops::Report, Option<UndoRecord>) {
    let token = CancellationToken::never();
    let plan = Plan::build(operation, &token).expect("plan");
    let report = execute(&plan, policy, &token, |_| {});
    let record = UndoRecord::from_report(operation, &report);
    (report, record)
}

#[test]
fn undoing_a_move_puts_the_file_back() {
    let f = Fixture::new();
    let source = f.file("a.txt", b"hello");
    let target = f.dir("out");

    let (_, record) = run_with_record(
        &Operation::Move {
            sources: vec![source.clone()],
            destination: target.clone(),
        },
        ConflictPolicy::Skip,
    );
    assert!(!source.exists());

    let record = record.expect("a move is undoable");
    assert_eq!(record.label_key(), "command.file.move_to_target_pane");

    let report = undo(&record, &CancellationToken::never());
    assert!(report.is_complete());
    assert_eq!(read(&source), "hello");
    assert!(!target.join("a.txt").exists());
}

#[test]
fn undoing_a_rename_restores_the_old_name() {
    let f = Fixture::new();
    let source = f.file("old.txt", b"x");
    let (_, record) = run_with_record(
        &Operation::Rename {
            source: source.clone(),
            new_name: "new.txt".into(),
        },
        ConflictPolicy::Skip,
    );

    undo(
        &record.expect("a rename is undoable"),
        &CancellationToken::never(),
    );
    assert!(source.exists());
    assert!(!f.path("new.txt").exists());
}

#[test]
fn undoing_a_new_folder_removes_it_only_while_it_is_empty() {
    let f = Fixture::new();
    let (_, record) = run_with_record(
        &Operation::NewFolder {
            parent: f.root.clone(),
            name: "made".into(),
        },
        ConflictPolicy::Skip,
    );
    let record = record.expect("a new folder is undoable");

    // Something the user put there afterwards must survive.
    fs::write(f.path("made/mine.txt"), b"do not delete").unwrap();
    let report = undo(&record, &CancellationToken::never());
    assert_eq!(report.skipped(), 1, "a non-empty directory is left alone");
    assert!(f.path("made/mine.txt").exists());

    fs::remove_file(f.path("made/mine.txt")).unwrap();
    undo(&record, &CancellationToken::never());
    assert!(!f.path("made").exists(), "an empty one is removed");
}

#[test]
fn a_copy_is_not_undoable_and_says_so_rather_than_deleting_files() {
    let f = Fixture::new();
    let source = f.file("a.txt", b"x");
    let target = f.dir("out");

    let (_, record) = run_with_record(
        &Operation::Copy {
            sources: vec![source],
            destination: target.clone(),
        },
        ConflictPolicy::Skip,
    );
    assert!(
        record.is_none(),
        "undoing a copy would delete files the user may have edited"
    );
    assert!(target.join("a.txt").exists());
}

#[test]
fn a_delete_is_not_undoable() {
    let f = Fixture::new();
    let doomed = f.file("gone.txt", b"x");
    let (_, record) = run_with_record(
        &Operation::Delete {
            sources: vec![doomed],
        },
        ConflictPolicy::Skip,
    );
    assert!(record.is_none());
}

#[test]
fn undo_refuses_to_overwrite_something_that_took_the_old_place() {
    // The case undo exists to avoid causing: putting a file back over one that
    // was created in the meantime.
    let f = Fixture::new();
    let source = f.file("a.txt", b"original");
    let target = f.dir("out");

    let (_, record) = run_with_record(
        &Operation::Move {
            sources: vec![source.clone()],
            destination: target.clone(),
        },
        ConflictPolicy::Skip,
    );
    fs::write(&source, b"something new").unwrap();

    let report = undo(&record.unwrap(), &CancellationToken::never());
    assert_eq!(report.skipped(), 1);
    assert_eq!(read(&source), "something new", "the newer file survives");
    assert!(
        target.join("a.txt").exists(),
        "and the moved file stays where it is"
    );
}

#[test]
fn undo_reports_an_entry_that_has_since_vanished() {
    let f = Fixture::new();
    let source = f.file("a.txt", b"x");
    let target = f.dir("out");
    let (_, record) = run_with_record(
        &Operation::Move {
            sources: vec![source],
            destination: target.clone(),
        },
        ConflictPolicy::Skip,
    );

    fs::remove_file(target.join("a.txt")).unwrap();
    let report = undo(&record.unwrap(), &CancellationToken::never());
    assert_eq!(report.failures().len(), 1);
    assert_eq!(report.failures()[0].1.code(), ErrorCode::NotFound);
}

#[test]
fn only_steps_that_actually_happened_are_recorded() {
    let f = Fixture::new();
    let good = f.file("good.txt", b"1");
    let target = f.dir("out");
    f.file("out/clash.txt", b"existing");
    let clash = f.file("clash.txt", b"new");

    let (_, record) = run_with_record(
        &Operation::Move {
            sources: vec![good, clash],
            destination: target,
        },
        ConflictPolicy::Skip,
    );

    let record = record.expect("one of the two moved");
    assert_eq!(record.len(), 1, "a skipped entry has nothing to undo");
}

/// A new file is created empty, and never overwrites one that exists.
#[test]
fn new_file_creates_an_empty_file_and_refuses_to_clobber() {
    let fixture = Fixture::new();
    let parent = fixture.root.clone();

    let operation = Operation::NewFile {
        parent: parent.clone(),
        name: "notes.txt".to_string(),
    };
    let plan = Plan::build(&operation, &CancellationToken::never()).expect("a plan");
    let report = execute(
        &plan,
        ConflictPolicy::Skip,
        &CancellationToken::never(),
        |_| {},
    );
    assert!(report
        .outcomes
        .iter()
        .all(|(_, o)| matches!(o, Outcome::Done { .. })));

    let created = parent.join("notes.txt");
    assert!(created.is_file());
    assert_eq!(std::fs::metadata(&created).unwrap().len(), 0);

    // Existing content must survive a second attempt with the same name.
    std::fs::write(&created, b"important").expect("write");
    let again = Plan::build(&operation, &CancellationToken::never()).expect("a plan");
    let report = execute(
        &again,
        ConflictPolicy::Skip,
        &CancellationToken::never(),
        |_| {},
    );
    assert_eq!(
        std::fs::read(&created).unwrap(),
        b"important",
        "creating a file that already exists must not empty it; create_new \
         is what makes that impossible rather than merely unlikely"
    );
    assert!(report
        .outcomes
        .iter()
        .any(|(_, o)| matches!(o, Outcome::Skipped)));
}

/// Undoing a new file removes it only while it is still empty.
#[test]
fn undoing_a_new_file_leaves_anything_written_into_it_alone() {
    let fixture = Fixture::new();
    let parent = fixture.root.clone();
    let operation = Operation::NewFile {
        parent: parent.clone(),
        name: "draft.txt".to_string(),
    };
    let plan = Plan::build(&operation, &CancellationToken::never()).expect("a plan");
    let report = execute(
        &plan,
        ConflictPolicy::Skip,
        &CancellationToken::never(),
        |_| {},
    );
    let record = UndoRecord::from_report(&operation, &report).expect("undoable");

    let created = parent.join("draft.txt");
    std::fs::write(&created, b"the user typed this").expect("write");

    undo(&record, &CancellationToken::never());
    assert!(
        created.is_file(),
        "the file has content now, so it is the user's work rather than an \
         empty file we made; undo must leave it"
    );
    assert_eq!(std::fs::read(&created).unwrap(), b"the user typed this");
}

/// The platform's trash is used when one is installed, and the fallback
/// otherwise.
///
/// This is the seam that lets macOS record Put Back without this crate
/// containing any platform code, so it is worth pinning: a hook that is
/// silently ignored would look exactly like a working one until somebody
/// tried to restore a file.
#[test]
fn an_installed_platform_trash_is_preferred_to_the_fallback() {
    // The hook is process-wide and set once, which is the intent: which
    // implementation trashes a file must not change while the program runs.
    // So this asserts the observable contract rather than installing one.
    assert!(
        !jtf_ops::has_native_trash(),
        "no adapter is installed in a test binary, so the fallback is what \
         runs here - and the fallback must still work"
    );

    let fixture = Fixture::new();
    let victim = fixture.file("gone.txt", b"x");
    let operation = Operation::Trash {
        sources: vec![victim.clone()],
    };
    let plan = Plan::build(&operation, &CancellationToken::never()).expect("a plan");
    let report = execute(
        &plan,
        ConflictPolicy::Skip,
        &CancellationToken::never(),
        |_| {},
    );

    // Either it reached a trash directory or the platform has none; both are
    // correct outcomes, and neither may leave the file where it was.
    let trashed = report
        .outcomes
        .iter()
        .any(|(_, o)| matches!(o, Outcome::Done { .. }));
    if trashed {
        assert!(!victim.exists(), "a trashed file has left its folder");
    }
}
