// One pane: its own tab bar, its own path bar, its own list.
//
// AGENTS.md 7: tabs belong to a pane. There is no window-level tab bar here,
// because there is no window-level tab list in the model either.
#pragma once

#include "bridge.h"

#include <QColor>
#include <QWidget>

class FileListModel;
class QTabBar;
class QLabel;
class QTableView;

class PaneWidget : public QWidget {
    Q_OBJECT

public:
    PaneWidget(JtfApp *app, int paneId, QWidget *parent = nullptr);

    int paneId() const { return m_pane; }
    void refresh();
    void retranslate();
    void applyTheme(const QColor &mark, const QColor &directory, const QColor &indicator,
                    const QColor &border);
    void setActive(bool active);

signals:
    void focusRequested(int paneId);
    void stateChanged();

protected:
    bool eventFilter(QObject *watched, QEvent *event) override;

private:
    void openRow(int row);
    void syncTabs();
    void syncPath();

    JtfApp *m_app;
    int m_pane;
    QTabBar *m_tabs;
    QLabel *m_path;
    QLabel *m_status;
    QTableView *m_view;
    FileListModel *m_model;
    bool m_active = false;
    QColor m_indicator;
    QColor m_border;
};
