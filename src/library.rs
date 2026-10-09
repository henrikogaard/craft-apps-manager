//! Moving the library folder (downloads, sources, builds, backups and logs) somewhere else.
use crate::{
    jobs::Job,
    model::{Locations, Paths},
    platform, scheduler,
};
use anyhow::{bail, Context, Result};
use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

/// Space used by each part of the library, largest first. Links are not followed.
pub fn usage(paths: &Paths) -> Vec<(&'static str, u64)> {
    let mut parts: Vec<_> = [
        "workspace",
        "builds",
        "backups",
        "releases",
        "sources",
        "logs",
    ]
    .into_iter()
    .map(|name| (name, size(&paths.at(name))))
    .collect();
    parts.sort_by_key(|(_, bytes)| std::cmp::Reverse(*bytes));
    parts
}
fn size(folder: &Path) -> u64 {
    walkdir::WalkDir::new(folder)
        .follow_links(false)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file())
        .filter_map(|e| e.metadata().ok())
        .map(|m| m.len())
        .sum()
}
/// Deletes downloaded installers. They are fetched again when needed.
pub fn clear_downloads(paths: &Paths) -> Result<()> {
    let _lock = platform::Lock::take("Local\\CraftAppsManager")?;
    let releases = paths.at("releases");
    for folder in [releases.join("installers"), paths.at("runtime/downloads")] {
        crate::files::remove_managed(&folder, &paths.root)?;
    }
    Ok(())
}

/// Lives in the default library folder and points at the real one, so it never moves.
const POINTER: &str = "data-root.json";

/// Refuses folders that would nest the library in itself or mix it with other files.
pub fn check_target(from: &Path, to: &Path) -> Result<()> {
    if !to.is_absolute() {
        bail!("Choose an absolute folder");
    }
    if to == from {
        bail!("That is already the library folder");
    }
    if to.starts_with(from) || from.starts_with(to) {
        bail!("The new library folder can't be inside the current one, or contain it");
    }
    if fs::symlink_metadata(to).is_ok_and(|m| m.file_type().is_symlink()) {
        bail!("{} is a link; choose the real folder", to.display());
    }
    if to.exists() {
        let used = fs::read_dir(to)?
            .filter_map(Result::ok)
            .any(|e| !e.file_name().to_string_lossy().starts_with('.'));
        if used {
            bail!(
                "Choose an empty folder. {} already has files in it.",
                to.display()
            );
        }
    }
    Ok(())
}

/// Moves the library to `to`, points Craft Library at it and re-registers scheduled checks.
/// Craft Library must restart afterwards, because the running window still holds the old paths.
pub fn move_library(paths: &Paths, to: &Path, job: &Job) -> Result<()> {
    let _locks = [
        "Local\\CraftAppsManager",
        "Local\\CraftAppsSourceBuilder",
        "Local\\CraftAppsManagerHourlyChecks",
        "Local\\CraftAppsManagerHourlySources",
    ]
    .map(platform::Lock::take)
    .into_iter()
    .collect::<Result<Vec<_>>>()
    .context("Finish other Craft Library work before moving the library")?;
    let from = paths.root.clone();
    let skip = (from == Locations::home()?).then_some(POINTER);
    relocate(&from, to, skip, job)?;
    let tools = Locations::read()?.tools.map(|t| remap(&t, &from, to));
    point_to(to, tools)?;
    job.log(&format!("Moved the library to {}", to.display()));
    Ok(())
}

/// Points Craft Library at an existing library folder without moving anything.
pub fn use_library(to: &Path) -> Result<()> {
    if !to.is_absolute() {
        bail!("Choose an absolute folder");
    }
    fs::create_dir_all(to)?;
    point_to(to, Locations::read()?.tools)
}

fn point_to(root: &Path, tools: Option<PathBuf>) -> Result<()> {
    let home = Locations::home()?;
    Locations {
        root: (root != home).then(|| root.to_path_buf()),
        tools: tools.clone(),
    }
    .write()?;
    // Scheduled checks carry the library path on their command line.
    let paths = Paths::new(root.to_path_buf(), tools);
    for source in [false, true] {
        if scheduler::enabled(source) {
            scheduler::set(&paths, source, true)?;
        }
    }
    Ok(())
}

/// Moves every entry of `from` (except `skip`) into `to`, then rewrites saved paths.
/// If a move fails, the entries already moved are put back.
fn relocate(from: &Path, to: &Path, skip: Option<&str>, job: &Job) -> Result<()> {
    check_target(from, to)?;
    fs::create_dir_all(to)?;
    let entries: Vec<_> = fs::read_dir(from)?
        .collect::<std::io::Result<Vec<_>>>()?
        .into_iter()
        .filter(|e| skip.is_none_or(|s| e.file_name() != s))
        .collect();
    let mut moved: Vec<(PathBuf, PathBuf)> = Vec::new();
    for (n, entry) in entries.iter().enumerate() {
        let name = entry.file_name();
        let dest = to.join(&name);
        job.stage(
            "Moving library",
            Some(n as f32 / entries.len().max(1) as f32),
            name.to_string_lossy(),
        );
        if let Err(error) = job.check().and_then(|()| move_entry(&entry.path(), &dest)) {
            for (src, dst) in moved.iter().rev() {
                if let Err(e) = move_entry(dst, src) {
                    job.log(&format!("Could not put back {}: {e:#}", dst.display()));
                }
            }
            return Err(error.context("The library was not moved"));
        }
        moved.push((entry.path(), dest));
    }
    rewrite_paths(to, from, to)
}

