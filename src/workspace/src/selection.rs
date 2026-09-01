//! Selection and marking.
//!
//! `AGENTS.md` §10 and `docs/UI_UX_SPEC.md` §6.
//!
//! | | Selection | Mark |
//! |---|---|---|
//! | meaning | current native selection | persistent batch set |
//! | survives navigation | no | yes |
//! | survives sort/filter | yes | yes |
//!
//! These are two types with two separate APIs precisely so that no code path
//! can update one while meaning the other.

use std::collections::BTreeSet;

use jtf_core::Location;
use serde::{Deserialize, Serialize};

/// The current native selection within a tab.
///
/// Cleared on navigation, restored by history. The anchor is what
/// Shift-extension measures from.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Selection {
    entries: BTreeSet<Location>,
    anchor: Option<Location>,
}

impl Selection {
    /// An empty selection.
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether an entry is selected.
    pub fn contains(&self, location: &Location) -> bool {
        self.entries.contains(location)
    }

    /// How many entries are selected.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether nothing is selected.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The selected entries.
    pub fn iter(&self) -> impl Iterator<Item = &Location> {
        self.entries.iter()
    }

    /// The Shift-extension anchor.
    pub const fn anchor(&self) -> Option<&Location> {
        self.anchor.as_ref()
    }

    /// Select exactly one entry, which becomes the anchor.
    pub fn select_only(&mut self, location: Location) {
        self.entries.clear();
        self.anchor = Some(location.clone());
        self.entries.insert(location);
    }

    /// Add an entry without disturbing the rest.
    pub fn add(&mut self, location: Location) {
        self.anchor = Some(location.clone());
        self.entries.insert(location);
    }

    /// Toggle an entry, as Cmd/Ctrl-click does.
    pub fn toggle(&mut self, location: Location) {
        if self.entries.remove(&location) {
            if self.anchor.as_ref() == Some(&location) {
                self.anchor = None;
            }
        } else {
            self.anchor = Some(location.clone());
            self.entries.insert(location);
        }
    }

    /// Replace the selection with a range, as Shift-click does.
    pub fn select_range(&mut self, range: impl IntoIterator<Item = Location>) {
        self.entries = range.into_iter().collect();
    }

    /// Select everything given.
    pub fn select_all(&mut self, all: impl IntoIterator<Item = Location>) {
        self.entries.extend(all);
    }

    /// Clear the selection.
    pub fn clear(&mut self) {
        self.entries.clear();
        self.anchor = None;
    }

    /// Drop entries that are no longer present.
    ///
    /// Called after a refresh; a selection must not keep pointing at rows that
    /// have gone.
    pub fn retain_present(&mut self, present: &BTreeSet<Location>) {
        self.entries.retain(|l| present.contains(l));
        if let Some(anchor) = &self.anchor {
            if !present.contains(anchor) {
                self.anchor = None;
            }
        }
    }
}

/// How many entries may be marked at once.
///
/// The set survives navigation and is written to the session file, so it is
/// the one collection in this program that grows without anything ending it:
/// marking across folders over weeks accumulates, and「mark all」in a folder
/// of a million entries adds a million paths in one keystroke. Both are held
/// in memory *and* serialized at every save, which turns a keystroke into a
/// session file nobody can load.
///
/// A hundred thousand is far past any real selection and still a session file
/// measured in megabytes rather than hundreds of them.
pub const MAX_MARKS: usize = 100_000;

/// The persistent marked set.
///
/// Survives navigation, sorting, filtering, moving the tab to another pane,
/// and session restore. It may span directories
/// (`docs/UI_TEST_PLAN.md` MARK-012).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarkSet {
    entries: BTreeSet<Location>,
    /// How many marks were refused because the set was full, since it was
    /// last emptied. Reported rather than silently dropped: a mark that did
    /// not happen and says nothing is a file that will not be copied and no
    /// reason given.
    #[serde(default, skip_serializing_if = "is_zero")]
    refused: usize,
}

/// So a set that refused nothing - which is every ordinary one - does not
/// carry the field into the session file at all.
#[expect(
    clippy::trivially_copy_pass_by_ref,
    reason = "serde's skip_serializing_if shape"
)]
fn is_zero(value: &usize) -> bool {
    *value == 0
}

impl MarkSet {
    /// An empty marked set.
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether an entry is marked.
    pub fn contains(&self, location: &Location) -> bool {
        self.entries.contains(location)
    }

