//! Running a transfer, and making it look like any other operation.
//!
//! The window asks the same questions of a copy from a server as of a copy
//! between two local folders — how many entries, is anything in the way, how
//! far along, what happened — so the answers live behind the same calls and
//! this module is what makes a transfer able to give them.

use std::sync::{Arc, Mutex};
use std::thread;

use jtf_fs::sftp::SftpProvider;
use jtf_jobs::{CancellationToken, Canceller, JobKind};
use jtf_transfer::run::{Policy, Watcher};
use jtf_transfer::{Kind, Plan, Report};

/// What the worker publishes as it goes.
#[derive(Default)]
pub(crate) struct Shared {
    /// Bytes moved, the total as currently known, and what is in hand.
    pub progress: Option<(u64, u64, String)>,
    pub report: Option<Report>,
}

/// A transfer under way.
pub(crate) struct Running {
    canceller: Canceller,
    shared: Arc<Mutex<Shared>>,
    join: Option<thread::JoinHandle<()>>,
    kind: JobKind,
}

/// Passes the worker's progress to the shared slot.
struct Publisher(Arc<Mutex<Shared>>);

impl Watcher for Publisher {
    fn progress(&mut self, done: u64, total: u64, current: &str) {
        // A poisoned lock means a worker panicked; the window keeps its last
        // known progress rather than panicking with it.
        if let Ok(mut guard) = self.0.lock() {
            guard.progress = Some((done, total, current.to_string()));
        }
    }
}

