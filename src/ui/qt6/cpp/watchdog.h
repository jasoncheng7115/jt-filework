// UI-thread watchdog.
//
// AGENTS.md 3 forbids blocking the UI thread and AGENTS.md 18.2 puts a number
// on it: no single UI-thread task may exceed the frame budget. That is a
// claim, and docs/TESTING.md 7.1 requires it to be measured rather than
// believed.
//
// Every event Qt dispatches - input, timer, paint, layout - passes through
// QApplication::notify, so timing that one function measures the whole UI
// thread. The cost is one monotonic clock read per event, which is why this
// can be left on in ordinary runs rather than being a special build.
#pragma once

#include <QApplication>
#include <QElapsedTimer>
#include <QString>
#include <vector>

class WatchdogApplication : public QApplication {
public:
    WatchdogApplication(int &argc, char **argv);

    bool notify(QObject *receiver, QEvent *event) override;

    // Percentile summary in microseconds. Empty if nothing was recorded.
    QString report() const;

    /// Begin printing the report periodically as well as at exit.
    ///
    /// A diagnostic that can only be read by quitting cleanly is one you
    /// cannot read when it matters: a hang, a crash, or a session someone
    /// had to kill. This prints as it goes.
    void startPeriodicReports();

    bool enabled() const { return m_enabled; }

private:
    bool m_enabled;
    int m_depth = 0;      // only the outermost dispatch is timed
    QElapsedTimer m_timer;
    class QTimer *m_reportTimer = nullptr;
    qint64 m_started = 0;
    std::vector<qint64> m_samples; // microseconds
    qint64 m_overBudget = 0;
    qint64 m_worst = 0;
    QString m_worstEvent;
};
