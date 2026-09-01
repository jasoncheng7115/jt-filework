// What this program is, which version, and under what licence.
//
// Every application has one and people look for it when they need to report
// something: the version, the licence and where the source is are the three
// facts a bug report needs and the three a user has no other way to find.
#pragma once

#include "bridge.h"

#include <QDialog>

class AboutDialog : public QDialog {
    Q_OBJECT

public:
    explicit AboutDialog(JtfApp *app, QWidget *parent);

private:
    QString tr_(const char *key) const;

    JtfApp *m_app = nullptr;
};
