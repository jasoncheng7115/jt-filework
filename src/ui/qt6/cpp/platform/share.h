// The platform's own "do something with these files" menu.
//
// `docs/BASELINE_FEATURES.md` asks for the system's right-click integration.
// What that means differs by platform, and on macOS it splits in two:
//
//   - The **share sheet** (`NSSharingServicePicker`): the list of things that
//     can take these files — Mail, Messages, AirDrop, Photos, whatever is
//     installed. Reachable, and implemented here.
//   - The **Services menu**: reachable only by an application whose first
//     responder answers `validRequestorForSendType:returnType:` with the
//     selection on a pasteboard. Qt's widgets are not NSResponders in that
//     sense, so the items appear disabled. Faking a responder to carry the
//     selection is more machinery than the feature is worth, and would break
//     on any Qt release that changes how it hosts its views. It is recorded
//     as not done rather than half-done.
//
// Windows' shell context menu (`IContextMenu`) is the equivalent there and is
// not built.
#pragma once

#include <QStringList>

class QWidget;
class QPoint;

namespace share {

/// Whether this build can offer the platform's share sheet.
bool available();

/// Show the share sheet for `paths`, anchored at `at` in `parent`.
void showPicker(QWidget *parent, const QPoint &at, const QStringList &paths);

} // namespace share
