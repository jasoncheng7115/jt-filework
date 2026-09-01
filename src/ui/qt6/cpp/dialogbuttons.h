// Localized, iconed buttons for a QDialogButtonBox.
//
// Qt fills a standard button's text from its own translation catalogue, which
// this program does not ship - so every dialog came up with English "OK" and
// "Cancel" in a Chinese UI, and with no icon, while every other control in the
// program had both. The wording lives in our catalogue with the rest of the
// UI, so a dialog says the same word for "cancel" that the menus do.
#pragma once

#include <QColor>
#include <functional>

class QDialogButtonBox;
class QWidget;

namespace dialogs {

/// Give every standard button in `box` its catalogue text and its icon.
/// `translate` takes a catalogue key, as the owning dialog's own tr_ does.
void localizeButtons(QDialogButtonBox *box, const std::function<QString(const char *)> &translate,
                     const QColor &iconColour);

/// Ask for one line of text, with buttons that speak the user's language.
///
/// `QInputDialog::getText` is the convenient way to do this and builds its own
/// dialog, whose OK and Cancel come from Qt's translations - which this
/// program does not ship. Every prompt that used it came up in English, in an
/// interface that was otherwise Chinese. Rewording Qt's own dialog was tried
/// first and depended on finding its button box by search; when that search
/// came back empty the prompt silently reverted to English. This builds the
/// dialog instead, so the buttons cannot be anything but ours.
///
/// Returns the text, and sets `accepted` to whether the user confirmed.
QString askForText(QWidget *parent, const std::function<QString(const char *)> &translate,
                   const QString &title, const QString &label, const QString &initial,
                   const QColor &iconColour, bool *accepted);

/// The same, with the typing hidden.
///
/// A separate call rather than a flag on the one above, so that asking for a
/// password is a deliberate act at every call site: a prompt that echoes a
/// password because someone passed the wrong argument is not a mistake worth
/// leaving available.
QString askForPassword(QWidget *parent, const std::function<QString(const char *)> &translate,
                       const QString &title, const QString &label, const QColor &iconColour,
                       bool *accepted);

} // namespace dialogs
