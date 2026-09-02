# jt-filework — UI Test Plan

Complete coverage plan for the UI layer. `docs/TESTING.md` §7 defines the
level; this document enumerates **every UI surface, every state and every
interaction** that must be tested, and how.

The GUI stack is undecided (ADR-0001). This plan is therefore written against
*behaviour*, not against a toolkit API. It is also an input to ADR-0001: a
candidate that cannot execute this plan automatically is penalised, and a
candidate that cannot execute the gate rows at all is eliminated.

---

## 0. How UI Tests Run

### 0.1 Harness layers

```text
H0  static             a scan over the sources or the catalogues. Proves an
                       absence - no literal string, no literal colour - which
                       is the one thing no runtime test can prove.
H1  model-level        drive commands through the command bus, assert on the
                       view-model. No toolkit, runs everywhere, fastest.
H2  headless widget    instantiate real widgets offscreen, synthesize input
                       events, assert on widget state and rendered output.
H3  screenshot         render deterministic scenes, compare against golden
                       images per theme / locale / DPI.
H4  driven app         launch the real application, drive it, observe.
H5  manual             checklist with recorded OS version and hardware.
H6  benchmark          measured against a recorded baseline, fails on
                       regression rather than on an absolute number.
```

A case may name more than one, separated by `/`. A case for something that is
not built yet carries `—`, so that it is listed and cannot be mistaken for
something that passes.

**Rule: push every case down to the lowest layer that can still prove it.**
A case that only needs "pressing Tab moves focus to the next pane" belongs in
H1 against the focus model, not in H4.

Every row in this plan names its layer.

### 0.2 Determinism requirements

UI tests must not be flaky (`docs/TESTING.md` §1.6):

- a fake clock; no `sleep` as a synchronization primitive
- a controllable filesystem provider with injectable latency
  (`docs/TESTING.md` §8.4)
- animations disabled in test mode, or driven by the fake clock
- fixed window size, fixed font, fixed DPI per screenshot case
- fixed locale and theme per case, never inherited from the host machine
- a settled-state barrier: the harness can await "all pending UI work done"

### 0.3 The state matrix

Unless a row says otherwise, every screenshot case runs the full cross
product:

```text
theme   x  { Light, Dark }
locale  x  { en, zh-TW, pseudo }
dpi     x  { 1x, 2x }
```

`pseudo` is a generated locale with ~40 % longer strings and accented glyphs,
used to catch clipping and truncation (`docs/UI_UX_SPEC.md` §14).

### 0.4 Case identifiers

`UI-<AREA>-<NNN>`. Areas: `WIN`, `PANE`, `TAB`, `LIST`, `SEL`, `MARK`, `DND`,
`MENU`, `KEY`, `PAL`, `PREV`, `VIEW`, `JOB`, `SRCH`, `AI`, `SET`, `I18N`,
`THEME`, `A11Y`, `PERF`, `ERR`, `SESS`.

---

## 1. Window and Chrome — `UI-WIN`

| ID | Case | Layer |
|---|---|---|
| WIN-001 | Cold launch shows a usable window with one pane and one tab | H4 |
| WIN-002 | Window resize down to the minimum size clips nothing and hides no control | H2/H3 |
| WIN-003 | Window resize is smooth: no UI-thread task exceeds budget | H4 |
| WIN-004 | Maximize / restore / full screen preserve layout ratios | H4 |
| WIN-005 | Multiple windows have independent workspaces and do not share state | H4 |
| WIN-006 | Closing a window with a running job warns before discarding it | H2 |
| WIN-007 | Toolbar overflows into a menu rather than truncating | H2/H3 |
| WIN-008 | Status bar shows counts, selection count, marked count, active job | H1/H3 |
| WIN-009 | Status bar updates during enumeration without blocking | H4 |
| WIN-010 | Location/breadcrumb reflects the active tab and is click-navigable | H2 |
| WIN-011 | Breadcrumb truncates the middle, never the leaf, on a deep path | H3 |
| WIN-012 | Tool area docks, undocks, collapses, and restores its size | H2 |
| WIN-013 | Tool area never steals focus from the file list | H2 |

---

## 2. Split Layout — `UI-PANE`  *(gate area)*

| ID | Case | Layer |
|---|---|---|
| PANE-001 | Split horizontal creates a `Split` node, not a fixed pair | H1 |
| PANE-002 | Split vertical likewise | H1 |
| PANE-003 | Nested split to depth ≥ 4 renders and remains usable | H2/H3 |
| PANE-004 | 2×2 preset produces the expected tree and layout | H1/H3 |
| PANE-005 | 3-pane presets produce the expected trees | H1/H3 |
| PANE-006 | Close pane collapses the parent split; sibling takes the space | H1/H2 |
| PANE-007 | Closing the last pane is refused or leaves an empty pane, never a broken window | H1 |
| PANE-008 | Splitter drag resizes; ratio stays in bounds after many drags | H2 |
| PANE-009 | Splitter resize by keyboard | H2 |
| PANE-010 | Double-click splitter resets to an even ratio | H2 |
| PANE-011 | Focus next / previous cycles all panes deterministically | H1 |
| PANE-012 | Directional focus (up/down/left/right) picks the geometrically correct pane | H1 |
| PANE-013 | Swap panes preserves each pane's tabs and state | H1 |
| PANE-014 | Exactly one pane is active at all times | H1 |
| PANE-015 | Active pane is identifiable in Light and Dark, and without colour alone | H3/H5 |
| PANE-016 | Target-pane commands resolve against the active pane and the documented target rule | H1 |
| PANE-017 | Save / restore named layout round-trips the tree exactly | H1 |
| PANE-018 | A pane on a stalled mount does not freeze the other panes | H4 |
| PANE-019 | Very small pane still shows a usable list, or a documented minimum-size state | H3 |

---

## 3. Tabs — `UI-TAB`  *(gate area)*

| ID | Case | Layer |
|---|---|---|
| TAB-001 | Each pane owns an independent tab list | H1 |
| TAB-002 | New tab opens at the documented location and becomes active | H1 |
| TAB-003 | Close tab activates a deterministic neighbour | H1 |
| TAB-004 | Reopen closed tab restores location, history, sort, filter, scroll | H1 |
| TAB-005 | Duplicate tab copies state without aliasing it | H1 |
| TAB-006 | Pin / unpin; pinned tabs cannot be reordered into the unpinned range | H1/H2 |
| TAB-007 | Reorder by drag within a pane | H2 |
| TAB-008 | Drag a tab to another pane; all per-tab state moves with it | H2 |
| TAB-009 | Drag a tab to an invalid target cancels cleanly and changes nothing | H2 |
| TAB-010 | Middle-click closes; double-click on empty strip creates | H2 |
| TAB-011 | Tab strip overflow scrolls; no tab silently disappears | H2/H3 |
| TAB-012 | Two tabs with the same leaf name are disambiguated by a parent segment | H1/H3 |
| TAB-013 | Very long tab titles ellipsize without breaking the strip (pseudo-locale) | H3 |
| TAB-014 | Per-tab history: back/forward is per tab, never shared | H1 |
| TAB-015 | Switching tabs restores scroll position and selection exactly | H1/H2 |
| TAB-016 | Switching away from a loading tab cancels nothing the user still needs, and switching back resumes correctly | H4 |
| TAB-017 | Closing a tab cancels its in-flight enumeration | H4 |

---

## 4. File List — `UI-LIST`  *(gate area)*

