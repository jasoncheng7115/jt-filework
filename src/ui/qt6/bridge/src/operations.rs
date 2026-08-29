//! Running a file operation on a worker thread.
//!
//! The flow is deliberately three steps, because the middle one is where the
//! user gets to say no:
//!
//! ```text
//! prepare  ->  a plan: totals, conflicts, and whether it is irreversible
//! confirm  ->  the UI shows what will happen and asks
//! start    ->  a worker thread runs it, reporting progress
//! ```
//!
//! `AGENTS.md` §3: none of this happens on the UI thread. `AGENTS.md` §13: it
//! has progress, cancellation, error detail, and a per-entry result.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread;

use jtf_jobs::{CancellationToken, Canceller};
use jtf_ops::{execute, ConflictPolicy, OpProgress, Operation, Plan, Report};

/// What a worker publishes as it goes.
#[derive(Debug, Default)]
pub(crate) struct Shared {
    pub progress: Option<OpProgress>,
    pub report: Option<Report>,
}

/// A running operation.
pub(crate) struct Running {
    canceller: Canceller,
    shared: Arc<Mutex<Shared>>,
    join: Option<thread::JoinHandle<()>>,
    kind: jtf_jobs::JobKind,
}

impl Running {
    /// Spawn a worker for `plan`.
    pub(crate) fn start(plan: Plan, policy: ConflictPolicy) -> Self {
        let (token, canceller) = CancellationToken::new();
        let shared = Arc::new(Mutex::new(Shared::default()));
        let kind = plan.operation.job_kind();
        let worker_shared = Arc::clone(&shared);

        let join = thread::Builder::new()
            .name("jtf-operation".to_string())
            .spawn(move || {
                let report = execute(&plan, policy, &token, |progress| {
                    // A poisoned lock means a worker panicked; the UI keeps
                    // its last known progress rather than panicking too.
                    if let Ok(mut guard) = worker_shared.lock() {
                        guard.progress = Some(progress.clone());
                    }
                });
                if let Ok(mut guard) = worker_shared.lock() {
                    guard.report = Some(report);
                }
            })
            .ok();

        Self {
            canceller,
            shared,
            join,
            kind,
        }
    }

    /// Which job kind is running, for the label.
    pub(crate) const fn kind(&self) -> jtf_jobs::JobKind {
        self.kind
    }

    /// Fraction complete in `0..=100`, or `None` while the size is unknown.
    pub(crate) fn percent(&self) -> Option<u8> {
        let guard = self.shared.lock().ok()?;
        let progress = guard.progress.as_ref()?;
        if progress.entries_total == 0 {
            return None;
        }
        // Entries rather than bytes: a directory of ten thousand empty files
        // has no bytes to count, and a bar that sits at zero then jumps to a
        // hundred is worse than no bar.
        let done = progress.entries_done.min(progress.entries_total);
        u8::try_from(done * 100 / progress.entries_total).ok()
    }

    /// What is being worked on right now.
    pub(crate) fn current(&self) -> Option<PathBuf> {
        self.shared.lock().ok()?.progress.as_ref()?.current.clone()
    }

    /// Whether the worker has finished.
    pub(crate) fn is_finished(&self) -> bool {
        self.shared.lock().is_ok_and(|guard| guard.report.is_some())
    }

    /// Ask the worker to stop. The operation stops between entries, so a
    /// long copy of one huge file still finishes that file.
    pub(crate) fn cancel(&self) {
        self.canceller.cancel();
    }

    /// Join the worker and take its report.
    pub(crate) fn finish(mut self) -> Option<Report> {
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
        self.shared.lock().ok()?.report.take()
    }
}

impl Drop for Running {
    fn drop(&mut self) {
        // An operation must not outlive the application that started it.
        self.canceller.cancel();
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

/// A summary of what happened, as localization keys and counts.
///
/// Deliberately not a sentence: `AGENTS.md` §11 forbids assembling one from
/// fragments, so the UI picks a message and fills its placeholders.
#[derive(Debug, Clone, Default)]
pub(crate) struct Summary {
    pub key: &'static str,
    pub succeeded: usize,
    pub skipped: usize,
    pub failed: usize,
    /// The first failure's message key, for the detail line.
    pub first_error_key: Option<&'static str>,
    /// The first failing entry, so the user knows which one.
    pub first_error_path: Option<PathBuf>,
}

impl Summary {
    pub(crate) fn from_report(report: &Report) -> Self {
        let failures = report.failures();
        let failed = failures.len();
        let skipped = report.skipped();
        let succeeded = report.succeeded();

        // "Done" is reserved for actually done. Anything else says what
        // really happened (docs/UI_UX_SPEC.md 1).
        let key = if report.cancelled {
            "operation.cancelled"
        } else if failed > 0 {
            "operation.partial"
        } else if skipped > 0 {
            "operation.skipped"
        } else {
            "operation.done"
        };

        Self {
            key,
            succeeded,
            skipped,
            failed,
            first_error_key: failures.first().map(|(_, error)| error.message_key()),
            first_error_path: failures.first().map(|(path, _)| (*path).to_path_buf()),
        }
    }
}

/// Which operation a UI command means.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OperationKind {
    Copy,
    Move,
    Trash,
    Delete,
}

impl OperationKind {
    pub(crate) const fn from_code(code: i32) -> Self {
        match code {
            1 => Self::Move,
            3 => Self::Delete,
            2 => Self::Trash,
            _ => Self::Copy,
        }
    }

    pub(crate) fn build(self, sources: Vec<PathBuf>, destination: Option<PathBuf>) -> Operation {
        match self {
            Self::Copy => Operation::Copy {
                sources,
                destination: destination.unwrap_or_default(),
            },
            Self::Move => Operation::Move {
                sources,
                destination: destination.unwrap_or_default(),
            },
            Self::Trash => Operation::Trash { sources },
            Self::Delete => Operation::Delete { sources },
        }
    }

    /// Whether this needs a destination pane.
    pub(crate) const fn needs_destination(self) -> bool {
        matches!(self, Self::Copy | Self::Move)
    }
}
