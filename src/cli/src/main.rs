//! A headless walkthrough of everything the core can currently do.
//!
//! There is no GUI yet: the stack is undecided (ADR-0001), and `AGENTS.md` §4
//! forbids building core logic against one before that decision. So this is
//! how you *see* the core work rather than only read that 192 tests pass.
//!
//! It is deliberately honest about what does not exist. Nothing here fakes a
//! filesystem, a viewer or a search.
//!
//! ```text
//! cargo run -p jtf-cli
//! cargo run -p jtf-cli -- --locale zh-TW
//! ```

#![allow(clippy::print_stdout, clippy::expect_used, clippy::unwrap_used)]

use std::collections::BTreeSet;
use std::env;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

use jtf_commands::{CommandBus, CommandId, CommandRegistry, KeyChord, Keymap};
use jtf_core::i18n::{Args, Catalog, LocaleId, Localizer};
use jtf_core::theme::{Palette, ResolvedTheme, SystemAppearance, ThemeMode, ThemeToken};
use jtf_core::{ErrorCode, Location};
use jtf_jobs::{Job, JobId, JobKind, Progress};
use jtf_workspace::{
    LayoutPreset, Orientation, Restored, Session, SessionSettings, SortKey, Workspace,
    WorkspaceNode,
};

fn main() {
    let locale = parse_locale();
    let localizer = load_localizer(&locale);

    banner(&localizer, &locale);
    section("1", "Workspace — a recursive split tree, not two panes");
    let mut workspace = demo_layout(&localizer);

    section("2", "Tabs — owned by a pane, carried when moved");
    demo_tabs(&mut workspace);

    section("3", "Selection and marks are different things");
    demo_selection_and_marks(&mut workspace);

    section("4", "Session memory — and the switch that really forgets");
    demo_session(&workspace);

    section("5", "Runtime locale switch, with no loss of state");
    demo_locale_switch(&workspace);

    section("6", "Theme tokens resolve, and stay legible in both");
    demo_theme();

    section("7", "Commands: keymap resolves to an id, the bus runs it");
    demo_commands(&localizer);

    section("8", "Jobs: progress, conflict, cancellation");
    demo_jobs(&localizer);

    section("9", "What does not exist yet");
    not_yet();
}

// ---------------------------------------------------------------- scaffolding

fn parse_locale() -> LocaleId {
    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--locale" {
            if let Some(value) = args.next() {
                return LocaleId::new(value);
            }
        }
    }
    LocaleId::english()
}

fn repo_root() -> PathBuf {
    // src/cli -> src -> repo
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("crate lives at <repo>/src/cli")
        .to_path_buf()
}

fn load_catalog(locale: &LocaleId) -> Catalog {
    let dir = repo_root().join("locales").join(locale.as_str());
    let mut catalog = Catalog::new(locale.clone());
    let Ok(entries) = fs::read_dir(&dir) else {
        return catalog;
    };
    let mut files: Vec<_> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "catalog"))
        .collect();
    files.sort();
    for file in files {
        let text = fs::read_to_string(&file).expect("read catalogue");
        let parsed = Catalog::parse(locale.clone(), &text).expect("parse catalogue");
        catalog.merge(parsed).expect("merge catalogue");
    }
    catalog
}

fn load_localizer(locale: &LocaleId) -> Localizer {
    Localizer::new(load_catalog(locale), load_catalog(&LocaleId::english()))
}

fn section(number: &str, title: &str) {
    println!("\n\x1b[1m{number}. {title}\x1b[0m");
    println!("{}", "-".repeat(66));
}

fn banner(localizer: &Localizer, locale: &LocaleId) {
    println!("\x1b[1mJT FileWork — core walkthrough\x1b[0m");
    println!(
        "locale: {locale}   fallback: {}",
        localizer.fallback_locale()
    );
    println!("This exercises the real crates. No GUI exists yet (ADR-0001).");
}

