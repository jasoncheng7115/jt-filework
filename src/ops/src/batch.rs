//! Batch rename.
//!
//! Two things make batch rename either useful or dangerous, and both are here
//! rather than in the UI:
//!
//! 1. **A preview that is computed the same way the apply is.** A preview that
//!    is a separate implementation is a preview of something else.
//! 2. **Collision handling that survives a swap.** Renaming `a -> b` while
//!    `b -> a` cannot be done in one pass in any order; doing it naively
//!    destroys one of the two files.

use std::collections::HashMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use jtf_core::{Error, ErrorCode};
use regex::Regex;

/// How each new name is built.
///
/// The pattern language is small on purpose. Every placeholder is one thing
/// people actually ask for, and anything not recognised is left alone rather
/// than silently dropped, so a stray brace in a filename survives.
///
/// | Placeholder | Meaning |
/// |---|---|
/// | `{name}` | the original name without its extension |
/// | `{ext}` | the extension, without the dot |
/// | `{n}` | a counter |
/// | `{n:3}` | a counter padded to three digits |
/// | `{upper}` / `{lower}` | the original name, cased |
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenamePattern {
    /// The template for the new name.
    pub template: String,
    /// Text or expression to replace within the original name. Empty to skip.
    pub find: String,
    /// What to replace it with.
    pub replace: String,
    /// Whether `find` is a regular expression.
    pub regex: bool,
    /// Where `{n}` starts.
    pub start: u64,
}

impl Default for RenamePattern {
    fn default() -> Self {
        Self {
            template: "{name}.{ext}".to_string(),
            find: String::new(),
            replace: String::new(),
            regex: false,
            start: 1,
        }
    }
}

/// What is wrong with one proposed name, if anything.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenameIssue {
    /// Fine.
    Ok,
    /// The name did not change; the rename will be skipped.
    Unchanged,
    /// Empty, or contains a path separator.
    Invalid,
    /// Two entries in this batch would end up with the same name.
    Duplicate,
    /// Something not in this batch already has that name.
    Exists,
}

impl RenameIssue {
    /// Localization key for the label.
    pub const fn label_key(self) -> &'static str {
        match self {
            Self::Ok => "rename.ok",
            Self::Unchanged => "rename.unchanged",
            Self::Invalid => "rename.invalid",
            Self::Duplicate => "rename.duplicate",
            Self::Exists => "rename.exists",
        }
    }

    /// Whether this row would actually be renamed.
    pub const fn will_apply(self) -> bool {
        matches!(self, Self::Ok)
    }

    /// Whether this row stops the whole batch.
    ///
    /// A batch with a collision in it is not applied at all: applying the
    /// half that works leaves the user with a directory in a state they did
    /// not ask for and cannot easily reverse.
    pub const fn blocks(self) -> bool {
        matches!(self, Self::Invalid | Self::Duplicate | Self::Exists)
    }
}

/// One proposed rename.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenameRow {
    /// The entry.
    pub source: PathBuf,
    /// Its current name.
    pub from: String,
    /// The name it would get.
    pub to: String,
    /// What is wrong with it, if anything.
    pub issue: RenameIssue,
}

/// The whole proposal.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RenamePreview {
    /// One row per source, in order.
    pub rows: Vec<RenameRow>,
}

impl RenamePreview {
    /// Whether anything would change.
    pub fn has_changes(&self) -> bool {
        self.rows.iter().any(|row| row.issue.will_apply())
    }

    /// Whether something stops the batch from being applied.
    pub fn is_blocked(&self) -> bool {
        self.rows.iter().any(|row| row.issue.blocks())
    }

    /// How many rows would be renamed.
    pub fn change_count(&self) -> usize {
        self.rows
            .iter()
            .filter(|row| row.issue.will_apply())
            .count()
    }
}

/// Work out what a pattern would do, without touching anything.
///
/// The apply path uses this same function, so the preview cannot disagree
/// with the result.
pub fn preview(sources: &[PathBuf], pattern: &RenamePattern) -> RenamePreview {
    let expression = if pattern.regex && !pattern.find.is_empty() {
        Regex::new(&pattern.find).ok()
    } else {
        None
    };
    if pattern.regex && !pattern.find.is_empty() && expression.is_none() {
        // An invalid expression makes every row invalid rather than silently
        // matching nothing, which would look like the pattern did work.
        return RenamePreview {
            rows: sources
                .iter()
                .map(|source| RenameRow {
                    source: source.clone(),
                    from: file_name(source),
                    to: String::new(),
                    issue: RenameIssue::Invalid,
                })
                .collect(),
        };
    }

    let mut rows = Vec::with_capacity(sources.len());
    let mut counts: HashMap<String, usize> = HashMap::new();

    for (index, source) in sources.iter().enumerate() {
        let from = file_name(source);
        let stem = stem_of(source);
        let extension = extension_of(source);

        let stem = apply_find_replace(&stem, pattern, expression.as_ref());
        let to = expand(
            &pattern.template,
            &stem,
            &extension,
            pattern.start + index as u64,
        );

        let issue =
            if to.is_empty() || to.contains('/') || to.contains('\\') || to == "." || to == ".." {
                RenameIssue::Invalid
            } else if to == from {
                RenameIssue::Unchanged
            } else {
                RenameIssue::Ok
            };

        *counts.entry(to.to_lowercase()).or_default() += 1;
        rows.push(RenameRow {
            source: source.clone(),
            from,
            to,
            issue,
        });
    }

    // Two passes: duplicates inside the batch, then collisions with entries
    // that are not part of it.
    let batch: std::collections::HashSet<PathBuf> = sources.iter().cloned().collect();
    for row in &mut rows {
        if row.issue == RenameIssue::Invalid {
            continue;
        }
        if counts.get(&row.to.to_lowercase()).copied().unwrap_or(0) > 1 {
            row.issue = RenameIssue::Duplicate;
            continue;
        }
        if row.issue == RenameIssue::Unchanged {
            continue;
        }
        if let Some(parent) = row.source.parent() {
            let target = parent.join(&row.to);
            // A name currently held by another entry in the same batch is
            // fine: that entry is being renamed too, and the two-phase apply
            // handles the swap.
            if target.exists() && !batch.contains(&target) {
                row.issue = RenameIssue::Exists;
            }
        }
    }
    RenamePreview { rows }
}

