//! Being told when a folder a pane shows changes (ADR-0007).
//!
//! The window hands over the list of folders its panes are showing, whenever
//! that list changes, and drains what happened on each tick. Everything that
//! touches a path - asking what filesystem it is on, adding the watch - happens
//! on a thread of this crate's own, because a path on a mount that has stopped
//! answering blocks whoever touches it, and that must never be the UI thread
//! (`AGENTS.md` §3). The UI thread only sends a list and drains a channel.
//!
//! A folder that cannot be watched is reported as [`Standing::Polled`], with
//! the reason, and the caller goes on polling it the way it did before this
//! crate existed. Nothing here polls.

mod network;

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::SystemTime;

use notify::event::ModifyKind;
use notify::{Event, EventKind, RecursiveMode, Watcher as _};

/// What changed, in the spelling of the folders given to
/// [`Watcher::set_folders`].
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Change {
    /// Read this folder again: an entry appeared, went or was renamed, the
    /// folder itself changed, or the watcher lost track and cannot say what.
    Listing(PathBuf),
    /// An entry directly inside a watched folder changed in place - its size,
    /// its times, its attributes. Only a hint: the backends disagree about
    /// kinds, so the caller should re-list if the entry turns out to be one it
    /// has not got, or one that is no longer there.
    Entry(PathBuf),
}

/// Why a folder is polled rather than watched.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Polled {
    /// On a network or user-space filesystem, where a watch hears only what
    /// this machine did (`network.rs`).
    NetworkFilesystem,
    /// The filesystem would not say what it is, so whether a watch on it can
    /// hear anything is unknown (`network.rs`).
    UnknownFilesystem,
    /// The backend refused the watch: inotify's per-user limit, a permission,
    /// the folder gone before the watch was added.
    Refused,
    /// There is no watcher at all: the backend or its thread would not start.
    Unavailable,
}

/// Where a folder stands, as the watcher's thread last reported it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Standing {
    /// Watched: changes will arrive through [`Watcher::drain`].
    Watched,
    /// Not watched, for the reason given; the caller has to poll it.
    Polled(Polled),
}

/// What one [`Watcher::drain`] found.
#[derive(Debug, Default)]
pub struct Drained {
    /// What changed, each at most once.
    pub changes: Vec<Change>,
    /// Folders whose watch went into place since the last drain, with the
    /// folder's modification time taken just after it did. Anything that
    /// changed between the caller reading the folder and the watch starting
    /// was heard by nobody; comparing this time with the one taken when the
    /// folder was read is how the caller notices.
    pub watched: Vec<(PathBuf, Option<SystemTime>)>,
}

enum Message {
    Event(notify::Result<Event>),
    Watched {
        folder: PathBuf,
        canonical: PathBuf,
        stamp: Option<SystemTime>,
    },
    Polled {
        folder: PathBuf,
        why: Polled,
    },
}

/// The watcher: one per window, one thread behind it.
///
/// Dropping it closes the command channel, and the thread lets go of every
/// watch and ends by itself. It is not joined: a thread that is waiting on a
/// dead mount would hold the window's close hostage.
pub struct Watcher {
    commands: Option<Sender<Vec<PathBuf>>>,
    messages: Receiver<Message>,
    wanted: Vec<PathBuf>,
    standing: HashMap<PathBuf, Standing>,
    /// Every spelling an event can arrive in - the folder as given, and the
    /// folder with its links resolved, which is what FSEvents reports
    /// (`/private/tmp` for `/tmp`) - mapped to the folder as given.
    spellings: HashMap<PathBuf, PathBuf>,
}

impl std::fmt::Debug for Watcher {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Watcher")
            .field("wanted", &self.wanted)
            .field("standing", &self.standing)
            .finish_non_exhaustive()
    }
}

