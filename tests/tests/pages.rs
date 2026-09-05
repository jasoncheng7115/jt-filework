//! The published pages — `docs/UI_TEST_PLAN.md` §23.
//!
//! `docs/index.html` and `docs/features.html` are the only part of this project
//! a stranger sees before deciding whether to run it, and they are hand-written
//! HTML with no build step to catch a mistake. Two bugs got as far as the
//! published site, and both are checked here:
//!
//! - **Both languages showing at once.** The switch hid a language with
//!   `[data-lang] { display: none; }` — one attribute, weaker than any class.
//!   A component that set its own `display` therefore appeared in English *and*
//!   Chinese; the index of the specification page rendered all fourteen of its
//!   entries twice.
//! - **A navigation link that did nothing.** Both language versions of a
//!   heading carried the same `id`, so `#formats` resolved to whichever came
//!   first — the English one, hidden, with no box for the browser to scroll to.

// A test asserts by panicking, so the workspace's unwrap/expect/panic lints
// are exactly backwards here: an `unwrap` that fails *is* the failure report.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf()
}

const PAGES: [&str; 2] = ["docs/index.html", "docs/features.html"];

fn page(name: &str) -> String {
    let path = repo_root().join(name);
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()))
}

/// Every `id="…"` in the document, in order.
fn ids(html: &str) -> Vec<String> {
    attribute_values(html, " id=\"")
}

/// Every `href="#…"` in the document.
fn fragment_links(html: &str) -> BTreeSet<String> {
    attribute_values(html, "href=\"#").into_iter().collect()
}

fn attribute_values(html: &str, opener: &str) -> Vec<String> {
    let mut found = Vec::new();
    let mut rest = html;
    while let Some(at) = rest.find(opener) {
        rest = &rest[at + opener.len()..];
        match rest.find('"') {
            Some(end) => {
                found.push(rest[..end].to_string());
                rest = &rest[end..];
            }
            None => break,
        }
    }
    found
}

#[test]
fn a_navigation_link_always_has_something_to_scroll_to() {
    for name in PAGES {
        let html = page(name);
        let present: BTreeSet<String> = ids(&html).into_iter().collect();
        for target in fragment_links(&html) {
            assert!(
                present.contains(&target),
                "{name}: the link to #{target} goes nowhere"
            );
        }
    }
}

#[test]
fn no_two_elements_claim_the_same_anchor() {
    // The bug this catches: a section heading written once per language, both
    // copies carrying the section's id. The browser takes the first, which is
    // the hidden one in the other language, and the link does nothing at all.
    for name in PAGES {
        let html = page(name);
        let mut seen: BTreeMap<String, usize> = BTreeMap::new();
        for id in ids(&html) {
            *seen.entry(id).or_default() += 1;
        }
        let duplicated: Vec<_> = seen.iter().filter(|(_, &n)| n > 1).map(|(k, _)| k).collect();
        assert!(
            duplicated.is_empty(),
            "{name}: these ids appear more than once: {duplicated:?}"
        );
    }
}

#[test]
fn showing_a_language_never_sets_a_display_value() {
    // The language switch hides what is not the current language; it must never
    // *show* by naming a display, because the value it names is not the one the
    // component's own class asked for. Twice a component lost its layout that
    // way, and once it lost the hiding altogether, to a class of higher
    // specificity. A rule that only ever hides cannot do either.
    for name in PAGES {
        let html = page(name);
        for line in html.lines() {
            let rule = line.trim();
            if !rule.contains("data-lang") || !rule.contains("display:") {
                continue;
            }
            assert!(
                rule.contains("display: none"),
                "{name}: a language rule sets a display other than none, \
                 which overrides whatever layout the component's class gives it: {rule}"
            );
        }
    }
}

#[test]
fn the_language_switch_outranks_a_plain_class() {
    // Specificity, stated as an assertion rather than left to be rediscovered:
    // the hiding selector carries two attributes plus an element, so no
    // single-class rule can quietly outrank it and show both languages.
    for name in PAGES {
        let html = page(name);
        assert!(
            html.contains(r#"html[data-show="en"] [data-lang]:not([data-lang="en"])"#)
                && html.contains(r#"html[data-show="zh-tw"] [data-lang]:not([data-lang="zh-tw"])"#),
            "{name}: the language switch is not the hide-the-others form"
        );
        assert!(
            !html.contains("[data-lang] { display: none; }"),
            "{name}: the weak hiding rule is back; a class rule will beat it"
        );
    }
}

#[test]
fn the_page_starts_in_a_known_language() {
    // Before any script runs. Otherwise the first paint shows both languages,
    // or neither, depending on which way the rules fall.
    for name in PAGES {
        assert!(
            page(name).contains(r#"<html lang="en" data-show="en">"#),
            "{name}: the document does not declare a starting language"
        );
    }
}

#[test]
fn every_image_the_pages_reference_is_actually_there() {
    // A gallery whose images 404 is worse than no gallery: the page still
    // claims the program runs on three platforms, and shows nothing.
    for name in PAGES {
        let html = page(name);
        for src in attribute_values(&html, "src=\"") {
            if src.starts_with("http") {
                continue;
            }
            let path = repo_root().join("docs").join(&src);
            assert!(path.exists(), "{name}: references {src}, which does not exist");
        }
    }
}

#[test]
fn the_gallery_shows_all_three_platforms() {
    // The claim the page makes is "macOS, Windows and Linux". A reader is
    // entitled to see each one rather than take it.
    let html = page("docs/index.html");
    for os in ["macOS", "Windows", "Linux"] {
        let chip = format!("<span class=\"os\">{os}</span>");
        assert!(
            html.matches(&chip).count() >= 4,
            "the gallery shows fewer than two screenshots of {os} \
             (each appears twice, once per language)"
        );
    }
}

/// The version in the READMEs' titles is the version being built.
///
/// A number written by hand goes stale the moment it is not: the status line
/// in both files said 0.6.9 for twenty-eight releases, and the pages said it
/// too until someone read them. This makes the release bump the title.
#[test]
fn both_readmes_name_the_version_that_is_being_built() {
    let root = repo_root();
    let manifest = std::fs::read_to_string(root.join("Cargo.toml")).expect("Cargo.toml");
    let version = manifest
        .lines()
        .find_map(|line| line.strip_prefix("version = \""))
        .and_then(|rest| rest.split('"').next())
        .expect("a workspace version");

    for name in ["README.md", "README_zh-TW.md"] {
        let text = std::fs::read_to_string(root.join(name)).unwrap_or_else(|e| panic!("{name}: {e}"));
        let title = text.lines().next().unwrap_or_default();
        assert_eq!(
            title,
            format!("# jt-filework v{version}"),
            "{name}'s title does not name the version being built"
        );
        assert!(
            text.contains(version),
            "{name} does not mention {version} in its status line either"
        );
    }
}
