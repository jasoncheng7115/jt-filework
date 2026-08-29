//! Progress reporting.
//!
//! `docs/TESTING.md` §4: progress is monotonic and never exceeds total. A
//! progress bar that goes backwards or sits at 100 % while work continues is a
//! lie, and `docs/UI_UX_SPEC.md` §1 forbids lying to the user.

use serde::{Deserialize, Serialize};

/// How far along a job is.
///
/// `total` is optional because some work genuinely has an unknown size until
/// it finishes — a recursive scan does not know how many entries it will find.
/// Reporting a made-up total would be worse than reporting none.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Progress {
    completed: u64,
    total: Option<u64>,
}

impl Progress {
    /// Progress with an unknown total.
    pub const fn indeterminate() -> Self {
        Self {
            completed: 0,
            total: None,
        }
    }

    /// Progress with a known total.
    pub const fn with_total(total: u64) -> Self {
        Self {
            completed: 0,
            total: Some(total),
        }
    }

    /// Units completed so far.
    pub const fn completed(self) -> u64 {
        self.completed
    }

    /// Total units, where known.
    pub const fn total(self) -> Option<u64> {
        self.total
    }

    /// Whether the total is unknown.
    pub const fn is_indeterminate(self) -> bool {
        self.total.is_none()
    }

    /// Advance by `units`.
    ///
    /// Saturates at `total` rather than exceeding it: a provider that
    /// miscounts must not produce 103 %.
    #[must_use]
    pub fn advance(mut self, units: u64) -> Self {
        self.completed = self.completed.saturating_add(units);
        if let Some(total) = self.total {
            self.completed = self.completed.min(total);
        }
        self
    }

    /// Jump to an absolute position.
    ///
    /// Never moves backwards; a later report that is smaller is ignored, so a
    /// racing reporter cannot make the bar retreat.
    #[must_use]
    pub fn set_completed(mut self, completed: u64) -> Self {
        let clamped = match self.total {
            Some(total) => completed.min(total),
            None => completed,
        };
        self.completed = self.completed.max(clamped);
        self
    }

    /// Learn the total part-way through, for work that discovers its size.
    ///
    /// The total is never set below what is already completed.
    #[must_use]
    pub fn set_total(mut self, total: u64) -> Self {
        self.total = Some(total.max(self.completed));
        self
    }

    /// Fraction complete in `0.0..=1.0`, where the total is known.
    pub fn fraction(self) -> Option<f64> {
        match self.total {
            Some(0) => Some(1.0),
            Some(total) =>
            {
                #[allow(clippy::cast_precision_loss)]
                Some(self.completed as f64 / total as f64)
            }
            None => None,
        }
    }

    /// Whether every known unit is done.
    ///
    /// This is *not* the same as the job being complete: only a transition to
    /// [`crate::JobState::Completed`] means that.
    pub const fn is_full(self) -> bool {
        match self.total {
            Some(total) => self.completed >= total,
            None => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn advances_and_reports_a_fraction() {
        let p = Progress::with_total(10).advance(3);
        assert_eq!(p.completed(), 3);
        assert!((p.fraction().unwrap() - 0.3).abs() < f64::EPSILON);
    }

    #[test]
    fn never_exceeds_the_total() {
        let p = Progress::with_total(10).advance(99);
        assert_eq!(p.completed(), 10);
        assert!((p.fraction().unwrap() - 1.0).abs() < f64::EPSILON);
        assert!(p.is_full());
    }

    #[test]
    fn never_moves_backwards() {
        let p = Progress::with_total(100)
            .set_completed(80)
            .set_completed(20);
        assert_eq!(
            p.completed(),
            80,
            "a late smaller report must not retreat the bar"
        );
    }

    #[test]
    fn indeterminate_work_reports_no_fraction_rather_than_a_fake_one() {
        let p = Progress::indeterminate().advance(1000);
        assert!(p.is_indeterminate());
        assert_eq!(p.fraction(), None);
        assert!(!p.is_full(), "unknown total can never be full");
    }

    #[test]
    fn a_total_discovered_later_cannot_be_below_what_is_done() {
        let p = Progress::indeterminate().advance(50).set_total(10);
        assert_eq!(p.total(), Some(50));
        assert!(p.is_full());
    }

    #[test]
    fn an_empty_job_is_complete_not_divided_by_zero() {
        let p = Progress::with_total(0);
        assert_eq!(p.fraction(), Some(1.0));
        assert!(p.is_full());
    }

    #[test]
    fn saturates_instead_of_overflowing() {
        let p = Progress::indeterminate()
            .advance(u64::MAX)
            .advance(u64::MAX);
        assert_eq!(p.completed(), u64::MAX);
    }

    #[test]
    fn full_progress_does_not_mean_completed() {
        // docs/UI_UX_SPEC.md 1: "done" means done. Reaching the last byte is
        // not the same as the job having finished its work.
        let p = Progress::with_total(5).advance(5);
        assert!(p.is_full());
        // Completion is a JobState transition, tested in state.rs.
    }
}