impl Watcher {
    /// Start the watcher's thread. Never fails: if the backend cannot start,
    /// every folder is reported as [`Polled::Unavailable`], which is what the
    /// window did before there was a watcher.
    #[must_use]
    pub fn start() -> Self {
        let (commands, inbox) = mpsc::channel();
        let (outbox, messages) = mpsc::channel();
        let started = std::thread::Builder::new()
            .name("jtf-watch".into())
            .spawn(move || run(&inbox, &outbox))
            .is_ok();
        Self {
            commands: started.then_some(commands),
            messages,
            wanted: Vec::new(),
            standing: HashMap::new(),
            spellings: HashMap::new(),
        }
    }

    /// The folders the panes show now. Cheap to call on every tick: nothing
    /// is sent unless the set differs from the last one.
    pub fn set_folders(&mut self, folders: impl IntoIterator<Item = PathBuf>) {
        let mut wanted: Vec<PathBuf> = folders.into_iter().collect();
        wanted.sort();
        wanted.dedup();
        if wanted == self.wanted {
            return;
        }
        // Forgotten here at once rather than when the thread confirms, so an
        // event still in flight for a folder no pane shows is not reported.
        self.standing
            .retain(|folder, _| wanted.binary_search(folder).is_ok());
        self.spellings
            .retain(|_, folder| wanted.binary_search(folder).is_ok());
        match &self.commands {
            Some(commands) if commands.send(wanted.clone()).is_ok() => {}
            _ => {
                self.commands = None;
                for folder in &wanted {
                    self.standing
                        .insert(folder.clone(), Standing::Polled(Polled::Unavailable));
                }
            }
        }
        self.wanted = wanted;
    }

    /// Where `folder` stands, or `None` while the thread has not yet said -
    /// which the caller should treat as polled.
    #[must_use]
    pub fn standing(&self, folder: &Path) -> Option<Standing> {
        self.standing.get(folder).copied()
    }

    /// Whether changes to `folder` arrive through [`Watcher::drain`].
    #[must_use]
    pub fn is_watched(&self, folder: &Path) -> bool {
        self.standing(folder) == Some(Standing::Watched)
    }

    /// Everything that arrived since the last call. Never blocks.
    ///
    /// Changes are collected into a set as they arrive: extracting an archive
    /// of a hundred thousand files into a watched folder is a hundred thousand
    /// events that all say the same thing, and this runs on the UI thread.
    pub fn drain(&mut self) -> Drained {
        let mut watched = Vec::new();
        let mut changes = HashSet::new();
        while let Ok(message) = self.messages.try_recv() {
            self.receive(message, &mut watched, &mut changes);
        }
        Drained {
            changes: changes.into_iter().collect(),
            watched,
        }
    }

    fn receive(
        &mut self,
        message: Message,
        watched: &mut Vec<(PathBuf, Option<SystemTime>)>,
        changes: &mut HashSet<Change>,
    ) {
        match message {
            Message::Watched {
                folder,
                canonical,
                stamp,
            } => {
                // A folder let go of while the thread was adding its watch.
                if self.wanted.binary_search(&folder).is_err() {
                    return;
                }
                self.spellings.insert(canonical, folder.clone());
                self.spellings.insert(folder.clone(), folder.clone());
                self.standing.insert(folder.clone(), Standing::Watched);
                watched.push((folder, stamp));
            }
            Message::Polled { folder, why } => {
                if self.wanted.binary_search(&folder).is_ok() {
                    self.standing.insert(folder, Standing::Polled(why));
                }
            }
            Message::Event(Ok(event)) => classify(&event, &self.spellings, changes),
            // The backend lost track - a queue that overflowed, a watch the
            // kernel removed - and cannot say of what. Every watched folder
            // is read again, which is always correct and only ever slow.
            Message::Event(Err(_)) => {
                for (folder, standing) in &self.standing {
                    if *standing == Standing::Watched {
                        changes.insert(Change::Listing(folder.clone()));
                    }
                }
            }
        }
    }
}

