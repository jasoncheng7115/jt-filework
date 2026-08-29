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
#include <QAbstractItemView>
#include <QWidget>

class FileListModel;
class QTabBar;
class QLabel;
class QTableView;

class PaneWidget : public QWidget {
    Q_OBJECT

public:
    /// Human-readable byte count, shared with the window's status bar so the
    /// two never disagree about units.
    static QString formatSize(quint64 bytes);

    PaneWidget(JtfApp *app, int paneId, QWidget *parent = nullptr);
    ~PaneWidget() override;

    int paneId() const { return m_pane; }
    void refresh();
    // Rows and status only: what changes while a directory streams in.
    void refreshRows();
    // Row the keyboard is on, or -1. The window needs it for commands that
    // act on the focused entry.
    int currentRow() const;
    /// Put the keyboard in the file list.
    void focusList();
    void openCurrentRow();
    void toggleSearch();
    /// Run a search from the window's search field.
    void searchFor(const QString &query);
    /// Put the breadcrumb into its editable full-path form.
    void editPath();
    void clearSearch();
    void toggleFilter();
    void clearFilter();
    void advanceCurrentRow();
    void retranslate();
    void setListFont(const QFont &font);
    void applyTheme(const QColor &mark, const QColor &directory, const QColor &dim,
                    const QColor &indicator, const QColor &border,
                    const QColor &executable);
    void setActive(bool active);

signals:
    /// A keymap binding fired from inside the list; the window runs it.
    void commandRequested(const QString &id);
    void focusRequested(int paneId);
    void stateChanged();
    void selectionChanged();
    // Paths dropped on this pane, and 0 for copy or 1 for move.
    void dropRequested(const QStringList &paths, int kind);
    void contextMenuRequested(const QPoint &global, bool onEntry);
    void crumbMenuRequested(const QString &path, const QPoint &global);
    /// Move this tab into a window of its own.
    void tearOffRequested(int tabIndex);
    /// A tab from `fromPane` was dropped on this pane's strip.
    void tabMergeRequested(int fromPane, int tabIndex, int intoPane);

protected:
    void resizeEvent(class QResizeEvent *event) override;
    bool eventFilter(QObject *watched, QEvent *event) override;

private:
    void openRow(int row);
    bool handleDrop(class QDropEvent *event);
    void showContextMenu(const QPoint &position);
    void showHeaderMenu(const QPoint &position);
    void applyColumnVisibility();
    bool isPathColumn(int column) const;
    void fitNameColumn();
    void applyViewMode();
    class QAbstractItemView *currentView() const;
    void ensureCurrentRow();
    void setCurrentRow(int row, QAbstractItemView::ScrollHint hint);
    static QString chordFor(const class QKeyEvent *key);
    // Typing letters jumps to a matching row. docs/UI_UX_SPEC.md 5.4: it must
    // never start a rename and never trigger a destructive command.
    bool typeAhead(const QString &text);
    void syncTabs();
    void syncPath();
    void syncSortIndicator();

    JtfApp *m_app;
    int m_pane;
    QTabBar *m_tabs;
    class Breadcrumb *m_crumbs = nullptr;
    class QToolButton *m_newTab = nullptr;
    class QWidget *m_filterBar = nullptr;
    class QLabel *m_filterIcon = nullptr;
    class QLabel *m_filterCount = nullptr;
    class QToolButton *m_filterClose = nullptr;
    class QLineEdit *m_filter = nullptr;
    class QLineEdit *m_search = nullptr;
    QLabel *m_status;
    QTableView *m_view;
    FileListModel *m_model;
    bool m_active = false;
    QColor m_indicator;
    quint64 m_positionedGeneration = 0;
    /// Tab being dragged, or -1.
    int m_dragTab = -1;
    QPoint m_dragOrigin;
    class JtfHeaderView *m_header = nullptr;
    class QListView *m_grid = nullptr;
    class MatchDelegate *m_matches = nullptr;
    QColor m_border;
    QString m_typeAhead;
    class QElapsedTimer *m_typeAheadClock = nullptr;
};
