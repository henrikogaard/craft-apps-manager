#import <Foundation/Foundation.h>

// SMAppService (macOS 13+) is looked up at run time so older systems still start.
@interface CraftAppService : NSObject
+ (instancetype)mainAppService;
- (BOOL)registerAndReturnError:(NSError **)error;
- (BOOL)unregisterAndReturnError:(NSError **)error;
@property (readonly) NSInteger status;
@end

static CraftAppService *service(void) {
    Class cls = NSClassFromString(@"SMAppService");
    return cls ? [(id)cls mainAppService] : nil;
}

// 1 is SMAppServiceStatusEnabled.
bool craft_login_enabled(void) { return service().status == 1; }

// Returns NULL on success, or a malloc'd message the caller frees.
char *craft_login_set(bool on) {
    CraftAppService *svc = service();
    if (!svc) return strdup("Start at login needs macOS 13 or later");
    NSError *error = nil;
    BOOL ok = on ? [svc registerAndReturnError:&error] : [svc unregisterAndReturnError:&error];
    if (ok) return NULL;
    return strdup((error.localizedDescription ?: @"Could not change the login item").UTF8String);
}
