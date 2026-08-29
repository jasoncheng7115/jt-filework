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

## 18. Completion Checklist

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
- `git diff` reviewed
