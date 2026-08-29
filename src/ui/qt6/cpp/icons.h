// Toolbar glyphs drawn from theme tokens.
//
// QStyle::standardIcon returns whatever the platform style ships, which does
// not follow our palette: in a dark window the built-in arrows come out as
// dark shapes on a dark bar. AGENTS.md 12 says UI colour comes from tokens,
// and docs/UI_CONVENTIONS.md 5 says that includes icons, so these are drawn.
//
// They are drawn on a 16-unit grid with a single stroke weight, so a toolbar
// of them reads as one set rather than as a collection.
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
};

// Rendered at several sizes so the icon stays crisp on any display scale.
QIcon make(Shape shape, const QColor &colour);

} // namespace glyph
