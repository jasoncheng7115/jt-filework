//! Walking a tree to answer a query.
//!
//! Iterative, cancellable, depth-bounded, and incremental: results arrive
//! while the walk continues, so a search over a large tree is usable before it
//! finishes (`docs/SEARCH_AI.md` §2.3).

use std::path::PathBuf;
use std::sync::mpsc;
use std::thread;
use std::time::SystemTime;

use jtf_core::{Error, ErrorCode, FileEntry, Location};
use jtf_fs::{LocalProvider, Provider};
use jtf_jobs::{CancellationToken, Canceller};

use crate::query::Query;

/// How deep a search descends.
///
/// A bound rather than a hope: the tree is attacker-influenced — a symlink
/// loop, a pathological directory — and `AGENTS.md` §20.2 does not accept
/// "a directory tree is probably shallow". The walk also never follows a
/// symlink, so a cycle cannot be entered in the first place; this is the
/// second layer.
pub const MAX_DEPTH: usize = 64;

/// How many matches accumulate before they are sent.
const BATCH: usize = 64;

/// What a running search reports.
#[derive(Debug, Clone, PartialEq)]
pub enum SearchUpdate {
    /// More matches.
    Matches(Vec<FileEntry>),
    /// Progress: directories visited and entries examined so far.
    Progress {
        /// Directories opened.
        directories: u64,
        /// Entries looked at.
        examined: u64,
    },
    /// The walk finished.
    Done {
        /// Total matches.
        matches: usize,
        /// Whether the depth limit stopped it going deeper anywhere.
        depth_limited: bool,
    },
    /// The walk could not start.
    Failed(Error),
}

/// A running search.
///
/// Dropping it cancels the walk, for the same reason an abandoned enumeration
/// is cancelled: nobody is waiting for it, and it is still reading a disk.
#[derive(Debug)]
pub struct SearchHandle {
    canceller: Canceller,
    receiver: mpsc::Receiver<SearchUpdate>,
    join: Option<thread::JoinHandle<()>>,
}

impl SearchHandle {
    /// Take whatever has arrived, without blocking.
    pub fn poll(&self) -> Vec<SearchUpdate> {
        self.receiver.try_iter().collect()
    }

    /// Block until the next update. For tests and headless callers.
    pub fn recv(&self) -> Option<SearchUpdate> {
        self.receiver.recv().ok()
    }

    /// Ask the walk to stop.
    pub fn cancel(&self) {
        self.canceller.cancel();
    }

    /// Whether cancellation has been requested.
    pub fn is_cancelled(&self) -> bool {
        self.canceller.is_cancelled()
    }
}

impl Drop for SearchHandle {
    fn drop(&mut self) {
        self.canceller.cancel();
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

/// Start a search under `root`.
///
/// # Errors
///
/// Only for failures knowable before any work starts.
pub fn search(root: &Location, query: Query) -> Result<SearchHandle, Error> {
    let root = root
        .as_path()
        .ok_or_else(|| Error::new(ErrorCode::Unsupported, "not a local location"))?
        .to_path_buf();

    let (token, canceller) = CancellationToken::new();
    let (sender, receiver) = mpsc::channel();

    let join = thread::Builder::new()
        .name("jtf-search".to_string())
        .spawn(move || {
            let provider = LocalProvider::new();
            let now = SystemTime::now();
            let mut stack: Vec<(PathBuf, usize)> = vec![(root, 0)];
            let mut buffer = Vec::with_capacity(BATCH);
            let mut matches = 0usize;
            let mut directories = 0u64;
            let mut examined = 0u64;
            let mut depth_limited = false;

            while let Some((directory, depth)) = stack.pop() {
                if token.is_cancelled() {
                    // Silent, like a cancelled enumeration: nobody is waiting
                    // for these results any more (AGENTS.md 3).
                    return;
                }
                let Ok(entries) = provider.list(&Location::local(&directory), &token) else {
                    continue; // an unreadable subtree is skipped, not fatal
                };
                directories += 1;

                for entry in entries {
                    if token.is_cancelled() {
                        return;
                    }
                    examined += 1;

                    if query.matches(&entry, now) {
                        matches += 1;
                        buffer.push(entry.clone());
                        if buffer.len() >= BATCH
                            && sender
                                .send(SearchUpdate::Matches(std::mem::replace(
                                    &mut buffer,
                                    Vec::with_capacity(BATCH),
                                )))
                                .is_err()
                        {
                            return;
                        }
                    }

                    // Descend into real directories only. A symlink is never
                    // followed, so a cycle cannot be entered
                    // (docs/SECURITY.md 3.1).
                    if entry.kind().is_directory_on_disk() {
                        if depth + 1 > MAX_DEPTH {
                            depth_limited = true;
                        } else if let Some(path) = entry.location().as_path() {
                            stack.push((path.to_path_buf(), depth + 1));
                        }
                    }
                }

                if sender
                    .send(SearchUpdate::Progress {
                        directories,
                        examined,
                    })
                    .is_err()
                {
                    return;
                }
            }

            if !buffer.is_empty() && sender.send(SearchUpdate::Matches(buffer)).is_err() {
                return;
            }
            let _ = sender.send(SearchUpdate::Done {
                matches,
                depth_limited,
            });
        })
        .map_err(|e| Error::new(ErrorCode::Internal, format!("spawn search: {e}")))?;

    Ok(SearchHandle {
        canceller,
        receiver,
        join: Some(join),
    })
}
