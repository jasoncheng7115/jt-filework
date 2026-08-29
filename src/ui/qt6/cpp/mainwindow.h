// The window: builds a splitter tree from the layout Rust reports, drives the
// event-loop pump, and owns the menus.
//
// It holds no model state of its own. Rebuilding the layout from JSON keeps
// the recursive split tree (AGENTS.md 6) the single source of truth, instead
// of a second tree of widgets drifting away from it.
#pragma once

#include "bridge.h"
#include "icons.h"
#include "theme.h"

#include <QHash>
#include <QByteArray>
#include <QFont>
#include <QMainWindow>

class PaneWidget;
class QSplitter;
class QJsonObject;
class QLabel;

class MainWindow : public QMainWindow {
    Q_OBJECT

public:
    explicit MainWindow(JtfApp *app, QWidget *parent = nullptr);

protected:
    void closeEvent(QCloseEvent *event) override;

private:
    void buildMenus();
    void rebuildLayout();
    QWidget *buildNode(const QJsonObject &node);
    void refreshAll();
    void retranslate();
    void updateStatus();
    void applyTheme();
    void applyFont();
    QFont listFont() const;
    void buildToolbar();
    void applyCommandBindings();
    void stepFontSize(int delta);
    void chooseFontFamily();

    // What a menu item asks for. Kept separate from ops::Kind because rename
    // and new folder are not the same shape as copy and move.
    enum OperationRequest { OpCopy, OpMove, OpTrash, OpDelete, OpRename, OpNewFolder };
    void openSettings();
    void runOperation(OperationRequest request);
    void updateOperationUi();
    void syncToolbar();
    void markActivePane();
    QString tr_(const char *key) const;
    QByteArray familyUtf8() const;

    JtfApp *m_app;
    QWidget *m_root = nullptr;
    QHash<int, PaneWidget *> m_panes;
    QString m_layoutSignature;
    class QLineEdit *m_pathEdit = nullptr;
    class QAction *m_backAction = nullptr;
    class QAction *m_forwardAction = nullptr;
    class QAction *m_upAction = nullptr;
    class QAction *m_refreshAction = nullptr;
    Theme m_theme;
    class QLabel *m_statusMessage = nullptr;
    class QProgressBar *m_progress = nullptr;
    class QPushButton *m_cancelButton = nullptr;
    bool m_applyingTheme = false;

    QMenu *m_fileMenu = nullptr;
    QMenu *m_viewMenu = nullptr;
    QMenu *m_goMenu = nullptr;
    QList<QPair<QAction *, const char *>> m_translatable;
    // Actions bound to a command id: label from the catalogue, shortcut from
    // the keymap.
    QList<QPair<QAction *, const char *>> m_commandActions;
    QList<QPair<QMenu *, const char *>> m_translatableMenus;
};
