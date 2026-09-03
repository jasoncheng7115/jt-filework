#include "icons.h"

#include <QByteArray>
#include <QCoreApplication>
#include <QDir>
#include <QFile>
#include <QHash>
#include <QPainter>
#include <QPixmap>
#include <QStandardPaths>
#include <QStringList>
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
        // Named for the divider, drawn as the result.
        //
        // `Orientation::Horizontal` puts the two panes side by side, so its
        // picture is a box split by an upright line. `Vertical` stacks them,
        // so its picture is a box split by a lying-down one. The vertical
        // split used to show *three* columns, which is neither the right
        // count nor the right direction.
        {glyph::Shape::SplitHorizontal, QStringLiteral("view-columns-2")},
        {glyph::Shape::SplitVertical, QStringLiteral("split-rows-2")},
        {glyph::Shape::SplitRows, QStringLiteral("split-rows-2")},
        {glyph::Shape::NewFolder, QStringLiteral("folder-plus")},
        {glyph::Shape::Filter, QStringLiteral("filter-list")},
        {glyph::Shape::Search, QStringLiteral("search")},
        {glyph::Shape::Hidden, QStringLiteral("eye-closed")},
        {glyph::Shape::Settings, QStringLiteral("settings")},
        {glyph::Shape::Close, QStringLiteral("xmark")},
        {glyph::Shape::Theme, QStringLiteral("theme")},
        {glyph::Shape::Font, QStringLiteral("text-size")},
        {glyph::Shape::Language, QStringLiteral("language")},
        // Not view-columns-2: SplitHorizontal already uses it, so the
        // inspector button looked like a second split button.
        {glyph::Shape::Inspector, QStringLiteral("inspector")},
        {glyph::Shape::Keyboard, QStringLiteral("key-command")},
        // A hint, not a bar. The strip's job is to tell you what you can
        // press right now; a picture of the strip's own shape describes the
        // furniture rather than what it is for.
        {glyph::Shape::HintBar, QStringLiteral("light-bulb")},
        {glyph::Shape::Visible, QStringLiteral("eye")},
        {glyph::Shape::Check, QStringLiteral("check")},
        {glyph::Shape::SplitQuad, QStringLiteral("split-quad")},
        {glyph::Shape::SplitSingle, QStringLiteral("square")},
        {glyph::Shape::Copy, QStringLiteral("copy")},
        {glyph::Shape::NewWindow, QStringLiteral("open-new-window")},
        {glyph::Shape::Connected, QStringLiteral("server-connection")},
        {glyph::Shape::Eject, QStringLiteral("eject")},
        {glyph::Shape::ArrowDown, QStringLiteral("nav-arrow-down")},
        {glyph::Shape::Home, QStringLiteral("home-simple")},
        {glyph::Shape::Bookmark, QStringLiteral("bookmark")},
        {glyph::Shape::Recent, QStringLiteral("clock-rotate-right")},
        {glyph::Shape::Volume, QStringLiteral("hard-drive")},
        {glyph::Shape::Grid, QStringLiteral("view-grid")},
        {glyph::Shape::List, QStringLiteral("list")},
        // Folders gathered at the top, against one run of everything.
        {glyph::Shape::FoldersFirst, QStringLiteral("folder-plus")},
        {glyph::Shape::SortMixed, QStringLiteral("sort")},
        {glyph::Shape::Edit, QStringLiteral("page-edit")},
    };
    return files;
}

