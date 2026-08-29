// Native Quick Look.
//
// AGENTS.md 8: use the platform's own behaviour where users expect it, and
// pressing Space on a Mac is the clearest case there is. This is the real
// QLPreviewPanel, not a picture of one - the same panel Finder shows,
// including the formats we will never write a viewer for.
//
// Platform code, kept in its own file behind a platform-neutral interface so
// nothing above it needs an #ifdef (AGENTS.md 5).
#pragma once

#include <QString>

namespace quicklook {

// Whether this build can show a Quick Look panel at all.
bool available();

// Show, or update, the panel for `path`. Pressing Space again hides it, which
// is what Finder does.
void toggle(const QString &path);

// Hide the panel if it is showing.
void hide();

} // namespace quicklook

namespace platform {

/// Show an entry in the system's own file manager, selected.
///
/// `AGENTS.md` §8: users expect "reveal" to open Finder with the item
/// highlighted, not merely to open its folder.
bool reveal(const QString &path);

/// Whether this build can reveal at all, so the UI can disable the command
/// rather than offering something that does nothing.
bool canReveal();

} // namespace platform