/// Apply a preview.
///
/// Two phases when needed: every entry moves to a temporary name first, then
/// to its final one. Without that, renaming `a -> b` while `b -> a` destroys
/// one of them whichever order is chosen.
///
/// # Errors
///
/// [`ErrorCode::WrongKind`] if the preview is blocked. A blocked batch is
/// never partially applied.
pub fn apply(preview: &RenamePreview) -> Result<Vec<(PathBuf, PathBuf)>, Error> {
    if preview.is_blocked() {
        return Err(Error::new(
            ErrorCode::WrongKind,
            "the batch has collisions and was not applied",
        ));
    }
    let planned: Vec<&RenameRow> = preview
        .rows
        .iter()
        .filter(|row| row.issue.will_apply())
        .collect();
    if planned.is_empty() {
        return Ok(Vec::new());
    }

    let mut done = Vec::with_capacity(planned.len());
    let mut temporaries = Vec::with_capacity(planned.len());

    // Phase one: out of the way.
    for (index, row) in planned.iter().enumerate() {
        let Some(parent) = row.source.parent() else {
            continue;
        };
        let mut temporary = parent.join(format!(".jtf-rename-{}-{index}", std::process::id()));
        let mut attempt = 0;
        while temporary.exists() && attempt < 1000 {
            attempt += 1;
            temporary = parent.join(format!(
                ".jtf-rename-{}-{index}-{attempt}",
                std::process::id()
            ));
        }
        if let Err(error) = std::fs::rename(&row.source, &temporary) {
            // Roll back what has already moved, so a failure leaves the
            // directory as it was rather than half renamed.
            rollback(&temporaries);
            return Err(Error::new(
                ErrorCode::Io,
                format!("{}: {error}", row.source.display()),
            ));
        }
        temporaries.push((temporary, row.source.clone()));
    }

    // Phase two: into place.
    for ((temporary, original), row) in temporaries.iter().zip(planned.iter()) {
        let Some(parent) = original.parent() else {
            continue;
        };
        let target = parent.join(&row.to);
        if let Err(error) = std::fs::rename(temporary, &target) {
            rollback(&temporaries);
            return Err(Error::new(
                ErrorCode::Io,
                format!("{}: {error}", target.display()),
            ));
        }
        done.push((original.clone(), target));
    }
    Ok(done)
}

/// Put everything back where it came from.
fn rollback(moved: &[(PathBuf, PathBuf)]) {
    for (temporary, original) in moved {
        if temporary.exists() {
            let _ = std::fs::rename(temporary, original);
        }
    }
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map_or_else(String::new, |name| name.to_string_lossy().into_owned())
}

fn stem_of(path: &Path) -> String {
    path.file_stem()
        .map_or_else(String::new, |name| name.to_string_lossy().into_owned())
}

fn extension_of(path: &Path) -> String {
    path.extension()
        .map_or_else(String::new, |name| name.to_string_lossy().into_owned())
}

fn apply_find_replace(stem: &str, pattern: &RenamePattern, expression: Option<&Regex>) -> String {
    if pattern.find.is_empty() {
        return stem.to_string();
    }
    match expression {
        Some(regex) => regex
            .replace_all(stem, pattern.replace.as_str())
            .into_owned(),
        None => stem.replace(&pattern.find, &pattern.replace),
    }
}

/// Expand placeholders.
///
/// An unrecognised `{...}` is left exactly as written: a filename containing a
/// brace is ordinary, and eating it would be a surprise.
fn expand(template: &str, stem: &str, extension: &str, counter: u64) -> String {
    let mut out = String::with_capacity(template.len() + stem.len());
    let mut rest = template;

    while let Some(start) = rest.find('{') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        let Some(end) = after.find('}') else {
            out.push('{');
            rest = after;
            continue;
        };
        let token = &after[..end];
        rest = &after[end + 1..];

        match token {
            "name" => out.push_str(stem),
            "ext" => out.push_str(extension),
            "upper" => out.push_str(&stem.to_uppercase()),
            "lower" => out.push_str(&stem.to_lowercase()),
            "n" => {
                let _ = write!(out, "{counter}");
            }
            _ => {
                if let Some(width) = token
                    .strip_prefix("n:")
                    .and_then(|w| w.parse::<usize>().ok())
                {
                    let _ = write!(out, "{counter:0width$}");
                } else {
                    out.push('{');
                    out.push_str(token);
                    out.push('}');
                }
            }
        }
    }
    out.push_str(rest);
    // A template ending in "." because the file had no extension would leave a
    // trailing dot, which is a legal but never-intended name.
    out.trim_end_matches('.').to_string()
}
