# jt-filework — UI Conventions

`docs/UI_UX_SPEC.md` says what the product does. This says what every surface
must get right **without being asked**.

These are not preferences. They are the things a desktop application is
expected to do, that users notice only when they are missing, and that are
cheap to do at the time and expensive to retrofit. Treat this as a checklist
against every screen before calling it done.

---

## 1. Nothing is inert

- **A control that does nothing is disabled, and looks disabled.** Back and
  Forward are disabled when there is no history. A command with no
  implementation is greyed, not hidden — a keyboard layout that binds a
  command the menu does not show is a keyboard layout nobody can discover.
- **Double-clicking always does something.** A folder opens; anything else
  goes to the system's default application. Nothing happening is the single
  most obvious failure a file manager can have.
- **Every click target that looks pressable is pressable.** A header that
  sorts shows a sort indicator; if clicking changes state, the state is
  visible.

## 2. State is visible

- **The window title names what you are looking at**, then the application.
  A title that only ever says the app name is a wasted line.
- **Sorting shows which column and which direction.**
- **Counts are separate and named**: items, selected, marked. Selection and
  marking are different things (`AGENTS.md` §10), so they are counted
  separately and never added together.
- **Progress is real**, has a cancel button beside it, and an unknown total
  shows as indeterminate rather than as a guess.
- **The active pane is identifiable at a glance**, in both themes, without
  relying on colour alone.

## 3. Right-click works everywhere it plausibly could

- **A list row** has a context menu, and right-clicking an unselected row
  selects it first — acting on something other than what was clicked is a
  bug people only notice after it deletes the wrong thing.
- **A column header** has a menu to show and hide columns.
- **The context menu is built from the same commands as the menu bar**, so a
  command cannot exist in one place and not the other, and each entry shows
  the shortcut the keymap gives it.

## 4. Keyboard parity

- Everything reachable by mouse is reachable by keyboard
  (`AGENTS.md` §9), and the shortcut shown next to a menu item is the one
  the active keymap actually resolves.
- **Escape backs out** of whatever is in progress: a dialog, a search field,
  a drag, a viewer.
- **Enter commits** the obvious action.
- Type-ahead jumps to a row and never triggers a destructive command or an
  inline rename.
- Focus is never trapped, and the focus ring is always visible.

## 5. Every visual is a token, every string is a key

- No literal colour outside the theme module — including icons, which are
  drawn from tokens rather than shipped as fixed assets, and including
  tooltips, menus, scrollbars and headers, which some toolkits style from a
  palette rather than a stylesheet. Both are set.
- No user-visible string in UI code (`AGENTS.md` §11). Placeholders are named
  slots filled with data, never sentences assembled from fragments.
- Both are enforced by tests (`docs/TESTING.md` §3.3, §3.4), so a violation
  fails the build rather than a review.

## 6. Text and alignment

- **The file list is fixed-width by default.** With proportional text every
  column jitters, sizes do not line up digit for digit, and a long list is
  harder to scan than it needs to be.
- Numeric columns are right-aligned; dates use a fixed-width form.
- Font family and size are user-settable, and default to the platform's own
  fixed-width font — right on each OS, with correct CJK fallback and no
  licensing question.
- Row height follows the font, so a larger size does not clip descenders.

## 7. Destructive actions

- Say **before** the action when it cannot be undone, not after
  (`docs/UI_UX_SPEC.md` §10).
- The safe choice is the default button. A dialog people dismiss by reflex
  should do the harmless thing.
- Conflicts are asked once, up front, from a pre-flight scan, with the
  colliding path shown.
- The result names what actually happened — done, skipped, partial,
  cancelled — and identifies the first failing entry with its reason.
  "Permission denied" without a path is not something a user can act on.

## 8. Long and large

- No list is built by loading everything. Row height is uniform and cells are
  fetched as they are painted, so cost tracks what is on screen rather than
  what is on disk (`AGENTS.md` §18.2).
- While something streams in, rows are **inserted**, not the whole model
  reset: a reset per batch throws away the selection and the scroll position
  hundreds of times.
- A long-running action never blocks the window. Anything that might take
  more than a frame runs off the UI thread and reports progress.

## 9. Errors and empty states

- Six list states are visually distinct: empty folder, filtered to empty,
  permission denied, not found, device unavailable, still loading.
- Every failure says what was attempted, which entry failed, why, and what to
  do next. No dialog says only "operation failed".
- A refused action explains itself. Silently doing nothing is worse than an
  error.
- A background failure does not steal focus or interrupt typing.

## 10. Settings

- Changes apply as they are made. There is no panel where half the controls
  are live and half wait for OK.
- A setting that erases data says so at the moment it is changed, not at the
  next launch.
- Anything configurable is stored as data the application reads — a keymap
  file, a catalogue, a settings record — so the settings screen is an editor
  over that data rather than a second implementation of it.

---

## Review checklist

Before calling a UI change done, walk the surface once and confirm:

- [ ] every control is enabled only when it can act, and looks it
- [ ] every state change is visible somewhere
- [ ] right-click does something sensible on rows and on headers
- [ ] every action has a keyboard route, and Escape backs out
- [ ] no literal colour, no literal string
- [ ] columns align; the list is fixed-width
- [ ] destructive actions warn first and default to safe
- [ ] nothing loads more than it paints
- [ ] failures name the entry and the reason
- [ ] settings apply immediately and persist

This list is part of the Definition of Done (`docs/TESTING.md` §15).
