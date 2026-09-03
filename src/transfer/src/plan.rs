//! What is being moved, where to, and what that will cost.
//!
//! Separate from `jtf_ops::Plan` because the two answer different questions.
//! A local plan walks the tree to count bytes, because a local `stat` is
//! free. Here every `stat` is a round trip, and walking a ten-thousand-entry
//! tree before anything starts is minutes of a window that looks frozen — so
//! the sizes come from the listing that is already on screen, and folders are
//! measured as they are entered.

use std::path::PathBuf;

use jtf_core::{Error, ErrorCode, Location};
use jtf_fs::sftp::Endpoint;

/// Which machine an entry is on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Side {
    /// This machine.
    Local(PathBuf),
    /// A server, and the absolute path on it.
    Remote {
        /// Which server.
        endpoint: Endpoint,
        /// The path there, always `/`-separated.
        path: String,
    },
}

impl Side {
    /// Read a location as a side, whichever kind it is.
    pub fn of(location: &Location) -> Option<Self> {
        if let Some(path) = location.as_path() {
            return Some(Self::Local(path.to_path_buf()));
        }
        let endpoint = Endpoint::of(location)?;
        Some(Self::Remote {
            endpoint,
            path: location.remote_path()?.to_string(),
        })
    }

    /// The last component, which is the name the entry keeps when it lands.
    pub fn name(&self) -> Option<String> {
        match self {
            Self::Local(path) => path.file_name().map(|n| n.to_string_lossy().into_owned()),
            Self::Remote { path, .. } => path
                .rsplit('/')
                .find(|part| !part.is_empty())
                .map(str::to_owned),
        }
    }

    /// Whether this is on a server.
    pub const fn is_remote(&self) -> bool {
        matches!(self, Self::Remote { .. })
    }

    /// The server, if it is on one.
    pub const fn endpoint(&self) -> Option<&Endpoint> {
        match self {
            Self::Remote { endpoint, .. } => Some(endpoint),
            Self::Local(_) => None,
        }
    }

    /// This side with `name` appended.
    #[must_use]
    pub fn join(&self, name: &str) -> Self {
        match self {
            Self::Local(path) => Self::Local(path.join(name)),
            Self::Remote { endpoint, path } => Self::Remote {
                endpoint: endpoint.clone(),
                path: format!("{}/{name}", path.trim_end_matches('/')),
            },
        }
    }

    /// How it should be written in a message.
    pub fn display(&self) -> String {
        match self {
            Self::Local(path) => path.display().to_string(),
            Self::Remote {
                endpoint,
                path,
            } => format!("sftp://{}@{}{path}", endpoint.user, endpoint.host),
        }
    }
}

/// What to do with the sources.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// Put a copy at the destination and leave the source alone.
    Copy,
    /// Put a copy at the destination and then remove the source.
    ///
    /// Two steps, and there is no way to make it one. A local move within one
    /// filesystem is a rename and is atomic; nothing across a network is. An
    /// interrupted move leaves the copy that was made and the source still
    /// there, which is the safe half to be left with but is not nothing, and
    /// the user is told so before it starts.
    Move,
    /// Remove the sources. On a server there is no trash to take them back
    /// out of.
    Delete,
}

impl Kind {
    /// Whether a destination is needed.
    pub const fn needs_destination(self) -> bool {
        matches!(self, Self::Copy | Self::Move)
    }

    /// Whether the sources are removed once the bytes are across.
    pub const fn removes_source(self) -> bool {
        matches!(self, Self::Move | Self::Delete)
    }
}

/// One thing to transfer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    /// Where it is now.
    pub source: Side,
    /// Where it will land. `None` for a delete.
    pub destination: Option<Side>,
    /// Bytes, as far as the listing knew. Zero for a folder, whose contents
    /// are counted as they are reached.
    pub bytes: u64,
    /// Whether it is a folder, and so has to be walked.
    pub is_directory: bool,
    /// Whether it is a symbolic link.
    ///
    /// Carried because a link is not its target: copying one by following it
    /// duplicates data the user did not select, and a server's link can point
    /// somewhere that does not exist here at all.
    pub is_symlink: bool,
}

