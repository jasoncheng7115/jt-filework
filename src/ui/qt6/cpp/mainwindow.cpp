#include "mainwindow.h"
#include "destinationdialog.h"
#include "dialogbuttons.h"
#include "iconprovider.h"
#include "jobsdialog.h"
#include "remotedialog.h"
#include "jtfstring.h"
#include "panewidget.h"
#include "icons.h"
#include "batchrenamedialog.h"
#include "aboutdialog.h"
#include "archivewindow.h"
#include "comparewindow.h"
#include "usagewindow.h"
#include "commandpalette.h"
#include "foldertree.h"
#include "inspector.h"
#include "keyhintbar.h"
#include "modeswitch.h"
#include "placeslist.h"
#include "platform/filetype.h"
#include "platform/share.h"
#include "operations.h"
#include "platform/quicklook.h"
#include "settingsdialog.h"
#include "shortcutsdialog.h"
#include "viewerwindow.h"
#include "theme.h"

#include <QAction>
#include <QActionGroup>
#include <QApplication>
#include <QCheckBox>
#include <QDialogButtonBox>
#include <QFileInfo>
#include <QFormLayout>
#include <QFrame>
#include <QDialog>
#include <QElapsedTimer>
#include <QDir>
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
#include <QFileDialog>
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

/// The status bar's message, which gives way rather than pushing.
///
/// A plain QLabel asks for the width of its whole text, so a long message -
/// a full path, say - would shove the counts on the right off the end of the
/// bar. This one shrinks instead, elides in the middle where a path can spare
/// it, and keeps the whole string on hover.
///
/// `setText` is hidden rather than overridden, which is enough: every caller
/// holds this type, not a QLabel.
class StatusLabel : public QLabel {
public:
    using QLabel::QLabel;

    void setText(const QString &text) {
        m_full = text;
        elide();
    }

    void clear() {
        m_full.clear();
        QLabel::clear();
        setToolTip(QString());
    }

protected:
    void resizeEvent(QResizeEvent *event) override {
        QLabel::resizeEvent(event);
        elide();
    }

private:
    void elide() {
        // Middle, because the ends of a path are the halves worth keeping.
        QLabel::setText(fontMetrics().elidedText(m_full, Qt::ElideMiddle, qMax(0, width() - 4)));
        setToolTip(QLabel::text() == m_full ? QString() : m_full);
    }

    QString m_full;
};

namespace {

// The range the zoom slider offers. Below the minimum the list stops being
// readable; above the maximum a row is taller than the icons in it.
constexpr int kMinFontPoints = 9;
constexpr int kMaxFontPoints = 22;
constexpr int kPumpIntervalMs = 16; // one frame at 60Hz
}

QList<MainWindow *> &MainWindow::windows() {
    static QList<MainWindow *> open;
    return open;
}

void MainWindow::syncWindows(JtfApp *app) {
    // The model is the authority on how many windows there are. A tab torn
    // off adds one; a tab merged back removes one. Rather than have the
    // gesture create and destroy widgets itself, both just change the model
    // and this brings the screen into line - so every route to the same state
    // produces the same windows.
    QSet<quint64> wanted;
    const int count = jtf_window_count(app);
    for (int i = 0; i < count; ++i) {
        wanted.insert(jtf_window_id_at(app, i));
    }

    for (int i = MainWindow::windows().size() - 1; i >= 0; --i) {
        MainWindow *window = MainWindow::windows().at(i);
        if (!wanted.contains(window->windowId())) {
            MainWindow::windows().removeAt(i);
            window->close();
            window->deleteLater();
        }
    }

    QSet<quint64> shown;
    for (MainWindow *window : std::as_const(MainWindow::windows())) {
        shown.insert(window->windowId());
    }
    for (const quint64 id : wanted) {
        if (shown.contains(id)) {
            continue;
        }
        auto *window = new MainWindow(app, id);
        window->setAttribute(Qt::WA_DeleteOnClose, false);
        // Offset from the window it came from, cascading if there are
        // several. Restored at the same position they were hidden behind each
        // other exactly, which looks like the window never opened.
        if (const MainWindow *first = MainWindow::windows().value(0)) {
            const int step = 28 * MainWindow::windows().size();
            window->move(first->pos() + QPoint(step, step));
        }
        window->show();
    }
    for (MainWindow *window : std::as_const(MainWindow::windows())) {
        window->refreshAll();
    }
}

