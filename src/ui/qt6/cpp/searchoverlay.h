// What a running search looks like, and how to stop it.
//
// A search across a deep tree takes as long as it takes. Without something on
// screen saying so, a list that is filling slowly is indistinguishable from a
// list that is finished and short - and there was no way to call the search
// off short of clearing the box and hoping. This says "still going", says how
// much it has found, and offers the one button that matters.
//
// A floating child of the pane rather than a row in its layout: it appears and
// disappears constantly, and anything that changes a layout's size hint that
// often ends up fighting the layout.
#pragma once

#include <QColor>
#include <QWidget>

class QLabel;
class QPushButton;

/// A ring that turns while something is running.
class Spinner : public QWidget {
    Q_OBJECT

public:
    explicit Spinner(QWidget *parent = nullptr);

    void setColour(const QColor &colour);
    /// Runs only while visible: a timer ticking behind a hidden widget is a
    /// wakeup every frame for a repaint nobody sees.
    void showEvent(QShowEvent *event) override;
    void hideEvent(QHideEvent *event) override;

    QSize sizeHint() const override { return {18, 18}; }

protected:
    void paintEvent(QPaintEvent *event) override;

private:
    int m_angle = 0;
    QColor m_colour;
    class QTimer *m_timer = nullptr;
};

class SearchOverlay : public QWidget {
    Q_OBJECT

public:
    explicit SearchOverlay(QWidget *parent = nullptr);

    /// `running` keeps the spinner turning; `found` is what to report so far.
    void setState(bool running, int found, const QString &runningText,
                  const QString &doneText, const QString &cancelText);
    void applyTheme(const QColor &text, const QColor &accent);

signals:
    void cancelled();

private:
    Spinner *m_spinner = nullptr;
    QLabel *m_label = nullptr;
    QPushButton *m_cancel = nullptr;
};
