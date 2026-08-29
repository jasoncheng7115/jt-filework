# jt-filework — Viewer and Preview Specification

Preview and Viewer are different subsystems with different contracts
(`AGENTS.md` §14).

| | Preview | Viewer |
|---|---|---|
| Purpose | glance at the selected item | work with the content |
| Lifetime | disposable, tied to selection | stateful, explicit open/close |
| Cost | strictly bounded | may load more, still bounded |
| Cancellation | mandatory, on every selection change | user-driven |
| Editing | never | out of scope for Phase 1–2 |
| Trigger | selection, Space (Quick Look) | Enter / view command |

---

## 1. Dispatch

Both subsystems resolve a provider through the Viewer Registry
(`ARCHITECTURE.md` §11).

Input signals: extension, MIME type, magic bytes, platform content type
(UTType / Windows content type), size, local vs remote, provider cost.

```text
candidates = registry.query(signals)
score      = capability × confidence − cost
winner     = highest score, ties broken by explicit user preference
```

Rules:
- extension alone never decides; magic bytes override a lying extension
- a provider declares its cost so remote/huge files can pick a cheaper path
- the user can pin a preferred provider per type
- an unresolved type falls back to Hex (always available), never to an error

---

## 2. Preview Contract

Every preview request:
- runs as a Job (`AGENTS.md` §13), never on the UI thread
- receives a cancellation token honoured at every I/O boundary
- declares limits: max bytes read, max wall time, max memory
- returns one of: `Ready(content)`, `TooLarge`, `Unsupported`,
  `Failed(code)`, `Cancelled`
- is discarded when the selection changes; a late result is dropped, never
  rendered (stale result rejection, `AGENTS.md` §3)

Oversized content is not an error. It renders a bounded state with an
"open in viewer" action.

---

## 3. Native vs Internal

Hybrid strategy (`PRODUCT_SPEC.md` §11): native where the OS is stronger,
internal where jt-filework can be stronger.

**Native preferred**: Office and iWork documents, PDF, media playback,
platform-specific formats, thumbnails.

**Internal preferred**: text, source code, logs, hex, archives, structured
data — anywhere the workflow (huge files, encoding control, search, tailing,
tree navigation) matters more than pixel fidelity.

macOS: Space opens the native Quick Look panel; the tool area embeds a native
preview view where appropriate.

---

## 4. Internal Viewers

### 4.1 Text / Code / Log (Phase 1)
- memory-mapped or chunked reading; a 10 GB log opens without loading it
- encoding detection with an explicit override, including **Big5**, GB18030,
  Shift-JIS, UTF-8/16/32 with and without BOM
- line ending detection (LF/CRLF/CR/mixed) shown, not silently normalized
- very long lines handled without quadratic behaviour
- search within the file, incremental and cancellable
- go-to-line, wrap toggle, whitespace and control-character visibility
- optional syntax highlighting (Phase 2), never blocking first paint
- **log mode**: follow/tail, highlight rules, level filtering

### 4.2 Image (Phase 1)
- common raster formats, animated formats, platform-supported RAW where cheap
- zoom, fit, 1:1, pan, rotate; EXIF orientation respected
- decoding is bounded: pixel-count and memory ceilings before decode
- EXIF/metadata panel

### 4.3 Hex (Phase 1, universal fallback)
- windowed view over arbitrary file sizes
- offset navigation, byte/word grouping, ASCII and selected-encoding gutter
- find bytes and find text-in-encoding
- data inspector for the integer/float/timestamp interpretations at the cursor

### 4.4 Archive (Phase 2)
- list members **without extracting** (`SECURITY.md` §4)
- tree and flat views; preview a member by streaming it
- extract selected members as a Job with conflict resolution
- all archive limits enforced: ratio, total size, member count, depth
- encrypted archives prompt for a passphrase; the passphrase is never logged

### 4.5 Structured: JSON / YAML / XML / CSV (Phase 2)
- tree navigation with collapse/expand, path display and copy-path
- large-document strategy: streaming parse, virtualized tree, no full DOM for
  multi-GB inputs
- CSV: delimiter and encoding detection with override, column typing,
  virtualized grid, malformed-row reporting instead of silent truncation
