#include "placeslist.h"

#include "iconprovider.h"
#include "panewidget.h"
#include "dialogbuttons.h"
#include "icons.h"
#include "platform/filetype.h"
#include "jtfstring.h"
#include "platform/quicklook.h"

#include <QDir>
#include <QLineEdit>
#include <QSet>
#include <QFileInfo>
#include <QHeaderView>
#include <QInputDialog>
#include <QMenu>
#include <QStandardPaths>
#include <QStorageInfo>
#include <QTimer>
#include <QAbstractItemView>
#include <QMouseEvent>
#include <QApplication>
#include <QPainter>
#include <QStyledItemDelegate>
#include <QToolButton>
#include <QTreeWidget>
#include <QLabel>
#include <QVBoxLayout>

/// How full a volume is, 0 to 1. Absent on every row that is not a disk.
constexpr int kUsedRole = Qt::UserRole + 4;
/// Whether this row's disk can be ejected.
constexpr int kEjectRole = Qt::UserRole + 5;
/// The eject glyph drawn at the end of a removable volume's row.
constexpr int kEjectSize = 15;
/// The usage bar at the end of a volume's name, and the gap it keeps from it.
constexpr int kGaugeWidth = 34;
constexpr int kGaugeHeight = 6;
constexpr int kGaugeGap = 8;
/// Where a disk stops being comfortable, and where it starts being a problem.
/// Not tuned: 75% is where "plenty left" stops being true, and past 90% most
/// systems are already refusing to do useful things.
constexpr double kGaugeWarn = 0.75;
constexpr double kGaugeFull = 0.90;

/// Where the usage bar sits in a row: hard against the right-hand end.
inline QRect gaugeRectIn(const QRect &cell) {
    return {cell.right() - kGaugeWidth, cell.center().y() - kGaugeHeight / 2, kGaugeWidth,
            kGaugeHeight};
}

/// And where the eject control sits: immediately to its left.
///
/// Both are drawn rather than made widgets. A widget has to live in a column
/// of its own, a tree charges every row for that column's width, and only
/// disks have anything to put there - so an eject button two rows down was
/// costing the bookmarks and the servers the ends of their names.
inline QRect ejectRectIn(const QRect &cell) {
    const QRect gauge = gaugeRectIn(cell);
    return {gauge.left() - kGaugeGap - kEjectSize, cell.center().y() - kEjectSize / 2, kEjectSize,
            kEjectSize};
}

/// Paints a selected or hovered row as one pill, across both columns.
///
/// The stylesheet rounded `::item`, and a stylesheet rounds each *cell*: with
/// a second column holding the eject button and the usage bar, one row was
/// drawn as two rounded rectangles side by side, and the four corners where
/// they met showed through as small dark notches. One rect for the row, drawn
/// once from the first column and stretched to the viewport's edge, has no
/// seam to show.
class PlacesPillDelegate : public QStyledItemDelegate {
public:
    using QStyledItemDelegate::QStyledItemDelegate;

    void setColours(const QColor &selected, const QColor &hovered) {
        m_selected = selected;
        m_hovered = hovered;
    }

    void paint(QPainter *painter, const QStyleOptionViewItem &option,
               const QModelIndex &index) const override {
        QStyleOptionViewItem plain(option);
        initStyleOption(&plain, index);

        const bool selected = plain.state.testFlag(QStyle::State_Selected);
        const bool hovered = plain.state.testFlag(QStyle::State_MouseOver);
        if ((selected || hovered) && index.column() == 0 && plain.widget != nullptr) {
            const auto *view = qobject_cast<const QAbstractItemView *>(plain.widget);
            const QRect viewport =
                view != nullptr ? view->viewport()->rect() : plain.rect;
            // The whole row, from the viewport's own left edge rather than
            // from where the item starts. A tree paints the indentation to the
            // left of a child itself, square-cornered, out of the palette's
            // highlight - so a rounded pill that began at the item left the
            // two shapes meeting, and the pill's rounded corners cut two
            // notches out of the square block behind them. One shape, one set
            // of corners, and nothing behind it to show through.
            painter->save();
            painter->setRenderHint(QPainter::Antialiasing, true);
            painter->setPen(Qt::NoPen);
            painter->setBrush(selected ? m_selected : m_hovered);
            painter->drawRoundedRect(
                QRect(viewport.left(), plain.rect.top(), viewport.width(), plain.rect.height()), 5,
                5);
            painter->restore();
        }
        // The style still draws the icon and the text; only the background is
        // taken away from it, by the stylesheet.
        plain.state.setFlag(QStyle::State_Selected, false);
        plain.state.setFlag(QStyle::State_MouseOver, false);
        if (selected) {
            plain.palette.setColor(QPalette::Text, m_onSelected);
            plain.palette.setColor(QPalette::WindowText, m_onSelected);
        }
        // The name is given up whatever the bar needs, so a long volume name
        // is elided before it rather than drawn underneath it.
        plain.rect.setRight(plain.rect.right() - drawGauge(painter, plain, index));
        QStyle *style = plain.widget != nullptr ? plain.widget->style() : QApplication::style();
        style->drawControl(QStyle::CE_ItemViewItem, &plain, painter, plain.widget);
    }

