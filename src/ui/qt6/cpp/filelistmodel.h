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

    // Called when Rust says the rows changed. A full reset is correct here:
    // an enumeration replaces the row set, and Qt's view keeps its scroll
    // anchor.
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
};
