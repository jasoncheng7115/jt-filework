// The platform's own name for a file's type.
//
// "Markdown Document", the words Finder shows in its Kind column — not
// `text/markdown`, and not a guess made from the extension. AGENTS.md 8 asks
// for the platform's own behaviour where a user would recognise it, and the
// type column is one of those places: it should agree with the file manager
// the user already has.
//
// Qt's QMimeDatabase is the portable fallback, but on macOS it ships without
// the freedesktop description database, so its comments come back empty and
// the raw MIME name is all that is left.
//
// Platform code behind a platform-neutral interface, so nothing above it
// needs an #ifdef (AGENTS.md 5).
#pragma once

#include <QList>
#include <QString>

namespace filetype {

// Whether this build can answer from the platform at all.
bool available();

// The platform's localized description for `path`, or an empty string when
// it has none. The caller falls back.
QString describe(const QString &path);

// The name the platform shows for a file or folder, or empty.
//
// macOS localizes the standard folders: `~/Desktop` is shown as 桌面 on a
// Chinese system, and the folder on disk is still called Desktop. Reading the
// last path component would show the English name in a Chinese interface,
// which is not what the user's own file manager does.
QString displayName(const QString &path);

// Open `path` in the platform's terminal application.
//
// Through the platform's "open this with that application" API, never by
// building a command line: a folder name containing a quote or a semicolon
// must be a folder name, not syntax (`AGENTS.md` 20.3).
bool openInTerminal(const QString &path);

/// One application that can open a file.
struct Application {
    /// What to show in the menu.
    QString name;
    /// How to identify it when opening. Opaque to the caller.
    QString identifier;
};

// Applications the platform says can open `path`, best first.
//
// Empty when the platform cannot answer, in which case the caller offers
// nothing rather than a menu that does nothing.
QList<Application> applicationsFor(const QString &path);

// Open `path` with the application named by `identifier`.
bool openWith(const QString &path, const QString &identifier);

// Move `path` to the platform's trash, returning where it went, or empty.
//
// The platform's own call, so Finder's Put Back works afterwards and an item
// on another volume goes to that volume's trash rather than to the home
// directory's - neither of which a hand-rolled move can do.
QString moveToTrash(const QString &path);

} // namespace filetype