| ID | Case | Layer |
|---|---|---|
| LIST-001 | 100K entries: first rows visible within budget | H4/H6 |
| LIST-002 | 100K entries: scroll frame times within budget at p95 | H4/H6 |
| LIST-003 | 1M synthetic entries: usable, memory bounded | H4/H6 |
| LIST-004 | Rows appear incrementally; the list is usable before the scan ends | H4 |
| LIST-005 | Sorting applies to the whole set, not only loaded rows | H1 |
| LIST-006 | Sort by each column, ascending and descending, stable ties | H1 |
| LIST-007 | Sort indicator shown on the sorted column only | H3 |
| LIST-008 | Filter applies to the whole set; keystroke-to-result within budget | H1/H4 |
| LIST-009 | Column show / hide / reorder / resize, persisted per tab | H1/H2 |
| LIST-010 | Column auto-size fits content without reflow storms | H2 |
| LIST-011 | View mode switch (list ↔ grid) preserves selection and scroll anchor | H1/H2 |
| LIST-012 | Hidden files toggle | H1 |
| LIST-013 | Symlink, alias, bundle, package, archive, device each render with the correct affordance | H3 |
| LIST-014 | Broken symlink renders as broken, not as a missing row | H1/H3 |
| LIST-015 | Type-ahead jumps to a match; never starts a rename or a destructive command | H2 |
| LIST-016 | Inline rename: commit, cancel with Esc, invalid-name rejection, conflict prompt | H2 |
| LIST-017 | Inline rename with IME composition is not interrupted by shortcuts | H4/H5 |
| LIST-018 | Unicode names (CJK, emoji, combining marks, RTL) render without layout damage | H3 |
| LIST-019 | A non-UTF-8 name renders with a lossy marker and is still operable | H2 |
| LIST-020 | Extremely long name ellipsizes; the full name is available on demand | H3 |
| LIST-021 | Empty directory, filtered-to-empty, permission denied, not found, device unavailable, stalled, still loading — six visually distinct states | H3 |
| LIST-022 | External change (file added/removed/renamed) updates the list without losing selection or marks | H4 |
| LIST-023 | Row context: keyboard-invoked context menu targets the focused row | H2 |
| LIST-024 | Grid view: icon size steps, keyboard navigation in two dimensions | H2 |

---

## 5. Selection — `UI-SEL`

| ID | Case | Layer |
|---|---|---|
| SEL-001 | Click selects one; click empty space clears | H2 |
| SEL-002 | Cmd/Ctrl-click toggles individual rows | H2 |
| SEL-003 | Shift-click selects a range; anchor behaviour matches the platform | H2 |
| SEL-004 | Shift+arrow extends; Cmd/Ctrl+arrow moves focus without selecting | H2 |
| SEL-005 | Rubber-band selection, including with modifiers | H2 |
| SEL-006 | Select all / none / invert | H1 |
| SEL-007 | Selection survives sort and filter; rows filtered out are excluded from operations and this is shown | H1 |
| SEL-008 | Selection is cleared on navigation, and restored by back/forward | H1 |
| SEL-009 | Selection count in the status bar matches the model exactly | H1 |
| SEL-010 | Selection styling differs between focused and unfocused pane | H3 |

---

## 6. Marking — `UI-MARK`  *(AGENTS.md §10)*

Selection **is** the mark. What is highlighted is what is ticked, and what is
ticked is what an operation acts on, however the rows were picked. The section
below used to describe the opposite — the two were separate states, and cases
MARK-001/002/012 asserted that changing one left the other alone. The rule was
changed on the project owner's decision; these cases are the current one.

Every case here is about a state the user built up over several actions. That
is the class of bug this section exists for: each action looks right on its own
and the set is wrong by the third one.

| ID | Case | Layer |
|---|---|---|
| MARK-001 | Space marks the row under the cursor and moves to the next | H2 |
| MARK-002 | **Space, Space, Space marks three rows.** Moving the cursor must not clear what the previous press marked | H2 |
| MARK-003 | **Clicking one row's tick box and then another leaves both ticked.** A box adds one thing to a set; it is not the same gesture as choosing one row | H2 |
| MARK-004 | Clicking a row (not its box) selects only that row, which by the rule above unmarks the rest — this is the one gesture that *does* replace the set | H2 |
| MARK-005 | A row marked by its box and a row marked by Space look identical. Two appearances for one state is the rule not holding | H3 |
| MARK-006 | Ctrl/Cmd-click adds a row without clearing the others | H2 |
| MARK-007 | Shift-click and Shift-arrow extend the set | H2 |
| MARK-008 | Mark all / none / invert, over the filtered set and over the full set, each explicit | H1 |
| MARK-009 | The header's box marks everything, clears everything, and shows "some" when only part is marked | H2 |
| MARK-010 | Clicking a row's own box updates the header's box | H2 |
| MARK-011 | Clearing the header's box leaves no row highlighted — the boxes and the highlight cannot disagree | H2 |
| MARK-012 | Marks survive navigation away and back | H1 |
| MARK-013 | Marks survive sort, filter and view-mode change | H1 |
| MARK-014 | Marks survive a tab moving to another pane | H1 |
| MARK-015 | Marks survive session restore | H1 |
| MARK-016 | An entry deleted externally is dropped from the marked set on refresh | H4 |
| MARK-017 | The status line counts the rows marked **in the folder on screen**, not every row the tab has ever marked | H1/H3 |
| MARK-018 | An operation acts on exactly the marked rows in the folder on screen | H2 |
| MARK-019 | Dragging a row that is not marked carries that row, not the marked set | H2 |
| MARK-020 | A row that is both marked and the cursor row is unambiguous in both themes | H3 |
| MARK-021 | The marked set is bounded. `Mark all` over a folder larger than the bound marks what fits and **says how many it did not** | H1/H2 |
| MARK-022 | Unmarking makes room, and the message about the bound goes with it | H1 |
| MARK-023 | **`Space`, `↓`, `Space`, `↓`, `Space` marks three rows.** Moving the cursor with an arrow key must not undo what Space marked | H2 |
| MARK-024 | The same for Page Up/Down, Home and End | H2 |
| MARK-025 | With **nothing** marked the arrows behave like any list — the highlight moves with the cursor. The rule above applies only while a set is being built | H2 |
| MARK-026 | The row the keyboard is on is visible even when it is not selected, and a row that is both cursor and marked reads as both | H3 |
| MARK-027 | Shift-arrow still extends the selection; Ctrl/Cmd-arrow still moves without selecting | H2 |
| TAB-020 | A tab can be pinned from its context menu and from the File menu, and shows a mark when it is | H2 |
| TAB-021 | A pinned tab refuses to close and cannot be dragged out of the leading block | H1 |

### 6.1 Why these are listed one action at a time

MARK-002 and MARK-003 were both broken at once, by two unrelated changes, and
neither was caught by anything:

- Space marked a row and then moved the cursor with
  `QAbstractItemView::setCurrentIndex`, which *also selects* what it moves to.
  Since selection is the mark, the tick appeared and vanished as the cursor
  stepped off the row. Marking a second file from the keyboard was impossible.
- And when that was fixed, the same bug remained by another route: the fix
  covered the cursor move that Space performs itself, and not the *arrow key*
  the user presses between two Spaces. The case that had been written was
  "Space, Space, Space" — which passes, because Space advances on its own.
  "Space, ↓, Space" was the one anybody would actually do.
- A mouse-press handler added to make dragging carry the pressed row selected
  that row on every left click — including a click on the tick box. Ticking a
  second box emptied the first.

Both were single-action-correct and multi-action-wrong. A case that says
"pressing Space marks the row" passes in both broken builds. That is why every
case above names a **sequence** and the state after it.

---

## 7. Drag and Drop — `UI-DND`  *(gate area)*

| ID | Case | Layer |
|---|---|---|
| DND-001 | Pane → pane copy/move with correct default per platform | H2/H4 |
| DND-002 | Modifier keys change the operation, and the change is visible before the drop | H4 |
| DND-003 | Drop indicator identifies the exact target row or directory | H2/H3 |
| DND-004 | Drag multiple selected files; count badge correct | H4 |
| DND-005 | Drag the marked set when the drag starts on a marked row | H2 |
| DND-006 | Esc during a drag cancels with no filesystem change | H4 |
| DND-007 | Finder/Explorer → app | H4/H5 |
| DND-008 | App → Finder/Explorer | H4/H5 |
| DND-009 | Drop onto a read-only or full destination reports a clear error | H4 |
| DND-010 | Drop a directory onto itself or a descendant is refused before it starts | H1 |
| DND-011 | Drag from an archive extracts rather than moving | H2 |
| DND-012 | Drag to a third-party app that requests file promises | H5 |
| DND-013 | Auto-scroll while dragging near a list edge | H2 |
| DND-014 | Spring-loaded folder open on hover, where the platform expects it | H4 |
| DND-015 | Drag over a stalled mount does not freeze the drag session | H4 |

---

## 8. Context Menu — `UI-MENU`