/// Turn one backend event into changes to the watched folders.
///
/// The kinds are hints and the backends disagree about them - FSEvents
/// reports accumulated flags, Windows reports every write as `Modify(Any)` -
/// so only one distinction is drawn: something that plainly changed an entry
/// in place, which can be re-read on its own, and everything else, which
/// re-reads the folder.
fn classify(event: &Event, spellings: &HashMap<PathBuf, PathBuf>, out: &mut HashSet<Change>) {
    if event.need_rescan() {
        let folders: HashSet<&PathBuf> = spellings.values().collect();
        out.extend(
            folders
                .into_iter()
                .map(|folder| Change::Listing(folder.clone())),
        );
        return;
    }
    if matches!(event.kind, EventKind::Access(_)) {
        return; // opened or read: nothing on screen changes
    }
    let in_place = matches!(
        event.kind,
        EventKind::Modify(
            ModifyKind::Data(_) | ModifyKind::Metadata(_) | ModifyKind::Any | ModifyKind::Other
        )
    );
    for path in &event.paths {
        // The folder itself: renamed, removed, or its own times changed.
        if let Some(folder) = spellings.get(path) {
            out.insert(Change::Listing(folder.clone()));
            continue;
        }
        let (Some(parent), Some(name)) = (path.parent(), path.file_name()) else {
            continue;
        };
        // Deeper than a direct child: not something any pane lists.
        let Some(folder) = spellings.get(parent) else {
            continue;
        };
        out.insert(if in_place {
            Change::Entry(folder.join(name))
        } else {
            Change::Listing(folder.clone())
        });
    }
}

