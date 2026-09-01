// The list's column header.
//
// Qt's own sort indicator is a triangle pinned to the section's right edge,
// far from the word it refers to; in a wide Name column it ends up next to
// Size and reads as if it belonged there. Path Finder, and the reference
// layout, put a small caret immediately after the header text, and brighten
// that one header so the sorted column is legible at a glance.
//
// Both are reasons to paint the section rather than to style it: neither
// "next to the text" nor "this section only" is expressible in a stylesheet.
#pragma once

#include <QHeaderView>

class JtfHeaderView : public QHeaderView {
    Q_OBJECT

public:
    explicit JtfHeaderView(QWidget *parent = nullptr);

    void applyTheme(const QColor &text, const QColor &dim, const QColor &indicator);

    /// Whether to draw the sort caret at all.
    ///
    /// The caret marks the column the list is sorted by, and it is drawn
    /// rather than left to Qt so it can follow the theme. A table that does
    /// not sort has no sorted column - but `sortIndicatorSection` still
    /// answers 0, so without this the archive, ISO and usage listings each
    /// grew an arrow over their first column claiming a sort order they do
    /// not have.
    void setCaretVisible(bool visible);

    /// Draw a mark-all box at the head of the name column.
    ///
    /// The list marks with a checkbox per row, so the column of boxes wants a
    /// box at its head that means "all of them" - the convention every table
    /// with checkable rows uses, and the only way to mark everything with the
    /// mouse.
    void setMarkAllVisible(bool visible);
    /// What that box shows: every row marked, none, or some.
    void setMarkAllState(Qt::CheckState state);

signals:
    /// The box was clicked. `wanted` is the state it should move to.
    void markAllToggled(bool wanted);

private:
    void paintDivider(QPainter *painter, const QRect &rect, int index) const;

public:

protected:
    void paintSection(QPainter *painter, const QRect &rect, int index) const override;
    void mouseMoveEvent(QMouseEvent *event) override;
    void mousePressEvent(QMouseEvent *event) override;
    void leaveEvent(QEvent *event) override;

private:
    QColor m_text;
    QColor m_dim;
    QColor m_indicator;
    /// Divider the pointer is near, or -1.
    int m_hoveredDivider = -1;
    bool m_markAllVisible = false;
    bool m_caretVisible = true;
    Qt::CheckState m_markAllState = Qt::Unchecked;
    /// Where the box was last drawn, so a click can be tested against it.
    mutable QRect m_markAllRect;
};
