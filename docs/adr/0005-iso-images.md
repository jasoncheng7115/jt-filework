# ADR-0005: Browsing and extracting ISO images

- **Status:** Accepted — Option B, built
- **Date:** 2026-08-31
- **Deciders:** project owner
- **Decided:** 2026-08-31. Asked directly — 「我們現在有檢視壓縮檔的能力，那麼
  .iso 映像檔呢」 — and, on being offered this decision, answered 「好」.

## Context

Pressing Enter on a ZIP opens a listing window and `C`/`X` take members out of
it (ADR-0003). An `.iso` is the same thing to a user: a single file that holds
a filesystem, sitting in the list next to the archives. Today it is an opaque
file — the archive window refuses it, `Z` does nothing, and the only way in is
to leave the program and mount it.

An ISO image is a disc filesystem, not an archive, and that difference is
mostly in our favour:

- **Nothing is compressed.** A file inside an ISO is a contiguous run of
  2048-byte sectors. Reading one out is a bounded copy, not a decode. There is
  no decompressor to trust and no zip bomb to bound against — the extracted
  size is the stored size.
- **The structure is a table of offsets**, exactly like a ZIP central
  directory: a Primary Volume Descriptor at sector 16 names the root directory
  record, and each directory record gives an extent, a length, a flags byte
  and a name.
- **But it is a tree, not a flat list.** A ZIP names every member in one table;
  an ISO has to be walked, directory by directory. A walk over untrusted
  offsets is where the danger is: a record can point at its own parent, or at
  a sector outside the file, or claim a length that does not fit.

Three extensions matter in practice, and ignoring them is not an option if the
listing is to be usable:

| Extension | What it adds | Who uses it |
| --- | --- | --- |
| ISO 9660 base | `FILENAME.EXT;1`, uppercase, 8.3-ish | everything |
| Joliet | UCS-2 names up to 64 characters | Windows install media, most burned discs |
| Rock Ridge | POSIX names, permissions, symlinks | Linux distribution images |

Without Joliet, a Windows ISO lists as `SETUP.EXE;1` and a folder of documents
lists as mangled uppercase. Without Rock Ridge, a Linux ISO is readable but
loses its real names.

The constraints from ADR-0003 carry over unchanged and are the reason this is
a decision and not just a task:

- **Every name in the image is hostile.** A record can be called `../../etc/
  passwd`. `ArchiveEntry::unsafe_name` already exists to carry that fact to
  extraction rather than quietly normalizing it away.
- **Extraction is a job**: long, cancellable, and it must not write outside the
  folder the user chose.
- **No unbounded allocation from a header field** (`docs/SECURITY.md` §13).

## Options

### A. A third-party ISO crate

`iso9660`, `cdfs`, or similar.

- The parsing is written and (perhaps) tested by someone else.
- The candidates are small, thinly maintained crates with few users — this is
  not `serde`. A parser of untrusted binary input with three reverse
  publications and no fuzzing corpus is a worse bet than one we can read in an
  afternoon.
- The safety property we need is not "does it parse valid ISOs" but "what does
  it do with an invalid one", and that is exactly what such a crate is least
  likely to have been exercised on. We would inherit its panics as our
  crashes.
- It would be the first dependency in this project that parses hostile input
  in-process. `zip` decompresses, which is why ADR-0003 accepted it; nothing
  else does.

### B. Our own reader, in `jtf-viewer` and `jtf-fs`

The same shape as `viewer/src/archive.rs`, which reads a ZIP central directory
without a decompressor precisely so it could be made safe on purpose.

- ISO 9660 is a simpler format than the ZIP directory in every respect except
  that it is a tree. The base descriptor, Joliet and Rock Ridge `NM` together
  are a few hundred lines.
- Every bound is ours to state and to test: sectors checked against the file's
  real length, depth capped, entry count capped, name length capped, visited
  extents remembered so a cycle terminates.
