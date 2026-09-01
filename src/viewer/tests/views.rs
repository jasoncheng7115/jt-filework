//! Behaviour of the text and hex views.
//!
//! The interesting cases are the ones a naive implementation gets wrong: a
//! file with no trailing newline, mixed line endings, a legacy encoding, one
//! enormous line, and a file far larger than memory.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use jtf_jobs::CancellationToken;
use jtf_viewer::{Encoding, HexView, LineEnding, TextView, ROW_BYTES};

static COUNTER: AtomicU64 = AtomicU64::new(0);

fn write(name: &str, bytes: &[u8]) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    let unique = COUNTER.fetch_add(1, Ordering::SeqCst);
    let dir =
        std::env::temp_dir().join(format!("jtf-view-{}-{nanos}-{unique}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join(name);
    fs::write(&path, bytes).unwrap();
    path
}

/// `line 0\nline 1\n...`, built without formatting into a String per line.
fn numbered_lines(count: usize) -> String {
    use std::fmt::Write as _;
    let mut out = String::with_capacity(count * 12);
    for i in 0..count {
        let _ = writeln!(out, "line {i}");
    }
    out
}

fn open(path: &Path) -> TextView {
    TextView::open(path, &CancellationToken::never()).unwrap()
}

/// A file larger than the bound is presented as its first part, and says so.
///
/// Without a bound the index is eight bytes per line over the whole file and
/// the pass that builds it reads every byte, so opening a multi-gigabyte log
/// froze on the read and then held hundreds of megabytes of offsets.
#[test]
fn a_large_file_is_indexed_only_as_far_as_the_bound() {
    // 10 bytes a line, so the bound lands mid-file at a known place.
    let text = numbered_lines(1000);
    let path = write("big.txt", text.as_bytes());
    let full = text.len() as u64;

    let bounded = TextView::open_bounded(&path, 512, &CancellationToken::never()).unwrap();
    assert!(
        !bounded.is_complete(),
        "512 bytes of a larger file is a part"
    );
    assert_eq!(bounded.size(), 512, "only the bound is addressable");
    assert_eq!(bounded.full_size(), full, "the real size is still reported");
    assert!(
        bounded.line_count() < 1000,
        "indexing stopped at the bound, not at the end of the file"
    );
    // Eight bytes a line: the index must be bounded too, not merely the read.
    assert!(
        bounded.index_bytes() <= 8 * 100,
        "index grew past the bound"
    );

    let whole = TextView::open_bounded(&path, full * 2, &CancellationToken::never()).unwrap();
    assert!(whole.is_complete());
    assert_eq!(whole.line_count(), 1000);
    assert_eq!(whole.size(), full);
}

/// A file that fits is complete, and nothing about it changes.
#[test]
fn a_small_file_is_complete() {
    let path = write(
        "small.txt",
        b"one
two
",
    );
    let view = open(&path);
    assert!(view.is_complete());
    assert_eq!(view.size(), view.full_size());
}

#[test]
fn counts_lines_and_reads_a_window() {
    let path = write("a.txt", b"one\ntwo\nthree\n");
    let mut view = open(&path);

    assert_eq!(view.line_count(), 3);
    let window = view.window(0, 10).unwrap();
    assert_eq!(window.lines, vec!["one", "two", "three"]);
}

#[test]
fn a_file_without_a_trailing_newline_keeps_its_last_line() {
    let path = write("a.txt", b"one\ntwo");
    let mut view = open(&path);
    assert_eq!(view.line_count(), 2);
    assert_eq!(view.window(0, 10).unwrap().lines, vec!["one", "two"]);
}

#[test]
fn a_window_starts_where_it_was_asked_to() {
    let content = numbered_lines(1000);
    let path = write("a.txt", content.as_bytes());
    let mut view = open(&path);

    let window = view.window(500, 3).unwrap();
    assert_eq!(window.first_line, 500);
    assert_eq!(window.lines, vec!["line 500", "line 501", "line 502"]);
}

#[test]
fn line_endings_are_reported_and_not_normalized_away() {
    // docs/VIEWER_PREVIEW.md 4.1: a file with mixed endings is telling you
    // something.
    assert_eq!(
        open(&write("a.txt", b"a\nb\n")).line_ending(),
        LineEnding::Lf
    );
    assert_eq!(
        open(&write("b.txt", b"a\r\nb\r\n")).line_ending(),
        LineEnding::Crlf
    );
    assert_eq!(
        open(&write("c.txt", b"a\rb\r")).line_ending(),
        LineEnding::Cr
    );
    assert_eq!(
        open(&write("d.txt", b"a\nb\r\n")).line_ending(),
        LineEnding::Mixed
    );
    assert_eq!(
        open(&write("e.txt", b"no break at all")).line_ending(),
        LineEnding::None
    );
}

#[test]
fn crlf_lines_are_shown_without_the_carriage_return() {
    let path = write("a.txt", b"one\r\ntwo\r\n");
    let mut view = open(&path);
    assert_eq!(view.window(0, 10).unwrap().lines, vec!["one", "two"]);
}

#[test]
fn utf8_is_detected_and_cjk_survives() {
    let path = write("a.txt", "第一行\n第二行\n".as_bytes());
    let mut view = open(&path);
    assert_eq!(view.detected_encoding(), Encoding::Utf8);
    assert_eq!(view.window(0, 2).unwrap().lines, vec!["第一行", "第二行"]);
}

#[test]
fn a_big5_file_reads_correctly_once_the_encoding_is_chosen() {
    // The encoding this project's users have the most legacy files in. Auto
    // detection cannot tell Big5 from other legacy bytes, which is exactly why
    // the override exists.
    let (bytes, _, _) = encoding_rs::BIG5.encode("中文測試\n第二行\n");
    let path = write("big5.txt", &bytes);
    let mut view = open(&path);

    assert_ne!(
        view.window(0, 2).unwrap().lines[0],
        "中文測試",
        "not readable as detected"
    );

    view.set_encoding(Encoding::Big5);
    assert_eq!(view.window(0, 2).unwrap().lines, vec!["中文測試", "第二行"]);
    assert_eq!(view.effective_encoding(), Encoding::Big5);

    view.set_encoding(Encoding::Auto);
    assert_eq!(
        view.effective_encoding(),
        view.detected_encoding(),
        "Auto returns to detection"
    );
}

#[test]
fn a_utf8_bom_is_detected() {
    let mut bytes = vec![0xef, 0xbb, 0xbf];
    bytes.extend_from_slice("hello\n".as_bytes());
    assert_eq!(
        open(&write("bom.txt", &bytes)).detected_encoding(),
        Encoding::Utf8
    );
}

#[test]
fn malformed_bytes_produce_replacement_characters_rather_than_a_refusal() {
    // A viewer that refuses to show a file because one byte is wrong is less
    // useful than one that shows the byte as U+FFFD.
    let path = write("bad.txt", b"good \xff\xfe bad\n");
    let mut view = open(&path);
    let window = view.window(0, 1).unwrap();
    assert_eq!(window.lines.len(), 1);
    assert!(window.lines[0].starts_with("good"));
}

#[test]
fn one_enormous_line_does_not_hang_and_reports_truncation() {
    // docs/UI_TEST_PLAN.md VIEW-004.
    let huge = vec![b'x'; 4 << 20];
    let path = write("long.txt", &huge);
    let mut view = open(&path);

    assert_eq!(view.line_count(), 1);
    let window = view.window(0, 1).unwrap();
    assert!(
        window.truncated,
        "the caller must know it is not seeing everything"
    );
    assert!(window.lines[0].len() <= (1 << 20) + 8);
}

#[test]
fn opening_a_large_file_costs_an_index_and_not_the_file() {
    // 200k lines: proof the cost is the index, and that the index size is
    // reported rather than hidden.
    let content = numbered_lines(200_000);
    let path = write("big.txt", content.as_bytes());
    let mut view = open(&path);

    assert_eq!(view.line_count(), 200_000);
    assert_eq!(view.index_bytes(), 200_000 * 8);
    // Reading the far end is the same cost as reading the near end.
    assert_eq!(view.window(199_999, 1).unwrap().lines, vec!["line 199999"]);
}

#[test]
fn indexing_is_cancellable() {
    let content = numbered_lines(200_000);
    let path = write("big.txt", content.as_bytes());
    let error = TextView::open(&path, &CancellationToken::cancelled()).unwrap_err();
    assert_eq!(error.code(), jtf_core::ErrorCode::Cancelled);
}

#[test]
fn an_empty_file_is_not_an_error() {
    let path = write("empty.txt", b"");
    let mut view = open(&path);
    assert_eq!(view.size(), 0);
    assert!(view.window(0, 10).unwrap().lines.is_empty());
}

#[test]
fn asking_past_the_end_returns_nothing_rather_than_failing() {
    let path = write("a.txt", b"one\n");
    let mut view = open(&path);
    assert!(view.window(9999, 10).unwrap().lines.is_empty());
}

// ------------------------------------------------------------------ hex

#[test]
fn hex_reads_a_window_and_formats_rows() {
    let path = write("bytes.bin", b"ABCDEFGHIJKLMNOP0123456789abcdef");
    let mut view = HexView::open(&path).unwrap();

    assert_eq!(view.size(), 32);
    assert_eq!(view.row_count(), 2);

    let window = view.window(0, 1).unwrap();
    assert_eq!(window.bytes.len(), ROW_BYTES);
    let rows = window.rows();
    assert_eq!(rows.len(), 1);
    assert!(rows[0].starts_with("00000000  41 42 43"), "got {}", rows[0]);
    assert!(rows[0].ends_with("|ABCDEFGHIJKLMNOP|"), "got {}", rows[0]);
}

#[test]
fn hex_shows_unprintable_bytes_as_dots_without_losing_their_values() {
    let path = write("bytes.bin", &[0x00, 0x01, 0xff, b'A']);
    let mut view = HexView::open(&path).unwrap();
    let rows = view.window(0, 1).unwrap().rows();
    assert!(rows[0].contains("00 01 ff 41"), "got {}", rows[0]);
    assert!(rows[0].ends_with("|...A|"), "got {}", rows[0]);
}

#[test]
fn a_partial_final_row_is_read_without_reading_past_the_end() {
    let path = write("bytes.bin", b"12345");
    let mut view = HexView::open(&path).unwrap();
    let window = view.window(0, 4).unwrap();
    assert_eq!(window.bytes, b"12345");
}

#[test]
fn hex_past_the_end_is_empty_rather_than_an_error() {
    let path = write("bytes.bin", b"12345");
    let mut view = HexView::open(&path).unwrap();
    assert!(view.window(9999, 4).unwrap().bytes.is_empty());
}
