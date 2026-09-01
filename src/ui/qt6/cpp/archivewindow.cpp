#include "archivewindow.h"

#include "jtfstring.h"
#include "iconprovider.h"
#include "listlook.h"
#include "panewidget.h"

#include <QFileInfo>
#include <QFontDatabase>
#include <QHBoxLayout>
#include <QHeaderView>
#include <QKeyEvent>
#include <QLabel>
#include <QTableWidget>
#include <QVBoxLayout>

ArchiveWindow::ArchiveWindow(JtfApp *app, const QString &archive, QWidget *parent)
    : QWidget(parent, Qt::Window), m_app(app), m_archive(archive) {
    setAttribute(Qt::WA_DeleteOnClose);
    setWindowTitle(QFileInfo(archive).fileName());
    resize(720, 520);

    auto *layout = new QVBoxLayout(this);
    layout->setContentsMargins(0, 0, 0, 0);
    layout->setSpacing(0);

    m_table = new QTableWidget(this);
    m_table->setColumnCount(2);
    m_table->setHorizontalHeaderLabels({tr_("column.name"), tr_("column.size")});
    // One row at a time, moved with the cursor, marked with Space - the same
    // way the file list works, because this is a list of entries and there is
    // no reason for it to behave differently from the other one. That goes for
    // how it looks as well: same row height, same font, same header, same tick.
    listlook::apply(m_table, font());
    listlook::applyTheme(m_table, palette().color(QPalette::Text),
                         palette().color(QPalette::PlaceholderText),
                         palette().color(QPalette::HighlightedText));
    m_table->setSelectionMode(QAbstractItemView::SingleSelection);
    m_table->horizontalHeader()->setStretchLastSection(false);
    m_table->horizontalHeader()->setSectionResizeMode(0, QHeaderView::Stretch);
    m_table->horizontalHeader()->setSectionResizeMode(1, QHeaderView::Fixed);
    m_table->horizontalHeader()->resizeSection(1, 120);
    connect(m_table, &QTableWidget::itemChanged, this, [this] { updateStatus(); });
    layout->addWidget(m_table, 1);

    // The same strip of key chips the main window uses, rather than a
    // sentence: this is a keyboard window and the keys are the interface.
    m_hints = new QWidget(this);
    m_hints->setObjectName(QStringLiteral("JtfKeyHints"));
    auto *hintRow = new QHBoxLayout(m_hints);
    hintRow->setContentsMargins(10, 5, 10, 5);
    hintRow->setSpacing(14);
    const auto addHint = [this, hintRow](const QString &keyText, const char *labelKey) {
        auto *pair = new QWidget(m_hints);
        auto *row = new QHBoxLayout(pair);
        row->setContentsMargins(0, 0, 0, 0);
        row->setSpacing(5);
        auto *key = new QLabel(keyText, pair);
        key->setProperty("jtfHintKey", true);
        QFont keyFont = QFontDatabase::systemFont(QFontDatabase::FixedFont);
        keyFont.setPointSizeF(font().pointSizeF());
        keyFont.setBold(true);
        key->setFont(keyFont);
        auto *text = new QLabel(tr_(labelKey), pair);
        text->setProperty("jtfHintLabel", true);
        row->addWidget(key);
        row->addWidget(text);
        hintRow->addWidget(pair);
    };
    addHint(tr_("key.space"), "archive.key_mark");
    addHint(QStringLiteral("C"), "archive.key_extract_marked");
    addHint(QStringLiteral("X"), "archive.key_extract_all");
    addHint(tr_("key.escape"), "archive.key_close");
    hintRow->addStretch(1);

    m_status = new QLabel(m_hints);
    m_status->setProperty("jtfHintLabel", true);
    hintRow->addWidget(m_status);
    layout->addWidget(m_hints);

    const QByteArray utf8 = archive.toUtf8();
    const int count = jtf_open_archive_listing(m_app, utf8.constData());
    m_readable = count > 0;
    m_table->setRowCount(count);

    int unsafeCount = 0;
    for (int row = 0; row < count; ++row) {
        const QString name =
            jtfText([&](char *b, int l) { return jtf_archive_entry_name(m_app, row, b, l); });
        const bool directory = jtf_archive_entry_is_directory(m_app, row) != 0;
        const bool escapes = jtf_archive_entry_is_unsafe(m_app, row) != 0;
        if (escapes) {
            ++unsafeCount;
        }

        // An icon per row, as the file list has: a folder inside an archive
        // should look like a folder. Asked of the same provider the list uses,
        // by name only - nothing inside a container exists on disk to ask
        // about, and the extension is what the icon follows anyway.
        auto *nameItem = new QTableWidgetItem(m_icons.iconFor(name, directory), name);
        // Marks, on the name column, exactly as the file list puts them.
        nameItem->setFlags(nameItem->flags() | Qt::ItemIsUserCheckable);
        nameItem->setCheckState(Qt::Unchecked);
        if (escapes) {
            // Marked, not hidden. A member whose path leads outside the
            // destination is the single most interesting thing an archive can
            // contain, and extraction is going to refuse it - saying so here
            // is why the listing exists at all.
            nameItem->setToolTip(tr_("archive.unsafe_name"));
            QFont font = nameItem->font();
            font.setItalic(true);
            nameItem->setFont(font);
        }
        m_table->setItem(row, 0, nameItem);

        auto *sizeItem = new QTableWidgetItem(
            directory ? QString() : PaneWidget::formatSize(jtf_archive_entry_size(m_app, row)));
        sizeItem->setTextAlignment(Qt::AlignRight | Qt::AlignVCenter);
        m_table->setItem(row, 1, sizeItem);
    }

    m_unsafeCount = unsafeCount;
    updateStatus();

    m_table->installEventFilter(this);
    if (count > 0) {
        m_table->selectRow(0);
    }
    m_table->setFocus();
}

