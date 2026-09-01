//! Every shipped keymap must load.
//!
//! This test exists because it should have existed sooner. Both presets had a
//! chord bound twice, `Keymap::parse` correctly refused them, and the loader
//! quietly fell back to an empty map — so the application ran with no
//! shortcuts at all and every menu showed a blank where its hotkey belonged.
//!
//! A shipped file that does not load is a build failure, not a runtime
//! surprise.

// A test asserts by panicking, so the workspace's unwrap/expect/panic lints
// are exactly backwards here.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::{Path, PathBuf};

use jtf_commands::{CommandId, CommandRegistry, KeyChord, Keymap};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf()
}

fn shipped_keymaps() -> Vec<PathBuf> {
    let dir = repo_root().join("keymaps");
    let mut files: Vec<PathBuf> = fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", dir.display()))
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|e| e == "keymap"))
        .collect();
    files.sort();
    assert!(!files.is_empty(), "no keymaps found in {}", dir.display());
    files
}

#[test]
fn every_shipped_keymap_parses() {
    for path in shipped_keymaps() {
        let text = fs::read_to_string(&path).unwrap();
        let name = path.file_stem().unwrap().to_string_lossy().into_owned();
        if let Err(error) = Keymap::parse(&name, &text) {
            panic!("{} does not load: {error}", path.display());
        }
    }
}

#[test]
fn every_binding_names_a_registered_command() {
    let registry = CommandRegistry::baseline();
    for path in shipped_keymaps() {
        let text = fs::read_to_string(&path).unwrap();
        let name = path.file_stem().unwrap().to_string_lossy().into_owned();
        let keymap = Keymap::parse(&name, &text).unwrap();

        if let Err(error) = keymap.validate_against(&registry) {
            panic!("{}: {error}", path.display());
        }
    }
}

#[test]
fn the_default_preset_binds_the_commands_people_reach_for_first() {
    // A preset that loads but leaves navigation unbound is not much better
    // than one that does not load at all.
    let text = fs::read_to_string(repo_root().join("keymaps/native.keymap")).unwrap();
    let keymap = Keymap::parse("native", &text).unwrap();

    for id in [
        "nav.up",
        "nav.back",
        "nav.forward",
        "file.open",
        "file.rename",
        "file.trash",
        "file.undo",
        "tab.new",
        "tab.close",
        "workspace.split.horizontal",
        "workspace.split.vertical",
        "search.open",
        "view.filter",
        "view.refresh",
        "settings.open",
    ] {
        assert!(
            !keymap
                .chords_for(&jtf_commands::CommandId::new(id))
                .is_empty(),
            "the default preset leaves {id} unbound"
        );
    }
}

#[test]
fn a_keymap_round_trips_through_its_own_format() {
    // What the settings window writes is read by the same parser, so a saved
    // customisation cannot become unreadable.
    for path in shipped_keymaps() {
        let text = fs::read_to_string(&path).unwrap();
        let name = path.file_stem().unwrap().to_string_lossy().into_owned();
        let original = Keymap::parse(&name, &text).unwrap();
        let reloaded = Keymap::parse(&name, &original.to_text()).unwrap();

        assert_eq!(reloaded.len(), original.len(), "{}", path.display());
        for (chord, command) in original.iter() {
            assert_eq!(reloaded.resolve(chord), Some(command), "{}", path.display());
        }
    }
}

/// Every shipped preset binds the mode toggle to the same chord.
///
/// This is the property that makes switching usable rather than a trap. If
/// `cview` bound the toggle to one chord and `platform` to another, switching
/// into a mode would leave you with no key to switch back — you would have to
/// find the settings dialog using a keyboard whose layout had just changed
/// under you. Both presets agreeing means the escape hatch is always in the
/// same place.
#[test]
fn the_mode_toggle_is_the_same_chord_in_every_preset() {
    let toggle = CommandId::new("keymap.toggle");
    let mut chords: Vec<(String, Vec<String>)> = Vec::new();

    for path in shipped_keymaps() {
        let name = path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        let text = fs::read_to_string(&path).expect("a shipped keymap is readable");
        let map = Keymap::parse(&name, &text).expect("a shipped keymap parses");
        let bound: Vec<String> = map
            .chords_for(&toggle)
            .into_iter()
            .map(KeyChord::to_source_text)
            .collect();
        assert!(
            !bound.is_empty(),
            "{name} does not bind keymap.toggle; switching into it would be \
             one-way"
        );
        chords.push((name, bound));
    }

    assert!(
        !chords.is_empty(),
        "no keymaps found; did the layout change?"
    );
    let (first_name, first) = &chords[0];
    for (name, bound) in &chords[1..] {
        assert_eq!(
            bound, first,
            "{name} binds keymap.toggle to {bound:?} but {first_name} binds it \
             to {first:?}; the toggle must be in the same place in every mode"
        );
    }
}

