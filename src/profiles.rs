//! App data a Craft app leaves in the home folder, for "uninstall and delete app data".
//!
//! Each list was read from the app's own source code. It names exact files and folders, never a
//! whole Application Support folder, so user-made work stays: PdfCraft's Digital IDs (signing
//! keys), VectorCraft swatches and graphic styles, PhotoCraft and SoundCraft presets, EffectCraft
//! plug-ins, FilmCraft presets and shortcuts, LightCraft's photo library and ArtCraft's assets,
//! downloads and sign-in. Recovery and autosave folders can hold unsaved work, so they are a
//! separate choice.
use anyhow::{bail, Context, Result};
use std::{
    fs,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug)]
pub struct Item {
    pub path: PathBuf,
    /// Recovery or autosave data, which can be the only copy of unsaved work.
    pub recovery: bool,
}

const AS: &str = "Library/Application Support";

/// Settings, caches and logs (`false`) and recovery data (`true`) for `app`, relative to home.
fn known(app: &str) -> Vec<(String, bool)> {
    let support = |names: &[&str]| -> Vec<(String, bool)> {
        names.iter().map(|n| (format!("{AS}/{n}"), false)).collect()
    };
    let recovery = |name: &str| (format!("{AS}/{name}"), true);
    let mut items = match app {
        "designcraft" => {
            let mut v = support(&["DesignCraft/ui.json", "DesignCraft/prefs.json"]);
            v.push(recovery("DesignCraft/Recovery"));
            v
        }
        "deckcraft" => {
            let mut v = support(&["DeckCraft/ui.json", "DeckCraft/prefs.json"]);
            v.push(recovery("DeckCraft/Recovery"));
            v
        }
        "gridcraft" => support(&["GridCraft/ui.json", "GridCraft/prefs.json"]),
        "wordcraft" => support(&["WordCraft/ui.json"]),
        "photocraft" => {
            let mut v = support(&[
                "Photocraft/preferences.json",
                "Photocraft/ui.ron",
                "Photocraft/logs",
                "Photocraft/gpu-starting.json",
            ]);
            v.push(recovery("Photocraft/Recovery"));
            v
        }
        "printcraft" => {
            let mut v = support(&[
                "PdfCraft/app.ron",
                "PdfCraft/logs",
                "PrintCraft/app.ron",
                "PrintCraft/logs",
            ]);
            v.push(recovery("PdfCraft/Recovery"));
            v.push(recovery("PrintCraft/Recovery"));
            v
        }
        "vectorcraft" => {
            let mut v = support(&[
                "VectorCraft/ui.json",
                "VectorCraft/logs",
                "DrawCraft/ui.json",
            ]);
            v.push(recovery("VectorCraft/Data Recovery"));
            v
        }
        "effectcraft" => {
            let mut v = support(&[
                "EffectCraft/prefs.json",
                "EffectCraft/shortcuts.json",
                "EffectCraft/launch-pending",
                "EffectCraft/session.lock",
                "EffectCraft/models",
                "EffectCraft/Logs",
            ]);
            v.push(("Library/Caches/EffectCraft".into(), false));
            v.push(recovery("EffectCraft/EffectCraft Auto-Save"));
            v
        }
        "filmcraft" => {
            let mut v = support(&[
                "FilmCraft/preferences.json",
                "FilmCraft/workspaces.json",
                "FilmCraft/Logs",
                "FilmCraft/models",
                "FilmCraft/Media Cache",
            ]);
            v.push(recovery("FilmCraft/Recovery"));
            v.push(recovery("FilmCraft/Auto-Save"));
            v
        }
        "lightcraft" => support(&[
            "LightCraft/ui.json",
            "LightCraft/gpu-init.marker",
            "LightCraft/logs",
            "LightCraft/models",
            "LightCraft/denoise-models",
        ]),
        "soundcraft" => {
            let mut v = support(&["SoundCraft/ui.json"]);
            v.push(recovery("SoundCraft/Autosave"));
            v
        }
        "artcraft" => ["settings", "state", "temp"]
            .iter()
            .map(|n| (format!("Artcraft/{n}"), false))
            .collect(),
        "artcraftx" => ["settings", "state", "cache", "temp"]
            .iter()
            .map(|n| (format!("Artcraft/artcraftx/{n}"), false))
            .collect(),
        _ => Vec::new(),
    };
    // Files macOS and WebKit keep per bundle identifier.
    for id in crate::macos_build::identities(app) {
        for place in [
            format!("Library/Preferences/{id}.plist"),
            format!("Library/Saved Application State/{id}.savedState"),
            format!("Library/HTTPStorages/{id}"),
            format!("Library/Caches/{id}"),
            format!("Library/WebKit/{id}"),
            format!("Library/Logs/{id}"),
        ] {
            items.push((place, false));
        }
        if app.starts_with("artcraft") {
            items.push((format!("{AS}/{id}"), false));
        }
    }
    items
}

