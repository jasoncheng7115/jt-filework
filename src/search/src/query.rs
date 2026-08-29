//! What a query is, once parsed.

use std::time::{Duration, SystemTime};

use jtf_core::{FileEntry, FileKind};
use regex::Regex;

/// How a numeric or date term compares.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Comparison {
    /// Strictly less than.
    Less,
    /// Strictly greater than.
    Greater,
    /// Equal.
    Equal,
}

impl Comparison {
    pub(crate) fn matches(self, left: u64, right: u64) -> bool {
        match self {
            Self::Less => left < right,
            Self::Greater => left > right,
            Self::Equal => left == right,
        }
    }
}

/// Size units accepted in a query.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SizeUnit {
    /// Bytes.
    B,
    /// Kibibytes.
    K,
    /// Mebibytes.
    M,
    /// Gibibytes.
    G,
}

impl SizeUnit {
    pub(crate) const fn multiplier(self) -> u64 {
        match self {
            Self::B => 1,
            Self::K => 1 << 10,
            Self::M => 1 << 20,
            Self::G => 1 << 30,
        }
    }
}

/// One condition.
#[derive(Debug, Clone)]
pub enum Term {
    /// Case-insensitive substring of the name. What bare words mean.
    NameContains(String),
    /// Shell-style wildcard over the name.
    NameGlob(String),
    /// Regular expression over the name.
    NameRegex(Box<Regex>),
    /// Case-insensitive substring of the full path.
    PathContains(String),
    /// Extension, without the dot, case-insensitive.
    Extension(String),
    /// Entry kind.
    Kind(FileKind),
    /// Size in bytes.
    Size(Comparison, u64),
    /// Modified within, or before, this many seconds ago.
    ModifiedAge(Comparison, Duration),
    /// Whether the entry is hidden.
    Hidden(bool),
    /// Negation of another term.
    Not(Box<Term>),
}

impl Term {
    /// Whether an entry satisfies this term.
    pub fn matches(&self, entry: &FileEntry, now: SystemTime) -> bool {
        match self {
            Self::NameContains(needle) => entry.display_name().to_lowercase().contains(needle),
            Self::NameGlob(pattern) => glob_matches(pattern, &entry.display_name().to_lowercase()),
            Self::NameRegex(regex) => regex.is_match(&entry.display_name()),
            Self::PathContains(needle) => entry
                .location()
                .as_path()
                .is_some_and(|path| path.to_string_lossy().to_lowercase().contains(needle)),
            Self::Extension(wanted) => entry.extension_hint().is_some_and(|found| found == *wanted),
            Self::Kind(kind) => entry.kind() == *kind,
            Self::Size(comparison, bytes) => entry
                .size()
                .is_some_and(|size| comparison.matches(size, *bytes)),
            Self::ModifiedAge(comparison, age) => entry.timestamps().modified.is_some_and(|at| {
                now.duration_since(at).is_ok_and(|elapsed| {
                    // "modified:<7d" reads as "modified less than 7 days ago",
                    // so a smaller elapsed time is a match for Less.
                    comparison.matches(elapsed.as_secs(), age.as_secs())
                })
            }),
            Self::Hidden(wanted) => entry.attributes().hidden == *wanted,
            Self::Not(inner) => !inner.matches(entry, now),
        }
    }
}

/// A parsed query: every term must match.
///
/// Implicit AND, which is what people expect from a search box. `OR` is in
/// `TODO.md`; shipping it half-done would make the precedence rules a surprise.
#[derive(Debug, Clone, Default)]
pub struct Query {
    terms: Vec<Term>,
}

impl Query {
    /// Build from terms.
    pub fn new(terms: Vec<Term>) -> Self {
        Self { terms }
    }

    /// The terms.
    pub fn terms(&self) -> &[Term] {
        &self.terms
    }

    /// Whether the query would match everything.
    pub fn is_empty(&self) -> bool {
        self.terms.is_empty()
    }

    /// Whether an entry matches every term.
    pub fn matches(&self, entry: &FileEntry, now: SystemTime) -> bool {
        self.terms.iter().all(|term| term.matches(entry, now))
    }
}

/// Shell-style wildcard match: `*` any run, `?` one character.
///
/// Iterative with backtracking bounded to one restart point, so a pattern of
/// nothing but `*` cannot make it exponential — the failure mode a naive
/// recursive glob has.
fn glob_matches(pattern: &str, text: &str) -> bool {
    let pattern: Vec<char> = pattern.chars().collect();
    let text: Vec<char> = text.chars().collect();

    let mut p = 0usize;
    let mut t = 0usize;
    let mut star: Option<usize> = None;
    let mut resume = 0usize;

    while t < text.len() {
        if p < pattern.len() && (pattern[p] == '?' || pattern[p] == text[t]) {
            p += 1;
            t += 1;
        } else if p < pattern.len() && pattern[p] == '*' {
            star = Some(p);
            resume = t;
            p += 1;
        } else if let Some(star_at) = star {
            p = star_at + 1;
            resume += 1;
            t = resume;
        } else {
            return false;
        }
    }
    while p < pattern.len() && pattern[p] == '*' {
        p += 1;
    }
    p == pattern.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glob_handles_stars_and_question_marks() {
        assert!(glob_matches("*.log", "server.log"));
        assert!(glob_matches("*.log", ".log"));
        assert!(!glob_matches("*.log", "server.txt"));
        assert!(glob_matches("a?c", "abc"));
        assert!(!glob_matches("a?c", "ac"));
        assert!(glob_matches("*", "anything"));
        assert!(glob_matches("*mid*", "start mid end"));
        assert!(glob_matches("exact", "exact"));
        assert!(!glob_matches("exact", "exactly"));
    }

    #[test]
    fn a_pathological_glob_still_returns() {
        // The case a recursive implementation goes exponential on.
        let pattern = "*a*a*a*a*a*a*a*a*a*a*b";
        let text = "a".repeat(64);
        assert!(!glob_matches(pattern, &text));
    }

    #[test]
    fn comparisons_do_what_they_say() {
        assert!(Comparison::Less.matches(1, 2));
        assert!(Comparison::Greater.matches(2, 1));
        assert!(Comparison::Equal.matches(2, 2));
        assert!(!Comparison::Less.matches(2, 2));
    }

    #[test]
    fn size_units_are_binary() {
        assert_eq!(SizeUnit::K.multiplier(), 1024);
        assert_eq!(SizeUnit::M.multiplier(), 1024 * 1024);
        assert_eq!(SizeUnit::G.multiplier(), 1024 * 1024 * 1024);
    }
}
