#include "share.h"

namespace share {

// Windows: `IContextMenu` from the shell, which also carries "Send to" and
// anything the user has installed. Linux has no equivalent - the desktops
// disagree, and inventing one would be a jt-filework menu wearing the
// system's name. Reported as unavailable so the entry is absent rather than
// present and inert.
bool available() { return false; }

void showPicker(QWidget *, const QPoint &, const QStringList &) {}

} // namespace share