    void setTextOnSelected(const QColor &colour) { m_onSelected = colour; }

    /// The three colours a usage bar can be, by how full the disk is.
    void setGaugeColours(const QColor &track, const QColor &ok, const QColor &warn,
                         const QColor &full) {
        m_track = track;
        m_ok = ok;
        m_warn = warn;
        m_full = full;
    }

private:
    /// The bar at the right-hand end of a volume's name, and the room it needs.
    ///
    /// Drawn rather than made a widget in the second column: that column costs
    /// every row in the tree its width, and only disks have anything to put
    /// there.
    int drawGauge(QPainter *painter, const QStyleOptionViewItem &option,
                  const QModelIndex &index) const {
        const QVariant stored = index.data(kUsedRole);
        if (!stored.isValid() || index.column() != 0) {
            return 0;
        }
        const double used = qBound(0.0, stored.toDouble(), 1.0);
        const QRect track = gaugeRectIn(option.rect);
        painter->save();
        painter->setRenderHint(QPainter::Antialiasing, true);
        painter->setPen(Qt::NoPen);
        QColor behind = m_track;
        behind.setAlphaF(0.28F);
        painter->setBrush(behind);
        painter->drawRoundedRect(track, kGaugeHeight / 2.0, kGaugeHeight / 2.0);
        // At least a sliver, so an almost-empty disk still reads as a disk
        // with something on it rather than as a missing bar.
        const int width = qMax(2, static_cast<int>(used * track.width()));
        painter->setBrush(used >= kGaugeFull ? m_full : (used >= kGaugeWarn ? m_warn : m_ok));
        painter->drawRoundedRect(QRect(track.left(), track.top(), width, track.height()),
                                 kGaugeHeight / 2.0, kGaugeHeight / 2.0);
        painter->restore();

        int taken = kGaugeWidth + kGaugeGap;
        if (index.data(kEjectRole).toBool()) {
            m_eject.paint(painter, ejectRectIn(option.rect));
            taken += kEjectSize + kGaugeGap;
        }
        return taken;
    }

    QColor m_selected;
    QColor m_hovered;
    QColor m_onSelected;
    QColor m_track, m_ok, m_warn, m_full;

public:
    void setEjectIcon(const QIcon &icon) { m_eject = icon; }

private:
    QIcon m_eject;
};

namespace {
// What an item carries: the path to go to, and which list it came from.
constexpr int kPathRole = Qt::UserRole;
constexpr int kIndexRole = Qt::UserRole + 1;
constexpr int kKindRole = Qt::UserRole + 2;

enum class Kind { Section, Bookmark, Recent, Volume, Favorite, Server };

// The right-hand strip a per-row control sits in. Wide enough to touch
// comfortably, narrow enough not to eat the name beside it.
constexpr int kRowButton = 22;

// Room kept clear at the right edge for an overlay scrollbar. macOS draws its
// scrollbars *over* the content rather than beside it, so a control placed at
// the true right edge is partly hidden the moment the list is long enough to
// scroll. The column is this much wider than the button, and the button stays
// its own width at the column's left, which puts it clear of the overlay.
constexpr int kScrollbarAllowance = 14;



IconProvider &icons() {
    static IconProvider provider;
    return provider;
}
} // namespace

