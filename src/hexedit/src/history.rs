//! Undo and redo.
//!
//! Each edit is recorded with everything needed to go both ways: what was
//! there before and what is there now. The "before" has to be read *before*
//! the edit runs, which is why every method here wraps the buffer's rather
//! than being called after it.
//!
//! The buffer is not snapshotted. A snapshot per keystroke, on a file being
//! edited a byte at a time, is the one thing that would undo the point of the
//! piece table.

use jtf_core::Error;

use crate::buffer::{Buffer, Removed};

/// One reversible change, holding both sides of it.
///
/// What was removed is kept as the runs it was, not as bytes: putting runs
/// back restores where each byte came from, and re-typing the bytes would
/// leave undone work looking like fresh edits.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Edit {
    Inserted {
        at: u64,
        bytes: Vec<u8>,
    },
    Deleted {
        at: u64,
        removed: Removed,
    },
    /// Not stored as a delete followed by an insert, even though the buffer
    /// performs it that way: as one entry it undoes in one step, which is
    /// what someone who typed one byte expects Ctrl-Z to do.
    ///
    /// `removed` can be shorter than `after` — overwriting at the very end of
    /// the file extends it — so both lengths are carried rather than assumed
    /// equal.
    Overwrote {
        at: u64,
        removed: Removed,
        after: Vec<u8>,
    },
}

/// A buffer and everything done to it.
pub struct History {
    buffer: Buffer,
    done: Vec<Edit>,
    undone: Vec<Edit>,
}

impl History {
    /// Start with an unedited buffer.
    pub const fn new(buffer: Buffer) -> Self {
        Self {
            buffer,
            done: Vec::new(),
            undone: Vec::new(),
        }
    }

    /// The buffer, for reading.
    pub const fn buffer(&self) -> &Buffer {
        &self.buffer
    }

    /// The buffer, for reading rows and for saving.
    pub const fn buffer_mut(&mut self) -> &mut Buffer {
        &mut self.buffer
    }

    /// Whether there is anything to undo.
    pub fn can_undo(&self) -> bool {
        !self.done.is_empty()
    }

    /// Whether there is anything to redo.
    pub fn can_redo(&self) -> bool {
        !self.undone.is_empty()
    }

    /// How many changes have been made and not undone.
    pub fn depth(&self) -> usize {
        self.done.len()
    }

    /// Insert bytes, remembering how to take them back out and put them back.
    pub fn insert(&mut self, at: u64, bytes: &[u8]) {
        if bytes.is_empty() {
            return;
        }
        let at = at.min(self.buffer.len());
        self.buffer.insert(at, bytes);
        self.record(Edit::Inserted {
            at,
            bytes: bytes.to_vec(),
        });
    }

    /// Delete bytes, remembering what they were.
    ///
    /// # Errors
    ///
    /// Whatever reading the bytes about to be deleted reports.
    pub fn delete(&mut self, at: u64, len: u64) -> Result<(), Error> {
        if len == 0 || at >= self.buffer.len() {
            return Ok(());
        }
        let removed = self.buffer.take(at, len);
        self.record(Edit::Deleted { at, removed });
        Ok(())
    }

    /// Overwrite bytes, remembering what was underneath.
    ///
    /// # Errors
    ///
    /// Whatever reading the bytes about to be replaced reports.
    pub fn overwrite(&mut self, at: u64, bytes: &[u8]) -> Result<(), Error> {
        if bytes.is_empty() {
            return Ok(());
        }
        let at = at.min(self.buffer.len());
        let overlap = (self.buffer.len() - at).min(bytes.len() as u64);
        let removed = self.buffer.take(at, overlap);
        self.buffer.insert(at, bytes);
        self.record(Edit::Overwrote {
            at,
            removed,
            after: bytes.to_vec(),
        });
        Ok(())
    }

    fn record(&mut self, edit: Edit) {
        self.done.push(edit);
        // A new edit after undoing abandons the redo branch: redo would be
        // replaying a change against bytes that are no longer the ones it was
        // recorded against.
        self.undone.clear();
    }

    /// Undo the last change. Returns where it happened, for the cursor.
    pub fn undo(&mut self) -> Option<u64> {
        let edit = self.done.pop()?;
        let at = match &edit {
            Edit::Inserted { at, bytes } => {
                self.buffer.delete(*at, bytes.len() as u64);
                *at
            }
            Edit::Deleted { at, removed } => {
                self.buffer.put(*at, removed);
                *at
            }
            Edit::Overwrote { at, removed, after } => {
                self.buffer.delete(*at, after.len() as u64);
                self.buffer.put(*at, removed);
                *at
            }
        };
        self.undone.push(edit);
        Some(at)
    }

