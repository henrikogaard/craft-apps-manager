#import <Carbon/Carbon.h>

// Implemented in Rust (src/dock.rs).
extern void craft_dock_launch(const char *app);

static EventHotKeyRef hotKey;
static EventHandlerRef handler;

static OSStatus pressed(__unused EventHandlerCallRef next, __unused EventRef event, __unused void *data) {
    craft_dock_launch("__toggle");
    return noErr;
}

// Registers one system-wide shortcut, replacing the previous one. A zero key code turns it off.
// This Carbon call needs no accessibility permission.
bool craft_hotkey_set(uint32_t keyCode, uint32_t modifiers) {
    if (hotKey) {
        UnregisterEventHotKey(hotKey);
        hotKey = NULL;
    }
    if (!keyCode) return true;
    if (!handler) {
        EventTypeSpec spec = {kEventClassKeyboard, kEventHotKeyPressed};
        if (InstallApplicationEventHandler(&pressed, 1, &spec, NULL, &handler) != noErr) return false;
    }
    EventHotKeyID id = {'CrLb', 1};
    return RegisterEventHotKey(keyCode, modifiers, id, GetApplicationEventTarget(), 0, &hotKey) == noErr;
}