| ID | Case | Layer |
|---|---|---|
| MENU-001 | Provider order is stable: core, type, target-pane, AI, native, shell, plugin | H1 |
| MENU-002 | Menu opens within budget even when a provider is slow; slow provider contributes async or is omitted with a note | H2/H4 |
| MENU-003 | A crashing out-of-process provider does not affect the menu or the app | H4 |
| MENU-004 | Menu targets the marked set when non-empty, otherwise the selection, and says which | H2 |
| MENU-005 | Keyboard invocation and full keyboard navigation of the menu | H2 |
| MENU-006 | Every item is localized; no English literal leaks from a provider | H2 |
| MENU-007 | Disabled items explain why | H2 |
| MENU-008 | Submenus open in the right direction near a screen edge | H5 |
| MENU-009 | Menu appearance follows the platform in both themes | H3/H5 |
| MENU-010 | No item triggers a modal OS dialog that blocks the automation session | H4 |

---

## 9. Keyboard and Commands — `UI-KEY`, `UI-PAL`

| ID | Case | Layer |
|---|---|---|
| KEY-001 | Full keyboard-only walkthrough: split, focus, tab, navigate, mark, act, close — zero mouse events | H4 |
| KEY-002 | Every command in the registry is dispatchable with no key event | H1 |
| KEY-003 | Every command reachable by keyboard is also reachable by menu or mouse where the spec requires it | H1 |
| KEY-004 | Keymap binding resolves to a command id, never to a handler | H1 |
| KEY-005 | Conflicting bindings are detected at keymap load and reported | H1 |
| KEY-006 | Platform preset matches native expectations; CView preset matches the documented table | H1 |
| KEY-007 | Rebinding a key takes effect without restart and persists | H2 |
| KEY-008 | Focus is never trapped; Tab/Shift-Tab always escapes any panel | H2 |
| KEY-009 | Focus order is deterministic and follows visual order | H2 |
| KEY-010 | Focus ring is always visible in both themes | H3 |
| KEY-011 | Shortcuts do not fire while an IME composition is active | H4/H5 |
| KEY-012 | Shortcuts do not fire while a text field has focus, unless documented | H2 |
| KEY-013 | Modifier-key state is correct after window focus loss and regain | H4 |
| KEY-014 | `W` and `Alt-S` open search; `O` and `Alt-O` create a file; `Alt-P` toggles the preview panel; `primary+Enter` opens a terminal here — CV.HLP §二 keys we had commands for but had never bound | H2 |
| KEY-015 | Every command id named by the key hint strip exists in the registry | H1 |
| KEY-016 | In Single-Key mode the strip shows `C` and `M` for copy and move, not `Ins` and `Shift-C` — single keys come first | H2 |
| KEY-018 | `E` opens the entry under the cursor in the platform's text editor, and is absent on a folder or a remote row | H2 |
| KEY-019 | `O` creates a file and opens it for editing without a second keystroke | H2 |
| KEY-020 | `Shift`+letter moves the cursor to the first entry starting with it; pressing it again moves to the next, and wraps | H2 |
| KEY-021 | `Shift`+digit does the same for entries starting with that digit | H2 |
| KEY-022 | `Shift-Ins` moves to the other pane, and `Shift-C`/`Shift-M` no longer copy or move | H2 |
| KEY-023 | `H` opens the viewer in hex mode | H2 |
| KEY-024 | `Shift`+letter does **not** run the command bound to that bare letter — `Shift-H` jumps, it does not open the hex viewer | H2 |
| SET-020 | With fixed-width ticked the font list shows only fixed-width families, each with a digit's width beside it | H2 |
| SET-021 | Unticking it restores the full family list and keeps the chosen family | H2 |
| KEY-017 | In Native mode the strip shows no entry for a command that mode does not bind, rather than a blank | H2 |
| PAL-001 | Command palette lists every command by localized name and by id | H1/H2 |
| PAL-002 | Palette fuzzy match, ranking and recent-commands ordering | H1 |
| PAL-003 | Palette shows the binding for each command | H2 |
| PAL-004 | Palette executes against the correct active pane and target | H1 |
| PAL-005 | Palette closes on Esc without side effects | H2 |

---

## 10. Preview and Viewer — `UI-PREV`, `UI-VIEW`  *(gate area)*

| ID | Case | Layer |
|---|---|---|
| PREV-001 | Selecting a file starts a preview without blocking navigation | H4 |
| PREV-002 | Changing selection cancels the previous preview; a late result is never rendered | H4 |
| PREV-003 | Rapid arrow-key scrubbing through 200 files leaves exactly one preview showing, matching the final selection | H4 |
| PREV-004 | Oversized file shows a bounded state with "open in viewer", not an error | H2 |
| PREV-005 | Unsupported type falls back to hex, never to a blank panel | H2 |
| PREV-006 | Preview of a file on a stalled mount cancels cleanly | H4 |
| PREV-007 | macOS: Space opens native Quick Look; Space again closes | H4/H5 |
| PREV-008 | macOS: embedded Quick Look renders inside the tool area | H5 |
| PREV-009 | Preview panel resize does not re-trigger a full reload | H2 |
| PREV-010 | Preview never executes content: no script, macro, or remote load | H2 |
| VIEW-001 | Text viewer: 10 GB log opens without loading it all | H4/H6 |
| VIEW-002 | Text viewer: encoding override including Big5, GB18030, Shift-JIS, UTF-16 | H2 |
| VIEW-003 | Text viewer: mixed line endings shown, not normalized | H2 |
| VIEW-004 | Text viewer: a single 100 MB line does not hang | H4 |
| VIEW-005 | Text viewer: incremental search, go-to-line, wrap toggle | H2 |
| VIEW-006 | Log mode: follow/tail keeps up with an appending file and can be paused | H4 |
| VIEW-007 | Image viewer: zoom, fit, 1:1, pan, rotate; EXIF orientation respected | H2 |
| VIEW-008 | Image viewer: pixel-count ceiling refuses a decode bomb with a clear message | H2 |
| VIEW-009 | Hex viewer: navigation, grouping, find bytes, data inspector | H2 |
| VIEW-010 | Archive viewer: lists 50K members without extracting, and stays responsive | H4 |
| VIEW-011 | Archive viewer: ratio bomb and traversal member are refused with a clear message | H2 |
| VIEW-012 | Structured viewers: multi-GB JSON/CSV stream without a full DOM | H4 |
| VIEW-013 | CSV: delimiter/encoding detection with override; malformed rows reported not truncated | H2 |
| VIEW-014 | Viewer state (scroll, encoding, zoom) is per viewer instance and restored on reopen | H1 |
| VIEW-015 | Closing a viewer cancels its in-flight work | H4 |

---

## 11. Jobs UI — `UI-JOB`

| ID | Case | Layer |
|---|---|---|
| JOB-001 | Short operation shows inline status; long operation appears in the Jobs panel | H2 |
| JOB-002 | Progress is monotonic, never exceeds total, and reaches 100 % only on completion | H1 |
| JOB-003 | Cancel stops the job promptly and reports what was done | H4 |
| JOB-004 | Conflict prompt offers skip / overwrite / rename / merge / apply-to-all / cancel | H2 |
| JOB-005 | Apply-to-all applies only to the remaining conflicts of that job | H1 |
| JOB-006 | Partial failure is reported as partial, listing the failed entries | H2 |
| JOB-007 | Retry re-runs only the failed entries | H1 |
| JOB-008 | Undo is offered only where it is safe, and the UI says so before the action when it is not | H2 |
| JOB-009 | Multiple concurrent jobs are listed, individually cancellable | H2 |
| JOB-010 | A job continues correctly while the user navigates, splits, and switches tabs | H4 |
| JOB-011 | Error detail shows the failing entry, a localized message and a machine-readable code | H2 |
| JOB-012 | Operation log is written before a destructive action and is viewable | H2 |
| JOB-013 | Quitting with a running job warns, and the log survives a forced kill | H4 |

---

## 11a. Remote (SFTP) UI — `UI-RMT`  *(ADR-0004)*

Stage one is browsing. The rows about writing are listed so they are not
forgotten, and marked as belonging to stage two.

