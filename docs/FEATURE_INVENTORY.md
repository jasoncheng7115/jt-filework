# jt-filework — Feature Inventory

Everything a file manager is expected to do, in one list, with an honest
status. Not a wish list: the point is that nothing is missing because nobody
thought of it.

`docs/PRODUCT_SPEC.md` says what the product is. This says what it must
**do**, item by item, so a gap is visible rather than discovered by a user.

Status: **done** · **partial** · **planned** · **later** (after Phase 2) ·
**no** (a deliberate non-goal)

---

## 1. Getting around

| | Status |
|---|---|
| Enter a folder, go up, back, forward | done |
| Editable path bar; paste a path and go | done |
| Home | done |
| Type-ahead jump | done |
| Breadcrumb with clickable segments | planned |
| Bookmarks / favourites sidebar | planned |
| Recent locations | planned |
| Volumes and mounts list | planned |
| Go to a path by typing `~` or an environment variable | planned |
| Follow a symlink to its target's folder | planned |

## 2. Seeing what is there

| | Status |
|---|---|
| Detail list with columns | done |
| Sort by any column, both directions, indicator shown | done |
| Show and hide columns | done |
| Directories first | done |
| Hidden files toggle | done |
| Native per-file icons | done |
| **Filter the current folder**, instantly, live | done |
| Item / selected / marked counts | done |
| Column widths remembered per tab | partial |
| Icon and grid views | planned |
| Thumbnails for images and video | planned |
| Folder sizes on demand | planned |
| Free space on the current volume | planned |
| Tree view sidebar | later |

## 3. Selecting and marking

| | Status |
|---|---|
| Click, Shift range, Cmd/Ctrl toggle | done |
| Select all | done |
| Persistent marked set, distinct from selection | done |
| Mark all, none, invert | done |
| Marks survive navigation, sort, filter, pane move | done |
| Select by pattern (`*.log`) | planned |
| Invert selection | planned |
| Select by same extension / same date | later |

## 4. Doing things to files

| | Status |
|---|---|
| Copy and move to the other pane | done |
| Rename | done |
| New folder | done |
| Trash | done |
| Permanent delete, warned before | done |
| Progress, cancel, per-entry result | done |
| Conflicts asked once, with skip as the safe default | done |
| Drag and drop, in and out, with modifiers | done |
| Duplicate in place | planned |
| Batch rename with a pattern and a preview | planned |
| Undo for move, rename and trash | done |
| Per-item conflict prompt with apply-to-all | planned |
| Create a file from a template | planned |
| Copy path, copy name to the clipboard | planned |
| Clipboard cut / copy / paste of files | planned |
| Compare two folders | later |
| Checksums (hash a file, verify a list) | later |
| Change permissions and ownership | later |
| Set the modification time | later |

## 5. Looking inside

| | Status |
|---|---|
| Native Quick Look | done |
| Text viewer, any size, indexed not loaded | done |
| Encoding override including Big5, GB18030, Shift-JIS, EUC-KR | done |
| Line endings shown, not normalized | done |
| Hex viewer as the universal fallback | done |
| Find within the file | done |
| Go to line | planned |
| Follow / tail a growing log | planned |
| Syntax highlighting | planned |
| Image viewer with zoom and EXIF | planned |
| Archive contents without extracting | planned |
| JSON / YAML / XML / CSV structured views | planned |
| Diff two files | later |
| Edit: delegate to the user's editor, then an internal one | planned |

## 6. Finding things

| | Status |
|---|---|
| Filter the current folder | done |
| Search a tree by name, glob and regex | in progress |
| Filter by size, date, kind, extension | in progress |
| Results as a virtual folder you can act on | planned |
| Search inside file contents | planned |
| Saved searches | planned |
| Optional index for speed | later |
| Natural-language and semantic search | later (Phase 3) |

## 7. Workspace

| | Status |
|---|---|
| Arbitrary recursive splits, nested | done |
| Layout presets | done |
| Independent tabs per pane | done |
| Move a tab between panes with all its state | done |
| Session restore, with an off switch that really forgets | done |
| Named saved layouts | planned |
| Detach a tab into a new window | later |
| Synchronised browsing between two panes | later |

## 8. Keyboard and configuration

| | Status |
|---|---|
| Every command reachable without a mouse | done |
| Keymaps as data, switchable at runtime | done |
| Platform and CView/WinCV presets | done |
| Rebind any command, with conflicts named | done |
| Settings window | done |
| Command palette | planned |
| Import and export a keymap | planned |
| Per-type "open with" preference | planned |

## 9. Platform integration

| | Status |
|---|---|
| Open with the default application | done |
| Drag to and from the system file manager | done |
| Quick Look | done |
| Reveal in Finder / Explorer | planned |
| Native trash with Put Back metadata | planned |
| Finder tags | planned |
| Share / Services | planned |
| Shell context-menu extensions (Windows) | later (Phase 4) |
| Network mounts, SMB/NFS/UNC | later |

## 10. Comfort

| | Status |
|---|---|
| Light, dark, follow system | done |
| English and Taiwan Traditional Chinese, switched live | done |
| Fixed-width list with a configurable font | done |
| Tooltips, context menus and headers all themed | done |
| Status line that says what is happening | done |
| Accessibility: roles, names, focus order | planned |
| Reduce-motion and increase-contrast support | planned |

## 11. Deliberately not doing

| | Why |
|---|---|
| Replacing the OS shell | `docs/PRODUCT_SPEC.md` §17 |
| Editing office documents | out of scope |
| Being a cloud storage service | out of scope |
| In-process third-party plugins | `docs/SECURITY.md` §6 |
| AI that silently replaces exact search | `AGENTS.md` §15 |

---

## How to use this

- Adding a feature means moving a row, not adding one that was forgotten.
- A row that goes **done** gets its `TODO.md` items ticked and, if it is
  user-visible, a `UI-*` case in `docs/UI_TEST_PLAN.md`.
- If something a file manager obviously needs is not on this list, that is a
  bug in this document. Add it before building anything else.
