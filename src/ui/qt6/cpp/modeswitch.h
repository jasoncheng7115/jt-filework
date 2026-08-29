// A two-segment switch that slides.
//
// A pair of toggle buttons says "one of these is on". A pill that travels
// from one side to the other says the same thing and also says *which way it
// went*, which is the part that makes a mode change feel like a change rather
// than a repaint. The reference layouts use segmented controls throughout, and
// they read as one control rather than as adjacent buttons.
//
// Painted rather than styled: a stylesheet can colour two buttons, but it
// cannot move a highlight between them.
#pragma once

#include <QColor>
#include <QStringList>
#include <QWidget>

class ModeSwitch : public QWidget {
    Q_OBJECT
    // Animated by QPropertyAnimation: the pill's position, in segments.
    Q_PROPERTY(qreal slidePosition READ slidePosition WRITE setSlidePosition)

public:
    explicit ModeSwitch(QWidget *parent = nullptr);

    /// The segment labels, left to right. Resets the selection to the first.
    void setSegments(const QStringList &labels);
    /// Which segment is on, without animating — for initial state.
    void setCurrentIndex(int index);
    int currentIndex() const { return m_current; }

    void applyTheme(const QColor &track, const QColor &border, const QColor &accent,
                    const QColor &text, const QColor &dim, const QColor &onAccent);

    QSize sizeHint() const override;
    QSize minimumSizeHint() const override { return sizeHint(); }

    qreal slidePosition() const { return m_slide; }
    void setSlidePosition(qreal position);

signals:
    /// A segment was chosen by the user. Not emitted by setCurrentIndex.
    void segmentClicked(int index);

protected:
    void paintEvent(QPaintEvent *event) override;
    void mousePressEvent(QMouseEvent *event) override;
    void leaveEvent(QEvent *event) override;
    void mouseMoveEvent(QMouseEvent *event) override;

private:
    int segmentAt(const QPoint &point) const;
    QRectF segmentRect(qreal index) const;

    QStringList m_labels;
    int m_current = 0;
    int m_hovered = -1;
    qreal m_slide = 0.0;
    class QPropertyAnimation *m_animation = nullptr;

    QColor m_track, m_border, m_accent, m_text, m_dim, m_onAccent;
};
