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

private:
    void paintDivider(QPainter *painter, const QRect &rect, int index) const;

public:

protected:
    void paintSection(QPainter *painter, const QRect &rect, int index) const override;
    void mouseMoveEvent(QMouseEvent *event) override;
    void leaveEvent(QEvent *event) override;

private:
    QColor m_text;
    QColor m_dim;
    QColor m_indicator;
    /// Divider the pointer is near, or -1.
    int m_hoveredDivider = -1;
};
