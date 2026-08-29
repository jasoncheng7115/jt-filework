//! Pre-flight: work out what an operation will do before it does any of it.
//!
//! Planning first buys three things the user can see: an honest total for the
//! progress bar, every conflict in one question instead of a hundred, and a
//! refusal *before* anything moves when the operation is impossible — copying
//! a directory into itself, for instance.

use std::fs;
use std::path::{Path, PathBuf};

use jtf_core::{Error, ErrorCode};
use jtf_jobs::{CancellationToken, JobKind};

use crate::conflict::Conflict;

/// What the user asked for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Operation {
    /// Copy sources into a destination directory.
    Copy {
        /// What to copy.
        sources: Vec<PathBuf>,
        /// Directory to copy into.
        destination: PathBuf,
    },
    /// Move sources into a destination directory.
    Move {
        /// What to move.
        sources: Vec<PathBuf>,
        /// Directory to move into.
        destination: PathBuf,
    },
    /// Rename one entry in place.
    Rename {
        /// The entry.
        source: PathBuf,
        /// The new name, not a path.
        new_name: String,
    },
    /// Change the read-only flag on entries.
    ///
    /// Only read-only, and only that. The cross-platform permission summary
    /// has three bits, but "readable" and "executable" do not mean the same
    /// thing on Windows as on Unix, and offering a control that means
    /// something different per platform is worse than offering one that means
    /// the same thing everywhere.
    SetReadOnly {
        /// What to change.
        sources: Vec<PathBuf>,
        /// Whether they become read-only.
        read_only: bool,
    },
    /// Create an empty file.
    ///
    /// Separate from `NewFolder` rather than a flag on it, so the run step
    /// cannot create the wrong kind by getting a boolean backwards.
    NewFile {
        /// Where.
        parent: PathBuf,
        /// Its name.
        name: String,
    },
    /// Create a directory.
    NewFolder {
        /// Where.
        parent: PathBuf,
        /// Its name.
        name: String,
    },
    /// Move entries to the trash.
    Trash {
        /// What to trash.
        sources: Vec<PathBuf>,
    },
    /// Delete entries permanently.
    Delete {
        /// What to delete.
        sources: Vec<PathBuf>,
    },
}

impl Operation {
    /// Which job kind this is, for the UI's label and its undo claim.
    pub const fn job_kind(&self) -> JobKind {
        match self {
            Self::Copy { .. } => JobKind::Copy,
            Self::Move { .. } => JobKind::Move,
            Self::Rename { .. }
            | Self::NewFolder { .. }
            | Self::NewFile { .. }
            | Self::SetReadOnly { .. } => JobKind::Rename,
            Self::Trash { .. } => JobKind::Trash,
            Self::Delete { .. } => JobKind::Delete,
        }
    }

    /// Whether this operation destroys data that cannot be recovered.
    pub const fn is_irreversible(&self) -> bool {
        matches!(self, Self::Delete { .. })
    }
}

/// Why an operation cannot be attempted at all.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum PlanError {
    /// The destination is inside one of the sources.
    DestinationInsideSource(PathBuf),
    /// A source and the destination are the same path.
    SourceIsDestination(PathBuf),
    /// The destination is not a directory.
    DestinationNotADirectory(PathBuf),
    /// A name contained a path separator, a `..`, or was empty.
    InvalidName(String),
    /// Nothing was selected.
    NothingToDo,
    /// The filesystem refused during the scan.
    Failed(Error),
}

impl core::fmt::Display for PlanError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::DestinationInsideSource(p) => {
                write!(f, "destination is inside the source: {}", p.display())
            }
            Self::SourceIsDestination(p) => write!(f, "source is the destination: {}", p.display()),
            Self::DestinationNotADirectory(p) => {
                write!(f, "destination is not a directory: {}", p.display())
            }
            Self::InvalidName(n) => write!(f, "invalid name: {n}"),
            Self::NothingToDo => f.write_str("nothing selected"),
            Self::Failed(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for PlanError {}

impl From<PlanError> for Error {
    fn from(value: PlanError) -> Self {
        let code = match value {
            PlanError::Failed(ref e) => e.code(),
            PlanError::InvalidName(_)
            | PlanError::DestinationInsideSource(_)
            | PlanError::SourceIsDestination(_) => ErrorCode::InvalidPath,
            PlanError::DestinationNotADirectory(_) | PlanError::NothingToDo => ErrorCode::WrongKind,
        };
        Self::new(code, value.to_string())
    }
}

/// One source and where it is going.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Step {
    /// What to act on.
    pub source: PathBuf,
    /// Where it lands, for copy and move.
    pub destination: Option<PathBuf>,
    /// Bytes this step will move, as far as the scan could tell.
    pub bytes: u64,
    /// Whether the source is a directory.
    pub is_directory: bool,
}

