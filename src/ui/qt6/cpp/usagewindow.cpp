#include "usagewindow.h"

#include "jtfstring.h"
#include "iconprovider.h"
#include <QToolButton>
#include <QMenu>
#include "icons.h"
#include "platform/filetype.h"
#include <QFileInfo>
#include "destinationdialog.h"
#include "listlook.h"
#include "operations.h"
#include "searchoverlay.h" // for Spinner, which already does this job
#include "panewidget.h"

#include <QFontDatabase>
#include <QHBoxLayout>
#include <QHeaderView>
#include <QKeyEvent>
#include <QLabel>
#include <QPainter>
#include <QStyledItemDelegate>
#include <QPushButton>
#include <QSplitter>
#include <QTableWidget>
#include <QTimer>
#include <QVBoxLayout>

namespace {
/// How often the worker is asked whether it has finished.
constexpr int kPollMs = 100;

enum Column { ColumnName = 0, ColumnShare, ColumnSize, ColumnFiles, ColumnCount };

/// The row's share of the total, carried on the size cell so the bar can be
/// drawn from it.
constexpr int kShareRole = Qt::UserRole + 1;
/// Whether the row is a folder, and so can be descended into.
constexpr int kIsFolderRole = Qt::UserRole + 4;
/// The row's raw byte count, so sorting compares numbers rather than the
/// formatted text - "9.9 MB" must not sort above "1.2 GB".
constexpr int kBytesRole = Qt::UserRole + 2;
/// The raw file count, for the same reason.
constexpr int kFilesRole = Qt::UserRole + 3;

/// Gives the name column room between its icon and its text.
///
/// Qt sets the two side by side with the style's own spacing, which on a dense
/// list is a pixel or two - the name reads as if it were stuck to the icon.
class NameDelegate : public QStyledItemDelegate {
public:
    using QStyledItemDelegate::QStyledItemDelegate;

    void paint(QPainter *painter, const QStyleOptionViewItem &option,
               const QModelIndex &index) const override {
        QStyleOptionViewItem wider(option);
        initStyleOption(&wider, index);
        // A decoration slot wider than the icon it holds. Qt centres the icon
        // in the slot and starts the text after it, so this is a gap rather
        // than an indent - adjusting the whole rect would have moved the icon
        // along with the text and changed nothing between them.
        wider.decorationSize.setWidth(wider.decorationSize.width() + kNameGap);
        QStyledItemDelegate::paint(painter, wider, index);
    }

private:
    static constexpr int kNameGap = 8;
};

/// A cell that sorts by a number it carries rather than by its own text.
///
/// `QTableWidgetItem` keeps `DisplayRole` and `EditRole` in the same slot, so
/// stashing a sort key in `EditRole` overwrites the text - which is how the
/// size column came to show raw byte counts. The number lives in a role of its
/// own and only `operator<` reads it.
class NumericItem : public QTableWidgetItem {
public:
    NumericItem(const QString &text, quint64 value)
        : QTableWidgetItem(text), m_value(value) {}
    explicit NumericItem(quint64 value) : m_value(value) {}

    bool operator<(const QTableWidgetItem &other) const override {
        const auto *numeric = dynamic_cast<const NumericItem *>(&other);
        return numeric != nullptr ? m_value < numeric->m_value
                                  : QTableWidgetItem::operator<(other);
    }

private:
    quint64 m_value = 0;
};

/// Draws a bar showing how one row compares with the largest row in the list.
///
/// Against the biggest row, not against the whole disc: this column is for
/// reading the ranking off at a glance, and scaling by a total that is mostly
/// made up of everything *not* listed would leave every bar a stub.
///
/// Painted rather than a widget per row. A folder with two hundred children
/// would otherwise mean two hundred progress bars; painting costs nothing per
/// row and cannot be left behind when the table is refilled.
class ShareBarDelegate : public QStyledItemDelegate {
public:
    using QStyledItemDelegate::QStyledItemDelegate;

    void setBarColour(const QColor &colour) { m_bar = colour; }

