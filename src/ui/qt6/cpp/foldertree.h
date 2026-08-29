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
};

class FolderTree : public QWidget {
    Q_OBJECT

public:
    explicit FolderTree(JtfApp *app, QWidget *parent = nullptr);

    void selectPath(const QString &path);
    void refresh();

signals:
    // A folder was chosen; the active pane should go there.
    void folderActivated(const QString &path);

private:
    JtfApp *m_app;
    FolderTreeModel *m_model = nullptr;
    QTreeView *m_view = nullptr;
    QString m_current;
};
