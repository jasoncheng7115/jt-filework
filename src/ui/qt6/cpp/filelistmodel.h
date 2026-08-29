// The file list model.
//
// Deliberately thin: it owns no rows. Every cell is a question asked of Rust
// at paint time, and a virtualized view only paints what is on screen, so the
// cost per frame is proportional to visible rows rather than to directory
// size (AGENTS.md 18.2).
#pragma once

#include "bridge.h"
#include "iconprovider.h"

#include <QAbstractTableModel>
#include <QColor>

class FileListModel : public QAbstractTableModel {
    Q_OBJECT

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

    void setMarkColor(const QColor &color) { m_markColor = color; }
    void clearIconCache() { m_icons.clear(); }
    void setDirectoryColor(const QColor &color) { m_dirColor = color; }

private:
    JtfApp *m_app;
    int m_pane;
    QColor m_markColor;
    QColor m_dirColor;
    mutable IconProvider m_icons;
    quint64 m_generation = 0;
    int m_rows = 0;
};
