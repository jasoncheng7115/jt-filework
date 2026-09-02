# Changelog

All notable changes to jt-filework are recorded here.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and
the version numbers follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
While the major version is 0, the interface and the session format may still
change between minor versions; the session file carries its own format version
and is migrated forward (`docs/UPGRADE.md`).

A Traditional Chinese edition of this file is kept alongside it at
[`CHANGELOG_zh-TW.md`](CHANGELOG_zh-TW.md). Both are written by hand and both
must be updated in the same change.

## [0.6.7] - 2026-09-03

### Fixed

- **The cursor outline had no right-hand side.** The sides of the rectangle are
  drawn by the cells at the ends of the row, and the end was taken to be the
  model's last column — but the model carries eleven columns while the list
  shows four, so the right side was asked of a hidden column and never drawn at
  all. Every repaint fix aimed at this missed, because the line was not being
  drawn in the first place. The sides now belong to the first and last *visible*
  columns.
- **The disc usage window re-measured a folder it had already walked.** A
  finished walk is kept now, so stepping back up a level — or returning to any
  folder already measured — is instant instead of costing the whole walk again.
  A cancelled walk is not kept: its numbers are partial, and handing those back
  later as though they were the answer would be worse than walking again.
- **The disc usage window's columns could not be resized.** They were `Fixed`,
  which sets the width and also refuses the drag.
- **Its 「取消操作」 button sat flush against the window frame** and had no icon.
- **The path bar's context menu had no 計算磁碟用量.** It is the menu you get by
  right-clicking a folder, and it offered everything except the one measurement
  that is about a folder.

## [0.6.6] - 2026-09-03

### Fixed

- **The cursor fell to the top of the list after a delete.** It lands on the
  entry that took the deleted one's place now, so deleting a run of files no
  longer means scrolling back down after every one. This is the far side of the
  folder poll added in 0.6.4: the cursor is carried across a re-read by
  remembering *which file* it was on, and a deleted file matches nothing — so
  the row it held is used instead. Positioning also waits for the listing to
  finish, because a row number decided from the first batch of a large folder
  is a guess that never gets reconsidered.
- **Tab in the path bar did nothing.** The key was caught on the text field,
  and the completion list takes the keyboard the moment it appears — so the one
  moment Tab meant something was the one moment the field could not see it. The
  list is watched too now.
- **macOS printed `Ctrl+2` for a key that only answers to Command.** The
  behaviour was always right; Qt maps the portable chord onto the platform
  accelerator. It was the places that print the chord as text that were wrong,
  and there were several — a tooltip, the palette, the hint strip, the shortcut
  sheet, the settings list. One helper renders them all in the platform's own
  notation.

## [0.6.5] - 2026-09-03

### Documentation

- Six more reported items written down with their diagnoses: the cursor falling
  to the top of the list after a delete instead of landing on what took the
  deleted item's place; the disc usage window re-walking a folder it has already
  measured, both on the way down and on the way back up; its columns not being
  resizable; its cancel button sitting flush against the window edge without an
  icon; and the path bar's context menu offering everything except the one
  measurement that is about a folder.

## [0.6.4] - 2026-09-03

### Added

- **The list notices changes made outside the program.** A file added, renamed
  or removed by anything else - a sync client, a script, another file manager -
  used to stay invisible until F5 or until the folder was entered again, so the
  list quietly showed something that was no longer true. Working out of a synced
  folder, that is most of the time.

  Polled rather than watched, deliberately: `QFileSystemWatcher` costs a
  descriptor per directory and on Linux `inotify` has a per-user watch limit
  that a file manager with several panes and a folder tree reaches on its own.
  This is one `stat` per pane per second, needs no descriptors, behaves the same
  on all three platforms, and re-reads only when the folder actually changed.

  It never re-lists under a text field. Re-reading a folder while someone is
  typing into a rename box, a filter or the path bar moves the thing being named
  out from under the words being typed about it. The cursor and the marks are
  stored as locations rather than as row numbers, so a re-read puts the cursor
  back on the same *file* even when something was inserted above it.

  In-place edits to a file already listed - its size or its own timestamp
  changing - do not move the folder's own timestamp and are not caught yet.

## [0.6.3] - 2026-09-02

### Fixed

- **`Q` did nothing at all on Windows and Linux.** The command sat in the File
  menu on both and the key did nothing: `quicklook::available()` was written so
  the UI could hide it and had no callers anywhere. Rather than take the command
  away on two platforms out of three, it falls back to the viewer this program
  already carries — the key means the same thing everywhere, and only whose
  window opens differs. `Q` is in the key hint strip now, which that makes
  honest.

### Documentation

- Four requirements written down before the session holding them could end: what
  "arrow keys while the preview is open" has to mean (Finder's behaviour, and
  the note that half of it already works); the macOS shortcuts that print
  `Ctrl+2` for a key that only answers to Command, with the six call sites; the
  cursor outline that still breaks on arrow-key moves; Tab in the path bar not
  completing, with the lead that the completer's popup is taking the key. Each
  carries its diagnosis rather than a one-line title, and each has its cases in
  `docs/UI_TEST_PLAN.md`.
- A setting for whether a pane's filter bar is always present or only appears
  when asked for, recorded with the one thing that would otherwise be
  rediscovered expensively: the new field needs a serde default.
- Linux quick preview written down as a chain rather than as GNOME: `gnome-sushi`
  and Cinnamon's `nemo-preview` expose a previewer on the bus, KDE and XFCE have
  nothing to call at all, and on those the built-in viewer is the answer rather
  than the consolation prize.

## [0.6.2] - 2026-09-02

### Added

- **Writing a disk image to a removable disk.** Only removable, external disks
  that are not carrying the running system are ever offered; a disk whose
  properties could not be read does not appear at all. Unmount, write, flush,
  read back, compare — and the comparison is on by default, because a failing
  stick and a counterfeit one both accept every write and hand back something
  else. `authopen` on macOS, so nothing here ever runs as root; `pkexec` on
  Linux, skipped when the caller already has access.
- **Folders inside an archive are folders.** Enter or Right descends,
  Backspace or Left goes up, and a row shows its own name instead of its whole
  stored path. Folders are derived from the members, because plenty of archives
  store no directory entries at all.
