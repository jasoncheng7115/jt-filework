#include "panewidget.h"

#include <algorithm>

#include <QItemSelection>
#include "filelistmodel.h"
#include "breadcrumb.h"
#include "headerview.h"
#include "icons.h"
#include "matchdelegate.h"
#include "rowdelegate.h"
#include "searchoverlay.h"

#include <QTimer>

#include <QApplication>
#include "jtfstring.h"
#include "platform/quicklook.h"

#include <QDragMoveEvent>
#include <QDropEvent>
#include <QElapsedTimer>
#include <QMimeData>
#include <QUrl>
#include <QDesktopServices>
#include <QItemSelectionModel>
#include <QMenu>
#include <QFontMetrics>
#include <QEvent>
#include <QHeaderView>
#include <QKeyEvent>
#include <QMouseEvent>
#include <QLabel>
#include <QLineEdit>
#include <QHBoxLayout>
#include <QDrag>
#include <QListView>
#include <QStyle>
#include <QTabBar>
#include <QToolButton>
#include <QTableView>
#include <QVBoxLayout>

namespace {
// The payload a tab drag carries. Ours alone, so a drop from anywhere else is
// not mistaken for a tab. Declared here rather than beside the drag code
// because a tab can now be dropped on the pane as well as on its tab strip,
// and those two handlers sit at opposite ends of the file.
constexpr const char *kTabMimeType = "application/x-jt-filework-tab";

// The close mark on a tab, and the button around it. Both were smaller: at
// 11px, dimmed, and pressed against the tab's right border, the mark was
// something you had to already know was there.
// The pane's own edge. The same on an active and an inactive pane, so that
// taking the focus does not move the contents by two pixels; only the colour
// changes.
constexpr int kPaneBorder = 2;

// The close mark on a tab. Three things had to be right before it could be
// seen at all:
//
//   - the glyph: `xmark.svg` was a copy of `xmark-circle.svg`, whose cross
//     fills a quarter of its box;
//   - the size asked for: 13 is not one of the sizes renderIcon draws, so Qt
//     scaled the 16px pixmap down and thinned the stroke with it;
//   - the room left to draw in: the stylesheet's margins and padding came out
//     of the button's own box, so a 24px button had 7px of drawing area.
//
// So the gap to the tab title is now part of the widget's width rather than a
// margin inside it, and the box keeps its whole size for the mark.
constexpr int kTabCloseIcon = 20;
constexpr int kTabCloseBox = 22;
// Kept the same as JtfTabClose's margin-left in theme.cpp.
constexpr int kTabCloseGap = 6;
} // namespace


PaneWidget::~PaneWidget() { delete m_typeAheadClock; }

