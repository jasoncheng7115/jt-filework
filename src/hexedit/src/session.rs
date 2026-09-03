//! One file open for editing: where the cursor is, what is selected, what is
//! bookmarked, and which of the two typing modes is on.
//!
//! This is what the window talks to. It holds the state that makes a hex
//! editor an editor rather than a dump, and it is where the rules about that
//! state live — a selection that follows the cursor, a bookmark that moves
//! when bytes are inserted before it, an offset that cannot end up past the
//! end of a file that just got shorter.

use jtf_core::Error;

use crate::buffer::Buffer;
use crate::find::Bytes;
use crate::history::History;

/// What typing does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mode {
    /// The file is not changed by typing at all.
    #[default]
    ReadOnly,
    /// A typed byte replaces the one under the cursor. The length never
    /// changes, which is what editing a fixed-layout structure needs.
    Overwrite,
    /// A typed byte is pushed in and everything after it moves along.
    Insert,
}

impl Mode {
    /// Catalogue key for the mode indicator.
    pub const fn label_key(self) -> &'static str {
        match self {
            Self::ReadOnly => "hex.mode.readonly",
            Self::Overwrite => "hex.mode.overwrite",
            Self::Insert => "hex.mode.insert",
        }
    }

    /// Whether typing changes the file in this mode.
    pub const fn edits(self) -> bool {
        !matches!(self, Self::ReadOnly)
    }
}

/// Which of the two columns the cursor is in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Column {
    /// The hex digits. Two keystrokes make a byte.
    #[default]
    Hex,
    /// The text column. One keystroke makes a byte.
    Text,
}

/// A range of bytes, held as the two ends the user actually moved.
///
/// Kept as anchor-and-cursor rather than start-and-length because dragging
/// backwards is normal and a range that could not be built backwards would
/// have to be rebuilt on every keystroke that crossed its own start.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Selection {
    /// Where the selection was started.
    pub anchor: u64,
    /// Where it is being dragged to; may be before the anchor.
    pub cursor: u64,
}

impl Selection {
    /// First byte in the range.
    pub const fn start(self) -> u64 {
        if self.anchor < self.cursor {
            self.anchor
        } else {
            self.cursor
        }
    }

    /// One past the last byte.
    pub const fn end(self) -> u64 {
        if self.anchor < self.cursor {
            self.cursor
        } else {
            self.anchor
        }
    }

    /// How many bytes are in it.
    pub const fn len(self) -> u64 {
        self.end() - self.start()
    }

    /// Whether it covers nothing.
    pub const fn is_empty(self) -> bool {
        self.len() == 0
    }
}

/// A remembered offset.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bookmark {
    /// The offset remembered.
    pub offset: u64,
    /// What the person called it.
    pub label: String,
}

/// A file open for editing.
pub struct Session {
    history: History,
    cursor: u64,
    /// Set while a selection is being made, cleared when it collapses.
    anchor: Option<u64>,
    column: Column,
    mode: Mode,
    /// Half a byte typed in the hex column, waiting for its second digit.
    pending_nibble: Option<u8>,
    bookmarks: Vec<Bookmark>,
}

impl Session {
    /// Open a file.
    ///
    /// Read-only to begin with. Editing is switched on deliberately, because
    /// a window that opens ready to change a file is a window where a stray
    /// keystroke changes a file.
    ///
    /// # Errors
    ///
    /// Whatever opening the file reports.
    pub fn open(path: &std::path::Path) -> Result<Self, Error> {
        Ok(Self {
            history: History::new(Buffer::open(path)?),
            cursor: 0,
            anchor: None,
            column: Column::Hex,
            mode: Mode::ReadOnly,
            pending_nibble: None,
            bookmarks: Vec::new(),
        })
    }

    /// The buffer, for reading rows.
    pub const fn buffer_mut(&mut self) -> &mut Buffer {
        self.history.buffer_mut()
    }

    /// How long the file is now.
    pub fn len(&self) -> u64 {
        self.history.buffer().len()
    }

    /// Whether there is nothing in it.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Whether anything has changed since opening.
    pub fn is_modified(&self) -> bool {
        self.history.buffer().is_modified()
    }

    /// Where the cursor is.
    pub const fn cursor(&self) -> u64 {
        self.cursor
    }

    /// Which column it is in.
    pub const fn column(&self) -> Column {
        self.column
    }

