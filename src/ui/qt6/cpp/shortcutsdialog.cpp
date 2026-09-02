#include "shortcutsdialog.h"

#include "icons.h"
#include "dialogbuttons.h"

#include "jtfstring.h"

#include <QDialogButtonBox>
#include <QHash>
#include <QHeaderView>
#include <QLabel>
#include <QLineEdit>
#include <QTreeWidget>
#include <QVBoxLayout>

ShortcutsDialog::ShortcutsDialog(JtfApp *app, QWidget *parent)
    : QDialog(parent), m_app(app) {
    setObjectName(QStringLiteral("JtfShortcuts"));
    setWindowTitle(tr_("shortcuts.title"));
    resize(560, 620);

    auto *layout = new QVBoxLayout(this);
    layout->setContentsMargins(14, 14, 14, 14);
    layout->setSpacing(10);

    auto *mode = new QLabel(this);
    const QString keymap =
        jtfText([&](char *b, int l) { return jtf_keymap_name(m_app, b, l); });
    // `single-key` names a file; `keyboard.profile.single_key` names a
    // catalogue entry - one prefix and one underscore apart. This built
    // `keymap.single-key`, which is in no catalogue, so the line read
    // "鍵盤模式：keymap.single-key".
    QString profileKey = QStringLiteral("keyboard.profile.%1").arg(keymap);
    profileKey.replace(QLatin1Char('-'), QLatin1Char('_'));
    mode->setText(
        jtfFill(tr_("shortcuts.mode"), "name", tr_(profileKey.toUtf8().constData())));
    mode->setProperty("jtfFactLabel", true);
    layout->addWidget(mode);

    m_search = new QLineEdit(this);
    m_search->setClearButtonEnabled(true);
    m_search->setPlaceholderText(tr_("shortcuts.filter"));
    layout->addWidget(m_search);

    m_tree = new QTreeWidget(this);
    m_tree->setObjectName(QStringLiteral("JtfShortcutsTree"));
    m_tree->setColumnCount(2);
    m_tree->setHeaderLabels({tr_("shortcuts.command"), tr_("shortcuts.key")});
    m_tree->setRootIsDecorated(false);
    m_tree->setIndentation(10);
    m_tree->setAlternatingRowColors(true);
    m_tree->header()->setSectionResizeMode(0, QHeaderView::Stretch);
    m_tree->header()->setSectionResizeMode(1, QHeaderView::ResizeToContents);
    layout->addWidget(m_tree, 1);

    auto *buttons = new QDialogButtonBox(QDialogButtonBox::Close, this);
    connect(buttons, &QDialogButtonBox::rejected, this, &QDialog::reject);
    dialogs::localizeButtons(buttons, [this](const char *key) { return tr_(key); }, palette().color(QPalette::Text));
    layout->addWidget(buttons);

    connect(m_search, &QLineEdit::textChanged, this, &ShortcutsDialog::rebuild);
    rebuild(QString());
    m_search->setFocus();
}

QString ShortcutsDialog::tr_(const char *key) const {
    return jtfText([&](char *buf, int len) { return jtf_tr(m_app, key, buf, len); });
}

void ShortcutsDialog::rebuild(const QString &needle) {
    m_tree->clear();
    const QString filter = needle.trimmed().toLower();

    // Grouped by the registry's own categories, so the list reads as
    // "navigation, files, view" rather than as one alphabetical wall.
    QHash<QString, QTreeWidgetItem *> groups;
    const auto groupFor = [&](const QString &category) {
        auto it = groups.find(category);
        if (it != groups.end()) {
            return *it;
        }
        auto *group = new QTreeWidgetItem(m_tree);
        group->setText(0, category);
        group->setFirstColumnSpanned(true);
        group->setFlags(Qt::ItemIsEnabled);
        QFont bold = group->font(0);
        bold.setBold(true);
        group->setFont(0, bold);
        group->setExpanded(true);
        groups.insert(category, group);
        return group;
    };

    const int count = jtf_command_count(m_app);
    for (int i = 0; i < count; ++i) {
        char idBuf[128];
        char labelBuf[256];
        char categoryBuf[128];
        jtf_command_at(m_app, i, idBuf, sizeof(idBuf), labelBuf, sizeof(labelBuf), categoryBuf,
                       sizeof(categoryBuf));
        const QString id = QString::fromUtf8(idBuf);
        // `jtf_command_at` hands back catalogue *keys*, not text - the same
        // contract the command palette works to. Showing them unlocalized is
        // how this dialog came to list `command.file.attributes` instead of
        // 屬性, in every language including English.
        const QString label = tr_(labelBuf);
        const QString category = tr_(categoryBuf);
        const QByteArray idUtf8 = id.toUtf8();
        const QString shortcut = jtfShortcutText(jtfText(
            [&](char *b, int l) { return jtf_shortcut_for(m_app, idUtf8.constData(), b, l); }));

        // A command with no key is still worth listing: "this exists and has
        // no shortcut" is an answer, and it is where a user goes to pick one.
        if (!filter.isEmpty() && !label.toLower().contains(filter) &&
            !shortcut.toLower().contains(filter) && !id.contains(filter)) {
            continue;
        }
        auto *item = new QTreeWidgetItem(groupFor(category));
        item->setText(0, label);
        // The same picture the command wears in the menus. A list of every
        // command is exactly where knowing what a command looks like is worth
        // something: it is how you connect the row you are reading to the
        // entry you have seen on the toolbar.
        if (glyph::hasCommandIcon(id)) {
            item->setIcon(0, glyph::forCommand(id, palette().color(QPalette::Text)));
        }
        item->setText(1, shortcut.isEmpty() ? tr_("shortcuts.unbound") : shortcut);
        item->setToolTip(0, id);
        if (shortcut.isEmpty()) {
            item->setForeground(1, palette().brush(QPalette::Disabled, QPalette::Text));
        }
    }
}