PaneWidget::PaneWidget(JtfApp *app, int paneId, QWidget *parent)
    : QWidget(parent), m_app(app), m_pane(paneId) {
    setObjectName(QStringLiteral("JtfPane"));
    // Without this a plain QWidget subclass ignores the stylesheet's
    // background and border entirely - Qt only paints them for a widget that
    // asks. The pane's border has been in the stylesheet since the beginning
    // and has never once been drawn, which is why the active pane looked no
    // different from the other one.
    setAttribute(Qt::WA_StyledBackground, true);
    auto *layout = new QVBoxLayout(this);
    // Room for that border. The children fill the pane, so with no margin they
    // paint straight over the edge the stylesheet just drew.
    layout->setContentsMargins(kPaneBorder, kPaneBorder, kPaneBorder, kPaneBorder);
    layout->setSpacing(0);

    m_tabs = new QTabBar(this);
    m_tabs->setExpanding(false);
    m_tabs->setTabsClosable(false); // we install our own, see syncTabs
    m_tabs->setMovable(true);
    m_tabs->setDrawBase(false);
    m_tabs->setElideMode(Qt::ElideMiddle);

    // The tab bar and its "+" share a row: the button sits immediately after
    // the last tab, where a browser puts it and where the eye already is
    // after reading the tabs. The bar does not expand, so the button stays
    // beside the tabs rather than drifting to the far edge.
    auto *tabRow = new QWidget(this);
    tabRow->setObjectName(QStringLiteral("JtfTabRow"));
    auto *tabRowLayout = new QHBoxLayout(tabRow);
    tabRowLayout->setContentsMargins(0, 0, 0, 0);
    tabRowLayout->setSpacing(0);
    tabRowLayout->addWidget(m_tabs);
    m_newTab = new QToolButton(tabRow);
    m_newTab->setObjectName(QStringLiteral("JtfNewTab"));
    m_newTab->setAutoRaise(true);
    m_newTab->setText(QStringLiteral("+"));
    m_newTab->setFocusPolicy(Qt::NoFocus);
    connect(m_newTab, &QToolButton::clicked, this, [this] {
        jtf_focus_pane(m_app, m_pane);
        jtf_new_tab(m_app);
        emit stateChanged();
    });
    tabRowLayout->addWidget(m_newTab);
    tabRowLayout->addStretch(1);
    // Which pane a copy or a move would land in. Pressing C with two panes
    // open is a decision about a folder full of files, and "the one that is
    // not focused" is a rule the user has to remember and apply under a
    // highlight that is easy to misread. This says it instead.
    // The badge that says a copy or a move lands here.
    //
    // Always in the layout, never hidden - only emptied. Hiding it took its
    // width out of the tab row, which changed the pane's size hint, which made
    // the splitter redistribute every pane in the window: dragging a file
    // across three panes made all of them jump about as the target followed
    // the pointer. Its width is reserved once, from the longest text it can
    // hold, and switching the target after that moves nothing.
    // The badge that says a copy or a move lands here: a small arrow and one
    // word, rather than the sentence it used to be.
    //
    // Always in the layout, never hidden - only emptied. Hiding it took its
    // width out of the tab row, which changed the pane's size hint, which made
    // the splitter redistribute every pane in the window: dragging a file
    // across three panes made all of them jump about as the target followed
    // the pointer. Its width is reserved once and switching the target after
    // that moves nothing.
    m_targetBadge = new QWidget(tabRow);
    m_targetBadge->setObjectName(QStringLiteral("JtfTargetBadge"));
    auto *badgeRow = new QHBoxLayout(m_targetBadge);
    badgeRow->setContentsMargins(8, 0, 8, 0);
    badgeRow->setSpacing(4);
    m_targetIcon = new QLabel(m_targetBadge);
    m_targetIcon->setObjectName(QStringLiteral("JtfTargetBadgeIcon"));
    m_targetWord = new QLabel(m_targetBadge);
    m_targetWord->setObjectName(QStringLiteral("JtfTargetBadgeWord"));
    m_targetBadge->setProperty("jtfShowing", false);
    badgeRow->addWidget(m_targetIcon);
    badgeRow->addWidget(m_targetWord);
    tabRowLayout->addWidget(m_targetBadge);

    // Closing a pane from the menu closes whichever one has focus, so getting
    // rid of the pane you are looking at means focusing it first. The button
    // says which pane it closes by sitting in it. It focuses that pane before
    // closing, so the click and the menu item run the same command on the
    // same pane rather than two paths that can disagree.
    m_close = new QToolButton(tabRow);
    m_close->setObjectName(QStringLiteral("JtfClosePane"));
    m_close->setAutoRaise(true);
    m_close->setFocusPolicy(Qt::NoFocus);
    connect(m_close, &QToolButton::clicked, this, [this] {
        jtf_focus_pane(m_app, m_pane);
        jtf_close_active_pane(m_app);
        emit stateChanged();
    });
    tabRowLayout->addWidget(m_close);
    layout->addWidget(tabRow);

    m_crumbs = new Breadcrumb(this);
    // Completions come from the same call the folder tree lists with, so what
    // completes here and what the tree shows are the same set - and a path on
    // a server completes, which Qt's own file-system completer could not do.
    m_crumbs->setCompletionSource([this](const QString &folder) {
        const QByteArray utf8 = folder.toUtf8();
        const QString joined = jtfText([&](char *buf, int len) {
            return jtf_child_directories(m_app, utf8.constData(), buf, len);
        });
        return joined.isEmpty() ? QStringList()
                                : joined.split(QLatin1Char('\n'), Qt::SkipEmptyParts);
    });
    connect(m_crumbs, &Breadcrumb::navigate, this, [this](const QString &path) {
        const QByteArray utf8 = path.toUtf8();
        jtf_navigate(m_app, m_pane, utf8.constData());
        emit stateChanged();
    });
    connect(m_crumbs, &Breadcrumb::segmentMenuRequested, this,
            [this](const QString &path, const QPoint &global) {
                jtf_focus_pane(m_app, m_pane);
                emit crumbMenuRequested(path, global);
            });
    layout->addWidget(m_crumbs);

    // Filtering narrows what is already listed. It is instant because it
    // touches no disk, which is what separates it from search
    // (docs/SEARCH_AI.md 1) and why it belongs in the pane rather than in a
    // dialog.
    // The filter is a bar, not a bare text box. A lone line edit appearing
    // above the list says nothing about what it does, looks identical to the
    // search box, and gives no sign that it is narrowing what you can see -
    // which matters, because a filter hides files.
    m_filterBar = new QWidget(this);
    m_filterBar->setObjectName(QStringLiteral("JtfFilterBar"));
    m_filterBar->setVisible(jtf_filter_bar_always(m_app) != 0);
    auto *filterRow = new QHBoxLayout(m_filterBar);
    filterRow->setContentsMargins(8, 4, 6, 4);
    filterRow->setSpacing(6);
    m_filterIcon = new QLabel(m_filterBar);
    m_filterIcon->setObjectName(QStringLiteral("JtfFilterIcon"));
    filterRow->addWidget(m_filterIcon);
    // Set with the theme, alongside every other tinted glyph.

    m_filter = new QLineEdit(m_filterBar);
    m_filter->setObjectName(QStringLiteral("JtfFilter"));
    m_filter->setFrame(false);
    m_filter->installEventFilter(this);
    filterRow->addWidget(m_filter, 1);

    // How much is hidden, live. A filter that silently removes rows is how
    // someone concludes a file is missing.
    m_filterCount = new QLabel(m_filterBar);
    m_filterCount->setObjectName(QStringLiteral("JtfFilterCount"));
    filterRow->addWidget(m_filterCount);

    m_filterClose = new QToolButton(m_filterBar);
    m_filterClose->setObjectName(QStringLiteral("JtfFilterClose"));
    m_filterClose->setAutoRaise(true);
    m_filterClose->setFocusPolicy(Qt::NoFocus);
    connect(m_filterClose, &QToolButton::clicked, this, [this] { clearFilter(); });
    filterRow->addWidget(m_filterClose);
    connect(m_filter, &QLineEdit::textChanged, this, [this](const QString &text) {
        const QByteArray utf8 = text.toUtf8();
        jtf_set_filter(m_app, m_pane, utf8.constData());
        m_model->refresh();
        retranslate();
    });
    layout->addWidget(m_filterBar);

    // Search walks a tree, so it is a separate box from the filter and says
    // so: conflating them would make one of the two feel wrong
    // (docs/SEARCH_AI.md 1).

    m_view = new QTableView(this);
    m_model = new FileListModel(app, paneId, this);
    m_view->setModel(m_model);
    m_view->setSelectionBehavior(QAbstractItemView::SelectRows);
    // The columns are fitted to the width, so there is nothing to scroll to
    // sideways - and a bar that appears on hover would change the list's
    // height as the pointer crossed the pane's edge.
    m_view->setHorizontalScrollBarPolicy(Qt::ScrollBarAlwaysOff);
    m_view->setSelectionMode(QAbstractItemView::ExtendedSelection);
    m_view->setAlternatingRowColors(true);
    m_view->setMouseTracking(true); // so :hover in the stylesheet applies
    m_view->setShowGrid(false);
    m_view->setSortingEnabled(false); // sorting is the model's, not Qt's
    m_view->verticalHeader()->setVisible(false);
    m_view->verticalHeader()->setDefaultSectionSize(22);
    m_view->setIconSize(QSize(16, 16));
    // Uniform row heights is what lets Qt virtualize properly; without it the
    // view measures every row and the cost becomes O(directory size).
    m_view->horizontalHeader()->setSectionsClickable(true);
    // Sorting is done in the model, so the header has to be told what it is
    // showing: without this the arrow never appears and clicking a header
    // looks like it did nothing.
    m_header = new JtfHeaderView(m_view);
    m_view->setHorizontalHeader(m_header);
    // A box at the head of the column of boxes, meaning all of them. Selection
    // is the mark (`AGENTS.md` §10), so it does both: ticking it selects every
    // row, clearing it leaves nothing selected.
    m_header->setMarkAllVisible(true);
    // A mark made by clicking a row's own box has to move the header's box
    // too. `markChanged` was emitted and nobody was listening.
    connect(m_model, &FileListModel::markChanged, this, [this] {
        // And the selection follows, because selection *is* the mark
        // (`AGENTS.md` §10). Ticking a box marked the row in the model and
        // left it out of the view's selection, so a row marked by its box was
        // drawn one way and a row marked by being selected another - two
        // appearances for one state, in the same column, three rows apart.
        syncSelectionFromMarks();
        syncMarkAll();
        emit stateChanged();
    });
    connect(m_header, &JtfHeaderView::markAllToggled, this, [this](bool wanted) {
        // 0 marks every listed entry, 1 clears them - the same two actions the
        // Edit menu offers, so there is one implementation of "all".
        jtf_mark_listed(m_app, m_pane, wanted ? 0 : 1);
        // refreshRows puts the highlight back from the marks, so the rows go
        // dark with their boxes. Clearing the header box used to leave every
        // row still lit, which said the rows were selected while their boxes
        // said they were not - the two halves of one thing disagreeing.
        refreshRows();
        emit stateChanged();
    });
    // Only the name column: highlighting a date because the query happens to
    // contain a digit would be noise, not information.
    m_matches = new MatchDelegate(this);
    // Every other column gets the plain row-aware delegate, so hover covers
    // the row rather than stopping at the name column's edge.
    connect(m_model, &QAbstractItemModel::modelReset, this,
            [this] { scheduleFitNameColumn(); });
    connect(m_model, &QAbstractItemModel::columnsInserted, this,
            [this] { scheduleFitNameColumn(); });
    m_rows = new RowDelegate(this);
    m_view->setItemDelegate(m_rows);
    m_view->setItemDelegateForColumn(0, m_matches);
    // Both delegates have to agree on which row is hovered, or the row lights
    // up in pieces.
    m_searchOverlay = new SearchOverlay(this);
    m_searchOverlay->setVisible(false);
    connect(m_searchOverlay, &SearchOverlay::cancelled, this, [this] {
        clearSearch();
        emit stateChanged();
    });

    connect(m_view, &QAbstractItemView::entered, this, [this](const QModelIndex &index) {
        setHoveredRow(index.isValid() ? index.row() : -1);
    });
    // The name column takes whatever the others do not, so the list fills
    // the pane instead of ending in dead space, and widening a pane widens
    // the one column where the extra room is worth anything. The rest stay
    // draggable: a fixed date column is a date column you cannot widen when
    // the locale writes longer dates.
    m_view->horizontalHeader()->setStretchLastSection(false);
    m_view->horizontalHeader()->setSectionResizeMode(QHeaderView::Interactive);
    m_view->setHorizontalScrollMode(QAbstractItemView::ScrollPerPixel);
    m_view->setVerticalScrollMode(QAbstractItemView::ScrollPerPixel);
    m_view->setEditTriggers(QAbstractItemView::NoEditTriggers);
    // Dragging out reaches Finder; dropping in accepts from another pane or
    // from Finder, because both speak text/uri-list.
    m_view->setDragEnabled(true);
    m_view->setAcceptDrops(true);
    m_view->setDropIndicatorShown(true);
    m_view->setDragDropMode(QAbstractItemView::DragDrop);
    m_view->setDefaultDropAction(Qt::MoveAction);
    layout->addWidget(m_view, 1);

    // The status line, and beside it the one thing worth doing about a
    // failure. A pane that could not open says so and then leaves you with
    // nothing to press; the folder is unreachable, so the usual refresh key is
    // not an obvious answer and does not drop the dead session anyway.
    auto *statusRow = new QWidget(this);
    statusRow->setObjectName(QStringLiteral("JtfStatusRow"));
    auto *statusLayout = new QHBoxLayout(statusRow);
    statusLayout->setContentsMargins(0, 0, 0, 0);
    statusLayout->setSpacing(0);
    m_status = new QLabel(statusRow);
    m_status->setObjectName(QStringLiteral("JtfStatus"));
    // The line says how much is marked, and that text gets longer as more is
    // marked - so its width was reaching the splitter and the pane changed
    // size as the selection changed. Marking one more file is not a request to
    // rearrange the window.
    //
    // `Ignored` says the size hint is not a request; the text is elided in the
    // middle so what is lost is the middle of a figure rather than the start
    // of the sentence.
    m_status->setSizePolicy(QSizePolicy::Ignored, QSizePolicy::Fixed);
    m_status->setMinimumWidth(0);
    statusLayout->addWidget(m_status, 1);
    m_reconnect = new QToolButton(statusRow);
    m_reconnect->setObjectName(QStringLiteral("JtfReconnect"));
    m_reconnect->setAutoRaise(true);
    m_reconnect->setFocusPolicy(Qt::NoFocus);
    m_reconnect->setToolButtonStyle(Qt::ToolButtonTextBesideIcon);
    m_reconnect->setVisible(false);
    connect(m_reconnect, &QToolButton::clicked, this,
            [this] { emit reconnectRequested(m_pane); });
    statusLayout->addWidget(m_reconnect);
    layout->addWidget(statusRow);

    m_typeAheadClock = new QElapsedTimer();
    m_typeAheadClock->start();

    m_view->setContextMenuPolicy(Qt::CustomContextMenu);
    connect(m_view, &QTableView::customContextMenuRequested, this,
            &PaneWidget::showContextMenu);
    m_view->horizontalHeader()->setContextMenuPolicy(Qt::CustomContextMenu);
    connect(m_view->horizontalHeader(), &QHeaderView::customContextMenuRequested, this,
            &PaneWidget::showHeaderMenu);

    // The grid is a second view onto the *same* model and the same selection,
    // so switching between them keeps the cursor, the marks and the sort. Two
    // models would be two answers to "what is in this folder".
    m_grid = new QListView(this);
    m_grid->setObjectName(QStringLiteral("JtfGrid"));
    // The model is attached only while the grid is showing. A hidden view
    // still receives every model reset and still lays out every item, so in a
    // directory of a hundred thousand the grid was doing a full layout on
    // each listing that nobody would ever see.
    m_grid->setViewMode(QListView::IconMode);
    m_grid->setResizeMode(QListView::Adjust);
    m_grid->setMovement(QListView::Static);
    m_grid->setUniformItemSizes(true);
    m_grid->setWordWrap(true);
    m_grid->setSelectionBehavior(QAbstractItemView::SelectRows);
    m_grid->setSelectionMode(QAbstractItemView::ExtendedSelection);
    m_grid->setEditTriggers(QAbstractItemView::NoEditTriggers);
    m_grid->setDragEnabled(true);
    m_grid->setAcceptDrops(true);
    m_grid->setDropIndicatorShown(true);
    m_grid->setContextMenuPolicy(Qt::CustomContextMenu);
    m_grid->setVisible(false);
    m_grid->installEventFilter(this);
    m_grid->viewport()->installEventFilter(this);
    connect(m_grid, &QAbstractItemView::doubleClicked, this, [this](const QModelIndex &index) {
        // As in the list view: one route for opening, so the archive window
        // and the platform hand-off are decided in one place.
        setCurrentRow(index.row(), QAbstractItemView::EnsureVisible);
        emit commandRequested(QStringLiteral("file.open"));
    });
    layout->addWidget(m_grid, 1);
    connect(m_grid, &QWidget::customContextMenuRequested, this, [this](const QPoint &at) {
        jtf_focus_pane(m_app, m_pane);
        emit contextMenuRequested(m_grid->viewport()->mapToGlobal(at),
                                  m_grid->indexAt(at).isValid());
    });

    m_view->installEventFilter(this);
    m_view->horizontalHeader()->installEventFilter(this);
    m_view->viewport()->installEventFilter(this);
    m_tabs->installEventFilter(this);

    // The widget owns the visual selection; the model owns what an operation
    // will act on. Keeping them in step here is what lets Rust resolve
    // marked-then-selection-then-active without C++ deciding anything.
    connect(m_view->selectionModel(), &QItemSelectionModel::selectionChanged, this, [this] {
        QVector<int> rows;
        const auto indexes = m_view->selectionModel()->selectedRows();
        rows.reserve(indexes.size());
        for (const QModelIndex &index : indexes) {
            rows.append(index.row());
        }
        jtf_set_selection(m_app, m_pane, rows.constData(), static_cast<int>(rows.size()));
        // Selecting is marking. What is highlighted is what is ticked,
        // however the rows were picked - mouse, Shift and the arrows, or
        // Space. `AGENTS.md` §10 used to keep the two apart; the project
        // owner decided they should be one, and the rule was changed with it.
        //
        // Guarded because restoring the selection from the marks on arriving
        // in a folder would otherwise come straight back round here.
        if (!m_restoringMarks) {
            jtf_set_marks_from_selection(m_app, m_pane, rows.constData(),
                                         static_cast<int>(rows.size()));
            syncMarkAll();
        }
        emit selectionChanged();
    });

    // And which row the cursor itself is on, which is a different question
    // from what is selected: `Tab::active_entry` is what answers "what am I
    // pointing at" for Enter on an archive, `Z` on a folder, and the last
    // fallback of `operation_target`.
    connect(m_view->selectionModel(), &QItemSelectionModel::currentRowChanged, this,
            [this](const QModelIndex &current, const QModelIndex &previous) {
                jtf_set_current_row(m_app, m_pane, current.isValid() ? current.row() : -1);
                // Repaint the whole of both rows, not the cell Qt thinks
                // changed.
                //
                // The cursor outline is drawn a segment per cell: each cell
                // draws the top and bottom edges of its own stretch, and the
                // first and last add the sides. Qt repaints the current
                // *index*, which is column zero - so moving the cursor left
                // the other columns of the new row without their segments and
                // the other columns of the old row still carrying theirs. The
                // outline came apart, in both directions at once.
                repaintRow(previous);
                repaintRow(current);
            });

    connect(m_view, &QTableView::doubleClicked, this, [this](const QModelIndex &index) {
        // The same route as Enter, so a double click and the key cannot mean
        // two different things.
        setCurrentRow(index.row(), QAbstractItemView::EnsureVisible);
        emit commandRequested(QStringLiteral("file.open"));
    });

    connect(m_view->horizontalHeader(), &QHeaderView::sectionResized, this,
            [this](int column, int, int width) {
                // Only while the pointer is actually holding the divider.
                //
                // `sectionResized` fires for every width this widget sets
                // itself as well - the auto-measure, the squeeze when there is
                // not room, the redistribution when the view changes size -
                // and a guard against `fitNameColumn` alone was not enough: one
                // drag recorded three columns, because the others had been set
                // from elsewhere in the same breath. Recording those would
                // freeze every column at whatever the first folder happened to
                // need, and there is no way for the user to unfreeze one.
                //
                // A press on the header and its release bracket a real drag,
                // and nothing else.
                if (!m_userResizing || column == 0 || width <= 0) {
                    return;
                }
                jtf_set_column_width(m_app, column, width);
                // Written now, not on exit: a width that survives only a clean
                // quit has not been remembered. `stateChanged` is not used
                // here - it re-lists every pane, which is a great deal of work
                // for dragging an edge, and it does not save.
                jtf_app_save_session(m_app);
            });

    connect(m_view->horizontalHeader(), &QHeaderView::sectionClicked, this,
            [this](int section) {
                jtf_sort_by(m_app, m_pane, section);
                m_model->refresh();
                syncSortIndicator();
                emit stateChanged();
            });

    connect(m_tabs, &QTabBar::currentChanged, this, [this](int index) {
        if (index >= 0 && index != jtf_active_tab(m_app, m_pane)) {
            jtf_activate_tab(m_app, m_pane, index);
            emit stateChanged();
        }
    });

    // Dragging a tab far enough off the strip tears it into its own window,
    // as a browser does. Qt's own tab dragging reorders within the strip, so
    // this watches for the pointer leaving the strip's neighbourhood and
    // takes over from there.
    m_tabs->installEventFilter(this);

    // The same operation is on the tab's context menu, so it stays reachable
    // without the gesture - and reachable from the keyboard later.
    // A tab dropped on another pane's strip moves there, which is the other
    // half of tearing one off. The payload names the source pane and tab, so
    // a drop from any other application is simply not ours and is ignored.
    m_tabs->setAcceptDrops(true);
    m_tabs->setContextMenuPolicy(Qt::CustomContextMenu);
    connect(m_tabs, &QWidget::customContextMenuRequested, this, [this](const QPoint &at) {
        const int index = m_tabs->tabAt(at);
        if (index < 0) {
            return;
        }
        // Acting on a pane makes it the active one - the `+` button, the list's
        // own context menu and the breadcrumb all do this, and this menu was
        // the one that did not. Without it, 新增分頁 opened the tab in whichever
        // pane was already active rather than the one being right-clicked.
        jtf_focus_pane(m_app, m_pane);
        const auto label = [this](const char *key) {
            return jtfText([&](char *b, int l) { return jtf_tr(m_app, key, b, l); });
        };
        const QColor iconColour = palette().color(QPalette::Text);

        QMenu menu(this);
        QAction *newTab = menu.addAction(glyph::forCommand(QStringLiteral("tab.new"), iconColour),
                                         label("command.tab.new"));
        QAction *duplicate = menu.addAction(glyph::make(glyph::Shape::Copy, iconColour),
                                            label("tab.duplicate"));
        const bool pinned = jtf_tab_is_pinned(m_app, m_pane, index) != 0;
        QAction *pin = menu.addAction(glyph::make(glyph::Shape::Bookmark, iconColour),
                                      label(pinned ? "tab.unpin" : "command.tab.pin"));
        // The tab's own folder, which is not always the pane's: this menu
        // answers a right-click on one particular tab.
        const QString tabPath = jtfText(
            [&](char *b, int l) { return jtf_tab_path(m_app, m_pane, index, b, l); });
        const QByteArray tabUtf8 = tabPath.toUtf8();
        QAction *bookmark = nullptr;
        if (!tabPath.isEmpty()) {
            const bool marked = jtf_path_is_bookmarked(m_app, tabUtf8.constData()) != 0;
            bookmark = menu.addAction(glyph::forCommand(QStringLiteral("file.bookmark"),
                                                        iconColour),
                                      label(marked ? "crumb.unbookmark" : "crumb.bookmark"));
        }
        menu.addSeparator();
        QAction *close = menu.addAction(glyph::make(glyph::Shape::Close, iconColour),
                                        label("command.tab.close"));
        QAction *closeOthers = menu.addAction(glyph::make(glyph::Shape::Close, iconColour),
                                              label("tab.close_others"));
        QAction *closeLeft = menu.addAction(glyph::make(glyph::Shape::ArrowLeft, iconColour),
                                            label("tab.close_to_left"));
        QAction *closeRight = menu.addAction(glyph::make(glyph::Shape::ArrowRight, iconColour),
                                             label("tab.close_to_right"));
        // Nothing to close when this is the only tab, nothing to the right of
        // the last one and nothing to the left of the first. A menu that
        // offers an action which does nothing teaches the user to distrust
        // the menu.
        closeOthers->setEnabled(m_tabs->count() > 1);
        closeLeft->setEnabled(index > 0);
        closeRight->setEnabled(index < m_tabs->count() - 1);
        menu.addSeparator();
        QAction *tearOff = menu.addAction(glyph::make(glyph::Shape::NewWindow, iconColour),
                                          label("tab.tear_off"));
        // Only offered when it would do something: the last tab of the last
        // pane cannot become its own window.
        tearOff->setEnabled(m_tabs->count() > 1 || jtf_pane_count(m_app) > 1);

        QAction *chosen = menu.exec(m_tabs->mapToGlobal(at));
        if (chosen == tearOff) {
            emit tearOffRequested(index);
        } else if (chosen == newTab) {
            jtf_new_tab(m_app);
            emit stateChanged();
        } else if (chosen == duplicate) {
            // A second tab on the same folder: the usual reason for one is to
            // keep this place while going somewhere else from it.
            //
            // The tab that was right-clicked, in this pane - not whichever tab
            // and pane happen to be active. This used to read a path out of
            // the active tab and navigate a new tab to it, which duplicated
            // the wrong tab whenever the menu was opened on another one, and
            // duplicated nothing at all on a server, whose location has no
            // local path to read.
            jtf_duplicate_tab(m_app, m_pane, index);
            emit stateChanged();
        } else if (bookmark != nullptr && chosen == bookmark) {
            jtf_toggle_bookmark_path(m_app, tabUtf8.constData());
            emit stateChanged();
        } else if (chosen == pin) {
            jtf_toggle_tab_pinned(m_app, m_pane, index);
            emit stateChanged();
        } else if (chosen == close) {
            closeTab(index);
        } else if (chosen == closeOthers) {
            // Backwards, so closing one does not renumber the ones still to
            // go; and the kept tab's own index moves as those before it go, so
            // it is found by title-independent arithmetic rather than assumed.
            for (int at2 = m_tabs->count() - 1; at2 >= 0; --at2) {
                if (at2 != index) {
                    jtf_close_tab(m_app, m_pane, at2);
                }
            }
            emit stateChanged();
        } else if (chosen == closeRight) {
            for (int at2 = m_tabs->count() - 1; at2 > index; --at2) {
                jtf_close_tab(m_app, m_pane, at2);
            }
            emit stateChanged();
        } else if (chosen == closeLeft) {
            // Backwards again, so the indices of the ones still to close do
            // not shift under the loop.
            for (int at2 = index - 1; at2 >= 0; --at2) {
                jtf_close_tab(m_app, m_pane, at2);
            }
            emit stateChanged();
        }
    });
    connect(m_tabs, &QTabBar::tabCloseRequested, this,
            [this](int index) { closeTab(index); });

    syncTabs();
    syncPath();
    syncSortIndicator();
    applyColumnVisibility();
    m_view->setColumnWidth(0, 330);
    m_view->setColumnWidth(1, 92);
    m_view->setColumnWidth(2, 200);
    m_view->setColumnWidth(3, 160);
    // Not QHeaderView::Stretch. Stretch makes the name column absorb the
    // slack in both directions, so shrinking the window crushes the one
    // column that matters until the file names are gone while Permissions and
    // Owner sit there at full width. The name column takes the *surplus* and
    // gives it back last, down to a floor - the others are what should be cut
    // off, because you can widen the window to see a date and you cannot work
    // at all without names.
    m_view->horizontalHeader()->setSectionResizeMode(QHeaderView::Interactive);
    m_view->horizontalHeader()->setMinimumSectionSize(48);
    fitNameColumn();
    m_view->horizontalHeader()->setHighlightSections(false);
    m_view->horizontalHeader()->setDefaultAlignment(Qt::AlignLeft | Qt::AlignVCenter);
}

