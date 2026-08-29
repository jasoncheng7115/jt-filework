#include "breadcrumb.h"

#include <QHBoxLayout>
#include <QLabel>
#include <QPushButton>
#include <QResizeEvent>

namespace {
// Below this many segments there is never anything to hide.
constexpr int kMinimumSegments = 4;
} // namespace

Breadcrumb::Breadcrumb(QWidget *parent) : QWidget(parent) {
    setObjectName(QStringLiteral("JtfCrumbs"));
    m_layout = new QHBoxLayout(this);
    m_layout->setContentsMargins(6, 2, 6, 2);
    m_layout->setSpacing(0);
    m_layout->addStretch(1);
}

void Breadcrumb::setPath(const QString &path) {
    if (path == m_path) {
        return;
    }
    m_path = path;
    rebuild();
}

void Breadcrumb::resizeEvent(QResizeEvent *event) {
    QWidget::resizeEvent(event);
    // What fits changes with the width, so the elision is recomputed rather
    // than decided once.
    rebuild();
}

void Breadcrumb::rebuild() {
    while (QLayoutItem *item = m_layout->takeAt(0)) {
        delete item->widget();
        delete item;
    }

    const QStringList parts = m_path.split(QLatin1Char('/'), Qt::SkipEmptyParts);
    QStringList labels;
    QStringList paths;
    labels << QStringLiteral("/");
    paths << QStringLiteral("/");

    QString walked;
    for (const QString &part : parts) {
        walked += QLatin1Char('/') + part;
        labels << part;
        paths << walked;
    }

    // Work out how many trailing segments fit, then hide the middle. The last
    // segment is never dropped: it is the folder you are in.
    const QFontMetrics metrics(font());
    int available = width() - 24;
    int firstShown = 0;
    if (labels.size() > kMinimumSegments) {
        int used = metrics.horizontalAdvance(QStringLiteral("/  …  "));
        for (int i = labels.size() - 1; i >= 0; --i) {
            used += metrics.horizontalAdvance(labels.at(i)) + 22;
            if (used > available && i < labels.size() - 1) {
                firstShown = i + 1;
                break;
            }
        }
    }

    const auto addSegment = [this](const QString &label, const QString &path) {
        auto *button = new QPushButton(label, this);
        button->setFlat(true);
        button->setCursor(Qt::PointingHandCursor);
        button->setProperty("jtfCrumb", true);
        connect(button, &QPushButton::clicked, this, [this, path] { emit navigate(path); });
        m_layout->addWidget(button);
    };

    if (firstShown > 0) {
        addSegment(labels.first(), paths.first());
        auto *ellipsis = new QLabel(QStringLiteral("…"), this);
        ellipsis->setProperty("jtfCrumbSeparator", true);
        m_layout->addWidget(ellipsis);
    }
    for (int i = firstShown; i < labels.size(); ++i) {
        if (i > firstShown || firstShown > 0) {
            auto *separator = new QLabel(QStringLiteral("›"), this);
            separator->setProperty("jtfCrumbSeparator", true);
            m_layout->addWidget(separator);
        }
        addSegment(labels.at(i), paths.at(i));
    }
    m_layout->addStretch(1);
    Q_UNUSED(available);
}