PlacesList::PlacesList(JtfApp *app, QWidget *parent) : QWidget(parent), m_app(app) {
    setObjectName(QStringLiteral("JtfPlaces"));

    // A disk plugged in while the program is running has to appear on its own.
    // `QStorageInfo::mountedVolumes` is a snapshot taken when the list is
    // built, so until something else caused a rebuild the USB stick was simply
    // absent - it turned up the moment the user collapsed a section, which is
    // a confusing way to learn that a rebuild is all it needed.
    //
    // Polling because Qt has no mount notification. The platform ones exist -
    // NSWorkspace on macOS, udev on Linux - and belong in the platform adapter
    // with the rest of the native services; this is the portable stand-in, and
    // it costs one directory scan every few seconds and no rebuild at all
    // unless the set of mounts actually changed.
    auto *watch = new QTimer(this);
    watch->setInterval(2500);
    connect(watch, &QTimer::timeout, this, [this] {
        if (volumeSignature() != m_volumes) {
            refresh();
            return;
        }
        // The same disks, but not necessarily the same amount left on them.
        // Updated in place rather than by rebuilding: a bar that only moved
        // when you happened to navigate somewhere would be a number from
        // whenever you last did that, presented as if it were now.
        updateVolumeUsage();
    });
    watch->start();
    auto *layout = new QVBoxLayout(this);
    layout->setContentsMargins(0, 0, 0, 0);

    // Named, because the two lists in this column look alike: both are trees
    // of folder rows, and without a heading the places run straight into the
    // filesystem tree below with nothing to say where one ends.
    m_title = new QLabel(this);
    m_title->setObjectName(QStringLiteral("JtfSidebarTitle"));
    m_title->setText(tr_("places.title"));
    layout->addWidget(m_title);

    m_tree = new QTreeWidget(this);
    m_tree->setObjectName(QStringLiteral("JtfPlacesTree"));
    m_tree->setHeaderHidden(true);
    // Sections collapse. A sidebar with four sections open is a sidebar you
    // scroll; the point of the thing is that what you want is already on
    // screen.
    m_tree->setRootIsDecorated(true);
    m_tree->setIndentation(14);
    // One column. There was a second, a narrow strip at the right edge for a
    // per-row control, but a tree's column width is shared by every row: the
    // eject button on one removable disk cost every bookmark, server and
    // recent place that much off the end of its name. Both of the things a
    // volume row carries are painted by the delegate now, inside this column.
    m_pill = new PlacesPillDelegate(m_tree);
    m_tree->setItemDelegate(m_pill);
    // And the tree does not get to paint a selection of its own. `drawRow`
    // fills the indentation strip beside a selected child from the palette,
    // which no stylesheet rule reaches; taking the colour away is what stops
    // it, and the delegate above draws the whole row anyway.
    QPalette unhighlighted = m_tree->palette();
    unhighlighted.setColor(QPalette::Highlight, Qt::transparent);
    unhighlighted.setColor(QPalette::Inactive, QPalette::Highlight, Qt::transparent);
    m_tree->setPalette(unhighlighted);
    m_tree->setColumnCount(1);
    m_tree->header()->setSectionResizeMode(0, QHeaderView::Stretch);
    m_tree->viewport()->setMouseTracking(true);
    m_tree->viewport()->installEventFilter(this);
    // What was folded away last time. Kept in the session rather than only in
    // this object: a section the user closed stayed closed until they quit,
    // and then came back open, which reads as the program forgetting.
    const QString stored =
        jtfText([&](char *b, int l) { return jtf_collapsed_sections(m_app, b, l); });
    for (const QString &id : stored.split(QLatin1Char('\n'), Qt::SkipEmptyParts)) {
        m_collapsed.insert(id);
    }
    connect(m_tree, &QTreeWidget::itemExpanded, this, [this](QTreeWidgetItem *item) {
        m_collapsed.remove(item->data(0, Qt::UserRole + 3).toString());
        rememberCollapsed();
    });
    connect(m_tree, &QTreeWidget::itemCollapsed, this, [this](QTreeWidgetItem *item) {
        m_collapsed.insert(item->data(0, Qt::UserRole + 3).toString());
        rememberCollapsed();
    });
    m_tree->setUniformRowHeights(true);
    m_tree->setContextMenuPolicy(Qt::CustomContextMenu);
    m_tree->setSelectionMode(QAbstractItemView::SingleSelection);
    m_tree->setTextElideMode(Qt::ElideRight);
    m_tree->setHorizontalScrollBarPolicy(Qt::ScrollBarAsNeeded);
    m_tree->setMinimumWidth(80);
    layout->addWidget(m_tree);

    // One click, not two: these are destinations, and the sidebar's whole
    // point is getting there quickly.
    connect(m_tree, &QTreeWidget::itemClicked, this, [this](QTreeWidgetItem *item, int) {
        // A server is not a path: it is a host, a port and an account, and
        // opening it means connecting rather than navigating. Sending its
        // label down the path route would try to open a local folder called
        // `jt@example.com`.
        if (static_cast<Kind>(item->data(0, kKindRole).toInt()) == Kind::Server) {
            emit serverActivated(item->data(0, kIndexRole).toInt());
            return;
        }
        const QString path = item->data(0, kPathRole).toString();
        if (!path.isEmpty()) {
            emit locationActivated(path);
        }
    });
    connect(m_tree, &QTreeWidget::itemDoubleClicked, this, [this](QTreeWidgetItem *item, int) {
        // Single click already opens these, but a double click is what a
        // person raised on a file manager tries first, and having it do
        // nothing - or worse, open twice - is a small betrayal.
        if (static_cast<Kind>(item->data(0, kKindRole).toInt()) == Kind::Server) {
            emit serverActivated(item->data(0, kIndexRole).toInt());
        }
    });
    connect(m_tree, &QWidget::customContextMenuRequested, this, &PlacesList::showContextMenu);
}

