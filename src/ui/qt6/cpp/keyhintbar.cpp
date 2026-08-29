#include "keyhintbar.h"

#include "jtfstring.h"

#include <QHBoxLayout>
#include <QLabel>

namespace {

/// The commands worth offering, per context.
///
/// Ordered by how often a person reaches for them, not alphabetically: the
/// strip is read left to right and the first few are the ones that matter.
/// A command with no key in the active profile is skipped rather than shown
/// blank - in Native mode several of these are chords, and that is fine.
const char *const kNothing[] = {
    "file.new_folder", "view.refresh", "view.filter", "search.open", "view.hidden", nullptr,
};
const char *const kFile[] = {
    "file.view",   "file.edit",  "file.copy_to_target_pane", "file.move_to_target_pane",
    "file.rename", "file.trash", "file.mark.toggle",         "file.folder_size",
    nullptr,
};
const char *const kFolder[] = {
    "file.open",   "file.copy_to_target_pane", "file.move_to_target_pane", "file.rename",
    "file.trash",  "file.mark.toggle",         "file.folder_size",         nullptr,
};
const char *const kSeveral[] = {
    "file.copy_to_target_pane", "file.move_to_target_pane", "file.trash",
    "file.batch_rename",        "file.mark.none",           "file.mark.invert",
    "file.folder_size",         nullptr,
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

} // namespace

KeyHintBar::KeyHintBar(JtfApp *app, QWidget *parent) : QWidget(parent), m_app(app) {
    setObjectName(QStringLiteral("JtfKeyHints"));
    m_row = new QHBoxLayout(this);
    m_row->setContentsMargins(10, 3, 10, 3);
    m_row->setSpacing(14);
    m_row->addStretch(1);
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

void KeyHintBar::update(Context context) {
    if (m_valid && context == m_shown) {
        return;
    }
    m_shown = context;
    m_valid = true;
    rebuild(context);
}

void KeyHintBar::rebuild(Context context) {
    while (QLayoutItem *item = m_row->takeAt(0)) {
        if (QWidget *widget = item->widget()) {
            widget->hide();
            widget->setParent(nullptr);
            widget->deleteLater();
        }
        delete item;
    }

    for (const char *const *id = commandsFor(context); *id != nullptr; ++id) {
        const QString shortcut =
            jtfText([&](char *buf, int len) { return jtf_shortcut_for(m_app, *id, buf, len); });
        if (shortcut.isEmpty()) {
            continue;
        }
        const QString labelKey = QStringLiteral("command.%1").arg(QLatin1String(*id));
        const QByteArray utf8 = labelKey.toUtf8();
        const QString label =
            jtfText([&](char *buf, int len) { return jtf_tr(m_app, utf8.constData(), buf, len); });

        auto *hint = new QWidget(this);
        auto *row = new QHBoxLayout(hint);
        row->setContentsMargins(0, 0, 0, 0);
        row->setSpacing(5);

        auto *key = new QLabel(QKeySequence(shortcut).toString(QKeySequence::NativeText), hint);
        key->setProperty("jtfHintKey", true);
        auto *text = new QLabel(label, hint);
        text->setProperty("jtfHintLabel", true);
        row->addWidget(key);
        row->addWidget(text);
        m_row->addWidget(hint);
    }
    m_row->addStretch(1);
}
