# jt-filework v0.6.52

> A keyboard-first, mouse-complete file workspace for macOS, Windows and Linux.

**[Website](https://jasoncheng7115.github.io/jt-filework/)** ·
**[Full specification](https://jasoncheng7115.github.io/jt-filework/features.html)** ·
[繁體中文說明](README_zh-TW.md)

In Single-Key mode every common file command is one letter — copy, move,
rename, view, mark — with no chord to hold down and no menu to hunt through.
That keyboard sits on top of a modern native file manager: recursive pane
splitting, independent tabs in every pane, real previews, archives you can
browse, remote folders over SFTP, and a disc usage report that says which
*kind* of file is filling the disk.

By Jason Cheng (Jason Tools).

**Status: 0.6.52.** Runs on macOS, Windows and Linux. Installers for all
three are on the [Releases](https://github.com/jasoncheng7115/jt-filework/releases)
page - not signed yet, see [Download](#download) - or build it from source.
778 tests pass.

---

## What it does today

Everything below is built and working, not planned.

### The keyboard

Two profiles, both data files rather than code (`keymaps/`):

- **Single-Key** — one letter, one command. `C` copies, `M` moves, `D` moves to
  the trash, `V` views, `Z` measures a folder, `P` or `\` types a path, `Space`
  marks. The keys you can press right now are named along the foot of the
  window, and they change with what the cursor is on.
- **Native** — the platform's own chords, for when muscle memory says `Cmd-C`.

A binding is a line in a text file. Adding one does not need a rebuild of
anything but the file.

### Panes, tabs and layout

- Recursive split tree — not two fixed panes. Split any pane horizontally or
  vertically, as deep as you like.
- Every pane owns independent tabs. A tab can be torn off into its own window
  or dragged into another pane.
- The folder tree follows whichever pane has the keyboard, including onto a
  server.

### Selection is the mark

Selecting a row marks it, and marking a row selects it. They were two separate
concepts and are now one, because keeping them apart meant Shift-selecting six
files and finding none of them ticked.

### Files

Copy, move, rename, duplicate, trash, delete, new folder, new file, batch
rename. Every operation is planned before it runs, shows progress, can be
cancelled, and reports conflicts rather than guessing. There is an undo.

### Archives and images

| Format | Browse | Extract | Create |
| --- | --- | --- | --- |
| ZIP | ✅ | ✅ | ✅ |
| `.tar`, `.tar.gz` / `.tgz` | ✅ | ✅ | ✅ |
| `.tar.bz2`, `.tar.xz` | ✅ | ✅ | — |
| bare `.gz` / `.bz2` / `.xz` | ✅ | ✅ | — |
| ISO 9660 (+ Joliet, Rock Ridge) | ✅ | ✅ | — read only |

Pressing Enter on any of them opens a listing window: `Space` marks, `C`
extracts what is marked, `X` extracts everything.

Every reader treats its input as hostile. A member whose name would land
outside the folder you chose is **refused and reported** — never written,
never silently renamed — in every spelling: `../`, an absolute path, a Windows
drive letter, a UNC prefix, backslashes on any platform. Symlink members are
refused rather than created. Expansion is bounded against bytes that actually
arrive, not against what a header claims, because a lying header is what a
decompression bomb is.

The ISO reader is ours rather than a crate's (ADR-0005), and the compressors
are pure Rust rather than C bindings (ADR-0006). Both decisions are about the
same thing: a bug in a parser of a downloaded file should be a panic, not a
way in.

`.7z` and `.rar` are **not** supported, and are not labelled as archives this
build can open.

### An image onto a USB stick

Only removable, external disks that are not carrying the running system are ever
offered; a disk whose properties could not be read does not appear at all. The
order is unmount, write, flush, read back, compare — and the comparison is on by
default, because a failing stick and a counterfeit one both accept every write
and hand back something else. The privilege is borrowed for the one operation:
`authopen` on macOS, so nothing here ever runs as root, and `pkexec` on Linux.

### Remote folders

SFTP, with the host key checked and remembered, keys from your agent or
`~/.ssh`, and a password prompt when neither works. A password is held in
memory for the process lifetime, dropped on disconnect, and never written to
disk (ADR-0004).

Browsing is built. Uploading, downloading, renaming and deleting on the server
are stage two and are not yet built.

### Looking at things

- **Preview panel** and **viewer window**: text with encoding detection and
  line-ending detection, hex, images, archive listings.
- Large files are bounded on purpose: the preview indexes the first 4 MiB and
  the viewer 64 MiB, and both say when they are showing part of a file. The
  index is eight bytes per line and building it reads every byte, so an
  unbounded one meant a multi-gigabyte log froze the window and held hundreds
  of megabytes before showing a single line.
- **Quick Look** on macOS is the real `QLPreviewPanel`, the same panel Finder
  shows. Windows and Linux have no system equivalent, so the command is
  hidden there rather than offered and inert.

### Finding things

- **Filter** narrows the current folder as you type; matches are picked out.
- **Search** walks subfolders, streaming results as they arrive.
- **Compare folders** puts two panes side by side and lists what differs:
  only here, only there, different, identical. Subfolders optional. Compared
  by size and modification time — a rule printed under the table rather than
  implied.
- **Disc usage** answers two questions from one walk: which child folder holds
  the most, and **which kind of file** adds up to the most. The second is the
  half most tools cannot answer. Copy, move and trash work from inside the
  report, and it measures the level again once the operation finishes.

### The rest

- Light, dark and follow-system themes. Every colour comes from a semantic
  token resolved in Rust; there is no literal colour anywhere in the C++.
- English and 台灣繁體中文, with every user-visible string in a catalogue. A
  missing key is a test failure, not a label nobody notices.
- Bookmarks, recent places, volumes, servers — in a sidebar beside the folder
  tree.

---

## Download

From [Releases](https://github.com/jasoncheng7115/jt-filework/releases):

| Platform | File | Notes |
|---|---|---|
| macOS, Apple silicon | `jt-filework-<version>-macos-arm64.dmg` | Drag it to Applications. Qt is inside. |
| Windows, x64 | `jt-filework-<version>-windows-x64.msi` | Installs to Program Files, with a Start menu entry. |
| Windows, x64, no install | `jt-filework-<version>-windows-x64.zip` | Unpack anywhere and run `jt-filework.exe`. |
| Debian and Ubuntu, x64 | `jt-filework_<version>_amd64.deb` | `sudo apt install ./jt-filework_<version>_amd64.deb` |

`SHA256SUMS` lists the checksum of every file. Tested on macOS 15, Windows 11 and
Ubuntu 22.04. The macOS build declares macOS 14 as its minimum, the version
Qt itself needs; the `.deb` is built on 22.04 so that later releases can
install it, but neither claim has been tried on another machine yet.
There is no Intel Mac build.

> **These builds are not signed yet.** jt-filework does not have an Apple
> Developer ID or a Windows code-signing certificate, so your system cannot
> tell who made the file and will warn you before opening it. That warning
> is the system doing its job. Before going past it, check that the file's
> SHA-256 checksum matches the one in `SHA256SUMS` beside it.
>
> **macOS**
>
> 1. Open the `.dmg` and drag **jt-filework** into **Applications**.
> 2. Open jt-filework from Applications. macOS says **“jt-filework” Not
>    Opened**, because Apple could not verify it. Click **Done**.
> 3. Open **System Settings → Privacy & Security**, scroll down to
>    **Security**, and click **Open Anyway** beside the line about
>    jt-filework.
> 4. Confirm with your password or Touch ID, and choose **Open Anyway** if
>    macOS asks once more.
>
> This is needed once. Do not turn Gatekeeper off, and do not run commands
> that remove the quarantine attribute from files in general.
>
> **Windows**
>
> 1. If the browser says the file is not commonly downloaded, choose to keep
>    it.
> 2. Open the `.msi`. SmartScreen shows **Windows protected your PC**: click
>    **More info**, check that the file name is the one you downloaded, then
>    **Run anyway**.
> 3. Allow the installer to make changes when Windows asks, and follow it
>    through. jt-filework is then in the Start menu.
>
> The `.zip` needs no installing: unpack it and run `jt-filework.exe`, and
> SmartScreen asks the same question the first time. Do not turn SmartScreen
> off.
>
> **Debian and Ubuntu**
>
> ```text
> sudo apt install ./jt-filework_<version>_amd64.deb
> ```
>
> There is no warning to get past; apt installs the Qt libraries it needs.

The installers are made by the scripts in `packaging/`.

## Building

The Rust toolchain is pinned to 1.98.0 by `rust-toolchain.toml`; `rustup` will
fetch it. Build output goes outside the source tree
(`~/.cache/jt-filework-target`, `~/.cache/jt-filework-qt`), so a checkout in a
synced folder does not re-upload gigabytes after every build.

### macOS

```bash
brew install qt cmake
./src/ui/qt6/build.sh release
```

A release build also installs to `/Applications/jt-filework.app`.

### Linux

```bash
sudo apt install build-essential cmake ninja-build pkg-config \
     qt6-base-dev libqt6svg6-dev libgl1-mesa-dev
./src/ui/qt6/build.sh release
```

Ubuntu 22.04 ships Qt 6.2 and 24.04 ships 6.4; both build. Following the
desktop's light/dark setting *while running* needs Qt 6.5 or newer — on older
Qt the theme is read once at launch.

Build on the oldest distribution you intend to support: glibc's compatibility
runs one way, so a binary built on 22.04 runs on 24.04 and not the reverse.

### Windows

Needs MSVC Build Tools, CMake, Qt 6, and **NASM** (`aws-lc-sys`, which arrives
through the SFTP stack, assembles its own primitives). See
`docs/DEVELOPMENT_ENVIRONMENT.md` for the full runbook — it exists because
that machine has been rebuilt from backup once already.

### Tests

```bash
cargo test --workspace
cargo clippy --workspace --all-targets
```

The suite includes architecture tests that fail the build if core logic
depends on the GUI toolkit, if a platform `#cfg` appears outside the platform
layer, if a keymap binding reaches no handler, or if a user-visible string is
missing from either language.

---

## What is not built

Named rather than left to be discovered:

- SFTP writes (upload, download, rename, delete on the server).
- `.7z` and `.rar`.
- Quick Look, "reveal in file manager", eject and external editor on Windows
  and Linux — the adapters are stubs and report that they cannot, so the
  commands are hidden rather than inert.
- A batch command over marked files, and "repeat find" — the latter
  deliberately: `F` is a filter rather than a find, so "find next" has no
  meaning in this model.

---

## Documentation

Design decisions live in `docs/adr/`; the reasoning for anything surprising is
usually there rather than in the code.

| | |
| --- | --- |
| `AGENTS.md` | The rules this codebase is held to |
| `docs/ARCHITECTURE.md` | Crate layout and boundaries |
| `docs/SECURITY.md` | Untrusted input, and what is done about it |
| `docs/DEVELOPMENT_ENVIRONMENT.md` | Building on all three platforms |
| `docs/adr/` | Why the GUI stack, archives, SFTP, ISO and tar are what they are |
| `CHANGELOG.md` | What changed, and why |

## Licence

`GPL-3.0-or-later`. Copyright Jason Cheng (Jason Tools).
