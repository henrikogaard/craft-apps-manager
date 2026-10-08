//! Sparkle manager updates. All functions are called on the AppKit main thread.
use anyhow::{bail, Result};
use serde::Serialize;
pub const REPOSITORY_NAME: &str = env!("CRAFT_MANAGER_REPOSITORY");
pub const REPOSITORY: &str = concat!("https://github.com/", env!("CRAFT_MANAGER_REPOSITORY"));
extern "C" {
    fn craft_sparkle_start() -> bool;
    fn craft_sparkle_available() -> bool;
    fn craft_sparkle_can_check() -> bool;
    fn craft_sparkle_check();
    fn craft_sparkle_checks() -> bool;
    fn craft_sparkle_downloads() -> bool;
    fn craft_sparkle_configure(checks: bool, downloads: bool);
    fn craft_sparkle_busy(busy: bool);
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub available: bool,
    pub checks: bool,
    pub downloads: bool,
}
pub fn initialize() -> bool {
    unsafe { craft_sparkle_start() }
}
pub fn settings() -> Settings {
    unsafe {
        Settings {
            available: craft_sparkle_available(),
            checks: craft_sparkle_checks(),
            downloads: craft_sparkle_downloads(),
        }
    }
}
pub fn check() -> Result<()> {
    if !unsafe { craft_sparkle_available() } {
        bail!("Sparkle is available in packaged Craft Library builds");
    }
    if !unsafe { craft_sparkle_can_check() } {
        bail!("An update check or Craft operation is already running");
    }
    unsafe {
        craft_sparkle_check();
    }
    Ok(())
}
pub fn configure(checks: bool, downloads: bool) {
    unsafe {
        craft_sparkle_configure(checks, downloads);
    }
}
pub fn set_busy(busy: bool) {
    unsafe {
        craft_sparkle_busy(busy);
    }
}
