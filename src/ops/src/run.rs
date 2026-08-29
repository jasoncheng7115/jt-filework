//! Execution.
//!
//! Every loop checks the cancellation token, every failure is attributed to
//! the entry that caused it, and partial completion is a first-class outcome
//! rather than something hidden behind a success message
//! (`docs/UI_UX_SPEC.md` §10).

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use jtf_core::{Error, ErrorCode};
use jtf_jobs::CancellationToken;

use crate::conflict::{unique_destination, ConflictPolicy};
use crate::plan::{Operation, Plan, Step};

/// How far along an operation is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Progress {
    /// Bytes moved so far.
    pub bytes_done: u64,
    /// Bytes the plan expected.
    pub bytes_total: u64,
    /// Entries finished so far.
    pub entries_done: u64,
    /// Entries the plan expected.
    pub entries_total: u64,
    /// What is being worked on right now, for the UI to show.
    pub current: Option<PathBuf>,
}

/// What happened to one source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// Finished.
    Done {
        /// Where it ended up, for copy and move.
        destination: Option<PathBuf>,
    },
    /// Left alone because the destination existed and the policy was to skip.
    Skipped,
    /// Failed, with the reason.
    Failed(Error),
}

/// The result of running a plan.
///
/// A caller cannot mistake this for "it worked": there is no boolean, only
/// per-entry outcomes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    /// One entry per step, in plan order.
    pub outcomes: Vec<(PathBuf, Outcome)>,
    /// Whether the run stopped early because it was cancelled.
    pub cancelled: bool,
}

impl Report {
    /// How many finished.
    pub fn succeeded(&self) -> usize {
        self.outcomes
            .iter()
            .filter(|(_, o)| matches!(o, Outcome::Done { .. }))
            .count()
    }

    /// How many were skipped.
    pub fn skipped(&self) -> usize {
        self.outcomes
            .iter()
            .filter(|(_, o)| matches!(o, Outcome::Skipped))
            .count()
    }

    /// The failures, so the UI can list exactly which entries failed and why.
    pub fn failures(&self) -> Vec<(&Path, &Error)> {
        self.outcomes
            .iter()
            .filter_map(|(path, outcome)| match outcome {
                Outcome::Failed(error) => Some((path.as_path(), error)),
                _ => None,
            })
            .collect()
    }

    /// Whether every step finished.
    pub fn is_complete(&self) -> bool {
        !self.cancelled && self.succeeded() == self.outcomes.len()
    }
}

/// Run a plan.
///
/// `on_progress` is called as work proceeds; it must be cheap, because it runs
/// on the worker thread.
pub fn execute(
    plan: &Plan,
    policy: ConflictPolicy,
    cancel: &CancellationToken,
    mut on_progress: impl FnMut(&Progress),
) -> Report {
    let mut progress = Progress {
        bytes_done: 0,
        bytes_total: plan.total_bytes,
        entries_done: 0,
        entries_total: plan.total_entries,
        current: None,
    };
    let mut outcomes = Vec::with_capacity(plan.steps.len());

    for step in &plan.steps {
        if cancel.is_cancelled() {
            return Report {
                outcomes,
                cancelled: true,
            };
        }
        progress.current = Some(step.source.clone());
        on_progress(&progress);

        let outcome = run_step(&plan.operation, step, policy, cancel);
        if matches!(outcome, Outcome::Failed(_)) && policy == ConflictPolicy::Abort {
            outcomes.push((step.source.clone(), outcome));
            return Report {
                outcomes,
                cancelled: false,
            };
        }

        progress.bytes_done += step.bytes;
        progress.entries_done += 1;
        on_progress(&progress);
        outcomes.push((step.source.clone(), outcome));
    }

    progress.current = None;
    on_progress(&progress);
    Report {
        outcomes,
        cancelled: cancel.is_cancelled(),
    }
}

