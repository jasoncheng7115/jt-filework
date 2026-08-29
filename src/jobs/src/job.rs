//! A unit of tracked work.
//!
//! Everything expensive in JT FileWork is one of these: file operations
//! (`AGENTS.md` §13) and every other blocking activity (`AGENTS.md` §3).

use jtf_core::Error;
use serde::{Deserialize, Serialize};

use crate::cancel::{CancellationToken, Canceller};
use crate::progress::Progress;
use crate::state::{JobState, TransitionError};

/// Identifier for a job within a session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct JobId(u64);

impl JobId {
    /// Wrap a raw identifier.
    pub const fn new(raw: u64) -> Self {
        Self(raw)
    }

    /// The raw identifier.
    pub const fn get(self) -> u64 {
        self.0
    }
}

impl core::fmt::Display for JobId {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "job:{}", self.0)
    }
}

/// What a job is doing.
///
/// The list comes from `docs/ARCHITECTURE.md` §7. It exists so the UI can
/// label a job, decide whether undo is possible, and decide how loudly to
/// report a failure — without the job engine knowing anything about files.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum JobKind {
    /// Enumerating a directory.
    Enumerate,
    /// Copying entries.
    Copy,
    /// Moving entries.
    Move,
    /// Renaming, including batch rename.
    Rename,
    /// Moving to the platform trash.
    Trash,
    /// Permanent deletion.
    Delete,
    /// Computing a recursive size.
    RecursiveSize,
    /// Hashing content.
    Hash,
    /// Creating an archive.
    Compress,
    /// Extracting from an archive.
    Extract,
    /// Listing archive members without extracting.
    ArchiveScan,
    /// Running a search.
    Search,
    /// Building or updating an index.
    Index,
    /// Generating a thumbnail.
    Thumbnail,
    /// Preparing a preview.
    Preview,
    /// Calling an AI provider.
    AiRequest,
    /// Running an external agent such as Claude Code or Codex CLI.
    ExternalAgent,
}

impl JobKind {
    /// Whether this kind can change the filesystem.
    ///
    /// Destructive kinds require an operation-log entry before they act
    /// (`docs/SECURITY.md` §9).
    pub const fn mutates_filesystem(self) -> bool {
        matches!(
            self,
            Self::Copy
                | Self::Move
                | Self::Rename
                | Self::Trash
                | Self::Delete
                | Self::Compress
                | Self::Extract
                | Self::ExternalAgent
        )
    }

    /// Whether a completed job of this kind can be undone safely.
    ///
    /// Conservative on purpose: `docs/UI_UX_SPEC.md` §10 requires the UI to
    /// say *before* the action when undo is impossible, so a wrong `true` here
    /// would be a lie to the user.
    pub const fn is_undoable(self) -> bool {
        matches!(self, Self::Copy | Self::Move | Self::Rename | Self::Trash)
    }

    /// Localization key for the job's label.
    pub const fn label_key(self) -> &'static str {
        match self {
            Self::Enumerate => "jobs.kind.enumerate",
            Self::Copy => "jobs.kind.copy",
            Self::Move => "jobs.kind.move",
            Self::Rename => "jobs.kind.rename",
            Self::Trash => "jobs.kind.trash",
            Self::Delete => "jobs.kind.delete",
            Self::RecursiveSize => "jobs.kind.recursive_size",
            Self::Hash => "jobs.kind.hash",
            Self::Compress => "jobs.kind.compress",
            Self::Extract => "jobs.kind.extract",
            Self::ArchiveScan => "jobs.kind.archive_scan",
            Self::Search => "jobs.kind.search",
            Self::Index => "jobs.kind.index",
            Self::Thumbnail => "jobs.kind.thumbnail",
            Self::Preview => "jobs.kind.preview",
            Self::AiRequest => "jobs.kind.ai_request",
            Self::ExternalAgent => "jobs.kind.external_agent",
        }
    }

    /// Every kind, for exhaustive tests and catalogue parity.
    pub const ALL: &'static [Self] = &[
        Self::Enumerate,
        Self::Copy,
        Self::Move,
        Self::Rename,
        Self::Trash,
        Self::Delete,
        Self::RecursiveSize,
        Self::Hash,
        Self::Compress,
        Self::Extract,
        Self::ArchiveScan,
        Self::Search,
        Self::Index,
        Self::Thumbnail,
        Self::Preview,
        Self::AiRequest,
        Self::ExternalAgent,
    ];
}

/// A tracked unit of work.
#[derive(Debug, Clone)]
pub struct Job {
    id: JobId,
    kind: JobKind,
    state: JobState,
    progress: Progress,
    error: Option<Error>,
    token: CancellationToken,
    canceller: Canceller,
}

impl Job {
    /// Create a queued job.
    pub fn new(id: JobId, kind: JobKind) -> Self {
        let (token, canceller) = CancellationToken::new();
        Self {
            id,
            kind,
            state: JobState::Queued,
            progress: Progress::indeterminate(),
            error: None,
            token,
            canceller,
        }
    }

    /// Identifier.
    pub const fn id(&self) -> JobId {
        self.id
    }

    /// What the job does.
    pub const fn kind(&self) -> JobKind {
        self.kind
    }

    /// Current state.
    pub const fn state(&self) -> JobState {
        self.state
    }

    /// Current progress.
    pub const fn progress(&self) -> Progress {
        self.progress
    }

    /// Failure detail, once the job has failed.
    pub const fn error(&self) -> Option<&Error> {
        self.error.as_ref()
    }

    /// A token the worker checks to notice cancellation.
    pub fn token(&self) -> CancellationToken {
        self.token.clone()
    }

    /// Replace progress.
    ///
    /// [`Progress`] enforces monotonicity itself; this only stores it.
    pub fn set_progress(&mut self, progress: Progress) {
        self.progress = progress;
    }

