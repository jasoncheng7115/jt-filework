//! The file entry model.
//!
//! `docs/ARCHITECTURE.md` §8. Two rules drive the shape of this type:
//!
//! 1. An entry is not a path string.
//! 2. The **raw name** (what the platform stores) and the **display name**
//!    (what a human sees) are different fields and are never conflated.
//!    Filenames are not guaranteed to be valid UTF-8 on Unix, and normalizing
//!    or lossily converting them silently is how file managers lose data.

use std::ffi::{OsStr, OsString};
use std::time::SystemTime;

use serde::{Deserialize, Serialize};

use crate::location::Location;

/// What an entry actually is.
///
/// This is deliberately richer than file/directory: the difference between a
/// symlink, an alias, a bundle and a package changes what the UI shows and
/// what an operation is allowed to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum FileKind {
    /// A regular file.
    File,
    /// A directory.
    Directory,
    /// A symbolic link. Never followed implicitly by recursive delete
    /// (`docs/SECURITY.md` §3.1).
    Symlink,
    /// A macOS alias, resolved through the platform adapter.
    Alias,
    /// An application bundle. Presented as one item by default.
    ApplicationBundle,
    /// A non-application package directory presented as one item.
    Package,
    /// An archive. Untrusted (`docs/SECURITY.md` §4).
    Archive,
    /// A device node or similar special file.
    Device,
    /// An item on a remote provider that may not be materialized locally.
    RemoteItem,
    /// A row in a virtual result set, e.g. search results.
    VirtualResult,
    /// The kind could not be determined.
    Unknown,
}

impl FileKind {
    /// Whether the UI should treat this entry as a container to navigate into
    /// by default.
    ///
    /// Bundles and packages are containers on disk but are presented as single
    /// items, so they are excluded here and traversed only on explicit request
    /// (`docs/PLATFORM_INTEGRATION.md` §2.2).
    pub const fn is_navigable_by_default(self) -> bool {
        matches!(self, Self::Directory)
    }

    /// Whether this entry is a directory on disk, regardless of presentation.
    pub const fn is_directory_on_disk(self) -> bool {
        matches!(
            self,
            Self::Directory | Self::ApplicationBundle | Self::Package
        )
    }

    /// Whether content from this entry must be treated as untrusted input
    /// (`docs/SECURITY.md` §2).
    pub const fn is_untrusted_container(self) -> bool {
        matches!(self, Self::Archive)
    }
}

/// A filename exactly as the platform stores it.
///
/// Wrapping [`OsString`] rather than using `String` is deliberate: it makes a
/// lossy conversion an explicit call rather than an accident.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct RawName(OsString);

impl RawName {
    /// Wrap a platform-native name.
    pub fn new(name: impl Into<OsString>) -> Self {
        Self(name.into())
    }

    /// Borrow the platform-native name.
    pub fn as_os_str(&self) -> &OsStr {
        &self.0
    }

    /// The name as UTF-8, if it happens to be valid UTF-8.
    pub fn to_str(&self) -> Option<&str> {
        self.0.to_str()
    }

    /// A display form, replacing invalid sequences with U+FFFD.
    ///
    /// This is for **display only**. Never use the result to build a path.
    pub fn to_display_string(&self) -> String {
        self.0.to_string_lossy().into_owned()
    }

    /// Whether the name is not representable as UTF-8, meaning the display
    /// form is lossy and the UI should say so.
    pub fn is_lossy_as_utf8(&self) -> bool {
        self.0.to_str().is_none()
    }
}

/// Timestamps a platform may report. All are optional: not every filesystem
/// records every one, and pretending otherwise produces fake data.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Timestamps {
    /// Last content modification.
    pub modified: Option<SystemTime>,
    /// Creation / birth time.
    pub created: Option<SystemTime>,
    /// Last access.
    pub accessed: Option<SystemTime>,
    /// Last metadata change, where the platform distinguishes it.
    pub metadata_changed: Option<SystemTime>,
}

/// Platform-independent attribute flags.
///
/// These are four genuinely independent facts a platform reports about an
/// entry, not a state machine that should be an enum: a file can be hidden
/// and read-only and a cloud placeholder at the same time.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Attributes {
    /// Hidden by platform convention (dot-prefix, hidden flag, or both).
    pub hidden: bool,
    /// Marked read-only by the platform.
    pub read_only: bool,
    /// Marked as a system item.
    pub system: bool,
    /// A cloud placeholder that is not materialized locally. Enumeration and
    /// preview must not hydrate it implicitly
    /// (`docs/PLATFORM_INTEGRATION.md` §5).
    pub cloud_placeholder: bool,
}

/// A coarse, cross-platform summary of access rights.
///
/// This is a summary for display and for deciding what to offer. It is never
/// a substitute for asking the platform at the moment of the operation: the
/// answer can change between the check and the use
/// (`docs/SECURITY.md` §3.1).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PermissionsSummary {
    /// The current user appears able to read.
    pub readable: bool,
    /// The current user appears able to write.
    pub writable: bool,
    /// The entry appears executable or, for a directory, traversable.
    pub executable: bool,
}

/// One row in a pane, or one target of an operation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileEntry {
    location: Location,
    raw_name: RawName,
    kind: FileKind,
    size: Option<u64>,
    timestamps: Timestamps,
    attributes: Attributes,
    permissions: PermissionsSummary,
    content_type: Option<String>,
}