QString PlacesList::tr_(const char *key) const {
    return jtfText([&](char *buf, int len) { return jtf_tr(m_app, key, buf, len); });
}

QTreeWidgetItem *PlacesList::addSection(const char *labelKey) {
    auto *section = new QTreeWidgetItem(m_tree);
    section->setText(0, tr_(labelKey));
    section->setData(0, kKindRole, static_cast<int>(Kind::Section));
    section->setFlags(Qt::ItemIsEnabled);
    QFont heading = m_listFont;
    heading.setBold(true);
    heading.setPointSizeF(qMax(1.0, m_listFont.pointSizeF() * 0.85));
    section->setFont(0, heading);
    // Keyed by the section's own id, not its label: the label changes with
    // the language and a collapsed section must not reopen when it does.
    section->setData(0, Qt::UserRole + 3, QString::fromLatin1(labelKey));
    section->setExpanded(!m_collapsed.contains(QString::fromLatin1(labelKey)));
    return section;
}

void PlacesList::rememberCollapsed() {
    // Sorted, so the session file does not churn just because two sections
    // were folded in a different order.
    QStringList ids(m_collapsed.constBegin(), m_collapsed.constEnd());
    ids.sort();
    const QByteArray utf8 = ids.join(QLatin1Char('\n')).toUtf8();
    jtf_set_collapsed_sections(m_app, utf8.constData());
}

void PlacesList::setUsageOn(QTreeWidgetItem *item, const QStorageInfo &storage) {
    const qint64 total = storage.bytesTotal();
    const qint64 free = storage.bytesAvailable();
    item->setData(0, kUsedRole,
                  total > 0 ? 1.0 - (static_cast<double>(free) / static_cast<double>(total)) : 0.0);
    // The exact numbers on hover: the bar is for noticing, the tooltip is for
    // when noticing is not enough.
    item->setToolTip(
        0, jtfFill(jtfFill(tr_("places.volume_usage"), "free",
                           PaneWidget::formatSize(static_cast<quint64>(qMax<qint64>(free, 0)))),
                   "total",
                   PaneWidget::formatSize(static_cast<quint64>(qMax<qint64>(total, 0)))));
}

void PlacesList::updateVolumeUsage() {
    for (const auto &row : std::as_const(m_volumeRows)) {
        if (row.first == nullptr) {
            continue;
        }
        const QStorageInfo storage(row.second);
        if (storage.isValid() && storage.isReady()) {
            setUsageOn(row.first, storage);
        }
    }
    m_tree->viewport()->update();
}

QString PlacesList::volumeSignature() {
    // Just enough to notice a disk arriving or leaving. Rebuilding the list
    // unconditionally on a timer would fight the user's scroll position and
    // their selection, so the rebuild is spent only when this actually
    // changes.
    QStringList roots;
    for (const QStorageInfo &storage : QStorageInfo::mountedVolumes()) {
        if (storage.isValid() && storage.isReady()) {
            roots << storage.rootPath();
        }
    }
    roots.sort();
    return roots.join(QLatin1Char('\n'));
}

