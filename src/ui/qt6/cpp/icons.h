// Toolbar glyphs, rendered from Iconoir SVGs and tinted from theme tokens.
//
// QStyle::standardIcon returns whatever the platform style ships, which does
// not follow our palette: in a dark window the built-in arrows come out as
// dark shapes on a dark bar. AGENTS.md 12 says UI colour comes from tokens
// and docs/UI_CONVENTIONS.md 5 says that includes icons.
//
// The shapes come from Iconoir (MIT, assets/icons/iconoir) rather than being
// drawn by hand: one professionally drawn 24-unit set at a single stroke
// weight reads as one family, which a set of hand-rolled QPainterPaths never
// quite does. Iconoir strokes with `currentColor`, so one file serves both
// themes - the colour is substituted before rendering.
#pragma once

#include <QColor>
#include <QIcon>
#include <QString>

namespace glyph {

enum class Shape {
    ArrowLeft,
    ArrowRight,
    ArrowUp,
    Reload,
    Sidebar,
    SplitHorizontal,
    SplitVertical,
    SplitQuad,
    SplitSingle,
    NewFolder,
    Filter,
    Search,
    Hidden,
    Settings,
    Close,
    Inspector,
    Keyboard,
    HintBar,
    Visible,
    Home,
    Bookmark,
    Recent,
    Volume,
    Grid,
    List,
    Edit,
    Check,
    Copy,
    NewWindow,
    Connected,
    Eject,
    ArrowDown,
    SplitRows,
    Theme,
    Font,
    Language,
    FoldersFirst,
    SortMixed,
};

// Rendered at several sizes so the icon stays crisp on any display scale.
QIcon make(Shape shape, const QColor &colour);

// The icon for a command id, or a null icon when it has none.
//
// One table for the whole program, so a command carries the same picture in
// the menu, on the toolbar and in the palette. A command with no entry gets
// nothing rather than a placeholder: an approximate icon is worse than none,
// because it teaches the wrong association.
QIcon forCommand(const QString &id, const QColor &colour);

// A tinted glyph written out as a file, for a stylesheet's `url()`.
//
// Qt draws no arrow of its own once `QComboBox::drop-down` is styled - the
// stylesheet style takes over the whole control and has no image to draw - so
// every combo in the settings dialog read as a flat text box with no way to
// tell it opened. The arrow cannot come from `QIcon`: a stylesheet takes a
// path, not an object, and it cannot come from the SVG directly either,
// because those are stroked in `currentColor` and a stylesheet gives them no
// colour to inherit.
//
// So the tinted PNG is written once per shape, colour and size into the
// application's cache directory and the path handed to the sheet. Returns an
// empty string if it cannot be written, which leaves the sheet without an
// image rather than with a broken one.
QString stylesheetImage(Shape shape, const QColor &colour, int size);

// Whether a command has an icon at all.
bool hasCommandIcon(const QString &id);

} // namespace glyph
