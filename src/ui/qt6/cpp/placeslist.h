// The sidebar's places: bookmarks, recent locations and mounted volumes.
//
// Q-Dir and QSpace both put this above the folder tree, and it is where the
// eye goes first: the tree is for exploring, this is for the six folders you
// actually live in.
//
// Volumes are not stored - they are whatever is mounted right now, asked of
// Qt each time the list is built. Bookmarks and recents come from Rust, which
// owns them and persists them.
#pragma once

#include "bridge.h"

#include <QFont>
#include <QWidget>

class QTreeWidget;
class QTreeWidgetItem;

class PlacesList : public QWidget {
    Q_OBJECT

public:
    explicit PlacesList(JtfApp *app, QWidget *parent = nullptr);

    void refresh();
    void setListFont(const QFont &font);
    void retranslate();

signals:
    void locationActivated(const QString &path);
    /// A bookmark was renamed, removed or reordered; the window persists.
    void placesChanged();

private:
    QString tr_(const char *key) const;
    QTreeWidgetItem *addSection(const char *labelKey);
    void showContextMenu(const QPoint &at);

    JtfApp *m_app = nullptr;
    QTreeWidget *m_tree = nullptr;
    QFont m_listFont;
};