- **The copy/move target can be moved from the keyboard** (`Ctrl+Alt+T`), which
  matters from three panes up, where "the next one" is a choice rather than a
  fact. It is an offset from the active pane, so it can never name the pane the
  keyboard is in.
- **`Q` opens Quick Look** in single-key mode. Space is the system's key for it
  and is taken here by marking.
- **Tab fills in a path**, the way a shell does: one match completes it and adds
  the separator, several fill in as far as they agree and then show the list.
- **Folders-first is a toolbar toggle**, and off by default.
- A screenshot gallery on the site, and the pages open a screenshot full size.

### Fixed

- **Space in native mode killed the program.** Quick Look's panel forwarded keys
  to the key window, which while the panel is open is the panel: the event came
  straight back into the same handler until the stack ran out.
- **A new field made every existing session unreadable** — every tab, mark and
  open folder gone on upgrade, and the status bar said so at every launch.
- **Rename acted on a stale mark** rather than on the row the cursor was
  visibly sitting on.
- **The list showed UTC**, eight hours out here, and disagreed with the
  inspector beside it by exactly that much.
- **The header lost all its text** once the pane gained a border radius: the
  stylesheet style left the painter clipped to nothing.
- **The cursor outline came apart** when the cursor moved, because only one
  column of each row was repainted.
- **Dragging a file across panes resized them all**, and so did marking one more
  file: a badge and a status line were reaching the splitter with their widths.
  A long path did the same.
- **The preview background ignored the colour chosen** — a stylesheet painted
  over it. It defaults to white now, and the button shows the colour it is set
  to.
- The sidebar listed every mounted snap as a full disk.
- A disabled button in any dialog was drawn as an enabled one.
- Both languages showed at once on the specification page, and two links in the
  site's top bar did nothing.

### Changed

- The light theme stopped boxing every pane in a grey border; only the active
  ring and the target's dashed edge remain.
- The pane status line counts the marked set once instead of reporting it as
  both 「已選取」 and 「已標記」, and no longer carries free space — a property of
  the disk rather than of the folder.
- An unmeasured folder shows a dash rather than an empty cell.
- The Chinese on the site and in the Chinese README was rewritten in the
  author's own register.

## [0.6.1] - 2026-09-02

### Added

- **Writing a disk image to a removable disk.** The most destructive thing this
  program does, so the safety is in *which disks are offered* rather than in the
  confirmation: by the time anyone is reading a dialog they have decided.
  Enumeration is a whitelist — a disk appears only when the program positively
  established that it is removable, external and not carrying the running
  system, and a disk whose properties could not be read does not appear at all.
  There is no path where "I could not tell" produces a row.

  Each platform is asked in its own terms. macOS via `diskutil`; Linux from
  `/sys/block` directly, with the disk holding `/` excluded by name because a
  machine booted from a USB stick has a root filesystem on a genuinely
  removable disk; Windows via `Get-Disk`, whose `IsBoot` and `IsSystem` answer
  the question outright — and a missing safety flag reads as "yes, it is the
  boot disk".

  The privilege is borrowed for the one operation and handed back: `authopen`
  on macOS, so nothing of ours ever runs as root, and `pkexec` on Linux, which
  is skipped entirely when the caller already has access.

  It reads the disk back afterwards and compares byte for byte, on by default.
  A failing stick and a counterfeit one both accept every write and hand back
  something else, and nothing before that step can tell. A mismatch reports the
  byte offset. The CRC-32 shown is the one `gzip` and `cksum -a crc32` print,
  so it can be compared with a published checksum by eye.

- **A screenshot gallery**, fifteen shots taken on the three machines this is
  built on, each labelled with its platform. "It runs on three platforms" is a
  claim a reader is entitled to see rather than take.

### Fixed

- **The sidebar listed every mounted snap as a full disk.** An Ubuntu machine
  with the ordinary set of snaps mounts a dozen read-only squashfs images, and
  every one of them appeared as a disk that was one hundred per cent full —
  fourteen red bars burying the two disks the person actually had. The filter
  had only ever seen macOS: three path prefixes, all of them Apple's. It asks
  the filesystem type now, because the paths differ per distribution and the
  answer does not.

- **A disabled button in any dialog was drawn as an enabled one**, and a
  disabled *default* button as the filled, highlighted one the eye goes to.
  There was no `:disabled` rule for dialog buttons in the stylesheet at all.
  Found because the image writer's Write button, which is disabled until a disk
  has been chosen, was the most inviting control on the screen while it did
  nothing.

- **The specification page showed both languages at once.** All fourteen index
  entries rendered twice, English beside Chinese. The language switch hid a
  language with a single attribute selector, which any class rule outranks, so
  a component that set its own `display` appeared in both. The rule is inverted
  now: it hides what is not the current language and never sets a display on a
  visible element, so no new component can revive it.

- **Two links in the site's top bar did nothing.** Both language versions of a
  heading carried the same section id, so the browser took the first — the
  hidden English one — and had no box to scroll to.

- **The shortcut window's title disagreed with the button that opens it.** One
  said "keyboard shortcuts" and the other "shortcut reference".

### Changed

- The Chinese on both pages and in the Chinese README was rewritten. It read
  like it had been translated from the English, because it had been.

## [0.6.0] - 2026-09-01

### Added

- **The disc usage window can act on what it finds.** `C` copies, `M` moves and
  `D` moves to the trash, from the row the cursor is on, and the same three are
  on its context menu. Finding the folder that is eating the disc and then
  having to go somewhere else to do anything about it was most of the walk
  wasted.

  When the operation finishes, that level is measured again — the report is
  about a folder that has just changed, and going on showing the old numbers
  would be presenting them as if they were current. The panes are refreshed
  too, because a file trashed from the report is gone from the folder they are
  showing as well.

  Operations here name their own source paths rather than reading a pane's
  selection (`jtf_op_prepare_paths`): this window is a report about a folder,
  not a pane showing one, and there is no selection behind the boundary to
  read.

