//! Turning what the user typed into a [`Query`].
//!
//! Parsing is separate from walking so that a bad query fails instantly, with
//! a position and a reason, rather than after scanning a disk
//! (`docs/SEARCH_AI.md` §2.2).

use std::time::Duration;

use jtf_core::FileKind;
use regex::Regex;

use crate::query::{Comparison, Query, SizeUnit, Term};

/// Why a query could not be parsed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    /// Character offset where the problem is.
    pub position: usize,
    /// What went wrong.
    pub reason: ParseErrorReason,
}

/// The specific failure.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ParseErrorReason {
    /// A field name that does not exist. Not ignored: a silently dropped term
    /// returns the wrong results and looks like it worked.
    UnknownField(String),
    /// A field with nothing after the colon.
    EmptyValue(String),
    /// A size that is not a number and a unit.
    BadSize(String),
    /// A duration that is not a number and a unit.
    BadDuration(String),
    /// A kind name that does not exist.
    UnknownKind(String),
    /// A regular expression the engine refused.
    BadRegex(String),
    /// A quote that never closed.
    UnterminatedQuote,
    /// `NOT` with nothing after it.
    DanglingNot,
}

impl core::fmt::Display for ParseError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "at {}: {:?}", self.position, self.reason)
    }
}

impl std::error::Error for ParseError {}

impl ParseError {
    /// Localization key for the message shown to the user.
    pub const fn message_key(&self) -> &'static str {
        match self.reason {
            ParseErrorReason::UnknownField(_) => "query.unknown_field",
            ParseErrorReason::EmptyValue(_) => "query.empty_value",
            ParseErrorReason::BadSize(_) => "query.bad_size",
            ParseErrorReason::BadDuration(_) => "query.bad_duration",
            ParseErrorReason::UnknownKind(_) => "query.unknown_kind",
            ParseErrorReason::BadRegex(_) => "query.bad_regex",
            ParseErrorReason::UnterminatedQuote => "query.unterminated_quote",
            ParseErrorReason::DanglingNot => "query.dangling_not",
        }
    }
}

/// One token and where it started.
struct Token {
    text: String,
    position: usize,
}

/// Split on whitespace, keeping quoted runs together.
fn tokenize(input: &str) -> Result<Vec<Token>, ParseError> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut start = 0usize;
    let mut quote: Option<char> = None;

    for (index, character) in input.char_indices() {
        match quote {
            Some(open) if character == open => quote = None,
            Some(_) => current.push(character),
            None if character == '"' || character == '\'' => {
                if current.is_empty() {
                    start = index;
                }
                quote = Some(character);
            }
            None if character.is_whitespace() => {
                if !current.is_empty() {
                    tokens.push(Token {
                        text: std::mem::take(&mut current),
                        position: start,
                    });
                }
            }
            None => {
                if current.is_empty() {
                    start = index;
                }
                current.push(character);
            }
        }
    }
    if quote.is_some() {
        return Err(ParseError {
            position: start,
            reason: ParseErrorReason::UnterminatedQuote,
        });
    }
    if !current.is_empty() {
        tokens.push(Token {
            text: current,
            position: start,
        });
    }
    Ok(tokens)
}

/// Parse a query.
///
/// # Errors
///
/// [`ParseError`] with the position of the problem.
pub fn parse(input: &str) -> Result<Query, ParseError> {
    let tokens = tokenize(input)?;
    let mut terms = Vec::new();
    let mut negate_next = false;

    for token in &tokens {
        let text = token.text.as_str();

        if text.eq_ignore_ascii_case("not") {
            negate_next = true;
            continue;
        }
        let (text, negated) = match text.strip_prefix('-') {
            // A bare "-" is a filename character as often as it is a negation,
            // so only "-field:value" and "-word" negate, never a lone dash.
            Some(rest) if !rest.is_empty() => (rest, true),
            _ => (text, false),
        };
        let negated = negated || std::mem::take(&mut negate_next);

        let term = parse_term(text, token.position)?;
        terms.push(if negated {
            Term::Not(Box::new(term))
        } else {
            term
        });
    }

    if negate_next {
        return Err(ParseError {
            position: input.len(),
            reason: ParseErrorReason::DanglingNot,
        });
    }
    Ok(Query::new(terms))
}