| ID | Case | Layer |
|---|---|---|
| RMT-001 | 前往 → 連線到伺服器 opens a dialog asking host, port, user, folder and an optional password | H2 |
| RMT-002 | Connect is disabled until a host is typed | H2 |
| RMT-003 | Leaving the user empty uses this machine's account, as `ssh host` does | H1 |
| RMT-004 | The dialog states plainly that it signs in with the agent or a key, and that no password is stored | H2 |
| RMT-005 | A successful connection lists the remote folder in the pane, with sorting and filtering working as they do locally | H4 |
| RMT-006 | An unreachable host reports an error in the pane and never blocks the UI thread | H4 |
| RMT-007 | A host key that has **changed** is refused, showing both fingerprints, and the refusal cannot be dismissed by any setting | H1 |
| RMT-008 | An unknown host is refused unless the user ticked "trust this host the first time"; accepting writes to `~/.ssh/known_hosts`, not to a store of our own | H1 |
| RMT-009 | A password typed in the dialog is used for one connection and is not present in the saved session | H1 |
| RMT-010 | Quick Look, Reveal in Finder, Open in Terminal and Move to Trash are absent or refuse on a remote row — they are about local files | H2 |
| RMT-011 | 中斷所有伺服器連線 closes the sessions and the panes report it | H4 |
| RMT-012 | Navigating away from a remote folder cancels its enumeration; no late rows arrive | H1 |
| RMT-013 | A session holding a remote tab reopens without the host being reachable, showing it as disconnected rather than failing at startup | H4 |
| RMT-014 | A connected server appears under 伺服器 in the sidebar and reopens with one click | H4 |
| RMT-015 | A saved server records host, port and account and **no credential** — asserted by serializing one and searching the JSON | H1 |
| RMT-016 | Connecting to the same host and account twice updates the entry rather than adding a second | H1 |
| RMT-017 | A server is labelled `user@host` unless the user named it | H1 |
| RMT-018 | Clicking a server connects rather than trying to open a local folder called `user@host` | H2 |
| RMT-019 | 忘記這台伺服器 removes it from the sidebar and the session | H2 |
| RMT-020 | Copy or move on a remote selection is refused with "writing is the next stage", not with "nothing is selected" | H1 |
| RMT-021 | Upload, remote rename and remote delete — **stage two**, not built | — |
| RMT-022 | The path bar on a remote pane reads `sftp://user@host/path`, not blank | H1 |
| RMT-023 | The port appears in that path only when it is not 22 | H1 |
| RMT-024 | The folder tree shows **no** selection while the pane is on a server, rather than keeping the last local folder highlighted | H2 |
| RMT-025 | Returning from a server to a local folder restores the tree's selection to that folder | H2 |
| RMT-026 | Bookmarking is refused on a remote folder — the bookmark list is about local paths | H1 |
| RMT-027 | A server that refuses the sign-in prompts for a password and retries with it | H2 |
| RMT-028 | That prompt hides what is typed, and the password is not in the saved session | H1 |
| RMT-029 | It appears once per failure, not once per refresh, and not at all for an ordinary permission error | H2 |
| RMT-030 | Clicking a saved server whose machine is off does not block the window; the error arrives when the attempt gives up | H4 |

---

## 11b. Archives — `UI-ARC`  *(ADR-0003)*

| ID | Case | Layer |
|---|---|---|
| ARC-001 | `Z` on a `.zip` asks where to extract; `Z` on a folder measures it instead | H2 |
| ARC-002 | `Z` on anything else says so rather than failing part way through | H2 |
| ARC-003 | `Alt-Z` compresses the marked entries, or the one under the cursor, into a ZIP the user names | H2 |
| ARC-004 | Extraction shows a live file count and byte total, and the window keeps redrawing throughout | H4 |
| ARC-005 | Cancel stops an extraction and removes the partial file | H1 |
| ARC-006 | A member whose path leads outside the destination is refused — `../`, absolute, drive letter, UNC, and the backslash spelling — and the count of refusals is shown | H1 |
| ARC-007 | A symlink member is refused, never created | H1 |
| ARC-008 | An archive that expands past the per-member or total bound stops and says so | H1 |
| ARC-009 | A ZIP written by `Alt-Z` extracts back to exactly what went in | H1 |
| ARC-010 | Extract and Compress appear in the context menu only where they apply, and are absent on a remote pane | H2 |
| ARC-011 | Deleting a member from an existing archive is absent, not broken — it is deliberately not built | H2 |
| ARC-012 | `Enter` on a ZIP opens a window listing its contents; `Enter` on anything else opens it the ordinary way | H2 |
| ARC-013 | A file named `.zip` that is not a readable ZIP falls back to opening it, rather than showing an empty listing | H2 |
| ARC-014 | In that window, `C` extracts the selected members and `X` extracts everything, both asking where to | H2 |
| ARC-015 | A member whose path leads outside the destination is shown in the listing, marked, with a tooltip saying it will be refused | H2 |
| ARC-016 | Closing the archive window releases the listing; reopening re-reads it | H1 |

---

## 12. Search UI — `UI-SRCH`

| ID | Case | Layer |
|---|---|---|
| SRCH-001 | Search opens with an explicit scope: tab, pane, workspace, chosen root | H2 |
| SRCH-002 | Query syntax errors show position and a localized message while typing | H2 |
| SRCH-003 | Results stream in and are actionable before the scan completes | H4 |
| SRCH-004 | Results behave as a virtual folder: selection, marking, operations, columns | H1 |
| SRCH-005 | Path column present and sortable in results | H1 |
| SRCH-006 | Cancel stops the scan promptly | H4 |
| SRCH-007 | Saved search reopens as a tab and refreshes on demand | H1 |
| SRCH-008 | Deterministic and AI fields are visually and functionally separate | H2/H3 |
| SRCH-009 | A deterministic query never routes through AI, even when the AI panel is open | H1 |
| SRCH-010 | Provenance (deterministic / semantic / reranked) is visible per row | H3 |
| SRCH-011 | Zero results is distinct from "still searching" and from "scan failed" | H3 |

---

## 12a. Sidebar: Places — `UI-PLACES`

The fixed half of the sidebar: favourites, bookmarks, servers, disks, recent
places. It cannot be hidden; only the folder tree below it folds away.

| ID | Case | Layer |
|---|---|---|
| PLACES-001 | Every section can be collapsed, and what was collapsed comes back collapsed after a restart | H4 |
| PLACES-002 | A section with nothing in it is not drawn — an empty heading is a promise of content | H2 |
| PLACES-003 | Clicking a place navigates the active pane; the sidebar does not become the focus | H2 |
| PLACES-004 | The selected row is drawn as **one** shape across the whole row, with no notch where the indentation meets it | H3 |
| PLACES-005 | A bookmark's menu offers rename, remove and reorder, and says「從書籤中移除」rather than an ambiguous「移除」 | H2 |
| PLACES-006 | 「加入書籤」is absent from that menu when the current folder is already bookmarked | H2 |
| PLACES-007 | A recent place can be bookmarked from its own menu, and that item is absent once it is one | H2 |
| PLACES-008 | Reordering a bookmark survives a restart | H4 |
| PLACES-009 | A server row shows whether it is connected, and offers connect / reconnect accordingly | H2 |
| PLACES-010 | A volume row shows a usage bar: accent below 75%, amber to 90%, red past it | H3 |
| PLACES-011 | The bars of two disks start and end at the same x, whether or not either can be ejected | H3 |
| PLACES-012 | The bar is re-read while the window is open — filling a disk changes it without navigating | H4 |
| PLACES-013 | Hovering a volume shows free and total | H2 |
| PLACES-014 | The eject control appears only on removable volumes, and only where the platform can eject | H2 |
| PLACES-015 | Clicking the eject control ejects and does **not** select the row behind it | H2 |
| PLACES-016 | The pointer becomes a hand over the eject control and an arrow elsewhere | H2 |
| PLACES-017 | **No row loses name width to a control it does not have.** A tree's column width is shared by every row; one eject button must not shorten every bookmark | H3 |
| PLACES-018 | A volume that is unmounted while the window is open disappears within one poll | H4 |
| PLACES-020 | **Only disks a person navigates to are listed.** A machine with a dozen snaps mounts a dozen read-only squashfs images; none of them is a place | H2/H5 |
| PAGES-001 | **The published pages show one language at a time.** The switch hides what is not current rather than showing what is; a component with its own `display` cannot outrank it | H2/H5 |
| PAGES-002 | Every navigation and index link has a target that exists and is visible — no heading carries a language's copy of another's id | H2 |
| PAGES-004 | Every image the pages reference exists on disk — a gallery whose images 404 is worse than no gallery | H2 |
| PAGES-005 | The gallery shows at least two screenshots of each of macOS, Windows and Linux; the three-platform claim is shown, not asserted | H5 |
| PAGES-003 | The Chinese on both pages and in both READMEs reads the way the author speaks: short sentences, no em-dash asides, no translated cadence | H5 |
| PLACES-021 | The filter is by filesystem type, not by path — the paths differ per distribution and the answer does not | H1 |
| PLACES-019 | Every row's menu offers「在新視窗開啟」 | H2 |

