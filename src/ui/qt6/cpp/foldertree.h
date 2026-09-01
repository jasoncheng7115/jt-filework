// The folder tree sidebar, in the Q-Dir / QSpace tradition.
//
// It asks Rust for children the same way the file list does, so the two cannot
// disagree about what a directory contains, whether a symlink counts as a
// folder, or whether hidden entries are shown. Two sources of truth about one
// filesystem is the drift AGENTS.md 4 exists to prevent.
//
// Children are fetched when a node is expanded, never before: a tree that
// walked the disk on construction would take as long as the deepest branch.
#pragma once

#include "bridge.h"
#include "iconprovider.h"

#include <QAbstractItemModel>
#include <QVector>
#include <QColor>
#include <QFont>
#include <QWidget>

class QTreeView;

class FolderTreeModel : public QAbstractItemModel {
    Q_OBJECT

public:
    explicit FolderTreeModel(JtfApp *app, QObject *parent = nullptr);
    ~FolderTreeModel() override;

    QModelIndex index(int row, int column,
                      const QModelIndex &parent = QModelIndex()) const override;
    QModelIndex parent(const QModelIndex &child) const override;
    int rowCount(const QModelIndex &parent = QModelIndex()) const override;
    int columnCount(const QModelIndex &parent = QModelIndex()) const override;
    QVariant data(const QModelIndex &index, int role) const override;
    bool hasChildren(const QModelIndex &parent = QModelIndex()) const override;
    bool canFetchMore(const QModelIndex &parent) const override;
    void fetchMore(const QModelIndex &parent) override;

    QString pathAt(const QModelIndex &index) const;
    // Gives a server its own top-level row, if `path` names one and it has no
    // row yet. Does nothing for an ordinary path.
    void ensureServerRoot(const QString &path);
    // The colour for drawn glyphs; the widget knows the palette, the model
    // does not.
    void setIconColour(const QColor &colour) { m_iconColour = colour; }
    // Expands the tree down to `path`, creating the nodes on the way, and
    // returns the index for it. Empty if the path is not under a root.
    QModelIndex indexForPath(const QString &path);

    void refresh();

private:
    struct Node;
    Node *nodeFor(const QModelIndex &index) const;
    void loadChildren(Node *node) const;

    JtfApp *m_app;
    Node *m_root;
    mutable IconProvider m_icons;
    QColor m_iconColour;
};

class FolderTree : public QWidget {
    Q_OBJECT

public:
    explicit FolderTree(JtfApp *app, QWidget *parent = nullptr);

    void selectPath(const QString &path);
    // The tree uses the same font as the list: one window, one text size.
    void setListFont(const QFont &font);
    // The heading above the tree, in the current language.
    void retranslate();
    /// Re-read the tree, keeping the selected folder selected.
    void refreshKeepingPlace();
    void refresh();

protected:
    // Bare letters are commands in Single-Key mode, so no list may quietly
    // eat one.
    bool eventFilter(QObject *watched, QEvent *event) override;

signals:
    // A folder was chosen; the active pane should go there.
    void folderActivated(const QString &path);
    // A key the keymap knows was pressed while this tree had focus.
    void commandRequested(const QString &id);
    /// Open this folder in a tab of its own.
    void openInNewTabRequested(const QString &path);
    /// Open this folder in a window of its own.
    void openInNewWindowRequested(const QString &path);
    /// Show what fills this folder.
    void diskUsageRequested(const QString &path);
    /// Make a new folder inside this one.
    void newFolderRequested(const QString &path);
    /// A bookmark was added or removed from this tree's menu.
    void bookmarksChanged();

private:
    QString tr_(const char *key) const;

    JtfApp *m_app;
    class QLabel *m_title = nullptr;
    FolderTreeModel *m_model = nullptr;
    QTreeView *m_view = nullptr;
    QString m_current;
};
