# jt-filework — release checklist

The gate from `AGENTS.md` §20.5 and `docs/SECURITY.md` §14, written as steps
someone can follow rather than as principles they have to interpret.

Nothing here is optional, and nothing here is a judgement call at release time:
if a line cannot pass, the release waits. A gate that gets waived once is a
gate that gets waived.

Run `./scripts/release-gate.sh` for the automated half. It exits non-zero on
the first failure and says which line failed.

---

## 1. Automated — `scripts/release-gate.sh`

| Check | Why it is a gate |
|---|---|
| `cargo fmt --check` | a diff nobody chose is noise in every later review |
| `cargo clippy -- -D warnings` | the lint set includes the safety ones |
| `cargo test --workspace` | includes the architecture and migration levels |
| `cargo deny check` | advisories, licences, bans, sources — one policy file |
| version consistency | `Cargo.toml`, both changelogs, both READMEs, the Pages site |
| changelog has this version | in **both** languages, written by hand |
| fixture per format version | `tests/fixtures/session/vN.json` for every N |

## 2. Reviewed by a person

Read the diff since the last tag, not the summary of it.

- [ ] **New `unsafe`** — each block states the invariant it relies on, and the
      invariant is true.
- [ ] **New recursion over untrusted data** — bounded, with a test that the
      bound holds. Structured parsers included.
- [ ] **New dependencies** — justified in the change that added them; fuzzed if
      they parse untrusted input; licence in `deny.toml`'s allow list.
- [ ] **Secrets** — none in the repository, the binary, the logs, or a
      command line. `strings` the binary if anything was added near the SFTP
      stack.
- [ ] **Destructive paths** — anything new that deletes, overwrites or moves
      says what it will do first, and has a test that it refuses the
      out-of-bounds case.
- [ ] **Stored formats** — if any stored file changed shape,
      `SESSION_FORMAT_VERSION` went up, a migration step exists, and a fixture
      was committed in the same change.

## 3. Signing and distribution

Per platform. The runbook is `docs/SIGNING_RUNBOOK.md`; this is the tick list.

**macOS**

- [ ] Built with the hardened runtime and the minimal entitlement set
- [ ] Every nested binary signed, then the bundle, with a Developer ID
- [ ] `notarytool submit --wait` accepted
- [ ] `stapler staple` on the `.app` and on the `.dmg`
- [ ] `spctl -a -vvv` passes on a **different** machine, on a copy that still
      carries the quarantine attribute

**Windows**

- [ ] Signed from CI, not from a laptop
- [ ] RFC 3161 timestamp on the executable and on the installer
- [ ] `signtool verify /pa /v` passes on a clean machine

**Linux**

- [ ] Built on the oldest supported distribution — glibc compatibility runs
      one way
- [ ] `sha256sum` published beside the artefacts

**Any unsigned build shared with anyone** carries the warning text in
`docs/SIGNING_RUNBOOK.md` §5, verbatim. Telling someone to right-click → Open
without telling them why is teaching them to bypass the check that protects
them.

## 4. After the tag

- [ ] Release notes are the changelog section, not a rewrite of it
- [ ] The Pages site names the new version and links the artefacts
- [ ] A clean-machine install is done by someone who did not build it
