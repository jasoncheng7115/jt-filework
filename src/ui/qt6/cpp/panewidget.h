// One pane: its own tab bar, its own path bar, its own list.
//
// AGENTS.md 7: tabs belong to a pane. There is no window-level tab bar here,
// because there is no window-level tab list in the model either.
#pragma once

#include "bridge.h"

#include <QColor>
#include <QPoint>
#include <QStringList>
#include <QFont>
#include <QWidget>

class FileListModel;
class QTabBar;
class QLabel;
class QTableView;

class PaneWidget : public QWidget {
    Q_OBJECT

public:
    PaneWidget(JtfApp *app, int paneId, QWidget *parent = nullptr);
    ~PaneWidget() override;

    int paneId() const { return m_pane; }
    void refresh();
    // Rows and status only: what changes while a directory streams in.
    void refreshRows();
    // Row the keyboard is on, or -1. The window needs it for commands that
    // act on the focused entry.
    int currentRow() const;
    void openCurrentRow();
    void toggleFilter();
    void clearFilter();
    void advanceCurrentRow();
    void retranslate();
    void setListFont(const QFont &font);
    void applyTheme(const QColor &mark, const QColor &directory, const QColor &indicator,
                    const QColor &border);
    void setActive(bool active);

signals:
    void focusRequested(int paneId);
    void stateChanged();
    void selectionChanged();
    // Paths dropped on this pane, and 0 for copy or 1 for move.
    void dropRequested(const QStringList &paths, int kind);
    void contextMenuRequested(const QPoint &global, bool onEntry);

protected:
    bool eventFilter(QObject *watched, QEvent *event) override;

private:
    void openRow(int row);
    bool handleDrop(class QDropEvent *event);
    void showContextMenu(const QPoint &position);
    void showHeaderMenu(const QPoint &position);
    void applyColumnVisibility();
    // Typing letters jumps to a matching row. docs/UI_UX_SPEC.md 5.4: it must
    // never start a rename and never trigger a destructive command.
    bool typeAhead(const QString &text);
    void syncTabs();
    void syncPath();
    void syncSortIndicator();

    JtfApp *m_app;
    int m_pane;
    QTabBar *m_tabs;
    QLabel *m_path;
    class QLineEdit *m_filter = nullptr;
    QLabel *m_status;
    QTableView *m_view;
    FileListModel *m_model;
    bool m_active = false;
    QColor m_indicator;
    QColor m_border;
    QString m_typeAhead;
    class QElapsedTimer *m_typeAheadClock = nullptr;
};
