# JT FileWork — UI Test Plan

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
H1  model-level        drive commands through the command bus, assert on the
                       view-model. No toolkit, runs everywhere, fastest.
H2  headless widget    instantiate real widgets offscreen, synthesize input
                       events, assert on widget state and rendered output.
H3  screenshot         render deterministic scenes, compare against golden
                       images per theme / locale / DPI.
H4  driven app         launch the real application, drive it, observe.
H5  manual             checklist with recorded OS version and hardware.
```

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
| PANE-015 | Active pane is identifiable in Light and Dark, and without colour alone | H3 + manual |
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
| LIST-001 | 100K entries: first rows visible within budget | H4 + bench |
| LIST-002 | 100K entries: scroll frame times within budget at p95 | H4 + bench |
| LIST-003 | 1M synthetic entries: usable, memory bounded | H4 + bench |
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
| LIST-017 | Inline rename with IME composition is not interrupted by shortcuts | H4 + manual |
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

| ID | Case | Layer |
|---|---|---|
| MARK-001 | Mark toggle does not change selection | H1 |
| MARK-002 | Selection change does not change marks | H1 |
| MARK-003 | Mark all / none / invert, over the filtered set and over the full set, each explicit | H1 |
| MARK-004 | Marks survive navigation away and back | H1 |
| MARK-005 | Marks survive sort, filter and view-mode change | H1 |
| MARK-006 | Marks survive tab move to another pane | H1 |
| MARK-007 | Marks survive session restore | H1 |
| MARK-008 | An entry deleted externally is dropped from the marked set on refresh | H4 |
| MARK-009 | Marked rows are visually distinct from selected rows in both themes | H3 |
| MARK-010 | A row that is both marked and selected is unambiguous | H3 |
| MARK-011 | Marks across multiple directories are shown and counted correctly | H1/H3 |
| MARK-012 | Operation target resolution (marked → selection → active) is shown to the user before acting | H2 |

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
| DND-007 | Finder/Explorer → app | H4 + manual |
| DND-008 | App → Finder/Explorer | H4 + manual |
| DND-009 | Drop onto a read-only or full destination reports a clear error | H4 |
| DND-010 | Drop a directory onto itself or a descendant is refused before it starts | H1 |
| DND-011 | Drag from an archive extracts rather than moving | H2 |
| DND-012 | Drag to a third-party app that requests file promises | manual |
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
| MENU-008 | Submenus open in the right direction near a screen edge | manual |
| MENU-009 | Menu appearance follows the platform in both themes | H3 + manual |
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
| KEY-011 | Shortcuts do not fire while an IME composition is active | H4 + manual |
| KEY-012 | Shortcuts do not fire while a text field has focus, unless documented | H2 |
| KEY-013 | Modifier-key state is correct after window focus loss and regain | H4 |
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
| PREV-007 | macOS: Space opens native Quick Look; Space again closes | H4 + manual |
| PREV-008 | macOS: embedded Quick Look renders inside the tool area | manual |
| PREV-009 | Preview panel resize does not re-trigger a full reload | H2 |
| PREV-010 | Preview never executes content: no script, macro, or remote load | H2 |
| VIEW-001 | Text viewer: 10 GB log opens without loading it all | H4 + bench |
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
| I18N-006 | Taiwan terminology check on the zh-TW catalogue | manual |
| I18N-007 | Dates, times, numbers and file sizes are locale-formatted | H1 |
| I18N-008 | Filenames are never translated or transformed for display | H1 |
| I18N-009 | Plural forms render correctly at 0, 1, 2 and many | H1 |
| I18N-010 | No sentence is assembled from translated fragments (catalogue lint) | H0 |
| I18N-011 | Error display text comes from the catalogue; the code is shown separately | H1 |
| I18N-012 | IME composition works in filter, rename, search and AI fields | H4 + manual |
| I18N-013 | Layout does not break with RTL glyphs in filenames | H3 |
| I18N-014 | Locale switch while a job is running relabels the job without disturbing it | H4 |

---

## 16. Theme — `UI-THEME`  *(gate area)*

| ID | Case | Layer |
|---|---|---|
| THEME-001 | Runtime switch Light ↔ Dark ↔ System, no restart | H4 |
| THEME-002 | Follow System reacts to an OS appearance change while running | H4 + manual |
| THEME-003 | Explicit Light/Dark ignores an OS appearance change | H4 |
| THEME-004 | Theme preference persists across relaunch | H4 |
| THEME-005 | No literal colour outside the token module (static scan) | H0 |
| THEME-006 | Every semantic token defined in both palettes | H1 |
| THEME-007 | Contrast: primary and secondary text meet WCAG AA in both themes | H1 |
| THEME-008 | Contrast: active-pane indicator, focus ring, mark and selection all distinguishable in both themes | H1/H3 |
| THEME-009 | Icons legible in both themes; no baked-in single-theme asset | H3 |
| THEME-010 | Native menus and platform panels follow OS appearance | manual |
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
| A11Y-010 | No information conveyed by colour alone | H3 + manual |
| A11Y-011 | Respects reduce-motion and increase-contrast system settings | manual |
| A11Y-012 | VoiceOver / Narrator / Orca walkthrough per release | manual |

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

This is part of the Definition of Done (`docs/TESTING.md` §15).
