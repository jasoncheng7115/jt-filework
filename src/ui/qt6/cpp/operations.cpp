#include "operations.h"
#include "icons.h"
#include <QApplication>
#include <QAbstractButton>

#include "dialogbuttons.h"
#include "jtfstring.h"

#include <QElapsedTimer>
#include <QEventLoop>
#include <QProgressDialog>
#include <QTimer>
#include <QInputDialog>
#include <QLineEdit>
#include <QMessageBox>
#include <QPushButton>

namespace {

QString tr_(const JtfApp *app, const char *key) {
    return jtfText([&](char *buf, int len) { return jtf_tr(app, key, buf, len); });
}

QString errorMessage(const JtfApp *app) {
    const QString key =
        jtfText([&](char *buf, int len) { return jtf_op_error_key(app, buf, len); });
    if (key.isEmpty()) {
        return {};
    }
    const QByteArray utf8 = key.toUtf8();
    return jtfText([&](char *buf, int len) { return jtf_tr(app, utf8.constData(), buf, len); });
}

} // namespace

// Asks about conflicts. Returns the policy, or -1 if the user backed out.
int ops::askDropKind(JtfApp *app, QWidget *parent, int count, bool sameApplication) {
    QMessageBox box(parent);
    box.setIcon(QMessageBox::Question);
    box.setWindowTitle(tr_(app, "drop.title"));
    box.setText(jtfFill(tr_(app, "drop.question"), "count", QString::number(count)));

    // Copy first and default: it is the choice that cannot lose the original,
    // and a dialog people dismiss by reflex should do the safe thing - the
    // same rule the conflict dialog follows.
    auto *copy = box.addButton(tr_(app, "drop.copy"), QMessageBox::AcceptRole);
    auto *move = box.addButton(tr_(app, "drop.move"), QMessageBox::AcceptRole);
    auto *cancel = box.addButton(tr_(app, "drop.cancel"), QMessageBox::RejectRole);
    // Within our own window the drag started from a folder we are showing, so
    // moving is the ordinary intent and is offered as the default. From
    // another application it is somebody else's file and copying is.
    box.setDefaultButton(sameApplication ? move : copy);
    box.setEscapeButton(cancel);
    box.exec();

    if (box.clickedButton() == copy) {
        return ops::Copy;
    }
    if (box.clickedButton() == move) {
        return ops::Move;
    }
    return -1;
}

namespace {

/// The colour anything drawn into a dialog is drawn in.
QColor dialogInk(const QWidget *parent) {
    return parent != nullptr ? parent->palette().color(QPalette::Text)
                             : QApplication::palette().color(QPalette::Text);
}

/// Put our own icon on a message box, in place of the style's.
///
/// `QMessageBox::Question` and friends are drawn by the platform style in its
/// own colours: on a dark theme the standard question mark came out black on
/// near-black and could not be read at all. Ours are drawn from the palette
/// like every other glyph in the program - and a picture of the command is
/// more use than a punctuation mark, because it says what is about to happen.
void setBoxIcon(QMessageBox *box, const QIcon &icon) {
    box->setIconPixmap(icon.pixmap(48, 48));
}

/// The icon a button in one of these boxes carries.
void iconise(QAbstractButton *button, const QIcon &icon) {
    if (button != nullptr) {
        button->setIcon(icon);
    }
}

} // namespace

int ops::askConflictPolicy(JtfApp *app, QWidget *parent, int conflicts) {
    const QColor ink = dialogInk(parent);
    QMessageBox box(parent);
    setBoxIcon(&box, glyph::forCommand(QStringLiteral("file.copy_to"), ink));
    box.setWindowTitle(tr_(app, "operation.confirm_title"));
    box.setText(jtfFill(tr_(app, "operation.confirm_conflicts"), "count",
                        QString::number(conflicts)));
    box.setInformativeText(
        jtfText([&](char *buf, int len) { return jtf_op_first_conflict(app, buf, len); }));

    // Skip is first and default: it is the only choice that cannot destroy
    // anything, and a dialog people dismiss by reflex should do the safe thing.
    auto *skip = box.addButton(tr_(app, "conflict.skip"), QMessageBox::AcceptRole);
    auto *keep = box.addButton(tr_(app, "conflict.keep_both"), QMessageBox::AcceptRole);
    auto *replace = box.addButton(tr_(app, "conflict.overwrite"), QMessageBox::DestructiveRole);
    auto *cancel = box.addButton(tr_(app, "conflict.abort"), QMessageBox::RejectRole);
    iconise(skip, glyph::make(glyph::Shape::ArrowRight, ink));
    iconise(keep, glyph::make(glyph::Shape::Copy, ink));
    iconise(replace, glyph::make(glyph::Shape::Check, ink));
    iconise(cancel, glyph::make(glyph::Shape::Close, ink));
    box.setDefaultButton(skip);
    box.setEscapeButton(cancel);
    box.exec();

    if (box.clickedButton() == skip) {
        return 0;
    }
    if (box.clickedButton() == replace) {
        return 1;
    }
    if (box.clickedButton() == keep) {
        return 2;
    }
    return -1;
}

