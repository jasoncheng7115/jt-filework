#include "mainwindow.h"
#include "jtfstring.h"
#include "panewidget.h"
#include "icons.h"
#include "batchrenamedialog.h"
#include "commandpalette.h"
#include "foldertree.h"
#include "inspector.h"
#include "placeslist.h"
#include "operations.h"
#include "platform/quicklook.h"
#include "settingsdialog.h"
#include "shortcutsdialog.h"
#include "viewerwindow.h"
#include "theme.h"

#include <QAction>
#include <QActionGroup>
#include <QApplication>
#include <QCloseEvent>
#include <QJsonDocument>
#include <QJsonObject>
#include <QLabel>
#include <QClipboard>
#include <QSet>
#include <cstring>
#include <QFontDatabase>
#include <QMimeData>
#include <QProgressBar>
#include <QPushButton>
#include <QInputDialog>
#include <QLineEdit>
#include <QStyle>
#include <QSlider>
#include <QToolBar>
#include <QToolButton>
#include <QMenu>
#include <QMenuBar>
#include <QSplitter>
#include <QStatusBar>
#include <QStyleHints>
#include <QTimer>
#include <QVBoxLayout>

namespace {

// The range the zoom slider offers. Below the minimum the list stops being
// readable; above the maximum a row is taller than the icons in it.
constexpr int kMinFontPoints = 9;
constexpr int kMaxFontPoints = 22;
constexpr int kPumpIntervalMs = 16; // one frame at 60Hz
}

MainWindow::MainWindow(JtfApp *app, QWidget *parent) : QMainWindow(parent), m_app(app) {
    setMinimumSize(720, 420);
    resize(1180, 760);

    // Built before the layout, because rebuildLayout puts the pane area into
    // it.
    m_outer = new QSplitter(Qt::Horizontal, this);
    m_outer->setObjectName(QStringLiteral("JtfOuter"));
    m_outer->setChildrenCollapsible(false);
    m_outer->setHandleWidth(4);
    // Places above the tree, in one vertical splitter: the list you use is
    // short and the tree you explore with wants the rest of the height, and
    // where the line between them falls is the user's call.
    m_sidebar = new QSplitter(Qt::Vertical, m_outer);
    m_sidebar->setObjectName(QStringLiteral("JtfSidebar"));
    m_sidebar->setChildrenCollapsible(false);
    m_sidebar->setHandleWidth(4);
    m_sidebar->setMinimumWidth(140);
    m_sidebar->setVisible(false);
    m_places = new PlacesList(m_app, m_sidebar);
    m_tree = new FolderTree(m_app, m_sidebar);
    m_sidebar->addWidget(m_places);
    m_sidebar->addWidget(m_tree);
    m_sidebar->setStretchFactor(0, 0);
    m_sidebar->setStretchFactor(1, 1);
    m_outer->addWidget(m_sidebar);

    connect(m_places, &PlacesList::locationActivated, this, [this](const QString &path) {
        const QByteArray utf8 = path.toUtf8();
        jtf_navigate(m_app, jtf_active_pane(m_app), utf8.constData());
        refreshAll();
    });
    connect(m_places, &PlacesList::placesChanged, this,
            [this] { jtf_app_save_session(m_app); });
    // Added last so the outer splitter reads left to right as sidebar,
    // panes, inspector. The pane area is inserted between them in
    // rebuildLayout, which finds its slot by pointer rather than by index -
    // an index here was correct until the inspector was added, and then
    // silently replaced the wrong widget.
    m_inspector = new Inspector(m_app, m_outer);
    m_outer->addWidget(m_inspector);
    m_inspector->setVisible(false);
    connect(m_inspector, &Inspector::closeRequested, this, [this] { setInspectorVisible(false); });
    setCentralWidget(m_outer);

    connect(m_tree, &FolderTree::folderActivated, this, [this](const QString &path) {
        const QByteArray utf8 = path.toUtf8();
        jtf_navigate(m_app, jtf_active_pane(m_app), utf8.constData());
        refreshAll();
    });
    // Remember the width as it is dragged, so it survives a restart.
    connect(m_outer, &QSplitter::splitterMoved, this, [this](int, int) {
        if (m_sidebar->isVisible()) {
            jtf_set_tree_state(m_app, 1, m_outer->sizes().value(0));
        }
        if (m_inspector->isVisible()) {
            jtf_set_inspector_state(m_app, 1, m_outer->sizes().last());
        }
    });

    m_statusMessage = new QLabel(this);
    m_progress = new QProgressBar(this);
    m_progress->setMaximumWidth(180);
    m_progress->setTextVisible(false);
    m_progress->setVisible(false);
    m_cancelButton = new QPushButton(this);
    m_cancelButton->setVisible(false);
    m_cancelButton->setFlat(true);
    connect(m_cancelButton, &QPushButton::clicked, this, [this] { jtf_op_cancel(m_app); });
    // The right-hand side of the status bar answers "what is the workspace
    // as a whole doing" - counts summed over every pane, not just the active
    // one, because the panes are the reason you opened four of them.
    m_statusPanes = new QLabel(this);
    m_statusSelection = new QLabel(this);
    m_statusItems = new QLabel(this);
    m_statusTasks = new QLabel(this);
    m_statusKeymap = new QLabel(this);
    m_statusKeymap->setCursor(Qt::PointingHandCursor);
    for (QLabel *label :
         {m_statusPanes, m_statusSelection, m_statusItems, m_statusTasks, m_statusKeymap}) {
        label->setProperty("jtfStatusSummary", true);
    }
    statusBar()->addWidget(m_statusMessage, 1);
    statusBar()->addPermanentWidget(m_statusPanes);
    statusBar()->addPermanentWidget(m_statusSelection);
    statusBar()->addPermanentWidget(m_statusItems);
    statusBar()->addPermanentWidget(m_statusTasks);
    statusBar()->addPermanentWidget(m_statusKeymap);
    // Font size, bottom right, as the reference layout has it. The two
    // commands already exist and are on the keyboard; this is the same thing
    // for a hand on the mouse, and it shows the current size, which a pair of
    // shortcuts cannot.
    auto *zoom = new QWidget(this);
    auto *zoomRow = new QHBoxLayout(zoom);
    zoomRow->setContentsMargins(8, 0, 6, 0);
    zoomRow->setSpacing(6);
    auto *smaller = new QLabel(QStringLiteral("A"), zoom);
    smaller->setProperty("jtfZoomMark", true);
    QFont smallMark = smaller->font();
    smallMark.setPointSizeF(smallMark.pointSizeF() * 0.85);
    smaller->setFont(smallMark);
    m_zoom = new QSlider(Qt::Horizontal, zoom);
    m_zoom->setObjectName(QStringLiteral("JtfZoom"));
    m_zoom->setRange(kMinFontPoints, kMaxFontPoints);
    m_zoom->setFixedWidth(96);
    m_zoom->setPageStep(1);
    auto *larger = new QLabel(QStringLiteral("A"), zoom);
    larger->setProperty("jtfZoomMark", true);
    QFont bigMark = larger->font();
    bigMark.setPointSizeF(bigMark.pointSizeF() * 1.25);
    larger->setFont(bigMark);
    zoomRow->addWidget(smaller);
    zoomRow->addWidget(m_zoom);
    zoomRow->addWidget(larger);
    connect(m_zoom, &QSlider::valueChanged, this, [this](int points) {
        setFontPoints(points);
    });
    statusBar()->addPermanentWidget(zoom);

    statusBar()->addPermanentWidget(m_progress);
    statusBar()->addPermanentWidget(m_cancelButton);

    buildMenus();
    buildToolbar();
    rebuildLayout();
    applyTheme();
    applyFont();
    setTreeVisible(jtf_tree_visible(m_app) != 0);
    retranslate();
    // The toolbar was only ever filled in by refreshAll, which does not run
    // until something changes - so on the first frame the path field and the
    // mode switch were blank.
    syncToolbar();

    // The file list is where the keyboard belongs. Without this the path
    // field keeps the focus it got by being the first focusable widget built,
    // and every key the user presses - Home, the arrows, a letter that is a
    // command in CView mode - goes into a text box instead of the list.
    if (PaneWidget *pane = activePane()) {
        pane->focusList();
    }

    // The pump is the whole "never block the UI thread" contract in one
    // place: enumeration happens on worker threads, and the UI collects
    // whatever is ready, on a frame boundary, without ever waiting
    // (AGENTS.md 3).
    // Follow System means following it while running, not only at launch
    // (AGENTS.md 12). colorSchemeChanged fires when the OS appearance
    // changes, and - unlike a palette-change event - not when we set a
    // palette ourselves.
    connect(QApplication::styleHints(), &QStyleHints::colorSchemeChanged, this,
            [this](Qt::ColorScheme) { applyTheme(); });

    auto *timer = new QTimer(this);
    connect(timer, &QTimer::timeout, this, [this] {
        if (jtf_app_pump(m_app)) {
            // While a directory streams in, only the rows and the counters
            // change. Rebuilding splitters and re-resolving every menu label
            // on each of four hundred batches is work nobody asked for.
            for (auto *pane : std::as_const(m_panes)) {
                pane->refreshRows();
            }
            updateStatus();
            updateOperationUi();
        }
    });
    timer->start(kPumpIntervalMs);
}

