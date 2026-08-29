#include "icons.h"

#include <QPainter>
#include <QPainterPath>
#include <QPixmap>

namespace {

// Everything is drawn inside a 16x16 box and scaled, so the whole set shares
// one grid and one stroke weight.
constexpr qreal kGrid = 16.0;

void drawShape(QPainter &painter, glyph::Shape shape, qreal size) {
    const qreal k = size / kGrid;
    const auto x = [k](qreal units) { return units * k; };

    QPainterPath path;
    switch (shape) {
    case glyph::Shape::ArrowLeft:
        path.moveTo(x(10), x(4));
        path.lineTo(x(6), x(8));
        path.lineTo(x(10), x(12));
        break;
    case glyph::Shape::ArrowRight:
        path.moveTo(x(6), x(4));
        path.lineTo(x(10), x(8));
        path.lineTo(x(6), x(12));
        break;
    case glyph::Shape::ArrowUp:
        path.moveTo(x(4), x(10));
        path.lineTo(x(8), x(6));
        path.lineTo(x(12), x(10));
        break;
    case glyph::Shape::Reload:
        // An open circle with a tick, which reads as "again" at 16px where a
        // closed circle reads as a dot.
        path.arcMoveTo(x(3.5), x(3.5), x(9), x(9), 60);
        path.arcTo(x(3.5), x(3.5), x(9), x(9), 60, 300);
        path.moveTo(x(12.5), x(3));
        path.lineTo(x(12.5), x(6.5));
        path.lineTo(x(9), x(6.5));
        break;
    case glyph::Shape::Sidebar:
        path.addRoundedRect(x(2.5), x(3), x(11), x(10), x(1.5), x(1.5));
        path.moveTo(x(6.5), x(3));
        path.lineTo(x(6.5), x(13));
        break;
    case glyph::Shape::SplitHorizontal:
        path.addRoundedRect(x(2.5), x(3), x(11), x(10), x(1.5), x(1.5));
        path.moveTo(x(8), x(3));
        path.lineTo(x(8), x(13));
        break;
    case glyph::Shape::SplitVertical:
        path.addRoundedRect(x(2.5), x(3), x(11), x(10), x(1.5), x(1.5));
        path.moveTo(x(2.5), x(8));
        path.lineTo(x(13.5), x(8));
        break;
    case glyph::Shape::NewFolder:
        path.moveTo(x(2), x(12.5));
        path.lineTo(x(2), x(4));
        path.lineTo(x(6.5), x(4));
        path.lineTo(x(8), x(5.5));
        path.lineTo(x(12), x(5.5));
        path.lineTo(x(12), x(9));
        path.moveTo(x(2), x(12.5));
        path.lineTo(x(12), x(12.5));
        // The plus that makes it "new".
        path.moveTo(x(11.5), x(10.5));
        path.lineTo(x(15), x(10.5));
        path.moveTo(x(13.25), x(8.75));
        path.lineTo(x(13.25), x(12.25));
        break;
    case glyph::Shape::Filter:
        path.moveTo(x(2.5), x(4));
        path.lineTo(x(13.5), x(4));
        path.lineTo(x(9.5), x(8.5));
        path.lineTo(x(9.5), x(13));
        path.lineTo(x(6.5), x(11.5));
        path.lineTo(x(6.5), x(8.5));
        path.closeSubpath();
        break;
    case glyph::Shape::Search:
        path.addEllipse(QPointF(x(7), x(7)), x(4), x(4));
        path.moveTo(x(10), x(10));
        path.lineTo(x(13.5), x(13.5));
        break;
    case glyph::Shape::Hidden:
        // An eye with a stroke through it: "show what is normally not shown".
        path.moveTo(x(2), x(8));
        path.quadTo(x(8), x(3), x(14), x(8));
        path.quadTo(x(8), x(13), x(2), x(8));
        path.moveTo(x(3.5), x(13));
        path.lineTo(x(12.5), x(3));
        break;
    case glyph::Shape::Settings:
        path.addEllipse(QPointF(x(8), x(8)), x(2.5), x(2.5));
        path.addEllipse(QPointF(x(8), x(8)), x(5.5), x(5.5));
        break;
    }
    painter.drawPath(path);
}

QPixmap render(glyph::Shape shape, const QColor &colour, int size, qreal ratio) {
    QPixmap pixmap(int(size * ratio), int(size * ratio));
    pixmap.setDevicePixelRatio(ratio);
    pixmap.fill(Qt::transparent);

    QPainter painter(&pixmap);
    painter.setRenderHint(QPainter::Antialiasing, true);
    QPen pen(colour);
    pen.setWidthF(qMax(1.3, size * 0.085));
    pen.setCapStyle(Qt::RoundCap);
    pen.setJoinStyle(Qt::RoundJoin);
    painter.setPen(pen);
    painter.setBrush(Qt::NoBrush);
    drawShape(painter, shape, size);
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
    return icon;
}
