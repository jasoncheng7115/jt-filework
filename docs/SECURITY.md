# jt-filework — Security Specification

jt-filework opens files it did not create, parses formats it does not control,
and executes helpers it does not own. The security model follows from
`AGENTS.md` §16 and §17.

---

## 1. Threat Model

### 1.1 Assets

- the user's filesystem (read and write)
- the user's credentials for remote mounts and AI providers
- the integrity of file operations (no silent data loss)
- the user's trust that an action does what its label says

### 1.2 Adversaries

- a malicious or malformed **file** the user opens or previews
- a malicious **archive** the user inspects or extracts
- a malicious **filename** or path
- a hostile or buggy **shell extension** / plugin
- a compromised or prompt-injected **AI response**
- a hostile **remote filesystem** (SMB/NFS/UNC/cloud mount)
- a local process racing us on the same paths (TOCTOU)

### 1.3 Explicit non-goals

- defending against a fully compromised local account
- sandboxing the user against their own deliberate destructive action
- being an anti-malware product

---

## 2. Trust Boundaries

Everything in this list is **untrusted input** (`AGENTS.md` §17):

```text
archives
document parsers
external preview helpers
shell extensions
plugins
AI agents and AI responses
remote filesystems
file names and file contents
session state files written by an older version
locale catalogues loaded from disk
```

Each boundary must have: an owner, a parsing strategy, a resource limit, a
failure mode, and a fuzz target (see `TESTING.md` §9).

---

## 3. Path Handling

Path bugs are the most likely source of real damage.

Rules:
- Paths are opaque OS strings. Never round-trip through lossy UTF-8.
- Never build a path by string concatenation of untrusted components.
- Any write derived from untrusted input (extraction, batch rename, AI plan)
  must be verified to resolve **inside** the destination root after
  normalization and symlink resolution.
- Reject or explicitly handle: `..` components, absolute components,
  drive-relative Windows paths, UNC, alternate data streams, reserved device
  names, trailing dots/spaces, NUL bytes.
- Case-insensitive filesystems: a rename that only changes case must not be
  treated as a self-overwrite.
- Long paths: Windows long-path support is required, not optional.

### 3.1 TOCTOU

Destructive operations must not re-resolve a path between check and use.
Prefer directory-handle-relative operations (`openat`/`unlinkat` family and
their Windows equivalents) for recursive delete and recursive traversal.

Recursive delete must never follow a symlink out of the tree being deleted.

---

## 4. Archive Handling

Archives are hostile by default.

Required defences:
- path traversal check on every member (§3)
- absolute member paths rejected
- symlink and hardlink members are not created unless the user explicitly
  opts in; never during preview
- compression ratio limit (zip bomb) with a configurable ceiling
- total uncompressed size limit and member count limit
- nested archive depth limit
- per-archive memory ceiling; streaming rather than full extraction for
  listing and preview
- listing an archive never writes to disk outside a controlled temporary area
- extraction is a Job with progress, cancellation and a conflict resolver

---

## 5. Preview and Viewer Isolation

Preview runs on untrusted content and is therefore the most exposed surface.

- Internal parsers (text, hex, JSON, YAML, XML, CSV) are memory-safe Rust and
  fuzzed.
- Third-party parsing libraries are pinned, audited and updated deliberately.
- External preview helpers and OS preview services run out-of-process where
  the platform allows. A helper crash must not take down the application.
- Preview has hard limits: maximum bytes read, maximum time, maximum memory.
  Exceeding a limit cancels the preview and shows a bounded message.
- Preview never executes content: no scripts, no macros, no embedded objects,
  no remote resource loading.
- WebView panels (`AGENTS.md` §17, ARCHITECTURE §17) must have scripting
  disabled unless the panel's own content requires it, must block all remote
  loads by default, and must never render untrusted file content as HTML.

---

## 6. Shell Extension and Plugin Isolation

- Windows third-party `IContextMenu` handlers load into a **separate host
  process**. A hung or crashing handler is killed without affecting the main
  process.
- No arbitrary in-process native plugin execution (`PRODUCT_SPEC.md` §17 early
  non-goal).
- A future plugin SDK defines a capability-scoped, out-of-process protocol
  before any plugin is loaded.

---

## 7. External AI Agent Execution

This is the highest-risk feature in the product. `AGENTS.md` §16 is mandatory.

### 7.1 Process launch

- Always spawn with an **argument vector**. Never construct a shell command
  line. Never pass user or filesystem input through a shell.
- The working directory is explicit and required. There is no "inherit
  whatever the process happened to have".
- The environment passed to the child is an explicit allowlist. Provider
  credentials are passed only to the provider that needs them.
- Output is streamed and bounded; a runaway agent cannot exhaust memory.
- Cancellation terminates the child **and its process group**.

### 7.2 Scope

- The agent's working directory defines its scope. The scope is shown to the
  user before the run starts.
- Changed-file detection runs against that scope and produces a diff.
- Changes are presented for review. Nothing is committed or applied silently.

### 7.3 Prompt injection

AI responses are untrusted. A file's contents can contain instructions.

- An AI response can never directly trigger a filesystem mutation. It can only
  produce a **proposed plan**, which requires explicit user approval
  (`TESTING.md` §12).
- An AI response can never change application settings, keybindings, providers
  or trust decisions.
- An AI response can never cause a process to be launched.
- Rendered AI output is treated as text, not as markup with active content.

### 7.4 Data exfiltration

- The user must be told, before the first run of a provider, what leaves the
  machine.
- Local-first is the default. Remote providers are opt-in per provider.
- File contents are sent only for the explicit scope of the request.
- A visible indicator shows when a remote provider is in use.

