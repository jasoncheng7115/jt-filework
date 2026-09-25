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
        let duplicated: Vec<_> = seen
            .iter()
            .filter(|(_, &n)| n > 1)
            .map(|(k, _)| k)
            .collect();
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
            assert!(
                path.exists(),
                "{name}: references {src}, which does not exist"
            );
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
        let text =
            std::fs::read_to_string(root.join(name)).unwrap_or_else(|e| panic!("{name}: {e}"));
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

/// The notice for an unsigned build, read out of `docs/SIGNING_RUNBOOK.md`
/// section 5 in one language.
fn unsigned_notice(heading: &str) -> String {
    let runbook = fs::read_to_string(repo_root().join("docs/SIGNING_RUNBOOK.md"))
        .unwrap_or_else(|e| panic!("cannot read the signing runbook: {e}"));
    let section = runbook
        .split("## 5. When a Build Is Not Signed")
        .nth(1)
        .unwrap_or_else(|| panic!("SIGNING_RUNBOOK.md has no section 5"));
    let block = section
        .split(heading)
        .nth(1)
        .unwrap_or_else(|| panic!("section 5 has no {heading}"));
    block
        .split("\n### ")
        .next()
        .unwrap_or("")
        .trim()
        .to_string()
}

/// Both READMEs carry the unsigned-build notice word for word.
///
/// `docs/RELEASE_CHECKLIST.md` requires the text verbatim wherever the files
/// are offered, and the READMEs are where most people meet them. Nothing
/// checked it: the READMEs kept a shorter first draft after section 5 grew
/// into step-by-step instructions, so the page people read and the notice
/// the release carries said different things.
#[test]
fn both_readmes_carry_the_unsigned_notice_word_for_word() {
    for (readme, heading) in [
        ("README.md", "### English"),
        ("README_zh-TW.md", "### 中文"),
    ] {
        let text = fs::read_to_string(repo_root().join(readme))
            .unwrap_or_else(|e| panic!("cannot read {readme}: {e}"));
        let notice = unsigned_notice(heading);
        assert!(!notice.is_empty(), "section 5 {heading} is empty");
        assert!(
            text.contains(&notice),
            "{readme} does not carry SIGNING_RUNBOOK.md section 5 {heading} verbatim"
        );
    }
}

/// Whether a character is Chinese text or the punctuation that goes with it.
fn is_cjk(c: char) -> bool {
    matches!(u32::from(c),
        0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xF900..=0xFAFF | 0x3000..=0x303F | 0xFF00..=0xFFEF)
}

/// Whether the space a line break becomes would show up in Chinese text.
///
/// Between two Chinese characters, always. After full-width punctuation, too,
/// whatever follows: 回收筒、`V` 檢視 is right and 回收筒、 `V` 檢視 is not.
/// Not after a closing quotation mark, which English quoting Chinese uses
/// with a space after it: 「照片佔了 40 GB」 is now a question.
fn break_shows(before: char, after: char) -> bool {
    const SPACED: &str = "。，、：；！？）．";
    (is_cjk(before) && is_cjk(after)) || SPACED.contains(before) || SPACED.contains(after)
}

/// The page with the parts whose whitespace is kept as written blanked out:
/// `<pre>`, `<script>` and `<style>`. Their line breaks stay, so a line
/// number counted in what is left is still the line in the file.
fn without_raw_blocks(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut rest = html;
    'outer: while !rest.is_empty() {
        for tag in ["pre", "script", "style"] {
            if rest.starts_with(&format!("<{tag}")) {
                let close = format!("</{tag}>");
                let end = rest.find(&close).map_or(rest.len(), |at| at + close.len());
                out.extend(
                    rest[..end]
                        .chars()
                        .map(|c| if c == '\n' { '\n' } else { ' ' }),
                );
                rest = &rest[end..];
                continue 'outer;
            }
        }
        let mut chars = rest.chars();
        if let Some(c) = chars.next() {
            out.push(c);
        }
        rest = chars.as_str();
    }
    out
}

