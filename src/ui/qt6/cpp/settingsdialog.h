// The settings window.
//
// Everything here edits data the model owns: startup behaviour, fonts, the
// keymap, the theme, the locale. The dialog holds no state of its own and
// applies changes as they are made, so there is no OK/Cancel asymmetry where
// half the panel is live and half is not.
#pragma once

#include "bridge.h"

#include <QDialog>

class QTableWidget;
class QLineEdit;
class QComboBox;
class QCheckBox;
class QSpinBox;
class QLabel;

class SettingsDialog : public QDialog {
    Q_OBJECT

public:
    SettingsDialog(JtfApp *app, QWidget *parent = nullptr);

signals:
    // Anything that changes how the window looks or behaves.
    void changed();

private:
    void buildTabs();
    QWidget *buildGeneralTab();
    QWidget *buildAppearanceTab();
    QWidget *buildKeyboardTab();

    void reloadShortcuts();
    void editShortcut(int row);

    QString tr_(const char *key) const;
    QString trKey(const QString &key) const;

    JtfApp *m_app;
    class QTabWidget *m_tabs = nullptr;
    class QDialogButtonBox *m_buttons = nullptr;
    QComboBox *m_startupMode = nullptr;
    QLineEdit *m_startupLocation = nullptr;
    QCheckBox *m_rememberTabs = nullptr;
    QCheckBox *m_rememberMarks = nullptr;
    QTableWidget *m_shortcuts = nullptr;
    QLabel *m_shortcutHint = nullptr;
};

// Captures one chord. A key press is the only honest way to ask for a
// shortcut: typing "Ctrl+Shift+K" into a text field is a spelling test.
class ShortcutCapture : public QDialog {
    Q_OBJECT

public:
    ShortcutCapture(const QString &title, const QString &prompt, QWidget *parent);

    // The chord in the keymap file's own syntax, empty if nothing was pressed.
    QString chord() const { return m_chord; }

protected:
    void keyPressEvent(QKeyEvent *event) override;

private:
    QString m_chord;
    QLabel *m_display = nullptr;
};
