//! Comparing the contents of two folders.
//!
//! Two panes, side by side, and the question「這兩邊差在哪」. The answer is a
//! flat list of names with a verdict against each: only here, only there,
//! present on both but not the same, or the same.
//!
//! What "the same" means is deliberately narrow. Two files match when their
//! sizes match and their modification times are within [`TIME_SLACK`] of each
//! other. Reading both files through to compare bytes would be the only
//! certain answer, and on two folders of any size that is a disk-bound job
//! rather than a glance — so this is the cheap comparison, named as such
//! wherever it is shown, and the expensive one is not pretended to.
//!
//! Listing is done through a caller-supplied closure rather than a provider,
//! so the same code compares two local folders, two folders on a server, or
//! one of each, without this module knowing which is which.
//!
//! Iterative rather than recursive: a directory tree is untrusted input, and
//! a symlink loop or a pathologically deep tree must not consume the stack
//! (`AGENTS.md` §20.2, `docs/SECURITY.md` §13).

use std::collections::{BTreeMap, VecDeque};
use std::time::{Duration, SystemTime};

use jtf_core::{Error, ErrorCode, FileEntry, FileKind, Location, Result};
use jtf_jobs::CancellationToken;

/// How far apart two modification times may be and still count as equal.
///
/// FAT keeps two-second granularity, and a file copied between filesystems
/// routinely lands a second either side of its source. A stricter rule reports
/// every copied file as different, which makes the whole comparison useless.
pub const TIME_SLACK: Duration = Duration::from_secs(2);

/// How deep a recursive comparison will walk.
///
/// The same bound the size measurement and the search walker use. A tree
/// deeper than this is either a mistake or a loop, and either way the answer
/// is to stop.
pub const MAX_DEPTH: usize = 64;

/// How many rows a comparison will produce before it gives up.
///
/// A comparison of two large trees is a list nobody can read and a table Qt
/// has to hold in memory. Stopping and saying so is better than growing until
/// the window does.
pub const MAX_ROWS: usize = 200_000;

/// What a comparison found about one name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Difference {
    /// Present on the left only.
    OnlyLeft,
    /// Present on the right only.
    OnlyRight,
    /// On both sides, and not the same by the rule above.
    Differs,
    /// On both sides, and the same by the rule above.
    Same,
}

impl Difference {
    /// Whether this is a difference at all.
    ///
    /// The window hides matches by default: a list where most rows say
    /// "identical" buries the handful that do not.
    #[must_use]
    pub const fn is_difference(self) -> bool {
        !matches!(self, Self::Same)
    }

    /// A stable name, for the interface to look up and for tests to assert on.
    #[must_use]
    pub const fn id(self) -> &'static str {
        match self {
            Self::OnlyLeft => "only_left",
            Self::OnlyRight => "only_right",
            Self::Differs => "differs",
            Self::Same => "same",
        }
    }
}

/// One side's facts about a name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Side {
    /// Size in bytes; `None` for a folder, or when the provider did not say.
    pub size: Option<u64>,
    /// Modification time, if the provider knew it.
    pub modified: Option<SystemTime>,
    /// Whether this is a folder.
    pub is_directory: bool,
}

/// One row of a comparison.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    /// The path relative to both roots, `/`-separated. At the top level this
    /// is just the name.
    pub relative: String,
    /// What was found.
    pub difference: Difference,
    /// The left side, if it has this name.
    pub left: Option<Side>,
    /// The right side, if it has this name.
    pub right: Option<Side>,
}

impl Row {
    /// Whether either side is a folder.
    #[must_use]
    pub fn is_directory(&self) -> bool {
        self.left.is_some_and(|side| side.is_directory)
            || self.right.is_some_and(|side| side.is_directory)
    }
}

/// How far a comparison has got.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Progress {
    /// Folders read so far, both sides counted together.
    pub folders: u64,
    /// Rows produced so far.
    pub rows: u64,
    /// Of those, how many are an actual difference.
    pub differences: u64,
}

