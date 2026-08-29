// Quick Look on platforms that do not have it.
//
// A null implementation rather than an #ifdef at every call site
// (docs/PLATFORM_INTEGRATION.md 1). `available()` returning false is how the
// UI knows to hide the command instead of offering something that does
// nothing.
#include "quicklook.h"

bool quicklook::available() {
    return false;
}

void quicklook::toggle(const QString &) {}

void quicklook::hide() {}
