// What the program is doing, and what it is about to do.
//
// The status bar could say "2 jobs" and no more. That is enough to know
// something is happening and not enough to act on it: which of two copies is
// running, how large the one behind it is, or whether the queued one is the
// one to drop. Copying a folder is minutes of work the user has committed to,
// and a queue they cannot see is a queue they cannot change their mind about.
//
// Refreshed on a timer rather than driven by signals: the job state lives
// behind the C boundary and is polled everywhere else in this program too,
// and a second mechanism for this one window would be a second thing to keep
// in step.
#pragma once

#include "bridge.h"

#include <QDialog>

class QTimer;
class QTreeWidget;
class QPushButton;

class JobsDialog : public QDialog {
    Q_OBJECT

public:
    JobsDialog(JtfApp *app, QWidget *parent);

private:
    QString tr_(const char *key) const;
    void refresh();

    JtfApp *m_app = nullptr;
    QTreeWidget *m_list = nullptr;
    QPushButton *m_cancel = nullptr;
    QPushButton *m_clear = nullptr;
    QTimer *m_poll = nullptr;
};