---

## 12b. Sidebar: Folder Tree — `UI-TREE`

| ID | Case | Layer |
|---|---|---|
| TREE-001 | The tree can be folded away and brought back; the places above it stay either way | H2 |
| TREE-002 | Its visibility survives a restart | H4 |
| TREE-003 | The tree follows the focused pane, including onto a server | H2 |
| TREE-004 | Expanding a node lists it lazily; a large folder does not block the window | H4 |
| TREE-005 | A folder's menu offers new window, new tab and bookmark | H2 |
| TREE-006 | An unreadable folder shows as unreadable rather than as empty | H2 |
| TREE-007 | The sidebar comes up at a sensible width, not half the window, whether or not the tree is showing | H4 |
| TREE-008 | A width the user dragged to is remembered; a width the layout invented is not | H4 |

---

## 12c. Path Bar — `UI-PATH`

| ID | Case | Layer |
|---|---|---|
| PATH-001 | `P` and `\` both open the field with the current path selected | H2 |
| PATH-002 | Typing filters a completion list drawn from the same source the tree lists with | H2 |
| PATH-003 | **Typing in the field never fires a single-key command.** Every letter reaches the field | H2 |
| PATH-004 | The same while the completion list is open — a popup taking the focus must not re-arm the commands | H2 |
| PATH-005 | Enter navigates; Escape closes the list, and a second Escape leaves the field | H2 |
| PATH-006 | Clicking a crumb navigates to that ancestor | H2 |
| PATH-007 | **Right-clicking does not open the editor.** It opens the folder's menu | H2 |
| PATH-008 | A path too long for the bar elides in the middle, keeping both ends | H3 |
| PATH-009 | A remote path is shown as `sftp://user@host/path` and round-trips through the field | H1 |

---

## 12d. Disc Usage — `UI-USAGE`

A report about a folder, in a window of its own.

| ID | Case | Layer |
|---|---|---|
| USAGE-001 | Opens from the File menu and from a folder's context menu | H2 |
| USAGE-002 | The window stays responsive while the walk runs | H4 |
| USAGE-003 | A spinner turns while it works and stops when it finishes | H2 |
| USAGE-004 | The status line names the folder currently being walked, elided in the middle | H2 |
| USAGE-005 | Cancel stops the walk, and what is shown afterwards is labelled incomplete | H2 |
| USAGE-006 | An unreadable subfolder makes the total say it is partial rather than being silently omitted | H1 |
| USAGE-007 | Both breakdowns reconcile to the same total | H1 |
| USAGE-008 | Files sitting directly in the folder get a row of their own, so the rows add up | H1 |
| USAGE-009 | A folder too wide to list is capped and the remainder gathered into one labelled row | H1 |
| USAGE-010 | The share bar is drawn against the largest row in the list, not the disk | H3 |
| USAGE-011 | **The share bar is visible on the selected row** — the accent it uses is that row's background | H3 |
| USAGE-012 | Every column heading sorts, and the list opens sorted by size | H2 |
| USAGE-013 | Size sorts by bytes, not by the formatted text — "9.9 MB" must not sort above "1.2 GB" | H1 |
| USAGE-014 | The kind column shows a different icon per type, not one generic document | H3 |
| USAGE-015 | Right, Enter and double-click descend; Left and Backspace go back | H2 |
| USAGE-016 | Descending puts the cursor on the first row | H2 |
| USAGE-017 | Tab moves between the two lists; Enter on a kind does nothing, because a kind is not a place | H2 |
| USAGE-018 | `C`, `M` and `D` act on the row under the cursor, and are on its context menu | H2 |
| USAGE-019 | `D` asks before it acts, from this window as from anywhere else | H2 |
| USAGE-020 | After an operation the level is measured again, and the panes refresh | H4 |
| USAGE-021 | The key strip names exactly the keys this window answers to | H2 |
| USAGE-022 | Symlinks are neither followed nor counted | H1 |
| USAGE-023 | Every icon and colour in the window follows a theme change without reopening it | H3 |

---

## 12e. Folder Comparison — `UI-CMP`

| ID | Case | Layer |
|---|---|---|
| CMP-001 | Compares the focused pane against the one a copy would go to — the same pair the badge names | H2 |
| CMP-002 | Works for a top/bottom split as well as left/right; nothing in the wording assumes sides | H2 |
| CMP-003 | "Include subfolders" is off by default and changes the result when turned on | H2 |
| CMP-004 | The window stays responsive while both trees are read | H4 |
| CMP-005 | Rows are classified only-here, only-there, differs, identical, and each is distinguishable in both themes | H3 |
| CMP-006 | The rule it compared by — size and modification time — is printed under the table, not assumed | H2 |
| CMP-007 | A row can be revealed in its pane | H2 |
| CMP-008 | Result rows are capped, and the cap is stated rather than silently truncating | H1 |
| CMP-009 | A symlink is a name, not a door: it is not walked into | H1 |

---

## 12f. Archives, Images and Compression — `UI-ARCX`

Extends §11b to everything the current build reads and writes.

| ID | Case | Layer |
|---|---|---|
| ARCX-001 | ZIP, `.tar`, `.tar.gz`/`.tgz`, `.tar.bz2`, `.tar.xz`, bare `.gz`/`.bz2`/`.xz`, and ISO 9660 all open a listing on Enter | H2 |
| ARCX-002 | The format is decided by content, not by the name — a `.tar.gz` called anything still opens | H1 |
| ARCX-003 | `.7z` and `.rar` are **not** offered as openable, and say so rather than failing after the attempt | H2 |
| ARCX-004 | ISO listings show Joliet or Rock Ridge names where present | H1 |
| ARCX-005 | `Space` marks, `C` extracts what is marked, `X` extracts everything | H2 |
| ARCX-006 | Extraction shows progress and can be cancelled; cancelling leaves no half-written file claiming to be extracted | H2 |
| ARCX-007 | Compressing a selection offers ZIP and `.tar.gz`; formats this build cannot write are refused plainly rather than half-attempted | H2 |
| ARCX-008 | A member whose name would land outside the chosen folder is refused **and reported** — in every spelling: `../`, absolute, drive letter, UNC, backslash | H1 |
| ARCX-009 | A symlink member is refused, not created | H1 |
| ARCX-010 | Expansion is bounded against bytes that actually arrive, not against what the header claims | H1 |
| ARCX-011 | A listing of a huge archive is bounded and says it was | H1 |
| ARCX-012 | The listing window's key strip matches the keys it answers to | H2 |

---

## 12g. Hint Strip and Status Bar — `UI-HINT`, `UI-STATUS`

| ID | Case | Layer |
|---|---|---|
| HINT-001 | The strip is built from the live keymap, never from a written list — rebinding a key changes it | H1 |
| HINT-002 | It changes with what the cursor is on: nothing, a file, a folder, several | H2 |
| HINT-003 | A key that would do nothing is not shown — switching panes with one pane open, for instance | H2 |
| HINT-004 | A hint that does not fit is dropped and the strip says so, rather than ending silently | H2 |
| HINT-005 | The strip uses short names; the menus keep the full ones — `D` reads「回收」in the strip and「移到資源回收筒」in the menu | H2 |
| HINT-006 | Its three density modes each do what they say, and the choice survives a restart | H4 |
| HINT-007 | Auto fades the strip while the list is worked and brings it back when the hands stop | H2 |
| STATUS-001 | Counts are per workspace, summed over every pane | H1 |
| STATUS-002 | The selection count counts rows **in the folder on screen** | H1 |
| STATUS-003 | A long message on the left elides in the middle and never pushes the counters off the end | H3 |
| STATUS-004 | An empty counter is hidden, not left as padding and a divider | H3 |
| STATUS-005 | A running search names the folder it is in | H2 |
| STATUS-006 | The shortcut chip opens the reference, and reads as a control rather than a readout | H2 |
| STATUS-007 | The job counter opens the job list | H2 |
| STATUS-008 | The zoom slider changes the list font and the setting survives a restart | H4 |

