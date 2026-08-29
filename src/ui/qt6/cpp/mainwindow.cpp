#include "mainwindow.h"
#include "jtfstring.h"
#include "panewidget.h"
#include "icons.h"
#include "batchrenamedialog.h"
#include "foldertree.h"
#include "operations.h"
#include "platform/quicklook.h"
#include "settingsdialog.h"
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
#include <cstring>
#include <QFontDatabase>
#include <QMimeData>
#include <QProgressBar>
#include <QPushButton>
#include <QInputDialog>
#include <QLineEdit>
#include <QStyle>
#include <QToolBar>
#include <QMenu>
#include <QMenuBar>
#include <QSplitter>
#include <QStatusBar>
#include <QStyleHints>
#include <QTimer>
#include <QVBoxLayout>

namespace {
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
    m_tree = new FolderTree(m_app, m_outer);
    m_tree->setMinimumWidth(140);
    m_tree->setVisible(false);
    m_outer->addWidget(m_tree);
    setCentralWidget(m_outer);

    connect(m_tree, &FolderTree::folderActivated, this, [this](const QString &path) {
        const QByteArray utf8 = path.toUtf8();
        jtf_navigate(m_app, jtf_active_pane(m_app), utf8.constData());
        refreshAll();
    });
    // Remember the width as it is dragged, so it survives a restart.
    connect(m_outer, &QSplitter::splitterMoved, this, [this](int, int) {
        if (m_tree->isVisible()) {
            jtf_set_tree_state(m_app, 1, m_outer->sizes().value(0));
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
    statusBar()->addWidget(m_statusMessage, 1);
    statusBar()->addPermanentWidget(m_progress);
    statusBar()->addPermanentWidget(m_cancelButton);

    buildMenus();
    buildToolbar();
    rebuildLayout();
    applyTheme();
    applyFont();
    setTreeVisible(jtf_tree_visible(m_app) != 0);
    retranslate();

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
    m_editMenu->addSeparator();
    command(m_editMenu, "search.open", paneAction([](PaneWidget *pane) { pane->toggleSearch(); }));
    command(m_editMenu, "search.clear", paneAction([](PaneWidget *pane) { pane->clearSearch(); }));
    command(m_editMenu, "view.filter", paneAction([](PaneWidget *pane) { pane->toggleFilter(); }));

    // ---------------------------------------------------------------- View
    m_viewMenu = menuBar()->addMenu(QString());
    m_translatableMenus.append({m_viewMenu, "menu.view"});
    command(m_viewMenu, "view.tree", [this] { toggleTree(); });
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
    setting(keymapMenu, "keymap.platform", [this] { jtf_set_keymap(m_app, "platform"); });
    setting(keymapMenu, "keymap.cview", [this] { jtf_set_keymap(m_app, "cview"); });

    auto *localeMenu = m_viewMenu->addMenu(QString());
    m_translatableMenus.append({localeMenu, "menu.language"});
    setting(localeMenu, "language.english", [this] { jtf_set_locale(m_app, "en"); });
    setting(localeMenu, "language.zh_tw", [this] { jtf_set_locale(m_app, "zh-TW"); });

    m_viewMenu->addSeparator();
    command(m_viewMenu, "settings.open", [this] { openSettings(); });

    // ------------------------------------------------------------------ Go
    m_goMenu = menuBar()->addMenu(QString());
    m_translatableMenus.append({m_goMenu, "menu.go"});
    command(m_goMenu, "nav.back", [this] { jtf_go_back(m_app, jtf_active_pane(m_app)); });
    command(m_goMenu, "nav.forward", [this] { jtf_go_forward(m_app, jtf_active_pane(m_app)); });
    command(m_goMenu, "nav.up", [this] { jtf_navigate_up(m_app, jtf_active_pane(m_app)); });
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
    m_statusMessage->setText(tr_("status.copied"));
}

void MainWindow::clipboardPaste() {
    const QMimeData *data = QGuiApplication::clipboard()->mimeData();
    if (!data || !data->hasUrls()) {
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
        m_statusMessage->setText(current.isEmpty() ? label
                                                   : label + QStringLiteral("   ") + current);
        return;
    }

    const QString result = ops::takeResult(m_app);
    if (!result.isEmpty()) {
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
    // A separate window rather than a panel: AGENTS.md 14 makes the Viewer
    // stateful, and a stateful thing that disappears when the selection moves
    // is a preview wearing the wrong name.
    auto *viewer = new ViewerWindow(m_app, this);
    viewer->setAttribute(Qt::WA_DeleteOnClose);
    viewer->show();
    viewer->raise();
    viewer->activateWindow();
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
        action->setShortcut(shortcut.isEmpty() ? QKeySequence() : QKeySequence(shortcut));

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
    button("settings.open", glyph::Shape::Settings, [this] { openSettings(); });

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
    m_backAction->setEnabled(jtf_can_go_back(m_app, pane) != 0);
    m_forwardAction->setEnabled(jtf_can_go_forward(m_app, pane) != 0);
    m_upAction->setEnabled(jtf_can_go_up(m_app, pane) != 0);

    // A toggle button shows what it is toggling, or it is just a button that
    // sometimes does nothing visible (docs/UI_CONVENTIONS.md 1).
    if (m_treeAction) {
        QSignalBlocker blocker(m_treeAction);
        m_treeAction->setChecked(m_tree && m_tree->isVisible());
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
    setTreeVisible(!m_tree->isVisible());
}

void MainWindow::setTreeVisible(bool visible) {
    m_tree->setVisible(visible);
    if (visible) {
        // Restore the remembered width, or a sensible default the first time.
        const int width = jtf_tree_width(m_app);
        const int total = m_outer->width();
        const int sidebar = width > 0 ? width : 240;
        m_outer->setSizes({sidebar, qMax(200, total - sidebar)});
        syncTree();
    }
    jtf_set_tree_state(m_app, visible ? 1 : 0, m_outer->sizes().value(0));
}

void MainWindow::syncTree() {
    if (!m_tree->isVisible()) {
        return;
    }
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
        m_outer->replaceWidget(1, widget);
        m_paneArea->deleteLater();
    } else {
        m_outer->addWidget(widget);
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
    retranslate();
}

void MainWindow::updateStatus() {
    // Each pane reports its own counts, so a multi-pane workspace tells you
    // about the pane you are looking at rather than about one of them.
    for (auto *pane : std::as_const(m_panes)) {
        pane->retranslate();
    }
}

void MainWindow::retranslate() {
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

    for (auto *pane : std::as_const(m_panes)) {
        pane->applyTheme(m_theme.mark, m_theme.textPrimary, m_theme.indicator, m_theme.border);
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
