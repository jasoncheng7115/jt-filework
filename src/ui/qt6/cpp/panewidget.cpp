#include "panewidget.h"
#include "filelistmodel.h"
#include "breadcrumb.h"
#include "headerview.h"
#include "icons.h"
#include "matchdelegate.h"

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
#include <QStorageInfo>
#include <QEvent>
#include <QHeaderView>
#include <QKeyEvent>
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

PaneWidget::~PaneWidget() { delete m_typeAheadClock; }

PaneWidget::PaneWidget(JtfApp *app, int paneId, QWidget *parent)
    : QWidget(parent), m_app(app), m_pane(paneId) {
    setObjectName(QStringLiteral("JtfPane"));
    auto *layout = new QVBoxLayout(this);
    layout->setContentsMargins(0, 0, 0, 0);
    layout->setSpacing(0);

    m_tabs = new QTabBar(this);
    m_tabs->setExpanding(false);
    m_tabs->setTabsClosable(true);
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
    layout->addWidget(tabRow);

    m_crumbs = new Breadcrumb(this);
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
    m_filterBar->setVisible(false);
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
    m_filterClose->setToolTip(jtfText(
        [&](char *b, int l) { return jtf_tr(m_app, "filter.close", b, l); }));
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
    m_search = new QLineEdit(this);
    m_search->setObjectName(QStringLiteral("JtfSearch"));
    m_search->setClearButtonEnabled(true);
    m_search->setVisible(false);
    connect(m_search, &QLineEdit::returnPressed, this, [this] {
        const QByteArray query = m_search->text().trimmed().toUtf8();
        if (query.isEmpty()) {
            clearSearch();
            return;
        }
        char error[128] = {};
        if (!jtf_search_start(m_app, m_pane, query.constData(), error, sizeof(error))) {
            // The query is wrong in a specific way, and saying which is the
            // difference between fixing it and guessing.
            const QByteArray key(error);
            m_status->setText(jtfText(
                [&](char *buf, int len) { return jtf_tr(m_app, key.constData(), buf, len); }));
            return;
        }
        m_model->refresh();
        emit stateChanged();
    });
    layout->addWidget(m_search);

    m_view = new QTableView(this);
    m_model = new FileListModel(app, paneId, this);
    m_view->setModel(m_model);
    m_view->setSelectionBehavior(QAbstractItemView::SelectRows);
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
    // Only the name column: highlighting a date because the query happens to
    // contain a digit would be noise, not information.
    m_matches = new MatchDelegate(this);
    m_view->setItemDelegateForColumn(0, m_matches);
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

    m_status = new QLabel(this);
    m_status->setObjectName(QStringLiteral("JtfStatus"));
    layout->addWidget(m_status);

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
    connect(m_grid, &QAbstractItemView::doubleClicked, this,
            [this](const QModelIndex &index) { openRow(index.row()); });
    layout->addWidget(m_grid, 1);
    connect(m_grid, &QWidget::customContextMenuRequested, this, [this](const QPoint &at) {
        jtf_focus_pane(m_app, m_pane);
        emit contextMenuRequested(m_grid->viewport()->mapToGlobal(at),
                                  m_grid->indexAt(at).isValid());
    });

    m_view->installEventFilter(this);
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
        emit selectionChanged();
    });

    connect(m_view, &QTableView::doubleClicked, this,
            [this](const QModelIndex &index) { openRow(index.row()); });

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
        QMenu menu(this);
        QAction *tearOff = menu.addAction(jtfText(
            [&](char *b, int l) { return jtf_tr(m_app, "tab.tear_off", b, l); }));
        // Only offered when it would do something: the last tab of the last
        // pane cannot become its own window.
        tearOff->setEnabled(m_tabs->count() > 1 || jtf_pane_count(m_app) > 1);
        if (menu.exec(m_tabs->mapToGlobal(at)) == tearOff) {
            emit tearOffRequested(index);
        }
    });
    connect(m_tabs, &QTabBar::tabCloseRequested, this, [this](int index) {
        jtf_close_tab(m_app, m_pane, index);
        emit stateChanged();
    });

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