    void paint(QPainter *painter, const QStyleOptionViewItem &option,
               const QModelIndex &index) const override {
        // The cell's own background and selection, without any text: the
        // number lives in the next column.
        QStyleOptionViewItem plain(option);
        initStyleOption(&plain, index);
        plain.text.clear();
        QStyledItemDelegate::paint(painter, plain, index);

        const double share = index.data(kShareRole).toDouble();
        if (share <= 0.0) {
            return;
        }
        QRect track(option.rect.left() + 6, option.rect.center().y() - 4,
                    option.rect.width() - 12, 8);
        if (track.width() <= 0) {
            return;
        }
        painter->save();
        painter->setRenderHint(QPainter::Antialiasing, true);
        // On the selected row the bar is drawn in the selection's text colour:
        // the accent it normally uses *is* the selection background there, so
        // the bar on the current row - usually the biggest one - vanished.
        QColor bar = m_bar;
        if (option.state.testFlag(QStyle::State_Selected)) {
            bar = option.palette.color(QPalette::HighlightedText);
        }
        QColor behind = bar;
        behind.setAlphaF(0.16F);
        painter->setPen(Qt::NoPen);
        painter->setBrush(behind);
        painter->drawRoundedRect(track, 3, 3);
        // Filled from the right, so every bar ends against the size column
        // beside it. The numbers are right-aligned too, and a bar that grew
        // away from its own number left the two ends of one fact at opposite
        // sides of the row.
        //
        // At least a sliver, so a row that is present but tiny is still
        // visibly present rather than an empty cell.
        const int width = qMax(2, static_cast<int>(share * track.width()));
        QRect filled(track.right() - width + 1, track.top(), width, track.height());
        painter->setBrush(bar);
        painter->drawRoundedRect(filled, 3, 3);
        painter->restore();
    }

private:
    QColor m_bar;
};
} // namespace

