# Reference layout

![The reference layout](reference-layout.png)

This mockup is the target. It is not a screenshot of the program — it is the
picture the program is being built towards, and where this document and the
code disagree, the picture wins for *layout* and the written specs win for
*behaviour*.

One correction to the image: its title bar reads with capitals and a space.
The product name is `jt-filework`, lowercase and hyphenated, in the title bar
and everywhere else — `AGENTS.md` §10.1, enforced by
`tests/tests/architecture.rs::product_name_has_one_spelling`.

## What the picture commits us to

### 1. Window chrome

A single toolbar row: back, forward, up, refresh · view-mode group (icons,
list, detail, columns) · preview toggle · split-layout menu · path field ·
search field · filter menu · overflow menu. Navigation on the left, search on
the right, view controls between them.

### 2. Panes

Four panes in a 2×2 grid, each complete in itself: its own tab strip with a
`+`, its own back/forward/up/home row, its own path field, its own bookmark
star and settings gear, its own column header, its own status line.

A pane is a whole file manager. Nothing in a pane reaches outside it. This is
the recursive split tree of `AGENTS.md` §7 drawn out: there is no "left pane"
and no "right pane", only panes.

### 3. The list

Leading checkbox, file-type icon, then `Name · Size · Modified · Type`. The
sort indicator sits in the sorted column's header. The checkbox is the mark
set — the same one the keyboard's space bar drives, not a second concept.

### 4. Per-pane status line

`3 of 10 selected` on the left, the size of the selection in the middle, the
item count on the right. Three facts, three anchors, no wrapping.

### 5. Inspector

A right-hand panel: a preview of the focused file, then Type, Size, Modified,
Created, and format-specific rows (Authors and Pages for a PDF), then
`Show More`. Its own header with a close control, so it can go away.

### 6. Bottom dock

Tabs across the bottom left — AI Assistant, Search, Tasks, Transfers,
Bookmarks, History — and a job list on the bottom right with per-job progress,
pause and cancel, filtered by All / Running / Completed / Failed.

### 7. Window status bar

The workspace as a whole: readiness, pane count, total selection with its
size, total item count, running task count.

## State

| Area | Status |
| --- | --- |
| Toolbar: navigation, refresh, tree toggle, overflow | Done |
| Toolbar: view-mode group, split-layout menu | Planned |
| Per-pane tabs, path, status line | Done |
| Breadcrumb path with clickable segments | Done |
| Per-pane bookmark star and gear | Planned |
| List columns `Name · Size · Modified · Type` | Done |
| Checkbox marks in the list | Done |
| Sort indicator | Done |
| Window status bar aggregate | Done |
| Inspector panel | Planned |
| Bottom dock: Tasks, Transfers, Bookmarks, History | Planned |
| Bottom dock: AI Assistant | Planned — `docs/SEARCH_AI.md` |
