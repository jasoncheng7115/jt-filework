//! Batch rename: the preview, and the collision cases that make it dangerous.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use jtf_ops::{apply_batch_rename, preview_batch_rename, RenameIssue, RenamePattern};

static COUNTER: AtomicU64 = AtomicU64::new(0);

struct Dir {
    root: PathBuf,
}

impl Dir {
    fn new(names: &[&str]) -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        let unique = COUNTER.fetch_add(1, Ordering::SeqCst);
        let root =
            std::env::temp_dir().join(format!("jtf-batch-{}-{nanos}-{unique}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        for name in names {
            fs::write(root.join(name), name.as_bytes()).unwrap();
        }
        Self { root }
    }

    fn paths(&self, names: &[&str]) -> Vec<PathBuf> {
        names.iter().map(|name| self.root.join(name)).collect()
    }

    fn listing(&self) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(&self.root)
            .unwrap()
            .flatten()
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn pattern(template: &str) -> RenamePattern {
    RenamePattern {
        template: template.to_string(),
        ..RenamePattern::default()
    }
}

#[test]
fn a_counter_numbers_the_batch() {
    let dir = Dir::new(&["b.txt", "a.txt", "c.txt"]);
    let sources = dir.paths(&["a.txt", "b.txt", "c.txt"]);

    let preview = preview_batch_rename(&sources, &pattern("photo-{n:3}.{ext}"));
    assert_eq!(
        preview
            .rows
            .iter()
            .map(|r| r.to.clone())
            .collect::<Vec<_>>(),
        vec!["photo-001.txt", "photo-002.txt", "photo-003.txt"]
    );

    apply_batch_rename(&preview).unwrap();
    assert_eq!(
        dir.listing(),
        vec!["photo-001.txt", "photo-002.txt", "photo-003.txt"]
    );
}

#[test]
fn placeholders_expand_and_unknown_ones_are_left_alone() {
    let dir = Dir::new(&["Report.TXT"]);
    let sources = dir.paths(&["Report.TXT"]);

    let preview = preview_batch_rename(&sources, &pattern("{lower}-{upper}-{nope}.{ext}"));
    assert_eq!(preview.rows[0].to, "report-REPORT-{nope}.TXT");
}

#[test]
fn find_and_replace_works_plainly_and_as_a_regex() {
    let dir = Dir::new(&["draft_2024_final.txt"]);
    let sources = dir.paths(&["draft_2024_final.txt"]);

    let plain = RenamePattern {
        find: "_".into(),
        replace: "-".into(),
        ..pattern("{name}.{ext}")
    };
    assert_eq!(
        preview_batch_rename(&sources, &plain).rows[0].to,
        "draft-2024-final.txt"
    );

    let expression = RenamePattern {
        find: r"\d+".into(),
        replace: "YEAR".into(),
        regex: true,
        ..pattern("{name}.{ext}")
    };
    assert_eq!(
        preview_batch_rename(&sources, &expression).rows[0].to,
        "draft_YEAR_final.txt"
    );
}

#[test]
fn a_bad_expression_marks_every_row_invalid_rather_than_matching_nothing() {
    // Matching nothing would look like the pattern did work.
    let dir = Dir::new(&["a.txt"]);
    let sources = dir.paths(&["a.txt"]);
    let bad = RenamePattern {
        find: "[".into(),
        regex: true,
        ..pattern("{name}.{ext}")
    };

    let preview = preview_batch_rename(&sources, &bad);
    assert_eq!(preview.rows[0].issue, RenameIssue::Invalid);
    assert!(preview.is_blocked());
}

#[test]
fn an_unchanged_name_is_reported_and_skipped_rather_than_renamed() {
    let dir = Dir::new(&["a.txt"]);
    let sources = dir.paths(&["a.txt"]);
    let preview = preview_batch_rename(&sources, &pattern("{name}.{ext}"));

    assert_eq!(preview.rows[0].issue, RenameIssue::Unchanged);
    assert!(!preview.has_changes());
    assert!(!preview.is_blocked(), "nothing to do is not an error");
    assert!(apply_batch_rename(&preview).unwrap().is_empty());
}

#[test]
fn two_rows_landing_on_the_same_name_block_the_batch() {
    let dir = Dir::new(&["a.txt", "b.txt"]);
    let sources = dir.paths(&["a.txt", "b.txt"]);
    let preview = preview_batch_rename(&sources, &pattern("same.{ext}"));

    assert!(preview
        .rows
        .iter()
        .all(|r| r.issue == RenameIssue::Duplicate));
    assert!(preview.is_blocked());

    // A blocked batch is never partially applied.
    assert!(apply_batch_rename(&preview).is_err());
    assert_eq!(dir.listing(), vec!["a.txt", "b.txt"], "nothing moved");
}

#[test]
fn colliding_with_a_file_outside_the_batch_blocks_it() {
    let dir = Dir::new(&["a.txt", "taken.txt"]);
    let sources = dir.paths(&["a.txt"]);
    let preview = preview_batch_rename(&sources, &pattern("taken.{ext}"));

    assert_eq!(preview.rows[0].issue, RenameIssue::Exists);
    assert!(apply_batch_rename(&preview).is_err());
}

#[test]
fn swapping_two_names_works_because_the_apply_is_two_phase() {
    // The case a naive implementation destroys a file on, in either order.
    let dir = Dir::new(&["a.txt", "b.txt"]);
    let sources = dir.paths(&["a.txt", "b.txt"]);

    let swap = RenamePattern {
        find: "a".into(),
        replace: "TEMP".into(),
        ..pattern("{name}.{ext}")
    };
    // Build the swap explicitly rather than through a pattern.
    let mut preview = preview_batch_rename(&sources, &swap);
    preview.rows[0].to = "b.txt".into();
    preview.rows[0].issue = RenameIssue::Ok;
    preview.rows[1].to = "a.txt".into();
    preview.rows[1].issue = RenameIssue::Ok;

    apply_batch_rename(&preview).unwrap();

    assert_eq!(dir.listing(), vec!["a.txt", "b.txt"], "both still exist");
    assert_eq!(fs::read_to_string(dir.root.join("a.txt")).unwrap(), "b.txt");
    assert_eq!(fs::read_to_string(dir.root.join("b.txt")).unwrap(), "a.txt");
}

#[test]
fn a_name_that_would_escape_its_directory_is_invalid() {
    let dir = Dir::new(&["a.txt"]);
    let sources = dir.paths(&["a.txt"]);
    for template in ["../escape.txt", "sub/dir.txt", "", "."] {
        let preview = preview_batch_rename(&sources, &pattern(template));
        assert_eq!(
            preview.rows[0].issue,
            RenameIssue::Invalid,
            "{template:?} must be refused"
        );
    }
}

#[test]
fn a_file_without_an_extension_does_not_gain_a_trailing_dot() {
    let dir = Dir::new(&["README"]);
    let sources = dir.paths(&["README"]);
    let preview = preview_batch_rename(&sources, &pattern("{lower}.{ext}"));
    assert_eq!(preview.rows[0].to, "readme");
}

#[test]
fn the_preview_counts_what_it_would_change() {
    let dir = Dir::new(&["a.txt", "b.txt", "c.txt"]);
    let sources = dir.paths(&["a.txt", "b.txt", "c.txt"]);
    let mut preview = preview_batch_rename(&sources, &pattern("x{n}.{ext}"));
    preview.rows[1].issue = RenameIssue::Unchanged;

    assert_eq!(preview.change_count(), 2);
    assert!(preview.has_changes());
}
