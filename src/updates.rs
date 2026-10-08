use crate::{
    backups, files,
    jobs::Job,
    model::{Asset, Paths, Preferences, Release, Source},
    network::Network,
    platform,
};
use anyhow::{bail, Context, Result};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};
pub fn version(v: &str) -> Result<(u64, u64, u64)> {
    let nums: Vec<_> = v
        .trim_start_matches('v')
        .split('.')
        .map(str::parse::<u64>)
        .collect::<std::result::Result<_, _>>()?;
    if nums.len() != 3 {
        bail!("Unsupported version: {v}");
    }
    Ok((nums[0], nums[1], nums[2]))
}
pub fn release_version(app: &str, tag: &str) -> Result<String> {
    crate::model::valid_app(app)?;
    let value = if app == "artcraft" {
        tag.strip_prefix("artcraft-").unwrap_or(tag)
    } else {
        tag
    };
    let value = value.trim_start_matches('v');
    version(value)?;
    Ok(value.into())
}
pub fn check_app(paths: &Paths, app: &str) -> Result<Option<String>> {
    crate::model::valid_app(app)?;
    let installed = crate::apps::installed(paths, app)?;
    let release: Release = Network::new(&paths.root)?.json(&format!(
        "https://api.github.com/repos/storytold/{}/releases/latest",
        crate::model::repository(app)
    ))?;
    if release.draft || release.prerelease {
        bail!("No stable release available");
    }
    if version(&release_version(app, &release.tag_name)?)? > version(&installed.version)? {
        select_asset(&release, app, &paths.preferences()?)?;
        Ok(Some(release_version(app, &release.tag_name)?))
    } else {
        Ok(None)
    }
}
pub fn installed_check_targets(config: &crate::model::Config) -> Vec<crate::model::Installed> {
    config
        .apps
        .iter()
        .filter(|app| {
            crate::model::APPS.contains(&app.name.as_str())
                && !app.path.is_empty()
                && !app.version.is_empty()
        })
        .cloned()
        .collect()
}
pub fn select_asset<'a>(release: &'a Release, app: &str, prefs: &Preferences) -> Result<&'a Asset> {
    let asset = crate::macos_build::release_asset(Some(release), app, prefs)?
        .context("No compatible Mac release asset")?;
    release
        .assets
        .iter()
        .find(|a| a.name == asset.name)
        .context("Missing release asset")
}
fn backup_path(paths: &Paths, name: &str, v: &str, source: bool) -> PathBuf {
    paths
        .at(if source {
            "backups/sources"
        } else {
            "backups/releases"
        })
        .join(if source {
            format!(
                "{name}-source-{}-{}.zip",
                &v[..7],
                uuid::Uuid::new_v4().simple()
            )
        } else {
            format!("{name}-{v}-{}", uuid::Uuid::new_v4().simple())
        })
}
// Commit the filesystem and metadata together. Restore the old copy if metadata cannot be saved.
pub fn replace_transaction(
    staged: &Path,
    destination: &Path,
    backup: Option<&Path>,
    commit: impl FnOnce() -> Result<()>,
) -> Result<()> {
    if destination.exists() {
        let b = backup.context("Missing rollback path")?;
        fs::create_dir_all(b.parent().unwrap())?;
        fs::rename(destination, b)?;
    }
    fs::create_dir_all(destination.parent().unwrap())?;
    if let Err(e) = fs::rename(staged, destination) {
        if let Some(b) = backup {
            if b.exists() {
                fs::rename(b, destination).context("Could not restore rollback copy")?;
            }
        }
        return Err(e.into());
    }
    if let Err(e) = commit() {
        fs::rename(destination, staged).context("Could not remove uncommitted replacement")?;
        if let Some(b) = backup {
            if b.exists() {
                fs::rename(b, destination).context("Could not restore previous version")?;
            }
        }
        return Err(e);
    }
    Ok(())
}
pub fn releases(paths: &Paths, job: &Job, background: bool) -> Result<()> {
    if background {
        crate::hourly::run(paths, job)
    } else {
        crate::macos_build::install_selected(paths, job)
    }
}
pub fn install_app(paths: &Paths, app: &str, job: &Job) -> Result<()> {
    crate::macos_build::install_latest(paths, app, job)
}
pub fn sources(paths: &Paths, names: &[String], job: &Job) -> Result<()> {
    let _lock = platform::Lock::take("Local\\CraftAppsManager")?;
    job.log(&format!(
        "\nSource updates — {}",
        chrono::Utc::now().to_rfc3339()
    ));
    let prefs = paths.preferences()?;
    let network = Network::new(&paths.root)?;
    let mut index: BTreeMap<String, Source> =
        files::read_or_default(&paths.at("sources/source-index.json"))?;
    let mut errors = 0;
    for name in names {
        crate::model::valid_app(name)?;
        job.check()?;
        job.stage("Checking source", None, crate::model::title(name));
        let result = (|| -> Result<()> {
            let repository = crate::model::repository(name);
            let repo: serde_json::Value = network.json(&format!(
                "https://api.github.com/repos/storytold/{repository}"
            ))?;
            let branch = repo["default_branch"]
                .as_str()
                .context("No default branch")?;
            let mut endpoint = reqwest::Url::parse(&format!(
                "https://api.github.com/repos/storytold/{repository}/commits/"
            ))?;
            endpoint
                .path_segments_mut()
                .map_err(|_| anyhow::anyhow!("Invalid API endpoint"))?
                .pop_if_empty()
                .push(branch);
            let commit: serde_json::Value = network.json(endpoint.as_str())?;
            let sha = commit["sha"].as_str().context("No commit")?;
            if sha.len() != 40 || !sha.bytes().all(|b| b.is_ascii_hexdigit()) {
                bail!("Invalid source commit")
            }
            let destination = paths.at(format!("sources/{name}-source.zip"));
            files::inside(&destination, &paths.at("sources"))?;
            let old = index.get(name).cloned();
            if destination.exists() {
                if files::linked(&destination)? {
                    bail!("Linked source archive")
                };
                let old = old
                    .as_ref()
                    .context("Existing ZIP is unmanaged; leaving it intact")?;
                if old.sha.len() != 40 || !old.sha.bytes().all(|b| b.is_ascii_hexdigit()) {
                    bail!("Invalid recorded source commit; leaving the existing ZIP intact");
                }
                if old.sha == sha
                    && files::hash(&destination)?.eq_ignore_ascii_case(&old.archive_sha256)
                {
                    job.log(&format!("{name}: source up to date"));
                    return Ok(());
                }
            }
            let archive = paths.at(format!("runtime/downloads/{name}-{sha}.zip"));
            network.download(
                &format!("https://codeload.github.com/storytold/{repository}/zip/{sha}"),
                &archive,
                job,
            )?;
            files::verify_source(&archive, name, sha)?;
            let hash = files::hash(&archive)?;
            let backup = old
                .as_ref()
                .filter(|_| destination.exists())
                .map(|o| backup_path(paths, name, &o.sha, true));
            index.insert(
                name.clone(),
                Source {
                    sha: sha.into(),
                    branch: branch.into(),
                    repository: format!("storytold/{repository}"),
                    archive_sha256: hash,
                    downloaded_at: chrono::Utc::now().to_rfc3339(),
                },
            );
            if let Err(e) = replace_transaction(&archive, &destination, backup.as_deref(), || {
                files::write_json(&paths.at("sources/source-index.json"), &index)
            }) {
                if let Some(old) = old {
                    index.insert(name.clone(), old);
                } else {
                    index.remove(name);
                }
                return Err(e);
            }
            if let Err(e) = backups::finish(paths, &prefs, backup.as_deref(), name, true, job) {
                job.log(&format!("Source backup warning: {e}"))
            }
            job.log(&format!("{name}: source updated ({})", &sha[..7]));
            Ok(())
        })();
        if let Err(e) = result {
            errors += 1;
            job.log(&format!("{name}: {e:#}"));
            if e.to_string().contains("API limit")
                || job.cancel.load(std::sync::atomic::Ordering::Relaxed)
            {
                break;
            }
        }
    }
    if errors > 0 {
        bail!("{errors} source update(s) failed; see the log");
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn new_apps_select_matching_release_formats_and_reject_missing_architectures() {
        for app in [
            "wordcraft",
            "gridcraft",
            "deckcraft",
            "cadcraft",
            "soundcraft",
        ] {
            for format in ["portable", "installer"] {
                let preferences = Preferences {
                    architecture: "x64".into(),
                    release_format: format.into(),
                    ..Default::default()
                };
                let suffix = ".dmg";
                let name = format!(
                    "{app}-0.3.0-{}-{}{suffix}",
                    crate::model::release_os(),
                    crate::model::release_arch("x64")
                );
                let release = Release {
                    tag_name: "v0.3.0".into(),
                    draft: false,
                    prerelease: false,
                    assets: vec![crate::model::Asset {
                        name: name.clone(),
                        size: 1,
                        digest: None,
                        browser_download_url: format!(
                            "https://github.com/storytold/{app}/releases/download/v0.3.0/{name}"
                        ),
                    }],
                };
                assert_eq!(
                    select_asset(&release, app, &preferences).unwrap().name,
                    name
                );
            }
        }
    }
    #[test]
    fn rollback() {
        let root = std::env::temp_dir().join(uuid::Uuid::new_v4().to_string());
        fs::create_dir_all(&root).unwrap();
        let dest = root.join("app");
        let stage = root.join("new");
        let backup = root.join("backup");
        fs::write(&dest, "old").unwrap();
        fs::write(&stage, "new").unwrap();
        assert!(
            replace_transaction(&stage, &dest, Some(&backup), || bail!("metadata failure"))
                .is_err()
        );
        assert_eq!(fs::read_to_string(&dest).unwrap(), "old");
        assert_eq!(fs::read_to_string(&stage).unwrap(), "new");
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn versions() {
        assert!(version("v0.10.0").unwrap() > version("0.9.0").unwrap());
        assert!(version("1.2.3-beta").is_err());
    }
}
