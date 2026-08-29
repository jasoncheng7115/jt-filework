#include "mainwindow.h"
#include "jtfstring.h"
#include "panewidget.h"
#include "icons.h"
#include "theme.h"

#include <QAction>
#include <QActionGroup>
#include <QApplication>
#include <QCloseEvent>
#include <QJsonDocument>
#include <QJsonObject>
#include <QLabel>
#include <QFontDatabase>
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

    buildMenus();
    buildToolbar();
    rebuildLayout();
    applyTheme();
    applyFont();
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

    m_fileMenu = menuBar()->addMenu(QString());
    m_translatableMenus.append({m_fileMenu, "menu.file"});
    command(m_fileMenu, "tab.new", [this] { jtf_new_tab(m_app); });
    command(m_fileMenu, "tab.close", [this] {
        const int pane = jtf_active_pane(m_app);
        jtf_close_tab(m_app, pane, jtf_active_tab(m_app, pane));
    });
    m_fileMenu->addSeparator();
    // Registered commands with no implementation yet. Shown and disabled
    // rather than hidden, so the keyboard layout and the menu agree about
    // what exists (docs/UI_UX_SPEC.md 13).
    for (const char *id : {"file.new_folder", "file.rename", "file.trash"}) {
        auto *action = new QAction(this);
        action->setEnabled(false);
        m_fileMenu->addAction(action);
        m_commandActions.append({action, id});
    }

    m_viewMenu = menuBar()->addMenu(QString());
    m_translatableMenus.append({m_viewMenu, "menu.view"});
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
    command(m_viewMenu, "file.mark.all",
            [this] { jtf_mark_listed(m_app, jtf_active_pane(m_app), 0); });
    command(m_viewMenu, "file.mark.none",
            [this] { jtf_mark_listed(m_app, jtf_active_pane(m_app), 1); });
    command(m_viewMenu, "file.mark.invert",
            [this] { jtf_mark_listed(m_app, jtf_active_pane(m_app), 2); });
    m_viewMenu->addSeparator();
    command(m_viewMenu, "view.hidden",
            [this] { jtf_set_show_hidden(m_app, jtf_show_hidden(m_app) ? 0 : 1); });
    command(m_viewMenu, "view.refresh", [this] { jtf_refresh(m_app, jtf_active_pane(m_app)); });

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

    m_goMenu = menuBar()->addMenu(QString());
    m_translatableMenus.append({m_goMenu, "menu.go"});
    command(m_goMenu, "nav.up", [this] { jtf_navigate_up(m_app, jtf_active_pane(m_app)); });
    command(m_goMenu, "nav.back", [this] { jtf_go_back(m_app, jtf_active_pane(m_app)); });
    command(m_goMenu, "nav.forward", [this] { jtf_go_forward(m_app, jtf_active_pane(m_app)); });
    command(m_goMenu, "nav.home", [this] {
        const QByteArray home = qgetenv("HOME");
        if (!home.isEmpty()) {
            jtf_navigate(m_app, jtf_active_pane(m_app), home.constData());
        }
    });
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
    }
}

void MainWindow::buildToolbar() {
    auto *bar = addToolBar(QString());
    bar->setObjectName(QStringLiteral("JtfToolbar"));
    bar->setMovable(false);
    bar->setFloatable(false);
    bar->setIconSize(QSize(16, 16));

    const auto navAction = [&](const char *id, std::function<void(int)> handler) {
        auto *action = new QAction(QString(), this);
        connect(action, &QAction::triggered, this, [this, handler] {
            handler(jtf_active_pane(m_app));
            refreshAll();
        });
        bar->addAction(action);
        m_commandActions.append({action, id});
        return action;
    };

    m_backAction = navAction("nav.back", [this](int pane) { jtf_go_back(m_app, pane); });
    m_forwardAction = navAction("nav.forward", [this](int pane) { jtf_go_forward(m_app, pane); });
    m_upAction = navAction("nav.up", [this](int pane) { jtf_navigate_up(m_app, pane); });
    m_refreshAction = navAction("view.refresh", [this](int pane) { jtf_refresh(m_app, pane); });

    bar->addSeparator();

    m_pathEdit = new QLineEdit(bar);
    m_pathEdit->setClearButtonEnabled(true);
    m_pathEdit->setMinimumWidth(320);
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
    setCentralWidget(widget);
    m_root = widget;
    applyTheme();
    markActivePane();
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
    setWindowTitle(tr_("app.name"));
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
    QApplication::setPalette(palette);
    setPalette(palette);

    qApp->setStyleSheet(m_theme.styleSheet());

    // Icons are theme output too, not fixed assets.
    if (m_backAction) {
        m_backAction->setIcon(glyph::make(glyph::Shape::ArrowLeft, m_theme.textPrimary));
        m_forwardAction->setIcon(glyph::make(glyph::Shape::ArrowRight, m_theme.textPrimary));
        m_upAction->setIcon(glyph::make(glyph::Shape::ArrowUp, m_theme.textPrimary));
        m_refreshAction->setIcon(glyph::make(glyph::Shape::Reload, m_theme.textPrimary));
    }

    for (auto *pane : std::as_const(m_panes)) {
        pane->applyTheme(m_theme.mark, m_theme.textPrimary, m_theme.indicator, m_theme.border);
    }
    m_applyingTheme = false;
}

void MainWindow::closeEvent(QCloseEvent *event) {
    jtf_app_save_session(m_app);
    QMainWindow::closeEvent(event);
}
