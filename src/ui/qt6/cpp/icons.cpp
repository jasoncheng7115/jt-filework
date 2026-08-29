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

/// Command id to icon file. Names only, resolved the same way shapes are.
const QHash<QString, QString> &commandFiles() {
    static const QHash<QString, QString> files = {
        // Navigation
        {QStringLiteral("nav.back"), QStringLiteral("nav-arrow-left")},
        {QStringLiteral("nav.forward"), QStringLiteral("nav-arrow-right")},
        {QStringLiteral("nav.up"), QStringLiteral("nav-arrow-up")},
        {QStringLiteral("nav.home"), QStringLiteral("home-simple")},
        {QStringLiteral("nav.goto"), QStringLiteral("terminal")},
        {QStringLiteral("file.bookmark"), QStringLiteral("bookmark")},
        // Files
        {QStringLiteral("file.open"), QStringLiteral("open-in-browser")},
        {QStringLiteral("file.new_folder"), QStringLiteral("folder-plus")},
        {QStringLiteral("file.new_file"), QStringLiteral("page-plus")},
        {QStringLiteral("file.attributes"), QStringLiteral("tools")},
        {QStringLiteral("view.sort"), QStringLiteral("sort")},
        {QStringLiteral("file.rename"), QStringLiteral("edit-pencil")},
        {QStringLiteral("file.batch_rename"), QStringLiteral("multiple-pages")},
        {QStringLiteral("file.duplicate"), QStringLiteral("copy")},
        {QStringLiteral("file.trash"), QStringLiteral("bin-half")},
        {QStringLiteral("file.delete"), QStringLiteral("trash")},
        {QStringLiteral("file.undo"), QStringLiteral("undo")},
        {QStringLiteral("file.view"), QStringLiteral("eye")},
        {QStringLiteral("file.edit"), QStringLiteral("page-edit")},
        {QStringLiteral("file.reveal"), QStringLiteral("open-new-window")},
        {QStringLiteral("file.folder_size"), QStringLiteral("sort")},
        {QStringLiteral("file.copy_to_target_pane"), QStringLiteral("copy")},
        {QStringLiteral("file.move_to_target_pane"), QStringLiteral("arrow-separate-vertical")},
        // Clipboard
        {QStringLiteral("file.clipboard.copy"), QStringLiteral("copy")},
        {QStringLiteral("file.clipboard.cut"), QStringLiteral("scissor")},
        {QStringLiteral("file.clipboard.paste"), QStringLiteral("page-plus")},
        {QStringLiteral("file.copy_path"), QStringLiteral("terminal")},
        {QStringLiteral("file.copy_name"), QStringLiteral("text-size")},
        // Marks
        {QStringLiteral("file.mark.toggle"), QStringLiteral("check")},
        {QStringLiteral("file.mark.all"), QStringLiteral("frame-select")},
        {QStringLiteral("file.mark.none"), QStringLiteral("square-dashed")},
        {QStringLiteral("file.mark.invert"), QStringLiteral("xmark-circle")},
        {QStringLiteral("file.mark.pattern"), QStringLiteral("plus")},
        {QStringLiteral("file.unmark.pattern"), QStringLiteral("minus")},
        // View
        {QStringLiteral("view.tree"), QStringLiteral("sidebar-collapse")},
        {QStringLiteral("view.inspector"), QStringLiteral("view-columns-2")},
        {QStringLiteral("view.refresh"), QStringLiteral("refresh-double")},
        {QStringLiteral("view.hidden"), QStringLiteral("eye-closed")},
        {QStringLiteral("view.filter"), QStringLiteral("filter-list")},
        {QStringLiteral("view.font.larger"), QStringLiteral("expand-lines")},
        {QStringLiteral("view.font.smaller"), QStringLiteral("reduce")},
        {QStringLiteral("keymap.toggle"), QStringLiteral("key-command")},
        {QStringLiteral("help.shortcuts"), QStringLiteral("key-command")},
        // Search
        {QStringLiteral("search.open"), QStringLiteral("search")},
        {QStringLiteral("search.clear"), QStringLiteral("xmark")},
        {QStringLiteral("command.palette"), QStringLiteral("tools")},
        // Tabs and panes
        {QStringLiteral("tab.new"), QStringLiteral("plus")},
        {QStringLiteral("tab.close"), QStringLiteral("xmark")},
        {QStringLiteral("tab.duplicate"), QStringLiteral("multiple-pages")},
        {QStringLiteral("tab.reopen"), QStringLiteral("redo")},
        {QStringLiteral("workspace.split.horizontal"), QStringLiteral("view-columns-2")},
        {QStringLiteral("workspace.split.vertical"), QStringLiteral("view-columns-3")},
        {QStringLiteral("workspace.preset.single"), QStringLiteral("square-dashed")},
        {QStringLiteral("workspace.preset.quad"), QStringLiteral("table-2-columns")},
        {QStringLiteral("workspace.pane.close"), QStringLiteral("xmark")},
        // Settings and preview
        {QStringLiteral("settings.open"), QStringLiteral("settings")},
        {QStringLiteral("preview.quicklook"), QStringLiteral("eye")},
        {QStringLiteral("preview.toggle"), QStringLiteral("eye")},
        {QStringLiteral("jobs.show"), QStringLiteral("list")},
        {QStringLiteral("locale.set"), QStringLiteral("language")},
        // Sidebar places. Not commands, but they belong in this table for the
        // same reason: one place in the program decides what each thing looks
        // like.
        {QStringLiteral("place.home"), QStringLiteral("home-simple")},
        {QStringLiteral("place.desktop"), QStringLiteral("computer")},
        {QStringLiteral("place.documents"), QStringLiteral("page-edit")},
        {QStringLiteral("place.downloads"), QStringLiteral("download")},
        {QStringLiteral("place.pictures"), QStringLiteral("media-image")},
        {QStringLiteral("place.music"), QStringLiteral("music-double-note")},
        {QStringLiteral("place.movies"), QStringLiteral("media-video")},
        {QStringLiteral("place.volume"), QStringLiteral("hard-drive")},
        {QStringLiteral("place.removable"), QStringLiteral("usb")},
        {QStringLiteral("place.bookmark"), QStringLiteral("bookmark")},
        {QStringLiteral("place.recent"), QStringLiteral("clock-rotate-right")},
    };
    return files;
}

QByteArray tintedFile(const QString &name, const QColor &colour) {
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

QIcon renderIcon(const QByteArray &source, const QColor &colour, bool placeholder) {
    QIcon icon;
    for (const int size : {16, 20, 24, 32, 48}) {
        QPixmap pixmap(size, size);
        pixmap.fill(Qt::transparent);
        QPainter painter(&pixmap);
        painter.setRenderHint(QPainter::Antialiasing, true);
        if (source.isEmpty()) {
            if (!placeholder) {
                return {};
            }
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

} // namespace

namespace glyph {

QIcon make(Shape shape, const QColor &colour) {
    // A named shape that will not load is a programming error, so it draws a
    // visible placeholder: a missing toolbar glyph must look wrong rather than
    // look like a disabled button.
    return renderIcon(tintedSource(shape, colour), colour, true);
}

QIcon forCommand(const QString &id, const QColor &colour) {
    // No placeholder here. Menus are a long list, and a row of identical
    // boxes beside the commands nobody has drawn yet is worse than a menu
    // where only some rows carry a picture.
    return renderIcon(tintedFile(commandFiles().value(id), colour), colour, false);
}

bool hasCommandIcon(const QString &id) { return commandFiles().contains(id); }

} // namespace glyph
