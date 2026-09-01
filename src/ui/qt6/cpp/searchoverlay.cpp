#include "searchoverlay.h"

#include <QHBoxLayout>
#include <QLabel>
#include <QPainter>
#include <QPainterPath>
#include <QPushButton>
#include <QTimer>

namespace {
// Twelve steps a second: enough to read as motion, far short of anything that
// would show up next to the search itself in a profile.
constexpr int kTickMs = 80;
constexpr int kStep = 30; // degrees per tick
} // namespace

Spinner::Spinner(QWidget *parent) : QWidget(parent) {
    setFixedSize(sizeHint());
    m_timer = new QTimer(this);
    m_timer->setInterval(kTickMs);
    connect(m_timer, &QTimer::timeout, this, [this] {
        m_angle = (m_angle + kStep) % 360;
        update();
    });
}

void Spinner::setColour(const QColor &colour) {
    m_colour = colour;
    update();
}

void Spinner::showEvent(QShowEvent *event) {
    QWidget::showEvent(event);
    m_timer->start();
}

void Spinner::hideEvent(QHideEvent *event) {
    QWidget::hideEvent(event);
    m_timer->stop();
}

void Spinner::paintEvent(QPaintEvent *event) {
    Q_UNUSED(event);
    QPainter painter(this);
    painter.setRenderHint(QPainter::Antialiasing);
    const QRectF ring = QRectF(rect()).adjusted(2.5, 2.5, -2.5, -2.5);

    // A faint full ring with a bright arc running round it. The full ring is
    // what stops the bright part reading as a stray mark when it is at the top.
    QColor faint = m_colour;
    faint.setAlphaF(0.22);
    painter.setPen(QPen(faint, 2.0));
    painter.drawEllipse(ring);
    painter.setPen(QPen(m_colour, 2.0, Qt::SolidLine, Qt::RoundCap));
    // Qt measures in sixteenths of a degree, anticlockwise from three o'clock.
    painter.drawArc(ring, (90 - m_angle) * 16, -100 * 16);
}

SearchOverlay::SearchOverlay(QWidget *parent) : QWidget(parent) {
    setObjectName(QStringLiteral("JtfSearchOverlay"));
    // Without this a plain QWidget ignores the stylesheet's background, and
    // the card renders as bare text floating over the rows it covers.
    setAttribute(Qt::WA_StyledBackground, true);
    auto *row = new QHBoxLayout(this);
    row->setContentsMargins(16, 10, 12, 10);
    row->setSpacing(10);

    m_spinner = new Spinner(this);
    m_label = new QLabel(this);
    m_label->setProperty("jtfOverlayLabel", true);
    m_cancel = new QPushButton(this);
    m_cancel->setObjectName(QStringLiteral("JtfSearchCancel"));
    m_cancel->setCursor(Qt::PointingHandCursor);
    connect(m_cancel, &QPushButton::clicked, this, &SearchOverlay::cancelled);

    row->addWidget(m_spinner);
    row->addWidget(m_label);
    row->addWidget(m_cancel);
}

void SearchOverlay::setState(bool running, int found, const QString &runningText,
                             const QString &doneText, const QString &cancelText) {
    m_spinner->setVisible(running);
    m_label->setText(running ? runningText : doneText);
    m_cancel->setText(cancelText);
    Q_UNUSED(found);
    adjustSize();
}

void SearchOverlay::applyTheme(const QColor &text, const QColor &accent) {
    m_spinner->setColour(accent);
    QPalette pal = m_label->palette();
    pal.setColor(QPalette::WindowText, text);
    m_label->setPalette(pal);
}