- every parser is fuzzed (`TESTING.md` §9.1)

### 4.6 Diff (Phase 2/3)
Compare two selected files or a file across two panes. Text diff internally;
rich rendering may use the WebView panel (`ARCHITECTURE.md` §17).

---

## 4.7 Editing — what `E` / F4 does

`AGENTS.md` §14 separates Preview from Viewer. Editing is a third thing, and
it needs its own answer because the CView and WinCV muscle memory this product
targets has `F4` (and `E`) meaning *edit this file now*.

### The three verbs

| Key | Verb | What it opens |
|---|---|---|
| Space | **Preview** | disposable, cancelled by the next selection, read-only |
| Enter / F3 | **View** | internal viewer: stateful, huge files, encodings, search, read-only |
| F4 / E | **Edit** | something that can write the file back |

### The decision: delegate first, own it later

**Phase 1 and 2: hand the file to the editor the user already chose.** In
order:

1. an editor configured in settings for that file type
2. the editor configured in settings generally
3. `$VISUAL`, then `$EDITOR`, if it names a GUI editor
4. the platform's default application for the type

This is not laziness. A file manager that ships a mediocre editor and makes it
the default has taken something away from a user who already has a good one.
Delegation is also the only honest answer while no internal editor exists —
better than binding `E` to nothing, and far better than binding it to the
read-only viewer and letting the user discover their typing did nothing.

**Phase 3: an internal editor for the cases where the round trip is the
friction** — a one-line change to a config file, a quick note. It reuses the
text viewer's encoding and line-ending handling, so a file opened as Big5 with
CRLF is written back as Big5 with CRLF. Anything larger stays delegated.

### Rules

- **Never guess that a file is text.** Check magic bytes, not the extension.
  Refusing to open a binary with a clear message is right; letting an editor
  mangle it is not.
- **Check writability first.** Opening a read-only file for editing and
  failing at save is a worse experience than being told at the start.
- **Warn before opening something huge** in an external editor that will load
  it whole.
- **Launch by argument vector, never through a shell**, with the editor's
  absolute path, and the file path as its own argument (`AGENTS.md` §16,
  §20.3, §20.4). A filename containing a space, a quote or a `$` is a normal
  filename, not an incident.
- **Never pass the file through a shell command template** the user typed. If
  configuration ever accepts an editor command line, it is parsed into an
  argument vector and validated, not handed to `sh -c`.
- **The external editor is not trusted.** It runs as an ordinary child
  process; the application does not wait on it, and its exit status is
  reported, not acted on.
- **Watch the file and refresh.** After an external edit, the pane's row for
  that file updates its size and timestamp without the user reloading.
- **Preserve the inode where the platform allows it**, so hard links and
  extended attributes survive an internal-editor save.

### Why `E` as well as `F4`

`F4` is the Norton lineage; `E` is what CView users actually press, because it
is one key and needs no `Fn` on a Mac. Both are bound in
`keymaps/cview.keymap`, which is data — a user who wants `E` to mean something
else changes a line (`docs/UI_UX_SPEC.md` §7).

## 5. Resource Limits

| Limit | Applies to |
|---|---|
| max bytes read | preview |
| max wall time | preview |
| max decoded pixels | image |
| max memory per request | all |
| max archive members listed at once | archive |
| cache size and eviction policy | thumbnails, previews |

Hitting a limit is a normal reported outcome (`SECURITY.md` §10).

---

## 6. Security

Preview runs on untrusted content (`SECURITY.md` §5): no script execution, no
macro execution, no remote resource loading, no embedded object activation.
External and OS preview helpers run out-of-process where the platform allows,
and a helper crash must not take down the application.

---

## 7. Remote Files

- preview of a remote file is opt-in when it would trigger a large download
- cloud placeholder files are never hydrated implicitly
- a stalled remote read cancels cleanly and reports a distinct state

---

## 8. Testing

Covered by `TESTING.md`: dispatch unit tests, cancellation and stale-result
integration tests, `contract::viewer_provider` conformance, fuzz targets for
every parser, the hostile fixture set, and the UI-thread watchdog scenarios
for huge files and huge archives.