UsageWindow::UsageWindow(JtfApp *app, const QString &path, QWidget *parent)
    : QWidget(parent, Qt::Window), m_app(app), m_path(path) {
    setAttribute(Qt::WA_DeleteOnClose);
    setWindowTitle(tr_("usage.title"));
    resize(920, 620);

    auto *layout = new QVBoxLayout(this);
    layout->setContentsMargins(0, 0, 0, 0);
    layout->setSpacing(0);

    auto *headingRow = new QWidget(this);
    auto *headingLayout = new QHBoxLayout(headingRow);
    headingLayout->setContentsMargins(0, 0, 8, 0);
    headingLayout->setSpacing(6);
    m_up = new QToolButton(headingRow);
    m_up->setObjectName(QStringLiteral("JtfUsageUp"));
    m_up->setAutoRaise(true);
    m_up->setFocusPolicy(Qt::NoFocus);
    m_up->setIcon(glyph::make(glyph::Shape::ArrowUp, palette().color(QPalette::Text)));
    m_up->setToolTip(tr_("nav.up"));
    m_up->setVisible(false);
    connect(m_up, &QToolButton::clicked, this, [this] { goUp(); });
    headingLayout->addWidget(m_up);
    m_heading = new QLabel(headingRow);
    m_heading->setObjectName(QStringLiteral("JtfCompareHeading"));
    m_heading->setWordWrap(true);
    headingLayout->addWidget(m_heading, 1);
    layout->addWidget(headingRow);

    // Side by side, and draggable: which half matters depends on what you are
    // looking for, and that is the user's call rather than a fixed ratio.
    auto *split = new QSplitter(Qt::Horizontal, this);
    split->setChildrenCollapsible(false);
    split->setHandleWidth(7);

    const auto makeTable = [this](const char *headingKey) {
        auto *host = new QWidget(this);
        auto *column = new QVBoxLayout(host);
        column->setContentsMargins(0, 0, 0, 0);
        column->setSpacing(0);
        auto *caption = new QLabel(tr_(headingKey), host);
        caption->setObjectName(QStringLiteral("JtfSidebarTitle"));
        column->addWidget(caption);

        auto *table = new QTableWidget(host);
        table->setColumnCount(ColumnCount);
        table->setHorizontalHeaderLabels({tr_("usage.column.name"), tr_("usage.column.share"),
                                          tr_("usage.column.size"), tr_("usage.column.files")});
        listlook::apply(table, font());
        listlook::applyTheme(table, palette().color(QPalette::Text),
                             palette().color(QPalette::PlaceholderText),
                             palette().color(QPalette::HighlightedText));
        table->setSelectionMode(QAbstractItemView::SingleSelection);
        table->horizontalHeader()->setSectionResizeMode(ColumnName, QHeaderView::Stretch);
        // Interactive, not Fixed. `Fixed` sets the width *and* refuses the
        // drag, so the columns could not be resized at all - the starting
        // widths below are a starting point, not a decision the window gets to
        // keep making for the person looking at it.
        table->horizontalHeader()->setSectionResizeMode(ColumnShare, QHeaderView::Interactive);
        // Narrow: the bar is read as a length against its neighbours, and it
        // does that just as well in 70 pixels. The names are what actually
        // need the room - they are the answer people came for.
        table->horizontalHeader()->resizeSection(ColumnShare, 70);
        table->horizontalHeader()->setSectionResizeMode(ColumnSize, QHeaderView::Interactive);
        table->horizontalHeader()->resizeSection(ColumnSize, 100);
        table->horizontalHeader()->setSectionResizeMode(ColumnFiles, QHeaderView::Interactive);
        table->horizontalHeader()->resizeSection(ColumnFiles, 80);
        table->verticalHeader()->setDefaultSectionSize(26);

        // Sortable by any column. The list arrives largest-first, which is the
        // answer most of the time - but "which folder has the most *files*" and
        // "what is this called" are real questions too, and a column heading
        // that cannot be clicked is a heading people click anyway.
        table->setSortingEnabled(true);
        table->horizontalHeader()->setSectionsClickable(true);
        table->horizontalHeader()->setSortIndicatorShown(true);

        table->setItemDelegateForColumn(ColumnName, new NameDelegate(table));

        auto *bars = new ShareBarDelegate(table);
        bars->setBarColour(palette().color(QPalette::Highlight));
        table->setItemDelegateForColumn(ColumnShare, bars);
        for (const int at : {ColumnSize, ColumnFiles}) {
            if (auto *heading = table->horizontalHeaderItem(at)) {
                heading->setTextAlignment(Qt::AlignRight | Qt::AlignVCenter);
            }
        }
        column->addWidget(table, 1);
        return std::make_pair(host, table);
    };

    auto [folderHost, folderTable] = makeTable("usage.by_folder");
    m_folders = folderTable;
    auto [kindHost, kindTable] = makeTable("usage.by_kind");
    m_kinds = kindTable;
    split->addWidget(folderHost);
    split->addWidget(kindHost);
    split->setSizes({520, 400});
    layout->addWidget(split, 1);

    // Going *into* the big branch is the point of having found it: the next
    // question is always "and what inside there is big". Showing it in the
    // pane is still offered, from the row's own menu.
    connect(m_folders, &QTableWidget::itemDoubleClicked, this,
            [this](QTableWidgetItem *item) {
                if (item != nullptr) {
                    descendTo(folderAt(item->row()));
                }
            });
    m_folders->setContextMenuPolicy(Qt::CustomContextMenu);
    connect(m_folders, &QWidget::customContextMenuRequested, this,
            [this](const QPoint &at) { showRowMenu(at); });

    // The keys this window answers to, above its status line, the way the
    // file list and the viewer have them. It stopped being a report you can
    // only read the moment it grew C, M and D, and a window whose keys are
    // not on screen is a window nobody knows has any.
    auto *hints = new QWidget(this);
    hints->setObjectName(QStringLiteral("JtfUsageHints"));
    auto *hintRow = new QHBoxLayout(hints);
    hintRow->setContentsMargins(10, 5, 10, 5);
    hintRow->setSpacing(14);
    {
        struct Hint {
            const char *key;
            const char *label;
        };
        static const Hint kHints[] = {
            {"\u2192", "usage.hint.into"},   {"\u2190", "usage.hint.up"},
            {"C", "hint.short.file.copy_to"}, {"M", "hint.short.file.move_to"},
            {"D", "hint.short.file.trash"},   {"Tab", "usage.hint.switch"},
            {"Esc", "usage.hint.close"},
        };
        // A keycap is fixed-width, so the chips are too: in proportional type
        // `C` and `Esc` make boxes of wildly different weights and the row
        // stops reading as a row of keys.
        QFont keyFont = QFontDatabase::systemFont(QFontDatabase::FixedFont);
        keyFont.setPointSizeF(font().pointSizeF());
        keyFont.setBold(true);
        for (const Hint &hint : kHints) {
            auto *chip = new QWidget(hints);
            auto *pair = new QHBoxLayout(chip);
            pair->setContentsMargins(0, 0, 0, 0);
            pair->setSpacing(5);
            auto *key = new QLabel(QString::fromUtf8(hint.key), chip);
            key->setProperty("jtfHintKey", true);
            key->setFont(keyFont);
            auto *text = new QLabel(tr_(hint.label), chip);
            text->setProperty("jtfHintLabel", true);
            pair->addWidget(key);
            pair->addWidget(text);
            hintRow->addWidget(chip);
        }
        hintRow->addStretch(1);
    }
    layout->addWidget(hints);

    auto *statusRow = new QWidget(this);
    statusRow->setObjectName(QStringLiteral("JtfStatusRow"));
    auto *statusLayout = new QHBoxLayout(statusRow);
    statusLayout->setContentsMargins(0, 0, 0, 0);
    statusLayout->setSpacing(0);
    // The label carries its own left padding from the stylesheet; the spinner
    // does not, and sat against the window frame without this.
    statusLayout->addSpacing(10);
    m_spinner = new Spinner(statusRow);
    m_spinner->setColour(palette().color(QPalette::Highlight));
    m_spinner->setVisible(false);
    statusLayout->addWidget(m_spinner);
    statusLayout->addSpacing(8);
    m_status = new QLabel(statusRow);
    m_status->setObjectName(QStringLiteral("JtfStatus"));
    statusLayout->addWidget(m_status, 1);
    // Where the walk has got to, at the far end of the line. Its own label
    // rather than more text on the left: the totals change every tick and the
    // path is long, and one label holding both would jump about as the path
    // grew and shrank.
    m_where = new QLabel(statusRow);
    m_where->setObjectName(QStringLiteral("JtfStatus"));
    // Never the reason the window wants to be wider: elided in the middle,
    // where a path can spare it.
    m_where->setSizePolicy(QSizePolicy::Ignored, QSizePolicy::Preferred);
    statusLayout->addWidget(m_where, 2);
    m_cancel = new QPushButton(tr_("operation.cancel"), statusRow);
    m_cancel->setVisible(false);
    // An icon, because every other button in the program has one, and a cross
    // is what this does: it stops the thing that is running.
    m_cancel->setIcon(glyph::make(glyph::Shape::Close, palette().color(QPalette::WindowText)));
    connect(m_cancel, &QPushButton::clicked, this, [this] { jtf_usage_cancel(m_app); });
    statusLayout->addWidget(m_cancel);
    // Off the edge. The status row has no right margin of its own, so the
    // button sat flush against the window frame with nothing between them.
    statusLayout->addSpacing(4);
    layout->addWidget(statusRow);

    // Up and Down belong to the table; Left, Right, Enter, Backspace and Tab
    // belong to this window and would otherwise be eaten as the table's own
    // navigation and type-ahead.
    m_folders->installEventFilter(this);
    m_kinds->installEventFilter(this);
    m_folders->setFocus();

    m_poll = new QTimer(this);
    m_poll->setInterval(kPollMs);
    connect(m_poll, &QTimer::timeout, this, [this] { poll(); });

    run();
}

