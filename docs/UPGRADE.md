# jt-filework — Upgrade and Compatibility

What happens to a user's data, settings and habits when they install a new
version. Written now, before there are users, because every rule here is cheap
to follow from the start and expensive to retrofit once real installations
exist.

The governing principle: **an upgrade never loses anything the user did, and
never silently changes what they chose.** Where those two conflict, the user
is told rather than surprised.

---

## 1. What persists, and what owns each thing

| Artefact | Location | Written by | Versioned |
|---|---|---|---|
| Session (layout, tabs, marks) | `~/Library/Application Support/jt-filework/session.json` | the app | yes, `SESSION_FORMAT_VERSION` |
| Session settings (startup, memory, font, keymap name) | inside the session file | the app | with the session |
| User keymap | `~/Library/Application Support/jt-filework/user.keymap` | the app | **planned** |
| Shipped keymap presets | `keymaps/*.keymap` | the build | with the build |
| Locale catalogues | `locales/<locale>/*.catalog` | the build | with the build |
| Caches, thumbnails, indexes | a cache directory | the app | disposable by definition |

Rule: **anything the user can change is stored separately from anything the
build ships.** A preset the user edited becomes a user file; the shipped file
is never written to. That is what makes an upgrade able to improve a preset
without touching a customisation.

---

## 2. Versioning rules

Every stored artefact carries a format version.

- **Reading an older version**: migrate forward, in one place, with a test per
  migration step. Never scatter `if version < N` through the code that uses
  the data.
- **Reading a newer version**: refuse and start fresh, **and say so**. Guessing
  at a format from the future is how data gets corrupted by a downgrade.
- **Adding a field**: give it a serde default so an old file still loads. The
  default must be the behaviour the user already had, not the new feature
  turned on.
- **Removing a field**: ignore it on read for at least one release, then drop
  it. Never error on an unknown field.
- **Renaming a field**: read both names for one release; write only the new
  one. A rename is a migration, not an edit.

### 2.1 The stamp

Every file also records the application version that wrote it. Not for logic —
version comparisons in code age badly — but so that a bug report says which
build produced the file.

---

## 3. Migration mechanics

- **One migration module**, a chain of `vN -> vN+1` steps, applied in order.
  A file three versions old walks the chain; there is no matrix.
- **Migration is a pure function** over the parsed old form, so every step is
  unit-testable without touching a disk.
- **A fixture per version** is committed: an actual file written by that
  release, with a test that migrating it produces what the current version
  expects. This is the only way to know a migration works, because the shape
  of real data is not the shape anyone remembers.
- **Migration never writes in place.** Write the migrated file beside the
  original, `fsync`, rename over it. A crash mid-migration must leave the old
  file loadable (`docs/UI_TEST_PLAN.md` SESS-005).
- **Keep one backup** of the pre-migration file, named with its version. It
  costs a few kilobytes and it is the difference between "restore your
  session" and "sorry".
- **Migration failure is not fatal.** Fall back to defaults, keep the original
  untouched, tell the user where it is.

---

## 4. The keymap, which is the hard one

A keymap is the one stored artefact that references identifiers the build
owns. Every upgrade risk shows up here first.

### 4.1 Store a diff, not a copy

If the user file holds a **complete** keymap, then a command added in a later
release ships with **no binding** for every existing user, because their file
does not mention it and their file wins. They would have to reset to the
preset — losing their customisations — to get the new shortcut.

So the user file holds only what **differs** from the named preset:

```text
# a changed binding
primary+shift+k = tab.close

# an explicitly removed binding, which is different from "not mentioned"
none = preview.quicklook
```

On load: take the preset, apply the diff. A command the user never touched
picks up whatever the preset says today, including a new one. A command they
did touch keeps their choice.

### 4.2 Commands that disappear or get renamed

- A binding naming a command the registry does not know is **dropped on load**,
  not an error. The rest of the keymap still works.
- A rename ships with an **alias table**: `old.id -> new.id`, applied when
  loading a user keymap, so a customisation survives the rename. The alias
  stays for at least one release and is then removed with the migration.
- The settings window shows how many bindings were dropped, so the user can
  see something changed rather than discovering a dead key.

### 4.3 Preset changes

- A preset is shipped data. Changing it changes the defaults for everyone who
  did not customise that binding — which is the point.
- A preset change that **moves an existing default** is a release-note item,
  because muscle memory is a user's data too.

---

## 5. Settings

