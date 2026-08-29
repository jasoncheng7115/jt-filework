// The command palette.
//
// docs/UI_UX_SPEC.md 7 asks for one, and the registry already knows every
// command, its category, its localized name and its shortcut - so the palette
// is a view over data the application already has rather than a second list
// that can fall out of step with the menus.
//
// It is also the answer to discoverability: a keyboard-first application whose
// commands can only be found by reading menus is keyboard-first in name only.
#pragma once

#include "bridge.h"

#include <QDialog>
#include <QVector>

class QLineEdit;
class QListWidget;

class CommandPalette : public QDialog {
    Q_OBJECT

public:
    CommandPalette(JtfApp *app, QWidget *parent = nullptr);

    // The command the user chose, or empty if they backed out.
    QString chosen() const { return m_chosen; }

protected:
    bool eventFilter(QObject *watched, QEvent *event) override;

private:
    struct Entry {
        QString id;
        QString label;
        QString category;
        QString shortcut;
        bool enabled = true;
    };

    void load();
    void filter(const QString &needle);
    void accept_();

    JtfApp *m_app;
    QLineEdit *m_search = nullptr;
    QListWidget *m_list = nullptr;
    QVector<Entry> m_entries;
    QString m_chosen;
};
