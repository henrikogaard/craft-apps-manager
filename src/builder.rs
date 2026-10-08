//! Build entry point and history for native macOS app bundles.
use crate::{
    files,
    jobs::Job,
    model::{BuildInfo, Paths, SOURCES},
    platform,
};
use anyhow::Result;
use std::{fs, path::PathBuf};
pub fn build(paths: &Paths, app: &str, latest: bool, job: &Job) -> Result<()> {
    crate::model::valid_app(app)?;
    crate::macos_build::build(paths, app, latest, job)
}
pub fn history(paths: &Paths, app: &str) -> Option<PathBuf> {
    let root = paths.at(format!("builds/{app}"));
    let mut builds: Vec<_> = fs::read_dir(root)
        .ok()?
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path()
                .join(crate::model::build_executable_name(app))
                .exists()
        })
        .filter_map(|e| {
            if files::linked(&e.path()).ok()? {
                return None;
            }
            let info: BuildInfo = files::read_json(&e.path().join("build-info.json")).ok()?;
            if info.app != app {
                return None;
            }
            let time = chrono::DateTime::parse_from_rfc3339(&info.built_at).ok()?;
            Some((time, e.path()))
        })
        .collect();
    builds.sort_by_key(|v| v.0);
    builds.pop().map(|v| v.1)
}
pub fn clean(paths: &Paths) -> Result<()> {
    let _lock = platform::Lock::take("Local\\CraftAppsSourceBuilder")?;
    let workspace = paths.at("workspace");
    for name in SOURCES.into_iter().chain(["cache", "cache-previous"]) {
        files::remove_managed(&workspace.join(name), &workspace)?;
    }
    Ok(())
}
