//! Undo.
//!
//! `AGENTS.md` §13 asks for undo **where safe**, and
//! `docs/UI_UX_SPEC.md` §10 asks the UI to say so before the action when it is
//! not. The word "safe" is doing real work here, so this module is explicit
//! about which operations qualify and why the others do not.
//!
//! | Operation | Undo | Why |
//! |---|---|---|
//! | Move | yes | move each entry back where it came from |
//! | Rename | yes | rename back |
//! | Trash | yes | move back out of the trash |
//! | New folder | yes, if still empty | removing an empty directory destroys nothing |
//! | Copy | **no** | undoing means deleting the copies, and the user may have edited one |
//! | Delete | **no** | there is nothing to put back |
//!
//! Undoing a copy is the interesting refusal. Every file manager that offers
//! it deletes what the copy created, which is fine until the user edited one
//! of those files in the seconds before pressing undo — and then it is data
//! loss dressed up as a convenience.

use std::fs;
use std::path::PathBuf;

use jtf_core::{Error, ErrorCode};
use jtf_jobs::CancellationToken;

use crate::run::{Outcome, Report};

/// One reversal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UndoStep {
    /// Move `from` back to `to`.
    MoveBack {
        /// Where the entry is now.
        from: PathBuf,
        /// Where it was.
        to: PathBuf,
    },
    /// Remove a directory this operation created, if it is still empty.
    RemoveEmptyDirectory {
        /// The directory.
        path: PathBuf,
    },
}

/// Everything needed to reverse one completed operation.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct UndoRecord {
    steps: Vec<UndoStep>,
    label_key: &'static str,
}

impl UndoRecord {
    /// Build a record, or `None` when the operation is not reversible.
    pub fn from_report(operation: &crate::plan::Operation, report: &Report) -> Option<Self> {
        use crate::plan::Operation;

        let label_key = match operation {
            Operation::Move { .. } => "command.file.move_to_target_pane",
            Operation::Rename { .. } => "command.file.rename",
            Operation::Trash { .. } => "command.file.trash",
            Operation::NewFolder { .. } => "command.file.new_folder",
            // Copy and Delete are deliberately absent; see the module note.
            Operation::Copy { .. } | Operation::Delete { .. } => return None,
        };

        let mut steps = Vec::new();
        for (source, outcome) in &report.outcomes {
            let Outcome::Done { destination } = outcome else {
                continue; // only what actually happened can be reversed
            };
            let Some(destination) = destination else {
                continue;
            };

            if matches!(operation, Operation::NewFolder { .. }) {
                steps.push(UndoStep::RemoveEmptyDirectory {
                    path: destination.clone(),
                });
            } else {
                steps.push(UndoStep::MoveBack {
                    from: destination.clone(),
                    to: source.clone(),
                });
            }
        }
        if steps.is_empty() {
            return None;
        }
        Some(Self { steps, label_key })
    }

    /// Localization key naming what would be undone.
    pub const fn label_key(&self) -> &'static str {
        self.label_key
    }

    /// How many steps.
    pub fn len(&self) -> usize {
        self.steps.len()
    }

    /// Whether there is nothing to undo.
    pub fn is_empty(&self) -> bool {
        self.steps.is_empty()
    }
}

/// Reverse a recorded operation.
///
/// Every step verifies the world still looks the way it did before acting. A
/// file that has since been replaced, moved or deleted is **skipped, not
/// forced**: undo restoring something over a newer file would be exactly the
/// data loss it exists to prevent.
pub fn undo(record: &UndoRecord, cancel: &CancellationToken) -> Report {
    let mut outcomes = Vec::with_capacity(record.steps.len());

    // Reverse order, so a move that created intermediate state is unwound the
    // way it was wound.
    for step in record.steps.iter().rev() {
        if cancel.is_cancelled() {
            return Report {
                outcomes,
                cancelled: true,
            };
        }
        match step {
            UndoStep::MoveBack { from, to } => {
                if !from.exists() {
                    outcomes.push((
                        from.clone(),
                        Outcome::Failed(Error::new(
                            ErrorCode::NotFound,
                            format!("{} is no longer there", from.display()),
                        )),
                    ));
                    continue;
                }
                if to.exists() {
                    // Something now occupies the original place. Putting the
                    // entry back would overwrite it.
                    outcomes.push((from.clone(), Outcome::Skipped));
                    continue;
                }
                if let Some(parent) = to.parent() {
                    if !parent.exists() {
                        outcomes.push((
                            from.clone(),
                            Outcome::Failed(Error::new(
                                ErrorCode::NotFound,
                                format!("{} no longer exists", parent.display()),
                            )),
                        ));
                        continue;
                    }
                }
                match fs::rename(from, to) {
                    Ok(()) => outcomes.push((
                        from.clone(),
                        Outcome::Done {
                            destination: Some(to.clone()),
                        },
                    )),
                    Err(error) => outcomes.push((
                        from.clone(),
                        Outcome::Failed(Error::new(
                            ErrorCode::Io,
                            format!("{} -> {}: {error}", from.display(), to.display()),
                        )),
                    )),
                }
            }
            UndoStep::RemoveEmptyDirectory { path } => {
                if !path.is_dir() {
                    outcomes.push((path.clone(), Outcome::Skipped));
                    continue;
                }
                // `remove_dir` refuses a non-empty directory, which is the
                // check: anything the user has since put in there survives.
                match fs::remove_dir(path) {
                    Ok(()) => outcomes.push((path.clone(), Outcome::Done { destination: None })),
                    Err(_) => outcomes.push((path.clone(), Outcome::Skipped)),
                }
            }
        }
    }
    Report {
        outcomes,
        cancelled: false,
    }
}
