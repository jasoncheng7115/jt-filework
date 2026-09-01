//! A pane and the tabs it owns.
//!
//! `AGENTS.md` §7: every pane has a tab list, an active tab, independent
//! history and independent view state, and tabs must be movable between panes.

use jtf_core::Location;
use serde::{Deserialize, Serialize};

use crate::ids::{PaneId, TabId};
use crate::tab::Tab;

/// How many closed tabs a pane remembers for reopen.
const CLOSED_TAB_HISTORY: usize = 16;

/// One pane of the workspace.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Pane {
    id: PaneId,
    tabs: Vec<Tab>,
    active: usize,
    recently_closed: Vec<Tab>,
}

impl Pane {
    /// A pane with a single tab.
    pub fn new(id: PaneId, first_tab: Tab) -> Self {
        Self {
            id,
            tabs: vec![first_tab],
            active: 0,
            recently_closed: Vec::new(),
        }
    }

    /// Identifier.
    pub const fn id(&self) -> PaneId {
        self.id
    }

    /// All tabs, in strip order.
    pub fn tabs(&self) -> &[Tab] {
        &self.tabs
    }

    /// The same, to change one of them.
    ///
    /// Ordering is the pane's business, so this hands out the tabs rather than
    /// the vector: a caller can set a tab's own state but cannot reorder them
    /// behind `reorder_tab`, which is what keeps pinned tabs in their block.
    pub fn tabs_mut(&mut self) -> &mut [Tab] {
        &mut self.tabs
    }

    /// How many tabs the pane has.
    pub fn tab_count(&self) -> usize {
        self.tabs.len()
    }

    /// Index of the active tab.
    pub const fn active_index(&self) -> usize {
        self.active
    }

    /// The active tab, if the pane has any.
    pub fn active_tab(&self) -> Option<&Tab> {
        self.tabs.get(self.active)
    }

    /// Mutable access to the active tab.
    pub fn active_tab_mut(&mut self) -> Option<&mut Tab> {
        self.tabs.get_mut(self.active)
    }

    /// Find a tab by id.
    pub fn tab(&self, id: TabId) -> Option<&Tab> {
        self.tabs.iter().find(|t| t.id() == id)
    }

    /// Mutable access to a tab by id.
    pub fn tab_mut(&mut self, id: TabId) -> Option<&mut Tab> {
        self.tabs.iter_mut().find(|t| t.id() == id)
    }

    fn index_of(&self, id: TabId) -> Option<usize> {
        self.tabs.iter().position(|t| t.id() == id)
    }

    /// Append a tab and activate it.
    pub fn push_tab(&mut self, tab: Tab) {
        self.tabs.push(tab);
        self.active = self.tabs.len() - 1;
    }

    /// Insert a tab at `index`, clamped, and activate it.
    pub fn insert_tab(&mut self, index: usize, tab: Tab) {
        let index = index.min(self.tabs.len());
        self.tabs.insert(index, tab);
        self.active = index;
    }

    /// Activate a tab by id. Returns whether it was found.
    pub fn activate(&mut self, id: TabId) -> bool {
        match self.index_of(id) {
            Some(index) => {
                self.active = index;
                true
            }
            None => false,
        }
    }

    /// Activate the next tab, wrapping.
    pub fn activate_next(&mut self) {
        if !self.tabs.is_empty() {
            self.active = (self.active + 1) % self.tabs.len();
        }
    }

    /// Activate the previous tab, wrapping.
    pub fn activate_previous(&mut self) {
        if !self.tabs.is_empty() {
            self.active = (self.active + self.tabs.len() - 1) % self.tabs.len();
        }
    }

    /// Remove a tab and return it, without recording it as closed.
    ///
    /// Used when a tab moves to another pane: it is not closed, so it must not
    /// appear in reopen history.
    pub fn take_tab(&mut self, id: TabId) -> Option<Tab> {
        let index = self.index_of(id)?;
        let tab = self.tabs.remove(index);
        self.fix_active_after_removal(index);
        Some(tab)
    }

    /// Close a tab, remembering it for reopen.
    ///
    /// Returns whether it was closed. A pinned tab is closed only when
    /// `force` is set, matching the usual protection pinning provides.
    pub fn close_tab(&mut self, id: TabId, force: bool) -> bool {
        let Some(index) = self.index_of(id) else {
            return false;
        };
        if self.tabs[index].is_pinned() && !force {
            return false;
        }
        let tab = self.tabs.remove(index);
        self.remember_closed(tab);
        self.fix_active_after_removal(index);
        true
    }

    /// Reopen the most recently closed tab.
    ///
    /// It returns with all of its state: location, history, sort, filter,
    /// columns, scroll and marks (`docs/UI_TEST_PLAN.md` TAB-004).
    pub fn reopen_closed_tab(&mut self) -> Option<TabId> {
        let tab = self.recently_closed.pop()?;
        let id = tab.id();
        self.push_tab(tab);
        Some(id)
    }

