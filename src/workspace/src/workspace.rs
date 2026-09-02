//! The workspace: the split tree plus the panes it addresses.
//!
//! `docs/ARCHITECTURE.md` §3.

use std::collections::BTreeMap;

use jtf_core::i18n::LocaleId;
use jtf_core::theme::ThemeMode;
use jtf_core::Location;
use serde::{Deserialize, Serialize};

use crate::ids::{PaneId, SplitId, TabId, WindowId};
use crate::pane::Pane;
use crate::tab::Tab;
use crate::tree::{Orientation, WorkspaceNode};

/// Why a workspace operation was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum WorkspaceError {
    /// No such pane.
    NoSuchPane(PaneId),
    /// No such tab in the given pane.
    NoSuchTab(TabId),
    /// The workspace must always contain at least one pane.
    LastPane,
    /// The source and destination pane are the same.
    SamePane,
}

impl core::fmt::Display for WorkspaceError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::NoSuchPane(id) => write!(f, "no such pane: {id}"),
            Self::NoSuchTab(id) => write!(f, "no such tab: {id}"),
            Self::LastPane => f.write_str("cannot close the last pane"),
            Self::SamePane => f.write_str("source and destination pane are the same"),
        }
    }
}

impl std::error::Error for WorkspaceError {}

/// Ready-made layouts (`docs/PRODUCT_SPEC.md` §4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum LayoutPreset {
    /// One pane.
    Single,
    /// Two side by side.
    TwoColumns,
    /// Two stacked.
    TwoRows,
    /// Four in a grid.
    Quad,
    /// One tall pane beside two stacked panes.
    ThreeLeftMain,
}

/// The whole layout: a split tree, the panes it names, and session-level
/// preferences.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Workspace {
    /// One layout tree per top-level window.
    ///
    /// Panes stay in a single flat map keyed by an id that is unique across
    /// every window, so moving a pane between windows moves a tree entry and
    /// nothing else — the pane, its tabs and its state are untouched. Tearing
    /// a tab into its own window is therefore a change of tree, not a
    /// transfer between two separate models that then have to be kept in
    /// step.
    windows: BTreeMap<WindowId, WorkspaceNode>,
    panes: BTreeMap<PaneId, Pane>,
    active_pane: PaneId,
    /// How far round the pane order the copy/move target sits from the active
    /// pane.
    ///
    /// An offset rather than a pane id, because the target is defined relative
    /// to wherever the keyboard is: store an id and moving the focus can leave
    /// the target pointing at the pane you are standing in, which makes
    /// "copy to the target" a copy onto itself. An offset of at least one
    /// cannot express that.
    target_offset: usize,
    locale: LocaleId,
    theme_mode: ThemeMode,
    next_pane: u64,
    next_tab: u64,
    next_split: u64,
    #[serde(default = "next_window_default")]
    next_window: u64,
}

/// Sessions written before windows existed have one window, numbered 1.
const fn next_window_default() -> u64 {
    2
}

impl Workspace {
    /// A workspace with one pane holding one tab at `location`.
    pub fn new(location: Location) -> Self {
        let pane_id = PaneId::new(1);
        let tab = Tab::new(TabId::new(1), location);
        let mut panes = BTreeMap::new();
        panes.insert(pane_id, Pane::new(pane_id, tab));
        let mut windows = BTreeMap::new();
        windows.insert(Self::MAIN_WINDOW, WorkspaceNode::pane(pane_id));
        Self {
            windows,
            panes,
            active_pane: pane_id,
            target_offset: 1,
            locale: LocaleId::english(),
            theme_mode: ThemeMode::default(),
            next_pane: 2,
            next_tab: 2,
            next_split: 1,
            next_window: 2,
        }
    }

    /// The first window, which always exists.
    pub const MAIN_WINDOW: WindowId = WindowId::new(1);

    /// The layout tree of the main window, which always exists.
    ///
    /// # Panics
    ///
    /// Never in practice: the main window is created with the workspace and
    /// `close_window` refuses to remove it. The expect states that invariant
    /// rather than hiding it behind a default tree that would be wrong.
    #[allow(clippy::expect_used)] // the invariant is the point; see the doc
    pub fn root(&self) -> &WorkspaceNode {
        self.windows
            .get(&Self::MAIN_WINDOW)
            .expect("the main window always exists")
    }

    /// The layout tree of one window.
    pub fn root_of(&self, window: WindowId) -> Option<&WorkspaceNode> {
        self.windows.get(&window)
    }

    /// Every window, in creation order.
    pub fn window_ids(&self) -> Vec<WindowId> {
        self.windows.keys().copied().collect()
    }

    /// How many windows there are.
    pub fn window_count(&self) -> usize {
        self.windows.len()
    }

    /// The window whose tree contains `pane`.
    pub fn window_of(&self, pane: PaneId) -> Option<WindowId> {
        self.windows
            .iter()
            .find(|(_, root)| root.pane_order().contains(&pane))
            .map(|(id, _)| *id)
    }

    /// Panes in visual order, across every window.
    pub fn pane_order(&self) -> Vec<PaneId> {
        self.windows
            .values()
            .flat_map(WorkspaceNode::pane_order)
            .collect()
    }

    /// Panes in visual order within one window.
    pub fn pane_order_in(&self, window: WindowId) -> Vec<PaneId> {
        self.root_of(window)
            .map(WorkspaceNode::pane_order)
            .unwrap_or_default()
    }

    /// How many panes exist.
    pub fn pane_count(&self) -> usize {
        self.panes.len()
    }

    /// The active pane's id.
    pub const fn active_pane_id(&self) -> PaneId {
        self.active_pane
    }

