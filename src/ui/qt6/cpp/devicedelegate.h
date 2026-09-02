// How one removable disk is drawn in the writer's list.
//
// Three facts have to be readable at a glance, and they are not equally
// important. The model is what the person recognises. The size and the volume
// label are how they tell two of the same model apart. The device node is what
// they would check if they doubted any of it, and it is the one they will look
// at least often - so it is there, smallest and dimmest, rather than absent.
//
// Drawn rather than crammed into one string because a list item's text is one
// size, one weight and one colour, and three lines of that is a wall. This is
// the same reason the file list has its own delegate.
#pragma once

#include <QColor>
#include <QStyledItemDelegate>

class DeviceDelegate : public QStyledItemDelegate {
    Q_OBJECT

public:
    /// The roles the dialog fills in; the delegate reads no others.
    enum Role {
        ModelRole = Qt::UserRole + 10,
        DetailRole,  ///< Size, bus and what is mounted from it.
        NodeRole,    ///< The path that would be written to.
        RefusalRole, ///< Why it cannot be used. Empty when it can.
    };

    explicit DeviceDelegate(QObject *parent = nullptr) : QStyledItemDelegate(parent) {}

    /// Secondary text, the device node, and the colour a refusal is written in.
    void setColours(const QColor &dim, const QColor &faint, const QColor &warning);

    QSize sizeHint(const QStyleOptionViewItem &option, const QModelIndex &index) const override;
    void paint(QPainter *painter, const QStyleOptionViewItem &option,
               const QModelIndex &index) const override;

private:
    QColor m_dim;
    QColor m_faint;
    QColor m_warning;
};