void ArchiveWindow::updateStatus() {
    QString status = jtfFill(tr_("archive.entries"), "count",
                             QString::number(m_table->rowCount()));
    const int marked = markedRows().size();
    if (marked > 0) {
        status += QStringLiteral("   ")
                  + jtfFill(tr_("archive.marked"), "count", QString::number(marked));
    }
    if (m_unsafeCount > 0) {
        status += QStringLiteral("   ")
                  + jtfFill(tr_("archive.unsafe_count"), "count",
                            QString::number(m_unsafeCount));
    }
    m_status->setText(status);
}

QList<int> ArchiveWindow::markedRows() const {
    QList<int> rows;
    for (int row = 0; row < m_table->rowCount(); ++row) {
        if (auto *item = m_table->item(row, 0); item != nullptr
                                                && item->checkState() == Qt::Checked) {
            rows.append(row);
        }
    }
    return rows;
}

ArchiveWindow::~ArchiveWindow() { jtf_close_archive_listing(m_app); }

QString ArchiveWindow::tr_(const char *key) const {
    return jtfText([&](char *buf, int len) { return jtf_tr(m_app, key, buf, len); });
}

QStringList ArchiveWindow::selectedMembers() const {
    // Marks first, the cursor row otherwise - the same rule the file list
    // resolves an operation's target by, so `C` here means what `C` means
    // there.
    QList<int> rows = markedRows();
    if (rows.isEmpty()) {
        const int current = m_table->currentRow();
        if (current >= 0) {
            rows.append(current);
        }
    }
    QStringList members;
    members.reserve(rows.size());
    for (const int row : rows) {
        if (auto *item = m_table->item(row, 0)) {
            members.append(item->text());
        }
    }
    return members;
}

bool ArchiveWindow::eventFilter(QObject *watched, QEvent *event) {
    // `keyPressEvent` on the window only runs for keys the focus widget did
    // not want, and the table wants Space, C and X - Space toggles a check on
    // its own terms and the letters are type-to-find. Taken here instead, so
    // the window's keys mean what the strip at the bottom says they mean.
    if (watched == m_table && event->type() == QEvent::KeyPress) {
        auto *key = static_cast<QKeyEvent *>(event);
        switch (key->key()) {
        case Qt::Key_Space:
        case Qt::Key_C:
        case Qt::Key_X:
        case Qt::Key_Escape:
            keyPressEvent(key);
            return true;
        default:
            break;
        }
    }
    return QWidget::eventFilter(watched, event);
}

void ArchiveWindow::keyPressEvent(QKeyEvent *event) {
    // §四's keys. `C` for the selection and `X` for everything, which is the
    // same distinction `C` and `X` make in CView itself.
    switch (event->key()) {
    case Qt::Key_Escape:
        close();
        return;
    case Qt::Key_Space: {
        // Mark and move on, as Space does in the file list.
        const int row = m_table->currentRow();
        if (auto *item = row >= 0 ? m_table->item(row, 0) : nullptr) {
            item->setCheckState(item->checkState() == Qt::Checked ? Qt::Unchecked : Qt::Checked);
            if (row + 1 < m_table->rowCount()) {
                m_table->selectRow(row + 1);
            }
        }
        return;
    }
    case Qt::Key_C: {
        const QStringList members = selectedMembers();
        if (!members.isEmpty()) {
            emit extractRequested(m_archive, members);
        }
        return;
    }
    case Qt::Key_X:
        emit extractRequested(m_archive, {});
        return;
    default:
        break;
    }
    QWidget::keyPressEvent(event);
}
