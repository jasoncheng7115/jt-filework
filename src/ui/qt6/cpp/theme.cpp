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
    t.menu = c(TokenSurfaceMenu);
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

    // Split into several literals on purpose: MSVC refuses a single string
    // literal over 16380 bytes (C2026), and this sheet is longer than that.
    // The split is at line boundaries and carries no meaning.
    //
    // Copied into a mutable `QString` before the substitutions: `QStringLiteral`
    // yields a `const QString`, and `replace` is not const. Qt 6.11 on macOS
    // accepted the chain; Qt 6.2 with GCC does not, which is the reason the
    // Linux binaries are built on the oldest distribution rather than the
    // newest.
    return QString(QStringLiteral(R"(
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
/* A boxed cluster, so related buttons read as one control - the reference
   layouts frame each group, and a recessed fill alone was too faint to read
   as a frame at all against the toolbar's own shade. Fill *and* hairline:
   the fill separates the group from the bar, the hairline gives it an edge. */
QWidget[jtfToolGroup="true"] {
    background: %WINDOW%;
    border: 1px solid %BORDER%;
    border-radius: 8px;
}
QWidget[jtfToolGroup="true"] QToolButton { padding: 4px; border-radius: 6px; }
QWidget[jtfToolGroup="true"] QToolButton:hover { background: %HOVER%; }
QWidget[jtfToolGroup="true"] QToolButton:checked { background: %SEL%; }
QToolBar#JtfToolbar QToolButton:hover { background: %HOVER%; }
QToolBar#JtfToolbar QToolButton:pressed { background: %ALT%; }
QToolBar#JtfToolbar QToolButton:disabled { color: %DIM%; }
/* A pressed-in look for the toggles, so "the sidebar is open" is readable
   from the toolbar itself.
   The selection colour rather than its dimmed form: dimmed was a shade away
   from the toolbar's own background and, in the dark theme especially, a
   toggled button was indistinguishable from an untoggled one. This is the
   same treatment the mode pill beside it already uses, and that one reads. */
QToolBar#JtfToolbar QToolButton:checked {
    background: %SEL%;
    border: 1px solid %SEL%;
    border-radius: 5px;
}
QToolBar#JtfToolbar QToolButton:checked:hover { background: %SEL%; border-color: %FOCUS%; }
QToolBar#JtfToolbar::separator {
    background: %BORDER%;
    width: 1px;
    margin: 5px 5px;
}

/* The toolbar's search field carries a magnifier inside it, the way the
   reference does, so the field says what it is without a label. The extra
   left padding is the room that icon sits in. */
QLineEdit#JtfToolbarSearch {
    background: %WINDOW%;
    border: 1px solid %BORDER%;
    border-radius: 8px;
    padding: 0px 8px 0px 6px;
    /* Height is set in code, with the groups', so there is one source. */
    selection-background-color: %SEL%;
}
QLineEdit#JtfToolbarSearch:focus { border: 1px solid %FOCUS%; background: %PANE%; }

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
/* The box is filled here and the tick is drawn over it by RowDelegate: a
   stylesheet image comes from a file and so cannot take the theme's colour.
   `image: none` stays deliberately - it stops the platform style putting its
   own mark under ours. */
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
    padding: 6px 10px 6px 10px;
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
/* The close mark is a real QToolButton we install per tab (see
   PaneWidget::syncTabs), not Qt's subcontrol. Styling the subcontrol without
   giving it an `image` is what made it disappear. */
QToolButton#JtfTabClose {
    border: none;
    border-radius: 5px;
    /* No padding, and the gap to the title is the widget's own extra width
       (kTabCloseGap), not a margin: a margin here is taken out of the box the
       icon is drawn in, and 8 + 3 of margin plus 3 of padding on each side
       left a 24px button with 7px to draw a cross in. */
    padding: 0px;
    margin-left: 6px;
    margin-right: 0px;
}
QToolButton#JtfTabClose:hover { background: %SELDIM%; }
QToolButton#JtfTabClose:pressed { background: %SEL%; }
QWidget#JtfTabRow { background: %HEADER%; border-bottom: 1px solid %BORDER%; }

/* The active pane. The edge marks it, the tab strip brightens with it, and
   the inactive panes' tabs go quiet so only one strip looks lit at a time. */
/* Every pane keeps the same border thickness whether it is active or not, so
   that becoming active does not move its contents by a pixel.

   Transparent when it is neither active nor the target. A pane is a white
   surface on a grey window, so its edge is already where the colour changes -
   drawing a grey line round it as well boxed every pane in, and with a single
   pane open it was a frame around the whole window for no reason at all. The
   2px stays reserved; only its colour changes. */
