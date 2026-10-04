//! The hex editor's side of the boundary.
//!
//! One session at a time, held here rather than in the window, so the window
//! is a drawing of state it does not own. Everything the window needs to
//! paint a row — the bytes, which of them changed, where the cursor and the
//! selection are — is answered from here, and every keystroke that means
//! something is a call into `jtf_hexedit`.
//!
//! The window is `src/ui/qt6/cpp/hexeditorwindow.cpp`, and the C entry points
//! are the `jtf_hex_*` functions in `ffi.rs`.
//!
//! Errors are kept as catalogue keys, never as the core's context: that is
//! written for a log, in English (`jtf_core::Error`).

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

use std::path::Path;

use jtf_core::Error;
use jtf_hexedit::find::{find_backward, find_forward, Kind, Needle, Width};
use jtf_hexedit::session::{Column, Mode, Session, Summary};
use jtf_hexedit::{clip, goto};

/// Bytes shown per row. The window draws this many columns and the offsets
/// step by it, so it lives on this side and the window asks.
pub const ROW_BYTES: u64 = 16;

/// One open editor.
pub struct HexEdit {
    session: Session,
    /// The last error, for the window to show: what failed, as a catalogue
    /// key, and why, as the error code's own key. Cleared as soon as it is
    /// read: a stale message shown next to a successful action is worse than
    /// none.
    error: Option<(&'static str, &'static str)>,
    /// What the last paste was read as, so the window can say so.
    last_paste_kind: Option<&'static str>,
}

impl HexEdit {
    /// Open a file for editing.
    ///
    /// # Errors
    ///
    /// Whatever opening reports.
    pub fn open(path: &Path) -> Result<Self, Error> {
        Ok(Self {
            session: Session::open(path)?,
            error: None,
            last_paste_kind: None,
        })
    }

    /// The session, for the calls that are pure pass-through.
    pub const fn session(&self) -> &Session {
        &self.session
    }

    /// The session, mutably.
    pub const fn session_mut(&mut self) -> &mut Session {
        &mut self.session
    }

    /// Take the last error, if any: what failed and why, as catalogue keys.
    pub fn take_error(&mut self) -> Option<(&'static str, &'static str)> {
        self.error.take()
    }

