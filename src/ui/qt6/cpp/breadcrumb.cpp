#include "breadcrumb.h"

#include <QHBoxLayout>
#include <QLabel>
#include <QPushButton>
#include <QKeyEvent>
#include <QLineEdit>
#include <QMouseEvent>
#include <QResizeEvent>
#include <QVBoxLayout>

namespace {
// Below this many segments there is never anything to hide.
constexpr int kMinimumSegments = 4;
} // namespace

Breadcrumb::Breadcrumb(QWidget *parent) : QWidget(parent) {
    setObjectName(QStringLiteral("JtfCrumbs"));
    setCursor(Qt::IBeamCursor);

    auto *stack = new QVBoxLayout(this);
    stack->setContentsMargins(0, 0, 0, 0);

    m_crumbHost = new QWidget(this);
    m_layout = new QHBoxLayout(m_crumbHost);
    m_layout->setContentsMargins(6, 2, 6, 2);
    m_layout->setSpacing(0);
    m_layout->addStretch(1);
    stack->addWidget(m_crumbHost);

    m_edit = new QLineEdit(this);
    m_edit->setObjectName(QStringLiteral("JtfPathEdit"));
    m_edit->setFrame(false);
    m_edit->setVisible(false);
    connect(m_edit, &QLineEdit::returnPressed, this, [this] { endEditing(true); });
    m_edit->installEventFilter(this);
    stack->addWidget(m_edit);
}

void Breadcrumb::beginEditing() {
    m_edit->setText(m_path);
    m_crumbHost->setVisible(false);
    m_edit->setVisible(true);
    m_edit->setFocus();
    m_edit->selectAll();
}

void Breadcrumb::endEditing(bool navigateThere) {
    const QString typed = m_edit->text().trimmed();
    m_edit->setVisible(false);
    m_crumbHost->setVisible(true);
    // Abandoning an edit puts the real path back, so a half-typed path never
    // stays on screen pretending to be where you are.
    if (navigateThere && !typed.isEmpty() && typed != m_path) {
        emit navigate(typed);
    }
}

bool Breadcrumb::eventFilter(QObject *watched, QEvent *event) {
    if (watched == m_edit && event->type() == QEvent::KeyPress) {
        if (static_cast<QKeyEvent *>(event)->key() == Qt::Key_Escape) {
            endEditing(false);
            return true;
        }
    }
    if (watched == m_edit && event->type() == QEvent::FocusOut) {
        endEditing(false);
    }
    return QWidget::eventFilter(watched, event);
}

void Breadcrumb::mousePressEvent(QMouseEvent *event) {
    // Only the empty space: a click on a crumb is a click on that crumb.
    Q_UNUSED(event);
    beginEditing();
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
    if (m_edit != nullptr && m_edit->isVisible()) {
        return; // never rebuild the crumbs out from under an edit in progress
    }
    while (QLayoutItem *item = m_layout->takeAt(0)) {
        if (QWidget *widget = item->widget()) {
            // Hidden and reparented out of the layout now, destroyed later.
            // Deleting a widget outright leaves any event already posted to
            // it - a show, a polish - pointing at freed memory, and the crash
            // lands far away in the event loop rather than here. This rebuild
            // runs on every resize, so there is always something in flight.
            widget->hide();
            widget->setParent(nullptr);
            widget->deleteLater();
        }
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
        auto *button = new QPushButton(label, m_crumbHost);
        button->setFlat(true);
        button->setCursor(Qt::PointingHandCursor);
        button->setProperty("jtfCrumb", true);
        connect(button, &QPushButton::clicked, this, [this, path] { emit navigate(path); });
        m_layout->addWidget(button);
    };

    if (firstShown > 0) {
        addSegment(labels.first(), paths.first());
        auto *ellipsis = new QLabel(QStringLiteral("…"), m_crumbHost);
        ellipsis->setProperty("jtfCrumbSeparator", true);
        m_layout->addWidget(ellipsis);
    }
    for (int i = firstShown; i < labels.size(); ++i) {
        if (i > firstShown || firstShown > 0) {
            auto *separator = new QLabel(QStringLiteral("›"), m_crumbHost);
            separator->setProperty("jtfCrumbSeparator", true);
            m_layout->addWidget(separator);
        }
        addSegment(labels.at(i), paths.at(i));
    }
    m_layout->addStretch(1);
    Q_UNUSED(available);
}
