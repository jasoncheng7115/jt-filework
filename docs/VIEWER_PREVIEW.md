# JT FileWork — Viewer and Preview Specification

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
internal where JT FileWork can be stronger.

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