/// A checked, measured operation, ready to run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    /// What the user asked for.
    pub operation: Operation,
    /// Every source, in order.
    pub steps: Vec<Step>,
    /// Destinations that already exist.
    pub conflicts: Vec<Conflict>,
    /// Total bytes, for the progress bar.
    pub total_bytes: u64,
    /// Total entries, including directory contents.
    pub total_entries: u64,
}

impl Plan {
    /// Check and measure an operation.
    ///
    /// # Errors
    ///
    /// [`PlanError`] when the operation cannot be attempted.
    pub fn build(operation: &Operation, cancel: &CancellationToken) -> Result<Self, PlanError> {
        match operation {
            Operation::Copy {
                sources,
                destination,
            }
            | Operation::Move {
                sources,
                destination,
            } => Self::build_transfer(operation, sources, destination, cancel),
            Operation::Rename { source, new_name } => {
                Self::build_rename(operation, source, new_name)
            }
            Operation::NewFolder { parent, name } | Operation::NewFile { parent, name } => {
                Self::build_new_folder(operation, parent, name)
            }
            Operation::Trash { sources } | Operation::Delete { sources } => {
                Self::build_removal(operation, sources, cancel)
            }
            Operation::SetReadOnly { sources, .. } => {
                // Measured like a removal: the entries are the work, and no
                // bytes move.
                Self::build_removal(operation, sources, cancel)
            }
        }
    }

    fn build_transfer(
        operation: &Operation,
        sources: &[PathBuf],
        destination: &Path,
        cancel: &CancellationToken,
    ) -> Result<Self, PlanError> {
        if sources.is_empty() {
            return Err(PlanError::NothingToDo);
        }
        if !destination.is_dir() {
            return Err(PlanError::DestinationNotADirectory(
                destination.to_path_buf(),
            ));
        }

        let destination_real = canonical(destination);
        let mut steps = Vec::with_capacity(sources.len());
        let mut conflicts = Vec::new();
        let mut total_bytes = 0u64;
        let mut total_entries = 0u64;

        for source in sources {
            let source_real = canonical(source);
            if source_real == destination_real {
                return Err(PlanError::SourceIsDestination(source.clone()));
            }
            // Copying a directory into itself would recurse until the disk
            // filled. Refused here, before a single byte is written.
            if destination_real.starts_with(&source_real) {
                return Err(PlanError::DestinationInsideSource(
                    destination.to_path_buf(),
                ));
            }

            let Some(name) = source.file_name() else {
                return Err(PlanError::InvalidName(source.display().to_string()));
            };
            let target = destination.join(name);

            let is_directory = source.is_dir() && !source.is_symlink();
            let (bytes, entries) = measure_or_zero(source, cancel)?;
            total_bytes += bytes;
            total_entries += entries;

            if target.exists() {
                conflicts.push(Conflict {
                    source: source.clone(),
                    destination: target.clone(),
                    destination_is_directory: target.is_dir(),
                });
            }
            steps.push(Step {
                source: source.clone(),
                destination: Some(target),
                bytes,
                is_directory,
            });
        }

        Ok(Self {
            operation: operation.clone(),
            steps,
            conflicts,
            total_bytes,
            total_entries,
        })
    }

    fn build_rename(
        operation: &Operation,
        source: &Path,
        new_name: &str,
    ) -> Result<Self, PlanError> {
        validate_name(new_name)?;
        let Some(parent) = source.parent() else {
            return Err(PlanError::InvalidName(source.display().to_string()));
        };
        let target = parent.join(new_name);

        let mut conflicts = Vec::new();
        // A case-only rename on a case-insensitive filesystem targets the same
        // file; treating that as a conflict would make renaming "readme" to
        // "README" impossible (docs/TESTING.md 5.2).
        let same_entry = canonical(&target) == canonical(source);
        if target.exists() && !same_entry {
            conflicts.push(Conflict {
                source: source.to_path_buf(),
                destination: target.clone(),
                destination_is_directory: target.is_dir(),
            });
        }

        Ok(Self {
            operation: operation.clone(),
            steps: vec![Step {
                source: source.to_path_buf(),
                destination: Some(target),
                bytes: 0,
                is_directory: source.is_dir(),
            }],
            conflicts,
            total_bytes: 0,
            total_entries: 1,
        })
    }