/// The watcher's thread: add and remove watches as the list changes, and
/// report how each folder stands.
fn run(commands: &Receiver<Vec<PathBuf>>, messages: &Sender<Message>) {
    let events = messages.clone();
    let mut backend = notify::recommended_watcher(move |event| {
        // The receiving side gone means the window is closing; nothing to do.
        let _ = events.send(Message::Event(event));
    })
    .ok();
    // Folder as given -> the spelling events arrive in.
    let mut watching: HashMap<PathBuf, PathBuf> = HashMap::new();
    let mut polled: HashMap<PathBuf, Polled> = HashMap::new();

    while let Ok(mut wanted) = commands.recv() {
        // Only the latest list matters; a burst of navigation is one change.
        while let Ok(newer) = commands.try_recv() {
            wanted = newer;
        }
        let wanted: HashSet<PathBuf> = wanted.into_iter().collect();
        watching.retain(|folder, _| {
            if wanted.contains(folder) {
                return true;
            }
            if let Some(backend) = backend.as_mut() {
                // Gone already is as good as removed.
                let _ = backend.unwatch(folder);
            }
            false
        });
        polled.retain(|folder, _| wanted.contains(folder));

        // Every wanted folder is reported on every list, not only the new
        // ones. The window forgets a folder's standing the moment it stops
        // asking for it, and a folder dropped and asked for again between two
        // lists this thread saw as one would otherwise never be reported again.
        for folder in wanted {
            let why = if let Some(why) = polled.get(&folder) {
                Some(*why)
            } else if watching.contains_key(&folder) {
                None
            } else {
                let why = match (backend.as_mut(), network::is_network_filesystem(&folder)) {
                    (None, _) => Some(Polled::Unavailable),
                    (Some(_), Some(true)) => Some(Polled::NetworkFilesystem),
                    (Some(_), None) => Some(Polled::UnknownFilesystem),
                    (Some(backend), Some(false)) => backend
                        .watch(&folder, RecursiveMode::NonRecursive)
                        .err()
                        .map(|_| Polled::Refused),
                };
                if let Some(why) = why {
                    polled.insert(folder.clone(), why);
                } else {
                    let canonical =
                        std::fs::canonicalize(&folder).unwrap_or_else(|_| folder.clone());
                    watching.insert(folder.clone(), canonical);
                }
                why
            };
            let message = match (why, watching.get(&folder)) {
                (None, Some(canonical)) => Message::Watched {
                    canonical: canonical.clone(),
                    stamp: std::fs::metadata(&folder)
                        .and_then(|meta| meta.modified())
                        .ok(),
                    folder,
                },
                (why, _) => Message::Polled {
                    folder,
                    why: why.unwrap_or(Polled::Refused),
                },
            };
            if messages.send(message).is_err() {
                return;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use notify::event::{AccessKind, CreateKind, DataChange, MetadataKind, RemoveKind, RenameMode};

    fn spellings() -> HashMap<PathBuf, PathBuf> {
        let mut map = HashMap::new();
        map.insert(PathBuf::from("/tmp/a"), PathBuf::from("/tmp/a"));
        map.insert(PathBuf::from("/private/tmp/a"), PathBuf::from("/tmp/a"));
        map
    }

    fn changes(kind: EventKind, path: &str) -> Vec<Change> {
        let mut out = HashSet::new();
        classify(
            &Event::new(kind).add_path(PathBuf::from(path)),
            &spellings(),
            &mut out,
        );
        out.into_iter().collect()
    }

    #[test]
    fn an_entry_appearing_or_going_re_reads_the_folder() {
        let listing = vec![Change::Listing(PathBuf::from("/tmp/a"))];
        assert_eq!(
            changes(EventKind::Create(CreateKind::File), "/tmp/a/x"),
            listing
        );
        assert_eq!(
            changes(EventKind::Remove(RemoveKind::Any), "/tmp/a/x"),
            listing
        );
        assert_eq!(
            changes(
                EventKind::Modify(ModifyKind::Name(RenameMode::Both)),
                "/tmp/a/x"
            ),
            listing
        );
        assert_eq!(changes(EventKind::Any, "/tmp/a/x"), listing);
    }

    #[test]
    fn a_file_changing_in_place_is_re_read_on_its_own() {
        let entry = vec![Change::Entry(PathBuf::from("/tmp/a/x"))];
        assert_eq!(
            changes(
                EventKind::Modify(ModifyKind::Data(DataChange::Content)),
                "/tmp/a/x"
            ),
            entry
        );
        assert_eq!(
            changes(
                EventKind::Modify(ModifyKind::Metadata(MetadataKind::Any)),
                "/tmp/a/x"
            ),
            entry
        );
        // Windows reports every write this way.
        assert_eq!(
            changes(EventKind::Modify(ModifyKind::Any), "/tmp/a/x"),
            entry
        );
    }

    #[test]
    fn events_in_the_resolved_spelling_come_back_in_the_given_one() {
        assert_eq!(
            changes(EventKind::Create(CreateKind::File), "/private/tmp/a/x"),
            vec![Change::Listing(PathBuf::from("/tmp/a"))]
        );
        assert_eq!(
            changes(EventKind::Modify(ModifyKind::Any), "/private/tmp/a/x"),
            vec![Change::Entry(PathBuf::from("/tmp/a/x"))]
        );
    }

    #[test]
    fn the_folder_itself_changing_re_reads_it() {
        assert_eq!(
            changes(EventKind::Remove(RemoveKind::Folder), "/tmp/a"),
            vec![Change::Listing(PathBuf::from("/tmp/a"))]
        );
    }

    #[test]
    fn reading_a_file_and_changes_deeper_down_are_ignored() {
        assert!(changes(EventKind::Access(AccessKind::Any), "/tmp/a/x").is_empty());
        assert!(changes(EventKind::Create(CreateKind::File), "/tmp/a/sub/x").is_empty());
        assert!(changes(EventKind::Create(CreateKind::File), "/elsewhere/x").is_empty());
    }

    #[test]
    fn a_rescan_re_reads_every_watched_folder_once() {
        let mut out = HashSet::new();
        let event = Event::new(EventKind::Other).set_flag(notify::event::Flag::Rescan);
        classify(&event, &spellings(), &mut out);
        assert_eq!(
            out.into_iter().collect::<Vec<_>>(),
            vec![Change::Listing(PathBuf::from("/tmp/a"))]
        );
    }
}
