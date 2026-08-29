//! Keymaps: chord to command id.
//!
//! Keymaps are **data**, not code (`docs/UI_UX_SPEC.md` §7). They are loaded,
//! validated and swapped at runtime, and a conflict is reported at load rather
//! than discovered when a shortcut silently does the wrong thing.
//!
//! # The primary modifier
//!
//! Core must not know which platform it is on (`AGENTS.md` §5). So a chord
//! names [`Modifiers::primary`] — the platform's main accelerator, Command on
//! macOS and Control elsewhere — and the platform layer maps a physical event
//! onto it. A keymap file therefore works on all three platforms without
//! being rewritten, and `ctrl` stays available for the cases that genuinely
//! mean Control everywhere.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::ids::CommandId;

/// Modifier state of a chord.
///
/// Four independently held physical keys, not a state machine: any subset can
/// be down at once, which is what makes them modifiers.
#[allow(clippy::struct_excessive_bools)]
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize,
)]
pub struct Modifiers {
    /// The platform's primary accelerator: Command on macOS, Control else.
    pub primary: bool,
    /// Control, where it is genuinely meant as Control on every platform.
    pub control: bool,
    /// Shift.
    pub shift: bool,
    /// Option / Alt.
    pub alt: bool,
}

impl Modifiers {
    /// No modifiers.
    pub const NONE: Self = Self {
        primary: false,
        control: false,
        shift: false,
        alt: false,
    };

    /// Primary only.
    pub const PRIMARY: Self = Self {
        primary: true,
        control: false,
        shift: false,
        alt: false,
    };

    /// Whether any modifier is held.
    pub const fn any(self) -> bool {
        self.primary || self.control || self.shift || self.alt
    }
}

/// A key, independent of layout and of any toolkit's key codes.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum Key {
    /// A printable character key, stored lowercase.
    Char(char),
    /// A function key, 1-based.
    Function(u8),
    /// Arrow up.
    Up,
    /// Arrow down.
    Down,
    /// Arrow left.
    Left,
    /// Arrow right.
    Right,
    /// Enter / Return.
    Enter,
    /// Escape.
    Escape,
    /// Space. Distinct from `Char(' ')` because it is a command key here.
    Space,
    /// Tab.
    Tab,
    /// Backspace.
    Backspace,
    /// Delete forward.
    Delete,
    /// Home.
    Home,
    /// End.
    End,
    /// Page up.
    PageUp,
    /// Page down.
    PageDown,
    /// Insert.
    Insert,
}

/// A modifier combination plus a key.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct KeyChord {
    /// Modifier state.
    pub modifiers: Modifiers,
    /// The key.
    pub key: Key,
}

impl KeyChord {
    /// A chord.
    pub const fn new(modifiers: Modifiers, key: Key) -> Self {
        Self { modifiers, key }
    }

    /// A chord with no modifiers.
    pub const fn plain(key: Key) -> Self {
        Self {
            modifiers: Modifiers::NONE,
            key,
        }
    }

    /// Parse a chord such as `primary+shift+n`, `f5`, `space`.
    ///
    /// # Errors
    ///
    /// [`KeymapError::UnknownModifier`] or [`KeymapError::UnknownKey`].
    pub fn parse(text: &str) -> Result<Self, KeymapError> {
        let lowered = text.trim().to_ascii_lowercase();
        if lowered.is_empty() {
            return Err(KeymapError::UnknownKey(text.to_string()));
        }
        let mut modifiers = Modifiers::NONE;
        let mut key = None;

        for part in lowered.split('+') {
            let part = part.trim();
            match part {
                "primary" | "cmdorctrl" => modifiers.primary = true,
                "ctrl" | "control" => modifiers.control = true,
                "shift" => modifiers.shift = true,
                "alt" | "option" | "opt" => modifiers.alt = true,
                "" => return Err(KeymapError::UnknownModifier(text.to_string())),
                other => {
                    if key.is_some() {
                        return Err(KeymapError::UnknownModifier(other.to_string()));
                    }
                    key = Some(parse_key(other)?);
                }
            }
        }

        key.map(|key| Self { modifiers, key })
            .ok_or_else(|| KeymapError::UnknownKey(text.to_string()))
    }
}

