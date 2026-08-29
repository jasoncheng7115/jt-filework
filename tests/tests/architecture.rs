//! Architectural boundary tests — `docs/TESTING.md` §3.2.
//!
//! `AGENTS.md` §4, §5 and §6 are rules that a reviewer can miss and a
//! deadline can erode. These tests make them fail the build instead.

// A test asserts by panicking, so the workspace's unwrap/expect/panic lints
// are exactly backwards here: an `unwrap` that fails *is* the failure report.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::{Path, PathBuf};

/// Repository root, from this crate's manifest directory.
fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf()
}

/// Every crate directory under `src/`.
fn crate_dirs() -> Vec<PathBuf> {
    let mut found = Vec::new();
    collect_crates(&repo_root().join("src"), &mut found);
    found.sort();
    assert!(!found.is_empty(), "no crates found; did the layout change?");
    found
}

fn collect_crates(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        if path.join("Cargo.toml").is_file() {
            out.push(path.clone());
        }
        if path
            .file_name()
            .is_some_and(|n| n == "src" || n == "target")
        {
            continue;
        }
        collect_crates(&path, out);
    }
}

/// Rust source files inside a crate's own `src/` directory.
///
/// Test directories are excluded: a test may legitimately need a
/// platform-gated case, and `docs/TESTING.md` §5 asks such gates to be
/// explicit and justified rather than forbidden.
fn rust_sources(crate_dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    collect_rust(&crate_dir.join("src"), &mut out);
    out.sort();
    out
}

fn collect_rust(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_rust(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

/// Source text with comments removed.
///
/// Needed because these tests scan for the very strings the rules forbid, and
/// the modules that implement those rules quote them in their documentation.
/// A doc comment explaining "there is no `left_pane`" must not be read as a
/// `left_pane`.
fn code_only(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    let mut in_block = false;
    while let Some(c) = chars.next() {
        if in_block {
            if c == '*' && chars.peek() == Some(&'/') {
                chars.next();
                in_block = false;
            }
            continue;
        }
        if c == '/' {
            match chars.peek() {
                Some('/') => {
                    for next in chars.by_ref() {
                        if next == '\n' {
                            out.push('\n');
                            break;
                        }
                    }
                    continue;
                }
                Some('*') => {
                    chars.next();
                    in_block = true;
                    continue;
                }
                _ => {}
            }
        }
        out.push(c);
    }
    out
}

fn crate_name(crate_dir: &Path) -> String {
    let manifest = fs::read_to_string(crate_dir.join("Cargo.toml")).unwrap();
    let Some(name) = manifest
        .lines()
        .find_map(|l| l.trim().strip_prefix("name = "))
    else {
        panic!("no name in {}", crate_dir.display())
    };
    name.trim().trim_matches('"').to_string()
}

/// Whether a crate is a platform adapter, i.e. lives under `src/platform/`
/// but is not the trait crate itself.
fn is_platform_adapter(crate_dir: &Path) -> bool {
    let platform = repo_root().join("src").join("platform");
    crate_dir.starts_with(&platform) && crate_dir != platform
}

/// Whether a crate is the UI layer.
fn is_ui(crate_dir: &Path) -> bool {
    crate_dir == repo_root().join("src").join("ui")
}

/// Dependency names declared in a manifest, however the entry is written.
fn declared_dependencies(crate_dir: &Path) -> Vec<String> {
    let manifest = fs::read_to_string(crate_dir.join("Cargo.toml")).unwrap();
    let mut deps = Vec::new();
    let mut in_deps = false;
    for line in manifest.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            in_deps = trimmed.contains("dependencies");
            continue;
        }
        if !in_deps || trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if let Some((name, _)) = trimmed.split_once('=') {
            deps.push(name.trim().trim_matches('"').to_string());
        }
    }
    deps
}

