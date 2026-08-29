#include "theme.h"

namespace {
QColor tokenColour(const JtfApp *app, bool dark, int token) {
    return QColor::fromRgba(jtf_theme_color(app, dark ? 1 : 0, token));
}
} // namespace

Theme Theme::fromApp(const JtfApp *app, bool systemIsDark) {
    const auto c = [&](int token) { return tokenColour(app, systemIsDark, token); };
    Theme t;
    t.window = c(TokenSurfaceWindow);
    t.pane = c(TokenSurfacePane);
    t.preview = c(TokenSurfacePreview);
    t.header = c(TokenSurfaceHeader);
    t.rowAlternate = c(TokenRowAlternate);
    t.rowHover = c(TokenRowHover);
    t.textPrimary = c(TokenTextPrimary);
    t.textSecondary = c(TokenTextSecondary);
    t.textOnAccent = c(TokenTextOnAccent);
    t.border = c(TokenBorder);
    t.selection = c(TokenSelectionActive);
    t.selectionInactive = c(TokenSelectionInactive);
    t.mark = c(TokenMarkActive);
    t.focusRing = c(TokenFocusRing);
    t.indicator = c(TokenPaneActiveIndicator);
    t.executable = c(TokenTextExecutable);
    t.error = c(TokenStatusError);
    return t;
}

QString Theme::styleSheet() const {
    const auto hex = [](const QColor &colour) { return colour.name(QColor::HexRgb); };

    return QStringLiteral(R"(
QMainWindow, QWidget#JtfRoot { background: %WINDOW%; }

QToolBar#JtfToolbar {
    background: %HEADER%;
    padding: 5px 8px;
    spacing: 3px;
    border: none;
    border-bottom: 1px solid %BORDER%;
    spacing: 4px;
    padding: 4px 6px;
}
QToolBar#JtfToolbar QToolButton {
    color: %TEXT%;
    padding: 4px 8px;
    border: 1px solid transparent;
    border-radius: 6px;
}
QToolBar#JtfToolbar QToolButton { padding: 5px; border-radius: 6px; }
/* A boxed cluster, so related buttons read as one control. */
/* A filled well rather than an outlined box. An outline around every group
   draws three rectangles across the toolbar and competes with the icons; a
   slightly recessed fill groups them without adding lines. */
QWidget[jtfToolGroup="true"] {
    background: %WINDOW%;
    border: none;
    border-radius: 8px;
}
QWidget[jtfToolGroup="true"] QToolButton { padding: 4px; border-radius: 6px; }
QWidget[jtfToolGroup="true"] QToolButton:hover { background: %HOVER%; }
QWidget[jtfToolGroup="true"] QToolButton:checked { background: %SEL%; }
QToolBar#JtfToolbar QToolButton:hover { background: %HOVER%; }
QToolBar#JtfToolbar QToolButton:pressed { background: %ALT%; }
QToolBar#JtfToolbar QToolButton:disabled { color: %DIM%; }
/* A pressed-in look for the toggles, so "the sidebar is open" is readable
   from the toolbar itself. */
QToolBar#JtfToolbar QToolButton:checked {
    background: %SELDIM%;
    border: 1px solid %BORDER%;
}
QToolBar#JtfToolbar QToolButton:checked:hover { background: %HOVER%; }
QToolBar#JtfToolbar::separator {
    background: %BORDER%;
    width: 1px;
    margin: 5px 5px;
}

QLineEdit {
    background: %PANE%;
    color: %TEXT%;
    border: 1px solid %BORDER%;
    border-radius: 6px;
    padding: 4px 8px;
    selection-background-color: %SEL%;
    selection-color: %ONSEL%;
}
QLineEdit:focus { border: 1px solid %FOCUS%; }

QTableView {
    background: %PANE%;
    alternate-background-color: %ALT%;
    color: %TEXT%;
    border: none;
    outline: none;
    selection-background-color: %SEL%;
    selection-color: %ONSEL%;
}
/* Rows breathe. 2px of padding puts the text against its own row edge, which
   is what makes a dense list read as cramped rather than as compact. */
QTableView::item { padding: 5px 8px; border: none; }

/* The icon grid, over the same model as the list. */
QListView#JtfGrid {
    background: %PANE%;
    color: %TEXT%;
    border: none;
    outline: none;
}
QListView#JtfGrid::item { padding: 6px; border-radius: 6px; }
QListView#JtfGrid::item:hover { background: %HOVER%; }
QListView#JtfGrid::item:selected { background: %SEL%; color: %ONSEL%; }
QListView#JtfGrid::item:selected:!active { background: %SELDIM%; color: %TEXT%; }
QTableView::item:hover { background: %HOVER%; }
QTableView::item:selected { background: %SEL%; color: %ONSEL%; }
QTableView::item:selected:!active { background: %SELDIM%; color: %TEXT%; }

