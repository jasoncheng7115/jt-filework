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

#include <QString>

namespace filetype {

// Whether this build can answer from the platform at all.
bool available();

// The platform's localized description for `path`, or an empty string when
// it has none. The caller falls back.
QString describe(const QString &path);

} // namespace filetype
