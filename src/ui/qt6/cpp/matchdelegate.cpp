#include "matchdelegate.h"

#include <QApplication>
#include <QPainter>
#include <QStyle>

MatchDelegate::MatchDelegate(QObject *parent) : RowDelegate(parent) {}

void MatchDelegate::setNeedle(const QString &needle) { m_needle = needle.trimmed(); }

void MatchDelegate::setHighlight(const QColor &background, const QColor &text) {
    m_background = background;
    m_text = text;
}

void MatchDelegate::paint(QPainter *painter, const QStyleOptionViewItem &option,
                          const QModelIndex &index) const {
    const QString text = index.data(Qt::DisplayRole).toString();
    if (m_needle.isEmpty() || text.isEmpty()) {
        RowDelegate::paint(painter, option, index);
        return;
    }

    // Case-insensitively, because that is how the search matched. A needle
    // that is not present - the row matched on something else, a size or a
    // date - simply draws normally.
    const int at = text.indexOf(m_needle, 0, Qt::CaseInsensitive);
    if (at < 0) {
        RowDelegate::paint(painter, option, index);
        return;
    }

    // The row's own background, selection and icon are the base style's work;
    // only the text is drawn here, so a highlighted row still looks selected.
    QStyleOptionViewItem base(option);
    initStyleOption(&base, index);
    const QString original = base.text;
    base.text.clear();
    QStyle *style = base.widget != nullptr ? base.widget->style() : QApplication::style();
    style->drawControl(QStyle::CE_ItemViewItem, &base, painter, base.widget);

    const QRect textRect =
        style->subElementRect(QStyle::SE_ItemViewItemText, &base, base.widget);
    const QFontMetrics metrics(base.font);
    const QString shown = metrics.elidedText(original, base.textElideMode, textRect.width());

    // The run is located in the *elided* text, so a name cut short in the
    // middle does not paint its highlight over the ellipsis.
    const int start = shown.indexOf(m_needle, 0, Qt::CaseInsensitive);
    painter->save();
    painter->setClipRect(textRect);
    const int baseline = textRect.top() + (textRect.height() + metrics.ascent()
                                           - metrics.descent()) / 2;
    int x = textRect.left();
    const auto draw = [&](const QString &part, bool highlighted) {
        const int width = metrics.horizontalAdvance(part);
        if (highlighted) {
            painter->fillRect(QRect(x, textRect.top() + 1, width, textRect.height() - 2),
                              m_background);
            painter->setPen(m_text);
        } else {
            painter->setPen(base.palette.color(
                base.state & QStyle::State_Selected ? QPalette::HighlightedText : QPalette::Text));
        }
        painter->drawText(x, baseline, part);
        x += width;
    };

    if (start < 0) {
        draw(shown, false);
    } else {
        draw(shown.left(start), false);
        draw(shown.mid(start, m_needle.size()), true);
        draw(shown.mid(start + m_needle.size()), false);
    }
    painter->restore();
    // This branch draws the row itself rather than calling the base, so the
    // tick has to be drawn here as well; without it a marked row loses its
    // tick the moment a search or filter is running.
    paintTick(painter, option, index);
}