---

## 12h. Destructive Operations and Undo — `UI-SAFE`

| ID | Case | Layer |
|---|---|---|
| SAFE-001 | **Every removal is confirmed** — menu, key, or disc usage window, trash as well as permanent delete | H2 |
| SAFE-002 | Cancel is the default button; Escape cancels | H2 |
| SAFE-003 | Permanent deletion says it cannot be undone **before** it runs | H2 |
| SAFE-004 | The confirmation names how many items, and the icon shows the action rather than a question mark | H3 |
| SAFE-005 | The dialog is readable in both themes — no black glyph on a black ground | H3 |
| SAFE-006 | A conflict is reported with the first conflicting name and a choice, never resolved by guessing | H2 |
| SAFE-007 | Undo reverses the last operation and says what it reversed | H2 |
| SAFE-008 | Undo history is bounded and the bound is not a surprise | H1 |
| SAFE-011 | An operation too large to undo gets **no** undo entry, not a partial one, and the user is told | H1/H2 |
| SAFE-009 | An operation reports what it did, including what it refused | H2 |
| SAFE-010 | A delete never touches anything outside the selection, whatever the tree does while it runs | H1 |

---

## 12i. About, Version and Upgrade — `UI-ABOUT`, `UI-UPG`

| ID | Case | Layer |
|---|---|---|
| ABOUT-001 | The About box shows the application icon, the version and the Qt version | H3 |
| ABOUT-002 | The version it shows is the version that was built | H4 |
| ABOUT-003 | The window and taskbar icon is set on all three platforms | H3 |
| UPG-001 | A session from an older format is migrated and nothing the user chose is lost | H1 |
| UPG-002 | The pre-migration file is kept, named with its version | H4 |
| UPG-003 | **A session from a newer build is refused and left untouched** — the older build writes beside it, never over it | H1 |
| UPG-004 | The user is told, once, when the previous session could not be used | H2 |
| UPG-005 | A keymap naming a command by an id it used to have still reaches that command | H1 |
| UPG-006 | A corrupt session starts fresh, keeps the bad file, and says so | H1 |

---

## 12j. File Operations in the Interface — `UI-OPS`

The dialogs and the feedback, not the engine. `docs/TESTING.md` §5.2 covers
what the operations do to a filesystem; this covers what the user is told.

| ID | Case | Layer |
|---|---|---|
| OPS-001 | Copy and move to the other pane, and to a folder chosen in a dialog, are both offered and both say where they will land | H2 |
| OPS-002 | The destination dialog opens on the current folder and remembers nothing it should not | H2 |
| OPS-003 | Rename opens with the name selected and the extension excluded from the selection | H2 |
| OPS-004 | Rename to a name that exists reports the clash instead of overwriting | H2 |
| OPS-005 | Batch rename previews every result **before** anything is renamed, and refuses to run if the preview shows a collision | H2 |
| OPS-006 | A batch-rename pattern that is user input cannot hang the preview | H1 |
| OPS-007 | Duplicate produces a name that does not collide and says what it made | H2 |
| OPS-008 | New folder and new file ask for a name, reject an empty one, and put the cursor on what they created | H2 |
| OPS-009 | The read-only toggle opens showing what is currently true rather than a guess | H2 |
| OPS-010 | Properties shows size, dates, permissions and location, and says「未計算」for an unmeasured folder rather than 0 | H2 |
| OPS-011 | **A folder measured with `Z` updates the properties panel**, whose path did not change | H2 |
| OPS-012 | A long operation shows progress, names what it is on, and can be cancelled | H2 |
| OPS-013 | A queued second operation is shown as queued, not as lost | H2 |
| OPS-014 | Every operation's result is reported once, in words, and cleared as it is read | H2 |
| OPS-015 | Copy path puts exactly the marked rows on the clipboard — not rows marked in a folder no longer on screen | H1 |
| OPS-016 | Cut/copy/paste through the system clipboard round-trips with the platform's own file manager | H5 |

---

## 12k. Views, Sorting and Thumbnails — `UI-VIEWS`

| ID | Case | Layer |
|---|---|---|
| VIEWS-001 | List and icon view show the same set, the same marks and the same cursor row | H2 |
| VIEWS-002 | Switching view keeps the cursor on the same entry | H2 |
| VIEWS-003 | The view mode is per tab and survives a restart | H4 |
| VIEWS-004 | Thumbnails load off the UI thread; a folder of large images does not stall the window | H4 |
| VIEWS-005 | A thumbnail that cannot be made falls back to the type icon rather than to blank | H2 |
| VIEWS-006 | Sorting by each column ascends and descends, and the heading says which | H2 |
| VIEWS-007 | Size sorts by bytes and date by instant, never by the formatted text | H1 |
| VIEWS-008 | Folders-first is honoured in both directions of every sort | H1 |
| VIEWS-009 | The sort is per tab and survives a restart | H4 |
| VIEWS-010 | Hidden files appear and disappear with the setting, and the count changes with them | H2 |
| VIEWS-011 | Column widths survive a restart; the name column fits the width it is actually drawn at | H2 |
| VIEWS-012 | **Columns are aligned on first paint**, not one resize later | H3 |

---

## 12l. Windows and Tear-off — `UI-MULTI`

| ID | Case | Layer |
|---|---|---|
| MULTI-001 | A tab dragged out becomes its own window holding that tab | H2 |
| MULTI-002 | A tab dragged onto another window's pane joins it | H2 |
| MULTI-003 | Dropping a tab anywhere on a pane works, not only on the tab strip | H2 |
| MULTI-004 | Every window shows the same model: a change in one appears in the others | H4 |
| MULTI-005 | Closing the main window closes the torn-off ones — they are parts of one workspace | H2 |
| MULTI-006 | Window positions and sizes survive a restart | H4 |
| MULTI-007 | The window title follows the focused pane | H2 |
| MULTI-008 | "Open in new window" from a folder's menu opens that folder | H2 |

---

## 12m. Platform Integration — `UI-PLAT`

Where the platforms differ, the difference must be visible rather than a
command that does nothing.

| ID | Case | Layer |
|---|---|---|
| PLAT-001 | Quick Look on macOS is the system panel, and the same one Finder shows | H5 |
| PLAT-002 | Where there is no system equivalent the command is **hidden**, not offered and inert | H2 |
| PLAT-003 | "Reveal in file manager" opens the platform's own and selects the entry | H5 |
| PLAT-004 | "Open in terminal" opens at the current folder | H5 |
| PLAT-005 | Open With lists the applications the platform associates, and ends with a chooser | H2 |
| PLAT-006 | Eject is offered only where the platform can, and reports failure rather than assuming success | H2 |
| PLAT-007 | Trash goes to the platform's own trash and is recoverable from it | H5 |
| PLAT-008 | A dropped file from another application is copied or moved after asking which | H2 |
| PLAT-009 | A file dragged out is accepted by the platform's own file manager | H5 |
| PLAT-010 | The type icon for an entry is the platform's, and a row about a *kind* asks about the type rather than about a file | H3 |

---

## 13. AI UI — `UI-AI`

| ID | Case | Layer |
|---|---|---|
| AI-001 | The scope sent to a provider is shown before the call | H2 |
| AI-002 | Output streams incrementally and is cancellable mid-stream | H4 |
| AI-003 | Provider unavailable / slow / failing shows a degraded state, never a fake result | H2 |
| AI-004 | An operation plan is rendered as a reviewable list with per-step opt-out | H2 |
| AI-005 | Generating a plan mutates nothing | H1 |
| AI-006 | Executing a plan creates ordinary jobs with ordinary progress and conflict handling | H1 |
| AI-007 | Remote-provider indicator is visible whenever a remote provider is active | H3 |
| AI-008 | AI output is rendered as text: no active content, no remote loads | H2 |
| AI-009 | An AI response cannot change settings, providers, or keybindings | H1 |
| AI-010 | External agent run shows working directory, streamed output, and a changed-file diff | H4 |