/// Crates that must never reach a GUI toolkit (`AGENTS.md` §4).
const FORBIDDEN_GUI: &[&str] = &[
    "qt",
    "qttypes",
    "cxx-qt",
    "cxx-qt-lib",
    "qmetaobject",
    "slint",
    "slint-build",
    "gtk",
    "gtk4",
    "gtk-sys",
    "iced",
    "egui",
    "eframe",
    "druid",
    "tauri",
    "wry",
    "webkit2gtk",
    "muda",
    "winit",
    "dioxus",
    "fltk",
    "azul",
    "makepad-widgets",
];

/// Crates that reach a platform SDK, allowed only in platform adapters
/// (`AGENTS.md` §5).
const FORBIDDEN_PLATFORM: &[&str] = &[
    "objc",
    "objc2",
    "cocoa",
    "core-foundation",
    "core-foundation-sys",
    "core-graphics",
    "windows",
    "windows-sys",
    "winapi",
    "x11",
    "x11rb",
    "wayland-client",
    "zbus",
    "dbus",
    "gio",
    "glib",
    "block2",
    "dispatch",
];

#[test]
fn core_has_no_gui_dependency() {
    for dir in crate_dirs() {
        if is_ui(&dir) {
            continue;
        }
        let deps = declared_dependencies(&dir);
        for forbidden in FORBIDDEN_GUI {
            assert!(
                !deps.iter().any(|d| d == forbidden),
                "{} depends on the GUI toolkit crate `{forbidden}`. AGENTS.md 4: core logic \
                 must not depend on a GUI framework, so that the framework stays replaceable.",
                crate_name(&dir),
            );
        }
    }
}

#[test]
fn core_has_no_platform_sdk_dependency() {
    for dir in crate_dirs() {
        if is_platform_adapter(&dir) || is_ui(&dir) {
            continue;
        }
        let deps = declared_dependencies(&dir);
        for forbidden in FORBIDDEN_PLATFORM {
            assert!(
                !deps.iter().any(|d| d == forbidden),
                "{} depends on the platform crate `{forbidden}`. AGENTS.md 5: platform code \
                 lives in src/platform/<os>, not in shared crates.",
                crate_name(&dir),
            );
        }
    }
}

#[test]
fn no_crate_depends_on_the_ui_layer() {
    for dir in crate_dirs() {
        if is_ui(&dir) {
            continue;
        }
        assert!(
            !declared_dependencies(&dir).iter().any(|d| d == "jtf-ui"),
            "{} depends on jtf-ui. The UI consumes commands, models and service contracts; \
             nothing consumes the UI (AGENTS.md 4).",
            crate_name(&dir),
        );
    }
}

/// Files outside the platform layer that may use a platform `cfg`, each with
/// the reason it has not moved yet.
///
/// The list exists so an exception is a deliberate, reviewable entry rather
/// than a habit. Every line is also a `TODO.md` item.
const CFG_ALLOWLIST: &[(&str, &str)] = &[(
    "src/ops/src/run.rs",
    "creating a symbolic link has no portable API; moves to the platform \
     adapter in Phase 4 together with Windows privilege handling",
)];

#[test]
fn no_target_os_cfg_outside_the_platform_layer() {
    let needles = [
        "cfg(target_os",
        "cfg(windows",
        "cfg(unix",
        "cfg(target_family",
    ];
    for dir in crate_dirs() {
        if is_platform_adapter(&dir) {
            continue;
        }
        for file in rust_sources(&dir) {
            let relative = file.strip_prefix(repo_root()).unwrap_or(&file);
            let relative_str = relative.to_string_lossy().replace('\\', "/");
            if CFG_ALLOWLIST.iter().any(|(path, _)| *path == relative_str) {
                continue;
            }
            let text = code_only(&fs::read_to_string(&file).unwrap());
            for needle in needles {
                assert!(
                    !text.contains(needle),
                    "{} contains `{needle}`. AGENTS.md 5: platform checks belong in \
                     src/platform/<os>, not scattered through unrelated modules.",
                    file.strip_prefix(repo_root()).unwrap_or(&file).display(),
                );
            }
        }
    }
}