QWidget#JtfPane { border: 2px solid transparent; border-radius: 8px; }
/* Where the keyboard is. A line along the top edge alone was easy to miss with
   two panes side by side - the eye has to find a 2px strip at the top of one
   column and compare it with the other. A ring around the whole pane is the
   thing every application uses to say "this one", and it reads without
   looking for it.

   `%PANERING%` rather than the selection colour, and this is the part that
   has to work in both themes. A ring is read against the surface behind it,
   and those surfaces are opposites: white in light, near-black in dark. A
   single blue cannot do both - the palette answers with a deeper one that
   holds against white and a lighter one that carries on black, so the ring is
   legible either way without this stylesheet knowing which theme is on.
   Measured: 5.0 against the pane in light, 7.3 in dark, and 3.8 and 5.7
   against the inactive border, all clear of the 3:1 a non-text indicator
   needs.

   In the light theme that token happens to be the same blue as the selection.
   Left alone: the platform does the same thing with its accent colour, and a
   2px ring around a pane is not mistakable for a filled row. */
QWidget#JtfPane[jtfActive="true"] { border: 2px solid %PANERING%; }
/* The pane a copy or a move would land in. Marked differently from the active
   one on purpose: the active pane is where the keyboard is, the target is
   where the files go, and confusing the two is how a folder ends up in the
   wrong place. Accent for "here you are", a dashed edge plus a worded badge
   for "here it goes". */
QWidget#JtfPane[jtfTarget="true"] {
    border: 2px dashed %MARK%;
}
QWidget#JtfTargetBadge { background: transparent; }
QLabel#JtfTargetBadgeWord {
    color: %MARK%;
    background: transparent;
    font-size: 11px;
    font-weight: 600;
}
QLabel#JtfTargetBadgeIcon { background: transparent; }
QTabBar[jtfActive="false"]::tab:selected {
    background: %HEADER%;
)")
        + QStringLiteral(R"(
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
QToolButton#JtfClosePane {
    color: %DIM%;
    background: transparent;
    border: none;
    padding: 0 8px;
}
QToolButton#JtfClosePane:hover { color: %TEXT%; background: %HOVER%; }

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
/* A properties sheet's title: the file's own name, set apart from the facts
   listed under it. */
QLabel[jtfHeadingLabel="true"] { color: %TEXT%; font-size: 15px; font-weight: 600; }
/* A block of explanation inside a dialog: set on its own surface so it reads
   as something to take in rather than another control to fill in. */
QFrame[jtfNoteBox="true"] {
    background: %WINDOW%;
    border: 1px solid %BORDER%;
    border-radius: 8px;
}

QFrame[jtfRule="true"] { color: %BORDER%; background: %BORDER%; max-height: 1px; border: none; }

/* A toggle that lives on the status bar. Quiet when off, lit when on, so the
   strip's state is readable from the bar even when the strip is hidden. */
QToolButton#JtfStatusToggle {
    border: 1px solid transparent;
    border-radius: 5px;
    padding: 2px 5px;
    margin: 0 4px;
}
QToolButton#JtfStatusToggle:hover { background: %HOVER%; }
QToolButton#JtfStatusToggle:checked { background: %SELDIM%; border-color: %BORDER%; }

/* The running-search card. It floats over the list, so it needs an edge and
   a shadow-substitute - a solid fill a shade off the list's - or it reads as
   text that has somehow landed on top of the rows. */
QWidget#JtfSearchOverlay {
    background: %MENU%;
    border: 1px solid %DIM%;
    border-radius: 10px;
}
QWidget#JtfSearchOverlay QLabel { color: %TEXT%; }
QPushButton#JtfSearchCancel {
    color: %TEXT%;
    background: %WINDOW%;
    border: 1px solid %BORDER%;
    border-radius: 6px;
    padding: 4px 12px;
}
QPushButton#JtfSearchCancel:hover { background: %HOVER%; border-color: %DIM%; }

/* The viewer's own foot: its key strip and its status line, set apart from
   the text being read so the reading area is the only thing that looks like
   content. */
QWidget#JtfViewerBar { background: %HEADER%; border-bottom: 1px solid %BORDER%; }
/* The text being read gets a margin. Set flush against the frame it reads as
   a terminal dump rather than as a document, and the first character of every
   line sits on the window edge. */
