# ADR-0008: A terminal front end, and the application layer it shares

- **Status:** Accepted — step 1 under way: the layer moved in 0.6.62
- **Date:** 2026-10-04
- **Deciders:** project owner
- **Asked for:** 2026-10-04, as "the modern CView for the Linux command line":
  the same single-key handling, viewers, archives and SFTP, usable over SSH on
  a machine with no desktop at all. The owner added the second half - 「在沒有
  xwindow desktop 的 linux 也要可以安裝使用」 - and it is a hard requirement
  below, not a preference.
- **Decided:** 2026-10-04. The command is 「jtf-tui」; the other three were
  answered with the recommendations: package `jt-filework-tui`, the shared
  layer first and the terminal screens after the platform adapters,
  Single-Key only, no root-only device commands in the first version.

## Context

jt-filework today is one front end, Qt 6 Widgets, over a Rust core. A large
share of the work people do with files happens where that front end cannot
go: logged in to a server over SSH, at a machine with no X or Wayland, inside
a container. CView itself was a text-mode program; a terminal is where its
single-key handling makes the most sense.

What exists already, and is the reason this is cheaper than it looks:

| Wanted in a terminal | Already in the core |
|---|---|
| listing, sort, filter, hidden files | `jtf-fs`, `jtf-workspace` |
| marks: Space, `*`, `+`, `-`, all, none | `jtf-workspace`, the bridge's `App` |
| copy, move, rename, trash, delete, new, duplicate, conflicts, undo | `jtf-ops`, `jtf-jobs` |
| text viewer for any size, Big5 and the rest, hex view and edit | `jtf-viewer`, `jtf-hexedit` |
| search by name, glob, regex, size, date, kind | `jtf-search` |
| archives browsed and extracted, with the safety rules | `jtf-fs`, `jtf-viewer` (ADR-0003, ADR-0006) |
| SFTP, and copying across it | `jtf-fs`, `jtf-transfer` (ADR-0004) |
| folder sizes, watching folders | `jtf-fs`, `jtf-platform-watch` (ADR-0007) |
| the CView keymap | `jtf-commands`, `keymaps/single-key.keymap` |

What is not shared yet is the layer between those and a window: which pane is
active, what the marks resolve to, what a command acts on, the operation
queue, the watcher, the viewer session. That layer is the `App` in
`src/ui/qt6/bridge/src/` - `app.rs`, `operations.rs`, `transfer.rs`,
`hexedit.rs`, about 8,000 lines - and **none of it uses a C type**. It is
UI-neutral code in a crate named after Qt. Only `ffi.rs` is the C ABI.

Some behaviour is in C++ and would have to be written twice:

- turning a key in the file list into a command (`PaneWidget::chordFor` and
  the switch around it), CV.HLP's Shift+letter jump, Shift-Up/Down, Space's
  step after marking;
- which keys the key strip offers for what the bar is on
  (`keyhintbar.cpp`, `kNothing` / `kFile` / `kFolder` / `kSeveral`);
- which commands a platform or a location can perform
  (`kLocalOnlyCommands`, `platformCan` in `mainwindow.cpp`).

Every selection bug fixed in 0.6.58 and 0.6.59 was in that C++ layer, where
no Rust test reaches.

### No desktop is a hard requirement

The `.deb` built today depends on Qt 6, `qt6-qpa-plugins` and `libqt6svg6`,
and recommends `qt6-wayland`. On a server that is several hundred megabytes of
toolkit and X client libraries to install for nothing. Two situations count as
"no desktop", and both have to work:

- **SSH from elsewhere** - the common case. The terminal on the other end
  draws the text: CJK, colour and mouse depend on it, not on the server.
- **The machine's own text console** (`TERM=linux`). The kernel console font
  holds at most 512 glyphs and **cannot show Chinese**; it has 16 colours and,
  without gpm, no mouse.

Also in scope: a container shell where `TERM` is unset or `dumb`, an 80x24
terminal, `LANG=C` with file names that are not UTF-8, running as root, and a
user who may not install packages at all.

## Options Considered

### A. A separate project

Start again with ratatui. Duplicates every rule the core already settles and
every bug already fixed; two programs disagree about what `C` copies within a
month. Rejected.

### B. A terminal front end over the Qt bridge crate

