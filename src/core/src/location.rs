//! Where an entry lives.
//!
//! `docs/ARCHITECTURE.md` §8: "Do not model an entry as a path string only."
//! A location may be a real path, a member inside an archive, or a row in a
//! virtual result set. Modelling this up front keeps search results and
//! archive browsing from being bolted on later as special cases.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// The port SSH and SFTP use unless told otherwise.
///
/// Written down once because three places had to agree on it: the display
/// form below, the `[host]:port` spelling `known_hosts` wants, and the saved
/// server list.
pub const DEFAULT_SSH_PORT: u16 = 22;

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
    /// A path on a host reached over SFTP (`docs/adr/0004-sftp.md`).
    ///
    /// The path is remote and `/`-separated whatever this machine's separator
    /// is, so it is a `String` rather than a `PathBuf`: turning it into a
    /// local path type is exactly the mistake that makes a Windows build
    /// send backslashes to a POSIX server.
    ///
    /// No credential is stored here. The host and user identify *which*
    /// connection; how to authenticate is the connection's own business and
    /// is never written to the session file.
    Remote {
        /// Hostname or address, as the user typed it.
        host: String,
        /// Port, normally 22.
        port: u16,
        /// Account on that host.
        user: String,
        /// Absolute path on the host, `/`-separated.
        path: String,
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

    /// A path on a host reached over SFTP.
    pub fn remote(
        host: impl Into<String>,
        port: u16,
        user: impl Into<String>,
        path: impl Into<String>,
    ) -> Self {
        Self::Remote {
            host: host.into(),
            port,
            user: user.into(),
            path: path.into(),
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

    /// What to show a person who asks "where am I?".
    ///
    /// Separate from [`Self::as_path`] on purpose. `as_path` answers a
    /// question about the local filesystem and is `None` whenever there is no
    /// local file - which is the right answer for a bookmark or for resolving
    /// a typed relative path, and the wrong one for the path bar, which then
    /// goes blank the moment a pane is pointed at a server.
    pub fn display_text(&self) -> String {
        match self {
            Self::Local { path } => path.display().to_string(),
            Self::Remote {
                host,
                port,
                user,
                path,
            } => {
                let authority = if *port == DEFAULT_SSH_PORT {
                    format!("{user}@{host}")
                } else {
                    format!("{user}@{host}:{port}")
                };
                // The stored path always starts with `/`, so this does not
                // double the separator.
                format!("sftp://{authority}{path}")
            }
            Self::ArchiveMember { archive, member } => {
                format!("{}/{member}", archive.display_text())
            }
            Self::Virtual { id, .. } => id.clone(),
        }
    }

    /// Read back what [`Self::display_text`] wrote.
    ///
    /// The interface passes locations around as the strings it shows - the
    /// folder tree stores a path per node, the breadcrumb hands one back when
    /// a segment is clicked - and those strings have to turn into locations
    /// again or everything downstream assumes local.
    ///
    /// Anything that is not an `sftp://` URL is a local path, including a
    /// Windows one: `C:\Users` has a colon but no `//` after it.
    #[must_use]
    pub fn parse_display(text: &str) -> Self {
        let Some(rest) = text.strip_prefix("sftp://") else {
            return Self::local(text);
        };
        // `user@host[:port]/path`. The first `/` ends the authority; a `@`
        // inside the authority separates the user, and a `:` after that the
        // port.
        let (authority, path) = match rest.find('/') {
            Some(at) => (&rest[..at], &rest[at..]),
            None => (rest, "/"),
        };
        let (user, host_port) = match authority.rsplit_once('@') {
            Some((user, host)) => (user, host),
            None => ("", authority),
        };
        let (host, port) = match host_port.rsplit_once(':') {
            Some((host, port)) => match port.parse::<u16>() {
                Ok(port) => (host, port),
                // Not a number, so the colon was part of the host - an IPv6
                // literal, most likely - and there is no port here.
                Err(_) => (host_port, DEFAULT_SSH_PORT),
            },
            None => (host_port, DEFAULT_SSH_PORT),
        };
        Self::remote(host, port, user, path)
    }

    /// The remote path, if this is a remote location.
    pub fn remote_path(&self) -> Option<&str> {
        match self {
            Self::Remote { path, .. } => Some(path),
            _ => None,
        }
    }

    /// Whether operations on this location may write to the filesystem.
    ///
    /// A remote location is writable in principle, but only stage two of
    /// ADR-0004 builds the writes; until then the provider refuses them, and
    /// refusing there rather than here keeps this answer about the *kind* of
    /// location rather than about how far the implementation has got.
    pub const fn is_writable_target(&self) -> bool {
        matches!(self, Self::Local { .. } | Self::Remote { .. })
    }

    /// Whether reaching this location needs a network connection.
    ///
    /// The UI asks so it can refuse the things that are about local files -
    /// Quick Look, Reveal, Open With, the trash - rather than offering them
    /// and failing.
    pub const fn is_remote(&self) -> bool {
        matches!(self, Self::Remote { .. })
    }

    /// The parent location, where one exists.
    pub fn parent(&self) -> Option<Self> {
        match self {
            Self::Local { path } => path.parent().map(Self::local),
            Self::ArchiveMember { archive, member } => match parent_member(member) {
                Some(p) => Some(Self::archive_member((**archive).clone(), p)),
                None => Some((**archive).clone()),
            },
            // A remote path is `/`-separated by protocol, so it is split
            // here rather than by `Path`, whose idea of a separator is this
            // machine's and not the server's.
            Self::Remote {
                host,
                port,
                user,
                path,
            } => remote_parent(path).map(|parent| Self::remote(host, *port, user, parent)),
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
            Self::Remote { path, .. } => path
                .trim_end_matches('/')
                .rsplit('/')
                .find(|s| !s.is_empty())
                .map(OsString::from),
            Self::Virtual { id, .. } => Some(OsString::from(id)),
        }
    }
}

/// Parent of a `/`-separated remote path, or `None` at the root.
fn remote_parent(path: &str) -> Option<String> {
    let trimmed = path.trim_end_matches('/');
    if trimmed.is_empty() || trimmed == "/" {
        return None;
    }
    match trimmed.rfind('/') {
        Some(0) => Some("/".to_string()),
        Some(at) => Some(trimmed[..at].to_string()),
        None => None,
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
    use std::ffi::OsStr;

    #[test]
    fn a_remote_path_walks_up_by_its_own_separator() {
        // Not by `Path`: on Windows `Path::parent` would treat a backslash as
        // a separator and a forward slash as an ordinary character, and the
        // server's paths are the server's.
        let deep = Location::remote("host", 22, "jt", "/srv/data/reports");
        let mid = deep.parent().expect("has a parent");
        assert_eq!(mid, Location::remote("host", 22, "jt", "/srv/data"));
        let top = mid.parent().expect("has a parent");
        assert_eq!(top, Location::remote("host", 22, "jt", "/srv"));
        let root = top.parent().expect("has a parent");
        assert_eq!(root, Location::remote("host", 22, "jt", "/"));
        assert_eq!(root.parent(), None, "the remote root has no parent");
    }

    #[test]
    fn a_remote_location_has_a_name_but_no_local_path() {
        let remote = Location::remote("host", 22, "jt", "/srv/data/report.pdf");
        assert_eq!(
            remote.file_name().as_deref(),
            Some(OsStr::new("report.pdf"))
        );
        assert_eq!(
            remote.as_path(),
            None,
            "a remote path is not a path on this machine, and treating it as \
             one is how a download lands in the wrong place"
        );
        assert!(remote.is_remote());
        assert_eq!(remote.remote_path(), Some("/srv/data/report.pdf"));
    }

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

    /// The path bar asked `as_path`, which is `None` for a server, so the bar
    /// went blank the moment a pane was pointed at one - and the folder tree,
    /// asking the same question, kept showing the last local folder as though
    /// nothing had happened.
    #[test]
    fn a_remote_location_has_something_to_show_in_the_path_bar() {
        let remote = Location::remote("host.example", DEFAULT_SSH_PORT, "jason", "/srv/data");
        assert_eq!(remote.as_path(), None);
        assert_eq!(remote.display_text(), "sftp://jason@host.example/srv/data");
    }

    /// The port is shown only when it is not the one everybody assumes, the
    /// same rule `known_hosts` follows.
    #[test]
    fn a_non_default_port_is_part_of_what_is_shown() {
        let remote = Location::remote("host.example", 2222, "jason", "/");
        assert_eq!(remote.display_text(), "sftp://jason@host.example:2222/");
    }

    /// What `display_text` writes, `parse_display` must read.
    ///
    /// These two are the only bridge between a location and the strings the
    /// interface passes around, so a disagreement between them silently turns
    /// a server into a local path named `sftp:`.
    #[test]
    fn a_displayed_location_parses_back_to_itself() {
        for original in [
            Location::remote("host.example", DEFAULT_SSH_PORT, "jason", "/srv/data"),
            Location::remote("host.example", 2222, "jason", "/"),
            Location::remote("10.0.0.1", DEFAULT_SSH_PORT, "root", "/var/log"),
            Location::local("/Users/someone/Documents"),
            Location::local("/"),
        ] {
            let shown = original.display_text();
            assert_eq!(
                Location::parse_display(&shown),
                original,
                "{shown} did not parse back"
            );
        }
    }

    /// A Windows path has a colon and is still a local path.
    #[test]
    fn a_windows_path_is_not_mistaken_for_a_url() {
        let parsed = Location::parse_display(r"C:\Users\jason");
        assert!(!parsed.is_remote(), "{parsed:?} should be local");
    }

    #[test]
    fn a_local_location_shows_its_path_unchanged() {
        let local = Location::local("/Users/someone/Documents");
        assert_eq!(local.display_text(), "/Users/someone/Documents");
    }

    /// An archive member is shown as the archive plus the member, so the path
    /// bar reads as one place rather than losing the archive it is inside.
    #[test]
    fn an_archive_member_is_shown_under_its_archive() {
        let inside = Location::archive_member(Location::local("/tmp/a.zip"), "docs/readme.md");
        assert_eq!(inside.display_text(), "/tmp/a.zip/docs/readme.md");
    }
}
