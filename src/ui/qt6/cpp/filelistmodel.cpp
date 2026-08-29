#include "filelistmodel.h"
#include "jtfstring.h"

#include <QBrush>
#include <QFont>

FileListModel::FileListModel(JtfApp *app, int paneId, QObject *parent)
    : QAbstractTableModel(parent), m_app(app), m_pane(paneId) {}

void FileListModel::setPane(int paneId) {
    beginResetModel();
    m_pane = paneId;
    endResetModel();
}

int FileListModel::rowCount(const QModelIndex &parent) const {
    if (parent.isValid()) {
        return 0;
    }
    return jtf_row_count(m_app, m_pane);
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
    if (orientation != Qt::Horizontal || role != Qt::DisplayRole) {
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

void FileListModel::refresh() {
    beginResetModel();
    endResetModel();
}
