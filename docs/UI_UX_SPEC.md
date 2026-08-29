# JT FileWork — UI / UX Specification

This document defines interaction behaviour independent of any GUI toolkit.
It must remain valid whichever stack ADR-0001 selects.

---

## 1. UX Principles

1. **Keyboard-first, not keyboard-only** (`AGENTS.md` §9). Every core action
   has a shortcut; every core action is also reachable by mouse or menu.
2. **Predictable over clever.** Native platform behaviour wins unless
   JT FileWork can be measurably better.
3. **Never block.** The window always responds, even when the filesystem does
   not (`AGENTS.md` §3).
4. **Never lie.** Progress is real, "done" means done, partial failure is
   shown as partial failure.
5. **Nothing is destroyed silently.**
6. **The user's layout is theirs.** Splits, tabs, sort, filter and marks
   survive restart.

---

## 2. Window Anatomy

```text
┌─ Title / toolbar (platform-appropriate) ────────────────────────────┐
├─ Location / breadcrumb / filter (per active tab) ───────────────────┤
│ ┌ Pane A ───────────────┐ ┌ Pane B ───────────────┐                 │
│ │ [Tab][Tab][Tab]   [+] │ │ [Tab][Tab]        [+] │                 │
│ │ file list             │ │ file list             │                 │
│ └───────────────────────┘ └───────────────────────┘                 │
├─ Tool area: Preview | Search | AI | Jobs (dockable, collapsible) ───┤
├─ Status bar: counts, selection, marked, active job, provider ───────┤
└─────────────────────────────────────────────────────────────────────┘
```

The tool area is not modal and never steals focus from the file list.

---

## 3. Workspace and Panes

Layout is a recursive split tree (`AGENTS.md` §6). The UI must never expose a
"left pane / right pane" concept in its API, its state, or its labels.

Required interactions:
- split active pane horizontally / vertically
- nested split to arbitrary reasonable depth
- 2×2 and 3-pane presets
- close active pane; the sibling takes its space
- resize by dragging the splitter and by keyboard
- focus next / previous / directional pane
- swap panes
- save and restore named workspace layouts

### 3.1 Active pane

Exactly one pane is active. The active pane must be identifiable at a glance
in **both** Light and Dark themes, and without relying on colour alone
(`TESTING.md` §10). Commands with a "target pane" (copy to other pane) resolve
against the active pane and a deterministic target rule.

---

## 4. Tabs

Tabs belong to a pane (`AGENTS.md` §7).

- new, close, reopen closed, duplicate, pin, reorder
- drag a tab to another pane; state moves with it
- middle-click closes; double-click on empty tab strip creates
- overflow handled by scrolling the strip, never by hiding tabs silently
- a tab shows its location's display name, with a disambiguating parent
  segment when two tabs would otherwise look identical

---

## 5. File List

### 5.1 View modes
- **List/detail** (primary, virtualized, configurable columns)
- **Icon/grid** (optional, Phase 2)

### 5.2 Columns
Name, size, kind, modified, created, permissions, owner, extension, tags,
path (in search results). User-configurable order, width, visibility, per tab.

### 5.3 Behaviour
- virtualized: 1M rows must scroll smoothly
- rows appear incrementally during enumeration; the list is usable before the
  scan completes
- sorting and filtering apply to the full result set, not only loaded rows
- a directory that is still loading shows a non-blocking progress affordance
- an empty directory, a permission error and a stalled mount are three
  visually distinct states with distinct messages

### 5.4 Type-ahead
Typing letters jumps to a matching entry. It never starts a rename and never
triggers a destructive command.

---

## 6. Selection and Marking

Two independent concepts (`AGENTS.md` §10).

| | Selection | Mark |
|---|---|---|
| Meaning | current native selection | persistent batch set |
| Set by | click, Shift/Cmd-click, arrows, Select All | Space / Insert / mark commands |
| Survives navigation | no | yes |
| Survives sort/filter | yes | yes |
| Visual | native selection styling | distinct mark indicator, distinguishable from selection in both themes |

Commands must state their target. Where ambiguity exists, the resolution order
is: **marked set if non-empty, otherwise selection, otherwise active file** —
and the UI states which was used.

---

## 7. Keyboard Model

```text
physical input -> keymap -> command id -> command bus -> operation
```

- keymaps are data, not code
- a platform-native preset and a CView/WinCV preset ship by default
- keymaps are user-editable; conflicts are detected and reported at load
- a command palette exposes every command by localized name and by id
- F-key operations follow the CView/WinCV preset where they do not collide
  with a platform-reserved key

### 7.1 Baseline commands (identifiers, not bindings)

```text
workspace.split.horizontal   workspace.split.vertical
workspace.pane.next          workspace.pane.previous
workspace.pane.close         workspace.pane.focus.direction
tab.new    tab.close    tab.reopen    tab.duplicate    tab.pin
tab.next   tab.previous tab.move_to_pane
nav.up     nav.back     nav.forward   nav.home    nav.goto
file.open  file.view    file.edit     file.rename
file.copy_to_target_pane   file.move_to_target_pane
file.trash file.delete  file.new_folder
file.mark.toggle  file.mark.all  file.mark.none  file.mark.invert
preview.toggle  preview.quicklook
search.open  search.ai
jobs.show   jobs.cancel_active
theme.set   locale.set
```