    /// Move to `next`.
    ///
    /// # Errors
    ///
    /// [`TransitionError`] if the transition is illegal.
    pub fn transition_to(&mut self, next: JobState) -> Result<(), TransitionError> {
        self.state = self.state.transition_to(next)?;
        Ok(())
    }

    /// Start the job.
    ///
    /// # Errors
    ///
    /// [`TransitionError`] if it is not queued or waiting.
    pub fn start(&mut self) -> Result<(), TransitionError> {
        self.transition_to(JobState::Running)
    }

    /// Finish successfully.
    ///
    /// Progress is snapped to its total so a completed job never renders at
    /// 97 % (`docs/TESTING.md` §4).
    ///
    /// # Errors
    ///
    /// [`TransitionError`] if the job was not running.
    pub fn complete(&mut self) -> Result<(), TransitionError> {
        self.transition_to(JobState::Completed)?;
        if let Some(total) = self.progress.total() {
            self.progress = self.progress.set_completed(total);
        }
        Ok(())
    }

    /// Fail with detail.
    ///
    /// # Errors
    ///
    /// [`TransitionError`] if the job was already terminal.
    pub fn fail(&mut self, error: Error) -> Result<(), TransitionError> {
        self.transition_to(JobState::Failed)?;
        self.error = Some(error);
        Ok(())
    }

    /// Pause for a user decision such as a conflict.
    ///
    /// # Errors
    ///
    /// [`TransitionError`] if the job was not running.
    pub fn wait_for_user(&mut self) -> Result<(), TransitionError> {
        self.transition_to(JobState::WaitingForUser)
    }

    /// Request cancellation and record it.
    ///
    /// Signals the worker first, so a worker that is mid-loop stops even if
    /// the state update races with it.
    ///
    /// # Errors
    ///
    /// [`TransitionError`] if the job is already terminal.
    pub fn cancel(&mut self) -> Result<(), TransitionError> {
        self.canceller.cancel();
        self.transition_to(JobState::Cancelled)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jtf_core::ErrorCode;

    fn job(kind: JobKind) -> Job {
        Job::new(JobId::new(1), kind)
    }

    #[test]
    fn a_new_job_is_queued_with_no_progress_and_no_error() {
        let j = job(JobKind::Copy);
        assert_eq!(j.state(), JobState::Queued);
        assert!(j.progress().is_indeterminate());
        assert!(j.error().is_none());
        assert!(!j.token().is_cancelled());
    }

    #[test]
    fn completing_snaps_progress_to_the_total() {
        let mut j = job(JobKind::Copy);
        j.set_progress(Progress::with_total(100).advance(97));
        j.start().unwrap();
        j.complete().unwrap();
        assert_eq!(j.state(), JobState::Completed);
        assert_eq!(j.progress().completed(), 100, "a completed job is not at 97%");
    }

    #[test]
    fn cancelling_signals_the_worker_before_recording_the_state() {
        let mut j = job(JobKind::Search);
        let token = j.token();
        j.start().unwrap();
        j.cancel().unwrap();
        assert!(token.is_cancelled(), "the worker must see the signal");
        assert_eq!(j.state(), JobState::Cancelled);
    }

    #[test]
    fn a_queued_job_can_be_cancelled_before_it_starts() {
        let mut j = job(JobKind::Index);
        j.cancel().unwrap();
        assert_eq!(j.state(), JobState::Cancelled);
        assert!(j.token().is_cancelled());
    }

    #[test]
    fn failing_records_a_machine_readable_code() {
        let mut j = job(JobKind::Extract);
        j.start().unwrap();
        j.fail(Error::new(ErrorCode::LimitExceeded, "ratio bomb")).unwrap();
        assert_eq!(j.state(), JobState::Failed);
        assert_eq!(j.error().unwrap().code(), ErrorCode::LimitExceeded);
    }

    #[test]
    fn a_terminal_job_cannot_be_restarted() {
        let mut j = job(JobKind::Copy);
        j.start().unwrap();
        j.complete().unwrap();
        assert!(j.start().is_err());
        assert!(j.cancel().is_err());
        assert!(j.fail(Error::bare(ErrorCode::Io)).is_err());
    }

    #[test]
    fn conflict_round_trip_pauses_and_resumes() {
        let mut j = job(JobKind::Move);
        j.start().unwrap();
        j.wait_for_user().unwrap();
        assert_eq!(j.state(), JobState::WaitingForUser);
        j.start().unwrap();
        assert_eq!(j.state(), JobState::Running);
        j.complete().unwrap();
    }

    #[test]
    fn destructive_kinds_are_flagged_and_undo_claims_are_conservative() {
        // docs/SECURITY.md 9 and docs/UI_UX_SPEC.md 10.
        assert!(JobKind::Delete.mutates_filesystem());
        assert!(!JobKind::Delete.is_undoable(), "permanent delete must never claim undo");
        assert!(JobKind::Trash.is_undoable());
        assert!(!JobKind::Search.mutates_filesystem());
        assert!(JobKind::ExternalAgent.mutates_filesystem(), "an agent writes files");
        assert!(!JobKind::ExternalAgent.is_undoable());
    }

    #[test]
    fn every_kind_has_a_distinct_label_key() {
        let mut keys: Vec<_> = JobKind::ALL.iter().map(|k| k.label_key()).collect();
        keys.sort_unstable();
        let before = keys.len();
        keys.dedup();
        assert_eq!(keys.len(), before);
    }

    #[test]
    fn undoable_implies_it_actually_changes_something() {
        for &kind in JobKind::ALL {
            if kind.is_undoable() {
                assert!(kind.mutates_filesystem(), "{kind:?} claims undo but changes nothing");
            }
        }
    }
}
