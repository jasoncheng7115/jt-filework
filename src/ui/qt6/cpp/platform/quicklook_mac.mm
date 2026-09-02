// Quick Look on macOS, through QLPreviewPanel.
//
// The panel is shared and asks a data source for its items, so this file owns
// a small delegate object that answers for exactly one path at a time. That
// is enough for Space-to-preview and keeps the object graph trivial; previewing
// a whole selection with arrow keys is a later step and is in TODO.md.
#include "quicklook.h"

#import <AppKit/AppKit.h>
#import <Foundation/Foundation.h>
#import <Quartz/Quartz.h>

@interface JtfQuickLookSource : NSObject <QLPreviewPanelDataSource, QLPreviewPanelDelegate>
@property(nonatomic, strong) NSURL *url;
@end

@implementation JtfQuickLookSource

- (NSInteger)numberOfPreviewItemsInPreviewPanel:(QLPreviewPanel *)panel {
    (void)panel;
    return self.url ? 1 : 0;
}

- (id<QLPreviewItem>)previewPanel:(QLPreviewPanel *)panel previewItemAtIndex:(NSInteger)index {
    (void)panel;
    (void)index;
    return self.url;
}

// Let the panel forward keys it does not use back to the application, so the
// arrow keys still move the selection underneath it, as they do in Finder.
//
// Never back to the panel, which is what this used to do. While the panel is
// open it *is* `[NSApp keyWindow]`, so sending the event there re-entered this
// method with the same event, which sent it there again: pressing Space in
// front of an open Quick Look panel recursed until the stack ran out and the
// program died. The file list is the main window, and it is the one that
// wanted the key.
- (BOOL)previewPanel:(QLPreviewPanel *)panel handleEvent:(NSEvent *)event {
    if (event.type != NSEventTypeKeyDown) {
        return NO;
    }
    NSWindow *target = [NSApp mainWindow];
    if (target == nil || target == (NSWindow *)panel) {
        // Nowhere safe to send it. Letting the panel have it is worse than
        // dropping it, and both are better than recursing.
        return NO;
    }
    [target sendEvent:event];
    return YES;
}

@end

namespace {

JtfQuickLookSource *sharedSource() {
    static JtfQuickLookSource *source = [[JtfQuickLookSource alloc] init];
    return source;
}

} // namespace

bool quicklook::available() {
    return [QLPreviewPanel sharedPreviewPanelExists] || QLPreviewPanel.class != nil;
}

void quicklook::toggle(const QString &path) {
    if (path.isEmpty()) {
        return;
    }
    @autoreleasepool {
        NSString *native = [NSString stringWithUTF8String:path.toUtf8().constData()];
        if (!native) {
            return;
        }

        QLPreviewPanel *panel = [QLPreviewPanel sharedPreviewPanel];
        JtfQuickLookSource *source = sharedSource();

        // Space on the item already showing closes the panel, exactly as in
        // Finder; on a different item it swaps rather than stacking panels.
        const bool sameItem = source.url && [source.url.path isEqualToString:native];
        if ([QLPreviewPanel sharedPreviewPanelExists] && panel.isVisible && sameItem) {
            [panel orderOut:nil];
            return;
        }

        source.url = [NSURL fileURLWithPath:native];
        panel.dataSource = source;
        panel.delegate = source;

        if (panel.isVisible) {
            [panel reloadData];
        } else {
            [panel makeKeyAndOrderFront:nil];
        }
    }
}

void quicklook::hide() {
    @autoreleasepool {
        if ([QLPreviewPanel sharedPreviewPanelExists]) {
            [[QLPreviewPanel sharedPreviewPanel] orderOut:nil];
        }
    }
}

bool platform::reveal(const QString &path) {
    if (path.isEmpty()) {
        return false;
    }
    @autoreleasepool {
        NSString *native = [NSString stringWithUTF8String:path.toUtf8().constData()];
        if (!native) {
            return false;
        }
        // selectFile: highlights the item; openFile: would only open its
        // folder, which is not what "reveal" means.
        return [[NSWorkspace sharedWorkspace] selectFile:native
                                inFileViewerRootedAtPath:native.stringByDeletingLastPathComponent];
    }
}

bool platform::canReveal() {
    return true;
}

bool platform::eject(const QString &mountPoint) {
    if (mountPoint.isEmpty()) {
        return false;
    }
    @autoreleasepool {
        NSString *native = [NSString stringWithUTF8String:mountPoint.toUtf8().constData()];
        if (!native) {
            return false;
        }
        // NSWorkspace rather than `diskutil eject`: this is the same call
        // Finder's own eject makes, so the system notifies whatever has the
        // disk open and the volume actually leaves rather than being
        // unmounted behind the desktop's back.
        NSError *error = nil;
        const BOOL ok = [[NSWorkspace sharedWorkspace]
            unmountAndEjectDeviceAtURL:[NSURL fileURLWithPath:native]
                                 error:&error];
        return ok == YES;
    }
}

bool platform::canEject() {
    return true;
}
