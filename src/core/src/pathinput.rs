//! Turning what a person typed into a path.
//!
//! A path field accepts shorthand that no filesystem call understands: `~`
//! for the home directory, `$HOME` and `%USERPROFILE%` for what the
//! environment says, `..` and `./x` relative to where you already are. Typing
//! `~` and getting a folder *named* `~` is the failure this exists to
//! prevent.
//!
//! Expansion is deliberately conservative. It is not a shell: there is no
//! globbing, no command substitution, no word splitting, and an unset
//! variable expands to nothing rather than to an error or to its own name.
//! The result is a path, and the caller still has to check it exists.

use std::path::{Path, PathBuf};

/// The longest input accepted.
///
/// A path field is untrusted input like any other; expansion allocates, so it
/// is bounded (`docs/SECURITY.md` §13).
pub const MAX_INPUT: usize = 4096;

/// How many times a variable expansion may itself contain a variable.
///
/// One. `$A` is expanded, and whatever it expands to is used as written —
/// otherwise a variable whose value mentions itself is an infinite loop.
const EXPANSION_PASSES: usize = 1;

/// Expand what the user typed, relative to `current` when it is not absolute.
///
/// `home` is the home directory, and `lookup` reads an environment variable.
/// Both are passed in rather than read here so the behaviour is testable
/// without touching the real environment.
pub fn expand(
    input: &str,
    home: &Path,
    current: &Path,
    lookup: &dyn Fn(&str) -> Option<String>,
) -> Option<PathBuf> {
    let trimmed = input.trim();
    if trimmed.is_empty() || trimmed.len() > MAX_INPUT {
        return None;
    }

    let mut text = trimmed.to_string();
    for _ in 0..EXPANSION_PASSES {
        text = expand_variables(&text, lookup);
    }

    // `~` alone, or `~/` prefixed. `~user` is deliberately not supported:
    // resolving another user's home needs the platform's account database,
    // and guessing it from a sibling directory name is wrong often enough to
    // matter.
    let expanded = if text == "~" {
        home.to_path_buf()
    } else if let Some(rest) = text.strip_prefix("~/").or_else(|| text.strip_prefix("~\\")) {
        home.join(rest)
    } else {
        PathBuf::from(&text)
    };

    if expanded.as_os_str().is_empty() {
        return None;
    }
    if expanded.is_absolute() {
        return Some(normalize(&expanded));
    }
    Some(normalize(&current.join(expanded)))
}

/// Replace `$NAME`, `${NAME}` and `%NAME%` with the environment's value.
fn expand_variables(text: &str, lookup: &dyn Fn(&str) -> Option<String>) -> String {
    let mut out = String::with_capacity(text.len());
    let bytes: Vec<char> = text.chars().collect();
    let mut i = 0;

    while i < bytes.len() {
        match bytes[i] {
            '$' if i + 1 < bytes.len() && bytes[i + 1] == '{' => {
                let start = i + 2;
                if let Some(offset) = bytes[start..].iter().position(|c| *c == '}') {
                    let name: String = bytes[start..start + offset].iter().collect();
                    out.push_str(&lookup(&name).unwrap_or_default());
                    i = start + offset + 1;
                } else {
                    // An unterminated `${` is text, not an error: the user is
                    // probably still typing it.
                    out.push(bytes[i]);
                    i += 1;
                }
            }
            '$' => {
                let start = i + 1;
                let end = start
                    + bytes[start..]
                        .iter()
                        .take_while(|c| c.is_alphanumeric() || **c == '_')
                        .count();
                if end == start {
                    out.push('$');
                    i += 1;
                } else {
                    let name: String = bytes[start..end].iter().collect();
                    out.push_str(&lookup(&name).unwrap_or_default());
                    i = end;
                }
            }
            '%' => {
                let start = i + 1;
                match bytes[start..].iter().position(|c| *c == '%') {
                    Some(offset) if offset > 0 => {
                        let name: String = bytes[start..start + offset].iter().collect();
                        if name.chars().all(|c| c.is_alphanumeric() || c == '_') {
                            out.push_str(&lookup(&name).unwrap_or_default());
                            i = start + offset + 1;
                        } else {
                            out.push('%');
                            i += 1;
                        }
                    }
                    _ => {
                        out.push('%');
                        i += 1;
                    }
                }
            }
            other => {
                out.push(other);
                i += 1;
            }
        }
    }
    out
}

