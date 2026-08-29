// Turns semantic tokens into a Qt stylesheet.
//
// AGENTS.md 12: UI code consumes tokens, never colours. This header is where
// tokens become pixels, and it is the only place in the C++ that names a
// colour at all - by asking Rust for one.
#pragma once

#include "bridge.h"

#include <QColor>
#include <QString>

struct Theme {
    QColor window, pane, preview, header, rowAlternate, rowHover;
    QColor textPrimary, textSecondary, textOnAccent;
    QColor border, selection, selectionInactive, mark, focusRing, indicator, executable;
    QColor error;

    static Theme fromApp(const JtfApp *app, bool systemIsDark);

    // The whole application look, in one sheet. Metrics live here too so
    // spacing stays consistent instead of being scattered across widgets.
    QString styleSheet() const;
};
