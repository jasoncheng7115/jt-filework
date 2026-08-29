//! The job engine.
//!
//! `AGENTS.md` §13: copy, move, rename, delete, trash, extract, compress,
//! hash and batch rename are Jobs, and every Job has progress, cancellation,
//! conflict resolution, logging, error detail, retry where safe and undo where
//! safe. `AGENTS.md` §3 adds that everything expensive — enumeration,
//! thumbnails, preview, indexing, AI calls, external agents — runs here rather
//! than on the UI thread.
//!
//! This crate owns the *contract*: what states a job can be in, how it reports
//! progress, and how it is cancelled. Execution lives with the subsystem that
//! knows the work.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

mod cancel;
mod job;
mod progress;
mod state;

pub use cancel::{CancellationToken, Cancelled, Canceller};
pub use job::{Job, JobId, JobKind};
pub use progress::Progress;
pub use state::{JobState, TransitionError};
