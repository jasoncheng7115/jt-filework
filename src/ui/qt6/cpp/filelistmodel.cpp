#include "filelistmodel.h"
#include "jtfstring.h"

#include <QBrush>
#include <QPixmap>
#include <QMimeData>
#include <QUrl>
#include <QFileInfo>
#include <QFont>
#include <QSet>

namespace {
// The square a list thumbnail is decoded into. Matches the icon column, so a
// row's height does not change when a thumbnail replaces an icon.
constexpr int kThumbnailEdge = 32;
} // namespace

FileListModel::FileListModel(JtfApp *app, int paneId, QObject *parent)
    : QAbstractTableModel(parent), m_app(app), m_pane(paneId) {
    m_thumbnails = new ThumbnailCache(this);
    // A thumbnail arrives later than the row that asked for it, so the row is
    // repainted when it does. Only that row: repainting the list would undo
    // the point of decoding off the UI thread.
    connect(m_thumbnails, &ThumbnailCache::ready, this, [this](const QString &path) {
        for (int row = 0; row < m_rows; ++row) {
            const QString rowPath = jtfText([&](char *buf, int len) {
                return jtf_row_path(m_app, m_pane, row, buf, len);
            });
            if (rowPath == path) {
                const QModelIndex at = index(row, 0);
                emit dataChanged(at, at, {Qt::DecorationRole});
                return;
            }
        }
    });
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
    case Qt::DisplayRole: {
        // The kind column is answered by the platform, like the icon beside
        // it: "PDF Document" rather than the core's coarse "File".
        // A folder is a folder in every locale we ship; only files need the
        // platform asked, and asking it about a directory returns the
        // freedesktop wording ("Directory") rather than the one the rest of
        // the program uses.
        if (column == kindColumn() && !jtf_row_is_parent(m_app, m_pane, row) &&
            !jtf_row_is_directory(m_app, m_pane, row)) {
            const QString path = jtfText([&](char *buf, int len) {
                return jtf_row_path(m_app, m_pane, row, buf, len);
            });
            if (!path.isEmpty()) {
                const QString type = m_icons.typeNameFor(path, false);
                if (!type.isEmpty()) {
                    return type;
                }
                // Neither the platform nor Qt could name it. Say so in the
                // shape the reference layout uses - "TOML File" - rather than
                // leaving the column blank or printing a MIME identifier.
                const QString suffix = QFileInfo(path).suffix().toUpper();
                if (!suffix.isEmpty()) {
                    const QString pattern = jtfText([&](char *buf, int len) {
                        return jtf_tr(m_app, "kind.suffix_file", buf, len);
                    });
                    return jtfFill(pattern, "ext", suffix);
                }
            }
        }
        return jtfText([&](char *buf, int len) {
            return jtf_row_text(m_app, m_pane, row, column, buf, len);
        });
    }

    case Qt::CheckStateRole:
        // Marks are shown as a checkbox on the name column, which is what a
        // person expects to be able to click. The keyboard route (space) and
        // this one set the same state - there is one mark set, not two.
        // The `..` row is a way out of the folder, not a thing you can mark,
        // so it gets no box rather than an empty one that never ticks.
        if (column != 0 || jtf_row_is_parent(m_app, m_pane, row)) {
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
        const bool isDirectory = jtf_row_is_directory(m_app, m_pane, row) != 0;
        if (m_showThumbnails && !isDirectory) {
            // A picture of the file beats a picture of its type, when we can
            // have one. The icon stands in until the decode finishes.
            const QPixmap thumb = m_thumbnails->thumbnail(path, kThumbnailEdge);
            if (!thumb.isNull()) {
                return QIcon(thumb);
            }
        }
        return m_icons.iconFor(path, isDirectory);
    }

    case Qt::ForegroundRole:
        // A marked row is coloured, and marks are distinct from selection -
        // AGENTS.md 10 keeps them separate in the model, so the UI must keep
        // them distinguishable on screen.
        if (jtf_row_is_marked(m_app, m_pane, row)) {
            return QBrush(m_markColor);
        }
        // The whole row, not just the name: CView colours the line, and the
        // point is to notice before you double-click something that runs.
        if (jtf_row_is_executable(m_app, m_pane, row)) {
            return QBrush(m_execColor);
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

int FileListModel::kindColumn() const {
    // Found by key, not assumed to be a fixed index: the columns are data and
    // their order has already changed once.
    if (m_kindColumn == -2) {
        m_kindColumn = -1;
        for (int i = 0; i < jtf_column_count(); ++i) {
            const QString key =
                jtfText([&](char *buf, int len) { return jtf_column_key(i, buf, len); });
            if (key == QLatin1String("column.kind")) {
                m_kindColumn = i;
                break;
            }
        }
    }
    return m_kindColumn;
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
        if (index.column() == 0 && !jtf_row_is_parent(m_app, m_pane, index.row())) {
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

void FileListModel::setThumbnailsEnabled(bool on) {
    if (on == m_showThumbnails) {
        return;
    }
    m_showThumbnails = on;
    m_thumbnails->clear();
    if (m_rows > 0) {
        emit dataChanged(index(0, 0), index(m_rows - 1, 0), {Qt::DecorationRole});
    }
}
