#include "panewidget.h"
#include "filelistmodel.h"
#include "jtfstring.h"

#include <QEvent>
#include <QHeaderView>
#include <QKeyEvent>
#include <QLabel>
#include <QTabBar>
#include <QTableView>
#include <QVBoxLayout>

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

    m_path = new QLabel(this);
    m_path->setObjectName(QStringLiteral("JtfPath"));
    m_path->setTextInteractionFlags(Qt::TextSelectableByMouse);
    layout->addWidget(m_path);

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
    m_view->horizontalHeader()->setStretchLastSection(false);
    m_view->setHorizontalScrollMode(QAbstractItemView::ScrollPerPixel);
    m_view->setVerticalScrollMode(QAbstractItemView::ScrollPerPixel);
    m_view->setEditTriggers(QAbstractItemView::NoEditTriggers);
    layout->addWidget(m_view, 1);

    m_status = new QLabel(this);
    m_status->setObjectName(QStringLiteral("JtfStatus"));
    layout->addWidget(m_status);

    m_view->installEventFilter(this);
    m_view->viewport()->installEventFilter(this);
    m_tabs->installEventFilter(this);

    connect(m_view, &QTableView::doubleClicked, this,
            [this](const QModelIndex &index) { openRow(index.row()); });

    connect(m_view->horizontalHeader(), &QHeaderView::sectionClicked, this,
            [this](int section) {
                jtf_sort_by(m_app, m_pane, section);
                m_model->refresh();
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
    m_view->setColumnWidth(0, 330);
    m_view->setColumnWidth(1, 92);
    m_view->setColumnWidth(2, 128);
    m_view->setColumnWidth(3, 168);
    m_view->horizontalHeader()->setMinimumSectionSize(56);
}

void PaneWidget::openRow(int row) {
    if (jtf_open_row(m_app, m_pane, row)) {
        emit stateChanged();
    }
}

bool PaneWidget::eventFilter(QObject *watched, QEvent *event) {
    if (event->type() == QEvent::FocusIn || event->type() == QEvent::MouseButtonPress) {
        emit focusRequested(m_pane);
    }

    if (event->type() == QEvent::KeyPress && watched == m_view) {
        auto *key = static_cast<QKeyEvent *>(event);
        switch (key->key()) {
        case Qt::Key_Space: {
            // Space marks. Marking is not selecting (AGENTS.md 10), so this
            // must not disturb the view's own selection.
            const QModelIndex current = m_view->currentIndex();
            if (current.isValid()) {
                jtf_toggle_mark(m_app, m_pane, current.row());
                m_model->refresh();
                const int next = qMin(current.row() + 1, m_model->rowCount() - 1);
                m_view->setCurrentIndex(m_model->index(next, 0));
                emit stateChanged();
            }
            return true;
        }
        case Qt::Key_Return:
        case Qt::Key_Enter: {
            const QModelIndex current = m_view->currentIndex();
            if (current.isValid()) {
                openRow(current.row());
            }
            return true;
        }
        case Qt::Key_Backspace:
            jtf_navigate_up(m_app, m_pane);
            emit stateChanged();
            return true;
        default:
            break;
        }
    }
    return QWidget::eventFilter(watched, event);
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

void PaneWidget::syncPath() {
    m_path->setText(
        jtfText([&](char *buf, int len) { return jtf_current_path(m_app, m_pane, buf, len); }));
}

void PaneWidget::refresh() {
    syncTabs();
    syncPath();
    m_model->refresh();
    retranslate();
}

void PaneWidget::retranslate() {
    const auto tr = [&](const char *key) {
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
    } else if (jtf_is_loading(m_app, m_pane)) {
        status = tr("status.loading");
    } else {
        const int rows = jtf_row_count(m_app, m_pane);
        const int marked = jtf_marked_count(m_app, m_pane);
        status = jtfFill(tr("status.items"), "count", QString::number(rows));
        if (marked > 0) {
            status += QStringLiteral("   ") + jtfFill(tr("status.marked"), "count", QString::number(marked));
        }
    }
    m_status->setText(status);
}

void PaneWidget::applyTheme(const QColor &mark, const QColor &directory, const QColor &indicator,
                            const QColor &border) {
    m_model->setMarkColor(mark);
    m_model->setDirectoryColor(directory);
    m_indicator = indicator;
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
