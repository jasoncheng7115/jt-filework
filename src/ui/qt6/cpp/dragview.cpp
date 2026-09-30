#include "dragview.h"

#include <QAbstractItemModel>
#include <QDrag>
#include <QFontMetrics>
#include <QIcon>
#include <QItemSelectionModel>
#include <QMimeData>
#include <QPainter>
#include <QPixmap>
#include <QUrl>

namespace {

// The image of a drag: the first row's icon and name on the list's own
// background, and - when more than one file is going - the count in a badge,
// as Finder and Explorer show it. Colours come from the palette, which the
// theme sets, so it reads in both themes.
QPixmap dragImage(const QAbstractItemView *view, const QModelIndex &index, int count) {
    constexpr int kPad = 6;
    constexpr int kNameWidth = 260;
    const QFont font = view->font();
    const QFontMetrics metrics(font);
    const int side = qMax(16, metrics.height());
    const QString name = metrics.elidedText(index.data(Qt::DisplayRole).toString(),
                                            Qt::ElideMiddle, kNameWidth);
    const QString badge = count > 1 ? QString::number(count) : QString();
    const int badgeWidth =
        badge.isEmpty() ? 0 : qMax(metrics.height() + 4, metrics.horizontalAdvance(badge) + 12);

    const int height = side + 2 * kPad;
    const int width = kPad + side + kPad + metrics.horizontalAdvance(name) + kPad
                      + (badge.isEmpty() ? 0 : badgeWidth + kPad);
    const qreal ratio = view->devicePixelRatioF();
    QPixmap image(QSize(width, height) * ratio);
    image.setDevicePixelRatio(ratio);
    image.fill(Qt::transparent);

    const QPalette palette = view->palette();
    QPainter painter(&image);
    painter.setRenderHint(QPainter::Antialiasing);
    painter.setFont(font);

    QColor background = palette.color(QPalette::Base);
    background.setAlpha(230);
    painter.setPen(palette.color(QPalette::Mid));
    painter.setBrush(background);
    painter.drawRoundedRect(QRectF(0.5, 0.5, width - 1, height - 1), 6, 6);

    const QVariant decoration = index.data(Qt::DecorationRole);
    QIcon icon = decoration.value<QIcon>();
    if (icon.isNull()) {
        icon = QIcon(decoration.value<QPixmap>());
    }
    if (!icon.isNull()) {
        icon.paint(&painter, QRect(kPad, kPad, side, side));
    }

    painter.setPen(palette.color(QPalette::Text));
    const QRect nameRect(kPad + side + kPad, 0, metrics.horizontalAdvance(name), height);
    painter.drawText(nameRect, Qt::AlignVCenter | Qt::AlignLeft, name);

    if (!badge.isEmpty()) {
        const QRectF pill(width - kPad - badgeWidth, (height - (metrics.height() + 4)) / 2.0,
                          badgeWidth, metrics.height() + 4);
        painter.setPen(Qt::NoPen);
        painter.setBrush(palette.color(QPalette::Highlight));
        painter.drawRoundedRect(pill, pill.height() / 2, pill.height() / 2);
        painter.setPen(palette.color(QPalette::HighlightedText));
        painter.drawText(pill, Qt::AlignCenter, badge);
    }
    return image;
}

} // namespace

void jtfStartDrag(QAbstractItemView *view, Qt::DropActions supported) {
    QAbstractItemModel *model = view->model();
    QItemSelectionModel *selection = view->selectionModel();
    if (model == nullptr || selection == nullptr) {
        return;
    }
    QModelIndexList indexes;
    for (const QModelIndex &index : selection->selectedIndexes()) {
        if (index.column() == 0 && model->flags(index).testFlag(Qt::ItemIsDragEnabled)) {
            indexes.append(index);
        }
    }
    if (indexes.isEmpty()) {
        return;
    }
    QMimeData *data = model->mimeData(indexes);
    if (data == nullptr) {
        return;
    }
    const int count = data->hasUrls() ? int(data->urls().size()) : 1;

    // Deleted by Qt once the drag is over, as QAbstractItemView's own is.
    auto *drag = new QDrag(view);
    drag->setMimeData(data);
    const QPixmap image = dragImage(view, indexes.first(), count);
    drag->setPixmap(image);
    drag->setHotSpot(QPoint(12, int(image.height() / image.devicePixelRatio() / 2)));

    // The same choice of default as Qt's startDrag: the view's own when the
    // source allows it, otherwise copy.
    Qt::DropAction fallback = Qt::IgnoreAction;
    if (view->defaultDropAction() != Qt::IgnoreAction
        && supported.testFlag(view->defaultDropAction())) {
        fallback = view->defaultDropAction();
    } else if (supported.testFlag(Qt::CopyAction)) {
        fallback = Qt::CopyAction;
    }
    drag->exec(supported, fallback);
}