/// Render the split tree, so "recursive" is something you can see.
fn draw(node: &WorkspaceNode, workspace: &Workspace, indent: usize, out: &mut String) {
    let pad = "  ".repeat(indent);
    match node {
        WorkspaceNode::Pane { id } => {
            let pane = workspace.pane(*id).expect("pane in tree exists");
            let active = if workspace.active_pane_id() == *id {
                " *active"
            } else {
                ""
            };
            let tabs: Vec<String> = pane
                .tabs()
                .iter()
                .enumerate()
                .map(|(i, t)| {
                    let name = t
                        .location()
                        .file_name()
                        .map_or_else(|| "/".to_string(), |n| n.to_string_lossy().into_owned());
                    let marks = t.marks().len();
                    let marker = if i == pane.active_index() { "[" } else { " " };
                    let close = if i == pane.active_index() { "]" } else { " " };
                    if marks > 0 {
                        format!("{marker}{name} ({marks} marked){close}")
                    } else {
                        format!("{marker}{name}{close}")
                    }
                })
                .collect();
            let _ = writeln!(out, "{pad}Pane {}{active}  {}", id.get(), tabs.join(" "));
        }
        WorkspaceNode::Split {
            orientation,
            ratio,
            first,
            second,
            ..
        } => {
            let word = match orientation {
                Orientation::Horizontal => "Split horizontal",
                Orientation::Vertical => "Split vertical",
            };
            let _ = writeln!(out, "{pad}{word}  ratio {ratio:.2}");
            draw(first, workspace, indent + 1, out);
            draw(second, workspace, indent + 1, out);
        }
    }
}

fn show(workspace: &Workspace) {
    let mut out = String::new();
    draw(workspace.root(), workspace, 0, &mut out);
    print!("{out}");
}

// ------------------------------------------------------------------- sections

fn demo_layout(localizer: &Localizer) -> Workspace {
    let mut workspace = Workspace::new(Location::local("/Users/you"));
    println!("one pane, one tab:");
    show(&workspace);

    workspace.apply_preset(LayoutPreset::Quad);
    println!(
        "\nafter the 2x2 preset ({}):",
        localizer.text_or_key("workspace.split.horizontal")
    );
    show(&workspace);

    workspace.focus_pane(workspace.pane_order()[3]);
    workspace.split_active(Orientation::Vertical);
    println!("\nsplitting one of those panes again — depth is unbounded:");
    show(&workspace);
    println!(
        "\ndepth {}, {} panes",
        workspace.root().depth(),
        workspace.pane_count()
    );
    workspace
}

fn demo_tabs(workspace: &mut Workspace) {
    workspace.focus_pane(workspace.pane_order()[0]);
    let project = workspace.new_tab(Location::local("/Users/you/project"));
    workspace.new_tab(Location::local("/Volumes/NAS/media"));

    {
        let tab = workspace.active_pane_mut().tab_mut(project).unwrap();
        tab.navigate_to(Location::local("/Users/you/project/src"));
        tab.sort_by(SortKey::Modified);
        tab.filter_mut().text = "*.rs".to_string();
        tab.marks_mut()
            .mark(Location::local("/Users/you/project/src/main.rs"));
    }

    let from = workspace.pane_order()[0];
    let to = workspace.pane_order()[2];
    println!("moving a tab from pane {} to pane {}", from.get(), to.get());

    let before = workspace.pane(from).unwrap().tab(project).unwrap().clone();
    workspace
        .move_tab_to_pane(from, project, to)
        .expect("move tab");
    let after = workspace.pane(to).unwrap().tab(project).unwrap();

    println!("state identical after the move: {}", &before == after);
    println!(
        "  location {}, history {}, sort {:?}, filter {:?}, marks {}",
        after.location().as_path().unwrap().display(),
        after.back_history().len(),
        after.sort().key,
        after.filter().text,
        after.marks().len(),
    );
    show(workspace);
}

fn demo_selection_and_marks(workspace: &mut Workspace) {
    workspace.focus_pane(workspace.pane_order()[0]);
    let tab = workspace.active_tab_mut().expect("active tab");

    tab.marks_mut().mark(Location::local("/Users/you/a.log"));
    tab.marks_mut().mark(Location::local("/Users/you/b.log"));
    tab.selection_mut()
        .select_only(Location::local("/Users/you/c.txt"));
    tab.set_active_entry(Some(Location::local("/Users/you/d.txt")));

    println!(
        "marks {}, selection {}",
        tab.marks().len(),
        tab.selection().len()
    );
    let target = tab.operation_target();
    println!(
        "an operation would act on: {} -> {:?}",
        target.source_key(),
        target.locations().len()
    );

    tab.selection_mut().clear();
    println!(
        "after clearing the selection, marks are still {}",
        tab.marks().len()
    );

    tab.navigate_to(Location::local("/Users/you/elsewhere"));
    println!(
        "after navigating away: selection {}, marks {}",
        tab.selection().len(),
        tab.marks().len()
    );
}

