// A delegate that makes hover a property of the row, not of the cell.
//
// Qt hovers the item under the pointer. In a list whose selection behaviour is
// already "whole row", that is inconsistent: the row is what gets picked, so
// the row is what should light up on the way to picking it. With a wide name
// column the difference is not subtle - the highlight stopped at the name's
// right edge and the rest of the row stayed dark, which reads as a cell being
// hovered rather than a file.
//
// The hovered row is *reported* here rather than painted here: every cell in
// it is given State_MouseOver, and the stylesheet's existing `:hover` rule
// does the drawing. So there is still exactly one place that decides what
// hover looks like.
#pragma once

#include <QColor>
#include <QStyledItemDelegate>

class RowDelegate : public QStyledItemDelegate {
    Q_OBJECT

public:
    explicit RowDelegate(QObject *parent = nullptr) : QStyledItemDelegate(parent) {}

    /// The row under the pointer, or -1 for none.
    void setHoveredRow(int row) { m_hoveredRow = row; }
    int hoveredRow() const { return m_hoveredRow; }

    /// The colour of the tick drawn in a marked row's box.
    void setTickColour(const QColor &colour) { m_tick = colour; }

    /// The colour of the outline round the row the keyboard is on.
    void setCursorColour(const QColor &colour) { m_cursor = colour; }

    void paint(QPainter *painter, const QStyleOptionViewItem &option,
               const QModelIndex &index) const override;

protected:
    void initStyleOption(QStyleOptionViewItem *option, const QModelIndex &index) const override;

protected:
    /// Draw the tick over the checked box the style has just painted.
    void paintTick(QPainter *painter, const QStyleOptionViewItem &option,
                   const QModelIndex &index) const;
    /// Outline the row the keyboard is on, which is no longer the same thing
    /// as the row that is selected.
    void paintCursor(QPainter *painter, const QStyleOptionViewItem &option,
                     const QModelIndex &index) const;

private:
    int m_hoveredRow = -1;
    QColor m_tick;
    QColor m_cursor;
};
