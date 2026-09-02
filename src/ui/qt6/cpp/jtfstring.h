// Small helper for the buffer-based string convention the bridge uses.
#pragma once

#include "bridge.h"

#include <QKeySequence>
#include <QString>
#include <vector>

// Substitutes a {name} placeholder. The catalogue format uses {name}
// (docs/I18N_THEME.md), not Qt's %1, and a translated string must never be
// assembled from fragments (AGENTS.md 11) - so this replaces a whole named
// slot rather than concatenating anything.
inline QString jtfFill(QString text, const char *name, const QString &value) {
    return text.replace(QStringLiteral("{%1}").arg(QLatin1String(name)), value);
}

// A shortcut spelled the way the platform this is running on spells it.
//
// The core stores a *portable* chord - "Ctrl+2", "Alt+R" - which is what
// QKeySequence parses and what Qt maps onto the platform accelerator: on macOS
// that Ctrl is Command. Printing the portable string as-is therefore named a
// key that does not work, and the tooltip on the list-view button said
// "(Ctrl+2)" for something only ⌘2 answers to. Menu items were always right
// because Qt renders those from the QKeySequence itself; it is only the places
// that print the text by hand that were wrong, and there were six of them.
inline QString jtfShortcutText(const QString &portable) {
    if (portable.isEmpty()) {
        return portable;
    }
    return QKeySequence(portable).toString(QKeySequence::NativeText);
}

// Calls a bridge function that writes UTF-8 into a caller buffer and returns
// the length it needed. Grows once if the first attempt was truncated, so a
// long path costs at most two calls and the common case costs none.
template <typename Fn> inline QString jtfText(Fn fn) {
    char stack[512];
    const int needed = fn(stack, static_cast<int>(sizeof(stack)));
    if (needed < static_cast<int>(sizeof(stack))) {
        return QString::fromUtf8(stack, needed < 0 ? 0 : needed);
    }
    std::vector<char> heap(static_cast<size_t>(needed) + 1);
    const int written = fn(heap.data(), static_cast<int>(heap.size()));
    return QString::fromUtf8(heap.data(), written);
}
