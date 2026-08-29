# jt-filework — Testing Specification

This document defines what "tested" means for jt-filework. It is the concrete
expansion of `AGENTS.md` §21 (Completion Checklist) and the `TODO.md` Quality
section.

Testing is not a phase. Every architectural rule in `AGENTS.md` that can be
mechanically verified **must** have a test that fails when the rule is broken.

---

## 1. Testing Principles

1. A rule without an enforcing test is a suggestion, not a rule.
2. Deterministic behaviour is tested for exact results; AI behaviour is tested
   for contracts, not for content.
3. Filesystem tests never depend on the developer's real home directory.
4. Every test that touches the filesystem creates and destroys its own fixture.
5. Tests must pass on macOS, Windows and Linux, or be explicitly gated by
   platform with a documented reason.
6. Flaky tests are bugs. A test that must sleep to pass is broken; use
   deterministic clocks, channels or barriers instead.
7. Performance claims require a benchmark, not an anecdote.

---

## 2. Test Levels

```text
L0  static / lint          rustfmt, clippy, dependency boundary checks
L1  unit                   pure logic, no I/O, no toolkit
L2  integration            real filesystem fixtures, job engine, providers
L3  contract               trait/interface conformance across implementations
L4  UI / interaction       toolkit-level, headless where possible
L5  system / manual        native OS behaviour that cannot be automated
L6  performance            benchmarks with recorded baselines
L7  robustness             fuzz, fault injection, hostile input
```

Every crate must be usable with `cargo test -p <crate>` in isolation.

---

## 3. L0 — Static and Architectural Tests

