#include "comparewindow.h"

#include "jtfstring.h"
#include "listlook.h"
#include "panewidget.h"

#include <QCheckBox>
#include <QDateTime>
#include <QHBoxLayout>
#include <QHeaderView>
#include <QKeyEvent>
#include <QLabel>
#include <QPushButton>
#include <QTableWidget>
#include <QTimer>
#include <QVBoxLayout>

namespace {
// How often the worker is asked whether it has finished. Often enough that a
// quick comparison feels immediate, rarely enough that a long one costs
// nothing to wait for.
constexpr int kPollMs = 60;

enum Column { ColumnName = 0, ColumnFirst, ColumnSecond, ColumnResult, ColumnCount };

// The two sides, as the FFI numbers them. Called first and second rather than
// left and right: the panes can be split top and bottom, and which of them is
// which side of a comparison has nothing to do with where they sit.
constexpr int kFirst = 0;
constexpr int kSecond = 1;
} // namespace

CompareWindow::CompareWindow(JtfApp *app, int leftPane, int rightPane, QWidget *parent)
    : QWidget(parent, Qt::Window), m_app(app), m_leftPane(leftPane), m_rightPane(rightPane) {
    setAttribute(Qt::WA_DeleteOnClose);
    setWindowTitle(tr_("compare.title"));
    resize(860, 600);

    auto *layout = new QVBoxLayout(this);
    layout->setContentsMargins(0, 0, 0, 0);
    layout->setSpacing(0);

    // What is being compared, said once at the top. Without it the table is a
    // list of names with no answer to "of what".
    m_heading = new QLabel(this);
    m_heading->setObjectName(QStringLiteral("JtfCompareHeading"));
    m_heading->setWordWrap(true);
    layout->addWidget(m_heading);

    auto *options = new QWidget(this);
    options->setObjectName(QStringLiteral("JtfCompareOptions"));
    auto *optionRow = new QHBoxLayout(options);
    optionRow->setContentsMargins(10, 6, 10, 6);
    optionRow->setSpacing(18);

    // Off by default, as asked: comparing the folders you are looking at is
    // the quick question, and walking everything underneath them is the slow
    // one you opt into.
    m_recursive = new QCheckBox(tr_("compare.recursive"), options);
    m_recursive->setChecked(false);
    connect(m_recursive, &QCheckBox::toggled, this, [this] { run(); });
    optionRow->addWidget(m_recursive);

    // Also off: a list where most rows say "identical" buries the handful
    // that do not, which are the only reason to open this window.
    m_showSame = new QCheckBox(tr_("compare.show_same"), options);
    m_showSame->setChecked(false);
    connect(m_showSame, &QCheckBox::toggled, this, [this] { fill(); });
    optionRow->addWidget(m_showSame);
    optionRow->addStretch(1);
    layout->addWidget(options);

    m_table = new QTableWidget(this);
    m_table->setColumnCount(ColumnCount);
    // Column headings are filled in once the two folders are known; they name
    // the folders themselves.
    m_table->setHorizontalHeaderLabels({tr_("compare.column.name"), QString(), QString(),
                                        tr_("compare.column.result")});
    // The same look as the pane's file list, and as the archive window: these
    // are all lists of entries, and three different-looking tables read as
    // three different programs.
    listlook::apply(m_table, font());
    listlook::applyTheme(m_table, palette().color(QPalette::Text),
                         palette().color(QPalette::PlaceholderText),
                         palette().color(QPalette::HighlightedText));
    m_table->setSelectionMode(QAbstractItemView::SingleSelection);
    // Headings sit over their own columns: the name reads from the left like
    // its cells, the two side columns from the right like the figures in them.
    for (int column = 0; column < ColumnCount; ++column) {
        if (auto *heading = m_table->horizontalHeaderItem(column)) {
            heading->setTextAlignment((column == ColumnFirst || column == ColumnSecond)
                                          ? (Qt::AlignRight | Qt::AlignVCenter)
                                          : (Qt::AlignLeft | Qt::AlignVCenter));
        }
    }
    m_table->horizontalHeader()->setSectionResizeMode(ColumnName, QHeaderView::Stretch);
    for (const int column : {ColumnFirst, ColumnSecond, ColumnResult}) {
        m_table->horizontalHeader()->setSectionResizeMode(column, QHeaderView::Fixed);
        m_table->horizontalHeader()->resizeSection(column, column == ColumnResult ? 120 : 170);
    }
    layout->addWidget(m_table, 1);

    // The status line and, while a walk is running, the way to stop it.
    auto *statusRow = new QWidget(this);
    statusRow->setObjectName(QStringLiteral("JtfStatusRow"));
    auto *statusLayout = new QHBoxLayout(statusRow);
    statusLayout->setContentsMargins(0, 0, 0, 0);
    statusLayout->setSpacing(0);
    m_status = new QLabel(statusRow);
    m_status->setObjectName(QStringLiteral("JtfStatus"));
    statusLayout->addWidget(m_status, 1);
    m_cancel = new QPushButton(tr_("operation.cancel"), statusRow);
    m_cancel->setVisible(false);
    connect(m_cancel, &QPushButton::clicked, this, [this] {
        // Stopped, not abandoned: the walk drops out at the next folder and
        // what it has already found is still shown. A comparison you had to
        // give up on halfway is still worth reading.
        jtf_compare_cancel(m_app);
    });
    statusLayout->addWidget(m_cancel);
    layout->addWidget(statusRow);

    m_poll = new QTimer(this);
    m_poll->setInterval(kPollMs);
    connect(m_poll, &QTimer::timeout, this, [this] { poll(); });

    run();
}