namespace {

// Moving to the trash is recoverable, and still asked about.
//
// The question is not only whether the data survives: `D` is one key away from
// `S` and `F` on this keyboard, and an operation that takes twenty files off
// the screen with no question asked is one nobody can tell from a bug. The
// wording says where they are going, so the answer is easy to give.
bool confirmTrash(JtfApp *app, QWidget *parent, int entries) {
    const QColor ink = dialogInk(parent);
    QMessageBox box(parent);
    setBoxIcon(&box, glyph::forCommand(QStringLiteral("file.trash"), ink));
    box.setWindowTitle(tr_(app, "operation.confirm_title"));
    box.setText(
        jtfFill(tr_(app, "operation.confirm_trash"), "count", QString::number(entries)));
    auto *proceed = box.addButton(tr_(app, "command.file.trash"), QMessageBox::AcceptRole);
    auto *cancel = box.addButton(tr_(app, "conflict.abort"), QMessageBox::RejectRole);
    iconise(proceed, glyph::forCommand(QStringLiteral("file.trash"), ink));
    iconise(cancel, glyph::make(glyph::Shape::Close, ink));
    box.setDefaultButton(cancel);
    box.setEscapeButton(cancel);
    box.exec();
    return box.clickedButton() == proceed;
}

bool confirmIrreversible(JtfApp *app, QWidget *parent, int entries) {
    const QColor ink = dialogInk(parent);
    QMessageBox box(parent);
    setBoxIcon(&box, glyph::forCommand(QStringLiteral("file.delete"), ink));
    box.setWindowTitle(tr_(app, "operation.confirm_title"));
    // docs/UI_UX_SPEC.md 10: say undo is impossible *before* the action.
    box.setText(jtfFill(tr_(app, "operation.confirm_irreversible"), "count",
                        QString::number(entries)));
    auto *proceed = box.addButton(tr_(app, "command.file.delete"), QMessageBox::DestructiveRole);
    auto *cancel = box.addButton(tr_(app, "conflict.abort"), QMessageBox::RejectRole);
    iconise(proceed, glyph::forCommand(QStringLiteral("file.delete"), ink));
    iconise(cancel, glyph::make(glyph::Shape::Close, ink));
    box.setDefaultButton(cancel);
    box.setEscapeButton(cancel);
    box.exec();
    return box.clickedButton() == proceed;
}

} // namespace

bool ops::awaitPlan(JtfApp *app, QWidget *parent) {
    // Below this, a dialog would appear and vanish before it could be read.
    constexpr int kShowAfterMs = 400;
    constexpr int kPollMs = 30;

    QElapsedTimer clock;
    clock.start();
    QProgressDialog *dialog = nullptr;
    bool cancelled = false;

    QEventLoop loop;
    QTimer poll;
    poll.setInterval(kPollMs);
    QObject::connect(&poll, &QTimer::timeout, &loop, [&] {
        const int state = jtf_plan_poll(app);
        if (state >= 0) {
            loop.exit(state);
            return;
        }
        if (dialog == nullptr && clock.elapsed() > kShowAfterMs) {
            dialog = new QProgressDialog(tr_(app, "plan.counting"), tr_(app, "operation.cancel"),
                                         0, 0, parent);
            dialog->setWindowModality(Qt::WindowModal);
            dialog->setMinimumDuration(0);
            dialog->setAutoClose(false);
            dialog->setAutoReset(false);
            QObject::connect(dialog, &QProgressDialog::canceled, &loop, [&] {
                cancelled = true;
                jtf_plan_cancel(app);
                loop.exit(0);
            });
            dialog->show();
        }
    });
    poll.start();
    const int result = loop.exec();
    poll.stop();
    delete dialog;

    if (cancelled) {
        // Cancelling the count is not a failure to report; the user said no.
        return false;
    }
    return result == 1;
}

bool ops::confirmAndStartTo(JtfApp *app, QWidget *parent, int pane, Kind kind,
                            const QString &destination, QString *message) {
    const QByteArray utf8 = destination.toUtf8();
    if (!jtf_op_prepare_to(app, pane, static_cast<int>(kind), utf8.constData()) ||
        !ops::awaitPlan(app, parent)) {
        if (message) {
            *message = errorMessage(app);
        }
        return false;
    }
    return ops::confirmAndRun(app, parent);
}

bool ops::confirmAndStartPaths(JtfApp *app, QWidget *parent, Kind kind,
                               const QStringList &sources, const QString &destination,
                               QString *message) {
    const QByteArray list = sources.join(QLatin1Char('\n')).toUtf8();
    const QByteArray into = destination.toUtf8();
    if (!jtf_op_prepare_paths(app, static_cast<int>(kind), list.constData(), into.constData()) ||
        !ops::awaitPlan(app, parent)) {
        if (message) {
            *message = errorMessage(app);
        }
        return false;
    }
    return ops::confirmAndRun(app, parent);
}