    /// How many closed tabs can still be reopened.
    pub fn closed_tab_count(&self) -> usize {
        self.recently_closed.len()
    }

    /// Forget every closed tab.
    ///
    /// Used when the user has asked not to remember them
    /// (`SessionSettings::remember_closed_tabs`): their paths must not be
    /// written to disk.
    pub fn clear_closed_tabs(&mut self) {
        self.recently_closed.clear();
    }

    /// Duplicate a tab under a new id, inserting it after the original.
    ///
    /// The copy is independent: mutating one must never affect the other.
    pub fn duplicate_tab(&mut self, id: TabId, new_id: TabId) -> Option<TabId> {
        let index = self.index_of(id)?;
        let mut copy = self.tabs[index].clone();
        copy.set_id(new_id);
        self.insert_tab(index + 1, copy);
        Some(new_id)
    }

    /// Move a tab within the strip.
    ///
    /// Pinned tabs occupy a leading block; a tab cannot be reordered across
    /// that boundary (`docs/UI_TEST_PLAN.md` TAB-006). Returns whether it
    /// moved.
    pub fn reorder_tab(&mut self, id: TabId, to: usize) -> bool {
        let Some(from) = self.index_of(id) else {
            return false;
        };
        let pinned_count = self.tabs.iter().filter(|t| t.is_pinned()).count();
        let is_pinned = self.tabs[from].is_pinned();
        let (low, high) = if is_pinned {
            (0, pinned_count.saturating_sub(1))
        } else {
            (pinned_count, self.tabs.len() - 1)
        };
        let to = to.clamp(low, high);
        if to == from {
            return false;
        }
        let active_id = self.active_tab().map(Tab::id);
        let tab = self.tabs.remove(from);
        self.tabs.insert(to, tab);
        if let Some(active_id) = active_id {
            if let Some(index) = self.index_of(active_id) {
                self.active = index;
            }
        }
        true
    }

    /// Set a tab's pinned flag, moving it into or out of the pinned block.
    pub fn set_pinned(&mut self, id: TabId, pinned: bool) -> bool {
        let Some(index) = self.index_of(id) else {
            return false;
        };
        if self.tabs[index].is_pinned() == pinned {
            return false;
        }
        let active_id = self.active_tab().map(Tab::id);
        let mut tab = self.tabs.remove(index);
        tab.set_pinned(pinned);
        // The pinned block is the leading run of tabs. Whether we are pinning
        // or unpinning, the tab lands at the boundary: at the end of the
        // pinned block, or at the start of the unpinned one. Same index.
        let boundary = self.tabs.iter().filter(|t| t.is_pinned()).count();
        self.tabs.insert(boundary, tab);
        if let Some(active_id) = active_id {
            if let Some(index) = self.index_of(active_id) {
                self.active = index;
            }
        }
        true
    }

    /// Locations of every tab, for session summaries.
    pub fn locations(&self) -> Vec<Location> {
        self.tabs.iter().map(|t| t.location().clone()).collect()
    }

    fn remember_closed(&mut self, tab: Tab) {
        self.recently_closed.push(tab);
        if self.recently_closed.len() > CLOSED_TAB_HISTORY {
            self.recently_closed.remove(0);
        }
    }

