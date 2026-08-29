# Session fixtures

One file per released session format version, exactly as that version wrote
it. **These files never change.** A fixture edited to make a test pass stops
being evidence that an upgrade works and becomes a restatement of today's
code.

Adding a format version means: bump `SESSION_FORMAT_VERSION`, add a step to
`migrate_json`, and commit the new version's file here beside the others.
