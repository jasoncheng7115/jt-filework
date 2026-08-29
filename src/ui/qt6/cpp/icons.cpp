#include "icons.h"

#include <QPainter>
#include <QPainterPath>
#include <QPixmap>

namespace {

QPainterPath chevron(qreal size, glyph::Shape shape) {
    // A chevron rather than a filled triangle: it reads as navigation at
    // 16px, where a triangle turns into a blob.
    const qreal c = size / 2.0;
    const qreal r = size * 0.22;
    QPainterPath path;

    switch (shape) {
    case glyph::Shape::ArrowLeft:
        path.moveTo(c + r * 0.7, c - r);
        path.lineTo(c - r * 0.7, c);
        path.lineTo(c + r * 0.7, c + r);
        break;
    case glyph::Shape::ArrowRight:
        path.moveTo(c - r * 0.7, c - r);
        path.lineTo(c + r * 0.7, c);
        path.lineTo(c - r * 0.7, c + r);
        break;
    case glyph::Shape::ArrowUp:
        path.moveTo(c - r, c + r * 0.7);
        path.lineTo(c, c - r * 0.7);
        path.lineTo(c + r, c + r * 0.7);
        break;
    case glyph::Shape::Reload:
        path.addEllipse(QPointF(c, c), r, r);
        break;
    }
    return path;
}

QPixmap render(glyph::Shape shape, const QColor &colour, int size, qreal ratio) {
    QPixmap pixmap(int(size * ratio), int(size * ratio));
    pixmap.setDevicePixelRatio(ratio);
    pixmap.fill(Qt::transparent);

    QPainter painter(&pixmap);
    painter.setRenderHint(QPainter::Antialiasing, true);
    QPen pen(colour);
    pen.setWidthF(qMax(1.4, size * 0.11));
    pen.setCapStyle(Qt::RoundCap);
    pen.setJoinStyle(Qt::RoundJoin);
    painter.setPen(pen);
    painter.setBrush(Qt::NoBrush);
    painter.drawPath(chevron(size, shape));
    return pixmap;
}

} // namespace

QIcon glyph::make(Shape shape, const QColor &colour) {
    QIcon icon;
    for (int size : {16, 20, 24, 32}) {
        for (qreal ratio : {1.0, 2.0}) {
            icon.addPixmap(render(shape, colour, size, ratio));
        }
    }
    // A disabled control uses the same glyph at reduced opacity, which the
    // stylesheet's disabled colour then reinforces.
    return icon;
}