/// Rename when possible; across volumes copy with ditto, then remove the original.
fn move_entry(src: &Path, dst: &Path) -> Result<()> {
    if fs::rename(src, dst).is_ok() {
        return Ok(());
    }
    let status = Command::new("/usr/bin/ditto").arg(src).arg(dst).status()?;
    if !status.success() {
        let _ = fs::remove_dir_all(dst);
        bail!("Could not copy {}", src.display());
    }
    if src.is_dir() {
        fs::remove_dir_all(src)
    } else {
        fs::remove_file(src)
    }
    .with_context(|| format!("Copied, but could not remove {}", src.display()))
}

fn remap(path: &Path, from: &Path, to: &Path) -> PathBuf {
    path.strip_prefix(from)
        .map(|rest| to.join(rest))
        .unwrap_or_else(|_| path.to_path_buf())
}

/// Saved records hold absolute paths (portable installs, backups, build logs). Point the ones
/// inside the old library at the new one.
fn rewrite_paths(root: &Path, from: &Path, to: &Path) -> Result<()> {
    let mut files: Vec<PathBuf> = fs::read_dir(root)?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "json"))
        .collect();
    for pattern in ["backups/releases", "builds"] {
        for entry in walkdir::WalkDir::new(root.join(pattern))
            .max_depth(3)
            .into_iter()
            .filter_map(Result::ok)
        {
            let name = entry.file_name();
            if name == "installed-app.json" || name == "build-info.json" {
                files.push(entry.into_path());
            }
        }
    }
    for file in files {
        let Ok(text) = fs::read_to_string(&file) else {
            continue;
        };
        let Ok(mut value) = serde_json::from_str::<Value>(&text) else {
            continue;
        };
        if rewrite(&mut value, from, to) {
            crate::files::write_json(&file, &value)?;
        }
    }
    Ok(())
}

fn rewrite(value: &mut Value, from: &Path, to: &Path) -> bool {
    match value {
        Value::String(s) => {
            let path = Path::new(s.as_str());
            if path.starts_with(from) {
                *s = remap(path, from, to).to_string_lossy().into_owned();
                true
            } else {
                false
            }
        }
        Value::Array(items) => items
            .iter_mut()
            .fold(false, |changed, v| rewrite(v, from, to) | changed),
        Value::Object(map) => map
            .values_mut()
            .fold(false, |changed, v| rewrite(v, from, to) | changed),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn library_moves_with_its_saved_paths_and_keeps_the_pointer_file() {
        let base = std::env::temp_dir().join(format!("craft-library-{}", uuid::Uuid::new_v4()));
        let from = base.join("old");
        let to = base.join("new");
        fs::create_dir_all(from.join("backups/releases/wordcraft-0.2.0-x")).unwrap();
        fs::create_dir_all(from.join("sources")).unwrap();
        fs::write(from.join("sources/wordcraft.zip"), b"zip").unwrap();
        fs::write(from.join(POINTER), b"{}").unwrap();
        let old = from.to_string_lossy();
        fs::write(
            from.join("settings.json"),
            format!(r#"{{"appsRoot":"{old}","apps":[{{"name":"wordcraft","path":"{old}/releases/wordcraft"}},{{"name":"lightcraft","path":"/Applications"}}]}}"#),
        )
        .unwrap();
        fs::write(
            from.join("backups/releases/wordcraft-0.2.0-x/installed-app.json"),
            format!(r#"{{"path":"{old}/releases/wordcraft"}}"#),
        )
        .unwrap();
        let job = Job::new(base.join("job.log"), &Default::default());
        relocate(&from, &to, Some(POINTER), &job).unwrap();
        assert!(to.join("sources/wordcraft.zip").exists());
        assert!(!from.join("sources").exists());
        // The pointer file stays behind in the default folder.
        assert!(from.join(POINTER).exists());
        assert!(!to.join(POINTER).exists());
        let new = to.to_string_lossy();
        let settings = fs::read_to_string(to.join("settings.json")).unwrap();
        assert!(settings.contains(&format!("{new}/releases/wordcraft")));
        assert!(settings.contains(r#""/Applications""#));
        assert!(!settings.contains(old.as_ref()));
        let backup =
            fs::read_to_string(to.join("backups/releases/wordcraft-0.2.0-x/installed-app.json"))
                .unwrap();
        assert!(backup.contains(new.as_ref()));
        fs::remove_dir_all(base).unwrap();
    }
    #[test]
    fn library_targets_must_be_empty_separate_folders() {
        let base = std::env::temp_dir().join(format!("craft-target-{}", uuid::Uuid::new_v4()));
        let from = base.join("library");
        fs::create_dir_all(&from).unwrap();
        assert!(check_target(&from, &from).is_err());
        assert!(check_target(&from, &from.join("inside")).is_err());
        assert!(check_target(&from, &base).is_err());
        assert!(check_target(&from, Path::new("relative")).is_err());
        let used = base.join("used");
        fs::create_dir_all(&used).unwrap();
        fs::write(used.join("notes.txt"), b"keep").unwrap();
        assert!(check_target(&from, &used).is_err());
        let empty = base.join("empty");
        fs::create_dir_all(&empty).unwrap();
        fs::write(empty.join(".DS_Store"), b"").unwrap();
        assert!(check_target(&from, &empty).is_ok());
        assert!(check_target(&from, &base.join("new")).is_ok());
        fs::remove_dir_all(base).unwrap();
    }
}
