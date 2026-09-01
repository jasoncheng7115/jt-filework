#include "foldertree.h"

#include "icons.h"
#include "platform/filetype.h"
#include <QGuiApplication>
#include <QClipboard>
#include "platform/quicklook.h"
#include <QKeyEvent>
#include <QAction>
#include <QMenu>
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

// The server part of an `sftp://user@host[:port]/path` display path, with its
// trailing slash, or empty if this is an ordinary path. A Windows path has a
// colon and no scheme, so it never matches.
static QString serverRootOf(const QString &path) {
    static const QString scheme = QStringLiteral("sftp://");
    if (!path.startsWith(scheme)) {
        return {};
    }
    const int slash = path.indexOf(QLatin1Char('/'), scheme.size());
    return slash < 0 ? path + QLatin1Char('/') : path.left(slash + 1);
}

// Append one segment, without doubling the separator after a root.
static QString joined(const QString &base, const QString &part) {
    return base.endsWith(QLatin1Char('/')) ? base + part
                                           : base + QLatin1Char('/') + part;
}

FolderTreeModel::FolderTreeModel(JtfApp *app, QObject *parent)
    : QAbstractItemModel(parent), m_app(app), m_root(new Node) {
    // One visible root, exactly as Q-Dir and QSpace do.
    //
    // On Unix that is `/`, and volumes appear under it because that is where
    // the platform mounts them. Windows has no single root - the drives are
    // separate trees - so the root there is the one Explorer itself shows
    // above them, 「本機」 / "This PC". Labelling it `\` would name a thing
    // Windows does not have and that nothing can be opened from.
    auto *filesystem = new Node;
    filesystem->path = QStringLiteral("/");
    const QString label = filetype::rootLabel();
    filesystem->name = label.isEmpty() ? QStringLiteral("/") : label;
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
        // A server root is not on this disk, so there is no file icon to ask
        // for; it gets the same glyph the places list gives it.
        if (node->parent == m_root && !serverRootOf(node->path).isEmpty()) {
            return glyph::make(glyph::Shape::Connected, m_iconColour);
        }
        return m_icons.iconFor(node->path, true);
    default:
        return {};
    }
}

QString FolderTreeModel::pathAt(const QModelIndex &index) const {
    Node *node = nodeFor(index);
    return node && node != m_root ? node->path : QString();
}

void FolderTreeModel::ensureServerRoot(const QString &path) {
    // A server is not under `/`, so it gets a root of its own beside it - the
    // tree has to be able to show wherever the focused pane is, and a pane can
    // be on a server. The root stays once added, so switching focus back and
    // forth does not rebuild the branch each time.
    const QString root = serverRootOf(path);
    if (root.isEmpty()) {
        return;
    }
    for (const Node *child : std::as_const(m_root->children)) {
        if (child->path == root) {
            return;
        }
    }
    auto *node = new Node;
    node->path = root;
    // `sftp://jason@host/` shown as `jason@host`: the scheme is noise once the
    // row is sitting in a tree of places.
    node->name = root.mid(7, root.size() - 8);
    node->parent = m_root;
    const int row = static_cast<int>(m_root->children.size());
    beginInsertRows(QModelIndex(), row, row);
    m_root->children.append(node);
    endInsertRows();
}