- **A key hint strip in the disc usage window**, the same keycaps the file list
  and the viewer use: `→ 進入 · ← 上層 · C 複製 · M 移動 · D 回收 · Tab 換一邊 ·
  Esc 關閉`. It stopped being a report you can only read the moment it grew
  those three commands, and a window whose keys are not on screen is a window
  nobody knows has any.

- **Arrow keys in the disc usage window.** Right and Enter go in, Left and
  Backspace come back, Tab moves between the two lists — the same walk the file
  list does, done the same way. Arriving in a folder puts the cursor on its
  first row, so there is somewhere to press Right from.

- **Both walks say where they have got to.** A running search names the folder
  it is in, in the status bar; the disc usage window names it at the end of its
  own status line. A count says a walk is alive but not how far in it is, and
  「32,894 個檔案」reads the same whether it is in `Downloads` or four levels
  into a cache. The path is what tells someone whether to wait or to narrow the
  question.

- **A usage bar on every disk in the sidebar**, coloured by how full it is:
  the accent below 75%, amber to 90%, red past it. The exact numbers are on
  hover. The volume rows said which disks exist and nothing about them, and
  whether a disk is nearly full is the one thing about a disk that changes.
  It is re-read on the same timer that watches for a disk arriving, so it is a
  number from now rather than from whenever you last navigated.

- **Every removal is confirmed**, whichever route built it — a menu, a key, the
  disc usage window. Permanent deletion keeps its stronger warning; the trash
  gets a plainer question. The trash is recoverable and still worth asking
  about: the question is not only whether the data survives, it is whether the
  person meant to press the key. `D` is one key away from `S` and `F` on this
  keyboard.

- **The application has a window icon** on all three platforms, and the About
  box shows it. macOS reads the `.icns` for the Dock and Finder; Qt reads
  neither, so `QApplication::windowIcon()` was empty and the About box opened
  with a blank square where its icon goes.

- **A recent place can be bookmarked from its own menu**, offered only when it
  is not one already.

### Changed

- **The sidebar is always shown; only the folder tree folds.** The command that
  used to hide it hid the special places with it, and those are the sidebar's
  fixed part: bookmarks, servers, disks and where you have just been do not
  belong to whatever folder is open.

- **The hint strip uses short names** — `複製`, `移動`, `回收`, `標記` — while
  the menus keep the full ones. The menu says「移到資源回收筒」because that is
  what the command does and there is a「永久刪除」beside it; a strip fitting a
  dozen of these wants「回收」. `F5 重新整理` and `A 屬性` have left the strip:
  refreshing is what you reach for when something already looks wrong, and it
  was spending width on every screen for that. The density setting is now
  「顯示按鍵與名稱」and「只顯示按鍵」, the second of which is genuinely more
  compact than it used to be.

- **The status bar gives its width to the left.** The counters on the right lost
  half their padding, an empty counter is hidden rather than left blank with its
  divider, and the message on the left now shrinks and elides in the middle
  rather than demanding the width of its whole text — so a long path there
  cannot shove the counters off the end.

- **The status chip says「快速鍵參考表」** and no longer carries the current
  mode. The toolbar's mode switch says that a few centimetres away, and a
  control whose label is half status readout reads as a status readout and does
  not get clicked. It is also set in the interface font: a tool button takes the
  style's own smaller one, which left the single clickable thing in that row
  reading a size below the numbers beside it.

- **Single-Key mode is「全鍵模式」** in Traditional Chinese.

- **The About box** names the author, links the right repository, and no longer
  describes the program in terms of another one.

- **The viewer** has the same keycaps as the rest of the program in place of a
  line of bold letters, a toolbar set apart from the text with a magnifier in
  its find box, and a margin around what is being read.

- **A selected row in the sidebar is one pill**, drawn across the whole row.

### Fixed

- **Search matches in the viewer were drawn as black blocks.** `setHighlight`
  was called by the file list and never by the viewer, so the delegate held two
  default-constructed — that is, invalid — colours, and an invalid colour fills
  black: black text on a black box, over the one word you were looking for.

- **Escape, Enter and Tab were being taken from text fields.** On macOS the menu
  bar is application-wide, and the keymap binds all three (`escape` is
  `search.clear`, `enter` is `file.open`, `tab` is the next pane) - so they
  fired before the field with the keyboard ever saw them. Escape in the filter
  box cleared nothing, Tab jumped panes instead of handing the keyboard to the
  narrowed list, and Enter opened a file. The rule about typing now claims them
  for whichever field has the focus, as it already claimed the letters.

- **Escape over a filtered list did nothing.** It is bound to `search.clear`,
  which cleared a search and not a filter. Both narrow the list and Escape means
  「stop narrowing it」whichever is in force.

- **The same menu-bar collision took Left and Right from the disc usage window**,
  which is why Backspace went up a level and Left did not.

- **`重新連線` on a server never asked for a password.** The prompt was raised
  only from a full refresh, and a sign-in fails on a worker thread: at the
  moment the button was pressed nothing had failed yet, and when it did fail
  the tick that noticed only refreshed rows and the status line. The check runs
  on every tick now.

- **The sidebar took half the window at startup.** Restoring its width lived
  inside "the folder tree became visible", which stopped being the same thing as
  "the sidebar appeared"; with the tree folded away nothing applied a width at
  all, and dragging any divider then saved the layout's guess as a preference.
  Only a width the divider could actually have been dragged to is remembered.

- **The eject button cost every row in the sidebar the end of its name.** A
  tree's column width is shared by every row, so one button on one removable
  disk was charged to the bookmarks, the servers and the recent places. Both of
  the controls a volume row carries are painted inside the name column now.

- **The selection in the sidebar had dark notches at its corners.** A tree
  paints the indentation beside a selected child itself, square-cornered, out of
  the palette's highlight — and a rounded pill starting where the item starts
  left the two shapes meeting, with the pill's corners cutting notches out of
  the block behind them.

- **The share bar vanished on the selected row** of the disc usage window,
  because the accent it is drawn in *is* the selection background there.

- **Message box icons were unreadable on a dark theme.** The style draws
  `QMessageBox::Question` in its own colours, which on a dark background is a
  black question mark on near-black. These now draw our own icon in the
  palette's colour — and a picture of the command about to run, which says more
  than a punctuation mark.

