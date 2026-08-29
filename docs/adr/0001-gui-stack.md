# ADR-0001: GUI Technology Stack

- **Status:** Proposed — **blocked on the Phase 0B PoC. Do not implement UI
  against any candidate before this ADR is Accepted.**
- **Date:** 2026-08-29 (opened)
- **Deciders:** project owner

## Context

JT FileWork needs a desktop UI that can do all of the following at once:

- render a virtualized list of 100 000–1 000 000 entries smoothly
- never block the UI thread on filesystem, preview, index or AI work
  (`AGENTS.md` §3)
- host **native** macOS Quick Look and native context menus
- participate in Finder/Explorer drag-and-drop with correct modifier semantics
- support arbitrary recursive split layouts with independent tabs per pane
  (`AGENTS.md` §6, §7)
- switch locale and theme **at runtime** without restart or data loss
  (`AGENTS.md` §11, §12)
- run on macOS, Windows and Linux from one core
- remain replaceable: the UI layer is a consumer of commands and models, never
  the owner of logic (`AGENTS.md` §4)

`ARCHITECTURE.md` §2 fixes two constraints up front: the core is Rust, and C#
is rejected for this project.

This decision cannot be made from documentation. It is made from measurements
produced by the Phase 0B PoC, built identically on each candidate.

## Options Considered

### A. Rust + Qt 6 Widgets (via a Rust/C++ bridge)
Mature, complete desktop widget set; a real virtualized item view; native menu
integration on macOS and Windows; strong high-DPI and IME track record;
built-in translation and theming machinery.
Costs: C++ bridge complexity; binding maintenance; Qt licensing must be
confirmed compatible with `GPL-3.0-or-later` distribution on all three
platforms; larger distribution size.

### B. Rust + Slint
Pure-Rust toolchain, no C++ bridge, small binaries, declarative UI, good
control over rendering.
Costs: fewer batteries-included desktop behaviours; native menu, native
drag-and-drop and embedded native preview integration need more custom work;
must be proven at 1M rows and with platform IME.

### C. Selective WebView hybrid
Native shell for the file list and chrome, WebView only for AI conversation,
Markdown, rich diff and documentation (`ARCHITECTURE.md` §17).
This is not a standalone option — it is a modifier on A or B, and it is
explicitly forbidden for the core FilePane unless the PoC proves native
fidelity is otherwise unreachable.

## Required PoC Scope (identical on every candidate)

From `DEVELOPMENT_PLAN.md` Phase 0B:

**Scale and input**
- 100 000+ entries, virtualized, smooth scroll; plus a 1M synthetic run
- keyboard navigation; native selection; **separate** mark state
- stable IME composition and focus behaviour

**Layout**
- horizontal split, vertical split, nested split, 2×2
- independent tabs per pane, tab reorder, tab drag between panes
- pane → pane file drag

**Native integration**
- Finder → app and app → Finder drag-and-drop with modifiers
- Quick Look panel proof
- embedded native preview proof
- native context menu proof

**i18n and theme**
- English UI and Taiwan Traditional Chinese UI
- runtime locale switch with no restart and no state loss
- Light, Dark, Follow System; runtime theme switch
- high-DPI at 1x and 2x

**Optional**
- WebView AI panel embedded in the native shell

**Testability (added by `docs/TESTING.md` §7)**
- can the UI be driven headlessly in CI?
- can a UI-thread watchdog observe task durations?
A candidate that cannot be tested automatically is penalised.

## Evaluation Rubric

Each criterion is scored 0–5 on measured evidence. Gate criteria are
pass/fail: **a candidate that fails any gate is eliminated regardless of
score.**

| # | Criterion | Type | Weight |
|---|---|---|---|
| 1 | 100K virtualized list performance (first paint, scroll, sort, filter) | Gate + score | 3 |
| 2 | UI thread never blocks under the `TESTING.md` §7.1 scenarios | **Gate** | — |
| 3 | Recursive split + per-pane tabs + tab drag between panes | **Gate** | 2 |
| 4 | Native drag-and-drop both directions with modifiers | **Gate** | 3 |
| 5 | Quick Look panel + embedded native preview | **Gate** | 3 |
| 6 | Native context menu hosting | Score | 2 |
| 7 | Runtime locale switch, no restart, no state loss | **Gate** | 2 |
| 8 | Runtime theme switch, Light/Dark/System, semantic tokens | **Gate** | 2 |
| 9 | IME, focus, high-DPI correctness | **Gate** | 2 |
| 10 | Windows and Linux viability of the same code | Score | 3 |
| 11 | Headless/automated UI testability | Score | 2 |
| 12 | Accessibility API exposure (role/name/value) | Score | 2 |
| 13 | Bridge/binding complexity and maintenance risk | Score | 2 |
| 14 | Build, packaging, signing and notarization friction | Score | 1 |
| 15 | Binary size and memory footprint | Score | 1 |
| 16 | Licensing compatibility with `GPL-3.0-or-later` on all platforms | **Gate** | — |
| 17 | Project health: releases, maintenance, community | Score | 1 |

## Measurements to Record

For each candidate, recorded on the same machine and committed to the
repository:

```text
first row visible after entering 100K directory   (ms)
full enumeration of 100K entries                  (ms)
sort 100K by name / size / date                   (ms)
keystroke to filtered result on 100K              (ms)
scroll frame time p50 / p95 / p99                 (ms)
longest single UI-thread task                     (ms)
memory at 100K / 1M entries                       (MB)
cold start                                        (ms)
release binary size                               (MB)
build time, clean and incremental                 (s)
```

Qualitative notes are recorded for every gate, with a screenshot or recording
as evidence.

## Decision

**Not yet made.** To be filled in when the PoC completes, on branches
`poc/qt6` and `poc/slint` (`AGENTS.md` §2).

The decision must state: the chosen stack, the scores, which gates each
candidate passed or failed, and the WebView policy that follows.

## Consequences

To be filled in with the decision.

## Compliance

Whatever is chosen must keep `AGENTS.md` §4 true: the UI layer consumes
commands, models and service contracts, and no core crate depends on the
toolkit. This is enforced by
`architecture::core_has_no_gui_dependency` (`TESTING.md` §3.2), which is
written before the UI crate exists.

## Revisit Criteria

- a gate failure is discovered after adoption
- the chosen toolkit's Windows or Linux story fails in Phase 4 or Phase 5
- a measured performance target in `TESTING.md` §8.2 proves unreachable