QByteArray MainWindow::familyUtf8() const {
    return jtfText([&](char *buf, int len) { return jtf_font_family(m_app, buf, len); }).toUtf8();
}

QString MainWindow::tr_(const char *key) const {
    return jtfText([&](char *buf, int len) { return jtf_tr(m_app, key, buf, len); });
}

void MainWindow::buildMenus() {
    // Every action carries a command id. Its label comes from the localization
    // catalogue and its shortcut from the active keymap, so the whole keyboard
    // layout is data: switching preset re-reads a file, and a settings screen
    // for it is an editor over that data rather than new code
    // (AGENTS.md 9, docs/UI_UX_SPEC.md 7).
    const auto command = [this](QMenu *menu, const char *id, std::function<void()> handler) {
        auto *action = new QAction(this);
        connect(action, &QAction::triggered, this, [this, handler] {
            handler();
            refreshAll();
        });
        menu->addAction(action);
        m_commandActions.append({action, id});
        // The same handler, reachable by id: this is what lets the palette
        // invoke anything the menus can without a second implementation.
        m_handlers.insert(QString::fromLatin1(id), handler);
        return action;
    };

    // Menu entries that are settings rather than commands: they have a label
    // but no place in the keymap.
    const auto setting = [this](QMenu *menu, const char *key, std::function<void()> handler) {
        auto *action = new QAction(this);
        connect(action, &QAction::triggered, this, [this, handler] {
            handler();
            refreshAll();
        });
        menu->addAction(action);
        m_translatable.append({action, key});
        return action;
    };

    const auto paneAction = [this](std::function<void(PaneWidget *)> handler) {
        return [this, handler] {
            if (PaneWidget *pane = activePane()) {
                handler(pane);
            }
        };
    };

    // ---------------------------------------------------------------- File
    m_fileMenu = menuBar()->addMenu(QString());
    m_translatableMenus.append({m_fileMenu, "menu.file"});
    command(m_fileMenu, "tab.new", [this] { jtf_new_tab(m_app); });
    command(m_fileMenu, "tab.close", [this] {
        const int pane = jtf_active_pane(m_app);
        jtf_close_tab(m_app, pane, jtf_active_tab(m_app, pane));
    });
    command(m_fileMenu, "tab.reopen", [this] {
        // Reopening is a pane operation with no arguments; the model knows
        // which tab was closed last.
        jtf_activate_tab(m_app, jtf_active_pane(m_app), 0);
    });
    m_fileMenu->addSeparator();
    command(m_fileMenu, "file.open", paneAction([](PaneWidget *pane) { pane->openCurrentRow(); }));
    command(m_fileMenu, "file.view", [this] { openViewer(); });
    command(m_fileMenu, "preview.quicklook", [this] { quickLookSelection(); });
    m_fileMenu->addSeparator();
    command(m_fileMenu, "file.new_folder", [this] { runOperation(OpNewFolder); });
    command(m_fileMenu, "file.rename", [this] { runOperation(OpRename); });
    command(m_fileMenu, "file.batch_rename", [this] { openBatchRename(); });
    command(m_fileMenu, "file.duplicate", [this] { runOperation(OpDuplicate); });
    m_fileMenu->addSeparator();
    command(m_fileMenu, "file.copy_to_target_pane", [this] { runOperation(OpCopy); });
    command(m_fileMenu, "file.move_to_target_pane", [this] { runOperation(OpMove); });
    m_fileMenu->addSeparator();
    command(m_fileMenu, "file.reveal", [this] { revealSelection(); });
    m_fileMenu->addSeparator();
    command(m_fileMenu, "file.trash", [this] { runOperation(OpTrash); });
    command(m_fileMenu, "file.delete", [this] { runOperation(OpDelete); });

    // ---------------------------------------------------------------- Edit
    m_editMenu = menuBar()->addMenu(QString());
    m_translatableMenus.append({m_editMenu, "menu.edit"});
    m_undoAction = command(m_editMenu, "file.undo", [this] {
        jtf_undo(m_app);
        updateOperationUi();
    });
    m_editMenu->addSeparator();
    command(m_editMenu, "file.clipboard.cut", [this] { clipboardPut(true); });
    command(m_editMenu, "file.clipboard.copy", [this] { clipboardPut(false); });
    command(m_editMenu, "file.clipboard.paste", [this] { clipboardPaste(); });
    m_editMenu->addSeparator();
    command(m_fileMenu, "file.folder_size", [this] {
        const int measured = jtf_measure_folder_sizes(m_app, jtf_active_pane(m_app));
        m_statusIsIdle = false;
        m_statusMessage->setText(
            measured > 0
                ? jtfFill(tr_("status.measured_folders"), "count", QString::number(measured))
                : tr_("status.no_folders_selected"));
        refreshAll();
    });
    command(m_editMenu, "file.copy_path", [this] { copyText(true); });
    command(m_editMenu, "file.copy_name", [this] { copyText(false); });
    m_editMenu->addSeparator();
    command(m_editMenu, "file.mark.toggle", paneAction([this](PaneWidget *pane) {
        if (pane->currentRow() >= 0) {
            jtf_toggle_mark(m_app, pane->paneId(), pane->currentRow());
            pane->advanceCurrentRow();
        }
    }));
    command(m_editMenu, "file.mark.all",
            [this] { jtf_mark_listed(m_app, jtf_active_pane(m_app), 0); });
    command(m_editMenu, "file.mark.none",
            [this] { jtf_mark_listed(m_app, jtf_active_pane(m_app), 1); });
    command(m_editMenu, "file.mark.invert",
            [this] { jtf_mark_listed(m_app, jtf_active_pane(m_app), 2); });
    command(m_editMenu, "file.mark.pattern", [this] { markByPattern(true); });
    command(m_editMenu, "file.unmark.pattern", [this] { markByPattern(false); });
    m_editMenu->addSeparator();
    command(m_editMenu, "search.open", paneAction([](PaneWidget *pane) { pane->toggleSearch(); }));
    command(m_editMenu, "search.clear", paneAction([](PaneWidget *pane) { pane->clearSearch(); }));
    command(m_editMenu, "view.filter", paneAction([](PaneWidget *pane) { pane->toggleFilter(); }));

    // ---------------------------------------------------------------- View
    m_viewMenu = menuBar()->addMenu(QString());
    m_translatableMenus.append({m_viewMenu, "menu.view"});
    command(m_viewMenu, "view.tree", [this] { toggleTree(); });
    command(m_viewMenu, "keymap.toggle", [this] { toggleKeymap(); });
    command(m_viewMenu, "help.shortcuts", [this] { openShortcuts(); });
    command(m_viewMenu, "view.inspector",
            [this] { setInspectorVisible(!m_inspector->isVisible()); });
    command(m_viewMenu, "view.hidden",
            [this] { jtf_set_show_hidden(m_app, jtf_show_hidden(m_app) ? 0 : 1); });
    command(m_viewMenu, "view.refresh", [this] { jtf_refresh(m_app, jtf_active_pane(m_app)); });
    m_viewMenu->addSeparator();
    command(m_viewMenu, "workspace.split.horizontal", [this] { jtf_split_active(m_app, 0); });
    command(m_viewMenu, "workspace.split.vertical", [this] { jtf_split_active(m_app, 1); });
    command(m_viewMenu, "workspace.pane.close", [this] { jtf_close_active_pane(m_app); });
    command(m_viewMenu, "workspace.pane.next", [this] { jtf_focus_next_pane(m_app); });
    command(m_viewMenu, "workspace.pane.previous", [this] {
        // Cycling forward n-1 times is one step back, and needs no second
        // traversal order to keep in agreement with the first.
        const int panes = jtf_pane_count(m_app);
        for (int i = 1; i < panes; ++i) {
            jtf_focus_next_pane(m_app);
        }
    });
    m_viewMenu->addSeparator();
    command(m_viewMenu, "workspace.preset.single", [this] { jtf_apply_preset(m_app, 0); });
    command(m_viewMenu, "workspace.preset.quad", [this] { jtf_apply_preset(m_app, 3); });
    m_viewMenu->addSeparator();

    auto *themeMenu = m_viewMenu->addMenu(QString());
    m_translatableMenus.append({themeMenu, "menu.theme"});
    setting(themeMenu, "theme.system", [this] { jtf_set_theme_mode(m_app, 0); });
    setting(themeMenu, "theme.light", [this] { jtf_set_theme_mode(m_app, 1); });
    setting(themeMenu, "theme.dark", [this] { jtf_set_theme_mode(m_app, 2); });

    auto *fontMenu = m_viewMenu->addMenu(QString());
    m_translatableMenus.append({fontMenu, "menu.font"});
    setting(fontMenu, "font.system_mono", [this] { jtf_set_font(m_app, "", 0, 1); });
    setting(fontMenu, "font.system_proportional", [this] { jtf_set_font(m_app, "", 0, 0); });
    fontMenu->addSeparator();
    command(fontMenu, "view.font.smaller", [this] { stepFontSize(-1); });
    command(fontMenu, "view.font.larger", [this] { stepFontSize(1); });
    fontMenu->addSeparator();
    setting(fontMenu, "font.choose", [this] { chooseFontFamily(); });

    auto *keymapMenu = m_viewMenu->addMenu(QString());
    m_translatableMenus.append({keymapMenu, "menu.keymap"});
    setting(keymapMenu, "keyboard.profile.single_key",
            [this] { jtf_set_keymap(m_app, "single-key"); });
    setting(keymapMenu, "keyboard.profile.native",
            [this] { jtf_set_keymap(m_app, "native"); });

    auto *localeMenu = m_viewMenu->addMenu(QString());
    m_translatableMenus.append({localeMenu, "menu.language"});
    setting(localeMenu, "language.english", [this] { jtf_set_locale(m_app, "en"); });
    setting(localeMenu, "language.zh_tw", [this] { jtf_set_locale(m_app, "zh-TW"); });

    m_viewMenu->addSeparator();
    command(m_viewMenu, "command.palette", [this] { openPalette(); });
    command(m_viewMenu, "settings.open", [this] { openSettings(); });

    // ------------------------------------------------------------------ Go
    m_goMenu = menuBar()->addMenu(QString());
    m_translatableMenus.append({m_goMenu, "menu.go"});
    command(m_goMenu, "nav.back", [this] { jtf_go_back(m_app, jtf_active_pane(m_app)); });
    command(m_goMenu, "nav.forward", [this] { jtf_go_forward(m_app, jtf_active_pane(m_app)); });
    command(m_goMenu, "nav.up", [this] { jtf_navigate_up(m_app, jtf_active_pane(m_app)); });
    m_goMenu->addSeparator();
    command(m_goMenu, "file.bookmark", [this] { toggleBookmark(); });
    m_goMenu->addSeparator();
    command(m_goMenu, "nav.home", [this] {
        const QByteArray home = qgetenv("HOME");
        if (!home.isEmpty()) {
            jtf_navigate(m_app, jtf_active_pane(m_app), home.constData());
        }
    });
    command(m_goMenu, "nav.goto", [this] {
        m_pathEdit->setFocus();
        m_pathEdit->selectAll();
    });
    m_goMenu->addSeparator();
    command(m_goMenu, "tab.next", [this] {
        // Tab cycling is a pane operation; the model owns the order.
        const int pane = jtf_active_pane(m_app);
        const int count = jtf_tab_count(m_app, pane);
        if (count > 0) {
            jtf_activate_tab(m_app, pane, (jtf_active_tab(m_app, pane) + 1) % count);
        }
    });
    command(m_goMenu, "tab.previous", [this] {
        const int pane = jtf_active_pane(m_app);
        const int count = jtf_tab_count(m_app, pane);
        if (count > 0) {
            jtf_activate_tab(m_app, pane, (jtf_active_tab(m_app, pane) + count - 1) % count);
        }
    });
}

