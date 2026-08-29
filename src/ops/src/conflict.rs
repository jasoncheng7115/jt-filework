//! What to do when a destination already exists.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// One destination that is already occupied.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Conflict {
    /// What is being copied or moved.
    pub source: PathBuf,
    /// The path that already exists.
    pub destination: PathBuf,
    /// Whether the existing entry is a directory.
    pub destination_is_directory: bool,
}

/// How the user chose to resolve conflicts.
///
/// Decided **before** the job runs, from a pre-flight scan, so the answer
/// applies to the whole operation. `docs/UI_UX_SPEC.md` §10 also asks for
/// per-item prompting with apply-to-all; that needs the job to pause and
/// resume mid-flight and is tracked in `TODO.md`. Asking once, up front, with
/// the full list of conflicts visible, is honest and is not a worse answer for
/// the common case.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConflictPolicy {
    /// Leave the existing entry alone and skip the source. The default,
    /// because it is the only choice that cannot destroy anything.
    #[default]
    Skip,
    /// Replace the existing entry.
    Overwrite,
    /// Write alongside it under a generated name.
    KeepBoth,
    /// Stop the whole operation.
    Abort,
}

impl ConflictPolicy {
    /// Localization key for the label.
    pub const fn label_key(self) -> &'static str {
        match self {
            Self::Skip => "conflict.skip",
            Self::Overwrite => "conflict.overwrite",
            Self::KeepBoth => "conflict.keep_both",
            Self::Abort => "conflict.abort",
        }
    }

    /// Whether this choice can destroy existing data.
    pub const fn is_destructive(self) -> bool {
        matches!(self, Self::Overwrite)
    }

    /// Every policy, for the UI and for exhaustive tests.
    pub const ALL: &'static [Self] = &[Self::Skip, Self::Overwrite, Self::KeepBoth, Self::Abort];
}

/// Pick a name that does not exist yet, in the style the platform uses.
///
/// `report.txt` becomes `report 2.txt`, then `report 3.txt`. The counter is
/// bounded: a directory that somehow contains every candidate is a failure to
/// report, not a loop to spin in.
pub fn unique_destination(destination: &std::path::Path) -> Option<PathBuf> {
    if !destination.exists() {
        return Some(destination.to_path_buf());
    }
    let parent = destination.parent()?;
    let stem = destination.file_stem()?.to_owned();
    let extension = destination.extension().map(std::ffi::OsStr::to_owned);

    for n in 2..1000 {
        let mut name = stem.clone();
        name.push(format!(" {n}"));
        if let Some(extension) = &extension {
            name.push(".");
            name.push(extension);
        }
        let candidate = parent.join(name);
        if !candidate.exists() {
            return Some(candidate);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skip_is_the_default_because_it_cannot_destroy_anything() {
        assert_eq!(ConflictPolicy::default(), ConflictPolicy::Skip);
        assert!(!ConflictPolicy::Skip.is_destructive());
        assert!(ConflictPolicy::Overwrite.is_destructive());
        assert!(!ConflictPolicy::KeepBoth.is_destructive());
    }

    #[test]
    fn every_policy_has_a_distinct_label_key() {
        let mut keys: Vec<_> = ConflictPolicy::ALL.iter().map(|p| p.label_key()).collect();
        keys.sort_unstable();
        let before = keys.len();
        keys.dedup();
        assert_eq!(keys.len(), before);
    }

    #[test]
    fn a_free_destination_is_returned_unchanged() {
        let path = std::env::temp_dir().join("jtf-does-not-exist-xyz.txt");
        assert_eq!(unique_destination(&path).unwrap(), path);
    }
}
