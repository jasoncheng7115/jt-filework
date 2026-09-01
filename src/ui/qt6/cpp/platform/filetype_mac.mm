#include "filetype.h"
#include <QPixmap>
#include <QImage>

#import <AppKit/AppKit.h>
#import <Foundation/Foundation.h>
#import <UniformTypeIdentifiers/UniformTypeIdentifiers.h>

namespace filetype {

bool available() { return true; }

QString describe(const QString &path) {
    @autoreleasepool {
        NSString *native = path.toNSString();
        NSURL *url = [NSURL fileURLWithPath:native];
        if (url == nil) {
            return {};
        }

        // Asked of the file itself rather than of its extension: macOS knows
        // the type of an extensionless file, and an extension that lies about
        // its contents should not be believed either.
        NSError *error = nil;
        UTType *type = nil;
        NSURLResourceKey key = NSURLContentTypeKey;
        NSDictionary *values = [url resourceValuesForKeys:@[ key ] error:&error];
        if (values != nil) {
            type = values[key];
        }
        if (type == nil) {
            return {};
        }

        NSString *description = type.localizedDescription;
        if (description == nil) {
            return {};
        }
        return QString::fromNSString(description);
    }
}

QString displayName(const QString &path) {
    @autoreleasepool {
        NSString *native = path.toNSString();
        NSString *shown = [[NSFileManager defaultManager] displayNameAtPath:native];
        return shown == nil ? QString() : QString::fromNSString(shown);
    }
}

bool canOpenInTerminal() { return true; }

bool openInTerminal(const QString &path) {
    @autoreleasepool {
        NSURL *folder = [NSURL fileURLWithPath:path.toNSString()];
        NSURL *terminal = [[NSWorkspace sharedWorkspace]
            URLForApplicationWithBundleIdentifier:@"com.apple.Terminal"];
        if (folder == nil || terminal == nil) {
            return false;
        }
        // The path travels as a URL in an argument list, so it is never
        // parsed as shell syntax.
        [[NSWorkspace sharedWorkspace] openURLs:@[ folder ]
                           withApplicationAtURL:terminal
                                  configuration:[NSWorkspaceOpenConfiguration configuration]
                              completionHandler:nil];
        return true;
    }
}

QString rootLabel() {
    // macOS has one root and calls it `/`. Nothing to add.
    return {};
}

bool openInEditor(const QString &path) {
    @autoreleasepool {
        NSURL *file = [NSURL fileURLWithPath:path.toNSString()];
        if (file == nil) {
            return false;
        }
        // The application registered for plain text, which is what `open -t`
        // uses and what the user has already chosen in their own system. Asked
        // for by content type rather than by bundle id, so it is their editor
        // and not one this program picked.
        NSURL *editor = [[NSWorkspace sharedWorkspace]
            URLForApplicationToOpenContentType:UTTypePlainText];
        if (editor == nil) {
            return false;
        }
        // As a URL in an argument list; never as shell syntax.
        [[NSWorkspace sharedWorkspace] openURLs:@[ file ]
                           withApplicationAtURL:editor
                                  configuration:[NSWorkspaceOpenConfiguration configuration]
                              completionHandler:nil];
        return true;
    }
}

bool canOpenInEditor() {
    return true;
}

QList<Application> applicationsFor(const QString &path) {
    @autoreleasepool {
        QList<Application> found;
        NSURL *url = [NSURL fileURLWithPath:path.toNSString()];
        if (url == nil) {
            return found;
        }
        // Launch Services already knows the answer, and it is the same answer
        // Finder's own Open With shows. Building our own list from extensions
        // would disagree with the rest of the system.
        NSArray<NSURL *> *apps =
            [[NSWorkspace sharedWorkspace] URLsForApplicationsToOpenURL:url];
        for (NSURL *app in apps) {
            NSString *name = [[NSFileManager defaultManager] displayNameAtPath:app.path];
            if (name == nil) {
                name = app.lastPathComponent;
            }
            // Without the `.app`. Finder never shows it, and a menu of
            // "Preview.app, GIMP.app, Safari.app" reads as a directory
            // listing rather than as a list of applications.
            if ([name hasSuffix:@".app"]) {
                name = [name substringToIndex:name.length - 4];
            }
            // The URL is the identifier: a bundle id can be shared by two
            // copies of an application, and the path cannot.
            found.append({QString::fromNSString(name), QString::fromNSString(app.path)});
        }
        return found;
    }
}

bool openWith(const QString &path, const QString &identifier) {
    @autoreleasepool {
        NSURL *file = [NSURL fileURLWithPath:path.toNSString()];
        NSURL *app = [NSURL fileURLWithPath:identifier.toNSString()];
        if (file == nil || app == nil) {
            return false;
        }
        [[NSWorkspace sharedWorkspace] openURLs:@[ file ]
                           withApplicationAtURL:app
                                  configuration:[NSWorkspaceOpenConfiguration configuration]
                              completionHandler:nil];
        return true;
    }
}

QString moveToTrash(const QString &path) {
    @autoreleasepool {
        NSURL *url = [NSURL fileURLWithPath:path.toNSString()];
        if (url == nil) {
            return {};
        }
        NSURL *resulting = nil;
        NSError *error = nil;
        // trashItemAtURL is what Finder itself uses: it records the original
        // location for Put Back and picks the right volume's trash.
        const BOOL ok = [[NSFileManager defaultManager] trashItemAtURL:url
                                                      resultingItemURL:&resulting
                                                                 error:&error];
        if (!ok || resulting == nil) {
            return {};
        }
        return QString::fromNSString(resulting.path);
    }
}

QStringList tagsFor(const QString &path) {
    @autoreleasepool {
        QStringList tags;
        NSURL *url = [NSURL fileURLWithPath:path.toNSString()];
        if (url == nil) {
            return tags;
        }
        NSArray<NSString *> *names = nil;
        NSError *error = nil;
        // Asked of the file, not of a database: a tag set in Finder a second
        // ago is on the file already.
        if (![url getResourceValue:&names forKey:NSURLTagNamesKey error:&error]) {
            return tags;
        }
        for (NSString *name in names) {
            tags.append(QString::fromNSString(name));
        }
        return tags;
    }
}

QIcon iconForExtension(const QString &extension) {
    @autoreleasepool {
        if (extension.isEmpty()) {
            return {};
        }
        // Asked of the type, not of a file. `iconForContentType:` is the
        // modern spelling and needs no file to exist - which is the whole
        // point here, since a breakdown by kind has a hundred `.png` files
        // and no single path to point at.
        UTType *type = [UTType typeWithFilenameExtension:extension.toNSString()];
        if (type == nil) {
            return {};
        }
        NSImage *image = [[NSWorkspace sharedWorkspace] iconForContentType:type];
        if (image == nil) {
            return {};
        }
        // An NSImage is resolution independent; a QIcon is a set of pixmaps,
        // so one has to be rendered. Through PNG rather than TIFF: Qt always
        // has a PNG reader, and its TIFF one is an optional plugin that may
        // not be in the deployed bundle.
        [image setSize:NSMakeSize(64, 64)];
        NSBitmapImageRep *rep =
            [NSBitmapImageRep imageRepWithData:[image TIFFRepresentation]];
        if (rep == nil) {
            return {};
        }
        NSData *png = [rep representationUsingType:NSBitmapImageFileTypePNG
                                        properties:@{}];
        if (png == nil) {
            return {};
        }
        QImage rendered;
        if (!rendered.loadFromData(static_cast<const uchar *>(png.bytes),
                                   static_cast<int>(png.length), "PNG")) {
            return {};
        }
        return QIcon(QPixmap::fromImage(rendered));
    }
}

} // namespace filetype