QString PaneWidget::chordFor(const QKeyEvent *key) {
    // The keymap's spelling, so C++ never invents a second name for a key.
    // Only the keys a file list can legitimately claim are named here: a
    // chord this returns empty for falls through to Qt.
    static const QHash<int, QString> named = {
        {Qt::Key_Left, QStringLiteral("left")},     {Qt::Key_Right, QStringLiteral("right")},
        {Qt::Key_Up, QStringLiteral("up")},         {Qt::Key_Down, QStringLiteral("down")},
        {Qt::Key_Home, QStringLiteral("home")},     {Qt::Key_End, QStringLiteral("end")},
        {Qt::Key_PageUp, QStringLiteral("pageup")}, {Qt::Key_PageDown, QStringLiteral("pagedown")},
        {Qt::Key_Insert, QStringLiteral("insert")}, {Qt::Key_Delete, QStringLiteral("delete")},
    };

    QString name = named.value(key->key());
    if (name.isEmpty()) {
        // From the key code, not from the text. On macOS Option is a text
        // modifier: Option+T types a dagger, so reading text() here made the
        // chord `alt+†`, which matches nothing and left Alt-T - CView's mark
        // all - silently dead.
        const int code = key->key();
        if (code >= Qt::Key_A && code <= Qt::Key_Z) {
            name = QChar(QLatin1Char('a' + (code - Qt::Key_A)));
        } else if (code >= Qt::Key_0 && code <= Qt::Key_9) {
            name = QChar(QLatin1Char('0' + (code - Qt::Key_0)));
        } else {
            const QString text = key->text();
            if (text.size() != 1 || !text.at(0).isPrint()) {
                return {};
            }
            name = text.toLower();
        }
    }

    QStringList parts;
    const Qt::KeyboardModifiers mods = key->modifiers();
    if (mods.testFlag(Qt::ControlModifier)) {
        parts << QStringLiteral("primary");
    }
    if (mods.testFlag(Qt::AltModifier)) {
        parts << QStringLiteral("alt");
    }
    if (mods.testFlag(Qt::ShiftModifier)) {
        parts << QStringLiteral("shift");
    }
    parts << name;
    return parts.join(QLatin1Char('+'));
}

void PaneWidget::openRow(int row) {
    // A folder is entered; anything else is handed to the system's default
    // application. Double-clicking a file and having nothing happen is the
    // single most obvious thing a file manager can get wrong.
    if (jtf_open_row(m_app, m_pane, row)) {
        emit stateChanged();
        return;
    }
    const QString path = jtfText(
        [&](char *buf, int len) { return jtf_row_path(m_app, m_pane, row, buf, len); });
    if (!path.isEmpty()) {
        // QDesktopServices asks the platform which application owns the type.
        // It is an API call, not a shell command line (AGENTS.md 20.3).
        QDesktopServices::openUrl(QUrl::fromLocalFile(path));
    }
}

void PaneWidget::searchFor(const QString &query) {
    if (query.isEmpty()) {
        clearSearch();
        return;
    }
    const QByteArray utf8 = query.toUtf8();
    char error[256] = {0};
    jtf_search_start(m_app, m_pane, utf8.constData(), error, sizeof(error));
    emit stateChanged();
}

void PaneWidget::editPath() { m_crumbs->beginEditing(); }


void PaneWidget::clearSearch() {
    // Clearing returns to the folder the pane was already on, rather than
    // navigating anywhere: a search never moved you.
    if (jtf_is_searching(m_app, m_pane)) {
        jtf_search_clear(m_app, m_pane);
        m_view->setFocus();
        emit stateChanged();
        return;
    }
    // A filter narrows the list too, and Escape means "stop narrowing it"
    // whichever of the two is in force. Bound as `search.clear`, but the key
    // is Escape and pressing it over a filtered list did nothing at all -
    // which reads as a broken key, not as a distinction between two features.
    if (m_filterBar->isVisible()) {
        clearFilter();
        return;
    }
    m_view->setFocus();
    emit stateChanged();
}

void PaneWidget::toggleFilter() {
    if (m_filterBar->isVisible() && m_filter->hasFocus()) {
        clearFilter();
        return;
    }
    m_filterBar->setVisible(true);
    m_filter->setPlaceholderText(jtfText(
        [&](char *buf, int len) { return jtf_tr(m_app, "filter.placeholder", buf, len); }));
    m_filter->setFocus();
    m_filter->selectAll();
}

void PaneWidget::syncFilterBar() {
    // A filter is saved with the tab and restored with it, but nothing used to
    // put it back on screen - so a folder came up filtered with no filter box,
    // no text and no hint that anything was being hidden. `~/Downloads` showed
    // 163 zip files and not one of its 92 folders, and there was no way to
    // find out why, let alone undo it.
    //
    // Whatever the core is actually filtering by is what the box shows.
    const QString active =
        jtfText([&](char *b, int l) { return jtf_filter(m_app, m_pane, b, l); });
    if (active.isEmpty()) {
        return; // Never hides the bar: the user may have just opened an empty one.
    }
    if (m_filter->text() != active) {
        QSignalBlocker blocker(m_filter);
        m_filter->setText(active);
    }
    if (!m_filterBar->isVisible()) {
        m_filter->setPlaceholderText(jtfText(
            [&](char *buf, int len) { return jtf_tr(m_app, "filter.placeholder", buf, len); }));
        m_filterBar->setVisible(true);
    }
}

void PaneWidget::clearFilter() {
    // Escape clears and hides, rather than leaving an empty box that still
    // looks like a mode the user is in - unless the box has been asked to
    // stay, in which case hiding it is the program overruling a setting.
    m_filter->clear();
    m_filterBar->setVisible(jtf_filter_bar_always(m_app) != 0);
    // And the highlight goes with it. The matched text was still picked out in
    // orange after the filter had been left, which says the list is still
    // narrowed by something when it is not - the one thing the highlight
    // exists to communicate, said wrongly.
    if (m_matches != nullptr) {
        m_matches->setNeedle(QString());
        m_view->viewport()->update();
        m_grid->viewport()->update();
    }
    m_view->setFocus();
}

