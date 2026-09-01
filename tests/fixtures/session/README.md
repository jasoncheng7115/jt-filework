# Session fixtures

One file per released session format version, exactly as that version wrote
it. **These files never change.** A fixture edited to make a test pass stops
being evidence that an upgrade works and becomes a restatement of today's
code.

Adding a format version means: bump `SESSION_FORMAT_VERSION`, add a step to
`migrate_json`, and commit the new version's file here beside the others.

`v2.json` is the second released format. It differs from v1 in one setting:
the key hint strip is on. The v1 -> v2 step exists because a v1 file always
records `key_hints_visible` explicitly, so a changed default would otherwise
have reached nobody who had already run the program.

Like `v1.json`, it never changes again.
