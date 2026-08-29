# AGENTS.md — jt-filework

This file defines mandatory rules for Claude Code, Codex, other coding agents, and human contributors.

## 1. Read Before Coding

Before changing implementation, read:

1. `README.md`
2. `docs/PRODUCT_SPEC.md`
3. `docs/ARCHITECTURE.md`
4. the relevant subsystem document
5. all relevant accepted ADRs

If a change alters a major architectural boundary, write or update an ADR first.

## 2. Git Is Mandatory

The repository must be under Git from the first implementation commit.

Rules:
- `main` must remain buildable and testable.
- Work on feature/PoC branches.
- Do not mix unrelated changes in one commit.
- Architecture/specification changes are committed just like source changes.
- Use `git status` and `git diff` before and after AI-driven modifications.
- Do not commit build artifacts, caches, generated temporary files, AI scratch data, secrets, or local credentials.
- Phase 0 candidate UI implementations should use separate branches such as:
  - `poc/qt6`
  - `poc/slint`

## 3. No Blocking I/O on UI Thread

Forbidden on UI thread:
- directory enumeration
- recursive traversal
- metadata/stat storms
- thumbnails
- preview generation
- hashing
- archive inspection
- network filesystem access
- full-text indexing
- semantic indexing
- AI calls
- Claude Code CLI
- Codex CLI
- external helper execution

Expensive operations must:
- be asynchronous
- support cancellation
- reject stale results
- report progress where relevant

## 4. GUI Framework Must Be Replaceable

Core logic must not depend on:
- Qt
- Slint
- AppKit
- WinUI/Win32 UI
- GTK
- WebView

The UI layer consumes commands, models, and service contracts.

## 5. Platform Code Must Be Isolated

Preferred high-level layout:

```text
src/
  core/
  workspace/
  search/
  viewer/
  jobs/
  ai/
  platform/
    macos/
    windows/
    linux/
  ui/
```

Avoid scattered `#ifdef`, `cfg(target_os)` or platform checks across unrelated core modules.

## 6. Multi-Pane Is a Core Architecture Requirement

Never implement:
```text
leftPane
rightPane
```

Use recursive split layout:

```text
Workspace
└── Split(horizontal|vertical)
    ├── Pane
    └── Split(...)
        ├── Pane
        └── Pane
```

Each Pane owns independent Tabs.

## 7. Tabs Are Per-Pane

Every pane has:
- tab list
- active tab
- independent history
- independent sort/filter/view state

Tabs must be movable between panes.

## 8. Native Semantics

Use native platform behavior where users expect it:
- Finder/Explorer drag-and-drop semantics
- Quick Look on macOS
- Windows Shell context integration
- Trash / Recycle Bin / XDG Trash
- Open With
- Share / Services where supported
- native file type/association behavior

Do not mimic native capabilities poorly if a stable public API exists.

## 9. Keyboard-First, Not Keyboard-Only

Correct flow:

```text
physical input
 -> keymap
 -> command
 -> command bus
 -> operation
```

Do not wire keyboard events directly to business logic.

All core commands must also be invokable from mouse/menu UI where appropriate.

## 10. Selection and Mark Are Different

Support:
- native current selection
- persistent CView-style marked set

Do not conflate their state.

## 10.1 The Product Name Is `jt-filework`

Lowercase, hyphenated, everywhere: documentation, window title, bundle
identifier, package names, catalogue values, file names, commit messages.

Not `JT FileWork`, not `JTFileWork`, not `Jt-Filework`. One spelling means
search finds everything and no build artefact disagrees with any other.

## 10.2 Ordinary UI Conventions Are Not Optional

`docs/UI_CONVENTIONS.md` lists what every surface must get right without being
asked: disabled controls that look disabled, visible state, right-click menus,
keyboard parity, tokens instead of colours, alignment, warnings before
destructive actions, and failures that name the entry and the reason.

They are not preferences. They are what a desktop application is expected to
do, they are noticed only when missing, and they are cheap now and expensive
later. Walk that document's checklist before calling a UI change done.

## 10.3 Know What Is Missing

`docs/FEATURE_INVENTORY.md` lists everything a file manager is expected to do,
with an honest status for each. It exists so that a gap is visible rather than
discovered by a user.

Adding a feature means moving a row, not adding one that was forgotten. If
something a file manager obviously needs is not on that list, that is a bug in
the document; fix it before building anything else.

## 11. i18n Is Mandatory

No user-visible string literals in UI implementation.

Initial locales:
- `en`
- `zh-TW`

Requirements:
- localization keys
- fallback language
- locale switch without data loss
- placeholders/pluralization handled by the chosen localization system
- layout must tolerate text expansion
- no concatenating translated sentence fragments

## 12. Theme Is Mandatory

Support:
- Light
- Dark
- Follow System