---

## 14. Settings — `UI-SET`

| ID | Case | Layer |
|---|---|---|
| SET-001 | Theme mode selector: System / Light / Dark, applied immediately | H2 |
| SET-008 | Startup selector: Restore last session / Start at home / Start at a fixed location | H2 |
| SET-009 | Choosing a fixed start location opens a picker and validates the path | H2 |
| SET-010 | Turning session memory off warns that the stored session will be erased, then erases it | H2/H4 |
| SET-011 | With memory off, no path from the previous session exists in the stored state | H1/H4 |
| SET-012 | "Remember closed tabs" off: no closed tab's path is written | H1 |
| SET-013 | "Remember marks" off: layout still restores, marked set does not | H1 |
| SET-014 | The startup preference itself persists across relaunch even with memory off | H4 |
| SET-002 | Language selector applied immediately | H2 |
| SET-003 | Keymap preset selector and per-command rebinding | H2 |
| SET-004 | Every setting persists and is restored on relaunch | H1/H4 |
| SET-005 | Resetting a setting to default works and is reversible within the session | H2 |
| SET-006 | An invalid or corrupt settings file falls back to defaults with a visible notice | H1 |
| SET-007 | Settings UI itself is fully keyboard operable and localized | H2 |

---

## 15. Internationalization — `UI-I18N`  *(gate area)*

| ID | Case | Layer |
|---|---|---|
| I18N-001 | Runtime locale switch redraws every visible string, no restart | H4 |
| I18N-002 | Locale switch preserves workspace, panes, tabs, selection, marks, scroll, jobs | H1/H4 |
| I18N-003 | No user-visible string literal in UI sources (static scan) | H0 |
| I18N-004 | Every visible string resolves to a key present in both locales | H2 |
| I18N-005 | Pseudo-locale (+40 % length): no clipping, no overlap, no truncated control | H3 |
| I18N-006 | Taiwan terminology check on the zh-TW catalogue | H5 |
| I18N-007 | Dates, times, numbers and file sizes are locale-formatted | H1 |
| I18N-008 | Filenames are never translated or transformed for display | H1 |
| I18N-009 | Plural forms render correctly at 0, 1, 2 and many | H1 |
| I18N-010 | No sentence is assembled from translated fragments (catalogue lint) | H0 |
| I18N-011 | Error display text comes from the catalogue; the code is shown separately | H1 |
| I18N-012 | IME composition works in filter, rename, search and AI fields | H4/H5 |
| I18N-013 | Layout does not break with RTL glyphs in filenames | H3 |
| I18N-014 | Locale switch while a job is running relabels the job without disturbing it | H4 |

---

## 16. Theme — `UI-THEME`  *(gate area)*

| ID | Case | Layer |
|---|---|---|
| THEME-001 | Runtime switch Light ↔ Dark ↔ System, no restart | H4 |
| THEME-002 | Follow System reacts to an OS appearance change while running | H4/H5 |
| THEME-003 | Explicit Light/Dark ignores an OS appearance change | H4 |
| THEME-004 | Theme preference persists across relaunch | H4 |
| THEME-005 | No literal colour outside the token module (static scan) | H0 |
| THEME-006 | Every semantic token defined in both palettes | H1 |
| THEME-007 | Contrast: primary and secondary text meet WCAG AA in both themes | H1 |
| THEME-008 | Contrast: active-pane indicator, focus ring, mark and selection all distinguishable in both themes | H1/H3 |
| THEME-009 | Icons legible in both themes; no baked-in single-theme asset | H3 |
| THEME-010 | Native menus and platform panels follow OS appearance | H5 |
| THEME-011 | Theme switch mid-drag, mid-job and mid-preview does not corrupt state | H4 |
| THEME-012 | Screenshot matrix for every major surface in both themes | H3 |

### 16.1 Screenshot surface list

Each rendered in the full §0.3 matrix: file list (populated, empty, error,
loading), tab bar (normal, overflow, pinned), split layouts (2, 2×2, nested),
active vs inactive pane, selection, marked rows, selection+mark combined,
context menu, command palette, preview panel per viewer type, jobs panel
(running, conflict, partial failure), search panel with results, AI panel,
settings, status bar, drag indicator.

---

## 17. Accessibility — `UI-A11Y`

| ID | Case | Layer |
|---|---|---|
| A11Y-001 | Every actionable control exposes role, name and value | H2 |
| A11Y-002 | Accessible names come from the catalogue, not English literals | H2 |
| A11Y-003 | Full keyboard reachability (same walkthrough as KEY-001, asserted via the accessibility tree) | H2 |
| A11Y-004 | Focus order deterministic and matching visual order | H2 |
| A11Y-005 | List exposes row count, position and selection state to assistive tech | H2 |
| A11Y-006 | Marked state is exposed to assistive tech, distinctly from selected | H2 |
| A11Y-007 | Progress and job state are announced as they change | H2 |
| A11Y-008 | Errors are announced, not only rendered | H2 |
| A11Y-009 | Contrast AA for text, selection and marks in both themes | H1/H3 |
| A11Y-010 | No information conveyed by colour alone | H3/H5 |
| A11Y-011 | Respects reduce-motion and increase-contrast system settings | H5 |
| A11Y-012 | VoiceOver / Narrator / Orca walkthrough per release | H5 |

---

## 18. UI-Thread Responsiveness — `UI-PERF`  *(hard gate, AGENTS.md §3)*

A watchdog instruments the UI thread and fails the run if any single task
exceeds the budget (`docs/TESTING.md` §7.1).

| ID | Scenario |
|---|---|
| PERF-001 | Enter a directory with 100 000 entries |
| PERF-002 | Enter a directory with 1 000 000 synthetic entries |
| PERF-003 | Navigate on a mount with 200 ms injected latency |
| PERF-004 | Navigate on a mount that stalls indefinitely |
| PERF-005 | Mount disappears mid-enumeration |
| PERF-006 | Select a 2 GB file with preview enabled |
| PERF-007 | Select an archive with 50 000 members |
| PERF-008 | Sort and filter a 100K list repeatedly |
| PERF-009 | Scroll a 100K list continuously for 30 s |
| PERF-010 | Trigger an AI query and an external agent run |
| PERF-011 | Run three concurrent copy jobs while browsing |
| PERF-012 | Restore a large session (many panes, many tabs, large marked sets) |
| PERF-013 | Rapid tab switching between loading tabs |
| PERF-014 | Theme and locale switch under load |
| PERF-015 | Scroll 100K rows: frame time p95 and p99 within budget |
| PERF-016 | Scroll while a background enumeration runs: zero dropped frames |
| PERF-017 | Splitter drag and window resize hold the frame budget |
| PERF-018 | 2x DPI costs no measurable frame time over 1x |
| PERF-019 | Per-frame cost is constant in directory size — proof the list is virtualized |
| PERF-020 | Cold start to a usable window |
| PERF-021 | The same budgets hold on Windows and Linux, not only macOS |

Also recorded per scenario: peak memory, and memory after N cycles to prove no
unbounded growth.

---

## 19. Errors and Empty States — `UI-ERR`

| ID | Case | Layer |
|---|---|---|
| ERR-001 | Every failure states what was attempted, which entry failed, why, and what to do next | H2 |
| ERR-002 | No dialog says only "operation failed" (static scan on the catalogue plus review) | H0/H2 |
| ERR-003 | Machine-readable code shown alongside the localized message and is copyable | H2 |
| ERR-004 | A background failure does not steal focus or interrupt typing | H2 |
| ERR-005 | Repeated identical errors are coalesced, not stacked into a wall of dialogs | H2 |
| ERR-006 | Six distinct list states render distinctly (see LIST-021) | H3 |

---

## 19a. Writing a Disk Image — `UI-IMG`

The most destructive thing the program does: no undo, no trash, and the disk
is gone. The cases below are weighted towards *what is never offered* rather
than *what the dialog says*, because the dialog is read after the decision.

