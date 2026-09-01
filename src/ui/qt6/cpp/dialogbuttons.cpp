#include "dialogbuttons.h"

#include "icons.h"

#include <QAbstractButton>
#include <QDialog>
#include <QDialogButtonBox>
#include <QLabel>
#include <QLineEdit>
#include <QPushButton>
#include <QVBoxLayout>

namespace dialogs {

void localizeButtons(QDialogButtonBox *box, const std::function<QString(const char *)> &translate,
                     const QColor &iconColour) {
    if (box == nullptr) {
        return;
    }
    struct Entry {
        QDialogButtonBox::StandardButton button;
        const char *key;
        glyph::Shape shape;
    };
    // Only the buttons this program actually uses. A button that turns up
    // without an entry keeps Qt's text rather than being given a wrong one.
    static const Entry kEntries[] = {
        {QDialogButtonBox::Ok, "dialog.ok", glyph::Shape::Check},
        {QDialogButtonBox::Cancel, "dialog.cancel", glyph::Shape::Close},
        {QDialogButtonBox::Close, "dialog.close", glyph::Shape::Close},
        {QDialogButtonBox::Apply, "dialog.apply", glyph::Shape::Check},
        {QDialogButtonBox::Save, "dialog.save", glyph::Shape::Check},
    };
    for (const Entry &entry : kEntries) {
        if (QAbstractButton *button = box->button(entry.button)) {
            button->setText(translate(entry.key));
            button->setIcon(glyph::make(entry.shape, iconColour));
        }
    }
}

namespace {

/// Shared by both prompts; `hidden` decides whether the typing shows.
QString askOneLine(QWidget *parent, const std::function<QString(const char *)> &translate,
                   const QString &title, const QString &label, const QString &initial,
                   const QColor &iconColour, bool hidden, bool *accepted);

} // namespace

QString askForPassword(QWidget *parent, const std::function<QString(const char *)> &translate,
                       const QString &title, const QString &label, const QColor &iconColour,
                       bool *accepted) {
    return askOneLine(parent, translate, title, label, QString(), iconColour, true, accepted);
}

QString askForText(QWidget *parent, const std::function<QString(const char *)> &translate,
                   const QString &title, const QString &label, const QString &initial,
                   const QColor &iconColour, bool *accepted) {
    return askOneLine(parent, translate, title, label, initial, iconColour, false, accepted);
}

namespace {

QString askOneLine(QWidget *parent, const std::function<QString(const char *)> &translate,
                   const QString &title, const QString &label, const QString &initial,
                   const QColor &iconColour, bool hidden, bool *accepted) {
    // Built here rather than borrowed from QInputDialog. That version reached
    // into the dialog with findChild to reword Qt's buttons, and when the
    // search came back empty it fell back to Qt's own words without saying so
    // - which is why 重新命名 kept coming up with "Cancel" and "OK" long after
    // every other dialog had been translated. Owning the widgets means the
    // buttons are ours by construction, with nothing to fail quietly.
    QDialog dialog(parent);
    dialog.setWindowTitle(title);

    auto *layout = new QVBoxLayout(&dialog);
    layout->setContentsMargins(16, 16, 16, 16);
    layout->setSpacing(10);

    auto *caption = new QLabel(label, &dialog);
    layout->addWidget(caption);

    auto *field = new QLineEdit(initial, &dialog);
    if (hidden) {
        field->setEchoMode(QLineEdit::Password);
    }
    // A name is the whole reason this dialog exists, so it gets room to be
    // read. Qt's default is narrow enough that a normal filename arrives
    // already scrolled off both ends.
    field->setMinimumWidth(420);
    caption->setBuddy(field);
    layout->addWidget(field);

    auto *buttons = new QDialogButtonBox(QDialogButtonBox::Ok | QDialogButtonBox::Cancel, &dialog);
    QObject::connect(buttons, &QDialogButtonBox::accepted, &dialog, &QDialog::accept);
    QObject::connect(buttons, &QDialogButtonBox::rejected, &dialog, &QDialog::reject);
    localizeButtons(buttons, translate, iconColour);
    layout->addWidget(buttons);

    // Renaming `photo.jpg` is nearly always about `photo`, not about `.jpg`,
    // so the extension is left out of the initial selection - the same thing
    // the platform's own rename does. With no dot, everything is selected.
    const int dot = hidden ? -1 : initial.lastIndexOf(QLatin1Char('.'));
    if (dot > 0) {
        field->setSelection(0, dot);
    } else {
        field->selectAll();
    }
    field->setFocus();

    const bool confirmed = dialog.exec() == QDialog::Accepted;
    if (accepted != nullptr) {
        *accepted = confirmed;
    }
    return confirmed ? field->text() : QString();
}

} // namespace

} // namespace dialogs
