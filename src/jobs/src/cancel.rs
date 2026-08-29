//! Cancellation.
//!
//! `AGENTS.md` §3: expensive operations must support cancellation. That is
//! only true if the token is cheap to check, safe to share across threads, and
//! impossible to un-cancel.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use jtf_core::{Error, ErrorCode};

/// Returned by [`CancellationToken::check`] when cancellation was requested.
///
/// A distinct type rather than `()` so a work loop's `?` produces something
/// that carries meaning and converts into a core [`Error`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cancelled;

impl core::fmt::Display for Cancelled {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("cancelled")
    }
}

impl std::error::Error for Cancelled {}

impl From<Cancelled> for Error {
    fn from(_: Cancelled) -> Self {
        Self::bare(ErrorCode::Cancelled)
    }
}

/// The read side of a cancellation signal.
///
/// Clone it freely; every clone observes the same signal. Long-running work
/// must check it at every I/O boundary, not only between phases.
#[derive(Debug, Clone, Default)]
pub struct CancellationToken {
    flag: Arc<AtomicBool>,
}

/// The write side.
///
/// Held by whoever may cancel — the job engine, or the UI command that
/// abandons a stale request.
#[derive(Debug, Clone)]
pub struct Canceller {
    flag: Arc<AtomicBool>,
}

impl CancellationToken {
    /// Create a linked token and canceller.
    pub fn new() -> (Self, Canceller) {
        let flag = Arc::new(AtomicBool::new(false));
        (Self { flag: Arc::clone(&flag) }, Canceller { flag })
    }

    /// A token that is never cancelled, for tests and for work that genuinely
    /// cannot be interrupted.
    pub fn never() -> Self {
        Self { flag: Arc::new(AtomicBool::new(false)) }
    }

    /// A token that is already cancelled.
    pub fn cancelled() -> Self {
        Self { flag: Arc::new(AtomicBool::new(true)) }
    }

    /// Whether cancellation has been requested.
    pub fn is_cancelled(&self) -> bool {
        self.flag.load(Ordering::Acquire)
    }

    /// [`Cancelled`] if cancelled, for use with `?` inside a work loop.
    ///
    /// # Errors
    ///
    /// [`Cancelled`] when cancellation has been requested.
    pub fn check(&self) -> Result<(), Cancelled> {
        if self.is_cancelled() {
            Err(Cancelled)
        } else {
            Ok(())
        }
    }
}

impl Canceller {
    /// Request cancellation. Idempotent, and irreversible by design.
    pub fn cancel(&self) {
        self.flag.store(true, Ordering::Release);
    }

    /// Whether cancellation has already been requested.
    pub fn is_cancelled(&self) -> bool {
        self.flag.load(Ordering::Acquire)
    }

    /// Another read handle on the same signal.
    pub fn token(&self) -> CancellationToken {
        CancellationToken { flag: Arc::clone(&self.flag) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn cancelling_is_observed_by_every_clone() {
        let (token, canceller) = CancellationToken::new();
        let clone = token.clone();
        assert!(!token.is_cancelled());

        canceller.cancel();

        assert!(token.is_cancelled());
        assert!(clone.is_cancelled());
        assert!(canceller.token().is_cancelled());
    }

    #[test]
    fn cancellation_is_irreversible_and_idempotent() {
        let (token, canceller) = CancellationToken::new();
        canceller.cancel();
        canceller.cancel();
        assert!(token.is_cancelled(), "there is no way back from cancelled");
    }

    #[test]
    fn check_is_usable_with_the_question_mark_operator() {
        fn work(token: &CancellationToken) -> Result<u32, Cancelled> {
            let mut done = 0;
            for _ in 0..1000 {
                token.check()?;
                done += 1;
            }
            Ok(done)
        }

        assert_eq!(work(&CancellationToken::never()), Ok(1000));
        assert_eq!(work(&CancellationToken::cancelled()), Err(Cancelled));

        // And it converts into a core error with the right code.
        let core_err: jtf_core::Error = Cancelled.into();
        assert_eq!(core_err.code(), jtf_core::ErrorCode::Cancelled);
    }

    #[test]
    fn crosses_thread_boundaries() {
        let (token, canceller) = CancellationToken::new();
        let worker = thread::spawn(move || {
            let mut spins: u64 = 0;
            while !token.is_cancelled() {
                spins = spins.wrapping_add(1);
                std::hint::spin_loop();
            }
            spins
        });
        canceller.cancel();
        let spins = worker.join().unwrap();
        // The point is that it stopped at all, not how far it got.
        let _ = spins;
    }

    #[test]
    fn a_stale_request_can_be_abandoned_independently() {
        // AGENTS.md 3: stale results must be rejected. Two requests, two
        // tokens: cancelling the first must not touch the second.
        let (first, cancel_first) = CancellationToken::new();
        let (second, _cancel_second) = CancellationToken::new();
        cancel_first.cancel();
        assert!(first.is_cancelled());
        assert!(!second.is_cancelled());
    }
}
