#include "filetype.h"

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

} // namespace filetype