/// The key hint strip names its commands as string literals in C++, where the
/// compiler cannot check them. Two of them - `file.copy` and `file.execute` -
/// were ids that had never existed, and because a command with no shortcut is
/// skipped rather than drawn blank, they vanished silently instead of showing
/// up as a gap anyone would notice.
#[test]
fn every_command_the_hint_strip_names_is_a_real_command() {
    let path = repo_root().join("src/ui/qt6/cpp/keyhintbar.cpp");
    let text =
        fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
    let registry = CommandRegistry::baseline();

    // Only the arrays, so that `#include "keyhintbar.h"` is not mistaken for a
    // command id.
    let start = text
        .find("const char *const k")
        .expect("keyhintbar.cpp no longer declares its command arrays");
    let end = text
        .find("const char *const *commandsFor")
        .expect("keyhintbar.cpp no longer has commandsFor");
    let arrays = &text[start..end];

    // Inside the arrays every string literal is a command id.
    let mut checked = 0_usize;
    for raw in arrays.split('"').skip(1).step_by(2) {
        if raw.is_empty() || !raw.contains('.') {
            continue;
        }
        if !raw
            .chars()
            .all(|c| c.is_ascii_lowercase() || c == '.' || c == '_')
        {
            continue;
        }
        checked += 1;
        assert!(
            registry.contains(&CommandId::new(raw)),
            "{} names `{raw}`, which is not a registered command",
            path.display()
        );
    }
    assert!(
        checked > 20,
        "found only {checked} ids; the scan stopped matching"
    );
}

/// Every command carries a picture.
///
/// The menus and the toolbar both draw a command's icon from one table in
/// `icons.cpp`, and a command missing from that table gets no icon at all -
/// which is invisible until someone opens the menu and sees a ragged column
/// of labels with gaps where the pictures should be. That is how `file.copy_to`,
/// `file.move_to`, `file.terminal`, `view.key_hints` and eleven others shipped
/// blank.
#[test]
fn every_command_has_an_icon() {
    let path = repo_root().join("src/ui/qt6/cpp/icons.cpp");
    let text =
        fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));

    // Entries read `{QStringLiteral("id"), QStringLiteral("file")}`; the first
    // literal of each pair is the command id.
    let mut mapped = std::collections::BTreeSet::new();
    for pair in text.split("QStringLiteral(\"").skip(1) {
        if let Some(end) = pair.find('"') {
            mapped.insert(pair[..end].to_string());
        }
    }

    let missing: Vec<_> = CommandRegistry::baseline()
        .iter()
        .map(|c| c.id().as_str().to_string())
        .filter(|id| !mapped.contains(id))
        .collect();

    assert!(
        missing.is_empty(),
        "these commands have no icon in {}: {missing:?}",
        path.display()
    );
}

/// The chords the file list spells out by hand must resolve.
///
/// `PaneWidget` builds a chord string for the keys it wants to claim - the
/// arrows, Home, End, Insert, Delete - and asks the core what command it
/// names. If `KeyChord::parse` and the keymap disagree with the spelling
/// `chordFor` produces, the key silently does nothing: the lookup returns an
/// empty id and the event falls through to Qt.
#[test]
fn the_chords_the_file_list_spells_by_hand_resolve_to_commands() {
    let path = repo_root().join("keymaps/single-key.keymap");
    let text = fs::read_to_string(&path).unwrap();
    let keymap = Keymap::parse("single-key", &text).expect("the shipped keymap parses");

    // Exactly the spellings in `PaneWidget::chordFor`.
    for (chord, expected) in [
        ("left", "nav.up"),
        ("right", "file.open"),
        ("backspace", "nav.up"),
        ("enter", "file.open"),
    ] {
        let parsed = KeyChord::parse(chord)
            .unwrap_or_else(|e| panic!("`{chord}` is not a chord this build can parse: {e:?}"));
        let found = keymap.command_for(&parsed);
        assert_eq!(
            found.map(CommandId::as_str),
            Some(expected),
            "`{chord}` should run {expected}, but the keymap answers {found:?}"
        );
    }
}

