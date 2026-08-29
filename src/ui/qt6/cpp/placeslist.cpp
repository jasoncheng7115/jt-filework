#include "placeslist.h"

#include "iconprovider.h"
#include "icons.h"
#include "platform/filetype.h"
#include "jtfstring.h"

#include <QDir>
#include <QLineEdit>
#include <QSet>
#include <QFileInfo>
#include <QHeaderView>
#include <QInputDialog>
#include <QMenu>
#include <QStandardPaths>
#include <QStorageInfo>
#include <QTreeWidget>
#include <QVBoxLayout>

namespace {
// What an item carries: the path to go to, and which list it came from.
constexpr int kPathRole = Qt::UserRole;
constexpr int kIndexRole = Qt::UserRole + 1;
constexpr int kKindRole = Qt::UserRole + 2;

enum class Kind { Section, Bookmark, Recent, Volume, Favorite };

IconProvider &icons() {
    static IconProvider provider;
    return provider;
}
} // namespace

PlacesList::PlacesList(JtfApp *app, QWidget *parent) : QWidget(parent), m_app(app) {
    setObjectName(QStringLiteral("JtfPlaces"));
    auto *layout = new QVBoxLayout(this);
    layout->setContentsMargins(0, 0, 0, 0);

    m_tree = new QTreeWidget(this);
    m_tree->setObjectName(QStringLiteral("JtfPlacesTree"));
    m_tree->setHeaderHidden(true);
    // Sections collapse. A sidebar with four sections open is a sidebar you
    // scroll; the point of the thing is that what you want is already on
    // screen.
    m_tree->setRootIsDecorated(true);
    m_tree->setIndentation(14);
    connect(m_tree, &QTreeWidget::itemExpanded, this, [this](QTreeWidgetItem *item) {
        m_collapsed.remove(item->data(0, Qt::UserRole + 3).toString());
    });
    connect(m_tree, &QTreeWidget::itemCollapsed, this, [this](QTreeWidgetItem *item) {
        m_collapsed.insert(item->data(0, Qt::UserRole + 3).toString());
    });
    m_tree->setUniformRowHeights(true);
    m_tree->setContextMenuPolicy(Qt::CustomContextMenu);
    m_tree->setSelectionMode(QAbstractItemView::SingleSelection);
    layout->addWidget(m_tree);

    // One click, not two: these are destinations, and the sidebar's whole
    // point is getting there quickly.
    connect(m_tree, &QTreeWidget::itemClicked, this, [this](QTreeWidgetItem *item, int) {
        const QString path = item->data(0, kPathRole).toString();
        if (!path.isEmpty()) {
            emit locationActivated(path);
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

void PlacesList::refresh() {
    // Remember what was expanded: rebuilding is how this list stays true, and
    // a rebuild that collapses the user's sections is a rebuild they notice.
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

    // Volumes, with anything removable in its own section: a USB stick you
    // just plugged in is the reason you opened the sidebar, and burying it
    // among the system's own mounts is how you fail to find it.
    QTreeWidgetItem *volumes = nullptr;
    QTreeWidgetItem *removable = nullptr;
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
        QString label = storage.displayName();
        if (label.isEmpty()) {
            label = root;
        }
        // Not the boot volume, and mounted where the platform puts removable
        // media. Qt has no "is removable", so this is the honest proxy.
        const bool isRemovable =
            !storage.isRoot() && (root.startsWith(QStringLiteral("/Volumes/")) ||
                                  root.startsWith(QStringLiteral("/media/")) ||
                                  root.startsWith(QStringLiteral("/run/media/")) ||
                                  root.startsWith(QStringLiteral("/mnt/")));
        QTreeWidgetItem *&section = isRemovable ? removable : volumes;
        if (section == nullptr) {
            section = addSection(isRemovable ? "places.devices" : "places.volumes");
        }
        auto *item = addChild(section, label, root, Kind::Volume, -1);
        item->setIcon(0, glyph::forCommand(isRemovable ? QStringLiteral("place.removable")
                                                       : QStringLiteral("place.volume"),
                                           m_glyphColour));
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

void PlacesList::applyTheme(const QColor &glyphColour) {
    m_glyphColour = glyphColour;
    refresh();
}

void PlacesList::setListFont(const QFont &font) {
    m_listFont = font;
    refresh();
}

void PlacesList::retranslate() {
    refresh();
}

void PlacesList::showContextMenu(const QPoint &at) {
    QTreeWidgetItem *item = m_tree->itemAt(at);
    if (!item) {
        return;
    }
    const auto kind = static_cast<Kind>(item->data(0, kKindRole).toInt());
    const int index = item->data(0, kIndexRole).toInt();

    QMenu menu(this);
    if (kind == Kind::Bookmark) {
        QAction *rename = menu.addAction(tr_("places.rename"));
        QAction *remove = menu.addAction(tr_("places.remove"));
        QAction *chosen = menu.exec(m_tree->viewport()->mapToGlobal(at));
        if (chosen == rename) {
            bool accepted = false;
            const QString name = QInputDialog::getText(this, tr_("places.rename"),
                                                       tr_("places.rename_label"),
                                                       QLineEdit::Normal, item->text(0), &accepted);
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
    if (kind == Kind::Recent) {
        QAction *clear = menu.addAction(tr_("places.clear_recent"));
        if (menu.exec(m_tree->viewport()->mapToGlobal(at)) == clear) {
            jtf_clear_recent(m_app);
            refresh();
            emit placesChanged();
        }
    }
}