### 3.1 Mandatory lint gates

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
```

### 3.2 Dependency boundary tests (enforces AGENTS.md §4, §5)

The platform-neutral core must not be able to reach a GUI toolkit or a
platform SDK. This is enforced structurally, not by review:

- Core crates declare no dependency on Qt, Slint, AppKit, WinUI, GTK or any
  WebView crate. A test parses the workspace manifests and fails if a
  forbidden crate appears in a core crate's dependency graph.
- A test asserts that no file outside `src/platform/**` contains
  `#[cfg(target_os` , `#[cfg(windows)`, `#[cfg(unix)` or equivalent, except in
  an explicit allowlist with a written justification.

```text
test: architecture::core_has_no_gui_dependency
test: architecture::core_has_no_platform_sdk_dependency
test: architecture::no_target_os_cfg_outside_platform_layer
test: architecture::ui_layer_is_not_a_dependency_of_core
```

### 3.3 Hard-coded string test (enforces AGENTS.md §11)

A test scans UI-layer sources for user-visible string literals passed to
label/title/tooltip/menu APIs and fails on any literal that is not a
localization key.

```text
test: i18n::no_hardcoded_user_visible_strings
```

### 3.4 Hard-coded colour test (enforces AGENTS.md §12)

A test scans UI sources for literal colour values (`#rrggbb`, `rgb(`,
`Color::from_rgb`, platform colour constants) outside the theme token
definition module.

```text
test: theme::no_literal_colors_outside_token_module
```

---

## 4. L1 — Unit Tests

Pure logic, no filesystem, no toolkit, no clock, no network.

Mandatory unit coverage:

**Workspace split tree (AGENTS.md §6)**
- split horizontal / vertical produces a `Split` node, never a fixed pair
- nested split to depth ≥ 4 preserves tree shape
- 2×2 preset produces the expected tree
- closing a pane collapses its parent split and reparents the sibling
- closing the last remaining pane is rejected or replaced by an empty pane
- ratios stay within bounds after repeated resize
- serialize → deserialize round-trips identically
- a test asserts the public API contains no `left_pane` / `right_pane`
  accessor, so a dual-pane model cannot be reintroduced silently

**Pane and tabs (AGENTS.md §7)**
- each pane owns an independent tab list
- moving a tab between panes preserves history, sort, filter, view state,
  scroll and selection
- closing the active tab activates a deterministic neighbour
- reopen-closed-tab restores location and history
- pinned tabs cannot be reordered into the unpinned range
- per-tab state does not leak between tabs or panes

**Selection vs mark (AGENTS.md §10)**
- selection changes do not modify the marked set
- marked set survives navigation away and back
- marked set survives sort and filter changes
- an entry removed from disk is dropped from the marked set on refresh
- operations resolve their target correctly for
  `selection` / `marked` / `active file`

**Command bus (AGENTS.md §9)**
- a keymap binding resolves to a command id, never to a handler function
- every command id is dispatchable without any key event
- unknown command id is a typed error, not a panic
- conflicting keybindings are detected at keymap load time
- every command reachable by keyboard is also exposed to menu/mouse where the
  spec requires it

**i18n (AGENTS.md §11)**
- missing key in `zh-TW` falls back to `en`
- missing key in `en` is a hard error in debug builds
- placeholder arity mismatch between locales fails the test
- locale switch at runtime leaves workspace, tabs and marks untouched
- **key parity**: the key sets of `en` and `zh-TW` are identical
- no translated value is assembled from concatenated fragments (checked by a
  lint on the catalogue format, not on the code)

**Theme (AGENTS.md §12)**
- `ThemeMode::{System,Light,Dark}` round-trips through persistence
- every semantic token is defined in both light and dark palettes
- no token resolves to an undefined value in either palette
- system appearance change updates the resolved palette when mode is `System`
  and does not when mode is explicit

**Job engine (AGENTS.md §13)**
- state machine only allows
  `Queued → Running → {Completed, Failed, Cancelled, WaitingForUser}`
- cancellation from every cancellable state reaches `Cancelled`
- `WaitingForUser` resumes to `Running` after a conflict decision
- progress is monotonic and never exceeds total
- a failed job records an error detail and a machine-readable code

**File model**
- `FileEntry` distinguishes file / directory / symlink / alias / bundle /
  package / archive / device / remote / virtual
- names are handled as opaque OS strings, never as lossy UTF-8
- display name and raw name are separate fields and never conflated

---

## 5. L2 — Integration Tests

Run against real filesystem fixtures created per test in a temporary
directory, never in the user's home.

### 5.1 Fixture generator

A shared test crate builds trees on demand:

```text
fixture::flat(n)                  n files in one directory
fixture::deep(depth, fanout)      nested tree
fixture::unicode()                CJK, emoji, combining marks, RTL, NFC/NFD
fixture::hostile()                see §9.2
fixture::large_files(sizes)       sparse where the platform supports it
fixture::symlinks()               valid, broken, absolute, relative, cyclic
```

### 5.2 Required integration scenarios

- async directory enumeration delivers rows incrementally and completes
- enumeration of a directory that is deleted mid-scan fails cleanly
- enumeration cancelled mid-scan stops promptly and produces no late rows
- navigating away discards the previous enumeration's results (stale result
  rejection, AGENTS.md §3)
- copy/move across the same volume and across volumes
- copy into a destination that already exists triggers `WaitingForUser`
- conflict resolution: skip / overwrite / rename / merge-directory
- move to trash uses the platform trash and is reversible where the platform
  allows
- rename with case-only change on a case-insensitive filesystem
- batch rename applies atomically or reports partial completion precisely
- hashing a large file reports progress and is cancellable
- an operation on a path that becomes unreadable mid-flight reports the exact
  failing entry, not a generic failure
- session persistence: workspace with nested splits, multiple panes, multiple
  tabs, marks and scroll positions restores byte-identically

### 5.3 Symlink, alias and package semantics

- a symlink is never silently followed during recursive delete
- cyclic symlink does not cause unbounded recursion
- macOS alias resolves through the platform adapter, not by path guessing
- an application bundle is presented as a single item by default and is
  traversable on explicit request

---

## 6. L3 — Contract Tests

Every trait with more than one implementation gets a reusable conformance
suite that each implementation must pass:

```text
contract::filesystem_provider     local, archive, search-results, remote
contract::native_preview_service  macOS, Windows, Linux, null
contract::native_trash_service    macOS, Windows, Linux
contract::viewer_provider         text, image, hex, archive, structured
contract::ai_provider             ClaudeCodeCLI, CodexCLI, local, remote
contract::locale_source           file-backed, embedded, test double
```

A null/in-memory implementation of each service must exist so that core tests
never require a real desktop session.

### 6.1 AI provider contract (AGENTS.md §16)

Tested with a fake CLI binary, never with a real paid API call in CI:

- the provider spawns a process with an argument vector; a test asserts no
  shell string concatenation occurs anywhere on the path from user input to
  process spawn
- a path containing spaces, quotes, `;`, `$(`, backticks, newlines and
  non-UTF-8 bytes is passed through unmodified and executes nothing
- the working directory is explicit and the process cannot be launched without
  one
- output is streamed incrementally, not buffered to completion
- cancellation terminates the child process and its group
- changed-file detection reports exactly the files the fake agent touched
- a provider that fails to start produces a typed error, not a hang

---

## 7. L4 — UI and Interaction Tests

**The complete UI coverage plan is `docs/UI_TEST_PLAN.md`.** It enumerates
every UI surface, state and interaction as numbered `UI-*` cases, assigns each
one a harness layer, and defines the theme × locale × DPI screenshot matrix.
This section states only the level's rules; the cases live there.

The GUI stack is undecided (ADR-0001). The plan is therefore written against
behaviour, not against a toolkit API, and it is an input to that decision:
**a candidate that cannot execute the plan's gate areas automatically is
eliminated; one that cannot be driven headlessly loses points.**

Harness layers (`docs/UI_TEST_PLAN.md` §0.1): model-level, headless widget,
screenshot, driven app, manual. Every case is pushed down to the lowest layer
that can still prove it.

Determinism is mandatory: fake clock, injectable-latency provider, animations
disabled or clock-driven, fixed window size, font, DPI, locale and theme per
case, and a settled-state barrier. A UI test that needs a `sleep` is broken.

Gate areas: split layout, tabs, file list at scale, drag and drop, preview,
runtime locale switch, runtime theme switch, and the UI-thread watchdog below.

### 7.1 UI-thread blocking test (AGENTS.md §3 — non-negotiable)

A watchdog instruments the UI thread during a scripted interaction run and
fails the test if any single UI-thread task exceeds a budget (proposed: 16 ms,
tightened later).

Scenarios that must not block:
- entering a directory with 100 000 entries
- entering a directory on a simulated slow/stalled network mount
- selecting a 2 GB file with preview enabled
- selecting an archive with 50 000 members
- triggering an AI query
- triggering an external agent run

```text
test: uithread::no_task_exceeds_budget_during_large_directory_entry
test: uithread::no_task_exceeds_budget_during_slow_mount_navigation
test: uithread::no_task_exceeds_budget_during_preview_of_huge_file
```

The full scenario list is `docs/UI_TEST_PLAN.md` §18 (`UI-PERF-001` …
`UI-PERF-014`), which also records peak memory and memory after N cycles to
prove there is no unbounded growth.

---

## 8. L6 — Performance and Benchmarks

Benchmarks are code, live in the repository, and record baselines. A
performance-sensitive change without a benchmark does not satisfy
`AGENTS.md` §21.

### 8.1 Required benchmarks

```text
bench: enumerate_100k_entries
bench: enumerate_1m_entries_synthetic
bench: sort_100k_by_name_size_date
bench: filter_100k_substring_and_regex
bench: scroll_virtualized_list_100k
bench: session_restore_large_workspace
bench: hash_1gb_file
bench: archive_list_50k_members
bench: search_filename_glob_over_1m_paths
```

### 8.2 Targets (Phase 0 provisional, revised after ADR-0001)

`AGENTS.md` §18 makes these a product requirement rather than a goal, and
requires them to hold **on every platform**, on the lowest hardware the
project claims to support — not on the author's machine only.

**Responsiveness**

| Scenario | Target |
|---|---|
| First rows visible after entering 100K directory | < 150 ms |
| Full enumeration of 100K local entries | < 2 s |
| Sort 100K entries | < 250 ms |
| Keystroke to filter result on 100K entries | < 100 ms |
| Any UI-thread task | < 16 ms |
| Cold start to usable window | < 500 ms |
| Memory for 1M-entry model | documented, bounded, no unbounded growth |

**Display** (`AGENTS.md` §18.2)

| Scenario | Target |
|---|---|
| Scroll frame time, 100K rows, p95 | < 16 ms |
| Scroll frame time, 100K rows, p99 | < 24 ms |
| Dropped frames while scrolling during a background enumeration | 0 |
| Window resize / splitter drag frame time, p95 | < 16 ms |
| Repaint after a theme switch | < 100 ms, no visible stall |
| Repaint after a locale switch | < 100 ms, no visible stall |
| Cost of 2x DPI versus 1x, same window size | no measurable regression |
| Per-row work as a function of directory size | constant — the list is virtualized |

The last row is the one that matters most: if any per-frame cost grows with
the number of rows in the directory rather than the number on screen, the
list is not virtualized and no amount of tuning will save it.

### 8.3 Memory

- a 1M-entry model must have a measured, documented footprint
- repeated navigation between large directories must not grow memory without
  bound (leak check over N cycles)
- thumbnail and preview caches must have an enforced ceiling and be evicted

### 8.4 Slow storage simulation

A test harness injects latency and stalls into the provider layer:

```text
slow::latency(200ms)      every operation delayed
slow::stall(indefinite)   operation never returns
slow::flaky(10%)          random EIO
slow::disconnect()        mount disappears mid-enumeration
```

Under all four, the UI must remain responsive, operations must be cancellable,
and no operation may deadlock.

---

## 9. L7 — Robustness, Fuzzing and Hostile Input

### 9.1 Fuzz targets (AGENTS.md §17)

Everything in the untrusted set gets a fuzz target:

```text
fuzz: path_parsing
fuzz: query_parser              search query syntax
fuzz: archive_entry_parsing
fuzz: text_encoding_detection
fuzz: structured_viewer_json
fuzz: structured_viewer_yaml
fuzz: structured_viewer_xml
fuzz: structured_viewer_csv
fuzz: locale_catalog_parsing
fuzz: session_state_deserialization
```

Rules: no panic, no unbounded allocation, no unbounded recursion, no infinite
loop. Every crash found is committed as a regression corpus entry.

### 9.2 Hostile fixture set

Must be handled without crash, hang or path escape:

- names with `..`, leading/trailing spaces and dots
- names that are Windows reserved devices (`CON`, `NUL`, `COM1`, …)
- names longer than 255 bytes; paths longer than `PATH_MAX`
- non-UTF-8 byte sequences in names on Unix
- NFC vs NFD duplicates that collide on macOS
- RTL override and zero-width characters in names
- a symlink chain 100 deep and a symlink cycle
- an archive with absolute paths, `../` traversal, symlink members, a
  compression ratio bomb, and a member count bomb
- a file whose extension lies about its content
- a directory with 1M entries
- a file that changes size while being read

### 9.3 Path traversal

Extraction and any write derived from untrusted input must be verified to stay
inside the destination root. This is a dedicated test suite, not a code review
item.

### 9.4 Crash and session recovery

- kill the process mid-copy: the operation log allows the state to be
  reported accurately on next launch
- kill the process mid-write of session state: the previous session state is
  still loadable (atomic write, no truncated state file)
- a corrupted session file is rejected and replaced with a default workspace,
  never silently loses user data without a message

---

## 10. Accessibility Testing

- every actionable control exposes a role, a name and a value to the platform
  accessibility API
- the accessible name comes from the localization catalogue, not from an
  English literal
- full keyboard reachability is asserted by the L4 walkthrough
- focus order is deterministic and follows visual order
- contrast of text, selection and marked rows meets WCAG AA in both themes
- macOS VoiceOver, Windows Narrator and Orca are exercised manually per
  release (L5)

---

## 11. L5 — Manual and Platform Verification

Automation cannot cover everything. These are checklist items per release,
recorded with OS version and hardware.

**macOS**
- Quick Look with Space; embedded Quick Look panel
- drag Finder → app, app → Finder, with copy/move/link modifiers
- Trash and Put Back
- Open With, Share/Services
- Finder tags read and write
- app bundles and packages presented correctly
- system appearance change while running
- notarization and Gatekeeper acceptance of the signed build

**Windows**
- Explorer drag/drop both directions with modifiers
- Recycle Bin
- `IContextMenu` third-party entries (Nextcloud, 7-Zip, Git tooling)
- out-of-process shell extension host survives an extension crash
- UNC paths, long paths, reparse points, junctions
- light/dark following system

**Linux**
- Wayland and X11
- XDG Trash spec compliance
- MIME/Open With via desktop entries
- D-Bus and GIO integration
- drag/drop with Nautilus and Dolphin
- desktop theme following

---

## 12. AI Testing Policy

AI output is non-deterministic; tests target the contract and the guarantees,
never the wording.

Required:
- **AI never replaces deterministic search** (AGENTS.md §15): for a query with
  an explicit filter (exact name, glob, regex, size, date, metadata), a test
  asserts the deterministic result set is returned unchanged and unreordered
  regardless of the AI layer's state, including when the AI provider is
  unavailable, slow or returning nonsense
- a natural-language query with a deterministic component extracts that
  component and applies it exactly
- AI results are labelled as AI-derived in the result model
- every AI call runs as a job and is cancellable
- an AI provider timeout degrades to deterministic results with a visible
  notice, never to an empty result set presented as complete
- proposed batch operations from AI are never executed without explicit user
  approval; a test asserts no mutation occurs on plan generation

CI uses recorded fixtures and fake providers. No test in CI makes a paid or
network AI call.

---

## 13. Test Data and Isolation Rules

- Tests never read or write outside their temporary fixture directory and the
  target directory.
- Tests never require network access, except an explicitly tagged and
  skippable `network` group.
- Tests never require the user's real Nextcloud, cloud accounts or credentials.
- No secrets, tokens or personal paths in fixtures or snapshots.
- Snapshot files are reviewed like code in `git diff`.

---

## 14. CI Matrix

```text
job: lint          macOS   fmt + clippy + architecture tests
job: test-macos    macOS arm64      L1 L2 L3 L6(smoke)
job: test-windows  Windows x64      L1 L2 L3
job: test-linux    Linux x64        L1 L2 L3
job: i18n-parity   any              locale key parity + placeholder arity
job: bench         macOS arm64      nightly, baseline comparison, regression alert
job: fuzz          Linux x64        nightly, time-boxed, corpus persisted
```

`main` must stay green. A red `main` is fixed or reverted before new work
lands (`AGENTS.md` §2).

Platform jobs are added as their adapters land (Phase 4 / Phase 5), but the
platform-neutral core must build and test on all three from Phase 1.

---

## 15. Definition of Done

A change is complete only when all of the following hold:

- [ ] `cargo fmt --check` and `cargo clippy -D warnings` pass
- [ ] all affected test levels pass locally and in CI
- [ ] new logic has unit tests; new I/O has integration tests
- [ ] no new UI-thread blocking (§7.1 watchdog unchanged or improved)
- [ ] no new hard-coded user-visible string
- [ ] no new literal colour outside the token module
- [ ] if UI changed: the affected `UI-*` cases in `docs/UI_TEST_PLAN.md` pass,
      a new interaction has a new case, the screenshot matrix is regenerated
      and reviewed, and Light/Dark plus both locales are verified
- [ ] cancellation behaviour considered and tested where applicable
- [ ] error paths produce typed, machine-readable codes with localized display
- [ ] architecture boundary tests still pass
- [ ] performance-sensitive changes ship with a benchmark and a baseline
- [ ] platform impact documented
- [ ] `git diff` reviewed by a human
