# JT FileWork

> A modern keyboard-first, mouse-complete, native-integrated file workspace for macOS, Windows and Linux.

JT FileWork is inspired by the workflow philosophy of CView / WinCV while incorporating modern native desktop behavior, Q-Dir/QSpace-style multi-pane workspaces, strong search, rich preview, and AI/agent integration.

## Project Status

**Phase 0 — Architecture and Technology Validation**

The first shipping platform is **macOS on Apple Silicon**, but architecture must support Windows and Linux from day one.

## Product Vision

JT FileWork is not merely a file manager, file viewer, or Finder replacement.

It is a **local file productivity workspace** designed around:

- CView / WinCV keyboard efficiency
- Finder / Windows Explorer native behavior
- Q-Dir / QSpace-style arbitrary pane splitting
- independent tabs inside every pane
- fast deterministic search
- AI-assisted semantic search
- hybrid native/internal previews
- native shell ecosystem integration
- external AI agents such as Claude Code and Codex CLI
- strong local-first behavior

## Non-Negotiable Principles

1. Keyboard-first.
2. Mouse-complete.
3. Platform-native semantics.
4. Workspace-oriented.
5. Cross-platform core.
6. Shell ecosystem compatible.
7. Native-first where the OS is stronger; internal-first where JT FileWork can be stronger.
8. AI is additive, never a replacement for deterministic behavior.
9. UI thread never blocks on filesystem, network, preview, indexing, or AI work.
10. GUI framework must remain replaceable.
11. Workspace layout is a recursive split tree, not fixed left/right panes.
12. Every pane owns independent tabs.
13. Selection and CView-style marking are separate concepts.
14. i18n exists from the first UI string.
15. First supported languages are English and Taiwan Traditional Chinese.
16. Light, Dark, and Follow System themes exist from Phase 0.
17. Git version control is initialized before implementation begins.
18. macOS Apple Silicon is the primary development environment.
19. Windows and Linux must be continuously considered in architecture and later validated in CI.
20. No hard-coded user-visible strings.
21. No platform-specific implementation leaking into cross-platform core modules.

## Initial Language Support

- English (`en`)
- 台灣繁體中文 (`zh-TW`)

All user-visible strings must use localization keys.

## Theme Support

- Light
- Dark
- Follow System

Theme support must be validated in the GUI technology PoC.

## Development Environment

Primary:
- macOS
- Apple Silicon
- local Git repository from project creation
- Rust toolchain
- candidate GUI stacks evaluated during Phase 0

Cross-platform validation:
- macOS CI
- Windows CI
- Linux CI

## License Direction

Main application:
- `GPL-3.0-or-later`

Possible future plugin SDK / protocol:
- `Apache-2.0`

## Documentation

- `docs/PRODUCT_SPEC.md`
- `docs/ARCHITECTURE.md`
- `docs/UI_UX_SPEC.md`
- `docs/I18N_THEME.md`
- `docs/PLATFORM_INTEGRATION.md`
- `docs/VIEWER_PREVIEW.md`
- `docs/SEARCH_AI.md`
- `docs/SECURITY.md`
- `docs/TESTING.md`
- `docs/UI_TEST_PLAN.md`
- `docs/DISTRIBUTION.md`
- `docs/SIGNING_RUNBOOK.md`
- `docs/DEVELOPMENT_ENVIRONMENT.md`
- `docs/DEVELOPMENT_PLAN.md`
- `TODO.md`
- `AGENTS.md`
- `assets/icon/README.md`
- `docs/adr/*`
