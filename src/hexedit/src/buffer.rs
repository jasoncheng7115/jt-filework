//! The bytes being edited, and where each one came from.
//!
//! A piece table over the file rather than a `Vec<u8>` of it. Inserting one
//! byte at the front of a 2 GB file must not copy 2 GB, and opening that file
//! must not read it either — the table holds *descriptions* of runs, and the
//! bytes are fetched from the file only for the rows actually on screen.
//!
//! The other thing the table gives for free is the answer to "which bytes did
//! I change". A run either points into the original file or into the bytes
//! this session typed, so a byte is modified exactly when it comes from the
//! latter. No separate set of dirty ranges to keep in step through inserts and
//! deletes — the shifting is the table's job already, and a second record of
//! it would be a second thing to get wrong.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use jtf_core::{Error, ErrorCode};

/// Where a run of bytes lives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Source {
    /// The file as it was when it was opened.
    Original,
    /// Bytes this session added, in `Buffer::added`.
    Added,
}

/// One run of consecutive bytes from one source.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Piece {
    source: Source,
    /// Offset within the source.
    start: u64,
    len: u64,
}

/// A file open for editing.
pub struct Buffer {
    path: PathBuf,
    file: File,
    /// The file's length when it was opened. Runs from `Original` are indexed
    /// against this, and it never changes while the buffer is open.
    original_len: u64,
    /// Everything typed this session, appended and never removed: a piece
    /// that stops pointing at a stretch of this leaves it behind rather than
    /// compacting it, because undo may want it back and a stable index is
    /// worth more than the bytes.
    added: Vec<u8>,
    pieces: Vec<Piece>,
}

/// A stretch that was taken out, held as the runs it was made of.
///
/// Undo puts this back rather than re-inserting the bytes. Re-inserting would
/// file them as typed-this-session, so undoing an overwrite left the original
/// bytes looking edited and the file reading as still unsaved - correct
/// content, wrong provenance, and provenance is what the colouring and the
/// "is it modified" question are both built on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Removed {
    pieces: Vec<Piece>,
}

impl Removed {
    /// How many bytes it holds.
    pub fn len(&self) -> u64 {
        self.pieces.iter().map(|p| p.len).sum()
    }

    /// Whether it holds none.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// One byte, and whether it is one of the ones that changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Byte {
    /// The byte itself.
    pub value: u8,
    /// True when this byte was written this session rather than read from the
    /// file. What the display colours differently.
    pub modified: bool,
}

impl Buffer {
    /// Open a file for editing.
    ///
    /// # Errors
    ///
    /// Whatever the filesystem reports.
    pub fn open(path: &Path) -> Result<Self, Error> {
        let file = File::open(path)
            .map_err(|e| Error::new(ErrorCode::Io, format!("{}: {e}", path.display())))?;
        let original_len = file
            .metadata()
            .map_err(|e| Error::new(ErrorCode::Io, e.to_string()))?
            .len();
        let pieces = if original_len == 0 {
            Vec::new()
        } else {
            vec![Piece {
                source: Source::Original,
                start: 0,
                len: original_len,
            }]
        };
        Ok(Self {
            path: path.to_path_buf(),
            file,
            original_len,
            added: Vec::new(),
            pieces,
        })
    }

    /// The file this buffer came from.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// How many bytes the buffer currently holds.
    pub fn len(&self) -> u64 {
        self.pieces.iter().map(|p| p.len).sum()
    }

    /// Whether the buffer holds nothing.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The length the file had when it was opened.
    pub const fn original_len(&self) -> u64 {
        self.original_len
    }

    /// Whether anything has been changed since opening.
    ///
    /// Asked of the table rather than tracked with a flag: a buffer that was
    /// edited and then undone all the way back is not modified, and a flag
    /// would have to be told that.
    pub fn is_modified(&self) -> bool {
        // The runs have to tile the original file, in order, with nothing
        // added. Counting the runs instead was wrong: undoing an edit leaves
        // the original split at the seams where the edit was, which describes
        // exactly the same bytes and used to read as still dirty - so a file
        // edited and then undone all the way back still asked to be saved.
        let mut expected = 0u64;
        for piece in &self.pieces {
            if piece.source != Source::Original || piece.start != expected {
                return true;
            }
            expected += piece.len;
        }
        expected != self.original_len
    }