fn demo_session(workspace: &Workspace) {
    let home = Location::local("/Users/you");

    let json = Session::capture(workspace, SessionSettings::remembering())
        .to_json()
        .expect("encode");
    let Restored {
        workspace: back,
        outcome,
        ..
    } = Session::restore(Some(&json), &home);
    println!("remembering:  {outcome:?}");
    println!("  identical after a round trip: {}", &back == workspace);
    println!(
        "  {} panes, {} marked entries restored",
        back.pane_count(),
        back.total_marked()
    );

    let off = Session::capture(workspace, SessionSettings::forgetting())
        .to_json()
        .expect("encode");
    let Restored {
        workspace: fresh,
        outcome,
        settings,
    } = Session::restore(Some(&off), &home);
    println!("\nforgetting:   {outcome:?}");
    println!(
        "  starts with {} pane, at {}",
        fresh.pane_count(),
        fresh
            .active_tab()
            .unwrap()
            .location()
            .as_path()
            .unwrap()
            .display()
    );
    println!(
        "  preference itself survives: {:?}",
        settings.restore_on_launch
    );
    println!(
        "  stored bytes mention 'project': {}",
        off.contains("project")
    );

    let broken = Session::restore(Some("{ truncated"), &home);
    println!("\ncorrupt file: {:?}", broken.outcome);
    println!("  notice key: {:?}", broken.outcome.notice_key());
    println!(
        "  fallback workspace is sound: {}",
        broken.workspace.invariants_hold()
    );
}

fn demo_locale_switch(workspace: &Workspace) {
    let en = load_localizer(&LocaleId::english());
    let tw = load_localizer(&LocaleId::new(LocaleId::ZH_TW));

    for key in [
        "command.file.trash",
        "command.tab.new",
        "jobs.state.waiting_for_user",
        "column.modified",
    ] {
        println!(
            "{key:<32} en: {:<20} zh-TW: {}",
            en.text_or_key(key),
            tw.text_or_key(key)
        );
    }

    let mut switched = workspace.clone();
    switched.set_locale(LocaleId::new(LocaleId::ZH_TW));
    println!(
        "\nlayout, tabs and marks unchanged by the switch: {}",
        switched.root() == workspace.root() && switched.total_marked() == workspace.total_marked()
    );
}

fn demo_theme() {
    for (mode, system) in [
        (ThemeMode::System, SystemAppearance::Dark),
        (ThemeMode::System, SystemAppearance::Light),
        (ThemeMode::Light, SystemAppearance::Dark),
    ] {
        println!("{mode:?} + system {system:?} -> {:?}", mode.resolve(system));
    }

    println!();
    for theme in [ResolvedTheme::Light, ResolvedTheme::Dark] {
        let palette = Palette::for_theme(theme);
        let text = palette.color(ThemeToken::TextPrimary);
        let pane = palette.color(ThemeToken::SurfacePane);
        let mark = palette.color(ThemeToken::MarkActive);
        let selection = palette.color(ThemeToken::SelectionActive);
        let name = format!("{theme:?}");
        println!(
            "{name:<6} text/pane {:.2}:1 (AA needs 4.5)   mark vs selection {:.2}:1",
            text.contrast_ratio(pane),
            mark.contrast_ratio(selection),
        );
    }
}