void PaneWidget::openCurrentRow() {
    const int row = currentRow();
    if (row >= 0) {
        openRow(row);
    }
}

void PaneWidget::showContextMenu(const QPoint &position) {
    const QModelIndex index = m_view->indexAt(position);
    if (index.isValid() && !m_view->selectionModel()->isSelected(index)) {
        m_view->setCurrentIndex(index);
        m_view->selectionModel()->select(
            index, QItemSelectionModel::ClearAndSelect | QItemSelectionModel::Rows);
    }
    emit focusRequested(m_pane);
    emit contextMenuRequested(m_view->viewport()->mapToGlobal(position), index.isValid());
}

void PaneWidget::showHeaderMenu(const QPoint &position) {
    // Right-clicking a header to choose columns is a convention old enough
    // that its absence reads as a missing feature.
    QMenu menu(this);
    for (int column = 0; column < jtf_column_count(); ++column) {
        const QString key =
            jtfText([&](char *buf, int len) { return jtf_column_key(column, buf, len); });
        const QByteArray keyUtf8 = key.toUtf8();
        const QString label = jtfText(
            [&](char *buf, int len) { return jtf_tr(m_app, keyUtf8.constData(), buf, len); });

        QAction *action = menu.addAction(label);
        action->setCheckable(true);
        action->setChecked(jtf_column_visible(m_app, m_pane, column) != 0);
        // The name column cannot be hidden: a list of blank rows is not a
        // view of anything.
        action->setEnabled(column != 0);
        connect(action, &QAction::toggled, this, [this, column](bool on) {
            jtf_set_column_visible(m_app, m_pane, column, on ? 1 : 0);
            applyColumnVisibility();
            emit stateChanged();
        });
    }
    menu.exec(m_view->horizontalHeader()->mapToGlobal(position));
}

void PaneWidget::applyColumnVisibility() {
    // In search results the Path column is shown whatever the tab's own
    // setting says: two files called `notes.md` are indistinguishable without
    // it, and results come from everywhere below the folder. The tab's
    // setting is untouched, so leaving the search restores it.
    const QString query =
        jtfText([&](char *b, int l) { return jtf_search_query(m_app, m_pane, b, l); });
    const bool searching = jtf_is_searching(m_app, m_pane) || !query.isEmpty();

    // What to pick out: the search's terms, or the filter's, or nothing.
    // Both narrow the list by matching text, and in both the reader wants to
    // see which part matched.
    QString needle = query;
    if (needle.isEmpty() && m_filterBar->isVisible()) {
        needle = m_filter->text();
    }
    m_matches->setNeedle(needle);

    m_wantedColumns.clear();
    for (int column = 0; column < jtf_column_count(); ++column) {
        bool visible = jtf_column_visible(m_app, m_pane, column) != 0;
        if (searching && isPathColumn(column)) {
            visible = true;
        }
        if (visible) {
            m_wantedColumns.append(column);
        }
    }
    scheduleFitNameColumn();
}

bool PaneWidget::isPathColumn(int column) const {
    // By key, not by index: the column order is data and has changed once.
    const QString key =
        jtfText([&](char *buf, int len) { return jtf_column_key(column, buf, len); });
    return key == QLatin1String("column.path");
}

void PaneWidget::setHoveredRow(int row) {
    if (row == m_hoveredRow) {
        return;
    }
    const int previous = m_hoveredRow;
    m_hoveredRow = row;
    m_matches->setHoveredRow(row);
    m_rows->setHoveredRow(row);
    // Only the two rows that changed, so moving the pointer down a long list
    // does not repaint the whole viewport per pixel.
    const auto repaint = [this](int line) {
        if (line >= 0 && line < m_model->rowCount()) {
            m_view->viewport()->update(m_view->visualRect(m_model->index(line, 0)).adjusted(
                0, 0, m_view->viewport()->width(), 0));
        }
    };
    repaint(previous);
    repaint(row);
}

void PaneWidget::scheduleFitNameColumn() {
    // Coalesced onto the event loop. The width the name column should have
    // depends on three things that settle in no fixed order at startup - the
    // viewport's real width, which columns are visible, and whether the model
    // has its columns yet - and fitting from whichever one happened to arrive
    // last is what left the list either half-filled or spilling past the
    // right edge. Running once after they have all been applied is the only
    // ordering that is right regardless of which came first.
    if (m_fitScheduled) {
        return;
    }
    m_fitScheduled = true;
    QTimer::singleShot(0, this, [this] {
        m_fitScheduled = false;
        fitNameColumn();
    });
}

void PaneWidget::fitNameColumn() {
    // Whatever the other visible columns do not use, with a floor. The floor
    // matters more than any other column: a date you cannot fully read is an
    // inconvenience, a file name you cannot read at all is the list failing at
    // its one job.
    static constexpr int kNameFloor = 160;
    // Below this a column shows an ellipsis and nothing else, which is worse
    // than not showing it: it costs the width and gives back no fact.
    static constexpr int kUseful = 78;

    if (m_fittingName) {
        return;
    }
    const QSignalBlocker blockFit(m_view->horizontalHeader());
    m_fittingName = true;

    const int viewport = m_view->viewport()->width();
    if (viewport <= 0) {
        m_fittingName = false;
        return;
    }

    // Start from what the user asked for and drop from the right - the
    // columns are ordered by how often they are wanted, so the last is the
    // first that can go. In a three-way split there is not room for four
    // columns, and squeezing all of them into ellipses serves nobody.
    QList<int> shown = m_wantedColumns;
    if (shown.isEmpty()) {
        for (int column = 0; column < m_model->columnCount(); ++column) {
            shown.append(column);
        }
    }
    while (shown.size() > 1 && kNameFloor + (shown.size() - 1) * kUseful > viewport) {
        shown.removeLast();
    }

    for (int column = 0; column < m_model->columnCount(); ++column) {
        m_view->setColumnHidden(column, !shown.contains(column));
    }

    // Give every other column the width its contents actually need, before
    // working out what is left for the name. Without this the pass only ever
    // *shrank* them: a column that started narrow stayed narrow no matter how
    // wide the window grew, so 修改日期 sat at "2023-12-11 …" with empty space
    // to its right and the name column swallowing all of it.
    //
    // Sampling is bounded - a directory of a hundred thousand rows must not
    // cost a hundred thousand text measurements on every resize - and the cap
    // stops one absurd value from taking the row.
    static constexpr int kColumnPadding = 16;
    static constexpr int kColumnCeiling = 260;

    // Measured once per folder, not on every pass.
    //
    // Rows arrive in batches, and every batch re-runs this. Measuring the
    // contents each time meant the widths grew as rows landed, so the columns
    // visibly shifted and then settled a moment later. What a column needs is
    // a property of the folder, so it is worked out when the folder changes
    // and reused for the resizes in between.
    const QString here = m_shownPath;
    if (here != m_measuredFor) {
        m_measuredFor = here;
        m_view->horizontalHeader()->setResizeContentsPrecision(64);
        for (int column : shown) {
            if (column == 0) {
                continue;
            }
            // A width the user dragged this column to is an instruction, and
            // it outlives walking into another folder. Only a column nobody
            // has touched measures itself against what is in it.
            const int chosen = jtf_column_width(m_app, column);
            if (chosen > 0) {
                m_view->setColumnWidth(column, chosen);
                continue;
            }
            // `resizeColumnToContents` is the public way to ask; the width it
            // leaves behind is then read back and clamped.
            m_view->resizeColumnToContents(column);
            m_view->setColumnWidth(
                column,
                qBound(kUseful, m_view->columnWidth(column) + kColumnPadding, kColumnCeiling));
        }
    }
    int used = 0;
    for (int column : shown) {
        if (column != 0) {
            used += m_view->columnWidth(column);
        }
    }

    int nameWidth = viewport - used;
    if (nameWidth < kNameFloor) {
        // The survivors give way rather than the name, proportionally, and
        // never below what is useful.
        int squeezable = 0;
        for (int column : shown) {
            if (column != 0) {
                squeezable += qMax(0, m_view->columnWidth(column) - kUseful);
            }
        }
        const int wanted = qMin(kNameFloor - nameWidth, squeezable);
        if (wanted > 0 && squeezable > 0) {
            int reclaimed = 0;
            for (int column : shown) {
                if (column == 0) {
                    continue;
                }
                const int slack = qMax(0, m_view->columnWidth(column) - kUseful);
                // Rounded down per column, so the total taken never exceeds
                // what was asked for; the remainder stays with the columns,
                // which is the harmless direction to be wrong in.
                const int take =
                    static_cast<int>(static_cast<qint64>(slack) * wanted / squeezable);
                m_view->setColumnWidth(column, m_view->columnWidth(column) - take);
                reclaimed += take;
            }
            nameWidth += reclaimed;
        }
    }
    m_view->setColumnWidth(0, qMax(nameWidth, m_view->horizontalHeader()->minimumSectionSize()));
    m_fittingName = false;
}

namespace {

// How far below the tab strip the pointer must go before a drag means "tear
// this out" rather than "reorder these". Generous, because tearing off by
// accident loses your place.
constexpr int kTearOffDistance = 28;

// The icon edge in the grid. Large enough for a photograph to be recognised,
// small enough that a folder of a thousand files is still navigable.
constexpr int kGridIconEdge = 72;


} // namespace

void PaneWidget::toggleCurrentInSelection() {
    // Through the selection, because the selection is what the tick shows.
    // `Toggle | Rows` adds the row if it is out and removes it if it is in,
    // which is exactly what Space meant when it worked on a separate mark set.
    const int row = currentRow();
    if (row < 0) {
        return;
    }
    const int columns = m_model->columnCount();
    const QItemSelection range(m_model->index(row, 0), m_model->index(row, columns - 1));
    currentView()->selectionModel()->select(range, QItemSelectionModel::Toggle |
                                                       QItemSelectionModel::Rows);
    advanceCurrentRow();
}

QList<int> PaneWidget::selectedRows() const {
    QList<int> rows;
    const QModelIndexList indexes = currentView()->selectionModel()->selectedRows();
    rows.reserve(indexes.size());
    for (const QModelIndex &index : indexes) {
        rows.append(index.row());
    }
    std::sort(rows.begin(), rows.end());
    return rows;
}

int PaneWidget::currentRow() const {
    const QModelIndex current = m_view->currentIndex();
    return current.isValid() ? current.row() : -1;
}

void PaneWidget::syncSelectionFromMarks() {
    // Guarded, because setting the selection is what emits `selectionChanged`,
    // which writes the selection back as the mark set - and a mark made here
    // would arrive there as a mark to make, forever.
    if (m_syncingSelection) {
        return;
    }
    m_syncingSelection = true;

    QVector<int> rows(m_model->rowCount());
    const int count = jtf_marked_rows(m_app, m_pane, rows.data(), rows.size());
    QItemSelection wanted;
    const int columns = m_model->columnCount();
    for (int i = 0; i < count; ++i) {
        const int row = rows.at(i);
        wanted.select(m_model->index(row, 0), m_model->index(row, columns - 1));
    }
    currentView()->selectionModel()->select(wanted, QItemSelectionModel::ClearAndSelect
                                                        | QItemSelectionModel::Rows);
    m_syncingSelection = false;
}

/// How far Page Up and Page Down move when the cursor is walking on its own.
///
/// A fixed step rather than a measured screenful: the two are the same for any
/// list tall enough to page through, and asking the viewport mid-event brings
/// in a geometry that is being scrolled at the same time.
static constexpr int kPageStep = 20;

void PaneWidget::advanceCurrentRow() {
    const int next = qMin(currentRow() + 1, m_model->rowCount() - 1);
    if (next < 0) {
        return;
    }
    // The cursor moves; the selection does not.
    //
    // `QAbstractItemView::setCurrentIndex` also *selects* what it moves to,
    // and since selection is the mark that undid the mark Space had just
    // made: the tick appeared and vanished as the cursor stepped off the row.
    // Marking a second file from the keyboard was impossible.
    m_view->selectionModel()->setCurrentIndex(m_model->index(next, 0),
                                              QItemSelectionModel::NoUpdate);
}

