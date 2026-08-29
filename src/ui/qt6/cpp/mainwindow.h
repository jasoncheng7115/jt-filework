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
#include <functional>
#include <QPoint>
#include <QStringList>
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
    void updateStatusSummary();
    void applyTheme();
    void applyFont();
    QFont listFont() const;
    void buildToolbar();
    void applyCommandBindings();
    void stepFontSize(int delta);
    void chooseFontFamily();

    // What a menu item asks for. Kept separate from ops::Kind because rename
    // and new folder are not the same shape as copy and move.
    enum OperationRequest {
        OpCopy,
        OpMove,
        OpTrash,
        OpDelete,
        OpRename,
        OpNewFolder,
        OpDuplicate
    };
    void showCrumbMenu(int paneId, const QString &path, const QPoint &global);
    void showEntryMenu(int paneId, const QPoint &global, bool onEntry);
    void toggleTree();
    void setTreeVisible(bool visible);
    void syncTree();
    void setFontPoints(int points);
    void openShortcuts();
    QString profileLabel(const QString &profile) const;
    void toggleKeymap();
    void setKeymap(const QString &name);
    void announceKeymap(const QString &name);
    void runCommand(const QString &id);
    void toggleBookmark();
    void setInspectorVisible(bool visible);
    void syncInspector();
    void openViewer();
    void quickLookSelection();
    void openBatchRename();
    void openPalette();
    void openSettings();
    void runOperation(OperationRequest request);
    void runDrop(int pane, const QStringList &paths, int kind);
    QStringList targetPaths() const;
    void clipboardPut(bool cut);
    void markByPattern(bool mark);
    void clipboardPaste();
    void copyText(bool fullPath);
    void revealSelection();
    void updateOperationUi();
    void syncToolbar();
    void markActivePane();
    PaneWidget *activePane() const;
    QString tr_(const char *key) const;
    QByteArray familyUtf8() const;

    JtfApp *m_app;
    QWidget *m_root = nullptr;
    class QSplitter *m_outer = nullptr;
    class FolderTree *m_tree = nullptr;
    QWidget *m_paneArea = nullptr;
    QHash<int, PaneWidget *> m_panes;
    QString m_layoutSignature;
    class QLineEdit *m_searchEdit = nullptr;
    class QAction *m_backAction = nullptr;
    class QAction *m_forwardAction = nullptr;
    class QAction *m_upAction = nullptr;
    class QAction *m_refreshAction = nullptr;
    class QAction *m_treeAction = nullptr;
    class QAction *m_inspectorAction = nullptr;
    class Inspector *m_inspector = nullptr;
    class PlacesList *m_places = nullptr;
    class QSplitter *m_sidebar = nullptr;
    class QAction *m_hiddenAction = nullptr;
    // Which glyph each toolbar action draws, so they can be redrawn when the
    // theme changes.
    QHash<class QAction *, glyph::Shape> m_toolbarShapes;
    // Command id to handler, so anything the menus can do the palette can do.
    QHash<QString, std::function<void()>> m_handlers;
    Theme m_theme;
    class QLabel *m_statusMessage = nullptr;
    /// Whether the message area is showing the idle text rather than a report.
    bool m_statusIsIdle = true;
    class QLabel *m_statusPanes = nullptr;
    class QLabel *m_statusSelection = nullptr;
    class QLabel *m_statusItems = nullptr;
    class QLabel *m_statusTasks = nullptr;
    class QLabel *m_statusKeymap = nullptr;
    class ModeSwitch *m_modeSwitch = nullptr;
    class QSlider *m_zoom = nullptr;
    class QProgressBar *m_progress = nullptr;
    class QPushButton *m_cancelButton = nullptr;
    class QAction *m_undoAction = nullptr;
    class ViewerWindow *m_viewer = nullptr;
    // Whether the last clipboard put was a cut. Remembered here because
    // there is no portable convention for it in the clipboard itself.
    bool m_clipboardIsCut = false;
    bool m_applyingTheme = false;

    QMenu *m_fileMenu = nullptr;
    QMenu *m_editMenu = nullptr;
    QMenu *m_viewMenu = nullptr;
    QMenu *m_goMenu = nullptr;
    QList<QPair<QAction *, const char *>> m_translatable;
    // Actions bound to a command id: label from the catalogue, shortcut from
    // the keymap.
    QList<QPair<QAction *, const char *>> m_commandActions;
    QList<QPair<QMenu *, const char *>> m_translatableMenus;
};
