#include "imagewriterdialog.h"

#include "devicedelegate.h"
#include "dialogbuttons.h"
#include "icons.h"
#include "jtfstring.h"
#include "panewidget.h"
#include "theme.h"

#include <QCheckBox>
#include <QDialogButtonBox>
#include <QFileInfo>
#include <QHBoxLayout>
#include <QLabel>
#include <QListWidget>
#include <QMessageBox>
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

    // Asked of the palette the dialog has already inherited rather than of the
    // system, so this cannot disagree with the window it opened from - which is
    // what would happen while the theme is set to light on a dark desktop.
    const bool dark = palette().color(QPalette::Window).lightness() < 128;
    const Theme theme = Theme::fromApp(m_app, dark);

    auto *layout = new QVBoxLayout(this);
    layout->setContentsMargins(18, 16, 18, 14);
    layout->setSpacing(10);

    // What is being written. Named in full: an image chosen from a file list
    // an hour ago is not necessarily the one the person now has in mind.
    m_source = new QLabel(this);
    m_source->setTextFormat(Qt::PlainText);
    m_source->setWordWrap(true);
    m_source->setText(QStringLiteral("%1  ·  %2")
                          .arg(QFileInfo(m_image).fileName(), sizeText(m_imageSize)));
    auto *sourceCaption = new QLabel(tr_("imaging.source"), this);
    sourceCaption->setStyleSheet(
        QStringLiteral("color: %1; font-size: 12px;").arg(theme.textSecondary.name()));
    layout->addWidget(sourceCaption);
    layout->addWidget(m_source);

    auto *targetCaption = new QLabel(tr_("imaging.target"), this);
    targetCaption->setStyleSheet(
        QStringLiteral("color: %1; font-size: 12px;").arg(theme.textSecondary.name()));
    layout->addSpacing(4);
    layout->addWidget(targetCaption);

    m_devices = new QListWidget(this);
    // No current row, so there is nothing to confirm by reflex. The Write
    // button is disabled until a disk is deliberately chosen.
    m_devices->setSelectionMode(QAbstractItemView::SingleSelection);
    m_devices->setUniformItemSizes(false);
    m_devices->setSpacing(2);
    auto *rows = new DeviceDelegate(m_devices);
    rows->setColours(theme.textSecondary, theme.border, theme.error);
    m_devices->setItemDelegate(rows);
    layout->addWidget(m_devices, 1);

    m_verify = new QCheckBox(tr_("imaging.verify"), this);
    // On by default. A write that was not checked has not been shown to have
    // worked, and the disks this is used with are exactly the ones that fail
    // silently.
    m_verify->setChecked(true);
    layout->addWidget(m_verify);

    // The sentence someone has to disagree with, and the only one in the
    // dialog that is styled to be hard to skip. It sits in its own tinted band
    // rather than being another grey line under the checkbox, where it read as
    // a caption.
    m_warning = new QLabel(this);
    m_warning->setWordWrap(true);
    m_warning->setTextFormat(Qt::PlainText);
    m_warning->setStyleSheet(QStringLiteral("color: %1; background: %2; border-left: 3px solid %1;"
                                            "border-radius: 4px; padding: 8px 10px;")
                                 .arg(theme.error.name(), theme.rowHover.name()));
    m_warning->setVisible(false);
    layout->addWidget(m_warning);

    m_stage = new QLabel(this);
    m_stage->setTextFormat(Qt::PlainText);
    m_stage->setWordWrap(true);
    m_stage->setStyleSheet(QStringLiteral("color: %1;").arg(theme.textSecondary.name()));
    layout->addWidget(m_stage);

    m_progress = new QProgressBar(this);
    m_progress->setVisible(false);
    m_progress->setTextVisible(true);
    // Beside the bar rather than under it. A proportion answers "how far",
    // and the two questions a person actually has while watching a disk being
    // written are "how long has this been going" and "is it moving at all" -
    // a bar that has not visibly moved for thirty seconds looks identical to
    // one that has stopped.
    m_rate = new QLabel(this);
    m_rate->setTextFormat(Qt::PlainText);
    m_rate->setVisible(false);
    m_rate->setStyleSheet(QStringLiteral("color: %1;").arg(theme.textSecondary.name()));
    // Its width must not push the bar around as the numbers change.
    m_rate->setSizePolicy(QSizePolicy::Ignored, QSizePolicy::Preferred);
    auto *progressRow = new QHBoxLayout;
    progressRow->setContentsMargins(0, 0, 0, 0);
    progressRow->setSpacing(10);
    progressRow->addWidget(m_progress, 1);
    progressRow->addWidget(m_rate, 0);
    layout->addLayout(progressRow);
    layout->addSpacing(2);

    auto *buttons = new QDialogButtonBox(this);
    m_refresh = buttons->addButton(tr_("command.view.refresh"), QDialogButtonBox::ResetRole);
    m_write = buttons->addButton(tr_("imaging.confirm"), QDialogButtonBox::AcceptRole);
    m_cancel = buttons->addButton(tr_("imaging.cancel"), QDialogButtonBox::DestructiveRole);
    m_cancel->setVisible(false);
    m_close = buttons->addButton(QDialogButtonBox::Close);
    dialogs::localizeButtons(buttons, [this](const char *key) { return tr_(key); },
                             palette().color(QPalette::WindowText));
    // localizeButtons only knows Qt's standard buttons. These two were added
    // by name, so they are the only ones in the row that would have come up
    // without a picture while every other button in the program has one.
    const QColor iconColour = palette().color(QPalette::WindowText);
    m_refresh->setIcon(glyph::make(glyph::Shape::Reload, iconColour));
    m_write->setIcon(glyph::forCommand(QStringLiteral("file.write_image"), iconColour));
    m_cancel->setIcon(glyph::make(glyph::Shape::Close, iconColour));
    layout->addWidget(buttons);

    connect(m_refresh, &QPushButton::clicked, this, &ImageWriterDialog::reloadDevices);
    connect(m_write, &QPushButton::clicked, this, &ImageWriterDialog::startWrite);
    connect(m_cancel, &QPushButton::clicked, this, [this] {
        if (!m_running || m_cancelling) {
            return;
        }
        // Stopping leaves the disk partly written. The pump reports that as a
        // failure with the wording that says so, rather than the dialog
        // closing as though nothing had happened.
        m_cancelling = true;
        jtf_write_cancel(m_app);
        updateAffordances();
    });
    connect(m_close, &QPushButton::clicked, this, [this] {
        if (m_running) {
            // Cannot happen - Close is disabled while a write is running -
            // but a window that could be closed mid-write by a stray Escape
            // would leave a half-written disk and no report of it.
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

        // The second line: how big, how attached, and what is on it right now -
        // the last of which is how two sticks of the same model are told apart.
        QStringList detail;
        detail << sizeText(size);
        if (!busKey.isEmpty()) {
            detail << tr_(busKey.toUtf8().constData());
        }
        if (!volumes.isEmpty()) {
            detail << volumes;
        }

        QString why;
        const bool writable = refusal.isEmpty();
        if (!writable) {
            why = tr_(refusal.toUtf8().constData());
            why = jtfFill(why, "needed", sizeText(m_imageSize));
            why = jtfFill(why, "available", sizeText(size));
        }

        auto *row = new QListWidgetItem(m_devices);
        row->setData(DeviceDelegate::ModelRole, name);
        row->setData(DeviceDelegate::DetailRole, detail.join(QStringLiteral(" · ")));
        row->setData(DeviceDelegate::NodeRole, node);
        row->setData(DeviceDelegate::RefusalRole, why);
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
    m_devices->setEnabled(!m_running);
    // Close is not available while a disk is being written. It would leave
    // the disk half written with the window gone and nothing to say so; the
    // way out of a running write is Stop, which says what it does and reports
    // the state the disk is left in.
    m_close->setEnabled(!m_running);
    m_cancel->setVisible(m_running);
    m_cancel->setEnabled(m_running && !m_cancelling);
    m_cancel->setText(m_cancelling ? tr_("imaging.cancelling") : tr_("imaging.cancel"));

    if (!chosen) {
        m_warning->clear();
        m_warning->setVisible(false);
        return;
    }
    m_warning->setVisible(true);
    const int index = row->data(kDeviceIndexRole).toInt();
    const QString name =
        jtfText([&](char *b, int l) { return jtf_device_name(m_app, index, b, l); });
    // The disk is named in the sentence. "This will erase the selected disk"
    // asks someone to agree to something the sentence does not contain.
    m_warning->setText(jtfFill(tr_("imaging.warning"), "device", name));
}

bool ImageWriterDialog::confirmTwice(const QListWidgetItem *row) {
    // Twice, and both times with the disk written out in full.
    //
    // One dialog is one reflex. The disk is named in both because the thing
    // being agreed to is *which disk*, and a second question that only says
    // "are you sure" adds a click without adding a decision - the person
    // reads the same words they have already dismissed once. So the first
    // asks whether to write to this disk, and the second puts the disk, its
    // size, its node and the image side by side one last time.
    const QString device = row->data(DeviceDelegate::ModelRole).toString();
    const QString detail = row->data(DeviceDelegate::DetailRole).toString();
    const QString node = row->data(DeviceDelegate::NodeRole).toString();
    const QString image = QFileInfo(m_image).fileName();
    const QColor ink = palette().color(QPalette::Text);

    const auto ask = [&](const char *titleKey, const char *questionKey, const char *detailKey) {
        const auto fill = [&](const char *key) {
            QString text = tr_(key);
            text = jtfFill(text, "device", device);
            text = jtfFill(text, "detail", detail);
            text = jtfFill(text, "node", node);
            text = jtfFill(text, "image", image);
            return text;
        };

        QMessageBox box(this);
        box.setIconPixmap(
            glyph::forCommand(QStringLiteral("file.write_image"), ink).pixmap(48, 48));
        box.setWindowTitle(tr_(titleKey));
        // The question is the heading and the disk is underneath it. Both in
        // one block gave a paragraph with no shape, where the sentence that
        // has to be read and the disk that has to be checked carried the same
        // weight - so neither was read.
        box.setText(fill(questionKey));
        box.setInformativeText(fill(detailKey));

        // Cancel is the default on both. A dialog answered by pressing Return
        // without reading is answered "no" here.
        QPushButton *go =
            box.addButton(tr_("imaging.confirm_write_now"), QMessageBox::DestructiveRole);
        QPushButton *stop =
            box.addButton(tr_("imaging.confirm_cancel"), QMessageBox::RejectRole);
        // Every other button in the program carries a picture; these two came
        // up bare.
        go->setIcon(glyph::forCommand(QStringLiteral("file.write_image"), ink));
        stop->setIcon(glyph::make(glyph::Shape::Close, ink));
        box.setDefaultButton(stop);
        box.setEscapeButton(stop);
        box.exec();
        return box.clickedButton() == go;
    };

    return ask("imaging.confirm_title", "imaging.confirm_first", "imaging.confirm_first_detail")
           && ask("imaging.confirm_again_title", "imaging.confirm_again",
                  "imaging.confirm_again_detail");
}

void ImageWriterDialog::startWrite() {
    auto *row = m_devices->currentItem();
    if (row == nullptr || !row->data(kWritableRole).toBool()) {
        return;
    }
    if (!confirmTwice(row)) {
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
    m_cancelling = false;
    m_since.start();
    m_progress->setVisible(true);
    m_progress->setRange(0, 0);
    m_devices->setEnabled(false);
    updateAffordances();
    m_pump->start();
}

void ImageWriterDialog::pump() {
    jtf_pump_write(m_app);
    updateRate();

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
        m_cancelling = false;
        // How long it took stays on screen. It is the one number nobody can
        // recover afterwards, and it is what tells you whether the next one
        // is worth starting now or later.
        m_rate->setVisible(true);
        showOutcome();
    }
}

void ImageWriterDialog::updateRate() {
    if (!m_running) {
        m_rate->setVisible(false);
        return;
    }
    const qint64 ms = m_since.elapsed();
    const qint64 seconds = ms / 1000;
    // Minutes and seconds rather than a count of seconds: 214 has to be
    // divided in the reader's head to mean anything.
    const QString elapsed = seconds >= 3600
                                ? QStringLiteral("%1:%2:%3")
                                      .arg(seconds / 3600)
                                      .arg((seconds / 60) % 60, 2, 10, QLatin1Char('0'))
                                      .arg(seconds % 60, 2, 10, QLatin1Char('0'))
                                : QStringLiteral("%1:%2")
                                      .arg(seconds / 60)
                                      .arg(seconds % 60, 2, 10, QLatin1Char('0'));

    const quint64 done = jtf_write_progress(m_app, 0);
    QString rate;
    // A rate computed over the first fraction of a second is noise, and a
    // number that swings wildly reads as a fault in the program rather than
    // in the arithmetic.
    if (ms > 1500 && done > 0) {
        const quint64 perSecond = static_cast<quint64>(static_cast<double>(done) * 1000.0
                                                       / static_cast<double>(ms));
        rate = jtfFill(tr_("imaging.rate"), "rate", sizeText(perSecond));
    } else {
        rate = tr_("imaging.rate_unknown");
    }

    m_rate->setVisible(true);
    m_rate->setText(QStringLiteral("%1   %2")
                        .arg(jtfFill(tr_("imaging.elapsed"), "time", elapsed), rate));
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