/// Whether `at` is inside the row's tick box rather than on the row.
///
/// Asked of the style rather than measured here, so it stays right when the
/// style, the font size or the platform changes the box.
bool PaneWidget::onCheckBox(const QModelIndex &index, const QPoint &at) const {
    if (index.column() != 0) {
        return false;
    }
    QStyleOptionViewItem option;
    option.initFrom(m_view);
    option.rect = m_view->visualRect(index);
    option.features |= QStyleOptionViewItem::HasCheckIndicator;
    const QRect box = m_view->style()->subElementRect(QStyle::SE_ItemViewItemCheckIndicator,
                                                      &option, m_view);
    // A little slack: the box is small, and a press a pixel outside it is a
    // press meant for it.
    return box.adjusted(-2, -2, 2, 2).contains(at);
}

QString PaneWidget::formatSize(quint64 bytes) {
    // Binary units, matching what the list shows, so two numbers on one line
    // cannot be in two different systems.
    static const char *const units[] = {"B", "KB", "MB", "GB", "TB", "PB"};
    double value = static_cast<double>(bytes);
    int unit = 0;
    while (value >= 1024.0 && unit < 5) {
        value /= 1024.0;
        ++unit;
    }
    return unit == 0 ? QStringLiteral("%1 B").arg(bytes)
                     : QStringLiteral("%1 %2").arg(value, 0, 'f', 1).arg(QLatin1String(units[unit]));
}

bool PaneWidget::handleDrop(QDropEvent *event) {
    const QMimeData *data = event->mimeData();
    if (!data->hasUrls()) {
        return false;
    }

    QStringList paths;
    for (const QUrl &url : data->urls()) {
        // Only local files. A drop of an http URL is a download, which is a
        // different feature and not one to fake.
        if (url.isLocalFile()) {
            paths << url.toLocalFile();
        }
    }
    if (paths.isEmpty()) {
        return false;
    }

    emit focusRequested(m_pane);
    // The window asks whether this is a move or a copy. What is passed on is
    // only where the drag came from: a drag out of one of our own panes is
    // ordinarily a move, one from another application ordinarily a copy, and
    // that decides which button the dialog offers first - not what happens.
    emit dropRequested(paths, event->source() != nullptr ? 1 : 0);
    return true;
}

void PaneWidget::resizeEvent(QResizeEvent *event) {
    QWidget::resizeEvent(event);
    scheduleFitNameColumn();
    positionSearchOverlay();
}

void PaneWidget::positionSearchOverlay() {
    if (m_searchOverlay == nullptr || !m_searchOverlay->isVisible()) {
        return;
    }
    // Centred horizontally over the list and near its top, where the eye
    // already is - dead centre would sit on top of the results it is
    // reporting, which is the one place it must not be.
    const QRect area = m_view->geometry();
    const QSize hint = m_searchOverlay->sizeHint();
    m_searchOverlay->setGeometry(area.x() + (area.width() - hint.width()) / 2,
                                 area.y() + 18, hint.width(), hint.height());
    m_searchOverlay->raise();
}

bool PaneWidget::eventFilter(QObject *watched, QEvent *event) {
    if (event->type() == QEvent::FocusIn || event->type() == QEvent::MouseButtonPress) {
        emit focusRequested(m_pane);
    }

    // A drag carries what the cursor is on, not what happens to be selected.
    //
    // Qt builds a drag's payload from the selected rows, so pressing on an
    // unselected row and dragging it away sent the *old* selection instead -
    // drag `bb` while `aa` is selected and the drop was handed `aa`. Selecting
    // the pressed row first is what every file manager does, and here it also
    // sets the mark, because selection is the mark (`AGENTS.md` §10).
    if (watched == m_view && event->type() == QEvent::MouseButtonPress) {
        auto *mouse = static_cast<QMouseEvent *>(event);
        if (mouse->button() == Qt::LeftButton
            && (mouse->modifiers() & (Qt::ShiftModifier | Qt::ControlModifier | Qt::MetaModifier))
                   == Qt::NoModifier) {
            const QModelIndex under = m_view->indexAt(mouse->pos());
            // Not when the press lands on the tick box. Selection *is* the
            // mark here, so replacing the selection with the pressed row
            // clears every other mark - which is what ticking a second box
            // did: the first box emptied itself as the second filled.
            //
            // A box is for adding one thing to a set. The row beside it is for
            // choosing one thing. They must not be the same gesture.
            if (under.isValid() && !onCheckBox(under, mouse->pos())
                && !m_view->selectionModel()->isSelected(under)) {
                m_view->selectionModel()->select(
                    under, QItemSelectionModel::ClearAndSelect | QItemSelectionModel::Rows);
                m_view->setCurrentIndex(under);
            }
        }
    }

    // Claim Shift+letter before the shortcut system does.
    //
    // Qt matches a one-letter QKeySequence against Shift+letter as well as the
    // bare letter, because the text both produce is the same capital. Shortcuts
    // are also delivered *before* the focus widget sees a key press. Together
    // that meant `Shift-H` ran `file.view_hex`, `Shift-C` would have copied and
    // `Shift-M` moved - every bare-letter command swallowing its own shifted
    // form, and CV.HLP §二's Shift+letter jump never reaching the code that
    // implements it.
    //
    // `ShortcutOverride` is the mechanism for exactly this: accepting it says
    // "this key is mine", and the key then arrives as an ordinary press below.
    if (event->type() == QEvent::ShortcutOverride && watched == m_view
        && !jtf_type_ahead(m_app)) {
        auto *key = static_cast<QKeyEvent *>(event);
        const int code = key->key();
        const bool jumpable = (code >= Qt::Key_A && code <= Qt::Key_Z)
                              || (code >= Qt::Key_0 && code <= Qt::Key_9);
        const Qt::KeyboardModifiers mods = key->modifiers();
        if (jumpable && mods.testFlag(Qt::ShiftModifier)
            && !mods.testFlag(Qt::ControlModifier) && !mods.testFlag(Qt::AltModifier)
            && !mods.testFlag(Qt::MetaModifier)) {
            event->accept();
            return true;
        }
    }

    // The name column is fitted here rather than in the pane's resizeEvent.
    // The pane learns its new size before the view inside it does, so fitting
    // there computed the surplus from the *previous* viewport width - which on
    // the very first show is the width the view had before any layout ran, and
    // no later resize arrives to correct it. That is why the columns came up
    // filling half the window and stayed there. The viewport's own resize
    // always carries the width the rows are actually drawn at.
    // A drag of a column divider starts with a press on the header and ends
    // with its release. Between those two, a width change is the user's.
    if (watched == m_view->horizontalHeader()) {
        if (event->type() == QEvent::MouseButtonPress) {
            m_userResizing = true;
        } else if (event->type() == QEvent::MouseButtonRelease) {
            m_userResizing = false;
        }
    }
    if (watched == m_view->viewport() && event->type() == QEvent::Resize) {
        scheduleFitNameColumn();
    }
    if (watched == m_view->viewport() && event->type() == QEvent::Leave) {
        // Otherwise the last row the pointer touched stays lit after the
        // pointer has gone somewhere else entirely.
        setHoveredRow(-1);
    }

    switch (event->type()) {
    case QEvent::DragEnter:
    case QEvent::DragMove: {
        auto *drag = static_cast<QDragMoveEvent *>(event);
        // A tab dragged onto this pane at all, not only onto its tab strip.
        // The strip is a thin target, and "put this tab over there" means the
        // pane, not the two-centimetre band along its top.
        if (drag->mimeData()->hasFormat(kTabMimeType)) {
            drag->setDropAction(Qt::MoveAction);
            drag->accept();
            return true;
        }
        if (drag->mimeData()->hasUrls()) {
            drag->acceptProposedAction();
            return true;
        }
        return false;
    }
    case QEvent::Drop: {
        auto *drop = static_cast<QDropEvent *>(event);
        if (drop->mimeData()->hasFormat(kTabMimeType)) {
            const QList<QByteArray> parts = drop->mimeData()->data(kTabMimeType).split(':');
            if (parts.size() == 2) {
                drop->setDropAction(Qt::MoveAction);
                drop->accept();
                emit tabMergeRequested(parts.at(0).toInt(), parts.at(1).toInt(), m_pane);
                return true;
            }
            return false;
        }
        if (handleDrop(drop)) {
            drop->acceptProposedAction();
            return true;
        }
        return false;
    }
    default:
        break;
    }

    // Typing a filter narrows the list; the next thing anyone wants is to act
    // on what is left. Tab, Enter and Down all hand the keyboard to the list
    // rather than leaving it in a box whose work is done.
    if (watched == m_tabs) {
        switch (event->type()) {
        case QEvent::MouseButtonPress: {
            auto *mouse = static_cast<QMouseEvent *>(event);
            if (mouse->button() == Qt::LeftButton) {
                m_dragTab = m_tabs->tabAt(mouse->position().toPoint());
                m_dragOrigin = mouse->globalPosition().toPoint();
            }
            break;
        }
        case QEvent::MouseMove: {
            auto *mouse = static_cast<QMouseEvent *>(event);
            if (m_dragTab < 0 || !(mouse->buttons() & Qt::LeftButton)) {
                break;
            }
            // Vertical distance, not any distance: dragging sideways along
            // the strip is Qt reordering the tabs, which is a different and
            // equally wanted gesture. Only leaving the strip means "out".
            const int dy = qAbs(mouse->globalPosition().toPoint().y() - m_dragOrigin.y());
            if (dy > m_tabs->height() + kTearOffDistance) {
                const int index = m_dragTab;
                m_dragTab = -1;

                // Offered to other strips first. Only if nobody takes it does
                // the tab become its own window - so dragging onto another
                // window merges, and dragging into empty space tears off,
                // from one gesture.
                auto *mime = new QMimeData;
                mime->setData(kTabMimeType,
                              QStringLiteral("%1:%2").arg(m_pane).arg(index).toUtf8());
                auto *drag = new QDrag(this);
                drag->setMimeData(mime);
                if (drag->exec(Qt::MoveAction) == Qt::MoveAction) {
                    return true; // another strip took it
                }
                // Released first, or the new window opens under a pointer
                // that Qt still believes is dragging a tab in the old one.
                QMouseEvent release(QEvent::MouseButtonRelease, mouse->position(),
                                    mouse->globalPosition(), Qt::LeftButton, Qt::NoButton,
                                    Qt::NoModifier);
                QApplication::sendEvent(m_tabs, &release);
                emit tearOffRequested(index);
                return true;
            }
            break;
        }
        case QEvent::MouseButtonRelease:
            m_dragTab = -1;
            break;

        case QEvent::DragEnter:
        case QEvent::DragMove: {
            auto *drag = static_cast<QDragMoveEvent *>(event);
            if (drag->mimeData()->hasFormat(kTabMimeType)) {
                drag->setDropAction(Qt::MoveAction);
                drag->accept();
                return true;
            }
            break;
        }
        case QEvent::Drop: {
            auto *drop = static_cast<QDropEvent *>(event);
            const QByteArray payload = drop->mimeData()->data(kTabMimeType);
            const QList<QByteArray> parts = payload.split(':');
            if (parts.size() != 2) {
                break;
            }
            const int fromPane = parts.at(0).toInt();
            const int tabIndex = parts.at(1).toInt();
            drop->setDropAction(Qt::MoveAction);
            drop->accept();
            emit tabMergeRequested(fromPane, tabIndex, m_pane);
            return true;
        }
        default:
            break;
        }
    }

    if (event->type() == QEvent::KeyPress && watched == m_filter) {
        auto *key = static_cast<QKeyEvent *>(event);
        switch (key->key()) {
        case Qt::Key_Tab:
        case Qt::Key_Return:
        case Qt::Key_Enter:
        case Qt::Key_Down:
            // Tab, Enter and Down all mean "I have typed enough, take me to
            // the results". The filter itself stays in force - the list is
            // narrowed and the box still shows why.
            focusList();
            if (m_view->currentIndex().isValid()) {
                return true;
            }
            ensureCurrentRow();
            return true;
        case Qt::Key_Escape:
            // Escape ends the filter and empties it. Leaving the text behind
            // would mean the next `F` reopened a box already narrowing the
            // list, which is a mode the user thought they had left.
            clearFilter();
            return true;
        default:
            break;
        }
    }

    // Moving the cursor must not undo the marks.
    //
    // Selection is the mark here, and Qt's own navigation replaces the
    // selection on every plain arrow key - so `Space`, `Down`, `Space` marked
    // one file rather than two, and there was no way to build a set from the
    // keyboard at all. The cursor moves on its own; `Space` is what marks.
    //
    // Only while something is marked. With an empty set the arrows behave the
    // way every list behaves, moving the highlight with the cursor, because
    // that is what browsing a folder should feel like. Once a set is being
    // built, moving through the list stops destroying it.
    if (event->type() == QEvent::KeyPress && watched == m_view
        && jtf_marked_count(m_app, m_pane) > 0) {
        auto *key = static_cast<QKeyEvent *>(event);
        const bool plain =
            (key->modifiers()
             & (Qt::ShiftModifier | Qt::ControlModifier | Qt::MetaModifier | Qt::AltModifier))
            == Qt::NoModifier;
        int target = -1;
        const int rows = m_model->rowCount();
        const int at = currentRow();
        if (plain && rows > 0) {
            switch (key->key()) {
            case Qt::Key_Down:
                target = qMin(at + 1, rows - 1);
                break;
            case Qt::Key_Up:
                target = qMax(at - 1, 0);
                break;
            case Qt::Key_PageDown:
                target = qMin(at + kPageStep, rows - 1);
                break;
            case Qt::Key_PageUp:
                target = qMax(at - kPageStep, 0);
                break;
            case Qt::Key_Home:
                target = 0;
                break;
            case Qt::Key_End:
                target = rows - 1;
                break;
            default:
                break;
            }
        }
        if (target >= 0) {
            const QModelIndex to = m_model->index(target, 0);
            m_view->selectionModel()->setCurrentIndex(to, QItemSelectionModel::NoUpdate);
            m_view->scrollTo(to, QAbstractItemView::EnsureVisible);
            return true;
        }
    }

    if (event->type() == QEvent::KeyPress && watched == m_view) {
        auto *key = static_cast<QKeyEvent *>(event);
        switch (key->key()) {
        case Qt::Key_Return:
        case Qt::Key_Enter: {
            // Through the command, not straight to `openRow`.
            //
            // Opening is not always "hand this to the platform": on an archive
            // it shows the contents in a window instead, and that decision
            // lives with the window rather than here. Calling `openRow`
            // directly bypassed it, so Enter on a `.zip` did the ordinary
            // thing and the archive window never appeared.
            if (m_view->currentIndex().isValid()) {
                emit commandRequested(QStringLiteral("file.open"));
            }
            return true;
        }
        case Qt::Key_Backspace:
            if (!m_typeAhead.isEmpty()) {
                m_typeAhead.chop(1);
                typeAhead(m_typeAhead);
                return true;
            }
            jtf_navigate_up(m_app, m_pane);
            emit stateChanged();
            return true;

        case Qt::Key_Escape:
            if (jtf_is_searching(m_app, m_pane)) {
                clearSearch();
                return true;
            }
            if (m_filterBar->isVisible()) {
                clearFilter();
                return true;
            }
            m_typeAhead.clear();
            return true;

        case Qt::Key_Home:
        case Qt::Key_End: {
            // QTableView reads Home as "first column of this row" and only
            // moves rows on Ctrl+Home. A file list has one axis that matters,
            // so both spellings go to the top and bottom of the list.
            //
            // Home lands on the first entry rather than on `..`, matching
            // where the cursor is put on arriving in a folder. `..` is one
            // press of Up away.
            const int rows = m_model->rowCount();
            if (rows == 0) {
                return true;
            }
            const bool toEnd = key->key() == Qt::Key_End;
            int row = 0;
            if (toEnd) {
                row = rows - 1;
            } else if (rows > 1 && jtf_row_is_parent(m_app, m_pane, 0)) {
                row = 1;
            }
            setCurrentRow(row, QAbstractItemView::EnsureVisible);
            return true;
        }

        case Qt::Key_Left:
        case Qt::Key_Right: {
            // The list has columns, so Qt would move the current cell
            // sideways. In a file manager the horizontal axis is the folder
            // hierarchy, not the column list, and both CView and every
            // tree-walking file manager read Left as "out" and Right as "in".
            //
            // Tested against the modifiers that mean something, not against
            // "no modifiers at all". macOS reports the arrow keys with
            // `KeypadModifier` set - they are on the keypad as far as the
            // window system is concerned - so an exact comparison with
            // NoModifier was never true, Left and Right fell through to Qt,
            // and the cursor walked across the columns instead of the folder
            // tree. Up and Down were unaffected because they test individual
            // flags, which is why only these two looked broken.
            constexpr Qt::KeyboardModifiers kMeaningful =
                Qt::ControlModifier | Qt::AltModifier | Qt::ShiftModifier | Qt::MetaModifier;
            if ((key->modifiers() & kMeaningful) != Qt::NoModifier) {
                break;
            }
            const QString chord = chordFor(key);
            const QByteArray utf8 = chord.toUtf8();
            const QString id = jtfText([&](char *b, int l) {
                return jtf_command_for_chord(m_app, utf8.constData(), b, l);
            });
            if (id.isEmpty()) {
                break;
            }
            emit commandRequested(id);
            return true;
        }

        case Qt::Key_Down:
            // Cmd+Down opens, the macOS convention, and the counterpart to
            // Cmd+Up for going back out.
            if (key->modifiers().testFlag(Qt::ControlModifier)) {
                const QModelIndex current = m_view->currentIndex();
                if (current.isValid()) {
                    openRow(current.row());
                }
                return true;
            }
            break;

        default: {
            // A keymap that binds bare letters to commands - the CView
            // tradition, where `e` edits and `v` views - decides this before
            // type-ahead gets a chance, because in that tradition typing a
            // letter is not how you find a file.
            if (!jtf_type_ahead(m_app)) {
                // `CV.HLP` §二: Shift-A..Z and 0..9 move the cursor to the
                // first entry starting with that character. This is CView's
                // own answer to having spent the bare letters on commands,
                // and it is what keeps the mode navigable without them.
                //
                // Checked before the keymap, so a chord the keymap happens to
                // bind on Shift+letter does not take a key CView reserves for
                // this. Nothing else does today; the check being first is what
                // keeps it that way.
                const int code = key->key();
                const bool shiftOnly =
                    key->modifiers().testFlag(Qt::ShiftModifier)
                    && !key->modifiers().testFlag(Qt::ControlModifier)
                    && !key->modifiers().testFlag(Qt::AltModifier)
                    && !key->modifiers().testFlag(Qt::MetaModifier);
                const bool jumpable = (code >= Qt::Key_A && code <= Qt::Key_Z)
                                      || (code >= Qt::Key_0 && code <= Qt::Key_9);
                if (shiftOnly && jumpable) {
                    const QChar letter = code <= Qt::Key_Z && code >= Qt::Key_A
                                             ? QChar(QLatin1Char('a' + (code - Qt::Key_A)))
                                             : QChar(QLatin1Char('0' + (code - Qt::Key_0)));
                    // From the row after the current one, so pressing it again
                    // walks through the entries sharing that first letter -
                    // which is what makes it useful in a folder of hundreds.
                    const int rows = m_model->rowCount();
                    if (rows > 0) {
                        const int from = qMax(0, m_view->currentIndex().row()) + 1;
                        for (int step = 0; step < rows; ++step) {
                            const int row = (from + step) % rows;
                            const QString name =
                                m_model->data(m_model->index(row, 0), Qt::DisplayRole).toString();
                            if (name.startsWith(letter, Qt::CaseInsensitive)) {
                                setCurrentRow(row, QAbstractItemView::PositionAtCenter);
                                break;
                            }
                        }
                    }
                    return true;
                }

                const QString chord = chordFor(key);
                if (!chord.isEmpty()) {
                    const QByteArray utf8 = chord.toUtf8();
                    const QString id = jtfText([&](char *b, int l) {
                        return jtf_command_for_chord(m_app, utf8.constData(), b, l);
                    });
                    if (!id.isEmpty()) {
                        emit commandRequested(id);
                        return true;
                    }
                }
                // Swallow an unbound bare key rather than let the view treat
                // a letter as cursor movement. A key held with a modifier is
                // left alone: it may be a window shortcut, and eating it here
                // is how Alt-T stopped reaching the menu action that runs it.
                const bool modified =
                    key->modifiers().testFlag(Qt::ControlModifier) ||
                    key->modifiers().testFlag(Qt::AltModifier) ||
                    key->modifiers().testFlag(Qt::MetaModifier);
                if (!modified && !key->text().isEmpty() && key->text().at(0).isPrint()) {
                    return true;
                }
                break;
            }

            // Plain printable text starts or continues a type-ahead search.
            const QString text = key->text();
            if (!text.isEmpty() && text.at(0).isPrint() &&
                !key->modifiers().testFlag(Qt::ControlModifier) &&
                !key->modifiers().testFlag(Qt::AltModifier)) {
                // A pause resets the search, so "ab" then later "c" looks for
                // "c" rather than "abc".
                if (m_typeAheadClock->elapsed() > 900) {
                    m_typeAhead.clear();
                }
                m_typeAheadClock->restart();
                m_typeAhead += text;
                if (typeAhead(m_typeAhead)) {
                    return true;
                }
                m_typeAhead.clear();
                return true;
            }
            break;
        }
        }
    }
    return QWidget::eventFilter(watched, event);
}

