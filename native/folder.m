#import <AppKit/AppKit.h>
#include <stdlib.h>
#include <string.h>

// Shows the standard folder picker. Returns a malloc'd path the caller frees, or NULL if cancelled.
char *craft_choose_folder(const char *message, const char *initial) {
    NSOpenPanel *panel = [NSOpenPanel openPanel];
    panel.canChooseFiles = NO;
    panel.canChooseDirectories = YES;
    panel.canCreateDirectories = YES;
    panel.allowsMultipleSelection = NO;
    panel.prompt = @"Choose";
    if (message && *message) panel.message = @(message);
    if (initial && *initial) panel.directoryURL = [NSURL fileURLWithPath:@(initial) isDirectory:YES];
    if ([panel runModal] != NSModalResponseOK || !panel.URL.path) return NULL;
    return strdup(panel.URL.path.fileSystemRepresentation);
}