    fn build_new_folder(
        operation: &Operation,
        parent: &Path,
        name: &str,
    ) -> Result<Self, PlanError> {
        validate_name(name)?;
        if !parent.is_dir() {
            return Err(PlanError::DestinationNotADirectory(parent.to_path_buf()));
        }
        let target = parent.join(name);
        let mut conflicts = Vec::new();
        if target.exists() {
            conflicts.push(Conflict {
                source: target.clone(),
                destination: target.clone(),
                destination_is_directory: target.is_dir(),
            });
        }
        Ok(Self {
            operation: operation.clone(),
            steps: vec![Step {
                source: target.clone(),
                destination: Some(target),
                bytes: 0,
                is_directory: true,
            }],
            conflicts,
            total_bytes: 0,
            total_entries: 1,
        })
    }

    fn build_removal(
        operation: &Operation,
        sources: &[PathBuf],
        cancel: &CancellationToken,
    ) -> Result<Self, PlanError> {
        if sources.is_empty() {
            return Err(PlanError::NothingToDo);
        }
        let mut steps = Vec::with_capacity(sources.len());
        let mut total_bytes = 0;
        let mut total_entries = 0;
        for source in sources {
            let (bytes, entries) = measure_or_zero(source, cancel)?;
            total_bytes += bytes;
            total_entries += entries;
            steps.push(Step {
                source: source.clone(),
                destination: None,
                bytes,
                is_directory: source.is_dir() && !source.is_symlink(),
            });
        }
        Ok(Self {
            operation: operation.clone(),
            steps,
            conflicts: Vec::new(),
            total_bytes,
            total_entries,
        })
    }
}

/// A name the user typed, not a path.
fn validate_name(name: &str) -> Result<(), PlanError> {
    let trimmed = name.trim();
    if trimmed.is_empty()
        || trimmed == "."
        || trimmed == ".."
        || trimmed.contains('/')
        || trimmed.contains('\\')
        || trimmed.contains('\0')
    {
        return Err(PlanError::InvalidName(name.to_string()));
    }
    Ok(())
}

/// Resolve as far as the filesystem allows, falling back to the literal path.
///
/// Used for containment checks, where a symlinked destination must not be
/// able to smuggle the operation somewhere else.
fn canonical(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

/// Measure a source, tolerating one that has gone.
///
/// A source can disappear between the moment the user selected it and the
/// moment the plan is built — another process deleted it, a mount went away.
/// Refusing the whole operation for that would mean one vanished file costs
/// the user the other forty-nine. The step stays in the plan with a zero size
/// and fails on its own during execution, where the failure is attributed to
/// the entry that caused it.
///
/// Cancellation is the exception: it applies to everything, so it propagates.
fn measure_or_zero(path: &Path, cancel: &CancellationToken) -> Result<(u64, u64), PlanError> {
    match measure(path, cancel) {
        Ok(measured) => Ok(measured),
        Err(error) if error.code() == ErrorCode::Cancelled => Err(PlanError::Failed(error)),
        Err(_) => Ok((0, 1)),
    }
}

/// Total bytes and entries under `path`, without following symlinks.
fn measure(path: &Path, cancel: &CancellationToken) -> Result<(u64, u64), Error> {
    let meta = fs::symlink_metadata(path)
        .map_err(|e| Error::new(ErrorCode::Io, format!("{}: {e}", path.display())))?;

    if meta.file_type().is_symlink() || !meta.is_dir() {
        return Ok((meta.len(), 1));
    }

    // Iterative, not recursive: the depth here is attacker-influenced, and
    // AGENTS.md 20.2 does not accept "a directory tree is probably shallow".
    let mut bytes = 0u64;
    let mut entries = 1u64;
    let mut stack = vec![path.to_path_buf()];

    while let Some(dir) = stack.pop() {
        if cancel.is_cancelled() {
            return Err(Error::bare(ErrorCode::Cancelled));
        }
        let Ok(read_dir) = fs::read_dir(&dir) else {
            continue; // unreadable subtree: measured as zero, reported when we act
        };
        for entry in read_dir.flatten() {
            if cancel.is_cancelled() {
                return Err(Error::bare(ErrorCode::Cancelled));
            }
            entries += 1;
            let Ok(meta) = entry.metadata_no_follow() else {
                continue;
            };
            if meta.is_dir() {
                stack.push(entry.path());
            } else {
                bytes += meta.len();
            }
        }
    }
    Ok((bytes, entries))
}

/// `DirEntry::metadata` does not follow symlinks on any platform we target,
/// but the name does not say so; this makes the intent explicit.
trait NoFollow {
    fn metadata_no_follow(&self) -> std::io::Result<fs::Metadata>;
}

impl NoFollow for fs::DirEntry {
    fn metadata_no_follow(&self) -> std::io::Result<fs::Metadata> {
        self.metadata()
    }
}