void PlacesList::refresh() {
    // Remember what was expanded: rebuilding is how this list stays true, and
    // a rebuild that collapses the user's sections is a rebuild they notice.
    m_volumes = volumeSignature();
    m_volumeRows.clear();
    m_tree->clear();

    const auto addChild = [this](QTreeWidgetItem *section, const QString &label,
                                 const QString &path, Kind kind, int index) {
        auto *item = new QTreeWidgetItem(section);
        item->setText(0, label);
        item->setToolTip(0, path);
        item->setData(0, kPathRole, path);
        item->setData(0, kIndexRole, index);
        item->setData(0, kKindRole, static_cast<int>(kind));
        item->setIcon(0, icons().iconFor(path, true));
        item->setFont(0, m_listFont);
        return item;
    };

    // Favorites: the folders a person actually keeps things in. Asked of the
    // platform through QStandardPaths rather than assembled from $HOME, so
    // this is right on a Mac with a localized Desktop, on Windows where
    // Downloads is not under the profile root, and on Linux where XDG says
    // where they are.
    QTreeWidgetItem *favorites = addSection("places.favorites");
    struct Favorite {
        QStandardPaths::StandardLocation location;
        const char *icon;
    };
    static const Favorite kFavorites[] = {
        {QStandardPaths::HomeLocation, "place.home"},
        {QStandardPaths::DesktopLocation, "place.desktop"},
        {QStandardPaths::DocumentsLocation, "place.documents"},
        {QStandardPaths::DownloadLocation, "place.downloads"},
        {QStandardPaths::PicturesLocation, "place.pictures"},
        {QStandardPaths::MusicLocation, "place.music"},
        {QStandardPaths::MoviesLocation, "place.movies"},
    };
    for (const Favorite &favorite : kFavorites) {
        const QString path = QStandardPaths::writableLocation(favorite.location);
        // A location the platform does not have, or that does not exist, is
        // simply absent: a sidebar entry that leads nowhere is worse than a
        // shorter sidebar.
        if (path.isEmpty() || !QFileInfo::exists(path)) {
            continue;
        }
        // The platform's own name first: macOS shows ~/Desktop as 桌面 on a
        // Chinese system while the folder on disk is still called Desktop.
        QString label = filetype::displayName(path);
        if (label.isEmpty()) {
            label = QStandardPaths::displayName(favorite.location);
        }
        if (label.isEmpty()) {
            label = QFileInfo(path).fileName();
        }
        auto *item = addChild(favorites, label, path, Kind::Favorite, -1);
        item->setIcon(0, glyph::forCommand(QString::fromLatin1(favorite.icon), m_glyphColour));
    }

    const int bookmarks = jtf_bookmark_count(m_app);
    if (bookmarks > 0) {
        QTreeWidgetItem *section = addSection("places.bookmarks");
        for (int i = 0; i < bookmarks; ++i) {
            addChild(section,
                     jtfText([&](char *b, int l) { return jtf_bookmark_name(m_app, i, b, l); }),
                     jtfText([&](char *b, int l) { return jtf_bookmark_path(m_app, i, b, l); }),
                     Kind::Bookmark, i);
        }
    }

    // Servers, above the volumes: a machine you connect to is closer to a
    // bookmark than to a disk, and it is the thing you came here to click.
    const int servers = jtf_server_count(m_app);
    if (servers > 0) {
        QTreeWidgetItem *section = addSection("places.servers");
        for (int i = 0; i < servers; ++i) {
            QTreeWidgetItem *item = addChild(
                section, jtfText([&](char *b, int l) { return jtf_server_name(m_app, i, b, l); }),
                QString(), Kind::Server, i);
            if (item != nullptr) {
                // Connected servers are lit and marked with a plug; saved but
                // idle ones are quiet and marked with a window. The difference
                // answers "will clicking this be instant, or is it about to
                // open a connection" without having to try it.
                const bool live = jtf_server_is_connected(m_app, i) != 0;
                item->setIcon(0, glyph::make(live ? glyph::Shape::Connected
                                                  : glyph::Shape::NewWindow,
                                             live ? m_connectedColour : m_glyphColour));
                if (live) {
                    QFont bold = m_listFont;
                    bold.setBold(true);
                    item->setFont(0, bold);
                }
                item->setToolTip(0, tr_(live ? "places.server_connected"
                                             : "places.server_idle"));
            }
        }
    }

    // One section for every mounted disk, internal or not. They were split in
    // two - the idea being that a USB stick you just plugged in should not be
    // buried among the system's own mounts - but with one or two of each the
    // split cost two headings to separate two rows, and the removable ones are
    // already the only rows carrying an eject button. The icon says which kind
    // it is; the heading does not have to.
    QTreeWidgetItem *volumes = nullptr;
    for (const QStorageInfo &storage : QStorageInfo::mountedVolumes()) {
        // Read-only pseudo-filesystems are mounted in their dozens and are not
        // places anyone navigates to.
        if (!storage.isValid() || !storage.isReady() || storage.bytesTotal() <= 0) {
            continue;
        }
        const QString root = storage.rootPath();
        if (root.startsWith(QStringLiteral("/System/Volumes/")) ||
            root.startsWith(QStringLiteral("/dev")) || root.startsWith(QStringLiteral("/private/var/vm"))) {
            continue;
        }
        // Not everything the system has mounted is a place a person goes.
        //
        // Asked by filesystem type rather than by path, because the paths
        // differ per distribution and the answer does not: a squashfs is a
        // package image, a tmpfs is memory, an overlay is a container's view.
        // An Ubuntu machine with the ordinary set of snaps mounts a dozen
        // read-only squashfs images, and every one of them was appearing as a
        // disk that is one hundred per cent full - fourteen red bars burying
        // the two disks the person actually has.
        static const QSet<QByteArray> kNotPlaces = {
            "squashfs", "tmpfs", "devtmpfs", "devfs", "overlay", "overlayfs",
            "proc",     "sysfs", "cgroup",   "cgroup2", "debugfs", "tracefs",
            "securityfs", "pstore", "bpf",   "configfs", "fusectl", "autofs",
            "efivarfs", "ramfs",   "mqueue", "hugetlbfs", "binfmt_misc",
            "nsfs",     "selinuxfs",
        };
        if (kNotPlaces.contains(storage.fileSystemType().toLower())) {
            continue;
        }
        // A snap or an AppImage mount is a package, not a disk, whatever it
        // says its type is.
        if (root.startsWith(QStringLiteral("/snap/")) ||
            root.startsWith(QStringLiteral("/var/lib/snapd/")) ||
            root.startsWith(QStringLiteral("/var/snap/")) ||
            root.startsWith(QStringLiteral("/run/"))) {
            continue;
        }
        QString label = storage.displayName();
        if (label.isEmpty()) {
            label = root;
        }
        // On Windows a volume's name and its letter are both how people refer
        // to it - Explorer shows 「系統碟 (C:)」 - and the name alone is
        // ambiguous the moment two disks are called the same thing. The letter
        // is prepended only when it is not already part of the name.
        const QString letter = root.left(2);
        if (letter.size() == 2 && letter.at(1) == QLatin1Char(':')
            && !label.contains(letter, Qt::CaseInsensitive)) {
            label = QStringLiteral("%1 (%2)").arg(label, letter);
        }
        // Not the boot volume, and mounted where the platform puts removable
        // media. Qt has no "is removable", so this is the honest proxy.
        const bool isRemovable =
            !storage.isRoot() && (root.startsWith(QStringLiteral("/Volumes/")) ||
                                  root.startsWith(QStringLiteral("/media/")) ||
                                  root.startsWith(QStringLiteral("/run/media/")) ||
                                  root.startsWith(QStringLiteral("/mnt/")));
        if (volumes == nullptr) {
            volumes = addSection("places.volumes");
        }
        auto *item = addChild(volumes, label, root, Kind::Volume, -1);
        item->setIcon(0, glyph::forCommand(isRemovable ? QStringLiteral("place.removable")
                                                       : QStringLiteral("place.volume"),
                                           m_glyphColour));
        // A removable disk needs a way off. Pulling one out without ejecting
        // is how filesystems get corrupted, so the control belongs next to
        // the disk rather than buried in a menu - which is where every
        // platform's own file manager puts it.
        // How full it is, carried on the row for the delegate to draw at the
        // right-hand end of the name. Not a widget in the second column: that
        // column is as wide as the widest thing in it and a tree charges every
        // row for it, so a bar there cost bookmarks and recent places forty
        // pixels of name apiece for something they never show.
        setUsageOn(item, storage);
        // A removable disk needs a way off - pulling one out without ejecting
        // is how filesystems get corrupted - so the control stays beside the
        // disk rather than going into a menu. Only its shape has changed.
        item->setData(0, kEjectRole, isRemovable && platform::canEject());
        m_volumeRows.append({item, root});
    }

    const int recents = jtf_recent_count(m_app);
    if (recents > 0) {
        QTreeWidgetItem *section = addSection("places.recent");
        for (int i = 0; i < recents; ++i) {
            const QString path =
                jtfText([&](char *b, int l) { return jtf_recent_path(m_app, i, b, l); });
            QString label = QFileInfo(path).fileName();
            if (label.isEmpty()) {
                label = path;
            }
            addChild(section, label, path, Kind::Recent, i);
        }
    }
}