UsageWindow::~UsageWindow() { jtf_usage_close(m_app); }

QString UsageWindow::tr_(const char *key) const {
    return jtfText([&](char *buf, int len) { return jtf_tr(m_app, key, buf, len); });
}

void UsageWindow::run() {
    jtf_usage_close(m_app);
    m_folders->setRowCount(0);
    m_kinds->setRowCount(0);
    const QByteArray utf8 = m_path.toUtf8();
    if (jtf_usage_start(m_app, utf8.constData()) == 0) {
        m_status->setText(tr_("usage.failed"));
        m_spinner->setVisible(false);
        m_where->clear();
        return;
    }
    m_heading->setText(jtfText([&](char *b, int l) { return jtf_usage_root(m_app, b, l); }));
    m_status->setText(tr_("usage.running"));
    m_spinner->setVisible(true);
    m_cancel->setVisible(true);
    m_poll->start();
}

QIcon UsageWindow::icon(const QString &nameOrPath, bool isFolder) {
    // A kind row has no file to point at - `file.png` does not exist - and
    // `QFileIconProvider` answers about the file on disk, so every sample name
    // came back with the same generic document. The platform can answer about
    // a *type*, and does so here; where it cannot, the ordinary provider's
    // generic icon is still better than none.
    if (!isFolder) {
        const QString extension = QFileInfo(nameOrPath).suffix();
        if (!extension.isEmpty()) {
            auto cached = m_byExtension.constFind(extension);
            if (cached != m_byExtension.constEnd()) {
                return cached.value();
            }
            QIcon fromPlatform = filetype::iconForExtension(extension);
            if (fromPlatform.isNull()) {
                fromPlatform = m_icons.iconFor(nameOrPath, false);
            }
            m_byExtension.insert(extension, fromPlatform);
            return fromPlatform;
        }
    }
    return m_icons.iconFor(nameOrPath, isFolder);
}

