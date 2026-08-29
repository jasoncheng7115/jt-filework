# Development Plan

## Phase 0A — Repository Bootstrap

- initialize local Git
- create `main`
- commit specifications
- add `LICENSE`
- add `.gitignore`
- add `.gitattributes`
- configure Rust formatting/linting
- create base source layout
- configure initial CI skeleton

## Phase 0B — GUI Technology PoC

Candidates:
1. Rust + Qt 6 Widgets
2. Rust + Slint
3. selective WebView hybrid where useful

C# is out of scope.

PoC must include:
- 100K+ virtualized entries
- keyboard navigation
- native selection
- separate mark state
- horizontal split
- vertical split
- nested split
- 2×2 layout
- independent tabs per pane
- tab reorder
- tab drag between panes
- pane → pane file drag
- Finder → app
- app → Finder
- Quick Look panel proof
- embedded native preview proof
- native context menu proof
- stable IME/focus
- English UI
- Taiwan Traditional Chinese UI
- runtime locale switch proof
- Light
- Dark
- Follow System
- runtime theme switch proof
- high DPI
- optional WebView AI panel proof

Decision ADR:
- `docs/adr/0001-gui-stack.md`

## Phase 1 — macOS Functional Skeleton

- command bus
- workspace split tree
- pane/tab models
- local filesystem provider
- async directory model
- keyboard system
- selection + mark
- i18n framework
- en/zh-TW strings
- theme system
- Light/Dark/System
- drag/drop
- basic file operations
- Quick Look
- native context baseline
- Text viewer
- Image viewer
- Job Engine
- persistence

## Phase 2 — macOS Power Features

- archive
- hex
- structured viewers
- strong search
- metadata
- Finder tags
- Spotlight/native search adapter
- saved workspaces
- batch rename
- checksum
- operation history
- polished multi-pane/tab UX

## Phase 3 — AI

- AIProvider abstraction
- natural-language search
- semantic search optional
- AI viewer assistant
- Claude Code CLI
- Codex CLI
- agent job UI
- changed-file detection
- diff/review UX

## Phase 4 — Windows

- Windows platform adapter
- Explorer DnD
- Recycle Bin
- native thumbnails/preview
- Shell context integration
- Nextcloud extension compatibility
- optional ShellHost
- UNC/long-path/reparse support
- Windows theme support

## Phase 5 — Linux

- filesystem integration
- Wayland
- XDG
- GIO/D-Bus
- Open With
- Trash
- thumbnails
- Nautilus/Dolphin adapters
- Linux theme integration
- packaging

## Phase 6 — Plugin SDK

Only after contracts stabilize.
