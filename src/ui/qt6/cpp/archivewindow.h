// What is inside an archive, in a window of its own.
//
// `CV.HLP` §四: pressing Enter on an archive shows its contents and they are
// acted on from there. A separate window rather than a pane, because an
// archive is not a folder - you cannot navigate into it, create in it or drop
// onto it, and a pane that looked like one would promise all three.
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

    JtfApp *m_app = nullptr;
    QString m_archive;
    QTableWidget *m_table = nullptr;
    IconProvider m_icons;
    QLabel *m_status = nullptr;
    QWidget *m_hints = nullptr;
    int m_unsafeCount = 0;
    bool m_readable = false;
};
