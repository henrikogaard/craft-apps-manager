//! Build entry point and history for native macOS app bundles.
use crate::{
    files,
    jobs::Job,
    model::{BuildInfo, Paths, SOURCES},
    platform,
};
use anyhow::{bail, Context, Result};
use std::{collections::BTreeMap, fs, path::PathBuf, process::Command};
pub fn build(paths: &Paths, app: &str, latest: bool, job: &Job) -> Result<()> {
    crate::model::valid_app(app)?;
    crate::macos_build::build(paths, app, latest, job)
}
/// Launch arguments for local builds, kept apart from the installed app's settings.
pub fn launch_options(paths: &Paths, app: &str) -> Result<crate::apps::LaunchSettings> {
    crate::model::valid_app(app)?;
    let all: BTreeMap<String, crate::apps::LaunchSettings> =
        files::read_or_default(&paths.at("runtime/build-launch-settings.json"))?;
    Ok(all.get(app).cloned().unwrap_or_default())
}
pub fn save_launch_options(
    paths: &Paths,
    app: &str,
    value: &crate::apps::LaunchSettings,
) -> Result<()> {
    crate::model::valid_app(app)?;
    if !value.executable.is_empty() {
        bail!("Local builds use their compiled app bundle.");
    }
    let file = paths.at("runtime/build-launch-settings.json");
    let mut all: BTreeMap<String, crate::apps::LaunchSettings> = files::read_or_default(&file)?;
    all.insert(app.to_owned(), value.clone());
    files::write_json(&file, &all)
}
/// Opens the newest completed build without installing it.
pub fn launch_local(paths: &Paths, app: &str) -> Result<()> {
    crate::model::valid_app(app)?;
    let folder =
        history(paths, app).context("No completed local build was found. Build the app first.")?;
    let bundle = folder.join(crate::model::build_executable_name(app));
    files::inside(&bundle, &paths.at(format!("builds/{app}")))?;
    if !bundle.is_dir() || files::linked(&bundle)? {
        bail!("The built app is missing. Rebuild the app first.");
    }
    Command::new("/usr/bin/open")
        .arg("-n")
        .arg("-a")
        .arg(&bundle)
        .arg("--args")
        .args(launch_options(paths, app)?.arguments)
        .current_dir(&folder)
        .spawn()
        .with_context(|| {
            format!(
                "Could not open the local build of {}",
                crate::model::title(app)
            )
        })?;
    Ok(())
}
/// Removes one completed build folder, keeping other builds, sources and installed apps.
pub fn delete_local(paths: &Paths, app: &str, folder: &std::path::Path) -> Result<()> {
    crate::model::valid_app(app)?;
    let _lock = platform::Lock::take("Local\\CraftAppsSourceBuilder")?;
    let root = paths.at(format!("builds/{app}"));
    files::inside(folder, &root)?;
    if folder.parent() != Some(root.as_path()) || files::linked(folder)? {
        bail!("Only a completed build folder can be deleted here.");
    }
    let info: BuildInfo = files::read_json(&folder.join("build-info.json"))?;
    if info.app != app {
        bail!("The build belongs to another app.");
    }
    if platform::running_app(app)? {
        bail!("Close the app before deleting its local build.");
    }
    files::remove_managed(folder, &root)
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

#[cfg(test)]
mod local_build_tests {
    use super::*;
    #[test]
    fn build_launch_options_are_persisted_separately_from_release_options() {
        let temp =
            std::env::temp_dir().join(format!("craft-build-options-{}", uuid::Uuid::new_v4()));
        let paths = Paths::new(temp.clone(), None);
        let release = crate::apps::LaunchSettings {
            executable: String::new(),
            arguments: vec!["--release".into()],
        };
        crate::apps::save(&paths, "filmcraft", &release).unwrap();
        let build = crate::apps::LaunchSettings {
            executable: String::new(),
            arguments: vec!["--build".into(), "a value with spaces".into()],
        };
        save_launch_options(&paths, "filmcraft", &build).unwrap();
        assert_eq!(
            launch_options(&paths, "filmcraft").unwrap().arguments,
            build.arguments
        );
        assert_eq!(
            crate::apps::settings(&paths, "filmcraft")
                .unwrap()
                .arguments,
            release.arguments
        );
        let invalid = crate::apps::LaunchSettings {
            executable: "../Other.app".into(),
            arguments: vec![],
        };
        assert!(save_launch_options(&paths, "filmcraft", &invalid).is_err());
        fs::remove_dir_all(temp).unwrap();
    }
    #[test]
    fn local_build_actions_reject_missing_and_unrelated_outputs() {
        let temp =
            std::env::temp_dir().join(format!("craft-build-actions-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&temp).unwrap();
        let paths = Paths::new(temp.join("library"), None);
        let outside = temp.join("unrelated");
        fs::create_dir_all(&outside).unwrap();
        fs::write(outside.join("keep.txt"), "keep").unwrap();
        assert!(launch_local(&paths, "filmcraft").is_err());
        assert!(delete_local(&paths, "filmcraft", &outside).is_err());
        let wrong = paths.at("builds/filmcraft/wrong");
        fs::create_dir_all(&wrong).unwrap();
        files::write_json(
            &wrong.join("build-info.json"),
            &BuildInfo {
                app: "soundcraft".into(),
                commit: "abcdef".into(),
                source_branch: "main".into(),
                built_at: chrono::Utc::now().to_rfc3339(),
                profile: "release".into(),
                log: String::new(),
            },
        )
        .unwrap();
        assert!(delete_local(&paths, "filmcraft", &wrong).is_err());
        assert!(wrong.exists());
        assert!(outside.join("keep.txt").exists());
        fs::remove_dir_all(temp).unwrap();
    }
}
