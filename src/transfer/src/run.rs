//! Doing it.
//!
//! One item at a time, in plan order, and every failure recorded rather than
//! thrown: a transfer of forty files that loses the connection on the ninth
//! has thirty-one results the user needs to see, and returning early throws
//! them away.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use jtf_core::{Error, ErrorCode};
use jtf_fs::sftp::{Connection, Endpoint, SftpProvider};
use jtf_jobs::CancellationToken;

use crate::plan::{Item, Kind, Plan, Side};

/// The suffix a transfer in progress carries.
///
/// A dropped connection is the ordinary failure here, not a rare one, and a
/// half-written 4 GB image under its final name is indistinguishable from a
/// whole one. Bytes land under this and are renamed into place only once all
/// of them have arrived.
const PARTIAL: &str = ".jtf-part";

/// What happened to one item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// Finished, and where it ended up.
    Done {
        /// Where it landed, for a copy or a move.
        destination: Option<String>,
    },
    /// Left alone because something was already there.
    Skipped,
    /// The bytes arrived but the source could not be removed.
    ///
    /// Only a move produces this, and it is not a failure of the copy: the
    /// data is at the destination and also still at the source. Its own
    /// outcome because "it worked" and "it failed" are both wrong.
    CopiedButSourceRemains(Error),
    /// Failed, with the reason.
    Failed(Error),
}

/// Everything that happened.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    /// One entry per item acted on, in the order they were acted on.
    pub outcomes: Vec<(String, Outcome)>,
    /// Whether it stopped early because it was cancelled.
    pub cancelled: bool,
}

impl Report {
    /// How many finished cleanly.
    pub fn succeeded(&self) -> usize {
        self.outcomes
            .iter()
            .filter(|(_, o)| matches!(o, Outcome::Done { .. }))
            .count()
    }

    /// How many were left alone.
    pub fn skipped(&self) -> usize {
        self.outcomes
            .iter()
            .filter(|(_, o)| matches!(o, Outcome::Skipped))
            .count()
    }

    /// How many failed, counting a move whose source survived.
    pub fn failed(&self) -> usize {
        self.outcomes
            .iter()
            .filter(|(_, o)| {
                matches!(o, Outcome::Failed(_) | Outcome::CopiedButSourceRemains(_))
            })
            .count()
    }
}

/// What to do when something is already at the destination.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Policy {
    /// Leave it and skip the source. The only choice that destroys nothing.
    #[default]
    Skip,
    /// Replace it.
    Overwrite,
    /// Write beside it under a generated name.
    KeepBoth,
    /// Stop the whole transfer.
    Abort,
}

/// Told as it goes.
pub trait Watcher {
    /// Bytes moved so far, and the total as currently known.
    ///
    /// The total grows: a folder contributes nothing until it is walked, and
    /// pretending otherwise would need the tree measured first, which is the
    /// round trips this crate exists to avoid.
    fn progress(&mut self, done: u64, total: u64, current: &str);
}

/// A watcher that does nothing, for tests and for callers that do not care.
pub struct Silent;

impl Watcher for Silent {
    fn progress(&mut self, _done: u64, _total: u64, _current: &str) {}
}

/// Run a transfer.
///
/// Never returns `Err` for something one item did; only for a failure that
/// makes the whole thing meaningless, such as not being able to reach the
/// server at all.
///
/// # Errors
///
/// [`ErrorCode::ProviderFailed`] when a needed connection cannot be opened.
pub fn run(
    plan: &Plan,
    sftp: &SftpProvider,
    policy: Policy,
    watcher: &mut dyn Watcher,
    cancel: &CancellationToken,
) -> Result<Report, Error> {
    let mut state = State {
        sftp,
        policy,
        done: 0,
        total: plan.known_bytes,
        outcomes: Vec::new(),
    };

    for item in &plan.items {
        if cancel.is_cancelled() {
            return Ok(Report {
                outcomes: state.outcomes,
                cancelled: true,
            });
        }
        let name = item.source.display();
        watcher.progress(state.done, state.total, &name);

        let outcome = state.one(item, plan.kind, watcher, cancel);
        if matches!(outcome, Outcome::Failed(_)) && policy == Policy::Abort {
            state.outcomes.push((name, outcome));
            return Ok(Report {
                outcomes: state.outcomes,
                cancelled: false,
            });
        }
        state.outcomes.push((name, outcome));
    }

    watcher.progress(state.done, state.total, "");
    Ok(Report {
        outcomes: state.outcomes,
        cancelled: cancel.is_cancelled(),
    })
}

