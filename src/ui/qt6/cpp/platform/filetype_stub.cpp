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

} // namespace filetype
