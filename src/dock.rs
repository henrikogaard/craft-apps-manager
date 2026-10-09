//! Native macOS pieces: Dock menu and badge, menu bar menu, login item and folder picker.
//! Main thread only.
use std::{
    ffi::{c_char, CStr, CString},
    sync::OnceLock,
};
extern "C" {
    fn craft_dock_start() -> bool;
    fn craft_dock_set_apps(
        ids: *const *const c_char,
        titles: *const *const c_char,
        icons: *const *const u8,
        icon_lengths: *const usize,
        count: usize,
    );
    fn craft_dock_set_badge(label: *const c_char);
}
type Launch = Box<dyn Fn(String) + Send + Sync>;
static LAUNCH: OnceLock<Launch> = OnceLock::new();

#[no_mangle]
extern "C" fn craft_dock_launch(app: *const c_char) {
    if app.is_null() {
        return;
    }
    let app = unsafe { CStr::from_ptr(app) }
        .to_string_lossy()
        .into_owned();
    if let Some(launch) = LAUNCH.get() {
        launch(app);
    }
}
/// Installs the Dock menu. `launch` receives the app id of the chosen item.
pub fn start(launch: impl Fn(String) + Send + Sync + 'static) -> bool {
    let _ = LAUNCH.set(Box::new(launch));
    unsafe { craft_dock_start() }
}
/// Replaces the Dock menu items with `(id, title, png icon)` entries.
pub fn set_apps(apps: &[(&str, String, &[u8])]) {
    let ids: Vec<_> = apps
        .iter()
        .filter_map(|(id, _, _)| CString::new(*id).ok())
        .collect();
    let titles: Vec<_> = apps
        .iter()
        .filter_map(|(_, title, _)| CString::new(title.as_str()).ok())
        .collect();
    if ids.len() != apps.len() || titles.len() != apps.len() {
        return;
    }
    let id_ptrs: Vec<_> = ids.iter().map(|s| s.as_ptr()).collect();
    let title_ptrs: Vec<_> = titles.iter().map(|s| s.as_ptr()).collect();
    let icons: Vec<_> = apps.iter().map(|(_, _, icon)| icon.as_ptr()).collect();
    let lengths: Vec<_> = apps.iter().map(|(_, _, icon)| icon.len()).collect();
    unsafe {
        craft_dock_set_apps(
            id_ptrs.as_ptr(),
            title_ptrs.as_ptr(),
            icons.as_ptr(),
            lengths.as_ptr(),
            apps.len(),
        );
    }
}
/// Shows the number of available updates on the Dock icon, or clears it at zero.
pub fn set_badge(updates: usize) {
    let label = CString::new(if updates == 0 {
        String::new()
    } else {
        updates.to_string()
    })
    .unwrap_or_default();
    unsafe { craft_dock_set_badge(label.as_ptr()) }
}

extern "C" {
    fn craft_choose_folder(message: *const c_char, initial: *const c_char) -> *mut c_char;
}
/// Shows the macOS folder picker and returns the chosen folder, or None if cancelled.
pub fn choose_folder(message: &str, initial: &std::path::Path) -> Option<std::path::PathBuf> {
    let message = CString::new(message).ok()?;
    let initial = CString::new(initial.to_string_lossy().as_bytes()).ok()?;
    let chosen = unsafe { craft_choose_folder(message.as_ptr(), initial.as_ptr()) };
    if chosen.is_null() {
        return None;
    }
    let path = unsafe { CStr::from_ptr(chosen) }
        .to_string_lossy()
        .into_owned();
    unsafe { libc::free(chosen.cast()) };
    Some(path.into())
}

extern "C" {
    fn craft_status_set(on: bool);
    fn craft_login_enabled() -> bool;
    fn craft_login_set(on: bool) -> *mut c_char;
}
/// Shows or hides the Craft Library menu in the menu bar.
pub fn set_menu_bar(on: bool) {
    unsafe { craft_status_set(on) }
}
/// Whether Craft Library opens when you log in.
pub fn login_enabled() -> bool {
    unsafe { craft_login_enabled() }
}
pub fn set_login(on: bool) -> anyhow::Result<()> {
    let error = unsafe { craft_login_set(on) };
    if error.is_null() {
        return Ok(());
    }
    let message = unsafe { CStr::from_ptr(error) }
        .to_string_lossy()
        .into_owned();
    unsafe { libc::free(error.cast()) };
    anyhow::bail!(message)
}
