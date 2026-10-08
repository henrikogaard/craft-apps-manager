mod native_menu;
mod web_ui;
#[derive(Default, serde::Serialize, serde::Deserialize)]
pub struct Locations {
    pub root: Option<std::path::PathBuf>,
    pub tools: Option<std::path::PathBuf>,
}
use anyhow::{Context, Result};
use craft_apps_manager::{builder, files, jobs::Job, model::Paths, tools, updates};
use std::{fs, path::PathBuf};
fn main() {
    if let Err(e) = run() {
        let message = format!("{e:#}");
        let path = std::env::var_os("HOME").map(|home| {
            PathBuf::from(home)
                .join("Library/Application Support/Craft Apps Manager/logs/startup.log")
        });
        if let Some(path) = path {
            let _ = fs::create_dir_all(path.parent().unwrap());
            let _ = fs::write(path, &message);
        }
        eprintln!("{message}");
        if std::env::args().any(|a| {
            a == "--build-app"
                || a == "--install-build"
                || a == "--install-latest"
                || a == "--update"
                || a == "--update-source"
                || a == "--background"
                || a == "--check-app-updates"
                || a == "--check-source-updates"
        }) {
            std::process::exit(1);
        }
    }
}
fn run() -> Result<()> {
    let args: Vec<_> = std::env::args().collect();
    let arg = |key: &str| {
        args.iter()
            .position(|s| s == key)
            .and_then(|i| args.get(i + 1))
            .map(PathBuf::from)
    };
    let home = PathBuf::from(std::env::var_os("HOME").context("No macOS home directory")?)
        .join("Library/Application Support/Craft Apps Manager");
    let saved: Locations = files::read_or_default(&home.join("data-root.json"))?;
    let root = arg("--root").or(saved.root).unwrap_or_else(|| home.clone());
    let paths = Paths::new(root, arg("--tools").or(saved.tools));
    if let Some(i) = args
        .iter()
        .position(|s| s == "--build-app" || s == "--install-build" || s == "--install-latest")
    {
        let app = args.get(i + 1).context("Missing app name")?;
        craft_apps_manager::model::valid_app(app)?;
        let job = Job::new(
            paths.at(format!("logs/{app}.log")),
            &paths.builder_preferences()?,
        );
        if args[i] == "--build-app" {
            return builder::build(&paths, app, args.iter().any(|a| a == "--latest"), &job);
        }
        if args[i] == "--install-latest" {
            return craft_apps_manager::macos_build::install_latest(&paths, app, &job);
        }
        if craft_apps_manager::macos_build::supported(app) {
            return craft_apps_manager::macos_build::install_build(&paths, app, &job);
        }
        anyhow::bail!("Native bundle installation is available on macOS");
    }

    // Old scheduled tasks keep their command line after an executable update.
    // Route both legacy background commands and new commands to checks only.
    let background = args.iter().any(|s| s == "--background");
    if args
        .iter()
        .any(|s| s == "--check-app-updates" || s == "--check-source-updates")
        || (background
            && args
                .iter()
                .any(|s| s == "--update" || s == "--update-source"))
    {
        let job = Job::new(paths.at("logs/updates.log"), &paths.builder_preferences()?);
        if args
            .iter()
            .any(|s| s == "--check-source-updates" || s == "--update-source")
        {
            craft_apps_manager::hourly::sources(&paths, &job)?;
        } else {
            craft_apps_manager::hourly::run(&paths, &job)?;
        }
        return Ok(());
    }
    if args
        .iter()
        .any(|s| s == "--update" || s == "--update-source")
    {
        let job = Job::new(paths.at("logs/updates.log"), &paths.builder_preferences()?);
        if args.iter().any(|s| s == "--update-source") {
            updates::sources(&paths, &paths.preferences()?.selected_sources, &job)?;
        } else {
            craft_apps_manager::macos_build::install_selected(&paths, &job)?;
        }
        return Ok(());
    }
    if args.iter().any(|s| s == "--diagnostics") {
        let config = paths.config()?;
        files::write_json(
            &home.join("diagnostics.json"),
            &serde_json::json!({"root":paths.root,"tools":paths.tools,"cargo":tools::cargo(&paths),"apps":config.apps,"lastFilmCraftBuild":builder::history(&paths,"filmcraft"),"lastArtCraftXBuild":builder::history(&paths,"artcraftx")}),
        )?;
        return Ok(());
    }
    web_ui::run(paths)
}
