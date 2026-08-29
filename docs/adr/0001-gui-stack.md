# ADR-0001: GUI Technology Stack

- **Status:** Proposed — the macOS performance gates are now measured and met
  (see below). What remains before this can be Accepted is a decision by the
  project owner, plus the same numbers on Windows and Linux.
- **Date:** 2026-08-29 (opened), 2026-08-30 (performance gates measured)
- **Deciders:** project owner

## Context

jt-filework needs a desktop UI that can do all of the following at once:

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

**Testability (added by `docs/TESTING.md` §7 and `docs/UI_TEST_PLAN.md`)**
- can the UI be driven headlessly in CI?
- can a UI-thread watchdog observe task durations?
- can the harness layers in `docs/UI_TEST_PLAN.md` §0.1 be implemented:
  headless widget instantiation, synthetic input events, deterministic
  screenshot capture, and an accessibility-tree query?
A candidate that cannot execute the plan's gate areas automatically is
eliminated; one that cannot be driven headlessly is penalised.

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
| 11 | Executes `docs/UI_TEST_PLAN.md`: headless widgets, synthetic input, screenshots, a11y tree | Gate (plan's gate areas) + score | 2 |
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

## Measurements so far — Qt 6, `poc/qt6`

Recorded 2026-08-29. Machine: MacBook, Apple Silicon, macOS 15.7.5,
Qt 6.11.1 (arm64, Homebrew), Rust 1.98.0, release build. Fixture: 100 000
flat entries with mixed extensions, generated by `cargo run -p jtf-bench`.

| Measurement | Result | Budget | |
|---|---|---|---|
| First rows visible | 2.9 ms | 150 ms | pass |
| Full enumeration, async | 586 ms | 2 s | pass |
| Full enumeration, blocking | 198 ms | — | reference |
| Sort by name | 26.3 ms | 250 ms | pass |
| Sort by size | 8.0 ms | 250 ms | pass |
| Sort by modified | 15.1 ms | 250 ms | pass |
| Sort by extension | 21.9 ms | 250 ms | pass |
| Filter, substring | 7.0 ms | 100 ms | pass |
| Model footprint | 27.4 MB | documented | 287 bytes/entry |

Two findings, both acted on rather than noted:

**Sorting by name cost 226 ms**, 90 % of its budget, because the comparator
called `to_lowercase()` on both sides of every comparison — roughly 3.4
million allocations for 100 000 entries. Decorate-sort-undecorate brought it
to 26 ms, an 8.8× improvement, and the function now lives in `jtf-workspace`
so the benchmark measures the same code the application runs.

**Batch size is a trade-off between first paint and total cost.** A fixed 256
gave first rows in 4 ms but tripled the total against the blocking path; a
fixed 2048 halved the overhead but pushed first rows to 20 ms. Ramping from
64 to 4096 gives 2.9 ms first rows and the lower total.

**Resolved: async enumeration was never 3× the blocking path.** The gap was
the benchmark's own doing. Both paths ran in one pass, async first, so the
async pass paid for a cold filesystem cache and the blocking pass read what it
had just warmed. Warming both before timing puts them within noise of each
other — 421 ms and 376 ms at 100 000 entries. The cross-thread allocator
hypothesis was wrong, and nothing needed fixing.

The lesson is recorded in the benchmark itself: a measurement that compares a
cold read against a warm one is measuring the cache.

## Measurements at 1 000 000 entries

Recorded 2026-08-30, same machine, release build. Fixture: 1 000 000 flat
entries, 275 MB of model data.

| Measurement | 100 000 | 1 000 000 | Budget at 1M | |
|---|---|---|---|---|
| First rows visible | 0.6 ms | 3.5 ms | 150 ms | pass |
| Full enumeration, warm | 421 ms | 57.0 s | — | filesystem |
| Full enumeration, blocking | 376 ms | 57.0 s | — | reference |
| First visit, cold | 2.4 s | 57 s | — | filesystem |
| Sort by name | 43 ms | 999 ms | 2 520 ms | pass |
| Sort by size | 11 ms | 229 ms | 2 520 ms | pass |
| Sort by modified | 21 ms | 506 ms | 2 520 ms | pass |
| Sort by extension | 39 ms | 749 ms | 2 520 ms | pass |
| Filter, substring | 14 ms | 196 ms | 1 010 ms | pass |
| Model footprint | 27.4 MB | 274.9 MB | documented | 288 bytes/entry |

**Sorting and filtering scale linearly and stay well inside budget.** Ten
times the entries costs roughly ten times the time, which is what the
algorithms promise.

**Enumeration is the filesystem, not us.** 5 µs an entry at 100 000 and 57 µs
at 1 000 000, with the blocking path agreeing to within noise both times. The
per-entry cost degrades with directory size in a way that is APFS's
behaviour — on a volume that was 98 % full — and no amount of work here
changes it. `DirEntry::metadata` was tried in place of `fs::symlink_metadata`
on the theory that re-resolving the path per entry was the cost; it made no
measurable difference at either size and was reverted rather than kept on a
justification that measurement had disproved.

**So enumeration is reported without a verdict.** A budget it cannot meet, for
a reason nobody can act on, is a red line that teaches people to ignore red
lines. What this program actually promises is the line above it: first rows in
3.5 ms at a million entries, so a directory's size is never the user's problem
even when the disk's answer takes a minute.

**Budgets now scale with entry count**, because the work does. A flat 250 ms
for a sort is a statement about 100 000 entries; stated flat it called a
correct 999 ms sort of a million names a failure while letting a slow sort of
ten thousand pass.

## UI-thread watchdog — first recorded run

Recorded 2026-08-30, same machine. `JTF_WATCHDOG=1`, session restored into the
100 000-entry fixture, ~25 s of running, 6 339 events dispatched.

| | |
|---|---|
| p50 | 0 µs |
| p95 | 55 µs |
| p99 | 486 µs |
| worst | 187 ms (the first tick after the window appears) |
| over one 60 Hz frame | 20 events, 0.32 % |

The watchdog earned its keep immediately by finding two real defects.

**A linear scan per thumbnail.** When a decoded thumbnail arrived, the model
searched the whole list for the row that had asked for it — one FFI call and
one string allocation per row, on the UI thread. In a directory of a hundred
thousand that was a hundred thousand lookups to repaint a single line: far
more work than the decoding it existed to keep off the thread. The row is now
carried with the request and verified on arrival, which is one lookup. p99
halved, from 944 µs to 486 µs.

**A hidden view doing full layouts.** The icon grid shares the list's model,
and a hidden view still receives every model reset and still lays out every
item. It is now attached only while it is showing.

**Where the remaining time goes.** Timing the model's share of a tick
separately from the repaint that follows settles it: the model pump never
exceeded one frame even at 100 000 entries, while the view refresh grew to
~28 ms as the listing streamed in. The cost is Qt's own bookkeeping for
incremental insertion into a large table, it is confined to the seconds a
directory is loading, and first rows are still on screen in under a
millisecond. Recorded rather than chased.

**Two changes the watchdog also prompted.** It only printed its report on a
clean exit, so a session that was killed — or that hung, which is when you
want it most — produced nothing; it now reports as it goes. And the session
file was written only when the window closed, so a crash or a force quit lost
the layout the user had arranged. It is now saved periodically as well.

**Not yet measured:** scroll frame times, memory after repeated navigation,
and the same numbers on Windows and Linux.

## Decision

**Not yet made.** Qt 6 has been selected by the project owner and is proven to
build and run natively on Apple Silicon. The performance gates are measured
and met on macOS at both 100 000 and 1 000 000 entries, including the one that
matters most — first rows on screen in under 4 ms at a million entries.

This stays *Proposed* rather than Accepted for two reasons, neither of which
is a measurement: the same numbers have not been taken on Windows or Linux,
and the decision itself belongs to the project owner rather than to whoever
ran the benchmark.

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