fn parse_key(text: &str) -> Result<Key, KeymapError> {
    if let Some(rest) = text.strip_prefix('f') {
        if let Ok(n) = rest.parse::<u8>() {
            if (1..=24).contains(&n) {
                return Ok(Key::Function(n));
            }
        }
    }
    Ok(match text {
        "up" => Key::Up,
        "down" => Key::Down,
        "left" => Key::Left,
        "right" => Key::Right,
        "enter" | "return" => Key::Enter,
        "escape" | "esc" => Key::Escape,
        "space" => Key::Space,
        "tab" => Key::Tab,
        "backspace" => Key::Backspace,
        "delete" | "del" => Key::Delete,
        "home" => Key::Home,
        "end" => Key::End,
        "pageup" => Key::PageUp,
        "pagedown" => Key::PageDown,
        "insert" | "ins" => Key::Insert,
        // Punctuation by name, because a keymap file that has to contain a
        // bare comma is a keymap file that cannot be split on commas later.
        "comma" => Key::Char(','),
        "period" | "dot" => Key::Char('.'),
        "minus" | "dash" => Key::Char('-'),
        "equal" => Key::Char('='),
        // CView marks with the numeric keypad's `+`, `-` and `*`, so these
        // are keys in their own right rather than shifted spellings of
        // something else. `plus` is the character, not `shift+equal`.
        "plus" => Key::Char('+'),
        "asterisk" | "star" => Key::Char('*'),
        "underscore" => Key::Char('_'),
        "slash" => Key::Char('/'),
        "backslash" => Key::Char('\\'),
        "semicolon" => Key::Char(';'),
        "quote" => Key::Char('\''),
        "bracketleft" => Key::Char('['),
        "bracketright" => Key::Char(']'),
        "grave" | "backtick" => Key::Char('`'),
        other => {
            let mut chars = other.chars();
            match (chars.next(), chars.next()) {
                (Some(c), None) => Key::Char(c),
                _ => return Err(KeymapError::UnknownKey(other.to_string())),
            }
        }
    })
}

/// Why a keymap was rejected.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum KeymapError {
    /// A modifier name was not recognised.
    UnknownModifier(String),
    /// A key name was not recognised.
    UnknownKey(String),
    /// Two bindings claim the same chord.
    Conflict {
        /// The contested chord, as written.
        chord: String,
        /// The command bound first.
        existing: CommandId,
        /// The command that tried to take it.
        incoming: CommandId,
    },
    /// A binding names a command that does not exist.
    UnknownCommand(CommandId),
    /// A line could not be read as `chord = command`.
    MalformedLine {
        /// 1-based line number.
        line: usize,
    },
}

impl core::fmt::Display for KeymapError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::UnknownModifier(m) => write!(f, "unknown modifier: {m}"),
            Self::UnknownKey(k) => write!(f, "unknown key: {k}"),
            Self::Conflict {
                chord,
                existing,
                incoming,
            } => {
                write!(
                    f,
                    "chord {chord} is bound to both {existing} and {incoming}"
                )
            }
            Self::UnknownCommand(id) => write!(f, "binding names unknown command: {id}"),
            Self::MalformedLine { line } => write!(f, "malformed binding on line {line}"),
        }
    }
}

impl std::error::Error for KeymapError {}

impl KeyChord {
    /// Render in the keymap file's own syntax, so a saved keymap reloads.
    pub fn to_source_text(&self) -> String {
        let mut parts = Vec::new();
        if self.modifiers.primary {
            parts.push("primary".to_string());
        }
        if self.modifiers.control {
            parts.push("ctrl".to_string());
        }
        if self.modifiers.alt {
            parts.push("alt".to_string());
        }
        if self.modifiers.shift {
            parts.push("shift".to_string());
        }
        parts.push(match &self.key {
            // Written back by name so the file round-trips through the parser.
            Key::Char(',') => "comma".into(),
            Key::Char('.') => "period".into(),
            Key::Char('-') => "minus".into(),
            Key::Char('=') => "equal".into(),
            Key::Char('+') => "plus".into(),
            Key::Char('*') => "asterisk".into(),
            Key::Char('_') => "underscore".into(),
            Key::Char('/') => "slash".into(),
            Key::Char('\\') => "backslash".into(),
            Key::Char(';') => "semicolon".into(),
            Key::Char('\'') => "quote".into(),
            Key::Char('[') => "bracketleft".into(),
            Key::Char(']') => "bracketright".into(),
            Key::Char('`') => "grave".into(),
            Key::Char(c) => c.to_lowercase().to_string(),
            Key::Function(n) => format!("f{n}"),
            Key::Up => "up".into(),
            Key::Down => "down".into(),
            Key::Left => "left".into(),
            Key::Right => "right".into(),
            Key::Enter => "enter".into(),
            Key::Escape => "escape".into(),
            Key::Space => "space".into(),
            Key::Tab => "tab".into(),
            Key::Backspace => "backspace".into(),
            Key::Delete => "delete".into(),
            Key::Home => "home".into(),
            Key::End => "end".into(),
            Key::PageUp => "pageup".into(),
            Key::PageDown => "pagedown".into(),
            Key::Insert => "insert".into(),
        });
        parts.join("+")
    }

