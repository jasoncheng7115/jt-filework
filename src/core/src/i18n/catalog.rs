//! Catalogue parsing.
//!
//! Format, one message per line:
//!
//! ```text
//! # a comment
//! menu.file.open = Open
//! jobs.copying   = Copying {count} items to {destination}
//! ```
//!
//! Rules:
//!
//! - keys are stable semantic identifiers, never English text
//!   (`docs/I18N_THEME.md` §3)
//! - `{name}` marks a placeholder; the set of placeholders is part of the
//!   message's contract and must match across locales
//! - `{{` and `}}` are literal braces. A message that documents a pattern
//!   language needs to be able to show one without it being substituted
//! - `\n` in a value is an escaped newline; there is no line continuation,
//!   because multi-line values invite fragment concatenation
//! - a duplicate key is an error, not a silent last-wins

use std::collections::{BTreeMap, BTreeSet};

use super::LocaleId;

/// Why a catalogue could not be parsed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogError {
    /// 1-based line number.
    pub line: usize,
    /// What went wrong, for developers.
    pub reason: CatalogErrorReason,
}

/// The specific parse failure.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum CatalogErrorReason {
    /// The line had no `=` separator.
    MissingSeparator,
    /// The key was empty.
    EmptyKey,
    /// The key contained a character outside `[a-z0-9._]`.
    InvalidKeyCharacter(char),
    /// The same key appeared twice in one locale.
    DuplicateKey(String),
    /// A `{` was never closed.
    UnterminatedPlaceholder,
    /// A placeholder name was empty or malformed.
    InvalidPlaceholder(String),
}

impl core::fmt::Display for CatalogError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "line {}: {:?}", self.line, self.reason)
    }
}

impl std::error::Error for CatalogError {}

/// A single localized message and the placeholders it declares.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
    template: String,
    placeholders: BTreeSet<String>,
}

impl Message {
    /// The raw template, placeholders unsubstituted.
    pub fn template(&self) -> &str {
        &self.template
    }

    /// The placeholder names this message requires.
    pub const fn placeholders(&self) -> &BTreeSet<String> {
        &self.placeholders
    }

    /// Substitute `{name}` occurrences.
    ///
    /// A placeholder with no supplied value is left as-is rather than being
    /// dropped, so the gap is visible in development instead of producing a
    /// sentence with a hole in it.
    pub fn render(&self, args: &BTreeMap<String, String>) -> String {
        if self.placeholders.is_empty() {
            return unescape_braces(&self.template);
        }
        let mut out = String::with_capacity(self.template.len());
        let mut rest = self.template.as_str();
        while let Some(start) = rest.find('{') {
            out.push_str(&rest[..start]);
            let after = &rest[start + 1..];
            // A doubled brace is a literal one.
            if let Some(stripped) = after.strip_prefix('{') {
                out.push('{');
                rest = stripped;
                continue;
            }
            if let Some(end) = after.find('}') {
                let name = &after[..end];
                if let Some(value) = args.get(name) {
                    out.push_str(value);
                } else {
                    out.push('{');
                    out.push_str(name);
                    out.push('}');
                }
                rest = &after[end + 1..];
            } else {
                out.push('{');
                rest = after;
            }
        }
        out.push_str(rest);
        // The opening halves were consumed while scanning; the closing ones
        // are collapsed here.
        out.replace("}}", "}")
    }
}

/// `{{` and `}}` become single braces.
fn unescape_braces(text: &str) -> String {
    if text.contains('{') || text.contains('}') {
        text.replace("{{", "{").replace("}}", "}")
    } else {
        text.to_string()
    }
}

/// All messages for one locale.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Catalog {
    locale: LocaleId,
    messages: BTreeMap<String, Message>,
}

impl Catalog {
    /// An empty catalogue for a locale.
    pub fn new(locale: LocaleId) -> Self {
        Self {
            locale,
            messages: BTreeMap::new(),
        }
    }