bool PaneWidget::typeAhead(const QString &prefix) {
    if (prefix.isEmpty()) {
        return false;
    }
    const int rows = m_model->rowCount();
    const int from = qMax(0, m_view->currentIndex().row());

    // Search from the current row so repeated presses walk through matches,
    // then wrap.
    for (int step = 0; step < rows; ++step) {
        const int row = (from + step) % rows;
        const QString name =
            m_model->data(m_model->index(row, 0), Qt::DisplayRole).toString();
        if (name.startsWith(prefix, Qt::CaseInsensitive)) {
            const QModelIndex index = m_model->index(row, 0);
            m_view->setCurrentIndex(index);
            m_view->scrollTo(index, QAbstractItemView::PositionAtCenter);
            return true;
        }
    }
    return false;
}

void PaneWidget::closeTab(int index) {
    // Deferred, because closing the last tab of a pane closes the pane, and
    // the pane is the widget whose close button we are standing inside. Doing
    // it here would delete this object and the button that called us while
    // Qt is still delivering that button's click.
    QTimer::singleShot(0, this, [this, index] {
        jtf_close_tab(m_app, m_pane, index);
        emit stateChanged();
    });
}

void PaneWidget::syncTabCloseButtons() {
    // The last tab of the last pane has nothing to close to, so it shows no
    // close control. A button that is present and does nothing is worse than
    // no button: the rule becomes indistinguishable from a fault.
    //
    // Sized away rather than hidden. QTabBar shows the buttons it has been
    // given whenever it lays its tabs out, so setVisible(false) lasts only
    // until the next layout; a button with no size is laid out, shown, and
    // occupies nothing. Removing it instead would mean owning its lifetime
    // against a tab bar that may already have deleted it.
    //
    // Both callers come through here rather than each setting the icon
    // themselves: applyTheme re-coloured every close mark unconditionally,
    // which put back the one syncTabs had just taken away.
    // Asked of the core rather than guessed from a pane count: a torn-off
    // window's last pane can close (the window goes with it) while the main
    // window's cannot, and that is not something a count can tell you.
    const bool closable = m_tabs->count() > 1 || jtf_can_close_pane(m_app, m_pane) != 0;
    for (int i = 0; i < m_tabs->count(); ++i) {
        if (auto *close = qobject_cast<QToolButton *>(m_tabs->tabButton(i, QTabBar::RightSide))) {
            close->setEnabled(closable);
            // Full strength on the tab you are on, quiet on the others. The
            // one you are most likely to close is the one you can see the
            // mark on, and the rest do not compete with their own titles.
            // The mark on the current tab is at full strength; the others are
            // quieter but not faint. The secondary text colour turned out to
            // be near enough to the tab background that the mark was
            // effectively invisible - "quiet" has to stay legible, or the
            // control might as well not be drawn.
            QColor colour = m_tabCloseStrong;
            if (i != m_tabs->currentIndex()) {
                colour.setAlphaF(0.70F);
            }
            // The icon is what actually paints, so taking it away is what
            // takes the mark away; a zero icon size still draws a scaled-down
            // one. The zero geometry then stops it holding room open.
            close->setIcon(closable ? glyph::make(glyph::Shape::Close, colour) : QIcon());
            close->setFixedSize(closable ? QSize(kTabCloseBox + kTabCloseGap, kTabCloseBox)
                                        : QSize(0, 0));
        }
    }
}

