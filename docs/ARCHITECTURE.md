# JT FileWork — Architecture

## 1. Architectural Goal

Separate:
- cross-platform core
- workspace state
- jobs
- search
- viewers
- AI
- platform-native services
- GUI
- optional WebView-rich panels

Proposed architecture:

```text
UI Layer
  |
Command / Workspace Layer
  |
Core Services
  |-- File Model
  |-- File Operations / Jobs
  |-- Search
  |-- Viewer Registry
  |-- Metadata
  |-- Archive
  |-- i18n contracts
  |-- theme contracts
  |-- AI provider contracts
  |
Platform Adapters
  |-- macOS
  |-- Windows
  |-- Linux
```

## 2. Candidate Technology Stack

Phase 0 compares:
- Rust + Qt 6 Widgets
- Rust + Slint
- optional selective WebView hybrid

C# is explicitly rejected for this project.

Current architectural preference:
- Rust core
- mature native desktop UI
- platform bridges
- selective WebView only where it provides strong value, e.g. AI/Markdown/rich diff

Final GUI stack must not be selected until PoC gates pass.

## 3. Workspace Model

```text
Workspace
├── id
├── root_layout: WorkspaceNode
├── active_pane
├── tool_areas
├── locale
├── theme_mode
└── persisted session state
```

```text
WorkspaceNode =
  Pane(PaneId)
  Split {
    orientation,
    ratio,
    first: WorkspaceNode,
    second: WorkspaceNode
  }
```

## 4. Pane Model

```text
Pane
├── id
├── tabs[]
├── active_tab
├── pane_display_settings
└── focus_state
```

## 5. Tab / FileViewSession

```text
FileViewSession
├── location
├── back_history
├── forward_history
├── selection
├── marked_set
├── sort
├── filter
├── columns
├── view_mode
├── scroll_position
└── provider
```

## 6. Command Architecture

All actions become commands.

Examples:
```text
workspace.split.horizontal
workspace.split.vertical
workspace.pane.next
workspace.pane.close
tab.new
tab.close
tab.move_to_pane
file.view
file.edit
file.copy_to_target_pane
file.move_to_target_pane
file.mark.toggle
preview.quicklook
search.open
ai.ask
theme.set
locale.set
```

## 7. Job Engine

States:
```text
Queued -> Running -> Completed
                  -> Failed
                  -> Cancelled
                  -> WaitingForUser
```

Jobs include:
- copy/move
- recursive size
- hash
- search
- thumbnail
- preview preparation
- archive scan
- indexing
- AI calls
- external agents

## 8. File Abstraction

Do not model an entry as a path string only.

```text
FileEntry
├── URI/location
├── display_name
├── raw_name
├── kind
├── size
├── timestamps
├── attributes
├── permissions summary
├── MIME/content type
├── platform metadata
└── provider data
```

Kinds:
- file
- directory
- symlink
- alias
- application bundle
- package
- archive
- device
- remote item
- virtual search result

## 9. Native Services

Interfaces:
```text
NativePreviewService
NativeThumbnailService
NativeContextMenuService
NativeTrashService
NativeOpenWithService
NativeShareService
NativeMetadataService
NativeDragDropService
NativeSearchService
NativeThemeService
NativeLocaleService
```

## 10. Context Menu

```text
ContextMenuBuilder
├── CoreCommandProvider
├── PlatformCommandProvider
└── PluginCommandProvider
```

Windows third-party shell extension hosting should be evaluated for out-of-process isolation.

## 11. Viewer Dispatcher

Input signals:
- extension
- MIME
- magic bytes
- UTType/platform content type
- size
- local/remote
- provider cost

Providers return capability/score.

## 12. Search

Deterministic:
```text
query -> parser -> index/live scan -> results
```

AI:
```text
natural language
 -> deterministic filter extraction
 -> exact retrieval
 -> optional semantic retrieval
 -> rerank
 -> explanation
```

## 13. AI Provider Model

```text
AIProvider
├── ClaudeCodeCLIProvider
├── CodexCLIProvider
├── LocalModelProvider
└── RemoteAPIProvider
```

All external agents execute through Job Engine.

## 14. i18n Architecture

The core and UI must never assume English strings.

Use localization IDs such as:
```text
menu.file.open
menu.file.rename
workspace.split.horizontal
viewer.hex.title
ai.search.placeholder
```

Locale data:
```text
locales/
  en/
  zh-TW/
```

Exact format depends on selected GUI/localization technology.

Rules:
- English fallback
- no concatenated sentence fragments
- placeholders are typed/validated where possible
- locale may be changed at runtime if toolkit supports it reliably
- core error codes/messages should separate machine code from localized display text

## 15. Theme Architecture

Represent:
```text
ThemeMode =
  System
  Light
  Dark
```

UI components consume semantic theme tokens, not literal colors.

Examples:
```text
surface.background
surface.panel
text.primary
text.secondary
selection.active
mark.active
border.divider
status.warning
```

Native preview/menu components should use platform appearance mechanisms.

## 16. Persistence

SQLite may store:
- workspace
- pane/tab sessions
- history
- saved searches
- metadata cache
- AI job history
- locale preference
- theme preference

Filesystem remains canonical truth.

### 16.1 Session Format

Stored session state carries an explicit format version. A session written by
a newer version is not guessed at: the application starts fresh and reports
why.

Two independent things are stored:

```text
Session
├── version
├── settings          always stored
└── workspace         stored only when the user asked to remember it
```

Session preferences are returned alongside the restored workspace rather than
embedded in it: the workspace is what the user is looking at, the settings are
how the application behaves, and two copies of a preference drift apart.

Reading stored state is parsing untrusted input (`docs/SECURITY.md` §2): it
may be truncated by a crash, edited by hand, or structurally valid but
internally inconsistent. All three cases fall back to a sound default
workspace with a reported reason.

## 17. WebView Policy

Allowed:
- AI conversation
- Markdown
- rich diff
- HTML
- documentation

Do not make core FilePane depend on WebView unless Phase 0 proves native fidelity requirements.
