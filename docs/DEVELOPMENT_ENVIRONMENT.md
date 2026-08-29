# Development Environment

## Primary Host

Use **macOS on Apple Silicon** as the primary development environment.

Reason:
- macOS is the first product target
- deepest initial native integration is on macOS
- Quick Look, Finder drag/drop, tags, bundles, Services and system appearance must be tested locally
- cross-platform core remains platform-neutral

## Git

Git is mandatory from project creation.

Initial bootstrap:

```bash
mkdir jt-filework
cd jt-filework
git init
git branch -M main
```

Then add:
```text
README.md
LICENSE
AGENTS.md
TODO.md
docs/
.gitignore
.gitattributes
```

Create the baseline architecture/spec commit before implementation.

Recommended branch strategy:
```text
main
poc/qt6
poc/slint
feature/...
fix/...
```

`main` must stay buildable and testable (`AGENTS.md` §2). Every branch runs
the same gates as `main` before merge: see `docs/TESTING.md` §15.

## Initial Toolchain

Expected:
- Git
- Rust stable
- rustfmt
- clippy
- cargo test
- C/C++ toolchain required by GUI/native bridges
- Xcode Command Line Tools
- selected GUI framework after ADR
- optional Node tooling only for isolated WebView content if used

### Verified baseline (2026-08-29)

```text
macOS            15.7.5, arm64 (Apple Silicon)
Xcode CLT        /Library/Developer/CommandLineTools
git              2.50.1
rustc            1.98.0 (stable)
cargo            1.98.0
rustfmt          1.9.0-stable
clippy           0.1.98
```

Rust is installed via `rustup`. The toolchain is pinned in
`rust-toolchain.toml` so every contributor and every CI job uses the same
compiler, `rustfmt` and `clippy`.

## Build Output Location

This working copy lives inside a **Nextcloud-synced directory**. Rust build
output can reach several gigabytes and changes on every build, which is
pathological for a file-sync client, and a partially synced `target/` can
produce confusing build failures.

Build output is therefore redirected outside the synced tree:

```bash
export CARGO_TARGET_DIR="$HOME/.cache/jt-filework-target"
```

`.cargo/config.toml` in the repository sets this by default, so no manual
step is required. `target/` remains in `.gitignore` regardless.

Consequences:
- `.git` still lives inside the synced tree. Do not run Git operations from
  two machines at the same time on the same synced copy; use a Git remote for
  multi-machine work rather than relying on file sync.
- Anything expensive and regenerable (caches, previews, thumbnails, fuzz
  corpora artifacts) belongs outside the synced tree as well.

## Cross-Platform Validation

CI matrix later:
- macOS Apple Silicon / supported runner
- Windows x64
- Linux x64

Architectural tests must compile platform-neutral core independently from UI/platform adapters where practical.

## Repository Hygiene

Required:
- `.gitignore`
- `.gitattributes`
- consistent line endings
- no secrets
- no local AI credentials
- no build directories
- no generated preview/cache files

## AI Development Workflow

Preferred:
- Claude Code: primary implementation/architecture agent
- Codex: independent review, tests, alternate implementation, bug/security review

Before agent execution:
```text
git status
```

After:
```text
git status
git diff
```

Review before commit.