/* The mark checkbox. Qt's default is a heavy platform box that dominates the
   first column; this is a quiet square that reads as part of the row until it
   is ticked, and then reads as the accent colour everything else selected
   uses. */
QTableView::indicator {
    width: 13px;
    height: 13px;
    border: 1px solid %BORDER%;
    border-radius: 3px;
    background: transparent;
}
QTableView::indicator:hover { border-color: %DIM%; }
QTableView::indicator:checked {
    background: %SEL%;
    border-color: %SEL%;
    image: none;
}

QHeaderView::section {
    background: %HEADER%;
    /* Painted by JtfHeaderView; this is the fallback for any header that is
       not one of ours. */
    color: %DIM%;
    padding: 6px 8px;
    border: none;
    /* One rule under the whole header, not a grid of dividers. A vertical
       line between every column draws the table's structure instead of its
       contents, and the columns are already legible from their alignment. */
    border-bottom: 1px solid %BORDER%;
    font-weight: 500;
}
QHeaderView::section:hover { color: %TEXT%; background: %HOVER%; }
/* The sort caret is painted by JtfHeaderView, beside the header text rather
   than at the section's right edge, so it stays next to the word it refers
   to. Qt's own indicator is switched off there; these rules make sure the
   style cannot bring one back. */
QHeaderView::up-arrow, QHeaderView::down-arrow { image: none; width: 0; height: 0; }

/* The active tab is marked three ways at once - a lit accent rule along its
   top, the pane's own background, and full-strength text against dimmed
   neighbours. One of those alone is what made the tab bar unreadable: a tab
   that differs from its neighbour only by a hairline border is not a tab bar,
   it is a row of words. The reference layout uses the same three. */
QTabBar { background: %HEADER%; qproperty-drawBase: 0; }
QTabBar::tab {
    background: %HEADER%;
    color: %DIM%;
    padding: 6px 16px 6px 6px;
    margin: 0;
    border: none;
    border-top: 2px solid transparent;
    border-right: 1px solid %BORDER%;
    min-width: 60px;
    max-width: 220px;
}
QTabBar::tab:hover { background: %HOVER%; color: %TEXT%; }
QTabBar::tab:selected {
    background: %PANE%;
    color: %TEXT%;
    border-top: 2px solid %SEL%;
    font-weight: 600;
}
/* macOS puts the close control on the leading edge. It needs its own room:
   flush against the tab's border it reads as a rendering fault. */
QTabBar::close-button {
    subcontrol-position: left;
    margin-left: 8px;
    margin-right: 2px;
    padding: 2px;
    border-radius: 4px;
}
QTabBar::close-button:hover { background: %HOVER%; }
QWidget#JtfTabRow { background: %HEADER%; border-bottom: 1px solid %BORDER%; }

/* The active pane. The edge marks it, the tab strip brightens with it, and
   the inactive panes' tabs go quiet so only one strip looks lit at a time. */
QWidget#JtfPane { border-top: 1px solid %BORDER%; border-right: 1px solid %BORDER%; }
QWidget#JtfPane[jtfActive="true"] { border-top: 2px solid %SEL%; }
QTabBar[jtfActive="false"]::tab:selected {
    background: %HEADER%;
    color: %DIM%;
    border-top: 2px solid %BORDER%;
    font-weight: 500;
}
QWidget#JtfCrumbs[jtfActive="true"] { background: %ALT%; }
QToolButton#JtfNewTab {
    color: %DIM%;
    background: transparent;
    border: none;
    padding: 0 10px;
    font-size: 15px;
}
QToolButton#JtfNewTab:hover { color: %TEXT%; background: %HOVER%; }

QLineEdit#JtfFilter, QLineEdit#JtfSearch { margin: 2px 6px 4px 6px; }
QWidget#JtfCrumbs { background: %HEADER%; border-bottom: 1px solid %BORDER%; }
QWidget#JtfCrumbs QPushButton {
    color: %TEXT%;
    background: transparent;
    border: none;
    padding: 2px 7px;
    border-radius: 4px;
}
QWidget#JtfCrumbs QPushButton:hover { background: %HOVER%; }
QWidget#JtfCrumbs QLabel { color: %DIM%; padding: 0 1px; }
/* The message sits in from the window edge; text flush against the frame
   reads as a rendering slip rather than as a layout. */
