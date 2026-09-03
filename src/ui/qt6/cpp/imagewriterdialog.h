// Writing a disk image to a removable disk.
//
// The most destructive thing this program does. Everything on the chosen disk
// is gone, there is no undo, and there is no trash to fish it back out of - so
// the dialog is built around not letting the wrong disk be chosen rather than
// around asking nicely afterwards:
//
//   - Nothing is preselected. There is no disk under the cursor to confirm by
//     reflex, and the Write button stays disabled until a disk is picked.
//   - Only disks the core positively established are removable, external and
//     not carrying the running system appear at all. A disk that could not be
//     read does not appear. That decision is not made here; see
//     jtf-platform-devices.
//   - Every disk shows its model, its size and what is mounted from it right
//     now, because "8 GB USB disk" does not distinguish two sticks and
//     "8 GB USB disk holding TAX-2025" does.
//   - A disk that cannot take this image is still listed, greyed, with the
//     reason. A disk someone expected to see and cannot is worse than one they
//     can see and are told about.
//   - The warning names the disk. Not "this will erase the selected disk" -
//     the words the person has to disagree with have to contain the thing they
//     would be disagreeing about.
#pragma once

#include "bridge.h"

#include <QDialog>
#include <QElapsedTimer>
#include <QString>

class QCheckBox;
class QLabel;
class QListWidget;
class QListWidgetItem;
class QProgressBar;
class QPushButton;
class QTimer;

class ImageWriterDialog : public QDialog {
    Q_OBJECT

public:
    ImageWriterDialog(JtfApp *app, const QString &image, QWidget *parent);

private:
    QString tr_(const char *key) const;
    void reloadDevices();
    void updateAffordances();
    /// Ask twice, naming the disk both times. False means do not write.
    bool confirmTwice(const QListWidgetItem *row);
    void startWrite();
    void pump();
    void showOutcome();
    /// The elapsed time and the current rate, as one line.
    void updateRate();
    QString sizeText(quint64 bytes) const;

    JtfApp *m_app = nullptr;
    QString m_image;
    quint64 m_imageSize = 0;

    QListWidget *m_devices = nullptr;
    QLabel *m_source = nullptr;
    QLabel *m_warning = nullptr;
    QLabel *m_stage = nullptr;
    QCheckBox *m_verify = nullptr;
    QProgressBar *m_progress = nullptr;
    QPushButton *m_write = nullptr;
    QPushButton *m_refresh = nullptr;
    QPushButton *m_close = nullptr;
    /// Stops a write that is under way. Only ever visible while one is.
    QPushButton *m_cancel = nullptr;
    /// How long it has been going and how fast, to the right of the bar.
    QLabel *m_rate = nullptr;
    QTimer *m_pump = nullptr;
    bool m_running = false;
    /// Set once Stop has been pressed, so the label can say so and the button
    /// does not invite a second press at something already stopping.
    bool m_cancelling = false;
    /// When the write started, for the elapsed time and the rate.
    QElapsedTimer m_since;
};