QString UsageWindow::folderAt(int row) const {
    QTableWidgetItem *item = m_folders->item(row, ColumnName);
    if (item == nullptr || !item->data(kIsFolderRole).toBool()) {
        return {}; // a file, or the gathered remainder: nothing to go into
    }
    return item->data(Qt::UserRole).toString();
}

QString UsageWindow::targetAt(int row) const {
    // Files as well as folders: this side lists both, and「delete the 4 GB
    // thing」is the whole reason for finding it. The gathered remainder has no
    // path and is not a thing that can be acted on.
    QTableWidgetItem *item = m_folders->item(row, ColumnName);
    return item == nullptr ? QString() : item->data(Qt::UserRole).toString();
}

void UsageWindow::runOn(int kind) {
    // Only the left list. A kind is not a place and not a file - there is
    // nothing on that side to delete.
    if (!m_folders->hasFocus() && m_folders->currentRow() < 0) {
        return;
    }
    const QString path = targetAt(m_folders->currentRow());
    if (path.isEmpty()) {
        return;
    }

    QString destination;
    if (kind == ops::Copy || kind == ops::Move) {
        DestinationDialog dialog(m_app, kind == ops::Move, 1, this);
        if (dialog.exec() != QDialog::Accepted) {
            return;
        }
        destination = dialog.destination();
        if (destination.isEmpty()) {
            return;
        }
    }

    QString message;
    if (!ops::confirmAndStartPaths(m_app, this, static_cast<ops::Kind>(kind), {path}, destination,
                                   &message)) {
        if (!message.isEmpty()) {
            m_status->setText(message);
        }
        return;
    }
    // The report is now describing a folder that has changed. Waiting for the
    // operation rather than measuring straight away: the numbers would be the
    // ones from before it ran, presented as if they were after.
    if (m_afterOperation == nullptr) {
        m_afterOperation = new QTimer(this);
        m_afterOperation->setInterval(kPollMs);
        connect(m_afterOperation, &QTimer::timeout, this, [this] {
            if (jtf_op_running(m_app) != 0 || jtf_op_queued(m_app) > 0) {
                return;
            }
            m_afterOperation->stop();
            run();
            emit folderChanged();
        });
    }
    m_afterOperation->start();
}

void UsageWindow::descendTo(const QString &path) {
    if (path.isEmpty()) {
        return;
    }
    m_trail.append(m_path);
    m_path = path;
    m_up->setVisible(true);
    run();
}

void UsageWindow::goUp() {
    if (m_trail.isEmpty()) {
        return;
    }
    m_path = m_trail.takeLast();
    m_up->setVisible(!m_trail.isEmpty());
    run();
}