/// Resolve `.` and `..` textually, without touching the filesystem.
///
/// Textual on purpose: this runs while the user types, and it must not stat
/// anything. A `..` that climbs past the root stops there rather than
/// wrapping into nonsense.
fn normalize(path: &Path) -> PathBuf {
    let mut parts: Vec<std::ffi::OsString> = Vec::new();
    let mut prefix = PathBuf::new();

    for component in path.components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                parts.pop();
            }
            std::path::Component::RootDir | std::path::Component::Prefix(_) => {
                prefix.push(component.as_os_str());
            }
            std::path::Component::Normal(part) => parts.push(part.to_os_string()),
        }
    }

    let mut out = prefix;
    for part in parts {
        out.push(part);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn home() -> PathBuf {
        PathBuf::from("/Users/someone")
    }

    fn here() -> PathBuf {
        PathBuf::from("/Users/someone/Projects")
    }

    fn env(name: &str) -> Option<String> {
        match name {
            "HOME" => Some("/Users/someone".to_string()),
            "WORK" => Some("/Volumes/Work".to_string()),
            "EMPTY" => Some(String::new()),
            _ => None,
        }
    }

    fn expanded(input: &str) -> Option<PathBuf> {
        expand(input, &home(), &here(), &env)
    }

    #[test]
    fn a_bare_tilde_is_the_home_directory() {
        assert_eq!(
            expanded("~"),
            Some(home()),
            "typing ~ and getting a folder named ~ is the whole reason this \
             function exists"
        );
    }

    #[test]
    fn a_tilde_prefix_joins_onto_home() {
        assert_eq!(
            expanded("~/Documents"),
            Some(PathBuf::from("/Users/someone/Documents"))
        );
    }

    #[test]
    fn a_tilde_in_the_middle_is_an_ordinary_character() {
        assert_eq!(
            expanded("/tmp/a~b"),
            Some(PathBuf::from("/tmp/a~b")),
            "a file may legitimately be called a~b"
        );
    }

    #[test]
    fn another_users_home_is_not_guessed() {
        assert_eq!(
            expanded("~other/Documents"),
            Some(PathBuf::from("/Users/someone/Projects/~other/Documents")),
            "~user needs the account database; guessing it from a sibling \
             directory name is wrong often enough to matter"
        );
    }

    #[test]
    fn environment_variables_expand_in_every_spelling() {
        assert_eq!(expanded("$WORK"), Some(PathBuf::from("/Volumes/Work")));
        assert_eq!(expanded("${WORK}"), Some(PathBuf::from("/Volumes/Work")));
        assert_eq!(expanded("%WORK%"), Some(PathBuf::from("/Volumes/Work")));
        assert_eq!(
            expanded("$WORK/sub"),
            Some(PathBuf::from("/Volumes/Work/sub"))
        );
    }

    #[test]
    fn an_unset_variable_expands_to_nothing_rather_than_to_its_own_name() {
        assert_eq!(
            expanded("$NOPE/x"),
            Some(PathBuf::from("/x")),
            "leaving the literal $NOPE in the path would produce a folder \
             name nobody meant"
        );
        assert_eq!(expanded("$NOPE"), None, "nothing left to navigate to");
    }

    #[test]
    fn a_relative_path_is_relative_to_the_current_folder() {
        assert_eq!(
            expanded("sub"),
            Some(PathBuf::from("/Users/someone/Projects/sub"))
        );
        assert_eq!(
            expanded("./sub"),
            Some(PathBuf::from("/Users/someone/Projects/sub"))
        );
        assert_eq!(expanded(".."), Some(PathBuf::from("/Users/someone")));
        assert_eq!(
            expanded("../Music"),
            Some(PathBuf::from("/Users/someone/Music"))
        );
    }

    #[test]
    fn climbing_past_the_root_stops_there() {
        assert_eq!(
            expanded("/../../.."),
            Some(PathBuf::from("/")),
            "there is nothing above the root, and wrapping into nonsense is \
             worse than stopping"
        );
    }

    #[test]
    fn nothing_is_globbed_or_executed() {
        // A shell would expand these. This is not a shell.
        assert_eq!(expanded("/tmp/*"), Some(PathBuf::from("/tmp/*")));
        assert_eq!(
            expanded("/tmp/$(whoami)"),
            Some(PathBuf::from("/tmp/$(whoami)")),
            "`$(` is not a variable name, so it stays as typed rather than \
             being run"
        );
        assert_eq!(
            expanded("/tmp/a;rm -rf b"),
            Some(PathBuf::from("/tmp/a;rm -rf b")),
            "a semicolon is a character in a file name here, not a separator"
        );
    }

    #[test]
    fn whitespace_is_trimmed_and_empty_input_is_refused() {
        assert_eq!(expanded("  ~  "), Some(home()));
        assert_eq!(expanded(""), None);
        assert_eq!(expanded("   "), None);
    }

    #[test]
    fn an_absurdly_long_input_is_refused_rather_than_expanded() {
        let long = "a".repeat(MAX_INPUT + 1);
        assert_eq!(expanded(&long), None);
    }

    #[test]
    fn a_variable_whose_value_contains_a_variable_is_not_re_expanded() {
        let recursive = |name: &str| match name {
            "LOOP" => Some("$LOOP".to_string()),
            _ => None,
        };
        assert_eq!(
            expand("$LOOP", &home(), &here(), &recursive),
            Some(PathBuf::from("/Users/someone/Projects/$LOOP")),
            "one pass only; a value that mentions itself must not loop"
        );
    }
}
