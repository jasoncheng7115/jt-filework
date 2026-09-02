// One pane: its own tab bar, its own path bar, its own list.
//
// AGENTS.md 7: tabs belong to a pane. There is no window-level tab bar here,
// because there is no window-level tab list in the model either.
#pragma once

#include "bridge.h"

#include <QColor>
#include <QList>
#include <QPoint>
#include <QStringList>
#include <QFont>
#include <QAbstractItemView>
#include <QHash>
#include <QPixmap>
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

private:
    /// Repaint every column of one row, not just the cell Qt thinks changed.
    void repaintRow(const QModelIndex &index);

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
    /// Every row the mouse or Shift-arrow selection covers, in order.
    QList<int> selectedRows() const;
    /// Add or remove the cursor's row from the selection, then step down.
    void toggleCurrentInSelection();
    /// Put the keyboard in the file list.
    void focusList();
    void openCurrentRow();
    /// Run a search from the window's search field.
    void searchFor(const QString &query);
    /// Put the breadcrumb into its editable full-path form.
    void editPath();
    void clearSearch();
    void toggleFilter();
    /// Show or hide the filter bar to match the setting.
    void applyFilterBarSetting();
    void clearFilter();
    // Shows the filter box whenever a filter is actually in force, so a
    // restored one cannot hide rows without saying so.
    void syncFilterBar();
    /// Keep the header's mark-all box in step with the marks.
    void syncMarkAll();
    /// Put the selection back to the marks on arriving in a folder.
    void restoreSelectionFromMarks();
    /// True while doing that, so the write-back does not loop.
    bool m_restoringMarks = false;
    // The folder whose contents the current column widths were measured from,
    // so a resize does not re-measure and make the columns crawl.
    QString m_measuredFor;
    void advanceCurrentRow();
    /// Make the view's selection say what the mark set says.
    void syncSelectionFromMarks();
    /// Whether a press at `at` landed on the row's tick box.
    bool onCheckBox(const QModelIndex &index, const QPoint &at) const;
    void retranslate();
    void setListFont(const QFont &font, const QFont &fixed, bool fixedEverywhere);
    void applyTheme(const QColor &mark, const QColor &directory, const QColor &dim,
                    const QColor &indicator, const QColor &border,
                    const QColor &executable);
    void setActive(bool active);
    void setTarget(bool target);

signals:
    /// A keymap binding fired from inside the list; the window runs it.
    void commandRequested(const QString &id);
    void focusRequested(int paneId);
    /// Try this pane's location again. The window handles it rather than the
    /// pane, because retrying a sign-in has to clear the record of having
    /// already asked for a password - otherwise the second attempt fails the
    /// same way in silence.
    void reconnectRequested(int paneId);
    void stateChanged();
    void selectionChanged();
    // Paths dropped on this pane, and 1 when the drag started inside this
    // application rather than in another one.
    void dropRequested(const QStringList &paths, int fromUs);
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
    void setHoveredRow(int row);
    int m_hoveredRow = -1;
    void scheduleFitNameColumn();
    bool m_fitScheduled = false;
    void fitNameColumn();
    bool m_fittingName = false;
    QList<int> m_wantedColumns;
    void applyViewMode();
    class QAbstractItemView *currentView() const;
    void ensureCurrentRow();
    void setCurrentRow(int row, QAbstractItemView::ScrollHint hint);
    static QString chordFor(const class QKeyEvent *key);
    // Typing letters jumps to a matching row. docs/UI_UX_SPEC.md 5.4: it must
    // never start a rename and never trigger a destructive command.
    bool typeAhead(const QString &text);
    void closeTab(int index);
    QColor m_tabCloseColour;
    QColor m_tabCloseStrong;
    QString m_shownPath;
    void syncTabCloseButtons();
    void syncTabs();
    void syncPath();
    void syncSortIndicator();

    JtfApp *m_app;
    int m_pane;
    QTabBar *m_tabs;
    class Breadcrumb *m_crumbs = nullptr;
    class QToolButton *m_newTab = nullptr;
    class QToolButton *m_close = nullptr;
    class QToolButton *m_reconnect = nullptr;
    class QWidget *m_filterBar = nullptr;
    class QLabel *m_filterIcon = nullptr;
    class QLabel *m_filterCount = nullptr;
    class QToolButton *m_filterClose = nullptr;
    class QLineEdit *m_filter = nullptr;
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
    /// Guards the selection/mark round trip against itself.
    bool m_syncingSelection = false;
    class RowDelegate *m_rows = nullptr;
    /// True only between a press on the column header and its release.
    /// Every column width this widget has applied itself, so a width it did
    /// not apply can be recognised as the user's.
    QHash<int, int> m_appliedWidths;
    class QLabel *m_targetIcon = nullptr;
    class QLabel *m_targetWord = nullptr;
    QPixmap m_targetGlyph;
    class SearchOverlay *m_searchOverlay = nullptr;
    class QWidget *m_targetBadge = nullptr;
    void positionSearchOverlay();
    QColor m_border;
    QString m_typeAhead;
    class QElapsedTimer *m_typeAheadClock = nullptr;
};
