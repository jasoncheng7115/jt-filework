//! The provider contract.

use jtf_core::{FileEntry, Location, Result};
use jtf_jobs::{CancellationToken, Canceller};

/// One incremental delivery from an enumeration.
///
/// Rows arrive in batches so the UI can show the first screenful of a
/// 100 000-entry directory immediately (`docs/TESTING.md` §8.2) without
/// paying a channel round trip per file.
#[derive(Debug, Clone, PartialEq)]
pub enum Batch {
    /// More rows.
    Rows(Vec<FileEntry>),
    /// Enumeration finished normally. No further messages.
    Done {
        /// Total rows delivered.
        total: usize,
    },
    /// Enumeration stopped early.
    Failed(jtf_core::Error),
}

/// A running enumeration.
///
/// Dropping the handle cancels the work: an abandoned request must not keep a
/// disk busy, and its results must never reach a pane that has navigated away
/// (`AGENTS.md` §3, stale result rejection).
#[derive(Debug)]
pub struct EnumerationHandle {
    canceller: Canceller,
    receiver: std::sync::mpsc::Receiver<Batch>,
    join: Option<std::thread::JoinHandle<()>>,
}

impl EnumerationHandle {
    pub(crate) fn new(
        canceller: Canceller,
        receiver: std::sync::mpsc::Receiver<Batch>,
        join: std::thread::JoinHandle<()>,
    ) -> Self {
        Self {
            canceller,
            receiver,
            join: Some(join),
        }
    }

    /// Take whatever has arrived so far without blocking.
    ///
    /// This is what a UI event loop calls on a timer or a wake-up: it never
    /// waits, so it can never block the UI thread.
    pub fn poll(&self) -> Vec<Batch> {
        self.receiver.try_iter().collect()
    }

    /// Block until the next batch. For tests and for headless callers.
    ///
    /// Returns `None` once the producer has finished and the channel is
    /// drained.
    pub fn recv(&self) -> Option<Batch> {
        self.receiver.recv().ok()
    }

    /// Ask the worker to stop. Idempotent.
    pub fn cancel(&self) {
        self.canceller.cancel();
    }

    /// Whether cancellation has been requested.
    pub fn is_cancelled(&self) -> bool {
        self.canceller.is_cancelled()
    }
}

impl Drop for EnumerationHandle {
    fn drop(&mut self) {
        self.canceller.cancel();
        if let Some(join) = self.join.take() {
            // The worker checks the token at every entry, so this is a short
            // wait, not a stall. Joining rather than detaching means a
            // cancelled scan cannot still be reading a disk after the pane
            // that asked for it has gone.
            let _ = join.join();
        }
    }
}

/// Something that can list a location.
pub trait Provider: Send + Sync {
    /// Whether this provider handles the location.
    fn handles(&self, location: &Location) -> bool;

    /// List a location synchronously.
    ///
    /// # Errors
    ///
    /// Whatever the underlying storage reports, mapped to an
    /// [`ErrorCode`](jtf_core::ErrorCode).
    ///
    /// Only for small, local, known-good directories and for tests. Anything
    /// user-facing uses [`Provider::enumerate_async`].
    fn list(&self, location: &Location, cancel: &CancellationToken) -> Result<Vec<FileEntry>>;

    /// Start an enumeration that delivers rows incrementally.
    ///
    /// # Errors
    ///
    /// Only for failures that are knowable before any work starts, such as an
    /// unsupported location. Everything else is reported as
    /// [`Batch::Failed`].
    fn enumerate_async(&self, location: &Location) -> Result<EnumerationHandle>;
}
