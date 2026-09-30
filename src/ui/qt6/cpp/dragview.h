// Dragging rows out of a list: what is carried, and what the pointer shows.
//
// Qt's own `QAbstractItemView::startDrag` takes both from the selection. Here
// the selection is only ever the row under the bar (AGENTS.md 10) - the marks
// are what was chosen - so Qt's drag carried one file and drew one row while
// seven were ticked. The model decides what a drag carries
// (`FileListModel::mimeData`); this draws the drag so that it says so.
#pragma once

#include <QListView>
#include <QTableView>

class QAbstractItemView;

/// Start a drag from `view`: the model's data for the rows under the bar, and
/// an image of the first with the number being carried beside it.
void jtfStartDrag(QAbstractItemView *view, Qt::DropActions supported);

/// A view whose drags go through `jtfStartDrag`.
template <class View>
class JtfDragView : public View {
public:
    using View::View;

protected:
    void startDrag(Qt::DropActions supported) override { jtfStartDrag(this, supported); }
};

using JtfTableView = JtfDragView<QTableView>;
using JtfListView = JtfDragView<QListView>;
