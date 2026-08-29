//! The job state machine.
//!
//! `docs/ARCHITECTURE.md` §7:
//!
//! ```text
//! Queued -> Running -> Completed
//!                   -> Failed
//!                   -> Cancelled
//!                   -> WaitingForUser
//! ```
//!
//! Two things this module refuses to allow:
//!
//! - leaving a terminal state, so a completed job can never appear to restart
//! - reaching `Completed` from anywhere but `Running`, so "done" always means
//!   work actually ran

use jtf_core::{Error, ErrorCode};
use serde::{Deserialize, Serialize};

/// Where a job is in its life.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobState {
    /// Accepted, not started.
    Queued,
    /// Doing work.
    Running,
    /// Paused for a decision such as a conflict resolution
    /// (`docs/UI_UX_SPEC.md` §10).
    WaitingForUser,
    /// Finished successfully. Terminal.
    Completed,
    /// Finished unsuccessfully. Terminal.
    Failed,
    /// Stopped on request. Terminal.
    Cancelled,
}

/// A transition that the state machine refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TransitionError {
    /// The state the job was in.
    pub from: JobState,
    /// The state that was requested.
    pub to: JobState,
}

impl core::fmt::Display for TransitionError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "illegal job transition {:?} -> {:?}", self.from, self.to)
    }
}

impl std::error::Error for TransitionError {}

impl From<TransitionError> for Error {
    fn from(value: TransitionError) -> Self {
        Self::new(ErrorCode::Internal, value.to_string())
    }
}

impl JobState {
    /// Whether no further transition is possible.
    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Failed | Self::Cancelled)
    }

    /// Whether the job is doing or about to do work.
    pub const fn is_active(self) -> bool {
        matches!(self, Self::Queued | Self::Running | Self::WaitingForUser)
    }

    /// Whether a cancel request can still be honoured.
    ///
    /// A queued job can be cancelled before it ever starts; the diagram in
    /// `docs/ARCHITECTURE.md` §7 shows the common path, but refusing to cancel
    /// a job that has not started would be absurd.
    pub const fn is_cancellable(self) -> bool {
        self.is_active()
    }

    /// Whether `next` is a legal successor.
    pub const fn can_transition_to(self, next: Self) -> bool {
        match self {
            Self::Queued => matches!(next, Self::Running | Self::Cancelled | Self::Failed),
            Self::Running => matches!(
                next,
                Self::Completed | Self::Failed | Self::Cancelled | Self::WaitingForUser
            ),
            Self::WaitingForUser => {
                matches!(next, Self::Running | Self::Cancelled | Self::Failed)
            }
            Self::Completed | Self::Failed | Self::Cancelled => false,
        }
    }

    /// Apply a transition.
    ///
    /// # Errors
    ///
    /// [`TransitionError`] if the transition is not legal.
    pub const fn transition_to(self, next: Self) -> Result<Self, TransitionError> {
        if self.can_transition_to(next) {
            Ok(next)
        } else {
            Err(TransitionError { from: self, to: next })
        }
    }

    /// Localization key for the state label.
    pub const fn label_key(self) -> &'static str {
        match self {
            Self::Queued => "jobs.state.queued",
            Self::Running => "jobs.state.running",
            Self::WaitingForUser => "jobs.state.waiting_for_user",
            Self::Completed => "jobs.state.completed",
            Self::Failed => "jobs.state.failed",
            Self::Cancelled => "jobs.state.cancelled",
        }
    }

    /// Every state, for exhaustive tests.
    pub const ALL: &'static [Self] = &[
        Self::Queued,
        Self::Running,
        Self::WaitingForUser,
        Self::Completed,
        Self::Failed,
        Self::Cancelled,
    ];
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_documented_happy_path_is_legal() {
        let s = JobState::Queued.transition_to(JobState::Running).unwrap();
        let s = s.transition_to(JobState::Completed).unwrap();
        assert!(s.is_terminal());
    }

    #[test]
    fn running_may_end_four_ways() {
        for end in [
            JobState::Completed,
            JobState::Failed,
            JobState::Cancelled,
            JobState::WaitingForUser,
        ] {
            assert!(JobState::Running.can_transition_to(end), "Running -> {end:?}");
        }
    }

    #[test]
    fn waiting_for_user_resumes_to_running() {
        // The conflict-resolution path in docs/UI_UX_SPEC.md 10.
        let s = JobState::Running.transition_to(JobState::WaitingForUser).unwrap();
        assert_eq!(s.transition_to(JobState::Running).unwrap(), JobState::Running);
    }

    #[test]
    fn terminal_states_are_final() {
        for terminal in [JobState::Completed, JobState::Failed, JobState::Cancelled] {
            assert!(terminal.is_terminal());
            for &next in JobState::ALL {
                assert!(
                    !terminal.can_transition_to(next),
                    "{terminal:?} must not transition to {next:?}"
                );
            }
        }
    }

    #[test]
    fn completed_is_reachable_only_from_running() {
        for &from in JobState::ALL {
            let allowed = from.can_transition_to(JobState::Completed);
            assert_eq!(
                allowed,
                from == JobState::Running,
                "{from:?} -> Completed should be {}",
                from == JobState::Running
            );
        }
    }

    #[test]
    fn cancellation_is_reachable_from_every_active_state() {
        // AGENTS.md 3: expensive operations must support cancellation.
        for &state in JobState::ALL {
            if state.is_active() {
                assert!(state.is_cancellable());
                assert!(
                    state.can_transition_to(JobState::Cancelled),
                    "{state:?} must be cancellable"
                );
            } else {
                assert!(!state.is_cancellable());
            }
        }
    }

    #[test]
    fn illegal_transitions_report_both_ends() {
        let err = JobState::Queued.transition_to(JobState::Completed).unwrap_err();
        assert_eq!(err.from, JobState::Queued);
        assert_eq!(err.to, JobState::Completed);

        let core_err: jtf_core::Error = err.into();
        assert_eq!(core_err.code(), ErrorCode::Internal);
    }

    #[test]
    fn every_state_has_a_distinct_label_key() {
        let mut keys: Vec<_> = JobState::ALL.iter().map(|s| s.label_key()).collect();
        keys.sort_unstable();
        let before = keys.len();
        keys.dedup();
        assert_eq!(keys.len(), before);
    }
}
