#include "aboutdialog.h"

#include "dialogbuttons.h"
#include "icons.h"
#include "jtfstring.h"

#include <QApplication>
#include <QDialogButtonBox>
#include <QGuiApplication>
#include <QHBoxLayout>
#include <QLabel>
#include <QVBoxLayout>

AboutDialog::AboutDialog(JtfApp *app, QWidget *parent) : QDialog(parent), m_app(app) {
    setWindowTitle(tr_("about.title"));
    setMinimumWidth(440);

    auto *layout = new QVBoxLayout(this);
    layout->setContentsMargins(20, 18, 20, 14);
    layout->setSpacing(12);

    // The icon and the name, the way every About box opens.
    auto *header = new QHBoxLayout;
    header->setSpacing(14);
    auto *icon = new QLabel(this);
    icon->setPixmap(QApplication::windowIcon().pixmap(64, 64));
    icon->setFixedSize(64, 64);
    header->addWidget(icon, 0, Qt::AlignTop);

    auto *titles = new QVBoxLayout;
    titles->setSpacing(2);
    auto *name = new QLabel(tr_("app.name"), this);
    QFont bold = name->font();
    bold.setPointSizeF(bold.pointSizeF() * 1.5);
    bold.setBold(true);
    name->setFont(bold);
    titles->addWidget(name);

    const QString version =
        jtfText([&](char *buf, int len) { return jtf_app_version(buf, len); });
    auto *versionLabel = new QLabel(jtfFill(tr_("about.version"), "version", version), this);
    versionLabel->setProperty("jtfFactLabel", true);
    titles->addWidget(versionLabel);

    // The Qt build, because "which Qt" is the second question every Qt bug
    // report is asked and the user cannot answer it from anywhere else.
    auto *qt = new QLabel(jtfFill(tr_("about.qt"), "version",
                                  QString::fromLatin1(qVersion())),
                          this);
    qt->setProperty("jtfFactLabel", true);
    titles->addWidget(qt);
    header->addLayout(titles, 1);
    layout->addLayout(header);

    auto *rule = new QFrame(this);
    rule->setFrameShape(QFrame::HLine);
    rule->setProperty("jtfRule", true);
    layout->addWidget(rule);

    auto *description = new QLabel(tr_("about.description"), this);
    description->setWordWrap(true);
    layout->addWidget(description);

    // Licence and source, as links rather than as prose: they are things to
    // go and read, not things to be told about.
    auto *licence = new QLabel(tr_("about.licence"), this);
    licence->setWordWrap(true);
    licence->setOpenExternalLinks(true);
    licence->setTextFormat(Qt::RichText);
    layout->addWidget(licence);

    auto *source = new QLabel(tr_("about.source"), this);
    source->setOpenExternalLinks(true);
    source->setTextFormat(Qt::RichText);
    layout->addWidget(source);

    auto *author = new QLabel(tr_("about.author"), this);
    author->setWordWrap(true);
    layout->addWidget(author);

    auto *credit = new QLabel(tr_("about.credit"), this);
    credit->setWordWrap(true);
    credit->setProperty("jtfFactLabel", true);
    layout->addWidget(credit);

    auto *buttons = new QDialogButtonBox(QDialogButtonBox::Close, this);
    connect(buttons, &QDialogButtonBox::rejected, this, &QDialog::reject);
    dialogs::localizeButtons(
        buttons, [this](const char *key) { return tr_(key); }, palette().color(QPalette::Text));
    layout->addWidget(buttons);
}

QString AboutDialog::tr_(const char *key) const {
    return jtfText([&](char *buf, int len) { return jtf_tr(m_app, key, buf, len); });
}