/// What a comparison produced.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Comparison {
    /// The rows, sorted by relative path.
    pub rows: Vec<Row>,
    /// Whether [`MAX_ROWS`] cut the walk short. The interface has to say so:
    /// a truncated comparison that looks complete is a wrong answer.
    pub truncated: bool,
}

impl Comparison {
    /// How many rows are an actual difference.
    #[must_use]
    pub fn difference_count(&self) -> usize {
        self.rows
            .iter()
            .filter(|row| row.difference.is_difference())
            .count()
    }
}

/// Compare `left` against `right`.
///
/// `list` enumerates one location; it is the caller's provider, so this works
/// for local folders, remote ones, or a mix. A side that cannot be listed at
/// all is an error — comparing against a folder we could not read would
/// report every name on the other side as "only there", which is a confident
/// wrong answer. A *sub*folder that cannot be read is different: the walk
/// keeps going and that subtree is simply not descended into, because one
/// unreadable directory deep in a tree should not throw away the rest.
///
/// # Errors
///
/// If either root cannot be listed.
pub fn compare(
    left: &Location,
    right: &Location,
    list: &dyn Fn(&Location) -> Result<Vec<FileEntry>>,
    recursive: bool,
    cancel: &CancellationToken,
) -> Result<Comparison> {
    compare_with(left, right, list, recursive, cancel, &mut |_| {})
}

/// Compare `left` against `right`, reporting as it goes.
///
/// `progress` is called with the running totals each time a folder has been
/// read. A comparison of two large trees takes as long as reading them, and a
/// window that says nothing for that whole time is indistinguishable from one
/// that has hung - so the caller is given something to show.
///
/// # Errors
///
/// If either root cannot be listed.
pub fn compare_with(
    left: &Location,
    right: &Location,
    list: &dyn Fn(&Location) -> Result<Vec<FileEntry>>,
    recursive: bool,
    cancel: &CancellationToken,
    progress: &mut dyn FnMut(Progress),
) -> Result<Comparison> {
    // Both roots, up front, so an unreadable one fails before any work.
    let left_root = list(left)?;
    let right_root = list(right)?;

    let mut comparison = Comparison::default();
    // (relative prefix, left listing, right listing, depth). The roots' own
    // listings are already in hand; deeper ones are fetched as they come off
    // the queue.
    let mut queue: VecDeque<(String, Vec<FileEntry>, Vec<FileEntry>, usize)> = VecDeque::new();
    queue.push_back((String::new(), left_root, right_root, 0));
    let mut folders = 2u64;
    let mut differences = 0u64;

    while let Some((prefix, left_entries, right_entries, depth)) = queue.pop_front() {
        if cancel.is_cancelled() {
            break;
        }
        let mut names: BTreeMap<String, (Option<&FileEntry>, Option<&FileEntry>)> = BTreeMap::new();
        for entry in &left_entries {
            names.entry(entry.display_name()).or_default().0 = Some(entry);
        }
        for entry in &right_entries {
            names.entry(entry.display_name()).or_default().1 = Some(entry);
        }

        for (name, (left_entry, right_entry)) in names {
            if comparison.rows.len() >= MAX_ROWS {
                comparison.truncated = true;
                return Ok(finish(comparison));
            }
            let relative = if prefix.is_empty() {
                name.clone()
            } else {
                format!("{prefix}/{name}")
            };
            let left_side = left_entry.map(side_of);
            let right_side = right_entry.map(side_of);
            let difference = verdict(left_side, right_side);
            if difference.is_difference() {
                differences += 1;
            }
            comparison.rows.push(Row {
                relative: relative.clone(),
                difference,
                left: left_side,
                right: right_side,
            });

            // Descend only where both sides have a folder of that name. A
            // folder on one side only is a single row saying so; walking it to
            // list every file underneath as "only here" turns one fact the
            // reader already understands into hundreds of rows.
            if !recursive || depth + 1 >= MAX_DEPTH {
                continue;
            }
            let (Some(left_entry), Some(right_entry)) = (left_entry, right_entry) else {
                continue;
            };
            if !is_walkable(left_entry) || !is_walkable(right_entry) {
                continue;
            }
            // An unreadable subfolder stops that subtree and nothing else.
            let (Ok(left_children), Ok(right_children)) =
                (list(left_entry.location()), list(right_entry.location()))
            else {
                continue;
            };
            folders += 2;
            queue.push_back((relative, left_children, right_children, depth + 1));
        }
        // Once per folder read, not once per row: a report per row on a large
        // tree costs more than the comparison.
        progress(Progress {
            folders,
            rows: comparison.rows.len() as u64,
            differences,
        });
    }

    Ok(finish(comparison))
}

