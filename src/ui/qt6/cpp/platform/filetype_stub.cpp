#include "filetype.h"

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

// Windows: ShellExecute on wt.exe or cmd.exe; Linux: the XDG terminal. Not
// built yet, and reported as unavailable so the menu entry can be absent
// rather than present and inert.
bool openInTerminal(const QString &) { return false; }

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