/// A checked transfer, ready to run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    /// What is being done.
    pub kind: Kind,
    /// The top-level items. Folders are expanded while running, not here.
    pub items: Vec<Item>,
    /// Where everything is going. `None` for a delete.
    pub destination: Option<Side>,
    /// Bytes known up front. A folder contributes nothing until it is walked,
    /// so this grows during the run and the bar is honest about that.
    pub known_bytes: u64,
    /// Top-level destinations that are already occupied.
    ///
    /// Only the top level, and filled from a single listing of the
    /// destination folder. The local planner walks the whole tree to find
    /// every collision; here that is a round trip per entry, and one listing
    /// answers for every name being dropped in - which is the collision
    /// anyone is actually about to make. Deeper ones get the policy applied
    /// as they are reached, and are reported afterwards.
    pub conflicts: Vec<Side>,
}

impl Plan {
    /// Build a transfer, refusing the shapes that cannot work.
    ///
    /// # Errors
    ///
    /// [`ErrorCode::InvalidPath`] when the destination is inside a source, or
    /// a source is the destination; [`ErrorCode::WrongKind`] when there is
    /// nothing to do or a destination is missing.
    pub fn build(kind: Kind, sources: Vec<Item>, destination: Option<Side>) -> Result<Self, Error> {
        if sources.is_empty() {
            return Err(Error::new(ErrorCode::WrongKind, "nothing selected"));
        }
        if kind.needs_destination() && destination.is_none() {
            return Err(Error::new(ErrorCode::WrongKind, "no destination"));
        }

        let mut items = Vec::with_capacity(sources.len());
        let mut known_bytes = 0u64;
        for mut item in sources {
            if let Some(into) = &destination {
                let Some(name) = item.source.name() else {
                    return Err(Error::new(
                        ErrorCode::InvalidPath,
                        format!("{}: no name to give it", item.source.display()),
                    ));
                };
                let target = into.join(&name);
                // Copying a folder into itself would recurse until something
                // filled up. Refused before a byte moves.
                if item.is_directory && contains(&item.source, into) {
                    return Err(Error::new(
                        ErrorCode::InvalidPath,
                        format!("{}: that is inside itself", item.source.display()),
                    ));
                }
                if target == item.source {
                    return Err(Error::new(
                        ErrorCode::InvalidPath,
                        format!("{}: source and destination are the same", target.display()),
                    ));
                }
                item.destination = Some(target);
            } else {
                item.destination = None;
            }
            known_bytes = known_bytes.saturating_add(item.bytes);
            items.push(item);
        }

        Ok(Self {
            kind,
            items,
            destination,
            known_bytes,
            conflicts: Vec::new(),
        })
    }

    /// Record which top-level destinations are taken.
    ///
    /// `existing` is the names directly inside the destination folder, from
    /// one listing. Pure, so the rule is testable without a server.
    pub fn note_conflicts(&mut self, existing: &[String]) {
        self.conflicts = self
            .items
            .iter()
            .filter_map(|item| item.destination.as_ref())
            .filter(|target| {
                target
                    .name()
                    .is_some_and(|name| existing.contains(&name))
            })
            .cloned()
            .collect();
    }

    /// Whether anything here touches a server.
    pub fn touches_a_server(&self) -> bool {
        self.destination.as_ref().is_some_and(Side::is_remote)
            || self.items.iter().any(|i| i.source.is_remote())
    }

    /// Whether this removes anything from a server.
    ///
    /// What decides which confirmation is asked: a server has no trash, so
    /// this is the permanent kind however the local side would have behaved.
    pub fn deletes_on_a_server(&self) -> bool {
        self.kind.removes_source() && self.items.iter().any(|i| i.source.is_remote())
    }

