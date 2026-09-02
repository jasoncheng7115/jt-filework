#include "imagewriterdialog.h"

#include "dialogbuttons.h"
#include "icons.h"
#include "jtfstring.h"
#include "panewidget.h"

#include <QCheckBox>
#include <QDialogButtonBox>
#include <QFileInfo>
#include <QHBoxLayout>
#include <QLabel>
#include <QListWidget>
#include <QProgressBar>
#include <QPushButton>
#include <QTimer>
#include <QVBoxLayout>

namespace {

// How often the UI asks the writing thread what it has done. Fast enough that
// the bar moves smoothly, slow enough that a poll is not the reason the write
// is slow.
constexpr int kPumpMs = 120;

// The index a row carries, so the list can be re-sorted or filtered later
// without the selection meaning the wrong disk.
constexpr int kDeviceIndexRole = Qt::UserRole + 1;
// Whether the row may be written to at all.
constexpr int kWritableRole = Qt::UserRole + 2;

} // namespace

ImageWriterDialog::ImageWriterDialog(JtfApp *app, const QString &image, QWidget *parent)
    : QDialog(parent), m_app(app), m_image(image) {
    setWindowTitle(tr_("imaging.title"));
    setModal(true);
    resize(560, 460);

    m_imageSize = static_cast<quint64>(QFileInfo(m_image).size());

    auto *layout = new QVBoxLayout(this);

    // What is being written. Named in full: an image chosen from a file list
    // an hour ago is not necessarily the one the person now has in mind.
    m_source = new QLabel(this);
    m_source->setTextFormat(Qt::PlainText);
    m_source->setWordWrap(true);
    m_source->setText(QStringLiteral("%1: %2 (%3)")
                          .arg(tr_("imaging.source"), QFileInfo(m_image).fileName(),
                               sizeText(m_imageSize)));
    layout->addWidget(m_source);

    layout->addWidget(new QLabel(tr_("imaging.target"), this));
    m_devices = new QListWidget(this);
    // No current row, so there is nothing to confirm by reflex. The Write
    // button is disabled until a disk is deliberately chosen.
    m_devices->setSelectionMode(QAbstractItemView::SingleSelection);
    layout->addWidget(m_devices, 1);

    m_verify = new QCheckBox(tr_("imaging.verify"), this);
    // On by default. A write that was not checked has not been shown to have
    // worked, and the disks this is used with are exactly the ones that fail
    // silently.
    m_verify->setChecked(true);
    layout->addWidget(m_verify);

    m_warning = new QLabel(this);
    m_warning->setWordWrap(true);
    m_warning->setTextFormat(Qt::PlainText);
    layout->addWidget(m_warning);

    m_stage = new QLabel(this);
    m_stage->setTextFormat(Qt::PlainText);
    m_stage->setWordWrap(true);
    layout->addWidget(m_stage);

    m_progress = new QProgressBar(this);
    m_progress->setVisible(false);
    layout->addWidget(m_progress);

    auto *buttons = new QDialogButtonBox(this);
    m_refresh = buttons->addButton(tr_("command.view.refresh"), QDialogButtonBox::ResetRole);
    m_write = buttons->addButton(tr_("imaging.confirm"), QDialogButtonBox::AcceptRole);
    m_close = buttons->addButton(QDialogButtonBox::Close);
    dialogs::localizeButtons(buttons, [this](const char *key) { return tr_(key); },
                             palette().color(QPalette::WindowText));
    layout->addWidget(buttons);

    connect(m_refresh, &QPushButton::clicked, this, &ImageWriterDialog::reloadDevices);
    connect(m_write, &QPushButton::clicked, this, &ImageWriterDialog::startWrite);
    connect(m_close, &QPushButton::clicked, this, [this] {
        if (m_running) {
            // Cancelling leaves the disk partly written. The pump reports that
            // as a failure with the wording that says so, rather than the
            // dialog closing as though nothing had happened.
            jtf_write_cancel(m_app);
            return;
        }
        jtf_write_close(m_app);
        reject();
    });
    connect(m_devices, &QListWidget::currentRowChanged, this,
            &ImageWriterDialog::updateAffordances);

    m_pump = new QTimer(this);
    m_pump->setInterval(kPumpMs);
    connect(m_pump, &QTimer::timeout, this, &ImageWriterDialog::pump);

    reloadDevices();
}

