#include "filelistmodel.h"
#include "jtfstring.h"

#include <QBrush>
#include <QMimeData>
#include <QUrl>
#include <QFont>
#include <QSet>

FileListModel::FileListModel(JtfApp *app, int paneId, QObject *parent)
    : QAbstractTableModel(parent), m_app(app), m_pane(paneId) {
    m_generation = jtf_row_generation(app, paneId);
    m_rows = jtf_row_count(app, paneId);
}

void FileListModel::setPane(int paneId) {
    beginResetModel();
    m_pane = paneId;
    m_generation = jtf_row_generation(m_app, m_pane);
    m_rows = jtf_row_count(m_app, m_pane);
    endResetModel();
}

int FileListModel::rowCount(const QModelIndex &parent) const {
    if (parent.isValid()) {
        return 0;
    }
    // The count Qt knows about, not the live one: a model must not grow
    // underneath a view without an insert notification.
    return m_rows;
}

int FileListModel::columnCount(const QModelIndex &parent) const {
    if (parent.isValid()) {
        return 0;
    }
    return jtf_column_count();
}

QVariant FileListModel::data(const QModelIndex &index, int role) const {
    if (!index.isValid()) {
        return {};
    }
    const int row = index.row();
    const int column = index.column();

    switch (role) {
    case Qt::DisplayRole:
        return jtfText([&](char *buf, int len) {
            return jtf_row_text(m_app, m_pane, row, column, buf, len);
        });

    case Qt::CheckStateRole:
        // Marks are shown as a checkbox on the name column, which is what a
        // person expects to be able to click. The keyboard route (space) and
        // this one set the same state - there is one mark set, not two.
        if (column != 0) {
            return {};
        }
        return jtf_row_is_marked(m_app, m_pane, row) ? Qt::Checked : Qt::Unchecked;

    case Qt::DecorationRole: {
        // The name column carries the file's own icon, the one the platform
        // would show for it (AGENTS.md 8).
        if (column != 0) {
            return {};
        }
        const QString path = jtfText([&](char *buf, int len) {
            return jtf_row_path(m_app, m_pane, row, buf, len);
        });
        if (path.isEmpty()) {
            return {};
        }
        return m_icons.iconFor(path, jtf_row_is_directory(m_app, m_pane, row) != 0);
    }

    case Qt::ForegroundRole:
        // A marked row is coloured, and marks are distinct from selection -
        // AGENTS.md 10 keeps them separate in the model, so the UI must keep
        // them distinguishable on screen.
        if (jtf_row_is_marked(m_app, m_pane, row)) {
            return QBrush(m_markColor);
        }
        if (column == 0 && jtf_row_is_directory(m_app, m_pane, row)) {
            return QBrush(m_dirColor);
        }
        return {};

    case Qt::FontRole:
        if (jtf_row_is_marked(m_app, m_pane, row)) {
            QFont font;
            font.setBold(true);
            return font;
        }
        return {};

    case Qt::TextAlignmentRole:
        if (column == 1) {
            return QVariant(Qt::AlignRight | Qt::AlignVCenter);
        }
        return {};

    default:
        return {};
    }
}

QVariant FileListModel::headerData(int section, Qt::Orientation orientation, int role) const {
    if (orientation != Qt::Horizontal) {
        return {};
    }
    // Headers align with their data: text left, numbers right. A centred
    // header over left-aligned rows is what makes a column look crooked.
    if (role == Qt::TextAlignmentRole) {
        return section == 1 ? QVariant(Qt::AlignRight | Qt::AlignVCenter)
                            : QVariant(Qt::AlignLeft | Qt::AlignVCenter);
    }
    if (role != Qt::DisplayRole) {
        return {};
    }
    // No English literal here: the header text is a localization key resolved
    // by Rust (AGENTS.md 11).
    const QString key =
        jtfText([&](char *buf, int len) { return jtf_column_key(section, buf, len); });
    const QByteArray keyUtf8 = key.toUtf8();
    return jtfText([&](char *buf, int len) {
        return jtf_tr(m_app, keyUtf8.constData(), buf, len);
    });
}

bool FileListModel::setData(const QModelIndex &index, const QVariant &value, int role) {
    if (role != Qt::CheckStateRole || !index.isValid() || index.column() != 0) {
        return false;
    }
    Q_UNUSED(value);
    jtf_toggle_mark(m_app, m_pane, index.row());
    // The whole row is repainted, not just the box: a mark also changes the
    // row's colour and weight.
    emit dataChanged(index.siblingAtColumn(0),
                     index.siblingAtColumn(columnCount() - 1),
                     {Qt::CheckStateRole, Qt::ForegroundRole, Qt::FontRole});
    emit markChanged();
    return true;
}

Qt::ItemFlags FileListModel::flags(const QModelIndex &index) const {
    Qt::ItemFlags base = QAbstractTableModel::flags(index);
    if (index.isValid()) {
        base |= Qt::ItemIsDragEnabled;
        if (index.column() == 0) {
            base |= Qt::ItemIsUserCheckable;
        }
        // Only a directory row is itself a drop target; dropping between rows
        // means "into this folder", which the view handles as the empty area.
        if (jtf_row_is_directory(m_app, m_pane, index.row())) {
            base |= Qt::ItemIsDropEnabled;
        }
    } else {
        base |= Qt::ItemIsDropEnabled;
    }
    return base;
}

QStringList FileListModel::mimeTypes() const {
    return {QStringLiteral("text/uri-list")};
}

QMimeData *FileListModel::mimeData(const QModelIndexList &indexes) const {
    QList<QUrl> urls;
    QSet<int> seen;
    for (const QModelIndex &index : indexes) {
        if (!index.isValid() || seen.contains(index.row())) {
            continue; // one URL per row, not one per selected cell
        }
        seen.insert(index.row());
        const QString path = jtfText([&](char *buf, int len) {
            return jtf_row_path(m_app, m_pane, index.row(), buf, len);
        });
        if (!path.isEmpty()) {
            urls.append(QUrl::fromLocalFile(path));
        }
    }
    if (urls.isEmpty()) {
        return nullptr;
    }
    auto *data = new QMimeData;
    data->setUrls(urls);
    return data;
}

Qt::DropActions FileListModel::supportedDragActions() const {
    return Qt::CopyAction | Qt::MoveAction;
}

Qt::DropActions FileListModel::supportedDropActions() const {
    return Qt::CopyAction | Qt::MoveAction;
}

void FileListModel::refresh() {
    const quint64 generation = jtf_row_generation(m_app, m_pane);
    const int rows = jtf_row_count(m_app, m_pane);

    if (generation == m_generation && rows == m_rows) {
        return; // nothing changed: do not disturb the view at all
    }

    if (generation == m_generation && rows > m_rows) {
        beginInsertRows(QModelIndex(), m_rows, rows - 1);
        m_rows = rows;
        endInsertRows();
        return;
    }

    beginResetModel();
    m_generation = generation;
    m_rows = rows;
    endResetModel();
}
