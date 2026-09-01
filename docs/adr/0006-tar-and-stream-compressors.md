# ADR-0006: tar, and the gzip/bzip2/xz stream compressors

- **Status:** Accepted — Option B, building
- **Date:** 2026-08-31
- **Deciders:** project owner
- **Decided:** 2026-08-31. Asked for `.gz`, `.bz2` and `.xz` as a priority, was
  shown the trade-off below, and answered 「好 依你建議」.
- **Amends:** ADR-0003, condition 3 ("no C decoder enters the process") — kept,
  and this is the reasoning that keeps it.

## Context

ADR-0003 took ZIP and said in as many words that "the tar family is a second
step and a second decision". This is that decision.

What exists today is ZIP (deflate, read and write) and ISO 9660 (read).
`.gz`, `.bz2`, `.xz`, `.7z` and `.rar` are recognised by the content detector
as「壓縮檔」and then cannot be opened — a label the program cannot honour,
which is worse than no label.

Two things have to be separated, because they are usually said in one breath:

- **tar** is an archive format and carries no compression. `tar` 0.4.46 is pure
  Rust and reads and writes it.
- **gzip, bzip2 and xz** are stream compressors. On their own they hold a
  single file (`notes.txt.gz`); in practice they wrap a tar
  (`.tar.gz`, `.tgz`, `.tar.bz2`, `.tar.xz`).

So "support `.gz`" means two features: unwrap a single compressed file, and
read a tar through a decompressor.

### The dependency question

| crate | version | what it is |
| --- | --- | --- |
| `tar` | 0.4.46 | pure Rust, archive format only |
| `flate2` | 1.x, `rust_backend` | pure Rust gzip — already a dependency |
| `bzip2-rs` | 0.1.2 | pure Rust, **decompress only** |
| `bzip2` | 0.6.1 | binds C `libbz2`, both directions |
| `lzma-rs` | 0.3.0 | pure Rust xz, decompress plus a weak compressor |
| `xz2` | 0.1.7 | binds C `liblzma`, both directions |

For bzip2 and xz, every option breaks one of the two rules this project has
already written down:

- ADR-0003 §3: **no C decoder enters the process.** `bzip2` and `xz2` break it.
- ADR-0005: **do not take a thinly maintained parser of hostile binary input.**
  `bzip2-rs` at 0.1.2 and `lzma-rs` at 0.3.0 are exactly that shape.

## Options

### A. The C libraries (`bzip2`, `xz2`)

- Mature, deployed everywhere, fast, and they compress as well as decompress.
- They are C parsers of attacker-controlled input running in our address
  space. A bug is a memory-safety bug.
- `liblzma` is the library that shipped CVE-2024-3094, a deliberate backdoor,
  in 2024. That is not an argument that the code is bad; it is an argument
  about what the blast radius of this class of dependency is.
- Breaks the "only the bridge has unsafe" property that the whole architecture
  is arranged around.

### B. The pure-Rust decompressors (`bzip2-rs`, `lzma-rs`), read only

- Young and thinly used — the shape ADR-0005 refused for ISO.
- But the reason ADR-0005 refused it does not carry over. There, the worry was
  a C-style parser of a downloaded file: a bug is memory corruption. Here the
  code is safe Rust, so **a bug is a panic or wrong output, not a way in**.
  The failure mode is the whole argument, and it points the other way.
- The defence that actually matters is not in the decoder at all. A
  decompression bomb is stopped by bounding *what comes out* — 8 GiB per
  member, 32 GiB in total, counted against bytes actually read — and that
  bound holds whichever decoder produced them.
- Cost: no compression. `.tar.bz2` and `.tar.xz` can be read and not written.

### C. Shell out to `tar`

- Rejected for the reasons ADR-0003 gave: behaviour differs per machine,
  progress and cancellation are poor, and path safety would be delegated to a
  tool whose version we do not control.

### D. Do not build it

- The detector goes on labelling five formats as archives it cannot open.

## Recommendation

**Option B**, with these conditions:

1. **`tar` + `flate2` for `.tar.gz` and `.tgz`, both directions.** No new risk:
   `flate2` is already here and already pinned to its Rust backend.
2. **`bzip2-rs` and `lzma-rs` for reading `.tar.bz2` and `.tar.xz`.** Reading
   only.
3. **Creating is gzip only.** Not offering `.tar.bz2` / `.tar.xz` creation is
   deliberate, not an omission: the pure-Rust compressors are weak, and
   `.tar.gz` covers what anyone actually needs. Adding C for a feature nobody
   asks for is a bad trade.
4. **ADR-0003 §3 stands.** No C decoder enters the process, and this decision
   is what keeps it rather than an exception to it.
5. **Every bound from ADR-0003 applies unchanged**, and applies to the
   decompressed stream rather than to any header: 8 GiB per member, 32 GiB in
   total, path traversal refused in every spelling through the same
   `safe_destination`, symlink members refused.
6. **A tar entry is not trusted to say how big it is.** tar headers are
   attacker-controlled; the size that counts is what the reader produced.
7. **Streamed, never buffered whole.** A `.tar.xz` is read through the
   decompressor a block at a time. Reading a 10 GB archive must not need 10 GB
   of memory.
8. **On a worker thread, with progress and a working Cancel**, like every other
   long operation here — and a cancelled extraction removes its partial file.
9. **The content detector stops claiming what it cannot open.** `.7z` and
   `.rar` are no longer reported as archives this build can browse.

## Consequences

Three more dependencies, two of them young. That is the real cost, and it is
accepted because they are safe Rust: the worst a bad one can do is fail
loudly.

`.7z` and `.rar` remain unsupported. `.7z` needs an LZMA *container* reader
as well as the decoder and is its own decision; `.rar` is patent-encumbered
and has no usable pure-Rust implementation. Both will be reported honestly
rather than labelled as archives.

Writing `.tar.bz2` or `.tar.xz` is not possible and will not be offered. If
that is ever wanted, it is a new decision about C, taken on purpose.