void UsageWindow::showRowMenu(const QPoint &at) {
    const int row = m_folders->rowAt(at.y());
    if (row < 0) {
        return;
    }
    m_folders->selectRow(row);
    QTableWidgetItem *item = m_folders->item(row, ColumnName);
    if (item == nullptr) {
        return;
    }
    const QString path = item->data(Qt::UserRole).toString();
    const bool isFolder = item->data(kIsFolderRole).toBool();
    if (path.isEmpty()) {
        return; // the gathered remainder is a total, not a thing
    }

    const QColor colour = palette().color(QPalette::Text);
    QMenu menu(this);
    QAction *into = nullptr;
    if (isFolder) {
        into = menu.addAction(glyph::forCommand(QStringLiteral("file.disk_usage"), colour),
                              tr_("usage.descend"));
    }
    QAction *show = menu.addAction(glyph::forCommand(QStringLiteral("file.open"), colour),
                                   tr_("usage.show_in_pane"));
    // Acting on what the walk found, without leaving the report to do it.
    menu.addSeparator();
    QAction *copy = menu.addAction(glyph::forCommand(QStringLiteral("file.copy_to"), colour),
                                   tr_("command.file.copy_to"));
    QAction *move = menu.addAction(glyph::forCommand(QStringLiteral("file.move_to"), colour),
                                   tr_("command.file.move_to"));
    QAction *trash = menu.addAction(glyph::forCommand(QStringLiteral("file.trash"), colour),
                                    tr_("command.file.trash"));
    QAction *up = nullptr;
    if (!m_trail.isEmpty()) {
        menu.addSeparator();
        up = menu.addAction(glyph::make(glyph::Shape::ArrowUp, colour), tr_("nav.up"));
    }

    QAction *chosen = menu.exec(m_folders->viewport()->mapToGlobal(at));
    if (chosen == nullptr) {
        return;
    }
    if (chosen == into) {
        descendTo(path);
    } else if (chosen == show) {
        emit folderChosen(path);
    } else if (chosen == copy) {
        runOn(ops::Copy);
    } else if (chosen == move) {
        runOn(ops::Move);
    } else if (chosen == trash) {
        runOn(ops::Trash);
    } else if (chosen == up) {
        goUp();
    }
}

void UsageWindow::poll() {
    const bool moved = jtf_pump_usage(m_app) != 0;
    if (jtf_usage_is_done(m_app) == 0) {
        // Still walking. Saying how far it has got is the difference between
        // a window that is working and one that has hung.
        if (moved) {
            m_status->setText(jtfFill(
                jtfFill(tr_("usage.progress"), "size",
                        PaneWidget::formatSize(jtf_usage_progress(m_app, 0))),
                "files", QString::number(jtf_usage_progress(m_app, 1))));
            const QString where =
                jtfText([&](char *b, int l) { return jtf_usage_in(m_app, b, l); });
            m_where->setText(m_where->fontMetrics().elidedText(where, Qt::ElideMiddle,
                                                              qMax(0, m_where->width() - 8)));
            m_where->setToolTip(where);
        }
        return;
    }
    m_poll->stop();
    m_spinner->setVisible(false);
    m_where->clear();
    m_where->setToolTip(QString());
    m_cancel->setVisible(false);
    fill();
}

