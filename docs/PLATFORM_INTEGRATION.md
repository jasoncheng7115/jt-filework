# JT FileWork — Platform Integration Specification

Platform code is isolated behind service interfaces (`AGENTS.md` §5,
`ARCHITECTURE.md` §9). Core never imports a platform SDK; platform adapters
never import UI.

---

## 1. Service Interfaces

```text
NativePreviewService     NativeThumbnailService   NativeContextMenuService
NativeTrashService       NativeOpenWithService    NativeShareService
NativeMetadataService    NativeDragDropService    NativeSearchService
NativeThemeService       NativeLocaleService
```

Rules:
- every service has a **null implementation** so core tests run without a
  desktop session (`TESTING.md` §6)
- every service reports capability, so the UI can hide what a platform cannot
  do instead of failing at use time
- every service call that can block is async and cancellable
- a service that is slow or hung degrades gracefully; it never blocks the UI

### 1.1 Capability model

```text
Capability {
  supported: bool,
  requires_user_permission: bool,
  degraded_reason: Option<ErrorCode>,
}
```

The UI asks before it offers. A missing capability is a documented, visible
limitation, never a silent no-op.

---

## 2. macOS (first shipping target)

### 2.1 Preview
- **Quick Look panel** via `QLPreviewPanel` for the Space shortcut
- **embedded preview** via `QLPreviewView` for the tool area
- **thumbnails** via `QuickLookThumbnailing`
- all of it out-of-band from the UI thread; generation is a Job

### 2.2 Filesystem semantics
- APFS/HFS+ case-insensitivity, and case-sensitive volumes
- Unicode normalization: NFD on disk, NFC in many inputs — comparison and
  deduplication must normalize deliberately
- **aliases** resolved via bookmark APIs, never by path guessing
- **application bundles** and **packages** presented as single items by
  default, traversable on request
- extended attributes, `com.apple.quarantine`, Finder tags
- resource forks are not silently dropped by copy operations

### 2.3 Trash and file operations
- Trash via `NSFileManager.trashItem`, preserving Put Back where the OS does
- copy/move using platform APIs where they preserve metadata correctly

### 2.4 Shell ecosystem
- **Open With**: `LaunchServices` application list for a content type
- **Reveal in Finder**
- **Share / Services / Quick Actions** where public APIs allow
- **Finder extensions are Finder-only**: third-party Finder Sync extensions
  (e.g. cloud-sync badges) cannot be hosted by another application. This is a
  documented limitation, and JT FileWork provides its own overlay mechanism
  instead of pretending otherwise.

### 2.5 Drag and drop
`NSPasteboard` with file URL promises; honour Option/Command modifier
semantics for copy/move/alias; support multi-file drags in both directions.

### 2.6 Appearance and locale
`NSAppearance` for Light/Dark/System with live change notification;
`NSLocale` for the system language and for locale-aware formatting.

### 2.7 Search
Optional `NSMetadataQuery` (Spotlight) adapter as an accelerator. It must be
optional: results must be reproducible with the deterministic scanner when
Spotlight is disabled or the volume is not indexed.

### 2.8 Distribution
Hardened runtime, signing, notarization, stapling. Minimum macOS version is
fixed in Phase 0A.

---

## 3. Windows

### 3.1 Filesystem semantics
- long paths (`\\?\` and the manifest opt-in), UNC paths
- reparse points, junctions, symlinks, and the privileges they require
- alternate data streams: preserved on copy where the target supports them,
  and never used as a hiding place we ignore
- reserved device names and trailing dot/space names handled explicitly
- case-insensitive by default, case-sensitive directories possible

### 3.2 Shell integration
- **Recycle Bin** via `IFileOperation`
- copy/move via `IFileOperation` so Explorer-consistent progress, conflict and
  undo semantics apply
- **thumbnails** via `IThumbnailProvider` / `IShellItemImageFactory`
- **preview** via `IPreviewHandler`
- **context menu**: `IContextMenu` for classic handlers and
  `IExplorerCommand` for modern ones
- goal: maximum compatibility with installed extensions such as Nextcloud,
  OneDrive, 7-Zip and Git tooling (`PRODUCT_SPEC.md` §10)

### 3.3 ShellHost isolation
Third-party handlers are hosted **out-of-process** (`SECURITY.md` §6). A
crashing or hanging handler must not affect the main process; the menu shows
the entry as unavailable instead.

### 3.4 Appearance
Follow the system light/dark setting where the OS exposes it; provide explicit
overrides regardless.

---

## 4. Linux

### 4.1 Display
Wayland first, X11 supported. Drag-and-drop, clipboard and window behaviour
differ between them and are tested separately.

### 4.2 Desktop integration
- **XDG Trash** specification, including `trashinfo` and cross-device rules
- **MIME** via shared-mime-info; **Open With** via desktop entries
- **thumbnails** via the freedesktop thumbnail spec and cache
- **GIO / D-Bus** for mounts, volumes and file-manager interoperability
- `org.freedesktop.FileManager1` for "reveal in file manager" interop
- extended attributes and POSIX ACLs

### 4.3 Desktop-specific adapters
Nautilus and Dolphin interoperability where practical. Anything unreliable is
optional and degrades cleanly.

### 4.4 Appearance
Follow the desktop/toolkit theme where it is reliably reported; explicit
Light/Dark override always available.

### 4.5 Packaging
Direction: an archive build plus Flatpak, with distribution packages
considered later. Sandbox portals affect filesystem access and must be
evaluated before committing to Flatpak-only distribution.

---

## 5. Remote and Network Filesystems

SMB, NFS, UNC and cloud-sync folders are treated as hostile with respect to
latency (`SECURITY.md` §2):

- every operation is cancellable and has a timeout policy
- a stalled mount produces a distinct, visible state, never a frozen window
- metadata is fetched lazily and in batches; no stat storm on enumeration
- cloud placeholder / offline files are detected and **not** hydrated
  implicitly by enumeration, preview or thumbnailing
- recursive size and hashing over a network mount always warn about cost

---

## 6. Cross-Platform Rules

- no `#[cfg(target_os = …)]` outside `src/platform/**` (`TESTING.md` §3.2)
- platform differences are expressed as capabilities, not as branches in core
- a feature that exists on one platform only is still modelled in core with a
  null implementation elsewhere
- every platform limitation is documented in this file rather than being
  discovered by users