struct State<'a> {
    sftp: &'a SftpProvider,
    policy: Policy,
    done: u64,
    total: u64,
    outcomes: Vec<(String, Outcome)>,
}

impl State<'_> {
    fn connection(&self, endpoint: &Endpoint) -> Result<Arc<Connection>, Error> {
        self.sftp.connection_for(endpoint)
    }

    /// One top-level item, whatever shape it is.
    fn one(
        &mut self,
        item: &Item,
        kind: Kind,
        watcher: &mut dyn Watcher,
        cancel: &CancellationToken,
    ) -> Outcome {
        if kind == Kind::Delete {
            return match self.remove(&item.source, item.is_directory) {
                Ok(()) => Outcome::Done { destination: None },
                Err(e) => Outcome::Failed(e),
            };
        }

        let Some(destination) = &item.destination else {
            return Outcome::Failed(Error::new(ErrorCode::WrongKind, "no destination"));
        };

        // A rename within one server moves no bytes and is atomic. Taken
        // whenever it applies, because doing it as copy-then-delete instead
        // would pull a 4 GB file down and push it back up to end where it
        // could have arrived in one message.
        if kind == Kind::Move {
            if let (
                Side::Remote {
                    endpoint: from,
                    path: from_path,
                },
                Side::Remote {
                    endpoint: to,
                    path: to_path,
                },
            ) = (&item.source, destination)
            {
                if from == to {
                    return match self.rename_remote(from, from_path, to_path) {
                        Ok(Some(landed)) => Outcome::Done {
                            destination: Some(landed),
                        },
                        Ok(None) => Outcome::Skipped,
                        Err(e) => Outcome::Failed(e),
                    };
                }
            }
        }

        // A link is not its target. Following one would copy data the user
        // did not select, and a server's link may point at something that
        // does not exist on this machine at all.
        if item.is_symlink {
            return Outcome::Failed(Error::new(
                ErrorCode::Unsupported,
                format!("{}: a symbolic link is not copied", item.source.display()),
            ));
        }

        let copied = if item.is_directory {
            self.directory(&item.source, destination, watcher, cancel)
        } else {
            self.file(&item.source, destination, item.bytes, watcher, cancel)
        };

        match copied {
            Ok(None) => Outcome::Skipped,
            Ok(Some(landed)) => {
                if kind == Kind::Move {
                    // Only now. Removing the source before the bytes are
                    // known to have arrived is how a move loses a file.
                    if let Err(e) = self.remove(&item.source, item.is_directory) {
                        return Outcome::CopiedButSourceRemains(e);
                    }
                }
                Outcome::Done {
                    destination: Some(landed),
                }
            }
            Err(e) => Outcome::Failed(e),
        }
    }

    /// One file across. `Ok(None)` means it was skipped.
    fn file(
        &mut self,
        source: &Side,
        destination: &Side,
        bytes: u64,
        watcher: &mut dyn Watcher,
        cancel: &CancellationToken,
    ) -> Result<Option<String>, Error> {
        let Some(target) = self.resolve(destination)? else {
            return Ok(None);
        };

        let started = self.done;
        let tick = |moved: u64, watcher: &mut dyn Watcher| {
            let now = started.saturating_add(moved);
            watcher.progress(now, self.total.max(now), "");
        };

        match (source, &target) {
            (Side::Remote { endpoint, path }, Side::Local(local)) => {
                let connection = self.connection(endpoint)?;
                let partial = partial_path(local);
                let outcome = connection.download(path, &partial, |moved| {
                    tick(moved, watcher);
                    !cancel.is_cancelled()
                });
                finish_local(outcome, &partial, local)?;
            }
            (Side::Local(local), Side::Remote { endpoint, path }) => {
                let connection = self.connection(endpoint)?;
                let partial = format!("{path}{PARTIAL}");
                let outcome = connection.upload(local, &partial, |moved| {
                    tick(moved, watcher);
                    !cancel.is_cancelled()
                });
                finish_remote(&connection, outcome, &partial, path)?;
            }
            (Side::Local(from), Side::Local(to)) => {
                // Both local: not this crate's job, but reachable when a
                // selection spans both machines. Done rather than refused.
                std::fs::copy(from, to)
                    .map_err(|e| Error::new(ErrorCode::Io, format!("{}: {e}", from.display())))?;
            }
            (Side::Remote { .. }, Side::Remote { .. }) => {
                return Err(Error::new(
                    ErrorCode::Unsupported,
                    "copying between two servers is not built yet",
                ));
            }
        }

        self.done = self.done.saturating_add(bytes);
        self.total = self.total.max(self.done);
        Ok(Some(target.display()))
    }

    /// A folder and everything under it.
    fn directory(
        &mut self,
        source: &Side,
        destination: &Side,
        watcher: &mut dyn Watcher,
        cancel: &CancellationToken,
    ) -> Result<Option<String>, Error> {
        let Some(target) = self.resolve(destination)? else {
            return Ok(None);
        };
        self.make_directory(&target)?;

        for child in self.children(source)? {
            if cancel.is_cancelled() {
                break;
            }
            let into = target.join(&child.name);
            // The total is learned here rather than measured up front. It
            // grows as folders are entered, which is honest about a number
            // that genuinely is not known at the start.
            self.total = self.total.saturating_add(child.bytes);
            let child_source = source.join(&child.name);
            if child.is_symlink {
                continue;
            }
            let result = if child.is_dir {
                self.directory(&child_source, &into, watcher, cancel)
            } else {
                self.file(&child_source, &into, child.bytes, watcher, cancel)
            };
            if let Err(e) = result {
                // Recorded against the child rather than failing the folder:
                // one unreadable file in a thousand should not throw away the
                // other nine hundred and ninety-nine.
                self.outcomes
                    .push((child_source.display(), Outcome::Failed(e)));
            }
        }
        Ok(Some(target.display()))
    }

    /// What is directly inside a folder.
    fn children(&self, side: &Side) -> Result<Vec<Child>, Error> {
        match side {
            Side::Local(path) => {
                let mut out = Vec::new();
                for entry in std::fs::read_dir(path)
                    .map_err(|e| Error::new(ErrorCode::Io, format!("{}: {e}", path.display())))?
                {
                    let entry = entry
                        .map_err(|e| Error::new(ErrorCode::Io, format!("{}: {e}", path.display())))?;
                    let meta = entry.metadata().or_else(|_| entry.path().symlink_metadata());
                    let (is_dir, bytes, is_symlink) = meta.map_or((false, 0, false), |m| {
                        (m.is_dir(), m.len(), m.file_type().is_symlink())
                    });
                    out.push(Child {
                        name: entry.file_name().to_string_lossy().into_owned(),
                        is_dir,
                        bytes,
                        is_symlink,
                    });
                }
                Ok(out)
            }
            Side::Remote { endpoint, path } => {
                let connection = self.connection(endpoint)?;
                Ok(connection
                    .read_dir(path)?
                    .into_iter()
                    .map(|entry| Child {
                        name: entry.name,
                        is_dir: entry.is_dir,
                        bytes: entry.size,
                        is_symlink: entry.is_symlink,
                    })
                    .collect())
            }
        }
    }

    /// Where a file should actually land, applying the conflict policy.
    ///
    /// `Ok(None)` means leave the source alone.
    fn resolve(&self, target: &Side) -> Result<Option<Side>, Error> {
        if !self.exists(target)? {
            return Ok(Some(target.clone()));
        }
        match self.policy {
            Policy::Skip | Policy::Abort => Ok(None),
            Policy::Overwrite => Ok(Some(target.clone())),
            Policy::KeepBoth => self.free_name(target).map(Some),
        }
    }

    fn exists(&self, side: &Side) -> Result<bool, Error> {
        match side {
            Side::Local(path) => Ok(path.symlink_metadata().is_ok()),
            Side::Remote { endpoint, path } => {
                let connection = self.connection(endpoint)?;
                // Asked of the parent's listing rather than with a stat per
                // candidate: one round trip answers for every name in the
                // folder, and "keep both" asks about several.
                let (parent, name) = split_remote(path);
                Ok(connection
                    .read_dir(&parent)
                    .is_ok_and(|entries| entries.iter().any(|e| e.name == name)))
            }
        }
    }

    /// A name beside `target` that nothing is using.
    fn free_name(&self, target: &Side) -> Result<Side, Error> {
        let Some(name) = target.name() else {
            return Err(Error::new(ErrorCode::InvalidPath, "no name"));
        };
        let (stem, extension) = split_extension(&name);
        let parent = parent_of(target);
        for n in 2..1000 {
            let candidate = parent.join(&match extension {
                Some(ext) => format!("{stem} {n}.{ext}"),
                None => format!("{stem} {n}"),
            });
            if !self.exists(&candidate)? {
                return Ok(candidate);
            }
        }
        Err(Error::new(
            ErrorCode::AlreadyExists,
            format!("{}: no free name beside it", target.display()),
        ))
    }

    fn make_directory(&self, side: &Side) -> Result<(), Error> {
        match side {
            Side::Local(path) => std::fs::create_dir_all(path)
                .map_err(|e| Error::new(ErrorCode::Io, format!("{}: {e}", path.display()))),
            Side::Remote { endpoint, path } => {
                let connection = self.connection(endpoint)?;
                // Already there is success: a copy into an existing tree is
                // the ordinary case, not a clash.
                match connection.create_dir(path) {
                    Ok(()) => Ok(()),
                    Err(e) if self.exists(side).unwrap_or(false) => {
                        let _ = e;
                        Ok(())
                    }
                    Err(e) => Err(e),
                }
            }
        }
    }

    /// Remove something, recursively for a folder.
    fn remove(&self, side: &Side, is_directory: bool) -> Result<(), Error> {
        match side {
            Side::Local(path) => {
                if is_directory {
                    std::fs::remove_dir_all(path)
                } else {
                    std::fs::remove_file(path)
                }
                .map_err(|e| Error::new(ErrorCode::Io, format!("{}: {e}", path.display())))
            }
            Side::Remote { endpoint, path } => {
                let connection = self.connection(endpoint)?;
                if !is_directory {
                    return connection.remove_file(path);
                }
                // Depth first: a directory cannot go until it is empty, and
                // the protocol has no recursive remove.
                for child in self.children(side)? {
                    let inside = side.join(&child.name);
                    self.remove(&inside, child.is_dir && !child.is_symlink)?;
                }
                connection.remove_dir(path)
            }
        }
    }

    fn rename_remote(
        &self,
        endpoint: &Endpoint,
        from: &str,
        to: &str,
    ) -> Result<Option<String>, Error> {
        let target = Side::Remote {
            endpoint: endpoint.clone(),
            path: to.to_string(),
        };
        let Some(resolved) = self.resolve(&target)? else {
            return Ok(None);
        };
        let Side::Remote { path: to, .. } = &resolved else {
            return Err(Error::new(ErrorCode::WrongKind, "not a remote target"));
        };
        let connection = self.connection(endpoint)?;
        connection.rename(from, to)?;
        Ok(Some(resolved.display()))
    }
}

