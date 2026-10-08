//! Native application menu; the content UI remains local and monochrome.
use anyhow::Result;
use muda::{
    accelerator::{Accelerator, Code, Modifiers},
    AboutMetadata, Menu, MenuItem, PredefinedMenuItem as Item, Submenu,
};

pub fn create() -> Result<Menu> {
    let pixels = image::load_from_memory(include_bytes!("../assets/icon.png"))?.into_rgba8();
    let icon = muda::Icon::from_rgba(pixels.clone().into_raw(), pixels.width(), pixels.height())?;
    let menu = Menu::new();
    let application = Submenu::new("Craft Library", true);
    application.append_items(&[
        &Item::about(
            Some("About Craft Library"),
            Some(AboutMetadata {
                name: Some("Craft Library".into()),
                version: Some(env!("CARGO_PKG_VERSION").into()),
                copyright: Some("Henrik Øgård".into()),
                short_version: Some(String::new()),
                icon: Some(icon),
                ..Default::default()
            }),
        ),
        &Item::separator(),
        &MenuItem::with_id("updates", "Check for Updates…", true, None),
        &Item::services(None),
        &Item::separator(),
        &Item::hide(None),
        &Item::hide_others(None),
        &Item::show_all(None),
        &Item::separator(),
        &MenuItem::with_id(
            "quit",
            "Quit Craft Library",
            true,
            Some(Accelerator::new(Modifiers::META, Code::KeyQ)),
        ),
    ])?;
    let edit = Submenu::new("Edit", true);
    edit.append_items(&[
        &Item::undo(None),
        &Item::redo(None),
        &Item::separator(),
        &Item::cut(None),
        &Item::copy(None),
        &Item::paste(None),
        &Item::select_all(None),
    ])?;
    let window = Submenu::new("Window", true);
    window.append_items(&[
        &Item::minimize(None),
        &Item::zoom(None),
        &Item::fullscreen(None),
        &Item::separator(),
        &Item::close_window(None),
    ])?;
    menu.append_items(&[&application, &edit, &window])?;
    menu.init_for_nsapp();
    Ok(menu)
}
