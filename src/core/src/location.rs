//! Where an entry lives.
//!
//! `docs/ARCHITECTURE.md` §8: "Do not model an entry as a path string only."
//! A location may be a real path, a member inside an archive, or a row in a
//! virtual result set. Modelling this up front keeps search results and
//! archive browsing from being bolted on later as special cases.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// The identity of something a pane can show or an operation can target.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[non_exhaustive]
pub enum Location {
    /// A path on a mounted filesystem, local or remote.
    Local {
        /// The path exactly as the platform reports it.
        path: PathBuf,
    },
    /// A member inside an archive. Listing one must not extract it
    /// (`docs/SECURITY.md` §4).
    ArchiveMember {
        /// Location of the archive itself. Archives may nest.
        archive: Box<Location>,
        /// The member path as recorded inside the archive. Untrusted.
        member: String,
    },
    /// A virtual container such as a search result set
    /// (`docs/SEARCH_AI.md` §1).
    Virtual {
        /// Namespace, e.g. `search`.
        scheme: String,
        /// Opaque identifier within the namespace.
        id: String,
    },
}

impl Location {
    /// A location on the local (or mounted) filesystem.
    pub fn local(path: impl Into<PathBuf>) -> Self {
        Self::Local { path: path.into() }
    }

    /// A member inside an archive.
    pub fn archive_member(archive: Location, member: impl Into<String>) -> Self {
        Self::ArchiveMember {
            archive: Box::new(archive),
            member: member.into(),
        }
    }

    /// A virtual location such as a search result set.
    pub fn virtual_location(scheme: impl Into<String>, id: impl Into<String>) -> Self {
        Self::Virtual {
            scheme: scheme.into(),
            id: id.into(),
        }
    }

    /// The filesystem path, if this location has one.
    ///
    /// Archive members and virtual locations return `None`; callers must not
    /// invent a path for them.
    pub fn as_path(&self) -> Option<&Path> {
        match self {
            Self::Local { path } => Some(path),
            _ => None,
        }
    }

    /// Whether operations on this location may write to the filesystem.
    pub const fn is_writable_target(&self) -> bool {
        matches!(self, Self::Local { .. })
    }

    /// The parent location, where one exists.
    pub fn parent(&self) -> Option<Self> {
        match self {
            Self::Local { path } => path.parent().map(Self::local),
            Self::ArchiveMember { archive, member } => match parent_member(member) {
                Some(p) => Some(Self::archive_member((**archive).clone(), p)),
                None => Some((**archive).clone()),
            },
            Self::Virtual { .. } => None,
        }
    }

    /// The final component, as the platform stores it.
    ///
    /// Returned as an [`OsString`] because a filename is not guaranteed to be
    /// valid UTF-8 (`docs/SECURITY.md` §3).
    pub fn file_name(&self) -> Option<OsString> {
        match self {
            Self::Local { path } => path.file_name().map(OsString::from),
            Self::ArchiveMember { member, .. } => member
                .rsplit('/')
                .find(|s| !s.is_empty())
                .map(OsString::from),
            Self::Virtual { id, .. } => Some(OsString::from(id)),
        }
    }
}

/// Parent of a `/`-separated archive member path, if it has one.
fn parent_member(member: &str) -> Option<String> {
    let trimmed = member.trim_end_matches('/');
    let idx = trimmed.rfind('/')?;
    Some(trimmed[..idx].to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_exposes_a_path_and_others_do_not() {
        let local = Location::local("/tmp/a.txt");
        assert_eq!(local.as_path(), Some(Path::new("/tmp/a.txt")));

        let member = Location::archive_member(Location::local("/tmp/a.zip"), "dir/b.txt");
        assert_eq!(
            member.as_path(),
            None,
            "an archive member has no filesystem path"
        );

        let virt = Location::virtual_location("search", "42");
        assert_eq!(virt.as_path(), None);
    }

    #[test]
    fn only_local_is_a_writable_target() {
        assert!(Location::local("/tmp").is_writable_target());
        assert!(!Location::virtual_location("search", "1").is_writable_target());
        assert!(!Location::archive_member(Location::local("/a.zip"), "x").is_writable_target());
    }

    #[test]
    fn archive_member_parent_walks_into_the_archive_then_stops() {
        let a = Location::local("/tmp/a.zip");
        let deep = Location::archive_member(a.clone(), "dir/sub/file.txt");

        let p1 = deep.parent().unwrap();
        assert_eq!(p1, Location::archive_member(a.clone(), "dir/sub"));

        let p2 = p1.parent().unwrap();
        assert_eq!(p2, Location::archive_member(a.clone(), "dir"));

        // The parent of a top-level member is the archive itself.
        assert_eq!(p2.parent().unwrap(), a);
    }

    #[test]
    fn nested_archives_are_representable() {
        let outer = Location::local("/tmp/outer.zip");
        let inner = Location::archive_member(outer, "inner.zip");
        let leaf = Location::archive_member(inner, "notes.txt");
        assert_eq!(leaf.file_name().unwrap(), OsString::from("notes.txt"));
    }

    #[test]
    fn round_trips_through_serde() {
        let leaf = Location::archive_member(Location::local("/tmp/a.zip"), "dir/b.txt");
        let json = serde_json::to_string(&leaf).unwrap();
        let back: Location = serde_json::from_str(&json).unwrap();
        assert_eq!(leaf, back);
    }
}
