#include "share.h"

#import <AppKit/AppKit.h>

#include <QPoint>
#include <QWidget>

namespace share {

bool available() { return true; }

void showPicker(QWidget *parent, const QPoint &at, const QStringList &paths) {
    @autoreleasepool {
        if (paths.isEmpty() || parent == nullptr) {
            return;
        }
        NSMutableArray<NSURL *> *urls = [NSMutableArray arrayWithCapacity:paths.size()];
        for (const QString &path : paths) {
            NSURL *url = [NSURL fileURLWithPath:path.toNSString()];
            if (url != nil) {
                [urls addObject:url];
            }
        }
        if (urls.count == 0) {
            return;
        }

        // Anchored on the widget the user clicked, so the sheet points at the
        // rows it is about rather than at the window's corner.
        NSView *view = reinterpret_cast<NSView *>(parent->winId());
        if (view == nil) {
            return;
        }
        auto *picker = [[NSSharingServicePicker alloc] initWithItems:urls];
        // The point arrives in the widget's own coordinates, so the rectangle
        // has to be in the view's own coordinates too - `bounds`, not `frame`.
        // `frame` is where the view sits in its *superview*, and using its
        // height put the sheet a pane and a half away from the row that was
        // clicked.
        //
        // AppKit's origin is normally the bottom left and Qt's is the top
        // left, but a view can be flipped and then the two already agree.
        // Asking is cheaper than assuming and being wrong on one of them.
        NSRect anchor = NSMakeRect(at.x(), at.y(), 1, 1);
        if (!view.isFlipped) {
            anchor.origin.y = view.bounds.size.height - at.y();
        }
        [picker showRelativeToRect:anchor ofView:view preferredEdge:NSMinYEdge];
    }
}

} // namespace share
