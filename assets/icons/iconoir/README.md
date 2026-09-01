# Iconoir

The toolbar and chrome glyphs. Vendored from
[Iconoir](https://iconoir.com) ([source](https://github.com/iconoir-icons/iconoir)),
MIT licensed — see `LICENSE`, which ships with the application.

Only the icons the program actually uses are here, rather than all 1,671:
a vendored asset that nothing references is a licence obligation with no
benefit.

Every file is a 24×24 stroke icon at weight 1.5 using `stroke="currentColor"`,
which is what lets `glyph::make` recolour them from the theme tokens instead
of shipping a light and a dark copy of each.

To add one: take it from `icons/regular/` in the Iconoir repository, drop it
here, and name it in `kGlyphFiles` in `src/ui/qt6/cpp/icons.cpp`.
