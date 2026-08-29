#include "mainwindow.h"
#include "jtfstring.h"
#include "panewidget.h"

#include <QAction>
#include <QActionGroup>
#include <QApplication>
#include <QCloseEvent>
#include <QJsonDocument>
#include <QJsonObject>
#include <QLabel>
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

    m_statusLeft = new QLabel(this);
    statusBar()->addWidget(m_statusLeft);

    buildMenus();
    rebuildLayout();
    applyTheme();
    retranslate();

    // The pump is the whole "never block the UI thread" contract in one
    // place: enumeration happens on worker threads, and the UI collects
    // whatever is ready, on a frame boundary, without ever waiting
    // (AGENTS.md 3).
    auto *timer = new QTimer(this);
    connect(timer, &QTimer::timeout, this, [this] {
        if (jtf_app_pump(m_app)) {
            refreshAll();
        }
    });
    timer->start(kPumpIntervalMs);
}

QString MainWindow::tr_(const char *key) const {
    return jtfText([&](char *buf, int len) { return jtf_tr(m_app, key, buf, len); });
}

void MainWindow::buildMenus() {
    const auto add = [this](QMenu *menu, const char *key, const QKeySequence &shortcut,
                            std::function<void()> handler) {
        auto *action = new QAction(this);
        if (!shortcut.isEmpty()) {
            action->setShortcut(shortcut);
        }
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
    add(m_fileMenu, "command.tab.new", QKeySequence::AddTab, [this] { jtf_new_tab(m_app); });
    add(m_fileMenu, "command.tab.close", QKeySequence::Close, [this] {
        const int pane = jtf_active_pane(m_app);
        jtf_close_tab(m_app, pane, jtf_active_tab(m_app, pane));
    });

    m_viewMenu = menuBar()->addMenu(QString());
    m_translatableMenus.append({m_viewMenu, "menu.view"});
    add(m_viewMenu, "command.workspace.split.horizontal", QKeySequence(QStringLiteral("Ctrl+D")),
        [this] { jtf_split_active(m_app, 0); });
    add(m_viewMenu, "command.workspace.split.vertical",
        QKeySequence(QStringLiteral("Ctrl+Shift+D")), [this] { jtf_split_active(m_app, 1); });
    add(m_viewMenu, "command.workspace.pane.close", QKeySequence(QStringLiteral("Ctrl+Shift+W")),
        [this] { jtf_close_active_pane(m_app); });
    add(m_viewMenu, "command.workspace.pane.next", QKeySequence(Qt::Key_F6),
        [this] { jtf_focus_next_pane(m_app); });
    m_viewMenu->addSeparator();
    add(m_viewMenu, "preset.quad", QKeySequence(QStringLiteral("Ctrl+4")),
        [this] { jtf_apply_preset(m_app, 3); });
    add(m_viewMenu, "preset.single", QKeySequence(QStringLiteral("Ctrl+1")),
        [this] { jtf_apply_preset(m_app, 0); });
    m_viewMenu->addSeparator();
    add(m_viewMenu, "command.view.hidden", QKeySequence(QStringLiteral("Ctrl+Shift+.")),
        [this] { jtf_set_show_hidden(m_app, jtf_show_hidden(m_app) ? 0 : 1); });

    auto *themeMenu = m_viewMenu->addMenu(QString());
    m_translatableMenus.append({themeMenu, "menu.theme"});
    add(themeMenu, "theme.system", {}, [this] { jtf_set_theme_mode(m_app, 0); });
    add(themeMenu, "theme.light", {}, [this] { jtf_set_theme_mode(m_app, 1); });
    add(themeMenu, "theme.dark", {}, [this] { jtf_set_theme_mode(m_app, 2); });

    auto *localeMenu = m_viewMenu->addMenu(QString());
    m_translatableMenus.append({localeMenu, "menu.language"});
    add(localeMenu, "language.english", {}, [this] { jtf_set_locale(m_app, "en"); });
    add(localeMenu, "language.zh_tw", {}, [this] { jtf_set_locale(m_app, "zh-TW"); });

    m_goMenu = menuBar()->addMenu(QString());
    m_translatableMenus.append({m_goMenu, "menu.go"});
    add(m_goMenu, "command.nav.up", QKeySequence(QStringLiteral("Ctrl+Up")),
        [this] { jtf_navigate_up(m_app, jtf_active_pane(m_app)); });
    add(m_goMenu, "command.nav.back", QKeySequence::Back,
        [this] { jtf_go_back(m_app, jtf_active_pane(m_app)); });
    add(m_goMenu, "command.nav.forward", QKeySequence::Forward,
        [this] { jtf_go_forward(m_app, jtf_active_pane(m_app)); });
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
    for (auto *pane : std::as_const(m_panes)) {
        pane->refresh();
    }
    markActivePane();
    retranslate();
}

void MainWindow::retranslate() {
    for (const auto &entry : std::as_const(m_translatable)) {
        entry.first->setText(tr_(entry.second));
    }
    for (const auto &entry : std::as_const(m_translatableMenus)) {
        entry.first->setTitle(tr_(entry.second));
    }
    setWindowTitle(tr_("app.name"));

    const int pane = jtf_active_pane(m_app);
    const int marked = jtf_marked_count(m_app, pane);
    QString status = jtfFill(tr_("status.items"), "count", QString::number(jtf_row_count(m_app, pane)));
    if (marked > 0) {
        status += QStringLiteral("   ") + jtfFill(tr_("status.marked"), "count", QString::number(marked));
    }
    m_statusLeft->setText(status);

    for (auto *p : std::as_const(m_panes)) {
        p->retranslate();
    }
}

void MainWindow::applyTheme() {
    const bool systemDark =
        QApplication::styleHints()->colorScheme() == Qt::ColorScheme::Dark;
    const auto colour = [&](int token) {
        return QColor::fromRgba(jtf_theme_color(m_app, systemDark ? 1 : 0, token));
    };

    // Every colour on screen comes from a semantic token resolved in Rust.
    // There is no literal colour anywhere in this file (AGENTS.md 12).
    QPalette palette = QApplication::palette();
    palette.setColor(QPalette::Window, colour(TokenSurfaceWindow));
    palette.setColor(QPalette::Base, colour(TokenSurfacePane));
    palette.setColor(QPalette::AlternateBase, colour(TokenSurfacePreview));
    palette.setColor(QPalette::WindowText, colour(TokenTextPrimary));
    palette.setColor(QPalette::Text, colour(TokenTextPrimary));
    palette.setColor(QPalette::ButtonText, colour(TokenTextPrimary));
    palette.setColor(QPalette::PlaceholderText, colour(TokenTextSecondary));
    palette.setColor(QPalette::Highlight, colour(TokenSelectionActive));
    palette.setColor(QPalette::HighlightedText, colour(TokenSurfacePane));
    setPalette(palette);
    QApplication::setPalette(palette);

    for (auto *pane : std::as_const(m_panes)) {
        pane->applyTheme(colour(TokenMarkActive), colour(TokenTextPrimary),
                         colour(TokenPaneActiveIndicator), colour(TokenBorder));
    }
}

void MainWindow::changeEvent(QEvent *event) {
    // Follow System means following it while running, not only at launch
    // (AGENTS.md 12).
    if (event->type() == QEvent::ApplicationPaletteChange ||
        event->type() == QEvent::PaletteChange || event->type() == QEvent::ThemeChange) {
        applyTheme();
    }
    QMainWindow::changeEvent(event);
}

void MainWindow::closeEvent(QCloseEvent *event) {
    jtf_app_save_session(m_app);
    QMainWindow::closeEvent(event);
}