struct Child {
    name: String,
    is_dir: bool,
    bytes: u64,
    is_symlink: bool,
}

/// The temporary a local download lands in.
fn partial_path(final_path: &Path) -> PathBuf {
    let mut name = final_path
        .file_name()
        .map_or_else(|| "download".to_string(), |n| n.to_string_lossy().into_owned());
    name.push_str(PARTIAL);
    final_path.with_file_name(name)
}

/// Put a finished download in its place, or clear up after one that failed.
fn finish_local(outcome: Result<u64, Error>, partial: &Path, final_path: &Path) -> Result<(), Error> {
    match outcome {
        Ok(_) => std::fs::rename(partial, final_path).map_err(|e| {
            let _ = std::fs::remove_file(partial);
            Error::new(ErrorCode::Io, format!("{}: {e}", final_path.display()))
        }),
        Err(e) => {
            // The half that arrived is removed. Left behind it is a file
            // that looks complete and is not.
            let _ = std::fs::remove_file(partial);
            Err(e)
        }
    }
}

/// The same on the far side.
fn finish_remote(
    connection: &Connection,
    outcome: Result<u64, Error>,
    partial: &str,
    final_path: &str,
) -> Result<(), Error> {
    match outcome {
        Ok(_) => connection.rename(partial, final_path).inspect_err(|_| {
            let _ = connection.remove_file(partial);
        }),
        Err(e) => {
            let _ = connection.remove_file(partial);
            Err(e)
        }
    }
}

