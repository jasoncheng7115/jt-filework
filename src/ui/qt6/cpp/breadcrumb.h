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

signals:
    void navigate(const QString &path);

protected:
    void resizeEvent(QResizeEvent *event) override;

private:
    void rebuild();

    QHBoxLayout *m_layout = nullptr;
    QString m_path;
};
