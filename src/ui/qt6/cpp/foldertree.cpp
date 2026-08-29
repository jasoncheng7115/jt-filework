#include "foldertree.h"
#include "jtfstring.h"

#include <QFileInfo>
#include <QFontMetrics>
#include <QHeaderView>
#include <QLabel>
#include <QTreeView>
#include <QVBoxLayout>

// ---------------------------------------------------------------- the model

struct FolderTreeModel::Node {
    QString path;
    QString name;
    Node *parent = nullptr;
    QVector<Node *> children;
    bool loaded = false;

    ~Node() { qDeleteAll(children); }
};

FolderTreeModel::FolderTreeModel(JtfApp *app, QObject *parent)
    : QAbstractItemModel(parent), m_app(app), m_root(new Node) {
    // One visible root, the filesystem root, exactly as Q-Dir and QSpace do.
    // Volumes appear under it because that is where the platform puts them.
    auto *filesystem = new Node;
    filesystem->path = QStringLiteral("/");
    filesystem->name = QStringLiteral("/");
    filesystem->parent = m_root;
    m_root->children.append(filesystem);
    m_root->loaded = true;
}

FolderTreeModel::~FolderTreeModel() {
    delete m_root;
}

FolderTreeModel::Node *FolderTreeModel::nodeFor(const QModelIndex &index) const {
    return index.isValid() ? static_cast<Node *>(index.internalPointer()) : m_root;
}

void FolderTreeModel::loadChildren(Node *node) const {
    if (node->loaded) {
        return;
    }
    node->loaded = true;

    const QByteArray path = node->path.toUtf8();
    const QString joined = jtfText([&](char *buf, int len) {
        return jtf_child_directories(m_app, path.constData(), buf, len);
    });
    if (joined.isEmpty()) {
        return;
    }
    for (const QString &child : joined.split(QLatin1Char('\n'))) {
        if (child.isEmpty()) {
            continue;
        }
        auto *entry = new Node;
        entry->path = child;
        entry->name = QFileInfo(child).fileName();
        entry->parent = node;
        node->children.append(entry);
    }
}

QModelIndex FolderTreeModel::index(int row, int column, const QModelIndex &parent) const {
    Node *node = nodeFor(parent);
    if (row < 0 || row >= node->children.size() || column != 0) {
        return {};
    }
    return createIndex(row, column, node->children.at(row));
}

QModelIndex FolderTreeModel::parent(const QModelIndex &child) const {
    Node *node = nodeFor(child);
    if (!node || node == m_root || !node->parent || node->parent == m_root) {
        return {};
    }
    Node *grandparent = node->parent->parent;
    const int row = grandparent ? grandparent->children.indexOf(node->parent) : 0;
    return createIndex(row, 0, node->parent);
}

int FolderTreeModel::rowCount(const QModelIndex &parent) const {
    return nodeFor(parent)->children.size();
}

int FolderTreeModel::columnCount(const QModelIndex &) const {
    return 1;
}

bool FolderTreeModel::hasChildren(const QModelIndex &parent) const {
    Node *node = nodeFor(parent);
    // Unloaded nodes claim children so the disclosure arrow appears without
    // reading the directory. Opening an empty folder collapses again, which is
    // what every lazy tree does and is cheaper than being right up front.
    return !node->loaded || !node->children.isEmpty();
}

bool FolderTreeModel::canFetchMore(const QModelIndex &parent) const {
    return !nodeFor(parent)->loaded;
}

void FolderTreeModel::fetchMore(const QModelIndex &parent) {
    Node *node = nodeFor(parent);
    if (node->loaded) {
        return;
    }
    loadChildren(node);
    if (!node->children.isEmpty()) {
        beginInsertRows(parent, 0, node->children.size() - 1);
        endInsertRows();
    }
}