    /// Redo the last undone change. Returns where it happened.
    pub fn redo(&mut self) -> Option<u64> {
        let edit = self.undone.pop()?;
        let at = match &edit {
            Edit::Inserted { at, bytes } => {
                self.buffer.insert(*at, bytes);
                *at
            }
            Edit::Deleted { at, removed } => {
                self.buffer.delete(*at, removed.len());
                *at
            }
            Edit::Overwrote { at, removed, after } => {
                self.buffer.delete(*at, removed.len());
                self.buffer.insert(*at, after);
                *at
            }
        };
        self.done.push(edit);
        Some(at)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("jtf-hexedit-history-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(name);
        let _ = std::fs::remove_file(&path);
        path
    }

    fn history(name: &str, contents: &[u8]) -> History {
        let path = scratch(name);
        std::fs::write(&path, contents).unwrap();
        History::new(Buffer::open(&path).unwrap())
    }

    fn all(h: &mut History) -> Vec<u8> {
        let len = usize::try_from(h.buffer().len()).unwrap();
        h.buffer_mut().read_bytes(0, len).unwrap()
    }

    #[test]
    fn undo_puts_back_each_kind_of_edit() {
        let mut h = history("kinds", b"hello world");

        h.overwrite(0, b"HELLO").unwrap();
        assert_eq!(all(&mut h), b"HELLO world");
        h.undo();
        assert_eq!(all(&mut h), b"hello world");

        h.insert(5, b",");
        assert_eq!(all(&mut h), b"hello, world");
        h.undo();
        assert_eq!(all(&mut h), b"hello world");

        h.delete(5, 6).unwrap();
        assert_eq!(all(&mut h), b"hello");
        h.undo();
        assert_eq!(all(&mut h), b"hello world");

        assert!(!h.can_undo());
        assert!(!h.buffer().is_modified(), "undone all the way is unmodified");
    }

    #[test]
    fn redo_replays_each_kind_of_edit() {
        let mut h = history("redo", b"hello world");

        h.insert(0, b">>");
        h.overwrite(2, b"HELLO").unwrap();
        h.delete(7, 6).unwrap();
        let after_all = all(&mut h);

        h.undo();
        h.undo();
        h.undo();
        assert_eq!(all(&mut h), b"hello world");
        assert!(h.can_redo());

        h.redo();
        h.redo();
        h.redo();
        assert_eq!(all(&mut h), after_all, "redo did not land where undo left");
        assert!(!h.can_redo());
    }

    #[test]
    fn a_long_run_of_edits_undoes_all_the_way_back_and_redoes_all_the_way_out() {
        let mut h = history("many", b"0123456789");
        for i in 0..20u8 {
            if i % 3 == 0 {
                h.insert(u64::from(i) % 5, &[b'a' + i]);
            } else if i % 3 == 1 {
                h.overwrite(u64::from(i) % 7, &[b'A' + i]).unwrap();
            } else {
                h.delete(u64::from(i) % 4, 1).unwrap();
            }
        }
        let end = all(&mut h);
        assert_eq!(h.depth(), 20);

        while h.can_undo() {
            h.undo();
        }
        assert_eq!(all(&mut h), b"0123456789");
        assert!(!h.buffer().is_modified());

        while h.can_redo() {
            h.redo();
        }
        assert_eq!(all(&mut h), end);
    }

    #[test]
    fn undoing_gives_the_bytes_back_their_provenance_not_just_their_values() {
        // The bug this guards: undo used to re-insert the old bytes, which
        // filed them as typed-this-session. The content was right and every
        // one of them was still coloured as an edit, and the file went on
        // asking to be saved after being undone all the way back.
        let mut h = history("provenance", b"hello world");
        h.overwrite(0, b"HELLO").unwrap();
        h.undo();

        let read = h.buffer_mut().read(0, 11).unwrap();
        assert_eq!(
            read.iter().map(|b| b.value).collect::<Vec<_>>(),
            b"hello world".to_vec()
        );
        assert!(
            read.iter().all(|b| !b.modified),
            "bytes that were put back still read as edited"
        );
        assert!(!h.buffer().is_modified());
        assert!(h.buffer().modified_ranges().is_empty());
    }

    #[test]
    fn a_deleted_stretch_comes_back_unedited_too() {
        let mut h = history("provenance2", b"0123456789");
        h.delete(3, 4).unwrap();
        h.undo();
        let read = h.buffer_mut().read(0, 10).unwrap();
        assert!(read.iter().all(|b| !b.modified));
        assert!(!h.buffer().is_modified());
    }

    #[test]
    fn editing_after_an_undo_abandons_the_redo_branch() {
        let mut h = history("branch", b"abc");
        h.overwrite(0, b"X").unwrap();
        h.undo();
        assert!(h.can_redo());
        h.overwrite(1, b"Y").unwrap();
        assert!(
            !h.can_redo(),
            "redo would replay against bytes it was not recorded against"
        );
        assert_eq!(all(&mut h), b"aYc");
    }

    #[test]
    fn undoing_an_overwrite_that_extended_the_file_shortens_it_again() {
        let mut h = history("extend", b"ab");
        h.overwrite(1, b"XYZ").unwrap();
        assert_eq!(all(&mut h), b"aXYZ");
        h.undo();
        assert_eq!(all(&mut h), b"ab", "the extension was left behind");
        h.redo();
        assert_eq!(all(&mut h), b"aXYZ");
    }

    #[test]
    fn undo_reports_where_it_happened_so_the_cursor_can_follow() {
        let mut h = history("where", b"0123456789");
        h.overwrite(4, b"X").unwrap();
        assert_eq!(h.undo(), Some(4));
        assert_eq!(h.redo(), Some(4));
        assert_eq!(h.undo(), Some(4));
        assert_eq!(h.undo(), None);
    }
}
