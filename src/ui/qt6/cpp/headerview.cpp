#include "headerview.h"

#include <QMouseEvent>
#include <QPainter>
#include <QStyleOptionButton>
#include <QStyleOptionViewItem>
#include <QPainterPath>

namespace {
// The caret is drawn on this box, placed after the text.
constexpr int kCaretWidth = 7;
constexpr int kCaretHeight = 4;
constexpr int kCaretGap = 6;

// The mark-all box at the head of the name column. Only a fallback: the box is
// normally placed by asking the style where a row's checkbox goes, so that the
// two line up whatever the platform or the font size does to them. Guessing at
// 8 and 14 put the header's box three pixels left of the rows' and made it a
// size smaller, which is what「看起來歪歪的」was.
constexpr int kMarkBox = 14;
constexpr int kMarkLeft = 8;

// Where a row in `view` draws its checkbox, relative to the start of the name
// column. Empty if there is no view to ask.
QRect rowCheckIndicator(const QWidget *view, int sectionWidth, int height) {
    if (view == nullptr) {
        return {};
    }
    QStyleOptionViewItem item;
    item.initFrom(view);
    item.rect = QRect(0, 0, sectionWidth, height);
    item.features = QStyleOptionViewItem::HasCheckIndicator;
    item.viewItemPosition = QStyleOptionViewItem::Beginning;
    return view->style()->subElementRect(QStyle::SE_ItemViewItemCheckIndicator, &item, view);
}
} // namespace

JtfHeaderView::JtfHeaderView(QWidget *parent) : QHeaderView(Qt::Horizontal, parent) {
    setMouseTracking(true);
    setSectionsClickable(true);
    setHighlightSections(false);
    // Qt's indicator is replaced, not merely restyled, so it must not also
    // paint one of its own.
    setSortIndicatorShown(false);
}