---

## 8. Credentials and Secrets

- API keys and mount credentials live in the platform keychain/credential
  store, never in the SQLite database, never in the session state, never in
  logs.
- Logs and crash reports are scrubbed of paths under the user's home unless
  the user opts in to include them.
- No telemetry by default. Any future telemetry is opt-in and documents
  exactly what it sends.
- The repository must contain no secrets (`AGENTS.md` §2,
  `DEVELOPMENT_ENVIRONMENT.md`).

---

## 9. Destructive Operation Safety

- Delete and overwrite require an explicit confirmation unless the user has
  disabled it deliberately.
- Trash is preferred over permanent delete; permanent delete is a distinct,
  clearly labelled command.
- Every destructive Job writes an operation log entry before it acts.
- Undo is offered where it is safe and honest. Where undo is impossible, the
  UI says so **before** the action, not after.
- A batch operation reports precisely which entries succeeded and which
  failed. "Partially completed" is a first-class outcome, never hidden.

---

## 10. Resource Limits

Every untrusted-input path declares a limit:

| Surface | Limit |
|---|---|
| Preview bytes read | bounded, configurable |
| Preview wall time | bounded, cancellable |
| Archive uncompressed total | bounded |
| Archive member count | bounded |
| Archive nesting depth | bounded |
| Symlink resolution depth | bounded |
| Directory recursion depth | bounded |
| AI response size | bounded |
| Thumbnail/preview cache | bounded with eviction |
| Search index size | bounded, user-visible |

Exceeding a limit is a normal, reported outcome — not a crash and not a hang.

---

## 11. Supply Chain

- Dependencies are pinned via a committed lockfile.
- `cargo audit` (or equivalent) runs in CI and fails on known advisories.
- `cargo deny` enforces the license policy: main application is
  `GPL-3.0-or-later` compatible.
- New dependencies that parse untrusted input require explicit justification
  in the pull request and a fuzz target.
- `unsafe` is denied by default in core crates and allowed only in platform
  adapters with a written safety comment per block.

---

## 13. Memory Safety and Recursion

`AGENTS.md` §20.1 and §20.2 are the rules; this is what they mean in practice.

### 13.1 The unsafe budget

| Layer | `unsafe` | Why |
|---|---|---|
| core, jobs, workspace, commands, fs | denied | there is no reason for it |
| FFI bridge | allowed, per-block justification | a C ABI cannot exist without raw pointers |
| platform adapters | allowed, per-block justification | the platform's own API is unsafe |
| C++ UI layer | the whole layer is unsafe | Qt Widgets is a C++ API |

The C++ layer is therefore the product's only broad memory-unsafety surface,
and it gets the treatment: hardening flags on every build, sanitizers in CI,
and no buffer arithmetic without a bound.

### 13.2 Strings across the FFI boundary

Text is copied into a caller-provided buffer. The bridge:

- returns the length the text *needed*, so truncation is detectable
- never writes past `len - 1`, and always NUL-terminates
- truncates at a UTF-8 character boundary, never mid-character
- allocates nothing that crosses the boundary, so there is no free function
  to forget and no per-row allocation while scrolling

### 13.3 Recursion over untrusted data

Every recursive walk over data a file can influence needs a **bound**, and
the bound check must itself be iterative.

The worked example is the workspace split tree. It is restored from a session
file, which is untrusted input, and it is walked recursively in three places:
serde while deserializing, the model while measuring and rendering, and the
UI while building widgets. Without a limit, a hand-edited session file is a
stack overflow before any code gets the chance to validate it.

The fix has three layers, which is what defence in depth actually looks like:

1. `serde_json`'s own recursion limit stops the deserializer.
2. `MAX_SPLIT_DEPTH` bounds the tree, checked **iteratively** in
   `WorkspaceNode::depth_within_limit`, and folded into
   `Workspace::invariants_hold` so `Session::restore` rejects a hostile file
   through the check it already performs.
3. The UI layer bounds its own recursion, on the assumption that the layer
   below it might one day be wrong.

Apply the same pattern to archive nesting, symlink chains, directory
recursion and every structured-document parser.

### 13.4 Indices and lengths at the boundary

Every index arriving from C++ is treated as hostile: a negative or oversized
index produces a zero or a no-op, never an out-of-range access. Every length
is converted with a checked conversion, never a cast.

## 14. Release Security Gate

The checklist is `AGENTS.md` §20.5, and it is a gate rather than a
suggestion: no build reaches anyone outside the project until every line
passes.

Automated, in CI:

```text
cargo audit          known advisories in the dependency tree
cargo deny           licence policy and duplicate/banned crates
sanitizer build      ASan + UBSan over the UI smoke suite
fuzz                 scheduled budget, corpus persisted, no new crash
hostile fixtures     docs/TESTING.md 9.2
```

Reviewed by a human, per release:

```text
new unsafe blocks         each states its invariant
new recursion             each has a bound and a test
new dependencies          justified, and fuzzed if they parse untrusted input
secrets                   none in the repository, the binary, or the logs
signing and notarization  docs/SIGNING_RUNBOOK.md
clean-machine check       downloaded, quarantined, opened
```

## 12. Release Signing

- macOS: hardened runtime, code signing, notarization, stapled ticket.
- Windows: Authenticode signing.
- Linux: signed packages and reproducible build direction.
- A documented process for reporting vulnerabilities and shipping a fix.

The procedure is `docs/SIGNING_RUNBOOK.md`; the decision and its cost are
`docs/DISTRIBUTION.md`.
