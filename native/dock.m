#import <AppKit/AppKit.h>
#import <objc/runtime.h>

// Implemented in Rust (src/dock.rs); called on the main thread when a Dock menu item is chosen.
extern void craft_dock_launch(const char *app);

static NSMenu *dockMenu;

@interface CraftDockTarget : NSObject
- (void)launch:(NSMenuItem *)item;
@end
@implementation CraftDockTarget
- (void)launch:(NSMenuItem *)item {
    NSString *app = item.representedObject;
    if (app) craft_dock_launch(app.UTF8String);
}
@end
static CraftDockTarget *dockTarget;

static NSMenu *craftDockMenu(__unused id self, __unused SEL _cmd, __unused NSApplication *sender) {
    return dockMenu.numberOfItems ? dockMenu : nil;
}

// The window library owns the application delegate, so the Dock menu is added to its class.
bool craft_dock_start(void) {
    id delegate = NSApp.delegate;
    if (!delegate) return false;
    dockTarget = [CraftDockTarget new];
    dockMenu = [NSMenu new];
    dockMenu.autoenablesItems = NO;
    SEL selector = @selector(applicationDockMenu:);
    Class cls = [delegate class];
    if (class_getInstanceMethod(cls, selector)) return false;
    return class_addMethod(cls, selector, (IMP)craftDockMenu, "@@:@");
}

void craft_dock_set_apps(const char *const *ids, const char *const *titles,
                         const unsigned char *const *icons, const size_t *iconLengths, size_t count) {
    if (!dockMenu) return;
    [dockMenu removeAllItems];
    for (size_t i = 0; i < count; i++) {
        NSMenuItem *item = [[NSMenuItem alloc] initWithTitle:@(titles[i]) action:@selector(launch:) keyEquivalent:@""];
        item.target = dockTarget;
        item.representedObject = @(ids[i]);
        if (icons[i] && iconLengths[i]) {
            NSImage *image = [[NSImage alloc] initWithData:[NSData dataWithBytes:icons[i] length:iconLengths[i]]];
            image.size = NSMakeSize(16, 16);
            item.image = image;
        }
        [dockMenu addItem:item];
    }
}

void craft_dock_set_badge(const char *label) {
    NSApp.dockTile.badgeLabel = (label && *label) ? @(label) : nil;
}
