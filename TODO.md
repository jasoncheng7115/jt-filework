# JT FileWork — TODO

## P0 — Bootstrap

- [ ] Create project directory `jt-filework`
- [ ] `git init`
- [ ] Rename default branch to `main`
- [ ] Add GPL-3.0-or-later LICENSE
- [ ] Add `.gitignore`
- [ ] Add `.gitattributes`
- [ ] Commit specification baseline
- [ ] Install/configure Rust stable
- [ ] Configure rustfmt
- [ ] Configure clippy
- [ ] Create CI skeleton
- [ ] Define minimum macOS version

## P0 — GUI Technology

- [ ] Build Qt 6 Widgets PoC
- [ ] Build Slint PoC
- [ ] Test selective WebView embedding
- [ ] Compare identical scenarios
- [ ] Write ADR-0001
- [ ] Decide Rust/UI bridge

## P0 — i18n

- [ ] Choose localization framework compatible with final GUI stack
- [ ] Define semantic translation key format
- [ ] Add `en`
- [ ] Add `zh-TW`
- [ ] No hard-coded visible strings
- [ ] English fallback
- [ ] Persist language preference
- [ ] Runtime language switch PoC
- [ ] CI translation-key parity check
- [ ] Verify Taiwan terminology

## P0 — Icon / Branding

- [x] Icon concept tied to the product architecture, not a generic folder
- [x] Master SVG for large sizes
- [x] Separate 32px and 16px artwork
- [x] Reproducible build script (.icns, .ico, PNG set, contact sheet)
- [x] Accent colours sourced from the theme tokens
- [ ] Monochrome template variant for macOS menu bar / toolbar
- [ ] Legibility check against light and dark Dock and taskbar backgrounds
- [ ] Linux hicolor install paths in packaging
- [ ] Document icon in the app bundle / installer once ADR-0001 lands

## P0 — Theme

- [ ] Implement ThemeMode enum/model
- [ ] Follow System
- [ ] Light
- [ ] Dark
- [ ] Persist setting
- [ ] Runtime switch
- [ ] Semantic theme tokens
- [ ] Theme-safe icons
- [ ] Active pane visibility in both modes
- [ ] Marked item visibility in both modes

## P0 — Workspace / Multi-Pane

- [ ] Split tree
- [ ] Horizontal split
- [ ] Vertical split
- [ ] Nested split
- [ ] 2×2 preset
- [ ] Pane focus
- [ ] Resize
- [ ] Close pane
- [ ] Save layout
- [ ] Restore layout

## P0 — Session Memory

- [x] Session format with an explicit version
- [x] Capture/restore the whole workspace
- [x] Startup preference: last session / home / fixed location
- [x] Preference persists even when the workspace is not
- [x] Turning memory off writes no workspace at all
- [x] Optional: remember closed tabs
- [x] Optional: remember marked set
- [x] Corrupt session falls back with a reported reason
- [x] Future-version session is not guessed at
- [x] Unavailable locations restore the rest and report the gap
- [ ] Atomic write of the session file (platform layer)
- [ ] Write on quit, on layout change, and periodically while idle
- [ ] Settings UI for the startup preference
- [ ] Erase stored session when memory is switched off (with confirmation)
- [ ] Multi-window arrangement restore

## P0 — Tabs Per Pane

- [ ] Tab model
- [ ] New tab
- [ ] Close tab
- [ ] Reopen closed
- [ ] Duplicate
- [ ] Pin
- [ ] History
- [ ] Per-tab sort/filter/view state
- [ ] Reorder
- [ ] Drag to another pane
- [ ] Session restore

## P0 — File View

- [ ] Async directory enumeration
- [ ] Incremental rows
- [ ] columns
- [ ] sort
- [ ] filter
- [ ] hidden files
- [ ] symlink/alias/package
- [ ] 100K benchmark
- [ ] 1M synthetic benchmark

## P0 — Input

- [ ] Command bus
- [ ] Keymap engine
- [ ] CView preset
- [ ] native shortcut compatibility
- [ ] selection
- [ ] mark
- [ ] pane navigation
- [ ] tab navigation

## P0 — Drag & Drop

- [ ] pane -> pane
- [ ] Finder -> app
- [ ] app -> Finder
- [ ] modifiers
- [ ] multi-file
- [ ] tab -> pane

## P0 — macOS Native

- [ ] Quick Look panel
- [ ] Embedded Quick Look
- [ ] thumbnails
- [ ] Open With
- [ ] Reveal in Finder
- [ ] Trash
- [ ] Finder tags
- [ ] aliases
- [ ] app bundles
- [ ] packages
- [ ] Share/Services research
- [ ] context menu research
- [ ] system appearance integration

## P1 — Viewer

- [ ] Preview dispatcher
- [ ] cancellation
- [ ] Text
- [ ] Big5
- [ ] huge text
- [ ] Image
- [ ] Hex
- [ ] Archive
- [ ] JSON
- [ ] YAML
- [ ] XML
- [ ] CSV

## P1 — Operations

- [ ] Job Engine
- [ ] Copy
- [ ] Move
- [ ] Rename
- [ ] Duplicate
- [ ] Trash
- [ ] Delete
- [ ] Mkdir
- [ ] Batch rename
- [ ] Conflict resolver
- [ ] Cancel
- [ ] Progress
- [ ] Retry
- [ ] Operation log

## P1 — Search

- [ ] query syntax
- [ ] filename
- [ ] glob
- [ ] regex
- [ ] metadata
- [ ] date/size
- [ ] content search
- [ ] optional index
- [ ] saved searches
- [ ] virtual result folder
- [ ] pane/workspace/root scopes

## P2 — AI

- [ ] AIProvider
- [ ] natural-language search
- [ ] deterministic filter extraction
- [ ] semantic index design
- [ ] local embedding
- [ ] summaries
- [ ] comparisons
- [ ] explain logs/config/source
- [ ] Claude Code CLI
- [ ] Codex CLI
- [ ] streaming
- [ ] cancellation
- [ ] changed-file detection
- [ ] diff
- [ ] operation plan approval

## P2 — Context / Shell Ecosystem

- [ ] ContextMenuBuilder
- [ ] app actions
- [ ] type actions
- [ ] target-pane actions
- [ ] AI actions
- [ ] native actions
- [ ] plugin placeholder

## P3 — Windows

- [ ] Win32/COM adapter
- [ ] Explorer DnD
- [ ] Recycle Bin
- [ ] Open With
- [ ] native preview
- [ ] thumbnails
- [ ] IContextMenu
- [ ] IExplorerCommand research
- [ ] Nextcloud compatibility
- [ ] OneDrive
- [ ] UNC
- [ ] long paths
- [ ] reparse points
- [ ] ShellHost isolation
- [ ] light/dark/system

## P4 — Linux

- [ ] Wayland
- [ ] XDG Trash
- [ ] MIME/Open With
- [ ] D-Bus
- [ ] GIO
- [ ] thumbnails
- [ ] xattrs
- [ ] ACL
- [ ] Nautilus
- [ ] Dolphin
- [ ] light/dark/system integration

## Quality

- [ ] fuzz path/archive code
- [ ] UI-thread blocking tests
- [ ] memory benchmark
- [ ] slow-NAS simulation
- [ ] crash/session recovery
- [ ] accessibility
- [ ] signing/notarization