    /// Render as a string `QKeySequence` accepts.
    ///
    /// `primary` becomes Qt's `Ctrl`, which Qt itself maps to Command on
    /// macOS — so one keymap file is correct on every platform, which is the
    /// whole point of naming the modifier by role rather than by key
    /// (`AGENTS.md` §5).
    pub fn to_portable_shortcut(&self) -> String {
        let mut parts = Vec::new();
        if self.modifiers.primary {
            parts.push("Ctrl".to_string());
        }
        if self.modifiers.control {
            // Qt calls the physical Control key "Meta" on macOS and "Ctrl"
            // elsewhere; `Meta` is the portable spelling for "really Control".
            parts.push("Meta".to_string());
        }
        if self.modifiers.alt {
            parts.push("Alt".to_string());
        }
        if self.modifiers.shift {
            parts.push("Shift".to_string());
        }
        parts.push(match &self.key {
            Key::Char(c) => c.to_uppercase().to_string(),
            Key::Function(n) => format!("F{n}"),
            Key::Up => "Up".into(),
            Key::Down => "Down".into(),
            Key::Left => "Left".into(),
            Key::Right => "Right".into(),
            Key::Enter => "Return".into(),
            Key::Escape => "Esc".into(),
            Key::Space => "Space".into(),
            Key::Tab => "Tab".into(),
            Key::Backspace => "Backspace".into(),
            Key::Delete => "Delete".into(),
            Key::Home => "Home".into(),
            Key::End => "End".into(),
            Key::PageUp => "PgUp".into(),
            Key::PageDown => "PgDown".into(),
            Key::Insert => "Ins".into(),
        });
        parts.join("+")
    }
}

/// A named set of bindings.
///
/// A keymap can also record that a command is **explicitly unbound**, which is
/// different from not mentioning it. The difference only matters for a user's
/// own keymap, which is stored as a diff against a preset
/// (`docs/UPGRADE.md` §4.1): "not mentioned" means "whatever the preset says",
/// and that has to be distinguishable from "I removed this".
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Keymap {
    name: String,
    bindings: BTreeMap<KeyChord, CommandId>,
    unbound: std::collections::BTreeSet<CommandId>,
    type_ahead: bool,
}