void UsageWindow::fill() {
    const quint64 total = jtf_usage_total(m_app, 0);

    // The scale is the biggest row in each list, not the disc's total. The
    // column is there to read the ranking off, and against a total that is
    // mostly things not listed, every bar would be a stub.
    quint64 largestFolder = jtf_usage_total(m_app, 3); // the loose-files row counts too
    for (int i = 0; i < jtf_usage_folder_count(m_app); ++i) {
        largestFolder = qMax(largestFolder, jtf_usage_folder_value(m_app, i, 0));
    }
    quint64 largestKind = 0;
    for (int i = 0; i < jtf_usage_kind_count(m_app); ++i) {
        largestKind = qMax(largestKind, jtf_usage_kind_value(m_app, i, 0));
    }

    const auto addRow = [this](QTableWidget *table, int row, const QString &name,
                               const QString &tooltip, const QString &path, quint64 bytes,
                               quint64 files, quint64 largest, const QString &iconFor,
                               bool isFolder) {
        table->insertRow(row);
        // The same icons the file list draws, from the same provider - a row
        // about a folder should look like a folder, and「圖片 (jpg)」should
        // carry the icon a `.jpg` carries three windows away.
        auto *nameItem = iconFor.isEmpty()
                             ? new QTableWidgetItem(name)
                             : new QTableWidgetItem(icon(iconFor, isFolder), name);
        if (!tooltip.isEmpty()) {
            nameItem->setToolTip(tooltip);
        }
        // The path travels with the row, so a double click knows where to go
        // without the window parsing its own display text back into a path.
        nameItem->setData(Qt::UserRole, path);
        nameItem->setData(kIsFolderRole, isFolder);
        table->setItem(row, ColumnName, nameItem);

        // The bar's own cell. It sorts by the same number the size does, so
        // clicking either heading orders the list the same way.
        // The bar's own cell: no text, just the share it is drawn from. It
        // sorts by the same number the size does, so either heading orders the
        // list the same way.
        auto *shareItem = new NumericItem(bytes);
        shareItem->setData(kShareRole,
                           largest == 0 ? 0.0
                                        : static_cast<double>(bytes) / static_cast<double>(largest));
        table->setItem(row, ColumnShare, shareItem);

        // Sorted on the raw byte count, so that "9.9 MB" does not sort above
        // "1.2 GB" the way the formatted text would.
        auto *sizeItem = new NumericItem(PaneWidget::formatSize(bytes), bytes);
        sizeItem->setTextAlignment(Qt::AlignRight | Qt::AlignVCenter);
        table->setItem(row, ColumnSize, sizeItem);

        auto *filesItem = new NumericItem(QString::number(files), files);
        filesItem->setTextAlignment(Qt::AlignRight | Qt::AlignVCenter);
        table->setItem(row, ColumnFiles, filesItem);
    };

    m_folders->setSortingEnabled(false); // re-sorting on every insert is quadratic
    m_folders->setUpdatesEnabled(false);
    m_folders->setRowCount(0);
    const int folders = jtf_usage_folder_count(m_app);
    for (int index = 0; index < folders; ++index) {
        const QString name =
            jtfText([&](char *b, int l) { return jtf_usage_folder_name(m_app, index, b, l); });
        const QString path =
            jtfText([&](char *b, int l) { return jtf_usage_folder_path(m_app, index, b, l); });
        // A row with no name is the gathered remainder of a folder too wide to
        // list, not a folder: it is named as what it is and has no path, so a
        // double click does not offer to go anywhere.
        const bool gathered = name.isEmpty();
        addRow(m_folders, m_folders->rowCount(), gathered ? tr_("usage.rest") : name,
               gathered ? QString() : path, gathered ? QString() : path,
               jtf_usage_folder_value(m_app, index, 0), jtf_usage_folder_value(m_app, index, 1),
               largestFolder,
               path.isEmpty() ? QStringLiteral("file") : path,
               jtf_usage_folder_is_directory(m_app, index) != 0);
    }
    m_folders->setUpdatesEnabled(true);
    m_folders->setSortingEnabled(true);
    // Largest first, which is the order the walk produced and the
    // answer people opened this for. Enabling sorting alone would
    // re-sort by column 0 - the names - and throw that away.
    m_folders->sortItems(ColumnSize, Qt::DescendingOrder);
    // Land on the first row - the biggest one, after that sort - so that
    // arriving in a folder leaves somewhere to press Right from. A list with
    // no current row makes the first arrow key a wasted press.
    if (m_folders->rowCount() > 0) {
        m_folders->setCurrentCell(0, ColumnName);
        m_folders->setFocus();
    }

    m_kinds->setSortingEnabled(false); // re-sorting on every insert is quadratic
    m_kinds->setUpdatesEnabled(false);
    m_kinds->setRowCount(0);
    const int kinds = jtf_usage_kind_count(m_app);
    for (int index = 0; index < kinds; ++index) {
        const QString extension =
            jtfText([&](char *b, int l) { return jtf_usage_kind_extension(m_app, index, b, l); });
        const QString groupKey =
            jtfText([&](char *b, int l) { return jtf_usage_kind_group(m_app, index, b, l); });
        const QString group = tr_(groupKey.toUtf8().constData());
        // 「影片 (mp4)」: the group is what the question was about, the
        // extension is which one it turned out to be.
        const QString name = extension.isEmpty()
                                 ? group
                                 : group + QStringLiteral(" (") + extension + QLatin1Char(')');
        // A name of that type, so the provider answers with that type's icon.
        // Nothing is looked up on disk - the icon follows the extension. Rows
        // with no extension of their own still get one, because a row without
        // an icon starts its text where every other row's icon is, and the
        // column stops lining up.
        const QString sample = extension.isEmpty() ? QStringLiteral("file")
                                                   : QStringLiteral("file.") + extension;
        addRow(m_kinds, m_kinds->rowCount(), name, QString(), QString(),
               jtf_usage_kind_value(m_app, index, 0), jtf_usage_kind_value(m_app, index, 1),
               largestKind, sample, false);
    }
    m_kinds->setUpdatesEnabled(true);
    m_kinds->setSortingEnabled(true);
    // Largest first, which is the order the walk produced and the
    // answer people opened this for. Enabling sorting alone would
    // re-sort by column 0 - the names - and throw that away.
    m_kinds->sortItems(ColumnSize, Qt::DescendingOrder);

    QString status = jtfFill(
        jtfFill(tr_("usage.summary"), "size", PaneWidget::formatSize(total)), "files",
        QString::number(jtf_usage_total(m_app, 1)));
    if (jtf_usage_total(m_app, 4) != 0) {
        // Cancelled, too deep, or blocked by a folder we could not read. A
        // breakdown that quietly omits part of a disc is worse than one
        // labelled incomplete.
        status += QStringLiteral("   ") + tr_("usage.partial");
    }
    m_status->setText(status);
}