/// Sort by path so the two sides read down the page together.
fn finish(mut comparison: Comparison) -> Comparison {
    comparison.rows.sort_by(|a, b| a.relative.cmp(&b.relative));
    comparison
}

/// A folder we are willing to walk into.
///
/// Symlinks are not followed: a link is a name, and following it is how a
/// comparison walks out of the tree it was asked about, or around a loop
/// (`docs/SECURITY.md` §3.1). Bundles and packages are one item to the user,
/// so they are compared as one item here too.
fn is_walkable(entry: &FileEntry) -> bool {
    matches!(entry.kind(), FileKind::Directory)
}

fn side_of(entry: &FileEntry) -> Side {
    Side {
        size: entry.size(),
        modified: entry.timestamps().modified,
        is_directory: entry.kind().is_directory_on_disk(),
    }
}

/// The verdict for one name, given what each side has.
fn verdict(left: Option<Side>, right: Option<Side>) -> Difference {
    let (Some(left), Some(right)) = (left, right) else {
        return if left.is_some() {
            Difference::OnlyLeft
        } else {
            Difference::OnlyRight
        };
    };
    // A folder against a file of the same name is as different as it gets.
    if left.is_directory != right.is_directory {
        return Difference::Differs;
    }
    // Two folders of the same name are the same *name*. Whether what is inside
    // them matches is the recursive walk's answer, not this row's.
    if left.is_directory {
        return Difference::Same;
    }
    if left.size != right.size {
        return Difference::Differs;
    }
    if same_time(left.modified, right.modified) {
        Difference::Same
    } else {
        Difference::Differs
    }
}

/// Whether two modification times count as equal.
///
/// Two unknown times are not evidence of a difference — a provider that does
/// not report times would otherwise mark every file as differing.
fn same_time(left: Option<SystemTime>, right: Option<SystemTime>) -> bool {
    match (left, right) {
        (Some(left), Some(right)) => {
            let gap = left
                .duration_since(right)
                .unwrap_or_else(|error| error.duration());
            gap <= TIME_SLACK
        }
        (None, None) => true,
        _ => false,
    }
}

