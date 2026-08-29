#include "watchdog.h"

#include <QDebug>
#include <QTimer>

#include <cstdio>
#include <QEvent>
#include <QMetaEnum>
#include <algorithm>

namespace {
// One frame at 60Hz, the budget in docs/TESTING.md 8.2.
constexpr qint64 kBudgetMicros = 16'000;
// Enough for a long session; sampling stops rather than growing without
// bound, because a diagnostic must not become the leak it is looking for.
constexpr size_t kMaxSamples = 2'000'000;

QString eventName(int type) {
    const QMetaEnum meta = QMetaEnum::fromType<QEvent::Type>();
    const char *key = meta.valueToKey(type);
    return key ? QString::fromLatin1(key) : QStringLiteral("Event(%1)").arg(type);
}
} // namespace

WatchdogApplication::WatchdogApplication(int &argc, char **argv)
    : QApplication(argc, argv), m_enabled(!qEnvironmentVariableIsEmpty("JTF_WATCHDOG")) {
    m_timer.start();
}

bool WatchdogApplication::notify(QObject *receiver, QEvent *event) {
    if (!m_enabled) {
        return QApplication::notify(receiver, event);
    }
    // Nested dispatches are already inside a timed one; timing them again
    // would double-count the same wall time.
    if (m_depth++ > 0) {
        const bool handled = QApplication::notify(receiver, event);
        --m_depth;
        return handled;
    }

    const qint64 started = m_timer.nsecsElapsed();
    const bool handled = QApplication::notify(receiver, event);
    const qint64 micros = (m_timer.nsecsElapsed() - started) / 1000;
    --m_depth;

    if (m_samples.size() < kMaxSamples) {
        m_samples.push_back(micros);
    }
    if (micros > kBudgetMicros) {
        ++m_overBudget;
    }
    if (micros > m_worst) {
        m_worst = micros;
        m_worstEvent = eventName(event->type());
    }
    return handled;
}

void WatchdogApplication::startPeriodicReports() {
    if (!m_enabled || m_reportTimer != nullptr) {
        return;
    }
    // Long enough not to be part of what it measures, short enough to be
    // useful in a session nobody gets to end cleanly.
    constexpr int kIntervalMs = 10'000;
    m_reportTimer = new QTimer(this);
    connect(m_reportTimer, &QTimer::timeout, this, [this] {
        const QString text = report();
        if (!text.isEmpty()) {
            std::fputs(qPrintable(text), stderr);
            std::fflush(stderr);
        }
    });
    m_reportTimer->start(kIntervalMs);
}

QString WatchdogApplication::report() const {
    if (m_samples.empty()) {
        return {};
    }
    std::vector<qint64> sorted = m_samples;
    std::sort(sorted.begin(), sorted.end());
    const auto at = [&](double q) {
        const size_t index =
            std::min(sorted.size() - 1, static_cast<size_t>(q * double(sorted.size())));
        return sorted[index];
    };

    return QStringLiteral(
               "UI-thread watchdog\n"
               "  events        %1\n"
               "  p50           %2 us\n"
               "  p95           %3 us\n"
               "  p99           %4 us\n"
               "  worst         %5 us  (%6)\n"
               "  over 16000 us %7  (%8%)\n")
        .arg(sorted.size())
        .arg(at(0.50))
        .arg(at(0.95))
        .arg(at(0.99))
        .arg(m_worst)
        .arg(m_worstEvent)
        .arg(m_overBudget)
        .arg(100.0 * double(m_overBudget) / double(sorted.size()), 0, 'f', 3);
}
