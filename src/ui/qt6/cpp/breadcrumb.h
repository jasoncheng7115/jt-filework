// A clickable path.
//
// docs/UI_UX_SPEC.md 2 asks for a breadcrumb whose segments navigate, and
// WIN-011 asks it to truncate the middle on a deep path rather than the leaf:
// the folder you are in is the one part you always need to see.
#pragma once

#include <QWidget>

class QHBoxLayout;

class Breadcrumb : public QWidget {
    Q_OBJECT

public:
    explicit Breadcrumb(QWidget *parent = nullptr);

    void setPath(const QString &path);

    /// Switch to the editable full path, selected and focused.
    ///
    /// The breadcrumb and the path field are one control: the pretty form is
    /// what you read, the text form is what you type into, and clicking the
    /// empty space beside the crumbs is how you get from one to the other.
    /// Two separate widgets showing the same path was one widget too many.
    void beginEditing();

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

    QHBoxLayout *m_layout = nullptr;
    class QLineEdit *m_edit = nullptr;
    class QWidget *m_crumbHost = nullptr;
    QString m_path;
};
