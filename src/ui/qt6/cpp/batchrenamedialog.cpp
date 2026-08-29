#include "batchrenamedialog.h"
#include "jtfstring.h"

#include <QCheckBox>
#include <QDialogButtonBox>
#include <QFormLayout>
#include <QHeaderView>
#include <QLabel>
#include <QLineEdit>
#include <QPushButton>
#include <QSpinBox>
#include <QTableWidget>
#include <QVBoxLayout>

QString BatchRenameDialog::tr_(const char *key) const {
    return jtfText([&](char *buf, int len) { return jtf_tr(m_app, key, buf, len); });
}

QString BatchRenameDialog::trKey(const QString &key) const {
    if (key.isEmpty()) {
        return {};
    }
    const QByteArray utf8 = key.toUtf8();
    return jtfText([&](char *buf, int len) { return jtf_tr(m_app, utf8.constData(), buf, len); });
}

BatchRenameDialog::BatchRenameDialog(JtfApp *app, int paneId, QWidget *parent)
    : QDialog(parent), m_app(app), m_pane(paneId) {
    setWindowTitle(tr_("batch.title"));
    resize(720, 520);

    auto *layout = new QVBoxLayout(this);
    auto *form = new QFormLayout;

    m_template = new QLineEdit(QStringLiteral("{name}.{ext}"), this);
    m_find = new QLineEdit(this);
    m_replace = new QLineEdit(this);
    m_regex = new QCheckBox(tr_("batch.regex"), this);
    m_start = new QSpinBox(this);
    m_start->setRange(0, 1'000'000);
    m_start->setValue(1);

    form->addRow(tr_("batch.template"), m_template);
    auto *hint = new QLabel(tr_("batch.template_hint"), this);
    hint->setWordWrap(true);
    form->addRow(QString(), hint);
    form->addRow(tr_("batch.find"), m_find);
    form->addRow(tr_("batch.replace"), m_replace);
    form->addRow(QString(), m_regex);
    form->addRow(tr_("batch.start"), m_start);
    layout->addLayout(form);

    m_rows = new QTableWidget(this);
    m_rows->setColumnCount(3);
    m_rows->setHorizontalHeaderLabels(
        {tr_("batch.column.from"), tr_("batch.column.to"), tr_("batch.column.issue")});
    m_rows->verticalHeader()->setVisible(false);
    m_rows->setEditTriggers(QAbstractItemView::NoEditTriggers);
    m_rows->setSelectionBehavior(QAbstractItemView::SelectRows);
    m_rows->setAlternatingRowColors(true);
    m_rows->horizontalHeader()->setStretchLastSection(true);
    layout->addWidget(m_rows, 1);

    m_summary = new QLabel(this);
    m_summary->setWordWrap(true);
    layout->addWidget(m_summary);

    auto *buttons = new QDialogButtonBox(QDialogButtonBox::Cancel, this);
    m_apply = buttons->addButton(QString(), QDialogButtonBox::AcceptRole);
    connect(buttons, &QDialogButtonBox::rejected, this, [this] {
        jtf_batch_clear(m_app);
        reject();
    });
    connect(m_apply, &QPushButton::clicked, this, [this] {
        if (jtf_batch_apply(m_app) > 0) {
            accept();
        }
    });
    layout->addWidget(buttons);

    // Live preview: every control recomputes it, because a preview you have to
    // ask for is one people forget to look at.
    const auto update = [this] { refreshPreview(); };
    connect(m_template, &QLineEdit::textChanged, this, update);
    connect(m_find, &QLineEdit::textChanged, this, update);
    connect(m_replace, &QLineEdit::textChanged, this, update);
    connect(m_regex, &QCheckBox::toggled, this, update);
    connect(m_start, &QSpinBox::valueChanged, this, update);

    refreshPreview();
}

void BatchRenameDialog::refreshPreview() {
    const QByteArray templateUtf8 = m_template->text().toUtf8();
    const QByteArray findUtf8 = m_find->text().toUtf8();
    const QByteArray replaceUtf8 = m_replace->text().toUtf8();

    const int count = jtf_batch_preview(m_app, m_pane, templateUtf8.constData(),
                                        findUtf8.constData(), replaceUtf8.constData(),
                                        m_regex->isChecked() ? 1 : 0, m_start->value());
    m_rows->setRowCount(count);

    for (int i = 0; i < count; ++i) {
        char from[1024] = {};
        char to[1024] = {};
        char issue[64] = {};
        if (!jtf_batch_row(m_app, i, from, sizeof(from), to, sizeof(to), issue, sizeof(issue))) {
            continue;
        }
        m_rows->setItem(i, 0, new QTableWidgetItem(QString::fromUtf8(from)));
        m_rows->setItem(i, 1, new QTableWidgetItem(QString::fromUtf8(to)));
        m_rows->setItem(i, 2, new QTableWidgetItem(trKey(QString::fromUtf8(issue))));
    }
    m_rows->resizeColumnsToContents();

    int changes = 0;
    const bool canApply = jtf_batch_can_apply(m_app, &changes) != 0;
    m_apply->setEnabled(canApply);
    m_apply->setText(jtfFill(tr_("batch.apply"), "count", QString::number(changes)));
    // A blocked batch says why, rather than leaving a disabled button with no
    // explanation.
    m_summary->setText(canApply || changes == 0 ? QString() : tr_("batch.blocked"));
    if (!canApply && changes == 0) {
        m_summary->setText(tr_("batch.blocked"));
    }
}