    /// The active pane.
    ///
    /// # Panics
    ///
    /// Only if a type invariant has already been violated: `active_pane`
    /// always names a pane in `panes`, every mutation re-establishes that, and
    /// [`Self::invariants_hold`] asserts it after every operation in the test
    /// suite. A broken invariant here is [`jtf_core::ErrorCode::Internal`]
    /// territory — a bug to fix, not a condition to handle.
    #[allow(clippy::expect_used)]
    pub fn active_pane(&self) -> &Pane {
        self.panes
            .get(&self.active_pane)
            .expect("active pane always exists")
    }

    /// Mutable access to the active pane.
    ///
    /// # Panics
    ///
    /// See [`Self::active_pane`].
    #[allow(clippy::expect_used)]
    pub fn active_pane_mut(&mut self) -> &mut Pane {
        self.panes
            .get_mut(&self.active_pane)
            .expect("active pane always exists")
    }

    /// A pane by id.
    pub fn pane(&self, id: PaneId) -> Option<&Pane> {
        self.panes.get(&id)
    }

    /// Mutable access to a pane by id.
    pub fn pane_mut(&mut self, id: PaneId) -> Option<&mut Pane> {
        self.panes.get_mut(&id)
    }

    /// The active tab of the active pane.
    pub fn active_tab(&self) -> Option<&Tab> {
        self.active_pane().active_tab()
    }

    /// Mutable access to the active tab of the active pane.
    pub fn active_tab_mut(&mut self) -> Option<&mut Tab> {
        self.active_pane_mut().active_tab_mut()
    }

    /// The current locale.
    pub const fn locale(&self) -> &LocaleId {
        &self.locale
    }

    /// Switch locale.
    ///
    /// Deliberately touches nothing else: `AGENTS.md` §11 requires a locale
    /// switch to lose no data.
    pub fn set_locale(&mut self, locale: LocaleId) {
        self.locale = locale;
    }

    /// The theme mode.
    pub const fn theme_mode(&self) -> ThemeMode {
        self.theme_mode
    }

    /// Set the theme mode. Touches nothing else, for the same reason.
    pub fn set_theme_mode(&mut self, mode: ThemeMode) {
        self.theme_mode = mode;
    }

    /// Focus a pane. Returns whether it exists.
    pub fn focus_pane(&mut self, id: PaneId) -> bool {
        if self.panes.contains_key(&id) {
            self.active_pane = id;
            true
        } else {
            false
        }
    }

    /// Focus the next pane in visual order, wrapping.
    pub fn focus_next_pane(&mut self) {
        self.focus_relative(1);
    }

    /// Focus the previous pane in visual order, wrapping.
    pub fn focus_previous_pane(&mut self) {
        self.focus_relative(-1);
    }

    fn focus_relative(&mut self, delta: isize) {
        let order = self.pane_order();
        if order.len() < 2 {
            return;
        }
        let Some(current) = order.iter().position(|id| *id == self.active_pane) else {
            return;
        };
        let len = order.len();
        #[allow(clippy::cast_possible_wrap, clippy::cast_sign_loss)]
        let next = ((current as isize + delta).rem_euclid(len as isize)) as usize;
        self.active_pane = order[next];
    }

    /// The pane a "copy to other pane" command targets.
    ///
    /// The rule is: the next pane in visual order, wrapping, unless
    /// [`Self::cycle_target`] has moved it further round. With one pane there
    /// is no target (`docs/UI_TEST_PLAN.md` PANE-016).
    pub fn target_pane_id(&self) -> Option<PaneId> {
        let order = self.pane_order();
        if order.len() < 2 {
            return None;
        }
        let current = order.iter().position(|id| *id == self.active_pane)?;
        Some(order[(current + self.effective_offset(order.len())) % order.len()])
    }

    /// The offset actually in force, clamped to the panes that exist.
    ///
    /// Panes close. An offset that outlived the pane it pointed at must not
    /// wrap round to zero, which would name the active pane as its own target.
    fn effective_offset(&self, panes: usize) -> usize {
        if panes < 2 {
            return 0;
        }
        let wrapped = self.target_offset % panes;
        if wrapped == 0 {
            1
        } else {
            wrapped
        }
    }

    /// Move the copy/move target to the next pane round, and return it.
    ///
    /// With two panes there is only one other pane and this does nothing
    /// visible, which is correct: the target is already the only candidate.
    /// It exists for three panes and up, where "the next one" is a choice
    /// rather than a fact and there was previously no way to make it from the
    /// keyboard at all.
    pub fn cycle_target(&mut self) -> Option<PaneId> {
        let panes = self.pane_order().len();
        if panes < 2 {
            return None;
        }
        // 1..=panes-1: every pane except the one the keyboard is in.
        self.target_offset = self.effective_offset(panes) % (panes - 1) + 1;
        self.target_pane_id()
    }

    /// Split the active pane, creating a new pane beside it.
    ///
    /// The new pane opens a tab at the active tab's location, which is what a
    /// user splitting a view expects. It becomes active.
    pub fn split_active(&mut self, orientation: Orientation) -> PaneId {
        let location = self
            .active_tab()
            .map_or_else(|| Location::local("/"), |t| t.location().clone());
        self.split_pane(self.active_pane, orientation, location)
    }

    /// Split a specific pane.
    fn split_pane(&mut self, target: PaneId, orientation: Orientation, at: Location) -> PaneId {
        let new_pane_id = self.allocate_pane();
        let tab = Tab::new(self.allocate_tab(), at);
        self.panes.insert(new_pane_id, Pane::new(new_pane_id, tab));

        let split = WorkspaceNode::split(
            self.allocate_split(),
            orientation,
            0.5,
            WorkspaceNode::pane(target),
            WorkspaceNode::pane(new_pane_id),
        );
        // Into the tree that already holds the target, so a split in a
        // torn-off window stays in that window.
        if let Some(window) = self.window_of(target) {
            if let Some(root) = self.windows.get_mut(&window) {
                root.replace_pane(target, split);
            }
        }
        self.active_pane = new_pane_id;
        new_pane_id
    }

