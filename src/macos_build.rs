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
    crate::model::SOURCES.contains(&app)
}
fn valid_sha(sha: &str) -> bool {
    sha.len() == 40 && sha.bytes().all(|c| c.is_ascii_hexdigit())
}
fn extract_project(
    archive: &Path,
    work: &Path,
    app: &str,
    sha: &str,
    job: &Job,
) -> Result<PathBuf> {
    let extracted = work.join("archive");
    files::extract_zip(archive, &extracted, job)?;
    let project = work.join("src");
    fs::rename(
        extracted.join(format!("{}-{sha}", crate::model::repository(app))),
        &project,
    )?;
    Ok(project)
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
    // Source ZIPs and their commit index are fetched only from official Storytold repos.
    if latest {
        updates::sources(paths, &[app.into()], job)?;
    }
    let index: BTreeMap<String, Source> = files::read_json(&paths.at("sources/source-index.json"))
        .context("Fetch official source first")?;
    let source = index
        .get(app)
        .context("No downloaded official source for this app")?;
    let sha = source.sha.clone();
    let branch = source.branch.clone();
    let archive = paths.at(format!("sources/{app}-source.zip"));
    if files::hash(&archive)? != source.archive_sha256 {
        bail!("Source checksum differs");
    }
    files::verify_source(&archive, app, &sha)?;
    if !valid_sha(&sha) {
        bail!("Invalid source commit");
    }
    let work = workspace.join(format!("{}-{}", &sha[..12], uuid::Uuid::new_v4().simple()));
    fs::create_dir_all(&work)?;
    job.stage("Preparing source", None, format!("Source commit: {sha}"));
    let project = extract_project(&archive, &work, app, &sha, job)?;
    let fonts = paths.at("workspace/craft-fonts");
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
    env.insert(
        format!("{}_BUILD_SHA", crate::model::repository(app).to_uppercase()),
        sha.clone(),
    );
    let arch = if cfg!(target_arch = "aarch64") {
        "aarch64"
    } else {
        "x86_64"
    };
    job.stage(
        "Building app",
        None,
        "Building a native bundle from official source",
    );
    let bundle = if matches!(app, "artcraft" | "artcraftx") {
        build_tauri(paths, app, &project, &target, &env, job)?
    } else {
        job.run(
            Command::new("bash")
                .args(["packaging/macos/package.sh", "--arch", arch])
                .current_dir(&project)
                .envs(&env),
            false,
        )?;
        let bundle = target
            .join("macos-package")
            .join(crate::model::executable_name(app));
        if inspect(&bundle, app)?.source_commit != sha {
            bail!("Built bundle revision differs from source");
        }
        bundle
    };
    job.check()?;
    let out = paths.at(format!(
        "builds/{app}/{}-{}",
        &sha[..7],
        uuid::Uuid::new_v4().simple()
    ));
    fs::create_dir_all(&out)?;
    let built = out.join(crate::model::executable_name(app));
    copy_bundle(&bundle, &built)?;
    set_bundle_value(&built, "CraftManagerBuildCommit", &sha)?;
    let status = Command::new("plutil")
        .args(["-replace", "CraftManagerChannel", "-string", "source"])
        .arg(built.join("Contents/Info.plist"))
        .status()?;
    if !status.success() {
        bail!("Could not record source channel");
    }
    let status = Command::new("codesign")
        .args([
            "--force",
            "--sign",
            "-",
            "--options",
            "runtime",
            "--preserve-metadata=entitlements",
        ])
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
    let installed = crate::apps::installed(paths, app)?;
    let status = if installed.source_commit.is_empty() {
        "Installed commit is unknown"
    } else if installed.source_commit == latest {
        "Installed commit matches upstream main"
    } else {
        "Upstream main differs from the installed commit; use Build .app"
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
    if !identities(app).contains(&id) {
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
    let commit = platform::bundle_value(bundle, "CraftManagerBuildCommit")
        .or_else(|| {
            platform::bundle_value(
                bundle,
                &format!(
                    "{}BuildCommit",
                    if app == "printcraft" {
                        "PdfCraft".to_owned()
                    } else {
                        crate::model::title(app)
                    }
                ),
            )
        })
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
    platform::clear_attributes(to)
}
fn verify(bundle: &Path, app: &str) -> Result<()> {
    inspect(bundle, app)?;
    let out = Command::new("codesign")
        .args(["--verify", "--deep", "--strict"])
        .arg(bundle)
        .output()?;
    if !out.status.success() {
        let reason = String::from_utf8_lossy(&out.stderr);
        bail!(
            "Invalid app signature: {}",
            reason.lines().next().unwrap_or_default().trim()
        );
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
    if paths.preferences()?.release_format == "portable" {
        let folder = paths.at(format!("releases/{app}"));
        files::inside(&folder, &paths.at("releases"))?;
        if folder.exists() {
            files::no_links(&folder)?;
        }
        fs::create_dir_all(&folder)?;
        return install_in(paths, app, bundle, &folder, job, true);
    }
    // Updates replace the app where it is; Move to app folder relocates it.
    let existing = crate::installers::detect_in(app, &paths.app_folders()?)?;
    let custom = !paths.preferences()?.install_folder.is_empty();
    let mut folder = match &existing {
        Some(record) => PathBuf::from(&record.path),
        None => paths.install_folder()?,
    };
    if existing.is_none() && custom {
        fs::create_dir_all(&folder)?;
    }
    if !writable(&folder) {
        if existing.is_some() || custom {
            bail!("{} is not writable", folder.display());
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
    if folder == paths.at(format!("releases/{app}")) {
        record.install_kind = "portable".into();
    }
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
            let mut old = inspect(&dest, app)?;
            if record.install_kind == "portable" {
                old.install_kind = "portable".into();
            }
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
    if let Some(home) = std::env::var_os("HOME") {
        retire_legacy_bundles(app, folder, &PathBuf::from(home).join(".Trash"), job);
    }
    job.log(&format!(
        "Installed {} {} ({})",
        crate::model::title(app),
        record.version,
        record.source_commit
    ));
    Ok(record)
}
/// Moves an installed app into the chosen app folder. Same-volume moves are a rename;
/// across volumes the bundle is copied, verified, and only then removed from its old place.
pub fn move_app(paths: &Paths, app: &str, job: &Job) -> Result<PathBuf> {
    crate::model::valid_app(app)?;
    let _lock = platform::Lock::take("Local\\CraftAppsManager")?;
    let installed = crate::apps::installed(paths, app)?;
    if installed.install_kind != "installer" {
        bail!("Only apps installed in an applications folder can be moved");
    }
    let bundle = crate::model::installed_executable(Path::new(&installed.path), app)
        .context("Installed app bundle is missing")?;
    let to = paths.install_folder()?;
    if platform::running_app(app)? {
        bail!("Close {} before moving it", crate::model::title(app));
    }
    let moved = relocate_bundle(app, &bundle, &to, job)?;
    job.log(&format!(
        "Moved {} to {}",
        crate::model::title(app),
        to.display()
    ));
    Ok(moved)
}
/// Moves every installed app that is outside the chosen app folder, continuing past failures.
pub fn move_apps(paths: &Paths, job: &Job) -> Result<()> {
    let to = paths.install_folder()?;
    let apps: Vec<_> = paths
        .config()?
        .apps
        .into_iter()
        .filter(|a| a.install_kind == "installer" && !a.path.is_empty())
        .filter(|a| Path::new(&a.path) != to)
        .collect();
    let mut failures = 0;
    for (n, app) in apps.iter().enumerate() {
        job.check()?;
        job.stage(
            "Moving apps",
            Some(n as f32 / apps.len() as f32),
            crate::model::title(&app.name),
        );
        if let Err(error) = move_app(paths, &app.name, job) {
            job.check()?;
            failures += 1;
            job.log(&format!("{}: {error:#}", app.name));
        }
    }
    if failures > 0 {
        bail!(
            "{failures} of {} app(s) could not be moved; see the activity log",
            apps.len()
        );
    }
    Ok(())
}
fn relocate_bundle(app: &str, bundle: &Path, to: &Path, job: &Job) -> Result<PathBuf> {
    let name = bundle.file_name().context("Missing bundle name")?;
    if !owned_bundle(bundle, app) {
        bail!(
            "{} is not a {} bundle",
            bundle.display(),
            crate::model::title(app)
        );
    }
    if bundle.parent() == Some(to) {
        return Ok(bundle.to_path_buf());
    }
    fs::create_dir_all(to)?;
    if files::linked(to)? {
        bail!("{} is a link; choose the real folder", to.display());
    }
    let dest = to.join(name);
    if fs::symlink_metadata(&dest).is_ok() {
        bail!("{} already exists", dest.display());
    }
    if fs::rename(bundle, &dest).is_ok() {
        return Ok(dest);
    }
    // Different volume: copy beside the destination, verify, publish, then remove the original.
    let stage = to.join(format!(".craft-move-{}", uuid::Uuid::new_v4().simple()));
    fs::create_dir_all(&stage)?;
    let result = (|| -> Result<()> {
        let staged = stage.join(name);
        job.stage(
            "Moving apps",
            None,
            format!("Copying {}", name.to_string_lossy()),
        );
        copy_bundle(bundle, &staged)?;
        verify(&staged, app)?;
        fs::rename(&staged, &dest)?;
        Ok(())
    })();
    let _ = fs::remove_dir_all(&stage);
    result?;
    fs::remove_dir_all(bundle).with_context(|| {
        format!(
            "Copied to {}, but could not remove {}",
            dest.display(),
            bundle.display()
        )
    })?;
    Ok(dest)
}
/// Moves bundles left under an app's former name (PrintCraft.app beside PdfCraft.app)
/// to the Trash after the current bundle is installed, so one copy remains and the
/// old one stays recoverable. Links and bundles of other apps are never touched.
fn retire_legacy_bundles(app: &str, folder: &Path, trash: &Path, job: &Job) {
    let current = crate::model::executable_name(app);
    for name in crate::model::executable_names(app) {
        let legacy = folder.join(&name);
        if name == current || !owned_bundle(&legacy, app) {
            continue;
        }
        let result = (|| -> Result<PathBuf> {
            fs::create_dir_all(trash)?;
            let mut to = trash.join(&name);
            if fs::symlink_metadata(&to).is_ok() {
                let stem = name.trim_end_matches(".app");
                to = trash.join(format!(
                    "{stem} {}.app",
                    &uuid::Uuid::new_v4().simple().to_string()[..8]
                ));
            }
            fs::rename(&legacy, &to)?;
            Ok(to)
        })();
        match result {
            Ok(to) => job.log(&format!(
                "Moved the old {name} to the Trash: {}",
                to.display()
            )),
            Err(e) => job.log(&format!(
                "Could not move the old {name} to the Trash: {e:#}"
            )),
        }
    }
}
fn owned_bundle(bundle: &Path, app: &str) -> bool {
    let real = |p: &Path| fs::symlink_metadata(p).is_ok_and(|m| !m.file_type().is_symlink());
    real(bundle)
        && bundle.is_dir()
        && real(&bundle.join("Contents"))
        && real(&bundle.join("Contents/Info.plist"))
        && platform::bundle_value(bundle, "CFBundleIdentifier")
            .is_some_and(|id| identities(app).contains(&id))
}
pub fn restore(paths: &Paths, app: &str, backup: &Path, job: &Job) -> Result<()> {
    files::inside(backup, &paths.at("backups/releases"))?;
    files::no_links(backup)?;
    let record: Installed = files::read_json(&backup.join("installed-app.json"))?;
    let allowed = paths
        .app_folders()?
        .iter()
        .any(|folder| folder == Path::new(&record.path));
    let portable = record.install_kind == "portable"
        && Path::new(&record.path) == paths.at(format!("releases/{app}"));
    if record.name != app || !(allowed || portable) {
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

pub fn identities(app: &str) -> Vec<String> {
    match app {
        "artcraft" => vec!["ai.artcraft.app".into()],
        "artcraftx" => vec!["ai.artcraftx.app".into()],
        _ => vec![
            format!("ai.storyteller.{}", crate::model::repository(app)),
            format!("ai.storyteller.{app}"),
        ],
    }
}
fn set_bundle_value(bundle: &Path, key: &str, value: &str) -> Result<()> {
    let status = Command::new("plutil")
        .args(["-replace", key, "-string", value])
        .arg(bundle.join("Contents/Info.plist"))
        .status()?;
    if !status.success() {
        bail!("Could not record bundle provenance");
    }
    Ok(())
}
fn tauri_crate(app: &str) -> Result<&'static str> {
    match app {
        "artcraft" => Ok("crates/desktop/artcraft"),
        "artcraftx" => Ok("crates/artcraftx"),
        _ => bail!("Not a Tauri app"),
    }
}
fn build_tauri(
    paths: &Paths,
    app: &str,
    project: &Path,
    target: &Path,
    env: &BTreeMap<String, String>,
    job: &Job,
) -> Result<PathBuf> {
    let mut env = env.clone();
    env.insert("VITE_ENVIRONMENT_TYPE".into(), "production".into());
    env.insert("SQLX_OFFLINE".into(), "true".into());
    env.insert("NX_DAEMON".into(), "false".into());
    let node = tools::system("node").context("Install Node.js with Set up build tools")?;
    let npm = tools::system("npm")
        .context("npm is missing")?
        .canonicalize()?;
    let frontend = project.join("frontend");
    job.stage(
        "Frontend dependencies",
        None,
        "Installing official locked frontend dependencies",
    );
    job.run(
        Command::new(&node)
            .arg(npm)
            .args(["ci", "--no-audit", "--no-fund"])
            .current_dir(&frontend)
            .envs(&env),
        false,
    )?;
    job.stage("Frontend build", None, "Building the ArtCraft interface");
    job.run(
        Command::new(&node)
            .args(["node_modules/nx/bin/nx.js", "run", "artcraft:build"])
            .current_dir(&frontend)
            .envs(&env),
        false,
    )?;
    if !frontend.join("apps/artcraft/dist/index.html").is_file() {
        bail!("Frontend output is missing");
    }
    let tauri = tools::tauri(paths).context("Tauri CLI is missing; run Set up build tools")?;
    let overlay = project.join("craft-manager-tauri.json");
    // The frontend was built from the upstream Nx project above; skip Tauri's hook.
    files::write_json(
        &overlay,
        &serde_json::json!({"build":{"beforeBuildCommand":""},"bundle":{"macOS":{"signingIdentity":"-"}}}),
    )?;
    let triple = if cfg!(target_arch = "aarch64") {
        "aarch64-apple-darwin"
    } else {
        "x86_64-apple-darwin"
    };
    job.stage("Building app", None, "Compiling and bundling the Tauri app");
    job.run(
        Command::new(tauri)
            .args([
                "tauri",
                "build",
                "--ci",
                "--bundles",
                "app",
                "--target",
                triple,
                "--config",
            ])
            .arg(&overlay)
            .args(["--", "--locked"])
            .current_dir(project.join(tauri_crate(app)?))
            .envs(&env),
        false,
    )?;
    Ok(target
        .join(triple)
        .join("release/bundle/macos")
        .join(crate::model::executable_name(app)))
}
/// None means there is no compatible stable Mac asset, not that a download failed.
pub(crate) fn release_asset(
    release: Option<&crate::model::Release>,
    app: &str,
    prefs: &crate::model::Preferences,
) -> Result<Option<crate::model::Asset>> {
    let Some(release) = release else {
        return Ok(None);
    };
    if release.draft || release.prerelease {
        return Ok(None);
    }
    let version = crate::updates::release_version(app, &release.tag_name)?;
    let expected = if app == "artcraft" {
        format!("ArtCraft_{version}_universal.dmg")
    } else {
        format!(
            "{}-{version}-macos-universal.dmg",
            crate::model::repository(app)
        )
    };
    let native = format!(
        "{}-{version}-macos-{}.dmg",
        crate::model::repository(app),
        if prefs.architecture == "arm64" {
            "aarch64"
        } else {
            "x86_64"
        }
    );
    // Prefer universal; a native DMG is the second choice when both are published.
    for name in [
        expected,
        native,
        if app == "printcraft" {
            format!("printcraft-{version}-macos-universal.dmg")
        } else {
            String::new()
        },
    ] {
        let matches: Vec<_> = release.assets.iter().filter(|a| a.name == name).collect();
        if matches.len() > 1 {
            bail!("Duplicate compatible Mac release asset");
        }
        if let Some(asset) = matches.first() {
            return Ok(Some((*asset).clone()));
        }
    }
    Ok(None)
}
pub fn install_latest(paths: &Paths, app: &str, job: &Job) -> Result<()> {
    crate::model::valid_app(app)?;
    job.check()?;
    job.stage(
        "Checking releases",
        None,
        "Looking for an official Mac release",
    );
    let network = crate::network::Network::new(&paths.root)?;
    let response = network.json::<crate::model::Release>(&format!(
        "https://api.github.com/repos/storytold/{}/releases/latest",
        crate::model::repository(app)
    ));
    let release = match response {
        Ok(release) => Some(release),
        Err(error)
            if error
                .downcast_ref::<reqwest::Error>()
                .is_some_and(|e| e.status() == Some(reqwest::StatusCode::NOT_FOUND)) =>
        {
            None
        }
        Err(error) => return Err(error),
    };
    if let Some(asset) = release_asset(release.as_ref(), app, &paths.preferences()?)? {
        let _lock = platform::Lock::take("Local\\CraftAppsManager")?;
        let prefix = format!(
            "https://github.com/storytold/{}/releases/download/",
            crate::model::repository(app)
        );
        if !asset.browser_download_url.starts_with(&prefix) {
            bail!("Unexpected release asset URL");
        }
        files::safe_relative(&asset.name)?;
        let version = crate::updates::release_version(app, &release.as_ref().unwrap().tag_name)?;
        if let Ok(installed) = crate::apps::installed(paths, app) {
            if installed.install_origin == "release"
                && crate::updates::version(&installed.version)?
                    >= crate::updates::version(&version)?
            {
                job.log("The installed official release is current");
                return Ok(());
            }
        }
        let image = paths.at(format!("releases/installers/{app}/{}", asset.name));
        if image.is_file() {
            crate::network::verify_asset(&image, &asset)?;
        } else {
            network.asset(&asset, &image, job)?;
        }
        let installed = install_release(paths, app, &image, job)?;
        // The new version is the latest release, so the library can show it as current.
        if let Err(error) = crate::hourly::record(paths, app, &installed.version, None) {
            job.log(&format!("Could not record the update check: {error:#}"));
        }
        return Ok(());
    }
    job.log("No compatible official Mac release is available. Building the latest official source instead.");
    if tools::preflight(paths, app).is_err() {
        tools::setup(paths, app, job)?;
    }
    build(paths, app, true, job)?;
    install_build(paths, app, job)
}
pub fn install_selected(paths: &Paths, job: &Job) -> Result<()> {
    let mut failures = 0;
    for app in paths.preferences()?.selected_apps {
        job.check()?;
        if let Err(error) = install_latest(paths, &app, job) {
            job.check()?;
            failures += 1;
            job.log(&format!("{app}: {error:#}"));
        }
    }
    if failures > 0 {
        bail!("{failures} app update(s) failed; see the activity log");
    }
    Ok(())
}
/// Installs every app whose last release check found a newer version than the one installed.
pub fn install_available(paths: &Paths, job: &Job) -> Result<()> {
    let prefs = paths.preferences()?;
    let checks = crate::hourly::read(paths)?;
    let apps: Vec<_> = crate::updates::installed_check_targets(&paths.config()?)
        .into_iter()
        .filter(|app| {
            checks
                .get(&crate::hourly::key(&prefs, &app.name))
                .is_some_and(|c| c.installed == app.version && c.latest.is_some())
        })
        .collect();
    if apps.is_empty() {
        bail!("No updates are waiting. Run Check all first.");
    }
    let mut failures = 0;
    for app in &apps {
        job.check()?;
        job.log(&format!("Updating {}", crate::model::title(&app.name)));
        if let Err(error) = install_latest(paths, &app.name, job) {
            job.check()?;
            failures += 1;
            job.log(&format!("{}: {error:#}", app.name));
        }
    }
    if failures > 0 {
        bail!(
            "{failures} of {} update(s) failed; see the activity log",
            apps.len()
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn relocating_a_bundle_moves_it_and_refuses_conflicts_and_foreign_bundles() {
        let root = std::env::temp_dir().join(format!("craft-move-{}", uuid::Uuid::new_v4()));
        let id = identities("wordcraft").remove(0);
        let bundle = |folder: &Path, id: &str| {
            let contents = folder.join("WordCraft.app/Contents");
            fs::create_dir_all(&contents).unwrap();
            fs::write(
                contents.join("Info.plist"),
                format!(r#"<?xml version="1.0" encoding="UTF-8"?><plist version="1.0"><dict><key>CFBundleIdentifier</key><string>{id}</string></dict></plist>"#),
            )
            .unwrap();
            folder.join("WordCraft.app")
        };
        let job = Job::new(root.join("job.log"), &Default::default());
        let from = root.join("Applications");
        let to = root.join("Craft");
        let original = bundle(&from, &id);
        let moved = relocate_bundle("wordcraft", &original, &to, &job).unwrap();
        assert_eq!(moved, to.join("WordCraft.app"));
        assert!(moved.join("Contents/Info.plist").exists());
        assert!(!original.exists());
        // Already in place is a no-op.
        assert_eq!(
            relocate_bundle("wordcraft", &moved, &to, &job).unwrap(),
            moved
        );
        // A bundle already at the destination is never overwritten.
        let second = bundle(&from, &id);
        assert!(relocate_bundle("wordcraft", &second, &to, &job).is_err());
        assert!(second.exists());
        // Another app's bundle under the same name is refused.
        fs::remove_dir_all(&second).unwrap();
        let foreign = bundle(&from, "com.example.other");
        assert!(relocate_bundle("wordcraft", &foreign, &root.join("Other"), &job).is_err());
        assert!(foreign.exists());
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn legacy_printcraft_bundle_moves_to_trash_but_unrelated_and_linked_bundles_stay() {
        let root = std::env::temp_dir().join(format!("craft-legacy-{}", uuid::Uuid::new_v4()));
        let folder = root.join("Applications");
        let trash = root.join("Trash");
        let bundle = |name: &str, id: &str| {
            let contents = folder.join(name).join("Contents");
            fs::create_dir_all(&contents).unwrap();
            fs::write(
                contents.join("Info.plist"),
                format!(r#"<?xml version="1.0" encoding="UTF-8"?><plist version="1.0"><dict><key>CFBundleIdentifier</key><string>{id}</string></dict></plist>"#),
            )
            .unwrap();
        };
        let id = identities("printcraft").remove(0);
        bundle("PdfCraft.app", &id);
        bundle("PrintCraft.app", &id);
        let job = Job::new(root.join("job.log"), &Default::default());
        retire_legacy_bundles("printcraft", &folder, &trash, &job);
        assert!(folder.join("PdfCraft.app").exists());
        assert!(!folder.join("PrintCraft.app").exists());
        assert!(trash.join("PrintCraft.app/Contents/Info.plist").exists());
        // A second legacy copy gets a unique Trash name instead of overwriting.
        bundle("PrintCraft.app", &id);
        retire_legacy_bundles("printcraft", &folder, &trash, &job);
        assert_eq!(fs::read_dir(&trash).unwrap().count(), 2);
        // Another app's bundle under the old name, and links, are left alone.
        bundle("PrintCraft.app", "com.example.other");
        retire_legacy_bundles("printcraft", &folder, &trash, &job);
        assert!(folder.join("PrintCraft.app").exists());
        fs::remove_dir_all(folder.join("PrintCraft.app")).unwrap();
        std::os::unix::fs::symlink(trash.join("PrintCraft.app"), folder.join("PrintCraft.app"))
            .unwrap();
        retire_legacy_bundles("printcraft", &folder, &trash, &job);
        assert!(files::linked(&folder.join("PrintCraft.app")).unwrap());
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn copied_bundles_drop_finder_info_that_strict_signing_rejects() {
        let root = std::env::temp_dir().join(format!("craft-xattr-{}", uuid::Uuid::new_v4()));
        let from = root.join("From.app/Contents/Resources");
        fs::create_dir_all(&from).unwrap();
        let file = from.join("icon.icns");
        fs::write(&file, b"icon").unwrap();
        // Non-empty Finder info, as Finder writes it; ditto --noextattr keeps this.
        let info = format!("{:0<64}", "69636E7369636E73");
        assert!(Command::new("/usr/bin/xattr")
            .args(["-wx", "com.apple.FinderInfo", &info])
            .arg(&file)
            .status()
            .unwrap()
            .success());
        let to = root.join("To.app");
        copy_bundle(&root.join("From.app"), &to).unwrap();
        let left = Command::new("/usr/bin/xattr")
            .arg(to.join("Contents/Resources/icon.icns"))
            .output()
            .unwrap();
        // macOS may keep its own com.apple.provenance tag; strict signing allows it.
        assert!(!String::from_utf8_lossy(&left.stdout).contains("com.apple.FinderInfo"));
        fs::remove_dir_all(root).unwrap();
    }
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
              <key>CFBundleIdentifier</key><string>{}</string>
              <key>CFBundleExecutable</key><string>fixture</string>
              <key>CFBundlePackageType</key><string>APPL</string>
              <key>CFBundleShortVersionString</key><string>{version}</string>
              <key>{}BuildCommit</key><string>{commit}</string>
              <key>CraftManagerChannel</key><string>source</string>
              </dict></plist>"#,
                identities(app)[0],
                if app == "printcraft" {
                    "PdfCraft".to_owned()
                } else {
                    crate::model::title(app)
                }
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
    fn official_archive_extracts_into_an_existing_build_workspace() {
        use std::io::Write;
        let f = Fixture::new();
        let work = f.0.join("work");
        fs::create_dir(&work).unwrap();
        let archive = f.0.join("source.zip");
        let sha = "a".repeat(40);
        let mut zip = zip::ZipWriter::new(fs::File::create(&archive).unwrap());
        zip.start_file(
            format!("soundcraft-{sha}/Cargo.toml"),
            zip::write::SimpleFileOptions::default(),
        )
        .unwrap();
        zip.write_all(b"[workspace]").unwrap();
        zip.finish().unwrap();
        let project = extract_project(&archive, &work, "soundcraft", &sha, &f.job()).unwrap();
        assert_eq!(
            fs::read_to_string(project.join("Cargo.toml")).unwrap(),
            "[workspace]"
        );
        assert!(extract_project(&archive, &work, "soundcraft", &sha, &f.job()).is_err());
    }
    #[test]
    fn portable_native_install_and_backup_preserve_origin_and_location() {
        let f = Fixture::new();
        let paths = f.paths();
        files::write_json(
            &paths.at("manager-settings.json"),
            &crate::model::Preferences {
                release_format: "portable".into(),
                compress_backups: false,
                ..Default::default()
            },
        )
        .unwrap();
        let first = f.bundle("portable-first", "cadcraft", "0.2.0", &"a".repeat(40));
        let second = f.bundle("portable-next", "cadcraft", "0.3.0", &"b".repeat(40));
        let record = install(&paths, "cadcraft", &first, &f.job()).unwrap();
        assert_eq!(record.install_kind, "portable");
        install(&paths, "cadcraft", &second, &f.job()).unwrap();
        let backup = crate::backups::list(&paths, "cadcraft").unwrap().remove(0);
        restore(&paths, "cadcraft", &backup.path, &f.job()).unwrap();
        let record = crate::apps::installed(&paths, "cadcraft").unwrap();
        assert_eq!(record.source_commit, "a".repeat(40));
        assert_eq!(record.install_kind, "portable");
    }
    #[test]
    fn release_selection_handles_entire_catalog_and_absent_mac_assets() {
        use crate::model::{Asset, Preferences, Release};
        let prefs = Preferences {
            architecture: "arm64".into(),
            ..Default::default()
        };
        for app in crate::model::APPS {
            assert!(supported(app));
            let tag = if app == "artcraft" {
                "artcraft-v1.2.3"
            } else {
                "v1.2.3"
            };
            let name = if app == "artcraft" {
                "ArtCraft_1.2.3_universal.dmg".into()
            } else {
                format!(
                    "{}-1.2.3-macos-universal.dmg",
                    crate::model::repository(app)
                )
            };
            let asset = Asset {
                name: name.clone(),
                size: 1,
                browser_download_url: String::new(),
                digest: None,
            };
            let mut release = Release {
                tag_name: tag.into(),
                draft: false,
                prerelease: false,
                assets: vec![asset.clone()],
                body: None,
            };
            assert_eq!(
                release_asset(Some(&release), app, &prefs)
                    .unwrap()
                    .unwrap()
                    .name,
                name
            );
            release.assets.push(Asset {
                name: format!("{}-1.2.3-macos-aarch64.dmg", crate::model::repository(app)),
                ..asset.clone()
            });
            assert_eq!(
                release_asset(Some(&release), app, &prefs)
                    .unwrap()
                    .unwrap()
                    .name,
                name
            );
            release.assets = vec![Asset {
                name: "windows.zip".into(),
                ..asset.clone()
            }];
            assert!(release_asset(Some(&release), app, &prefs)
                .unwrap()
                .is_none());
            release.prerelease = true;
            release.assets = vec![asset];
            assert!(release_asset(Some(&release), app, &prefs)
                .unwrap()
                .is_none());
            assert!(release_asset(None, app, &prefs).unwrap().is_none());
        }
    }
    #[test]
    fn obsolete_repository_setting_is_never_retained() {
        let prefs: crate::model::BuilderPreferences =
            serde_json::from_value(serde_json::json!({"localRepositories":"/must/not/be/used"}))
                .unwrap();
        assert!(serde_json::to_value(prefs)
            .unwrap()
            .get("localRepositories")
            .is_none());
        assert_eq!(tauri_crate("artcraft").unwrap(), "crates/desktop/artcraft");
        assert_eq!(tauri_crate("artcraftx").unwrap(), "crates/artcraftx");
    }
    #[test]
    fn bundles_are_backed_up_and_restored_with_provenance_for_entire_catalog() {
        let f = Fixture::new();
        let paths = f.paths();
        let folder = f.0.join("Applications");
        fs::create_dir(&folder).unwrap();
        for app in crate::model::APPS {
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