- A new setting defaults to the **current behaviour**, never to the new
  feature enabled. An upgrade that changes what the application does without
  being asked is a bug however good the new default is.
- A removed setting is ignored on read; its value is not migrated into
  something else without saying so.
- A setting whose meaning changes gets a **new key**. Reusing a key with new
  semantics silently reinterprets what the user chose.

---

## 6. Locales

- A key removed from the catalogue but still referenced by an older stored
  value falls back to English, then to the key itself. It never crashes and
  never shows an empty label.
- Adding a locale is not a migration; it is a new file.
- The parity and coverage tests (`docs/TESTING.md` §3.3) already prevent the
  common upgrade failure: a build that ships a command whose label nobody
  translated.

---

## 7. Caches and indexes

- Anything derivable is **versioned by a schema stamp and discarded on
  mismatch**, never migrated. Rebuilding a thumbnail is cheap; a subtly wrong
  migrated cache is not.
- A cache is never on the correctness path. If deleting the whole cache
  directory changes any result, that is a bug.
- Stale detection is by content, not by version: a file that changed since it
  was indexed is re-read regardless of what any version says.

---

## 8. Downgrade

Users downgrade — a bad release, a rollback, two machines on different
versions sharing a synced folder.

- An older build reading a newer file **starts fresh and says why**. It does
  not attempt a partial read.
- It does **not** overwrite the newer file until the user does something that
  writes. The newer version's data survives a look.
- A synced configuration directory is explicitly out of scope for conflict
  resolution: the sync client decides, and the application only guarantees it
  will not corrupt what it finds. This is documented rather than solved,
  because solving it means implementing a merge nobody asked for.

---

## 9. Platform-level upgrade

- **macOS**: a replaced app bundle re-triggers Gatekeeper. The build must be
  signed and notarized every time, not just for the first release
  (`docs/SIGNING_RUNBOOK.md`). A quarantined update that will not open looks
  identical to a broken update.
- **Windows**: an installer must not leave the old binary running; signing
  reputation follows the certificate, so changing certificates resets
  SmartScreen.
- **Linux**: a package manager handles this; an AppImage does not, so
  self-update expectations must be stated rather than assumed.
- **First launch after an update** is the moment to run migrations and, if
  anything was dropped or reset, to say so once.

---

## 10. Testing an upgrade

`docs/TESTING.md` gains a level for this. Every one of these is a test, not a
hope:

```text
migrate::session_v1_to_current          from a committed fixture
migrate::settings_default_preserves_behaviour
migrate::unknown_field_is_ignored
migrate::newer_version_starts_fresh_and_reports
migrate::crash_mid_write_leaves_old_file_loadable
migrate::backup_is_written_before_migrating
keymap::user_diff_gains_new_preset_bindings
keymap::user_diff_keeps_customised_bindings
keymap::binding_for_removed_command_is_dropped_not_fatal
keymap::renamed_command_follows_its_alias
cache::schema_mismatch_discards_rather_than_migrates
cache::deleting_the_cache_changes_no_result
```

The fixtures matter more than the assertions: each released format version
leaves a real file in `tests/fixtures/session/vN.json`, and it stays there
forever.

---

## 11. Status

| Item | Status |
|---|---|
| Session format version, refuse-newer, report | done |
| Atomic write of the session | done |
| Serde defaults on every added setting | done |
| Locale fallback chain | done |
| Keymap stored as a diff against its preset | done |
| Unknown command ids dropped on load, count shown | done |
| Command rename alias table | planned |
| Migration chain and per-version fixtures | done — `migrate_json`, `tests/fixtures/session/` |
| Pre-migration backup | done — `session.vN.backup.json`, written once |
| App version stamp in every stored file | done — session; keymap and cache still to do |
| Cache schema stamp and discard-on-mismatch | planned |
| First-launch-after-update notice | planned |

Each planned row is a `TODO.md` item. None of them are hard; all of them are
much harder after the first release.

The migration chain is deliberately a *chain* rather than one parser that
reads every past shape. Each step is a small change whose correctness can be
argued and tested against a real file from the version it came from, and the
alternative accumulates conditionals nobody dares touch. There is one step
site and no steps yet, because v1 is the first released format — the loop
exists so that the second migration is an edit rather than a design.

The backup is named for the version it came from, so a directory holding
several is readable rather than a pile of `.bak` files, and it is written only
once: a second run of the same upgrade must not overwrite the original with
the already-migrated copy, which would quietly destroy the thing being kept.