`jtf-tui` depends on `jtf-qt6-bridge` and uses its `App`. Nothing to move,
but the crate is the Qt bridge: its name, its build and its C ABI travel with
it, and the dependency direction ADR-0002 sets - nothing depends on a UI - is
broken on the first line. Rejected.

### C. Extract the application layer; two front ends over it (chosen)

## Decision

**Option C**, in two steps that are separate commits and separate releases.

### Step 1 - `jtf-app`

A new crate at `src/app`, `jtf-app`, holds what the bridge's `App` holds
today: `app.rs`, `operations.rs`, `transfer.rs` and `hexedit.rs` move there
unchanged except for visibility. `jtf-qt6-bridge` keeps `ffi.rs` and becomes
a C ABI over `jtf-app`. The window's behaviour does not change, and that is
how this step is checked: the 822 tests, and the Linux checks of
`UI_TEST_PLAN` MARK-027 and MARK-037 to MARK-045 run again.

With it, the C++ behaviour listed above moves into `jtf-app`, so both front
ends take it from one place and Rust tests cover it:

- key to command for the file list, including Shift+letter, Shift-arrows and
  Space-and-step;
- the key strip's choice of keys for the row the bar is on;
- what each command is available for, by platform and by location.

Drawing, column fitting, drag images and widgets stay in C++.

The locale catalogues and the keymap presets are **compiled into `jtf-app`**.
The files in `locales/` and `keymaps/` stay the source, and `JTF_REPO_ROOT`
still points a development build at them; a shipped binary no longer needs
files beside it.

Dependency direction, extending ADR-0002:

```text
ui/qt6 (C++ + jtf-qt6-bridge) --\
                                 +-> jtf-app -> commands -> workspace -> ...
ui/tui (jtf-tui) ---------------/
```

`jtf-app` depends on no GUI toolkit and no desktop service. Neither front end
depends on the other.

### Step 2 - `jtf-tui`

A crate at `src/ui/tui`, on **ratatui and crossterm**. No async runtime of its
own: the loop polls crossterm with a frame-length timeout and pumps `App` the
way the Qt window's timers do (listing every frame, the watcher every 250 ms).
Tokio stays where it is, inside the SFTP provider.

**No desktop, enforced.** The binary links nothing but libc, and there is a
statically linked musl build with no dependency at all. It must not need X,
Wayland, D-Bus, polkit, udisks, GVFS, a systemd user session or fontconfig.
Commands that only a desktop can carry out are replaced or left out:

| Command | In the window | In the terminal |
|---|---|---|
| Open | the platform's default application | the built-in viewer, or `$EDITOR` / `$PAGER` |
| Open With | the platform's list | commands the user lists in the settings |
| Quick Look | Quick Look | the preview pane |
| Clipboard | the system clipboard | files: the program's own; a path or text: OSC 52, so it reaches the clipboard of the machine the person is sitting at |
| Terminal here | a new terminal window | suspend, `$SHELL`, resume; and on quit, the shell can follow to the folder last shown |
| Eject, write an image | `udisksctl`, `pkexec` | not in the first version; both need root, and neither helper is usually present |

**The terminal it finds.**

- `TERM=linux`: the interface in English whatever the locale says, 16
  colours, ASCII in place of box-drawing and symbols.
- `TERM` unset or `dumb`: no colour, no cursor addressing beyond what
  crossterm requires; refuse with a sentence rather than draw garbage if even
  that is missing.
- Narrower than the tree, list and preview need: the tree and then the
  preview fold away, the list stays.
- `NO_COLOR` is honoured. Truecolour, 256 and 16 colours are chosen from what
  the terminal reports, and the theme tokens map to each.
- Width is measured in cells, with East Asian Wide characters as two. East
  Asian Ambiguous characters are one by default and two by a setting, because
  terminals disagree.

**Text from files is data** (`AGENTS.md` §20.3). A file name or a log line can
carry `ESC [`: drawn as it is, it can retitle the terminal, clear the screen
or, in some terminals, feed input back. Every string that reaches the screen
passes one function that shows C0 and C1 controls, DEL and the bidirectional
overrides (U+202A-U+202E, U+2066-U+2069) as visible escapes. ratatui is not
assumed to do this. The viewer shows ANSI colour codes in a log as text in the
first version; interpreting SGR safely is a later, separate decision.