    /// Parse catalogue text.
    ///
    /// # Errors
    ///
    /// Returns the first malformed line. Catalogues are loaded from disk and
    /// are therefore untrusted input (`docs/SECURITY.md` §2); this parser must
    /// never panic and is a fuzz target (`docs/TESTING.md` §9.1).
    pub fn parse(locale: LocaleId, text: &str) -> Result<Self, CatalogError> {
        let mut catalog = Self::new(locale);
        for (index, raw_line) in text.lines().enumerate() {
            let line_no = index + 1;
            let line = raw_line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let Some((key_part, value_part)) = line.split_once('=') else {
                return Err(CatalogError {
                    line: line_no,
                    reason: CatalogErrorReason::MissingSeparator,
                });
            };
            let key = key_part.trim();
            validate_key(key, line_no)?;
            if catalog.messages.contains_key(key) {
                return Err(CatalogError {
                    line: line_no,
                    reason: CatalogErrorReason::DuplicateKey(key.to_string()),
                });
            }
            let template = unescape(value_part.trim());
            let placeholders = extract_placeholders(&template, line_no)?;
            catalog.messages.insert(
                key.to_string(),
                Message {
                    template,
                    placeholders,
                },
            );
        }
        Ok(catalog)
    }

    /// Which locale this catalogue is for.
    pub const fn locale(&self) -> &LocaleId {
        &self.locale
    }

    /// Look up a message.
    pub fn get(&self, key: &str) -> Option<&Message> {
        self.messages.get(key)
    }

    /// Whether the catalogue defines a key.
    pub fn contains(&self, key: &str) -> bool {
        self.messages.contains_key(key)
    }

    /// Every key, in sorted order. Used by the parity check.
    pub fn keys(&self) -> impl Iterator<Item = &str> {
        self.messages.keys().map(String::as_str)
    }

    /// Number of messages.
    pub fn len(&self) -> usize {
        self.messages.len()
    }

    /// Whether the catalogue has no messages.
    pub fn is_empty(&self) -> bool {
        self.messages.is_empty()
    }

    /// Merge another catalogue of the same locale into this one.
    ///
    /// # Errors
    ///
    /// Fails on a duplicate key, so splitting catalogues across files cannot
    /// silently shadow a message.
    pub fn merge(&mut self, other: Self) -> Result<(), CatalogError> {
        for (key, message) in other.messages {
            if self.messages.contains_key(&key) {
                return Err(CatalogError {
                    line: 0,
                    reason: CatalogErrorReason::DuplicateKey(key),
                });
            }
            self.messages.insert(key, message);
        }
        Ok(())
    }
}

fn validate_key(key: &str, line: usize) -> Result<(), CatalogError> {
    if key.is_empty() {
        return Err(CatalogError {
            line,
            reason: CatalogErrorReason::EmptyKey,
        });
    }
    for ch in key.chars() {
        if !(ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '.' || ch == '_') {
            return Err(CatalogError {
                line,
                reason: CatalogErrorReason::InvalidKeyCharacter(ch),
            });
        }
    }
    Ok(())
}