/* The key hint strip. The key is a chip and the word beside it is quiet, so
   a row of them reads as pairs rather than as a sentence. */
QWidget#JtfKeyHints { background: %ALT%; border-top: 1px solid %BORDER%; }
QLabel[jtfHintKey="true"] {
    color: %TEXT%;
    background: %HEADER%;
    border: 1px solid %BORDER%;
    border-radius: 4px;
    padding: 1px 6px;
    font-weight: 600;
}
QLabel[jtfHintLabel="true"] { color: %DIM%; }

QStatusBar { background: %HEADER%; border-top: 1px solid %BORDER%; padding: 2px 4px; }
QStatusBar QLabel { padding-left: 8px; }
QStatusBar::item { border: none; }
QLabel[jtfStatusSummary="true"] {
    color: %DIM%;
    padding: 2px 12px;
    border-left: 1px solid %BORDER%;
}
/* The keyboard mode is a state, not a count, so it reads as a chip rather
   than as another number in the row. */
QLabel#JtfStatusKeymap {
    color: %ONSEL%;
    background: %SEL%;
    border: none;
    border-radius: 9px;
    padding: 2px 10px;
    margin: 0 6px;
}
QLabel[jtfZoomMark="true"] { color: %DIM%; }
/* Form controls. Qt's defaults leave a spin box's arrows shorter than the
   digits beside them, which reads as a rendering fault rather than a control. */
QSpinBox, QLineEdit, QComboBox {
    background: %HEADER%;
    color: %TEXT%;
    border: 1px solid %BORDER%;
    border-radius: 5px;
    padding: 4px 8px;
    min-height: 20px;
    selection-background-color: %SEL%;
    selection-color: %ONSEL%;
}
QSpinBox:focus, QLineEdit:focus, QComboBox:focus { border-color: %FOCUS%; }
QSpinBox { padding-right: 2px; }
QSpinBox::up-button, QSpinBox::down-button {
    subcontrol-origin: border;
    width: 18px;
    background: transparent;
    border-left: 1px solid %BORDER%;
}
QSpinBox::up-button { subcontrol-position: top right; border-top-right-radius: 5px; }
QSpinBox::down-button { subcontrol-position: bottom right; border-bottom-right-radius: 5px; }
QSpinBox::up-button:hover, QSpinBox::down-button:hover { background: %HOVER%; }
QSpinBox::up-arrow, QSpinBox::down-arrow { width: 7px; height: 7px; }
QSlider#JtfZoom::groove:horizontal { background: %BORDER%; height: 3px; border-radius: 2px; }
QSlider#JtfZoom::sub-page:horizontal { background: %SEL%; height: 3px; border-radius: 2px; }
QSlider#JtfZoom::handle:horizontal {
    background: %TEXT%;
    width: 10px;
    height: 10px;
    margin: -4px 0;
    border-radius: 5px;
}
/* The keyboard-mode switch: a recessed track with two segments, the active
   one raised out of it. The shape says "one of these two", which a pair of
   ordinary buttons would not. */
QWidget#JtfModeSwitch { background: %WINDOW%; border: 1px solid %BORDER%; border-radius: 7px; }
QToolButton[jtfModeSegment="true"] {
    color: %DIM%;
    background: transparent;
    border: none;
    border-radius: 5px;
    padding: 2px 10px;
    font-size: 11px;
}
QToolButton[jtfModeSegment="true"]:hover { color: %TEXT%; background: %HOVER%; }
QToolButton[jtfModeSegment="true"]:checked { color: %ONSEL%; background: %SEL%; }
QWidget#JtfInspector { background: %PANE%; border-left: 1px solid %BORDER%; }
QWidget#JtfInspectorHeader { background: %HEADER%; border-bottom: 1px solid %BORDER%; }
QLabel#JtfInspectorName { color: %TEXT%; }
QLabel#JtfInspectorPreview { background: %HEADER%; border-radius: 6px; padding: 10px; }
QLabel[jtfFactLabel="true"] { color: %DIM%; }
QPlainTextEdit#JtfInspectorText {
    background: %HEADER%;
    color: %TEXT%;
    border-radius: 6px;
    padding: 6px;
    selection-background-color: %SEL%;
    selection-color: %ONSEL%;
}
QLabel#JtfInspectorTextStatus { color: %DIM%; }
QTreeWidget#JtfPlacesTree { background: %PANE%; border: none; }
QTreeWidget#JtfPlacesTree::item { padding: 5px 4px; border-radius: 5px; }
QTreeWidget#JtfPlacesTree::item:hover { background: %HOVER%; }
QTreeWidget#JtfPlacesTree::item:selected { background: %SEL%; color: %ONSEL%; }
QSplitter#JtfSidebar { background: %PANE%; }
QLabel#JtfStatus { color: %DIM%; padding: 3px 8px; }
QLabel#JtfError { color: %ERROR%; padding: 3px 8px; }

