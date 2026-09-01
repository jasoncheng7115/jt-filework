#include "remotedialog.h"

#include "dialogbuttons.h"
#include "icons.h"
#include "jtfstring.h"

#include <QCheckBox>
#include <QAbstractButton>
#include <QDialogButtonBox>
#include <QFormLayout>
#include <QFrame>
#include <QHBoxLayout>
#include <QLabel>
#include <QLineEdit>
#include <QPushButton>
#include <QSpinBox>
#include <QVBoxLayout>

RemoteDialog::RemoteDialog(JtfApp *app, QWidget *parent) : QDialog(parent), m_app(app) {
    setWindowTitle(tr_("remote.title"));
    // Wide enough for the placeholders to be read rather than elided: they
    // are the only thing telling a first-time user what shape of value each
    // field wants.
    setMinimumWidth(560);

    auto *layout = new QVBoxLayout(this);
    layout->setContentsMargins(16, 14, 16, 12);
    layout->setSpacing(10);

    auto *form = new QFormLayout;
    form->setLabelAlignment(Qt::AlignRight | Qt::AlignVCenter);
    form->setHorizontalSpacing(14);
    form->setVerticalSpacing(10);
    // Without this the fields keep their own idea of a width and the dialog
    // grows around them, which is how a 540px window ended up with 140px
    // boxes and placeholders nobody could read.
    form->setFieldGrowthPolicy(QFormLayout::AllNonFixedFieldsGrow);

    m_host = new QLineEdit(this);
    m_host->setPlaceholderText(tr_("remote.host_placeholder"));
    m_user = new QLineEdit(this);
    m_user->setPlaceholderText(tr_("remote.user_placeholder"));
    m_path = new QLineEdit(this);
    m_path->setText(QStringLiteral("/"));

    // The port sits beside the host rather than on a row of its own: it is
    // part of the same answer, it is almost always 22, and a whole row for
    // three characters pushes everything else down.
    auto *hostRow = new QWidget(this);
    auto *hostLayout = new QHBoxLayout(hostRow);
    hostLayout->setContentsMargins(0, 0, 0, 0);
    hostLayout->setSpacing(8);
    m_port = new QSpinBox(hostRow);
    m_port->setRange(1, 65535);
    m_port->setValue(22);
    m_port->setFixedWidth(84);
    auto *portLabel = new QLabel(tr_("remote.port"), hostRow);
    portLabel->setProperty("jtfFactLabel", true);
    hostLayout->addWidget(m_host, 1);
    hostLayout->addWidget(portLabel);
    hostLayout->addWidget(m_port);

    form->addRow(tr_("remote.host"), hostRow);
    form->addRow(tr_("remote.user"), m_user);
    // Last, and optional. A key or the agent is tried first whatever is
    // typed here, so leaving it empty is the normal case; it exists because a
    // server that only accepts passwords is common and refusing to talk to
    // one would make the feature useless.
    m_password = new QLineEdit(this);
    m_password->setEchoMode(QLineEdit::Password);
    m_password->setPlaceholderText(tr_("remote.password_placeholder"));

    form->addRow(tr_("remote.path"), m_path);
    form->addRow(tr_("remote.password"), m_password);
    layout->addLayout(form);

    // Says plainly how it will authenticate, so nobody waits for a password
    // field that is never going to appear. Set apart from the fields, because
    // it is something to read once rather than something to fill in.
    auto *note = new QFrame(this);
    note->setProperty("jtfNoteBox", true);
    auto *noteLayout = new QVBoxLayout(note);
    noteLayout->setContentsMargins(12, 10, 12, 10);
    noteLayout->setSpacing(8);
    auto *how = new QLabel(tr_("remote.auth_note"), note);
    how->setWordWrap(true);
    how->setProperty("jtfFactLabel", true);
    noteLayout->addWidget(how);
    m_trust = new QCheckBox(tr_("remote.trust_unknown"), note);
    m_trust->setToolTip(tr_("remote.trust_unknown_hint"));
    noteLayout->addWidget(m_trust);
    layout->addSpacing(4);
    layout->addWidget(note);
    layout->addStretch(1);

    auto *buttons =
        new QDialogButtonBox(QDialogButtonBox::Ok | QDialogButtonBox::Cancel, this);
    connect(buttons, &QDialogButtonBox::accepted, this, &QDialog::accept);
    connect(buttons, &QDialogButtonBox::rejected, this, &QDialog::reject);
    dialogs::localizeButtons(
        buttons, [this](const char *key) { return tr_(key); },
        palette().color(QPalette::Text));
    if (QAbstractButton *ok = buttons->button(QDialogButtonBox::Ok)) {
        ok->setText(tr_("remote.connect"));
        ok->setIcon(glyph::make(glyph::Shape::NewWindow, palette().color(QPalette::Text)));
        // Nothing to connect to until a host is typed.
        ok->setEnabled(false);
        connect(m_host, &QLineEdit::textChanged, this,
                [ok](const QString &text) { ok->setEnabled(!text.trimmed().isEmpty()); });
    }
    layout->addWidget(buttons);

    m_host->setFocus();
}

QString RemoteDialog::tr_(const char *key) const {
    return jtfText([&](char *buf, int len) { return jtf_tr(m_app, key, buf, len); });
}

QString RemoteDialog::host() const { return m_host->text().trimmed(); }

int RemoteDialog::port() const { return m_port->value(); }

QString RemoteDialog::user() const {
    const QString typed = m_user->text().trimmed();
    // Empty means "the same account as here", which is what `ssh host` does.
    return typed.isEmpty() ? qEnvironmentVariable("USER") : typed;
}

QString RemoteDialog::path() const {
    const QString typed = m_path->text().trimmed();
    return typed.isEmpty() ? QStringLiteral("/") : typed;
}

QString RemoteDialog::password() const { return m_password->text(); }

bool RemoteDialog::trustUnknownHost() const { return m_trust->isChecked(); }