/// The error a caller should report when a side cannot be listed.
///
/// Kept here so the message about a failed comparison is written once.
#[must_use]
pub fn unreadable(location: &Location) -> Error {
    Error::new(
        ErrorCode::Io,
        format!("cannot list {}", location.display_text()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use jtf_core::{RawName, Timestamps};
    use std::collections::HashMap;

    /// A listing built by hand, so the tests do not need a disk.
    struct Fake {
        listings: HashMap<String, Vec<FileEntry>>,
    }

    impl Fake {
        fn new() -> Self {
            Self {
                listings: HashMap::new(),
            }
        }

        fn dir(mut self, at: &str, names: &[&str]) -> Self {
            let entries = names
                .iter()
                .map(|name| {
                    let path = format!("{at}/{name}");
                    FileEntry::new(
                        Location::local(&path),
                        RawName::new(*name),
                        FileKind::Directory,
                    )
                })
                .collect();
            self.listings.insert(at.to_string(), entries);
            self
        }

        fn files(mut self, at: &str, files: &[(&str, u64, u64)]) -> Self {
            let entries = files
                .iter()
                .map(|(name, size, secs)| {
                    FileEntry::new(
                        Location::local(format!("{at}/{name}")),
                        RawName::new(*name),
                        FileKind::File,
                    )
                    .with_size(*size)
                    .with_timestamps(Timestamps {
                        modified: Some(SystemTime::UNIX_EPOCH + Duration::from_secs(*secs)),
                        ..Timestamps::default()
                    })
                })
                .collect();
            self.listings.insert(at.to_string(), entries);
            self
        }

        fn add(mut self, at: &str, entries: Vec<FileEntry>) -> Self {
            self.listings.insert(at.to_string(), entries);
            self
        }

        fn list(&self) -> impl Fn(&Location) -> Result<Vec<FileEntry>> + '_ {
            move |location| {
                self.listings
                    .get(&location.display_text())
                    .cloned()
                    .ok_or_else(|| unreadable(location))
            }
        }
    }

    fn run(fake: &Fake, recursive: bool) -> Comparison {
        let list = fake.list();
        compare(
            &Location::local("/left"),
            &Location::local("/right"),
            &list,
            recursive,
            &CancellationToken::never(),
        )
        .expect("both roots are listable")
    }

    fn verdicts(comparison: &Comparison) -> Vec<(String, &'static str)> {
        comparison
            .rows
            .iter()
            .map(|row| (row.relative.clone(), row.difference.id()))
            .collect()
    }

    #[test]
    fn a_name_on_one_side_only_says_which_side() {
        let fake = Fake::new()
            .files("/left", &[("here.txt", 1, 100)])
            .files("/right", &[("there.txt", 1, 100)]);
        assert_eq!(
            verdicts(&run(&fake, false)),
            vec![
                ("here.txt".to_string(), "only_left"),
                ("there.txt".to_string(), "only_right"),
            ]
        );
    }

    #[test]
    fn files_matching_in_size_and_time_are_the_same() {
        let fake = Fake::new()
            .files("/left", &[("a.txt", 10, 100)])
            .files("/right", &[("a.txt", 10, 100)]);
        assert_eq!(
            verdicts(&run(&fake, false)),
            vec![("a.txt".to_string(), "same")]
        );
    }

    #[test]
    fn a_different_size_is_a_difference() {
        let fake = Fake::new()
            .files("/left", &[("a.txt", 10, 100)])
            .files("/right", &[("a.txt", 11, 100)]);
        assert_eq!(
            verdicts(&run(&fake, false)),
            vec![("a.txt".to_string(), "differs")]
        );
    }

    /// A file copied between filesystems lands a second either side of its
    /// source. Calling that a difference would flag every copied file.
    #[test]
    fn a_time_within_the_slack_is_still_the_same() {
        let fake = Fake::new()
            .files("/left", &[("a.txt", 10, 100)])
            .files("/right", &[("a.txt", 10, 102)]);
        assert_eq!(
            verdicts(&run(&fake, false)),
            vec![("a.txt".to_string(), "same")]
        );

        let wider = Fake::new()
            .files("/left", &[("a.txt", 10, 100)])
            .files("/right", &[("a.txt", 10, 103)]);
        assert_eq!(
            verdicts(&run(&wider, false)),
            vec![("a.txt".to_string(), "differs")]
        );
    }

    /// Whichever side is older, the gap is the same gap.
    #[test]
    fn the_time_comparison_does_not_depend_on_the_order() {
        let newer_on_the_right = Fake::new()
            .files("/left", &[("a.txt", 10, 100)])
            .files("/right", &[("a.txt", 10, 110)]);
        let newer_on_the_left = Fake::new()
            .files("/left", &[("a.txt", 10, 110)])
            .files("/right", &[("a.txt", 10, 100)]);
        assert_eq!(
            verdicts(&run(&newer_on_the_right, false)),
            verdicts(&run(&newer_on_the_left, false))
        );
    }

    #[test]
    fn a_folder_against_a_file_of_the_same_name_differs() {
        let fake = Fake::new()
            .dir("/left", &["thing"])
            .files("/right", &[("thing", 10, 100)]);
        assert_eq!(
            verdicts(&run(&fake, false)),
            vec![("thing".to_string(), "differs")]
        );
    }

    /// Without the box ticked, a folder on both sides is one row and the walk
    /// stops there.
    #[test]
    fn a_shallow_comparison_does_not_look_inside_folders() {
        let fake = Fake::new()
            .dir("/left", &["sub"])
            .dir("/right", &["sub"])
            .files("/left/sub", &[("deep.txt", 1, 100)])
            .files("/right/sub", &[("deep.txt", 2, 100)]);
        assert_eq!(
            verdicts(&run(&fake, false)),
            vec![("sub".to_string(), "same")]
        );
    }

    #[test]
    fn a_recursive_comparison_reports_the_path_below() {
        let fake = Fake::new()
            .dir("/left", &["sub"])
            .dir("/right", &["sub"])
            .files("/left/sub", &[("deep.txt", 1, 100)])
            .files("/right/sub", &[("deep.txt", 2, 100)]);
        assert_eq!(
            verdicts(&run(&fake, true)),
            vec![
                ("sub".to_string(), "same"),
                ("sub/deep.txt".to_string(), "differs"),
            ]
        );
    }

    /// A folder only one side has is one row. Listing everything inside it as
    /// "only here" restates a fact the reader already has, once per file.
    #[test]
    fn a_folder_on_one_side_only_is_not_walked() {
        let fake = Fake::new()
            .dir("/left", &["sub"])
            .files("/right", &[])
            .files("/left/sub", &[("a.txt", 1, 100), ("b.txt", 1, 100)]);
        assert_eq!(
            verdicts(&run(&fake, true)),
            vec![("sub".to_string(), "only_left")]
        );
    }

    /// One unreadable subfolder must not throw away the rest of the answer.
    #[test]
    fn an_unreadable_subfolder_stops_that_subtree_only() {
        let fake = Fake::new()
            .dir("/left", &["locked", "open"])
            .dir("/right", &["locked", "open"])
            .files("/left/open", &[("a.txt", 1, 100)])
            .files("/right/open", &[("a.txt", 2, 100)]);
        // `/left/locked` and `/right/locked` have no listing at all.
        assert_eq!(
            verdicts(&run(&fake, true)),
            vec![
                ("locked".to_string(), "same"),
                ("open".to_string(), "same"),
                ("open/a.txt".to_string(), "differs"),
            ]
        );
    }

    /// A root that cannot be read is an error, not an answer. Reporting every
    /// name on the other side as "only there" would be a confident lie.
    #[test]
    fn an_unreadable_root_is_an_error() {
        let fake = Fake::new().files("/left", &[("a.txt", 1, 100)]);
        let list = fake.list();
        assert!(compare(
            &Location::local("/left"),
            &Location::local("/right"),
            &list,
            false,
            &CancellationToken::never(),
        )
        .is_err());
    }

    /// A symlink is a name, not a door. Following one is how a comparison
    /// walks out of the tree it was asked about, or around a loop.
    #[test]
    fn a_symlink_is_not_walked_into() {
        let link = |at: &str| {
            vec![FileEntry::new(
                Location::local(format!("{at}/loop")),
                RawName::new("loop"),
                FileKind::Symlink,
            )]
        };
        let fake = Fake::new()
            .add("/left", link("/left"))
            .add("/right", link("/right"))
            // Present, so a walk that followed the link would find it.
            .files("/left/loop", &[("inside.txt", 1, 100)])
            .files("/right/loop", &[("inside.txt", 2, 100)]);
        assert_eq!(
            verdicts(&run(&fake, true)),
            vec![("loop".to_string(), "same")]
        );
    }

    #[test]
    fn cancelling_stops_the_walk() {
        let fake = Fake::new()
            .files("/left", &[("a.txt", 1, 100)])
            .files("/right", &[("a.txt", 2, 100)]);
        let list = fake.list();
        let cancel = CancellationToken::cancelled();
        let comparison = compare(
            &Location::local("/left"),
            &Location::local("/right"),
            &list,
            false,
            &cancel,
        )
        .expect("the roots were listed before the walk began");
        assert!(comparison.rows.is_empty());
    }

    #[test]
    fn the_difference_count_ignores_matches() {
        let fake = Fake::new()
            .files("/left", &[("same.txt", 1, 100), ("other.txt", 1, 100)])
            .files("/right", &[("same.txt", 1, 100)]);
        let comparison = run(&fake, false);
        assert_eq!(comparison.rows.len(), 2);
        assert_eq!(comparison.difference_count(), 1);
    }
}