QListView#JtfViewerList {
    background: %PANE%;
    border: none;
    padding: 6px 10px;
}
QWidget#JtfViewerHints, QWidget#JtfUsageHints {
    background: %ALT%;
    border-top: 1px solid %BORDER%;
}
QWidget#JtfViewerFoot { background: %HEADER%; border-top: 1px solid %BORDER%; }
QWidget#JtfViewerFoot QLabel { color: %DIM%; }

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
/* As wide as what they say, and no wider. The left of the bar is where the
   message goes - a path, when there is one - and every pixel of padding out
   here is a pixel it does not have. */
QLabel[jtfStatusSummary="true"] {
    color: %DIM%;
    padding: 2px 7px;
    border-left: 1px solid %BORDER%;
}
/* The keyboard mode is a state, not a count, so it reads as a chip rather
   than as another number in the row. */
QToolButton#JtfStatusKeymap {
    color: %ONSEL%;
    background: %SEL%;
    border: none;
    border-radius: 10px;
    padding: 3px 10px;
    margin: 0 4px;
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
/* The arrow needs its own room, or it sits on the last letter of the value.
   The popup needs styling of its own: it is a separate top-level window, so
   nothing the box itself is given reaches it, and unstyled it came up with
   the platform's colours inside our dialog. */
/* The arrow needs room, but the divider before it was drawn at the padding
   edge rather than at the box's, which left a narrow empty cell hanging off
   the right of every combo. No divider: the arrow is enough of a signal, and
   one less line is one less thing competing with the value. */
QComboBox { padding-right: 24px; }
QComboBox::drop-down {
    subcontrol-origin: border;
    subcontrol-position: center right;
    width: 24px;
    border: none;
    background: transparent;
}
QComboBox::drop-down:hover { background: %HOVER%; border-top-right-radius: 5px;
                             border-bottom-right-radius: 5px; }
QComboBox QAbstractItemView {
    background: %PANE%;
    color: %TEXT%;
    border: 1px solid %BORDER%;
    border-radius: 6px;
    padding: 4px;
    outline: none;
    selection-background-color: %SEL%;
    selection-color: %ONSEL%;
}
QComboBox QAbstractItemView::item { padding: 5px 8px; border-radius: 4px; min-height: 20px; }
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
/* No background here on purpose. The preview's background is the user's
   choice - theme, chequer, or a colour they picked - and it is set as a
   palette brush, which a stylesheet background would silently win against.
   Choosing white did nothing at all for exactly that reason. */
QLabel#JtfInspectorPreview { border-radius: 6px; padding: 10px; }
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
/* The same ground as the folder tree below it (%PREVIEW%), not the panes'.
   One column, two lists: giving them different backgrounds drew a line across
   the sidebar that meant nothing. */
QTreeWidget#JtfPlacesTree { background: %PREVIEW%; border: none; }
/* The row's own pill is painted by PillDelegate, across both columns; a
   stylesheet rounds each cell separately and left notches where they met. */
QTreeWidget#JtfPlacesTree::item { padding: 5px 4px; background: transparent; }
)")
        + QStringLiteral(R"(
QTreeWidget#JtfPlacesTree::item:selected { background: transparent; color: %ONSEL%; }
QSplitter#JtfSidebar { background: %PREVIEW%; }
QLabel#JtfStatus { color: %DIM%; padding: 3px 8px; }
QWidget#JtfStatusRow { background: transparent; }
/* The heading over each half of the sidebar. Quiet and small: it is a label
   for a column, not a row you can click, and it must not compete with the
   entries under it. */
QLabel#JtfSidebarTitle {
    color: %DIM%;
    background: %HEADER%;
    border-bottom: 1px solid %BORDER%;
    padding: 4px 10px;
    font-size: 11px;
    font-weight: 600;
}
QLabel#JtfCompareHeading {
    color: %DIM%;
    background: %HEADER%;
    border-bottom: 1px solid %BORDER%;
    padding: 6px 10px;
}
QWidget#JtfCompareOptions { background: %ALT%; }
/* Beside the failure it answers, so it reads as part of the same sentence. */
QToolButton#JtfReconnect {
    color: %ONSEL%;
    background: %SEL%;
    border: none;
    border-radius: 9px;
    padding: 2px 10px;
    margin: 2px 8px;
}
QToolButton#JtfReconnect:hover { background: %SELDIM%; }
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

/* A one-pixel line, with something you can actually grab around it.

   Qt makes the handle's drag area exactly its width, so `width: 1px` produced
   a divider one pixel wide that had to be hit within one pixel - in practice
   the panes could not be resized at all. The handle is 7px and the padding
   either side is painted in the pane colour, so what shows is still a single
   line and what responds is seven. */