/// A line break in the source never falls between two pieces of Chinese.
///
/// A browser shows a line break in the source as a space. In English that is
/// the space the words needed anyway; in Chinese there should be none, and
/// both pages were full of them - 「目前還沒有簽章， 所以」, eighty in all -
/// wherever a long sentence had been wrapped to fit the editor.
#[test]
fn chinese_text_is_not_broken_across_source_lines() {
    const INLINE: &[&str] = &["a", "strong", "b", "em", "i", "code", "kbd", "span"];
    let mut found = Vec::new();
    for name in PAGES {
        let html = without_raw_blocks(&page(name));
        let chars: Vec<char> = html.chars().collect();
        for (at, &c) in chars.iter().enumerate() {
            if c != '\n' {
                continue;
            }
            let before = chars[..at].iter().rev().find(|c| **c != ' ' && **c != '\t');
            let mut next = at + 1;
            while next < chars.len() && chars[next].is_whitespace() {
                next += 1;
            }
            // Through an opening inline tag to the text inside it.
            if chars.get(next) == Some(&'<') {
                let tag: String = chars[next + 1..]
                    .iter()
                    .take_while(|c| c.is_ascii_alphanumeric())
                    .collect();
                if !INLINE.contains(&tag.as_str()) {
                    continue;
                }
                while next < chars.len() && chars[next] != '>' {
                    next += 1;
                }
                next += 1;
            }
            if let (Some(&b), Some(&a)) = (before, chars.get(next)) {
                if break_shows(b, a) {
                    let line = chars[..at].iter().filter(|c| **c == '\n').count() + 1;
                    found.push(format!("{name}:{line}  {b}⏎{a}"));
                }
            }
        }
    }
    assert!(
        found.is_empty(),
        "Chinese broken across source lines, which a browser shows as a space:\n{}",
        found.join("\n")
    );
}

/// Every Markdown file in the repository, outside build output and hidden
/// directories. Walked with a list rather than recursion, and without
/// following links, so a symlink loop is a file it skips rather than a hang.
fn markdown_files() -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut pending = vec![repo_root()];
    while let Some(dir) = pending.pop() {
        let entries = fs::read_dir(&dir).unwrap_or_else(|e| panic!("{}: {e}", dir.display()));
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') || name == "target" || name == "node_modules" {
                continue;
            }
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            let path = entry.path();
            if kind.is_dir() {
                pending.push(path);
            } else if kind.is_file()
                && path
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("md"))
            {
                found.push(path);
            }
        }
    }
    found.sort();
    found
}

/// How deep in `>` a Markdown line is quoted, and the line without them.
fn unquoted(line: &str) -> (usize, &str) {
    let mut depth = 0;
    let mut rest = line;
    while let Some(after) = rest.trim_start().strip_prefix('>') {
        depth += 1;
        rest = after.strip_prefix(' ').unwrap_or(after);
    }
    (depth, rest)
}

/// Whether a Markdown line begins a block of its own - a heading, a list
/// item, a table, a fence, a quote, HTML - rather than carrying on the
/// paragraph above it. Only a line that carries on is joined by a space.
fn starts_block(body: &str) -> bool {
    let text = body.trim_start();
    let ordered = text
        .find(|c: char| !c.is_ascii_digit())
        .is_some_and(|at| at > 0 && (text[at..].starts_with(". ") || text[at..].starts_with(") ")));
    text.is_empty()
        || ordered
        || text.starts_with(['#', '|', '<', '>'])
        || ["```", "~~~", "---", "===", "- ", "* ", "+ "]
            .iter()
            .any(|opener| text.starts_with(opener))
        || (text.starts_with('[') && text.contains("]:"))
}

/// The same rule as the pages above, for Markdown, which GitHub renders the
/// same way: a line break inside a paragraph becomes a space.
///
/// The Chinese README, the Chinese changelog and the Chinese half of the
/// signing notice were all wrapped at eighty columns like the English, 404
/// breaks in all, so the page GitHub shows - and every release note built
/// from those files - read 「會跟著游標所在的東西 換」.
#[test]
fn chinese_markdown_is_not_broken_across_source_lines() {
    const MARKUP: &[char] = &['*', '_', '`', '~'];
    let root = repo_root();
    let mut found = Vec::new();
    for path in markdown_files() {
        let text = fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let lines: Vec<&str> = text.lines().collect();
        let mut fenced = false;
        for (at, pair) in lines.windows(2).enumerate() {
            let (depth, body) = unquoted(pair[0]);
            let (next_depth, next_body) = unquoted(pair[1]);
            let fence =
                |b: &str| b.trim_start().starts_with("```") || b.trim_start().starts_with("~~~");
            if fence(body) {
                fenced = !fenced;
            }
            if fenced
                || depth != next_depth
                || starts_block(next_body)
                || body.ends_with("  ")
                || body.ends_with('\\')
            {
                continue;
            }
            let before = body.trim_end().trim_end_matches(MARKUP).chars().next_back();
            let after = next_body
                .trim_start()
                .trim_start_matches(MARKUP)
                .trim_start_matches('[')
                .chars()
                .next();
            if let (Some(b), Some(a)) = (before, after) {
                if break_shows(b, a) {
                    let name = path.strip_prefix(&root).unwrap_or(&path).display();
                    found.push(format!("{name}:{}  {b}⏎{a}", at + 1));
                }
            }
        }
    }
    assert!(
        found.is_empty(),
        "Chinese broken across Markdown source lines, which GitHub shows as a space:\n{}",
        found.join("\n")
    );
}