QModelIndex FolderTreeModel::indexForPath(const QString &path) {
    if (path.isEmpty()) {
        return {};
    }
    // Which root is this under - the filesystem, or one of the servers?
    QString base = serverRootOf(path);
    if (base.isEmpty()) {
        base = QStringLiteral("/");
    }
    QModelIndex current;
    for (int row = 0; row < m_root->children.size(); ++row) {
        if (m_root->children.at(row)->path == base) {
            current = index(row, 0, QModelIndex());
            break;
        }
    }
    if (!current.isValid()) {
        return {};
    }

    // Walk down from that root, loading only the directories on the way, so
    // revealing a deep path costs its depth rather than the whole tree.
    const QStringList parts =
        path.mid(base.size()).split(QLatin1Char('/'), Qt::SkipEmptyParts);
    QString walked = base;
    for (const QString &part : parts) {
        walked = joined(walked, part);
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

    // Named, for the same reason the places list above it is: two trees of
    // folder rows stacked in one narrow column read as one long list.
    m_title = new QLabel(this);
    m_title->setObjectName(QStringLiteral("JtfSidebarTitle"));
    m_title->setText(tr_("tree.title"));
    layout->addWidget(m_title);

    m_model = new FolderTreeModel(app, this);
    m_view = new QTreeView(this);
    m_view->setModel(m_model);
    m_view->setHeaderHidden(true);
    m_view->setUniformRowHeights(true); // what keeps it virtualized
    m_view->setIconSize(QSize(16, 16));
    // Qt's default step is 20px, which leaves the arrow stranded away from
    // the folder it opens. Tighter binds the three parts of a row - arrow,
    // icon, name - into one thing to aim at.
    m_view->setIndentation(14);
    // A tree whose minimum width is decided by its longest folder name cannot
    // be made narrow, and the splitter it lives in silently ignores the width
    // it was given. Long names elide instead - the sidebar is for getting
    // somewhere, and the full name is a tooltip away.
    m_view->setTextElideMode(Qt::ElideRight);
    m_view->setHorizontalScrollBarPolicy(Qt::ScrollBarAsNeeded);
    m_view->setMinimumWidth(80);
    m_view->setEditTriggers(QAbstractItemView::NoEditTriggers);
    m_view->setSelectionMode(QAbstractItemView::SingleSelection);
    m_view->setExpandsOnDoubleClick(true);
    layout->addWidget(m_view);

    m_view->installEventFilter(this);
    m_model->setIconColour(palette().color(QPalette::Text));

    // The tree had no menu of its own, so a right-click in it did nothing -
    // in a sidebar whose whole purpose is getting to a folder, that reads as
    // a list you cannot act on.
    m_view->setContextMenuPolicy(Qt::CustomContextMenu);
    connect(m_view, &QWidget::customContextMenuRequested, this, [this](const QPoint &at) {
        const QString path = m_model->pathAt(m_view->indexAt(at));
        if (path.isEmpty()) {
            return;
        }
        // Everything you can do *to a folder* from anywhere else in the
        // program. A tree of folders whose menu offered three of them read as
        // a tree you cannot act on - the sidebar is where you go looking for a
        // folder, so it is where you are most likely to want to do something
        // with one.
        const QColor iconColour = palette().color(QPalette::Text);
        const QByteArray utf8 = path.toUtf8();
        const bool remote = path.startsWith(QStringLiteral("sftp://"));
        QMenu menu(this);

        QAction *open = menu.addAction(
            glyph::forCommand(QStringLiteral("file.open"), iconColour), tr_("crumb.open"));
        QAction *newTab = menu.addAction(
            glyph::forCommand(QStringLiteral("tab.new"), iconColour), tr_("crumb.open_tab"));
        QAction *newWindow = menu.addAction(
            glyph::forCommand(QStringLiteral("tab.tear_off"), iconColour),
            tr_("crumb.open_window"));
        menu.addSeparator();

        // Operations on the folder itself. Each is offered only where it can
        // actually run: a folder on a server has no path on this machine, so
        // measuring it, revealing it or opening a terminal in it would all be
        // acting on whatever local file happens to share its name.
        QAction *usage = menu.addAction(
            glyph::forCommand(QStringLiteral("file.disk_usage"), iconColour),
            tr_("command.file.disk_usage"));
        usage->setEnabled(!remote);
        QAction *newFolder = menu.addAction(
            glyph::forCommand(QStringLiteral("file.new_folder"), iconColour),
            tr_("command.file.new_folder"));
        newFolder->setEnabled(!remote);
        menu.addSeparator();

        const bool marked = jtf_path_is_bookmarked(m_app, utf8.constData()) != 0;
        QAction *bookmark = menu.addAction(
            glyph::forCommand(QStringLiteral("file.bookmark"), iconColour),
            tr_(marked ? "crumb.unbookmark" : "crumb.bookmark"));
        QAction *copyPath = menu.addAction(
            glyph::forCommand(QStringLiteral("file.copy_path"), iconColour),
            tr_("crumb.copy_path"));

        QAction *reveal = nullptr;
        QAction *terminal = nullptr;
        if (!remote) {
            menu.addSeparator();
            if (platform::canReveal()) {
                reveal = menu.addAction(
                    glyph::forCommand(QStringLiteral("file.reveal"), iconColour),
                    tr_("crumb.reveal"));
            }
            if (filetype::available()) {
                terminal = menu.addAction(
                    glyph::forCommand(QStringLiteral("nav.goto"), iconColour),
                    tr_("crumb.terminal"));
            }
        }
        menu.addSeparator();
        QAction *refresh = menu.addAction(
            glyph::forCommand(QStringLiteral("view.refresh"), iconColour),
            tr_("command.view.refresh"));

        QAction *chosen = menu.exec(m_view->viewport()->mapToGlobal(at));
        if (chosen == nullptr) {
            return;
        }
        if (chosen == open) {
            m_current = path;
            emit folderActivated(path);
        } else if (chosen == newTab) {
            emit openInNewTabRequested(path);
        } else if (chosen == newWindow) {
            emit openInNewWindowRequested(path);
        } else if (chosen == usage) {
            emit diskUsageRequested(path);
        } else if (chosen == newFolder) {
            emit newFolderRequested(path);
        } else if (chosen == bookmark) {
            jtf_toggle_bookmark_path(m_app, utf8.constData());
            emit bookmarksChanged();
        } else if (chosen == copyPath) {
            QGuiApplication::clipboard()->setText(path);
        } else if (chosen == reveal) {
            platform::reveal(path);
        } else if (chosen == terminal) {
            filetype::openInTerminal(path);
        } else if (chosen == refresh) {
            refreshKeepingPlace();
        }
    });

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

bool FolderTree::eventFilter(QObject *watched, QEvent *event) {
    if (watched != m_view || event->type() != QEvent::KeyPress) {
        return QWidget::eventFilter(watched, event);
    }
    auto *key = static_cast<QKeyEvent *>(event);

    // QTreeView answers a letter with its own type-to-find, and revealing a
    // match expands the branch it is in - so pressing `V` here folded and
    // unfolded the tree instead of viewing a file, and `T` did something
    // other than toggling this very tree. In Single-Key mode a bare letter is
    // a command wherever the focus happens to be; the file list already
    // refuses type-ahead for exactly this reason (`!type_ahead = off`), and
    // the tree has to agree or the mode is only true in one widget.
    //
    // The arrows, Enter and the rest are left to the tree, which is what they
    // are for here.
    constexpr Qt::KeyboardModifiers kMeaningful =
        Qt::ControlModifier | Qt::AltModifier | Qt::MetaModifier;
    const int code = key->key();
    const bool letter = (code >= Qt::Key_A && code <= Qt::Key_Z)
                        || (code >= Qt::Key_0 && code <= Qt::Key_9);
    if (!letter || (key->modifiers() & kMeaningful) != Qt::NoModifier) {
        return QWidget::eventFilter(watched, event);
    }

    QString chord = code <= Qt::Key_Z && code >= Qt::Key_A
                        ? QString(QChar(QLatin1Char('a' + (code - Qt::Key_A))))
                        : QString(QChar(QLatin1Char('0' + (code - Qt::Key_0))));
    if (key->modifiers().testFlag(Qt::ShiftModifier)) {
        chord = QStringLiteral("shift+") + chord;
    }
    const QByteArray utf8 = chord.toUtf8();
    const QString id = jtfText(
        [&](char *b, int l) { return jtf_command_for_chord(m_app, utf8.constData(), b, l); });
    if (id.isEmpty()) {
        // Nothing bound. Swallowed anyway: a letter that means nothing should
        // do nothing, not scroll the tree to a folder the user never asked
        // to go to.
        return true;
    }
    emit commandRequested(id);
    return true;
}

void FolderTree::selectPath(const QString &path) {
    if (path.isEmpty() || path == m_current) {
        return;
    }
    m_current = path;
    // A server the pane has just reached has no row yet; give it one before
    // asking where it is, so the tree follows onto the server rather than
    // going blank.
    m_model->ensureServerRoot(path);
    const QModelIndex index = m_model->indexForPath(path);
    QSignalBlocker blocker(m_view);
    if (!index.isValid()) {
        // Nowhere in this tree - the pane is inside an archive. Leaving the
        // old row highlighted would say the pane is still in a folder it
        // left, which is how "the tree does not follow" looked. Showing no
        // selection is the honest answer.
        m_view->clearSelection();
        m_view->setCurrentIndex(QModelIndex());
        return;
    }
    m_view->setCurrentIndex(index);
    m_view->scrollTo(index, QAbstractItemView::EnsureVisible);
    m_view->expand(index);
}

void FolderTree::setListFont(const QFont &font) {
    m_view->setFont(font);
    // Row height follows the font here too, or the sidebar and the list drift
    // apart as the size changes.
    m_view->setStyleSheet(
        QStringLiteral("QTreeView::item { min-height: %1px; }")
            .arg(QFontMetrics(font).height() + 4));
}

QString FolderTree::tr_(const char *key) const {
    return jtfText([&](char *buf, int len) { return jtf_tr(m_app, key, buf, len); });
}

void FolderTree::retranslate() { m_title->setText(tr_("tree.title")); }

void FolderTree::refreshKeepingPlace() {
    // The same as `refresh`, named for what the menu item promises: the tree
    // is re-read and the folder you were on is still the folder you are on.
    refresh();
}

void FolderTree::refresh() {
    const QString path = m_current;
    m_current.clear();
    m_model->refresh();
    m_model->setIconColour(palette().color(QPalette::Text));
    selectPath(path);
}