**Keys.** The first version ships the Single-Key keymap only: bare letters
are exactly what a terminal sends reliably. A chord a terminal cannot
distinguish - Ctrl+Shift+letter, Ctrl+I from Tab, Ctrl+M from Enter - is not
bound in the terminal. Where the terminal speaks the kitty keyboard protocol,
crossterm is asked to enable it. Keys a terminal emulator commonly keeps for
itself (F10, F11) each have a letter as well.

**Settings and session.** Under the same `jt-filework` configuration
directory, in a session file of the terminal's own with its own format
version (`docs/UPGRADE.md`). `user.keymap` is shared, since it is the same
keymap. Sharing bookmarks and the rest with the window is a later decision.

**Packaging.** A tarball of the static binary for x86_64 and aarch64, a `.deb`
and an `.rpm` that depend on libc and nothing else, and the binary usable from
`~/.local/bin` with no installation at all. `.github/workflows/release.yml`
builds them beside the existing three.

**First version.** One pane and two side by side, tabs per pane, the folder
tree, the list with sort, filter and hidden files, marks, copy, move, rename,
trash, delete, new file and folder, duplicate - with conflicts, progress,
cancel and undo - the text and hex viewers including files larger than
memory, the preview pane, filter and search, archives browsed and extracted,
folder sizes, and SFTP through the existing provider. Not in it: the root-only
device commands, AI, image protocols, syntax highlighting, Git, SQLite.

## Consequences

### Positive

- One set of rules for both front ends. What `C` copies, what a right-click or
  a drag acts on, what Shift-Up does, is decided once, in Rust, under tests.
- The window gains from step 1 alone: behaviour that was only checkable by
  driving a window on the Linux machine becomes unit-testable.
- A single file that can be copied to any Linux server and run.
- The terminal front end is testable in CI without a display, with ratatui's
  test backend.

### Negative

- A second front end to keep in step: every command, every catalogue key,
  every new behaviour has to be reachable from both, or deliberately absent
  from one with the reason recorded.
- Step 1 touches the most-changed crate in the repository; done in one commit
  with nothing else in it, so it can be reverted alone.
- Static musl builds are only as easy as the C and assembly in the
  dependency tree. The SFTP provider's cryptography backend has to be
  confirmed to link statically for musl on both architectures before the
  static build is promised.

### Neutral

- The Qt front end, its packaging and ADR-0001 are unaffected.

## Compliance

- `tests/tests/architecture.rs` gains a rule that `jtf-app` and `jtf-tui` do
  not depend, directly or transitively, on a GUI toolkit, `dbus`, `zbus`,
  `gio`, `glib`, `x11`, `x11rb` or `wayland-client`.
- A CI job installs the `.deb` into a `debian:stable-slim` container with no
  desktop packages, checks with `ldd` that nothing links libX, libQt or
  libdbus, and runs the binary.
- The hostile fixture set (`docs/TESTING.md` §9.2) gains names carrying ESC,
  CSI and OSC sequences, C1 controls and bidirectional overrides; a test
  renders each through the test backend and asserts no control character
  reaches the output.
- Step 1 is accepted only with every existing test passing and the window's
  behaviour unchanged on Linux.
- `AGENTS.md` §21 applies to the terminal front end as to the window: no
  blocking I/O on its loop, i18n keys rather than literals, cancellation.

## Decided

1. **Names.** The command is `jtf-tui`, chosen by the project owner. The
   package - `.deb`, `.rpm`, tarball - is `jt-filework-tui`, so a search for
   `jt-filework` finds both front ends. `AGENTS.md` §10.1 records the
   command as its one exception.
2. **Order.** Step 1 (`jtf-app`) first: it changes nothing visible and puts
   the window's own marking rules under Rust tests. Step 2, the terminal
   screens, after the Windows and Linux platform adapters.
3. **Single-Key only** in the first version.
4. **No root-only device commands** in the first version.

## Revisit Criteria

- A dependency the terminal front end needs turns out to require a desktop
  service: it is replaced or the feature is left out, never the requirement.
- Step 1 shows that the shared layer cannot serve both front ends without
  knowing which one it is serving.
