// The key hint strip, above the status bar.
//
// CView keeps a line of "C拷貝 D刪除 M移動 R改名 …" at the foot of the screen,
// and it is why nobody needed a manual: the keys you can press right now are
// on screen, and they change with what the cursor is on.
//
// Every hint is assembled from the live keymap and the catalogue, never from
// a written list. A hard-coded strip would be wrong the moment somebody
// rebound a key or switched profile, and wrong silently.
#pragma once

#include "bridge.h"

#include <QColor>
#include <QWidget>

class QHBoxLayout;

class KeyHintBar : public QWidget {
    Q_OBJECT

public:
    explicit KeyHintBar(JtfApp *app, QWidget *parent = nullptr);

    /// What the cursor is on, which decides which hints are useful.
    enum class Context { Nothing, File, Folder, Several };

    /// Rebuild for `context`. Cheap enough to call whenever selection moves;
    /// it does nothing when the context and the keymap have not changed.
    void update(Context context);

    void applyTheme(const QColor &key, const QColor &label, const QColor &chip);
    /// Forget what was shown, so the next update rebuilds. For a language or
    /// keymap change, where the context is the same but the words are not.
    void invalidate();

private:
    void rebuild(Context context);
    QString tr_(const char *key) const;

    JtfApp *m_app = nullptr;
    QHBoxLayout *m_row = nullptr;
    Context m_shown = Context::Nothing;
    bool m_valid = false;
    QColor m_key, m_label, m_chip;
};