impl Keymap {
    /// An empty keymap.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            bindings: BTreeMap::new(),
            unbound: std::collections::BTreeSet::new(),
            type_ahead: true,
        }
    }

    /// Whether a bare printable key jumps to a file name.
    ///
    /// True in the platform preset, where typing `r` selects `report.txt` the
    /// way Finder and Explorer do. False in the CView preset, where a bare
    /// letter is a command — `e` edits, `v` views — and jumping to a file
    /// name instead would run nothing and move the cursor.
    ///
    /// A property of the keymap rather than a setting of its own: the two
    /// answers belong to the two keyboard traditions, and a preset that binds
    /// letters to commands has already decided this.
    pub const fn type_ahead(&self) -> bool {
        self.type_ahead
    }

    /// Set whether bare printable keys jump to a file name.
    pub const fn set_type_ahead(&mut self, on: bool) {
        self.type_ahead = on;
    }

    /// Commands this map says have no shortcut.
    pub fn unbound(&self) -> impl Iterator<Item = &CommandId> {
        self.unbound.iter()
    }

    /// Record that a command is deliberately unbound.
    pub fn set_unbound(&mut self, command: CommandId) {
        self.unbind_command(&command);
        self.unbound.insert(command);
    }

    /// What this map changes relative to `preset`.
    ///
    /// Only differences are stored, so a later release's new bindings reach a
    /// user who customised something else. Storing a copy instead means every
    /// command added after the customisation ships unbound for that user
    /// (`docs/UPGRADE.md` §4.1).
    #[must_use]
    pub fn diff_from(&self, preset: &Self) -> Self {
        let mut diff = Self::new(&self.name);

        for (chord, command) in &self.bindings {
            let same_as_preset = preset
                .chords_for(command)
                .first()
                .is_some_and(|preset_chord| *preset_chord == chord);
            if !same_as_preset {
                diff.bindings.insert(chord.clone(), command.clone());
            }
        }
        // A command the preset binds and this map does not is an explicit
        // removal, not an omission.
        for command in preset.bindings.values() {
            if self.chords_for(command).is_empty() {
                diff.unbound.insert(command.clone());
            }
        }
        diff
    }

    /// Layer a diff on top of this map.
    ///
    /// `keep` decides which command ids still exist; a binding for a command
    /// that has been removed or renamed is dropped rather than treated as an
    /// error, and the count is returned so the UI can say something changed
    /// (`docs/UPGRADE.md` §4.2).
    pub fn apply_diff(&mut self, diff: &Self, keep: impl Fn(&CommandId) -> bool) -> usize {
        let mut dropped = 0usize;

        for command in &diff.unbound {
            if keep(command) {
                self.unbind_command(command);
            } else {
                dropped += 1;
            }
        }
        for (chord, command) in &diff.bindings {
            if !keep(command) {
                dropped += 1;
                continue;
            }
            // A user's choice wins over whatever held the chord in the preset.
            self.bindings.remove(chord);
            self.unbind_command(command);
            self.bindings.insert(chord.clone(), command.clone());
        }
        dropped
    }

    /// The keymap's name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// How many bindings there are.
    pub fn len(&self) -> usize {
        self.bindings.len()
    }

    /// Whether there are no bindings.
    pub fn is_empty(&self) -> bool {
        self.bindings.is_empty()
    }

    /// Bind a chord.
    ///
    /// # Errors
    ///
    /// [`KeymapError::Conflict`] if the chord is already bound to a different
    /// command. Rebinding a chord to the same command is a no-op, not an error.
    pub fn bind(&mut self, chord: KeyChord, command: CommandId) -> Result<(), KeymapError> {
        if let Some(existing) = self.bindings.get(&chord) {
            if *existing != command {
                return Err(KeymapError::Conflict {
                    chord: format!("{chord:?}"),
                    existing: existing.clone(),
                    incoming: command,
                });
            }
            return Ok(());
        }
        self.bindings.insert(chord, command);
        Ok(())
    }

    /// Resolve a chord to a command id.
    ///
    /// This returns an **id**, never a handler: `AGENTS.md` §9.
    pub fn resolve(&self, chord: &KeyChord) -> Option<&CommandId> {
        self.bindings.get(chord)
    }

    /// Which command a chord is bound to, if any.
    pub fn command_for(&self, chord: &KeyChord) -> Option<&CommandId> {
        self.bindings.get(chord)
    }

    /// Remove every chord bound to a command.
    ///
    /// Returns how many bindings were removed.
    pub fn unbind_command(&mut self, command: &CommandId) -> usize {
        let before = self.bindings.len();
        self.bindings.retain(|_, bound| bound != command);
        before - self.bindings.len()
    }

    /// Give a command a chord, replacing whatever it had.
    ///
    /// # Errors
    ///
    /// [`KeymapError::Conflict`] when another command already owns the chord.
    /// Nothing is changed in that case: a rebinding that half-applied would
    /// leave the user with neither shortcut working.
    pub fn rebind(&mut self, command: &CommandId, chord: KeyChord) -> Result<(), KeymapError> {
        if let Some(existing) = self.bindings.get(&chord) {
            if existing != command {
                return Err(KeymapError::Conflict {
                    chord: chord.to_portable_shortcut(),
                    existing: existing.clone(),
                    incoming: command.clone(),
                });
            }
        }
        self.unbind_command(command);
        self.bindings.insert(chord, command.clone());
        Ok(())
    }

    /// Serialize back to the `chord = command.id` format.
    ///
    /// Used to save a user's overrides, so what is written is the same thing
    /// the loader reads — no second format to drift out of step.
    pub fn to_text(&self) -> String {
        let mut lines = Vec::with_capacity(self.bindings.len() + self.unbound.len() + 1);
        // Written only when it differs from the default, so a keymap that
        // never thought about it stays as short as the user wrote it.
        if !self.type_ahead {
            lines.push("!type_ahead = off".to_string());
        }
        for (chord, command) in &self.bindings {
            lines.push(format!("{} = {command}", chord.to_source_text()));
        }
        for command in &self.unbound {
            lines.push(format!("none = {command}"));
        }
        lines.join("\n") + "\n"
    }

    /// Every chord bound to a command, for showing shortcuts in menus.
    pub fn chords_for(&self, command: &CommandId) -> Vec<&KeyChord> {
        self.bindings
            .iter()
            .filter(|(_, c)| *c == command)
            .map(|(k, _)| k)
            .collect()
    }

    /// Every binding.
    pub fn iter(&self) -> impl Iterator<Item = (&KeyChord, &CommandId)> {
        self.bindings.iter()
    }

    /// Parse a keymap from `chord = command.id` lines, `#` for comments.
    ///
    /// # Errors
    ///
    /// Any [`KeymapError`]. Conflicts are detected here, at load
    /// (`docs/UI_TEST_PLAN.md` KEY-005).
    pub fn parse(name: impl Into<String>, text: &str) -> Result<Self, KeymapError> {
        let mut map = Self::new(name);
        for (index, raw) in text.lines().enumerate() {
            // A trailing comment is stripped before parsing. Neither a key
            // name nor a command id can contain `#`, so the first one starts a
            // comment wherever it appears. Annotating a single binding - which
            // of these came from the real key table, and which is a guess - is
            // worth more on the line than in a paragraph at the top.
            let line = raw.split('#').next().unwrap_or(raw).trim();
            if line.is_empty() {
                continue;
            }
            let Some((chord_text, command_text)) = line.split_once('=') else {
                return Err(KeymapError::MalformedLine { line: index + 1 });
            };
            // Options read as `!name = value`, so they cannot collide with a
            // key name: no chord starts with `!`.
            if let Some(option) = chord_text.trim().strip_prefix('!') {
                match option.trim() {
                    "type_ahead" => {
                        map.type_ahead = matches!(command_text.trim(), "on" | "true" | "yes");
                    }
                    _ => return Err(KeymapError::MalformedLine { line: index + 1 }),
                }
                continue;
            }
            let command = CommandId::new(command_text.trim());
            if command.as_str().is_empty() {
                return Err(KeymapError::MalformedLine { line: index + 1 });
            }
            // `none` records an explicit removal, which a diff needs to be
            // able to say (docs/UPGRADE.md 4.1).
            if chord_text.trim().eq_ignore_ascii_case("none") {
                map.unbound.insert(command);
                continue;
            }
            let chord = KeyChord::parse(chord_text)?;
            if let Err(KeymapError::Conflict {
                existing, incoming, ..
            }) = map.bind(chord, command)
            {
                return Err(KeymapError::Conflict {
                    chord: chord_text.trim().to_string(),
                    existing,
                    incoming,
                });
            }
        }
        Ok(map)
    }

    /// Check every binding names a registered command.
    ///
    /// # Errors
    ///
    /// [`KeymapError::UnknownCommand`] for the first unknown id.
    pub fn validate_against(
        &self,
        registry: &crate::ids::CommandRegistry,
    ) -> Result<(), KeymapError> {
        for command in self.bindings.values() {
            if !registry.contains(command) {
                return Err(KeymapError::UnknownCommand(command.clone()));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::CommandRegistry;

    #[test]
    fn parses_modifiers_and_keys() {
        let c = KeyChord::parse("primary+shift+n").unwrap();
        assert!(c.modifiers.primary && c.modifiers.shift);
        assert!(!c.modifiers.alt && !c.modifiers.control);
        assert_eq!(c.key, Key::Char('n'));

        assert_eq!(KeyChord::parse("f5").unwrap().key, Key::Function(5));
        assert_eq!(KeyChord::parse("F12").unwrap().key, Key::Function(12));
        assert_eq!(
            KeyChord::parse("space").unwrap(),
            KeyChord::plain(Key::Space)
        );
        assert_eq!(KeyChord::parse("alt+left").unwrap().key, Key::Left);
    }

    #[test]
    fn punctuation_keys_have_names_and_round_trip() {
        for (name, character) in [
            ("comma", ','),
            ("period", '.'),
            ("minus", '-'),
            ("equal", '='),
            ("slash", '/'),
            ("bracketleft", '['),
            ("grave", '`'),
        ] {
            let chord = KeyChord::parse(&format!("primary+{name}")).unwrap();
            assert_eq!(chord.key, Key::Char(character), "{name}");
            assert_eq!(
                KeyChord::parse(&chord.to_source_text()).unwrap(),
                chord,
                "{name} must survive a round trip through the file format"
            );
        }
    }

    #[test]
    fn rejects_nonsense_instead_of_guessing() {
        assert!(KeyChord::parse("").is_err());
        assert!(KeyChord::parse("meta+x").is_err());
        assert!(KeyChord::parse("primary+").is_err());
        assert!(KeyChord::parse("notakey").is_err());
    }

    #[test]
    fn a_keymap_resolves_to_a_command_id_not_a_handler() {
        // AGENTS.md 9. The type system is the proof: there is nowhere in this
        // module to put a function pointer.
        let map = Keymap::parse("test", "primary+t = tab.new").unwrap();
        let resolved = map.resolve(&KeyChord::parse("primary+t").unwrap()).unwrap();
        assert_eq!(resolved.as_str(), "tab.new");
    }

    #[test]
    fn conflicts_are_detected_at_load_with_both_commands_named() {
        let err = Keymap::parse("test", "primary+t = tab.new\nprimary+t = tab.close").unwrap_err();
        match err {
            KeymapError::Conflict {
                chord,
                existing,
                incoming,
            } => {
                assert_eq!(chord, "primary+t");
                assert_eq!(existing.as_str(), "tab.new");
                assert_eq!(incoming.as_str(), "tab.close");
            }
            other => panic!("expected a conflict, got {other:?}"),
        }
    }

    #[test]
    fn rebinding_a_chord_to_the_same_command_is_not_a_conflict() {
        assert!(Keymap::parse("test", "primary+t = tab.new\nprimary+t = tab.new").is_ok());
    }

    #[test]
    fn the_primary_modifier_keeps_a_keymap_platform_neutral() {
        // A keymap file must not need rewriting per platform, and core must
        // not know which platform it is on (AGENTS.md 5).
        let map = Keymap::parse("test", "primary+c = file.copy_to_target_pane").unwrap();
        let chord = map.iter().next().unwrap().0;
        assert!(chord.modifiers.primary);
        assert!(
            !chord.modifiers.control,
            "primary is not the same as control"
        );
    }

    #[test]
    fn a_binding_to_an_unknown_command_is_caught_by_validation() {
        let map = Keymap::parse("test", "primary+q = nope.does.not.exist").unwrap();
        let err = map
            .validate_against(&CommandRegistry::baseline())
            .unwrap_err();
        assert!(matches!(err, KeymapError::UnknownCommand(_)));
    }

    #[test]
    fn a_realistic_keymap_validates_against_the_baseline_registry() {
        let text = "\
# comments and blanks are fine

primary+t          = tab.new
primary+w          = tab.close
primary+shift+t    = tab.reopen
primary+d          = workspace.split.horizontal
primary+shift+d    = workspace.split.vertical
alt+left           = nav.back
alt+right          = nav.forward
alt+up             = nav.up
space              = preview.quicklook
insert             = file.mark.toggle
f2                 = file.rename
f5                 = file.copy_to_target_pane
f6                 = file.move_to_target_pane
f8                 = file.trash
primary+f          = search.open
";
        let map = Keymap::parse("cview", text).unwrap();
        assert_eq!(map.len(), 15);
        map.validate_against(&CommandRegistry::baseline()).unwrap();

        let chords = map.chords_for(&CommandId::new("file.rename"));
        assert_eq!(chords.len(), 1, "menus need to show the shortcut");
    }

    #[test]
    fn chords_render_as_portable_qt_shortcuts() {
        let shortcut = |text: &str| KeyChord::parse(text).unwrap().to_portable_shortcut();
        assert_eq!(shortcut("primary+t"), "Ctrl+T");
        assert_eq!(shortcut("primary+shift+d"), "Ctrl+Shift+D");
        assert_eq!(shortcut("f5"), "F5");
        assert_eq!(shortcut("alt+left"), "Alt+Left");
        assert_eq!(shortcut("insert"), "Ins");
        assert_eq!(shortcut("space"), "Space");
        // "really Control" stays distinct from the platform accelerator.
        assert_eq!(shortcut("ctrl+r"), "Meta+R");
    }

    #[test]
    fn rebinding_moves_a_chord_and_reports_a_conflict_without_changing_anything() {
        let mut map = Keymap::parse("test", "primary+t = tab.new\nprimary+w = tab.close").unwrap();

        // Moving a command to a free chord takes its old one away.
        map.rebind(
            &CommandId::new("tab.new"),
            KeyChord::parse("primary+n").unwrap(),
        )
        .unwrap();
        assert!(map
            .resolve(&KeyChord::parse("primary+t").unwrap())
            .is_none());
        assert_eq!(
            map.resolve(&KeyChord::parse("primary+n").unwrap())
                .unwrap()
                .as_str(),
            "tab.new"
        );

        // Claiming an occupied chord fails, and leaves both bindings intact.
        let err = map
            .rebind(
                &CommandId::new("tab.new"),
                KeyChord::parse("primary+w").unwrap(),
            )
            .unwrap_err();
        assert!(matches!(err, KeymapError::Conflict { .. }));
        assert_eq!(
            map.resolve(&KeyChord::parse("primary+n").unwrap())
                .unwrap()
                .as_str(),
            "tab.new"
        );
        assert_eq!(
            map.resolve(&KeyChord::parse("primary+w").unwrap())
                .unwrap()
                .as_str(),
            "tab.close"
        );
    }

    #[test]
    fn a_saved_keymap_reloads_to_the_same_bindings() {
        // What is written is what the loader reads: no second format to drift.
        let original = Keymap::parse(
            "test",
            "primary+t = tab.new\nf5 = file.copy_to_target_pane\nalt+left = nav.back\n\
             ctrl+shift+r = view.refresh\ninsert = file.mark.toggle",
        )
        .unwrap();

        let reloaded = Keymap::parse("test", &original.to_text()).unwrap();
        assert_eq!(reloaded.len(), original.len());
        for (chord, command) in original.iter() {
            assert_eq!(reloaded.resolve(chord), Some(command));
        }
    }

    #[test]
    fn a_diff_carries_only_what_changed_and_survives_a_new_preset_binding() {
        // docs/UPGRADE.md 4.1: the upgrade case this design exists for.
        let preset = Keymap::parse("p", "primary+t = tab.new\nprimary+w = tab.close").unwrap();

        let mut mine = preset.clone();
        mine.rebind(
            &CommandId::new("tab.new"),
            KeyChord::parse("primary+n").unwrap(),
        )
        .unwrap();
        let diff = mine.diff_from(&preset);

        assert_eq!(diff.len(), 1, "only the changed binding is stored");
        assert!(diff.unbound().next().is_none());

        // A later release adds a command and a default for it.
        let mut newer = Keymap::parse(
            "p",
            "primary+t = tab.new\nprimary+w = tab.close\nprimary+f = search.open",
        )
        .unwrap();
        let dropped = newer.apply_diff(&diff, |_| true);

        assert_eq!(dropped, 0);
        assert_eq!(
            newer
                .resolve(&KeyChord::parse("primary+n").unwrap())
                .unwrap()
                .as_str(),
            "tab.new",
            "the customisation survived"
        );
        assert_eq!(
            newer
                .resolve(&KeyChord::parse("primary+f").unwrap())
                .unwrap()
                .as_str(),
            "search.open",
            "and the new default arrived"
        );
        assert!(
            newer
                .resolve(&KeyChord::parse("primary+t").unwrap())
                .is_none(),
            "the old chord did not come back"
        );
    }

    #[test]
    fn an_explicit_removal_is_not_the_same_as_an_omission() {
        let preset = Keymap::parse("p", "primary+t = tab.new\nprimary+w = tab.close").unwrap();

        let mut mine = preset.clone();
        mine.unbind_command(&CommandId::new("tab.close"));
        let diff = mine.diff_from(&preset);

        assert_eq!(diff.unbound().count(), 1);

        let mut fresh = preset.clone();
        fresh.apply_diff(&diff, |_| true);
        assert!(
            fresh.chords_for(&CommandId::new("tab.close")).is_empty(),
            "a removal the user made stays removed"
        );
        assert!(!fresh.chords_for(&CommandId::new("tab.new")).is_empty());
    }

    #[test]
    fn a_binding_for_a_removed_command_is_dropped_rather_than_fatal() {
        // docs/UPGRADE.md 4.2.
        let preset = Keymap::parse("p", "primary+t = tab.new").unwrap();
        let diff = Keymap::parse("p", "primary+x = gone.forever\nprimary+n = tab.new").unwrap();

        let mut merged = preset;
        let dropped = merged.apply_diff(&diff, |id| id.as_str() != "gone.forever");

        assert_eq!(dropped, 1, "the caller can say that something changed");
        assert_eq!(
            merged
                .resolve(&KeyChord::parse("primary+n").unwrap())
                .unwrap()
                .as_str(),
            "tab.new",
            "the rest of the keymap still works"
        );
    }

    #[test]
    fn a_diff_round_trips_through_text_including_removals() {
        let preset = Keymap::parse("p", "primary+t = tab.new\nprimary+w = tab.close").unwrap();
        let mut mine = preset.clone();
        mine.rebind(
            &CommandId::new("tab.new"),
            KeyChord::parse("primary+n").unwrap(),
        )
        .unwrap();
        mine.unbind_command(&CommandId::new("tab.close"));

        let diff = mine.diff_from(&preset);
        let reloaded = Keymap::parse("p", &diff.to_text()).unwrap();
        assert_eq!(reloaded, diff);
    }

    #[test]
    fn unbinding_leaves_the_command_with_no_shortcut() {
        let mut map = Keymap::parse("test", "primary+t = tab.new").unwrap();
        assert_eq!(map.unbind_command(&CommandId::new("tab.new")), 1);
        assert!(map.chords_for(&CommandId::new("tab.new")).is_empty());
        assert!(map.is_empty());
    }

    #[test]
    fn a_malformed_line_reports_its_number() {
        let err = Keymap::parse("test", "primary+t = tab.new\nthis is not a binding").unwrap_err();
        assert_eq!(err, KeymapError::MalformedLine { line: 2 });
    }
}

#[cfg(test)]
mod type_ahead_tests {
    use super::Keymap;

    #[test]
    fn type_ahead_is_on_unless_a_keymap_turns_it_off() {
        let map = Keymap::parse("m", "primary+c = file.copy\n").expect("parses");
        assert!(
            map.type_ahead(),
            "the platform tradition is the default: typing a letter finds a file"
        );
    }

    #[test]
    fn a_keymap_can_turn_type_ahead_off() {
        let map = Keymap::parse("m", "!type_ahead = off\ne = file.edit\n").expect("parses");
        assert!(
            !map.type_ahead(),
            "a preset that binds bare letters to commands cannot also use them \
             to jump to file names"
        );
        assert_eq!(map.len(), 1, "the option line is not a binding");
    }

    #[test]
    fn the_option_survives_a_round_trip() {
        let map = Keymap::parse("m", "!type_ahead = off\ne = file.edit\n").expect("parses");
        let again = Keymap::parse("m", &map.to_text()).expect("re-parses");
        assert!(
            !again.type_ahead(),
            "saving a keymap must not silently restore type-ahead"
        );
    }

    #[test]
    fn a_trailing_comment_annotates_a_binding() {
        let map =
            Keymap::parse("m", "e = file.edit   # confirmed\n# a whole line\n").expect("parses");
        assert_eq!(map.len(), 1);
        assert_eq!(
            map.command_for(&super::KeyChord::parse("e").expect("chord"))
                .map(super::CommandId::as_str),
            Some("file.edit"),
            "the comment must not become part of the command id"
        );
    }

    #[test]
    fn an_unknown_option_is_an_error_rather_than_a_shrug() {
        assert!(
            Keymap::parse("m", "!nonsense = on\n").is_err(),
            "a typo in an option must fail loudly; silently ignoring it is how \
             a keymap ends up not doing what it says"
        );
    }
}