Requirements:
- runtime theme switching
- no restart if framework allows stable switching
- no hard-coded UI colors
- icons must remain legible in both themes
- platform-native menus/panels should follow OS appearance

## 13. File Operations Are Jobs

Copy/move/rename/delete/trash/extract/compress/hash/batch rename require:
- progress
- cancellation
- conflict resolution
- logging
- error detail
- retry where safe
- undo where safe

## 14. Preview and Viewer Are Separate

Preview:
- lightweight
- cancellable
- disposable
- no edits

Viewer:
- stateful
- richer navigation/search/tools

## 15. Search Rules

Deterministic search remains first-class even after AI search exists.

AI must never silently replace:
- exact filename
- wildcard
- regex
- metadata
- date/size
- content query
- explicit user filters

## 16. External AI Agents

Claude Code / Codex CLI must run through:
- provider abstraction
- job engine
- explicit working directory
- safe argument passing
- streamed output
- cancellation
- changed-file detection
- diff/result capture

Never build shell commands by concatenating untrusted file paths.

## 17. Security Boundaries

Treat as untrusted:
- archives
- document parsers
- external preview helpers
- shell extensions
- plugins
- AI agents
- remote filesystems

## 18. Native Performance on Every Platform

This is a product requirement, not an aspiration. It constrains technology
choices, and it outranks convenience.

### 18.1 Native execution

Every supported platform runs **natively compiled code**:

- no interpreter, no VM, and no scripting runtime in any hot path
- **no WebView for the file list, the tab bar, the pane chrome, or any core
  surface** — WebView is permitted only for the auxiliary panels named in
  `docs/ARCHITECTURE.md` §17 (AI conversation, Markdown, rich diff, docs)
- no cross-platform abstraction that costs a marshalling layer per row
- the same architecture on macOS, Windows and Linux. "Fast on the primary
  platform, acceptable elsewhere" is a failed requirement, not a trade-off

### 18.2 Display performance

Rendering is half of perceived speed and gets its own budgets:

- the file list is **virtualized**: work is proportional to rows on screen,
  never to rows in the directory
- scrolling holds the frame budget at p95 and p99, not merely at p50
- no dropped frames while a background enumeration, thumbnailing or indexing
  job is running
- window resize and splitter drag do not trigger relayout storms
- high-DPI costs nothing extra: no software upscaling of rendered content
- theme and locale switches repaint without a visible stall
- text rendering uses the platform's own text stack, so CJK, emoji and
  combining marks are correct and fast

### 18.3 Measured, per platform

Targets live in `docs/TESTING.md` §8.2 and are verified by benchmarks in the
repository, on **each** operating system, on the lowest hardware the project
claims to support — not on the author's machine only.

A feature that cannot meet its budget on one platform ships **disabled on
that platform**, with the limitation documented. It does not ship by slowing
down the platforms that could have met it.

## 19. Keep the Implementation State Current

The section at the end of this file records what actually exists. **Update it
at the end of every stage**, in the same commit as the work.

A stage is any of: a crate added or substantially changed, a subsystem
becoming usable, a gate being passed or failed, a dependency or tool being
adopted, or an ADR changing status.

The record must be honest about what is *not* done. A status section that
flatters is worse than none, because the next person plans against it.

## 20. Security Is a Release Gate

`AGENTS.md` §17 says what to distrust. This says what to do about it, and it
is a **gate**: no build leaves this project without the checks in §20.5.

### 20.1 Memory safety

- The core is Rust and `unsafe` is denied. The two exceptions are the FFI
  bridge and platform adapters, where it is unavoidable; every block states
  the invariant it relies on, in a comment directly above it.
- The C++ UI layer is the only memory-unsafe surface in the product. It is
  compiled with stack protector, `_FORTIFY_SOURCE`, and the platform's
  control-flow mitigations, and it is exercised under AddressSanitizer and
  UndefinedBehaviorSanitizer in CI.
- No raw buffer arithmetic without a bound. Strings crossing the FFI boundary
  are copied into a caller-sized buffer, truncated at a character boundary,
  and always NUL-terminated.

### 20.2 Recursion is a bound, not a hope

**Any recursion over data that a user or a file can influence must have an
explicit depth limit**, and the check itself must be iterative so that
validating hostile input is not the overflow it exists to prevent.

Known instances, each with a bound:

```text
workspace split tree      MAX_SPLIT_DEPTH   restored from a session file
archive nesting           bounded           docs/SECURITY.md 4
symlink resolution        bounded           docs/SECURITY.md 3
directory recursion       bounded           docs/SECURITY.md 10
JSON / YAML / XML parsing bounded           docs/SECURITY.md 10
```

### 20.3 No injection, ever

- Processes are launched with an **argument vector**. A shell command line is
  never constructed (§16).
- SQL is parameterized. String-built queries are forbidden.
- Paths are never built by concatenating untrusted components, and any write
  derived from untrusted input is verified to resolve inside its destination
  root after normalization.