bool PlacesList::eventFilter(QObject *watched, QEvent *event) {
    if (watched != m_tree->viewport()) {
        return QWidget::eventFilter(watched, event);
    }
    const auto ejectUnder = [this](const QPoint &at) -> QString {
        QTreeWidgetItem *item = m_tree->itemAt(at);
        if (item == nullptr || !item->data(0, kEjectRole).toBool()) {
            return {};
        }
        if (!ejectRectIn(m_tree->visualItemRect(item)).contains(at)) {
            return {};
        }
        return item->data(0, kPathRole).toString();
    };

    if (event->type() == QEvent::MouseButtonPress) {
        auto *mouse = static_cast<QMouseEvent *>(event);
        const QString root = ejectUnder(mouse->position().toPoint());
        if (mouse->button() == Qt::LeftButton && !root.isEmpty()) {
            // Refreshing either way: on success the disk is gone and the row
            // must go with it; on failure the row is still right, and the
            // status line is where the reason belongs.
            if (!platform::eject(root)) {
                emit ejectFailed(root);
            }
            refresh();
            return true; // and not a selection of the row behind it
        }
    }
    if (event->type() == QEvent::MouseMove) {
        // A painted control has no frame to say it is one. The cursor does.
        auto *mouse = static_cast<QMouseEvent *>(event);
        m_tree->viewport()->setCursor(ejectUnder(mouse->position().toPoint()).isEmpty()
                                          ? Qt::ArrowCursor
                                          : Qt::PointingHandCursor);
    }
    return QWidget::eventFilter(watched, event);
}

