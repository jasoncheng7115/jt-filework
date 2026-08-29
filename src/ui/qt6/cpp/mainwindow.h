// The window: builds a splitter tree from the layout Rust reports, drives the
// event-loop pump, and owns the menus.
//
// It holds no model state of its own. Rebuilding the layout from JSON keeps
// the recursive split tree (AGENTS.md 6) the single source of truth, instead
// of a second tree of widgets drifting away from it.
#pragma once

#include "bridge.h"

#include <QHash>
#include <QMainWindow>

class PaneWidget;
class QSplitter;
class QJsonObject;
class QLabel;

class MainWindow : public QMainWindow {
    Q_OBJECT

public:
    explicit MainWindow(JtfApp *app, QWidget *parent = nullptr);

protected:
    void closeEvent(QCloseEvent *event) override;
    void changeEvent(QEvent *event) override;

private:
    void buildMenus();
    void rebuildLayout();
    QWidget *buildNode(const QJsonObject &node);
    void refreshAll();
    void retranslate();
    void applyTheme();
    void markActivePane();
    QString tr_(const char *key) const;

    JtfApp *m_app;
    QWidget *m_root = nullptr;
    QHash<int, PaneWidget *> m_panes;
    QString m_layoutSignature;
    QLabel *m_statusLeft = nullptr;

    QMenu *m_fileMenu = nullptr;
    QMenu *m_viewMenu = nullptr;
    QMenu *m_goMenu = nullptr;
    QList<QPair<QAction *, const char *>> m_translatable;
    QList<QPair<QMenu *, const char *>> m_translatableMenus;
};
