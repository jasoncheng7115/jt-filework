#include "listlook.h"

#include "headerview.h"
#include "rowdelegate.h"

#include <QHeaderView>
#include <QTableWidget>

namespace {
/// The same row height and icon size the pane's list uses. Kept as literals
/// here rather than reached for across a header, because a list that is
/// almost the same height as another list looks like a mistake.
constexpr int kRowHeight = 22;
constexpr int kIconEdge = 16;
} // namespace

void listlook::apply(QTableWidget *table, const QFont &listFont) {
    if (table == nullptr) {
        return;
    }
    table->setFont(listFont);
    table->verticalHeader()->setVisible(false);
    table->verticalHeader()->setDefaultSectionSize(kRowHeight);
    // Uniform row heights is what lets Qt virtualize; without it the view
    // measures every row and the cost grows with the listing.
    table->verticalHeader()->setSectionResizeMode(QHeaderView::Fixed);
    table->setIconSize(QSize(kIconEdge, kIconEdge));
    table->setShowGrid(false);
    table->setAlternatingRowColors(true);
    table->setSelectionBehavior(QAbstractItemView::SelectRows);
    table->setEditTriggers(QAbstractItemView::NoEditTriggers);
    table->setWordWrap(false);
    table->setTextElideMode(Qt::ElideRight);
    table->setSortingEnabled(false); // sorting is the caller's, not Qt's

    // The pane's own header: text drawn left for names and right for figures,
    // no platform pressed-look, no sort arrow of Qt's own. Its mark-all box is
    // left off - these windows mark with Space, and a box promising to mark
    // every member of an archive is a promise about a different operation.
    auto *header = new JtfHeaderView(table);
    header->setMarkAllVisible(false);
    table->setHorizontalHeader(header);
    header->setSectionsClickable(false);
    // No sort caret: these tables do not sort, and an arrow over the first
    // column would claim an order they do not have.
    header->setCaretVisible(false);
    header->setFont(listFont);

    // The tick over a marked row's box, drawn rather than left to the platform,
    // so a mark looks the same here as it does in the list.
    auto *rows = new RowDelegate(table);
    table->setItemDelegate(rows);
}

void listlook::applyTheme(QTableWidget *table, const QColor &text, const QColor &dim,
                          const QColor &tick) {
    if (table == nullptr) {
        return;
    }
    if (auto *header = qobject_cast<JtfHeaderView *>(table->horizontalHeader())) {
        header->applyTheme(text, dim, tick);
    }
    if (auto *rows = qobject_cast<RowDelegate *>(table->itemDelegate())) {
        rows->setTickColour(tick);
    }
}