    /// Whether [`close_pane`](Self::close_pane) would succeed for `id`.
    ///
    /// The same question the UI has to answer before it offers a close
    /// control: a control that is present and refuses reads as a fault
    /// rather than as a rule. Asked here so there is one answer - deciding
    /// it again in the UI is how the two come to disagree.
    #[must_use]
    pub fn can_close_pane(&self, id: PaneId) -> bool {
        if !self.panes.contains_key(&id) || self.panes.len() == 1 {
            return false;
        }
        let Some(window) = self.window_of(id) else {
            return false;
        };
        let Some(root) = self.windows.get(&window) else {
            return false;
        };
        // Emptying a torn-off window is fine - it goes away. Emptying the
        // main window is not, because there would be nothing left to look at.
        root.clone().remove_pane(id).is_some() || window != Self::MAIN_WINDOW
    }

    /// Close a pane, collapsing its split so the sibling takes the space.
    ///
    /// # Errors
    ///
    /// [`WorkspaceError::LastPane`] when it is the only pane, and
    /// [`WorkspaceError::NoSuchPane`] when it does not exist. The workspace is
    /// never left without a pane.
    pub fn close_pane(&mut self, id: PaneId) -> Result<(), WorkspaceError> {
        if !self.panes.contains_key(&id) {
            return Err(WorkspaceError::NoSuchPane(id));
        }
        if self.panes.len() == 1 {
            return Err(WorkspaceError::LastPane);
        }
        let previous_order = self.pane_order();
        let window = self.window_of(id).ok_or(WorkspaceError::NoSuchPane(id))?;
        let root = self
            .windows
            .remove(&window)
            .ok_or(WorkspaceError::NoSuchPane(id))?;
        // Cloned before the move, so the main window can be restored intact
        // if it turns out this was its last pane.
        let original = root.clone();
        match root.remove_pane(id) {
            Some(remaining) => {
                self.windows.insert(window, remaining);
            }
            None if window == Self::MAIN_WINDOW => {
                // The main window cannot be emptied; put it back untouched.
                self.windows.insert(window, original);
                return Err(WorkspaceError::LastPane);
            }
            None => {
                // A torn-off window whose last pane closed simply goes away.
            }
        }
        self.panes.remove(&id);

        if self.active_pane == id {
            // Focus the pane that was after the closed one, else the one
            // before it: the same rule the tab strip uses.
            let index = previous_order.iter().position(|p| *p == id).unwrap_or(0);
            let order = self.pane_order();
            let next = index.min(order.len() - 1);
            self.active_pane = order[next];
        }
        Ok(())
    }

    /// Move a tab out of its pane into a window of its own.
    ///
    /// The browser gesture: drag a tab off the strip and it becomes its own
    /// window. Returns the new window and the pane now holding the tab.
    ///
    /// # Errors
    ///
    /// [`WorkspaceError::NoSuchPane`] when the pane does not exist, and
    /// [`WorkspaceError::LastPane`] when the tab is the only one in the only
    /// pane of the main window — tearing that off would leave an empty window
    /// behind, which is a way of doing nothing with extra steps.
    pub fn tear_off_tab(
        &mut self,
        from: PaneId,
        tab: TabId,
    ) -> Result<(WindowId, PaneId), WorkspaceError> {
        let source = self
            .panes
            .get(&from)
            .ok_or(WorkspaceError::NoSuchPane(from))?;
        if source.tabs().len() == 1 && self.panes.len() == 1 {
            return Err(WorkspaceError::LastPane);
        }

        let pane = self
            .panes
            .get_mut(&from)
            .ok_or(WorkspaceError::NoSuchPane(from))?;
        let taken = pane.take_tab(tab).ok_or(WorkspaceError::NoSuchPane(from))?;

        let window = WindowId::new(self.next_window);
        self.next_window += 1;
        let pane_id = PaneId::new(self.next_pane);
        self.next_pane += 1;

        self.panes.insert(pane_id, Pane::new(pane_id, taken));
        self.windows.insert(window, WorkspaceNode::pane(pane_id));
        self.active_pane = pane_id;

        // The source pane may now be empty. A pane with no tabs cannot be
        // shown, so it closes, which may in turn close its window.
        if self.panes.get(&from).is_some_and(|p| p.tabs().is_empty()) {
            let _ = self.close_pane(from);
        }
        Ok((window, pane_id))
    }

    /// Move a tab into another pane, which may be in another window.
    ///
    /// The other half of the gesture: dragging a torn-off tab back onto a tab
    /// strip merges it there.
    ///
    /// # Errors
    ///
    /// [`WorkspaceError::NoSuchPane`] when either pane does not exist.
    pub fn merge_tab_into(
        &mut self,
        from: PaneId,
        tab: TabId,
        into: PaneId,
    ) -> Result<(), WorkspaceError> {
        if from == into {
            return Ok(());
        }
        if !self.panes.contains_key(&into) {
            return Err(WorkspaceError::NoSuchPane(into));
        }
        let source = self
            .panes
            .get_mut(&from)
            .ok_or(WorkspaceError::NoSuchPane(from))?;
        let taken = source
            .take_tab(tab)
            .ok_or(WorkspaceError::NoSuchPane(from))?;
        let emptied = source.tabs().is_empty();

        if let Some(target) = self.panes.get_mut(&into) {
            target.push_tab(taken);
        }
        self.active_pane = into;
        if emptied {
            let _ = self.close_pane(from);
        }
        Ok(())
    }

    /// Set a split's ratio. Returns whether the split exists.
    pub fn resize_split(&mut self, split: SplitId, ratio: f32) -> bool {
        self.windows
            .values_mut()
            .any(|root| root.set_ratio(split, ratio))
    }

