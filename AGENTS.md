# AGENTS.md — JT FileWork

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

## 20. Completion Checklist

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
- implementation state updated (§19)
- `git diff` reviewed

---

## Current Implementation State

**Updated:** 2026-08-29 · **Branch:** `poc/qt6` · **Phase:** 0B — GUI PoC

### Gates

```text
tests     204 passing, 0 failing
clippy    clean (-D warnings, workspace-wide)
rustfmt   clean
CI        lint / i18n / test on macOS, Windows, Linux / rustdoc
```

### Working, and exercised by `cargo run -p jtf-cli`

| Crate | What it does |
|---|---|
| `jtf-core` | file model, machine-readable error codes, i18n catalogue + localizer, theme tokens |
| `jtf-jobs` | job state machine, monotonic progress, cancellation |
| `jtf-workspace` | recursive split tree, per-pane tabs, selection vs marking, session memory |
| `jtf-commands` | command registry, keymap, command bus |
| `jtf-fs` | local provider, cancellable async enumeration in batches |
| `jtf-conformance` | architecture boundary tests, locale parity and coverage |
| `jtf-cli` | headless walkthrough of all of the above |

Also: `locales/{en,zh-TW}` (112 keys), application icon, CI, ADR-0002.

### Not built yet

```text
UI                  Qt 6 PoC in progress on this branch
platform adapters   no Quick Look, drag and drop, trash, or native menus
file operations     copy, move, rename, trash - the job engine has no work yet
viewers / preview   none
search              no query parser, no scanner, no index
AI providers        none
```

### Decisions outstanding

- **ADR-0001 (GUI stack)** — Qt 6 selected by the project owner; the PoC on
  `poc/qt6` must still record the gate results and measurements before the
  ADR moves to Accepted.
- Commercial dual-licensing, which decides whether SignPath Foundation
  signing is available for Windows (`docs/SIGNING_RUNBOOK.md` §B1).

### Next

1. Qt 6 Widgets shell: window, recursive splitter, per-pane tab bar,
   virtualized file list bound to `jtf-fs`.
2. Measure against `docs/TESTING.md` §8.2 and record in ADR-0001.
3. Runtime locale and theme switching in the real UI.