fn unescape(value: &str) -> String {
    if !value.contains('\\') {
        return value.to_string();
    }
    let mut out = String::with_capacity(value.len());
    let mut chars = value.chars();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            out.push(ch);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            // An unrecognised escape is preserved verbatim rather than being
            // swallowed, so a translator's stray backslash stays visible.
            Some(other) => {
                if other != '\\' {
                    out.push('\\');
                }
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}

fn extract_placeholders(template: &str, line: usize) -> Result<BTreeSet<String>, CatalogError> {
    let mut found = BTreeSet::new();
    let mut rest = template;
    while let Some(start) = rest.find('{') {
        let after = &rest[start + 1..];
        // `{{` is a literal brace, not the start of a placeholder: a message
        // that documents a pattern language has to be able to show one.
        if let Some(stripped) = after.strip_prefix('{') {
            rest = stripped;
            continue;
        }
        let Some(end) = after.find('}') else {
            return Err(CatalogError {
                line,
                reason: CatalogErrorReason::UnterminatedPlaceholder,
            });
        };
        let name = &after[..end];
        if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            return Err(CatalogError {
                line,
                reason: CatalogErrorReason::InvalidPlaceholder(name.to_string()),
            });
        }
        found.insert(name.to_string());
        rest = &after[end + 1..];
    }
    Ok(found)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn en(text: &str) -> Result<Catalog, CatalogError> {
        Catalog::parse(LocaleId::english(), text)
    }

    #[test]
    fn parses_keys_comments_and_blank_lines() {
        let c = en("# comment\n\nmenu.file.open = Open\nmenu.file.rename = Rename\n").unwrap();
        assert_eq!(c.len(), 2);
        assert_eq!(c.get("menu.file.open").unwrap().template(), "Open");
    }

    #[test]
    fn placeholders_are_part_of_the_contract() {
        let c = en("jobs.copying = Copying {count} items to {destination}").unwrap();
        let m = c.get("jobs.copying").unwrap();
        assert_eq!(
            m.placeholders()
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>(),
            vec!["count", "destination"]
        );
    }

    #[test]
    fn renders_with_arguments() {
        let c = en("jobs.copying = Copying {count} items").unwrap();
        let mut args = BTreeMap::new();
        args.insert("count".to_string(), "12".to_string());
        assert_eq!(
            c.get("jobs.copying").unwrap().render(&args),
            "Copying 12 items"
        );
    }

    #[test]
    fn a_missing_argument_stays_visible_instead_of_producing_a_hole() {
        let c = en("jobs.copying = Copying {count} items").unwrap();
        let rendered = c.get("jobs.copying").unwrap().render(&BTreeMap::new());
        assert_eq!(rendered, "Copying {count} items");
    }

    #[test]
    fn duplicate_key_is_rejected() {
        let err = en("a.b = one\na.b = two").unwrap_err();
        assert_eq!(err.line, 2);
        assert!(matches!(err.reason, CatalogErrorReason::DuplicateKey(_)));
    }

    #[test]
    fn english_text_as_a_key_is_rejected() {
        // docs/I18N_THEME.md 3: keys are semantic identifiers, not English.
        let err = en("Open File = Open").unwrap_err();
        assert!(matches!(
            err.reason,
            CatalogErrorReason::InvalidKeyCharacter(_)
        ));
    }

    #[test]
    fn malformed_input_is_an_error_not_a_panic() {
        assert!(en("no separator here").is_err());
        assert!(en(" = value").is_err());
        assert!(en("a.b = {unterminated").is_err());
        assert!(en("a.b = {}").is_err());
        assert!(en("a.b = {bad-name}").is_err());
    }

    #[test]
    fn a_doubled_brace_is_a_literal_one() {
        // A message that documents a pattern language has to be able to show
        // a placeholder without it being substituted.
        let c = en("batch.hint = use {{name}} and {{n:3}} in the template").unwrap();
        let message = c.get("batch.hint").unwrap();
        assert!(
            message.placeholders().is_empty(),
            "nothing here is a placeholder"
        );
        assert_eq!(
            message.render(&BTreeMap::new()),
            "use {name} and {n:3} in the template"
        );
    }

    #[test]
    fn literal_and_real_placeholders_coexist() {
        let c = en("mixed = {count} items match {{name}}").unwrap();
        let message = c.get("mixed").unwrap();
        assert_eq!(
            message
                .placeholders()
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>(),
            vec!["count"]
        );
        let mut args = BTreeMap::new();
        args.insert("count".to_string(), "3".to_string());
        assert_eq!(message.render(&args), "3 items match {name}");
    }

    #[test]
    fn merge_refuses_to_shadow_an_existing_key() {
        let mut a = en("a.b = one").unwrap();
        assert!(a.merge(en("c.d = two").unwrap()).is_ok());
        assert_eq!(a.len(), 2);
        assert!(a.merge(en("a.b = three").unwrap()).is_err());
    }

    #[test]
    fn escapes_are_handled_without_line_continuation() {
        let c = en(r"a.b = first\nsecond").unwrap();
        assert_eq!(c.get("a.b").unwrap().template(), "first\nsecond");
    }
}