    /// Every split id, in tree order, across every window.
    pub fn split_ids(&self) -> Vec<SplitId> {
        self.windows
            .values()
            .flat_map(WorkspaceNode::split_ids)
            .collect()
    }

    /// Open a new tab in the active pane.
    pub fn new_tab(&mut self, location: Location) -> TabId {
        let id = self.allocate_tab();
        self.active_pane_mut().push_tab(Tab::new(id, location));
        id
    }

    /// Duplicate a tab in a pane.
    ///
    /// # Errors
    ///
    /// [`WorkspaceError::NoSuchPane`] or [`WorkspaceError::NoSuchTab`].
    pub fn duplicate_tab(&mut self, pane: PaneId, tab: TabId) -> Result<TabId, WorkspaceError> {
        let new_id = self.allocate_tab();
        let pane_ref = self
            .panes
            .get_mut(&pane)
            .ok_or(WorkspaceError::NoSuchPane(pane))?;
        pane_ref
            .duplicate_tab(tab, new_id)
            .ok_or(WorkspaceError::NoSuchTab(tab))
    }

    /// Move a tab to another pane, carrying all of its state.
    ///
    /// If the source pane is left with no tabs it is closed, unless it is the
    /// last pane, in which case it is given a fresh tab.
    ///
    /// # Errors
    ///
    /// [`WorkspaceError::NoSuchPane`], [`WorkspaceError::NoSuchTab`] or
    /// [`WorkspaceError::SamePane`].
    pub fn move_tab_to_pane(
        &mut self,
        from: PaneId,
        tab: TabId,
        to: PaneId,
    ) -> Result<(), WorkspaceError> {
        if from == to {
            return Err(WorkspaceError::SamePane);
        }
        if !self.panes.contains_key(&to) {
            return Err(WorkspaceError::NoSuchPane(to));
        }
        let source = self
            .panes
            .get_mut(&from)
            .ok_or(WorkspaceError::NoSuchPane(from))?;
        let moved = source.take_tab(tab).ok_or(WorkspaceError::NoSuchTab(tab))?;
        let source_empty = source.tab_count() == 0;

        let destination = self
            .panes
            .get_mut(&to)
            .ok_or(WorkspaceError::NoSuchPane(to))?;
        destination.push_tab(moved);

        if source_empty {
            if self.panes.len() > 1 {
                self.close_pane(from)?;
            } else {
                let id = self.allocate_tab();
                let pane = self
                    .panes
                    .get_mut(&from)
                    .ok_or(WorkspaceError::NoSuchPane(from))?;
                pane.push_tab(Tab::new(id, Location::local("/")));
            }
        }
        Ok(())
    }

    /// Replace the layout with a preset, keeping the first pane's tabs.
    pub fn apply_preset(&mut self, preset: LayoutPreset) {
        // Within the window the preset was asked for, and no further. A
        // preset is a statement about the window whose toolbar it came from;
        // reaching into another window to close panes that are not even on
        // the screen the button was pressed on is not what it means, and it
        // destroys work the user can no longer see to object to.
        let Some(window) = self.window_of(self.active_pane) else {
            return;
        };
        while self.pane_order_in(window).len() > 1 {
            let order = self.pane_order_in(window);
            let victim = *order.last().unwrap_or(&self.active_pane);
            if victim == self.active_pane {
                self.active_pane = order[0];
            }
            if self.close_pane(victim).is_err() {
                break;
            }
        }
        let Some(&base) = self.pane_order_in(window).first() else {
            return;
        };
        self.active_pane = base;
        let at = self
            .active_tab()
            .map_or_else(|| Location::local("/"), |t| t.location().clone());

        match preset {
            LayoutPreset::Single => {}
            LayoutPreset::TwoColumns => {
                self.split_pane(base, Orientation::Horizontal, at);
            }
            LayoutPreset::TwoRows => {
                self.split_pane(base, Orientation::Vertical, at);
            }
            LayoutPreset::Quad => {
                let right = self.split_pane(base, Orientation::Horizontal, at.clone());
                self.split_pane(base, Orientation::Vertical, at.clone());
                self.split_pane(right, Orientation::Vertical, at);
            }
            LayoutPreset::ThreeLeftMain => {
                let right = self.split_pane(base, Orientation::Horizontal, at.clone());
                self.split_pane(right, Orientation::Vertical, at);
            }
        }
        self.active_pane = base;
    }

    fn allocate_pane(&mut self) -> PaneId {
        let id = PaneId::new(self.next_pane);
        self.next_pane += 1;
        id
    }

    fn allocate_tab(&mut self) -> TabId {
        let id = TabId::new(self.next_tab);
        self.next_tab += 1;
        id
    }

    fn allocate_split(&mut self) -> SplitId {
        let id = SplitId::new(self.next_split);
        self.next_split += 1;
        id
    }

    /// Total marked entries across every tab of every pane.
    pub fn total_marked(&self) -> usize {
        self.panes
            .values()
            .flat_map(Pane::tabs)
            .map(|t| t.marks().len())
            .sum()
    }

    /// Clear the marked set of every tab.
    ///
    /// Used when the user has asked not to remember marks between runs.
    pub fn clear_all_marks(&mut self) {
        for pane in self.panes.values_mut() {
            for index in 0..pane.tab_count() {
                if let Some(id) = pane.tabs().get(index).map(Tab::id) {
                    if let Some(tab) = pane.tab_mut(id) {
                        tab.marks_mut().clear();
                    }
                }
            }
        }
    }

    /// Forget every pane's reopen-closed-tab history.
    pub fn clear_closed_tab_history(&mut self) {
        for pane in self.panes.values_mut() {
            pane.clear_closed_tabs();
        }
    }

