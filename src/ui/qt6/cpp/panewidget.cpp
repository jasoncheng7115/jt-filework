#include "panewidget.h"
#include "filelistmodel.h"
#include "breadcrumb.h"
#include "headerview.h"
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
#include <QTabBar>
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
    layout->addWidget(m_tabs);

    m_crumbs = new Breadcrumb(this);
    connect(m_crumbs, &Breadcrumb::navigate, this, [this](const QString &path) {
        const QByteArray utf8 = path.toUtf8();
        jtf_navigate(m_app, m_pane, utf8.constData());
        emit stateChanged();
    });
    layout->addWidget(m_crumbs);

    // Filtering narrows what is already listed. It is instant because it
    // touches no disk, which is what separates it from search
    // (docs/SEARCH_AI.md 1) and why it belongs in the pane rather than in a
    // dialog.
    m_filter = new QLineEdit(this);
    m_filter->setObjectName(QStringLiteral("JtfFilter"));
    m_filter->setClearButtonEnabled(true);
    m_filter->setVisible(false);
    connect(m_filter, &QLineEdit::textChanged, this, [this](const QString &text) {
        const QByteArray utf8 = text.toUtf8();
        jtf_set_filter(m_app, m_pane, utf8.constData());
        m_model->refresh();
        retranslate();
    });
    layout->addWidget(m_filter);

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
    m_view->horizontalHeader()->setStretchLastSection(false);
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
    m_view->setColumnWidth(2, 128);
    m_view->setColumnWidth(3, 168);
    m_view->horizontalHeader()->setMinimumSectionSize(56);
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
        const QString text = key->text();
        if (text.size() != 1 || !text.at(0).isLetterOrNumber()) {
            return {};
        }
        name = text.toLower();
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
    if (m_filter->isVisible() && m_filter->hasFocus()) {
        clearFilter();
        return;
    }
    m_filter->setVisible(true);
    m_filter->setFocus();
    m_filter->selectAll();
}

void PaneWidget::clearFilter() {
    // Escape clears and hides, rather than leaving an empty box that still
    // looks like a mode the user is in.
    m_filter->clear();
    m_filter->setVisible(false);
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
    for (int column = 0; column < jtf_column_count(); ++column) {
        m_view->setColumnHidden(column, jtf_column_visible(m_app, m_pane, column) == 0);
    }
}

namespace {

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
            if (m_filter->isVisible()) {
                clearFilter();
                return true;
            }
            m_typeAhead.clear();
            return true;

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
                // No binding and no type-ahead: swallow it rather than let
                // the view treat a letter as cursor movement.
                if (!key->text().isEmpty() && key->text().at(0).isPrint()) {
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
    m_tabs->setVisible(count > 1);
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
    m_model->refresh();
    retranslate();
}

void PaneWidget::refreshRows() {
    m_model->refresh();
    retranslate();
}

void PaneWidget::setListFont(const QFont &font) {
    m_view->setFont(font);
    m_view->horizontalHeader()->setFont(font);
    // Row height follows the font, or descenders clip and the list looks
    // cramped at larger sizes.
    const int rowHeight = QFontMetrics(font).height() + 6;
    m_view->verticalHeader()->setDefaultSectionSize(qMax(20, rowHeight));

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
        if (m_filter->isVisible() && !m_filter->text().isEmpty()) {
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
                            const QColor &indicator,
                            const QColor &border) {
    m_model->setMarkColor(mark);
    m_model->setDirectoryColor(directory);
    m_indicator = indicator;
    // The sorted column's header is painted in the primary text colour and
    // the rest in the dim one: that contrast is what makes the sorted column
    // findable without hunting for the caret.
    m_header->applyTheme(directory, dim, indicator);
    m_border = border;
    setActive(m_active);
    m_model->refresh();
}

void PaneWidget::setActive(bool active) {
    m_active = active;
    // The active pane must be identifiable at a glance in both themes and
    // without relying on colour alone (docs/UI_UX_SPEC.md 3.1). A coloured
    // rule along the top edge reads instantly and, unlike a full border, does
    // not steal a pixel of list width when it appears.
    const QColor colour = active ? m_indicator : m_border;
    setStyleSheet(QStringLiteral("QWidget#JtfPane { border-top: %1px solid %2; "
                                 "border-right: 1px solid %3; }")
                      .arg(active ? 2 : 1)
                      .arg(colour.name(QColor::HexRgb))
                      .arg(m_border.name(QColor::HexRgb)));
}
