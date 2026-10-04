# ADR-0007: Watching the folders the panes show

- **Status:** Accepted — built (0.6.57)
- **Date:** 2026-09-28
- **Deciders:** project owner
- **Decided:** 2026-09-28. Native file watching has stood as item 1 of the
  implementation state's "Next" list since the one-second timer went in, named
  there as an interim; the owner asked for the list to be worked through
  (「繼續」). This records how.

## Context

A pane shows a folder. When something else changes that folder - a download
finishing, a build writing its output, a terminal in the same directory - the
pane has to say so without being asked.

What exists is a one-second timer on the UI thread
(`MainWindow` → `jtf_poll_folders` → `App::poll_folders` and
`App::refresh_rows`). Each tick it:

1. `stat`s every pane's folder and re-lists the ones whose modification time
   moved, which catches entries appearing and disappearing; and
2. `stat`s every row on screen, which catches a file already in the folder
   growing or being touched - the folder's own time does not move for that.

It works, it is the same on all three platforms, and it was written as an
interim (`AGENTS.md` §10.2 wants interims named). What it costs:

- **Latency.** Up to a second before anything shows, and on the second step
  only for rows that happen to be on screen.
- **Work proportional to nothing happening.** Forty-odd `stat` calls a second
  per pane on an idle folder, forever.
- **Blocking I/O on the UI thread** (`AGENTS.md` §3). One `stat` of a local
  folder is microseconds. One `stat` of a folder on a network mount that has
  stopped answering is however long the kernel waits, and the window is frozen
  for it, every second.

Every file manager this project measures itself against solves this the same
way: ask the kernel to say when a watched directory changes.

| | How it watches | Network mounts |
|---|---|---|
| Finder | FSEvents | the server's change notifications (SMB) |
| Explorer | `ReadDirectoryChangesW` | the same call, which SMB forwards |
| Dolphin (`KDirWatch`) | inotify | **polled** - `KDirWatch` checks the filesystem type and polls NFS and SMB mounts every 5 s, because inotify only sees changes made by this machine |

## Options Considered

### A. Keep the timer

Nothing to build, and the three costs above stay. Rejected: it is on the
"Next" list precisely because it was never meant to stay.

### B. `QFileSystemWatcher` in the UI layer

Qt wraps inotify, kqueue on macOS (an open descriptor per watched path) and
`ReadDirectoryChangesW`. It is already linked.

But it puts a behaviour of the file model - when a listing is stale - into the
replaceable layer (`AGENTS.md` §4), its signals say only "this path changed"
with no kind, and on macOS it holds an open descriptor per path, which is how
it earns the reputation the comment on `poll_folders` records.

### C. The `notify` crate in a platform adapter (chosen)

`notify` 8.2.0 (CC0-1.0) is the watcher the Rust ecosystem uses -
`rust-analyzer`, `mdBook`, `watchexec`, `deno`. It uses FSEvents on macOS,
inotify on Linux and `ReadDirectoryChangesW` on Windows, and reports the kind
of each change. Its backends run on their own thread and deliver into a
channel, so the UI thread only drains a queue.

Put in `src/platform/watch` as `jtf-platform-watch`, next to the other
adapters, because deciding which folders cannot be watched (below) needs
`statfs` and `GetDriveTypeW`, and those belong nowhere else (`AGENTS.md` §5).

## Decision

**Option C.** Each pane's folder is watched, non-recursively, while a pane
shows it. Events are drained on a 250 ms tick that costs no system call when
nothing happened. An entry appearing, disappearing or being renamed re-lists
the folder - off the UI thread, as every listing is. The first change after a
quiet spell is read at once; while changes keep coming, re-reads are spaced by
the folder's size, because redrawing costs the window time per row (about
100 ms for twenty thousand, measured under the watchdog): every half second up
to five thousand entries, then 0.1 ms per entry, up to three seconds. A file that only changed size or time is
re-read on its own; more than 64 of those in one tick re-lists instead.

