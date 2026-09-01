//! File operations.
//!
//! `AGENTS.md` §13: copy, move, rename, delete, trash and the rest are Jobs,
//! with progress, cancellation, conflict resolution, logging, error detail,
//! retry where safe and undo where safe.
//!
//! Everything here is written to be run on a worker thread and to check its
//! cancellation token at every step, because a copy across a network mount can
//! take minutes and must remain interruptible.
//!
//! # What this module refuses to do
//!
//! - follow a symlink out of the tree it was told to act on
//!   (`docs/SECURITY.md` §3.1)
//! - write outside the destination root
//! - report success for work it did not finish
//! - lose the other 999 entries because one of them failed

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

mod batch;
mod conflict;
mod plan;
mod run;
mod trash;
mod undo;

pub use batch::{
    apply as apply_batch_rename, preview as preview_batch_rename, RenameIssue, RenamePattern,
    RenamePreview, RenameRow,
};
pub use conflict::{unique_destination, Conflict, ConflictPolicy};
pub use plan::{Operation, Plan, PlanError};
pub use run::{execute, Outcome, Progress as OpProgress, Report};
pub use trash::{has_native_trash, set_native_trash, trash_directory, NativeTrash};
pub use undo::{undo, UndoRecord, UndoStep, MAX_STEPS};