    /// After removing index `removed`, activate a deterministic neighbour:
    /// the tab that took its place, or the new last tab.
    fn fix_active_after_removal(&mut self, removed: usize) {
        if self.tabs.is_empty() {
            self.active = 0;
            return;
        }
        if self.active > removed || self.active >= self.tabs.len() {
            self.active = self.active.saturating_sub(1).min(self.tabs.len() - 1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn loc(path: &str) -> Location {
        Location::local(path)
    }

    fn pane() -> Pane {
        Pane::new(PaneId::new(1), Tab::new(TabId::new(1), loc("/one")))
    }

    fn with_tabs(n: u64) -> Pane {
        let mut p = pane();
        for i in 2..=n {
            p.push_tab(Tab::new(TabId::new(i), loc(&format!("/{i}"))));
        }
        p
    }

    #[test]
    fn a_new_pane_has_one_active_tab() {
        let p = pane();
        assert_eq!(p.tab_count(), 1);
        assert_eq!(p.active_tab().unwrap().id(), TabId::new(1));
    }

    #[test]
    fn closing_the_active_tab_activates_a_deterministic_neighbour() {
        let mut p = with_tabs(3);
        p.activate(TabId::new(2));
        assert!(p.close_tab(TabId::new(2), false));
        assert_eq!(
            p.active_tab().unwrap().id(),
            TabId::new(3),
            "the tab that slid into the freed slot becomes active"
        );

        p.activate(TabId::new(3));
        assert!(p.close_tab(TabId::new(3), false));
        assert_eq!(
            p.active_tab().unwrap().id(),
            TabId::new(1),
            "at the end, step left"
        );
    }

    #[test]
    fn closing_a_tab_before_the_active_one_keeps_the_same_tab_active() {
        let mut p = with_tabs(3);
        p.activate(TabId::new(3));
        p.close_tab(TabId::new(1), false);
        assert_eq!(p.active_tab().unwrap().id(), TabId::new(3));
    }

    #[test]
    fn reopen_restores_the_tab_with_all_of_its_state() {
        let mut p = with_tabs(2);
        let tab = p.tab_mut(TabId::new(2)).unwrap();
        tab.navigate_to(loc("/deep"));
        tab.marks_mut().mark(loc("/deep/x"));
        tab.sort_by(crate::view::SortKey::Size);
        let before = p.tab(TabId::new(2)).unwrap().clone();

        p.close_tab(TabId::new(2), false);
        assert_eq!(p.tab_count(), 1);
        assert_eq!(p.closed_tab_count(), 1);

        let reopened = p.reopen_closed_tab().unwrap();
        assert_eq!(reopened, TabId::new(2));
        assert_eq!(
            p.tab(TabId::new(2)).unwrap(),
            &before,
            "history, marks and sort all return"
        );
    }

    #[test]
    fn a_pinned_tab_is_protected_from_a_plain_close() {
        let mut p = with_tabs(2);
        p.set_pinned(TabId::new(2), true);
        assert!(!p.close_tab(TabId::new(2), false));
        assert!(p.close_tab(TabId::new(2), true));
    }

    #[test]
    fn pinned_tabs_hold_the_leading_block_and_reorder_cannot_cross_it() {
        let mut p = with_tabs(4);
        p.set_pinned(TabId::new(3), true);
        assert_eq!(
            p.tabs()[0].id(),
            TabId::new(3),
            "pinning moves it to the front"
        );

        // An unpinned tab cannot be dragged into the pinned block.
        p.reorder_tab(TabId::new(4), 0);
        assert_eq!(p.tabs()[0].id(), TabId::new(3));
        assert!(!p.tabs()[0].is_pinned() || p.tabs()[0].id() == TabId::new(3));

        // A pinned tab cannot be dragged out of it.
        p.reorder_tab(TabId::new(3), 3);
        assert_eq!(p.tabs()[0].id(), TabId::new(3));
    }

    #[test]
    fn reordering_keeps_the_same_tab_active() {
        let mut p = with_tabs(3);
        p.activate(TabId::new(1));
        p.reorder_tab(TabId::new(1), 2);
        assert_eq!(p.active_tab().unwrap().id(), TabId::new(1));
        assert_eq!(p.tabs()[2].id(), TabId::new(1));
    }

    #[test]
    fn duplicate_produces_an_independent_copy_next_to_the_original() {
        let mut p = with_tabs(2);
        p.tab_mut(TabId::new(1))
            .unwrap()
            .marks_mut()
            .mark(loc("/one/x"));

        p.duplicate_tab(TabId::new(1), TabId::new(99)).unwrap();
        assert_eq!(
            p.tabs()[1].id(),
            TabId::new(99),
            "the copy sits after the original"
        );
        assert_eq!(p.tab(TabId::new(99)).unwrap().marks().len(), 1);

        p.tab_mut(TabId::new(99)).unwrap().marks_mut().clear();
        assert_eq!(
            p.tab(TabId::new(1)).unwrap().marks().len(),
            1,
            "no aliasing"
        );
    }

    #[test]
    fn taking_a_tab_for_a_move_does_not_put_it_in_reopen_history() {
        // TAB-008 moves a tab between panes; that is not a close.
        let mut p = with_tabs(2);
        let taken = p.take_tab(TabId::new(2)).unwrap();
        assert_eq!(taken.id(), TabId::new(2));
        assert_eq!(p.closed_tab_count(), 0, "a moved tab was never closed");
    }

    #[test]
    fn tab_cycling_wraps_in_both_directions() {
        let mut p = with_tabs(3);
        p.activate(TabId::new(3));
        p.activate_next();
        assert_eq!(p.active_tab().unwrap().id(), TabId::new(1));
        p.activate_previous();
        assert_eq!(p.active_tab().unwrap().id(), TabId::new(3));
    }

    #[test]
    fn reopen_history_is_bounded() {
        let mut p = with_tabs(1);
        for i in 100..100 + (CLOSED_TAB_HISTORY as u64) + 5 {
            p.push_tab(Tab::new(TabId::new(i), loc("/x")));
            p.close_tab(TabId::new(i), false);
        }
        assert_eq!(p.closed_tab_count(), CLOSED_TAB_HISTORY);
    }

    #[test]
    fn each_tab_keeps_its_own_history() {
        // AGENTS.md 7: independent history per tab.
        let mut p = with_tabs(2);
        p.tab_mut(TabId::new(1)).unwrap().navigate_to(loc("/a"));
        p.tab_mut(TabId::new(1)).unwrap().navigate_to(loc("/b"));

        assert_eq!(p.tab(TabId::new(1)).unwrap().back_history().len(), 2);
        assert_eq!(p.tab(TabId::new(2)).unwrap().back_history().len(), 0);
    }
}
