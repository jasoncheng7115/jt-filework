#include "keyhintbar.h"

#include "jtfstring.h"

#include <QFontDatabase>
#include <QFontMetrics>
#include <QGraphicsOpacityEffect>
#include <QHBoxLayout>
#include <QPropertyAnimation>
#include <QTimer>
#include <QResizeEvent>
#include <QLabel>

namespace {

/// The commands worth offering, per context.
///
/// Ordered by how often a person reaches for them, not alphabetically: the
/// strip is read left to right and the first few are the ones that matter.
/// A command with no key in the active profile is skipped rather than shown
/// blank - in Native mode several of these are chords, and that is fine.
/// Copy and move are listed as the chooser forms, `file.copy_to` and
/// `file.move_to`, not the two-pane ones. Both do the same job, but in
/// Single-Key mode the chooser forms are the bare `C` and `M` while the
/// two-pane forms are `Ins` and `Shift-C` - and this strip leads with single
/// keys. In Native mode neither is bound, so both are skipped and nothing is
/// lost by the choice.
// `workspace.pane.next` is in every list. With two panes open it is the key
// reached for most often after the arrows - everything two-pane is "look at
// that one now" - and it was the one common key the strip never mentioned.
// It is dropped again when there is only one pane, because a hint for a key
// that does nothing teaches people to distrust the strip.
// `view.refresh` is deliberately absent. The strip is for the keys that do
// the work in front of you; refreshing is what you reach for when something
// already looks wrong, and it was spending width on every screen for that.
const char *const kNothing[] = {
    "file.new_folder",     "file.new_file", "view.filter",    "workspace.pane.next",
    "search.open",         "view.hidden",   "view.sort",      "view.tree",
    "nav.up",              "tab.new",       "view.inspector", "settings.open",
    nullptr,
};
const char *const kFile[] = {
    "preview.quicklook",
    "file.view",        "file.edit",           "file.copy_to",
    "file.move_to",     "file.rename",         "file.trash",
    "file.mark.toggle", "workspace.pane.next", "file.folder_size",
    "view.filter",      "search.open",         "file.new_folder",
    "view.sort",        nullptr,
};
const char *const kFolder[] = {
    "file.open",        "file.copy_to",        "file.move_to",
    "file.rename",      "file.trash",          "file.mark.toggle",
    "nav.up",           "workspace.pane.next", "file.folder_size",
    "view.filter",      "search.open",         "file.new_folder",
    "view.sort",        nullptr,
};
const char *const kSeveral[] = {
    "file.copy_to",      "file.move_to",        "file.trash",
    "file.batch_rename", "workspace.pane.next", "file.mark.none",
    "file.mark.invert",  "file.folder_size",    "file.mark.all",
    "view.sort",         nullptr,
};

const char *const *commandsFor(KeyHintBar::Context context) {
    switch (context) {
    case KeyHintBar::Context::File:
        return kFile;
    case KeyHintBar::Context::Folder:
        return kFolder;
    case KeyHintBar::Context::Several:
        return kSeveral;
    case KeyHintBar::Context::Nothing:
        break;
    }
    return kNothing;
}

/// The chip's own padding and border, which the font metrics do not know
/// about. Kept with the stylesheet rule it mirrors.
constexpr int kChipPadding = 26;

} // namespace

KeyHintBar::KeyHintBar(JtfApp *app, QWidget *parent) : QWidget(parent), m_app(app) {
    setObjectName(QStringLiteral("JtfKeyHints"));
    m_row = new QHBoxLayout(this);
    m_row->setContentsMargins(10, 3, 10, 3);
    m_row->setSpacing(14);
    m_row->addStretch(1);

    // Auto mode fades the strip while the list is being worked and brings it
    // back when the hands stop. Faded rather than hidden: a strip that
    // disappears takes the rows below it up the screen, and a list that jumps
    // while you are moving through it is worse than a strip you can see
    // through.
    m_fade = new QGraphicsOpacityEffect(this);
    m_fade->setOpacity(1.0);
    setGraphicsEffect(m_fade);
    m_fadeAnimation = new QPropertyAnimation(m_fade, "opacity", this);
    m_fadeAnimation->setDuration(180);
    m_idle = new QTimer(this);
    m_idle->setSingleShot(true);
    m_idle->setInterval(900);
    connect(m_idle, &QTimer::timeout, this, [this] { fadeTo(1.0); });
}

