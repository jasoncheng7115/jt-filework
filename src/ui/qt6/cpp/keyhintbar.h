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

    /// How much the strip says.
    enum class Density {
        Full = 0,    ///< Key and the command's full name.
        Compact = 1, ///< Key and a short word, for when the names are known.
        Auto = 2,    ///< Full, but faded out of the way while the list is worked.
    };

    /// Rebuild for `context`. Cheap enough to call whenever selection moves;
    /// it does nothing when nothing that changes the strip has changed.
    ///
    /// `severalPanes` decides whether switching panes is offered: with one
    /// pane that key does nothing, and a strip that names it would be lying
    /// about what is available.
    void update(Context context, bool severalPanes);

    void setDensity(Density density);
    Density density() const { return m_density; }

    /// Tell the strip the user is working, so Auto can get out of the way.
    void noteActivity();

    void applyTheme(const QColor &key, const QColor &label, const QColor &chip);
    /// Forget what was shown, so the next update rebuilds. For a language or
    /// keymap change, where the context is the same but the words are not.
    void invalidate();

protected:
    void resizeEvent(class QResizeEvent *event) override;

private:
    void rebuild(Context context);
    QString tr_(const char *key) const;

    JtfApp *m_app = nullptr;
    QHBoxLayout *m_row = nullptr;
    Context m_shown = Context::Nothing;
    bool m_valid = false;
    bool m_severalPanes = false;
    Density m_density = Density::Full;
    class QTimer *m_idle = nullptr;
    class QGraphicsOpacityEffect *m_fade = nullptr;
    class QPropertyAnimation *m_fadeAnimation = nullptr;
    void fadeTo(qreal opacity);
    bool m_rebuilding = false;
    int m_builtForWidth = -1;
    QColor m_key, m_label, m_chip;
};
