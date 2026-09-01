// A clickable path.
//
// docs/UI_UX_SPEC.md 2 asks for a breadcrumb whose segments navigate, and
// WIN-011 asks it to truncate the middle on a deep path rather than the leaf:
// the folder you are in is the one part you always need to see.
#pragma once

#include <QPixmap>
#include <QWidget>

#include <functional>

class QHBoxLayout;
class QCompleter;
class QStringListModel;

class Breadcrumb : public QWidget {
    Q_OBJECT

public:
    explicit Breadcrumb(QWidget *parent = nullptr);

    void setPath(const QString &path);
    /// A small mark at the head of the trail, so the row reads as a path.
    void setLeadingIcon(const QPixmap &icon);

    /// Switch to the editable full path, selected and focused.
    ///
    /// The breadcrumb and the path field are one control: the pretty form is
    /// what you read, the text form is what you type into, and clicking the
    /// empty space beside the crumbs is how you get from one to the other.
    /// Two separate widgets showing the same path was one widget too many.
    void beginEditing();

    /// Where the completer gets its folders.
    ///
    /// A function rather than a `QFileSystemModel`: Qt's own model would walk
    /// the disk itself, which is a second source of truth about what a folder
    /// contains (`AGENTS.md` §4) and knows nothing about a server. The pane
    /// hands over the same call the folder tree uses, so what completes here
    /// and what the tree shows cannot disagree - and typing a path on a server
    /// completes too.
    void setCompletionSource(std::function<QStringList(const QString &folder)> source);

signals:
    void navigate(const QString &path);
    /// Right-click on one segment: the path it names, and where to pop up.
    void segmentMenuRequested(const QString &path, const QPoint &global);

protected:
    void resizeEvent(QResizeEvent *event) override;
    void mousePressEvent(QMouseEvent *event) override;
    bool eventFilter(QObject *watched, QEvent *event) override;

private:
    void rebuild();

    void endEditing(bool navigateThere);
    /// Refresh the completer's list for whatever folder is being typed into.
    void refreshCompletions(const QString &typed);

    QHBoxLayout *m_layout = nullptr;
    class QLineEdit *m_edit = nullptr;
    class QWidget *m_crumbHost = nullptr;
    class QLabel *m_leading = nullptr;
    QPixmap m_leadingIcon;
    QString m_path;
    int m_builtForWidth = -1;
    bool m_rebuilding = false;
    QCompleter *m_completer = nullptr;
    QStringListModel *m_completions = nullptr;
    /// The folder the current completion list is for, so it is refreshed when
    /// the typing crosses into a different one and not on every keystroke.
    QString m_completionFolder;
    std::function<QStringList(const QString &folder)> m_completionSource;
};