#[test]
fn the_dual_pane_model_cannot_be_reintroduced() {
    // AGENTS.md 6 names this failure mode explicitly, so the test names it too.
    let banned = [
        "left_pane",
        "right_pane",
        "leftPane",
        "rightPane",
        "LeftPane",
        "RightPane",
    ];
    for dir in crate_dirs() {
        for file in rust_sources(&dir) {
            let text = code_only(&fs::read_to_string(&file).unwrap());
            for needle in banned {
                assert!(
                    !text.contains(needle),
                    "{} contains `{needle}`. AGENTS.md 6: the workspace is a recursive split \
                     tree; there is no left pane and no right pane.",
                    file.strip_prefix(repo_root()).unwrap_or(&file).display(),
                );
            }
        }
    }
}

#[test]
fn colour_values_exist_only_in_the_theme_module() {
    // docs/TESTING.md 3.4. Semantic tokens are the interface; literal colours
    // are an implementation detail of exactly one file.
    let theme = repo_root().join("src/core/src/theme.rs");
    for dir in crate_dirs() {
        for file in rust_sources(&dir) {
            if file == theme {
                continue;
            }
            let text = code_only(&fs::read_to_string(&file).unwrap());
            for marker in ["Color::rgb(", "Color::rgba("] {
                assert!(
                    !text.contains(marker),
                    "{} constructs a colour. Use a ThemeToken instead; literal colours live \
                     only in src/core/src/theme.rs (AGENTS.md 12).",
                    file.strip_prefix(repo_root()).unwrap_or(&file).display(),
                );
            }
        }
    }
}

#[test]
fn every_crate_opts_into_the_workspace_lints() {
    for dir in crate_dirs() {
        let manifest = fs::read_to_string(dir.join("Cargo.toml")).unwrap();
        assert!(
            manifest.contains("[lints]") && manifest.contains("workspace = true"),
            "{} does not opt into [workspace.lints]. unsafe_code, unwrap_used and the rest \
             must apply everywhere, not just where someone remembered.",
            crate_name(&dir),
        );
    }
}

#[test]
fn the_layout_matches_what_adr_0002_promises() {
    let root = repo_root();
    for expected in ["src/core", "src/jobs", "src/workspace", "src/commands"] {
        assert!(
            root.join(expected).join("Cargo.toml").is_file(),
            "{expected} is missing. ADR-0002 fixes the crate layout; changing it needs a new ADR."
        );
    }
}

#[test]
fn the_comment_stripper_does_not_make_these_tests_vacuous() {
    // A rule test that silently matches nothing is worse than no test. Prove
    // the scanner still sees code, and still ignores prose about the rule.
    assert!(code_only("let x = 1; // cfg(target_os = \"macos\")").contains("let x = 1;"));
    assert!(!code_only("// cfg(target_os = \"macos\")").contains("cfg(target_os"));
    assert!(!code_only("/* left_pane */").contains("left_pane"));
    assert!(code_only("#[cfg(target_os = \"macos\")]").contains("cfg(target_os"));

    // And that there is something to scan at all.
    let files: usize = crate_dirs().iter().map(|d| rust_sources(d).len()).sum();
    assert!(
        files >= 15,
        "only {files} source files scanned; the walker is probably broken"
    );
}

#[test]
fn every_platform_cfg_exception_is_justified_and_still_needed() {
    // An allowlist that outlives the thing it excuses is how a rule rots.
    for (path, reason) in CFG_ALLOWLIST {
        let full = repo_root().join(path);
        assert!(full.is_file(), "allowlisted file no longer exists: {path}");
        assert!(
            reason.len() > 40,
            "{path}: the reason must actually explain something"
        );

        let text = code_only(&fs::read_to_string(&full).unwrap());
        assert!(
            text.contains("cfg(unix")
                || text.contains("cfg(target_os")
                || text.contains("cfg(windows"),
            "{path} no longer needs its exception; remove it from CFG_ALLOWLIST"
        );
    }
}