    /// What typing does.
    pub const fn mode(&self) -> Mode {
        self.mode
    }

    /// Whether a half-typed byte is waiting for its second digit.
    pub const fn has_pending_nibble(&self) -> bool {
        self.pending_nibble.is_some()
    }

    /// The current selection, if any.
    pub fn selection(&self) -> Option<Selection> {
        let anchor = self.anchor?;
        let selection = Selection {
            anchor,
            cursor: self.cursor,
        };
        (!selection.is_empty()).then_some(selection)
    }

    /// Switch what typing does.
    ///
    /// A half-typed byte is dropped: it was half a byte in the old mode and
    /// would mean something else in the new one.
    pub fn set_mode(&mut self, mode: Mode) {
        self.mode = mode;
        self.pending_nibble = None;
    }

    /// Switch which column the cursor is in.
    pub fn set_column(&mut self, column: Column) {
        self.column = column;
        self.pending_nibble = None;
    }

    /// Move the cursor, optionally dragging a selection with it.
    ///
    /// Clamped to the file, so a movement key at either end stops rather than
    /// wrapping or running off.
    pub fn move_to(&mut self, offset: u64, extend: bool) {
        if extend {
            if self.anchor.is_none() {
                self.anchor = Some(self.cursor);
            }
        } else {
            self.anchor = None;
        }
        self.cursor = offset.min(self.len());
        self.pending_nibble = None;
    }

    /// Select everything.
    pub fn select_all(&mut self) {
        self.anchor = Some(0);
        self.cursor = self.len();
        self.pending_nibble = None;
    }

    /// Whether undo would do anything.
    pub fn can_undo(&self) -> bool {
        self.history.can_undo()
    }

    /// Whether redo would do anything.
    pub fn can_redo(&self) -> bool {
        self.history.can_redo()
    }

    /// Undo, moving the cursor to where the change was.
    pub fn undo(&mut self) {
        if let Some(at) = self.history.undo() {
            self.anchor = None;
            self.cursor = at.min(self.len());
            self.pending_nibble = None;
        }
    }

    /// Redo, moving the cursor to where the change was.
    pub fn redo(&mut self) {
        if let Some(at) = self.history.redo() {
            self.anchor = None;
            self.cursor = at.min(self.len());
            self.pending_nibble = None;
        }
    }

    /// Type one hex digit into the hex column.
    ///
    /// Two digits make a byte. The first is held and shown as half-entered;
    /// the second commits it and steps on. That is what makes the hex column
    /// editable at all — a byte cannot be typed in one keystroke.
    ///
    /// Returns whether the keystroke was used.
    ///
    /// # Errors
    ///
    /// Whatever the edit reports.
    pub fn type_hex_digit(&mut self, digit: char) -> Result<bool, Error> {
        if !self.mode.edits() || self.column != Column::Hex {
            return Ok(false);
        }
        let Some(value) = digit.to_digit(16) else {
            return Ok(false);
        };
        let value = u8::try_from(value).unwrap_or(0);
        match self.pending_nibble.take() {
            None => {
                self.pending_nibble = Some(value);
            }
            Some(high) => {
                let byte = (high << 4) | value;
                self.write(&[byte])?;
            }
        }
        Ok(true)
    }

    /// Type one byte into the text column.
    ///
    /// # Errors
    ///
    /// Whatever the edit reports.
    pub fn type_byte(&mut self, byte: u8) -> Result<bool, Error> {
        if !self.mode.edits() || self.column != Column::Text {
            return Ok(false);
        }
        self.write(&[byte])?;
        Ok(true)
    }

    /// Put bytes in at the cursor, replacing a selection if there is one.
    ///
    /// # Errors
    ///
    /// Whatever the edit reports.
    pub fn write(&mut self, bytes: &[u8]) -> Result<(), Error> {
        if bytes.is_empty() {
            return Ok(());
        }
        // Typing over a selection replaces it, which is what every editor
        // does and what makes "select and retype" work.
        if let Some(selection) = self.selection() {
            self.history.delete(selection.start(), selection.len())?;
            self.cursor = selection.start();
            self.anchor = None;
        }
        match self.mode {
            Mode::ReadOnly => return Ok(()),
            Mode::Overwrite => self.history.overwrite(self.cursor, bytes)?,
            Mode::Insert => self.history.insert(self.cursor, bytes),
        }
        self.cursor += bytes.len() as u64;
        self.pending_nibble = None;
        self.clamp_bookmarks();
        Ok(())
    }

