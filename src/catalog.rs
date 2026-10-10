//! Craft apps Storytold publishes after this release of Craft Library.
//!
//! The 14 known apps are built in (`model::APPS`). Every few hours Craft Library looks at the
//! storytold organization for other `<name>craft` repositories whose latest release has a Mac
//! DMG, so a new app shows up without a Craft Library update. Discovered apps can be
//! installed, updated, opened and uninstalled; source builds need a recipe, so they can't be
//! built from source.
use crate::{files, model::Paths, network::Network};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::RwLock;

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct App {
    pub id: String,
    pub title: String,
    pub description: String,
}
#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
struct Cache {
    checked: i64,
    apps: Vec<App>,
}
const FILE: &str = "runtime/catalog.json";
/// How long a lookup stays fresh.
pub const EVERY: i64 = 6 * 3600;

static DISCOVERED: RwLock<Vec<App>> = RwLock::new(Vec::new());

/// Discovered apps known to this process.
pub fn apps() -> Vec<App> {
    DISCOVERED.read().map(|a| a.clone()).unwrap_or_default()
}
pub fn contains(id: &str) -> bool {
    DISCOVERED
        .read()
        .is_ok_and(|a| a.iter().any(|app| app.id == id))
}
fn set(apps: Vec<App>) {
    if let Ok(mut current) = DISCOVERED.write() {
        *current = apps;
    }
}
/// Loads the last lookup so discovered apps are valid from the start.
pub fn load(paths: &Paths) {
    let cache: Cache = files::read_or_default(&paths.at(FILE)).unwrap_or_default();
    set(cache.apps.into_iter().filter(|a| valid_id(&a.id)).collect());
}

/// A plain lowercase `<name>craft` repository name. Ids end up in paths and URLs.
fn valid_id(id: &str) -> bool {
    id.len() <= 40
        && id.len() > "craft".len()
        && id.ends_with("craft")
        && id
            .bytes()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        && id.as_bytes()[0].is_ascii_lowercase()
}
fn known(id: &str) -> bool {
    crate::model::APPS
        .iter()
        .any(|app| *app == id || crate::model::repository(app) == id)
}

/// Looks for new apps when the last lookup is older than `EVERY`, or always with `force`.
/// Returns apps found for the first time.
pub fn refresh(paths: &Paths, force: bool) -> Result<Vec<App>> {
    let file = paths.at(FILE);
    let cache: Cache = files::read_or_default(&file).unwrap_or_default();
    let now = chrono::Utc::now().timestamp();
    if !force && now - cache.checked < EVERY {
        set(cache.apps.into_iter().filter(|a| valid_id(&a.id)).collect());
        return Ok(Vec::new());
    }
    let network = Network::new(&paths.root)?;
    let mut repos = Vec::new();
    for page in 1..=3 {
        let batch: Vec<Value> = network.json(&format!(
            "https://api.github.com/orgs/storytold/repos?per_page=100&type=public&page={page}"
        ))?;
        let full = batch.len() == 100;
        repos.extend(batch);
        if !full {
            break;
        }
    }
    let mut found = Vec::new();
    for repo in repos {
        let id = repo["name"].as_str().unwrap_or_default();
        if !valid_id(id) || known(id) || repo["archived"].as_bool() == Some(true) {
            continue;
        }
        // Only apps with a Mac release can be installed here.
        let Ok(release) = network.json::<Value>(&format!(
            "https://api.github.com/repos/storytold/{id}/releases/latest"
        )) else {
            continue;
        };
        let mac = release["assets"].as_array().is_some_and(|assets| {
            assets.iter().any(|a| {
                a["name"].as_str().is_some_and(|n| {
                    n.starts_with(&format!("{id}-")) && n.contains("-macos-") && n.ends_with(".dmg")
                })
            })
        });
        if mac && release["draft"] != true && release["prerelease"] != true {
            found.push(App {
                id: id.to_owned(),
                title: crate::model::title(id),
                description: repo["description"]
                    .as_str()
                    .unwrap_or_default()
                    .chars()
                    .take(160)
                    .collect(),
            });
        }
    }
    found.sort_by(|a, b| a.id.cmp(&b.id));
    let new: Vec<_> = found
        .iter()
        .filter(|app| !cache.apps.iter().any(|old| old.id == app.id))
        .cloned()
        .collect();
    files::write_json(
        &file,
        &Cache {
            checked: now,
            apps: found.clone(),
        },
    )?;
    set(found);
    Ok(new)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_plain_unknown_craft_names_count_as_new_apps() {
        assert!(valid_id("mapcraft"));
        assert!(valid_id("map3dcraft"));
        for bad in [
            "craft",
            "MapCraft",
            "map-craft",
            "../craft",
            "3dcraft",
            "craft-fonts",
        ] {
            assert!(!valid_id(bad), "{bad}");
        }
        // Known apps, including PDFCraft's repository name, are never "discovered".
        assert!(known("wordcraft") && known("pdfcraft") && known("printcraft"));
        assert!(!known("mapcraft"));
    }
    #[test]
    fn cached_lookups_load_and_validate() {
        let root = std::env::temp_dir().join(format!("craft-catalog-{}", uuid::Uuid::new_v4()));
        let paths = Paths::new(root.clone(), None);
        files::write_json(
            &paths.at(FILE),
            &Cache {
                checked: chrono::Utc::now().timestamp(),
                apps: vec![
                    App {
                        id: "mapcraft".into(),
                        title: "MapCraft".into(),
                        description: String::new(),
                    },
                    App {
                        id: "../evil".into(),
                        ..Default::default()
                    },
                ],
            },
        )
        .unwrap();
        load(&paths);
        assert!(contains("mapcraft") && !contains("../evil"));
        assert!(crate::model::valid_app("mapcraft").is_ok());
        assert!(
            refresh(&paths, false).unwrap().is_empty(),
            "fresh cache, no network"
        );
        set(Vec::new());
        assert!(crate::model::valid_app("mapcraft").is_err());
        std::fs::remove_dir_all(root).unwrap();
    }
}
