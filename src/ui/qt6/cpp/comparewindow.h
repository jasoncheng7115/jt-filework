// What differs between two panes' folders, in a window of its own.
//
// Two panes side by side and the question「這兩邊差在哪」. The answer is a flat
// list of names with a verdict against each, so the eye runs down one column
// instead of back and forth between two.
//
// A window rather than a pane, for the same reason the archive listing is one:
// this is a report about two folders, not a third folder. Nothing in it can be
// navigated into, created in or dropped onto.
//
// The walk itself runs on a worker thread in Rust and is polled here, because
// comparing two trees is disk-bound - doubled, and network-bound on a server -
// and a window that stops painting while it works is the thing AGENTS.md 3
// forbids.
#pragma once

#include "bridge.h"

#include <QWidget>

class QCheckBox;
class QLabel;
class QPushButton;
class QTableWidget;
class QTimer;

class CompareWindow : public QWidget {
    Q_OBJECT

public:
    CompareWindow(JtfApp *app, int leftPane, int rightPane, QWidget *parent = nullptr);
    ~CompareWindow() override;

signals:
    // The comparison finished or was re-run; the main window repaints, because
    // a pane's own status line counts marks this window has not touched but
    // the window it belongs to may still want a turn of the loop.
    void stateChanged();

protected:
    void keyPressEvent(QKeyEvent *event) override;

private:
    QString tr_(const char *key) const;
    // The last segment of a path: what a person calls that folder.
    static QString shortName(const QString &path);
    // Start (or restart) the comparison with the boxes as they now stand.
    void run();
    // Take what the worker has said and, when it has finished, fill the table.
    void poll();
    void fill();
    void updateStatus();

    JtfApp *m_app = nullptr;
    int m_leftPane = -1;
    int m_rightPane = -1;
    // Named after the folders rather than after where their panes sit: a
    // split can be top and bottom, and then「左」and「右」name nothing.
    QString m_firstPath;
    QString m_secondPath;
    QString m_firstName;
    QString m_secondName;
    QCheckBox *m_recursive = nullptr;
    QCheckBox *m_showSame = nullptr;
    QLabel *m_heading = nullptr;
    QTableWidget *m_table = nullptr;
    QLabel *m_status = nullptr;
    QPushButton *m_cancel = nullptr;
    QTimer *m_poll = nullptr;
};
