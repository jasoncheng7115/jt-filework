//! Commands and the keymap.
//!
//! `AGENTS.md` §9 fixes the flow and forbids shortcuts:
//!
//! ```text
//! physical input -> keymap -> command -> command bus -> operation
//! ```
//!
//! A keymap maps a chord to a **command id**, never to a function. That is
//! what makes every command reachable from a menu, a palette, a script or a
//! test without a key event, and what stops keyboard handling from quietly
//! becoming the place business logic lives.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

mod bus;
mod ids;
mod keymap;

pub use bus::{CommandBus, DispatchError, Handler};
pub use ids::{Command, CommandCategory, CommandId, CommandRegistry};
pub use keymap::{Key, KeyChord, Keymap, KeymapError, Modifiers};