    /// Delete the selection, or one byte at the cursor.
    ///
    /// # Errors
    ///
    /// Whatever the edit reports.
    pub fn delete_forward(&mut self) -> Result<(), Error> {
        if !self.mode.edits() {
            return Ok(());
        }
        if let Some(selection) = self.selection() {
            self.history.delete(selection.start(), selection.len())?;
            self.cursor = selection.start();
            self.anchor = None;
        } else {
            self.history.delete(self.cursor, 1)?;
        }
        self.pending_nibble = None;
        self.clamp_bookmarks();
        Ok(())
    }

    /// Delete the selection, or the byte before the cursor.
    ///
    /// # Errors
    ///
    /// Whatever the edit reports.
    pub fn delete_backward(&mut self) -> Result<(), Error> {
        if !self.mode.edits() {
            return Ok(());
        }
        if self.selection().is_some() {
            return self.delete_forward();
        }
        if self.cursor == 0 {
            return Ok(());
        }
        self.cursor -= 1;
        self.history.delete(self.cursor, 1)?;
        self.pending_nibble = None;
        self.clamp_bookmarks();
        Ok(())
    }

    /// The bytes currently selected, or the byte under the cursor.
    ///
    /// # Errors
    ///
    /// Whatever reading reports.
    pub fn selected_bytes(&mut self) -> Result<Vec<u8>, Error> {
        let (at, len) = match self.selection() {
            Some(s) => (s.start(), s.len()),
            None => (self.cursor, 1.min(self.len().saturating_sub(self.cursor))),
        };
        self.history
            .buffer_mut()
            .read_bytes(at, usize::try_from(len).unwrap_or(0))
    }

    /// Remember this offset under a name.
    ///
    /// Bookmarking the same offset twice renames it rather than making a
    /// second entry for one place.
    pub fn add_bookmark(&mut self, offset: u64, label: impl Into<String>) {
        let offset = offset.min(self.len());
        let label = label.into();
        if let Some(existing) = self.bookmarks.iter_mut().find(|b| b.offset == offset) {
            existing.label = label;
        } else {
            self.bookmarks.push(Bookmark { offset, label });
            self.bookmarks.sort_by_key(|b| b.offset);
        }
    }

    /// Forget the bookmark at this offset.
    pub fn remove_bookmark(&mut self, offset: u64) {
        self.bookmarks.retain(|b| b.offset != offset);
    }

    /// Every bookmark, in file order.
    pub fn bookmarks(&self) -> &[Bookmark] {
        &self.bookmarks
    }

    /// A bookmark cannot point past the end of a file that got shorter.
    fn clamp_bookmarks(&mut self) {
        let len = self.history.buffer().len();
        for bookmark in &mut self.bookmarks {
            bookmark.offset = bookmark.offset.min(len);
        }
    }

    /// What would be written, as ranges and a total.
    ///
    /// Shown before saving. A count of changed bytes is not the same question
    /// as "did the file get longer", so both are here.
    pub fn summary(&self) -> Summary {
        let ranges = self.history.buffer().modified_ranges();
        Summary {
            ranges,
            edits: self.history.depth(),
            original_len: self.history.buffer().original_len(),
            new_len: self.history.buffer().len(),
        }
    }

    /// Write the file back.
    ///
    /// # Errors
    ///
    /// Whatever the filesystem reports.
    pub fn save(&mut self) -> Result<(), Error> {
        self.history.buffer_mut().save()
    }
}

/// What is about to be written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Summary {
    /// Where the changed bytes are, merged into runs.
    pub ranges: Vec<(u64, u64)>,
    /// How many separate edits were made.
    pub edits: usize,
    /// How long the file was when it was opened.
    pub original_len: u64,
    /// How long it will be once saved.
    pub new_len: u64,
}

impl Summary {
    /// How many bytes differ from the file on disk.
    pub fn changed_bytes(&self) -> u64 {
        self.ranges.iter().map(|(_, len)| len).sum()
    }

    /// Whether the file changed length.
    pub const fn resized(&self) -> bool {
        self.original_len != self.new_len
    }
}

