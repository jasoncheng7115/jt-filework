# ADR-0003: Archive extraction and creation

- **Status:** Accepted — Option B, built
- **Decided:** 2026-08-31. The project owner asked for the work to continue
  without further questions, so the recommendation below was taken as the
  decision rather than left blocking.
- **Date:** 2026-08-30
- **Deciders:** project owner

## Context

Browsing inside an archive is built and shipping: `viewer/src/archive.rs`
reads a ZIP central directory with no decompressor at all, and the listing is
navigated like a folder. Taking things *out* is not built, and neither is
putting things in.

`CV.HLP` §四 gives CView's keys, and the project owner has confirmed the two
that matter here:

- `Z` on an archive — extract, asking where to
- `Alt-Z` — compress the marked files

`CV.HLP` §四 also gives the keys *inside* an archive listing, which had not
been read into this decision before and which change its shape:

| Key | CView | What it means here |
| --- | --- | --- |
| `ENTER` | 觀看檔案內容 | View one member — built already, the listing reader |
| `C` | 拷貝(解壓)檔案到你所指定(輸入)的目錄 | Extract the marked members to a chosen folder |
| `X` | 拷貝(解壓)壓縮檔內之全部檔案到你所指定(輸入)的目錄 | Extract everything |
| `D` | 刪除檔案 | Remove a member **from the archive** |
| `G` | 執行檔案 | Run a member |

This matters for three reasons.

First, `C` inside an archive is the same letter as `C` in the file list, and
means the same thing: copy these entries somewhere. Extraction is not a
separate concept in CView's keyboard — it is copy, out of a container. That is
the shape to build, not a command called "extract".

Second, `Z` on an archive row and `X` inside the listing are the same
operation reached from two places, so there is one implementation, not two.

Third, `D` writes to the archive. Removing a member means rewriting the file,
which is a strictly larger commitment than reading and than writing a *new*
archive, and it can lose data if interrupted. It is called out here so it is
scoped deliberately rather than arriving as an assumed part of "write ZIP".

Both are in the baseline list. Both need something the program does not have:
code that can decompress and compress. The listing reader deliberately does
not have it — reading a central directory is parsing a table of offsets, and
that is why it could be written to be safe against hostile input without
trusting a third-party decoder.

Two further constraints already hold and must keep holding:

- **Every entry name is hostile.** `ArchiveEntry` already carries
  `unsafe_name`, and the listing *marks* such a name rather than quietly
  normalizing it (`docs/SECURITY.md`). A member called `../../etc/passwd`, an
  absolute path, or a name that is a symlink pointing out of the tree must
  never be written outside the folder the user chose.
- **Extraction is a job.** It is long, it must show progress, it must be
  cancellable, and cancelling must leave something the user can reason about
  — the same contract copy and move already meet.

## Options

### A. Shell out to the platform's own tools

`ditto`/`tar` on macOS, `tar`/`unzip` on Linux, `tar.exe` and PowerShell's
`Expand-Archive` on Windows.

- No new dependency, no new attack surface inside this process.
- Formats follow whatever the machine happens to have, so behaviour differs
  between machines and cannot be tested once.
- Progress and cancellation are poor: these tools report little and are killed
  rather than asked to stop, so a cancelled extraction leaves an unknown
  partial state.
- Path safety is delegated to a tool whose version we do not control. `unzip`
  has had traversal bugs; we would be trusting each machine's copy.

### B. A Rust decompressor, in-process

`zip` (with `flate2`) covers ZIP; `tar` plus `flate2`/`xz2`/`zstd` covers the
tar family. Both are widely used and pure Rust when built without their C
backends.

- One behaviour on every platform, testable in CI, including against a corpus
  of hostile archives.
- Progress, cancellation and the conflict policy come free: extraction becomes
  another `Operation` alongside copy and move, and reuses the job engine, the
  progress bar and the queue panel that already exist.
- Path safety is ours to enforce, in the one place that already knows how to
  say no: every member is resolved against the destination and rejected if it
  escapes, and symlink members are refused rather than followed.
- Cost: new dependencies in the supply chain, and `flate2`'s default backend
  is C — it must be pinned to `rust_backend` to keep the "only the bridge has
  unsafe" property meaningful.

### C. Do not build it

Browsing stays; extraction is left to the platform's file manager.

- Honest, and the baseline list would have to lose two of its items.
- `Z` and `Alt-Z` stay unbound, which for a program whose whole point is
  CView's keyboard is a visible hole.

## Recommendation

**Option B**, with these conditions:

1. `zip` and `flate2` only, to start. Read ZIP, write a *new* ZIP. The tar
   family is a second step and a second decision.
2. Deleting a member (`D` in §四) is **out of scope** for the first step. It
   rewrites an existing archive in place, so an interruption can destroy data
   the user still has no other copy of, and none of the three keys the owner
   actually asked for need it.
3. `flate2` pinned to `features = ["rust_backend"], default-features = false`,
   so no C decoder enters the process.
4. Extraction is an `Operation`, planned by `Plan::build` like any other, so
   it inherits progress, cancellation, the conflict policy and the queue.
5. A member whose resolved destination is not inside the chosen folder is
   refused and reported, never written and never silently renamed. Symlink
   members are refused. This is tested against a corpus that includes
   `../` traversal, absolute paths, drive-relative Windows paths, and names
   that differ only by Unicode normalization.
6. Decompressed size is bounded per member and in total against the plan's
   own estimate, so an archive that claims to be 4 KB and expands to 40 GB is
   stopped rather than allowed to fill the disk.

## Consequences

The dependency count rises, and that is the real cost: this project has kept
its supply chain small on purpose. Set against that, a decompressor is the
only one of the three options that can be made safe on purpose rather than by
hoping, and it is the only one where "cancel" means something.

## What was built

Option B, with every condition met:

- `zip` with `default-features = false, features = ["deflate"]`, which is the
  feature that selects `flate2/rust_backend`. The default set drags in bzip2,
  lzma, xz and zstd, all of which are C. `cargo tree` shows no new C in the
  process from this change.
- `jtf-fs::archive` holds both directions. Extraction resolves every member
  against the destination and refuses anything that lands outside it, however
  it is spelled — `../`, an absolute path, a Windows drive letter or UNC
  prefix, and backslashes are treated as separators on every platform so that
  an archive built on Windows cannot be a traversal there and a strange
  filename here. Symlink members are refused rather than created. Refusals are
  counted and reported, never silent.
- Bounds: 8 GiB per member, 32 GiB in total, checked against what actually
  arrives rather than against the header, because a lying header is what a zip
  bomb is.
- Both run on a worker thread and are cancellable, and a cancelled extraction
  removes the partial file. There is no percentage: the member sizes are not
  known until they arrive, so what is shown is what has come out so far.
- `src/fs/tests/archive_safety.rs` is the corpus: traversal in five spellings,
  the backslash case, nested directories, cancellation, and a create/extract
  round trip.

`Z` reads the row it is on — an archive extracts, a folder is measured — which
is how `Enter` already behaves. `Alt-Z` only ever compresses.

**Not built, deliberately.** Deleting a member from an existing archive
(`CV.HLP` §四's `D`) rewrites the file in place and can destroy data if
interrupted; nothing the owner asked for needs it. The tar family, and reading
RAR, LZH or 7z, are a second decision — the listing reader in `jtf-viewer`
still only understands ZIP.