void KeyHintBar::setDensity(Density density) {
    if (density == m_density) {
        return;
    }
    m_density = density;
    if (m_density != Density::Auto) {
        m_idle->stop();
        fadeTo(1.0);
    }
    invalidate();
    rebuild(m_shown);
}

void KeyHintBar::noteActivity() {
    if (m_density != Density::Auto || !isVisible()) {
        return;
    }
    fadeTo(0.18);
    m_idle->start();
}

void KeyHintBar::fadeTo(qreal opacity) {
    if (m_fade == nullptr || qFuzzyCompare(m_fade->opacity(), opacity)) {
        return;
    }
    m_fadeAnimation->stop();
    m_fadeAnimation->setStartValue(m_fade->opacity());
    m_fadeAnimation->setEndValue(opacity);
    m_fadeAnimation->start();
}

QString KeyHintBar::tr_(const char *key) const {
    return jtfText([&](char *buf, int len) { return jtf_tr(m_app, key, buf, len); });
}

void KeyHintBar::invalidate() { m_valid = false; }

void KeyHintBar::applyTheme(const QColor &key, const QColor &label, const QColor &chip) {
    m_key = key;
    m_label = label;
    m_chip = chip;
    invalidate();
}

void KeyHintBar::update(Context context, bool severalPanes) {
    if (m_valid && context == m_shown && severalPanes == m_severalPanes
        && width() == m_builtForWidth) {
        return;
    }
    m_shown = context;
    m_severalPanes = severalPanes;
    m_valid = true;
    rebuild(context);
}

void KeyHintBar::resizeEvent(QResizeEvent *event) {
    QWidget::resizeEvent(event);
    // A wider strip can say more, so what it shows is recomputed when the
    // width changes - and only then. Rebuilding on every resize would be a
    // loop: adding hints changes what this widget asks for, which resizes it.
    if (width() != m_builtForWidth) {
        rebuild(m_shown);
    }
}

