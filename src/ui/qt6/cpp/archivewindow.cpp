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

    // Where in the archive the listing is. The window title names the archive
    // and stops there, which was fine while the listing was flat and is not
    // once it can be three folders deep.
    m_where = new QLabel(this);
    m_where->setObjectName(QStringLiteral("JtfArchiveWhere"));
    m_where->setTextFormat(Qt::PlainText);
    m_where->setContentsMargins(12, 7, 12, 7);
    layout->addWidget(m_where);

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
    connect(m_table, &QTableWidget::itemChanged, this, [this](QTableWidgetItem *item) {
        // A tick made with the mouse. The keyboard path sets the mark itself;
        // this catches the other one, and both end in the same set.
        if (item == nullptr || item->column() != 0) {
            return;
        }
        const QString path = item->data(Qt::UserRole + 1).toString();
        if (path.isEmpty()) {
            return;
        }
        setMarked(path, item->data(Qt::UserRole + 2).toBool(),
                  item->checkState() == Qt::Checked);
        updateStatus();
    });
    connect(m_table, &QTableWidget::doubleClicked, this, [this](const QModelIndex &index) {
        if (!rowIsDirectory(index.row())) {
            return;
        }
        const QString path = pathOf(index.row());
        if (path.isEmpty()) {
            ascend();
        } else {
            descend(path);
        }
    });
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
    addHint(tr_("key.enter"), "archive.key_enter");
    addHint(tr_("key.backspace"), "archive.key_up");
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

    int unsafeCount = 0;
    m_members.reserve(count);
    for (int row = 0; row < count; ++row) {
        Member member;
        member.path =
            jtfText([&](char *b, int l) { return jtf_archive_entry_name(m_app, row, b, l); });
        member.directory = jtf_archive_entry_is_directory(m_app, row) != 0;
        member.unsafe = jtf_archive_entry_is_unsafe(m_app, row) != 0;
        member.size = member.directory ? 0 : jtf_archive_entry_size(m_app, row);
        if (member.unsafe) {
            ++unsafeCount;
        }
        m_members.append(member);
    }

    m_unsafeCount = unsafeCount;
    populate();

    m_table->installEventFilter(this);
    m_table->setFocus();
}

namespace {

/// The part of `path` below `prefix`, or nothing if it is not below it.
QString below(const QString &path, const QString &prefix) {
    if (!path.startsWith(prefix)) {
        return QString();
    }
    return path.mid(prefix.size());
}

/// Strip one trailing slash, which is how archives spell a directory.
QString withoutSlash(const QString &path) {
    return path.endsWith(QLatin1Char('/')) ? path.left(path.size() - 1) : path;
}

/// The last component of a stored path.
QString leafOf(const QString &path) {
    const QString trimmed = withoutSlash(path);
    const int slash = trimmed.lastIndexOf(QLatin1Char('/'));
    return slash < 0 ? trimmed : trimmed.mid(slash + 1);
}

// What a row carries: the full stored path, and whether it is a folder. The
// visible text is the leaf, so the path cannot be recovered from it.
constexpr int kPathRole = Qt::UserRole + 1;
constexpr int kDirRole = Qt::UserRole + 2;

} // namespace

QString ArchiveWindow::pathOf(int row) const {
    auto *item = row >= 0 ? m_table->item(row, 0) : nullptr;
    return item == nullptr ? QString() : item->data(kPathRole).toString();
}

bool ArchiveWindow::rowIsDirectory(int row) const {
    auto *item = row >= 0 ? m_table->item(row, 0) : nullptr;
    return item != nullptr && item->data(kDirRole).toBool();
}

bool ArchiveWindow::folderIsMarked(const QString &folder) const {
    for (const QString &marked : m_marked) {
        if (marked.startsWith(folder)) {
            return true;
        }
    }
    return false;
}

void ArchiveWindow::setMarked(const QString &path, bool directory, bool marked) {
    if (!directory) {
        if (marked) {
            m_marked.insert(path);
        } else {
            m_marked.remove(path);
        }
        return;
    }
    // Marking a folder marks what is in it. Extraction takes member paths, and
    // a folder on its own would extract an empty directory - which is not what
    // anyone means by ticking the box next to it.
    for (const Member &member : m_members) {
        if (!member.path.startsWith(path)) {
            continue;
        }
        if (marked) {
            m_marked.insert(member.path);
        } else {
            m_marked.remove(member.path);
        }
    }
}

