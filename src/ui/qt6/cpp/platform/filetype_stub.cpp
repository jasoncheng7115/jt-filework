#include "filetype.h"

#ifdef _WIN32
#include <shlobj.h>
#include <shellapi.h>
#endif

#include <QProcess>
#include <QStandardPaths>
#include <QtGlobal>

namespace filetype {

// Windows and Linux fall back to QMimeDatabase, which on those platforms does
// carry the freedesktop descriptions. A native implementation would use
// SHGetFileInfo on Windows; it is not needed for a correct answer there.
bool available() { return false; }

QString describe(const QString &) { return {}; }

// Windows localizes these through desktop.ini and SHGetFileInfo; Linux
// through XDG user-dirs, which QStandardPaths already reads. Both are handled
// by the caller's fallback.
QString displayName(const QString &) { return {}; }

namespace {

#ifndef Q_OS_WIN
// Tried in order. `$TERMINAL` first because a user who set it has already
// answered this question; `x-terminal-emulator` next because on Debian and
// its derivatives that is the answer the system itself gives.
const char *const kTerminals[] = {
    "x-terminal-emulator", "gnome-terminal", "konsole",   "xfce4-terminal",
    "kitty",               "alacritty",      "wezterm",   "tilix",
    "mate-terminal",       "lxterminal",     "foot",      "xterm",
};

QString firstTerminalOnPath() {
    const QString preferred = qEnvironmentVariable("TERMINAL");
    if (!preferred.isEmpty() && !QStandardPaths::findExecutable(preferred).isEmpty()) {
        return preferred;
    }
    for (const char *const name : kTerminals) {
        const QString found = QStandardPaths::findExecutable(QLatin1String(name));
        if (!found.isEmpty()) {
            return found;
        }
    }
    return {};
}
#endif

} // namespace

bool canOpenInTerminal() {
#ifdef Q_OS_WIN
    return true;
#else
    return !firstTerminalOnPath().isEmpty();
#endif
}

bool openInTerminal(const QString &path) {
    if (path.isEmpty()) {
        return false;
    }
#ifdef Q_OS_WIN
    // Windows Terminal if it is installed, else the console. Arguments go as
    // a list and the directory as the process's own working directory, so a
    // folder called `a & b` is a folder name and never syntax
    // (`AGENTS.md` 20.3).
    if (QProcess::startDetached(QStringLiteral("wt.exe"),
                                {QStringLiteral("-d"), path})) {
        return true;
    }
    return QProcess::startDetached(QStringLiteral("cmd.exe"), {}, path);
#else
    const QString terminal = firstTerminalOnPath();
    if (terminal.isEmpty()) {
        return false;
    }
    // The folder is the process's working directory rather than a flag:
    // every one of these spells that flag differently, and all of them
    // inherit the directory they were started in.
    return QProcess::startDetached(terminal, {}, path);
#endif
}

// Windows: SHAssocEnumHandlers; Linux: the XDG desktop database. Until then
// the menu offers nothing rather than a list that does nothing.
QList<Application> applicationsFor(const QString &) { return {}; }

bool openWith(const QString &, const QString &) { return false; }

// Windows: IFileOperation with FOF_ALLOWUNDO; Linux: the freedesktop trash
// specification's info files. Until then the caller's own fallback runs.
QString moveToTrash(const QString &) { return {}; }

// Windows has no equivalent; Linux stores tags in extended attributes that no
// two file managers agree on. The column stays empty rather than inventing
// something only this program would understand.
QStringList tagsFor(const QString &) { return {}; }

} // namespace filetype

bool filetype::openInEditor(const QString &) {
    // Windows and Linux get their own implementations with the platform
    // adapters; until then the command is absent rather than inert.
    return false;
}

bool filetype::canOpenInEditor() {
    return false;
}

QIcon filetype::iconForExtension(const QString &) {
    // Windows has `SHGetFileInfo` with `SHGFI_USEFILEATTRIBUTES`, which answers
    // about a type without a file; Linux has the icon theme by MIME name.
    // Neither is built yet, so this says so and the caller uses its generic
    // icon rather than showing nothing (docs/PLATFORM_INTEGRATION.md 1).
    return {};
}

QString filetype::rootLabel() {
#ifdef _WIN32
    // Asked of the shell, so it is Explorer's own word in the user's own
    // language - 「本機」 on a Chinese install, "This PC" on an English one -
    // rather than a string this program guessed and would have to translate
    // itself. CSIDL_DRIVES is the folder Explorer shows the drives under.
    PIDLIST_ABSOLUTE drives = nullptr;
    if (SUCCEEDED(SHGetFolderLocation(nullptr, CSIDL_DRIVES, nullptr, 0, &drives))) {
        SHFILEINFOW info = {};
        const bool ok = SHGetFileInfoW(reinterpret_cast<LPCWSTR>(drives), 0, &info, sizeof(info),
                                       SHGFI_PIDL | SHGFI_DISPLAYNAME)
                        != 0;
        CoTaskMemFree(drives);
        if (ok) {
            return QString::fromWCharArray(info.szDisplayName);
        }
    }
    // The shell would not say. Better an honest English fallback than `\`,
    // which names a path Windows has no such thing as.
    return QStringLiteral("This PC");
#else
    // Linux has one root and calls it `/`.
    return {};
#endif
}
