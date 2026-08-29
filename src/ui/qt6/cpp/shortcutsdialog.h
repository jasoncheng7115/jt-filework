// "What are the keys?", answered from the keymap rather than from a list
// someone has to remember to update.
//
// The command registry and the active keymap already know every binding, so
// this reads them at the moment it opens. A hand-written cheat sheet would be
// wrong the first time a binding changed, and wrong silently.
#pragma once

#include "bridge.h"

#include <QDialog>

class QLineEdit;
class QTreeWidget;

class ShortcutsDialog : public QDialog {
    Q_OBJECT

public:
    explicit ShortcutsDialog(JtfApp *app, QWidget *parent = nullptr);

private:
    QString tr_(const char *key) const;
    void rebuild(const QString &needle);

    JtfApp *m_app = nullptr;
    QLineEdit *m_search = nullptr;
    QTreeWidget *m_tree = nullptr;
};
