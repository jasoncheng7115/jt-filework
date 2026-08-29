//! Workspace state: the split tree, panes, per-pane tabs, and the two
//! independent concepts of selection and marking.
//!
//! Three rules from `AGENTS.md` shape everything here and are enforced by
//! tests rather than by comments:
//!
//! - **§6** the layout is a recursive split tree. There is no `left_pane` and
//!   no `right_pane`, and `tests/architecture.rs` fails if either name appears.
//! - **§7** tabs belong to a pane. Every pane has its own list, its own active
//!   tab, and its own history; a tab carries all of its state when it moves to
//!   another pane.
//! - **§10** selection and marking are different things. Changing one never
//!   changes the other.
//!
//! [`Session`] adds the fourth: reopening the application returns you to where
//! you were — unless you asked it not to, in which case nothing about where
//! you were is written down at all.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

mod ids;
mod pane;
mod selection;
mod session;
mod tab;
mod tree;
mod view;
mod workspace;

pub use ids::{PaneId, SplitId, TabId};
pub use pane::Pane;
pub use selection::{MarkSet, OperationTarget, Selection};
pub use session::{
    RestoreOnLaunch, RestoreOutcome, Restored, Session, SessionSettings, SESSION_FORMAT_VERSION,
};
pub use tab::Tab;
pub use tree::{Orientation, WorkspaceNode, MAX_SPLIT_DEPTH};
pub use view::{
    Column, ColumnSpec, Filter, FilterMode, ScrollPosition, SortKey, SortSpec, ViewMode,
};
pub use workspace::{LayoutPreset, Workspace, WorkspaceError};