- No new dependency, and the C-free property of the process is untouched.
- Cost: it is our bug if a real image will not open. Mitigated by the fact
  that the failure mode is "this ISO lists nothing", not "this ISO corrupted
  something" — reading is not writing.

### C. Mount it and browse the mount point

`hdiutil attach` on macOS, `udisksctl loop-setup` on Linux, `Mount-DiskImage`
on Windows.

- Free, and gives the real filesystem including UDF and hybrid images.
- Needs privileges on Linux, differs on every platform, and leaves state
  behind: a mount that outlives the program is a thing the user has to clean
  up, and a crash guarantees one.
- It is a side effect on the machine, which browsing a file should not be.
- Worth revisiting *as an explicit action* — 「掛載」 as a command — but not as
  what pressing Enter does.

### D. Do not build it

- Honest. `.iso` stays an opaque file.
- The owner asked for it directly.

## Recommendation

**Option B**, with these conditions:

1. **Read only.** Listing and extracting out. Nothing writes into an image;
   there is no equivalent of `Alt-Z` for ISO, and `D` (delete a member) is out
   of scope for the same reason it is in ADR-0003, doubled.
2. **ISO 9660 with Joliet and Rock Ridge `NM`.** Joliet is preferred when the
   image has it, because that is the name the person who made the image
   intended. Rock Ridge `NM` overrides within the base tree. UDF is **not**
   built: it is a second format of comparable size, and a UDF-only image will
   report that it cannot be read rather than list nothing and look empty.
3. **Every offset checked against the file's real length before use**, every
   sector read bounded, and no allocation sized from a header field.
4. **The walk is iterative and bounded**: a queue rather than recursion
   (`AGENTS.md` §20.2), a depth cap, an entry cap, and a set of visited extents
   so a directory that points at its own ancestor terminates instead of
   producing entries forever.
5. **Names are resolved for safety, not normalized.** The same
   `unsafe_name` flag the ZIP listing sets, set by the same rule, so extraction
   refuses the same things through the same code path — `safe_destination` in
   `jtf-fs::archive` is reused rather than reimplemented.
6. **Extraction reuses the archive job**: a worker thread, a running count, a
   working Cancel, and a cancelled extraction removes the partial file.
7. A corpus of hostile images is built in the test, not downloaded: truncated
   files, a record pointing outside the file, a directory cycle, a traversal
   name, and a lying length.

## Consequences

`.iso` joins `.zip` as something the archive window opens, with the same keys,
which is the point — CView's §四 keys do not know what a container is made of.

The reader is ours to maintain, and an image that will not open is our bug.
That is the accepted cost of not putting an unmaintained binary parser in the
path of a file the user downloaded from the internet.

UDF images and hybrid discs that carry only a UDF tree will not list. They are
reported as unreadable rather than shown as empty, which is the difference
between a limitation and a lie.

## What was built

Option B, with every condition met:

- `jtf-viewer::iso` reads the volume descriptors, prefers a Joliet
  supplementary descriptor when one is present, and walks the tree
  iteratively. Rock Ridge `NM` entries in the system-use area override the
  base name. Entries come back as the same `ArchiveEntry` the ZIP listing
  produces, so the archive window, the preview and the extraction path did not
  change shape.
- Bounds: `MAX_ENTRIES` 100 000, `MAX_DEPTH` 32, `MAX_DIRECTORY_BYTES` 16 MiB
  for a single directory's extent, and a `visited` set of extents so a cycle
  ends. Every extent is checked against the file's real length before a read
  is attempted.
- `jtf-fs::iso::extract` copies members out through
  `archive::safe_destination`, the same function the ZIP path uses, so a
  traversal name is refused identically in both.
- `src/viewer/tests/iso_listing.rs` builds images byte by byte: a valid one, a
  Joliet one, a truncated one, one whose record points past the end, one with
  a directory cycle, and one with a `../` name.