- **Type icons in the disc usage window were all the same generic document.**
  `QFileIconProvider` answers about a file on disk, and a row about a *kind*
  has no file to point at. The platform is asked about the type instead.

## [0.5.0] - 2026-08-31

### Added

- **Disc usage analysis.**「分析磁碟用量…」in the File menu and on a folder's
  context menu measures a folder and answers two questions from one walk:
  which child branch holds the most, and **which kind of file adds up to the
  most**. The second is the one the tools people use for this mostly cannot
  answer, and it is the more useful half as often as not — 「照片佔了 40 GB」
  is now a question with an answer.

  Each row carries a bar of its share, because a column of byte counts is a
  comparison the reader still has to do. Double-clicking a folder row goes
  there, which is the point of having found it. Files sitting in the folder
  itself get a row of their own, so the branches add up to the total instead
  of leaving a difference that looks like a fault.

  It runs on a worker thread with a running count and a working Cancel, and
  says so when it is incomplete — a breakdown that quietly omits a folder it
  could not read is worse than one labelled partial. Symlinks are neither
  followed nor counted: a link into a parent would loop, and a link to
  something huge elsewhere is not space used here.

  Measured against a real 23.8 GB folder: 80,460 files and 9,670 folders in
  2.8 seconds, with both breakdowns reconciling exactly to the total.