QVariant FolderTreeModel::data(const QModelIndex &index, int role) const {
    Node *node = nodeFor(index);
    if (!node || node == m_root) {
        return {};
    }
    switch (role) {
    case Qt::DisplayRole:
        return node->name;
    case Qt::ToolTipRole:
        return node->path;
    case Qt::DecorationRole:
        return m_icons.iconFor(node->path, true);
    default:
        return {};
    }
}

QString FolderTreeModel::pathAt(const QModelIndex &index) const {
    Node *node = nodeFor(index);
    return node && node != m_root ? node->path : QString();
}

QModelIndex FolderTreeModel::indexForPath(const QString &path) {
    if (path.isEmpty()) {
        return {};
    }
    // Walk down from the root, loading only the directories on the way, so
    // revealing a deep path costs its depth rather than the whole tree.
    QModelIndex current = index(0, 0, QModelIndex()); // "/"
    if (!current.isValid()) {
        return {};
    }

    const QStringList parts = path.split(QLatin1Char('/'), Qt::SkipEmptyParts);
    QString walked;
    for (const QString &part : parts) {
        walked += QLatin1Char('/') + part;
        if (canFetchMore(current)) {
            fetchMore(current);
        }
        bool found = false;
        for (int row = 0; row < rowCount(current); ++row) {
            const QModelIndex child = index(row, 0, current);
            if (pathAt(child) == walked) {
                current = child;
                found = true;
                break;
            }
        }
        if (!found) {
            return current; // as far as the tree can go
        }
    }
    return current;
}

void FolderTreeModel::refresh() {
    beginResetModel();
    for (Node *child : std::as_const(m_root->children)) {
        qDeleteAll(child->children);
        child->children.clear();
        child->loaded = false;
    }
    m_icons.clear();
    endResetModel();
}

// --------------------------------------------------------------- the widget

FolderTree::FolderTree(JtfApp *app, QWidget *parent) : QWidget(parent), m_app(app) {
    setObjectName(QStringLiteral("JtfTree"));
    auto *layout = new QVBoxLayout(this);
    layout->setContentsMargins(0, 0, 0, 0);
    layout->setSpacing(0);

    m_model = new FolderTreeModel(app, this);
    m_view = new QTreeView(this);
    m_view->setModel(m_model);
    m_view->setHeaderHidden(true);
    m_view->setUniformRowHeights(true); // what keeps it virtualized
    m_view->setIconSize(QSize(16, 16));
    m_view->setEditTriggers(QAbstractItemView::NoEditTriggers);
    m_view->setSelectionMode(QAbstractItemView::SingleSelection);
    m_view->setExpandsOnDoubleClick(true);
    layout->addWidget(m_view);

    // A single click navigates, as it does in Q-Dir and QSpace: the tree is a
    // navigator, not a second selection model.
    connect(m_view, &QTreeView::clicked, this, [this](const QModelIndex &index) {
        const QString path = m_model->pathAt(index);
        if (!path.isEmpty() && path != m_current) {
            m_current = path;
            emit folderActivated(path);
        }
    });
}

void FolderTree::selectPath(const QString &path) {
    if (path.isEmpty() || path == m_current) {
        return;
    }
    m_current = path;
    const QModelIndex index = m_model->indexForPath(path);
    if (index.isValid()) {
        QSignalBlocker blocker(m_view);
        m_view->setCurrentIndex(index);
        m_view->scrollTo(index, QAbstractItemView::EnsureVisible);
        m_view->expand(index);
    }
}

void FolderTree::setListFont(const QFont &font) {
    m_view->setFont(font);
    // Row height follows the font here too, or the sidebar and the list drift
    // apart as the size changes.
    m_view->setStyleSheet(
        QStringLiteral("QTreeView::item { min-height: %1px; }")
            .arg(QFontMetrics(font).height() + 4));
}

void FolderTree::refresh() {
    const QString path = m_current;
    m_current.clear();
    m_model->refresh();
    selectPath(path);
}
