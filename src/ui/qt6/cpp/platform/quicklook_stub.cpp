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

bool platform::reveal(const QString &) {
    // Windows and Linux get their own implementations with the platform
    // adapters; until then the UI hides the command rather than offering
    // something that does nothing (docs/PLATFORM_INTEGRATION.md 1).
    return false;
}

bool platform::canReveal() {
    return false;
}

bool platform::eject(const QString &) {
    // Windows and Linux get their own implementations with the platform
    // adapters; until then the sidebar shows no eject control rather than one
    // that does nothing.
    return false;
}

bool platform::canEject() {
    return false;
}