void KeyHintBar::rebuild(Context context) {
    if (m_rebuilding) {
        return;
    }
    m_rebuilding = true;
    m_builtForWidth = width();

    while (QLayoutItem *item = m_row->takeAt(0)) {
        if (QWidget *widget = item->widget()) {
            widget->hide();
            widget->setParent(nullptr);
            widget->deleteLater();
        }
        delete item;
    }

    // How much room there is to spend. The list is ordered by how often the
    // command is wanted for what the cursor is on, so filling from the front
    // means a narrow window keeps the useful ones and a wide one earns the
    // rest - rather than every window showing the same fixed few.
    const QMargins margins = m_row->contentsMargins();
    const int available = width() - margins.left() - margins.right();
    const QFontMetrics metrics(font());
    int used = 0;
    bool truncated = false;

    // Single keys first. This strip exists because CView put the keys you can
    // press right now along the bottom of the screen, and every one of those
    // was one keystroke; a row of platform chords teaches nothing a menu does
    // not already show, and it spends the width that the single keys need.
    // Within each group the order is unchanged, so the most useful command
    // for what the cursor is on is still leftmost.
    QList<QPair<QString, const char *>> ordered;
    QList<QPair<QString, const char *>> chords;
    for (const char *const *id = commandsFor(context); *id != nullptr; ++id) {
        // Nothing to switch to: the key is inert with one pane, so naming it
        // would be a promise the strip cannot keep.
        if (!m_severalPanes && qstrcmp(*id, "workspace.pane.next") == 0) {
            continue;
        }
        const QString shortcut =
            jtfText([&](char *buf, int len) { return jtf_shortcut_for(m_app, *id, buf, len); });
        if (shortcut.isEmpty()) {
            continue;
        }
        // A chord is anything the keymap spells with a modifier.
        if (shortcut.contains(QLatin1Char('+'))) {
            chords.append({shortcut, *id});
        } else {
            ordered.append({shortcut, *id});
        }
    }
    // In Single-Key mode the chords are not shown at all. The strip exists to
    // teach that profile's keyboard, and a platform chord in it is telling the
    // user about a way of working they have explicitly not chosen - while
    // spending width the single keys need. In Native mode the chords *are*
    // the keyboard, so there they are all there is.
    const QString profile =
        jtfText([&](char *buf, int len) { return jtf_keymap_name(m_app, buf, len); });
    if (profile == QLatin1String("native")) {
        ordered.append(chords);
    }

    for (const auto &entry : std::as_const(ordered)) {
        const QString &shortcut = entry.first;
        const char *const id = &*entry.second;
        QString label;
        if (m_density != Density::Compact) {
            // The short word for the command, which is what a strip wants: the
            // menu says「移到資源回收筒」because that is what the command does
            // and there is a「永久刪除」beside it, but a strip that has to fit
            // a dozen of these wants「刪除」. Falling back to the full name
            // rather than inventing an abbreviation - a truncated Chinese
            // command name is not shorter, it is wrong.
            const QByteArray shortKey =
                QStringLiteral("hint.short.%1").arg(QLatin1String(id)).toUtf8();
            const QString brief = jtfText(
                [&](char *buf, int len) { return jtf_tr(m_app, shortKey.constData(), buf, len); });
            if (!brief.startsWith(QLatin1String("hint.short."))) {
                label = brief;
            }
        }
        // Compact says nothing at all: the keys, and only the keys. Once the
        // names are known they are the part that is costing the width.
        if (label.isEmpty() && m_density != Density::Compact) {
            const QByteArray utf8 =
                QStringLiteral("command.%1").arg(QLatin1String(id)).toUtf8();
            label = jtfText(
                [&](char *buf, int len) { return jtf_tr(m_app, utf8.constData(), buf, len); });
        }

        // Measured before it is built, so a hint that would not fit is never
        // created rather than created and clipped.
        const QString keyText = QKeySequence(shortcut).toString(QKeySequence::NativeText);
        const int cost = metrics.horizontalAdvance(keyText) + metrics.horizontalAdvance(label) +
                         kChipPadding + m_row->spacing();
        if (used + cost > available) {
            truncated = true;
            break;
        }
        used += cost;

        auto *hint = new QWidget(this);
        auto *row = new QHBoxLayout(hint);
        row->setContentsMargins(0, 0, 0, 0);
        row->setSpacing(5);

        auto *key = new QLabel(keyText, hint);
        key->setProperty("jtfHintKey", true);
        // The key is set in a fixed-width face whatever the list is using.
        // A chip is a picture of a key on a keyboard, and keycaps are
        // fixed-width; in proportional type `I` and `W` make chips of wildly
        // different widths and the row stops reading as a row of keys.
        QFont keyFont = QFontDatabase::systemFont(QFontDatabase::FixedFont);
        keyFont.setPointSizeF(font().pointSizeF());
        keyFont.setBold(true);
        key->setFont(keyFont);
        auto *text = new QLabel(label, hint);
        text->setProperty("jtfHintLabel", true);
        row->addWidget(key);
        row->addWidget(text);
        m_row->addWidget(hint);
    }
    m_row->addStretch(1);
    // Say so rather than just stopping: a strip that quietly ends looks like
    // the list of what you can press, and it is not.
    if (truncated) {
        auto *more = new QLabel(QStringLiteral("…"), this);
        more->setProperty("jtfHintLabel", true);
        more->setToolTip(tr_("hints.more"));
        m_row->addWidget(more);
    }
    m_rebuilding = false;
}
