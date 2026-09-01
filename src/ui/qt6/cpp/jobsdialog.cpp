#include "jobsdialog.h"

#include "dialogbuttons.h"
#include "icons.h"
#include "jtfstring.h"

#include <QDialogButtonBox>
#include <QHBoxLayout>
#include <QHeaderView>
#include <QLabel>
#include <QProgressBar>
#include <QPushButton>
#include <QTimer>
#include <QTreeWidget>
#include <QVBoxLayout>

namespace {
// Often enough that a running copy looks alive, rarely enough that the window
// costs nothing while it sits open.
constexpr int kPollMs = 250;

QString formatSize(quint64 bytes) {
    static const char *const units[] = {"B", "KB", "MB", "GB", "TB"};
    double value = static_cast<double>(bytes);
    int unit = 0;
    while (value >= 1024.0 && unit < 4) {
        value /= 1024.0;
        ++unit;
    }
    return unit == 0 ? QStringLiteral("%1 B").arg(bytes)
                     : QStringLiteral("%1 %2")
                           .arg(value, 0, 'f', 1)
                           .arg(QLatin1String(units[unit]));
}
} // namespace

JobsDialog::JobsDialog(JtfApp *app, QWidget *parent) : QDialog(parent), m_app(app) {
    setWindowTitle(tr_("jobs.title"));
    resize(560, 320);

    auto *layout = new QVBoxLayout(this);
    layout->setContentsMargins(14, 12, 14, 12);
    layout->setSpacing(10);

    m_list = new QTreeWidget(this);
    m_list->setObjectName(QStringLiteral("JtfJobs"));
    m_list->setRootIsDecorated(false);
    m_list->setAlternatingRowColors(true);
    m_list->setColumnCount(3);
    m_list->setHeaderLabels({tr_("jobs.column.what"), tr_("jobs.column.size"),
                             tr_("jobs.column.state")});
    m_list->header()->setStretchLastSection(false);
    m_list->header()->setSectionResizeMode(0, QHeaderView::Stretch);
    layout->addWidget(m_list, 1);

    auto *row = new QHBoxLayout;
    row->setSpacing(8);
    const QColor iconColour = palette().color(QPalette::Text);
    m_cancel = new QPushButton(glyph::make(glyph::Shape::Close, iconColour),
                               tr_("jobs.cancel"), this);
    m_clear = new QPushButton(glyph::make(glyph::Shape::Close, iconColour),
                              tr_("jobs.clear_queue"), this);
    connect(m_cancel, &QPushButton::clicked, this, [this] {
        const int index = m_list->indexOfTopLevelItem(m_list->currentItem());
        if (index >= 0) {
            jtf_job_cancel(m_app, index);
            refresh();
        }
    });
    connect(m_clear, &QPushButton::clicked, this, [this] {
        jtf_op_clear_queue(m_app);
        refresh();
    });
    // Neither of these is the default action: Return in this window should
    // close it, not cancel whatever row happens to be selected. Qt makes the
    // first button the default unless told otherwise, and a blue "cancel the
    // selected job" is an invitation to press Return without reading.
    for (QPushButton *button : {m_cancel, m_clear}) {
        button->setAutoDefault(false);
        button->setDefault(false);
    }
    row->addWidget(m_cancel);
    row->addWidget(m_clear);
    row->addStretch(1);
    auto *buttons = new QDialogButtonBox(QDialogButtonBox::Close, this);
    connect(buttons, &QDialogButtonBox::rejected, this, &QDialog::accept);
    dialogs::localizeButtons(
        buttons, [this](const char *key) { return tr_(key); }, iconColour);
    if (QPushButton *close = buttons->button(QDialogButtonBox::Close)) {
        close->setDefault(true);
    }
    row->addWidget(buttons);
    layout->addLayout(row);

    m_poll = new QTimer(this);
    m_poll->setInterval(kPollMs);
    connect(m_poll, &QTimer::timeout, this, &JobsDialog::refresh);
    m_poll->start();
    refresh();
}

QString JobsDialog::tr_(const char *key) const {
    return jtfText([&](char *buf, int len) { return jtf_tr(m_app, key, buf, len); });
}

void JobsDialog::refresh() {
    const int count = jtf_job_count(m_app);
    // Rebuilt only when the number of jobs changes; otherwise the rows are
    // updated in place, so the selection survives and a job the user is
    // pointing at does not move out from under them.
    if (m_list->topLevelItemCount() != count) {
        m_list->clear();
        for (int i = 0; i < count; ++i) {
            m_list->addTopLevelItem(new QTreeWidgetItem);
        }
    }

    for (int i = 0; i < count; ++i) {
        QTreeWidgetItem *item = m_list->topLevelItem(i);
        const QByteArray key =
            jtfText([&](char *b, int l) { return jtf_job_label_key(m_app, i, b, l); }).toUtf8();
        item->setText(0, jtfText([&](char *b, int l) {
                          return jtf_tr(m_app, key.constData(), b, l);
                      }));
        const bool running = jtf_job_is_running(m_app, i) != 0;
        if (running) {
            const int percent = jtf_op_percent(m_app);
            item->setText(1, jtfText([&](char *b, int l) { return jtf_op_current(m_app, b, l); }));
            item->setText(2, percent >= 0
                                 ? QStringLiteral("%1%").arg(percent)
                                 : tr_("jobs.state.running"));
        } else {
            item->setText(1, formatSize(jtf_job_bytes(m_app, i)));
            item->setText(2, tr_("jobs.state.queued"));
        }
    }

    if (count == 0 && m_list->topLevelItemCount() == 0) {
        auto *empty = new QTreeWidgetItem(m_list);
        empty->setText(0, tr_("jobs.none"));
        empty->setFlags(Qt::NoItemFlags);
    }
    m_cancel->setEnabled(count > 0 && m_list->currentItem() != nullptr);
    m_clear->setEnabled(jtf_op_queued(m_app) > 0);
}