QString ImageWriterDialog::tr_(const char *key) const {
    return jtfText([&](char *buf, int len) { return jtf_tr(m_app, key, buf, len); });
}

QString ImageWriterDialog::sizeText(quint64 bytes) const {
    return PaneWidget::formatSize(bytes);
}

void ImageWriterDialog::reloadDevices() {
    m_devices->clear();

    if (jtf_write_is_supported() == 0) {
        auto *row = new QListWidgetItem(tr_("imaging.unsupported"), m_devices);
        row->setFlags(Qt::NoItemFlags);
        updateAffordances();
        return;
    }

    const int count = jtf_devices_refresh(m_app);
    if (count <= 0) {
        // Not an error, and worded so it does not read as one: most machines
        // have nothing removable plugged in. The line also says what the list
        // contains, so a disk that is missing is explained.
        auto *row = new QListWidgetItem(tr_("imaging.no_devices"), m_devices);
        row->setFlags(Qt::NoItemFlags);
        updateAffordances();
        return;
    }

    const QByteArray image = m_image.toUtf8();
    for (int i = 0; i < count; ++i) {
        const QString name =
            jtfText([&](char *b, int l) { return jtf_device_name(m_app, i, b, l); });
        const QString node =
            jtfText([&](char *b, int l) { return jtf_device_node(m_app, i, b, l); });
        const QString busKey =
            jtfText([&](char *b, int l) { return jtf_device_bus_key(m_app, i, b, l); });
        const QString volumes =
            jtfText([&](char *b, int l) { return jtf_device_volumes(m_app, i, b, l); });
        const QString refusal = jtfText([&](char *b, int l) {
            return jtf_device_refusal_key(m_app, i, image.constData(), b, l);
        });
        const quint64 size = jtf_device_size(m_app, i);

        QString text = QStringLiteral("%1 — %2").arg(name, sizeText(size));
        if (!busKey.isEmpty()) {
            text += QStringLiteral(" (%1)").arg(tr_(busKey.toUtf8().constData()));
        }
        // What is on it right now, which is how two identical sticks are told
        // apart.
        if (!volumes.isEmpty()) {
            text += QStringLiteral("\n%1").arg(volumes);
        }
        const bool writable = refusal.isEmpty();
        if (!writable) {
            QString why = tr_(refusal.toUtf8().constData());
            why = jtfFill(why, "needed", sizeText(m_imageSize));
            why = jtfFill(why, "available", sizeText(size));
            text += QStringLiteral("\n%1").arg(why);
        }
        text += QStringLiteral("\n%1").arg(node);

        auto *row = new QListWidgetItem(text, m_devices);
        row->setData(kDeviceIndexRole, i);
        row->setData(kWritableRole, writable);
        row->setIcon(glyph::forCommand(QStringLiteral("file.write_image"),
                                       palette().color(QPalette::WindowText)));
        if (!writable) {
            // Listed but not choosable. Hiding it would leave someone hunting
            // for a disk that is plugged in and visible in every other program.
            row->setFlags(row->flags() & ~Qt::ItemIsSelectable & ~Qt::ItemIsEnabled);
        }
    }
    m_devices->setCurrentRow(-1);
    updateAffordances();
}

