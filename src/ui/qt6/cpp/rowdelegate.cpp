#include "rowdelegate.h"

#include <QAbstractItemView>
#include <QApplication>
#include <QPainter>
#include <QTableView>
#include <QPainterPath>
#include <QStyle>
#include <QStyleOptionViewItem>

void RowDelegate::initStyleOption(QStyleOptionViewItem *option, const QModelIndex &index) const {
    QStyledItemDelegate::initStyleOption(option, index);
    // Set *and* cleared: Qt has already marked the one cell under the pointer,
    // so leaving that alone would light one cell of the wrong row whenever the
    // hover moves faster than the repaint.
    if (index.row() == m_hoveredRow) {
        option->state |= QStyle::State_MouseOver;
    } else {
        option->state &= ~QStyle::State_MouseOver;
    }
}

void RowDelegate::paint(QPainter *painter, const QStyleOptionViewItem &option,
                        const QModelIndex &index) const {
    QStyledItemDelegate::paint(painter, option, index);
    paintCursor(painter, option, index);
    paintTick(painter, option, index);
}

namespace {

/// The lowest column the view is actually showing.
///
/// A hidden column is never painted, so a side drawn on one is a side that
/// never appears. Falls back to the model's own first column for a view that
/// has no notion of hiding.
int firstVisibleColumn(const QWidget *widget, const QAbstractItemModel *model) {
    const auto *table = qobject_cast<const QTableView *>(widget);
    const int columns = model == nullptr ? 0 : model->columnCount();
    if (table == nullptr) {
        return 0;
    }
    for (int column = 0; column < columns; ++column) {
        if (!table->isColumnHidden(column)) {
            return column;
        }
    }
    return 0;
}

/// The highest column the view is actually showing.
int lastVisibleColumn(const QWidget *widget, const QAbstractItemModel *model) {
    const int columns = model == nullptr ? 0 : model->columnCount();
    const auto *table = qobject_cast<const QTableView *>(widget);
    if (table == nullptr) {
        return columns - 1;
    }
    for (int column = columns - 1; column >= 0; --column) {
        if (!table->isColumnHidden(column)) {
            return column;
        }
    }
    return columns - 1;
}

} // namespace

void RowDelegate::paintCursor(QPainter *painter, const QStyleOptionViewItem &option,
                              const QModelIndex &index) const {
    // Where the keyboard is, as opposed to what is chosen.
    //
    // They used to be the same thing: the arrow keys moved the cursor *and*
    // replaced the selection, so one highlight said both. Once the arrows
    // stopped clearing the marks - because clearing them made it impossible to
    // mark a second file - the cursor had nothing to draw it with, and moving
    // through a list looked like moving through nothing.
    //
    // An outline rather than a fill, so a row that is both the cursor and
    // marked reads as both instead of one hiding the other.
    if (!m_cursor.isValid()) {
        return;
    }
    const auto *view = qobject_cast<const QAbstractItemView *>(option.widget);
    if (view == nullptr || !view->hasFocus()) {
        return; // the cursor belongs to whichever list has the keyboard
    }
    const QModelIndex current = view->currentIndex();
    if (!current.isValid() || current.row() != index.row()) {
        return;
    }

    painter->save();
    painter->setRenderHint(QPainter::Antialiasing, false);
    painter->setPen(QPen(m_cursor, 1));
    const QRect r = option.rect.adjusted(0, 0, -1, -1);
    // Each cell draws its own segment; together they make one line round the
    // row. The ends are drawn only by the cells that own them.
    painter->drawLine(r.topLeft(), r.topRight());
    painter->drawLine(r.bottomLeft(), r.bottomRight());
    // The sides are drawn by the cells at the ends of the row - and the ends
    // are the first and last *visible* columns, not the first and last columns
    // the model has.
    //
    // The model carries eleven columns and the list shows four; the other
    // seven are hidden. Asking for `columnCount() - 1` therefore named column
    // ten, which is hidden and never painted, so the right-hand side of the
    // outline was never drawn at all and the rectangle hung open on the right.
    if (index.column() == firstVisibleColumn(option.widget, index.model())) {
        painter->drawLine(r.topLeft(), r.bottomLeft());
    }
    if (index.column() == lastVisibleColumn(option.widget, index.model())) {
        painter->drawLine(r.topRight(), r.bottomRight());
    }
    painter->restore();
}

void RowDelegate::paintTick(QPainter *painter, const QStyleOptionViewItem &option,
                            const QModelIndex &index) const {
    // A filled square says "something is true about this row" and leaves you
    // to work out what. A tick says it. The stylesheet cannot supply one -
    // an indicator image comes from a file and so cannot follow the theme's
    // colour - so it is drawn here, over the box the style has just filled.
    const QVariant state = index.data(Qt::CheckStateRole);
    if (!state.isValid() || state.toInt() != Qt::Checked) {
        return;
    }

    QStyleOptionViewItem box(option);
    initStyleOption(&box, index);
    const QStyle *style = box.widget != nullptr ? box.widget->style() : QApplication::style();
    const QRect rect =
        style->subElementRect(QStyle::SE_ItemViewItemCheckIndicator, &box, box.widget);
    if (rect.isEmpty()) {
        return;
    }

    // Proportional to the box, so it stays a tick at any font size.
    const qreal w = rect.width();
    const qreal h = rect.height();
    QPainterPath tick;
    tick.moveTo(rect.left() + w * 0.24, rect.top() + h * 0.52);
    tick.lineTo(rect.left() + w * 0.43, rect.top() + h * 0.72);
    tick.lineTo(rect.left() + w * 0.78, rect.top() + h * 0.28);

    painter->save();
    painter->setRenderHint(QPainter::Antialiasing);
    painter->setPen(QPen(m_tick.isValid() ? m_tick : option.palette.color(QPalette::HighlightedText),
                         1.8, Qt::SolidLine, Qt::RoundCap, Qt::RoundJoin));
    painter->drawPath(tick);
    painter->restore();
}