/* The divider between panes. The window's own colour, so it reads as the gap
   between two surfaces rather than as a bar laid over them - it used to be a
   grey stripe with white edges, which in the light theme looked like a seam
   where two pictures had been joined. It appears only when the pointer is on
   it, which is the only time it is a control. */
QSplitter::handle { background: %WINDOW%; }
/* A hairline in the gap, not a bar filling it. Without any line at all the
   sidebar and the list ran into each other with nothing to say where one
   ended; with the gap filled it was a seam. One pixel gives the structure and
   nothing else. */
QSplitter::handle:horizontal { width: 8px; border-left: 1px solid %BORDER%; }
QSplitter::handle:vertical { height: 8px; border-top: 1px solid %BORDER%; }
QSplitter::handle:hover { background: %SELDIM%; }
QSplitter::handle:pressed { background: %FOCUS%; }

/* The sidebar's own divider, between the places above and the folder tree
   below. A one-pixel border line is right between a pane and its neighbour,
   where the two already differ in background; here both sides are lists of
   folder rows on the same colour, so the seam vanished and the two read as one
   long list. This one is given height as well as a line: the gap is what says
   "these are two things". */
QSplitter#JtfSidebar::handle:vertical {
    height: 11px;
    /* Wide enough to grab, which the 1px rule above was not. */
    /* The window colour, which is darker than either list, so the band reads
       as a gap between two things rather than as a line drawn on one of them.
       A one-pixel border was not enough: both sides are folder rows on nearly
       the same colour, and the seam simply disappeared. */
    background: %WINDOW%;
    border-top: 1px solid %BORDER%;
    border-bottom: 1px solid %BORDER%;
}
QSplitter#JtfSidebar::handle:vertical:hover { background: %FOCUS%; }

/* The key hint strip. The key is a chip and the word beside it is quiet, so
   a row of them reads as pairs rather than as a sentence. */
/* A properties sheet's title: the file's own name, set apart from the facts
   listed under it. */
QLabel[jtfHeadingLabel="true"] { color: %TEXT%; font-size: 15px; font-weight: 600; }
/* A block of explanation inside a dialog: set on its own surface so it reads
   as something to take in rather than another control to fill in. */
QFrame[jtfNoteBox="true"] {
    background: %WINDOW%;
    border: 1px solid %BORDER%;
    border-radius: 8px;
}

QFrame[jtfRule="true"] { color: %BORDER%; background: %BORDER%; max-height: 1px; border: none; }

/* A toggle that lives on the status bar. Quiet when off, lit when on, so the
   strip's state is readable from the bar even when the strip is hidden. */
QToolButton#JtfStatusToggle {
    border: 1px solid transparent;
    border-radius: 5px;
    padding: 2px 5px;
    margin: 0 4px;
}
QToolButton#JtfStatusToggle:hover { background: %HOVER%; }
QToolButton#JtfStatusToggle:checked { background: %SELDIM%; border-color: %BORDER%; }

/* The running-search card. It floats over the list, so it needs an edge and
   a shadow-substitute - a solid fill a shade off the list's - or it reads as
   text that has somehow landed on top of the rows. */
QWidget#JtfSearchOverlay {
    background: %MENU%;
    border: 1px solid %DIM%;
    border-radius: 10px;
}
QWidget#JtfSearchOverlay QLabel { color: %TEXT%; }
QPushButton#JtfSearchCancel {
    color: %TEXT%;
    background: %WINDOW%;
    border: 1px solid %BORDER%;
    border-radius: 6px;
    padding: 4px 12px;
}
QPushButton#JtfSearchCancel:hover { background: %HOVER%; border-color: %DIM%; }

/* The viewer's own foot: its key strip and its status line, set apart from
   the text being read so the reading area is the only thing that looks like
   content. */
QWidget#JtfViewerBar { background: %HEADER%; border-bottom: 1px solid %BORDER%; }
/* The text being read gets a margin. Set flush against the frame it reads as
   a terminal dump rather than as a document, and the first character of every
   line sits on the window edge. */
QListView#JtfViewerList {
    background: %PANE%;
    border: none;
    padding: 6px 10px;
}
QWidget#JtfViewerHints, QWidget#JtfUsageHints {
    background: %ALT%;
    border-top: 1px solid %BORDER%;
}
QWidget#JtfViewerFoot { background: %HEADER%; border-top: 1px solid %BORDER%; }
QWidget#JtfViewerFoot QLabel { color: %DIM%; }

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

/* Above everything, and drawn so. A menu in the same colour as the list it
   covers is a menu you have to find by its text. */
