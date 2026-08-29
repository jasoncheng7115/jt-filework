//! A tab: one `FileViewSession` (`docs/ARCHITECTURE.md` §5).
//!
//! Everything a tab owns travels with it when it is dragged to another pane
//! (`AGENTS.md` §7, `docs/UI_TEST_PLAN.md` TAB-008).

use jtf_core::Location;
use serde::{Deserialize, Serialize};

use crate::ids::TabId;
use crate::selection::{MarkSet, OperationTarget, Selection};
use crate::view::{
    default_columns, ColumnSpec, Filter, ScrollPosition, SortKey, SortSpec, ViewMode,
};

/// One browsing session inside a pane.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Tab {
    id: TabId,
    location: Location,
    back: Vec<Location>,
    forward: Vec<Location>,
    selection: Selection,
    marks: MarkSet,
    sort: SortSpec,
    filter: Filter,
    columns: Vec<ColumnSpec>,
    view_mode: ViewMode,
    scroll: ScrollPosition,
    pinned: bool,
    active_entry: Option<Location>,
}

impl Tab {
    /// A new tab at `location`, with default view state and empty history.
    pub fn new(id: TabId, location: Location) -> Self {
        Self {
            id,
            location,
            back: Vec::new(),
            forward: Vec::new(),
            selection: Selection::new(),
            marks: MarkSet::new(),
            sort: SortSpec::default(),
            filter: Filter::default(),
            columns: default_columns(),
            view_mode: ViewMode::default(),
            scroll: ScrollPosition::default(),
            pinned: false,
            active_entry: None,
        }
    }

    /// Identifier.
    pub const fn id(&self) -> TabId {
        self.id
    }

    /// Give this tab a new identity, used when duplicating.
    pub(crate) fn set_id(&mut self, id: TabId) {
        self.id = id;
    }

    /// Where the tab is.
    pub const fn location(&self) -> &Location {
        &self.location
    }

    /// The current selection.
    pub const fn selection(&self) -> &Selection {
        &self.selection
    }

    /// Mutable access to the selection. Never touches marks.
    pub fn selection_mut(&mut self) -> &mut Selection {
        &mut self.selection
    }

    /// The persistent marked set.
    pub const fn marks(&self) -> &MarkSet {
        &self.marks
    }

    /// Mutable access to the marked set. Never touches the selection.
    pub fn marks_mut(&mut self) -> &mut MarkSet {
        &mut self.marks
    }

    /// Current sort.
    pub const fn sort(&self) -> SortSpec {
        self.sort
    }

    /// Sort by a column, flipping direction if it is already the sort key.
    pub fn sort_by(&mut self, key: SortKey) {
        self.sort = self.sort.toggled_by(key);
    }

    /// The live filter.
    pub const fn filter(&self) -> &Filter {
        &self.filter
    }

    /// Mutable access to the filter.
    pub fn filter_mut(&mut self) -> &mut Filter {
        &mut self.filter
    }

    /// Column layout.
    pub fn columns(&self) -> &[ColumnSpec] {
        &self.columns
    }

    /// Mutable column layout.
    pub fn columns_mut(&mut self) -> &mut Vec<ColumnSpec> {
        &mut self.columns
    }

    /// List or grid.
    pub const fn view_mode(&self) -> ViewMode {
        self.view_mode
    }

    /// Set list or grid.
    pub fn set_view_mode(&mut self, mode: ViewMode) {
        self.view_mode = mode;
    }

    /// Scroll position.
    pub const fn scroll(&self) -> ScrollPosition {
        self.scroll
    }

    /// Set scroll position.
    pub fn set_scroll(&mut self, scroll: ScrollPosition) {
        self.scroll = scroll;
    }

    /// Whether the tab is pinned.
    pub const fn is_pinned(&self) -> bool {
        self.pinned
    }

    /// Pin or unpin.
    pub fn set_pinned(&mut self, pinned: bool) {
        self.pinned = pinned;
    }

    /// The focused entry, if any.
    pub const fn active_entry(&self) -> Option<&Location> {
        self.active_entry.as_ref()
    }

    /// Set the focused entry.
    pub fn set_active_entry(&mut self, entry: Option<Location>) {
        self.active_entry = entry;
    }

    /// What an operation will act on, and which source it came from.
    pub fn operation_target(&self) -> OperationTarget {
        OperationTarget::resolve(&self.marks, &self.selection, self.active_entry.as_ref())
    }

    /// Navigate to `location`.
    ///
    /// Selection, scroll and the focused entry are per-location and are reset;
    /// **marks are not** — they are the persistent set
    /// (`docs/UI_TEST_PLAN.md` MARK-004).
    pub fn navigate_to(&mut self, location: Location) {
        if location == self.location {
            return;
        }
        self.back
            .push(core::mem::replace(&mut self.location, location));
        self.forward.clear();
        self.reset_per_location_state();
    }

    /// Whether there is history to go back to.
    pub fn can_go_back(&self) -> bool {
        !self.back.is_empty()
    }

    /// Whether there is history to go forward to.
    pub fn can_go_forward(&self) -> bool {
        !self.forward.is_empty()
    }

    /// Step back in this tab's own history.
    ///
    /// Returns whether it moved.
    pub fn go_back(&mut self) -> bool {
        let Some(previous) = self.back.pop() else {
            return false;
        };
        self.forward
            .push(core::mem::replace(&mut self.location, previous));
        self.reset_per_location_state();
        true
    }