void JtfHeaderView::setCaretVisible(bool visible) {
    if (m_caretVisible == visible) {
        return;
    }
    m_caretVisible = visible;
    update();
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

    const bool sorted = m_caretVisible && index == sortIndicatorSection();
    const QString label = model()->headerData(index, Qt::Horizontal, Qt::DisplayRole).toString();
    const auto alignment = static_cast<Qt::Alignment>(
        model()->headerData(index, Qt::Horizontal, Qt::TextAlignmentRole).toInt());
    const Qt::Alignment horizontal =
        (alignment & Qt::AlignHorizontal_Mask) ? (alignment & Qt::AlignHorizontal_Mask)
                                               : Qt::AlignLeft;

    QRect content = rect.adjusted(8, 0, -8, 0);
    qWarning("JTFHDR idx=%d rect=%dx%d label='%s' dim=%s", index, rect.width(), rect.height(),
             qPrintable(label), qPrintable(m_dim.name(QColor::HexArgb)));

    // The mark-all box, on the name column only, drawn before the text so the
    // text starts after it.
    if (m_markAllVisible && index == 0) {
        const QRect indicator = rowCheckIndicator(parentWidget(), rect.width(), rect.height());
        const int left = indicator.isEmpty() ? kMarkLeft : indicator.left();
        const int side = indicator.isEmpty() ? kMarkBox : indicator.width();
        const QRect box(rect.left() + left, rect.center().y() - side / 2, side, side);
        m_markAllRect = box;
        QStyleOptionButton check;
        check.rect = box;
        check.state = QStyle::State_Enabled;
        switch (m_markAllState) {
        case Qt::Checked:
            check.state |= QStyle::State_On;
            break;
        case Qt::PartiallyChecked:
            check.state |= QStyle::State_NoChange;
            break;
        case Qt::Unchecked:
            check.state |= QStyle::State_Off;
            break;
        }
        style()->drawControl(QStyle::CE_CheckBox, &check, painter, this);
        content.setLeft(box.right() + kCaretGap);
    } else if (index == 0) {
        m_markAllRect = QRect();
    }
    // Explicitly, so the font the text is measured with is the font it is
    // drawn with. drawControl above goes through the stylesheet style, which
    // is free to leave the painter carrying a different one, and a width
    // measured with one font and drawn with another loses the last glyph.
    painter->setFont(font());
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
    // Drawn into the room that is left, not into a box cut to the measured
    // width. Advance width is where the *next* glyph would start, which for a
    // CJK glyph with any overhang is short of where this one ends - so a box
    // that size shaved the last character. The text is already elided to fit,
    // so a rect that runs to the edge of the section cannot overflow it.
    const QRect textRect(textLeft, rect.top(), qMax(textWidth, content.right() - textLeft + 1),
                         rect.height());
    painter->drawText(textRect, Qt::AlignVCenter | Qt::AlignLeft, shown);

    paintDivider(painter, rect, index);

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

void JtfHeaderView::paintDivider(QPainter *painter, const QRect &rect, int index) const {
    // The grab handle has to be visible or the column is not resizable in
    // practice: people aim at what they can see. A full-height rule between
    // every column drew the table's structure instead of its contents, so
    // this is a short one, inset from both edges, and brighter under the
    // pointer - which is also the moment it matters.
    if (index >= count() - 1) {
        return; // nothing to resize past the last column
    }
    // Always drawn, and faintly. It says where one column's clickable strip
    // ends, which matters because clicking a header sorts by it - and it is
    // the thing you reach for to resize. It used to appear only under the
    // pointer, on the argument that a rule between every column is a comb
    // across the header; at a tenth of the text's weight it is not, and a
    // resize handle nobody can see is a resize handle nobody uses.
    //
    // Three weights: quiet at rest, a little clearer beside the pointer,
    // and solid under it, so the one that would actually be grabbed reads as
    // a control rather than as a stray line.
    const bool hot = index == m_hoveredDivider;
    const bool near = qAbs(index - m_hoveredDivider) == 1 && m_hoveredDivider >= 0;
    const int inset = hot ? qMax(3, rect.height() / 5) : qMax(5, rect.height() / 3);
    QColor colour = hot ? m_text : m_dim;
    if (!hot) {
        colour.setAlphaF(near ? 0.5 : 0.22);
    }
    painter->save();
    painter->setPen(QPen(colour, hot ? 1.6 : 1.0));
    painter->drawLine(rect.right(), rect.top() + inset, rect.right(), rect.bottom() - inset);
    painter->restore();
}

void JtfHeaderView::setMarkAllVisible(bool visible) {
    if (m_markAllVisible == visible) {
        return;
    }
    m_markAllVisible = visible;
    viewport()->update();
}

void JtfHeaderView::setMarkAllState(Qt::CheckState state) {
    if (m_markAllState == state) {
        return;
    }
    m_markAllState = state;
    viewport()->update();
}

void JtfHeaderView::mousePressEvent(QMouseEvent *event) {
    // A click in the box marks or clears everything, and does not also sort
    // the column it happens to sit in.
    if (m_markAllVisible && m_markAllRect.isValid()
        && m_markAllRect.contains(event->position().toPoint())) {
        emit markAllToggled(m_markAllState != Qt::Checked);
        event->accept();
        return;
    }
    QHeaderView::mousePressEvent(event);
}

void JtfHeaderView::mouseMoveEvent(QMouseEvent *event) {
    // Which divider the pointer is near, by the same margin Qt uses to decide
    // that a press starts a resize.
    constexpr int kGrabMargin = 4;
    int near = -1;
    for (int i = 0; i < count() - 1; ++i) {
        const int edge = sectionViewportPosition(i) + sectionSize(i);
        if (qAbs(event->position().x() - edge) <= kGrabMargin) {
            near = i;
            break;
        }
    }
    if (near != m_hoveredDivider) {
        m_hoveredDivider = near;
        viewport()->update();
    }
    QHeaderView::mouseMoveEvent(event);
}

void JtfHeaderView::leaveEvent(QEvent *event) {
    if (m_hoveredDivider != -1) {
        m_hoveredDivider = -1;
        viewport()->update();
    }
    QHeaderView::leaveEvent(event);
}