- User data is never a format string, a glob, a regex or a selector without
  being escaped or bounded first.
- Text from a file, an archive, an AI response or a remote provider is
  **data**. It never becomes a command, a setting, or markup with active
  content.

### 20.4 No hijacking

- Helper binaries are launched by absolute path. No `PATH` lookup for
  anything privileged.
- Library search order is pinned: no writable directory on the load path, and
  no `@rpath` entry an attacker could occupy.
- Third-party shell extensions run out of process (§17,
  `docs/SECURITY.md` §6).
- Destructive operations use directory-relative syscalls rather than
  re-resolving a path between check and use.
- Credentials live in the platform keychain and are never passed on a command
  line, where any local process can read them.

### 20.5 The release gate

Before **any** build is given to anyone outside the project:

- [ ] `cargo audit` and `cargo deny` clean, or every exception justified in
      writing
- [ ] sanitizer build of the UI layer runs the smoke suite with no report
- [ ] fuzz targets run for their scheduled budget with no new crash
- [ ] the hostile fixture set (`docs/TESTING.md` §9.2) passes
- [ ] every new recursion over untrusted input has a bound and a test
- [ ] every new `unsafe` block has a stated invariant
- [ ] every new dependency that parses untrusted input has a justification
      and a fuzz target
- [ ] no secret in the repository, the binary, or the logs
- [ ] the build is signed and, on macOS, notarized
      (`docs/SIGNING_RUNBOOK.md`)
- [ ] verified on a clean machine with the quarantine attribute present

A release that skips a line here is not a release. It is a liability.

## 21. Completion Checklist

Before marking work complete:
- tests pass
- no UI-thread blocking introduced
- i18n strings are not hard-coded
- light/dark themes tested if UI changed
- architecture boundaries preserved
- cancellation considered
- error handling present
- platform impact documented
- benchmark added for performance-sensitive code
- native and display performance budgets still met (§18)
- security obligations met (§20): bounds, no injection, no hijacking
- implementation state updated (§19)
- `git diff` reviewed

---

## Current Implementation State

**Updated:** 2026-08-29 · **Branch:** `poc/qt6` · **Phase:** 0B — GUI PoC

### Gates

```text
tests     209 passing, 0 failing
clippy    clean (-D warnings, workspace-wide)
rustfmt   clean
CI        lint / i18n / security audit / test on macOS, Windows, Linux / rustdoc
```

### Runnable

```bash
./src/ui/qt6/build.sh          # builds and launches the Qt 6 window
cargo run -p jtf-cli           # headless walkthrough of the core
```

The window opens the home folder and does: navigation (double-click, Enter,
Backspace, back/forward, editable path field), native per-file icons,
sortable columns, horizontal/vertical/nested splits, the quad preset, tab bars
per pane, Space to mark, hidden-file toggle, Light/Dark/System following the
system live, runtime `en` ↔ `zh-TW` switching, and session restore.

### Crates

| Crate | What it does |
|---|---|
| `jtf-core` | file model, error codes, i18n catalogue + localizer, theme tokens |
| `jtf-jobs` | job state machine, monotonic progress, cancellation |
| `jtf-workspace` | recursive split tree, per-pane tabs, selection vs marking, session memory |
| `jtf-commands` | command registry, keymap, command bus |
| `jtf-fs` | local provider, cancellable async enumeration in batches |
| `jtf-qt6-bridge` | C ABI over the core; the only `unsafe` in Rust |
| `jtf-conformance` | architecture boundary tests, locale parity and coverage |
| `jtf-cli` | headless walkthrough |
| `src/ui/qt6/cpp` | Qt 6 Widgets front end (C++) |

Also: `locales/{en,zh-TW}` (137 keys), application icon, CI, ADR-0002.

### Not built yet

```text
file operations     copy, move, rename, trash - the job engine has no work yet
platform adapters   no Quick Look, drag and drop, trash, or native menus
viewers / preview   none
search              no query parser, no scanner, no index
AI providers        none
```

Known PoC shortcuts, each tracked in `TODO.md`: dates are formatted by hand
rather than by a locale-aware platform API; the list is not yet benchmarked at
100K rows; there is no drag and drop.

### Decisions outstanding

- **ADR-0001 (GUI stack)** — Qt 6 selected by the project owner and now
  proven to build and run; the ADR stays *Proposed* until the gate
  measurements in `docs/TESTING.md` §8.2 are recorded against it.
- Commercial dual-licensing, which decides whether SignPath Foundation
  signing is available for Windows (`docs/SIGNING_RUNBOOK.md` §B1).

### Next

1. Benchmark the list at 100K and 1M rows; record the numbers in ADR-0001.
2. File operations through the job engine: copy, move, rename, trash.
3. macOS platform adapter: Quick Look, Finder drag and drop, Trash.