void MainWindow::quickLookSelection() {
    PaneWidget *pane = activePane();
    if (!pane || pane->currentRow() < 0) {
        return;
    }
    const int row = pane->currentRow();
    quicklook::toggle(jtfText([&](char *buf, int len) {
        return jtf_row_path(m_app, pane->paneId(), row, buf, len);
    }));
}

void MainWindow::openBatchRename() {
    BatchRenameDialog dialog(m_app, jtf_active_pane(m_app), this);
    dialog.exec();
    refreshAll();
}

// --------------------------------------------------------------- operations

QStringList MainWindow::targetPaths() const {
    const QString joined = jtfText([&](char *buf, int len) {
        return jtf_target_paths(m_app, jtf_active_pane(m_app), buf, len);
    });
    return joined.isEmpty() ? QStringList() : joined.split(QLatin1Char('\n'));
}

void MainWindow::clipboardPut(bool cut) {
    const QStringList paths = targetPaths();
    if (paths.isEmpty()) {
        return;
    }
    QList<QUrl> urls;
    urls.reserve(paths.size());
    for (const QString &path : paths) {
        urls.append(QUrl::fromLocalFile(path));
    }

    // File URLs, so pasting into the system file manager works. Whether it was
    // a cut is remembered here rather than encoded in the clipboard: there is
    // no portable convention for it, and inventing one that another
    // application misreads would move files nobody asked to move.
    auto *data = new QMimeData;
    data->setUrls(urls);
    data->setText(paths.join(QLatin1Char('\n')));
    QGuiApplication::clipboard()->setMimeData(data);

    m_clipboardIsCut = cut;
    m_statusIsIdle = false;
    m_statusMessage->setText(tr_("status.copied"));
}