    /// Replace tab locations that are no longer reachable.
    ///
    /// A restored session may name a volume that is not mounted any more.
    /// `docs/UI_TEST_PLAN.md` SESS-004 requires the rest of the session to
    /// come back and the gap to be reported, rather than the whole layout
    /// being thrown away. Returns the locations that were unavailable.
    ///
    /// The availability check is a closure so this stays pure and testable:
    /// the workspace layer never touches the filesystem itself.
    pub fn replace_unavailable_locations(
        &mut self,
        fallback: &Location,
        is_available: impl Fn(&Location) -> bool,
    ) -> Vec<Location> {
        let mut dropped = Vec::new();
        for pane in self.panes.values_mut() {
            let ids: Vec<TabId> = pane.tabs().iter().map(Tab::id).collect();
            for id in ids {
                if let Some(tab) = pane.tab_mut(id) {
                    dropped.extend(tab.drop_unavailable(fallback, &is_available));
                }
            }
        }
        dropped
    }

    /// Whether the internal invariants hold.
    ///
    /// Every pane in the tree exists in the map and vice versa, the active
    /// pane is one of them, and the tree is shallow enough to walk
    /// recursively in safety. Called by tests after every mutation, and by
    /// [`crate::Session::restore`] before a stored workspace is trusted.
    pub fn invariants_hold(&self) -> bool {
        // Checked first, and iteratively: everything below recurses.
        if !self.windows.values().all(WorkspaceNode::depth_within_limit) {
            return false;
        }
        // The main window must exist: everything else assumes it.
        if !self.windows.contains_key(&Self::MAIN_WINDOW) {
            return false;
        }
        let in_tree: Vec<PaneId> = self
            .windows
            .values()
            .flat_map(WorkspaceNode::pane_order)
            .collect();
        if in_tree.len() != self.panes.len() {
            return false;
        }
        // A pane in two windows' trees would pass a count check but is a
        // corrupt layout: the same pane cannot be in two places.
        let unique: std::collections::BTreeSet<PaneId> = in_tree.iter().copied().collect();
        if unique.len() != in_tree.len() {
            return false;
        }
        if !in_tree.iter().all(|id| self.panes.contains_key(id)) {
            return false;
        }
        self.panes.contains_key(&self.active_pane)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_single_preset_leaves_exactly_one_pane_showing_where_it_was() {
        let mut w = workspace();
        w.split_active(Orientation::Horizontal);
        w.split_active(Orientation::Vertical);
        assert_eq!(w.pane_count(), 3);

        w.apply_preset(LayoutPreset::Single);
        assert_eq!(w.pane_count(), 1, "single means one pane");
        assert_eq!(
            w.active_tab().unwrap().location(),
            &loc("/home"),
            "collapsing the layout must not throw away where you were"
        );
    }

    #[test]
    fn a_preset_applies_to_its_own_window_and_leaves_the_others_alone() {
        // The preset is invoked from one window's toolbar. Reaching into
        // another window and closing its panes is not what "make this window
        // a single pane" means, and the panes it would destroy are not on
        // screen where the button was pressed.
        let mut w = workspace();
        let torn = w.pane(w.active_pane_id()).unwrap().tabs()[0].id();
        w.new_tab(loc("/other"));
        let (_window, moved) = w.tear_off_tab(w.active_pane_id(), torn).unwrap();
        w.focus_pane(moved);
        w.split_active(Orientation::Horizontal);
        let torn_panes = w.pane_count() - 1;
        assert!(torn_panes >= 2);

        w.apply_preset(LayoutPreset::Single);
        assert_eq!(
            w.pane_count(),
            2,
            "one pane in the window the preset was applied to, and the other \
             window's pane untouched"
        );
    }

    #[test]
    fn can_close_pane_agrees_with_close_pane_everywhere() {
        // The prediction and the act are two pieces of code answering one
        // question, and the UI believes the prediction. Checked over every
        // pane of every shape this can be in, rather than over one example,
        // because the case that matters is the one nobody thought of.
        let mut w = workspace();
        w.split_active(Orientation::Horizontal);
        w.split_active(Orientation::Vertical);
        let first = w.pane_order()[0];
        let tab = w.pane(first).unwrap().tabs()[0].id();
        w.tear_off_tab(first, tab).ok();

        for id in w.pane_order() {
            let predicted = w.can_close_pane(id);
            let mut trial = w.clone();
            let actual = trial.close_pane(id).is_ok();
            assert_eq!(
                predicted, actual,
                "can_close_pane said {predicted} for {id:?}, close_pane did {actual}"
            );
        }

        // And the case the whole thing exists for: one pane, nothing to
        // close to.
        let lone = workspace();
        assert!(!lone.can_close_pane(lone.active_pane_id()));
    }

    use super::*;

    fn loc(path: &str) -> Location {
        Location::local(path)
    }

    fn workspace() -> Workspace {
        Workspace::new(loc("/home"))
    }

    /// Duplicating copies the tab's *location*, which is the whole point: the
    /// interface used to read a path out of the tab and navigate a new one to
    /// it, and a remote location has no local path to read - so duplicating a
    /// tab on a server produced a tab pointing nowhere.
    #[test]
    fn duplicating_a_tab_keeps_where_it_points_even_on_a_server() {
        let server = Location::remote("host.example", jtf_core::DEFAULT_SSH_PORT, "jason", "/srv");
        let mut w = Workspace::new(server.clone());
        let pane = w.active_pane_id();
        let original = w.pane(pane).unwrap().tabs()[0].id();

        let copy = w.duplicate_tab(pane, original).unwrap();

        assert_ne!(copy, original, "a duplicate is its own tab");
        let tabs = w.pane(pane).unwrap().tabs();
        assert_eq!(tabs.len(), 2);
        assert_eq!(
            tabs[1].location(),
            &server,
            "the copy points at the same server, not at nothing"
        );
        assert!(w.invariants_hold());
    }

    /// The copy goes beside the tab it came from, not at the end of the strip.
    #[test]
    fn a_duplicate_is_placed_next_to_its_original() {
        let mut w = workspace();
        let pane = w.active_pane_id();
        let first = w.pane(pane).unwrap().tabs()[0].id();
        w.new_tab(loc("/last"));

        let copy = w.duplicate_tab(pane, first).unwrap();

        let ids: Vec<_> = w.pane(pane).unwrap().tabs().iter().map(Tab::id).collect();
        assert_eq!(ids[0], first);
        assert_eq!(ids[1], copy, "the copy sits beside what it copied");
    }

    /// Asking for a tab that is not there is an error, not a panic and not a
    /// silent no-op that leaves the caller thinking it worked.
    #[test]
    fn duplicating_a_tab_that_is_not_in_that_pane_is_refused() {
        let mut w = workspace();
        let pane = w.active_pane_id();
        let elsewhere = w.new_tab(loc("/other"));
        w.split_active(Orientation::Horizontal);
        let other_pane = w.active_pane_id();
        assert_ne!(pane, other_pane);

        assert!(w.duplicate_tab(other_pane, elsewhere).is_err());
    }

    #[test]
    fn a_new_workspace_is_one_pane_with_one_tab() {
        let w = workspace();
        assert_eq!(w.pane_count(), 1);
        assert_eq!(w.active_pane().tab_count(), 1);
        assert_eq!(w.active_tab().unwrap().location(), &loc("/home"));
        assert_eq!(w.root().depth(), 0);
        assert!(w.invariants_hold());
    }

    #[test]
    fn splitting_produces_a_tree_and_focuses_the_new_pane() {
        let mut w = workspace();
        let new_pane = w.split_active(Orientation::Horizontal);
        assert_eq!(w.pane_count(), 2);
        assert_eq!(w.active_pane_id(), new_pane);
        assert_eq!(w.root().depth(), 1);
        assert_eq!(w.pane_order(), vec![PaneId::new(1), new_pane]);
        assert!(w.invariants_hold());
    }

    #[test]
    fn nested_splits_reach_arbitrary_depth() {
        let mut w = workspace();
        for i in 0..5 {
            let orientation = if i % 2 == 0 {
                Orientation::Horizontal
            } else {
                Orientation::Vertical
            };
            w.split_active(orientation);
            assert!(w.invariants_hold());
        }
        assert_eq!(w.pane_count(), 6);
        assert_eq!(w.root().depth(), 5);
    }

    #[test]
    fn the_quad_preset_produces_four_panes_at_depth_two() {
        let mut w = workspace();
        w.apply_preset(LayoutPreset::Quad);
        assert_eq!(w.pane_count(), 4);
        assert_eq!(w.root().depth(), 2);
        assert!(w.invariants_hold());
    }

    #[test]
    fn presets_collapse_back_to_a_single_pane() {
        let mut w = workspace();
        w.apply_preset(LayoutPreset::Quad);
        w.apply_preset(LayoutPreset::Single);
        assert_eq!(w.pane_count(), 1);
        assert!(w.invariants_hold());
    }

    #[test]
    fn closing_a_pane_collapses_the_split_and_moves_focus() {
        let mut w = workspace();
        let second = w.split_active(Orientation::Vertical);
        w.close_pane(second).unwrap();
        assert_eq!(w.pane_count(), 1);
        assert_eq!(w.active_pane_id(), PaneId::new(1));
        assert_eq!(w.root().depth(), 0);
        assert!(w.invariants_hold());
    }

    #[test]
    fn the_last_pane_cannot_be_closed() {
        let mut w = workspace();
        assert_eq!(w.close_pane(PaneId::new(1)), Err(WorkspaceError::LastPane));
        assert!(w.invariants_hold());
    }

    #[test]
    fn focus_cycles_through_every_pane_in_visual_order() {
        let mut w = workspace();
        w.split_active(Orientation::Horizontal);
        w.split_active(Orientation::Vertical);
        let order = w.pane_order();
        assert_eq!(order.len(), 3);

        w.focus_pane(order[0]);
        for expected in order.iter().cycle().skip(1).take(order.len()) {
            w.focus_next_pane();
            assert_eq!(w.active_pane_id(), *expected);
        }

        // The loop ends back on the first pane, so stepping back wraps to the
        // last one.
        assert_eq!(w.active_pane_id(), order[0]);
        w.focus_previous_pane();
        assert_eq!(w.active_pane_id(), *order.last().unwrap());
    }

    #[test]
    fn with_three_panes_the_target_can_be_moved_round_and_never_lands_on_the_active_one() {
        // The case this exists for. With two panes "the other pane" is a fact;
        // with three it is a choice, and there was no way to make it.
        let mut w = workspace();
        w.split_active(Orientation::Vertical);
        w.split_active(Orientation::Vertical);
        let order = w.pane_order();
        assert_eq!(order.len(), 3);

        let first = w.target_pane_id().unwrap();
        let second = w.cycle_target().unwrap();
        assert_ne!(first, second, "cycling did not move the target");
        assert_ne!(second, w.active_pane_id(), "the target became the active pane");

        // And it comes back round rather than running out.
        assert_eq!(w.cycle_target(), Some(first));
    }

    #[test]
    fn with_two_panes_cycling_the_target_keeps_naming_the_only_other_pane() {
        let mut w = workspace();
        w.split_active(Orientation::Vertical);
        let target = w.target_pane_id().unwrap();
        assert_eq!(w.cycle_target(), Some(target));
        assert_eq!(w.cycle_target(), Some(target));
    }

    #[test]
    fn cycling_the_target_with_one_pane_does_nothing_and_says_so() {
        let mut w = workspace();
        assert_eq!(w.cycle_target(), None);
        assert_eq!(w.target_pane_id(), None);
    }

    #[test]
    fn a_target_offset_that_outlives_its_pane_never_points_at_the_active_pane() {
        // Panes close. An offset left over from a wider layout must not wrap
        // round to zero, which would make "copy to the target" a copy onto
        // itself - and that would silently do nothing, or worse, conflict with
        // every file in the folder.
        let mut w = workspace();
        w.split_active(Orientation::Vertical);
        w.split_active(Orientation::Vertical);
        w.cycle_target();
        w.cycle_target();
        // Back to two panes, with an offset that was valid for three.
        let doomed = w.pane_order()[2];
        w.close_pane(doomed).unwrap();
        assert_eq!(w.pane_order().len(), 2);
        let target = w.target_pane_id().expect("two panes still have a target");
        assert_ne!(target, w.active_pane_id());
    }

    #[test]
    fn the_target_pane_is_the_next_one_and_absent_when_alone() {
        let mut w = workspace();
        assert_eq!(
            w.target_pane_id(),
            None,
            "one pane has no other pane to target"
        );

        let second = w.split_active(Orientation::Horizontal);
        w.focus_pane(PaneId::new(1));
        assert_eq!(w.target_pane_id(), Some(second));
        w.focus_pane(second);
        assert_eq!(w.target_pane_id(), Some(PaneId::new(1)), "and it wraps");
    }

    #[test]
    fn moving_a_tab_between_panes_carries_every_piece_of_its_state() {
        // AGENTS.md 7 / UI-TAB-008.
        let mut w = workspace();
        let second = w.split_active(Orientation::Horizontal);
        w.focus_pane(PaneId::new(1));

        let tab_id = w.new_tab(loc("/project"));
        {
            let tab = w.active_pane_mut().tab_mut(tab_id).unwrap();
            tab.navigate_to(loc("/project/src"));
            tab.marks_mut().mark(loc("/project/src/a.rs"));
            tab.sort_by(crate::view::SortKey::Modified);
            tab.filter_mut().text = "*.rs".to_string();
            tab.set_scroll(crate::view::ScrollPosition {
                first_visible_row: 42,
                row_offset: 0.5,
            });
        }
        let before = w.pane(PaneId::new(1)).unwrap().tab(tab_id).unwrap().clone();

        w.move_tab_to_pane(PaneId::new(1), tab_id, second).unwrap();

        assert!(w.pane(PaneId::new(1)).unwrap().tab(tab_id).is_none());
        let after = w.pane(second).unwrap().tab(tab_id).unwrap();
        assert_eq!(
            after, &before,
            "location, history, marks, sort, filter and scroll all move"
        );
        assert!(w.invariants_hold());
    }

    #[test]
    fn moving_the_last_tab_out_of_a_pane_closes_that_pane() {
        let mut w = workspace();
        let second = w.split_active(Orientation::Horizontal);
        let lone_tab = w.pane(second).unwrap().tabs()[0].id();

        w.move_tab_to_pane(second, lone_tab, PaneId::new(1))
            .unwrap();

        assert_eq!(w.pane_count(), 1);
        assert_eq!(w.pane(PaneId::new(1)).unwrap().tab_count(), 2);
        assert!(w.invariants_hold());
    }

    #[test]
    fn moving_a_tab_to_the_same_pane_is_refused() {
        let mut w = workspace();
        let tab = w.active_tab().unwrap().id();
        assert_eq!(
            w.move_tab_to_pane(PaneId::new(1), tab, PaneId::new(1)),
            Err(WorkspaceError::SamePane)
        );
    }

    #[test]
    fn each_pane_owns_its_tabs_independently() {
        let mut w = workspace();
        let second = w.split_active(Orientation::Horizontal);
        w.new_tab(loc("/b"));
        w.new_tab(loc("/c"));

        assert_eq!(w.pane(second).unwrap().tab_count(), 3);
        assert_eq!(w.pane(PaneId::new(1)).unwrap().tab_count(), 1);
    }

    #[test]
    fn switching_locale_or_theme_leaves_the_layout_untouched() {
        // AGENTS.md 11 and 12: no data loss on a runtime switch.
        let mut w = workspace();
        w.apply_preset(LayoutPreset::Quad);
        w.active_tab_mut().unwrap().marks_mut().mark(loc("/home/x"));
        let before = w.clone();

        w.set_locale(LocaleId::new(LocaleId::ZH_TW));
        w.set_theme_mode(ThemeMode::Dark);

        assert_eq!(w.root(), before.root());
        assert_eq!(w.pane_count(), before.pane_count());
        assert_eq!(w.active_pane_id(), before.active_pane_id());
        assert_eq!(
            w.active_tab().unwrap().marks(),
            before.active_tab().unwrap().marks()
        );
    }

    #[test]
    fn resize_clamps_and_survives_abuse() {
        let mut w = workspace();
        w.split_active(Orientation::Horizontal);
        let split = w.split_ids()[0];
        for ratio in [-1.0, 0.0, 2.0, f32::NAN, 0.5, f32::INFINITY] {
            assert!(w.resize_split(split, ratio));
        }
        assert!(w.invariants_hold());
    }

    #[test]
    fn a_full_workspace_round_trips_through_serde() {
        let mut w = workspace();
        w.apply_preset(LayoutPreset::Quad);
        w.new_tab(loc("/downloads"));
        w.active_tab_mut()
            .unwrap()
            .marks_mut()
            .mark(loc("/downloads/x.zip"));
        w.set_locale(LocaleId::new(LocaleId::ZH_TW));
        w.set_theme_mode(ThemeMode::Dark);

        let json = serde_json::to_string(&w).unwrap();
        let back: Workspace = serde_json::from_str(&json).unwrap();
        assert_eq!(w, back, "session restore must be exact");
        assert!(back.invariants_hold());
    }

    #[test]
    fn a_session_naming_an_unmounted_volume_restores_the_rest_and_reports_the_gap() {
        // UI-SESS-004. Losing one NAS mount must not cost the user their
        // whole layout.
        let mut w = workspace();
        w.split_active(Orientation::Horizontal);
        let tab = w.new_tab(loc("/Volumes/NAS/projects"));
        w.active_pane_mut()
            .tab_mut(tab)
            .unwrap()
            .navigate_to(loc("/Volumes/NAS/projects/old"));

        let dropped = w.replace_unavailable_locations(&loc("/home"), |l| {
            !l.as_path().is_some_and(|p| p.starts_with("/Volumes/NAS"))
        });

        assert_eq!(dropped.len(), 2, "the location and its history entry");
        assert_eq!(w.pane_count(), 2, "the layout survives");
        assert_eq!(w.active_pane().tab(tab).unwrap().location(), &loc("/home"));
        assert!(w.active_pane().tab(tab).unwrap().back_history().is_empty());
        assert!(w.invariants_hold());
    }

    #[test]
    fn an_available_session_is_left_completely_alone() {
        let mut w = workspace();
        w.apply_preset(LayoutPreset::Quad);
        let before = w.clone();
        let dropped = w.replace_unavailable_locations(&loc("/home"), |_| true);
        assert!(dropped.is_empty());
        assert_eq!(w, before);
    }

    #[test]
    fn a_workspace_with_an_absurdly_deep_tree_fails_its_invariants() {
        let mut w = workspace();
        for _ in 0..(crate::tree::MAX_SPLIT_DEPTH + 2) {
            w.split_active(Orientation::Horizontal);
        }
        assert!(
            !w.invariants_hold(),
            "a tree past the recursion bound is not a valid workspace"
        );
    }

    #[test]
    fn ids_are_never_reused_after_a_close() {
        let mut w = workspace();
        let second = w.split_active(Orientation::Horizontal);
        w.close_pane(second).unwrap();
        let third = w.split_active(Orientation::Horizontal);
        assert_ne!(
            third, second,
            "a stale reference must not resolve to a new pane"
        );
    }
}

#[cfg(test)]
mod window_tests {
    use super::*;
    use jtf_core::Location;

