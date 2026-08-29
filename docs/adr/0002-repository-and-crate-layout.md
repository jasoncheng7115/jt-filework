# ADR-0002: Repository and Crate Layout

- **Status:** Accepted
- **Date:** 2026-08-29
- **Deciders:** project owner
- **Supersedes:** —

## Context

`AGENTS.md` §4 requires that core logic must not depend on any GUI toolkit,
and §5 requires platform code to be isolated with no scattered `cfg(target_os)`
across core modules. `DEVELOPMENT_ENVIRONMENT.md` requires that the
platform-neutral core compile and test independently from UI and platform
adapters.

Enforcing this by convention and code review is unreliable: the first time
someone adds a toolkit type to a core signature "just for now", the boundary
is gone and nothing fails.

`AGENTS.md` §5 states the preferred high-level layout as directories under
`src/` (`core/`, `workspace/`, `search/`, `viewer/`, `jobs/`, `ai/`,
`platform/`, `ui/`).

## Options Considered

**A. Single crate, modules under `src/`.**
Matches §5 literally and is the simplest to start. But module boundaries are
not dependency boundaries: any module can `use` any other, and a GUI
dependency added to the crate is visible to every module including core.
Nothing prevents §4 from being violated.

**B. Cargo workspace with crates under `crates/`.**
Idiomatic Rust. Cargo enforces the boundary: a crate cannot use what is not in
its `Cargo.toml`. Deviates from the directory layout written in §5.

**C. Cargo workspace whose members live at the paths §5 names
(`src/core/`, `src/workspace/`, …).**
Cargo enforces the boundary exactly as in B, while the on-disk layout is the
one `AGENTS.md` §5 asks for. Cost: paths are one level deeper
(`src/core/src/lib.rs`), which is slightly unusual to read.

## Decision

Option **C**. JT FileWork is a Cargo workspace whose members live at the paths
`AGENTS.md` §5 specifies:

```text
src/
  core/        jtf-core        model, errors, i18n + theme contracts
  workspace/   jtf-workspace   split tree, panes, tabs, selection, marking
  jobs/        jtf-jobs        job engine, cancellation, progress
  commands/    jtf-commands    command ids, command bus, keymap
  search/      jtf-search      query parser, execution, result sets
  viewer/      jtf-viewer      viewer registry and providers
  ai/          jtf-ai          AI provider contracts and CLI providers
  platform/    jtf-platform    native service traits + null impls
    macos/     jtf-platform-macos
    windows/   jtf-platform-windows
    linux/     jtf-platform-linux
  ui/          jtf-ui          added after ADR-0001
tests/         workspace-level architecture and integration tests
```

`src/commands/` is added to the §5 list because `AGENTS.md` §9 makes the
command bus a first-class layer that both UI and core depend on, and it must
not live inside either.

Dependency direction is one-way:

```text
ui -> commands -> workspace -> jobs -> core
                     |            \
                     +-> search    +-> platform (traits)
                     +-> viewer
                     +-> ai
platform-{macos,windows,linux} -> platform (traits) -> core
```

No crate may depend on `ui`. No crate other than the platform adapters may
depend on a platform SDK. No crate other than `ui` may depend on a GUI toolkit.

## Consequences

### Positive
- `AGENTS.md` §4 and §5 become compile errors instead of review comments.
- `cargo test -p jtf-core` runs with no toolkit and no desktop session.
- Platform adapters are separate compilation units, so `cfg(target_os)` is
  needed only at the point where an adapter is selected.
- The GUI decision (ADR-0001) can be deferred without blocking core work.

### Negative
- Deeper paths (`src/core/src/lib.rs`).
- More `Cargo.toml` files to keep consistent; shared settings live in the
  workspace root `[workspace.package]`, `[workspace.dependencies]` and
  `[workspace.lints]`.
- Moving a type between crates is a visible, deliberate change rather than a
  file move — this is intended.

### Neutral
- Crate names are prefixed `jtf-` to avoid collisions on any future registry
  publication.

## Compliance

Verified by the architecture tests in `docs/TESTING.md` §3.2:

```text
architecture::core_has_no_gui_dependency
architecture::core_has_no_platform_sdk_dependency
architecture::no_target_os_cfg_outside_platform_layer
architecture::ui_layer_is_not_a_dependency_of_core
```

These tests parse the workspace manifests and fail on violation, so the
boundary cannot regress silently.

## Revisit Criteria

- ADR-0001 selects a stack whose build system cannot tolerate this layout.
- Compile times make the crate split a net loss at a scale we can measure.