---

## 8. Mouse Model

Full parity for common operations. Native semantics: click, Cmd/Ctrl
multi-select, Shift range, rubber-band selection, context menu, splitter drag,
tab drag, drag-and-drop with platform modifier semantics for copy/move/link.

Drag-and-drop is required pane→pane, Finder/Explorer→app and app→
Finder/Explorer (`PRODUCT_SPEC.md` §9). The drop target and the resulting
operation must be visible **before** the drop.

---

## 9. Context Menu

Merged from providers in a stable order (`ARCHITECTURE.md` §10): core
commands, file-type commands, target-pane commands, AI commands, platform
native actions, shell ecosystem actions, plugin actions.

A slow or hung platform provider must not delay the menu; it contributes
asynchronously or is omitted with a note.

---

## 10. Jobs UI

Every file operation is a Job (`AGENTS.md` §13).

- a short operation shows inline progress in the status bar
- a long or failed operation is available in the Jobs panel
- each job exposes: progress, cancel, error detail, retry where safe, undo
  where safe
- conflicts surface as `WaitingForUser` with: skip, overwrite, rename, merge,
  apply-to-all, cancel
- a completed job with partial failure is reported as partial, listing the
  failed entries

---

## 11. Search UX

Deterministic search is first-class and always reachable (`AGENTS.md` §15).

- search opens with scope: this tab, this pane, workspace, or a chosen root
- results are a virtual folder in a tab, with a Path column
- results support the same selection, marking and operations as a directory
- saved searches reopen as tabs
- the AI entry point is a **separate, clearly labelled** field; it never
  silently reinterprets a deterministic query

---

## 12. Preview and Viewer

Preview is lightweight, cancellable and disposable. Viewer is stateful and
richer (`AGENTS.md` §14).

- selecting a file starts a preview; changing selection cancels the previous
  one and discards its result
- preview never blocks navigation
- oversized or unsupported content shows a bounded, explanatory state with an
  "open in viewer" action
- on macOS, Space triggers native Quick Look

---

## 13. Errors and Empty States

Every failure shows: what was attempted, which entry failed, why (localized
message plus a machine-readable code), and what the user can do next. No
dialog that only says "operation failed".

Distinct states: empty directory, filtered-to-empty, permission denied,
path not found, device not available, stalled mount, still loading.

---

## 14. Localization and Theme in the UI

- no user-visible literal strings (`AGENTS.md` §11)
- layout tolerates ~40% text expansion without clipping (`TESTING.md` §7)
- no sentence assembled from translated fragments
- Light / Dark / Follow System, switchable at runtime (`AGENTS.md` §12)
- semantic tokens only; no literal colours in UI code
- active pane, selection and marks remain distinguishable in both themes and
  do not rely on colour alone

---

## 15. Accessibility

Every control exposes role, name and value; the accessible name comes from the
localization catalogue. Focus order is deterministic and follows visual order.
Contrast meets WCAG AA in both themes. Full keyboard reachability is a test,
not an aspiration (`TESTING.md` §10).

---

## 16. Persistence and Session Memory

Restored on launch: workspace split tree, panes, tabs, per-tab location,
history, sort, filter, columns, view mode, scroll position, selection, marked
set, locale, theme mode, tool area layout.

### 16.1 The user decides

Startup behaviour is a setting (`docs/PRODUCT_SPEC.md` §5.1):

| Option | Behaviour |
|---|---|
| Restore last session (default) | Everything above comes back |
| Start at home | One pane, one tab, at the home location |
| Start at a fixed location | One pane, one tab, at a chosen path |

Plus two finer switches: remember reopen-closed-tab history, and remember the
marked set.

### 16.2 Rules the UI must honour

- The preference is persisted even when the workspace is not. Turning memory
  off is itself remembered.
- Turning memory off **erases** the stored session. The settings panel says so
  before it happens, because it is not reversible.
- Anything the user chose not to remember is never written, not merely ignored
  on read. A closed tab's path must not sit in the session file after the user
  turned that off.
- **Missing session**: normal first launch. No notice, no error.
- **Deliberate fresh start**: no notice. The user asked for it.
- **Corrupt or future-version session**: start fresh, and tell the user with a
  dismissible notice naming the machine-readable code. Never silently lose a
  layout.
- **Unavailable location** (an unmounted volume, a deleted directory): restore
  everything else, put the affected tab at a fallback location, and report
  which locations could not be restored.
- Saving a session must never change what the user is looking at.

### 16.3 When state is written

At least: on quit, on a layout change, and periodically while idle — so a
crash costs seconds of state, not a session. Writes are atomic
(`docs/UI_TEST_PLAN.md` SESS-005).
