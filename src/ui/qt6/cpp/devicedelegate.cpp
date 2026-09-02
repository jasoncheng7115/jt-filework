#include "devicedelegate.h"

#include <QApplication>
#include <QPainter>
#include <QStyleOptionViewItem>

namespace {

// The gaps. Generous by this program's standards, because there are only ever
// a handful of rows and each one is a decision that cannot be undone - this is
// not a list to be scanned at speed.
constexpr int kPadX = 12;
constexpr int kPadY = 10;
constexpr int kIconSize = 28;
constexpr int kIconGap = 12;
constexpr int kLineGap = 3;

/// The three text lines of a row, in the order they are drawn.
struct Lines {
    QString model;
    QString detail;
    QString node;
    QString refusal;
};

Lines linesOf(const QModelIndex &index) {
    return Lines{
        index.data(DeviceDelegate::ModelRole).toString(),
        index.data(DeviceDelegate::DetailRole).toString(),
        index.data(DeviceDelegate::NodeRole).toString(),
        index.data(DeviceDelegate::RefusalRole).toString(),
    };
}

/// A font one step down, for everything that is not the model name.
QFont smaller(const QFont &base, int steps = 1) {
    QFont font = base;
    const int size = base.pointSize();
    if (size > 0) {
        font.setPointSize(qMax(size - steps, 7));
    } else {
        font.setPixelSize(qMax(base.pixelSize() - steps * 2, 9));
    }
    return font;
}

} // namespace

void DeviceDelegate::setColours(const QColor &dim, const QColor &faint, const QColor &warning) {
    m_dim = dim;
    m_faint = faint;
    m_warning = warning;
}

QSize DeviceDelegate::sizeHint(const QStyleOptionViewItem &option, const QModelIndex &index) const {
    const Lines lines = linesOf(index);
    if (lines.model.isEmpty()) {
        // The placeholder row - "nothing is plugged in" - is one line of
        // ordinary text and is not a device.
        return QStyledItemDelegate::sizeHint(option, index);
    }
    const QFontMetrics big(option.font);
    const QFontMetrics small(smaller(option.font));
    int height = kPadY * 2 + big.height() + kLineGap + small.height();
    if (!lines.node.isEmpty()) {
        height += kLineGap + small.height();
    }
    if (!lines.refusal.isEmpty()) {
        height += kLineGap + small.height();
    }
    // Never shorter than the icon, or the icon is clipped on a row whose text
    // happens to be short.
    height = qMax(height, kIconSize + kPadY * 2);
    return QSize(option.rect.width(), height);
}

void DeviceDelegate::paint(QPainter *painter, const QStyleOptionViewItem &option,
                           const QModelIndex &index) const {
    const Lines lines = linesOf(index);
    if (lines.model.isEmpty()) {
        QStyledItemDelegate::paint(painter, option, index);
        return;
    }

    QStyleOptionViewItem opt = option;
    initStyleOption(&opt, index);
    // The text is drawn here, so the style must not also draw it underneath.
    opt.text.clear();
    opt.icon = QIcon();
    QStyle *style = opt.widget ? opt.widget->style() : QApplication::style();
    style->drawControl(QStyle::CE_ItemViewItem, &opt, painter, opt.widget);

    const bool selected = (opt.state & QStyle::State_Selected) != 0;
    const bool enabled = (opt.state & QStyle::State_Enabled) != 0;
    // On a selected row every line is drawn in the selection's own foreground,
    // because a dim grey that is legible on the pane background is not legible
    // on a saturated blue one.
    const QColor primary = selected ? opt.palette.color(QPalette::HighlightedText)
                                    : opt.palette.color(QPalette::Text);
    const QColor secondary = selected ? primary : (m_dim.isValid() ? m_dim : primary);
    const QColor tertiary = selected ? primary : (m_faint.isValid() ? m_faint : secondary);
    const QColor refusalColour =
        selected ? primary : (m_warning.isValid() ? m_warning : secondary);

    painter->save();
    if (!enabled) {
        // A disk that cannot be used is drawn faded as a whole, so that the
        // reason underneath reads as an explanation rather than a warning
        // about the disk you are about to write to.
        painter->setOpacity(0.72);
    }

    QRect content = opt.rect.adjusted(kPadX, kPadY, -kPadX, -kPadY);

    const QIcon icon = qvariant_cast<QIcon>(index.data(Qt::DecorationRole));
    if (!icon.isNull()) {
        const QRect iconRect(content.left(), content.top(), kIconSize, kIconSize);
        icon.paint(painter, iconRect, Qt::AlignCenter,
                   enabled ? QIcon::Normal : QIcon::Disabled);
        content.setLeft(iconRect.right() + kIconGap);
    }

    const QFont base = opt.font;
    QFont bold = base;
    bold.setBold(true);
    const QFont small = smaller(base);

    int y = content.top();

    painter->setFont(bold);
    painter->setPen(primary);
    const QFontMetrics boldMetrics(bold);
    painter->drawText(QRect(content.left(), y, content.width(), boldMetrics.height()),
                      Qt::AlignLeft | Qt::AlignVCenter,
                      boldMetrics.elidedText(lines.model, Qt::ElideRight, content.width()));
    y += boldMetrics.height() + kLineGap;

    painter->setFont(small);
    const QFontMetrics smallMetrics(small);

    painter->setPen(secondary);
    painter->drawText(QRect(content.left(), y, content.width(), smallMetrics.height()),
                      Qt::AlignLeft | Qt::AlignVCenter,
                      smallMetrics.elidedText(lines.detail, Qt::ElideRight, content.width()));
    y += smallMetrics.height() + kLineGap;

    if (!lines.refusal.isEmpty()) {
        painter->setPen(refusalColour);
        painter->drawText(QRect(content.left(), y, content.width(), smallMetrics.height()),
                          Qt::AlignLeft | Qt::AlignVCenter,
                          smallMetrics.elidedText(lines.refusal, Qt::ElideRight, content.width()));
        y += smallMetrics.height() + kLineGap;
    }

    if (!lines.node.isEmpty()) {
        painter->setPen(tertiary);
        // Elided from the left: the end of a device node is what distinguishes
        // one from another, and the start is the same on every disk.
        painter->drawText(QRect(content.left(), y, content.width(), smallMetrics.height()),
                          Qt::AlignLeft | Qt::AlignVCenter,
                          smallMetrics.elidedText(lines.node, Qt::ElideLeft, content.width()));
    }

    painter->restore();
}
