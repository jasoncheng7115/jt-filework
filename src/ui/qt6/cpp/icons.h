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

namespace glyph {

enum class Shape {
    ArrowLeft,
    ArrowRight,
    ArrowUp,
    Reload,
    Sidebar,
    SplitHorizontal,
    SplitVertical,
    NewFolder,
    Filter,
    Search,
    Hidden,
    Settings,
    Close,
    Inspector,
    Keyboard,
    Home,
    Bookmark,
    Recent,
    Volume,
    Grid,
    List,
    Edit,
};

// Rendered at several sizes so the icon stays crisp on any display scale.
QIcon make(Shape shape, const QColor &colour);

} // namespace glyph
