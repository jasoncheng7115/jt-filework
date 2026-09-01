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

The **Qt build directory is redirected the same way**, for the same reason and
after the same lesson: it had been left at `src/ui/qt6/build/` and had reached
4.2 GB of object files re-uploading after every build. `src/ui/qt6/build.sh`
now builds into `~/.cache/jt-filework-qt/<mode>`, overridable with
`JTF_BUILD_ROOT`.

A `release` build also copies the bundle to `/Applications/jt-filework.app`.
The Dock pins a path, not a project: pinned straight at a build directory it
kept whichever configuration was pinned first, which is how the Dock icon
ended up launching a stale debug build while the release one ran beside it —
two icons for one application, because macOS groups by bundle path.

Consequences:
- `.git` still lives inside the synced tree. Do not run Git operations from
  two machines at the same time on the same synced copy; use a Git remote for
  multi-machine work rather than relying on file sync.
- Anything expensive and regenerable (caches, previews, thumbnails, fuzz
  corpora artifacts) belongs outside the synced tree as well.

## The Windows Build Host

A Windows machine on the maintainer's own network, reached over SSH. Its
address is not written down here: an address plus a note saying what runs on it
is a description of a target, and this file is public.

It has been restored from backup once and everything below was gone afterwards,
so this section exists to make the rebuild a script rather than an afternoon.

What the machine has after a restore: Python, Git, winget, an SSH server.
What it does not have and the build needs, in this order:

| Needed | Installed by | Notes |
| --- | --- | --- |
| MSVC 2022 Build Tools | `winget install Microsoft.VisualStudio.2022.BuildTools` with `--override "--quiet --wait --norestart --add Microsoft.VisualStudio.Workload.VCTools …"` | ~9 minutes, several GB. Everything else depends on it. |
| CMake | `winget install Kitware.CMake` | Not on `PATH` in a fresh SSH session; find it at `C:\Program Files\CMake\bin`. |
| Rust | `rustup-init.exe -y --default-host x86_64-pc-windows-msvc --profile minimal` | Must be run through `Start-Process -Wait -PassThru` with the streams redirected. Piping it to `Out-Null` returned in one second having done nothing, and the failure was silent. |
| Qt 6.8.3 msvc2022_64 | `python -m aqt install-qt windows desktop 6.8.3 win64_msvc2022_64 -O C:\Qt` | ~2 minutes. |
| NASM | `winget install NASM.NASM` | `aws-lc-sys`, which arrives through russh and rustls for SFTP, assembles its own primitives on Windows. Without NASM the build fails at 2% with the reason buried under a screen of unrelated `cl.exe` warnings, and the message that matters is the last line, not the first. Not needed on macOS or Linux. |

Three things that cost time and should not cost it again:

- **Ninja is not there and is not worth installing.** CMake defaults to it and
  fails with `CMAKE_MAKE_PROGRAM is not set`. `NMake Makefiles` comes with the
  MSVC toolset that is already required — slower, and one fewer thing to be
  missing after the next restore.
- **`$ErrorActionPreference = 'Stop'` and `2>&1` must not meet.** cargo writes
  its progress to stderr; with `2>&1` those lines arrive as PowerShell error
  records, and under `Stop` the first one ends the script. The build stopped
  silently at 2% and still exited 0 — twice, and the second time it was
  mistaken for the SSH session killing it. Use `Continue` and check
  `$LASTEXITCODE`, which is what actually says whether a native command failed.
- **Start the build as a scheduled task, not with `Start-Process`.** A build
  that takes twenty minutes should not depend on a connection staying up for
  twenty minutes — but `Start-Process … -WindowStyle Hidden` over Windows
  OpenSSH silently starts nothing at all. It reports success, the log keeps
  its old contents, and the obvious next move is to read that stale log and
  diagnose a problem that was fixed an hour ago. What works:

  ```
  schtasks /create /tn jtfbuild /tr "powershell -NoProfile -ExecutionPolicy Bypass -File C:\jtdev\winbuild.ps1" /sc once /st 23:59 /f /ru user /rp <password> /rl highest
  schtasks /run /tn jtfbuild
  ```

  It runs under the Task Scheduler service, and `/ru user` is what gives it
  the user's `.cargo`. **Check the log's timestamp before believing it**: an
  unchanged `LastWriteTime` means the build never started, which looks exactly
  like a build that failed instantly.
- **Find the toolchain rather than hardcoding it.** `vswhere` gives the MSVC
  path; a path written into a script is a path that breaks at the next upgrade.

`C:\jtdev\winsetup.ps1` does the installs and `C:\jtdev\winbuild.ps1` does the
build, deploys the Qt runtime with `windeployqt`, and rewrites the Desktop
shortcut so it always points at the newest executable. Both are idempotent:
each step checks before it acts, so a failed run can simply be repeated.

## Cross-Platform Validation

CI matrix later:
- macOS Apple Silicon / supported runner
- Windows x64
- Linux x64

**Intel Macs.** Nothing in the source is architecture-specific, and
`build.sh` pins `CMAKE_OSX_ARCHITECTURES` to `uname -m`, so building *on* an
Intel Mac produces an x86_64 build with no changes — given Homebrew Qt and
`rustup target add x86_64-apple-darwin` on that machine. Two things are not
free:

- The shipped bundle is **arm64 only**. Rosetta translates x86_64 to arm64 and
  not the other way, so it will not run on an Intel Mac.
- A universal binary needs a universal Qt. Homebrew's is single-architecture;
  the official Qt installer's frameworks are universal.
- The deployment target is currently macOS 15, which excludes Intel Macs left
  on Monterey, Ventura or Sonoma. Lower it before claiming Intel support.

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