/// Where the vendored icons live.
///
/// Beside the locales and keymaps: inside the bundle when there is one, and
/// up the source tree when running from a build directory.
/// Both sets: the vendored Iconoir icons, and the few drawn here for shapes
/// Iconoir does not have. Two directories rather than one so the vendored set
/// stays exactly as it was vendored - a hand-drawn file dropped in among them
/// would be covered by their licence and their attribution, and it is not.
QStringList iconRoots() {
    static const QStringList roots = [] {
        QStringList found;
        // Directory names, not glyph names.
        static const char *const sets[] = {"iconoir", "jtf"};
        for (const char *const setName : sets) {
            const QString set = QString::fromLatin1(setName);
            QDir dir(QCoreApplication::applicationDirPath());
            const QString bundled =
                dir.absoluteFilePath(QStringLiteral("../Resources/icons/") + set);
            if (QDir(bundled).exists()) {
                found.append(QDir(bundled).absolutePath());
                continue;
            }
            // Beside the executable, where a build with no bundle to put them
            // in keeps them. Checked before the walk up the source tree, so an
            // installed build does not depend on finding one.
            const QString alongside = dir.absoluteFilePath(QStringLiteral("icons/") + set);
            if (QDir(alongside).exists()) {
                found.append(QDir(alongside).absolutePath());
                continue;
            }
            for (int i = 0; i < 16; ++i) {
                const QString candidate =
                    dir.absoluteFilePath(QStringLiteral("assets/icons/") + set);
                if (QDir(candidate).exists()) {
                    found.append(QDir(candidate).absolutePath());
                    break;
                }
                if (!dir.cdUp()) {
                    break;
                }
            }
        }
        return found;
    }();
    return roots;
}

/// The file for an icon name, searched across both sets, or empty.
QString iconFile(const QString &name) {
    const QStringList roots = iconRoots();
    for (const QString &root : roots) {
        const QString candidate = root + QLatin1Char('/') + name + QStringLiteral(".svg");
        if (QFile::exists(candidate)) {
            return candidate;
        }
    }
    return {};
}

