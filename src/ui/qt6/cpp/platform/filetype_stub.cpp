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

} // namespace filetype