void ArchiveWindow::descend(const QString &folder) {
    m_prefix = folder;
    populate();
}

void ArchiveWindow::ascend() {
    if (m_prefix.isEmpty()) {
        return;
    }
    const QString leaving = m_prefix;
    const QString trimmed = withoutSlash(m_prefix);
    const int slash = trimmed.lastIndexOf(QLatin1Char('/'));
    m_prefix = slash < 0 ? QString() : trimmed.left(slash + 1);
    populate();
    // The cursor lands on the folder just left, the way going up a level in
    // the file list does. Coming back to the top of an unrelated list is the
    // thing that makes people lose their place.
    for (int row = 0; row < m_table->rowCount(); ++row) {
        if (pathOf(row) == leaving) {
            m_table->selectRow(row);
            return;
        }
    }
}

void ArchiveWindow::populate() {
    // Rebuilt rather than filtered, because the rows at one level are not a
    // subset of the rows at another: a folder five members deep appears as one
    // row here and as none at all one level down.
    const QSignalBlocker quiet(m_table);
    m_table->setRowCount(0);

    // Immediate children of the current folder, split into the folders that
    // have to be walked into and the files that can be extracted. Folders are
    // derived from the members rather than read from the archive, because many
    // archives store no directory entries at all - a zip of `a/b.txt` and
    // nothing else still has to show `a`.
    QStringList folders;
    QVector<const Member *> files;
    QSet<QString> seenFolders;
    for (const Member &member : m_members) {
        const QString rest = below(member.path, m_prefix);
        if (rest.isEmpty() || (!member.path.startsWith(m_prefix))) {
            continue;
        }
        const int slash = rest.indexOf(QLatin1Char('/'));
        if (slash < 0) {
            if (member.directory) {
                // A directory entry stored without its trailing slash.
                const QString folder = member.path + QLatin1Char('/');
                if (!seenFolders.contains(folder)) {
                    seenFolders.insert(folder);
                    folders.append(folder);
                }
            } else {
                files.append(&member);
            }
            continue;
        }
        if (slash == rest.size() - 1 && member.directory) {
            // The folder itself, stored with its trailing slash.
            if (!seenFolders.contains(member.path)) {
                seenFolders.insert(member.path);
                folders.append(member.path);
            }
            continue;
        }
        // Something deeper down: the folder on the way to it is a row here.
        const QString folder = m_prefix + rest.left(slash) + QLatin1Char('/');
        if (!seenFolders.contains(folder)) {
            seenFolders.insert(folder);
            folders.append(folder);
        }
    }
    folders.sort(Qt::CaseInsensitive);
    std::sort(files.begin(), files.end(), [](const Member *a, const Member *b) {
        return a->path.compare(b->path, Qt::CaseInsensitive) < 0;
    });

    const bool atRoot = m_prefix.isEmpty();
    const int rows = folders.size() + files.size() + (atRoot ? 0 : 1);
    m_table->setRowCount(rows);
    int row = 0;

    if (!atRoot) {
        // The way back, drawn as a row rather than left to a key nobody knows
        // is there. Not markable: it is not a member.
        auto *up = new QTableWidgetItem(m_icons.iconFor(QStringLiteral(".."), true),
                                        QStringLiteral(".."));
        up->setFlags(up->flags() & ~Qt::ItemIsUserCheckable);
        up->setData(kPathRole, QString());
        up->setData(kDirRole, true);
        m_table->setItem(row, 0, up);
        m_table->setItem(row, 1, new QTableWidgetItem(QString()));
        ++row;
    }

    for (const QString &folder : folders) {
        auto *item = new QTableWidgetItem(m_icons.iconFor(leafOf(folder), true), leafOf(folder));
        item->setFlags(item->flags() | Qt::ItemIsUserCheckable);
        item->setCheckState(folderIsMarked(folder) ? Qt::Checked : Qt::Unchecked);
        item->setData(kPathRole, folder);
        item->setData(kDirRole, true);
        m_table->setItem(row, 0, item);
        auto *size = new QTableWidgetItem(QString());
        size->setTextAlignment(Qt::AlignRight | Qt::AlignVCenter);
        m_table->setItem(row, 1, size);
        ++row;
    }

    for (const Member *member : files) {
        const QString leaf = leafOf(member->path);
        auto *item = new QTableWidgetItem(m_icons.iconFor(leaf, false), leaf);
        item->setFlags(item->flags() | Qt::ItemIsUserCheckable);
        item->setCheckState(m_marked.contains(member->path) ? Qt::Checked : Qt::Unchecked);
        item->setData(kPathRole, member->path);
        item->setData(kDirRole, false);
        if (member->unsafe) {
            // Marked, not hidden. A member whose path leads outside the
            // destination is the single most interesting thing an archive can
            // contain, and extraction is going to refuse it - saying so here
            // is why the listing exists at all.
            item->setToolTip(tr_("archive.unsafe_name"));
            QFont font = item->font();
            font.setItalic(true);
            item->setFont(font);
        }
        m_table->setItem(row, 0, item);
        auto *size = new QTableWidgetItem(PaneWidget::formatSize(member->size));
        size->setTextAlignment(Qt::AlignRight | Qt::AlignVCenter);
        m_table->setItem(row, 1, size);
        ++row;
    }

    m_where->setText(m_prefix.isEmpty() ? QFileInfo(m_archive).fileName()
                                        : QStringLiteral("%1 › %2").arg(
                                              QFileInfo(m_archive).fileName(),
                                              withoutSlash(m_prefix)));
    if (m_table->rowCount() > 0) {
        m_table->selectRow(0);
    }
    updateStatus();
}