fn demo_commands(localizer: &Localizer) {
    let registry = CommandRegistry::baseline();
    println!("{} commands registered", registry.len());

    let keymap = Keymap::parse(
        "demo",
        "primary+t = tab.new\nprimary+shift+d = workspace.split.vertical\nf8 = file.trash\n",
    )
    .expect("keymap");

    let mut bus = CommandBus::new(registry);
    for id in ["tab.new", "workspace.split.vertical", "file.trash"] {
        bus.set_handler(id, Box::new(|| Ok(())))
            .expect("known command");
    }

    for chord_text in ["primary+t", "f8"] {
        let chord = KeyChord::parse(chord_text).expect("chord");
        let id = keymap.resolve(&chord).expect("bound").clone();
        let label = localizer.text_or_key(bus.registry().get(&id).expect("registered").label_key());
        bus.dispatch(&id).expect("dispatch");
        println!("{chord_text:<16} -> {id:<28} {label}");
    }

    let unknown = bus.dispatch(&CommandId::new("tab.nwe"));
    println!("\na typo is a typed error, not a panic: {unknown:?}");
    println!("commands with no handler yet: {}", bus.unhandled().len());
}

fn demo_jobs(localizer: &Localizer) {
    let mut job = Job::new(JobId::new(1), JobKind::Copy);
    let token = job.token();
    println!(
        "{} -> {}",
        job.id(),
        localizer.text_or_key(job.kind().label_key())
    );

    job.set_progress(Progress::with_total(1000));
    job.start().expect("start");

    for step in [250_u64, 500, 750] {
        job.set_progress(job.progress().set_completed(step));
        print!("  {:>4}/1000", job.progress().completed());
    }
    println!();

    job.wait_for_user().expect("conflict");
    println!(
        "  conflict: {} ({:?})",
        localizer.text_or_key(job.state().label_key()),
        job.state()
    );
    job.start().expect("resume");
    job.complete().expect("complete");
    println!(
        "  {} at {}/1000",
        localizer.text_or_key(job.state().label_key()),
        job.progress().completed()
    );
    println!(
        "  a completed job never renders at 97%: {}",
        job.progress().is_full()
    );

    let mut cancellable = Job::new(JobId::new(2), JobKind::Search);
    let watcher = cancellable.token();
    cancellable.start().expect("start");
    cancellable.cancel().expect("cancel");
    println!(
        "\n  cancelling a search: worker sees the signal {} , state {:?}",
        watcher.is_cancelled(),
        cancellable.state()
    );
    println!(
        "  a terminal job cannot restart: {}",
        cancellable.start().is_err()
    );
    println!(
        "  the first job's token is untouched: {}",
        !token.is_cancelled()
    );

    let error = jtf_core::Error::new(ErrorCode::PermissionDenied, "read-only volume");
    println!("\n  an error carries a stable code and a separate localized message:");
    println!("    code    {}", error.code());
    println!("    message {}", localizer.text_or_key(error.message_key()));

    let counted = localizer
        .format("target.marked", &Args::new().with("count", "12"))
        .unwrap_or_else(|_| "?".into());
    println!("    with a placeholder: {counted}");
}

fn not_yet() {
    let done: BTreeSet<&str> = [
        "workspace split tree",
        "panes and per-pane tabs",
        "selection and marking",
        "session memory",
        "i18n catalogues (en, zh-TW)",
        "theme tokens",
        "command registry, keymap, command bus",
        "job state machine, progress, cancellation",
    ]
    .into_iter()
    .collect();

    let missing = [
        (
            "filesystem provider",
            "nothing here has touched a real directory",
        ),
        (
            "async enumeration",
            "no incremental rows, no cancellation of a scan",
        ),
        (
            "file operations",
            "no copy, move, rename, trash — the job engine has no work",
        ),
        (
            "viewers and preview",
            "no text, image, hex or archive viewer",
        ),
        ("search", "no query parser, no scanner, no index"),
        (
            "platform adapters",
            "no Quick Look, no drag and drop, no trash",
        ),
        ("AI providers", "no Claude Code or Codex CLI integration"),
        (
            "the UI",
            "blocked on ADR-0001 — the GUI stack is not chosen",
        ),
    ];

    println!("working, and exercised above:");
    for item in &done {
        println!("  + {item}");
    }
    println!("\nnot built yet:");
    for (item, why) in missing {
        println!("  - {item:<22} {why}");
    }
    println!(
        "\nThe next decision is ADR-0001. Until a GUI stack is chosen, there is\n\
         deliberately no window to open (AGENTS.md 4)."
    );
}