CompareWindow::~CompareWindow() { jtf_compare_close(m_app); }

QString CompareWindow::shortName(const QString &path) {
    const QString trimmed = path.endsWith(QLatin1Char('/')) && path.size() > 1
                                ? path.left(path.size() - 1)
                                : path;
    const int slash = trimmed.lastIndexOf(QLatin1Char('/'));
    return slash >= 0 && slash + 1 < trimmed.size() ? trimmed.mid(slash + 1) : trimmed;
}

QString CompareWindow::tr_(const char *key) const {
    return jtfText([&](char *buf, int len) { return jtf_tr(m_app, key, buf, len); });
}

void CompareWindow::run() {
    // Any comparison already running is this window's own, and it is now
    // answering a question nobody asked. Closed before starting the next, so
    // the two never overlap.
    jtf_compare_close(m_app);
    m_table->setRowCount(0);
    if (jtf_compare_start(m_app, m_leftPane, m_rightPane, m_recursive->isChecked() ? 1 : 0) == 0) {
        m_status->setText(tr_("compare.failed"));
        return;
    }
    m_firstPath = jtfText([&](char *b, int l) { return jtf_compare_left(m_app, b, l); });
    m_secondPath = jtfText([&](char *b, int l) { return jtf_compare_right(m_app, b, l); });
    // The folders' own names, which is how a person refers to them - unless
    // both are called the same thing, in which case only the full paths tell
    // them apart and the names would be worse than useless.
    m_firstName = shortName(m_firstPath);
    m_secondName = shortName(m_secondPath);
    if (m_firstName == m_secondName) {
        m_firstName = m_firstPath;
        m_secondName = m_secondPath;
    }
    m_heading->setText(tr_("compare.first") + QStringLiteral(": ") + m_firstPath
                       + QStringLiteral("\n") + tr_("compare.second") + QStringLiteral(": ")
                       + m_secondPath);
    if (auto *heading = m_table->horizontalHeaderItem(ColumnFirst)) {
        heading->setText(jtfFill(tr_("compare.column.first"), "name", m_firstName));
    }
    if (auto *heading = m_table->horizontalHeaderItem(ColumnSecond)) {
        heading->setText(jtfFill(tr_("compare.column.second"), "name", m_secondName));
    }
    m_status->setText(tr_("compare.running"));
    m_cancel->setVisible(true);
    m_poll->start();
}

void CompareWindow::poll() {
    const bool moved = jtf_pump_compare(m_app) != 0;
    const int state = jtf_compare_state(m_app);
    if (state == 0) {
        // Still walking. Saying how far it has got is the difference between
        // a window that is working and a window that has hung - they look the
        // same from outside, and only one of them is worth waiting for.
        if (moved) {
            m_status->setText(
                jtfFill(jtfFill(jtfFill(tr_("compare.progress"), "folders",
                                        QString::number(jtf_compare_folders_done(m_app))),
                                "rows", QString::number(jtf_compare_rows_so_far(m_app))),
                        "differences",
                        QString::number(jtf_compare_differences_so_far(m_app))));
        }
        return;
    }
    m_poll->stop();
    m_cancel->setVisible(false);
    if (state == 1) {
        fill();
    } else {
        m_table->setRowCount(0);
        const QString detail =
            jtfText([&](char *b, int l) { return jtf_compare_error(m_app, b, l); });
        m_status->setText(detail.isEmpty()
                              ? tr_("compare.failed")
                              : tr_("compare.failed") + QStringLiteral("  (") + detail
                                    + QLatin1Char(')'));
    }
    emit stateChanged();
}

