// One look for every list of entries in the program.
//
// The pane's file list, an archive's contents, an ISO's contents and a folder
// comparison are all the same thing to a reader: rows of names with facts
// beside them, marked with a box, moved through with the same keys. They were
// four different-looking tables — different row heights, different fonts,
// different header alignment, a tick drawn one way in one and by the platform
// in another — which made the other windows read as someone else's dialogs
// rather than as part of this program.
//
// This is that look, in one function, so a change to the list is a change to
// all of them.
#pragma once

#include <QFont>

class QTableWidget;

namespace listlook {

/// Make `table` look and behave like the pane's file list.
///
/// Sets the row height, icon size, selection behaviour, header look and the
/// delegate that draws a mark's tick in the theme's colour. Columns, contents
/// and keys stay the caller's business — this is the look, not the list.
void apply(QTableWidget *table, const QFont &listFont);

/// Re-colour what the stylesheet cannot reach: the drawn tick and the header's
/// text. Call from the window's own theme handling.
void applyTheme(QTableWidget *table, const QColor &text, const QColor &dim,
                const QColor &tick);

} // namespace listlook