    /// Take the catalogue key naming how the last paste was read.
    pub fn take_paste_kind(&mut self) -> Option<&'static str> {
        self.last_paste_kind.take()
    }

    /// Remember an error for the window to show.
    fn fail(&mut self, what: &'static str, error: &Error) {
        self.error = Some((what, error.code().message_key()));
    }

    /// Refuse a change while the file is open read-only, and say so.
    ///
    /// The session itself ignores a write in read-only mode, which is right
    /// for a stray keystroke and wrong for a paste or a replace: those were
    /// asked for, and doing nothing without a word looks like a bug.
    fn refuse_read_only(&mut self) -> bool {
        if self.session.mode() == Mode::ReadOnly {
            self.error = Some(("hex.error.readonly", "error.permission_denied"));
            return true;
        }
        false
    }

    /// How many rows the file occupies.
    ///
    /// One more than the bytes need when the cursor can sit at the very end,
    /// which in insert mode it must — otherwise there is nowhere to stand to
    /// append to a file whose length is a multiple of the row width.
    pub fn row_count(&self) -> u64 {
        let len = self.session.len();
        let rows = len.div_ceil(ROW_BYTES);
        if len.is_multiple_of(ROW_BYTES) {
            rows + 1
        } else {
            rows
        }
    }

    /// The bytes of one row, and whether each was changed.
    ///
    /// # Errors
    ///
    /// Whatever reading reports.
    pub fn row(&mut self, row: u64) -> Result<Vec<jtf_hexedit::Byte>, Error> {
        let offset = row.saturating_mul(ROW_BYTES);
        self.session
            .buffer_mut()
            .read(offset, usize::try_from(ROW_BYTES).unwrap_or(16))
    }

    /// Move the cursor to a position given as text.
    ///
    /// Returns whether it was understood; the error is kept for the window.
    pub fn goto(&mut self, text: &str, extend: bool) -> bool {
        match goto::resolve(text, self.session.cursor(), self.session.len()) {
            Ok(offset) => {
                self.session.move_to(offset, extend);
                true
            }
            Err(error) => {
                self.fail("hex.error.goto", &error);
                false
            }
        }
    }

    /// Find the next or previous match of `text`, read as `kind`.
    ///
    /// Returns whether something was found. Wraps once, because a search that
    /// stops at the end of the file and says "not found" when the match is
    /// three rows above is answering a different question than the one asked.
    pub fn find(&mut self, text: &str, kind: Kind, forwards: bool) -> bool {
        let needle = match Needle::compile(text, kind) {
            Ok(needle) => needle,
            Err(error) => {
                self.fail("hex.error.find", &error);
                return false;
            }
        };
        if needle.is_empty() {
            return false;
        }
        let cursor = self.session.cursor();
        let found = if forwards {
            find_forward(&mut self.session, &needle, cursor.saturating_add(1))
                .ok()
                .flatten()
                .or_else(|| find_forward(&mut self.session, &needle, 0).ok().flatten())
        } else {
            find_backward(&mut self.session, &needle, cursor)
                .ok()
                .flatten()
                .or_else(|| {
                    let end = self.session.len();
                    find_backward(&mut self.session, &needle, end)
                        .ok()
                        .flatten()
                })
        };
        match found {
            Some(at) => {
                // Selected, not just jumped to: what was found is what a
                // replace would replace, and it has to be visible as such.
                self.session.move_to(at, false);
                self.session
                    .move_to(at.saturating_add(needle.len() as u64), true);
                true
            }
            None => false,
        }
    }

    /// Replace the current selection, then find the next match.
    ///
    /// Returns whether anything was replaced.
    pub fn replace(&mut self, find_text: &str, kind: Kind, with: &[u8]) -> bool {
        if self.refuse_read_only() {
            return false;
        }
        let Some(selection) = self.session.selection() else {
            // Nothing selected means nothing has been found yet; find first,
            // so the first press of Replace does not silently do nothing.
            return self.find(find_text, kind, true);
        };
        self.session.move_to(selection.start(), false);
        self.session
            .move_to(selection.start() + selection.len(), true);
        if let Err(error) = self.session.write(with) {
            self.fail("hex.error.write", &error);
            return false;
        }
        self.find(find_text, kind, true);
        true
    }

    /// Replace every match from the start of the file. Returns how many.
    pub fn replace_all(&mut self, find_text: &str, kind: Kind, with: &[u8]) -> u64 {
        if self.refuse_read_only() {
            return 0;
        }
        let needle = match Needle::compile(find_text, kind) {
            Ok(needle) => needle,
            Err(error) => {
                self.fail("hex.error.find", &error);
                return 0;
            }
        };
        if needle.is_empty() {
            return 0;
        }
        let mut count = 0u64;
        let mut at = 0u64;
        while let Ok(Some(found)) = find_forward(&mut self.session, &needle, at) {
            self.session.move_to(found, false);
            self.session.move_to(found + needle.len() as u64, true);
            if let Err(error) = self.session.write(with) {
                self.fail("hex.error.write", &error);
                break;
            }
            count += 1;
            // Past what was just written, so a replacement containing the
            // needle does not match itself forever.
            at = found + with.len() as u64;
            if at >= self.session.len() {
                break;
            }
        }
        count
    }

    /// The selected bytes rendered in a copy format.
    pub fn copy_as(&mut self, format: clip::Format) -> String {
        match self.session.selected_bytes() {
            Ok(bytes) => clip::render(&bytes, format),
            Err(error) => {
                self.fail("hex.error.copy", &error);
                String::new()
            }
        }
    }

    /// Work out what pasted text is and write it in.
    ///
    /// Returns whether anything went in.
    pub fn paste(&mut self, text: &str) -> bool {
        if self.refuse_read_only() {
            return false;
        }
        match clip::parse_paste(text) {
            Ok(pasted) => {
                self.last_paste_kind = Some(pasted.read_as);
                if let Err(error) = self.session.write(&pasted.bytes) {
                    self.fail("hex.error.write", &error);
                    return false;
                }
                true
            }
            Err(error) => {
                self.fail("hex.error.paste", &error);
                false
            }
        }
    }

    /// The bytes a replacement box's text stands for, read as `kind`.
    ///
    /// `None`, with the reason kept, when it cannot be read or has wildcards:
    /// a `??` in "replace with" has nothing to put in the hole.
    pub fn replacement(&mut self, text: &str, kind: Kind) -> Option<Vec<u8>> {
        match Needle::compile(text, kind) {
            Ok(needle) => {
                let bytes = needle.literal();
                if bytes.is_none() {
                    self.error = Some(("hex.error.wildcard", "error.invalid_path"));
                }
                bytes
            }
            Err(error) => {
                self.fail("hex.error.replacement", &error);
                None
            }
        }
    }

    /// Record a failed keystroke's error, for the window to show.
    pub fn note(&mut self, what: &'static str, error: &Error) {
        self.fail(what, error);
    }

    /// What would be written if it were saved now.
    pub fn summary(&self) -> Summary {
        self.session.summary()
    }

    /// Write the file back.
    ///
    /// Returns whether it worked.
    pub fn save(&mut self) -> bool {
        match self.session.save() {
            Ok(()) => true,
            Err(error) => {
                self.fail("hex.error.save", &error);
                false
            }
        }
    }
}