void CompareWindow::fill() {
    const bool showSame = m_showSame->isChecked();
    const int total = jtf_compare_row_count(m_app);

    m_table->setUpdatesEnabled(false);
    m_table->setRowCount(0);
    for (int source = 0; source < total; ++source) {
        const QString verdict =
            jtfText([&](char *b, int l) { return jtf_compare_row_difference(m_app, source, b, l); });
        if (!showSame && verdict == QLatin1String("same")) {
            continue;
        }
        const int row = m_table->rowCount();
        m_table->insertRow(row);

        const QString path =
            jtfText([&](char *b, int l) { return jtf_compare_row_path(m_app, source, b, l); });
        m_table->setItem(row, ColumnName, new QTableWidgetItem(path));

        // Each side's facts, or nothing at all when that side does not have
        // the name. An empty cell says "not here" more plainly than a dash or
        // a zero, either of which could be a real value.
        for (const int side : {kFirst, kSecond}) {
            QString text;
            if (jtf_compare_row_has_side(m_app, source, side) != 0) {
                const long long size = jtf_compare_row_size(m_app, source, side);
                const long long when = jtf_compare_row_time(m_app, source, side);
                if (jtf_compare_row_is_directory(m_app, source) != 0) {
                    // A folder has no size worth showing, so the time carries
                    // the column on its own.
                    text = when > 0
                               ? QDateTime::fromSecsSinceEpoch(when).toString(
                                     QStringLiteral("yyyy-MM-dd HH:mm"))
                               : QString();
                } else {
                    text = size >= 0 ? PaneWidget::formatSize(static_cast<quint64>(size))
                                     : QString();
                    if (when > 0) {
                        text += QStringLiteral("  ")
                                + QDateTime::fromSecsSinceEpoch(when).toString(
                                    QStringLiteral("yyyy-MM-dd HH:mm"));
                    }
                }
            }
            auto *item = new QTableWidgetItem(text);
            item->setTextAlignment(Qt::AlignRight | Qt::AlignVCenter);
            m_table->setItem(row, side == kFirst ? ColumnFirst : ColumnSecond, item);
        }

        // The verdict is looked up by the name Rust gave it, so the two ends
        // cannot drift the way a shared integer would. `only_left` and
        // `only_right` are the walk's own words for its two arguments; here
        // they become the folders' names, because「只在左側」is wrong the
        // moment the panes are stacked rather than side by side.
        QString result;
        if (verdict == QLatin1String("only_left")) {
            result = jtfFill(tr_("compare.only_first"), "name", m_firstName);
        } else if (verdict == QLatin1String("only_right")) {
            result = jtfFill(tr_("compare.only_second"), "name", m_secondName);
        } else {
            result = tr_(("compare." + verdict).toUtf8().constData());
        }
        m_table->setItem(row, ColumnResult, new QTableWidgetItem(result));
    }
    m_table->setUpdatesEnabled(true);

    if (m_table->rowCount() > 0) {
        m_table->selectRow(0);
    }
    updateStatus();
    m_table->setFocus();
}

void CompareWindow::updateStatus() {
    const int total = jtf_compare_row_count(m_app);
    const int differences = jtf_compare_difference_count(m_app);

    QString status;
    if (differences == 0) {
        status = tr_("compare.none");
    } else {
        status = jtfFill(jtfFill(tr_("compare.summary"), "differences",
                                 QString::number(differences)),
                         "total", QString::number(total));
    }
    // What "the same" meant here, said next to the answer rather than left to
    // be assumed. Two files with matching size and time are not certainly
    // identical, and the window must not imply that they are.
    status += QStringLiteral("   ") + tr_("compare.rule");
    if (jtf_compare_truncated(m_app) != 0) {
        status += QStringLiteral("   ") + tr_("compare.truncated");
    }
    m_status->setText(status);
}

void CompareWindow::keyPressEvent(QKeyEvent *event) {
    if (event->key() == Qt::Key_Escape) {
        close();
        return;
    }
    QWidget::keyPressEvent(event);
}