void MainWindow::markByPattern(bool mark) {
    // The pattern language is the one the search box already uses, so there is
    // only one wildcard syntax in the program to learn (docs/SEARCH_AI.md 3).
    bool accepted = false;
    const QString pattern =
        QInputDialog::getText(this,
                              mark ? tr_("prompt.pattern_title") : tr_("prompt.unmark_title"),
                              tr_("prompt.pattern_label"),
                              QLineEdit::Normal,
                              QStringLiteral("*"),
                              &accepted);
    if (!accepted || pattern.isEmpty()) {
        return;
    }
    const QByteArray utf8 = pattern.toUtf8();
    const int count =
        jtf_mark_pattern(m_app, jtf_active_pane(m_app), utf8.constData(), mark ? 1 : 0);
    // Say how many matched: a pattern that matched nothing looks identical to
    // one that was ignored, and the difference matters.
    statusBar()->showMessage(jtfFill(tr_("status.marked_count"), "count", QString::number(count)),
                             4000);
    refreshAll();
}

void MainWindow::clipboardPaste() {
    const QMimeData *data = QGuiApplication::clipboard()->mimeData();
    if (!data || !data->hasUrls()) {
        m_statusIsIdle = false;
        m_statusMessage->setText(tr_("status.clipboard_empty"));
        return;
    }
    QStringList paths;
    for (const QUrl &url : data->urls()) {
        if (url.isLocalFile()) {
            paths << url.toLocalFile();
        }
    }
    if (paths.isEmpty()) {
        m_statusIsIdle = false;
        m_statusMessage->setText(tr_("status.clipboard_empty"));
        return;
    }

    // A paste from elsewhere copies. Only a cut this application made moves,
    // and it moves once: the clipboard still holds the paths afterwards, and
    // pasting them again would be a second move of files that are no longer
    // there.
    const int kind = m_clipboardIsCut ? 1 : 0;
    m_clipboardIsCut = false;
    runDrop(jtf_active_pane(m_app), paths, kind);
}

void MainWindow::copyText(bool fullPath) {
    const int pane = jtf_active_pane(m_app);
    const QString text = jtfText([&](char *buf, int len) {
        return fullPath ? jtf_target_paths(m_app, pane, buf, len)
                        : jtf_target_names(m_app, pane, buf, len);
    });
    if (text.isEmpty()) {
        return;
    }
    QGuiApplication::clipboard()->setText(text);
    m_statusIsIdle = false;
    m_statusMessage->setText(tr_("status.copied"));
}

void MainWindow::revealSelection() {
    const QStringList paths = targetPaths();
    if (!paths.isEmpty()) {
        platform::reveal(paths.first());
    }
}

void MainWindow::runDrop(int pane, const QStringList &paths, int kind) {
    if (jtf_op_running(m_app)) {
        return;
    }
    const QByteArray joined = paths.join(QLatin1Char('\n')).toUtf8();
    if (!jtf_op_prepare_drop(m_app, pane, kind, joined.constData())) {
        // Dropping into the folder something already lives in is the common
        // accident, and produces no plan; saying nothing is right there.
        const QString key =
            jtfText([&](char *buf, int len) { return jtf_op_error_key(m_app, buf, len); });
        if (!key.isEmpty()) {
            const QByteArray utf8 = key.toUtf8();
            m_statusIsIdle = false;
            m_statusMessage->setText(jtfText(
                [&](char *buf, int len) { return jtf_tr(m_app, utf8.constData(), buf, len); }));
        }
        return;
    }

    int policy = 0;
    if (jtf_op_conflicts(m_app) > 0) {
        policy = ops::askConflictPolicy(m_app, this, jtf_op_conflicts(m_app));
        if (policy < 0) {
            return;
        }
    }
    jtf_op_start(m_app, policy);
    updateOperationUi();
}

void MainWindow::runOperation(OperationRequest request) {
    const int pane = jtf_active_pane(m_app);
    QString message;
    bool started = false;

    switch (request) {
    case OpCopy:
        started = ops::confirmAndStart(m_app, this, pane, ops::Copy, &message);
        break;
    case OpMove:
        started = ops::confirmAndStart(m_app, this, pane, ops::Move, &message);
        break;
    case OpTrash:
        started = ops::confirmAndStart(m_app, this, pane, ops::Trash, &message);
        break;
    case OpDelete:
        started = ops::confirmAndStart(m_app, this, pane, ops::Delete, &message);
        break;
    case OpRename:
        started = ops::renameSelection(m_app, this, pane, &message);
        break;
    case OpNewFolder:
        started = ops::createFolder(m_app, this, pane, &message);
        break;
    case OpDuplicate:
        // Always "keep both": a duplicate that overwrote the original would
        // be a contradiction in terms.
        if (jtf_op_prepare_duplicate(m_app, pane)) {
            started = jtf_op_start(m_app, 2) != 0;
        }
        break;
    }

    // A refusal is explained rather than silently ignored
    // (docs/UI_CONVENTIONS.md 9).
    if (!started && !message.isEmpty()) {
        m_statusIsIdle = false;
        m_statusMessage->setText(message);
    }
    updateOperationUi();
}

void MainWindow::updateOperationUi() {
    const bool running = jtf_op_running(m_app) != 0;

    // Undo names what it would reverse, so the menu says "Undo Rename" rather
    // than an unqualified "Undo" that could mean anything.
    if (m_undoAction) {
        const bool can = jtf_can_undo(m_app) != 0;
        m_undoAction->setEnabled(can);
        QString label = tr_("command.file.undo");
        if (can) {
            const QString key =
                jtfText([&](char *buf, int len) { return jtf_undo_label_key(m_app, buf, len); });
            if (!key.isEmpty()) {
                const QByteArray utf8 = key.toUtf8();
                label = jtfFill(tr_("command.file.undo_named"), "what",
                                jtfText([&](char *buf, int len) {
                                    return jtf_tr(m_app, utf8.constData(), buf, len);
                                }));
            }
        }
        m_undoAction->setText(label);
    }

    m_progress->setVisible(running);
    m_cancelButton->setVisible(running);

    if (running) {
        const int percent = jtf_op_percent(m_app);
        if (percent < 0) {
            m_progress->setRange(0, 0); // indeterminate: an honest unknown
        } else {
            m_progress->setRange(0, 100);
            m_progress->setValue(percent);
        }
        const QString labelKey =
            jtfText([&](char *buf, int len) { return jtf_op_label_key(m_app, buf, len); });
        const QByteArray keyUtf8 = labelKey.toUtf8();
        const QString label = labelKey.isEmpty()
                                  ? QString()
                                  : jtfText([&](char *buf, int len) {
                                        return jtf_tr(m_app, keyUtf8.constData(), buf, len);
                                    });
        const QString current =
            jtfText([&](char *buf, int len) { return jtf_op_current(m_app, buf, len); });
        m_statusIsIdle = false;
        m_statusMessage->setText(current.isEmpty() ? label
                                                   : label + QStringLiteral("   ") + current);
        return;
    }

    const QString result = ops::takeResult(m_app);
    if (!result.isEmpty()) {
        m_statusIsIdle = false;
        m_statusMessage->setText(result);
    }
}

// -------------------------------------------------------------- other windows

void MainWindow::openViewer() {
    PaneWidget *pane = activePane();
    if (!pane || pane->currentRow() < 0) {
        return;
    }
    if (!jtf_viewer_open(m_app, pane->paneId(), pane->currentRow())) {
        return;
    }

    // One window, because the bridge holds one viewer session. Two windows
    // fought over it: closing the first called jtf_viewer_close and killed the
    // second's session out from under it.
    if (!m_viewer) {
        // A separate window rather than a panel: AGENTS.md 14 makes the Viewer
        // stateful, and a stateful thing that disappears when the selection
        // moves is a preview wearing the wrong name.
        m_viewer = new ViewerWindow(m_app, this);
        m_viewer->setAttribute(Qt::WA_DeleteOnClose);
        connect(m_viewer, &QObject::destroyed, this, [this] { m_viewer = nullptr; });
    } else {
        m_viewer->refresh();
    }
    m_viewer->show();
    m_viewer->raise();
    m_viewer->activateWindow();
}

