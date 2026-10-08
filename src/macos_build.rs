//! Native source bundles and persistent backups for installed macOS apps.
use crate::{
    files,
    jobs::Job,
    model::{BuildInfo, Installed, Paths, Source},
    platform, tools, updates,
};
use anyhow::{bail, Context, Result};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    process::Command,
};
pub fn supported(app: &str) -> bool {
    matches!(app, "wordcraft" | "gridcraft" | "deckcraft")
}
fn valid_sha(sha: &str) -> bool {
    sha.len() == 40 && sha.bytes().all(|c| c.is_ascii_hexdigit())
}
fn git(repo: &Path, args: &[&str]) -> Result<String> {
    let o = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()?;
    if !o.status.success() {
        bail!("Git failed: {}", String::from_utf8_lossy(&o.stderr));
    }
    Ok(String::from_utf8(o.stdout)?.trim().into())
}
/// Operate only on an independent clone; never checkout/reset/clean the user's repo.
fn snapshot(repo: &Path, project: &Path, sha: &str, upstream: &str, job: &Job) -> Result<()> {
    if !valid_sha(sha) {
        bail!("Invalid source commit");
    }
    if project.exists() {
        bail!("Source workspace already exists");
    }
    job.run(
        Command::new("git")
            .args([
                "clone",
                "--no-hardlinks",
                "--dissociate",
                "--no-checkout",
                "--",
            ])
            .arg(repo)
            .arg(project),
        false,
    )?;
    if git(project, &["cat-file", "-t", sha]).is_err() {
        job.run(
            Command::new("git")
                .arg("-C")
                .arg(project)
                .args(["fetch", "--", upstream, sha]),
            false,
        )?;
    }
    job.run(
        Command::new("git")
            .arg("-C")
            .arg(project)
            .args(["checkout", "--detach", sha]),
        false,
    )?;
    if git(project, &["rev-parse", "HEAD"])? != sha {
        bail!("Workspace revision differs");
    }
    Ok(())
}
pub fn build(paths: &Paths, app: &str, latest: bool, job: &Job) -> Result<()> {
    let _lock = platform::Lock::take("Local\\CraftAppsSourceBuilder")?;
    if !supported(app) {
        bail!("Native source bundles are not supported for {app}");
    }
    tools::preflight(paths, app)?;
    let prefs = paths.builder_preferences()?;
    let workspace = paths.at(format!("workspace/{app}"));
    fs::create_dir_all(&workspace)?;
    let local = (!prefs.local_repositories.trim().is_empty())
        .then(|| PathBuf::from(&prefs.local_repositories).join(app));
    let (sha, branch, archive) = if let Some(repo) = &local {
        if git(repo, &["rev-parse", "--show-toplevel"])?
            != repo.canonicalize()?.display().to_string()
        {
            bail!("Expected a repository root at {}", repo.display());
        }
        let sha = if latest {
            job.stage(
                "Checking source",
                None,
                "Checking upstream main; local files are preserved",
            );
            latest_commit(paths, app)?
        } else {
            git(repo, &["rev-parse", "HEAD"])?
        };
        (
            sha,
            if latest {
                "main".to_owned()
            } else {
                git(repo, &["rev-parse", "--abbrev-ref", "HEAD"])?
            },
            None,
        )
    } else {
        if latest {
            updates::sources(paths, &[app.into()], job)?;
        }
        let index: BTreeMap<String, Source> =
            files::read_json(&paths.at("sources/source-index.json")).context(
                "Download source first, or configure a local repository folder in Builder settings",
            )?;
        let s = index
            .get(app)
            .context("No downloaded source for this app")?;
        let archive = paths.at(format!("sources/{app}-source.zip"));
        if files::hash(&archive)? != s.archive_sha256 {
            bail!("Source checksum differs");
        }
        files::verify_source(&archive, app, &s.sha)?;
        (s.sha.clone(), s.branch.clone(), Some(archive))
    };
    if !valid_sha(&sha) {
        bail!("Invalid source commit");
    }
    let work = workspace.join(format!("{}-{}", &sha[..12], uuid::Uuid::new_v4().simple()));
    fs::create_dir_all(&work)?;
    let project = work.join("src");
    job.stage("Preparing source", None, format!("Source commit: {sha}"));
    if let Some(repo) = &local {
        snapshot(
            repo,
            &project,
            &sha,
            &format!("https://github.com/storytold/{app}.git"),
            job,
        )?;
    } else {
        files::extract_zip(&archive.context("Missing source archive")?, &work, job)?;
        fs::rename(work.join(format!("{app}-{sha}")), &project)?;
    }
    let sibling = local
        .as_ref()
        .and_then(|r| r.parent())
        .map(|p| p.join("craft-fonts"));
    let fonts = sibling
        .filter(|p| p.is_dir())
        .unwrap_or_else(|| paths.at("workspace/craft-fonts"));
    if !fonts.is_dir() {
        job.run(
            Command::new("git")
                .args([
                    "clone",
                    "--",
                    "https://github.com/storytold/craft-fonts.git",
                ])
                .arg(&fonts),
            false,
        )?;
    }
    let target = paths.at(format!("workspace/cache/{app}"));
    let mut env = tools::environment(paths)?;
    env.insert("CARGO_TARGET_DIR".into(), target.display().to_string());
    env.insert("CARGO_BUILD_JOBS".into(), "4".into());
    env.insert(
        "CRAFT_FONTS_DIR".into(),
        fonts.canonicalize()?.display().to_string(),
    );
    env.insert("MACOS_SIGN_IDENTITY".into(), "-".into());
    env.insert(format!("{}_BUILD_SHA", app.to_uppercase()), sha.clone());
    let arch = if cfg!(target_arch = "aarch64") {
        "aarch64"
    } else {
        "x86_64"
    };
    job.stage(
        "Building app",
        None,
        "Compiling and packaging a locally signed app bundle",
    );
    job.run(
        Command::new("bash")
            .args(["packaging/macos/package.sh", "--arch", arch])
            .current_dir(&project)
            .envs(env),
        false,
    )?;
    job.check()?;
    let bundle = target
        .join("macos-package")
        .join(crate::model::executable_name(app));
    if inspect(&bundle, app)?.source_commit != sha {
        bail!("Built bundle revision differs from source");
    }
    let out = paths.at(format!(
        "builds/{app}/{}-{}",
        &sha[..7],
        uuid::Uuid::new_v4().simple()
    ));
    fs::create_dir_all(&out)?;
    let built = out.join(crate::model::executable_name(app));
    copy_bundle(&bundle, &built)?;
    let status = Command::new("plutil")
        .args(["-insert", "CraftManagerChannel", "-string", "source"])
        .arg(built.join("Contents/Info.plist"))
        .status()?;
    if !status.success() {
        bail!("Could not record source channel");
    }
    let status = Command::new("codesign")
        .args(["--force", "--sign", "-", "--options", "runtime"])
        .arg(&built)
        .status()?;
    if !status.success() {
        bail!("Could not sign source bundle");
    }
    verify(&built, app)?;
    files::write_json(
        &out.join("build-info.json"),
        &BuildInfo {
            app: app.into(),
            commit: sha,
            source_branch: branch,
            built_at: chrono::Utc::now().to_rfc3339(),
            profile: "release".into(),
            log: job.log_path.display().to_string(),
        },
    )?;
    job.check()?;
    job.state.lock().unwrap().output = Some(out.clone());
    job.log(&format!(
        "Build complete: {}. Use Install built app to replace the installed copy.",
        out.display()
    ));
    if prefs.delete_workspace_after_success {
        files::remove_managed(&work, &workspace)?;
    }
    if prefs.delete_cache_after_success {
        files::remove_managed(&target, &paths.at("workspace/cache"))?;
    }
    Ok(())
}
fn latest_commit(paths: &Paths, app: &str) -> Result<String> {
    crate::model::valid_app(app)?;
    let response: serde_json::Value = crate::network::Network::new(&paths.root)?.json(&format!(
        "https://api.github.com/repos/storytold/{}/commits/main",
        crate::model::repository(app)
    ))?;
    let sha = response["sha"]
        .as_str()
        .context("Upstream commit is missing")?;
    if !valid_sha(sha) {
        bail!("Invalid upstream source commit");
    }
    Ok(sha.to_owned())
}
pub fn check_source(paths: &Paths, app: &str, job: &Job) -> Result<()> {
    job.check()?;
    let latest = latest_commit(paths, app)?;
    job.check()?;
    let installed =
        crate::installers::detect(app)?.context("App is not installed in Applications")?;
    let status = if installed.source_commit.is_empty() {
        "Installed commit is unknown"
    } else if installed.source_commit == latest {
        "Installed commit matches upstream main"
    } else {
        "Upstream main differs from the installed commit; use Build app bundle with Use latest source"
    };
    job.stage("Source checked", None, status);
    job.log(&format!(
        "{status}\nInstalled: {}\nUpstream: {latest}",
        installed.source_commit
    ));
    Ok(())
}
pub fn inspect(bundle: &Path, app: &str) -> Result<Installed> {
    crate::model::valid_app(app)?;
    let id = platform::bundle_value(bundle, "CFBundleIdentifier")
        .context("Missing bundle identifier")?;
    if id != format!("ai.storyteller.{}", crate::model::repository(app))
        && id != format!("ai.storyteller.{app}")
    {
        bail!("Bundle identity differs from selected app");
    }
    let executable = platform::bundle_value(bundle, "CFBundleExecutable")
        .context("Missing bundle executable")?;
    if Path::new(&executable).components().count() != 1
        || executable == "."
        || executable == ".."
        || !bundle.join("Contents/MacOS").join(&executable).is_file()
    {
        bail!("Invalid bundle executable");
    }
    let version =
        platform::bundle_value(bundle, "CFBundleShortVersionString").context("Missing version")?;
    updates::version(&version)?;
    let commit =
        platform::bundle_value(bundle, &format!("{}BuildCommit", crate::model::title(app)))
            .unwrap_or_default();
    let commit = if valid_sha(&commit) {
        commit
    } else {
        String::new()
    };
    let mut origin = platform::bundle_value(bundle, "CraftManagerChannel").unwrap_or_default();
    if origin.is_empty() && !commit.is_empty() {
        let o = Command::new("codesign")
            .args(["-d", "--verbose=2"])
            .arg(bundle)
            .output()?;
        if String::from_utf8_lossy(&o.stderr).contains("Signature=adhoc") {
            origin = "source".into();
        }
    }
    Ok(Installed {
        name: app.into(),
        version,
        path: bundle
            .parent()
            .context("No bundle parent")?
            .display()
            .to_string(),
        architecture: crate::model::MANAGER_ARCH.into(),
        install_kind: "installer".into(),
        product_code: id,
        source_commit: commit,
        install_origin: if origin.is_empty() {
            "release".into()
        } else {
            origin
        },
    })
}
fn copy_bundle(from: &Path, to: &Path) -> Result<()> {
    let s = Command::new("ditto")
        .args(["--noextattr", "--norsrc"])
        .arg(from)
        .arg(to)
        .status()?;
    if !s.success() {
        bail!("Could not copy bundle");
    }
    Ok(())
}
fn verify(bundle: &Path, app: &str) -> Result<()> {
    inspect(bundle, app)?;
    let s = Command::new("codesign")
        .args(["--verify", "--deep", "--strict"])
        .arg(bundle)
        .status()?;
    if !s.success() {
        bail!("Invalid app signature");
    }
    Ok(())
}
pub fn install_build(paths: &Paths, app: &str, job: &Job) -> Result<()> {
    let _lock = platform::Lock::take("Local\\CraftAppsManager")?;
    let out = crate::builder::history(paths, app).context("No finished app build")?;
    files::inside(&out, &paths.at("builds"))?;
    let info: BuildInfo = files::read_json(&out.join("build-info.json"))?;
    let bundle = out.join(crate::model::executable_name(app));
    if inspect(&bundle, app)?.source_commit != info.commit || info.app != app {
        bail!("Build provenance differs");
    }
    install(paths, app, &bundle, job)?;
    Ok(())
}
fn install(paths: &Paths, app: &str, bundle: &Path, job: &Job) -> Result<Installed> {
    let existing = crate::installers::detect(app)?;
    let mut folder = existing
        .as_ref()
        .map(|r| PathBuf::from(&r.path))
        .unwrap_or_else(|| PathBuf::from("/Applications"));
    if !writable(&folder) {
        if existing.is_some() {
            bail!("Installed app folder is not writable");
        }
        folder = PathBuf::from(std::env::var_os("HOME").context("Missing home folder")?)
            .join("Applications");
        fs::create_dir_all(&folder)?;
    }
    install_in(paths, app, bundle, &folder, job, true)
}
fn writable(folder: &Path) -> bool {
    let p = folder.join(format!(".craft-write-{}", uuid::Uuid::new_v4().simple()));
    fs::create_dir(&p).is_ok() && fs::remove_dir(p).is_ok()
}
fn install_in(
    paths: &Paths,
    app: &str,
    bundle: &Path,
    folder: &Path,
    job: &Job,
    prune: bool,
) -> Result<Installed> {
    verify(bundle, app)?;
    if platform::running_app(app)? {
        bail!("Close the app before replacing it");
    }
    let mut record = inspect(bundle, app)?;
    record.path = folder.display().to_string();
    let dest = folder.join(crate::model::executable_name(app));
    if dest.exists() && files::linked(&dest)? {
        bail!("Installed app is a link");
    }
    let stage = folder.join(format!(".craft-install-{}", uuid::Uuid::new_v4().simple()));
    fs::create_dir_all(&stage)?;
    let staged = stage.join(crate::model::executable_name(app));
    let previous = stage.join("previous.app");
    let mut backup = None;
    let result = (|| -> Result<()> {
        copy_bundle(bundle, &staged)?;
        verify(&staged, app)?;
        if dest.exists() {
            let old = inspect(&dest, app)?;
            let saved = paths.at("backups/releases").join(format!(
                "{app}-{}-{}",
                old.version,
                uuid::Uuid::new_v4().simple()
            ));
            fs::create_dir_all(&saved)?;
            copy_bundle(&dest, &saved.join(crate::model::executable_name(app)))?;
            verify(&saved.join(crate::model::executable_name(app)), app)?;
            files::write_json(&saved.join("installed-app.json"), &old)?;
            backup = Some(saved);
        }
        job.check()?;
        if platform::running_app(app)? {
            bail!("App opened during installation; close it and retry");
        }
        let mut config = paths.config()?;
        let item = config
            .apps
            .iter_mut()
            .find(|a| a.name == app)
            .context("App missing from catalog")?;
        *item = record.clone();
        updates::replace_transaction(&staged, &dest, Some(&previous), || {
            paths.save_config(&config)
        })?;
        Ok(())
    })();
    if result.is_ok() || !previous.exists() {
        let _ = fs::remove_dir_all(&stage);
    } else {
        job.log(&format!("Recovery copy retained at {}", previous.display()));
    }
    result?;
    if prune {
        if let Err(e) = crate::backups::finish(
            paths,
            &paths.preferences()?,
            backup.as_deref(),
            app,
            false,
            job,
        ) {
            job.log(&format!("Backup retained: {e:#}"));
        }
    }
    job.log(&format!(
        "Installed {} {} ({})",
        crate::model::title(app),
        record.version,
        record.source_commit
    ));
    Ok(record)
}
pub fn restore(paths: &Paths, app: &str, backup: &Path, job: &Job) -> Result<()> {
    files::inside(backup, &paths.at("backups/releases"))?;
    files::no_links(backup)?;
    let record: Installed = files::read_json(&backup.join("installed-app.json"))?;
    let allowed = record.path == "/Applications"
        || std::env::var_os("HOME")
            .is_some_and(|h| PathBuf::from(h).join("Applications") == Path::new(&record.path));
    if record.name != app || !allowed {
        bail!("Invalid backup destination");
    }
    let bundle = backup.join(crate::model::executable_name(app));
    let actual = inspect(&bundle, app)?;
    if actual.version != record.version || actual.source_commit != record.source_commit {
        bail!("Backup provenance differs");
    }
    install_in(paths, app, &bundle, Path::new(&record.path), job, false)?;
    Ok(())
}