void PaneWidget::toggleSearch() {
    if (m_search->isVisible() && m_search->hasFocus()) {
        clearSearch();
        return;
    }
    m_search->setVisible(true);
    m_search->setPlaceholderText(jtfText(
        [&](char *buf, int len) { return jtf_tr(m_app, "search.placeholder", buf, len); }));
    m_search->setFocus();
    m_search->selectAll();
}

void PaneWidget::clearSearch() {
    // Clearing returns to the folder the pane was already on, rather than
    // navigating anywhere: a search never moved you.
    if (jtf_is_searching(m_app, m_pane)) {
        jtf_search_clear(m_app, m_pane);
    }
    m_search->clear();
    m_search->setVisible(false);
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

void PaneWidget::clearFilter() {
    // Escape clears and hides, rather than leaving an empty box that still
    // looks like a mode the user is in.
    m_filter->clear();
    m_filterBar->setVisible(false);
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

    for (int column = 0; column < jtf_column_count(); ++column) {
        bool visible = jtf_column_visible(m_app, m_pane, column) != 0;
        if (searching && isPathColumn(column)) {
            visible = true;
        }
        m_view->setColumnHidden(column, !visible);
    }
    fitNameColumn();
}

bool PaneWidget::isPathColumn(int column) const {
    // By key, not by index: the column order is data and has changed once.
    const QString key =
        jtfText([&](char *buf, int len) { return jtf_column_key(column, buf, len); });
    return key == QLatin1String("column.path");
}

void PaneWidget::fitNameColumn() {
    // Whatever the other visible columns do not use, with a floor. Below the
    // floor the list scrolls sideways, which is the honest outcome: the
    // columns genuinely do not fit, and hiding the file names to pretend they
    // do is not an improvement.
    static constexpr int kNameFloor = 160;
    int used = 0;
    for (int column = 1; column < m_model->columnCount(); ++column) {
        if (!m_view->isColumnHidden(column)) {
            used += m_view->columnWidth(column);
        }
    }
    const int available = m_view->viewport()->width() - used;
    m_view->setColumnWidth(0, qMax(kNameFloor, available));
}

namespace {

// How far below the tab strip the pointer must go before a drag means "tear
// this out" rather than "reorder these". Generous, because tearing off by
// accident loses your place.
constexpr int kTearOffDistance = 28;

// The icon edge in the grid. Large enough for a photograph to be recognised,
// small enough that a folder of a thousand files is still navigable.
constexpr int kGridIconEdge = 72;

// The payload a tab drag carries. Ours alone, so a drop from anywhere else is
// not mistaken for a tab.
constexpr const char *kTabMimeType = "application/x-jt-filework-tab";

// What the drop should do, from the action Qt resolved out of the platform's
// own modifier conventions. Respecting Qt here is what makes Option-drag mean
// copy on macOS and Ctrl-drag mean copy elsewhere without this code knowing
// which platform it is on.
int dropKind(Qt::DropAction action) {
    return action == Qt::CopyAction ? 0 : 1; // ops::Copy : ops::Move
}

} // namespace

int PaneWidget::currentRow() const {
    const QModelIndex current = m_view->currentIndex();
    return current.isValid() ? current.row() : -1;
}

void PaneWidget::advanceCurrentRow() {
    const int next = qMin(currentRow() + 1, m_model->rowCount() - 1);
    if (next >= 0) {
        m_view->setCurrentIndex(m_model->index(next, 0));
    }
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
    emit dropRequested(paths, dropKind(event->dropAction()));
    return true;
}

void PaneWidget::resizeEvent(QResizeEvent *event) {
    QWidget::resizeEvent(event);
    fitNameColumn();
}

bool PaneWidget::eventFilter(QObject *watched, QEvent *event) {
    if (event->type() == QEvent::FocusIn || event->type() == QEvent::MouseButtonPress) {
        emit focusRequested(m_pane);
    }

    switch (event->type()) {
    case QEvent::DragEnter:
    case QEvent::DragMove: {
        auto *drag = static_cast<QDragMoveEvent *>(event);
        if (drag->mimeData()->hasUrls()) {
            drag->acceptProposedAction();
            return true;
        }
        return false;
    }
    case QEvent::Drop: {
        auto *drop = static_cast<QDropEvent *>(event);
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

    if (event->type() == QEvent::KeyPress && (watched == m_filter || watched == m_search)) {
        auto *key = static_cast<QKeyEvent *>(event);
        switch (key->key()) {
        case Qt::Key_Tab:
        case Qt::Key_Return:
        case Qt::Key_Enter:
        case Qt::Key_Down:
            focusList();
            if (m_view->currentIndex().isValid()) {
                return true;
            }
            ensureCurrentRow();
            return true;
        default:
            break;
        }
    }

    if (event->type() == QEvent::KeyPress && watched == m_view) {
        auto *key = static_cast<QKeyEvent *>(event);
        switch (key->key()) {
        case Qt::Key_Return:
        case Qt::Key_Enter: {
            const QModelIndex current = m_view->currentIndex();
            if (current.isValid()) {
                openRow(current.row());
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
            if (m_search->isVisible() || jtf_is_searching(m_app, m_pane)) {
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
            if (key->modifiers() != Qt::NoModifier) {
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
    m_crumbs->setPath(
        jtfText([&](char *buf, int len) { return jtf_current_path(m_app, m_pane, buf, len); }));
}

void PaneWidget::refresh() {
    syncTabs();
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
    retranslate();
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

void PaneWidget::setListFont(const QFont &font) {
    m_view->setFont(font);
    m_view->horizontalHeader()->setFont(font);
    // Row height follows the font, or descenders clip and the list looks
    // cramped at larger sizes.
    // Generous rather than tight: the reference layouts get their calm from
    // row height more than from anything else, and a list at the minimum
    // legible spacing is the single thing that makes an interface look cheap.
    const int rowHeight = QFontMetrics(font).height() + 12;
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

    QString status;
    const QString errorKey =
        jtfText([&](char *buf, int len) { return jtf_error_key(m_app, m_pane, buf, len); });
    if (!errorKey.isEmpty()) {
        const QByteArray keyUtf8 = errorKey.toUtf8();
        status = jtfText([&](char *buf, int len) {
            return jtf_tr(m_app, keyUtf8.constData(), buf, len);
        });
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
        const int selected = jtf_selection_count(m_app, m_pane);
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
        // Selection and marks are different things and are counted
        // separately, because conflating them is exactly what AGENTS.md 10
        // forbids in the model.
        if (selected > 0) {
            status += QStringLiteral("   ") +
                      jtfFill(tr_("status.selected"), "count", QString::number(selected));
        }
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
        // Free space comes from Qt, which asks the platform, exactly as the
        // file icons do. It moves to the platform adapter with the rest of
        // the native services (docs/PLATFORM_INTEGRATION.md 1).
        const QString here =
            jtfText([&](char *b, int l) { return jtf_current_path(m_app, m_pane, b, l); });
        const QStorageInfo storage(here);
        if (storage.isValid() && storage.bytesAvailable() > 0) {
            status += QStringLiteral("   ") +
                      jtfFill(tr_("status.free"),
                              "size",
                              formatSize(static_cast<quint64>(storage.bytesAvailable())));
        }
    }
    m_status->setText(status);
}

void PaneWidget::applyTheme(const QColor &mark, const QColor &directory, const QColor &dim,
                            const QColor &indicator, const QColor &border,
                            const QColor &executable) {
    m_model->setMarkColor(mark);
    m_model->setDirectoryColor(directory);
    m_model->setExecutableColor(executable);
    m_indicator = indicator;
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