- **The path field completes as you type** (`P`, or `\` in CView's keyboard).
  Completions come from the same call the folder tree lists with, rather than
  from Qt's own file-system model — one source of truth about what a folder
  contains, and it works for a path on a server, which Qt's could not.

- **Switching panes is in the hint strip.** With two panes open it is the key
  reached for most often after the arrows, and it was the one common key the
  strip never mentioned. It is left out when there is only one pane, because a
  hint for a key that does nothing teaches people to distrust the strip.

## [0.4.0] - 2026-08-31

### Added

- **ISO images open like archives** (ADR-0005). Enter on a `.iso` lists what is
  inside it, and `C`/`X` copy members out, with the same keys and the same
  window the ZIP listing uses. ISO 9660 with Joliet (preferred when the image
  has it, because those are the names whoever built it meant people to see)
  and Rock Ridge `NM` for the POSIX names Linux images carry.

  The reader is ours rather than a crate: the candidates are thinly maintained
  parsers of untrusted binary input, and the property we need is not "does it
  parse valid images" but "what does it do with an invalid one". Every extent
  is checked against the file's real length before a read, the walk is a queue
  with a depth cap and a visited set so a directory cycle ends, nothing is
  allocated to a size the file chose, and a name that climbs out of the
  destination is refused by the same function the ZIP path uses.

  UDF is not read. A UDF-only image says it cannot be read rather than listing
  as empty — those are different answers and only one of them is true.

  Nothing writes into an image.

- **A disc image is its own kind in the type column**, not「壓縮檔」. Detected
  by the `CD001` signature at byte 32769 rather than by extension, so a `.img`
  that is one is one.

### Changed

- **The archive, ISO and comparison windows look like the pane's file list**:
  the same row height, font, header, icons and drawn tick. They were three
  differently-styled tables, which read as someone else's dialogs rather than
  as part of this program.

- **The Qt build directory moved out of the source tree** to
  `~/.cache/jt-filework-qt`, where the Rust target directory already lives.
  This checkout sits in a synced folder and the build output was 4.2 GB of
  object files re-uploading after every build. Override with
  `JTF_BUILD_ROOT`. A release build also installs to
  `/Applications/jt-filework.app`, which is the path the Dock pins.

### Fixed

- The i18n test now walks `ContentKind::ALL`. `label_key` returns a literal
  rather than calling `tr_`, so the source scan could not see those keys, and
  a kind added without its string would have shown the user a raw key. Checked
  by removing a key and watching the test fail.

## [0.3.0] - 2026-08-31

### Added

- **A comparison now says what it is doing while it does it**, and can be
  stopped. It always ran on a worker thread, but it said nothing until it
  finished, which from outside is indistinguishable from a window that has
  hung. It now reports folders read, items seen and differences found as it
  goes, and Cancel keeps whatever it has already found.

- **Bookmark this folder** and **Open in New Window** on the folders you can
  point at: the path bar, a tab, a row in the places list, the folder tree,
  and a folder in the file list. The folder tree had no menu at all before.

- **A drop now asks whether to move or copy**, whether it came from another
  pane or from another application. Qt resolves a drop action out of the
  platform's modifier conventions, so the same gesture moved within one disk
  and copied across two, with nothing on screen saying which had happened.
  Copy is the default from another application, move from one of our own.

### Changed

- **The two sides of a comparison are named after their folders**, not "left"
  and "right". Panes can be split top and bottom, and then left and right name
  nothing.

- **The text preview indexes the first 4 MiB of a file, and the viewer 64 MiB.**
  Neither had a bound: the index is eight bytes per line and building it reads
  every byte, so previewing a multi-gigabyte log read the whole thing and held
  hundreds of megabytes of offsets before one line appeared.

- **Volumes and devices are one section** in the places list. The split cost
  two headings to separate two rows, and the removable ones are already the
  only rows carrying an eject button.

### Fixed

- **Marks in other folders were counted as selected here.** Marks survive
  navigating away and back, so the stored set holds entries from folders the
  tab has left - and the status line counted them, the header checkbox showed
  everything ticked over a folder with nothing ticked in it, and `複製路徑`
  pasted three paths when one row was selected. Worse, a copy or a move would
  have acted on files that were not on screen. Counts and operations now use
  the rows the pane is showing.

- **Dragging a row that was not selected dragged the selection instead.** Qt
  builds a drag's payload from the selected rows; the pressed row is now
  selected first, as it is in every file manager.

- **The share sheet opened a pane and a half away from the row it was about.**
  It anchored with the view's `frame` - where the view sits in its superview -
  instead of its `bounds`.

- **Right-clicking the empty part of the path bar** opened Qt's own
  Undo/Cut/Paste menu, in English, about a text field the user cannot see. It
  now opens the same menu a path segment does, for the folder the pane is on.

- The Theme, Font, Keyboard and Language submenus had no icons.

## [0.2.0] - 2026-08-31

### Added

- **Folder comparison.** `檔案 ▸ Compare Folders…` puts the focused pane's
  folder beside the one a copy would land in and lists what differs: only on
  the left, only on the right, different, or identical. Subfolders are off by
  default and are one checkbox away; identical rows are hidden by default,
  because a list where most rows say "identical" buries the handful that do
  not.

  Two files match when their sizes match and their times are within two
  seconds — the FAT granularity a copy between filesystems lands inside. That
  rule is printed under the table rather than assumed: matching size and time
  is not proof of identical content, and the window must not imply it is.

  The walk runs on a worker thread, so two large trees — or two folders on a
  server — do not stop the window painting. A folder present on one side only
  is one row, not one row per file underneath it; a symlink is a name, not a
  door; and an unreadable subfolder stops that subtree alone, while an
  unreadable root is an error rather than a confident "everything is only on
  the other side".

- **A close button on every pane**, beside its tabs, and **a Try Again button**
  on a pane that could not open its folder. Retrying drops the connection the
  provider is holding first, so after a failure the retry is a real one rather
  than a second look at the same broken session.

- **Headings over the two halves of the sidebar** (Places, Folders), which
  previously ran into each other as one long list of folder rows, and the same
  background under both.

### Changed

- **The folder tree follows the focused pane onto a server.** A server gets a
  root of its own beside the filesystem, so a pane on `sftp://…` is somewhere
  the tree can show instead of clearing its selection. It expands only servers
  already signed in to — listing one from the interface thread would sign in
  from there, and a sign-in takes as long as the network says it does.

- **The window title and the folder tree now follow the focus**, not only a
  refresh. Moving to the other pane left both naming the pane you had left.

- **Clearing the header checkbox clears the highlight too.** Selection is the
  mark; unticking every box while every row stayed lit was the two halves of
  one thing disagreeing.

### Fixed

- **The close mark on a tab was invisible.** Three things had to be wrong at
  once: `xmark.svg` was a copy of `xmark-circle.svg`, whose cross fills a
  quarter of its box; the size asked for was not one the icon is drawn at, so
  Qt scaled a pixmap down and thinned the stroke with it; and the stylesheet's
  margins and padding came out of the button's own box, leaving a 24px button
  7px to draw in.

- **The header's mark-all box sat three pixels left of the rows' checkboxes**
  and a size smaller. It now asks the style where a row draws its checkbox
  rather than guessing at 8 and 14.

- **The Dock icon launched a stale debug build** while the release build ran
  beside it — two icons for one application, because macOS groups by bundle
  path. A release build now installs to `/Applications/jt-filework.app`, which
  is a path worth pinning.

- The version is read from `Cargo.toml` by CMake instead of being written out
  again in `CMakeLists.txt`, where a second copy could only drift.

## [Unreleased]

### Added

- **Archive extraction and creation** (ADR-0003, accepted and built). `Z` on a
  ZIP asks where to put it and unpacks there; `Alt-Z` compresses what is
  marked, or the entry under the cursor, into a ZIP you name. Both run on a
  worker thread with a live count and a working Cancel, because unpacking a
  large archive on the UI thread would freeze the window. There is no
  percentage bar: the member sizes are not known until they arrive, and a bar
  claiming a fraction would be inventing it.

  Every member is resolved against the destination and refused if it lands
  outside, however it is spelled — `../`, an absolute path, a Windows drive
  letter or UNC prefix — and backslashes count as separators on every
  platform, so an archive built on Windows cannot be a traversal there and a
  harmless odd filename here. Symlink members are refused rather than created.
  Refusals are counted and said out loud. Extraction stops at 8 GiB per member
  and 32 GiB in total, measured against what actually arrives rather than what
  the header claimed, which is the whole trick of a zip bomb. A cancelled
  extraction removes the partial file.

  `zip` is pinned to `deflate`, the feature that selects `flate2`'s Rust
  backend; the crate's default set would have pulled in bzip2, lzma, xz and
  zstd, all of which are C.

  `Enter` on a ZIP opens its contents in a window of its own, which is what
  `CV.HLP` §四 describes. A separate window rather than a pane, because an
  archive is not a folder — you cannot navigate into it, create in it or drop
  onto it, and a pane that looked like one would promise all three. Inside it
  `C` extracts what is selected and `X` extracts everything, the same
  distinction CView makes. A member whose path leads outside the destination
  is listed, marked, and says so on hover; it is refused at extraction either
  way, and seeing it is the point of a listing. A file named `.zip` that turns
  out not to be a readable one falls back to opening the ordinary way rather
  than showing an empty window.

  Deleting a member from an existing archive is deliberately not built: it
  rewrites the file in place and can destroy data if interrupted. Viewing or
  running a member from inside the window needs it extracted to a temporary
  file whose lifetime somebody owns, so those two of §四's keys are absent
  rather than inert.
- **Selecting is marking.** What is highlighted is what is ticked, whether the
  rows were picked with the mouse, with Shift and the arrow keys, or with
  Space. Until now the two were separate states, so a list could show five
  rows highlighted and one ticked, with nothing to say which the next command
  would act on. `AGENTS.md` §10 said to keep them apart and has been changed
  to say the opposite, with the reason recorded.

  The marks remain the *stored* state — the session keeps them, an operation
  reads them — and the selection is restored from them on arriving in a
  folder, so marks still survive navigating away and back. `Space` now adds
  the row to the selection and steps down, which is what it always meant.
- **The name column's header has a mark-all box.** The rows carry checkboxes,
  so the column of boxes wanted one at its head; it shows all, none or some,
  and follows a mark made anywhere else. `markChanged` had been emitted by the
  model since the beginning with nothing listening to it.
- **The panes could not be resized.** The splitter handles were styled one
  pixel wide, and Qt makes a handle's drag area exactly its width — so the
  divider had to be hit within a single pixel, which in practice meant never.
  They are seven pixels wide now, with the padding painted in the pane colour
  so what shows is still a single line.
- **The active pane's border had never been drawn at all.** It was in the
  stylesheet from the beginning, but a plain `QWidget` subclass ignores a
  stylesheet background and border unless it asks for them — so the mark that
  was supposed to say which pane has the keyboard was painting nothing. The
  pane now asks, keeps room for the border, and is ringed all the way round
  rather than underlined along its top edge.

  The ring uses the palette's pane-indicator colour rather than the selection
  colour, and that is the part that has to work in both themes: a ring is read
  against the pane behind it, and those surfaces are opposites. The palette
  answers with a deeper blue for white and a lighter one for near-black —
  measured at 5.0 and 7.3 against the pane, and 3.8 and 5.7 against an
  inactive pane's border, all clear of the 3:1 a non-text indicator needs. A
  test now fails if a palette change drops below that.
- **The vertical-split button showed three columns.** `Split Vertically`
  stacks two panes one above the other; its icon was `view-columns-3`, which
  is neither the right count nor the right direction. It is now a box divided
  by a horizontal line, against the horizontal split's upright one.
- **Pinning a tab was modelled and unreachable.** A pinned tab keeps a leading
  place in the strip, refuses to reorder out of it and refuses to close
  without force — all built, with no way to pin anything. It is now on the tab
  context menu and the File menu, and a pinned tab is marked in the strip.
- **Two chords ran nothing at all.** `Cmd-Shift-D` was bound to
  `tab.duplicate` and `Cmd-Shift-F` to `search.ai`, both listed in the
  shortcuts window and the palette, and neither had a handler behind it —
  pressing them did nothing, which teaches people that some keys just do not
  work. Duplicating a tab was built and reachable only from the tab strip's
  context menu, so that one is now wired up; AI search is not built, so its
  chord is gone until it is. A test now fails if any bound chord names a
  command the interface does not run — the same fault `file.edit` shipped
  with.
- **A server that refused the sign-in left you with no way to give it a
  password.** The connect dialog asks once, and after that a restored session,
  a reconnect or a dropped connection all reached the failure with nothing to
  offer — the pane said "you do not have permission" and that was the end of
  it. It now asks for the password and tries again, once per failure rather
  than once per refresh, and only when the server refused the *sign-in*: a
  folder the account genuinely cannot read is not something a password fixes.
  The prompt hides what is typed, which is its own call rather than a flag on
  the ordinary text prompt.
- **Clicking a saved server froze the window until it gave up.** The
  connection was opened on the calling thread — the UI thread — so that a bad
  address came back as an error immediately rather than through the channel
  that already carries failures. When the machine is simply off, "immediately"
  is the twenty-second connect timeout, and the window stopped repainting for
  all of it. Connecting now happens on the worker; the error arrives a moment
  later, which is a much smaller price than a frozen window
  (`AGENTS.md` §3).
- **Nothing ever told the core which row the cursor was on.**
  `Tab::active_entry` is a real part of the model — the session stores it, and
  an operation falls back to it when nothing is marked or selected — but no
  code path in the interface set it, so it was always empty and every question
  of the form "what is the cursor pointing at" got no answer. `Enter` on an
  archive did nothing, and `Z` could not find the folder under the cursor when
  the marks named no folder. The pane now reports the cursor row as it moves.
- **Every bare-letter command was also firing on `Shift` + that letter.** Qt
  matches a one-letter shortcut against the shifted form too — the text both
  produce is the same capital — and shortcuts are delivered before the focus
  widget sees the key at all. So `Shift-H` opened the hex viewer, `Shift-C`
  would have copied and `Shift-M` moved, and the Shift+letter jump below never
  reached the code that implements it. The file list now claims those chords
  through `ShortcutOverride`, which is the mechanism for saying "this key is
  mine" before the shortcut system takes it.
- **The font list offered proportional families while "fixed-width" was
  ticked**, and gave no way to tell which monospace face is narrower than
  another — which is the whole question when you are trying to fit more
  columns. With fixed-width on it now lists only fixed-width families, each
  with the width of a digit beside it.
- **`Shift` + a letter or digit jumps to the first entry starting with it**,
  and again to the next — `CV.HLP` §二's `Shift-A`–`Z` and `0`–`9`. This is
  CView's own answer to having spent the bare letters on commands, and it is
  what makes Single-Key mode navigable without type-ahead. `Shift-C` and
  `Shift-M` used to copy and move to the other pane; those were ours rather
  than CView's, so they gave way. Copy keeps `C` and `Ins`; move keeps `M` and
  now `Shift-Ins`.
- **`H` views a file as hex** — `CV.HLP` §二's 以 HEX 16 進制方式觀看檔案. The
  viewer has had a hex mode all along; nothing opened it.
- **`E` edits.** It was bound in the keymap and listed on the hint strip from
  the beginning with nothing behind it, so pressing it did nothing at all. It
  now hands the file to whatever the platform opens plain text with, and a
  file created with `O` opens for editing straight away — which is what
  CView's `Alt-O`, 呼叫 ce.exe 建立或編輯檔案, always did.
- **SFTP, stage one** (ADR-0004, accepted): a `Location::Remote` variant, host
  key verification against `~/.ssh/known_hosts`, an SSH connection that
  authenticates by agent or by `~/.ssh` key file, and an `SftpProvider` that
  lists a remote directory through the same `Provider` contract the local
  filesystem uses — so sorting, filtering, search and the incremental row
  delivery all work against it unchanged. A changed host key is refused with
  both fingerprints and cannot be waved through; an unknown one is refused
  until the user accepts it, and accepting writes to `known_hosts` rather than
  to a store of our own. No password is asked for, held or written. Reachable
  Remote write operations exist at the library level and are tested against a
  real server: create and remove a directory, remove a file, rename, upload
  and download with progress and cancellation. Removing a **non-empty**
  directory is refused — SFTP has no recursive remove and this layer does not
  invent one. A 30-second request timeout means a server that stops
  answering is reported rather than hung on; that was written while a faulty
  network cable was stalling every transfer past 64 KB, and it kept the
  program usable throughout (`docs/TESTING.md` §5.3.1). Nothing in the
  interface calls the write path yet.
  Reachable from the UI: **前往 → 連線到伺服器…** asks for host, port, user,
  folder and — only if the server has no key for you — a password, which is
  used for that one connection and never written anywhere. Verified against a
  real server: key exchange, authentication, subsystem request, listing,
  `canonicalize` and connection reuse (`src/fs/tests/sftp_live.rs`, run with
  `JTF_SFTP_HOST` set). Saved connections in the sidebar and the write path
  are stage two.
- **Share** in the file context menu on macOS: the system's own share sheet
  for the selected files, from `NSSharingServicePicker`. The Services menu
  itself is not reachable from a Qt view and is recorded as such rather than
  faked (`src/ui/qt6/cpp/platform/share.h`).
- ADR-0004 proposing SFTP support, in two stages, with keys and agent
  authentication, host keys verified against `~/.ssh/known_hosts`, and an
  explicit table of which operations can and cannot work against a remote
  host. Awaiting a decision.

### Fixed

- **The thumbnail cache was bounded by count, not by size.** `QCache`'s
  default cost is one per entry, so "4096 thumbnails" meant 4096 pictures of
  whatever size: at the grid's 72-pixel edge, about 84 MB of pixmaps, and more
  at a larger edge. It now has a budget in bytes and holds whatever number
  fits.
- **The folder tree ate letter keys.** `QTreeView` answers a letter with its
  own type-to-find, and revealing a match expands the branch it is in — so
  with focus in the tree, `V` folded and unfolded it instead of viewing a
  file, and `T` did something other than toggling that very tree. In
  Single-Key mode a bare letter is a command wherever the focus is; the file
  list already refused type-ahead for this reason and the tree now agrees.
- **On Windows the session was written into the temporary directory.** The
  path was built from `HOME` in the macOS shape, and `HOME` is usually unset
  there, so everything landed in `%TEMP%\Library\Application Support` — a
  macOS-shaped path inside a directory the system may empty at any time. Tabs,
  marks and window positions were one cleanup away from being forgotten. The
  platform's own location is used now: `%APPDATA%` on Windows,
  `$XDG_CONFIG_HOME` on Linux, and the same path as before on macOS.
- **The Windows build had no application icon**, and the root of the folder
  tree was labelled `\` — a path Windows has no such thing as. The icon and
  a version stamp now travel in a resource script, which is what Explorer and
  the taskbar actually read, and the root is named by asking the shell, so it
  reads 「本機」 on a Chinese install and "This PC" on an English one rather
  than a string this program guessed. Volumes show their drive letter beside
  their name, as Explorer does.
- The menu bar has mnemonics: `Alt-F`, `Alt-E`, `Alt-V`, `Alt-G`.
- **Left and Right did nothing in the file list, on macOS only.** They were
  guarded by `modifiers() != NoModifier`, and macOS reports the arrow keys
  with `KeypadModifier` set — the comparison was never true, so both keys fell
  through to Qt and walked the cursor across the columns instead of the folder
  tree. Up, Down, Home, End and Backspace were unaffected because none of them
  compares the modifier set as a whole, which is why only these two looked
  broken.
- **A folder filtered on one launch stayed filtered on the next, invisibly.**
  The filter is saved with the tab and restored with it, but nothing put it
  back on screen: `~/Downloads` came up showing 163 zip files and none of its
  92 folders, with no filter box, no text and no way to find out why. The box
  now appears whenever a filter is actually in force.
- **Closing a torn-off window did not close it.** The widget went; the
  workspace kept the window, the session recorded it, and the next launch
  opened it again — so the program came back with two windows however many
  times one was closed. The model had no way to close a window at all;
  dismissing one now closes its panes, which is what removes it.
- **`Z` refused to measure the folder under the cursor** when anything else
  was marked, even marked files it could not measure. Marks are how you ask
  for several folders at once; when they name no folder, the one under the
  cursor is what was meant.
- **The key hint strip ignored the cursor** whenever more than one entry was
  marked, so two marks left an hour ago froze it on the several-items list and
  moving between a file and a folder changed nothing. The row under the cursor
  decides; marks speak only when there is no current row.
- **The sidebar did not notice a disk being plugged in.** The mounted-volume
  list is a snapshot taken when the sidebar is built, so a USB stick appeared
  only when something else happened to cause a rebuild — collapsing a section
  made it show up, which is a confusing way to learn that.
- **Hidden entries were drawn like any other**, where CView and WinCV both
  fade them. They are now drawn as a faded version of whatever colour they
  would otherwise have, so a hidden folder is still recognisably a folder.
- A removable volume has an eject control beside it, using the platform's own
  eject rather than an unmount behind the desktop's back.
- The tab strip is hidden when a pane has one tab, its close mark is legible
  rather than nearly invisible, and its context menu gained **Close Tabs to
  the Left** and icons on the three close entries.
- Sidebar sections stay collapsed across launches, the divider between the
  places list and the folder tree is visible, and the number of recent folders
  shown is a setting.
- The keyboard-shortcuts window shows each command's icon.
- `Cmd-R` refreshes in Single-Key mode, as it already did in Native.
- `T` toggles the folder tree. Mark-all keeps `Alt-T`, which is where
  `CV.HLP` puts it — it was WinCV that moved it to a bare `T`.
- **Left stopped going up a level whenever a second window sat at the root.**
  Each window enabled its own Back, Forward and Up from *the* active pane
  rather than from one of its own, so with two windows open and the other at
  `/`, "can this go up?" was answered about that pane. Up went grey in a window
  nowhere near the root — and because a key triggers the very same action the
  menu and toolbar do, the Left key died with it. Each window now asks about a
  pane it actually owns.
- **The keyboard-shortcuts window listed `command.file.attributes` instead of
  屬性.** The bridge hands back catalogue *keys*, which the command palette
  translates and this window did not — so every row, group heading and the
  keyboard-mode line showed raw identifiers, in every language including
  English.
- **Rename and the other one-line prompts still had "Cancel" and "OK".** They
  reworded Qt's own dialog by searching it for its button box, and when the
  search came back empty they fell back to Qt's English without saying so.
  The prompt is now built here, so the buttons cannot be anything else. The
  field is wide enough to read a filename, and renaming selects the name
  without the extension, as the platform's own rename does.
- **The list said 種類 and the panel said 類型, and they disagreed on the
  answer as well as the word.** The column asks the platform, which is
  localized; the panel asked Qt's own MIME database, which is English-only —
  so one row read 「Zip封存檔」 and the panel beside it read "Compressed
  Archive File". Both now go through the same lookup, and both are called
  種類.
- **Columns never grew to fit their contents.** The fitting pass could only
  shrink them, so a column that started narrow stayed narrow however wide the
  window became: 修改日期 sat truncated at "2023-12-11 …" with empty space to
  its right and the name column swallowing all of it.
- **Sixteen commands had no icon**, among them 複製到, 移動到, 以終端機開啟,
  熱鍵提示列 and both pane-switching entries, leaving ragged gaps down the
  menus. A test now fails if any registered command lacks one.
- **Duplicating a tab copied the wrong tab, and copied nothing at all on a
  server.** It read a path out of whichever tab was active and navigated a new
  tab to it, so right-clicking a different tab duplicated the active one — and
  a remote location has no local path to read. It now duplicates the tab that
  was clicked, by copying where it points.
- **A tab strip's context menu acted on the active pane, not its own.** Every
  other way into a pane makes it active first; this one did not, so 新增分頁
  opened the tab somewhere else.
- **Local-only commands were offered on a remote pane** from the menu bar and
  the keyboard. The context menu had always left out Quick Look, Reveal, Open
  in Terminal, Share, the clipboard and Move to Trash on a remote row, but
  both other routes reach the same handlers and neither checked. They are now
  disabled in one place that no route can go around. The preview panel was
  the worst of it: it read a *server* path off the local disk, so a remote
  `/etc/hosts` previewed this machine's.
- The language is called 繁體中文 rather than 台灣繁體中文.
- **The path bar was blank on a server, and the folder tree did not follow.**
  Both asked the core for the pane's folder, and the core answered with the
  *local* path — which a remote location does not have, so both got an empty
  string. The path bar drew nothing and the tree, given nothing, silently kept
  the last local folder highlighted, which is what "the tree does not follow"
  looked like. There are now two questions instead of one: the local path,
  still empty for a server and still what bookmarks and typed relative paths
  ask, and the shown path, which for a server is `sftp://user@host/path`. The
  tree clears its selection when the pane is somewhere the tree cannot
  represent, rather than pointing at a folder the pane has left.
- **The key hint strip named two commands that do not exist** — `file.copy`
  and `file.execute` — and because a command with no shortcut is skipped
  rather than drawn blank, they vanished without a trace instead of showing as
  a gap. A test now checks every id the strip names against the registry.
- **The hint strip offered `Ins` and `Shift-C` for copy and move** in
  Single-Key mode, where the keys are plain `C` and `M`. It listed the
  two-pane commands; it now lists the chooser ones, which are the single keys
  — the strip leads with single keys by design.
- A symlink test could fail when the suite ran in parallel. Both tests in the
  file built their fixtures from the process id and the clock, and two calls a
  few instructions apart can read the same nanosecond — so both wrote into one
  directory and each counted the other's files.
- Every window refreshes its rows when a directory batch arrives. Each window
  ran its own pump timer against one shared application state, so a batch was
  drained by whichever timer fired first and the others were told nothing had
  happened. A window that kept losing that race sat showing only `..` while
  its status line, read straight from the core, counted the thousands of
  entries it was not displaying.
- Text prompts (new folder, new file, rename, mark by pattern, font family)
  show their buttons in the user's language, with icons. They were built by
  `QInputDialog::getText`, whose OK and Cancel come from Qt's own translation
  catalogue, which this program does not ship.

## [0.1.0] — 2026-08-30

The first numbered version. Everything below has been built and is working;
it is recorded here as the starting point rather than as a list of changes
against an earlier release, because there was not one.

### Browsing

- Recursive split panes with independent tabs, restored between sessions,
  including windows torn off from a tab.
- Folder tree, favourites, volumes and recent locations in a sidebar.
- Detail list and icon grid over one model and one selection.
- Breadcrumb path bar that becomes an editable full path when its empty space
  is clicked, the way Explorer's address bar does.
- Columns adapt to the width available: the name column keeps a readable
  floor, and columns that could no longer be read usefully are dropped rather
  than squeezed into ellipses.
- Filter (the current folder) and search (the folder and everything under it)
  as two separate things, with a floating card reporting a running search and
  offering to stop it.

### Keyboard

- Two keyboard profiles as data, not code: **Single-Key**, following CView's
  key table, and **Native**, following the platform's conventions. Switched
  from the toolbar or by shortcut, with a sliding indicator.
- A key hint strip along the foot, built from the live keymap and the
  catalogue, which changes with what the cursor is on. Full, compact and
  fade-while-working modes.
- Every command carries an id, and menus, the toolbar, the palette and the
  keymap all reach the same one.

### Files

- Copy, move, duplicate, rename, batch rename, new file and folder, trash
  through the platform's own call so Put Back works, and permanent delete
  behind a confirmation.
- A destination chooser for copy and move, listing every open tab as well as
  a path to type or browse for.
- Operations queue and run in turn, with a window that lists them, shows the
  running one's progress and lets a waiting one be dropped.
- Folder sizes on demand, cached against the folder's own modification time.
- Archive contents browsed like a folder — a listing only; extraction is not
  built (see `docs/adr/0003-archive-extraction.md`).

### Viewing

- Text and hex viewer that streams the file and indexes line offsets, so a
  multi-gigabyte log opens without being read into memory.
- Preview panel that follows the cursor, waiting for it to settle so a held
  arrow key does not read every file it passes.
- Quick Look on macOS.

### Interface

- Light and dark themes from one set of semantic tokens; no literal colour
  outside the palette.
- English and Traditional Chinese, chosen from the system and overridable,
  switched without restart.
- Iconoir icon set, tinted from the theme, with a small in-house set for the
  shapes it does not have.

### Known gaps

- Archive extraction and creation are not built.
- Native Services and shell context-menu integration are not built.
- The Windows and Linux type-database, trash and Quick Look adapters are
  stubs; the terminal adapter is implemented on all three.
