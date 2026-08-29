# JT FileWork — Product Specification

## 1. Definition

JT FileWork is a cross-platform desktop file workspace for power users.

It combines:
- CView/WinCV-style keyboard workflows
- full mouse operation
- Finder/Explorer-compatible desktop behavior
- Q-Dir/QSpace-style split workspaces
- independent tabs per pane
- strong preview/viewer capabilities
- deterministic and AI-assisted search
- AI/CLI agent workflows

Primary audiences:
- developers
- system administrators
- security engineers
- content-heavy knowledge workers
- heavy Finder/Explorer/QSpace/Q-Dir/Total Commander users
- former CView/WinCV users

## 2. Platform Strategy

First release target:
- macOS

Architectural targets from day one:
- macOS
- Windows
- Linux

Primary development host:
- macOS Apple Silicon

## 3. Workspace

The application opens a Workspace.

Workspace may contain:
- one pane
- horizontal split
- vertical split
- nested split
- 2×2 layout
- arbitrary reasonable split trees
- tabs inside every pane
- preview/tool areas

Example:

```text
┌──────────────────────────────┬──────────────────────────────┐
│ Pane A                       │ Pane B                       │
│ [Home][Downloads][NAS]       │ [Project][USB]              │
│                              │                              │
│ files...                     │ files...                     │
├──────────────────────────────┴──────────────────────────────┤
│ Preview / Search / AI / Tool Area                          │
└─────────────────────────────────────────────────────────────┘
```

## 4. Split Pane Requirements

- split horizontally
- split vertically
- nested split
- 2×2 preset
- 3-pane presets
- close pane
- resize pane
- focus next/previous pane
- swap/move pane if architecture allows cleanly
- save workspace
- restore workspace
- drag files between panes

Never hard-code a dual-pane architecture.

## 5. Tabs Per Pane

Each Pane owns independent Tabs.

Required:
- new
- close
- reopen closed
- duplicate
- pin
- reorder
- drag tab to another pane
- per-tab navigation history
- per-tab location
- per-tab sorting/filtering
- per-tab view settings
- per-tab scroll/selection restoration
- session persistence

Future:
- detach tab to new window
- tab groups
- workspace templates

## 5.1 Session Memory

Closing the application and reopening it returns the user to exactly where
they were: the split layout, every pane, every tab, each tab's location and
history, sort, filter, columns, view mode, scroll position, selection, marked
set, locale and theme.

This is a **preference, not a law.** Some users want a clean window every
launch, and on a shared or audited machine remembering the last paths is a
privacy question rather than a convenience.

Startup behaviour options:

- **Restore last session** (default)
- **Start at home**
- **Start at a fixed location** the user chooses

Finer-grained switches:

- remember the reopen-closed-tab history
- remember the marked set

Rules:

- The preference itself is always persisted. Turning memory off must still be
  remembered next launch.
- Turning memory off **discards** what was already stored. A switch that
  leaves yesterday's paths on disk is not an off switch.
- Nothing that is not remembered is written to disk in the first place.
- A missing session is a normal first launch and is not reported as a problem.
- A corrupt or future-version session starts fresh **and says so**. It never
  silently discards the user's layout.
- A restored session naming a volume that is no longer mounted restores
  everything else and reports the gap.
- Session state is written atomically: a crash mid-write must leave the
  previous session loadable.

## 6. File Browsing

- list/detail view
- optional icon/grid view
- configurable columns
- sorting
- filtering
- hidden/system files
- symlinks
- aliases
- reparse points
- app bundles/packages
- local volumes
- network mounts
- SMB/NFS/UNC
- very large directories

## 7. Input Model

### Keyboard
- full core operation without mouse
- configurable keymaps
- CView/WinCV-style preset
- platform-native shortcut compatibility
- command palette
- pane focus shortcuts
- tab shortcuts
- mark/unmark shortcuts
- F-key operations where appropriate

### Mouse
- native click selection
- Cmd/Ctrl multiselect
- Shift range
- drag rectangle where appropriate
- context menu
- drag/drop
- splitter resize
- tab drag

## 8. Selection vs Marking

Selection = current native selection.

Marking = persistent CView-style batch set.

Operations can explicitly target:
- selection
- marked set
- active file

## 9. Native Drag and Drop

Required:
- pane → pane
- Finder → app
- app → Finder
- Explorer → app
- app → Explorer
- Linux file manager ↔ app
- compatible external apps

Respect platform modifier semantics for copy/move/link.

## 10. Context Menu

Merge:
1. JT FileWork commands
2. file-type commands
3. target-pane commands
4. AI commands
5. platform-native actions
6. shell/ecosystem actions
7. future plugin actions

Windows goal:
- maximize compatibility with installed Explorer shell integrations such as Nextcloud/OneDrive/7-Zip/Git tools

macOS:
- Open With
- Share/Services/Quick Actions where public APIs allow
- native Finder-like actions
- document limitations for Finder-only extensions

Linux:
- XDG/Open With
- desktop-specific adapters where practical

## 11. Preview and Viewer

Hybrid strategy:
- native preview when OS is stronger
- internal viewer where JT FileWork can provide a richer workflow

macOS:
- Space => native Quick Look
- embedded Quick Look for Office/iWork/PDF/media where appropriate
- internal Text/Code/Log/Hex/Archive/Structured viewers

## 12. Search

Traditional search:
- filename
- wildcard
- regex
- path
- extension/type
- MIME
- size
- timestamps
- tags
- metadata
- permissions
- content
- optional index
- saved searches

Search results behave as virtual folders.

## 13. AI

AI capabilities:
- natural-language search
- semantic search
- query translation
- reranking/explanation
- summarize selected files
- compare files
- explain logs/config/code
- directory Q&A
- classify/organize
- batch operation planning

External agent providers:
- Claude Code CLI
- Codex CLI
- future local model
- future remote API providers

## 14. Internationalization

First release languages:
- English
- Taiwan Traditional Chinese

Requirements:
- all user-facing text localized
- no hard-coded UI strings
- runtime language switching if practical
- locale preference persisted
- fallback to English
- robust handling of Unicode filenames
- date/number formatting uses locale-aware presentation

## 15. Themes

Required:
- Light
- Dark
- Follow System

Requirements:
- persisted preference
- system appearance change handling
- no hard-coded colors
- theme-safe icons
- preview/native panels retain platform correctness

## 16. Git / Development Governance

From project creation:
- initialize local Git repository
- commit baseline docs/spec
- maintain clean history
- use branches for PoCs/features
- AI-generated changes must be reviewable by `git diff`

## 17. Early Non-Goals

- replacing Finder/Explorer as the OS shell
- office document editing
- full cloud storage service implementation
- arbitrary in-process native plugin execution