    /// Whether a move here can be done as a rename.
    ///
    /// Only within one server: the protocol renames, which is atomic and
    /// moves no bytes. Everything else is a copy followed by a delete.
    pub fn is_same_server_rename(&self) -> bool {
        if self.kind != Kind::Move {
            return false;
        }
        let Some(Side::Remote { endpoint: to, .. }) = &self.destination else {
            return false;
        };
        self.items
            .iter()
            .all(|item| item.source.endpoint() == Some(to))
    }
}

/// Whether `outer` is `inner` or contains it.
fn contains(outer: &Side, inner: &Side) -> bool {
    match (outer, inner) {
        (Side::Local(a), Side::Local(b)) => b.starts_with(a),
        (
            Side::Remote {
                endpoint: ea,
                path: a,
            },
            Side::Remote {
                endpoint: eb,
                path: b,
            },
        ) => {
            // Component-wise, so `/srv/data2` is not inside `/srv/data`.
            ea == eb
                && (b == a
                    || b.strip_prefix(a.trim_end_matches('/'))
                        .is_some_and(|rest| rest.starts_with('/')))
        }
        _ => false,
    }
}

impl Side {
    /// A path for messages, matching `Path::display`.
    fn display_path(&self) -> String {
        self.display()
    }
}

impl core::fmt::Display for Side {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(&self.display_path())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn endpoint() -> Endpoint {
        Endpoint {
            host: "host.example".into(),
            port: 22,
            user: "jason".into(),
        }
    }

    fn remote(path: &str) -> Side {
        Side::Remote {
            endpoint: endpoint(),
            path: path.to_string(),
        }
    }

    fn item(source: Side, bytes: u64, is_directory: bool) -> Item {
        Item {
            source,
            destination: None,
            bytes,
            is_directory,
            is_symlink: false,
        }
    }

    #[test]
    fn a_remote_location_reads_as_a_remote_side() {
        let location = Location::parse_display("sftp://jason@host.example/srv/data/file.txt");
        let side = Side::of(&location).expect("a side");
        assert_eq!(side, remote("/srv/data/file.txt"));
        assert_eq!(side.name().as_deref(), Some("file.txt"));
        assert!(side.is_remote());
    }

    #[test]
    fn a_local_location_reads_as_a_local_side() {
        let location = Location::local("/tmp/x/y.txt");
        let side = Side::of(&location).expect("a side");
        assert_eq!(side, Side::Local(PathBuf::from("/tmp/x/y.txt")));
        assert!(!side.is_remote());
    }

    #[test]
    fn joining_uses_the_separator_that_side_actually_uses() {
        assert_eq!(remote("/srv/data").join("f.txt"), remote("/srv/data/f.txt"));
        assert_eq!(
            remote("/srv/data/").join("f.txt"),
            remote("/srv/data/f.txt"),
            "a trailing slash produced a doubled one"
        );
        assert_eq!(
            Side::Local(PathBuf::from("/tmp")).join("f.txt"),
            Side::Local(PathBuf::from("/tmp/f.txt"))
        );
    }

    #[test]
    fn a_destination_is_worked_out_for_every_item() {
        let plan = Plan::build(
            Kind::Copy,
            vec![item(remote("/srv/a.txt"), 10, false)],
            Some(Side::Local(PathBuf::from("/tmp/here"))),
        )
        .unwrap();
        assert_eq!(
            plan.items[0].destination,
            Some(Side::Local(PathBuf::from("/tmp/here/a.txt")))
        );
        assert_eq!(plan.known_bytes, 10);
    }

    #[test]
    fn a_folder_cannot_be_copied_into_itself() {
        let err = Plan::build(
            Kind::Copy,
            vec![item(remote("/srv/data"), 0, true)],
            Some(remote("/srv/data/inner")),
        )
        .unwrap_err();
        assert_eq!(err.code(), ErrorCode::InvalidPath);
    }