void MainWindow::openPalette() {
    CommandPalette palette(m_app, this);
    // Centred on the window, near the top, where every palette is.
    palette.move(frameGeometry().center().x() - palette.width() / 2,
                 frameGeometry().top() + 80);
    if (palette.exec() != QDialog::Accepted) {
        return;
    }
    const QString id = palette.chosen();
    const auto handler = m_handlers.constFind(id);
    if (handler != m_handlers.constEnd()) {
        (*handler)();
        refreshAll();
        return;
    }
    // A command with no handler yet is a registered intention, not a failure;
    // saying so beats doing nothing silently.
    m_statusIsIdle = false;
    m_statusMessage->setText(tr_("palette.unimplemented"));
}

void MainWindow::openSettings() {
    SettingsDialog dialog(m_app, this);
    // Changes apply as they are made, so the window follows along live rather
    // than waiting for the dialog to close.
    connect(&dialog, &SettingsDialog::changed, this, [this] {
        applyTheme();
        applyFont();
        refreshAll();
    });
    dialog.exec();
    refreshAll();
}

void MainWindow::showEntryMenu(int paneId, const QPoint &global, bool onEntry) {
    jtf_focus_pane(m_app, paneId);
    markActivePane();

    // Built from the same commands as the menu bar, so a command cannot exist
    // in one place and not the other, and each entry carries the shortcut the
    // keymap gives it.
    QMenu menu(this);
    const auto add = [&](const char *id, std::function<void()> handler, bool enabled = true) {
        const QString labelKey = QStringLiteral("command.%1").arg(QLatin1String(id));
        const QByteArray keyUtf8 = labelKey.toUtf8();
        QAction *action = menu.addAction(jtfText([&](char *buf, int len) {
            return jtf_tr(m_app, keyUtf8.constData(), buf, len);
        }));
        const QString shortcut =
            jtfText([&](char *buf, int len) { return jtf_shortcut_for(m_app, id, buf, len); });
        if (!shortcut.isEmpty()) {
            action->setShortcut(QKeySequence(shortcut));
        }
        action->setEnabled(enabled);
        connect(action, &QAction::triggered, this, [this, handler] {
            handler();
            refreshAll();
        });
        return action;
    };

    PaneWidget *pane = m_panes.value(paneId, nullptr);
    const bool hasTarget = onEntry && pane && pane->currentRow() >= 0;
    const bool twoPanes = jtf_pane_count(m_app) > 1;

    if (hasTarget) {
        add("file.open", [pane] { pane->openCurrentRow(); });
        add("file.view", [this] { openViewer(); });
        add("preview.quicklook", [this] { quickLookSelection(); });
        menu.addSeparator();
        add("file.clipboard.cut", [this] { clipboardPut(true); });
        add("file.clipboard.copy", [this] { clipboardPut(false); });
    }
    add("file.clipboard.paste", [this] { clipboardPaste(); });
    if (hasTarget) {
        add("file.copy_path", [this] { copyText(true); });
        add("file.copy_name", [this] { copyText(false); });
        menu.addSeparator();
        add("file.copy_to_target_pane", [this] { runOperation(OpCopy); }, twoPanes);
        add("file.move_to_target_pane", [this] { runOperation(OpMove); }, twoPanes);
        add("file.duplicate", [this] { runOperation(OpDuplicate); });
        add("file.rename", [this] { runOperation(OpRename); });
        add("file.batch_rename", [this] { openBatchRename(); });
        menu.addSeparator();
        add("file.mark.toggle", [this, pane] {
            jtf_toggle_mark(m_app, pane->paneId(), pane->currentRow());
            pane->advanceCurrentRow();
        });
        add("file.reveal", [this] { revealSelection(); }, platform::canReveal());
    }

    menu.addSeparator();
    add("file.new_folder", [this] { runOperation(OpNewFolder); });
    add("view.refresh", [this, paneId] { jtf_refresh(m_app, paneId); });

    if (hasTarget) {
        menu.addSeparator();
        add("file.trash", [this] { runOperation(OpTrash); });
        add("file.delete", [this] { runOperation(OpDelete); });
    }

    menu.exec(global);
}

void MainWindow::stepFontSize(int delta) {
    const int stored = jtf_font_point_size(m_app);
    const int current = stored > 0 ? stored : listFont().pointSize();
    jtf_set_font(m_app, familyUtf8().constData(), qBound(8, current + delta, 32),
                 jtf_font_monospace(m_app));
}

void MainWindow::chooseFontFamily() {
    bool accepted = false;
    const QString chosen = QInputDialog::getText(
        this, tr_("font.choose"), tr_("font.family_prompt"), QLineEdit::Normal,
        jtfText([&](char *b, int l) { return jtf_font_family(m_app, b, l); }), &accepted);
    if (!accepted) {
        return;
    }
    const QByteArray utf8 = chosen.trimmed().toUtf8();
    jtf_set_font(m_app, utf8.constData(), jtf_font_point_size(m_app), jtf_font_monospace(m_app));
    applyFont();
}

// Labels from the catalogue, shortcuts from the keymap. Called on every
// retranslate, so switching locale or keymap updates both without a restart.
void MainWindow::applyCommandBindings() {
    // A command appears in both the menu and the toolbar. Giving the shortcut
    // to both QActions makes Qt call it an ambiguous overload and stop
    // delivering it at all - the shortcut silently dies. Only the first action
    // registered for an id carries it; the others show it in their tooltip,
    // which is where a toolbar button should show it anyway.
    QSet<QString> shortcutAssigned;

    for (const auto &entry : std::as_const(m_commandActions)) {
        QAction *action = entry.first;
        const char *id = entry.second;

        const QString labelKey = QStringLiteral("command.%1").arg(QLatin1String(id));
        const QByteArray labelKeyUtf8 = labelKey.toUtf8();
        action->setText(jtfText([&](char *buf, int len) {
            return jtf_tr(m_app, labelKeyUtf8.constData(), buf, len);
        }));

        const QString shortcut = jtfText(
            [&](char *buf, int len) { return jtf_shortcut_for(m_app, id, buf, len); });
        const QString key = QString::fromLatin1(id);

        if (!shortcut.isEmpty() && !shortcutAssigned.contains(key)) {
            action->setShortcut(QKeySequence(shortcut));
            shortcutAssigned.insert(key);
        } else {
            action->setShortcut(QKeySequence());
        }

        // Toolbar buttons have no room for a label, so the tooltip carries the
        // name and the shortcut.
        action->setToolTip(shortcut.isEmpty()
                               ? action->text()
                               : action->text() + QStringLiteral("  (") + shortcut +
                                     QStringLiteral(")"));

        // A command the registry does not know about cannot be invoked; it is
        // left disabled rather than silently doing nothing.
        if (jtf_has_command(m_app, id) == 0) {
            action->setEnabled(false);
        }
        // A command the platform cannot perform is shown and disabled rather
        // than offered (docs/PLATFORM_INTEGRATION.md 1.1).
        if (std::strcmp(id, "file.reveal") == 0 && !platform::canReveal()) {
            action->setEnabled(false);
        }
    }
}

