// Small helper for the buffer-based string convention the bridge uses.
#pragma once

#include "bridge.h"

#include <QString>
#include <vector>

// Substitutes a {name} placeholder. The catalogue format uses {name}
// (docs/I18N_THEME.md), not Qt's %1, and a translated string must never be
// assembled from fragments (AGENTS.md 11) - so this replaces a whole named
// slot rather than concatenating anything.
inline QString jtfFill(QString text, const char *name, const QString &value) {
    return text.replace(QStringLiteral("{%1}").arg(QLatin1String(name)), value);
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
