// Toolbar glyphs drawn from theme tokens.
//
// QStyle::standardIcon returns whatever the platform style ships, which does
// not follow our palette: in a dark window the built-in arrows come out as
// dark shapes on a dark bar. AGENTS.md 12 says UI colour comes from tokens,
// and that has to include icons, so these are drawn.
#pragma once

#include <QColor>
#include <QIcon>

namespace glyph {

enum class Shape { ArrowLeft, ArrowRight, ArrowUp, Reload };

// Rendered at several sizes so the icon stays crisp on any display scale.
QIcon make(Shape shape, const QColor &colour);

} // namespace glyph