/// Turn the window's integer choice into a search kind.
pub const fn kind_of(code: i32, width: i32, little_endian: bool) -> Kind {
    match code {
        1 => Kind::Utf8,
        2 => Kind::Latin1,
        3 => Kind::Utf16Le,
        4 => Kind::Utf16Be,
        5 => Kind::Integer {
            width: match width {
                1 => Width::U8,
                2 => Width::U16,
                8 => Width::U64,
                _ => Width::U32,
            },
            little_endian,
        },
        _ => Kind::Hex,
    }
}

/// Turn the window's integer choice into a copy format.
pub const fn format_of(code: i32) -> clip::Format {
    match code {
        1 => clip::Format::HexString,
        2 => clip::Format::HexSpaced,
        3 => clip::Format::CArray,
        4 => clip::Format::RustArray,
        5 => clip::Format::PythonBytes,
        6 => clip::Format::Base64,
        _ => clip::Format::Raw,
    }
}

/// Turn the window's integer choice into a typing mode.
pub const fn mode_of(code: i32) -> Mode {
    match code {
        1 => Mode::Overwrite,
        2 => Mode::Insert,
        _ => Mode::ReadOnly,
    }
}

/// And back, for the window to read the current one.
pub const fn mode_code(mode: Mode) -> i32 {
    match mode {
        Mode::ReadOnly => 0,
        Mode::Overwrite => 1,
        Mode::Insert => 2,
    }
}