impl Running {
    /// Spawn a worker for `plan`.
    ///
    /// The provider is cloned rather than borrowed: it is an `Arc` over the
    /// connection pool and is built to be reached from a worker, which is how
    /// the transfer gets the connection the window already opened instead of
    /// making a second one and asking for the password again.
    pub(crate) fn start(plan: Plan, sftp: SftpProvider, policy: Policy) -> Self {
        let (token, canceller) = CancellationToken::new();
        let shared = Arc::new(Mutex::new(Shared::default()));
        let kind = match plan.kind {
            Kind::Copy => JobKind::Copy,
            Kind::Move => JobKind::Move,
            Kind::Delete => JobKind::Delete,
        };
        let worker_shared = Arc::clone(&shared);

        let join = thread::Builder::new()
            .name("jtf-transfer".to_string())
            .spawn(move || {
                let mut publisher = Publisher(Arc::clone(&worker_shared));
                let report = jtf_transfer::run(&plan, &sftp, policy, &mut publisher, &token);
                if let Ok(mut guard) = worker_shared.lock() {
                    guard.report = Some(match report {
                        Ok(report) => report,
                        // A failure to reach the server at all is still a
                        // report: the window has to say something happened.
                        Err(error) => Report {
                            outcomes: vec![(
                                String::new(),
                                jtf_transfer::Outcome::Failed(error),
                            )],
                            cancelled: false,
                        },
                    });
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

    /// Which job is running, for the label.
    pub(crate) const fn kind(&self) -> JobKind {
        self.kind
    }

    /// How far along, or `None` while the total is still unknown.
    pub(crate) fn percent(&self) -> Option<u8> {
        let guard = self.shared.lock().ok()?;
        let (done, total, _) = guard.progress.as_ref()?;
        if *total == 0 {
            return None;
        }
        // Capped: the total grows as folders are entered, and a moment where
        // done briefly exceeds the total it was measured against must not
        // show 130%.
        Some(u8::try_from((done.saturating_mul(100) / total).min(100)).unwrap_or(100))
    }

    /// What is in hand right now.
    pub(crate) fn current(&self) -> Option<String> {
        let guard = self.shared.lock().ok()?;
        let (_, _, current) = guard.progress.as_ref()?;
        (!current.is_empty()).then(|| current.clone())
    }

    /// Whether the worker has published its report.
    pub(crate) fn is_finished(&self) -> bool {
        self.shared
            .lock()
            .map_or(true, |guard| guard.report.is_some())
    }

    /// Ask it to stop.
    pub(crate) fn cancel(&self) {
        self.canceller.cancel();
    }

    /// Take the report, waiting for the worker to end.
    pub(crate) fn finish(mut self) -> Option<Report> {
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
        self.shared.lock().ok()?.report.take()
    }
}

impl Drop for Running {
    fn drop(&mut self) {
        // A transfer outliving the window it reports to would go on writing
        // to somebody's disk with nothing watching.
        self.canceller.cancel();
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

/// What a finished transfer amounts to, for the one-line summary.
pub(crate) struct Summary {
    pub key: &'static str,
    pub succeeded: usize,
    pub skipped: usize,
    pub failed: usize,
    pub first_error: Option<String>,
}

impl Summary {
    pub(crate) fn from_report(report: &Report) -> Self {
        let left_both_copies = report
            .outcomes
            .iter()
            .filter(|(_, o)| matches!(o, jtf_transfer::Outcome::CopiedButSourceRemains(_)))
            .count();
        let failed = report.failed();
        let first_error = report.outcomes.iter().find_map(|(name, outcome)| {
            match outcome {
                jtf_transfer::Outcome::Failed(e)
                | jtf_transfer::Outcome::CopiedButSourceRemains(e) => {
                    // The context, not the whole error. `Display` prefixes the
                    // code, and for a failure the server described in its own
                    // words that came out as "Permission denied: Permission
                    // denied" on the status line.
                    Some(format!("{name}: {}", e.context()))
                }
                _ => None,
            }
        });
        // The same keys a local operation reports through, except for the
        // one it can never produce: a move whose bytes arrived and whose
        // source would not go. That is not "partly done" - the file exists
        // twice now, and the person has to be told which two places.
        let key = if report.cancelled {
            "operation.cancelled"
        } else if left_both_copies > 0 {
            "transfer.both_copies"
        } else if failed > 0 {
            "operation.partial"
        } else if report.skipped() > 0 {
            "operation.skipped"
        } else {
            "operation.done"
        };
        Self {
            key,
            succeeded: report.succeeded(),
            skipped: report.skipped(),
            failed,
            first_error,
        }
    }
}

/// Turn the window's conflict code into a policy.
pub(crate) const fn policy_of(code: i32) -> Policy {
    match code {
        1 => Policy::Overwrite,
        2 => Policy::KeepBoth,
        3 => Policy::Abort,
        _ => Policy::Skip,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jtf_core::{Error, ErrorCode};
    use jtf_transfer::Outcome;

    fn report(outcomes: Vec<(String, Outcome)>, cancelled: bool) -> Report {
        Report {
            outcomes,
            cancelled,
        }
    }

    #[test]
    fn a_move_that_left_both_copies_is_counted_and_named() {
        let summary = Summary::from_report(&report(
            vec![
                ("a".into(), Outcome::Done { destination: None }),
                (
                    "b".into(),
                    Outcome::CopiedButSourceRemains(Error::new(ErrorCode::Io, "permission denied")),
                ),
            ],
            false,
        ));
        assert_eq!(summary.succeeded, 1);
        assert_eq!(summary.failed, 1, "it is not a success");
        assert_eq!(summary.key, "transfer.both_copies");
        assert!(
            summary.first_error.is_some_and(|e| e.contains("permission denied")),
            "the reason the source survived was not carried"
        );
    }

    #[test]
    fn a_clean_run_says_so_and_a_cancelled_one_says_that_instead() {
        let clean = Summary::from_report(&report(
            vec![("a".into(), Outcome::Done { destination: None })],
            false,
        ));
        assert_eq!(clean.key, "operation.done");

        let stopped = Summary::from_report(&report(
            vec![("a".into(), Outcome::Done { destination: None })],
            true,
        ));
        assert_eq!(stopped.key, "operation.cancelled");
    }

    #[test]
    fn skipping_is_reported_rather_than_passing_for_success() {
        let summary = Summary::from_report(&report(vec![("a".into(), Outcome::Skipped)], false));
        assert_eq!(summary.key, "operation.skipped");
        assert_eq!(summary.skipped, 1);
        assert_eq!(summary.succeeded, 0);
    }

    #[test]
    fn the_window_codes_map_to_the_same_policies_the_dialog_offers() {
        assert_eq!(policy_of(0), Policy::Skip);
        assert_eq!(policy_of(1), Policy::Overwrite);
        assert_eq!(policy_of(2), Policy::KeepBoth);
        assert_eq!(policy_of(3), Policy::Abort);
        assert_eq!(policy_of(99), Policy::Skip, "an unknown code must be safe");
    }
}
