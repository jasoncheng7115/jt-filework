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
    color: %DIM%;
    padding: 4px 8px;
    border: none;
    border-right: 1px solid %BORDER%;
    border-bottom: 1px solid %BORDER%;
    font-weight: 600;
}
QHeaderView::section:hover { color: %TEXT%; background: %HOVER%; }
/* The sort indicator sits just inside the section's right edge, small and
   quiet. Qt's default places a large triangle that reads as a control rather
   than as a state. */
QHeaderView::up-arrow, QHeaderView::down-arrow {
    subcontrol-origin: padding;
    subcontrol-position: center right;
    width: 8px;
    height: 8px;
    margin-right: 4px;
}
/* Only the sorted column shows one at all. */
QHeaderView::up-arrow:!enabled, QHeaderView::down-arrow:!enabled { image: none; }

QTabBar { background: %HEADER%; }
QTabBar::tab {
    background: transparent;
    color: %DIM%;
    padding: 5px 12px;
    margin: 2px 1px 0 1px;
    border: 1px solid transparent;
    border-radius: 6px 6px 0 0;
    max-width: 220px;
}
QTabBar::tab:hover { background: %HOVER%; color: %TEXT%; }
QTabBar::tab:selected {
    background: %PANE%;
    color: %TEXT%;
    border-color: %BORDER%;
    border-bottom-color: %PANE%;
}
QTabBar::close-button { subcontrol-position: right; }

QLineEdit#JtfFilter, QLineEdit#JtfSearch { margin: 2px 6px 4px 6px; }
QLabel#JtfPath { color: %DIM%; padding: 3px 8px; }
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