**A folder that cannot be watched is polled exactly as today.** That is:

- a folder on a network filesystem - NFS, SMB/CIFS, AFP, WebDAV, 9P, Ceph,
  AFS, FUSE - because the kernel only reports what this machine changed, which
  is the `KDirWatch` rule;
- a folder whose filesystem will not say what it is. The desktop's document
  portal, a FUSE mount, refuses `statfs` and then accepts an inotify watch
  that hears nothing another process does; found by this ADR's own test;
- a folder the watcher refused (inotify's per-user limit, a permission);
- everything, if the watcher could not start at all.

The poll is not deleted. It becomes the fallback, with the same one-second
cadence and the same behaviour the timer has today.

## Consequences

### Positive

- A change in a local folder shows in a quarter of a second, not up to one.
- An idle local pane costs nothing: no `stat` calls at all.
- Rows that are off screen are right too, so scrolling down never shows a size
  from before the change.

### Negative

- A dependency, and through it `fsevent-sys` on macOS (bindings to the
  CoreServices framework the system ships - no C of our own) and `inotify` on
  Linux. `notify` parses nothing untrusted - it reports paths the kernel
  gives it - so the fuzz-target requirement of `AGENTS.md` §20.5 does not
  reach it.
- `notify` is pinned to `=9.0.0-rc.5`, a release candidate. 8.2.0, the last
  release, panics inside the FSEvents callback on a flag bit it does not know
  or a path that is not UTF-8, and a panic there aborts the whole program;
  9.0 fixes both (notify #790) and has been through five candidates since
  April. It also brings `xxhash-rust`, under the Boost Software License,
  which `deny.toml` now allows. Move to 9.0.0 when it is released.
- Two code paths for one job, watched and polled. The polled one is the one
  that already exists and is tested; the watched one is new.
- FSEvents coalesces and delivers late by design (tens of milliseconds) and
  reports flags rather than a single kind. Handled by trusting none of the
  kinds for more than a hint: anything that is not plainly "this existing file
  changed" re-lists.

- A re-read replaces the whole listing and the window redraws every row,
  which for twenty thousand entries holds the UI thread for about 90 ms. That
  cost is not new - the poll paid it on every re-read, and entering the folder
  pays it once - but a watcher hears a folder being filled sooner and more
  often. Measured with the watchdog on the Linux test machine, twenty thousand
  files written in three seconds: 54 events over 16 ms with re-reads spaced by
  size, 77 without. The real answer is to apply the additions and removals to
  the rows in place rather than re-reading; that is a change to the listing,
  not to the watcher, and is left for it.

### Neutral

- The folder tree in the sidebar is not watched. It re-reads a folder when it
  is expanded, as it does today.
- Remote (SFTP) locations are neither watched nor polled, as today: nothing on
  this machine can see them change.

## Compliance

- `AGENTS.md` §3: adding a watch touches the path - `statfs` to decide whether
  it is on a network filesystem, then the backend's own call - and a path on a
  mount that has stopped answering blocks whoever touches it. So the adapter
  owns a thread that does both, and the UI thread only sends it the list of
  folders the panes show and drains what comes back. The one system call left
  on the UI thread is a `stat` per changed file, bounded at 64 a tick, for a
  file the watcher has just reported as local and changed.
- `AGENTS.md` §5: the `cfg` checks for `statfs` and `GetDriveTypeW` are in
  `src/platform/watch`; `tests/tests/architecture.rs` enforces it.
- `AGENTS.md` §20.2: a watch is non-recursive, so there is no depth to bound.
- Tests: the crate's own tests create, modify, rename and delete in a
  temporary folder and wait for each kind; the bridge's tests drive a pane
  through a change with no call to the poll.

## Revisit Criteria

- A platform's backend proves unreliable on a local filesystem - then that
  platform polls, and says so.
- A reason to watch the folder tree as well, or to watch recursively (a
  "folder sizes stay current" feature would be one).