bool ops::confirmAndStart(JtfApp *app, QWidget *parent, int pane, Kind kind, QString *message) {
    if (!jtf_op_prepare(app, pane, static_cast<int>(kind)) || !ops::awaitPlan(app, parent)) {
        if (message) {
            *message = errorMessage(app);
        }
        return false;
    }

    return ops::confirmAndRun(app, parent);
}

// Everything between "there is a plan" and "it is running": the same
// questions whatever route built the plan.
bool ops::confirmAndRun(JtfApp *app, QWidget *parent) {
    // Every removal is confirmed, whichever route built the plan - the menu,
    // a key, the disc usage window. Permanent deletion gets the stronger
    // warning; the trash gets the plainer question.
    if (jtf_op_is_irreversible(app)) {
        if (!confirmIrreversible(app, parent, jtf_op_entries(app))) {
            return false;
        }
    } else if (jtf_op_removes(app) && !confirmTrash(app, parent, jtf_op_entries(app))) {
        return false;
    }

    int policy = 0;
    const int conflicts = jtf_op_conflicts(app);
    if (conflicts > 0) {
        policy = ops::askConflictPolicy(app, parent, conflicts);
        if (policy < 0) {
            return false;
        }
    }
    return jtf_op_start(app, policy) != 0;
}

namespace {

// Asks for a name, then prepares and starts a single-step operation.
bool nameThenStart(JtfApp *app, QWidget *parent, int pane, const char *titleKey,
                   const char *labelKey, const QString &initial,
                   int (*prepare)(JtfApp *, int, const char *), QString *message) {
    bool accepted = false;
    const QString name = dialogs::askForText(
        parent, [app](const char *key) { return tr_(app, key); }, tr_(app, titleKey),
        tr_(app, labelKey), initial,
        parent != nullptr ? parent->palette().color(QPalette::Text) : QColor(), &accepted);
    if (!accepted || name.trimmed().isEmpty()) {
        return false;
    }

    const QByteArray utf8 = name.trimmed().toUtf8();
    if (!prepare(app, pane, utf8.constData()) || !ops::awaitPlan(app, parent)) {
        if (message) {
            *message = errorMessage(app);
        }
        return false;
    }
    if (jtf_op_conflicts(app) > 0) {
        const int policy = ops::askConflictPolicy(app, parent, jtf_op_conflicts(app));
        if (policy < 0) {
            return false;
        }
        return jtf_op_start(app, policy) != 0;
    }
    return jtf_op_start(app, 0) != 0;
}

} // namespace

bool ops::renameSelection(JtfApp *app, QWidget *parent, int pane, QString *message) {
    // The name of the entry being renamed, which is what makes a small edit
    // small. This used to read jtf_op_current, which reports the *running
    // operation's* current entry and is therefore empty whenever nothing is
    // running - so the field was always blank while a comment claimed
    // otherwise.
    // The row the cursor is on, not the marked set. A mark made earlier and
    // left behind used to win over the row the cursor was visibly sitting on,
    // so pressing R on the fifth file brought up the first file's name - and
    // renamed that one.
    const QString current =
        jtfText([&](char *buf, int len) { return jtf_cursor_name(app, pane, buf, len); });

    return nameThenStart(app, parent, pane, "prompt.rename_title", "prompt.rename_label",
                         current, jtf_op_prepare_rename, message);
}

bool ops::createFile(JtfApp *app, QWidget *parent, int pane, QString *message) {
    return nameThenStart(app, parent, pane, "new_file.title", "new_file.label", QString(),
                         jtf_op_prepare_new_file, message);
}

bool ops::createFolder(JtfApp *app, QWidget *parent, int pane, QString *message) {
    return nameThenStart(app, parent, pane, "prompt.new_folder_title", "prompt.new_folder_label",
                         QString(), jtf_op_prepare_new_folder, message);
}

QString ops::takeResult(JtfApp *app) {
    if (!jtf_op_has_result(app)) {
        return {};
    }
    char keyBuf[128] = {};
    char errorBuf[1024] = {};
    int succeeded = 0;
    int skipped = 0;
    int failed = 0;
    if (!jtf_op_result(app, keyBuf, sizeof(keyBuf), errorBuf, sizeof(errorBuf), &succeeded,
                       &skipped, &failed)) {
        return {};
    }
    jtf_op_clear_result(app);

    QString text = tr_(app, keyBuf);
    text = jtfFill(text, "count", QString::number(succeeded));
    text = jtfFill(text, "done", QString::number(succeeded));
    text = jtfFill(text, "skipped", QString::number(skipped));
    text = jtfFill(text, "failed", QString::number(failed));

    // The first failure, with the entry that caused it: "permission denied"
    // without a path is not something a user can act on.
    const QString detail = QString::fromUtf8(errorBuf);
    if (!detail.isEmpty()) {
        const QStringList parts = detail.split(QLatin1Char('\t'));
        const QByteArray keyUtf8 = parts.value(0).toUtf8();
        QString reason = jtfText(
            [&](char *buf, int len) { return jtf_tr(app, keyUtf8.constData(), buf, len); });
        if (parts.size() > 1) {
            reason += QStringLiteral("  ") + parts.value(1);
        }
        text += QStringLiteral("   ") + reason;
    }
    return text;
}