    #[test]
    fn a_sibling_whose_name_is_a_prefix_is_not_inside() {
        // /srv/data2 starts with /srv/data as text and is a different folder.
        assert!(Plan::build(
            Kind::Copy,
            vec![item(remote("/srv/data"), 0, true)],
            Some(remote("/srv/data2")),
        )
        .is_ok());
    }

    #[test]
    fn copying_something_onto_itself_is_refused() {
        let err = Plan::build(
            Kind::Copy,
            vec![item(remote("/srv/a.txt"), 10, false)],
            Some(remote("/srv")),
        )
        .unwrap_err();
        assert_eq!(err.code(), ErrorCode::InvalidPath);
    }

    #[test]
    fn a_delete_needs_no_destination_and_a_copy_does() {
        assert!(Plan::build(Kind::Delete, vec![item(remote("/srv/a"), 1, false)], None).is_ok());
        assert!(Plan::build(Kind::Copy, vec![item(remote("/srv/a"), 1, false)], None).is_err());
        assert!(Plan::build(Kind::Copy, Vec::new(), Some(remote("/srv"))).is_err());
    }

    #[test]
    fn conflicts_come_from_one_listing_of_the_destination() {
        let mut plan = Plan::build(
            Kind::Copy,
            vec![
                item(remote("/srv/a.txt"), 10, false),
                item(remote("/srv/b.txt"), 10, false),
                item(remote("/srv/c.txt"), 10, false),
            ],
            Some(Side::Local(PathBuf::from("/tmp/here"))),
        )
        .unwrap();
        assert!(plan.conflicts.is_empty(), "nothing asked, nothing found");

        plan.note_conflicts(&["a.txt".to_string(), "c.txt".to_string(), "z.txt".to_string()]);
        assert_eq!(plan.conflicts.len(), 2);
        assert_eq!(
            plan.conflicts[0],
            Side::Local(PathBuf::from("/tmp/here/a.txt"))
        );
        assert_eq!(
            plan.conflicts[1],
            Side::Local(PathBuf::from("/tmp/here/c.txt"))
        );
    }

    #[test]
    fn a_move_within_one_server_is_a_rename_and_across_two_is_not() {
        let same = Plan::build(
            Kind::Move,
            vec![item(remote("/srv/a.txt"), 10, false)],
            Some(remote("/srv/other")),
        )
        .unwrap();
        assert!(same.is_same_server_rename());

        let elsewhere = Plan::build(
            Kind::Move,
            vec![item(
                Side::Remote {
                    endpoint: Endpoint {
                        host: "other.example".into(),
                        port: 22,
                        user: "jason".into(),
                    },
                    path: "/srv/a.txt".into(),
                },
                10,
                false,
            )],
            Some(remote("/srv/other")),
        )
        .unwrap();
        assert!(
            !elsewhere.is_same_server_rename(),
            "two servers cannot rename between themselves"
        );

        let down = Plan::build(
            Kind::Move,
            vec![item(remote("/srv/a.txt"), 10, false)],
            Some(Side::Local(PathBuf::from("/tmp"))),
        )
        .unwrap();
        assert!(!down.is_same_server_rename());
    }

    #[test]
    fn removing_from_a_server_is_known_to_be_permanent() {
        let moved = Plan::build(
            Kind::Move,
            vec![item(remote("/srv/a.txt"), 10, false)],
            Some(Side::Local(PathBuf::from("/tmp"))),
        )
        .unwrap();
        assert!(moved.deletes_on_a_server(), "a move takes the source away");

        let copied = Plan::build(
            Kind::Copy,
            vec![item(remote("/srv/a.txt"), 10, false)],
            Some(Side::Local(PathBuf::from("/tmp"))),
        )
        .unwrap();
        assert!(!copied.deletes_on_a_server());

        let uploaded = Plan::build(
            Kind::Move,
            vec![item(Side::Local(PathBuf::from("/tmp/a.txt")), 10, false)],
            Some(remote("/srv")),
        )
        .unwrap();
        assert!(
            !uploaded.deletes_on_a_server(),
            "the source is local, so the trash still applies to it"
        );
    }
}