/// The product name is `jt-filework`, always, everywhere — `AGENTS.md` §10.1.
///
/// This is not pedantry about capitals. The name appears in the window title,
/// the bundle, the signing identity and the update feed, and a second spelling
/// in any one of them is a different product to the operating system.
#[test]
fn product_name_has_one_spelling() {
    // Assembled rather than written out, so this file is not itself an
    // offender against the rule it enforces.
    let (jt, file, work) = ("JT", "File", "Work");
    let wrong = [
        format!("{jt} {file}{work}"),
        format!("{jt}{file}{work}"),
        format!("{jt}-{file}{work}"),
        format!("Jt-{file}work"),
    ];
    let root = repo_root();
    let mut offences = Vec::new();
    walk_text_files(&root, &mut |path, text| {
        // AGENTS.md is where the rule is written down, so it necessarily
        // quotes the spellings it forbids.
        if path.file_name().is_some_and(|n| n == "AGENTS.md") {
            return;
        }
        for spelling in &wrong {
            if text.contains(spelling.as_str()) {
                offences.push(format!("{}: {spelling}", path.display()));
            }
        }
    });
    assert!(
        offences.is_empty(),
        "the product name is `jt-filework`; found:\n  {}",
        offences.join("\n  ")
    );
}

/// Every checked-in text file, skipping build output and version control.
fn walk_text_files(dir: &Path, visit: &mut impl FnMut(&Path, &str)) {
    const SKIP: &[&str] = &["target", "build", ".git", "node_modules"];
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if SKIP.contains(&name.as_ref()) {
            continue;
        }
        if path.is_dir() {
            walk_text_files(&path, visit);
        } else if let Ok(text) = fs::read_to_string(&path) {
            visit(&path, &text);
        }
    }
}

/// Every `%TOKEN%` in the Qt stylesheet has a substitution.
///
/// Qt does not report a bad declaration and carry on — one unrecognised value
/// makes it reject the *entire* stylesheet, with a single line on stderr that
/// nobody watching a GUI ever sees. The program then runs with no theme at
/// all and simply looks wrong, which is a hard symptom to trace back to a
/// missing string replacement.
///
/// This happened: `%PREVIEW%` was added to the sheet without its `.replace`,
/// and the whole theme was silently off for several commits.
#[test]
fn every_stylesheet_token_is_substituted() {
    let theme = repo_root().join("src/ui/qt6/cpp/theme.cpp");
    let text = fs::read_to_string(&theme).expect("theme.cpp is readable");

    let used: std::collections::BTreeSet<String> = token_names(&text, false);
    let substituted: std::collections::BTreeSet<String> = token_names(&text, true);

    let missing: Vec<&String> = used.difference(&substituted).collect();
    assert!(
        missing.is_empty(),
        "these appear in the stylesheet with no .replace, which makes Qt \
         reject the whole sheet: {missing:?}"
    );
}

/// `%TOKEN%` names in `text`. With `in_replace`, only those inside a
/// `QStringLiteral("%…%")`, which is how a substitution is written.
fn token_names(text: &str, in_replace: bool) -> std::collections::BTreeSet<String> {
    let mut found = std::collections::BTreeSet::new();
    let needle = if in_replace { "QStringLiteral(\"%" } else { "%" };
    let mut rest = text;
    while let Some(at) = rest.find(needle) {
        let after = &rest[at + needle.len()..];
        let Some(end) = after.find('%') else { break };
        let name = &after[..end];
        if !name.is_empty() && name.chars().all(|c| c.is_ascii_uppercase()) {
            found.insert(name.to_string());
        }
        rest = &after[end..];
        // Step past the closing % so the next search does not rematch it.
        rest = rest.get(1..).unwrap_or("");
    }
    found
}
