#include "modeswitch.h"

#include <QFontMetrics>
#include <QMouseEvent>
#include <QPainter>
#include <QPainterPath>
#include <QPropertyAnimation>

namespace {
// Tight. The labels are the full mode names and must stay that way, so the
// only room left to give back is the padding around them - at 12px the
// control took a fifth of the toolbar and read as the loudest thing on it.
constexpr int kPaddingX = 9;
constexpr int kPaddingY = 4;
constexpr int kInset = 2;
// Long enough to be seen as movement, short enough that it never delays the
// mode actually changing - the switch animates, the keymap does not wait.
constexpr int kSlideMs = 160;
} // namespace

ModeSwitch::ModeSwitch(QWidget *parent) : QWidget(parent) {
    setObjectName(QStringLiteral("JtfModeSwitch"));
    setCursor(Qt::PointingHandCursor);
    setMouseTracking(true);
    m_animation = new QPropertyAnimation(this, "slidePosition", this);
    m_animation->setDuration(kSlideMs);
    m_animation->setEasingCurve(QEasingCurve::OutCubic);
}

void ModeSwitch::setSegments(const QStringList &labels) {
    if (labels == m_labels) {
        return;
    }
    m_labels = labels;
    m_current = qBound(0, m_current, qMax(0, static_cast<int>(labels.size()) - 1));
    m_slide = m_current;
    updateGeometry();
    update();
}

void ModeSwitch::setCurrentIndex(int index) {
    if (index < 0 || index >= static_cast<int>(m_labels.size()) || index == m_current) {
        return;
    }
    m_current = index;
    // Animated from wherever the pill currently is, so a change made by the
    // keyboard or the settings screen slides too. Only a change is animated;
    // arriving already on a segment must not slide from nowhere.
    m_animation->stop();
    m_animation->setStartValue(m_slide);
    m_animation->setEndValue(static_cast<qreal>(index));
    m_animation->start();
}

void ModeSwitch::setSlidePosition(qreal position) {
    m_slide = position;
    update();
}

void ModeSwitch::applyTheme(const QColor &track, const QColor &border, const QColor &accent,
                            const QColor &text, const QColor &dim, const QColor &onAccent) {
    m_track = track;
    m_border = border;
    m_accent = accent;
    m_text = text;
    m_dim = dim;
    m_onAccent = onAccent;
    update();
}

QSize ModeSwitch::sizeHint() const {
    const QFontMetrics metrics(font());
    int widest = 0;
    for (const QString &label : m_labels) {
        widest = qMax(widest, metrics.horizontalAdvance(label));
    }
    // Every segment is the same width, so the pill is one shape that moves
    // rather than one that also resizes.
    // The dot and its gap, which sizeHint has to allow for or the labels
    // shift under it.
    const int segment = widest + kPaddingX * 2 + 12;
    const int count = qMax(1, static_cast<int>(m_labels.size()));
    return {segment * count + kInset * 2, metrics.height() + kPaddingY * 2 + kInset * 2};
}

QRectF ModeSwitch::segmentRect(qreal index) const {
    const int count = qMax(1, static_cast<int>(m_labels.size()));
    const qreal width = (qreal(this->width()) - kInset * 2) / count;
    return {kInset + index * width, qreal(kInset), width,
            qreal(height()) - kInset * 2};
}

int ModeSwitch::segmentAt(const QPoint &point) const {
    const int count = static_cast<int>(m_labels.size());
    if (count == 0 || width() <= 0) {
        return -1;
    }
    const int index = point.x() * count / width();
    return qBound(0, index, count - 1);
}

void ModeSwitch::paintEvent(QPaintEvent *) {
    QPainter painter(this);
    painter.setRenderHint(QPainter::Antialiasing, true);

    const qreal radius = height() / 2.0;
    painter.setPen(QPen(m_border, 1));
    painter.setBrush(m_track);
    painter.drawRoundedRect(QRectF(0.5, 0.5, width() - 1.0, height() - 1.0), radius, radius);

    if (m_labels.isEmpty()) {
        return;
    }

    // The pill, at the animated position.
    const QRectF pill = segmentRect(m_slide);
    painter.setPen(Qt::NoPen);
    painter.setBrush(m_accent);
    painter.drawRoundedRect(pill, pill.height() / 2.0, pill.height() / 2.0);

    for (int i = 0; i < static_cast<int>(m_labels.size()); ++i) {
        const QRectF rect = segmentRect(i);
        // Text colour follows how far the pill has actually travelled, so the
        // label lights up as the pill arrives rather than a frame before it.
        const qreal covered = 1.0 - qMin(1.0, qAbs(m_slide - i));
        QColor colour = i == m_hovered && covered < 0.5 ? m_text : m_dim;
        if (covered > 0.0) {
            const auto mix = [covered](int from, int to) {
                return static_cast<int>(from + (to - from) * covered);
            };
            colour = QColor(mix(colour.red(), m_onAccent.red()),
                            mix(colour.green(), m_onAccent.green()),
                            mix(colour.blue(), m_onAccent.blue()));
        }
        // A filled dot on the segment that is on, hollow on the one that is
        // not. The travelling pill already says which mode is current, but it
        // says it only by position and colour; a state marker says it in a
        // third way, and reads at a glance in a screenshot or to anyone who
        // cannot separate the two fills.
        const QFontMetrics metrics(font());
        const QString label = m_labels.at(i);
        const qreal dot = 6.0;
        const qreal gap = 6.0;
        const qreal textWidth = metrics.horizontalAdvance(label);
        const qreal total = dot + gap + textWidth;
        const qreal left = rect.center().x() - total / 2.0;
        const QRectF marker(left, rect.center().y() - dot / 2.0, dot, dot);

        painter.setPen(QPen(colour, 1.2));
        painter.setBrush(covered > 0.5 ? QBrush(colour) : Qt::NoBrush);
        painter.drawEllipse(marker);

        painter.setBrush(Qt::NoBrush);
        painter.setPen(colour);
        painter.drawText(QRectF(left + dot + gap, rect.top(), textWidth, rect.height()),
                         Qt::AlignVCenter | Qt::AlignLeft, label);
    }
}

void ModeSwitch::mousePressEvent(QMouseEvent *event) {
    const int index = segmentAt(event->pos());
    if (index >= 0 && index != m_current) {
        emit segmentClicked(index);
    }
}

void ModeSwitch::mouseMoveEvent(QMouseEvent *event) {
    const int index = segmentAt(event->pos());
    if (index != m_hovered) {
        m_hovered = index;
        update();
    }
}

void ModeSwitch::leaveEvent(QEvent *) {
    m_hovered = -1;
    update();
}