/// The items that exist for `app` under `home`. Links are skipped, never followed.
pub fn existing(home: &Path, app: &str) -> Vec<Item> {
    let mut items: Vec<Item> = known(app)
        .into_iter()
        .map(|(rel, recovery)| Item {
            path: home.join(rel),
            recovery,
        })
        .filter(|item| fs::symlink_metadata(&item.path).is_ok_and(|m| !m.file_type().is_symlink()))
        .collect();
    // An app's two bundle identifiers can be the same name.
    let mut seen = std::collections::BTreeSet::new();
    items.retain(|item| seen.insert(item.path.clone()));
    // ArtCraft debug logs sit loose in their folder.
    let logs = match app {
        "artcraft" => Some((home.join("Artcraft"), "artcraft_debug")),
        "artcraftx" => Some((home.join("Artcraft/artcraftx"), "artcraftx_debug")),
        _ => None,
    };
    if let Some((folder, prefix)) = logs {
        for entry in fs::read_dir(folder).into_iter().flatten().flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with(prefix)
                && name.ends_with(".log")
                && entry.file_type().is_ok_and(|t| t.is_file())
            {
                items.push(Item {
                    path: entry.path(),
                    recovery: false,
                });
            }
        }
    }
    items
}

/// Deletes the given items, recovery ones only when asked. Returns what was deleted.
pub fn remove(home: &Path, items: &[Item], recovery: bool) -> Result<Vec<PathBuf>> {
    let mut removed = Vec::new();
    for item in items.iter().filter(|i| recovery || !i.recovery) {
        let path = &item.path;
        // At least Library/<area>/<name> or Artcraft/<name>: never a shared parent folder.
        let depth = path
            .strip_prefix(home)
            .map(|rest| rest.components().count())
            .unwrap_or(0);
        let safe = (path.starts_with(home.join("Library")) && depth >= 3)
            || (path.starts_with(home.join("Artcraft")) && depth >= 2);
        if !safe {
            bail!("Refusing to delete {}", path.display());
        }
        let meta = match fs::symlink_metadata(path) {
            Ok(meta) => meta,
            Err(_) => continue,
        };
        if meta.file_type().is_symlink() {
            continue;
        }
        if meta.is_dir() {
            fs::remove_dir_all(path)
        } else {
            fs::remove_file(path)
        }
        .with_context(|| format!("Could not delete {}", path.display()))?;
        removed.push(path.clone());
    }
    // Remove an app's own folder once nothing is left in it (remove_dir fails if not empty).
    for path in &removed {
        if let Some(parent) = path.parent() {
            let depth = parent
                .strip_prefix(home)
                .map_or(0, |p| p.components().count());
            if depth >= 3 || (parent.starts_with(home.join("Artcraft")) && depth >= 2) {
                let _ = fs::remove_dir(parent);
            }
        }
    }
    Ok(removed)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn app_data_removal_keeps_user_work_and_recovery_unless_asked() {
        let home = std::env::temp_dir().join(format!("craft-home-{}", uuid::Uuid::new_v4()));
        let pdf = home.join(AS).join("PdfCraft");
        fs::create_dir_all(pdf.join("logs")).unwrap();
        fs::create_dir_all(pdf.join("Recovery")).unwrap();
        fs::create_dir_all(pdf.join("Digital IDs")).unwrap();
        fs::write(pdf.join("app.ron"), "settings").unwrap();
        fs::write(pdf.join("Digital IDs/me.p12"), "key").unwrap();
        let items = existing(&home, "printcraft");
        assert!(items
            .iter()
            .any(|i| i.path == pdf.join("app.ron") && !i.recovery));
        assert!(items
            .iter()
            .any(|i| i.path == pdf.join("Recovery") && i.recovery));
        assert!(!items
            .iter()
            .any(|i| i.path.starts_with(pdf.join("Digital IDs"))));
        let removed = remove(&home, &items, false).unwrap();
        assert!(removed.contains(&pdf.join("app.ron")));
        assert!(!pdf.join("app.ron").exists() && !pdf.join("logs").exists());
        assert!(
            pdf.join("Recovery").exists(),
            "recovery needs its own choice"
        );
        assert!(
            pdf.join("Digital IDs/me.p12").exists(),
            "signing keys are user data"
        );
        let word = home.join(AS).join("WordCraft");
        fs::create_dir_all(&word).unwrap();
        fs::write(word.join("ui.json"), "{}").unwrap();
        remove(&home, &existing(&home, "wordcraft"), false).unwrap();
        assert!(!word.exists(), "an emptied app folder goes too");
        remove(&home, &items, true).unwrap();
        assert!(!pdf.join("Recovery").exists());
        // Only exact app paths inside the home Library or Artcraft folder can be removed.
        let outside = Item {
            path: home.join("Documents"),
            recovery: false,
        };
        fs::create_dir_all(&outside.path).unwrap();
        assert!(remove(&home, &[outside], true).is_err());
        let whole = Item {
            path: home.join(AS),
            recovery: false,
        };
        assert!(remove(&home, &[whole], true).is_err());
        // ArtCraft's shared folder and its user media stay; only its own data goes.
        let art = home.join("Artcraft");
        fs::create_dir_all(art.join("settings")).unwrap();
        fs::create_dir_all(art.join("assets")).unwrap();
        fs::create_dir_all(art.join("artcraftx/settings")).unwrap();
        fs::write(art.join("artcraft_debug.1.log"), "log").unwrap();
        remove(&home, &existing(&home, "artcraft"), true).unwrap();
        assert!(!art.join("settings").exists() && !art.join("artcraft_debug.1.log").exists());
        assert!(art.join("assets").exists() && art.join("artcraftx/settings").exists());
        fs::remove_dir_all(home).unwrap();
    }
}