    /// Read `len` bytes from `offset`, with each byte's provenance.
    ///
    /// Short at the end of the buffer rather than an error: a window near the
    /// end asks for a whole screenful and gets what is there.
    ///
    /// # Errors
    ///
    /// Whatever reading the file reports.
    pub fn read(&mut self, offset: u64, len: usize) -> Result<Vec<Byte>, Error> {
        let mut out = Vec::with_capacity(len);
        if len == 0 {
            return Ok(out);
        }
        let mut want = len as u64;
        let mut cursor = 0u64;

        // Cloned first: reading from `self.file` needs `&mut self`, and the
        // pieces are small.
        let pieces = self.pieces.clone();
        for piece in pieces {
            if want == 0 {
                break;
            }
            let end = cursor + piece.len;
            if end <= offset {
                cursor = end;
                continue;
            }
            // How far into this piece the wanted range starts.
            let skip = offset.saturating_sub(cursor);
            let take = (piece.len - skip).min(want);
            match piece.source {
                Source::Original => {
                    let mut bytes = vec![0u8; usize::try_from(take).unwrap_or(0)];
                    self.file
                        .seek(SeekFrom::Start(piece.start + skip))
                        .map_err(|e| Error::new(ErrorCode::Io, e.to_string()))?;
                    self.file
                        .read_exact(&mut bytes)
                        .map_err(|e| Error::new(ErrorCode::Io, e.to_string()))?;
                    out.extend(bytes.into_iter().map(|value| Byte {
                        value,
                        modified: false,
                    }));
                }
                Source::Added => {
                    let from = usize::try_from(piece.start + skip).unwrap_or(0);
                    let to = from + usize::try_from(take).unwrap_or(0);
                    out.extend(self.added[from..to].iter().map(|value| Byte {
                        value: *value,
                        modified: true,
                    }));
                }
            }
            want -= take;
            cursor = end;
        }
        Ok(out)
    }

    /// Read `len` bytes from `offset`, values only.
    ///
    /// # Errors
    ///
    /// Whatever reading the file reports.
    pub fn read_bytes(&mut self, offset: u64, len: usize) -> Result<Vec<u8>, Error> {
        Ok(self.read(offset, len)?.into_iter().map(|b| b.value).collect())
    }

    /// Split the run containing `offset` so that a run boundary falls there.
    ///
    /// Returns the index of the first piece at or after `offset`. Every edit
    /// starts here, because with a boundary in the right place an insert is a
    /// `Vec::insert` and a delete is a `Vec::drain`.
    fn split_at(&mut self, offset: u64) -> usize {
        let mut cursor = 0u64;
        for index in 0..self.pieces.len() {
            let piece = self.pieces[index];
            if cursor == offset {
                return index;
            }
            let end = cursor + piece.len;
            if offset < end {
                let left = offset - cursor;
                self.pieces[index] = Piece {
                    len: left,
                    ..piece
                };
                self.pieces.insert(
                    index + 1,
                    Piece {
                        source: piece.source,
                        start: piece.start + left,
                        len: piece.len - left,
                    },
                );
                return index + 1;
            }
            cursor = end;
        }
        self.pieces.len()
    }

    /// Join runs that describe consecutive bytes of the same source.
    ///
    /// Every edit splits a run, and undoing puts the halves back beside each
    /// other. Without this the table grows by two entries per edit-and-undo
    /// and never shrinks, which costs on every read.
    fn coalesce(&mut self) {
        let mut merged: Vec<Piece> = Vec::with_capacity(self.pieces.len());
        for piece in self.pieces.drain(..) {
            match merged.last_mut() {
                Some(last)
                    if last.source == piece.source && last.start + last.len == piece.start =>
                {
                    last.len += piece.len;
                }
                _ => merged.push(piece),
            }
        }
        self.pieces = merged;
    }

    /// Insert `bytes` at `offset`, moving everything after it along.
    pub fn insert(&mut self, offset: u64, bytes: &[u8]) {
        if bytes.is_empty() {
            return;
        }
        let offset = offset.min(self.len());
        let at = self.split_at(offset);
        let start = self.added.len() as u64;
        self.added.extend_from_slice(bytes);
        self.pieces.insert(
            at,
            Piece {
                source: Source::Added,
                start,
                len: bytes.len() as u64,
            },
        );
        self.coalesce();
    }

    /// Remove `len` bytes at `offset`, pulling everything after it back.
    ///
    /// Stops at the end of the buffer rather than erroring, so deleting "the
    /// rest" does not need the length measured first.
    pub fn delete(&mut self, offset: u64, len: u64) {
        if len == 0 || offset >= self.len() {
            return;
        }
        let len = len.min(self.len() - offset);
        let from = self.split_at(offset);
        let to = self.split_at(offset + len);
        self.pieces.drain(from..to);
        self.coalesce();
    }

    /// Remove `len` bytes at `offset` and hand back what was there.
    ///
    /// The same as [`Self::delete`] except that the runs come back, so they
    /// can be put where they were with [`Self::put`].
    pub fn take(&mut self, offset: u64, len: u64) -> Removed {
        if len == 0 || offset >= self.len() {
            return Removed { pieces: Vec::new() };
        }
        let len = len.min(self.len() - offset);
        let from = self.split_at(offset);
        let to = self.split_at(offset + len);
        let pieces: Vec<Piece> = self.pieces.drain(from..to).collect();
        self.coalesce();
        Removed { pieces }
    }

