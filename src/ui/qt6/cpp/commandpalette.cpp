#include "commandpalette.h"
#include "jtfstring.h"

#include <QCoreApplication>
#include <QKeyEvent>
#include <QLineEdit>
#include <QListWidget>
#include <QVBoxLayout>

namespace {

// A subsequence match, which is what people expect from a palette: "cptp"
// finds "Copy to Other Pane". Scored so that earlier and tighter matches sort
// first, because a palette that finds everything in no useful order is a list.
bool fuzzyScore(const QString &needle, const QString &haystack, int *score) {
    if (needle.isEmpty()) {
        *score = 0;
        return true;
    }
    int at = 0;
    int total = 0;
    int previous = -2;
    for (const QChar &wanted : needle) {
        const int found = haystack.indexOf(wanted, at, Qt::CaseInsensitive);
        if (found < 0) {
            return false;
        }
        // Adjacent characters and word starts are worth more.
        total += (found == previous + 1) ? 8 : 0;
        if (found == 0 || haystack.at(found - 1).isSpace()) {
            total += 6;
        }
        total -= found - at;
        previous = found;
        at = found + 1;
    }
    *score = total;
    return true;
}

} // namespace

CommandPalette::CommandPalette(JtfApp *app, QWidget *parent)
    : QDialog(parent, Qt::Popup), m_app(app) {
    resize(560, 400);

    auto *layout = new QVBoxLayout(this);
    layout->setContentsMargins(8, 8, 8, 8);
    layout->setSpacing(6);

    m_search = new QLineEdit(this);
    m_search->setPlaceholderText(jtfText(
        [&](char *buf, int len) { return jtf_tr(m_app, "palette.placeholder", buf, len); }));
    layout->addWidget(m_search);

    m_list = new QListWidget(this);
    m_list->setUniformItemSizes(true);
    layout->addWidget(m_list, 1);

    connect(m_search, &QLineEdit::textChanged, this, &CommandPalette::filter);
    connect(m_list, &QListWidget::itemActivated, this, [this] { accept_(); });

    // Arrows and Enter belong to the list even while the field has focus,
    // which is what makes a palette usable without reaching for the mouse.
    m_search->installEventFilter(this);
    m_search->setFocus();

    load();
    filter(QString());
}

void CommandPalette::load() {
    const int count = jtf_command_count(m_app);
    m_entries.reserve(count);

    for (int i = 0; i < count; ++i) {
        char id[128] = {};
        char label[128] = {};
        char category[128] = {};
        if (!jtf_command_at(m_app, i, id, sizeof(id), label, sizeof(label), category,
                            sizeof(category))) {
            continue;
        }
        const auto translate = [&](const char *key) {
            return jtfText([&](char *buf, int len) { return jtf_tr(m_app, key, buf, len); });
        };
        Entry entry;
        entry.id = QString::fromUtf8(id);
        entry.label = translate(label);
        entry.category = translate(category);
        entry.shortcut =
            jtfShortcutText(
            jtfText([&](char *buf, int len) { return jtf_shortcut_for(m_app, id, buf, len); }));
        m_entries.append(entry);
    }
}

void CommandPalette::filter(const QString &needle) {
    QVector<QPair<int, const Entry *>> matches;
    for (const Entry &entry : std::as_const(m_entries)) {
        int score = 0;
        // Match against the category too, so "view split" finds it the way
        // people actually remember commands.
        const QString haystack = entry.category + QLatin1Char(' ') + entry.label;
        if (fuzzyScore(needle, haystack, &score)) {
            matches.append({score, &entry});
        }
    }
    std::stable_sort(matches.begin(), matches.end(),
                     [](const auto &a, const auto &b) { return a.first > b.first; });

    m_list->clear();
    for (const auto &match : std::as_const(matches)) {
        const Entry *entry = match.second;
        QString text = entry->category + QStringLiteral("  ·  ") + entry->label;
        if (!entry->shortcut.isEmpty()) {
            text += QStringLiteral("      ") + entry->shortcut;
        }
        auto *item = new QListWidgetItem(text, m_list);
        item->setData(Qt::UserRole, entry->id);
    }
    if (m_list->count() > 0) {
        m_list->setCurrentRow(0);
    }
}

void CommandPalette::accept_() {
    if (QListWidgetItem *item = m_list->currentItem()) {
        m_chosen = item->data(Qt::UserRole).toString();
    }
    accept();
}

bool CommandPalette::eventFilter(QObject *watched, QEvent *event) {
    if (watched == m_search && event->type() == QEvent::KeyPress) {
        auto *key = static_cast<QKeyEvent *>(event);
        switch (key->key()) {
        case Qt::Key_Down:
        case Qt::Key_Up:
        case Qt::Key_PageDown:
        case Qt::Key_PageUp:
            QCoreApplication::sendEvent(m_list, event);
            return true;
        case Qt::Key_Return:
        case Qt::Key_Enter:
            accept_();
            return true;
        default:
            break;
        }
    }
    return QDialog::eventFilter(watched, event);
}