/// Release extraction retains the upstream signature and Gatekeeper checks.
/// The caller holds the manager lock, just as for other release installers.
pub fn install_release(paths: &Paths, app: &str, image: &Path, job: &Job) -> Result<Installed> {
    let stage = paths.at(format!(
        "runtime/downloads/install-{}",
        uuid::Uuid::new_v4().simple()
    ));
    fs::create_dir_all(&stage)?;
    let result = (|| {
        let bundle = crate::installers::extract_app(image, &stage, app, job)?;
        install(paths, app, &bundle, job)
    })();
    let _ = fs::remove_dir_all(&stage);
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            let root =
                std::env::temp_dir().join(format!("craft-mac-build-{}", uuid::Uuid::new_v4()));
            fs::create_dir_all(&root).unwrap();
            Self(root)
        }
        fn paths(&self) -> Paths {
            Paths::new(self.0.join("library"), None)
        }
        fn job(&self) -> Job {
            Job::new(self.0.join("test.log"), &Default::default())
        }
        fn bundle(&self, parent: &str, app: &str, version: &str, commit: &str) -> PathBuf {
            let bundle = self.0.join(parent).join(crate::model::executable_name(app));
            fs::create_dir_all(bundle.join("Contents/MacOS")).unwrap();
            fs::copy("/usr/bin/true", bundle.join("Contents/MacOS/fixture")).unwrap();
            let plist = format!(
                r#"<?xml version="1.0"?><plist version="1.0"><dict>
              <key>CFBundleIdentifier</key><string>ai.storyteller.{app}</string>
              <key>CFBundleExecutable</key><string>fixture</string>
              <key>CFBundlePackageType</key><string>APPL</string>
              <key>CFBundleShortVersionString</key><string>{version}</string>
              <key>{}BuildCommit</key><string>{commit}</string>
              <key>CraftManagerChannel</key><string>source</string>
              </dict></plist>"#,
                crate::model::title(app)
            );
            fs::write(bundle.join("Contents/Info.plist"), plist).unwrap();
            assert!(Command::new("codesign")
                .args(["--force", "--sign", "-"])
                .arg(&bundle)
                .status()
                .unwrap()
                .success());
            bundle
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    #[test]
    fn snapshot_preserves_dirty_files_branch_and_old_commit() {
        let f = Fixture::new();
        let repo = f.0.join("user-repo");
        fs::create_dir(&repo).unwrap();
        git(&repo, &["init", "-b", "user-branch"]).unwrap();
        fs::write(repo.join("document"), "committed").unwrap();
        git(&repo, &["add", "document"]).unwrap();
        git(
            &repo,
            &[
                "-c",
                "user.name=Fixture",
                "-c",
                "user.email=fixture@example.invalid",
                "commit",
                "-m",
                "fixture",
            ],
        )
        .unwrap();
        let sha = git(&repo, &["rev-parse", "HEAD"]).unwrap();
        fs::write(repo.join("document"), "uncommitted work").unwrap();
        fs::write(repo.join("untracked"), "keep").unwrap();
        let before = git(&repo, &["status", "--porcelain"]).unwrap();
        let project = f.0.join("snapshot");
        snapshot(&repo, &project, &sha, "unused", &f.job()).unwrap();
        assert_eq!(
            fs::read_to_string(project.join("document")).unwrap(),
            "committed"
        );
        assert!(!project.join("untracked").exists());
        assert_eq!(git(&repo, &["status", "--porcelain"]).unwrap(), before);
        assert_eq!(
            git(&repo, &["rev-parse", "--abbrev-ref", "HEAD"]).unwrap(),
            "user-branch"
        );
        assert_eq!(
            fs::read_to_string(repo.join("document")).unwrap(),
            "uncommitted work"
        );
    }
    #[test]
    fn bundles_are_backed_up_and_restored_with_provenance_for_all_three_apps() {
        let f = Fixture::new();
        let paths = f.paths();
        let folder = f.0.join("Applications");
        fs::create_dir(&folder).unwrap();
        for app in ["wordcraft", "gridcraft", "deckcraft"] {
            let first = f.bundle("first", app, "0.2.0", &"a".repeat(40));
            let second = f.bundle("second", app, "0.2.1", &"b".repeat(40));
            install_in(&paths, app, &first, &folder, &f.job(), true).unwrap();
            let next = install_in(&paths, app, &second, &folder, &f.job(), true).unwrap();
            assert_eq!(next.source_commit, "b".repeat(40));
            assert_eq!(next.install_origin, "source");
            let backups = crate::backups::list(&paths, app).unwrap();
            assert_eq!(backups.len(), 1);
            let backup = &backups[0].path;
            let saved: Installed = files::read_json(&backup.join("installed-app.json")).unwrap();
            assert_eq!(saved.source_commit, "a".repeat(40));
            assert_eq!(saved.path, folder.display().to_string());
            let restored = install_in(
                &paths,
                app,
                &backup.join(crate::model::executable_name(app)),
                &folder,
                &f.job(),
                false,
            )
            .unwrap();
            assert_eq!(restored.source_commit, "a".repeat(40));
            verify(&folder.join(crate::model::executable_name(app)), app).unwrap();
            assert!(backup.is_dir());
            assert_eq!(crate::backups::list(&paths, app).unwrap().len(), 2);
            assert_eq!(
                crate::model::build_executable_name(app),
                crate::model::executable_name(app)
            );
        }
    }
    #[test]
    fn wrong_identity_invalid_signature_and_cancel_do_not_replace_installed_app() {
        let f = Fixture::new();
        let paths = f.paths();
        let folder = f.0.join("Applications");
        fs::create_dir(&folder).unwrap();
        let original = f.bundle("original", "wordcraft", "0.2.0", &"a".repeat(40));
        install_in(&paths, "wordcraft", &original, &folder, &f.job(), true).unwrap();
        let wrong = f.bundle("wrong", "gridcraft", "0.2.1", &"b".repeat(40));
        assert!(install_in(&paths, "wordcraft", &wrong, &folder, &f.job(), true).is_err());
        let damaged = f.bundle("damaged", "wordcraft", "0.2.1", &"b".repeat(40));
        fs::write(damaged.join("Contents/MacOS/fixture"), "damaged").unwrap();
        assert!(install_in(&paths, "wordcraft", &damaged, &folder, &f.job(), true).is_err());
        let next = f.bundle("next", "wordcraft", "0.2.1", &"b".repeat(40));
        let cancelled = f.job();
        cancelled
            .cancel
            .store(true, std::sync::atomic::Ordering::Relaxed);
        assert!(install_in(&paths, "wordcraft", &next, &folder, &cancelled, true).is_err());
        assert_eq!(
            inspect(&folder.join("WordCraft.app"), "wordcraft")
                .unwrap()
                .source_commit,
            "a".repeat(40)
        );
    }
    #[test]
    fn restore_rejects_metadata_pointing_outside_application_folders() {
        let f = Fixture::new();
        let paths = f.paths();
        let backup = paths.at("backups/releases/wordcraft-0.2.0-00000000000000000000000000000000");
        fs::create_dir_all(&backup).unwrap();
        files::write_json(
            &backup.join("installed-app.json"),
            &Installed {
                name: "wordcraft".into(),
                path: f.0.display().to_string(),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(restore(&paths, "wordcraft", &backup, &f.job())
            .unwrap_err()
            .to_string()
            .contains("destination"));
    }
}
