#import <Foundation/Foundation.h>

// Runtime loading keeps headless commands and unbundled development builds usable.
@interface CraftSparkleUpdater : NSObject
@property BOOL automaticallyChecksForUpdates;
@property BOOL automaticallyDownloadsUpdates;
@property (readonly) BOOL canCheckForUpdates;
@end
@interface CraftSparkleController : NSObject
- (instancetype)initWithStartingUpdater:(BOOL)start updaterDelegate:(id)delegate userDriverDelegate:(id)driver;
- (void)checkForUpdates:(id)sender;
@property (readonly) CraftSparkleUpdater *updater;
@end

static CraftSparkleController *controller;
static BOOL jobBusy;
static void (^pendingInstall)(void);
@interface CraftUpdateDelegate : NSObject
@end
@implementation CraftUpdateDelegate
- (BOOL)updater:(id)updater mayPerformUpdateCheck:(NSInteger)kind error:(NSError **)error {
    if (!jobBusy) return YES;
    if (error) *error = [NSError errorWithDomain:@"CraftLibrary" code:1 userInfo:@{NSLocalizedDescriptionKey: @"Finish the current Craft operation before checking for a manager update."}];
    return NO;
}
- (BOOL)updater:(id)updater shouldPostponeRelaunchForUpdate:(id)item untilInvokingBlock:(void (^)(void))handler {
    if (!jobBusy) return NO;
    pendingInstall = [handler copy];
    return YES;
}
@end
static CraftUpdateDelegate *updateDelegate;

bool craft_sparkle_start(void) {
    NSBundle *main = NSBundle.mainBundle;
    if (![main objectForInfoDictionaryKey:@"SUFeedURL"] || ![main objectForInfoDictionaryKey:@"SUPublicEDKey"]) return false;
    NSString *path = [main.privateFrameworksPath stringByAppendingPathComponent:@"Sparkle.framework"];
    if (!path || ![[NSBundle bundleWithPath:path] load]) return false;
    Class updaterClass = NSClassFromString(@"SPUStandardUpdaterController");
    if (!updaterClass) return false;
    updateDelegate = [CraftUpdateDelegate new];
    controller = [[updaterClass alloc] initWithStartingUpdater:YES updaterDelegate:updateDelegate userDriverDelegate:nil];
    return controller != nil;
}
bool craft_sparkle_available(void) { return controller != nil; }
bool craft_sparkle_can_check(void) { return controller && controller.updater.canCheckForUpdates && !jobBusy; }
void craft_sparkle_check(void) { if (craft_sparkle_can_check()) [controller checkForUpdates:nil]; }
bool craft_sparkle_checks(void) { return controller && controller.updater.automaticallyChecksForUpdates; }
bool craft_sparkle_downloads(void) { return controller && controller.updater.automaticallyDownloadsUpdates; }
void craft_sparkle_configure(bool checks, bool downloads) {
    if (!controller) return;
    if (controller.updater.automaticallyChecksForUpdates != checks) controller.updater.automaticallyChecksForUpdates = checks;
    if (controller.updater.automaticallyDownloadsUpdates != downloads) controller.updater.automaticallyDownloadsUpdates = downloads;
}
void craft_sparkle_busy(bool busy) {
    jobBusy = busy;
    if (!busy && pendingInstall) { void (^handler)(void) = pendingInstall; pendingInstall = nil; handler(); }
}