MainWindow::MainWindow(JtfApp *app, quint64 windowId, QWidget *parent)
    : QMainWindow(parent), m_app(app), m_windowId(windowId) {
    windows().append(this);
    setMinimumSize(720, 420);
    resize(1180, 760);

    // Built before the layout, because rebuildLayout puts the pane area into
    // it.
    m_outer = new QSplitter(Qt::Horizontal, this);
    m_outer->setObjectName(QStringLiteral("JtfOuter"));
    m_outer->setChildrenCollapsible(false);
    // The stylesheet sets the real width; this only has to be large enough
    // not to clamp it. Qt makes the drag area exactly the handle's width, so a
    // thin handle is a divider nobody can grab.
    m_outer->setHandleWidth(7);
    // Places above the tree, in one vertical splitter: the list you use is
    // short and the tree you explore with wants the rest of the height, and
    // where the line between them falls is the user's call.
    m_sidebar = new QSplitter(Qt::Vertical, m_outer);
    m_sidebar->setObjectName(QStringLiteral("JtfSidebar"));
    m_sidebar->setChildrenCollapsible(false);
    m_sidebar->setHandleWidth(11);
    m_sidebar->setMinimumWidth(140);
    // Always on. The command that used to hide it hid the special places with
    // it, and those are the sidebar's fixed part: bookmarks, servers, disks
    // and where you have just been do not belong to whatever folder is open,
    // so there is nothing about the current folder that makes them not worth
    // showing. Only the folder tree under them is foldable now.
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
    connect(m_places, &PlacesList::serverActivated, this, [this](int index) {
        jtf_open_server(m_app, jtf_active_pane(m_app), index);
        refreshAll();
    });
    connect(m_tree, &FolderTree::commandRequested, this, &MainWindow::runCommand);
    connect(m_tree, &FolderTree::openInNewTabRequested, this, [this](const QString &path) {
        const QByteArray utf8 = path.toUtf8();
        jtf_new_tab(m_app);
        jtf_navigate(m_app, jtf_active_pane(m_app), utf8.constData());
        refreshAll();
    });
    connect(m_tree, &FolderTree::diskUsageRequested, this,
            [this](const QString &path) { openUsageWindow(path); });
    connect(m_tree, &FolderTree::newFolderRequested, this, [this](const QString &path) {
        // Made where the menu was opened, not where the pane happens to be:
        // the pane goes there first, so the new folder lands in the folder the
        // user pointed at and is then visible in the list they are looking at.
        const QByteArray utf8 = path.toUtf8();
        jtf_navigate(m_app, jtf_active_pane(m_app), utf8.constData());
        refreshAll();
        runOperation(OpNewFolder);
    });
    connect(m_tree, &FolderTree::openInNewWindowRequested, this, [this](const QString &path) {
        const QByteArray utf8 = path.toUtf8();
        jtf_open_in_new_window(m_app, jtf_active_pane(m_app), utf8.constData());
        jtf_app_save_session(m_app);
        refreshAll();
    });
    connect(m_tree, &FolderTree::bookmarksChanged, this, [this] {
        jtf_app_save_session(m_app);
        m_places->refresh();
    });
    connect(m_places, &PlacesList::ejectFailed, this, [this](const QString &mountPoint) {
        m_statusIsIdle = false;
        m_statusMessage->setText(
            jtfFill(tr_("places.eject_failed"), "name", QFileInfo(mountPoint).fileName()));
    });
    connect(m_places, &PlacesList::openInNewWindowRequested, this, [this](const QString &path) {
        const QByteArray utf8 = path.toUtf8();
        jtf_open_in_new_window(m_app, jtf_active_pane(m_app), utf8.constData());
        jtf_app_save_session(m_app);
        refreshAll();
    });
    connect(m_places, &PlacesList::addBookmarkRequested, this, [this] {
        toggleBookmark();
        m_places->refresh();
    });
    // Added last so the outer splitter reads left to right as sidebar,
    // panes, inspector. The pane area is inserted between them in
    // rebuildLayout, which finds its slot by pointer rather than by index -
    // an index here was correct until the inspector was added, and then
    // silently replaced the wrong widget.
    // The panes live in a column of their own. The inspector goes either
    // beside that column (in the outer splitter) or below it (in this one),
    // and rebuildLayout only ever touches the column - so moving the panel
    // cannot disturb where the panes get rebuilt, which is the mistake that
    // once left the pane area blank.
    m_paneColumn = new QSplitter(Qt::Vertical, m_outer);
    m_paneColumn->setObjectName(QStringLiteral("JtfPaneColumn"));
    m_paneColumn->setChildrenCollapsible(false);
    m_paneColumn->setHandleWidth(7);
    m_outer->addWidget(m_paneColumn);

    m_inspector = new Inspector(m_app, m_outer);
    m_outer->addWidget(m_inspector);
    m_inspector->setVisible(false);
    connect(m_inspector, &Inspector::closeRequested, this, [this] { setInspectorVisible(false); });
    // The hint strip lives between the panes and the status bar, which is
    // where CView puts it and where the eye already travels after reading the
    // list.
    auto *centre = new QWidget(this);
    auto *centreLayout = new QVBoxLayout(centre);
    centreLayout->setContentsMargins(0, 0, 0, 0);
    centreLayout->setSpacing(0);
    centreLayout->addWidget(m_outer, 1);
    m_keyHints = new KeyHintBar(m_app, centre);
    m_keyHints->setVisible(false);
    centreLayout->addWidget(m_keyHints);
    setCentralWidget(centre);

    connect(m_tree, &FolderTree::folderActivated, this, [this](const QString &path) {
        const QByteArray utf8 = path.toUtf8();
        jtf_navigate(m_app, jtf_active_pane(m_app), utf8.constData());
        refreshAll();
    });
    // Remember the width as it is dragged, so it survives a restart.
    connect(m_outer, &QSplitter::splitterMoved, this, [this](int, int) {
        // Only a plausible width: with the sidebar taking half the window
        // from a layout nobody asked for, this was recording that as the
        // remembered size the moment any divider moved.
        const int sidebar = m_outer->sizes().value(0);
        if (sidebar >= 170 && sidebar <= qMax(170, m_outer->width() / 3)) {
            jtf_set_tree_state(m_app, m_tree->isVisible() ? 1 : 0, sidebar);
        }
        if (m_inspector->isVisible()) {
            jtf_set_inspector_state(m_app, 1, m_outer->sizes().last());
        }
    });

    m_statusMessage = new StatusLabel(this);
    // Ignored, not Preferred: the label must never be the reason the bar wants
    // to be wider than the window.
    m_statusMessage->setSizePolicy(QSizePolicy::Ignored, QSizePolicy::Preferred);
    m_progress = new QProgressBar(this);
    m_progress->setMaximumWidth(180);
    m_progress->setTextVisible(false);
    m_progress->setVisible(false);
    m_cancelButton = new QPushButton(this);
    m_cancelButton->setVisible(false);
    m_cancelButton->setFlat(true);
    connect(m_cancelButton, &QPushButton::clicked, this, [this] {
        // Whichever is running. The button appears for both a file operation
        // and a folder measurement, and cancelling only one of them would
        // make it inert exactly when the walk is long enough to want stopping.
        jtf_op_cancel(m_app);
        jtf_cancel_measure(m_app);
        jtf_cancel_archive(m_app);
    });
    // The right-hand side of the status bar answers "what is the workspace
    // as a whole doing" - counts summed over every pane, not just the active
    // one, because the panes are the reason you opened four of them.
    m_statusPanes = new QLabel(this);
    m_statusSelection = new QLabel(this);
    m_statusItems = new QLabel(this);
    m_statusTasks = new QLabel(this);
    // A button, not a label. It opens the list of shortcuts, and a label that
    // opens a window is a control disguised as a readout - the pointing-hand
    // cursor was the only hint, and a cursor is not a hint anyone reads.
    m_statusKeymap = new QToolButton(this);
    m_statusKeymap->setObjectName(QStringLiteral("JtfStatusKeymap"));
    m_statusKeymap->setCursor(Qt::PointingHandCursor);
    m_statusKeymap->setAutoRaise(true);
    m_statusKeymap->setFocusPolicy(Qt::NoFocus);
    m_statusKeymap->setToolButtonStyle(Qt::ToolButtonTextBesideIcon);
    m_statusKeymap->setIconSize(QSize(16, 16));
    // A tool button takes the style's own smaller font, which left this
    // reading a size below the counts sitting next to it - smaller type on the
    // one thing in the row that is clickable. Set back to the interface font,
    // and a shade heavier because it is a control.
    QFont chipFont = QApplication::font();
    chipFont.setWeight(QFont::DemiBold);
    m_statusKeymap->setFont(chipFont);
    m_statusKeymap->setToolTip(tr_("command.help.shortcuts"));
    connect(m_statusKeymap, &QToolButton::clicked, this, [this] { openShortcuts(); });
    // On the application, not on any one widget: a shortcut is delivered
    // before the focus widget sees the key, and the rule about typing has to
    // hold for every field in every window - including ones opened later.
    qApp->installEventFilter(this);
    for (QLabel *label : {m_statusPanes, m_statusSelection, m_statusItems, m_statusTasks}) {
        label->setProperty("jtfStatusSummary", true);
    }
    // The hint strip's own switch sits beside the strip, at the bottom of the
    // window, rather than up on the toolbar: a control for a thing you are
    // looking at belongs next to the thing.
    m_keyHintsButton = new QToolButton(this);
    m_keyHintsButton->setObjectName(QStringLiteral("JtfStatusToggle"));
    m_keyHintsButton->setCheckable(true);
    m_keyHintsButton->setAutoRaise(true);
    m_keyHintsButton->setFocusPolicy(Qt::NoFocus);
    m_keyHintsButton->setIconSize(QSize(15, 15));
    connect(m_keyHintsButton, &QToolButton::clicked, this,
            [this] { setKeyHintsVisible(!m_keyHints->isVisible()); });
    // Right-click the strip's own switch to say how much it should say. The
    // three modes are a property of the strip, so they live on the control
    // that turns it on rather than three levels into a settings screen.
    m_keyHintsButton->setContextMenuPolicy(Qt::CustomContextMenu);
    connect(m_keyHintsButton, &QWidget::customContextMenuRequested, this,
            [this](const QPoint &at) { showKeyHintMenu(m_keyHintsButton->mapToGlobal(at)); });

    statusBar()->addWidget(m_statusMessage, 1);
    statusBar()->addPermanentWidget(m_keyHintsButton);
    statusBar()->addPermanentWidget(m_statusPanes);
    statusBar()->addPermanentWidget(m_statusSelection);
    statusBar()->addPermanentWidget(m_statusItems);
    statusBar()->addPermanentWidget(m_statusTasks);
    // The tasks counter is the obvious place to click when you want to know
    // what those tasks are.
    m_statusTasks->setCursor(Qt::PointingHandCursor);
    m_statusTasks->setToolTip(tr_("command.jobs.show"));
    m_statusTasks->installEventFilter(this);
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
    setInspectorPosition(jtf_inspector_position(m_app));
    setKeyHintDensity(jtf_key_hints_density(m_app));
    setKeyHintsVisible(jtf_key_hints_visible(m_app) != 0);
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
#if QT_VERSION >= QT_VERSION_CHECK(6, 5, 0)
    connect(QApplication::styleHints(), &QStyleHints::colorSchemeChanged, this,
            [this](Qt::ColorScheme) { applyTheme(); });
#endif

    // The session is written periodically, not only when the window closes.
    // Closing is the one exit that was covered: a crash, a force quit, or a
    // machine losing power all lost the layout the user had arranged, and
    // "remember where I was" that forgets under exactly those circumstances
    // is not much of a promise. Saving is writing a small file to a temporary
    // and renaming it, so doing it on a timer costs nothing worth measuring.
    if (m_windowId == 1) {
        auto *persist = new QTimer(this);
        connect(persist, &QTimer::timeout, this, [this] { jtf_app_save_session(m_app); });
        persist->start(30'000);

        // If the last session could not be used, say so - once, in the status
        // line rather than in a dialog. Coming back to an empty window with no
        // explanation is the moment people conclude the program forgot on
        // purpose (`docs/UPGRADE.md` §2). Deferred so the first paint has
        // happened and the message is not immediately overwritten by the
        // refresh that follows construction.
        QTimer::singleShot(0, this, [this] {
            const QString key =
                jtfText([&](char *buf, int len) { return jtf_session_notice(m_app, buf, len); });
            if (!key.isEmpty()) {
                m_statusIsIdle = false;
                m_statusMessage->setText(tr_(key.toUtf8().constData()));
            }
        });
    }

    auto *timer = new QTimer(this);
    connect(timer, &QTimer::timeout, this, [this] {
        // Under the watchdog, the model's share of a tick is timed apart from
        // the repaint that follows it. A slow tick is one or the other, and
        // "Timer took 200ms" does not say which.
        QElapsedTimer tick;
        const bool timing = !qEnvironmentVariableIsEmpty("JTF_WATCHDOG");
        if (timing) {
            tick.start();
        }
        const bool pumped = jtf_app_pump(m_app);
        if (timing) {
            const qint64 modelMicros = tick.nsecsElapsed() / 1000;
            if (modelMicros > 16'000) {
                qWarning("[jtf] model pump %lldus", static_cast<long long>(modelMicros));
            }
            tick.restart();
        }
        if (pumped) {
            // Every window, not just this one. There is one application state
            // behind the boundary and every window ticks its own timer against
            // it, so a batch is drained by whichever timer fires first and the
            // others are told "nothing happened" - which was true for the
            // pump and false for the rows. The window that kept losing that
            // race simply never refreshed, and sat showing `..` while the
            // status line, read straight from the core, counted thousands of
            // entries it was not displaying.
            //
            // While a directory streams in, only the rows and the counters
            // change. Rebuilding splitters and re-resolving every menu label
            // on each of four hundred batches is work nobody asked for.
            for (MainWindow *window : std::as_const(windows())) {
                for (auto *pane : std::as_const(window->m_panes)) {
                    pane->refreshRows();
                }
                window->updateStatus();
                window->updateOperationUi();
                window->checkServerCredentials();
            }
        }
        if (timing && pumped) {
            const qint64 viewMicros = tick.nsecsElapsed() / 1000;
            if (viewMicros > 16'000) {
                qWarning("[jtf] view refresh %lldus", static_cast<long long>(viewMicros));
            }
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
        // The menus are created down this function in order, so an entry
        // written above the menu it belongs to gets a null pointer. That was
        // a segfault at startup with nothing to read; now it is a warning and
        // a command that still works from the keyboard and the palette.
        Q_ASSERT_X(menu != nullptr, "buildMenus", id);
        auto *action = new QAction(this);
        connect(action, &QAction::triggered, this,
                [this, handler] { runAndSettleFocus(handler); });
        if (menu != nullptr) {
            menu->addAction(action);
        } else {
            qWarning("command %s was registered before its menu existed", id);
        }
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
        connect(action, &QAction::triggered, this,
                [this, handler] { runAndSettleFocus(handler); });
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
    // Enter on an archive shows what is inside it rather than handing the
    // file to the platform, which is what CV.HLP §四 describes and what the
    // project owner asked for. Anything else opens the ordinary way.
    command(m_fileMenu, "file.open", [this] {
        if (openArchiveWindow()) { return; }
        if (PaneWidget *pane = activePane()) { pane->openCurrentRow(); }
    });
    command(m_fileMenu, "file.view", [this] { openViewer(); });
    // CV.HLP §二: `H` 以 HEX 16 進制方式觀看檔案. The same viewer, opened
    // straight into its hex mode rather than into text.
    command(m_fileMenu, "file.view_hex", [this] {
        openViewer();
        if (m_viewer != nullptr) {
            jtf_viewer_toggle_hex(m_app);
            m_viewer->refresh();
        }
    });
    command(m_fileMenu, "file.edit", [this] { editSelection(); });
    command(m_fileMenu, "preview.quicklook", [this] { quickLookSelection(); });
    m_fileMenu->addSeparator();
    command(m_fileMenu, "file.new_folder", [this] { runOperation(OpNewFolder); });
    command(m_fileMenu, "file.new_file", [this] { runOperation(OpNewFile); });
    command(m_fileMenu, "file.attributes", [this] { showAttributes(); });
    command(m_fileMenu, "file.rename", [this] { runOperation(OpRename); });
    command(m_fileMenu, "file.batch_rename", [this] { openBatchRename(); });
    command(m_fileMenu, "file.duplicate", [this] { runOperation(OpDuplicate); });
    m_fileMenu->addSeparator();
    command(m_fileMenu, "file.copy_to_target_pane", [this] { runOperation(OpCopy); });
    command(m_fileMenu, "file.move_to_target_pane", [this] { runOperation(OpMove); });
    command(m_fileMenu, "file.copy_to", [this] { runOperationTo(ops::Copy); });
    command(m_fileMenu, "file.move_to", [this] { runOperationTo(ops::Move); });
    m_fileMenu->addSeparator();
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
    // Space adds the row to the selection and steps down, which is CView's
    // Space now that selecting and marking are the same thing: the tick
    // follows the highlight, so toggling one without the other would put them
    // back out of step.
    command(m_editMenu, "file.mark.toggle",
            paneAction([](PaneWidget *pane) { pane->toggleCurrentInSelection(); }));
    command(m_editMenu, "file.mark.all",
            [this] { jtf_mark_listed(m_app, jtf_active_pane(m_app), 0); });
    command(m_editMenu, "file.mark.none",
            [this] { jtf_mark_listed(m_app, jtf_active_pane(m_app), 1); });
    command(m_editMenu, "file.mark.invert",
            [this] { jtf_mark_listed(m_app, jtf_active_pane(m_app), 2); });
    command(m_editMenu, "file.mark.pattern", [this] { markByPattern(true); });
    command(m_editMenu, "file.unmark.pattern", [this] { markByPattern(false); });
    m_editMenu->addSeparator();
    command(m_editMenu, "search.open", [this] { focusSearchField(); });
    command(m_editMenu, "search.clear", paneAction([](PaneWidget *pane) { pane->clearSearch(); }));
    command(m_editMenu, "view.filter", paneAction([](PaneWidget *pane) { pane->toggleFilter(); }));

    // ---------------------------------------------------------------- View
    m_viewMenu = menuBar()->addMenu(QString());
    m_translatableMenus.append({m_viewMenu, "menu.view"});
    command(m_viewMenu, "view.tree", [this] { toggleTree(); });
    command(m_viewMenu, "keymap.toggle", [this] { toggleKeymap(); });
    command(m_viewMenu, "view.key_hints",
            [this] { setKeyHintsVisible(!m_keyHints->isVisible()); });
    command(m_viewMenu, "view.sort", [this] { showSortMenu(); });
    command(m_viewMenu, "view.mode.list",
            [this] { jtf_set_view_mode(m_app, jtf_active_pane(m_app), 0); });
    command(m_viewMenu, "view.mode.grid",
            [this] { jtf_set_view_mode(m_app, jtf_active_pane(m_app), 1); });
    command(m_viewMenu, "view.thumbnails", [this] {
        jtf_set_thumbnails(m_app, jtf_thumbnails(m_app) ? 0 : 1);
        jtf_app_save_session(m_app);
    });
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
    command(m_viewMenu, "workspace.focus.next", [this] { focusNextArea(); });
    command(m_viewMenu, "workspace.pane.previous", [this] {
        // Cycling forward n-1 times is one step back, and needs no second
        // traversal order to keep in agreement with the first.
        const int panes = jtf_pane_count(m_app);
        for (int i = 1; i < panes; ++i) {
            jtf_focus_next_pane(m_app);
        }
    });
    m_viewMenu->addSeparator();
    command(m_viewMenu, "jobs.show", [this] { openJobs(); });
    command(m_viewMenu, "workspace.preset.single", [this] { jtf_apply_preset(m_app, 0); });
    command(m_viewMenu, "workspace.preset.quad", [this] { jtf_apply_preset(m_app, 3); });
    m_viewMenu->addSeparator();

    // Submenus carry a picture too. They are rows in the same list as the
    // commands around them, and a row without one reads as a row that is not
    // quite finished.
    const QColor menuIconColour = palette().color(QPalette::Text);
    auto *themeMenu = m_viewMenu->addMenu(QString());
    themeMenu->setIcon(glyph::make(glyph::Shape::Theme, menuIconColour));
    m_translatableMenus.append({themeMenu, "menu.theme"});
    // Each of these repaints as well as records. `refreshAll` rebuilds the
    // lists and the furniture but does not touch colour - deliberately, since
    // it runs on every navigation and rebuilding the stylesheet each time
    // would be waste - so a theme chosen here changed the setting and left the
    // window exactly as it was. The Settings dialog always called
    // `applyTheme`; this menu never did, which is why one worked and the
    // other appeared to do nothing.
    const auto themeMode = [this](int mode) {
        return [this, mode] {
            jtf_set_theme_mode(m_app, mode);
            applyTheme();
        };
    };
    setting(themeMenu, "theme.system", themeMode(0));
    setting(themeMenu, "theme.light", themeMode(1));
    setting(themeMenu, "theme.dark", themeMode(2));

    auto *fontMenu = m_viewMenu->addMenu(QString());
    fontMenu->setIcon(glyph::make(glyph::Shape::Font, menuIconColour));
    m_translatableMenus.append({fontMenu, "menu.font"});
    setting(fontMenu, "font.system_mono", [this] { jtf_set_font(m_app, "", 0, 1); });
    setting(fontMenu, "font.system_proportional", [this] { jtf_set_font(m_app, "", 0, 0); });
    fontMenu->addSeparator();
    command(fontMenu, "view.font.smaller", [this] { stepFontSize(-1); });
    command(fontMenu, "view.font.larger", [this] { stepFontSize(1); });
    fontMenu->addSeparator();
    setting(fontMenu, "font.choose", [this] { chooseFontFamily(); });

    auto *keymapMenu = m_viewMenu->addMenu(QString());
    keymapMenu->setIcon(glyph::make(glyph::Shape::Keyboard, menuIconColour));
    m_translatableMenus.append({keymapMenu, "menu.keymap"});
    setting(keymapMenu, "keyboard.profile.single_key",
            [this] { jtf_set_keymap(m_app, "single-key"); });
    setting(keymapMenu, "keyboard.profile.native",
            [this] { jtf_set_keymap(m_app, "native"); });

    auto *localeMenu = m_viewMenu->addMenu(QString());
    localeMenu->setIcon(glyph::make(glyph::Shape::Language, menuIconColour));
    m_translatableMenus.append({localeMenu, "menu.language"});
    setting(localeMenu, "language.english", [this] { jtf_set_locale(m_app, "en"); });
    setting(localeMenu, "language.zh_tw", [this] { jtf_set_locale(m_app, "zh-TW"); });

    m_viewMenu->addSeparator();
    command(m_viewMenu, "command.palette", [this] { openPalette(); });
    command(m_viewMenu, "settings.open", [this] { openSettings(); });

    // ------------------------------------------------------------------ Go
    // ---------------------------------------------------------------- Tabs
    // Their own menu rather than the head of the File menu: they are about the
    // window, not about a file, and File had grown to twenty-nine items -
    // which on a platform that draws its menus in the window came out as a
    // two-column wall.
    m_tabMenu = menuBar()->addMenu(QString());
    m_translatableMenus.append({m_tabMenu, "menu.tabs"});
    command(m_tabMenu, "tab.new", [this] { jtf_new_tab(m_app); });
    command(m_tabMenu, "tab.close", [this] {
        const int pane = jtf_active_pane(m_app);
        jtf_close_tab(m_app, pane, jtf_active_tab(m_app, pane));
    });
    command(m_tabMenu, "tab.reopen", [this] {
        // Reopening is a pane operation with no arguments; the model knows
        // which tab was closed last.
        jtf_activate_tab(m_app, jtf_active_pane(m_app), 0);
    });
    // `tab.duplicate` was registered, bound to a chord and listed in the
    // shortcuts window, with nothing behind it - the duplicate itself was
    // built and reachable only from the tab strip's context menu, so the key
    // did nothing at all.
    command(m_tabMenu, "tab.duplicate", [this] {
        const int pane = jtf_active_pane(m_app);
        jtf_duplicate_tab(m_app, pane, jtf_active_tab(m_app, pane));
        refreshAll();
    });
    // Pinning was modelled from the beginning and reachable from nowhere.
    command(m_tabMenu, "tab.pin", [this] {
        const int pane = jtf_active_pane(m_app);
        jtf_toggle_tab_pinned(m_app, pane, jtf_active_tab(m_app, pane));
        refreshAll();
    });

    // --------------------------------------------------------------- Tools
    // The heavier things, each of which opens a window or starts a walk. All
    // five were being added to the File menu from inside the Edit menu's
    // block, which is why they arrived at the end of File in no order at all.
    m_toolsMenu = menuBar()->addMenu(QString());
    m_translatableMenus.append({m_toolsMenu, "menu.tools"});
    command(m_toolsMenu, "file.folder_size", [this] { measureFolderSizes(); });
    command(m_toolsMenu, "file.extract", [this] { extractArchive(); });
    command(m_toolsMenu, "file.compress", [this] { compressSelection(); });
    command(m_toolsMenu, "file.compare_panes", [this] { openCompareWindow(); });
    command(m_toolsMenu, "file.disk_usage", [this] { openUsageWindow(QString()); });
    m_toolsMenu->addSeparator();
    command(m_toolsMenu, "file.reveal", [this] { revealSelection(); });
    command(m_toolsMenu, "file.terminal", [this] { openTerminalHere(); });

    m_goMenu = menuBar()->addMenu(QString());
    m_translatableMenus.append({m_goMenu, "menu.go"});
    command(m_goMenu, "nav.back", [this] { jtf_go_back(m_app, jtf_active_pane(m_app)); });
    command(m_goMenu, "nav.forward", [this] { jtf_go_forward(m_app, jtf_active_pane(m_app)); });
    command(m_goMenu, "nav.up", [this] { jtf_navigate_up(m_app, jtf_active_pane(m_app)); });
    m_goMenu->addSeparator();
    command(m_goMenu, "remote.connect", [this] { connectToServer(); });
    command(m_goMenu, "remote.disconnect", [this] {
        jtf_remote_disconnect(m_app);
        refreshAll();
    });
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
        if (PaneWidget *pane = activePane()) {
            pane->editPath();
        }
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

    // ---------------------------------------------------------------- Help
    // Its own menu, because that is where every platform's user looks for it.
    // `About` lived in the View menu with an `AboutRole`, which macOS honours
    // by moving it into the application menu - and which does nothing at all
    // anywhere else, so on Linux and Windows it sat among the view settings
    // and nobody found it.
    m_helpMenu = menuBar()->addMenu(QString());
    m_translatableMenus.append({m_helpMenu, "menu.help"});
    command(m_helpMenu, "help.shortcuts", [this] { openShortcuts(); });
    command(m_helpMenu, "help.about", [this] {
        AboutDialog dialog(m_app, this);
        dialog.exec();
    })->setMenuRole(QAction::AboutRole);
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
    const QString pattern = dialogs::askForText(
        this, [this](const char *key) { return tr_(key); },
        mark ? tr_("prompt.pattern_title") : tr_("prompt.unmark_title"),
        tr_("prompt.pattern_label"), QStringLiteral("*"), m_theme.textPrimary, &accepted);
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

// CV.HLP §二 gives Ctrl-ENTER as 執行 DOS 指令 - drop me at a shell, here.
// The modern reading is the platform's terminal opened on this folder, or on
// the folder the current file lives in.
void MainWindow::openTerminalHere() {
    PaneWidget *pane = activePane();
    if (pane == nullptr || !filetype::canOpenInTerminal()) { return; }
    const int paneId = pane->paneId();
    const int row = pane->currentRow();
    QString path =
        jtfText([&](char *b, int l) { return jtf_row_path(m_app, paneId, row, b, l); });
    if (path.isEmpty()) { return; }
    if (jtf_row_is_directory(m_app, paneId, row) == 0) { path = QFileInfo(path).absolutePath(); }
    filetype::openInTerminal(path);
}

// CV.HLP's `E`, and the DOS hint strip's 「E編輯」.
//
// CView called CEdit, its own editor. This program has none yet, so the file
// goes to whatever the platform opens plain text with. `E` was bound in the
// keymap and listed on the hint strip all along with nothing behind it, so
// pressing it did nothing at all.
void MainWindow::editSelection() {
    PaneWidget *pane = activePane();
    if (pane == nullptr || !filetype::canOpenInEditor()) { return; }
    const int paneId = pane->paneId();
    if (jtf_pane_is_remote(m_app, paneId) != 0) { return; }
    const int row = pane->currentRow();
    if (row < 0 || jtf_row_is_directory(m_app, paneId, row) != 0) { return; }
    const QString path =
        jtfText([&](char *b, int l) { return jtf_row_path(m_app, paneId, row, b, l); });
    if (!path.isEmpty()) {
        filetype::openInEditor(path);
    }
}

void MainWindow::runDrop(int pane, const QStringList &paths, bool fromUs) {
    // Asked, not inferred from the modifier Qt happened to resolve. Nothing on
    // screen distinguishes a move from a copy until it has happened, and the
    // two are not equally undoable.
    const int kind = ops::askDropKind(m_app, this, static_cast<int>(paths.size()), fromUs);
    if (kind < 0) {
        return;
    }
    const QByteArray joined = paths.join(QLatin1Char('\n')).toUtf8();
    if (!jtf_op_prepare_drop(m_app, pane, kind, joined.constData()) ||
        !ops::awaitPlan(m_app, this)) {
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
    case OpNewFile:
        started = ops::createFile(m_app, this, pane, &message);
        // CV.HLP gives `Alt-O` as 呼叫 ce.exe 建立或編輯檔案 - creating and
        // editing are one action there, and a new empty file you then have to
        // find and open is only half of it. The cursor lands on the new file
        // once the listing refreshes, which is what `editSelection` acts on.
        if (started) {
            m_editAfterCreate = true;
        }
        break;
    case OpDuplicate:
        // Always "keep both": a duplicate that overwrote the original would
        // be a contradiction in terms.
        if (jtf_op_prepare_duplicate(m_app, pane) && ops::awaitPlan(m_app, this)) {
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

    // A file that was just created opens for editing, which is what CView's
    // `Alt-O` does. Deferred by one turn of the event loop: the cursor lands
    // on the new file when the listing refreshes, and `editSelection` acts on
    // wherever the cursor is.
    if (m_editAfterCreate) {
        m_editAfterCreate = false;
        QTimer::singleShot(0, this, [this] { editSelection(); });
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
        m_viewer->applyTheme(m_theme.mark, m_theme.textPrimary);
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
        // The settings that move or repaint furniture, not just recolour it.
        // Without these the panel stayed where it was and the preview kept its
        // old background until the next launch, which reads as the setting
        // having done nothing.
        setInspectorPosition(jtf_inspector_position(m_app));
        setKeyHintDensity(jtf_key_hints_density(m_app));
        applyTheme();
        applyFont();
        if (m_inspector) {
            m_inspector->applyPreviewBackground();
        }
        refreshAll();
    });
    dialog.exec();
    refreshAll();
}

void MainWindow::showAttributes() {
    const int pane = jtf_active_pane(m_app);
    // How many the operation would act on: the marks if there are any, else
    // the row under the cursor. Asked of the model, which is the one that
    // decides.
    const QString names =
        jtfText([&](char *buf, int len) { return jtf_target_names(m_app, pane, buf, len); });
    const int count = names.isEmpty() ? 0 : static_cast<int>(names.split(QLatin1Char('\n')).size());
    if (count == 0) {
        m_statusIsIdle = false;
        m_statusMessage->setText(tr_("plan.nothing_to_do"));
        return;
    }

    QDialog dialog(this);
    dialog.setWindowTitle(tr_("attributes.title"));
    auto *layout = new QVBoxLayout(&dialog);
    layout->setContentsMargins(16, 16, 16, 12);
    layout->setSpacing(10);

    if (count > 1) {
        // Say what it will touch. A dialog that silently applies to a
        // selection is how someone locks thirty files meaning to lock one.
        auto *scope = new QLabel(
            jtfFill(tr_("attributes.applies_to"), "count", QString::number(count)), &dialog);
        scope->setProperty("jtfFactLabel", true);
        layout->addWidget(scope);
    } else if (PaneWidget *widget = activePane()) {
        // What the file *is*, before what can be changed about it. A
        // properties window that offers one checkbox and tells you nothing is
        // not a properties window. Every fact here is a column the list
        // already knows how to produce, so this cannot drift out of step with
        // what the list shows, and a column added to the model turns up here
        // without a second edit.
        const int row = widget->currentRow();
        if (row >= 0) {
            auto *heading = new QHBoxLayout;
            heading->setSpacing(10);
            auto *icon = new QLabel(&dialog);
            // The platform's own icon for this file, the same one the list
            // shows - a drawn glyph here would be a second, worse answer to a
            // question the list has already answered.
            const QString rowPath = jtfText(
                [&](char *buf, int len) { return jtf_row_path(m_app, pane, row, buf, len); });
            IconProvider icons;
            icon->setPixmap(
                icons.iconFor(rowPath, jtf_row_is_directory(m_app, pane, row) != 0)
                    .pixmap(32, 32));
            heading->addWidget(icon);
            auto *name = new QLabel(
                jtfText([&](char *buf, int len) {
                    return jtf_row_text(m_app, pane, row, 0, buf, len);
                }),
                &dialog);
            name->setProperty("jtfHeadingLabel", true);
            name->setTextInteractionFlags(Qt::TextSelectableByMouse);
            heading->addWidget(name, 1);
            layout->addLayout(heading);

            auto *facts = new QFormLayout;
            facts->setLabelAlignment(Qt::AlignRight | Qt::AlignVCenter);
            facts->setHorizontalSpacing(14);
            facts->setVerticalSpacing(8);
            // A long value - a full path, a type name - takes the width it
            // needs and wraps under its label rather than being cut.
            facts->setFieldGrowthPolicy(QFormLayout::AllNonFixedFieldsGrow);
            facts->setRowWrapPolicy(QFormLayout::WrapLongRows);
            for (int column = 1; column < jtf_column_count(); ++column) {
                const QString value = jtfText([&](char *buf, int len) {
                    return jtf_row_text(m_app, pane, row, column, buf, len);
                });
                if (value.isEmpty()) {
                    continue; // a column with nothing to say says nothing
                }
                const QByteArray columnKey =
                    jtfText([&](char *buf, int len) { return jtf_column_key(column, buf, len); })
                        .toUtf8();
                if (columnKey == QByteArrayLiteral("column.path")) {
                    continue; // the full location is listed once, at the end
                }
                const QString label = jtfText([&](char *buf, int len) {
                    return jtf_tr(m_app, columnKey.constData(), buf, len);
                });
                auto *field = new QLabel(value, &dialog);
                field->setTextInteractionFlags(Qt::TextSelectableByMouse);
                facts->addRow(label + QLatin1Char(':'), field);
            }
            // The full path last: it is the longest line, and it is what you
            // came here to copy.
            auto *path = new QLabel(rowPath, &dialog);
            path->setTextInteractionFlags(Qt::TextSelectableByMouse);
            path->setWordWrap(true);
            facts->addRow(tr_("attributes.path") + QLatin1Char(':'), path);
            layout->addLayout(facts);

            auto *rule = new QFrame(&dialog);
            rule->setFrameShape(QFrame::HLine);
            rule->setProperty("jtfRule", true);
            layout->addWidget(rule);
        }
    }

    auto *readOnly = new QCheckBox(tr_("attributes.read_only"), &dialog);
    const bool wasReadOnly = jtf_targets_read_only(m_app, pane) != 0;
    readOnly->setChecked(wasReadOnly);
    layout->addWidget(readOnly);

    auto *buttons =
        new QDialogButtonBox(QDialogButtonBox::Ok | QDialogButtonBox::Cancel, &dialog);
    connect(buttons, &QDialogButtonBox::accepted, &dialog, &QDialog::accept);
    connect(buttons, &QDialogButtonBox::rejected, &dialog, &QDialog::reject);
    dialogs::localizeButtons(
        buttons, [&](const char *key) { return tr_(key); }, m_theme.textPrimary);
    layout->addWidget(buttons);

    if (dialog.exec() != QDialog::Accepted || readOnly->isChecked() == wasReadOnly) {
        return; // nothing chosen, or nothing changed
    }
    if (jtf_op_prepare_read_only(m_app, pane, readOnly->isChecked() ? 1 : 0) &&
        ops::awaitPlan(m_app, this)) {
        jtf_op_start(m_app, 0);
    }
    refreshAll();
}

void MainWindow::showSortMenu() {
    // Built from the columns rather than from a written list, so a column
    // added to the model is sortable without a second edit. CView reaches
    // this with S; the header is still the mouse's way in.
    const int pane = jtf_active_pane(m_app);
    const int current = jtf_sort_column(m_app, pane);
    const bool ascending = jtf_sort_ascending(m_app, pane) != 0;

    QMenu menu(this);
    // Named sections rather than a bare rule between them. The menu holds two
    // different questions - which column, and which way - and a thin line
    // between two lists of similar-looking checkable entries did not say that
    // loudly enough to stop them reading as one list.
    QAction *byHeading = menu.addAction(tr_("sort.by_heading"));
    byHeading->setEnabled(false);
    auto *columns = new QActionGroup(&menu);
    for (int column = 0; column < jtf_column_count(); ++column) {
        const QString key =
            jtfText([&](char *buf, int len) { return jtf_column_key(column, buf, len); });
        const QByteArray utf8 = key.toUtf8();
        QAction *entry = menu.addAction(jtfText(
            [&](char *buf, int len) { return jtf_tr(m_app, utf8.constData(), buf, len); }));
        entry->setCheckable(true);
        entry->setChecked(column == current);
        columns->addAction(entry);
        connect(entry, &QAction::triggered, this, [this, pane, column] {
            jtf_sort_by(m_app, pane, column);
            refreshAll();
        });
    }

    menu.addSeparator();
    QAction *directionHeading = menu.addAction(tr_("sort.direction_heading"));
    directionHeading->setEnabled(false);
    // Direction is shown as state, not as a command: clicking the column you
    // are already sorted by is what reverses it, and this says which way it
    // currently runs.
    // The arrows carry the meaning here, which is why this pair gets pictures
    // and the column list above does not: those are a radio group whose only
    // indicator is the tick, and an icon beside a tick reads as two competing
    // marks rather than one.
    QAction *up = menu.addAction(glyph::make(glyph::Shape::ArrowUp, m_theme.textSecondary),
                                 tr_("sort.ascending"));
    QAction *down = menu.addAction(glyph::make(glyph::Shape::ArrowDown, m_theme.textSecondary),
                                   tr_("sort.descending"));
    auto *direction = new QActionGroup(&menu);
    for (QAction *entry : {up, down}) {
        entry->setCheckable(true);
        direction->addAction(entry);
    }
    up->setChecked(ascending);
    down->setChecked(!ascending);
    const auto reverse = [this, pane, ascending](bool wanted) {
        if (wanted != ascending) {
            jtf_sort_by(m_app, pane, jtf_sort_column(m_app, pane));
            refreshAll();
        }
    };
    connect(up, &QAction::triggered, this, [reverse] { reverse(true); });
    connect(down, &QAction::triggered, this, [reverse] { reverse(false); });

    // Under the pointer if it is over the window, else under the active
    // pane's header - a keyboard-invoked menu must not appear off screen.
    menu.exec(QCursor::pos());
}

void MainWindow::showCrumbMenu(int paneId, const QString &path, const QPoint &global) {
    jtf_focus_pane(m_app, paneId);
    markActivePane();

    QMenu menu(this);
    const auto entry = [&](const char *key, const QString &iconId, std::function<void()> run) {
        QAction *action = menu.addAction(tr_(key));
        if (!iconId.isEmpty() && glyph::hasCommandIcon(iconId)) {
            action->setIcon(glyph::forCommand(iconId, m_theme.textSecondary));
        }
        connect(action, &QAction::triggered, this, [this, run] {
            run();
            refreshAll();
        });
        return action;
    };

    const QByteArray utf8 = path.toUtf8();
    entry("crumb.open", QStringLiteral("file.open"), [this, paneId, utf8] {
        jtf_navigate(m_app, paneId, utf8.constData());
    });
    entry("crumb.open_tab", QStringLiteral("tab.new"), [this, utf8] {
        jtf_new_tab(m_app);
        jtf_navigate(m_app, jtf_active_pane(m_app), utf8.constData());
    });
    entry("crumb.open_window", QStringLiteral("tab.tear_off"), [this, paneId, utf8] {
        jtf_open_in_new_window(m_app, paneId, utf8.constData());
        jtf_app_save_session(m_app);
    });
    entry(jtf_path_is_bookmarked(m_app, utf8.constData()) != 0 ? "crumb.unbookmark"
                                                              : "crumb.bookmark",
          QStringLiteral("file.bookmark"), [this, utf8] {
              jtf_toggle_bookmark_path(m_app, utf8.constData());
              jtf_app_save_session(m_app);
              if (m_places) {
                  m_places->refresh();
              }
          });

    // The folders inside this one, so an ancestor's siblings are reachable
    // without walking there first - the reason Path Finder's breadcrumb has
    // a submenu at all.
    QMenu *contents = menu.addMenu(tr_("crumb.contents"));
    QDir dir(path);
    const QFileInfoList children =
        dir.entryInfoList(QDir::Dirs | QDir::NoDotAndDotDot, QDir::Name | QDir::LocaleAware);
    if (children.isEmpty()) {
        contents->addAction(tr_("crumb.no_subfolders"))->setEnabled(false);
    } else {
        // Bounded: a folder with ten thousand subdirectories must not build a
        // ten-thousand-item menu while the pointer waits.
        constexpr int kMaxSubmenuEntries = 60;
        int shown = 0;
        for (const QFileInfo &child : children) {
            if (shown++ >= kMaxSubmenuEntries) {
                contents->addSeparator();
                contents->addAction(tr_("crumb.more"))->setEnabled(false);
                break;
            }
            const QString childPath = child.absoluteFilePath();
            QAction *action = contents->addAction(child.fileName());
            connect(action, &QAction::triggered, this, [this, paneId, childPath] {
                const QByteArray target = childPath.toUtf8();
                jtf_navigate(m_app, paneId, target.constData());
                refreshAll();
            });
        }
    }

    menu.addSeparator();
    entry("crumb.copy_path", QStringLiteral("file.copy_path"), [this, path] {
        QGuiApplication::clipboard()->setText(path);
        m_statusIsIdle = false;
        m_statusMessage->setText(tr_("status.copied"));
    });
    if (platform::canReveal()) {
        entry("crumb.reveal", QStringLiteral("file.reveal"),
              [path] { platform::reveal(path); });
    }
    if (filetype::available()) {
        entry("crumb.terminal", QStringLiteral("nav.goto"),
              [path] { filetype::openInTerminal(path); });
    }

    menu.exec(global);
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
            // Qt does not paint a shortcut in a popup menu on every style, so
            // the text carries it: a context menu that hides the key teaches
            // people the command has none.
            action->setText(action->text() + QLatin1Char('\t') +
                            QKeySequence(shortcut).toString(QKeySequence::NativeText));
        }
        const QString commandId = QString::fromLatin1(id);
        if (glyph::hasCommandIcon(commandId)) {
            action->setIcon(glyph::forCommand(commandId, m_theme.textSecondary));
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
    // Anything that hands a path to the platform is about a file on this
    // machine. A server's `/srv/data` and this machine's `/srv/data` are
    // different files and the platform cannot tell them apart, so those
    // entries are absent on a remote row rather than present and wrong.
    const bool remote = jtf_pane_is_remote(m_app, paneId) != 0;
    const bool localTarget = hasTarget && !remote;
    const bool twoPanes = jtf_pane_count(m_app) > 1;

    if (hasTarget) {
        add("file.open", [pane] { pane->openCurrentRow(); });

        // Open With, from the platform's own list - the same one Finder
        // shows. Building our own from extensions would disagree with the
        // rest of the system, and disagreeing about which application owns a
        // file type is worse than not offering the menu.
        const QString targetPath = jtfText([&](char *buf, int len) {
            return jtf_row_path(m_app, paneId, pane->currentRow(), buf, len);
        });
        const QList<filetype::Application> apps =
            (targetPath.isEmpty() || remote) ? QList<filetype::Application>()
                                             : filetype::applicationsFor(targetPath);
        if (!apps.isEmpty()) {
            QMenu *openWith = menu.addMenu(tr_("file.open_with"));
            openWith->setIcon(glyph::make(glyph::Shape::NewWindow, m_theme.textPrimary));
            // Each application's own icon, which is how anyone actually picks
            // one out of a list of fifteen. The identifier is the bundle's
            // path, so the same provider that draws the file list can answer
            // for it - no second way of asking the platform what something
            // looks like.
            IconProvider icons;
            for (const filetype::Application &app : apps) {
                QAction *entry =
                    openWith->addAction(icons.iconFor(app.identifier, true), app.name);
                const QString id = app.identifier;
                connect(entry, &QAction::triggered, this,
                        [targetPath, id] { filetype::openWith(targetPath, id); });
            }
        }

        // A folder row can be bookmarked and opened in a window of its own.
        // Both are about the folder under the pointer, which may not be the
        // one the pane is showing - that is the whole point of offering them
        // here rather than only on the path bar.
        const bool onFolder =
            onEntry && !remote && !targetPath.isEmpty() && QFileInfo(targetPath).isDir();
        if (onFolder) {
            const QByteArray folderUtf8 = targetPath.toUtf8();
            const bool bookmarked = jtf_path_is_bookmarked(m_app, folderUtf8.constData()) != 0;
            QAction *window = menu.addAction(
                glyph::forCommand(QStringLiteral("tab.tear_off"), m_theme.textPrimary),
                tr_("crumb.open_window"));
            connect(window, &QAction::triggered, this, [this, paneId, folderUtf8] {
                jtf_open_in_new_window(m_app, paneId, folderUtf8.constData());
                jtf_app_save_session(m_app);
                refreshAll();
            });
            QAction *bookmark = menu.addAction(
                glyph::forCommand(QStringLiteral("file.bookmark"), m_theme.textPrimary),
                tr_(bookmarked ? "crumb.unbookmark" : "crumb.bookmark"));
            connect(bookmark, &QAction::triggered, this, [this, folderUtf8] {
                jtf_toggle_bookmark_path(m_app, folderUtf8.constData());
                jtf_app_save_session(m_app);
                if (m_places) {
                    m_places->refresh();
                }
            });
            menu.addSeparator();
        }

        add("file.view", [this] { openViewer(); });
        add("preview.quicklook", [this] { quickLookSelection(); }, localTarget);
        menu.addSeparator();
        add("file.clipboard.cut", [this] { clipboardPut(true); });
        add("file.clipboard.copy", [this] { clipboardPut(false); });
    }
    add("file.clipboard.paste", [this] { clipboardPaste(); });
    if (hasTarget) {
        // On the menu as well as the keyboard: the folder sizes are the one
        // column the list cannot fill in by itself, so the way to ask for
        // them has to be somewhere you would look for it, not only on a key
        // you would have to already know.
        add("file.folder_size", [this] { measureFolderSizes(); });
        add(
            "file.extract", [this] { extractArchive(); },
            localTarget && jtf_cursor_is_archive(m_app, paneId) != 0);
        add("file.compress", [this] { compressSelection(); }, localTarget);
        add("file.compare_panes", [this] { openCompareWindow(); }, twoPanes);
        // On a folder row it measures that folder; anywhere else, the one the
        // pane is showing. Both are what「這裡面什麼吃掉了空間」means where the
        // pointer is. The row is read when the item is chosen rather than
        // captured now, so it is the row the menu was opened on however long
        // the menu stayed up.
        add(
            "file.disk_usage",
            [this, paneId] {
                PaneWidget *on = m_panes.value(paneId, nullptr);
                QString row;
                if (on != nullptr && on->currentRow() >= 0) {
                    row = jtfText([&](char *buf, int len) {
                        return jtf_row_path(m_app, paneId, on->currentRow(), buf, len);
                    });
                }
                openUsageWindow(QFileInfo(row).isDir() ? row : QString());
            },
            !remote);
        add("file.attributes", [this] { showAttributes(); });
        menu.addSeparator();
        add("file.copy_path", [this] { copyText(true); });
        add("file.copy_name", [this] { copyText(false); });
        menu.addSeparator();
        add("file.copy_to_target_pane", [this] { runOperation(OpCopy); }, twoPanes);
        add("file.move_to_target_pane", [this] { runOperation(OpMove); }, twoPanes);
        add("file.duplicate", [this] { runOperation(OpDuplicate); });
        add("file.rename", [this] { runOperation(OpRename); });
        add("file.batch_rename", [this] { openBatchRename(); });
        menu.addSeparator();
        add("file.mark.toggle", [pane] { pane->toggleCurrentInSelection(); });
        add(
            "file.edit", [this] { editSelection(); },
            localTarget && filetype::canOpenInEditor());
        add("file.reveal", [this] { revealSelection(); }, localTarget && platform::canReveal());
        // The system's own list of things that can take these files. What the
        // platform offers, not a list of our own - see platform/share.h for
        // why the Services menu itself is not reachable from here.
        add("file.share",
            [this, global] {
                const QStringList paths = targetPaths();
                if (!paths.isEmpty()) {
                    share::showPicker(this, mapFromGlobal(global), paths);
                }
            },
            localTarget && share::available());
        // The folder this row is in, or the row itself when it is a folder.
        // Only offered where the platform can actually do it - on Linux and
        // Windows this is still a stub, and a menu entry that does nothing is
        // worse than no entry.
        add(
            "file.terminal", [this] { openTerminalHere(); },
            localTarget && filetype::canOpenInTerminal());
    }

    menu.addSeparator();
    add("file.new_folder", [this] { runOperation(OpNewFolder); });
    add("file.new_file", [this] { runOperation(OpNewFile); });
    add("view.refresh", [this, paneId] { jtf_refresh(m_app, paneId); });

    if (hasTarget) {
        menu.addSeparator();
        // SFTP has no trash. Stage two deletes remotely and permanently, and
        // saying so is the point of leaving this out rather than having it
        // fail.
        add("file.trash", [this] { runOperation(OpTrash); }, !remote);
        add("file.delete", [this] { runOperation(OpDelete); });
    }

    menu.exec(global);
}

void MainWindow::runOperationTo(int kindCode) {
    const auto kind = static_cast<ops::Kind>(kindCode);
    // Ask first. With one pane there is no "other pane" to mean, and even
    // with two the place you want is often a tab open somewhere else.
    // How many are about to move, so the title can say so. The marks win when
    // there are any - that is what the operation itself acts on - and the row
    // under the cursor is the one entry otherwise.
    const int pane = jtf_active_pane(m_app);
    const int marked = jtf_marked_count(m_app, pane);
    DestinationDialog dialog(m_app, kind == ops::Move, marked > 0 ? marked : 1, this);
    if (dialog.exec() != QDialog::Accepted) {
        return;
    }
    const QString destination = dialog.destination();
    if (destination.isEmpty()) {
        return;
    }
    QString message;
    if (!ops::confirmAndStartTo(m_app, this, pane, kind, destination, &message)) {
        if (!message.isEmpty()) {
            m_statusIsIdle = false;
            m_statusMessage->setText(message);
        }
    }
    refreshAll();
}

// Whether `widget` is something a person types into.
//
// Every text field in the program, however it was made: the path bar, the
// filter, the toolbar search, a rename dialog, a field in the compare or usage
// windows. Asked by class rather than kept as a list, because a list is a
// thing that goes out of date the next time a dialog is added.
static bool isTextEntry(const QWidget *widget) {
    return widget != nullptr
           && (widget->inherits("QLineEdit") || widget->inherits("QTextEdit")
               || widget->inherits("QPlainTextEdit") || widget->inherits("QAbstractSpinBox")
               || widget->inherits("QComboBox"));
}

bool MainWindow::eventFilter(QObject *watched, QEvent *event) {
    // Typing is typing. In Single-Key mode a bare letter is a command, and Qt
    // delivers a shortcut *before* the focus widget sees the key - so pressing
    // `P` to edit the path and then typing `c` ran「複製到」instead of writing
    // a `c`. Accepting the ShortcutOverride says "this key belongs to whoever
    // has the focus", and the key then arrives there as an ordinary press.
    //
    // Claimed for the whole application rather than per field, because the
    // rule is about text entry and not about any one of them (`AGENTS.md` §4:
    // one place decides).
    // A popup owns the keyboard while it is open. The path field's completer
    // shows its list in one, and showing it moves the focus off the field - so
    // checking the focus widget alone stopped guarding the moment the list
    // appeared, and the next letter typed ran a command again. Anything else
    // that opens a popup (a menu, a combo box's list) wants the same: a
    // single-key command firing out from under an open list is never what the
    // person typing meant.
    const bool typingSomewhere =
        isTextEntry(QApplication::focusWidget()) || QApplication::activePopupWidget() != nullptr;
    if (event->type() == QEvent::ShortcutOverride && typingSomewhere) {
        auto *key = static_cast<QKeyEvent *>(event);
        constexpr Qt::KeyboardModifiers kChord =
            Qt::ControlModifier | Qt::AltModifier | Qt::MetaModifier;
        // Anything that produces text, with no chord on it.
        const bool types = !key->text().isEmpty() && key->text().at(0).isPrint();
        // And the keys that mean something *inside* a field rather than to the
        // window behind it. This used to let them through on the grounds that
        // they produce no text - but a keymap binds them: `escape` is
        // `search.clear`, `enter` is `file.open`, `tab` is the next pane. On
        // macOS the menu bar is application-wide, so those fired first and
        // Escape in the filter box cleared nothing, Tab jumped panes instead
        // of handing the keyboard to the list, and Enter opened a file. The
        // arrows are still left alone: moving through a completer's list is
        // what they are for here, and no bare arrow is bound to anything a
        // field would want to keep.
        const bool edits = key->key() == Qt::Key_Escape || key->key() == Qt::Key_Return
                           || key->key() == Qt::Key_Enter || key->key() == Qt::Key_Tab
                           || key->key() == Qt::Key_Backtab;
        if ((types || edits) && (key->modifiers() & kChord) == Qt::NoModifier) {
            event->accept();
            return true;
        }
        // The editing chords a text field must keep, whatever a keymap has
        // done with those letters. Copying inside a field has to copy the
        // text, not the file the cursor happens to be on.
        for (const QKeySequence::StandardKey standard :
             {QKeySequence::Copy, QKeySequence::Cut, QKeySequence::Paste,
              QKeySequence::SelectAll, QKeySequence::Undo, QKeySequence::Redo,
              QKeySequence::Delete, QKeySequence::Backspace}) {
            if (key->matches(standard) == QKeySequence::ExactMatch) {
                event->accept();
                return true;
            }
        }
    }

    // The counter said how many jobs there were and took a pointing-hand
    // cursor, which promises a click does something. This is that something.
    if (watched == m_statusTasks && event->type() == QEvent::MouseButtonRelease) {
        openJobs();
        return true;
    }
    return QMainWindow::eventFilter(watched, event);
}

void MainWindow::connectToServer() {
    RemoteDialog dialog(m_app, this);
    if (dialog.exec() != QDialog::Accepted) {
        return;
    }
    const QByteArray host = dialog.host().toUtf8();
    const QByteArray user = dialog.user().toUtf8();
    const QByteArray path = dialog.path().toUtf8();
    if (host.isEmpty()) {
        return;
    }
    // Recorded before navigating, because the connection is opened by the
    // enumeration that navigating starts.
    if (dialog.trustUnknownHost()) {
        jtf_remote_accept_host(m_app, host.constData(), dialog.port(), user.constData());
    }
    // Handed over for this one connection. Converted to UTF-8 in a local that
    // goes out of scope here; nothing keeps it afterwards.
    const QString typed = dialog.password();
    if (!typed.isEmpty()) {
        const QByteArray secret = typed.toUtf8();
        jtf_remote_set_password(m_app, host.constData(), dialog.port(), user.constData(),
                                secret.constData());
    }
    jtf_navigate_remote(m_app, jtf_active_pane(m_app), host.constData(), dialog.port(),
                        user.constData(), path.constData());
    // Remembered so it need not be typed again. Host, port and account only -
    // the way in is never saved.
    jtf_add_server(m_app, host.constData(), dialog.port(), user.constData(), path.constData());
    jtf_app_save_session(m_app);
    if (m_places) {
        m_places->refresh();
    }
    refreshAll();
}

void MainWindow::openJobs() {
    // Modeless: the point of the window is to watch work that is still going
    // on, and a modal one would stop the user doing anything else while it
    // was open - including the thing they opened it to decide about.
    auto *jobs = new JobsDialog(m_app, this);
    jobs->setAttribute(Qt::WA_DeleteOnClose);
    jobs->show();
}

void MainWindow::focusSearchField() {
    // Search lives in the toolbar, where it is always visible and its
    // placeholder can say that it looks inside subfolders. Filter stays in the
    // pane, right above the list it narrows, because that is what it is about.
    // They were two different things sharing one word; now they are two
    // different things in two different places.
    if (m_searchEdit == nullptr) {
        return;
    }
    m_searchEdit->setFocus();
    m_searchEdit->selectAll();
}

void MainWindow::measureFolderSizes() {
    const int queued = jtf_measure_folder_sizes(m_app, jtf_active_pane(m_app));
    if (queued == 0) {
        m_statusIsIdle = false;
        m_statusMessage->setText(tr_("status.no_folders_selected"));
        return;
    }
    // The walk runs on a worker thread now, so this returns immediately and
    // the window keeps drawing. What is left here is saying so: a spinner
    // with no numbers beside it is indistinguishable from a hang.
    m_statusIsIdle = false;
    m_statusMessage->setText(tr_("status.measuring"));
    if (m_measurePoll == nullptr) {
        m_measurePoll = new QTimer(this);
        m_measurePoll->setInterval(120);
        connect(m_measurePoll, &QTimer::timeout, this, [this] {
            const bool changed = jtf_pump_measure(m_app) != 0;
            if (jtf_is_measuring(m_app) != 0) {
                if (changed) {
                    // Files and bytes so far. No percentage: the total is
                    // what the walk is being run to find out, and a bar
                    // claiming 40% would be inventing it.
                    m_statusMessage->setText(
                        jtfFill(jtfFill(tr_("status.measuring_progress"), "files",
                                        QString::number(jtf_measure_files(m_app))),
                                "size", PaneWidget::formatSize(jtf_measure_bytes(m_app))));
                }
                return;
            }
            m_measurePoll->stop();
            m_progress->setVisible(false);
            m_cancelButton->setVisible(false);
            m_statusMessage->setText(
                jtfFill(tr_("status.measured_folders"), "count", QString::number(m_measureCount)));
            refreshAll();
            // The panel is showing the folder that was just measured, and its
            // path has not changed - so nothing else would tell it to look
            // again.
            if (m_inspector != nullptr) {
                m_inspector->refreshTarget();
            }
        });
    }
    m_measureCount = queued;
    m_progress->setRange(0, 0); // indeterminate: an honest unknown
    m_progress->setVisible(true);
    m_cancelButton->setVisible(true);
    m_measurePoll->start();
}

// CV.HLP §二: `Z` 解壓縮檔案. The destination is asked for first, which is what
// CView does and what the project owner asked for.
void MainWindow::extractArchive() {
    PaneWidget *pane = activePane();
    if (pane == nullptr) { return; }
    const int paneId = pane->paneId();
    if (jtf_cursor_is_archive(m_app, paneId) == 0) {
        m_statusIsIdle = false;
        m_statusMessage->setText(tr_("status.not_an_archive"));
        return;
    }
    const QString archive =
        jtfText([&](char *b, int l) { return jtf_row_path(m_app, paneId, pane->currentRow(), b, l); });
    extractInto(archive, {});
}

void MainWindow::extractInto(const QString &archive, const QStringList &members) {
    PaneWidget *pane = activePane();
    if (pane == nullptr) { return; }
    const int paneId = pane->paneId();

    const QString suggested =
        jtfText([&](char *b, int l) { return jtf_current_path(m_app, paneId, b, l); });
    const QString destination = QFileDialog::getExistingDirectory(
        this, tr_("command.file.extract"), suggested);
    if (destination.isEmpty()) { return; }

    // Members joined by newline, because a member name may contain anything
    // except that: it is the one separator a ZIP name cannot hold.
    const QByteArray archiveUtf8 = archive.toUtf8();
    const QByteArray wanted = members.join(QLatin1Char('\n')).toUtf8();
    const QByteArray destUtf8 = destination.toUtf8();
    if (jtf_start_extract_from(m_app, archiveUtf8.constData(), destUtf8.constData(),
                               wanted.constData())
        == 0) {
        m_statusIsIdle = false;
        m_statusMessage->setText(tr_("status.not_an_archive"));
        return;
    }
    watchArchiveJob();
}

/// Enter on an archive shows what is in it, in a window of its own.
///
/// Returns whether it did, so the caller can fall back to opening the file the
/// ordinary way when it is not an archive this build can read.
void MainWindow::openUsageWindow(const QString &path) {
    QString root = path;
    if (root.isEmpty() || !QFileInfo(root).isDir()) {
        // Whatever the focused pane is showing. A file row means "measure the
        // folder it is in", which is what someone looking at a file and
        // wondering about space actually wants.
        root = jtfText([&](char *buf, int len) {
            return jtf_current_path(m_app, jtf_active_pane(m_app), buf, len);
        });
    }
    if (root.isEmpty()) {
        // A server's disc is not ours to walk: the walk reads a local tree,
        // and there is no local tree behind a remote pane.
        statusBar()->showMessage(tr_("usage.failed"), 4000);
        return;
    }
    auto *window = new UsageWindow(m_app, root, this);
    connect(window, &UsageWindow::folderChosen, this, [this](const QString &target) {
        const QByteArray utf8 = target.toUtf8();
        jtf_navigate(m_app, jtf_active_pane(m_app), utf8.constData());
        refreshAll();
    });
    // A file trashed from the report is gone from the folder the panes are
    // showing too, and an entry that is not there any more is worse than a
    // stale count.
    connect(window, &UsageWindow::folderChanged, this, [this] { refreshAll(); });
    window->show();
    window->raise();
    window->activateWindow();
}

void MainWindow::openCompareWindow() {
    // The focused pane against the one a copy would land in - the same pair
    // the「複製/移動的目標」badge already names, so "the other pane" means the
    // same thing here as it does for every other two-pane command.
    const int left = jtf_active_pane(m_app);
    const int right = jtf_target_pane(m_app);
    if (right < 0 || right == left) {
        statusBar()->showMessage(tr_("compare.needs_two_panes"), 4000);
        return;
    }
    auto *window = new CompareWindow(m_app, left, right, this);
    connect(window, &CompareWindow::stateChanged, this, [this] { refreshAll(); });
    window->show();
    window->raise();
    window->activateWindow();
}

bool MainWindow::openArchiveWindow() {
    PaneWidget *pane = activePane();
    if (pane == nullptr || jtf_cursor_is_archive(m_app, pane->paneId()) == 0) {
        return false;
    }
    const QString archive = jtfText([&](char *b, int l) {
        return jtf_row_path(m_app, pane->paneId(), pane->currentRow(), b, l);
    });
    auto *window = new ArchiveWindow(m_app, archive, this);
    if (!window->isReadable()) {
        // Named like an archive, unreadable as one. Better to hand it to the
        // platform than to show an empty window claiming it is empty.
        window->deleteLater();
        return false;
    }
    connect(window, &ArchiveWindow::extractRequested, this,
            [this](const QString &from, const QStringList &members) {
                extractInto(from, members);
            });
    window->show();
    window->raise();
    window->activateWindow();
    return true;
}

// CV.HLP §二: `Alt-Z` 壓縮檔案.
void MainWindow::compressSelection() {
    PaneWidget *pane = activePane();
    if (pane == nullptr) { return; }
    const int paneId = pane->paneId();

    const QString here =
        jtfText([&](char *b, int l) { return jtf_current_path(m_app, paneId, b, l); });
    const QString archive = QFileDialog::getSaveFileName(
        this, tr_("command.file.compress"), here + QStringLiteral("/archive.zip"),
        QStringLiteral("ZIP (*.zip)"));
    if (archive.isEmpty()) { return; }

    const QByteArray utf8 = archive.toUtf8();
    if (jtf_start_compress(m_app, paneId, utf8.constData()) == 0) {
        m_statusIsIdle = false;
        m_statusMessage->setText(tr_("status.nothing_to_compress"));
        return;
    }
    watchArchiveJob();
}

/// Watch the archive worker and report as it goes.
///
/// No percentage: the member count is known but their sizes are not until
/// they arrive, so a bar claiming a fraction would be inventing it. What is
/// shown is what has actually come out.
void MainWindow::watchArchiveJob() {
    if (m_archivePoll == nullptr) {
        m_archivePoll = new QTimer(this);
        m_archivePoll->setInterval(120);
        connect(m_archivePoll, &QTimer::timeout, this, [this] {
            const bool changed = jtf_pump_archive(m_app) != 0;
            if (jtf_is_archiving(m_app) != 0) {
                if (changed) {
                    const bool compressing = jtf_archive_is_compressing(m_app) != 0;
                    m_statusMessage->setText(jtfFill(
                        jtfFill(tr_(compressing ? "status.compressing" : "status.extracting"),
                                "files", QString::number(jtf_archive_files(m_app))),
                        "size", PaneWidget::formatSize(jtf_archive_bytes(m_app))));
                }
                return;
            }
            char reason[512] = {0};
            if (jtf_take_archive_result(m_app, reason, static_cast<int>(sizeof(reason))) == 0) {
                return; // nothing to report yet
            }
            m_archivePoll->stop();
            m_progress->setVisible(false);
            m_cancelButton->setVisible(false);
            m_statusIsIdle = false;
            const QString failure = QString::fromUtf8(reason);
            if (!failure.isEmpty()) {
                m_statusMessage->setText(
                    jtfFill(tr_("status.archive_failed"), "reason", failure));
            } else {
                QString text = jtfFill(tr_("status.archive_done"), "files",
                                       QString::number(jtf_archive_files(m_app)));
                // Refusals are said out loud. A member that would have landed
                // outside the chosen folder is exactly the thing a person
                // needs to know was in the archive.
                const quint64 refused = jtf_archive_refused(m_app);
                if (refused > 0) {
                    text += QStringLiteral("  ") +
                            jtfFill(tr_("status.archive_refused"), "count",
                                    QString::number(refused));
                }
                m_statusMessage->setText(text);
            }
            refreshAll();
        });
    }
    m_statusIsIdle = false;
    m_statusMessage->setText(tr_("status.extracting_start"));
    m_progress->setRange(0, 0);
    m_progress->setVisible(true);
    m_cancelButton->setVisible(true);
    m_archivePoll->start();
}

void MainWindow::stepFontSize(int delta) {
    const int stored = jtf_font_point_size(m_app);
    const int current = stored > 0 ? stored : listFont().pointSize();
    jtf_set_font(m_app, familyUtf8().constData(), qBound(8, current + delta, 32),
                 jtf_font_monospace(m_app));
}

void MainWindow::chooseFontFamily() {
    bool accepted = false;
    const QString chosen = dialogs::askForText(
        this, [this](const char *key) { return tr_(key); }, tr_("font.choose"),
        tr_("font.family_prompt"),
        jtfText([&](char *b, int l) { return jtf_font_family(m_app, b, l); }),
        m_theme.textPrimary, &accepted);
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
    bar->setIconSize(QSize(18, 18));

    // Every button is a command: same id, same handler, same shortcut as the
    // menu entry, so the toolbar cannot drift away from the rest of the UI
    // (docs/UI_CONVENTIONS.md 3).
    // Buttons that belong together sit in one boxed cluster, as the
    // reference layouts have them: navigation, then view, then actions. A
    // single unbroken run of icons makes the eye search the whole row for the
    // one it wants; three short groups make it search one group.
    QWidget *group = nullptr;
    QHBoxLayout *groupLayout = nullptr;
    // One height for every control on the row. Deriving it from padding meant
    // the field and the groups each arrived at their own answer, and the
    // search box sat visibly taller than the buttons either side of it.
    constexpr int kToolRowHeight = 30;
    const auto beginGroup = [&] {
        group = new QWidget(bar);
        group->setFixedHeight(kToolRowHeight);
        group->setProperty("jtfToolGroup", true);
        groupLayout = new QHBoxLayout(group);
        groupLayout->setContentsMargins(2, 2, 2, 2);
        groupLayout->setSpacing(1);
        bar->addWidget(group);
    };
    const auto endGroup = [&] {
        group = nullptr;
        groupLayout = nullptr;
    };

    const auto button = [&](const char *id, glyph::Shape shape, std::function<void()> handler,
                            bool checkable = false) {
        // A command that is already on a menu keeps that menu's action. Making
        // a second one for the toolbar meant two QActions answering to one id,
        // and runCommand triggering whichever was registered first - so
        // `search.open` from the keyboard did one thing and the same command
        // from the toolbar did another. It also means the toolbar button now
        // follows the menu entry's enabled and checked state for free.
        QAction *action = nullptr;
        for (const auto &entry : std::as_const(m_commandActions)) {
            if (QLatin1String(entry.second) == QLatin1String(id)) {
                action = entry.first;
                break;
            }
        }
        const bool fresh = action == nullptr;
        if (fresh) {
            action = new QAction(this);
            connect(action, &QAction::triggered, this,
                    [this, handler] { runAndSettleFocus(handler); });
        }
        if (checkable) {
            action->setCheckable(true);
        }
        if (groupLayout != nullptr) {
            auto *widget = new QToolButton(group);
            widget->setDefaultAction(action);
            widget->setAutoRaise(true);
            widget->setFocusPolicy(Qt::NoFocus);
            groupLayout->addWidget(widget);
        } else {
            bar->addAction(action);
        }
        if (fresh) {
            m_commandActions.append({action, id});
            m_handlers.insert(QString::fromLatin1(id), handler);
        }
        m_toolbarShapes.insert(action, shape);
        return action;
    };

    beginGroup();
    m_backAction = button("nav.back", glyph::Shape::ArrowLeft,
                          [this] { jtf_go_back(m_app, jtf_active_pane(m_app)); });
    m_forwardAction = button("nav.forward", glyph::Shape::ArrowRight,
                             [this] { jtf_go_forward(m_app, jtf_active_pane(m_app)); });
    m_upAction = button("nav.up", glyph::Shape::ArrowUp,
                        [this] { jtf_navigate_up(m_app, jtf_active_pane(m_app)); });
    m_refreshAction = button("view.refresh", glyph::Shape::Reload,
                             [this] { jtf_refresh(m_app, jtf_active_pane(m_app)); });
    endGroup();

    beginGroup();
    m_listModeAction = button(
        "view.mode.list", glyph::Shape::List,
        [this] { jtf_set_view_mode(m_app, jtf_active_pane(m_app), 0); }, true);
    m_gridModeAction = button(
        "view.mode.grid", glyph::Shape::Grid,
        [this] { jtf_set_view_mode(m_app, jtf_active_pane(m_app), 1); }, true);

    endGroup();

    // The two above are a choice - exactly one view mode is on. The rest are
    // independent toggles. Framing them together made a row of seven
    // identical squares where the eye could not tell a radio from a switch,
    // which is the reference's reason for keeping its segmented view control
    // apart from its panel buttons.
    beginGroup();
    m_treeAction = button("view.tree", glyph::Shape::Sidebar, [this] { toggleTree(); }, true);
    m_inspectorAction = button(
        "view.inspector", glyph::Shape::Inspector,
        [this] { setInspectorVisible(!m_inspector->isVisible()); }, true);
    // The way back. Splitting had two buttons and the quad preset a third,
    // but returning to one pane existed only in the menu - so a split window
    // was easy to get into and, for anyone looking at the toolbar, not
    // obviously possible to get out of. The four now read as one set: one
    // pane, split across, split down, four.
    button("workspace.preset.single", glyph::Shape::SplitSingle,
           [this] { jtf_apply_preset(m_app, 0); });
    button("workspace.split.horizontal", glyph::Shape::SplitHorizontal,
           [this] { jtf_split_active(m_app, 0); });
    button("workspace.split.vertical", glyph::Shape::SplitVertical,
           [this] { jtf_split_active(m_app, 1); });
    // Four at once, in one press. Reaching it by splitting twice in the right
    // order is a trick, not a layout, and the menu had it while the toolbar -
    // where you actually change layout - did not.
    button("workspace.preset.quad", glyph::Shape::SplitQuad,
           [this] { jtf_apply_preset(m_app, 3); });
    endGroup();

    // The path lives in the breadcrumb now - click its empty space and it
    // becomes the editable full path, the way Explorer's address bar does.
    // That frees this space, which the reference layouts spend on search.
    m_searchEdit = new QLineEdit(bar);
    m_searchEdit->setObjectName(QStringLiteral("JtfToolbarSearch"));
    m_searchEdit->setClearButtonEnabled(true);
    m_searchEdit->setMinimumWidth(240);
    m_searchEdit->setFixedHeight(kToolRowHeight);
    // The icon lives inside the field rather than beside it. Beside it, it is
    // a button the user may try to press; inside, it is the field's label.
    // Re-tinted on every theme change by syncToolbar.
    m_searchIconAction = m_searchEdit->addAction(QIcon(), QLineEdit::LeadingPosition);
    // Set in retranslate too, so it follows the language.
    bar->addWidget(m_searchEdit);
    connect(m_searchEdit, &QLineEdit::returnPressed, this, [this] {
        if (PaneWidget *pane = activePane()) {
            pane->searchFor(m_searchEdit->text().trimmed());
        }
    });
    connect(m_searchEdit, &QLineEdit::textChanged, this, [this](const QString &text) {
        if (text.isEmpty()) {
            if (PaneWidget *pane = activePane()) {
                pane->clearSearch();
            }
        }
    });

    beginGroup();
    button("file.new_folder", glyph::Shape::NewFolder, [this] { runOperation(OpNewFolder); });
    button("view.filter", glyph::Shape::Filter, [this] {
        if (PaneWidget *pane = activePane()) {
            pane->toggleFilter();
        }
    });
    button("search.open", glyph::Shape::Search, [this] { focusSearchField(); });
    m_hiddenAction = button(
        "view.hidden", glyph::Shape::Hidden,
        [this] { jtf_set_show_hidden(m_app, jtf_show_hidden(m_app) ? 0 : 1); }, true);
    button("help.shortcuts", glyph::Shape::Keyboard, [this] { openShortcuts(); });
    button("settings.open", glyph::Shape::Settings, [this] { openSettings(); });
    endGroup();

    // The keyboard-mode switch. A segmented control, not two buttons: the
    // pill slides from one side to the other, which says which way the mode
    // went rather than just repainting to show where it landed.
    auto *modeHolder = new QWidget(bar);
    auto *modeRow = new QHBoxLayout(modeHolder);
    modeRow->setContentsMargins(8, 0, 4, 0);
    m_modeSwitch = new ModeSwitch(modeHolder);
    connect(m_modeSwitch, &ModeSwitch::segmentClicked, this, [this](int index) {
        setKeymap(index == 0 ? QStringLiteral("single-key") : QStringLiteral("native"));
    });
    modeRow->addWidget(m_modeSwitch);
    bar->addWidget(modeHolder);

    auto *focusPath = new QAction(this);
    focusPath->setShortcut(QKeySequence(QStringLiteral("Ctrl+L")));
    connect(focusPath, &QAction::triggered, this, [this] {
        if (PaneWidget *pane = activePane()) {
            pane->editPath();
        }
    });
    addAction(focusPath);
}

namespace {

/// Commands that are about a file *on this machine*.
///
/// `/srv/data` on a server and `/srv/data` here are different files and the
/// platform cannot tell them apart, so handing it a remote path does not fail
/// - it acts on whatever local file happens to have that name. The context
/// menu already left these out on a remote row; the menu bar and the keyboard
/// did not, and both reach the same handlers. This is that list, in one place,
/// applied where neither route can go around it.
const char *const kLocalOnlyCommands[] = {
    "preview.quicklook",
    "file.reveal",
    "file.terminal",
    "file.edit",
    "file.share",
    // The clipboard carries `file://` URLs for other applications to paste;
    // a server path in one of those points at the wrong machine.
    "file.clipboard.copy",
    "file.clipboard.cut",
    // SFTP has no trash. Stage two deletes remotely and permanently, and
    // saying so is the point of leaving this out rather than having it fail.
    "file.trash",
    nullptr,
};

} // namespace

void MainWindow::syncToolbar() {
    const int pane = chromePaneId();

    // Enabled here rather than at each call site: `runCommand` triggers the
    // very same QAction the menu does and honours `isEnabled`, so disabling it
    // once closes the menu entry, the toolbar button and the shortcut
    // together.
    const bool remote = jtf_pane_is_remote(m_app, pane) != 0;
    for (const auto &entry : std::as_const(m_commandActions)) {
        for (const char *const *id = kLocalOnlyCommands; *id != nullptr; ++id) {
            if (qstrcmp(entry.second, *id) == 0) {
                entry.first->setEnabled(!remote);
                break;
            }
        }
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
    if (m_modeSwitch) {
        m_modeSwitch->setSegments({profileLabel(QStringLiteral("single-key")),
                                   profileLabel(QStringLiteral("native"))});
        m_modeSwitch->setCurrentIndex(keymap == QLatin1String("native") ? 1 : 0);
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
    if (m_keyHintsButton) {
        QSignalBlocker blocker(m_keyHintsButton);
        m_keyHintsButton->setChecked(m_keyHints && m_keyHints->isVisible());
    }
    const bool grid = jtf_view_mode(m_app, pane) != 0;
    if (m_listModeAction) {
        QSignalBlocker blocker(m_listModeAction);
        m_listModeAction->setChecked(!grid);
    }
    if (m_gridModeAction) {
        QSignalBlocker blocker(m_gridModeAction);
        m_gridModeAction->setChecked(grid);
    }
    if (m_treeAction) {
        QSignalBlocker blocker(m_treeAction);
        m_treeAction->setChecked(m_tree && m_tree->isVisible());
    }
    if (m_hiddenAction) {
        QSignalBlocker blocker(m_hiddenAction);
        const bool showing = jtf_show_hidden(m_app) != 0;
        m_hiddenAction->setChecked(showing);
        // Open eye while hidden files are showing, closed while they are not.
        // A checked-state background alone is easy to miss on a toolbar; the
        // icon changing shape says which way the toggle is without looking
        // for a highlight.
        m_toolbarShapes.insert(m_hiddenAction,
                               showing ? glyph::Shape::Visible : glyph::Shape::Hidden);
        m_hiddenAction->setIcon(glyph::make(m_toolbarShapes.value(m_hiddenAction),
                                            m_theme.textPrimary));
    }
}

QFont MainWindow::fixedListFont() const {
    // The same size and family rules as `listFont`, but always fixed-width.
    // The list needs both at once: one face for names, one for the columns
    // that are read down.
    const QString family =
        jtfText([&](char *buf, int len) { return jtf_font_family(m_app, buf, len); });
    QFont font = QFontDatabase::systemFont(QFontDatabase::FixedFont);
    if (!family.isEmpty() && jtf_font_monospace(m_app) != 0) {
        font.setFamily(family);
        font.setStyleHint(QFont::Monospace, QFont::PreferMatch);
    }
    const int size = jtf_font_point_size(m_app);
    if (size > 0) {
        font.setPointSize(size);
    }
    return font;
}

QFont MainWindow::listFont() const {
    const QString family =
        jtfText([&](char *buf, int len) { return jtf_font_family(m_app, buf, len); });
    const int size = jtf_font_point_size(m_app);
    // The face for names and for everything outside the list.
    //
    // Fixed-width only when the user asked for it *everywhere*; the default is
    // fixed-width on the aligned columns alone, which `fixedListFont` supplies
    // and the model applies per column. A monospace face across a list of file
    // names is harder to read than proportional type, which is what names are
    // set in in every other file manager.
    const bool monospace =
        jtf_font_monospace(m_app) != 0 && jtf_font_monospace_everywhere(m_app) != 0;

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
    const QFont fixed = fixedListFont();
    const bool everywhere = jtf_font_monospace(m_app) != 0
                            && jtf_font_monospace_everywhere(m_app) != 0;
    for (auto *pane : std::as_const(m_panes)) {
        pane->setListFont(font, fixed, everywhere);
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
        // Moving the cursor is not a state change - it does not touch the
        // layout, the tabs or the folder - so it deliberately does not go
        // through refreshAll. But it does change what the preview panel, the
        // status line and the hint strip are describing, and until this was
        // connected the panel went on showing whichever file happened to be
        // current when it was opened.
        connect(pane, &PaneWidget::selectionChanged, this, [this] {
            if (m_keyHints) {
                m_keyHints->noteActivity();
            }
            syncInspector();
            syncKeyHints();
            updateStatus();
        });
        connect(pane, &PaneWidget::commandRequested, this, &MainWindow::runCommand);
        connect(pane, &PaneWidget::tearOffRequested, this, [this, paneId](int tabIndex) {
            if (jtf_tear_off_tab(m_app, paneId, tabIndex) != 0) {
                jtf_app_save_session(m_app);
                MainWindow::syncWindows(m_app);
            }
        });
        connect(pane, &PaneWidget::tabMergeRequested, this,
                [this](int from, int tabIndex, int into) {
                    if (jtf_merge_tab_into(m_app, from, tabIndex, into) != 0) {
                        jtf_app_save_session(m_app);
                        MainWindow::syncWindows(m_app);
                    }
                });
        connect(pane, &PaneWidget::crumbMenuRequested, this,
                [this, paneId](const QString &path, const QPoint &global) {
                    showCrumbMenu(paneId, path, global);
                });
        connect(pane, &PaneWidget::contextMenuRequested, this,
                [this, paneId](const QPoint &global, bool onEntry) {
                    showEntryMenu(paneId, global, onEntry);
                });
        connect(pane, &PaneWidget::reconnectRequested, this, [this](int id) {
            jtf_focus_pane(m_app, id);
            // Forget that we already asked. "Try again" is the user saying the
            // last attempt should not count, and the password prompt is the
            // main thing a retry needs to be able to do.
            m_askedForPassword.remove(id);
            jtf_reconnect(m_app, id);
            refreshAll();
        });
        connect(pane, &PaneWidget::dropRequested, this,
                [this, paneId](const QStringList &paths, int fromUs) {
                    runDrop(paneId, paths, fromUs != 0);
                });
        m_panes.insert(paneId, pane);
        return pane;
    }

    const bool vertical = node.value(QStringLiteral("vertical")).toBool();
    auto *splitter = new QSplitter(vertical ? Qt::Vertical : Qt::Horizontal);
    splitter->setChildrenCollapsible(false);
    splitter->setHandleWidth(7);
    splitter->addWidget(buildNode(node.value(QStringLiteral("first")).toObject()));
    splitter->addWidget(buildNode(node.value(QStringLiteral("second")).toObject()));

    const double ratio = node.value(QStringLiteral("ratio")).toDouble(0.5);
    const int total = 1000;
    splitter->setSizes({static_cast<int>(ratio * total), total - static_cast<int>(ratio * total)});
    return splitter;
}

void MainWindow::focusNextArea() {
    // The parts of the window that can hold the keyboard, in the order they
    // are laid out: the places, the folder tree, then the panes in visual
    // order. Tab walks them; Shift-Tab is Qt's own and still works inside a
    // list.
    //
    // Only what is on screen. The folder tree folds away, and a stop at a
    // hidden widget is a press that appears to do nothing - which teaches
    // people the key is unreliable rather than that the tree is closed.
    QList<QWidget *> stops;
    if (m_places != nullptr && m_places->isVisible()) {
        stops.append(m_places);
    }
    if (m_tree != nullptr && m_tree->isVisible()) {
        stops.append(m_tree);
    }
    const int paneCount = jtf_pane_count(m_app);
    QList<int> paneIds;
    for (int index = 0; index < paneCount; ++index) {
        const int id = jtf_pane_id_at(m_app, index);
        if (auto *pane = m_panes.value(id, nullptr)) {
            if (pane->isVisible()) {
                stops.append(pane);
                paneIds.append(id);
            }
        }
    }
    if (stops.isEmpty()) {
        return;
    }

    // Where the keyboard is now. Asked of the focus widget rather than of a
    // remembered value: the focus moves for reasons this function never sees.
    int at = -1;
    const QWidget *focused = QApplication::focusWidget();
    for (int index = 0; index < stops.size() && at < 0; ++index) {
        for (const QWidget *w = focused; w != nullptr; w = w->parentWidget()) {
            if (w == stops.at(index)) {
                at = index;
                break;
            }
        }
    }

    QWidget *next = stops.at((at + 1) % stops.size());
    // A pane is focused through the model as well as the widget, so the rest
    // of the program agrees about which one is active.
    const int paneIndex = stops.indexOf(next) - (stops.size() - paneIds.size());
    if (paneIndex >= 0 && paneIndex < paneIds.size()) {
        jtf_focus_pane(m_app, paneIds.at(paneIndex));
        markActivePane();
    }
    next->setFocus(Qt::TabFocusReason);
}

void MainWindow::toggleTree() {
    setTreeVisible(!m_tree->isVisible());
}

void MainWindow::applySidebarWidth() {
    // Applied after the current event, when the splitter knows how wide it
    // is. QSplitter::setSizes on a splitter that has not been laid out treats
    // the numbers as *proportions*: asking for 160 of a nominal 600 became
    // 316 of a real 1180.
    //
    // Unconditional now. It used to run only when the folder tree was turned
    // on, which was the same thing as "the sidebar appeared" - until the
    // sidebar became permanent and the tree the only foldable half. With the
    // tree folded away at startup nothing applied a width at all, the
    // splitter gave the sidebar half the window, and dragging any divider
    // saved that as the user's preference.
    QTimer::singleShot(0, this, [this] {
        const int total = m_outer->width();
        if (total <= 0) {
            return;
        }
        // A width narrower than this cannot have been chosen - the divider
        // will not go there - so a stored value below it is wreckage rather
        // than a preference, and clamping it *up* to the minimum would
        // honour a number nobody picked. It is treated as absent instead.
        static constexpr int kUsable = 170;
        static constexpr int kDefault = 240;
        const int stored = jtf_tree_width(m_app);
        const int wanted = stored >= kUsable ? stored : kDefault;
        // And a sidebar wider than a quarter of the window is not a sidebar.
        const int sidebar = qBound(kUsable, wanted, qMax(kUsable, total / 4));
        m_outer->setSizes({sidebar, total - sidebar});
    });
}

void MainWindow::setTreeVisible(bool visible) {
    // The tree half only. The special places above it stay.
    m_tree->setVisible(visible);
    applySidebarWidth();
    if (visible) {
        m_places->refresh();
        syncTree();
    }
    // The width is recorded when the user drags the divider, not here: at
    // this point the splitter may not have applied the size it was just
    // given, and writing back what it currently reports is how a wrong value
    // gets saved and then restored next launch.
    jtf_set_tree_state(m_app, visible ? 1 : 0, jtf_tree_width(m_app));
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

MainWindow::~MainWindow() { windows().removeAll(this); }

void MainWindow::runAndSettleFocus(const std::function<void()> &handler) {
    // Whether this was a navigation is not something the command id can be
    // trusted to say - "go to bookmark", "open", "up", a breadcrumb segment
    // and a tree click are all navigations with different names, and new ones
    // will be added. So it is decided by what happened: if the active pane is
    // showing a different folder afterwards, the user moved, and the keyboard
    // belongs back in the list they moved to.
    const int pane = jtf_active_pane(m_app);
    const QString before =
        jtfText([&](char *b, int l) { return jtf_current_path(m_app, pane, b, l); });
    handler();
    refreshAll();
    const int after_pane = jtf_active_pane(m_app);
    const QString after =
        jtfText([&](char *b, int l) { return jtf_current_path(m_app, after_pane, b, l); });
    if (after != before) {
        // After the current event has finished. A menu returns the focus to
        // whatever had it before the menu opened, and that happens as the
        // menu closes - which is after this handler runs. Setting the focus
        // here is setting it before it is taken away again.
        QTimer::singleShot(0, this, [this] { returnFocusToList(); });
    }
}

void MainWindow::returnFocusToList() {
    // After navigating, the keyboard belongs in the list again. Commands
    // reached from a menu, the toolbar or the breadcrumb leave the focus
    // wherever the mouse put it, and then Left and Right - which are the
    // folder hierarchy in this program - go to whatever has it instead. The
    // one thing that must never be interrupted is typing, so a text field
    // keeps the focus it has.
    if (qobject_cast<QLineEdit *>(QApplication::focusWidget()) != nullptr) {
        return;
    }
    if (PaneWidget *pane = activePane()) {
        pane->focusList();
    }
}

void MainWindow::runCommand(const QString &id) {
    // `Z` reads the row it is on.
    //
    // CView's `Z` is 解壓縮; ours also measures a folder, because CView never
    // measured folders and the key was free. One key, two meanings, decided
    // by what the cursor is on - which is how `Enter` already behaves, and
    // what the project owner asked for. The menu entries stay separate and
    // each does only its own thing; this is about the key.
    if (id == QLatin1String("file.folder_size")) {
        PaneWidget *pane = activePane();
        if (pane != nullptr && jtf_cursor_is_archive(m_app, pane->paneId()) != 0) {
            runCommand(QStringLiteral("file.extract"));
            return;
        }
    }
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

void MainWindow::showKeyHintMenu(const QPoint &global) {
    QMenu menu(this);
    auto *heading = menu.addAction(tr_("hints.density"));
    heading->setEnabled(false);
    auto *group = new QActionGroup(&menu);
    const int current = jtf_key_hints_density(m_app);
    const char *const keys[] = {"hints.density.full", "hints.density.compact",
                                "hints.density.auto"};
    for (int mode = 0; mode < 3; ++mode) {
        QAction *entry = menu.addAction(tr_(keys[mode]));
        entry->setCheckable(true);
        entry->setChecked(mode == current);
        group->addAction(entry);
        connect(entry, &QAction::triggered, this, [this, mode] { setKeyHintDensity(mode); });
    }
    menu.exec(global);
}

void MainWindow::setKeyHintDensity(int density) {
    jtf_set_key_hints_density(m_app, density);
    if (m_keyHints) {
        m_keyHints->setDensity(static_cast<KeyHintBar::Density>(density));
    }
}

void MainWindow::setInspectorPosition(int position) {
    if (m_inspector == nullptr || m_paneColumn == nullptr) {
        return;
    }
    // Beside the list, or under it. A tall thin panel is right for a column of
    // facts and wrong for a wide page or a landscape photograph, and which of
    // those someone looks at is not something this program can know.
    const bool below = position == 1;
    if (below) {
        if (m_paneColumn->indexOf(m_inspector) < 0) {
            m_paneColumn->addWidget(m_inspector);
        }
    } else if (m_outer->indexOf(m_inspector) < 0) {
        m_outer->insertWidget(m_outer->indexOf(m_paneColumn) + 1, m_inspector);
    }
    m_inspectorPosition = position;
}

void MainWindow::setKeyHintsVisible(bool visible) {
    m_keyHints->setVisible(visible);
    if (m_keyHintsButton) {
        QSignalBlocker blocker(m_keyHintsButton);
        m_keyHintsButton->setChecked(visible);
    }
    jtf_set_key_hints_visible(m_app, visible ? 1 : 0);
    syncKeyHints();
}

void MainWindow::syncKeyHints() {
    if (m_keyHints == nullptr || !m_keyHints->isVisible()) {
        return;
    }
    PaneWidget *pane = activePane();
    if (pane == nullptr) {
        return;
    }
    const int id = pane->paneId();
    // The row under the cursor decides, because the strip is meant to answer
    // "what can I do with *this*" as the cursor moves - that is what it is
    // for, and the ordering was specified as most-useful-for-the-current-item
    // first.
    //
    // Marks used to win outright, so two marked files an hour ago froze the
    // strip on the several-items list and moving the cursor between a file
    // and a folder changed nothing. Marks now only speak when the cursor has
    // nothing to say: an empty folder, or a listing with no current row.
    KeyHintBar::Context context = KeyHintBar::Context::Nothing;
    const int row = pane->currentRow();
    if (row >= 0 && !jtf_row_is_parent(m_app, id, row)) {
        context = jtf_row_is_directory(m_app, id, row) ? KeyHintBar::Context::Folder
                                                       : KeyHintBar::Context::File;
    } else if (jtf_marked_count(m_app, id) > 1) {
        context = KeyHintBar::Context::Several;
    }
    m_keyHints->update(context, jtf_pane_count(m_app) > 1);
}

void MainWindow::syncInspector() {
    if (!m_inspector->isVisible()) {
        return;
    }
    PaneWidget *active = activePane();
    if (!active) {
        return;
    }

    // Held arrow keys walk the list faster than a file can be read. Reading
    // one per row means the disk is asked for hundreds of files nobody looked
    // at, and the panel flickers through them on the way to the row the user
    // actually stopped on. So the request waits for the cursor to settle -
    // and the wait restarts on every move, so the only file read is the one
    // still under the cursor when the keys stop.
    static constexpr int kSettleMs = 140;
    if (m_inspectorSettle == nullptr) {
        m_inspectorSettle = new QTimer(this);
        m_inspectorSettle->setSingleShot(true);
        m_inspectorSettle->setInterval(kSettleMs);
        connect(m_inspectorSettle, &QTimer::timeout, this, [this] { showInspectorTarget(); });
    }
    m_inspectorSettle->start();
}

void MainWindow::showInspectorTarget() {
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
    // A remote row's path is a path on the *server*. Reading it here would
    // open whatever local file happens to share that name - `/etc/hosts` on
    // the server previewing this machine's `/etc/hosts` - which is worse than
    // showing nothing. Remote preview arrives with the rest of stage two.
    if (jtf_pane_is_remote(m_app, pane) != 0) {
        m_inspector->setTarget(QString(), marked);
        return;
    }
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

void MainWindow::askForServerPassword(int pane) {
    // Only when the server refused the *sign-in*. A folder the account cannot
    // read is not something a password fixes, and asking there would be noise.
    if (jtf_pane_needs_credentials(m_app, pane) == 0) {
        // The pane is fine, or has failed some other way: let it be asked
        // again if it next fails to sign in.
        m_askedForPassword.remove(pane);
        return;
    }
    // Once per failure. Without this the prompt would return on every refresh,
    // including the one its own retry causes.
    if (m_askedForPassword.contains(pane)) {
        return;
    }
    m_askedForPassword.insert(pane);

    const QString where =
        jtfText([&](char *b, int l) { return jtf_display_path(m_app, pane, b, l); });
    bool accepted = false;
    const QString password = dialogs::askForPassword(
        this, [this](const char *key) { return tr_(key); }, tr_("remote.sign_in"),
        jtfFill(tr_("remote.password_for"), "server", where), m_theme.textPrimary, &accepted);
    if (!accepted || password.isEmpty()) {
        return;
    }
    const QByteArray utf8 = password.toUtf8();
    jtf_pane_set_password(m_app, pane, utf8.constData());
    refreshAll();
}

void MainWindow::syncTree() {
    if (!m_sidebar->isVisible()) {
        return;
    }
    m_places->refresh();
    const int pane = jtf_active_pane(m_app);
    m_tree->selectPath(
        jtfText([&](char *buf, int len) { return jtf_display_path(m_app, pane, buf, len); }));
}

void MainWindow::rebuildLayout() {
    const QString json = jtfText([&](char *buf, int len) {
        return jtf_window_layout_json(m_app, m_windowId, buf, len);
    });
    if (json.isEmpty()) {
        return; // this window is gone from the model; syncWindows will close it
    }
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
        const int slot = m_paneColumn->indexOf(m_paneArea);
        Q_ASSERT(slot >= 0);
        m_paneColumn->replaceWidget(slot, widget);
        m_paneArea->deleteLater();
    } else {
        // Always first in the column; the inspector may or may not follow it.
        m_paneColumn->insertWidget(0, widget);
    }
    m_paneArea = widget;
    m_root = widget;
    applyTheme();
    markActivePane();
}

PaneWidget *MainWindow::activePane() const {
    return m_panes.value(jtf_active_pane(m_app), nullptr);
}

int MainWindow::chromePaneId() const {
    // The active pane when it is one of ours; otherwise any of ours.
    //
    // Back, Forward and Up were enabled from `jtf_active_pane` regardless of
    // which window held it. With two windows open and the other one sitting at
    // `/`, "can this go up?" was answered about that pane - so Up went grey
    // here, and because the keyboard triggers the very same QAction, the Left
    // key stopped going up as well, in a window that was nowhere near the
    // root.
    const int active = jtf_active_pane(m_app);
    if (m_panes.contains(active)) {
        return active;
    }
    return m_panes.isEmpty() ? active : m_panes.constBegin().key();
}

void MainWindow::markActivePane() {
    const int active = jtf_active_pane(m_app);
    // Asked of the core, not worked out here: "the other one" is only right
    // for two panes, and the operation itself does not use that rule.
    const int target = jtf_target_pane(m_app);
    // The ring says *which* pane has the keyboard. With one pane there is no
    // which - it is the only one - so the ring is decoration that draws the
    // eye to a question nobody asked.
    const bool several = m_panes.size() > 1;
    for (auto it = m_panes.begin(); it != m_panes.end(); ++it) {
        it.value()->setActive(several && it.key() == active);
        it.value()->setTarget(it.key() == target && it.key() != active);
    }
    // The tree shows where the focused pane is, so it moves when the focus
    // does. It lives here rather than at each place that focuses a pane -
    // clicking into one, a keyboard switch, a rebuild - because a fourth way
    // to focus a pane would otherwise arrive without it and the tree would go
    // stale again for that one path only.
    syncTree();
    syncInspector();
    syncWindowTitle();
}

void MainWindow::refreshAll() {
    rebuildLayout();
    applyFont();
    for (auto *pane : std::as_const(m_panes)) {
        pane->refresh();
    }
    markActivePane(); // syncs the tree and the inspector with it
    syncToolbar();
    syncKeyHints();
    retranslate();

    checkServerCredentials();
}

void MainWindow::checkServerCredentials() {
    // A server that would not let us in has nowhere else to say so, and the
    // pane cannot ask on its own. Deferred by one turn of the event loop so
    // whatever is refreshing finishes before a modal dialog goes up on it.
    //
    // Called from the pump as well as from a full refresh. A sign-in fails on
    // a worker thread, and the tick that notices only refreshes rows and the
    // status line - so after「重新連線」the failure arrived with nobody
    // looking, and the prompt that the retry exists to raise never came.
    int asking = -1;
    const QList<int> panes = m_panes.keys();
    for (const int pane : panes) {
        if (jtf_pane_needs_credentials(m_app, pane) != 0) {
            if (asking < 0) {
                asking = pane;
            }
            continue;
        }
        // Signed in, or failed some other way: let it be asked again if it
        // next fails to sign in.
        m_askedForPassword.remove(pane);
    }
    if (asking >= 0 && !m_askedForPassword.contains(asking)) {
        QTimer::singleShot(0, this, [this, asking] { askForServerPassword(asking); });
    }
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
        // Not jtf_row_count: that includes the `..` row, which is a way out
        // of the folder and not a thing in it. Finder, Explorer and Total
        // Commander all leave it out of the count, and the pane's own status
        // line already did - so the two lines disagreed by one.
        items += jtf_listed_count(m_app, id);
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
    // A summary with nothing to say is hidden rather than left blank: an empty
    // label still takes its padding and draws its divider, which is a column's
    // width spent on a number that is not there.
    for (QLabel *label : {m_statusSelection, m_statusTasks}) {
        label->setVisible(!label->text().isEmpty());
    }
    // The chip opens the list of what the keyboard does, so it is named after
    // the thing a click produces. It used to carry the current mode as well -
    // dropped, because the toolbar's mode switch says that a few centimetres
    // away, and a control whose label is half status readout reads as a status
    // readout and does not get clicked.
    m_statusKeymap->setText(tr_("status.keymap_chip"));
    m_statusKeymap->setToolTip(tr_("command.help.shortcuts"));
    m_statusKeymap->setIcon(glyph::make(glyph::Shape::Keyboard, m_theme.textOnAccent));
    // One running, plus whatever is waiting behind it.
    const int queued = jtf_op_queued(m_app);
    const int active = (jtf_op_running(m_app) ? 1 : 0) + queued;
    m_statusTasks->setText(
        active > 0 ? jtfFill(tr_("status.tasks_running"), "count", QString::number(active))
                   : QString());
    // A set that could not take everything says so. It is the one limit in
    // this program a user can walk into by pressing one key - the header's
    // box, over a folder larger than the set holds - and files that were not
    // marked will not be copied.
    const int refused = jtf_marks_refused(m_app, jtf_active_pane(m_app));
    if (refused > 0) {
        m_statusIsIdle = false;
        m_statusMessage->setText(
            jtfFill(tr_("status.marks_full"), "count", QString::number(refused)));
        return;
    }

    // A running search says where it has got to. The overlay over the list
    // gives a count, and a count on a home folder is the same number whether
    // the walk is in `Downloads` or four levels into a cache - which is the
    // thing that decides whether to wait or to narrow the search.
    const QString searchIn = jtfText([&](char *buf, int len) {
        return jtf_search_in(m_app, jtf_active_pane(m_app), buf, len);
    });
    if (!searchIn.isEmpty()) {
        m_statusMessage->setText(jtfFill(tr_("status.searching_in"), "path", searchIn));
    } else if (m_statusIsIdle) {
        // Tracked with a flag rather than by testing for an empty string:
        // after a language change the label still holds the *previous*
        // language's "Ready", which is not empty and would never be replaced.
        m_statusMessage->setText(tr_("status.ready"));
    }
}

void MainWindow::retranslate() {
    if (m_keyHints) {
        m_keyHints->invalidate();
        syncKeyHints();
    }
    // Everything with words in it, including the parts the frame pump owns:
    // changing the language is exactly the case where nothing else changed.
    updateStatusSummary();
    if (m_keyHintsButton) {
        m_keyHintsButton->setToolTip(tr_("hints.toggle"));
    }
    if (m_searchEdit) {
        // An empty box with no label is a box that does not say what it is
        // for; the placeholder is the only label it gets.
        m_searchEdit->setPlaceholderText(tr_("search.placeholder_toolbar"));
    }
    for (const auto &entry : std::as_const(m_translatable)) {
        entry.first->setText(tr_(entry.second));
    }
    applyCommandBindings();
    for (const auto &entry : std::as_const(m_translatableMenus)) {
        entry.first->setTitle(tr_(entry.second));
    }
    syncWindowTitle();
    m_places->retranslate();
    m_tree->retranslate();
    m_cancelButton->setText(tr_("operation.cancel"));
    for (auto *p : std::as_const(m_panes)) {
        p->retranslate();
    }
}

void MainWindow::syncWindowTitle() {
    // The window title names what you are looking at, then the application.
    // A title that only ever says the app name is a wasted line.
    //
    // Called on every focus change as well as on every refresh: what the
    // window is "looking at" is the focused pane's folder, so moving the focus
    // to the other pane and leaving the title naming the first one made the
    // title describe a pane the user had left.
    const QString folder = jtfText([&](char *buf, int len) {
        return jtf_current_name(m_app, jtf_active_pane(m_app), buf, len);
    });
    // The version travels with the title, so a screenshot or a bug report
    // says which build it came from without anyone having to go and look.
    const QString version =
        jtfText([](char *buf, int len) { return jtf_app_version(buf, len); });
    const QString named =
        version.isEmpty() ? tr_("app.name")
                          : tr_("app.name") + QLatin1Char(' ') + version;
    setWindowTitle(folder.isEmpty() ? named : folder + QStringLiteral(" — ") + named);
    // The proxy icon: macOS shows the icon of the file or folder a window
    // stands for, beside its title, and lets you drag it. Our window stands
    // for the folder the active pane is showing, so saying which folder gets
    // that behaviour for free and matches what Finder does.
    const QString here = jtfText([&](char *buf, int len) {
        return jtf_current_path(m_app, jtf_active_pane(m_app), buf, len);
    });
    if (windowFilePath() != here) {
        setWindowFilePath(here);
    }
}

// Whether the desktop is asking for a dark appearance.
//
// `QStyleHints::colorScheme` is Qt 6.5 and later. Ubuntu 24.04 LTS ships
// 6.4.2, and building against the distribution's own Qt is the difference
// between a package and a tarball - so older Qt falls back to reading the
// palette, which is what everyone did before the hint existed: a window
// background darker than its text means a dark desktop.
//
// The fallback cannot notice the desktop changing while the program runs, so
// Follow System there means "at launch". That is a real limitation of the
// older Qt, said here rather than left to be discovered.
static bool systemPrefersDark() {
#if QT_VERSION >= QT_VERSION_CHECK(6, 5, 0)
    return QApplication::styleHints()->colorScheme() == Qt::ColorScheme::Dark;
#else
    const QPalette palette = QApplication::palette();
    return palette.color(QPalette::Window).lightness()
           < palette.color(QPalette::WindowText).lightness();
#endif
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

    const bool systemDark = systemPrefersDark();
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

    if (m_searchIconAction) {
        // Dimmed: it labels the field, it is not something to look at.
        m_searchIconAction->setIcon(glyph::make(glyph::Shape::Search, m_theme.textSecondary));
    }
    if (m_keyHintsButton) {
        // Not in m_toolbarShapes any more - it lives on the status bar now, so
        // it is repainted here rather than by the toolbar's loop. Without this
        // it came up as a blank square.
        m_keyHintsButton->setIcon(glyph::make(glyph::Shape::HintBar, m_theme.textSecondary));
    }

    if (m_inspector) {
        m_inspector->applyTheme(m_theme.textSecondary, m_theme.preview);
    }
    if (m_places) {
        m_places->applyTheme(m_theme.textSecondary, m_theme.indicator, m_theme.indicator,
                             m_theme.mark, m_theme.error, m_theme.selection, m_theme.rowHover,
                             m_theme.textOnAccent);
    }
    if (m_keyHints) {
        m_keyHints->applyTheme(m_theme.textPrimary, m_theme.textSecondary, m_theme.header);
    }
    if (m_modeSwitch) {
        m_modeSwitch->applyTheme(m_theme.window, m_theme.border, m_theme.selection,
                                 m_theme.textPrimary, m_theme.textSecondary,
                                 m_theme.textOnAccent);
    }
    // Every command that has an icon shows it, in the menu as well as on the
    // toolbar: the same command should carry the same picture wherever it
    // appears, or the picture teaches nothing.
    for (const auto &entry : std::as_const(m_commandActions)) {
        const QString id = QString::fromLatin1(entry.second);
        if (!m_toolbarShapes.contains(entry.first) && glyph::hasCommandIcon(id)) {
            entry.first->setIcon(glyph::forCommand(id, m_theme.textSecondary));
        }
    }
    if (m_viewer != nullptr) {
        m_viewer->applyTheme(m_theme.mark, m_theme.textPrimary);
    }
    for (auto *pane : std::as_const(m_panes)) {
        pane->applyTheme(m_theme.mark,
                         m_theme.textPrimary,
                         m_theme.textSecondary,
                         m_theme.indicator,
                         m_theme.border,
                         m_theme.executable);
    }
    m_applyingTheme = false;
}

void MainWindow::closeEvent(QCloseEvent *event) {
    // The same guard the drag path uses: a width the layout invented is not a
    // width to come back to next launch.
    const int sidebar = m_outer->sizes().value(0);
    if (sidebar >= 170 && sidebar <= qMax(170, m_outer->width() / 3)) {
        jtf_set_tree_state(m_app, m_tree->isVisible() ? 1 : 0, sidebar);
    }
    // Closing the main window quits: the torn-off ones are parts of the same
    // workspace, not independent documents, so leaving them behind would
    // leave the program running with its centre gone.
    if (m_windowId == 1) {
        const QList<MainWindow *> others = windows();
        for (MainWindow *window : others) {
            if (window != this) {
                window->m_quitting = true;
                window->close();
            }
        }
    } else if (!m_quitting) {
        // A torn-off window the user dismissed. Closing the widget is not
        // enough: the workspace still holds the window, the session records
        // it, and the next launch opens it again - which is why the program
        // kept coming back with two windows however many times one was
        // closed. `close_pane` drops a torn-off window once its last pane
        // goes, so closing this window's panes is what actually closes it.
        const QList<int> ours = m_panes.keys();
        for (const int pane : ours) {
            jtf_close_pane(m_app, pane);
        }
    }
    jtf_app_save_session(m_app);
    QMainWindow::closeEvent(event);
}
