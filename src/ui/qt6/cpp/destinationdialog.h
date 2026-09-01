// Where a copy or a move should go.
//
// The two-pane keys are the fast path and stay: with two panes open, C and M
// mean "to the other one" and that is one keystroke. But CView's M asked for a
// destination, and with a single pane "the other pane" does not exist - which
// is how pressing M ended up planning a move into the folder the files were
// already in.
//
// So the key opens this instead: every open tab in every pane, listed with the
// folder it is showing, plus a path to type and a folder to browse for. The
// tab that C and M would have used is preselected, so the two-pane habit still
// works by pressing Enter.
#pragma once

#include "bridge.h"

#include <QDialog>
#include <QString>

class QLineEdit;
class QListWidget;
class QTreeView;
class FolderTreeModel;

class DestinationDialog : public QDialog {
    Q_OBJECT

public:
    /// `moving` only changes the wording; the caller runs the operation.
    ///
    /// `count` is how many entries are about to move, so the title can say so
    /// - "移動到" alone does not tell you whether you are about to move the one
    /// file under the cursor or the two hundred you marked an hour ago.
    DestinationDialog(JtfApp *app, bool moving, int count, QWidget *parent);

    /// The chosen folder, or empty if the dialog was dismissed.
    QString destination() const;

private:
    QString tr_(const char *key) const;
    void addOpenTabs();

    JtfApp *m_app = nullptr;
    QListWidget *m_tabs = nullptr;
    QTreeView *m_tree = nullptr;
    FolderTreeModel *m_treeModel = nullptr;
    QLineEdit *m_path = nullptr;
};