/// Which column, as the window numbers them.
pub const fn column_of(code: i32) -> Column {
    if code == 1 {
        Column::Text
    } else {
        Column::Hex
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str, contents: &[u8]) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("jtf-bridge-hexedit-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(name);
        std::fs::write(&path, contents).unwrap();
        path
    }

    fn editor(name: &str, contents: &[u8]) -> HexEdit {
        HexEdit::open(&scratch(name, contents)).unwrap()
    }

    #[test]
    fn there_is_a_row_to_stand_on_at_the_end_of_an_exact_multiple() {
        // 32 bytes is two full rows; insert mode needs a third to append to.
        let e = editor("rows", &[0u8; 32]);
        assert_eq!(e.row_count(), 3);
        // 33 bytes already has a third row with a byte in it.
        let e = editor("rows2", &[0u8; 33]);
        assert_eq!(e.row_count(), 3);
    }

    #[test]
    fn goto_moves_the_cursor_and_a_bad_expression_says_why() {
        let mut e = editor("goto", &[0u8; 0x100]);
        assert!(e.goto("0x20", false));
        assert_eq!(e.session().cursor(), 0x20);
        assert!(e.take_error().is_none());

        assert!(!e.goto("nonsense", false));
        assert_eq!(e.session().cursor(), 0x20, "a bad goto moved the cursor");
        assert!(e.take_error().is_some(), "it failed without saying why");
    }

    #[test]
    fn find_selects_the_match_so_replace_has_something_to_act_on() {
        let mut e = editor("find", b"....PK\x03\x04....");
        assert!(e.find("50 4B 03 04", Kind::Hex, true));
        let selection = e.session().selection().unwrap();
        assert_eq!((selection.start(), selection.len()), (4, 4));
    }

    #[test]
    fn find_wraps_once_rather_than_reporting_not_found() {
        let mut e = editor("wrap", b"AA....");
        e.session_mut().move_to(4, false);
        assert!(
            e.find("41 41", Kind::Hex, true),
            "a match above the cursor was reported as absent"
        );
        assert_eq!(e.session().cursor(), 2);
    }

    #[test]
    fn replace_all_reaches_every_match() {
        let mut e = editor("all", b"aXaXaX");
        e.session_mut().set_mode(Mode::Overwrite);
        let count = e.replace_all("61", Kind::Hex, b"b");
        assert_eq!(count, 3);
        assert_eq!(
            e.session_mut().buffer_mut().read_bytes(0, 6).unwrap(),
            b"bXbXbX".to_vec()
        );
    }

    #[test]
    fn replace_all_terminates_when_the_replacement_contains_the_needle() {
        // The scan resumes past what was written, not inside it. Without that
        // this runs until the disk fills.
        let mut e = editor("all2", b"aXaXaX");
        e.session_mut().set_mode(Mode::Insert);
        let count = e.replace_all("61", Kind::Hex, b"aa");
        assert!(count > 0 && count < 100, "ran away: {count}");
    }

    #[test]
    fn copying_and_pasting_go_through_the_formats() {
        let mut e = editor("clipboard", b"PK\x03\x04rest");
        e.session_mut().move_to(0, false);
        e.session_mut().move_to(4, true);
        assert_eq!(e.copy_as(clip::Format::HexSpaced), "50 4B 03 04");

        e.session_mut().set_mode(Mode::Overwrite);
        e.session_mut().move_to(0, false);
        assert!(e.paste("FF FF"));
        assert_eq!(e.take_paste_kind(), Some("hex.paste.hex"));
        assert_eq!(
            e.session_mut().buffer_mut().read_bytes(0, 2).unwrap(),
            vec![0xff, 0xff]
        );
    }

    #[test]
    fn a_read_only_session_refuses_a_paste_and_says_so() {
        let mut e = editor("ro", b"abcd");
        assert!(
            !e.paste("FF"),
            "a paste into a read-only file reported success"
        );
        assert_eq!(
            e.session_mut().buffer_mut().read_bytes(0, 4).unwrap(),
            b"abcd".to_vec(),
            "a paste changed a read-only buffer"
        );
        assert_eq!(
            e.take_error().map(|(what, _)| what),
            Some("hex.error.readonly"),
            "refused without saying why - it looked like the paste had simply not happened"
        );
    }

    #[test]
    fn a_replacement_with_a_wildcard_is_refused_with_a_reason() {
        let mut e = editor("wild", b"abcd");
        assert!(e.replacement("41 ??", Kind::Hex).is_none());
        assert_eq!(
            e.take_error().map(|(what, _)| what),
            Some("hex.error.wildcard")
        );
        assert_eq!(e.replacement("41 42", Kind::Hex), Some(b"AB".to_vec()));
        assert_eq!(e.replacement("hi", Kind::Utf8), Some(b"hi".to_vec()));
    }

    #[test]
    fn the_window_codes_map_to_the_right_things_both_ways() {
        for mode in [Mode::ReadOnly, Mode::Overwrite, Mode::Insert] {
            assert_eq!(mode_of(mode_code(mode)), mode);
        }
        assert_eq!(kind_of(0, 0, true), Kind::Hex);
        assert_eq!(kind_of(1, 0, true), Kind::Utf8);
        assert_eq!(
            kind_of(5, 2, false),
            Kind::Integer {
                width: Width::U16,
                little_endian: false
            }
        );
        assert_eq!(format_of(6), clip::Format::Base64);
        assert_eq!(column_of(1), Column::Text);
    }
}
