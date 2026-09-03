//! Copying, moving and deleting between this machine and a server.
//!
//! `jtf-ops` does the same three things on one filesystem, and does them by
//! renaming: a local move is atomic and instant. None of that survives a
//! network, so this is a separate crate rather than a flag on that one.
//!
//! What is different, and what each difference forces:
//!
//! - **A move cannot be one step.** Across machines it is a copy and then a
//!   delete. Interrupted, it leaves the copy that was made and the source
//!   still there. Within one server it *is* a rename, and that case is
//!   detected and taken, because it is atomic and moves no bytes.
//! - **A server has no trash.** Removing something there is permanent, so it
//!   asks the permanent question however the local side would have behaved.
//! - **Measuring costs round trips.** The local planner walks the tree to
//!   total the bytes. Here the sizes come from the listing already on screen
//!   and folders are counted as they are entered, so nothing stares at a
//!   frozen window first.
//! - **A partial file must not look finished.** A dropped connection is the
//!   normal failure here, not a rare one, so bytes land under a temporary
//!   name and are renamed into place only once they are all there.
//! - **Names a server allows may not fit here.** Reported, never silently
//!   changed.

#![forbid(unsafe_code)]
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

pub mod plan;
pub mod run;

pub use plan::{Item, Kind, Plan, Side};
pub use run::{run, Outcome, Policy, Report, Silent, Watcher};