void ImageWriterDialog::updateAffordances() {
    auto *row = m_devices->currentItem();
    const bool chosen = row != nullptr && row->data(kWritableRole).toBool();
    const bool ready = chosen && !m_running && m_imageSize > 0;
    m_write->setEnabled(ready);
    // Not merely disabled: not the default button either. An AcceptRole button
    // is painted as the highlighted one it is safe to press, and a disabled
    // button painted that way is an invitation to click the most destructive
    // control in the program and be told nothing. It becomes the default only
    // once a disk has actually been chosen.
    m_write->setDefault(ready);
    m_write->setAutoDefault(ready);
    m_refresh->setEnabled(!m_running);
    m_verify->setEnabled(!m_running);

    if (!chosen) {
        m_warning->clear();
        return;
    }
    const int index = row->data(kDeviceIndexRole).toInt();
    const QString name =
        jtfText([&](char *b, int l) { return jtf_device_name(m_app, index, b, l); });
    // The disk is named in the sentence. "This will erase the selected disk"
    // asks someone to agree to something the sentence does not contain.
    m_warning->setText(jtfFill(tr_("imaging.warning"), "device", name));
}

void ImageWriterDialog::startWrite() {
    auto *row = m_devices->currentItem();
    if (row == nullptr || !row->data(kWritableRole).toBool()) {
        return;
    }
    if (jtf_write_needs_elevation() != 0) {
        // Said before anything is opened, so the prompt that follows is
        // expected rather than alarming.
        m_stage->setText(tr_("imaging.needs_elevation"));
    }
    const int index = row->data(kDeviceIndexRole).toInt();
    const QByteArray image = m_image.toUtf8();
    if (jtf_write_start(m_app, index, image.constData(), m_verify->isChecked() ? 1 : 0) == 0) {
        // The plan was refused on the values as they are now rather than as
        // they were when the list was drawn - the disk may have been unplugged
        // in between. Re-reading the list explains it.
        reloadDevices();
        return;
    }
    m_running = true;
    m_progress->setVisible(true);
    m_progress->setRange(0, 0);
    m_devices->setEnabled(false);
    updateAffordances();
    m_pump->start();
}

void ImageWriterDialog::pump() {
    jtf_pump_write(m_app);

    const QString stageKey =
        jtfText([&](char *b, int l) { return jtf_write_stage_key(m_app, b, l); });
    if (!stageKey.isEmpty()) {
        m_stage->setText(tr_(stageKey.toUtf8().constData()));
    }

    const quint64 done = jtf_write_progress(m_app, 0);
    const quint64 total = jtf_write_progress(m_app, 1);
    if (total > 0) {
        // Scaled to a range Qt can hold: a progress bar takes an int, and a
        // 4 GB image in bytes does not fit in one.
        constexpr int kSteps = 1000;
        m_progress->setRange(0, kSteps);
        m_progress->setValue(static_cast<int>(done * kSteps / total));
        m_progress->setFormat(QStringLiteral("%1 / %2").arg(sizeText(done), sizeText(total)));
    } else {
        m_progress->setRange(0, 0);
        m_progress->setFormat(QString());
    }

    if (jtf_write_is_done(m_app) != 0) {
        m_pump->stop();
        m_running = false;
        showOutcome();
    }
}

void ImageWriterDialog::showOutcome() {
    const QString key =
        jtfText([&](char *b, int l) { return jtf_write_outcome_key(m_app, b, l); });
    QString message = key.isEmpty() ? QString() : tr_(key.toUtf8().constData());
    message = jtfFill(message, "bytes", sizeText(jtf_write_bytes(m_app)));
    // Hexadecimal and upper case, which is how every published checksum is
    // written, so the two can be compared by eye.
    message = jtfFill(message, "checksum",
                      QStringLiteral("%1").arg(jtf_write_checksum(m_app), 8, 16, QLatin1Char('0'))
                          .toUpper());
    m_stage->setText(message);

    m_progress->setRange(0, 1);
    m_progress->setValue(1);
    m_progress->setFormat(QString());
    m_devices->setEnabled(true);
    updateAffordances();
    // Deliberately does not close itself. The checksum and the outcome are the
    // reason the dialog was opened, and a dialog that vanishes on completion
    // takes them with it.
}
