#include "headerview.h"

#include <QPainter>
#include <QPainterPath>

namespace {
// The caret is drawn on this box, placed after the text.
constexpr int kCaretWidth = 7;
constexpr int kCaretHeight = 4;
constexpr int kCaretGap = 6;
} // namespace

JtfHeaderView::JtfHeaderView(QWidget *parent) : QHeaderView(Qt::Horizontal, parent) {
    setSectionsClickable(true);
    setHighlightSections(false);
    // Qt's indicator is replaced, not merely restyled, so it must not also
    // paint one of its own.
    setSortIndicatorShown(false);
}

void JtfHeaderView::applyTheme(const QColor &text, const QColor &dim, const QColor &indicator) {
    m_text = text;
    m_dim = dim;
    m_indicator = indicator;
    update();
}

void JtfHeaderView::paintSection(QPainter *painter, const QRect &rect, int index) const {
    if (!rect.isValid()) {
        return;
    }
    painter->save();

    // The section's own background and separators still come from the
    // stylesheet, so the header matches the rest of the chrome; only the text
    // and the caret are painted here.
    QStyleOptionHeader option;
    initStyleOption(&option);
    option.rect = rect;
    option.section = index;
    option.text = QString();
    option.sortIndicator = QStyleOptionHeader::None;
    style()->drawControl(QStyle::CE_Header, &option, painter, this);

    const bool sorted = index == sortIndicatorSection();
    const QString label = model()->headerData(index, Qt::Horizontal, Qt::DisplayRole).toString();
    const auto alignment = static_cast<Qt::Alignment>(
        model()->headerData(index, Qt::Horizontal, Qt::TextAlignmentRole).toInt());
    const Qt::Alignment horizontal =
        (alignment & Qt::AlignHorizontal_Mask) ? (alignment & Qt::AlignHorizontal_Mask)
                                               : Qt::AlignLeft;

    QRect content = rect.adjusted(8, 0, -8, 0);
    const QFontMetrics metrics(font());
    const int caretRoom = sorted ? kCaretGap + kCaretWidth : 0;
    // The text is elided against the room left after the caret, so a narrow
    // column loses letters rather than losing the indicator.
    const QString shown =
        metrics.elidedText(label, Qt::ElideRight, qMax(0, content.width() - caretRoom));
    const int textWidth = metrics.horizontalAdvance(shown);

    int textLeft = content.left();
    if (horizontal & Qt::AlignRight) {
        textLeft = content.right() - textWidth - caretRoom + 1;
    } else if (horizontal & Qt::AlignHCenter) {
        textLeft = content.left() + (content.width() - textWidth - caretRoom) / 2;
    }

    painter->setPen(sorted ? m_text : m_dim);
    painter->drawText(QRect(textLeft, rect.top(), textWidth, rect.height()),
                      Qt::AlignVCenter | Qt::AlignLeft,
                      shown);

    if (!sorted) {
        painter->restore();
        return;
    }

    // A caret, not a triangle: it is a state, and a filled arrowhead the size
    // of a button reads as something you press.
    const int caretLeft = textLeft + textWidth + kCaretGap;
    const int caretTop = rect.center().y() - kCaretHeight / 2;
    const bool ascending = sortIndicatorOrder() == Qt::AscendingOrder;
    QPainterPath caret;
    if (ascending) {
        caret.moveTo(caretLeft, caretTop + kCaretHeight);
        caret.lineTo(caretLeft + kCaretWidth / 2.0, caretTop);
        caret.lineTo(caretLeft + kCaretWidth, caretTop + kCaretHeight);
    } else {
        caret.moveTo(caretLeft, caretTop);
        caret.lineTo(caretLeft + kCaretWidth / 2.0, caretTop + kCaretHeight);
        caret.lineTo(caretLeft + kCaretWidth, caretTop);
    }
    painter->setRenderHint(QPainter::Antialiasing, true);
    painter->setBrush(Qt::NoBrush);
    painter->setPen(QPen(m_indicator, 1.4, Qt::SolidLine, Qt::RoundCap, Qt::RoundJoin));
    painter->drawPath(caret);

    painter->restore();
}
