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
    t.error = c(TokenStatusError);
    return t;
}

QString Theme::styleSheet() const {
    const auto hex = [](const QColor &colour) { return colour.name(QColor::HexRgb); };

    return QStringLiteral(R"(
QMainWindow, QWidget#JtfRoot { background: %WINDOW%; }

QToolBar#JtfToolbar {
    background: %HEADER%;
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
QTableView::item { padding: 2px 6px; border: none; }
QTableView::item:hover { background: %HOVER%; }
QTableView::item:selected { background: %SEL%; color: %ONSEL%; }
QTableView::item:selected:!active { background: %SELDIM%; color: %TEXT%; }

QHeaderView::section {
    background: %HEADER%;
    /* Painted by JtfHeaderView; this is the fallback for any header that is
       not one of ours. */
    color: %DIM%;
    padding: 4px 8px;
    border: none;
    border-right: 1px solid %BORDER%;
    border-bottom: 1px solid %BORDER%;
    font-weight: 600;
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
    padding: 6px 14px;
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
QTabBar::close-button { subcontrol-position: right; }

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
QLabel[jtfStatusSummary="true"] { color: %DIM%; padding: 0 10px; }
QLabel[jtfZoomMark="true"] { color: %DIM%; }
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
QTreeWidget#JtfPlacesTree::item { padding: 3px 4px; border-radius: 4px; }
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