/* The settings dialog. Three tabs and a column of grey rows is the flattest a
   window can be; these give the pages a surface of their own, mark the
   current tab the way the file tabs are marked, and let the labels sit
   quieter than the values so a row reads as one thing rather than two. */
QTabWidget::pane {
    background: %PANE%;
    border: 1px solid %BORDER%;
    border-radius: 8px;
    top: -1px;
}
QTabBar::tab {
    background: transparent;
    color: %DIM%;
    padding: 7px 16px;
    margin-right: 2px;
    border: 1px solid transparent;
    border-top-left-radius: 7px;
    border-top-right-radius: 7px;
}
QTabBar::tab:hover { color: %TEXT%; background: %HOVER%; }
QTabBar::tab:selected {
    color: %TEXT%;
    background: %PANE%;
    border-color: %BORDER%;
    border-bottom-color: %PANE%;
}
QDialog QLabel { color: %TEXT%; }
QLabel[jtfFactLabel="true"] { color: %DIM%; }
/* Only the spacing and the text colour. The box itself is left to the
   platform: styling `::indicator` at all makes Qt draw it from the
   stylesheet, and a stylesheet cannot supply a tick without an image file -
   which is how these became solid blue squares with nothing in them. The
   file list draws its own tick in a delegate because it has one; a checkbox
   does not, so it keeps the system's. */
QCheckBox { color: %TEXT%; spacing: 7px; padding: 2px 0; }
QDialog QPushButton {
    color: %TEXT%;
    background: %HEADER%;
    border: 1px solid %BORDER%;
    border-radius: 6px;
    padding: 5px 14px;
}
QDialog QPushButton:hover { background: %HOVER%; border-color: %DIM%; }
QDialog QPushButton:default { background: %SEL%; color: %ONSEL%; border-color: %SEL%; }
QDialog QPushButton:default:hover { background: %FOCUS%; }
/* Disabled, and looking it. There was no rule here at all, so a disabled
   button in any dialog was drawn exactly like an enabled one - and a disabled
   *default* button was drawn as the filled, highlighted one the eye is meant
   to go to. The image writer's Write button is disabled until a disk has been
   chosen, and it was the most inviting control on the screen while it did
   nothing. The `:default:disabled` rule has to come after `:default` to win,
   since the two have the same specificity and Qt takes the last. */
QDialog QPushButton:disabled {
    color: %DIM%;
    background: %WINDOW%;
    border-color: %BORDER%;
}
QDialog QPushButton:default:disabled {
    color: %DIM%;
    background: %WINDOW%;
    border-color: %BORDER%;
}

QMenu {
    background: %MENU%;
    color: %TEXT%;
    border: 1px solid %DIM%;
    border-radius: 8px;
    padding: 5px;
}
QToolTip {
    background: %MENU%;
    color: %TEXT%;
    border: 1px solid %DIM%;
    border-radius: 6px;
    padding: 4px 7px;
}
QMenu::item { padding: 5px 24px 5px 20px; border-radius: 4px; }
QMenu::item:selected { background: %SEL%; color: %ONSEL%; }
QMenu::separator { height: 1px; background: %BORDER%; margin: 4px 8px; }
)"))
        .replace(QStringLiteral("%WINDOW%"), hex(window))
        .replace(QStringLiteral("%PANE%"), hex(pane))
        .replace(QStringLiteral("%PREVIEW%"), hex(preview))
        .replace(QStringLiteral("%HEADER%"), hex(header))
        .replace(QStringLiteral("%MENU%"), hex(menu))
        .replace(QStringLiteral("%ALT%"), hex(rowAlternate))
        .replace(QStringLiteral("%HOVER%"), hex(rowHover))
        .replace(QStringLiteral("%TEXT%"), hex(textPrimary))
        .replace(QStringLiteral("%DIM%"), hex(textSecondary))
        .replace(QStringLiteral("%ONSEL%"), hex(textOnAccent))
        .replace(QStringLiteral("%BORDER%"), hex(border))
        .replace(QStringLiteral("%SELDIM%"), hex(selectionInactive))
        .replace(QStringLiteral("%SEL%"), hex(selection))
        .replace(QStringLiteral("%FOCUS%"), hex(focusRing))
        // The active pane's ring. Its own token, not the selection colour:
        // the palette gives it a *lighter* blue on dark and a *deeper* one on
        // light, because a ring has to read against the surface behind it and
        // those surfaces are opposites.
        .replace(QStringLiteral("%PANERING%"), hex(indicator))
        .replace(QStringLiteral("%MARK%"), hex(mark))
        .replace(QStringLiteral("%ERROR%"), hex(error));
}
