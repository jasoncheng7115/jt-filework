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
    let text = fs::read_to_string(repo_root().join("keymaps/platform.keymap")).unwrap();
    let keymap = Keymap::parse("platform", &text).unwrap();

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
