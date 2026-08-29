#include "placeslist.h"

#include "iconprovider.h"
#include "jtfstring.h"

#include <QDir>
#include <QLineEdit>
#include <QSet>
#include <QFileInfo>
#include <QHeaderView>
#include <QInputDialog>
#include <QMenu>
#include <QStorageInfo>
#include <QTreeWidget>
#include <QVBoxLayout>

namespace {
// What an item carries: the path to go to, and which list it came from.
constexpr int kPathRole = Qt::UserRole;
constexpr int kIndexRole = Qt::UserRole + 1;
constexpr int kKindRole = Qt::UserRole + 2;

enum class Kind { Section, Bookmark, Recent, Volume };

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
    m_tree->setRootIsDecorated(false);
    m_tree->setIndentation(12);
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
    section->setExpanded(true);
    return section;
}

void PlacesList::refresh() {
    // Remember what was expanded: rebuilding is how this list stays true, and
    // a rebuild that collapses the user's sections is a rebuild they notice.
    QSet<QString> collapsed;
    for (int i = 0; i < m_tree->topLevelItemCount(); ++i) {
        QTreeWidgetItem *item = m_tree->topLevelItem(i);
        if (!item->isExpanded()) {
            collapsed.insert(item->text(0));
        }
    }
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

    QTreeWidgetItem *volumes = addSection("places.volumes");
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
        addChild(volumes, label, root, Kind::Volume, -1);
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
