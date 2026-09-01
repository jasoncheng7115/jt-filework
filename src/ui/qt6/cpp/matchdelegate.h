// Draws a file name with the searched-for text picked out.
//
// A results list that does not say *why* a row is in it makes the reader
// compare each name against the query themselves. Highlighting the matched
// run turns that into something the eye does.
//
// A delegate rather than rich text in the model: the model's job is to say
// what a row is, not how it looks, and HTML in a file name would be a way for
// a file called `<b>` to change the display.
#pragma once

#include <QColor>

#include "rowdelegate.h"

class MatchDelegate : public RowDelegate {
    Q_OBJECT

public:
    explicit MatchDelegate(QObject *parent = nullptr);

    /// The text to pick out. Empty draws normally.
    void setNeedle(const QString &needle);
    void setHighlight(const QColor &background, const QColor &text);

protected:
    void paint(QPainter *painter, const QStyleOptionViewItem &option,
               const QModelIndex &index) const override;

private:
    QString m_needle;
    QColor m_background;
    QColor m_text;
};