/// The SVG source for a shape, with `currentColor` replaced by `colour`.
///
/// Iconoir strokes with `currentColor`, which Qt's SVG renderer does not
/// resolve - it is a CSS cascade value and there is no cascade here. Doing it
/// as a text substitution keeps one file serving every theme.
QByteArray tintedSource(glyph::Shape shape, const QColor &colour) {
    const QString name = glyphFiles().value(shape);
    if (name.isEmpty()) {
        return {};
    }
    QFile file(iconFile(name));
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
        {QStringLiteral("file.view_hex"), QStringLiteral("terminal")},
        {QStringLiteral("file.edit"), QStringLiteral("page-edit")},
        {QStringLiteral("file.reveal"), QStringLiteral("open-new-window")},
        {QStringLiteral("file.folder_size"), QStringLiteral("sort")},
        // Out of a container, and into one.
        {QStringLiteral("file.extract"), QStringLiteral("open-in-browser")},
        {QStringLiteral("file.compress"), QStringLiteral("multiple-pages")},
        {QStringLiteral("file.compare_panes"), QStringLiteral("compare-folders")},
        {QStringLiteral("file.disk_usage"), QStringLiteral("disk-usage")},
        {QStringLiteral("view.folders_first"), QStringLiteral("sort")},
        // The picture is of the port, because that is what the person is
        // looking at while they decide which disk this is.
        {QStringLiteral("file.write_image"), QStringLiteral("usb")},
        {QStringLiteral("file.copy_to_target_pane"), QStringLiteral("copy")},
        {QStringLiteral("file.move_to_target_pane"), QStringLiteral("arrow-separate-vertical")},
        // The chooser forms of the same two commands - the single keys C and
        // M. They wear their two-pane siblings' pictures because they are the
        // same action, only asked about first.
        {QStringLiteral("file.copy_to"), QStringLiteral("copy")},
        {QStringLiteral("file.move_to"), QStringLiteral("arrow-separate-vertical")},
        {QStringLiteral("file.terminal"), QStringLiteral("terminal")},
        {QStringLiteral("file.share"), QStringLiteral("open-new-window")},
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
        {QStringLiteral("help.about"), QStringLiteral("light-bulb")},
        {QStringLiteral("view.key_hints"), QStringLiteral("light-bulb")},
        {QStringLiteral("view.mode.list"), QStringLiteral("list")},
        {QStringLiteral("view.mode.grid"), QStringLiteral("view-grid")},
        {QStringLiteral("view.thumbnails"), QStringLiteral("media-image")},
        {QStringLiteral("theme.set"), QStringLiteral("light-bulb")},
        // Search
        {QStringLiteral("search.open"), QStringLiteral("search")},
        {QStringLiteral("search.clear"), QStringLiteral("xmark")},
        {QStringLiteral("search.ai"), QStringLiteral("light-bulb")},
        {QStringLiteral("ai.ask"), QStringLiteral("light-bulb")},
        {QStringLiteral("command.palette"), QStringLiteral("tools")},
        // Tabs and panes
        {QStringLiteral("tab.new"), QStringLiteral("plus")},
        {QStringLiteral("tab.close"), QStringLiteral("xmark")},
        {QStringLiteral("tab.duplicate"), QStringLiteral("multiple-pages")},
        {QStringLiteral("tab.reopen"), QStringLiteral("redo")},
        {QStringLiteral("workspace.split.horizontal"), QStringLiteral("view-columns-2")},
        {QStringLiteral("workspace.split.vertical"), QStringLiteral("split-rows-2")},
        {QStringLiteral("workspace.preset.single"), QStringLiteral("square-dashed")},
        {QStringLiteral("workspace.preset.quad"), QStringLiteral("table-2-columns")},
        {QStringLiteral("workspace.pane.close"), QStringLiteral("xmark")},
        {QStringLiteral("workspace.pane.next"), QStringLiteral("nav-arrow-right")},
        // The same arrow the pane list draws beside the target pane.
        {QStringLiteral("workspace.target.next"), QStringLiteral("arrow-separate-vertical")},
        // Walking the areas of the window, not the panes inside it: the
        // sidebar glyph, because the sidebar is where the walk starts.
        {QStringLiteral("workspace.focus.next"), QStringLiteral("sidebar-collapse")},
        {QStringLiteral("workspace.pane.previous"), QStringLiteral("nav-arrow-left")},
        {QStringLiteral("tab.next"), QStringLiteral("nav-arrow-right")},
        {QStringLiteral("tab.previous"), QStringLiteral("nav-arrow-left")},
        {QStringLiteral("tab.pin"), QStringLiteral("bookmark")},
        {QStringLiteral("tab.move_to_pane"), QStringLiteral("arrow-separate-vertical")},
        // Servers. Connected and not are two states of one thing, so they
        // share the plug and differ by colour, which the sidebar already does.
        {QStringLiteral("remote.connect"), QStringLiteral("server-connection")},
        {QStringLiteral("remote.disconnect"), QStringLiteral("xmark-circle")},
        {QStringLiteral("jobs.cancel_active"), QStringLiteral("xmark-circle")},
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
    if (name.isEmpty()) {
        return {};
    }
    QFile file(iconFile(name));
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

QString stylesheetImage(Shape shape, const QColor &colour, int size) {
    const QString cache = QStandardPaths::writableLocation(QStandardPaths::CacheLocation);
    if (cache.isEmpty()) {
        return {};
    }
    const QString dir = cache + QStringLiteral("/stylesheet-glyphs");
    if (!QDir().mkpath(dir)) {
        return {};
    }
    // Keyed by everything that changes the picture, so switching theme picks
    // up a different file rather than a stale one, and switching back does
    // not re-render.
    const QString path = QStringLiteral("%1/%2-%3-%4.png")
                             .arg(dir)
                             .arg(static_cast<int>(shape))
                             .arg(colour.name(QColor::HexRgb).mid(1))
                             .arg(size);
    if (!QFile::exists(path)) {
        // Rendered at three times the size it is drawn at, so it stays clean
        // on a scaled display; the stylesheet gives the width and height.
        QPixmap pixmap = make(shape, colour).pixmap(QSize(size, size) * 3);
        if (pixmap.isNull() || !pixmap.save(path, "PNG")) {
            return {};
        }
    }
    return path;
}

bool hasCommandIcon(const QString &id) { return commandFiles().contains(id); }

} // namespace glyph