    /// Put a taken stretch back at `offset`.
    pub fn put(&mut self, offset: u64, removed: &Removed) {
        if removed.pieces.is_empty() {
            return;
        }
        let offset = offset.min(self.len());
        let at = self.split_at(offset);
        for (i, piece) in removed.pieces.iter().enumerate() {
            self.pieces.insert(at + i, *piece);
        }
        self.coalesce();
    }

    /// Replace `bytes.len()` bytes at `offset` without changing the length.
    ///
    /// Past the end it extends, which is what typing at the last byte of a
    /// file in overwrite mode has to mean - the alternative is a keystroke
    /// that does nothing and does not say why.
    pub fn overwrite(&mut self, offset: u64, bytes: &[u8]) {
        if bytes.is_empty() {
            return;
        }
        let offset = offset.min(self.len());
        let overlap = (self.len() - offset).min(bytes.len() as u64);
        self.delete(offset, overlap);
        self.insert(offset, bytes);
    }

    /// The ranges that differ from the file on disk, in buffer coordinates.
    ///
    /// For the summary shown before saving. Adjacent runs are merged, so a
    /// byte typed after another byte reads as one change of two rather than
    /// as two changes.
    pub fn modified_ranges(&self) -> Vec<(u64, u64)> {
        let mut out: Vec<(u64, u64)> = Vec::new();
        let mut cursor = 0u64;
        for piece in &self.pieces {
            if piece.source == Source::Added {
                match out.last_mut() {
                    Some(last) if last.0 + last.1 == cursor => last.1 += piece.len,
                    _ => out.push((cursor, piece.len)),
                }
            }
            cursor += piece.len;
        }
        out
    }

    /// Write the buffer back over the file it came from.
    ///
    /// Through a temporary in the same directory and a rename, so a failure
    /// part way leaves the original file intact rather than half rewritten.
    /// The same directory because a rename across filesystems is a copy, and
    /// a copy is not atomic.
    ///
    /// # Errors
    ///
    /// Whatever the filesystem reports.
    pub fn save(&mut self) -> Result<(), Error> {
        use std::io::Write as _;

        // A chunk at a time: the whole point of the piece table is that the
        // file was never held in memory, and saving must not be the one
        // operation that does.
        const CHUNK: usize = 1 << 20;

        let parent = self.path.parent().unwrap_or_else(|| Path::new("."));
        let name = self
            .path
            .file_name()
            .map_or_else(|| "buffer".to_string(), |n| n.to_string_lossy().into_owned());
        let temporary = parent.join(format!(".{name}.jtf-hexedit"));

        {
            let file = File::create(&temporary).map_err(|e| {
                Error::new(ErrorCode::Io, format!("{}: {e}", temporary.display()))
            })?;
            let mut out = std::io::BufWriter::new(file);
            let total = self.len();
            let mut at = 0u64;
            while at < total {
                let take = usize::try_from((total - at).min(CHUNK as u64)).unwrap_or(0);
                let bytes = self.read_bytes(at, take)?;
                out.write_all(&bytes)
                    .map_err(|e| Error::new(ErrorCode::Io, e.to_string()))?;
                at += take as u64;
            }
            out.flush()
                .map_err(|e| Error::new(ErrorCode::Io, e.to_string()))?;
            out.get_ref()
                .sync_all()
                .map_err(|e| Error::new(ErrorCode::Io, e.to_string()))?;
        }

        std::fs::rename(&temporary, &self.path).map_err(|e| {
            let _ = std::fs::remove_file(&temporary);
            Error::new(ErrorCode::Io, format!("{}: {e}", self.path.display()))
        })?;

        // Reopened, so the buffer now describes the file as just written and
        // nothing reads as modified any more.
        *self = Self::open(&self.path.clone())?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join("jtf-hexedit-buffer");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(name);
        let _ = std::fs::remove_file(&path);
        path
    }

    fn buffer(name: &str, contents: &[u8]) -> Buffer {
        let path = scratch(name);
        std::fs::write(&path, contents).unwrap();
        Buffer::open(&path).unwrap()
    }

    fn all(b: &mut Buffer) -> Vec<u8> {
        let len = usize::try_from(b.len()).unwrap();
        b.read_bytes(0, len).unwrap()
    }

    #[test]
    fn an_untouched_buffer_reads_back_the_file_and_is_not_modified() {
        let mut b = buffer("plain", b"hello world");
        assert_eq!(all(&mut b), b"hello world");
        assert_eq!(b.len(), 11);
        assert!(!b.is_modified());
        assert!(b.modified_ranges().is_empty());
    }

