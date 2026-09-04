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

#include <QColor>
#include <QSet>
#include <QFont>
#include <QWidget>

class QTreeWidget;
class QTreeWidgetItem;

class QLabel;

class PlacesList : public QWidget {
    Q_OBJECT

public:
    explicit PlacesList(JtfApp *app, QWidget *parent = nullptr);

    void refresh();
    void setListFont(const QFont &font);
    /// `gaugeOk`, `gaugeWarn` and `gaugeFull` colour the volume usage bars.
    /// Clicks and hovers on the eject control, which is painted rather than a
    /// widget, so there is nothing to receive them on its own.
    bool eventFilter(QObject *watched, QEvent *event) override;

    void applyTheme(const QColor &glyphColour, const QColor &connectedColour,
                    const QColor &gaugeOk, const QColor &gaugeWarn, const QColor &gaugeFull,
                    const QColor &selection, const QColor &hover,
                    const QColor &textOnSelection);
    void retranslate();

signals:
    void locationActivated(const QString &path);
    /// A bookmark was renamed, removed or reordered; the window persists.
    void placesChanged();
    /// The user asked to bookmark the folder they are looking at.
    void addBookmarkRequested();
    /// Open this folder in a window of its own.
    void openInNewWindowRequested(const QString &path);
    /// A saved server was clicked; the index is into the saved list.
    void serverActivated(int index);
    /// Ejecting a removable volume did not work. The window says why; a
    /// button that quietly does nothing is worse than one that reports.
    void ejectFailed(const QString &mountPoint);
    /// A volume was ejected. Panes showing it are now showing a disk that is
    /// not there.
    void volumeEjected(const QString &mountPoint);
    /// Write a disk image, asked for from a removable volume's own menu.
    ///
    /// Carries no disk with it on purpose. The writer preselects nothing -
    /// see the comment at the top of `imagewriterdialog.h` - and a disk
    /// arriving already chosen is exactly the reflex confirmation that
    /// rule exists to prevent.
    void writeImageRequested();

private:
    QLabel *m_title = nullptr;
    QString tr_(const char *key) const;
    QTreeWidgetItem *addSection(const char *labelKey);
    void showContextMenu(const QPoint &at);

    // The set of mounted volumes as of the last rebuild, so the watch can tell
    // a real change from a quiet tick.
    void rememberCollapsed();
    static QString volumeSignature();
    /// Put a disk's free/total on its row, for the bar and for the tooltip.
    void setUsageOn(class QTreeWidgetItem *item, const class QStorageInfo &storage);
    /// Re-read every mounted disk and redraw the bars, without rebuilding.
    void updateVolumeUsage();
    /// The volume rows and the disks they are about, so the bars can be
    /// brought up to date without touching the rest of the list.
    QList<QPair<class QTreeWidgetItem *, QString>> m_volumeRows;
    QString m_volumes;

    JtfApp *m_app = nullptr;
    QTreeWidget *m_tree = nullptr;
    QFont m_listFont;
    QColor m_glyphColour;
    QColor m_gaugeOk, m_gaugeWarn, m_gaugeFull;
    class PlacesPillDelegate *m_pill = nullptr;
    QColor m_connectedColour;
    /// Sections the user closed, by section id.
    QSet<QString> m_collapsed;
};