    /// How many entries are marked, across all directories.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether nothing is marked.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The marked entries.
    pub fn iter(&self) -> impl Iterator<Item = &Location> {
        self.entries.iter()
    }

    /// Mark an entry.
    ///
    /// Does nothing once the set is full, and counts the refusal.
    pub fn mark(&mut self, location: Location) {
        if self.entries.len() >= MAX_MARKS && !self.entries.contains(&location) {
            self.refused = self.refused.saturating_add(1);
            return;
        }
        self.entries.insert(location);
    }

    /// How many marks have been refused for want of room.
    ///
    /// Cleared whenever the set is emptied, so it describes the set as it
    /// stands rather than the history of the session.
    pub const fn refused(&self) -> usize {
        self.refused
    }

    /// Whether the set is full.
    pub fn is_full(&self) -> bool {
        self.entries.len() >= MAX_MARKS
    }

    /// Unmark an entry.
    pub fn unmark(&mut self, location: &Location) {
        self.entries.remove(location);
    }

    /// Toggle an entry's mark.
    pub fn toggle(&mut self, location: Location) {
        if !self.entries.remove(&location) {
            self.mark(location);
        }
    }

    /// Mark everything in `scope`.
    ///
    /// `scope` is explicit — the caller decides whether "all" means the
    /// filtered view or the whole directory
    /// (`docs/UI_TEST_PLAN.md` MARK-003).
    pub fn mark_all(&mut self, scope: impl IntoIterator<Item = Location>) {
        for location in scope {
            self.mark(location);
        }
    }

    /// Unmark everything in `scope`, leaving marks elsewhere untouched.
    pub fn unmark_all(&mut self, scope: impl IntoIterator<Item = Location>) {
        for location in scope {
            self.entries.remove(&location);
        }
        if self.entries.len() < MAX_MARKS {
            // Room again. What was refused was refused against a set that no
            // longer exists, and saying so afterwards would be reporting a
            // limit the user is no longer up against.
            self.refused = 0;
        }
    }

    /// Invert marks within `scope` only.
    pub fn invert(&mut self, scope: impl IntoIterator<Item = Location>) {
        for location in scope {
            self.toggle(location);
        }
    }

    /// Clear every mark everywhere.
    pub fn clear(&mut self) {
        self.entries.clear();
        // The count describes the set as it stands, not the history of the
        // session: an empty set has refused nothing.
        self.refused = 0;
    }

    /// Drop marks for entries that no longer exist in `scope`.
    ///
    /// Only entries *within* the scope are considered, so refreshing one
    /// directory cannot silently drop marks in another
    /// (`docs/UI_TEST_PLAN.md` MARK-008).
    pub fn drop_missing_within(
        &mut self,
        scope: &BTreeSet<Location>,
        present: &BTreeSet<Location>,
    ) {
        self.entries
            .retain(|l| !scope.contains(l) || present.contains(l));
    }
}

/// What an operation will actually act on.
///
/// `docs/UI_UX_SPEC.md` §6: marked set if non-empty, otherwise selection,
/// otherwise the active file — and the UI states which was used.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OperationTarget {
    /// The persistent marked set.
    Marked(Vec<Location>),
    /// The current selection.
    Selection(Vec<Location>),
    /// The focused entry alone.
    Active(Location),
    /// Nothing to act on.
    Empty,
}

impl OperationTarget {
    /// Resolve the documented precedence.
    pub fn resolve(marks: &MarkSet, selection: &Selection, active: Option<&Location>) -> Self {
        if !marks.is_empty() {
            return Self::Marked(marks.iter().cloned().collect());
        }
        if !selection.is_empty() {
            return Self::Selection(selection.iter().cloned().collect());
        }
        match active {
            Some(location) => Self::Active(location.clone()),
            None => Self::Empty,
        }
    }

    /// The entries to act on.
    pub fn locations(&self) -> Vec<Location> {
        match self {
            Self::Marked(v) | Self::Selection(v) => v.clone(),
            Self::Active(l) => vec![l.clone()],
            Self::Empty => Vec::new(),
        }
    }