/// So a search can run over the buffer without copying it.
impl Bytes for Session {
    fn len(&self) -> u64 {
        Self::len(self)
    }
    fn read_at(&mut self, offset: u64, len: usize) -> Result<Vec<u8>, Error> {
        self.history.buffer_mut().read_bytes(offset, len)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join("jtf-hexedit-session");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(name);
        let _ = std::fs::remove_file(&path);
        path
    }

    fn session(name: &str, contents: &[u8]) -> Session {
        let path = scratch(name);
        std::fs::write(&path, contents).unwrap();
        Session::open(&path).unwrap()
    }

    fn all(s: &mut Session) -> Vec<u8> {
        let len = usize::try_from(Session::len(s)).unwrap();
        s.buffer_mut().read_bytes(0, len).unwrap()
    }

    #[test]
    fn a_file_opens_read_only_so_a_stray_key_cannot_change_it() {
        let mut s = session("ro", b"hello");
        assert_eq!(s.mode(), Mode::ReadOnly);
        assert!(!s.type_hex_digit('4').unwrap());
        s.set_column(Column::Text);
        assert!(!s.type_byte(b'X').unwrap());
        s.delete_forward().unwrap();
        assert_eq!(all(&mut s), b"hello");
        assert!(!s.is_modified());
    }

    #[test]
    fn two_hex_digits_make_one_byte_and_the_first_alone_changes_nothing() {
        let mut s = session("nibble", b"\x00\x00");
        s.set_mode(Mode::Overwrite);

        assert!(s.type_hex_digit('4').unwrap());
        assert!(s.has_pending_nibble());
        assert_eq!(all(&mut s), b"\x00\x00", "half a byte was written");
        assert_eq!(s.cursor(), 0);

        assert!(s.type_hex_digit('1').unwrap());
        assert!(!s.has_pending_nibble());
        assert_eq!(all(&mut s), b"\x41\x00");
        assert_eq!(s.cursor(), 1, "the cursor did not step on to the next byte");
    }

    #[test]
    fn the_text_column_takes_one_keystroke_per_byte() {
        let mut s = session("textcol", b"aaaa");
        s.set_mode(Mode::Overwrite);
        s.set_column(Column::Text);
        s.type_byte(b'X').unwrap();
        s.type_byte(b'Y').unwrap();
        assert_eq!(all(&mut s), b"XYaa");
        assert_eq!(s.cursor(), 2);
    }

    #[test]
    fn overwrite_keeps_the_length_and_insert_grows_it() {
        let mut s = session("modes", b"abcd");
        s.set_mode(Mode::Overwrite);
        s.write(b"XY").unwrap();
        assert_eq!(all(&mut s), b"XYcd");
        assert_eq!(Session::len(&s), 4);

        s.move_to(0, false);
        s.set_mode(Mode::Insert);
        s.write(b"ZZ").unwrap();
        assert_eq!(all(&mut s), b"ZZXYcd");
        assert_eq!(Session::len(&s), 6);
    }

    #[test]
    fn switching_mode_drops_a_half_typed_byte() {
        let mut s = session("switch", b"\x00");
        s.set_mode(Mode::Overwrite);
        s.type_hex_digit('f').unwrap();
        assert!(s.has_pending_nibble());
        s.set_mode(Mode::Insert);
        assert!(!s.has_pending_nibble(), "half a byte survived a mode change");
        assert_eq!(all(&mut s), b"\x00");
    }

    #[test]
    fn a_selection_knows_its_range_whichever_way_it_was_dragged() {
        let mut s = session("sel", b"0123456789");
        s.move_to(7, false);
        s.move_to(2, true);
        let selection = s.selection().unwrap();
        assert_eq!((selection.start(), selection.end()), (2, 7));
        assert_eq!(selection.len(), 5);
    }

    #[test]
    fn typing_over_a_selection_replaces_it() {
        let mut s = session("replace", b"0123456789");
        s.set_mode(Mode::Insert);
        s.move_to(2, false);
        s.move_to(5, true);
        s.write(b"--").unwrap();
        assert_eq!(all(&mut s), b"01--56789");
        assert!(s.selection().is_none());
        assert_eq!(s.cursor(), 4);
    }

    #[test]
    fn backspace_and_delete_do_the_two_different_things() {
        let mut s = session("del", b"abcdef");
        s.set_mode(Mode::Insert);
        s.move_to(3, false);
        s.delete_forward().unwrap();
        assert_eq!(all(&mut s), b"abcef");
        s.delete_backward().unwrap();
        assert_eq!(all(&mut s), b"abef");
        assert_eq!(s.cursor(), 2);
    }