/// A remote path split into the folder and the last component.
fn split_remote(path: &str) -> (String, String) {
    let trimmed = path.trim_end_matches('/');
    match trimmed.rfind('/') {
        Some(0) | None => ("/".to_string(), trimmed.trim_start_matches('/').to_string()),
        Some(at) => (trimmed[..at].to_string(), trimmed[at + 1..].to_string()),
    }
}

/// A name split into the part before the extension and the extension.
fn split_extension(name: &str) -> (&str, Option<&str>) {
    match name.rfind('.') {
        // A leading dot is the whole name, not an extension.
        Some(0) | None => (name, None),
        Some(at) => (&name[..at], Some(&name[at + 1..])),
    }
}

/// The folder a side is in.
fn parent_of(side: &Side) -> Side {
    match side {
        Side::Local(path) => Side::Local(
            path.parent()
                .map_or_else(|| PathBuf::from("."), Path::to_path_buf),
        ),
        Side::Remote { endpoint, path } => Side::Remote {
            endpoint: endpoint.clone(),
            path: split_remote(path).0,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_partial_download_is_named_so_it_cannot_be_mistaken_for_a_whole_one() {
        let partial = partial_path(Path::new("/tmp/big.iso"));
        assert_eq!(partial, PathBuf::from("/tmp/big.iso.jtf-part"));
        assert_ne!(partial, PathBuf::from("/tmp/big.iso"));
    }

    #[test]
    fn a_failed_download_leaves_nothing_behind() {
        let dir = std::env::temp_dir().join("jtf-transfer-partial");
        std::fs::create_dir_all(&dir).unwrap();
        let final_path = dir.join("x.bin");
        let partial = partial_path(&final_path);
        std::fs::write(&partial, b"half").unwrap();

        let err = finish_local(
            Err(Error::new(ErrorCode::Io, "connection lost")),
            &partial,
            &final_path,
        )
        .unwrap_err();
        assert_eq!(err.code(), ErrorCode::Io);
        assert!(!partial.exists(), "the half-written file was left");
        assert!(!final_path.exists(), "a half file was put in place");
    }

    #[test]
    fn a_finished_download_is_renamed_into_place() {
        let dir = std::env::temp_dir().join("jtf-transfer-finish");
        std::fs::create_dir_all(&dir).unwrap();
        let final_path = dir.join("y.bin");
        let _ = std::fs::remove_file(&final_path);
        let partial = partial_path(&final_path);
        std::fs::write(&partial, b"whole").unwrap();

        finish_local(Ok(5), &partial, &final_path).unwrap();
        assert!(!partial.exists());
        assert_eq!(std::fs::read(&final_path).unwrap(), b"whole");
        let _ = std::fs::remove_file(&final_path);
    }

    #[test]
    fn a_remote_path_splits_into_a_folder_and_a_name() {
        assert_eq!(
            split_remote("/srv/data/file.txt"),
            ("/srv/data".to_string(), "file.txt".to_string())
        );
        assert_eq!(
            split_remote("/file.txt"),
            ("/".to_string(), "file.txt".to_string())
        );
        assert_eq!(
            split_remote("/srv/data/"),
            ("/srv".to_string(), "data".to_string()),
            "a trailing slash changed which part was the name"
        );
    }

    #[test]
    fn a_generated_name_keeps_the_extension_where_it_belongs() {
        assert_eq!(split_extension("report.txt"), ("report", Some("txt")));
        assert_eq!(split_extension("report"), ("report", None));
        assert_eq!(
            split_extension(".hidden"),
            (".hidden", None),
            "a leading dot is the name, not an extension"
        );
        assert_eq!(
            split_extension("archive.tar.gz"),
            ("archive.tar", Some("gz"))
        );
    }

    #[test]
    fn the_report_counts_a_move_whose_source_survived_as_a_failure() {
        let report = Report {
            outcomes: vec![
                ("a".into(), Outcome::Done { destination: None }),
                ("b".into(), Outcome::Skipped),
                (
                    "c".into(),
                    Outcome::CopiedButSourceRemains(Error::new(ErrorCode::Io, "no")),
                ),
                ("d".into(), Outcome::Failed(Error::new(ErrorCode::Io, "no"))),
            ],
            cancelled: false,
        };
        assert_eq!(report.succeeded(), 1);
        assert_eq!(report.skipped(), 1);
        assert_eq!(
            report.failed(),
            2,
            "a move that left the source behind is not a success"
        );
    }
}