    fn workspace() -> Workspace {
        Workspace::new(Location::local("/tmp"))
    }

    #[test]
    fn tearing_off_a_tab_gives_it_a_window_and_leaves_the_rest_alone() {
        let mut ws = workspace();
        let pane = ws.active_pane_id();
        let moved = ws.new_tab(Location::local("/tmp/second"));

        let (window, new_pane) = ws.tear_off_tab(pane, moved).expect("tears off");
        assert_ne!(window, Workspace::MAIN_WINDOW);
        assert_eq!(ws.window_count(), 2);
        assert_eq!(ws.window_of(new_pane), Some(window));
        assert_eq!(
            ws.pane_order_in(Workspace::MAIN_WINDOW),
            vec![pane],
            "the pane the tab left is still in the main window"
        );
        assert!(ws.invariants_hold());
    }

    #[test]
    fn the_only_tab_of_the_only_pane_cannot_be_torn_off() {
        let mut ws = workspace();
        let pane = ws.active_pane_id();
        let tab = ws.pane(pane).expect("pane").tabs()[0].id();
        assert!(
            matches!(ws.tear_off_tab(pane, tab), Err(WorkspaceError::LastPane)),
            "tearing off the only tab would leave an empty window behind, \
             which is doing nothing with extra steps"
        );
        assert_eq!(ws.window_count(), 1);
        assert!(ws.invariants_hold());
    }

