# JT FileWork — Application Icon

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
  JTFileWork.icns          macOS, 16 … 512@2x
  JTFileWork.ico           Windows, 16 … 256
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
