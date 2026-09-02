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
#include <QSet>

class PaneWidget;
class QSplitter;
class QJsonObject;
class QLabel;

class MainWindow : public QMainWindow {
    Q_OBJECT

public:
    /// `windowId` is the workspace window this shows. The main window is 1;
    /// a torn-off tab gets its own.
    explicit MainWindow(JtfApp *app, quint64 windowId = 1, QWidget *parent = nullptr);
    ~MainWindow() override;

    /// The workspace window this shows.
    quint64 windowId() const { return m_windowId; }

    /// Every open MainWindow, so a model change can reach all of them.
    static QList<MainWindow *> &windows();

    /// Bring every window up to date with the model, opening or closing
    /// top-level windows as the model gained or lost them.
    static void syncWindows(JtfApp *app);

protected:
    bool eventFilter(QObject *watched, QEvent *event) override;
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
    QFont fixedListFont() const;
    void buildToolbar();
    void applyCommandBindings();
    void runOperationTo(int kind);
    void runAndSettleFocus(const std::function<void()> &handler);
    void returnFocusToList();
    void connectToServer();
    void openJobs();
    void focusSearchField();
    void measureFolderSizes();
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
        OpNewFile,
        OpDuplicate
    };
    void showAttributes();
    void showSortMenu();
    void showCrumbMenu(int paneId, const QString &path, const QPoint &global);
    void showEntryMenu(int paneId, const QPoint &global, bool onEntry);
    void toggleTree();
    /// Move the keyboard to the next visible area: places, folder tree, panes.
    void focusNextArea();
    void setTreeVisible(bool visible);
    /// Give the sidebar its remembered width, or a sensible one.
    void applySidebarWidth();
    void syncTree();
    // The title and proxy icon, named after the focused pane's folder.
    void syncWindowTitle();
    void setFontPoints(int points);
    void openShortcuts();
    QString profileLabel(const QString &profile) const;
    void toggleKeymap();
    void setKeymap(const QString &name);
    void announceKeymap(const QString &name);
    void runCommand(const QString &id);
    void toggleBookmark();
    void setInspectorVisible(bool visible);
    void setInspectorPosition(int position);
    int m_inspectorPosition = 0;
    class QSplitter *m_paneColumn = nullptr;
    void setKeyHintsVisible(bool visible);
    void showKeyHintMenu(const QPoint &global);
    void setKeyHintDensity(int density);
    void syncKeyHints();
    void syncInspector();
    void showInspectorTarget();
    class QTimer *m_inspectorSettle = nullptr;
    void openViewer();
    void quickLookSelection();
    void openBatchRename();
    void openPalette();
    void openSettings();
    void runOperation(OperationRequest request);
    void runDrop(int pane, const QStringList &paths, bool fromUs);
    QStringList targetPaths() const;
    void clipboardPut(bool cut);
    void markByPattern(bool mark);
    void clipboardPaste();
    void copyText(bool fullPath);
    void revealSelection();
    void openTerminalHere();
    void editSelection();
    void updateOperationUi();
    void syncToolbar();
    void markActivePane();
    PaneWidget *activePane() const;
    // A pane this window actually owns, for deciding what this window's own
    // chrome should say. `activePane` is null whenever the active pane lives
    // in another window, and the toolbar of a window still has to be right.
    int chromePaneId() const;
    // Set on the secondary windows while the main window is quitting, so that
    // the cascade of closes does not mistake "the program is exiting" for
    // "the user dismissed this window" and throw the workspace away.
    bool m_quitting = false;
    // Folder measuring runs on a worker thread; this reads its progress.
    QTimer *m_measurePoll = nullptr;
    int m_measureCount = 0;
    // Set when a new file was just created, so it can be opened for editing
    // once the listing has settled and the cursor has landed on it.
    bool m_editAfterCreate = false;
    /// Ask for a server's password when it could not sign in - once per
    /// failure, not once per refresh.
    void askForServerPassword(int pane);
    /// Raise the sign-in prompt for any pane whose server refused it. Called
    /// from the pump too: the refusal arrives on a worker thread, long after
    /// whatever refresh started the attempt.
    void checkServerCredentials();
    QSet<int> m_askedForPassword;
    // Extraction and compression, watched the same way measuring is.
    QTimer *m_archivePoll = nullptr;
    void extractArchive();
    // Extract `members` - empty means all - from `archive`, asking where to.
    void extractInto(const QString &archive, const QStringList &members);
    // Show what is inside the archive under the cursor, in its own window.
    bool openArchiveWindow();
    // Compare the focused pane's folder against the target pane's.
    void openCompareWindow();
    // Where the space went, under `path` or under the focused pane's folder.
    void openUsageWindow(const QString &path);
    void compressSelection();
    void watchArchiveJob();
    QString tr_(const char *key) const;
    QByteArray familyUtf8() const;

    JtfApp *m_app;
    quint64 m_windowId = 1;
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
    class QAction *m_listModeAction = nullptr;
    class QAction *m_gridModeAction = nullptr;
    class KeyHintBar *m_keyHints = nullptr;
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
    class StatusLabel *m_statusMessage = nullptr;
    /// Whether the message area is showing the idle text rather than a report.
    bool m_statusIsIdle = true;
    class QLabel *m_statusPanes = nullptr;
    class QLabel *m_statusSelection = nullptr;
    class QLabel *m_statusItems = nullptr;
    class QLabel *m_statusTasks = nullptr;
    class QToolButton *m_statusKeymap = nullptr;
    class ModeSwitch *m_modeSwitch = nullptr;
    QAction *m_searchIconAction = nullptr;
    class QToolButton *m_keyHintsButton = nullptr;
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
    class QMenu *m_tabMenu = nullptr;
    class QMenu *m_toolsMenu = nullptr;
    class QMenu *m_helpMenu = nullptr;
    QList<QPair<QAction *, const char *>> m_translatable;
    // Actions bound to a command id: label from the catalogue, shortcut from
    // the keymap.
    QList<QPair<QAction *, const char *>> m_commandActions;
    QList<QPair<QMenu *, const char *>> m_translatableMenus;
};