    #[test]
    fn backspace_at_the_start_does_nothing_rather_than_wrapping() {
        let mut s = session("del0", b"abc");
        s.set_mode(Mode::Insert);
        s.move_to(0, false);
        s.delete_backward().unwrap();
        assert_eq!(all(&mut s), b"abc");
        assert_eq!(s.cursor(), 0);
    }

    #[test]
    fn the_cursor_cannot_be_moved_past_the_end() {
        let mut s = session("clamp", b"abc");
        s.move_to(999, false);
        assert_eq!(s.cursor(), 3);
    }

    #[test]
    fn undo_and_redo_move_the_cursor_to_the_change() {
        let mut s = session("undo", b"abcdef");
        s.set_mode(Mode::Overwrite);
        s.move_to(4, false);
        s.write(b"Z").unwrap();
        assert_eq!(all(&mut s), b"abcdZf");

        s.undo();
        assert_eq!(all(&mut s), b"abcdef");
        assert_eq!(s.cursor(), 4);
        assert!(!s.is_modified());

        s.redo();
        assert_eq!(all(&mut s), b"abcdZf");
        assert_eq!(s.cursor(), 4);
    }

    #[test]
    fn a_bookmark_is_one_entry_per_offset_and_stays_inside_the_file() {
        let mut s = session("marks", b"0123456789");
        s.add_bookmark(8, "header");
        s.add_bookmark(2, "magic");
        s.add_bookmark(8, "trailer");
        assert_eq!(s.bookmarks().len(), 2, "one offset made two entries");
        assert_eq!(s.bookmarks()[0].offset, 2, "not in file order");
        assert_eq!(s.bookmarks()[1].label, "trailer");

        s.set_mode(Mode::Insert);
        s.move_to(0, false);
        s.move_to(10, true);
        s.delete_forward().unwrap();
        assert!(
            s.bookmarks().iter().all(|b| b.offset <= Session::len(&s)),
            "a bookmark points past the end of the file"
        );
    }

    #[test]
    fn the_summary_says_what_changed_and_whether_the_file_resized() {
        let mut s = session("summary", b"0123456789");
        s.set_mode(Mode::Overwrite);
        s.move_to(1, false);
        s.write(b"XY").unwrap();
        s.move_to(6, false);
        s.write(b"Z").unwrap();

        let summary = s.summary();
        assert_eq!(summary.changed_bytes(), 3);
        assert_eq!(summary.ranges, vec![(1, 2), (6, 1)]);
        assert_eq!(summary.edits, 2);
        assert!(!summary.resized());

        s.set_mode(Mode::Insert);
        s.write(b"!!").unwrap();
        assert!(s.summary().resized());
        assert_eq!(s.summary().new_len, 12);
    }

    #[test]
    fn saving_writes_the_bytes_and_clears_the_summary() {
        let mut s = session("save", b"hello world");
        s.set_mode(Mode::Overwrite);
        s.move_to(0, false);
        s.write(b"HELLO").unwrap();
        let path = s.buffer_mut().path().to_path_buf();
        s.save().unwrap();

        assert_eq!(std::fs::read(&path).unwrap(), b"HELLO world");
        assert!(!s.is_modified());
        assert_eq!(s.summary().changed_bytes(), 0);
    }

    #[test]
    fn a_search_runs_over_the_session_without_copying_the_file() {
        use crate::find::{find_forward, Kind, Needle};
        let mut s = session("search", b"....PK\x03\x04....");
        let needle = Needle::compile("50 4B 03 04", Kind::Hex).unwrap();
        assert_eq!(find_forward(&mut s, &needle, 0).unwrap(), Some(4));
    }

    #[test]
    fn a_search_sees_edits_that_have_not_been_saved_yet() {
        use crate::find::{find_forward, Kind, Needle};
        let mut s = session("search2", b"..........");
        s.set_mode(Mode::Overwrite);
        s.move_to(3, false);
        s.write(&[0xde, 0xad]).unwrap();
        let needle = Needle::compile("DE AD", Kind::Hex).unwrap();
        assert_eq!(
            find_forward(&mut s, &needle, 0).unwrap(),
            Some(3),
            "the search read the file instead of the buffer"
        );
    }
}