    /// Step forward in this tab's own history.
    ///
    /// Returns whether it moved.
    pub fn go_forward(&mut self) -> bool {
        let Some(next) = self.forward.pop() else {
            return false;
        };
        self.back.push(core::mem::replace(&mut self.location, next));
        self.reset_per_location_state();
        true
    }

    /// The back history, oldest first.
    pub fn back_history(&self) -> &[Location] {
        &self.back
    }

    /// The forward history.
    pub fn forward_history(&self) -> &[Location] {
        &self.forward
    }

    /// Replace an unreachable location with `fallback` and prune history.
    ///
    /// Returns the locations that were unavailable, so the caller can tell the
    /// user what could not be restored.
    pub(crate) fn drop_unavailable(
        &mut self,
        fallback: &Location,
        is_available: &impl Fn(&Location) -> bool,
    ) -> Vec<Location> {
        let mut dropped = Vec::new();

        let prune = |history: &mut Vec<Location>, dropped: &mut Vec<Location>| {
            history.retain(|l| {
                if is_available(l) {
                    true
                } else {
                    dropped.push(l.clone());
                    false
                }
            });
        };
        prune(&mut self.back, &mut dropped);
        prune(&mut self.forward, &mut dropped);

        if !is_available(&self.location) {
            dropped.push(self.location.clone());
            self.location = fallback.clone();
            self.reset_per_location_state();
        }
        dropped
    }

    fn reset_per_location_state(&mut self) {
        self.selection.clear();
        self.scroll = ScrollPosition::default();
        self.active_entry = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::view::FilterMode;

    fn tab() -> Tab {
        Tab::new(TabId::new(1), Location::local("/start"))
    }

    fn loc(path: &str) -> Location {
        Location::local(path)
    }

    #[test]
    fn navigation_pushes_history_and_clears_the_forward_stack() {
        let mut t = tab();
        t.navigate_to(loc("/a"));
        t.navigate_to(loc("/b"));
        assert_eq!(t.back_history(), &[loc("/start"), loc("/a")]);
        assert!(t.can_go_back());
        assert!(!t.can_go_forward());

        assert!(t.go_back());
        assert_eq!(t.location(), &loc("/a"));
        assert!(t.can_go_forward());

        t.navigate_to(loc("/c"));
        assert!(
            !t.can_go_forward(),
            "a new navigation discards the forward stack"
        );
    }

    #[test]
    fn navigating_to_the_current_location_is_a_no_op() {
        let mut t = tab();
        t.navigate_to(loc("/start"));
        assert!(!t.can_go_back(), "no spurious history entry");
    }

    #[test]
    fn going_back_and_forward_at_the_ends_reports_no_movement() {
        let mut t = tab();
        assert!(!t.go_back());
        assert!(!t.go_forward());
        assert_eq!(t.location(), &loc("/start"));
    }

    #[test]
    fn navigation_clears_selection_but_never_marks() {
        // AGENTS.md 10 / MARK-004.
        let mut t = tab();
        t.selection_mut().select_only(loc("/start/a"));
        t.marks_mut().mark(loc("/start/a"));
        t.marks_mut().mark(loc("/start/b"));

        t.navigate_to(loc("/elsewhere"));

        assert!(t.selection().is_empty(), "selection is per location");
        assert_eq!(t.marks().len(), 2, "marks are persistent");

        t.go_back();
        assert_eq!(t.marks().len(), 2, "and they survive coming back");
    }

    #[test]
    fn navigation_resets_scroll_and_the_focused_entry() {
        let mut t = tab();
        t.set_scroll(ScrollPosition {
            first_visible_row: 900,
            row_offset: 0.25,
        });
        t.set_active_entry(Some(loc("/start/x")));

        t.navigate_to(loc("/other"));

        assert_eq!(t.scroll(), ScrollPosition::default());
        assert_eq!(t.active_entry(), None);
    }

    #[test]
    fn sort_and_filter_are_per_tab_and_survive_navigation() {
        let mut t = tab();
        t.sort_by(SortKey::Size);
        t.filter_mut().text = "*.log".to_string();
        t.filter_mut().mode = FilterMode::Glob;

        t.navigate_to(loc("/other"));

        assert_eq!(
            t.sort(),
            SortSpec {
                key: SortKey::Size,
                ascending: true
            }
        );
        assert_eq!(t.filter().text, "*.log");
        assert_eq!(t.filter().mode, FilterMode::Glob);
    }

    #[test]
    fn operation_target_reports_which_source_it_used() {
        let mut t = tab();
        t.set_active_entry(Some(loc("/start/active")));
        assert_eq!(t.operation_target().source_key(), "target.active");

        t.selection_mut().select_only(loc("/start/sel"));
        assert_eq!(t.operation_target().source_key(), "target.selection");

        t.marks_mut().mark(loc("/start/marked"));
        let target = t.operation_target();
        assert_eq!(target.source_key(), "target.marked");
        assert_eq!(target.locations(), vec![loc("/start/marked")]);
    }

    #[test]
    fn a_tab_round_trips_through_serde_with_all_of_its_state() {
        let mut t = tab();
        t.navigate_to(loc("/a"));
        t.sort_by(SortKey::Modified);
        t.filter_mut().text = "x".to_string();
        t.marks_mut().mark(loc("/a/1"));
        t.selection_mut().select_only(loc("/a/2"));
        t.set_scroll(ScrollPosition {
            first_visible_row: 12,
            row_offset: 0.5,
        });
        t.set_view_mode(ViewMode::Grid);
        t.set_pinned(true);

        let back: Tab = serde_json::from_str(&serde_json::to_string(&t).unwrap()).unwrap();
        assert_eq!(t, back);
    }
}
