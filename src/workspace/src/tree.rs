//! The recursive split tree.
//!
//! `AGENTS.md` §6:
//!
//! ```text
//! Workspace
//! └── Split(horizontal|vertical)
//!     ├── Pane
//!     └── Split(...)
//!         ├── Pane
//!         └── Pane
//! ```

use serde::{Deserialize, Serialize};

use crate::ids::{PaneId, SplitId};

/// Which way a split divides its space.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Orientation {
    /// Children sit side by side.
    Horizontal,
    /// Children sit one above the other.
    Vertical,
}

impl Orientation {
    /// The other orientation.
    #[must_use]
    pub const fn flipped(self) -> Self {
        match self {
            Self::Horizontal => Self::Vertical,
            Self::Vertical => Self::Horizontal,
        }
    }

    /// Localization key for the label.
    pub const fn label_key(self) -> &'static str {
        match self {
            Self::Horizontal => "workspace.split.horizontal",
            Self::Vertical => "workspace.split.vertical",
        }
    }
}

/// Maximum nesting depth of the split tree.
///
/// Not a UI limit — a person will never nest sixteen splits. It is a bound on
/// **recursion over attacker-influenced data**: the tree is restored from a
/// session file, and both this module and the UI layer walk it recursively.
/// Without a bound, a hand-edited session file is a stack overflow
/// (`AGENTS.md` §21, `docs/SECURITY.md` §13).
pub const MAX_SPLIT_DEPTH: usize = 16;

/// Smallest fraction a split child may occupy.
///
/// Prevents a pane from being dragged to zero width and becoming unreachable.
pub(crate) const MIN_RATIO: f32 = 0.05;
/// Largest fraction a split child may occupy.
pub(crate) const MAX_RATIO: f32 = 0.95;

/// A node in the layout tree: either a pane, or a split of two subtrees.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "node", rename_all = "snake_case")]
pub enum WorkspaceNode {
    /// A leaf holding one pane.
    ///
    /// A struct variant rather than a newtype so the tree serializes with an
    /// internal tag; session state is written and read as JSON.
    Pane {
        /// Which pane occupies this leaf.
        id: PaneId,
    },
    /// A division of space between two subtrees.
    Split {
        /// Identity, so the split can be resized by reference.
        id: SplitId,
        /// Which way it divides.
        orientation: Orientation,
        /// Fraction of the space given to `first`, clamped to
        /// `[MIN_RATIO, MAX_RATIO]`.
        ratio: f32,
        /// Left or top subtree.
        first: Box<WorkspaceNode>,
        /// Right or bottom subtree.
        second: Box<WorkspaceNode>,
    },
}

impl WorkspaceNode {
    /// A leaf.
    pub const fn pane(id: PaneId) -> Self {
        Self::Pane { id }
    }

    /// A split with a clamped ratio.
    pub fn split(
        id: SplitId,
        orientation: Orientation,
        ratio: f32,
        first: Self,
        second: Self,
    ) -> Self {
        Self::Split {
            id,
            orientation,
            ratio: clamp_ratio(ratio),
            first: Box::new(first),
            second: Box::new(second),
        }
    }

    /// Panes in visual order: first subtree before second, depth first.
    ///
    /// This order defines "next pane" and "previous pane"
    /// (`docs/UI_TEST_PLAN.md` PANE-011).
    pub fn pane_order(&self) -> Vec<PaneId> {
        let mut out = Vec::new();
        self.collect_panes(&mut out);
        out
    }

    fn collect_panes(&self, out: &mut Vec<PaneId>) {
        match self {
            Self::Pane { id } => out.push(*id),
            Self::Split { first, second, .. } => {
                first.collect_panes(out);
                second.collect_panes(out);
            }
        }
    }

    /// How many panes the subtree holds.
    pub fn pane_count(&self) -> usize {
        match self {
            Self::Pane { .. } => 1,
            Self::Split { first, second, .. } => first.pane_count() + second.pane_count(),
        }
    }

    /// Nesting depth: a lone pane is 0.
    pub fn depth(&self) -> usize {
        match self {
            Self::Pane { .. } => 0,
            Self::Split { first, second, .. } => 1 + first.depth().max(second.depth()),
        }
    }

    /// Whether the tree is shallow enough to walk recursively in safety.
    ///
    /// Computed iteratively, so checking a hostile tree cannot itself be the
    /// stack overflow it is meant to prevent.
    pub fn depth_within_limit(&self) -> bool {
        let mut stack = vec![(self, 0usize)];
        while let Some((node, depth)) = stack.pop() {
            if depth > MAX_SPLIT_DEPTH {
                return false;
            }
            if let Self::Split { first, second, .. } = node {
                stack.push((first, depth + 1));
                stack.push((second, depth + 1));
            }
        }
        true
    }