    /// Localization key naming which source was used, so the UI can say so.
    pub const fn source_key(&self) -> &'static str {
        match self {
            Self::Marked(_) => "target.marked",
            Self::Selection(_) => "target.selection",
            Self::Active(_) => "target.active",
            Self::Empty => "target.empty",
        }
    }

    /// Whether there is anything to act on.
    pub const fn is_empty(&self) -> bool {
        matches!(self, Self::Empty)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn loc(name: &str) -> Location {
        Location::local(format!("/tmp/{name}"))
    }

    fn set(names: &[&str]) -> BTreeSet<Location> {
        names.iter().map(|n| loc(n)).collect()
    }

    #[test]
    fn changing_the_selection_never_touches_the_marks() {
        // AGENTS.md 10, the central invariant of this module.
        let mut selection = Selection::new();
        let mut marks = MarkSet::new();
        marks.mark(loc("a"));
        marks.mark(loc("b"));
        let marks_before = marks.clone();

        selection.select_only(loc("c"));
        selection.toggle(loc("d"));
        selection.select_all(vec![loc("e"), loc("f")]);
        selection.clear();

        assert_eq!(
            marks, marks_before,
            "no selection operation may alter marks"
        );
    }

    #[test]
    fn changing_the_marks_never_touches_the_selection() {
        let mut selection = Selection::new();
        selection.select_only(loc("a"));
        selection.add(loc("b"));
        let selection_before = selection.clone();

        let mut marks = MarkSet::new();
        marks.mark(loc("c"));
        marks.toggle(loc("d"));
        marks.invert(vec![loc("a"), loc("b")]);
        marks.clear();

        assert_eq!(
            selection, selection_before,
            "no mark operation may alter selection"
        );
    }

    #[test]
    fn selection_tracks_an_anchor_for_shift_extension() {
        let mut s = Selection::new();
        s.select_only(loc("a"));
        assert_eq!(s.anchor(), Some(&loc("a")));

        s.add(loc("b"));
        assert_eq!(s.anchor(), Some(&loc("b")));
        assert_eq!(s.len(), 2);

        s.toggle(loc("b"));
        assert_eq!(s.len(), 1);
        assert_eq!(s.anchor(), None, "deselecting the anchor drops it");
    }

    #[test]
    fn select_only_replaces_rather_than_accumulates() {
        let mut s = Selection::new();
        s.select_all(vec![loc("a"), loc("b"), loc("c")]);
        s.select_only(loc("d"));
        assert_eq!(s.len(), 1);
        assert!(s.contains(&loc("d")));
    }

    #[test]
    fn selection_drops_entries_that_disappeared() {
        let mut s = Selection::new();
        s.select_all(vec![loc("a"), loc("b")]);
        s.retain_present(&set(&["a"]));
        assert_eq!(s.len(), 1);
        assert!(s.contains(&loc("a")));
    }

    #[test]
    fn marks_may_span_directories() {
        let mut m = MarkSet::new();
        m.mark(Location::local("/one/a"));
        m.mark(Location::local("/two/b"));
        assert_eq!(m.len(), 2);
    }

    #[test]
    fn refreshing_one_directory_does_not_drop_marks_in_another() {
        // MARK-008: an entry deleted externally is dropped on refresh, but
        // only the refreshed directory is in scope.
        let mut m = MarkSet::new();
        m.mark(Location::local("/one/a"));
        m.mark(Location::local("/one/gone"));
        m.mark(Location::local("/two/b"));

        let scope: BTreeSet<_> = [Location::local("/one/a"), Location::local("/one/gone")]
            .into_iter()
            .collect();
        let present: BTreeSet<_> = [Location::local("/one/a")].into_iter().collect();
        m.drop_missing_within(&scope, &present);

        assert!(m.contains(&Location::local("/one/a")));
        assert!(!m.contains(&Location::local("/one/gone")));
        assert!(
            m.contains(&Location::local("/two/b")),
            "another directory is untouched"
        );
    }

    #[test]
    fn invert_applies_only_to_the_given_scope() {
        let mut m = MarkSet::new();
        m.mark(loc("a"));
        m.mark(Location::local("/elsewhere/z"));

        m.invert(vec![loc("a"), loc("b")]);

        assert!(!m.contains(&loc("a")));
        assert!(m.contains(&loc("b")));
        assert!(
            m.contains(&Location::local("/elsewhere/z")),
            "out of scope, untouched"
        );
    }

    #[test]
    fn operation_target_follows_the_documented_precedence() {
        let mut marks = MarkSet::new();
        let mut selection = Selection::new();
        let active = loc("active");

        assert_eq!(
            OperationTarget::resolve(&marks, &selection, None),
            OperationTarget::Empty
        );

        let t = OperationTarget::resolve(&marks, &selection, Some(&active));
        assert_eq!(t, OperationTarget::Active(active.clone()));
        assert_eq!(t.source_key(), "target.active");

        selection.select_only(loc("sel"));
        let t = OperationTarget::resolve(&marks, &selection, Some(&active));
        assert_eq!(t.locations(), vec![loc("sel")]);
        assert_eq!(t.source_key(), "target.selection");

        marks.mark(loc("marked"));
        let t = OperationTarget::resolve(&marks, &selection, Some(&active));
        assert_eq!(t.locations(), vec![loc("marked")]);
        assert_eq!(t.source_key(), "target.marked", "marks win over selection");
    }

    #[test]
    fn round_trips_through_serde() {
        let mut s = Selection::new();
        s.select_all(vec![loc("a"), loc("b")]);
        let mut m = MarkSet::new();
        m.mark(loc("c"));

        let s_back: Selection = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
        let m_back: MarkSet = serde_json::from_str(&serde_json::to_string(&m).unwrap()).unwrap();
        assert_eq!(s, s_back);
        assert_eq!(m, m_back);
    }
}

