#include "icons.h"

#include <QByteArray>
#include <QCoreApplication>
#include <QDir>
#include <QFile>
#include <QHash>
#include <QPainter>
#include <QPixmap>
#include <QSvgRenderer>

namespace {

/// Each shape's Iconoir file, without the extension.
///
/// A missing entry is a programming error rather than a missing icon, so the
/// lookup below draws an obvious placeholder instead of nothing: an icon that
/// silently fails to appear is a toolbar button that looks disabled.
const QHash<glyph::Shape, QString> &glyphFiles() {
    static const QHash<glyph::Shape, QString> files = {
        {glyph::Shape::ArrowLeft, QStringLiteral("nav-arrow-left")},
        {glyph::Shape::ArrowRight, QStringLiteral("nav-arrow-right")},
        {glyph::Shape::ArrowUp, QStringLiteral("nav-arrow-up")},
        {glyph::Shape::Reload, QStringLiteral("refresh-double")},
        {glyph::Shape::Sidebar, QStringLiteral("sidebar-collapse")},
        {glyph::Shape::SplitHorizontal, QStringLiteral("view-columns-2")},
        {glyph::Shape::SplitVertical, QStringLiteral("view-columns-3")},
        {glyph::Shape::NewFolder, QStringLiteral("folder-plus")},
        {glyph::Shape::Filter, QStringLiteral("filter-list")},
        {glyph::Shape::Search, QStringLiteral("search")},
        {glyph::Shape::Hidden, QStringLiteral("eye-closed")},
        {glyph::Shape::Settings, QStringLiteral("settings")},
        {glyph::Shape::Close, QStringLiteral("xmark")},
        {glyph::Shape::Inspector, QStringLiteral("view-columns-2")},
        {glyph::Shape::Keyboard, QStringLiteral("key-command")},
        {glyph::Shape::Home, QStringLiteral("home-simple")},
        {glyph::Shape::Bookmark, QStringLiteral("bookmark")},
        {glyph::Shape::Recent, QStringLiteral("clock-rotate-right")},
        {glyph::Shape::Volume, QStringLiteral("hard-drive")},
        {glyph::Shape::Grid, QStringLiteral("view-grid")},
        {glyph::Shape::List, QStringLiteral("list")},
        {glyph::Shape::Edit, QStringLiteral("page-edit")},
    };
    return files;
}

/// Where the vendored icons live.
///
/// Beside the locales and keymaps: inside the bundle when there is one, and
/// up the source tree when running from a build directory.
QString iconRoot() {
    static const QString root = [] {
        QDir dir(QCoreApplication::applicationDirPath());
        const QString bundled = dir.absoluteFilePath(QStringLiteral("../Resources/icons/iconoir"));
        if (QDir(bundled).exists()) {
            return QDir(bundled).absolutePath();
        }
        for (int i = 0; i < 16; ++i) {
            const QString candidate =
                dir.absoluteFilePath(QStringLiteral("assets/icons/iconoir"));
            if (QDir(candidate).exists()) {
                return QDir(candidate).absolutePath();
            }
            if (!dir.cdUp()) {
                break;
            }
        }
        return QString();
    }();
    return root;
}

/// The SVG source for a shape, with `currentColor` replaced by `colour`.
///
/// Iconoir strokes with `currentColor`, which Qt's SVG renderer does not
/// resolve - it is a CSS cascade value and there is no cascade here. Doing it
/// as a text substitution keeps one file serving every theme.
QByteArray tintedSource(glyph::Shape shape, const QColor &colour) {
    const QString name = glyphFiles().value(shape);
    if (name.isEmpty() || iconRoot().isEmpty()) {
        return {};
    }
    QFile file(iconRoot() + QLatin1Char('/') + name + QStringLiteral(".svg"));
    if (!file.open(QIODevice::ReadOnly)) {
        return {};
    }
    QByteArray source = file.readAll();
    source.replace("currentColor", colour.name(QColor::HexRgb).toLatin1());
    return source;
}

} // namespace

namespace glyph {

QIcon make(Shape shape, const QColor &colour) {
    const QByteArray source = tintedSource(shape, colour);
    QIcon icon;
    // Several sizes, so the icon stays crisp wherever it is used and on any
    // display scale.
    for (const int size : {16, 20, 24, 32, 48}) {
        QPixmap pixmap(size, size);
        pixmap.fill(Qt::transparent);
        QPainter painter(&pixmap);
        painter.setRenderHint(QPainter::Antialiasing, true);
        if (source.isEmpty()) {
            // Visible placeholder: a missing glyph must look wrong rather
            // than look like a disabled button.
            painter.setPen(QPen(colour, 1.3));
            painter.drawRect(QRectF(size * 0.2, size * 0.2, size * 0.6, size * 0.6));
        } else {
            QSvgRenderer renderer(source);
            renderer.render(&painter, QRectF(0, 0, size, size));
        }
        painter.end();
        icon.addPixmap(pixmap);
    }
    return icon;
}

} // namespace glyph
