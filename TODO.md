# JT FileWork — TODO

## P0 — Bootstrap

- [ ] Create project directory `jt-filework`
- [x] `git init`
- [x] Rename default branch to `main`
- [x] Add GPL-3.0-or-later LICENSE
- [x] Add `.gitignore`
- [x] Add `.gitattributes`
- [x] Commit specification baseline
- [x] Install/configure Rust stable
- [x] Configure rustfmt
- [x] Configure clippy
- [x] Create CI skeleton
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
- [x] Define semantic translation key format
- [x] Add `en`
- [x] Add `zh-TW`
- [ ] No hard-coded visible strings
- [x] English fallback
- [ ] Persist language preference
- [ ] Runtime language switch PoC
- [x] CI translation-key parity check
- [x] Verify Taiwan terminology

## P0 — Icon / Branding

- [x] Icon concept tied to the product architecture, not a generic folder
- [x] Master SVG for large sizes
- [x] Separate 32px and 16px artwork
- [x] Reproducible build script (.icns, .ico, PNG set, contact sheet)
- [x] Accent colours sourced from the theme tokens
- [x] Originality check against competitor icons (2026-08-29)
- [ ] Trademark / trade dress search before first public release
- [ ] Monochrome template variant for macOS menu bar / toolbar
- [ ] Legibility check against light and dark Dock and taskbar backgrounds
- [ ] Linux hicolor install paths in packaging
- [ ] Document icon in the app bundle / installer once ADR-0001 lands

## P0 — Theme

- [x] Implement ThemeMode enum/model
- [ ] Follow System
- [ ] Light
- [ ] Dark
- [ ] Persist setting
- [ ] Runtime switch
- [x] Semantic theme tokens
- [ ] Theme-safe icons
- [ ] Active pane visibility in both modes
- [ ] Marked item visibility in both modes

## P0 — Workspace / Multi-Pane

- [x] Split tree
- [x] Horizontal split
- [x] Vertical split
- [x] Nested split
- [x] 2×2 preset
- [x] Pane focus
- [ ] Resize
- [x] Close pane
- [x] Save layout
- [x] Restore layout

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

- [x] Tab model
- [x] New tab
- [x] Close tab
- [x] Reopen closed
- [x] Duplicate
- [x] Pin
- [x] History
- [x] Per-tab sort/filter/view state
- [x] Reorder
- [x] Drag to another pane (model)
- [x] Session restore

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

- [x] Command bus
- [x] Keymap engine
- [ ] CView preset
- [ ] native shortcut compatibility
- [x] selection
- [x] mark
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

## Distribution / Code Signing

Long-lead items: both identities take real time to obtain. See
`docs/DISTRIBUTION.md`.

- [ ] Decide the release channel: source / Homebrew formula / signed .dmg
      (nothing needs buying until a ready-to-run build leaves the project —
      docs/DISTRIBUTION.md 1.2)
- [ ] Join the Apple Developer Program (US$99/yr, only when the above says so)
- [ ] Obtain a Developer ID Application certificate
- [ ] Build with the hardened runtime and a minimal entitlement set
- [ ] codesign all nested binaries, then the bundle
- [ ] notarytool submit + stapler staple, for the .app and the .dmg
- [ ] Verify on a clean machine with the quarantine attribute present
- [ ] Decide: SignPath Foundation (free, OSS) vs own OV/EV certificate
- [ ] **Decide whether a commercial dual-licence is ever wanted** — accepting
      SignPath Foundation signing rules it out (docs/SIGNING_RUNBOOK.md B1)
- [ ] Publish the code signing policy page required by SignPath Foundation
- [ ] Define Author / Reviewer / Approver roles for release signing
- [ ] Windows release pipeline in CI (SignPath signs from CI, not a laptop)
- [ ] If own certificate: solve hardware-token / cloud-HSM signing in CI
- [ ] Evaluate Microsoft's cloud signing service (Phase 4)
- [ ] signtool with RFC 3161 timestamp, for the exe and the installer
- [ ] Linux: Flatpak portal permissions decision
- [ ] Linux: signed artefacts and published checksums
- [ ] Keep signing secrets out of the repository; a fork must still build
- [ ] Honest warning instructions for any pre-signing build shared externally
- [ ] Confirm GPL-3.0-or-later stance on Mac App Store (assumed: not used)

## Quality

- [ ] fuzz path/archive code
- [ ] UI-thread blocking tests
- [ ] memory benchmark
- [ ] slow-NAS simulation
- [ ] crash/session recovery
- [ ] accessibility
- [ ] signing/notarization