#[cfg(test)]
mod mark_bound_tests {
    use super::{MarkSet, MAX_MARKS};
    use jtf_core::Location;

    fn at(n: usize) -> Location {
        Location::local(format!("/files/{n}"))
    }

    /// The set is the one collection here that nothing ends: it survives
    /// navigation and is written to the session file at every save. Without a
    /// bound, one「mark all」in a large folder is a session file that cannot
    /// be loaded next launch.
    #[test]
    fn marking_stops_at_the_bound_rather_than_growing_forever() {
        let mut marks = MarkSet::new();
        marks.mark_all((0..MAX_MARKS + 5_000).map(at));

        assert_eq!(marks.len(), MAX_MARKS, "the set is capped");
        assert_eq!(marks.refused(), 5_000, "and says how much it turned away");
        assert!(marks.is_full());
    }

    /// Refusing must not be silent, and must not be confused with the file
    /// simply not being there.
    #[test]
    fn a_refused_mark_is_counted() {
        let mut marks = MarkSet::new();
        marks.mark_all((0..MAX_MARKS).map(at));
        assert_eq!(marks.refused(), 0);

        marks.mark(at(MAX_MARKS + 1));
        assert_eq!(marks.refused(), 1);
        assert!(!marks.contains(&at(MAX_MARKS + 1)));
    }

    /// Re-marking something already marked is not a refusal - it changes
    /// nothing and the set is not any fuller for it.
    #[test]
    fn marking_something_already_marked_at_the_bound_is_not_refused() {
        let mut marks = MarkSet::new();
        marks.mark_all((0..MAX_MARKS).map(at));
        marks.mark(at(0));
        assert_eq!(marks.refused(), 0);
        assert_eq!(marks.len(), MAX_MARKS);
    }

    /// Making room clears the complaint: a limit the user is no longer up
    /// against should not go on being reported.
    #[test]
    fn making_room_forgets_the_refusals() {
        let mut marks = MarkSet::new();
        marks.mark_all((0..MAX_MARKS + 10).map(at));
        assert!(marks.refused() > 0);

        marks.unmark_all((0..100).map(at));
        assert_eq!(marks.refused(), 0);
        assert!(!marks.is_full());
    }

    /// And clearing the set clears it too.
    #[test]
    fn clearing_forgets_the_refusals() {
        let mut marks = MarkSet::new();
        marks.mark_all((0..MAX_MARKS + 10).map(at));
        marks.clear();
        assert_eq!(marks.refused(), 0);
        assert!(marks.is_empty());
    }

    /// An ordinary selection is nowhere near the bound and must behave exactly
    /// as it always did.
    #[test]
    fn an_ordinary_selection_is_untouched_by_any_of_this() {
        let mut marks = MarkSet::new();
        marks.mark_all((0..2_000).map(at));
        assert_eq!(marks.len(), 2_000);
        assert_eq!(marks.refused(), 0);
        assert!(!marks.is_full());
        marks.toggle(at(0));
        assert!(!marks.contains(&at(0)));
        assert_eq!(marks.len(), 1_999);
    }
}