fn run_step(
    operation: &Operation,
    step: &Step,
    policy: ConflictPolicy,
    cancel: &CancellationToken,
) -> Outcome {
    match operation {
        Operation::Copy { .. } => transfer(step, policy, cancel, false),
        Operation::Move { .. } => transfer(step, policy, cancel, true),
        Operation::Rename { .. } => match resolve_target(step, policy) {
            Ok(None) => Outcome::Skipped,
            Ok(Some(target)) => match fs::rename(&step.source, &target) {
                Ok(()) => Outcome::Done {
                    destination: Some(target),
                },
                Err(e) => Outcome::Failed(io_error(&step.source, &e)),
            },
            Err(error) => Outcome::Failed(error),
        },
        Operation::NewFolder { .. } => match fs::create_dir(&step.source) {
            Ok(()) => Outcome::Done {
                destination: Some(step.source.clone()),
            },
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => Outcome::Skipped,
            Err(e) => Outcome::Failed(io_error(&step.source, &e)),
        },
        Operation::Trash { .. } => match crate::trash::trash_entry(&step.source) {
            Ok(target) => Outcome::Done {
                destination: Some(target),
            },
            Err(error) => Outcome::Failed(error),
        },
        Operation::Delete { .. } => match remove_tree(&step.source) {
            Ok(()) => Outcome::Done { destination: None },
            Err(error) => Outcome::Failed(error),
        },
    }
}

/// Work out where a step actually lands, applying the conflict policy.
///
/// `Ok(None)` means "skip this one", which is a decision rather than a
/// failure.
fn resolve_target(step: &Step, policy: ConflictPolicy) -> Result<Option<PathBuf>, Error> {
    let Some(target) = step.destination.clone() else {
        return Ok(None);
    };
    if !target.exists() {
        return Ok(Some(target));
    }
    match policy {
        ConflictPolicy::Skip | ConflictPolicy::Abort => Ok(None),
        ConflictPolicy::Overwrite => Ok(Some(target)),
        ConflictPolicy::KeepBoth => unique_destination(&target)
            .map(Some)
            .ok_or_else(|| Error::new(ErrorCode::AlreadyExists, "no free name")),
    }
}

fn transfer(
    step: &Step,
    policy: ConflictPolicy,
    cancel: &CancellationToken,
    remove_source: bool,
) -> Outcome {
    let target = match resolve_target(step, policy) {
        Ok(Some(target)) => target,
        Ok(None) => return Outcome::Skipped,
        Err(error) => return Outcome::Failed(error),
    };

    if policy == ConflictPolicy::Overwrite && target.exists() {
        if let Err(error) = remove_tree(&target) {
            return Outcome::Failed(error);
        }
    }

    if remove_source {
        // Same volume: a rename is atomic and instant, and is what "move"
        // should be whenever it can be.
        match fs::rename(&step.source, &target) {
            Ok(()) => {
                return Outcome::Done {
                    destination: Some(target),
                }
            }
            Err(error) if error.raw_os_error() != Some(18) => {
                return Outcome::Failed(io_error(&step.source, &error));
            }
            Err(_) => {} // EXDEV: fall through to copy-then-remove
        }
    }

    if let Err(error) = copy_tree(&step.source, &target, cancel) {
        return Outcome::Failed(error);
    }
    if remove_source {
        if let Err(error) = remove_tree(&step.source) {
            // The copy succeeded, so the data is safe; report the failure
            // against the source rather than claiming the move completed.
            return Outcome::Failed(error);
        }
    }
    Outcome::Done {
        destination: Some(target),
    }
}