    /// Whether the subtree contains a pane.
    pub fn contains_pane(&self, target: PaneId) -> bool {
        match self {
            Self::Pane { id } => *id == target,
            Self::Split { first, second, .. } => {
                first.contains_pane(target) || second.contains_pane(target)
            }
        }
    }

    /// Replace the leaf holding `target` with `replacement`.
    ///
    /// Returns whether the leaf was found.
    pub fn replace_pane(&mut self, target: PaneId, replacement: Self) -> bool {
        match self {
            Self::Pane { id } if *id == target => {
                *self = replacement;
                true
            }
            Self::Pane { .. } => false,
            Self::Split { first, second, .. } => {
                first.replace_pane(target, replacement.clone())
                    || second.replace_pane(target, replacement)
            }
        }
    }

    /// Remove the leaf holding `target`, collapsing its parent split so the
    /// sibling takes the space.
    ///
    /// Returns `None` if `target` was the only pane in the subtree, because a
    /// tree with no panes is not a representable state.
    pub fn remove_pane(self, target: PaneId) -> Option<Self> {
        match self {
            Self::Pane { id } => {
                if id == target {
                    None
                } else {
                    Some(Self::Pane { id })
                }
            }
            Self::Split {
                id,
                orientation,
                ratio,
                first,
                second,
            } => {
                if first.contains_pane(target) {
                    match first.remove_pane(target) {
                        Some(remaining) => Some(Self::Split {
                            id,
                            orientation,
                            ratio,
                            first: Box::new(remaining),
                            second,
                        }),
                        None => Some(*second),
                    }
                } else if second.contains_pane(target) {
                    match second.remove_pane(target) {
                        Some(remaining) => Some(Self::Split {
                            id,
                            orientation,
                            ratio,
                            first,
                            second: Box::new(remaining),
                        }),
                        None => Some(*first),
                    }
                } else {
                    Some(Self::Split {
                        id,
                        orientation,
                        ratio,
                        first,
                        second,
                    })
                }
            }
        }
    }

    /// Set a split's ratio, clamped. Returns whether the split was found.
    pub fn set_ratio(&mut self, split: SplitId, ratio: f32) -> bool {
        match self {
            Self::Pane { .. } => false,
            Self::Split {
                id,
                ratio: current,
                first,
                second,
                ..
            } => {
                if *id == split {
                    *current = clamp_ratio(ratio);
                    true
                } else {
                    first.set_ratio(split, ratio) || second.set_ratio(split, ratio)
                }
            }
        }
    }

    /// A split's ratio, if the split exists.
    pub fn ratio_of(&self, split: SplitId) -> Option<f32> {
        match self {
            Self::Pane { .. } => None,
            Self::Split {
                id,
                ratio,
                first,
                second,
                ..
            } => {
                if *id == split {
                    Some(*ratio)
                } else {
                    first.ratio_of(split).or_else(|| second.ratio_of(split))
                }
            }
        }
    }

    /// Every split id, in the same depth-first order as [`Self::pane_order`].
    pub fn split_ids(&self) -> Vec<SplitId> {
        let mut out = Vec::new();
        self.collect_splits(&mut out);
        out
    }

    fn collect_splits(&self, out: &mut Vec<SplitId>) {
        if let Self::Split {
            id, first, second, ..
        } = self
        {
            out.push(*id);
            first.collect_splits(out);
            second.collect_splits(out);
        }
    }
}