void MainWindow::buildToolbar() {
    auto *bar = addToolBar(QString());
    bar->setObjectName(QStringLiteral("JtfToolbar"));
    bar->setMovable(false);
    bar->setFloatable(false);
    bar->setIconSize(QSize(16, 16));

    // Every button is a command: same id, same handler, same shortcut as the
    // menu entry, so the toolbar cannot drift away from the rest of the UI
    // (docs/UI_CONVENTIONS.md 3).
    const auto button = [&](const char *id, glyph::Shape shape, std::function<void()> handler,
                            bool checkable = false) {
        auto *action = new QAction(this);
        action->setCheckable(checkable);
        connect(action, &QAction::triggered, this, [this, handler] {
            handler();
            refreshAll();
        });
        bar->addAction(action);
        m_commandActions.append({action, id});
        m_toolbarShapes.insert(action, shape);
        m_handlers.insert(QString::fromLatin1(id), handler);
        return action;
    };

    m_backAction = button("nav.back", glyph::Shape::ArrowLeft,
                          [this] { jtf_go_back(m_app, jtf_active_pane(m_app)); });
    m_forwardAction = button("nav.forward", glyph::Shape::ArrowRight,
                             [this] { jtf_go_forward(m_app, jtf_active_pane(m_app)); });
    m_upAction = button("nav.up", glyph::Shape::ArrowUp,
                        [this] { jtf_navigate_up(m_app, jtf_active_pane(m_app)); });
    m_refreshAction = button("view.refresh", glyph::Shape::Reload,
                             [this] { jtf_refresh(m_app, jtf_active_pane(m_app)); });

    bar->addSeparator();

    m_treeAction = button("view.tree", glyph::Shape::Sidebar, [this] { toggleTree(); }, true);
    m_inspectorAction = button(
        "view.inspector", glyph::Shape::Inspector,
        [this] { setInspectorVisible(!m_inspector->isVisible()); }, true);
    button("workspace.split.horizontal", glyph::Shape::SplitHorizontal,
           [this] { jtf_split_active(m_app, 0); });
    button("workspace.split.vertical", glyph::Shape::SplitVertical,
           [this] { jtf_split_active(m_app, 1); });

    bar->addSeparator();

    m_pathEdit = new QLineEdit(bar);
    m_pathEdit->setClearButtonEnabled(true);
    m_pathEdit->setMinimumWidth(280);
    bar->addWidget(m_pathEdit);
    // Typing a path and pressing Return navigates; Escape puts the real path
    // back, so an abandoned edit never leaves a lie on screen.
    connect(m_pathEdit, &QLineEdit::returnPressed, this, [this] {
        const QByteArray path = m_pathEdit->text().trimmed().toUtf8();
        if (!path.isEmpty()) {
            jtf_navigate(m_app, jtf_active_pane(m_app), path.constData());
            refreshAll();
        }
    });

    bar->addSeparator();

    button("file.new_folder", glyph::Shape::NewFolder, [this] { runOperation(OpNewFolder); });
    button("view.filter", glyph::Shape::Filter, [this] {
        if (PaneWidget *pane = activePane()) {
            pane->toggleFilter();
        }
    });
    button("search.open", glyph::Shape::Search, [this] {
        if (PaneWidget *pane = activePane()) {
            pane->toggleSearch();
        }
    });
    m_hiddenAction = button(
        "view.hidden", glyph::Shape::Hidden,
        [this] { jtf_set_show_hidden(m_app, jtf_show_hidden(m_app) ? 0 : 1); }, true);
    button("help.shortcuts", glyph::Shape::Keyboard, [this] { openShortcuts(); });
    button("settings.open", glyph::Shape::Settings, [this] { openSettings(); });

    // The keyboard-mode switch. A two-segment control rather than one button
    // that changes label: a lone button reading "CView" cannot say whether
    // that is the mode you are in or the mode you would get by pressing it.
    // Both modes are on screen, and the lit one is the answer.
    auto *modeHolder = new QWidget(bar);
    auto *modeRow = new QHBoxLayout(modeHolder);
    modeRow->setContentsMargins(6, 0, 2, 0);
    modeRow->setSpacing(0);
    m_modeSwitch = new QWidget(modeHolder);
    m_modeSwitch->setObjectName(QStringLiteral("JtfModeSwitch"));
    auto *modeInner = new QHBoxLayout(m_modeSwitch);
    modeInner->setContentsMargins(2, 2, 2, 2);
    modeInner->setSpacing(2);
    for (const char *name : {"single-key", "native"}) {
        auto *segment = new QToolButton(m_modeSwitch);
        segment->setCheckable(true);
        segment->setAutoRaise(true);
        // A QToolButton inherits the toolbar's icon-only style, and these
        // segments are words with no icon: without this the switch is an
        // empty box.
        segment->setToolButtonStyle(Qt::ToolButtonTextOnly);
        segment->setProperty("jtfModeSegment", true);
        segment->setProperty("jtfKeymap", QString::fromLatin1(name));
        connect(segment, &QToolButton::clicked, this, [this, name] { setKeymap(name); });
        modeInner->addWidget(segment);
        m_modeSegments.append(segment);
    }
    modeRow->addWidget(m_modeSwitch);
    bar->addWidget(modeHolder);

    auto *focusPath = new QAction(this);
    focusPath->setShortcut(QKeySequence(QStringLiteral("Ctrl+L")));
    connect(focusPath, &QAction::triggered, this, [this] {
        m_pathEdit->setFocus();
        m_pathEdit->selectAll();
    });
    addAction(focusPath);
}

void MainWindow::syncToolbar() {
    const int pane = jtf_active_pane(m_app);
    const QString path =
        jtfText([&](char *buf, int len) { return jtf_current_path(m_app, pane, buf, len); });
    if (!m_pathEdit->hasFocus() && m_pathEdit->text() != path) {
        m_pathEdit->setText(path);
    }

    // A navigation button that is always enabled teaches people that pressing
    // it does nothing.
    if (m_zoom) {
        int points = jtf_font_point_size(m_app);
        if (points <= 0) {
            // 0 means "the platform default"; show where that actually lands
            // rather than pinning the slider to its minimum.
            points = static_cast<int>(qRound(QApplication::font().pointSizeF()));
        }
        QSignalBlocker blocker(m_zoom);
        m_zoom->setValue(qBound(kMinFontPoints, points, kMaxFontPoints));
        m_zoom->setToolTip(
            jtfFill(tr_("status.font_size"), "size", QString::number(m_zoom->value())));
    }

    const QString keymap =
        jtfText([&](char *buf, int len) { return jtf_keymap_name(m_app, buf, len); });
    for (auto *segment : std::as_const(m_modeSegments)) {
        const QString name = segment->property("jtfKeymap").toString();
        QSignalBlocker blocker(segment);
        segment->setChecked(name == keymap);
        segment->setText(profileLabel(name));
        segment->setToolTip(jtfFill(tr_("keymap.switch_to"), "name", segment->text()));
    }

    m_backAction->setEnabled(jtf_can_go_back(m_app, pane) != 0);
    m_forwardAction->setEnabled(jtf_can_go_forward(m_app, pane) != 0);
    m_upAction->setEnabled(jtf_can_go_up(m_app, pane) != 0);

    // A toggle button shows what it is toggling, or it is just a button that
    // sometimes does nothing visible (docs/UI_CONVENTIONS.md 1).
    if (m_inspectorAction) {
        QSignalBlocker blocker(m_inspectorAction);
        m_inspectorAction->setChecked(m_inspector && m_inspector->isVisible());
    }
    if (m_treeAction) {
        QSignalBlocker blocker(m_treeAction);
        m_treeAction->setChecked(m_sidebar && m_sidebar->isVisible());
    }
    if (m_hiddenAction) {
        QSignalBlocker blocker(m_hiddenAction);
        m_hiddenAction->setChecked(jtf_show_hidden(m_app) != 0);
    }
}

QFont MainWindow::listFont() const {
    const QString family =
        jtfText([&](char *buf, int len) { return jtf_font_family(m_app, buf, len); });
    const int size = jtf_font_point_size(m_app);
    const bool monospace = jtf_font_monospace(m_app) != 0;

    // An empty family means the platform's own fixed-width font: Menlo or
    // SF Mono on macOS, Consolas on Windows, DejaVu Sans Mono on Linux. That
    // is the right default everywhere, needs no bundled asset and no licence,
    // and has correct CJK fallback (AGENTS.md 18.2).
    QFont font = monospace ? QFontDatabase::systemFont(QFontDatabase::FixedFont)
                           : QFontDatabase::systemFont(QFontDatabase::GeneralFont);
    if (!family.isEmpty()) {
        font.setFamily(family);
        if (monospace) {
            font.setStyleHint(QFont::Monospace, QFont::PreferMatch);
        }
    }
    if (size > 0) {
        font.setPointSize(size);
    }
    return font;
}