    #[test]
    fn a_torn_off_window_disappears_when_its_tab_merges_back() {
        let mut ws = workspace();
        let pane = ws.active_pane_id();
        let moved = ws.new_tab(Location::local("/tmp/second"));
        let (_window, new_pane) = ws.tear_off_tab(pane, moved).expect("tears off");
        assert_eq!(ws.window_count(), 2);

        ws.merge_tab_into(new_pane, moved, pane)
            .expect("merges back");
        assert_eq!(
            ws.window_count(),
            1,
            "the window emptied by the merge closes; an empty window has \
             nothing to show"
        );
        assert_eq!(ws.pane_count(), 1);
        assert_eq!(ws.pane(pane).expect("pane").tabs().len(), 2);
        assert!(ws.invariants_hold());
    }

    #[test]
    fn a_pane_belongs_to_exactly_one_window() {
        let mut ws = workspace();
        let pane = ws.active_pane_id();
        let moved = ws.new_tab(Location::local("/tmp/second"));
        let (_, new_pane) = ws.tear_off_tab(pane, moved).expect("tears off");

        let windows = ws.window_ids();
        for id in &windows {
            let order = ws.pane_order_in(*id);
            for other in &windows {
                if other == id {
                    continue;
                }
                for p in &order {
                    assert!(
                        !ws.pane_order_in(*other).contains(p),
                        "{p} appears in two windows"
                    );
                }
            }
        }
        assert_ne!(ws.window_of(pane), ws.window_of(new_pane));
        assert!(ws.invariants_hold());
    }

    #[test]
    fn splitting_inside_a_torn_off_window_stays_in_that_window() {
        let mut ws = workspace();
        let pane = ws.active_pane_id();
        let moved = ws.new_tab(Location::local("/tmp/second"));
        let (window, new_pane) = ws.tear_off_tab(pane, moved).expect("tears off");

        // The active pane is the torn-off one, which is what split_active
        // acts on.
        assert_eq!(ws.active_pane_id(), new_pane);
        let split_pane = ws.split_active(Orientation::Horizontal);
        assert_eq!(
            ws.window_of(split_pane),
            Some(window),
            "a split belongs to the window whose pane was split, not to the \
             main window"
        );
        assert_eq!(ws.pane_order_in(Workspace::MAIN_WINDOW), vec![pane]);
        assert!(ws.invariants_hold());
    }
}