pub(crate) fn clamp_ratio(ratio: f32) -> f32 {
    if ratio.is_nan() {
        0.5
    } else {
        ratio.clamp(MIN_RATIO, MAX_RATIO)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(n: u64) -> WorkspaceNode {
        WorkspaceNode::pane(PaneId::new(n))
    }

    fn two_pane() -> WorkspaceNode {
        WorkspaceNode::split(SplitId::new(1), Orientation::Horizontal, 0.5, p(1), p(2))
    }

    #[test]
    fn a_split_is_a_tree_node_not_a_pair_of_named_panes() {
        // AGENTS.md 6. The only way to hold two panes is a Split node whose
        // children are themselves nodes, so nesting is free.
        let tree = two_pane();
        assert_eq!(tree.pane_count(), 2);
        assert_eq!(tree.depth(), 1);
        assert_eq!(tree.pane_order(), vec![PaneId::new(1), PaneId::new(2)]);
    }

    #[test]
    fn nesting_to_depth_four_preserves_shape_and_order() {
        let mut tree = p(1);
        for n in 2..=5u64 {
            let split_id = SplitId::new(n);
            let orientation = if n % 2 == 0 {
                Orientation::Horizontal
            } else {
                Orientation::Vertical
            };
            let target = PaneId::new(n - 1);
            let replacement = WorkspaceNode::split(
                split_id,
                orientation,
                0.5,
                WorkspaceNode::pane(target),
                p(n),
            );
            assert!(tree.replace_pane(target, replacement));
        }
        assert_eq!(tree.depth(), 4);
        assert_eq!(tree.pane_count(), 5);
        assert_eq!(
            tree.pane_order(),
            (1..=5).map(PaneId::new).collect::<Vec<_>>(),
            "visual order is first-subtree-then-second, depth first"
        );
    }

    #[test]
    fn removing_a_pane_collapses_its_parent_and_the_sibling_takes_the_space() {
        let tree = two_pane().remove_pane(PaneId::new(1)).unwrap();
        assert_eq!(tree, p(2), "the sibling replaces the split entirely");
        assert_eq!(tree.depth(), 0);
    }

    #[test]
    fn removing_a_pane_from_a_nested_tree_keeps_the_rest_intact() {
        let inner = WorkspaceNode::split(SplitId::new(2), Orientation::Vertical, 0.4, p(2), p(3));
        let tree = WorkspaceNode::split(SplitId::new(1), Orientation::Horizontal, 0.6, p(1), inner);

        let after = tree.remove_pane(PaneId::new(2)).unwrap();
        assert_eq!(after.pane_order(), vec![PaneId::new(1), PaneId::new(3)]);
        assert_eq!(after.depth(), 1);
        assert!((after.ratio_of(SplitId::new(1)).unwrap() - 0.6).abs() < f32::EPSILON);
    }

    #[test]
    fn removing_the_only_pane_is_not_representable() {
        assert!(p(1).remove_pane(PaneId::new(1)).is_none());
    }

    #[test]
    fn removing_an_absent_pane_changes_nothing() {
        let tree = two_pane();
        assert_eq!(tree.clone().remove_pane(PaneId::new(99)).unwrap(), tree);
    }

    #[test]
    fn ratios_stay_in_bounds_after_repeated_resize() {
        let mut tree = two_pane();
        let split = SplitId::new(1);
        for ratio in [-5.0, 0.0, 0.001, 1.0, 99.0, f32::NAN, 0.5] {
            assert!(tree.set_ratio(split, ratio));
            let now = tree.ratio_of(split).unwrap();
            assert!(
                (MIN_RATIO..=MAX_RATIO).contains(&now),
                "ratio {now} escaped bounds after setting {ratio}"
            );
        }
    }

    #[test]
    fn a_nan_ratio_falls_back_to_even_rather_than_poisoning_layout() {
        let mut tree = two_pane();
        tree.set_ratio(SplitId::new(1), f32::NAN);
        assert!((tree.ratio_of(SplitId::new(1)).unwrap() - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn orientation_flips_and_has_a_label_key() {
        assert_eq!(Orientation::Horizontal.flipped(), Orientation::Vertical);
        assert_eq!(
            Orientation::Vertical.label_key(),
            "workspace.split.vertical"
        );
    }

    #[test]
    fn a_tree_deeper_than_the_limit_is_rejected_without_recursing_into_it() {
        // docs/SECURITY.md 13: the split tree arrives from a session file,
        // which is untrusted input, and is walked recursively everywhere.
        let mut tree = p(1);
        for n in 0..(MAX_SPLIT_DEPTH + 5) {
            tree = WorkspaceNode::split(
                SplitId::new(n as u64 + 100),
                Orientation::Horizontal,
                0.5,
                tree,
                p(n as u64 + 200),
            );
        }
        assert!(!tree.depth_within_limit());

        let shallow =
            WorkspaceNode::split(SplitId::new(1), Orientation::Horizontal, 0.5, p(1), p(2));
        assert!(shallow.depth_within_limit());
        assert!(p(1).depth_within_limit());
    }

    #[test]
    fn round_trips_through_serde_identically() {
        let inner = WorkspaceNode::split(SplitId::new(2), Orientation::Vertical, 0.33, p(2), p(3));
        let tree = WorkspaceNode::split(SplitId::new(1), Orientation::Horizontal, 0.7, p(1), inner);
        let json = serde_json::to_string(&tree).unwrap();
        let back: WorkspaceNode = serde_json::from_str(&json).unwrap();
        assert_eq!(tree, back);
    }
}