void ArchiveWindow::updateStatus() {
    // The whole archive's count, not this level's. "14 items" that changes to
    // "3 items" on walking into a folder reads as members having gone missing.
    QString status = jtfFill(tr_("archive.entries"), "count",
                             QString::number(m_members.size()));
    const int marked = m_marked.size();
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
        auto *item = m_table->item(row, 0);
        if (item != nullptr && item->checkState() == Qt::Checked) {
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
    if (!m_marked.isEmpty()) {
        // Sorted, so an extraction's order does not depend on how a hash set
        // happened to be laid out.
        QStringList members(m_marked.begin(), m_marked.end());
        members.sort();
        return members;
    }
    const int current = m_table->currentRow();
    const QString path = pathOf(current);
    if (path.isEmpty()) {
        return {};
    }
    if (!rowIsDirectory(current)) {
        return {path};
    }
    // The cursor is on a folder and nothing is marked: everything under it.
    QStringList members;
    for (const Member &member : m_members) {
        if (member.path.startsWith(path) && !member.directory) {
            members.append(member.path);
        }
    }
    members.sort();
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
        // Walking in and out. Claimed from the table for the same reason the
        // others are: Return activates an item on the table's own terms and
        // Backspace is type-to-find's rubout.
        case Qt::Key_Return:
        case Qt::Key_Enter:
        case Qt::Key_Right:
        case Qt::Key_Backspace:
        case Qt::Key_Left:
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
    case Qt::Key_Return:
    case Qt::Key_Enter:
    case Qt::Key_Right: {
        const int row = m_table->currentRow();
        if (!rowIsDirectory(row)) {
            return;
        }
        const QString path = pathOf(row);
        if (path.isEmpty()) {
            ascend(); // The `..` row.
        } else {
            descend(path);
        }
        return;
    }
    case Qt::Key_Backspace:
    case Qt::Key_Left:
        ascend();
        return;
    case Qt::Key_Space: {
        // Mark and move on, as Space does in the file list.
        const int row = m_table->currentRow();
        auto *item = row >= 0 ? m_table->item(row, 0) : nullptr;
        const QString path = pathOf(row);
        if (item != nullptr && !path.isEmpty()) {
            const bool marking = item->checkState() != Qt::Checked;
            setMarked(path, rowIsDirectory(row), marking);
            item->setCheckState(marking ? Qt::Checked : Qt::Unchecked);
            if (row + 1 < m_table->rowCount()) {
                m_table->selectRow(row + 1);
            }
            updateStatus();
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
