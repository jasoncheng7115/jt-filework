// Where the space went.
//
// Two answers to one walk, side by side:
//
//   * which child folder holds the most, so there is somewhere to go and look;
//   * which kind of file adds up to the most, so「照片佔了 40 GB」is a question
//     with an answer. The tools people use for this mostly cannot say that,
//     and it is the more useful half as often as not.
//
// A window rather than a pane, for the reason the archive listing is one: this
// is a report about a folder, not a folder. Nothing in it can be created in or
// dropped onto — but a folder row can be *gone to*, because the whole point of
// finding the big branch is going there.
//
// The walk runs on a worker thread in Rust and is polled here: reading a disc
// takes as long as the disc takes, and a window that stops painting while it
// works is what AGENTS.md 3 forbids.
#pragma once

#include "bridge.h"
#include "iconprovider.h"

#include <QHash>
#include <QIcon>
#include <QWidget>

class QLabel;
class QPushButton;
class QTableWidget;
class QTimer;

class UsageWindow : public QWidget {
    Q_OBJECT

public:
    UsageWindow(JtfApp *app, const QString &path, QWidget *parent = nullptr);
    ~UsageWindow() override;

signals:
    /// Show this folder in the active pane. Offered from the row's menu, for
    /// when the answer is "go and look at it" rather than "measure inside it".
    void folderChosen(const QString &path);

    /// Something in the measured folder was copied, moved or trashed from
    /// here, so whatever else is showing that folder is now out of date.
    void folderChanged();

protected:
    void keyPressEvent(QKeyEvent *event) override;
    /// Re-applies every colour this window draws itself, on a theme change.
    void changeEvent(QEvent *event) override;
    // The tables have the focus, so the window's keys are claimed from them
    // rather than waited for.
    bool eventFilter(QObject *watched, QEvent *event) override;

private:
    QString tr_(const char *key) const;
    void run();
    /// Measure `path` instead, remembering where we came from.
    void descendTo(const QString &path);
    /// Measure the folder we descended from, if there is one.
    void goUp();
    /// The folder the current row is about, or empty for a row that is not one.
    /// The icon for a row, asking the platform about a *type* where the row is
    /// about one rather than about a file that exists.
    QIcon icon(const QString &nameOrPath, bool isFolder);
    QString folderAt(int row) const;
    /// The file or folder a row is about - either can be acted on, and this
    /// side lists both.
    QString targetAt(int row) const;
    /// Copy, move or trash the row the cursor is on. `kind` is an `ops::Kind`.
    void runOn(int kind);
    void showRowMenu(const QPoint &at);
    void poll();
    void fill();

    JtfApp *m_app = nullptr;
    QString m_path;
    QLabel *m_heading = nullptr;
    QTableWidget *m_folders = nullptr;
    QTableWidget *m_kinds = nullptr;
    QLabel *m_status = nullptr;
    /// The folder the walk is in, at the right-hand end of the status line.
    QLabel *m_where = nullptr;
    QPushButton *m_cancel = nullptr;
    /// Turns while the walk runs, so a slow folder does not look like a hang.
    class Spinner *m_spinner = nullptr; // searchoverlay.h
    QTimer *m_poll = nullptr;
    /// Waits for a started operation to finish, then measures again: the
    /// report is about a folder that has just changed.
    QTimer *m_afterOperation = nullptr;
    IconProvider m_icons;
    /// One platform lookup per extension, not per row.
    QHash<QString, QIcon> m_byExtension;
    /// Where this window has been, so going up returns rather than guesses.
    QStringList m_trail;
    class QToolButton *m_up = nullptr;
};