    #[test]
    fn reading_past_the_end_stops_rather_than_failing() {
        let mut b = buffer("short", b"abc");
        assert_eq!(b.read_bytes(1, 99).unwrap(), b"bc");
        assert_eq!(b.read_bytes(3, 4).unwrap(), b"");
    }

    #[test]
    fn overwrite_replaces_in_place_and_marks_only_those_bytes() {
        let mut b = buffer("over", b"hello world");
        b.overwrite(6, b"WORLD");
        assert_eq!(all(&mut b), b"hello WORLD");
        assert_eq!(b.len(), 11, "overwrite changed the length");
        assert_eq!(b.modified_ranges(), vec![(6, 5)]);

        let read = b.read(0, 11).unwrap();
        assert!(read[..6].iter().all(|x| !x.modified));
        assert!(read[6..].iter().all(|x| x.modified));
    }

    #[test]
    fn insert_moves_the_tail_along_without_marking_it() {
        let mut b = buffer("ins", b"hello");
        b.insert(0, b">> ");
        assert_eq!(all(&mut b), b">> hello");
        assert_eq!(b.modified_ranges(), vec![(0, 3)]);
        let read = b.read(0, 8).unwrap();
        assert!(
            read[3..].iter().all(|x| !x.modified),
            "bytes that only moved were reported as changed"
        );
    }

    #[test]
    fn delete_pulls_the_tail_back() {
        let mut b = buffer("del", b"hello world");
        b.delete(5, 6);
        assert_eq!(all(&mut b), b"hello");
        assert!(b.is_modified());
        assert!(
            b.modified_ranges().is_empty(),
            "nothing was added, so nothing should be coloured as added"
        );
    }

    #[test]
    fn deleting_more_than_is_there_stops_at_the_end() {
        let mut b = buffer("del2", b"abc");
        b.delete(1, 999);
        assert_eq!(all(&mut b), b"a");
    }

    #[test]
    fn overwriting_at_the_very_end_extends_rather_than_doing_nothing() {
        let mut b = buffer("ext", b"ab");
        b.overwrite(2, b"cd");
        assert_eq!(all(&mut b), b"abcd");
    }

    #[test]
    fn edits_compose_in_any_order_and_the_bytes_stay_right() {
        let mut b = buffer("mix", b"0123456789");
        b.insert(5, b"---");
        b.overwrite(0, b"X");
        b.delete(9, 2);
        // 0123456789 -> 01234---56789 -> X1234---56789 -> X1234---589
        assert_eq!(all(&mut b), b"X1234---589");
    }

    #[test]
    fn an_empty_file_can_be_typed_into() {
        let mut b = buffer("empty", b"");
        assert!(b.is_empty());
        b.insert(0, b"new");
        assert_eq!(all(&mut b), b"new");
        assert!(b.is_modified());
    }

    #[test]
    fn saving_writes_the_bytes_and_leaves_nothing_looking_modified() {
        let mut b = buffer("save", b"hello world");
        b.overwrite(0, b"HELLO");
        b.save().unwrap();

        let on_disk = std::fs::read(b.path()).unwrap();
        assert_eq!(on_disk, b"HELLO world");
        assert!(!b.is_modified(), "a just-saved buffer still reads as dirty");
        assert!(b.modified_ranges().is_empty());
        assert_eq!(b.original_len(), 11);
    }

    #[test]
    fn saving_after_growing_and_shrinking_writes_the_right_length() {
        let mut b = buffer("save2", b"0123456789");
        b.delete(0, 5);
        b.insert(0, b"abcdefghij");
        b.save().unwrap();
        assert_eq!(std::fs::read(b.path()).unwrap(), b"abcdefghij56789");
    }

    #[test]
    fn saving_leaves_no_temporary_behind() {
        let mut b = buffer("save3", b"xyz");
        b.overwrite(1, b"Y");
        b.save().unwrap();
        // This file's own temporary, not any temporary: the other tests write
        // into the same directory at the same time and a shared scan is a
        // test that fails depending on scheduling.
        let temporary = b.path().parent().unwrap().join(".save3.jtf-hexedit");
        assert!(!temporary.exists(), "left behind: {}", temporary.display());
    }

    #[test]
    fn a_large_edit_does_not_have_to_read_the_whole_file() {
        // The point of the piece table. A megabyte file, one byte changed at
        // the front, and the table stays three runs long.
        let mut b = buffer("big", &vec![7u8; 1 << 20]);
        b.insert(0, b"!");
        assert_eq!(b.len(), (1 << 20) + 1);
        assert!(b.pieces.len() <= 3, "table grew to {}", b.pieces.len());
        assert_eq!(b.read_bytes(0, 3).unwrap(), b"!\x07\x07");
    }
}