impl FileEntry {
    /// Create an entry with the minimum a provider must always know.
    pub fn new(location: Location, raw_name: RawName, kind: FileKind) -> Self {
        Self {
            location,
            raw_name,
            kind,
            size: None,
            timestamps: Timestamps::default(),
            attributes: Attributes::default(),
            permissions: PermissionsSummary::default(),
            content_type: None,
        }
    }

    /// Set the size in bytes.
    #[must_use]
    pub fn with_size(mut self, size: u64) -> Self {
        self.size = Some(size);
        self
    }

    /// Set the timestamps.
    #[must_use]
    pub fn with_timestamps(mut self, timestamps: Timestamps) -> Self {
        self.timestamps = timestamps;
        self
    }

    /// Set the attribute flags.
    #[must_use]
    pub fn with_attributes(mut self, attributes: Attributes) -> Self {
        self.attributes = attributes;
        self
    }

    /// Set the permission summary.
    #[must_use]
    pub fn with_permissions(mut self, permissions: PermissionsSummary) -> Self {
        self.permissions = permissions;
        self
    }

    /// Set the resolved content type (MIME or platform content type).
    #[must_use]
    pub fn with_content_type(mut self, content_type: impl Into<String>) -> Self {
        self.content_type = Some(content_type.into());
        self
    }

    /// Where this entry lives.
    pub const fn location(&self) -> &Location {
        &self.location
    }

    /// The name exactly as the platform stores it. Use this to build paths.
    pub const fn raw_name(&self) -> &RawName {
        &self.raw_name
    }

    /// The name to show a human. Never use this to build a path.
    pub fn display_name(&self) -> String {
        self.raw_name.to_display_string()
    }

    /// What this entry is.
    pub const fn kind(&self) -> FileKind {
        self.kind
    }

    /// Size in bytes, where the provider knows it.
    pub const fn size(&self) -> Option<u64> {
        self.size
    }

    /// Timestamps, where the platform reports them.
    pub const fn timestamps(&self) -> &Timestamps {
        &self.timestamps
    }

    /// Attribute flags.
    pub const fn attributes(&self) -> &Attributes {
        &self.attributes
    }

    /// Coarse permission summary.
    pub const fn permissions(&self) -> &PermissionsSummary {
        &self.permissions
    }

    /// Resolved content type, where known.
    pub fn content_type(&self) -> Option<&str> {
        self.content_type.as_deref()
    }

    /// The file extension as UTF-8, lowercased, if the name has one.
    ///
    /// Extension is only ever a *hint*: `docs/VIEWER_PREVIEW.md` §1 requires
    /// magic bytes to override a lying extension.
    pub fn extension_hint(&self) -> Option<String> {
        let name = self.raw_name.to_str()?;
        let idx = name.rfind('.')?;
        if idx == 0 || idx + 1 == name.len() {
            return None;
        }
        Some(name[idx + 1..].to_ascii_lowercase())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(name: &str, kind: FileKind) -> FileEntry {
        FileEntry::new(
            Location::local(format!("/tmp/{name}")),
            RawName::new(name),
            kind,
        )
    }

    #[test]
    fn raw_name_and_display_name_are_separate_fields() {
        // Even for a valid-UTF-8 name the two are distinct accessors: the raw
        // name is what you build paths from, the display name is what you show.
        // The non-UTF-8 case is covered in tests/raw_name_non_utf8.rs, which is
        // gated to Unix because only Unix can express such a name.
        let e = FileEntry::new(
            Location::local("/tmp/\u{4e2d}\u{6587}.txt"),
            RawName::new("\u{4e2d}\u{6587}.txt"),
            FileKind::File,
        );
        assert_eq!(e.raw_name().as_os_str(), OsStr::new("\u{4e2d}\u{6587}.txt"));
        assert_eq!(e.display_name(), "\u{4e2d}\u{6587}.txt");
        assert!(!e.raw_name().is_lossy_as_utf8());
    }

    #[test]
    fn bundles_are_directories_on_disk_but_not_navigated_by_default() {
        assert!(FileKind::ApplicationBundle.is_directory_on_disk());
        assert!(!FileKind::ApplicationBundle.is_navigable_by_default());
        assert!(FileKind::Package.is_directory_on_disk());
        assert!(!FileKind::Package.is_navigable_by_default());
        assert!(FileKind::Directory.is_navigable_by_default());
    }

    #[test]
    fn archives_are_flagged_as_untrusted_containers() {
        assert!(FileKind::Archive.is_untrusted_container());
        assert!(!FileKind::Directory.is_untrusted_container());
    }

    #[test]
    fn extension_hint_ignores_dotfiles_and_trailing_dots() {
        assert_eq!(
            entry("a.TXT", FileKind::File).extension_hint().as_deref(),
            Some("txt")
        );
        assert_eq!(entry(".gitignore", FileKind::File).extension_hint(), None);
        assert_eq!(
            entry("archive.tar.gz", FileKind::File)
                .extension_hint()
                .as_deref(),
            Some("gz")
        );
        assert_eq!(entry("trailing.", FileKind::File).extension_hint(), None);
        assert_eq!(entry("noext", FileKind::File).extension_hint(), None);
    }

    #[test]
    fn optional_metadata_is_absent_rather_than_faked() {
        let e = entry("a.txt", FileKind::File);
        assert_eq!(e.size(), None, "unknown size must be None, not 0");
        assert_eq!(e.timestamps().modified, None);
        assert_eq!(e.content_type(), None);
    }

    #[test]
    fn round_trips_through_serde() {
        let e = entry("a.txt", FileKind::File)
            .with_size(42)
            .with_content_type("text/plain");
        let json = serde_json::to_string(&e).unwrap();
        let back: FileEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(e, back);
    }
}