void PaneWidget::syncTabs() {
    const int count = jtf_tab_count(m_app, m_pane);
    QSignalBlocker blocker(m_tabs);
    while (m_tabs->count() > count) {
        m_tabs->removeTab(m_tabs->count() - 1);
    }
    while (m_tabs->count() < count) {
        m_tabs->addTab(QString());
    }
    for (int i = 0; i < count; ++i) {
        m_tabs->setTabText(i, jtfText([&](char *buf, int len) {
                               return jtf_tab_title(m_app, m_pane, i, buf, len);
                           }));
    }
    // Our own close buttons rather than Qt's. Styling QTabBar::close-button
    // to give it room from the tab's edge is what made it vanish: a
    // stylesheet rule on a subcontrol with no `image` leaves Qt drawing
    // nothing at all. A real widget is also the only way the mark can follow
    // the theme's colour instead of the platform's.
    for (int i = 0; i < count; ++i) {
        if (m_tabs->tabButton(i, QTabBar::RightSide) != nullptr) {
            continue;
        }
        auto *close = new QToolButton(m_tabs);
        close->setObjectName(QStringLiteral("JtfTabClose"));
        close->setAutoRaise(true);
        close->setFocusPolicy(Qt::NoFocus);
        close->setIconSize(QSize(kTabCloseIcon, kTabCloseIcon));
        close->setIcon(glyph::make(glyph::Shape::Close, m_tabCloseColour));
        close->setToolTip(jtfText(
            [&](char *buf, int len) { return jtf_tr(m_app, "command.tab.close", buf, len); }));
        connect(close, &QToolButton::clicked, this, [this, close] {
            // By identity, not by a captured index: closing tab 0 renumbers
            // every tab after it, and a captured index would then close the
            // wrong one.
            for (int at = 0; at < m_tabs->count(); ++at) {
                if (m_tabs->tabButton(at, QTabBar::RightSide) == close) {
                    closeTab(at);
                    return;
                }
            }
        });
        m_tabs->setTabButton(i, QTabBar::RightSide, close);
    }

    // A pinned tab is marked in the strip. Pinning changes what the tab does -
    // it will not close and will not reorder out of the leading block - and a
    // state with no appearance is a state nobody can see they are in.
    for (int i = 0; i < m_tabs->count(); ++i) {
        m_tabs->setTabIcon(i, jtf_tab_is_pinned(m_app, m_pane, i) != 0
                                  ? glyph::make(glyph::Shape::Bookmark, m_tabCloseColour)
                                  : QIcon());
    }

    syncTabCloseButtons();

    // One tab is not a choice, so there is nothing to show. The strip goes and
    // the row keeps only the `+`, which is the one thing still worth reaching
    // for - hiding that too would leave no way to open a second tab from the
    // pane itself.
    m_tabs->setVisible(count > 1);

    m_tabs->setCurrentIndex(jtf_active_tab(m_app, m_pane));
    // Always shown, even for a single tab. Hiding the bar there left the "+"
    // beside it floating on an otherwise empty row, and a lone plus with
    // nothing to add to reads as a stray control rather than as a tab strip.
    m_tabs->setVisible(true);
}

void PaneWidget::syncSortIndicator() {
    const int column = jtf_sort_column(m_app, m_pane);
    const Qt::SortOrder order =
        jtf_sort_ascending(m_app, m_pane) ? Qt::AscendingOrder : Qt::DescendingOrder;
    QSignalBlocker blocker(m_view->horizontalHeader());
    m_view->horizontalHeader()->setSortIndicator(column, order);
}

void PaneWidget::syncPath() {
    // The shown path, not the local one: a pane on a server has no local path
    // at all, and asking for one left the bar blank.
    const QString path =
        jtfText([&](char *buf, int len) { return jtf_display_path(m_app, m_pane, buf, len); });
    const bool moved = !m_shownPath.isEmpty() && path != m_shownPath;
    m_shownPath = path;
    m_crumbs->setPath(path);

    // Having arrived somewhere, the keyboard belongs in the list. Decided here
    // rather than at each command because there are many ways to move - a
    // menu, the tree, the sidebar, a breadcrumb segment, a double click, a
    // key - and all of them end here. Deciding it per route means the routes
    // added later are the ones that get forgotten.
    //
    // Deferred, because a menu hands the focus back to whatever had it before
    // it opened, and that happens after this runs. Skipped while a text field
    // has the focus: the user is typing, and taking it away mid-word is worse
    // than an arrow key that goes to the wrong widget.
    if (moved && m_active) {
        QTimer::singleShot(0, this, [this] {
            // Only when nothing holds it. Taking the keyboard from something
            // that has it would break walking the folder tree with the arrow
            // keys - every step there is a navigation, and the focus would
            // jump to the list after each one. This is for the case the user
            // actually hits: a menu or a click has left the window with no
            // focused widget at all, and the arrows then go nowhere.
            if (QApplication::focusWidget() != nullptr) {
                return;
            }
            focusList();
        });
    }
}

void PaneWidget::refresh() {
    // Nothing to close down to: one pane is the floor, and a button that
    // cannot do anything is worse than no button.
    if (m_close != nullptr) {
        m_close->setVisible(jtf_pane_count(m_app) > 1);
    }
    syncTabs();
    syncFilterBar();
    syncPath();
    syncSortIndicator();
    m_model->setThumbnailsEnabled(jtf_thumbnails(m_app) != 0);
    applyViewMode();
    applyColumnVisibility();
    m_model->refresh();
    ensureCurrentRow();
    retranslate();
}

void PaneWidget::refreshRows() {
    m_model->refresh();
    ensureCurrentRow();
    restoreSelectionFromMarks();
    syncMarkAll();
    retranslate();
}

void PaneWidget::restoreSelectionFromMarks() {
    // The marks are the stored state - the session keeps them and an operation
    // reads them - so arriving in a folder puts the selection back to match,
    // which is what lets marks survive navigating away and back now that the
    // two are one thing (`docs/UI_TEST_PLAN.md` MARK-004).
    // Nothing marked means nothing selected. Returning early here instead
    // left the previous highlight standing after the marks were cleared.
    const int count = jtf_marked_rows(m_app, m_pane, nullptr, 0);
    if (count <= 0) {
        m_restoringMarks = true;
        m_view->selectionModel()->clearSelection();
        m_restoringMarks = false;
        return;
    }
    QVector<int> rows(count);
    jtf_marked_rows(m_app, m_pane, rows.data(), count);

    QItemSelection selection;
    const int columns = m_model->columnCount();
    for (const int row : rows) {
        if (row >= 0 && row < m_model->rowCount()) {
            selection.select(m_model->index(row, 0), m_model->index(row, columns - 1));
        }
    }
    m_restoringMarks = true;
    if (selection.isEmpty()) {
        m_view->selectionModel()->clearSelection();
    } else {
        m_view->selectionModel()->select(selection, QItemSelectionModel::ClearAndSelect);
    }
    m_restoringMarks = false;
}

void PaneWidget::syncMarkAll() {
    // Three states, because two would lie: with some rows marked the box has
    // to say "some", or it claims everything is marked when it is not.
    const int marked = jtf_marked_count(m_app, m_pane);
    const int listed = jtf_listed_count(m_app, m_pane);
    m_header->setMarkAllState(marked == 0                     ? Qt::Unchecked
                              : marked >= listed && listed > 0 ? Qt::Checked
                                                               : Qt::PartiallyChecked);
}

void PaneWidget::focusList() { currentView()->setFocus(Qt::OtherFocusReason); }

QAbstractItemView *PaneWidget::currentView() const {
    return m_grid->isVisible() ? static_cast<QAbstractItemView *>(m_grid)
                               : static_cast<QAbstractItemView *>(m_view);
}

void PaneWidget::applyViewMode() {
    const bool grid = jtf_view_mode(m_app, m_pane) != 0;
    if (grid == m_grid->isVisible()) {
        return;
    }
    // The icon size is the thumbnail's, so a grid of photographs shows the
    // photographs rather than a grid of magnified icons.
    m_grid->setIconSize(QSize(kGridIconEdge, kGridIconEdge));
    m_grid->setGridSize(QSize(kGridIconEdge + 40, kGridIconEdge + 46));
    if (grid) {
        // Same model and same selection model, so the cursor and the marks
        // survive the switch.
        m_grid->setModel(m_model);
        m_grid->setSelectionModel(m_view->selectionModel());
    } else {
        m_grid->setModel(nullptr);
    }
    m_grid->setVisible(grid);
    m_view->setVisible(!grid);
    if (grid) {
        m_grid->setFocus(Qt::OtherFocusReason);
    }
}

void PaneWidget::ensureCurrentRow() {
    const int rows = m_model->rowCount();
    if (rows == 0) {
        return;
    }

    // A list with no cursor has nothing for Home, End or the arrow keys to
    // move, so arriving in a folder has to leave one somewhere. Only on a new
    // listing: repositioning on every refresh would drag the cursor back
    // while rows are still streaming in.
    const quint64 generation = m_model->generation();
    const QModelIndex current = m_view->currentIndex();
    if (generation == m_positionedGeneration && current.isValid() && current.row() < rows) {
        return;
    }
    // Not while the rows are still arriving.
    //
    // The row to put the cursor back on is a position in the finished listing,
    // and a large folder is delivered in batches - so deciding from the first
    // batch lands somewhere arbitrary and then never reconsiders, because the
    // generation would already be recorded as positioned. The cursor stays put
    // for the moment the listing takes to fill; that is the same moment the
    // rows are visibly appearing, so there is nothing to sit still for.
    if (jtf_is_loading(m_app, m_pane) != 0) {
        return;
    }
    m_positionedGeneration = generation;

    // Stepping out of a folder puts the cursor on the folder you left; any
    // other arrival starts at the first entry, past the `..` row.
    int row = jtf_take_focus_row(m_app, m_pane);
    if (row < 0 || row >= rows) {
        row = rows > 1 && jtf_row_is_parent(m_app, m_pane, 0) ? 1 : 0;
    }
    setCurrentRow(row, QAbstractItemView::PositionAtCenter);

    // The cursor is only useful if the keys reach it. Never steal focus from
    // a text field the user is typing in.
    QWidget *focused = QApplication::focusWidget();
    const bool typing = qobject_cast<QLineEdit *>(focused) != nullptr;
    if (!typing && m_active) {
        m_view->setFocus(Qt::OtherFocusReason);
    }
}

void PaneWidget::setCurrentRow(int row, QAbstractItemView::ScrollHint hint) {
    // Through the selection model, not setCurrentIndex: that moves the
    // current cell without selecting anything, so the row the cursor is on
    // does not light up and the list looks like it ignored the key. The
    // arrow keys go through the selection model too, which is why they
    // always looked right.
    const QModelIndex index = m_model->index(row, 0);
    if (!index.isValid()) {
        return;
    }
    m_view->selectionModel()->setCurrentIndex(index,
                                              QItemSelectionModel::ClearAndSelect |
                                                  QItemSelectionModel::Rows);
    currentView()->scrollTo(index, hint);
}

void PaneWidget::setListFont(const QFont &font, const QFont &fixed, bool fixedEverywhere) {
    // The widget's own font is the proportional one; the model overrides it
    // per column for the ones that are read as aligned values. Row height is
    // measured from whichever is taller, so switching scope does not make the
    // rows jump.
    m_view->setFont(font);
    m_view->horizontalHeader()->setFont(font);
    m_model->setListFonts(font, fixed, fixedEverywhere);
    // Row height follows the font, or descenders clip and the list looks
    // cramped at larger sizes.
    // Generous rather than tight: the reference layouts get their calm from
    // row height more than from anything else, and a list at the minimum
    // legible spacing is the single thing that makes an interface look cheap.
    const int rowHeight =
        qMax(QFontMetrics(font).height(), QFontMetrics(fixed).height()) + 12;
    m_view->verticalHeader()->setDefaultSectionSize(qMax(26, rowHeight));

    QFont chrome = font;
    chrome.setPointSizeF(font.pointSizeF() * 0.95);
    m_crumbs->setFont(chrome);
    m_status->setFont(chrome);
}

