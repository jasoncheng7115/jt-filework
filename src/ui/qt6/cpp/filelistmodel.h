// The file list model.
//
// Deliberately thin: it owns no rows. Every cell is a question asked of Rust
// at paint time, and a virtualized view only paints what is on screen, so the
// cost per frame is proportional to visible rows rather than to directory
// size (AGENTS.md 18.2).
#pragma once

#include "bridge.h"
#include "iconprovider.h"
#include "thumbnails.h"

#include <QAbstractTableModel>
#include <QColor>
#include <QFont>
#include <QMimeData>
#include <QUrl>

class FileListModel : public QAbstractTableModel {
    Q_OBJECT

signals:
    void markChanged();

public:
    /// Identity of the current row set; changes on a new location or re-sort.
    quint64 generation() const { return m_generation; }

public:
    FileListModel(JtfApp *app, int paneId, QObject *parent = nullptr);

    void setPane(int paneId);
    int paneId() const { return m_pane; }

    int rowCount(const QModelIndex &parent = QModelIndex()) const override;
    int columnCount(const QModelIndex &parent = QModelIndex()) const override;
    QVariant data(const QModelIndex &index, int role) const override;
    QVariant headerData(int section, Qt::Orientation orientation, int role) const override;

    // Reconciles the view with Rust. While a directory is still loading the
    // row set only grows, so rows are *inserted* - a full reset on every
    // batch would be four hundred resets for a 100 000-entry directory, and
    // would throw away the selection and the scroll anchor each time. A reset
    // happens only when the row set changes identity: a new location, a
    // re-sort, a filter change.
    void refresh();

    // Drag and drop. The model produces and accepts text/uri-list, which is
    // what Finder, Explorer and every Linux file manager speak, so pane-to-pane
    // and app-to-Finder are the same code path rather than two
    // (docs/PRODUCT_SPEC.md 9).
    bool setData(const QModelIndex &index, const QVariant &value, int role) override;

private:
    int kindColumn() const;
    int tagsColumn() const;
    // The one column whose contents are numbers, so it and its header can be
    // right-aligned together. Found by key, never assumed to be an index.
    int sizeColumn() const;
    bool isAlignedColumn(int column) const;
    int columnWithKey(const QString &wanted) const;

public:
    Qt::ItemFlags flags(const QModelIndex &index) const override;
    QStringList mimeTypes() const override;
    QMimeData *mimeData(const QModelIndexList &indexes) const override;
    Qt::DropActions supportedDragActions() const override;
    Qt::DropActions supportedDropActions() const override;


    void setMarkColor(const QColor &color) { m_markColor = color; }
    void clearIconCache() { m_icons.clear(); }
    void setDirectoryColor(const QColor &color) { m_dirColor = color; }
    void setExecutableColor(const QColor &color) { m_execColor = color; }
    // The ordinary row colour. Needed because a hidden row is drawn as a
    // faded version of the colour it would otherwise have, and for a plain
    // file that colour is this one rather than nothing.
    void setTextColor(const QColor &color) { m_textColor = color; }
    // The two faces the list draws with, and whether the fixed-width one
    // covers every column or only the ones read as columns of values.
    void setListFonts(const QFont &proportional, const QFont &fixed, bool fixedEverywhere);
    void setThumbnailsEnabled(bool on);

private:
    JtfApp *m_app;
    int m_pane;
    QColor m_markColor;
    QColor m_dirColor;
    QColor m_execColor;
    QColor m_textColor;
    QFont m_proportional;
    QFont m_fixed;
    bool m_fixedEverywhere = false;
    mutable IconProvider m_icons;
    ThumbnailCache *m_thumbnails = nullptr;
    bool m_showThumbnails = true;
    quint64 m_generation = 0;
    /// -2 until looked up; -1 when there is no kind column.
    mutable int m_kindColumn = -2;
    mutable int m_tagsColumn = -2;
    mutable int m_sizeColumn = -2;
    int m_rows = 0;
};
