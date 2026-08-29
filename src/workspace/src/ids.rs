//! Identifiers.
//!
//! Distinct newtypes rather than bare integers so a pane id can never be
//! passed where a tab id is expected.

use serde::{Deserialize, Serialize};

macro_rules! define_id {
    ($name:ident, $prefix:literal, $doc:literal) => {
        #[doc = $doc]
        #[derive(
            Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize,
        )]
        pub struct $name(u64);

        impl $name {
            /// Wrap a raw identifier.
            pub const fn new(raw: u64) -> Self {
                Self(raw)
            }

            /// The raw identifier.
            pub const fn get(self) -> u64 {
                self.0
            }
        }

        impl core::fmt::Display for $name {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                write!(f, concat!($prefix, ":{}"), self.0)
            }
        }
    };
}

define_id!(PaneId, "pane", "Identifies a pane within a workspace.");
define_id!(TabId, "tab", "Identifies a tab within a workspace.");
define_id!(SplitId, "split", "Identifies a split node, so it can be resized by reference.");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_display_with_their_kind() {
        assert_eq!(PaneId::new(3).to_string(), "pane:3");
        assert_eq!(TabId::new(7).to_string(), "tab:7");
        assert_eq!(SplitId::new(1).to_string(), "split:1");
    }

    #[test]
    fn ids_of_different_kinds_are_different_types() {
        // This test exists to document intent; the real proof is that the
        // following line does not compile:
        //     let _: PaneId = TabId::new(1);
        assert_eq!(PaneId::new(1).get(), TabId::new(1).get());
    }
}