fn parse_term(text: &str, position: usize) -> Result<Term, ParseError> {
    let Some((field, value)) = text.split_once(':') else {
        // A bare word is a substring of the name, which is what a search box
        // is expected to do before anyone reads a syntax reference.
        return Ok(Term::NameContains(text.to_lowercase()));
    };
    if value.is_empty() {
        return Err(ParseError {
            position,
            reason: ParseErrorReason::EmptyValue(field.to_string()),
        });
    }

    match field.to_lowercase().as_str() {
        "name" => Ok(Term::NameContains(value.to_lowercase())),
        "glob" => Ok(Term::NameGlob(value.to_lowercase())),
        "re" | "regex" => Regex::new(value)
            .map(|r| Term::NameRegex(Box::new(r)))
            .map_err(|_| ParseError {
                position,
                reason: ParseErrorReason::BadRegex(value.to_string()),
            }),
        "path" => Ok(Term::PathContains(value.to_lowercase())),
        "ext" => Ok(Term::Extension(
            value.trim_start_matches('.').to_lowercase(),
        )),
        "kind" => parse_kind(value, position),
        "size" => parse_size(value, position),
        "modified" => parse_age(value, position),
        "hidden" => Ok(Term::Hidden(matches!(
            value.to_lowercase().as_str(),
            "1" | "true" | "yes"
        ))),
        other => Err(ParseError {
            position,
            reason: ParseErrorReason::UnknownField(other.to_string()),
        }),
    }
}

fn parse_kind(value: &str, position: usize) -> Result<Term, ParseError> {
    let kind = match value.to_lowercase().as_str() {
        "file" => FileKind::File,
        "dir" | "folder" | "directory" => FileKind::Directory,
        "link" | "symlink" => FileKind::Symlink,
        "archive" => FileKind::Archive,
        "app" | "application" => FileKind::ApplicationBundle,
        "package" => FileKind::Package,
        other => {
            return Err(ParseError {
                position,
                reason: ParseErrorReason::UnknownKind(other.to_string()),
            })
        }
    };
    Ok(Term::Kind(kind))
}

/// Split a leading `<` or `>` off a value.
fn comparison_of(value: &str) -> (Comparison, &str) {
    match value.as_bytes().first() {
        Some(b'<') => (Comparison::Less, &value[1..]),
        Some(b'>') => (Comparison::Greater, &value[1..]),
        _ => (Comparison::Equal, value),
    }
}

fn parse_size(value: &str, position: usize) -> Result<Term, ParseError> {
    let (comparison, rest) = comparison_of(value);
    let error = || ParseError {
        position,
        reason: ParseErrorReason::BadSize(value.to_string()),
    };

    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    if digits.is_empty() {
        return Err(error());
    }
    let unit = match rest[digits.len()..].to_lowercase().as_str() {
        "" | "b" => SizeUnit::B,
        "k" | "kb" | "kib" => SizeUnit::K,
        "m" | "mb" | "mib" => SizeUnit::M,
        "g" | "gb" | "gib" => SizeUnit::G,
        _ => return Err(error()),
    };
    let count: u64 = digits.parse().map_err(|_| error())?;
    let bytes = count.checked_mul(unit.multiplier()).ok_or_else(error)?;
    Ok(Term::Size(comparison, bytes))
}