void MainWindow::applyFont() {
    const QFont font = listFont();
    for (auto *pane : std::as_const(m_panes)) {
        pane->setListFont(font);
    }
    if (m_inspector) {
        m_inspector->setListFont(font);
    }
    if (m_places) {
        m_places->setListFont(font);
    }
    if (m_tree) {
        m_tree->setListFont(font);
    }
}

QWidget *MainWindow::buildNode(const QJsonObject &node) {
    if (node.contains(QStringLiteral("pane"))) {
        const int paneId = node.value(QStringLiteral("pane")).toInt();
        auto *pane = new PaneWidget(m_app, paneId);
        connect(pane, &PaneWidget::focusRequested, this, [this](int id) {
            jtf_focus_pane(m_app, id);
            markActivePane();
        });
        connect(pane, &PaneWidget::stateChanged, this, [this] { refreshAll(); });
        connect(pane, &PaneWidget::commandRequested, this, &MainWindow::runCommand);
        connect(pane, &PaneWidget::contextMenuRequested, this,
                [this, paneId](const QPoint &global, bool onEntry) {
                    showEntryMenu(paneId, global, onEntry);
                });
        connect(pane, &PaneWidget::dropRequested, this,
                [this, paneId](const QStringList &paths, int kind) {
                    runDrop(paneId, paths, kind);
                });
        m_panes.insert(paneId, pane);
        return pane;
    }

    const bool vertical = node.value(QStringLiteral("vertical")).toBool();
    auto *splitter = new QSplitter(vertical ? Qt::Vertical : Qt::Horizontal);
    splitter->setChildrenCollapsible(false);
    splitter->setHandleWidth(4);
    splitter->addWidget(buildNode(node.value(QStringLiteral("first")).toObject()));
    splitter->addWidget(buildNode(node.value(QStringLiteral("second")).toObject()));

    const double ratio = node.value(QStringLiteral("ratio")).toDouble(0.5);
    const int total = 1000;
    splitter->setSizes({static_cast<int>(ratio * total), total - static_cast<int>(ratio * total)});
    return splitter;
}

void MainWindow::toggleTree() {
    setTreeVisible(!m_sidebar->isVisible());
}

void MainWindow::setTreeVisible(bool visible) {
    m_sidebar->setVisible(visible);
    if (visible) {
        // Restore the remembered width, or a sensible default the first time.
        const int width = jtf_tree_width(m_app);
        const int total = m_outer->width();
        const int sidebar = width > 0 ? width : 240;
        m_outer->setSizes({sidebar, qMax(200, total - sidebar)});
        m_places->refresh();
        syncTree();
    }
    jtf_set_tree_state(m_app, visible ? 1 : 0, m_outer->sizes().value(0));
}

void MainWindow::setKeymap(const QString &name) {
    const QString current =
        jtfText([&](char *buf, int len) { return jtf_keymap_name(m_app, buf, len); });
    if (current == name) {
        // Already there. Re-loading would be harmless but the announcement
        // would not: saying "switched" when nothing switched is a lie.
        syncToolbar();
        return;
    }
    const QByteArray utf8 = name.toUtf8();
    jtf_set_keymap(m_app, utf8.constData());
    announceKeymap(name);
}

void MainWindow::setFontPoints(int points) {
    const int clamped = qBound(kMinFontPoints, points, kMaxFontPoints);
    const QString family =
        jtfText([&](char *buf, int len) { return jtf_font_family(m_app, buf, len); });
    const QByteArray utf8 = family.toUtf8();
    jtf_set_font(m_app, utf8.constData(), clamped, jtf_font_monospace(m_app));
    jtf_app_save_session(m_app);
    applyFont();
    m_zoom->setToolTip(jtfFill(tr_("status.font_size"), "size", QString::number(clamped)));
}

void MainWindow::openShortcuts() {
    // Built fresh each time: it reads the active keymap, and that changes.
    ShortcutsDialog dialog(m_app, this);
    dialog.exec();
}

QString MainWindow::profileLabel(const QString &profile) const {
    // `single-key` names a file; `keyboard.profile.single_key` names a
    // catalogue entry. One underscore apart, and worth converting in one
    // place rather than at each of the three call sites.
    QString key = QStringLiteral("keyboard.profile.%1").arg(profile);
    key.replace(QLatin1Char('-'), QLatin1Char('_'));
    return tr_(key.toUtf8().constData());
}

void MainWindow::toggleKeymap() {
    const QString name =
        jtfText([&](char *buf, int len) { return jtf_toggle_keymap(m_app, buf, len); });
    announceKeymap(name);
}

void MainWindow::announceKeymap(const QString &name) {
    jtf_app_save_session(m_app);
    // Say which mode you are now in. Switching a keyboard layout silently is
    // the one change a user cannot see until a key does the wrong thing.
    const QString label = profileLabel(name);
    m_statusIsIdle = false;
    m_statusMessage->setText(jtfFill(tr_("status.keymap_switched"), "name", label));
    refreshAll();
}

void MainWindow::runCommand(const QString &id) {
    // Routed to the same QAction the menu uses, so a key and a menu entry can
    // never drift into doing two different things - and so a command that is
    // disabled stays disabled however it is reached.
    for (const auto &entry : std::as_const(m_commandActions)) {
        if (QLatin1String(entry.second) == id) {
            if (entry.first->isEnabled()) {
                entry.first->trigger();
            }
            return;
        }
    }
}

void MainWindow::toggleBookmark() {
    jtf_toggle_bookmark(m_app, jtf_active_pane(m_app));
    // Persisted immediately rather than at quit: a bookmark the user made and
    // then lost to a crash is worse than a write nobody notices.
    jtf_app_save_session(m_app);
    if (m_places) {
        m_places->refresh();
    }
}

void MainWindow::setInspectorVisible(bool visible) {
    m_inspector->setVisible(visible);
    if (visible) {
        const int width = jtf_inspector_width(m_app);
        const int panel = width > 0 ? width : 280;
        QList<int> sizes = m_outer->sizes();
        // Take the panel's width from the pane area, not from the tree: the
        // sidebar's width is something the user set, and one panel opening
        // should not resize another.
        if (sizes.size() >= 2) {
            const int last = sizes.size() - 1;
            sizes[last - 1] = qMax(200, sizes.at(last - 1) - panel);
            sizes[last] = panel;
            m_outer->setSizes(sizes);
        }
        syncInspector();
    }
    jtf_set_inspector_state(m_app, visible ? 1 : 0, m_outer->sizes().last());
    syncToolbar();
}

void MainWindow::syncInspector() {
    if (!m_inspector->isVisible()) {
        return;
    }
    PaneWidget *active = activePane();
    if (!active) {
        return;
    }
    const int pane = active->paneId();
    const int marked = jtf_marked_count(m_app, pane);
    const int row = active->currentRow();
    QString path;
    if (row >= 0) {
        path = jtfText([&](char *buf, int len) { return jtf_row_path(m_app, pane, row, buf, len); });
    }
    if (path.isEmpty()) {
        // Nothing focused: describe the folder itself, which is still the
        // answer to "what am I looking at".
        path = jtfText([&](char *buf, int len) { return jtf_current_path(m_app, pane, buf, len); });
    }
    m_inspector->setTarget(path, marked);
}