void PaneWidget::retranslate() {
    // Named tr_ rather than tr: a lambda that shadows QObject::tr reads at a
    // glance as if it were QObject::tr, and QObject::tr would silently return
    // the key. One spelling for catalogue lookup across the whole Qt layer.
    const auto tr_ = [&](const char *key) {
        return jtfText([&](char *buf, int len) { return jtf_tr(m_app, key, buf, len); });
    };

    if (m_close != nullptr) {
        m_close->setToolTip(tr_("command.workspace.pane.close"));
    }
    if (m_filterClose != nullptr) {
        // Set here rather than at construction: a tooltip written once never
        // follows a language change, and this one names a key as well.
        m_filterClose->setToolTip(tr_("filter.close"));
    }

    QString status;
    const QString errorKey =
        jtfText([&](char *buf, int len) { return jtf_error_key(m_app, m_pane, buf, len); });
    if (m_reconnect != nullptr) {
        // Offered whenever the folder would not open, not only for servers: a
        // local folder can be a mount that has gone away, and trying again is
        // the same answer.
        m_reconnect->setVisible(!errorKey.isEmpty());
        m_reconnect->setText(tr_("pane.reconnect"));
    }
    if (!errorKey.isEmpty()) {
        const QByteArray keyUtf8 = errorKey.toUtf8();
        status = jtfText([&](char *buf, int len) {
            return jtf_tr(m_app, keyUtf8.constData(), buf, len);
        });
        // And what it actually said. Without this every sign-in failure read
        // 「你沒有執行這項操作的權限」, which named the wrong thing - the folder
        // was readable, the sign-in was not - and gave nothing to act on.
        const QString detail =
            jtfText([&](char *b, int l) { return jtf_error_detail(m_app, m_pane, b, l); });
        if (!detail.isEmpty()) {
            status += QStringLiteral("  (") + detail + QLatin1Char(')');
        }
    } else if (jtf_is_loading(m_app, m_pane) && jtf_is_searching(m_app, m_pane)) {
        status = jtfFill(tr_("status.searching"), "count",
                         QString::number(jtf_listed_count(m_app, m_pane)));
    } else if (jtf_is_loading(m_app, m_pane)) {
        status = tr_("status.loading");
    } else if (jtf_is_searching(m_app, m_pane)) {
        status = jtfFill(tr_("status.results"), "count",
                         QString::number(jtf_listed_count(m_app, m_pane)));
    } else {
        // Items, not rows: a `..` row is a way out of the folder, not a file in it.
    const int rows = jtf_listed_count(m_app, m_pane);
        const int marked = jtf_marked_count(m_app, m_pane);
        const int total = jtf_unfiltered_count(m_app, m_pane);
        // With a filter on, say how many of how many. Showing only the
        // filtered count makes a directory look empty when it is not.
        if (m_filterBar->isVisible() && !m_filter->text().isEmpty()) {
            status = jtfFill(jtfFill(tr_("status.filtered"), "count", QString::number(rows)),
                             "total", QString::number(total));
        } else {
            // "28 items, 3 folders" - the folder count is what tells you at a
            // glance whether you are in a directory of work or a directory of
            // directories, and the reference layout shows it for that reason.
            const int folders = jtf_folder_count(m_app, m_pane);
            if (folders == 1) {
                status = jtfFill(tr_("status.items_one_folder"), "count", QString::number(rows));
            } else if (folders > 1) {
                status = jtfFill(jtfFill(tr_("status.items_folders"), "count",
                                         QString::number(rows)),
                                 "folders", QString::number(folders));
            } else {
                status = jtfFill(tr_("status.items"), "count", QString::number(rows));
            }
            const quint64 listed = jtf_visible_bytes(m_app, m_pane);
            if (listed > 0) {
                status += QStringLiteral("   ") + formatSize(listed);
            }
        }
        // One count, not two. Selection and marks are kept in separate stores
        // inside the model, and the line reported both - 「已選取 10 個
        // 已標記 10 個」, the same fact twice under two names, because
        // AGENTS.md 10 makes selecting a row and marking it the same act. The
        // marks are the set an operation acts on, so that is the one counted.
        if (marked > 0) {
            status += QStringLiteral("   ") +
                      jtfFill(tr_("status.marked"), "count", QString::number(marked));
        }
        // The size of what an operation would act on, which is the number
        // people are actually looking for before they copy something.
        const quint64 bytes = jtf_target_size(m_app, m_pane);
        if (bytes > 0) {
            status += QStringLiteral("   ") +
                      jtfFill(tr_("status.size"), "size", formatSize(bytes));
        }
        // No free-space figure here. It is a property of the disk, not of the
        // folder being looked at, so it said the same thing in every pane on
        // the same volume - and it was the longest thing on the line, for the
        // fact least likely to be wanted.
    }
    // Elided against the room there is, because the label no longer asks for
    // room to fit it.
    const QFontMetrics statusMetrics(m_status->font());
    m_status->setText(
        statusMetrics.elidedText(status, Qt::ElideRight, qMax(0, m_status->width() - 4)));
    m_status->setToolTip(status);

    // The overlay says the same thing the status line does, in the place the
    // eye is actually looking, and adds the way out. Shown while a search is
    // running and while its results stand, so there is always a way back to
    // the folder without emptying the search box by hand.
    if (m_searchOverlay != nullptr) {
        const bool searching = jtf_is_searching(m_app, m_pane) != 0;
        const bool running = searching && jtf_is_loading(m_app, m_pane) != 0;
        m_searchOverlay->setVisible(searching);
        if (searching) {
            const int found = jtf_listed_count(m_app, m_pane);
            m_searchOverlay->setState(
                running, found,
                jtfFill(tr_("status.searching"), "count", QString::number(found)),
                jtfFill(tr_("status.results"), "count", QString::number(found)),
                tr_("search.cancel"));
            positionSearchOverlay();
        }
    }
}

// Repaint every column of one row.
//
// `update(index)` covers one cell, which is not enough for anything drawn
// across a row - the cursor outline is assembled from a segment per cell, so a
// repaint of one cell leaves the rest of the line as it was.
void PaneWidget::repaintRow(const QModelIndex &index) {
    if (!index.isValid() || m_view == nullptr) {
        return;
    }
    const QRect cell = m_view->visualRect(index);
    if (cell.isNull()) {
        return; // scrolled out of sight; it will be drawn correctly when it returns
    }
    m_view->viewport()->update(
        QRect(0, cell.y(), m_view->viewport()->width(), cell.height()));
}

void PaneWidget::applyTheme(const QColor &mark, const QColor &directory, const QColor &dim,
                            const QColor &indicator, const QColor &border,
                            const QColor &executable) {
    // The tick is drawn by the delegates, so it is coloured here with
    // everything else rather than left to the stylesheet.
    if (m_rows != nullptr) {
        m_rows->setTickColour(indicator);
        m_rows->setCursorColour(indicator);
    }
    if (m_matches != nullptr) {
        m_matches->setTickColour(indicator);
        m_matches->setCursorColour(indicator);
    }

    // The badge's arrow follows the mark colour its dashed border uses, so
    // the two say the same thing in the same colour.
    m_targetGlyph = glyph::make(glyph::Shape::ArrowDown, mark).pixmap(12, 12);
    if (m_targetBadge != nullptr) {
        // A wash of the mark colour rather than a line around it: at a tenth
        // of its strength the badge sits behind the word instead of boxing it
        // in, and stops looking like a control.
        // Both states in the one sheet, so the wash cannot outlive the word:
        // an empty tinted pill on every pane that is not the target would be
        // worse than no badge at all.
        m_targetBadge->setStyleSheet(
            QStringLiteral("QWidget#JtfTargetBadge {"
                           "  background: rgba(%1,%2,%3,0.14);"
                           "  border: none; border-radius: 9px; }"
                           "QWidget#JtfTargetBadge[jtfShowing=\"false\"] {"
                           "  background: transparent; }")
                .arg(mark.red())
                .arg(mark.green())
                .arg(mark.blue()));
    }
    if (m_targetIcon != nullptr && !m_targetIcon->pixmap().isNull()) {
        m_targetIcon->setPixmap(m_targetGlyph);
    }

    if (m_searchOverlay != nullptr) {
        m_searchOverlay->applyTheme(palette().color(QPalette::Text), indicator);
    }

    // The close marks are widgets we own, so they are repainted here rather
    // than by the stylesheet.
    m_tabCloseColour = dim;
    m_tabCloseStrong = palette().color(QPalette::Text);
    syncTabCloseButtons();

    m_model->setMarkColor(mark);
    m_model->setDirectoryColor(directory);
    m_model->setExecutableColor(executable);
    m_model->setTextColor(palette().color(QPalette::Text));
    m_indicator = indicator;
    if (m_close != nullptr) {
        m_close->setIcon(glyph::make(glyph::Shape::Close, dim));
        m_close->setIconSize(QSize(kTabCloseIcon, kTabCloseIcon));
    }
    if (m_filterIcon != nullptr) {
        m_filterIcon->setPixmap(
            glyph::make(glyph::Shape::Filter, dim).pixmap(14, 14));
    }
    if (m_filterClose != nullptr) {
        m_filterClose->setIcon(glyph::make(glyph::Shape::Close, dim));
    }
    if (m_crumbs != nullptr) {
        m_crumbs->setLeadingIcon(glyph::make(glyph::Shape::Sidebar, dim).pixmap(14, 14));
    }
    // The sorted column's header is painted in the primary text colour and
    // the rest in the dim one: that contrast is what makes the sorted column
    // findable without hunting for the caret.
    m_header->applyTheme(directory, dim, indicator);
    if (m_matches != nullptr) {
        // The mark colour: the program already uses it to mean "this is what
        // you asked about", and reusing it is one fewer colour to learn.
        m_matches->setHighlight(mark, directory);
    }
    m_border = border;
    setActive(m_active);
    m_model->refresh();
}

void PaneWidget::setTarget(bool target) {
    if (m_targetBadge != nullptr && m_targetWord != nullptr && m_targetIcon != nullptr) {
        const QString word = jtfText(
            [&](char *b, int l) { return jtf_tr(m_app, "pane.target", b, l); });
        // Sized once, from the text it holds when it has something to say, so
        // that having nothing to say costs exactly the same room.
        m_targetWord->setText(word);
        m_targetIcon->setPixmap(m_targetGlyph);
        m_targetBadge->setFixedWidth(m_targetBadge->sizeHint().width());
        // Emptied rather than hidden.
        m_targetWord->setText(target ? word : QString());
        m_targetIcon->setPixmap(target ? m_targetGlyph : QPixmap());
        m_targetBadge->setProperty("jtfShowing", target);
        m_targetBadge->setVisible(true); // always in the layout; see the constructor
        m_targetBadge->style()->unpolish(m_targetBadge);
        m_targetBadge->style()->polish(m_targetBadge);
    }
    // The outline is on the pane itself, so the whole side of the window is
    // marked rather than a word in one corner of it.
    setProperty("jtfTarget", target);
    style()->unpolish(this);
    style()->polish(this);
    update();
}

void PaneWidget::setActive(bool active) {
    m_active = active;
    // Which pane the keyboard is in has to be obvious, or every command is a
    // guess about where it will land. One mark was not enough: a rule along
    // the pane's top edge is easy to miss beside a tab strip that looked
    // identical in both panes, because the *tab* accent said "this tab is
    // current in its pane" and every pane has one of those.
    //
    // So three marks together, and none of them colour alone
    // (docs/UI_UX_SPEC.md 3.1): the pane's own edge, its tab strip lit or
    // dimmed, and the file list's selection drawn active or inactive - which
    // Qt already does for us once the focus is really there.
    for (QWidget *widget : {static_cast<QWidget *>(this), static_cast<QWidget *>(m_tabs),
                            static_cast<QWidget *>(m_crumbs)}) {
        widget->setProperty("jtfActive", active);
        widget->style()->unpolish(widget);
        widget->style()->polish(widget);
    }
    update();
}

void PaneWidget::applyFilterBarSetting() {
    // Turning the setting on shows the bar in every pane; turning it off hides
    // the ones that are empty, and leaves alone any pane actually filtering -
    // hiding the box while its text is still narrowing the list is how a
    // folder ends up looking empty for no visible reason.
    const bool always = jtf_filter_bar_always(m_app) != 0;
    if (always) {
        m_filter->setPlaceholderText(jtfText(
            [&](char *buf, int len) { return jtf_tr(m_app, "filter.placeholder", buf, len); }));
        m_filterBar->setVisible(true);
        return;
    }
    if (m_filter->text().isEmpty()) {
        m_filterBar->setVisible(false);
    }
}