void PlacesList::applyTheme(const QColor &glyphColour, const QColor &connectedColour,
                           const QColor &gaugeOk, const QColor &gaugeWarn,
                           const QColor &gaugeFull, const QColor &selection,
                           const QColor &hover, const QColor &textOnSelection) {
    if (m_pill != nullptr) {
        m_pill->setColours(selection, hover);
        m_pill->setTextOnSelected(textOnSelection);
        m_pill->setGaugeColours(glyphColour, gaugeOk, gaugeWarn, gaugeFull);
        m_pill->setEjectIcon(glyph::make(glyph::Shape::Eject, glyphColour));
    }
    m_gaugeOk = gaugeOk;
    m_gaugeWarn = gaugeWarn;
    m_gaugeFull = gaugeFull;
    m_glyphColour = glyphColour;
    m_connectedColour = connectedColour;
    refresh();
}

void PlacesList::setListFont(const QFont &font) {
    m_listFont = font;
    refresh();
}

void PlacesList::retranslate() {
    m_title->setText(tr_("places.title"));
    refresh();
}

void PlacesList::showContextMenu(const QPoint &at) {
    QTreeWidgetItem *item = m_tree->itemAt(at);
    if (!item) {
        // Empty space still offers the one thing this list is for. Without
        // this, a right-click below the entries did nothing at all, which
        // reads as "this list cannot be changed".
        QMenu menu(this);
        QAction *add = menu.addAction(
            glyph::make(glyph::Shape::Bookmark, palette().color(QPalette::Text)),
            tr_("places.add"));
        if (menu.exec(m_tree->viewport()->mapToGlobal(at)) == add) {
            emit addBookmarkRequested();
        }
        return;
    }
    const auto kind = static_cast<Kind>(item->data(0, kKindRole).toInt());
    const int index = item->data(0, kIndexRole).toInt();
    const QString path = item->data(0, kPathRole).toString();

    const QColor iconColour = palette().color(QPalette::Text);
    QMenu menu(this);
    // Every row that names a folder can open it in a window of its own. Added
    // first, because it is what you came to this list to do - the rest of each
    // menu is about maintaining the list itself.
    QAction *newWindow = nullptr;
    if (!path.isEmpty() && kind != Kind::Server && kind != Kind::Section) {
        newWindow = menu.addAction(glyph::forCommand(QStringLiteral("tab.tear_off"), iconColour),
                                   tr_("crumb.open_window"));
        menu.addSeparator();
    }
    const auto handledNewWindow = [&](QAction *chosen) {
        if (newWindow == nullptr || chosen != newWindow) {
            return false;
        }
        emit openInNewWindowRequested(path);
        return true;
    };
    if (kind == Kind::Bookmark) {
        QAction *rename =
            menu.addAction(glyph::make(glyph::Shape::Edit, iconColour), tr_("places.rename"));
        QAction *remove =
            menu.addAction(glyph::make(glyph::Shape::Close, iconColour), tr_("places.remove"));
        menu.addSeparator();
        // The order is the user's. Up and down rather than a drag, because a
        // drag inside a tree that also has sections and expandable rows is
        // ambiguous about what a drop between two things means.
        QAction *up = menu.addAction(tr_("places.move_up"));
        QAction *down = menu.addAction(tr_("places.move_down"));
        up->setEnabled(index > 0);
        down->setEnabled(index + 1 < jtf_bookmark_count(m_app));
        menu.addSeparator();
        // Offered only when it would do something. The current folder being
        // already bookmarked is the common case in this menu - it is opened
        // *from* a bookmark - and an item that silently does nothing is worse
        // than no item.
        QAction *addHere = nullptr;
        if (jtf_is_bookmarked(m_app, jtf_active_pane(m_app)) == 0) {
            addHere =
                menu.addAction(glyph::make(glyph::Shape::Bookmark, iconColour), tr_("places.add"));
        }
        QAction *chosen = menu.exec(m_tree->viewport()->mapToGlobal(at));
        if (handledNewWindow(chosen)) {
            return;
        }
        if (chosen == up || chosen == down) {
            jtf_move_bookmark(m_app, index, chosen == up ? index - 1 : index + 1);
            refresh();
            emit placesChanged();
            return;
        }
        if (addHere != nullptr && chosen == addHere) {
            emit addBookmarkRequested();
            return;
        }
        if (chosen == rename) {
            bool accepted = false;
            const QString name = dialogs::askForText(
                this, [this](const char *key) { return tr_(key); }, tr_("places.rename"),
                tr_("places.rename_label"), item->text(0), palette().color(QPalette::Text),
                &accepted);
            if (accepted) {
                const QByteArray utf8 = name.toUtf8();
                jtf_rename_bookmark(m_app, index, utf8.constData());
                refresh();
                emit placesChanged();
            }
        } else if (chosen == remove) {
            jtf_remove_bookmark(m_app, index);
            refresh();
            emit placesChanged();
        }
        return;
    }
    if (kind == Kind::Favorite || kind == Kind::Volume) {
        // A removable disk is the thing an image gets written to, so the way
        // to ask for that belongs on the disk as well as on the image. It
        // does not hand the disk over: the writer is built around nothing
        // being preselected, and the disk still has to be picked there.
        QAction *write = nullptr;
        if (kind == Kind::Volume) {
            write = menu.addAction(glyph::forCommand(QStringLiteral("file.write_image"),
                                                     iconColour),
                                   tr_("command.file.write_image"));
            menu.addSeparator();
        }
        QAction *add = menu.addAction(glyph::make(glyph::Shape::Bookmark, iconColour),
                                      tr_("places.add"));
        QAction *chosen = menu.exec(m_tree->viewport()->mapToGlobal(at));
        if (handledNewWindow(chosen)) {
            return;
        }
        if (write != nullptr && chosen == write) {
            emit writeImageRequested();
            return;
        }
        if (chosen == add) {
            emit addBookmarkRequested();
        }
        return;
    }
    if (kind == Kind::Server) {
        const bool live = jtf_server_is_connected(m_app, index) != 0;
        QAction *connect_ =
            menu.addAction(glyph::make(glyph::Shape::Connected, iconColour),
                           tr_(live ? "places.reconnect" : "places.connect"));
        QAction *disconnect =
            menu.addAction(glyph::make(glyph::Shape::Close, iconColour),
                           tr_("places.disconnect"));
        disconnect->setEnabled(live);
        menu.addSeparator();
        QAction *forget = menu.addAction(glyph::make(glyph::Shape::Close, iconColour),
                                         tr_("places.forget_server"));
        QAction *chosen = menu.exec(m_tree->viewport()->mapToGlobal(at));
        if (chosen == connect_) {
            emit serverActivated(index);
        } else if (chosen == disconnect) {
            jtf_disconnect_server(m_app, index);
            refresh();
        } else if (chosen == forget) {
            jtf_remove_server(m_app, index);
            refresh();
            emit placesChanged();
        }
        return;
    }
    if (kind == Kind::Recent) {
        // A recent place is a folder like any other, and the reason to open
        // this menu on one is usually that you have been back to it enough
        // times to want it kept. Offered only when it is not already a
        // bookmark: an item that would silently do nothing is worse than none.
        const QByteArray utf8 = path.toUtf8();
        QAction *bookmark = nullptr;
        if (!path.isEmpty() && jtf_path_is_bookmarked(m_app, utf8.constData()) == 0) {
            bookmark = menu.addAction(glyph::make(glyph::Shape::Bookmark, iconColour),
                                      tr_("places.add_this"));
            menu.addSeparator();
        }
        QAction *clear = menu.addAction(glyph::make(glyph::Shape::Close, iconColour),
                                        tr_("places.clear_recent"));
        QAction *chosen = menu.exec(m_tree->viewport()->mapToGlobal(at));
        if (handledNewWindow(chosen)) {
            return;
        }
        if (bookmark != nullptr && chosen == bookmark) {
            jtf_toggle_bookmark_path(m_app, utf8.constData());
            refresh();
            emit placesChanged();
            return;
        }
        if (chosen == clear) {
            jtf_clear_recent(m_app);
            refresh();
            emit placesChanged();
        }
    }
}