| ID | Case | Layer |
|---|---|---|
| ARCH-030 | **A folder inside an archive is a folder.** Enter or Right descends, Backspace or Left goes up, a row shows its own name and not its whole stored path | H2 |
| ARCH-031 | Folders appear even when the archive stores no directory entries — a zip of only `a/b.txt` still shows `a` | H2 |
| ARCH-032 | Marks survive walking into a folder and back out; the count is of the whole archive, not the level on screen | H2 |
| ARCH-033 | Ticking a folder ticks the members under it, and extraction gets those rather than an empty directory | H2 |
| ARCH-034 | Going up puts the cursor on the folder just left, not at the top of the list | H2 |
| VIEW-040 | Opening the hex view narrows the window to the width of the dump, and never widens it beyond the screen | H2 |
| PANE-030 | **With three or more panes the copy/move target can be moved from the keyboard**, round the panes and back | H1/H2 |
| PANE-031 | Cycling the target never lands on the active pane, including after the pane it pointed at was closed | H1 |
| PANE-032 | With two panes cycling the target is a no-op: there is only one other pane and it is already the target | H1 |

| SESS-030 | **A session written before a newly added field still loads.** A new field without a serde default makes every existing session unreadable — every tab, mark and open folder gone on upgrade | H1 |
| SESS-031 | The status bar's 「儲存的工作階段讀不出來」 appears only when the session really could not be read, and not on every launch | H2 |
| PANE-040 | **Changing the copy/move target moves nothing.** Drag a file across three panes: the target badge follows the pointer and no pane changes width | H2/H3 |
| PANE-041 | The target badge is an arrow and one word, not a sentence in a tab bar | H3 |
| TIME-001 | **A timestamp is shown in the machine's own zone**, not UTC. The list and the inspector agree to the minute | H1/H2 |
| TIME-002 | The list and the inspector use the same date format | H2 |
| MENU-010 | Every top-level menu has an access key, and no two claim the same letter | H2 |
| INSP-010 | The inspector's labels are left-aligned; both columns have a straight left edge | H3 |
| PREV-010 | **The preview background honours the chosen colour.** A stylesheet background must not override the palette the choice sets | H2 |
| PREV-011 | The colour button shows the colour it is set to, with its hex value beside it | H2 |
| PREV-012 | The preview background defaults to white | H2 |
| QL-001 | **Space in native mode with the Quick Look panel already open does not crash.** The panel is the key window; forwarding its keys to the key window re-entered the handler until the stack ran out | H5 |
| QL-002 | Space on the item already showing closes the panel; on a different item it swaps rather than stacking | H5 |
| OPS-040 | **Rename acts on the row the cursor is on, not on a mark left behind.** Mark row 1, move to row 5, press R: row 5's name is in the box and row 5 is what gets renamed | H2 |
| SORT-010 | Folders-first is off by default, and is a toolbar toggle rather than only a setting | H2 |
| SORT-011 | The folders-first icon changes shape with its state, not only its background | H3 |

| IMG-001 | **The disk carrying the running system is never listed.** Checked on a real machine, on each platform | H1/H5 |
| IMG-002 | An internal disk is never listed, even when it is not the boot disk — a second SSD is not a write target | H1 |
| IMG-003 | A disk whose properties could not be read is not listed. "I could not tell" never produces a row | H1 |
| IMG-004 | A card reader with no card (size zero) is not listed | H1 |
| IMG-005 | A machine booted from a USB stick does not have that stick offered, though it is genuinely removable | H1 |
| IMG-006 | Opening the dialog selects nothing, and Write is disabled until a disk is deliberately chosen | H2 |
| IMG-007 | **A disabled Write button looks disabled** — not painted as the filled default button | H3 |
| IMG-008 | A disk too small for the image is listed, greyed, with both numbers in the reason | H2 |
| IMG-009 | The disk holding the image itself is listed, greyed, and says why | H2 |
| IMG-010 | The warning names the disk by model, not "the selected disk" | H2 |
| IMG-011 | Each row shows the volumes mounted from it, so two identical sticks are distinguishable | H2 |
| IMG-012 | Verification is on by default and has to be turned off deliberately | H2 |
| IMG-013 | The stages are named as they run: unmounting, writing, flushing, verifying | H2 |
| IMG-014 | Progress is monotonic within a stage and resets to indeterminate when the stage changes | H1/H2 |
| IMG-015 | A disk that reads back differently fails verification, and the byte offset is reported | H1 |
| IMG-016 | A disk that wraps around (a counterfeit) fails verification rather than the write | H1 |
| IMG-017 | An image shorter than its stated length is refused, not half written | H1 |
| IMG-018 | Cancelling stops the write and says the disk is partly written and unusable — never "cancelled" alone | H2 |
| IMG-019 | Cancelling before the write begins leaves the disk byte-for-byte unchanged | H1 |
| IMG-020 | The final write is padded to a whole sector; the reported length is the image's own | H1 |
| IMG-021 | The CRC-32 shown is the one every other tool prints for the same bytes (check value `0xCBF43926`) | H1 |
| IMG-022 | The dialog does not close itself on completion — the checksum is why it was opened | H2 |
| IMG-023 | Declining the authorization prompt is reported as a refusal, not as a disk failure | H2 |
| IMG-024 | The disk is unmounted before it is opened, not after | H1 |
| IMG-025 | Asking for the device list twice gives the same disks in the same order | H1 |
| IMG-026 | On a platform with no implementation the dialog says so, rather than showing an empty list | H2 |

---

## 20. Session and Recovery — `UI-SESS`

| ID | Case | Layer |
|---|---|---|
| SESS-001 | Full state restores: split tree, panes, tabs, locations, history, sort, filter, columns, view mode, scroll, selection, marks, locale, theme, tool area | H4 |
| SESS-002 | Restore after a forced kill loses nothing that was already committed | H4 |
| SESS-003 | A corrupt session file is reported and replaced with a default workspace, never silently | H1/H4 |
| SESS-004 | A session referencing a now-missing volume restores the rest and reports the gap | H4 |
| SESS-005 | Session written atomically: a kill mid-write leaves the previous state loadable | H2 |
| SESS-006 | Restore of a very large session stays within the UI-thread budget | H4 |
| SESS-007 | Default install restores the last session — remembering is the default, not opt-in | H1 |
| SESS-008 | First launch (no stored session) opens at home with no error notice | H1/H4 |
| SESS-009 | Deliberate fresh start shows no notice; a corrupt session does | H1/H2 |
| SESS-010 | Corrupt-session notice names the machine-readable code and is dismissible | H2 |
| SESS-011 | A session from a newer format version is not guessed at; it starts fresh and reports | H1 |
| SESS-012 | A structurally valid but internally inconsistent session is rejected, and the fallback workspace is sound | H1 |
| SESS-013 | Saving the session never changes what the user is currently looking at | H1 |
| SESS-014 | State is written on quit, on layout change, and periodically while idle | H4 |
| SESS-015 | Session written after a settings change reflects the new preference immediately | H4 |
| SESS-016 | Quitting from a full-screen or multi-window state restores the same window arrangement | H4 |

---

## 21. Manual-Only Checklist

Recorded per release with OS version and hardware
(`docs/TESTING.md` §11): native Quick Look, Finder/Explorer drag both
directions, Trash/Recycle Bin and Put Back, Open With, Share/Services,
Finder tags, third-party shell extensions, spring-loaded folders, screen-edge
menu placement, system appearance change, reduce-motion, screen readers,
multi-monitor with mixed DPI, external keyboard layouts, IME per language.

---

## 22. Coverage Rule

A UI change is not complete until:

- the affected `UI-*` cases pass
- a new interaction has a new `UI-*` case in this document
- the screenshot matrix is regenerated and reviewed in `git diff`
- the UI-thread watchdog run is unchanged or improved

### 22.1 A case names a sequence, not an action

Most of what has actually broken in this program was correct for one action
and wrong for three. Marking, twice: `Space` marked a row and the cursor move
that followed unmarked it; a mouse handler added for dragging cleared the set
on every click, so ticking a second box emptied the first. Both would pass a
case that said "pressing Space marks the row".

So: **a case says what state exists after a sequence of actions.** "Space,
Space, Space marks three rows" is a case. "Space marks a row" is not enough.
The same goes for anything that accumulates — marks, tabs, undo history, the
job queue, bookmarks, recent places.

### 22.2 Every fix earns a case

A bug that reached the user is a gap in this document, not only in the code.
The change that fixes it adds the case that would have caught it, at the
lowest layer that can prove it, and says in the case what the sequence was.

This is part of the Definition of Done (`docs/TESTING.md` §15).