void MainWindow::syncTree() {
    if (!m_sidebar->isVisible()) {
        return;
    }
    m_places->refresh();
    const int pane = jtf_active_pane(m_app);
    m_tree->selectPath(
        jtfText([&](char *buf, int len) { return jtf_current_path(m_app, pane, buf, len); }));
}

void MainWindow::rebuildLayout() {
    const QString json =
        jtfText([&](char *buf, int len) { return jtf_layout_json(m_app, buf, len); });
    if (json == m_layoutSignature && m_root) {
        return; // structure unchanged: never rebuild widgets for nothing
    }
    m_layoutSignature = json;

    const QJsonObject root = QJsonDocument::fromJson(json.toUtf8()).object();
    m_panes.clear();
    auto *widget = buildNode(root);

    // The tree lives beside the whole workspace, not inside a pane: it
    // navigates the active pane, whichever that is.
    if (m_paneArea) {
        const int slot = m_outer->indexOf(m_paneArea);
        Q_ASSERT(slot >= 0);
        m_outer->replaceWidget(slot, widget);
        m_paneArea->deleteLater();
    } else {
        // Between the sidebar and the inspector, both of which already exist.
        m_outer->insertWidget(m_outer->indexOf(m_inspector), widget);
    }
    m_paneArea = widget;
    m_root = widget;
    applyTheme();
    markActivePane();
}

PaneWidget *MainWindow::activePane() const {
    return m_panes.value(jtf_active_pane(m_app), nullptr);
}

void MainWindow::markActivePane() {
    const int active = jtf_active_pane(m_app);
    for (auto it = m_panes.begin(); it != m_panes.end(); ++it) {
        it.value()->setActive(it.key() == active);
    }
}

void MainWindow::refreshAll() {
    rebuildLayout();
    applyFont();
    for (auto *pane : std::as_const(m_panes)) {
        pane->refresh();
    }
    markActivePane();
    syncToolbar();
    syncTree();
    syncInspector();
    retranslate();
}

void MainWindow::updateStatus() {
    // Each pane reports its own counts, so a multi-pane workspace tells you
    // about the pane you are looking at rather than about one of them.
    for (auto *pane : std::as_const(m_panes)) {
        pane->retranslate();
    }
    updateStatusSummary();
}

void MainWindow::updateStatusSummary() {
    const int panes = m_panes.size();
    int marked = 0;
    int items = 0;
    quint64 bytes = 0;
    for (auto *pane : std::as_const(m_panes)) {
        const int id = pane->paneId();
        marked += jtf_marked_count(m_app, id);
        items += jtf_row_count(m_app, id);
        bytes += jtf_target_size(m_app, id);
    }

    m_statusPanes->setText(panes == 1
                               ? tr_("status.pane_one")
                               : jtfFill(tr_("status.panes"), "count", QString::number(panes)));
    // Zero of something is not worth a slot on the bar; the label goes away
    // rather than sitting there saying nothing.
    if (marked > 0) {
        QString text = jtfFill(tr_("status.selected"), "count", QString::number(marked));
        if (bytes > 0) {
            text += QStringLiteral(" (") + PaneWidget::formatSize(bytes) + QLatin1Char(')');
        }
        m_statusSelection->setText(text);
    } else {
        m_statusSelection->clear();
    }
    m_statusItems->setText(jtfFill(tr_("status.items"), "count", QString::number(items)));
    const QString keymap =
        jtfText([&](char *buf, int len) { return jtf_keymap_name(m_app, buf, len); });
    m_statusKeymap->setText(profileLabel(keymap));
    m_statusKeymap->setToolTip(
        jtfText([&](char *buf, int len) { return jtf_tr(m_app, "command.keymap.toggle", buf, len); }));
    m_statusTasks->setText(jtf_op_running(m_app)
                               ? jtfFill(tr_("status.tasks_running"), "count", QStringLiteral("1"))
                               : QString());
    // Tracked with a flag rather than by testing for an empty string: after
    // a language change the label still holds the *previous* language's
    // "Ready", which is not empty and would never be replaced.
    if (m_statusIsIdle) {
        m_statusMessage->setText(tr_("status.ready"));
    }
}

void MainWindow::retranslate() {
    // Everything with words in it, including the parts the frame pump owns:
    // changing the language is exactly the case where nothing else changed.
    updateStatusSummary();
    for (const auto &entry : std::as_const(m_translatable)) {
        entry.first->setText(tr_(entry.second));
    }
    applyCommandBindings();
    for (const auto &entry : std::as_const(m_translatableMenus)) {
        entry.first->setTitle(tr_(entry.second));
    }
    // The window title names what you are looking at, then the application.
    // A title that only ever says the app name is a wasted line.
    const QString folder = jtfText([&](char *buf, int len) {
        return jtf_current_name(m_app, jtf_active_pane(m_app), buf, len);
    });
    setWindowTitle(folder.isEmpty() ? tr_("app.name")
                                    : folder + QStringLiteral(" — ") + tr_("app.name"));
    m_cancelButton->setText(tr_("operation.cancel"));
    for (auto *p : std::as_const(m_panes)) {
        p->retranslate();
    }
}

void MainWindow::applyTheme() {
    // Setting a palette makes Qt deliver PaletteChange to every widget. This
    // used to be handled in changeEvent, which called applyTheme again: an
    // unbounded recursion that overflowed the stack and crashed the app on
    // launch. The OS appearance is now observed through the signal that
    // actually means "the OS appearance changed", and this flag is the second
    // layer - AGENTS.md 20.2 asks for a bound on any recursion, including the
    // ones a framework introduces on your behalf.
    if (m_applyingTheme) {
        return;
    }
    m_applyingTheme = true;

    const bool systemDark = QApplication::styleHints()->colorScheme() == Qt::ColorScheme::Dark;
    m_theme = Theme::fromApp(m_app, systemDark);

    // Every colour on screen comes from a semantic token resolved in Rust.
    // There is no literal colour anywhere in the C++ (AGENTS.md 12).
    QPalette palette = QApplication::palette();
    palette.setColor(QPalette::Window, m_theme.window);
    palette.setColor(QPalette::Base, m_theme.pane);
    palette.setColor(QPalette::AlternateBase, m_theme.rowAlternate);
    palette.setColor(QPalette::WindowText, m_theme.textPrimary);
    palette.setColor(QPalette::Text, m_theme.textPrimary);
    palette.setColor(QPalette::ButtonText, m_theme.textPrimary);
    palette.setColor(QPalette::PlaceholderText, m_theme.textSecondary);
    palette.setColor(QPalette::Highlight, m_theme.selection);
    palette.setColor(QPalette::HighlightedText, m_theme.textOnAccent);
    // Tooltips are painted from the palette on some styles regardless of the
    // stylesheet, so both are set.
    palette.setColor(QPalette::ToolTipBase, m_theme.header);
    palette.setColor(QPalette::ToolTipText, m_theme.textPrimary);
    QApplication::setPalette(palette);
    setPalette(palette);

    qApp->setStyleSheet(m_theme.styleSheet());

    // Icons are theme output too, not fixed assets.
    for (auto it = m_toolbarShapes.constBegin(); it != m_toolbarShapes.constEnd(); ++it) {
        it.key()->setIcon(glyph::make(it.value(), m_theme.textPrimary));
    }

    if (m_inspector) {
        m_inspector->applyTheme(m_theme.textSecondary);
    }
    for (auto *pane : std::as_const(m_panes)) {
        pane->applyTheme(m_theme.mark,
                         m_theme.textPrimary,
                         m_theme.textSecondary,
                         m_theme.indicator,
                         m_theme.border);
    }
    m_applyingTheme = false;
}

void MainWindow::closeEvent(QCloseEvent *event) {
    if (m_tree->isVisible()) {
        jtf_set_tree_state(m_app, 1, m_outer->sizes().value(0));
    }
    jtf_app_save_session(m_app);
    QMainWindow::closeEvent(event);
}