/// Copy a file, a symlink or a whole tree.
///
/// Iterative, and never follows a symlink: a link is recreated as a link, so
/// a copy cannot silently pull in whatever it pointed at, and a link pointing
/// outside the tree cannot make the copy escape (`docs/SECURITY.md` §3.1).
pub(crate) fn copy_tree(
    source: &Path,
    target: &Path,
    cancel: &CancellationToken,
) -> Result<(), Error> {
    let meta = fs::symlink_metadata(source).map_err(|e| io_error(source, &e))?;

    if meta.file_type().is_symlink() {
        return copy_symlink(source, target);
    }
    if !meta.is_dir() {
        fs::copy(source, target).map_err(|e| io_error(source, &e))?;
        return Ok(());
    }

    fs::create_dir_all(target).map_err(|e| io_error(target, &e))?;
    let mut stack = vec![(source.to_path_buf(), target.to_path_buf())];

    while let Some((from, to)) = stack.pop() {
        if cancel.is_cancelled() {
            return Err(Error::bare(ErrorCode::Cancelled));
        }
        let read_dir = fs::read_dir(&from).map_err(|e| io_error(&from, &e))?;
        for entry in read_dir {
            if cancel.is_cancelled() {
                return Err(Error::bare(ErrorCode::Cancelled));
            }
            let entry = entry.map_err(|e| io_error(&from, &e))?;
            let child_from = entry.path();
            let child_to = to.join(entry.file_name());
            let child_meta =
                fs::symlink_metadata(&child_from).map_err(|e| io_error(&child_from, &e))?;

            if child_meta.file_type().is_symlink() {
                copy_symlink(&child_from, &child_to)?;
            } else if child_meta.is_dir() {
                fs::create_dir_all(&child_to).map_err(|e| io_error(&child_to, &e))?;
                stack.push((child_from, child_to));
            } else {
                fs::copy(&child_from, &child_to).map_err(|e| io_error(&child_from, &e))?;
            }
        }
    }
    Ok(())
}

#[cfg(unix)]
fn copy_symlink(source: &Path, target: &Path) -> Result<(), Error> {
    let link = fs::read_link(source).map_err(|e| io_error(source, &e))?;
    std::os::unix::fs::symlink(link, target).map_err(|e| io_error(target, &e))
}

#[cfg(not(unix))]
fn copy_symlink(source: &Path, target: &Path) -> Result<(), Error> {
    // Creating a symlink on Windows needs a privilege that is often absent.
    // Copying the target's contents instead would silently change what the
    // user asked for, so this reports rather than guesses.
    let _ = (source, target);
    Err(Error::new(
        ErrorCode::Unsupported,
        "copying a symbolic link is not supported on this platform yet",
    ))
}

/// Remove a file, a symlink or a whole tree.
///
/// Iterative, and removes a symlink as a link rather than descending through
/// it. Following one here would delete files outside the tree the user
/// selected, which is the single most damaging bug a file manager can have.
pub(crate) fn remove_tree(path: &Path) -> Result<(), Error> {
    let meta = fs::symlink_metadata(path).map_err(|e| io_error(path, &e))?;

    if meta.file_type().is_symlink() || !meta.is_dir() {
        return fs::remove_file(path).map_err(|e| io_error(path, &e));
    }

    // Collect depth-first, delete deepest first: a directory cannot be removed
    // until it is empty.
    let mut directories = Vec::new();
    let mut stack = vec![path.to_path_buf()];

    while let Some(dir) = stack.pop() {
        directories.push(dir.clone());
        let read_dir = fs::read_dir(&dir).map_err(|e| io_error(&dir, &e))?;
        for entry in read_dir {
            let entry = entry.map_err(|e| io_error(&dir, &e))?;
            let child = entry.path();
            let child_meta = fs::symlink_metadata(&child).map_err(|e| io_error(&child, &e))?;
            if child_meta.file_type().is_symlink() || !child_meta.is_dir() {
                fs::remove_file(&child).map_err(|e| io_error(&child, &e))?;
            } else {
                stack.push(child);
            }
        }
    }

    for dir in directories.into_iter().rev() {
        fs::remove_dir(&dir).map_err(|e| io_error(&dir, &e))?;
    }
    Ok(())
}

fn io_error(path: &Path, error: &io::Error) -> Error {
    let code = match error.kind() {
        io::ErrorKind::NotFound => ErrorCode::NotFound,
        io::ErrorKind::PermissionDenied => ErrorCode::PermissionDenied,
        io::ErrorKind::AlreadyExists => ErrorCode::AlreadyExists,
        io::ErrorKind::TimedOut => ErrorCode::TimedOut,
        _ => ErrorCode::Io,
    };
    Error::new(code, format!("{}: {error}", path.display()))
}
