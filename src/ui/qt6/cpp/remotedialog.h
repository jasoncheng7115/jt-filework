// Connecting to a host over SFTP.
//
// Deliberately not a password box. Authentication is by ssh-agent or by a key
// in `~/.ssh`, which is what a user who can already `sftp host` has; asking
// for a password here would mean this program holding one, and a file manager
// should not be a credential store (`docs/adr/0004-sftp.md`).
//
// The host key question is asked here rather than after a failure. A user who
// has just typed a hostname is in a position to say whether they expected to
// meet it for the first time; the same user three screens later, reading an
// error, is not.
#pragma once

#include "bridge.h"

#include <QDialog>
#include <QString>

class QCheckBox;
class QLineEdit;
class QSpinBox;

class RemoteDialog : public QDialog {
    Q_OBJECT

public:
    RemoteDialog(JtfApp *app, QWidget *parent);

    QString host() const;
    int port() const;
    QString user() const;
    QString path() const;
    /// The password typed, if any. Used once; never stored.
    QString password() const;

    /// Whether the user said to trust a host key they have not seen before.
    bool trustUnknownHost() const;

private:
    QString tr_(const char *key) const;

    JtfApp *m_app = nullptr;
    QLineEdit *m_host = nullptr;
    QSpinBox *m_port = nullptr;
    QLineEdit *m_user = nullptr;
    QLineEdit *m_path = nullptr;
    QLineEdit *m_password = nullptr;
    QCheckBox *m_trust = nullptr;
};