void UsageWindow::changeEvent(QEvent *event) {
    QWidget::changeEvent(event);
    if (event->type() != QEvent::PaletteChange) {
        return;
    }
    // Everything drawn here carries the colour it was drawn with, so a theme
    // change has to be applied rather than merely repainted: the up arrow is a
    // glyph, the bars and the ring take a colour, and the kind icons were
    // cached under the old palette.
    m_up->setIcon(glyph::make(glyph::Shape::ArrowUp, palette().color(QPalette::Text)));
    const QColor accent = palette().color(QPalette::Highlight);
    if (m_spinner != nullptr) {
        m_spinner->setColour(accent);
    }
    for (QTableWidget *table : {m_folders, m_kinds}) {
        if (auto *bars = static_cast<ShareBarDelegate *>(
                table->itemDelegateForColumn(ColumnShare))) {
            bars->setBarColour(accent);
        }
        table->viewport()->update();
    }
}

bool UsageWindow::eventFilter(QObject *watched, QEvent *event) {
    if (watched != m_folders && watched != m_kinds) {
        return QWidget::eventFilter(watched, event);
    }
    const bool override = event->type() == QEvent::ShortcutOverride;
    if (!override && event->type() != QEvent::KeyPress) {
        return QWidget::eventFilter(watched, event);
    }
    auto *key = static_cast<QKeyEvent *>(event);
    switch (key->key()) {
    case Qt::Key_Left:
    case Qt::Key_Right:
    case Qt::Key_Return:
    case Qt::Key_Enter:
    case Qt::Key_Backspace:
    case Qt::Key_Escape:
    case Qt::Key_Tab:
    case Qt::Key_D:
    case Qt::Key_C:
    case Qt::Key_M:
        // A menu bar is application-wide on macOS, so the main window's
        // `Left = 上一層` fired here too and swallowed the key before this
        // window ever saw it - Backspace worked and Left did not, for no
        // reason visible from this file. Accepting the override says the key
        // belongs to whoever has the focus, and it then arrives as an ordinary
        // press below.
        key->accept();
        if (!override) {
            keyPressEvent(key);
        }
        return true;
    default:
        return QWidget::eventFilter(watched, event);
    }
}

void UsageWindow::keyPressEvent(QKeyEvent *event) {
    switch (event->key()) {
    case Qt::Key_Escape:
        close();
        return;
    case Qt::Key_Return:
    case Qt::Key_Enter:
    case Qt::Key_Right:
        // Right goes in and Left goes back, which is what they do in the file
        // list. A window that walks a folder tree should walk it the same way
        // the rest of the program does.
        //
        // Only on the left. A kind is not a place - there is nowhere for
        //「文件 (odt)」to lead - and this read the left list's current row
        // whichever side had the focus, so Enter on a kind walked into an
        // unrelated folder that merely happened to be selected behind it.
        if (m_folders->hasFocus()) {
            descendTo(folderAt(m_folders->currentRow()));
        }
        return;
    case Qt::Key_Backspace:
    case Qt::Key_Left:
        goUp();
        return;
    case Qt::Key_Tab:
        // Between the two lists, as Tab moves between panes in the window
        // behind this one.
        (m_folders->hasFocus() ? m_kinds : m_folders)->setFocus();
        return;
    // The three file commands that make sense here, spelled the way they are
    // spelled everywhere else in Single-Key mode. Finding the folder that is
    // eating the disc and then having to go somewhere else to do anything
    // about it is most of the walk wasted.
    case Qt::Key_D:
        runOn(ops::Trash);
        return;
    case Qt::Key_C:
        runOn(ops::Copy);
        return;
    case Qt::Key_M:
        runOn(ops::Move);
        return;
    default:
        break;
    }
    QWidget::keyPressEvent(event);
}
