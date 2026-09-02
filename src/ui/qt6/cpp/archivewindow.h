// What is inside an archive, in a window of its own.
//
// `CV.HLP` §四: pressing Enter on an archive shows its contents and they are
// acted on from there. A separate window rather than a pane, because an
// archive is not a folder - you cannot create in it or drop onto it, and a
// pane that looked like one would promise both.
//
// Inside it, though, folders *are* folders. The listing used to be flat: every
// member as its full stored path, `site/`, `site/page-1.html`,
// `site/assets/logo.svg`, one under the next with nothing to say which
// contained which. That is unreadable at a dozen members and useless at
// twenty thousand.
//
// It navigates now, and it navigates the way the file list does - Enter or
// Right descends, Backspace or Left goes up, and a row shows its own name
// rather than its whole path. No tree with expanders: the pane this window
// mirrors is a list you walk into, so a tree here would be a second way of
// moving around inside a window whose entire premise is that it is the same as
// the outside. The keys already exist and already mean the right thing.
//
// The keys are §四's, as far as this build can honour them:
//
//   C   extract the selected members     X   extract everything
//   Esc close
//
// `Enter` (view a member) and `G` (run one) need a member extracted to a
// temporary file first; they are not built and are absent rather than inert.
#pragma once

#include "bridge.h"
#include "iconprovider.h"

#include <QWidget>

#include <QSet>
#include <QString>
#include <QVector>

class QLabel;
class QTableWidget;

class ArchiveWindow : public QWidget {
    Q_OBJECT

public:
    ArchiveWindow(JtfApp *app, const QString &archive, QWidget *parent = nullptr);
    ~ArchiveWindow() override;

    /// Whether the archive could be read at all.
    bool isReadable() const { return m_readable; }

signals:
    /// Extract these members - empty means all of them - into a folder the
    /// user is about to be asked for. The window that owns the job runs it.
    void extractRequested(const QString &archive, const QStringList &members);

protected:
    void keyPressEvent(QKeyEvent *event) override;
    // The table has the focus, so it sees the keys first; these are claimed
    // from it rather than waited for.
    bool eventFilter(QObject *watched, QEvent *event) override;

private:
    QString tr_(const char *key) const;
    QStringList selectedMembers() const;
    QList<int> markedRows() const;
    void updateStatus();

    /// One member, as the archive stores it.
    struct Member {
        QString path; ///< The full stored path, which is what extraction needs.
        bool directory = false;
        bool unsafe = false;
        quint64 size = 0;
    };

    void populate();
    void descend(const QString &folder);
    void ascend();
    /// Mark or unmark a row; a folder carries everything beneath it with it.
    void setMarked(const QString &path, bool directory, bool marked);
    /// Whether anything under `folder` is marked.
    bool folderIsMarked(const QString &folder) const;
    /// The full stored path behind the row the cursor is on.
    QString pathOf(int row) const;
    bool rowIsDirectory(int row) const;

    JtfApp *m_app = nullptr;
    QString m_archive;
    /// Every member, read once. The table shows one level of it at a time.
    QVector<Member> m_members;
    /// Marks, by full stored path, so they survive walking in and out of
    /// folders - which is the whole reason they are not the table's own check
    /// states any more.
    QSet<QString> m_marked;
    /// The folder inside the archive now being shown. Empty is the root, and
    /// anything else ends in a slash.
    QString m_prefix;
    QLabel *m_where = nullptr;
    QTableWidget *m_table = nullptr;
    IconProvider m_icons;
    QLabel *m_status = nullptr;
    QWidget *m_hints = nullptr;
    int m_unsafeCount = 0;
    bool m_readable = false;
};
