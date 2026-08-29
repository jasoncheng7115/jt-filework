//! Every parser, given input designed to break it.
//!
//! `docs/SECURITY.md` §2 lists what is untrusted: catalogues loaded from disk,
//! keymaps, session state, search queries, rename patterns, file content. Each
//! has a parser, and a parser that panics on hostile input is a denial of
//! service at best.
//!
//! This stands in for the fuzz targets in `docs/TESTING.md` §9.1 until fuzzing
//! is wired up: the corpus is deterministic and adversarial rather than
//! random, which finds different things and finds them in CI.

// A test asserts by panicking, so the workspace's unwrap/expect/panic lints
// are exactly backwards here.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use jtf_commands::{KeyChord, Keymap};
use jtf_core::i18n::{Catalog, LocaleId};
use jtf_ops::{preview_batch_rename, RenamePattern};
use jtf_workspace::Session;

/// Inputs chosen to hit boundaries: empty, unterminated, nested, enormous,
/// non-ASCII, and shaped like the format without being it.
fn adversarial_strings() -> Vec<String> {
    let mut inputs: Vec<String> = [
        "",
        " ",
        "\n",
        "\r\n",
        "\0",
        "=",
        "==",
        " = ",
        "a =",
        "= b",
        "#",
        "# only a comment",
        "{",
        "}",
        "{}",
        "{{",
        "}}",
        "{unterminated",
        "{a}{b}{c}",
        "\\",
        "\\\\",
        "\\n",
        "\"",
        "'",
        "\"unclosed",
        ":",
        "::",
        "a:",
        ":b",
        "-",
        "--",
        "NOT",
        "NOT NOT",
        "+",
        "*",
        "[",
        "(",
        "(?",
        "\u{4e2d}\u{6587}",
        "\u{1f600}",
        "\u{202e}reversed",
        "e\u{0301}combining",
        "\u{feff}bom",
    ]
    .iter()
    .map(|s| (*s).to_string())
    .collect();

    // Long, deep and repetitive: the shapes that turn a linear parser
    // quadratic or a recursive one into a stack overflow.
    inputs.push("x".repeat(100_000));
    inputs.push("{".repeat(10_000));
    inputs.push("a:b ".repeat(10_000));
    inputs.push("*".repeat(1_000));
    inputs.push(format!("key = {}", "v".repeat(50_000)));
    inputs.push("\u{4e2d}".repeat(20_000));
    inputs
}

#[test]
fn the_catalogue_parser_never_panics() {
    for input in adversarial_strings() {
        let _ = Catalog::parse(LocaleId::english(), &input);
    }
}

#[test]
fn the_keymap_parser_never_panics() {
    for input in adversarial_strings() {
        let _ = Keymap::parse("hostile", &input);
        let _ = KeyChord::parse(&input);
    }
}

#[test]
fn the_query_parser_never_panics() {
    for input in adversarial_strings() {
        let _ = jtf_search::parse(&input);
    }
}

#[test]
fn session_restore_never_panics() {
    let home = jtf_core::Location::local("/tmp");
    for input in adversarial_strings() {
        let restored = Session::restore(Some(&input), &home);
        // Whatever happens, what comes back is usable.
        assert!(
            restored.workspace.invariants_hold(),
            "unusable workspace from {input:.40?}"
        );
    }
}

#[test]
fn the_rename_pattern_never_panics() {
    let sources = vec![std::path::PathBuf::from("/tmp/jtf-hostile-name.txt")];
    for input in adversarial_strings() {
        for regex in [false, true] {
            let pattern = RenamePattern {
                template: input.clone(),
                find: input.clone(),
                replace: input.clone(),
                regex,
                start: u64::MAX,
            };
            let preview = preview_batch_rename(&sources, &pattern);
            // Anything derived from hostile input is either refused or a name
            // that stays inside its directory.
            for row in &preview.rows {
                if row.issue.will_apply() {
                    assert!(!row.to.contains('/'), "escaped: {:?}", row.to);
                    assert!(!row.to.contains('\\'), "escaped: {:?}", row.to);
                    assert!(!row.to.is_empty());
                }
            }
        }
    }
}

#[test]
fn a_query_that_parses_always_produces_a_usable_matcher() {
    // A parsed query must never then panic while matching.
    let entry = jtf_core::FileEntry::new(
        jtf_core::Location::local("/tmp/\u{4e2d}\u{6587}.txt"),
        jtf_core::RawName::new("\u{4e2d}\u{6587}.txt"),
        jtf_core::FileKind::File,
    );
    let now = std::time::SystemTime::now();

    for input in adversarial_strings() {
        if let Ok(query) = jtf_search::parse(&input) {
            let _ = query.matches(&entry, now);
        }
    }
}

#[test]
fn a_glob_of_nothing_but_wildcards_still_returns() {
    // The shape a recursive glob goes exponential on. It must finish, and the
    // test finishing at all is the assertion.
    let entry = jtf_core::FileEntry::new(
        jtf_core::Location::local("/tmp/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa.txt"),
        jtf_core::RawName::new("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa.txt"),
        jtf_core::FileKind::File,
    );
    let now = std::time::SystemTime::now();

    for pattern in [
        "*a*a*a*a*a*a*a*a*a*a*b",
        &"*".repeat(500),
        &"*a".repeat(200),
    ] {
        let query = jtf_search::parse(&format!("glob:{pattern}")).unwrap();
        let _ = query.matches(&entry, now);
    }
}