/// One command, one shortcut — and the right one.
///
/// A command may be bound to several chords, and only the first `QAction`
/// registered for an id carries a shortcut (Qt refuses to deliver an
/// ambiguous one). Which chord that is has to be stable and has to be the one
/// a person would expect, or a key appears to run somebody else's command.
#[test]
fn a_command_with_several_chords_advertises_a_chord_it_actually_has() {
    let path = repo_root().join("keymaps/single-key.keymap");
    let text = fs::read_to_string(&path).unwrap();
    let keymap = Keymap::parse("single-key", &text).expect("the shipped keymap parses");

    for id in [
        "view.tree",
        "file.open",
        "nav.up",
        "file.mark.all",
        "search.open",
    ] {
        let command = CommandId::new(id);
        let chords = keymap.chords_for(&command);
        assert!(!chords.is_empty(), "{id} is unbound");
        // Whatever is advertised must be one of the command's own chords.
        for chord in &chords {
            assert_eq!(
                keymap.command_for(chord).map(CommandId::as_str),
                Some(id),
                "{chord:?} is advertised for {id} but resolves elsewhere"
            );
        }
    }

    // And no chord resolves to two commands.
    let mut seen: std::collections::BTreeMap<String, String> = std::collections::BTreeMap::new();
    for command in CommandRegistry::baseline().iter() {
        for chord in keymap.chords_for(command.id()) {
            let key = format!("{chord:?}");
            if let Some(other) = seen.get(&key) {
                assert_eq!(
                    other,
                    command.id().as_str(),
                    "{key} is claimed by both {other} and {}",
                    command.id().as_str()
                );
            }
            seen.insert(key, command.id().as_str().to_string());
        }
    }
}

/// A bound chord must reach something that runs.
///
/// The interface wires a command up by naming its id in `command(...)`,
/// `button(...)` or the context menu's `add(...)`. A command that is bound to
/// a chord but named nowhere in the interface is a key that does nothing when
/// pressed — and it still appears in the shortcuts window and the palette, so
/// the user learns the key exists and then that it does not work. `file.edit`
/// shipped that way from the beginning, and `search.ai` and `tab.duplicate`
/// were found the same way.
///
/// A command with no chord is fine: it is reachable from a menu or not yet
/// built, and neither is a broken key.
/// Whether the interface actually attaches a handler to this command.
///
/// Naming the id is not enough: `icons.cpp` maps every command to a picture
/// and `keyhintbar.cpp` lists the ones worth offering, and neither runs
/// anything. What wires a handler is `command(...)`, `button(...)` or the
/// context menu's `add(...)`, so the id has to appear as the argument of one
/// of those.
fn is_wired(sources: &str, id: &str) -> bool {
    let needle = format!("\"{id}\"");
    let mut from = 0;
    while let Some(at) = sources[from..].find(&needle) {
        let at = from + at;
        // The call name sits shortly before the literal - directly for
        // `add("id"`, or after a menu argument for `command(menu, "id"`.
        let window = &sources[at.saturating_sub(64)..at];
        if window.contains("command(") || window.contains("button(") || window.contains("add(") {
            return true;
        }
        from = at + needle.len();
    }
    false
}

#[test]
fn every_bound_chord_reaches_a_handler_in_the_interface() {
    let ui = repo_root().join("src/ui/qt6/cpp");
    let mut wired = String::new();
    for entry in fs::read_dir(&ui)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", ui.display()))
        .flatten()
    {
        let path = entry.path();
        if path.extension().is_some_and(|e| e == "cpp" || e == "mm") {
            wired.push_str(&fs::read_to_string(&path).unwrap_or_default());
        }
    }

    let mut dead: Vec<String> = Vec::new();
    for path in shipped_keymaps() {
        let text = fs::read_to_string(&path).unwrap();
        let name = path.file_stem().unwrap().to_string_lossy().into_owned();
        let Ok(keymap) = Keymap::parse(&name, &text) else {
            continue; // another test reports an unparsable keymap
        };
        for command in CommandRegistry::baseline().iter() {
            let id = command.id().as_str();
            if keymap.chords_for(command.id()).is_empty() {
                continue;
            }
            if !is_wired(&wired, id) {
                dead.push(format!("{name}: {id}"));
            }
        }
    }
    dead.sort();
    dead.dedup();
    assert!(
        dead.is_empty(),
        "these commands are bound to a chord but nothing in the interface runs them:\n  {}",
        dead.join("\n  ")
    );
}
