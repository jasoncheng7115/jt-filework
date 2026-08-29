# jt-filework — Application Icon

## The mark

The icon is the product's own architecture rather than a generic folder:

- a **recursive split workspace** (`AGENTS.md` §6) — one tall pane beside a
  pair that is itself split, so the shape says "nesting", not "two panes"
- one **active pane**, opaque and bright, the others dimmed
  (`docs/UI_UX_SPEC.md` §3.1)
- one **selection** bar and one **mark** dot in its own gutter, because
  `AGENTS.md` §10 makes those two different things and the icon should not
  blur what the product refuses to blur

Accent colours are taken from the dark palette in `src/core/src/theme.rs`:

| Element | Token | Value |
|---|---|---|
| Selection bar | `selection.active` | `#2F6FC4` |
| Mark dot | `mark.active` | `#E88A3C` |

If those tokens change, change them here too.

## Originality and licensing

The artwork is **original**, authored directly as SVG source in this
repository. Nothing here is traced from, derived from, or copied out of
another product's icon; no third-party icon set, stock asset, clip art, AI
image generator output, or embedded font is used. Every shape is a rectangle,
circle or gradient written by hand, so the only copyright in the file is the
project's own and it ships under the repository licence
(`GPL-3.0-or-later`).

Copyright is therefore not the exposure. The one worth checking is **trade
dress**: an icon can be independently drawn and still be a problem if it is
confusingly similar to an established competitor's.

### Comparison performed — 2026-08-29

Checked against the icons of: QSpace, Commander One, Commander One PRO,
Nimble Commander, ForkLift, Marta, File Cabinet Pro, Folder Hub, Clover.

Result: no resemblance to any of them **except one family trait**. Commander
One and Commander One PRO are a squircle divided 50/50 into two flat columns
with list rows on *both* sides, bright blue and bright purple respectively.
An early draft of this icon also put rows on both sides, which put it in the
same visual family.

That was changed rather than argued about. In the current artwork only the
**active** pane carries a file list; the inactive panes carry their own tab
strips and nothing else. This is both further from Commander One and closer
to what the product actually is: not "two lists side by side", but a
workspace of panes where each owns its tabs and exactly one is active.

Remaining differences from the nearest neighbour:

| | Commander One | jt-filework |
|---|---|---|
| Ground | bright blue / purple, edge to edge | deep navy, panels inset on it |
| Division | symmetric 50/50, two columns | asymmetric, one pane beside a nested pair |
| Content | rows on both sides | rows on the active pane only |
| Panels | flat halves | floating cards with shadow |
| Accents | none | selection pill, mark dot in a gutter |
| Tabs | not shown | a tab strip per pane |

### If the icon changes

Re-run this comparison before shipping a new design, and update this section
with the date and what was checked. `TODO.md` tracks a trademark search
before first public release, which is a separate question from this one and
is not something a visual check can answer.

## Three masters, not one

An icon that is one drawing scaled down is unreadable at 16px. Each master is
drawn for the sizes it serves:

| File | Serves | What it drops |
|---|---|---|
| `jt-filework.svg` | ≥ 65px | nothing — full list, gutter, sheen, shadow |
| `jt-filework-32.svg` | 17–64px | most rows; wider gaps, chunkier bars |
| `jt-filework-16.svg` | ≤ 16px | rows and the mark dot; silhouette only |

At 16px a mark dot would be a fraction of a pixel, so it is dropped rather
than rendered as noise. The single blue bar stays: it is what makes the shape
read as a file list instead of three blocks.

All three are 1024×1024 with the macOS content square inset, so every size
comes from a clean vector render rather than a resample.

## Building

```bash
./assets/icon/build-icons.sh
```

Requires `rsvg-convert` (librsvg) and ImageMagick; `iconutil` on macOS for
the `.icns`.

Outputs into `assets/icon/generated/`, which is **not** committed
(`AGENTS.md` §2: no build artifacts in Git):

```text
generated/
  jt-filework.icns          macOS, 16 … 512@2x
  jt-filework.ico           Windows, 16 … 256
  png/jt-filework-<n>.png  16 24 32 48 64 128 256 512 1024
  contact-sheet.png        review artefact: 512 next to 128/64/32/16 upscaled
```

## Reviewing a change

Regenerate and look at `contact-sheet.png`. It shows the large rendering
beside the small sizes point-upscaled, which is the only honest way to judge
whether 16px and 32px still read. A change that looks better at 512 and worse
at 16 is not an improvement.

Still to do (`TODO.md`): a monochrome template variant for the macOS menu bar
and any toolbar use, and a check that the icon remains legible against both
light and dark Dock/taskbar backgrounds (`docs/TESTING.md` §10).