QWidget#JtfTree { background: %PREVIEW%; border-right: 1px solid %BORDER%; }
QTreeView {
    background: %PREVIEW%;
    color: %TEXT%;
    border: none;
    outline: none;
    show-decoration-selected: 1;
}
QTreeView::item { padding: 3px 2px; }
QTreeView::item:hover { background: %HOVER%; }
QTreeView::item:selected { background: %SEL%; color: %ONSEL%; }
QTreeView::item:selected:!active { background: %SELDIM%; color: %TEXT%; }

QSplitter::handle { background: %BORDER%; }
QSplitter::handle:horizontal { width: 1px; }
QSplitter::handle:vertical { height: 1px; }
QSplitter::handle:hover { background: %FOCUS%; }

/* The key hint strip. The key is a chip and the word beside it is quiet, so
   a row of them reads as pairs rather than as a sentence. */
QWidget#JtfKeyHints { background: %ALT%; border-top: 1px solid %BORDER%; }
QLabel[jtfHintKey="true"] {
    color: %TEXT%;
    background: %HEADER%;
    border: 1px solid %BORDER%;
    border-radius: 4px;
    padding: 1px 6px;
    font-weight: 600;
}
QLabel[jtfHintLabel="true"] { color: %DIM%; }

QStatusBar { background: %HEADER%; color: %DIM%; border-top: 1px solid %BORDER%; }
QStatusBar::item { border: none; }

QScrollBar:vertical { background: transparent; width: 12px; margin: 0; }
QScrollBar::handle:vertical {
    background: %BORDER%;
    min-height: 28px;
    border-radius: 6px;
    margin: 2px;
}
QScrollBar::handle:vertical:hover { background: %DIM%; }
QScrollBar::add-line, QScrollBar::sub-line { height: 0; width: 0; }
QScrollBar::add-page, QScrollBar::sub-page { background: transparent; }
QScrollBar:horizontal { background: transparent; height: 12px; margin: 0; }
QScrollBar::handle:horizontal {
    background: %BORDER%;
    min-width: 28px;
    border-radius: 6px;
    margin: 2px;
}
QScrollBar::handle:horizontal:hover { background: %DIM%; }

QToolTip {
    background: %HEADER%;
    color: %TEXT%;
    border: 1px solid %BORDER%;
    border-radius: 5px;
    padding: 4px 7px;
    /* Qt draws tooltips through the palette as well as the stylesheet; the
       opacity keeps the border from being blended away on some styles. */
    opacity: 255;
}

QMenu { background: %PANE%; color: %TEXT%; border: 1px solid %BORDER%; padding: 4px; }
QMenu::item { padding: 5px 24px 5px 20px; border-radius: 4px; }
QMenu::item:selected { background: %SEL%; color: %ONSEL%; }
QMenu::separator { height: 1px; background: %BORDER%; margin: 4px 8px; }
)")
        .replace(QStringLiteral("%WINDOW%"), hex(window))
        .replace(QStringLiteral("%PANE%"), hex(pane))
        .replace(QStringLiteral("%PREVIEW%"), hex(preview))
        .replace(QStringLiteral("%HEADER%"), hex(header))
        .replace(QStringLiteral("%ALT%"), hex(rowAlternate))
        .replace(QStringLiteral("%HOVER%"), hex(rowHover))
        .replace(QStringLiteral("%TEXT%"), hex(textPrimary))
        .replace(QStringLiteral("%DIM%"), hex(textSecondary))
        .replace(QStringLiteral("%ONSEL%"), hex(textOnAccent))
        .replace(QStringLiteral("%BORDER%"), hex(border))
        .replace(QStringLiteral("%SELDIM%"), hex(selectionInactive))
        .replace(QStringLiteral("%SEL%"), hex(selection))
        .replace(QStringLiteral("%FOCUS%"), hex(focusRing))
        .replace(QStringLiteral("%ERROR%"), hex(error));
}