fn parse_age(value: &str, position: usize) -> Result<Term, ParseError> {
    let (comparison, rest) = comparison_of(value);
    let error = || ParseError {
        position,
        reason: ParseErrorReason::BadDuration(value.to_string()),
    };

    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    if digits.is_empty() {
        return Err(error());
    }
    let seconds_per = match rest[digits.len()..].to_lowercase().as_str() {
        "m" | "min" => 60u64,
        "h" | "hour" => 3_600,
        "" | "d" | "day" => 86_400,
        "w" | "week" => 604_800,
        "y" | "year" => 31_536_000,
        _ => return Err(error()),
    };
    let count: u64 = digits.parse().map_err(|_| error())?;
    let total = count.checked_mul(seconds_per).ok_or_else(error)?;
    Ok(Term::ModifiedAge(comparison, Duration::from_secs(total)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bare_word_searches_the_name() {
        let query = parse("report").unwrap();
        assert_eq!(query.terms().len(), 1);
        assert!(matches!(query.terms()[0], Term::NameContains(ref s) if s == "report"));
    }

    #[test]
    fn terms_combine_with_implicit_and() {
        let query = parse("glob:*.log size:>1M").unwrap();
        assert_eq!(query.terms().len(), 2);
    }

    #[test]
    fn quotes_keep_a_phrase_together() {
        let query = parse("\"annual report\"").unwrap();
        assert!(matches!(query.terms()[0], Term::NameContains(ref s) if s == "annual report"));
    }

    #[test]
    fn an_unterminated_quote_is_reported_rather_than_guessed() {
        let error = parse("\"unclosed").unwrap_err();
        assert_eq!(error.reason, ParseErrorReason::UnterminatedQuote);
    }

    #[test]
    fn sizes_accept_units_and_comparisons() {
        assert!(matches!(
            parse("size:>100M").unwrap().terms()[0],
            Term::Size(Comparison::Greater, bytes) if bytes == 100 * 1024 * 1024
        ));
        assert!(matches!(
            parse("size:<2k").unwrap().terms()[0],
            Term::Size(Comparison::Less, 2048)
        ));
        assert!(matches!(
            parse("size:512").unwrap().terms()[0],
            Term::Size(Comparison::Equal, 512)
        ));
    }

    #[test]
    fn durations_accept_units() {
        assert!(matches!(
            parse("modified:<7d").unwrap().terms()[0],
            Term::ModifiedAge(Comparison::Less, d) if d.as_secs() == 7 * 86_400
        ));
        assert!(matches!(
            parse("modified:>2h").unwrap().terms()[0],
            Term::ModifiedAge(Comparison::Greater, d) if d.as_secs() == 7_200
        ));
    }

    #[test]
    fn an_unknown_field_is_an_error_not_a_silently_dropped_term() {
        // docs/SEARCH_AI.md 2.2: a dropped term returns the wrong results and
        // looks like it worked, which is worse than refusing.
        let error = parse("colour:red").unwrap_err();
        assert_eq!(
            error.reason,
            ParseErrorReason::UnknownField("colour".into())
        );
        assert_eq!(error.message_key(), "query.unknown_field");
    }

    #[test]
    fn malformed_values_report_their_field_and_position() {
        assert!(matches!(
            parse("size:big").unwrap_err().reason,
            ParseErrorReason::BadSize(_)
        ));
        assert!(matches!(
            parse("modified:soon").unwrap_err().reason,
            ParseErrorReason::BadDuration(_)
        ));
        assert!(matches!(
            parse("kind:sandwich").unwrap_err().reason,
            ParseErrorReason::UnknownKind(_)
        ));
        assert!(matches!(
            parse("name:").unwrap_err().reason,
            ParseErrorReason::EmptyValue(_)
        ));

        let error = parse("name:x size:big").unwrap_err();
        assert_eq!(error.position, 7, "the position points at the bad term");
    }

    #[test]
    fn a_bad_regex_is_refused_rather_than_matching_nothing() {
        assert!(matches!(
            parse("re:[unclosed").unwrap_err().reason,
            ParseErrorReason::BadRegex(_)
        ));
    }

    #[test]
    fn negation_works_both_ways_and_a_lone_dash_is_a_filename() {
        assert!(matches!(parse("-cache").unwrap().terms()[0], Term::Not(_)));
        assert!(matches!(
            parse("NOT cache").unwrap().terms()[0],
            Term::Not(_)
        ));
        // A dash on its own is a perfectly ordinary filename character.
        assert!(matches!(parse("-").unwrap().terms()[0], Term::NameContains(ref s) if s == "-"));
        assert_eq!(
            parse("NOT").unwrap_err().reason,
            ParseErrorReason::DanglingNot
        );
    }

    #[test]
    fn an_empty_query_matches_everything_rather_than_failing() {
        assert!(parse("").unwrap().is_empty());
        assert!(parse("   ").unwrap().is_empty());
    }

    #[test]
    fn parsing_never_panics_on_arbitrary_input() {
        // Stands in for the fuzz target until fuzzing is wired up.
        for input in [
            ":",
            "::",
            "a:",
            ":b",
            "size:",
            "size:>",
            "size:>>1",
            "re:(",
            "\"",
            "'",
            "-",
            "--",
            "NOT NOT",
            "name:\"a b\"",
            "kind:",
            "\u{4e2d}\u{6587}",
            "size:99999999999999999999G",
        ] {
            let _ = parse(input);
        }
    }
}
