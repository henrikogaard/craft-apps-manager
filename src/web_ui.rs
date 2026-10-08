//! Local HTML interface. The webview never loads remote pages or executes shell input.
use anyhow::{bail, Context, Result};
use base64::{engine::general_purpose::STANDARD, Engine};
use craft_apps_manager::{
    apps, backups, builder, files,
    jobs::Job,
    macos_build,
    model::{self, BuilderPreferences, Paths, Preferences, APPS},
    platform, scheduler, self_update, tools, updates,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    path::Path,
    sync::atomic::Ordering,
    time::{Duration, Instant},
};
use tao::{
    dpi::LogicalSize,
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoopBuilder, EventLoopProxy},
    platform::macos::WindowBuilderExtMacOS,
    window::{Icon, Theme, WindowBuilder},
};

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default)]
struct Appearance {
    theme: String,
}
#[derive(Debug)]
enum UiEvent {
    Message(Value),
    Snapshot(Value),
    Feedback(String),
}
fn post(proxy: &EventLoopProxy<UiEvent>, event: UiEvent) {
    let _ = proxy.send_event(event);
}
fn png(bytes: &[u8]) -> String {
    format!("data:image/png;base64,{}", STANDARD.encode(bytes))
}
fn catalog() -> Value {
    let descriptions = [
        (
            "designcraft",
            "Design & layout",
            "Arrange layouts, pages and creative projects.",
            include_bytes!("../assets/app-icons/designcraft.png").as_slice(),
        ),
        (
            "effectcraft",
            "Motion & video",
            "Compose visual effects and motion graphics.",
            include_bytes!("../assets/app-icons/effectcraft.png").as_slice(),
        ),
        (
            "filmcraft",
            "Motion & video",
            "Cut, edit and assemble your next film.",
            include_bytes!("../assets/app-icons/filmcraft.png").as_slice(),
        ),
        (
            "lightcraft",
            "Photography",
            "Develop photographs and shape light and color.",
            include_bytes!("../assets/app-icons/lightcraft.png").as_slice(),
        ),
        (
            "photocraft",
            "Photography",
            "Edit images and bring your photos into focus.",
            include_bytes!("../assets/app-icons/photocraft.png").as_slice(),
        ),
        (
            "printcraft",
            "Documents",
            "Read, edit and work with PDF documents.",
            include_bytes!("../assets/app-icons/pdfcraft.png").as_slice(),
        ),
        (
            "vectorcraft",
            "Design & layout",
            "Draw and refine precise vector artwork.",
            include_bytes!("../assets/app-icons/vectorcraft.png").as_slice(),
        ),
        (
            "wordcraft",
            "Documents",
            "Write, format and compose your documents.",
            include_bytes!("../assets/app-icons/wordcraft.png").as_slice(),
        ),
        (
            "gridcraft",
            "Documents",
            "Organize data and explore spreadsheets.",
            include_bytes!("../assets/app-icons/gridcraft.png").as_slice(),
        ),
        (
            "deckcraft",
            "Documents",
            "Build presentations and share your ideas.",
            include_bytes!("../assets/app-icons/deckcraft.png").as_slice(),
        ),
        (
            "cadcraft",
            "3D & modeling",
            "Model precise shapes and structures.",
            include_bytes!("../assets/app-icons/cadcraft.png").as_slice(),
        ),
        (
            "soundcraft",
            "Audio",
            "Record, edit and arrange audio.",
            include_bytes!("../assets/app-icons/soundcraft.png").as_slice(),
        ),
        (
            "artcraft",
            "Creative tools",
            "Create with ArtCraft’s creative toolkit.",
            include_bytes!("../assets/icon.png").as_slice(),
        ),
        (
            "artcraftx",
            "Creative tools",
            "Explore the experimental ArtCraft X app.",
            include_bytes!("../assets/icon.png").as_slice(),
        ),
    ];
    Value::Array(descriptions.into_iter().map(|(id, category, description, bytes)| json!({"id":id,"name":model::title(id),"category":category,"description":description,"icon":png(bytes)})).collect())
}
fn html() -> String {
    include_str!("web/index.html")
        .replace("__CATALOG__", &catalog().to_string())
        .replace(
            "__MANAGER_ICON__",
            &png(include_bytes!("../assets/icon.png")),
        )
        .replace(
            "__BODY_FONT__",
            &STANDARD.encode(include_bytes!("../assets/fonts/DM-Sans.ttf")),
        )
        .replace(
            "__DISPLAY_FONT__",
            &STANDARD.encode(include_bytes!("../assets/fonts/Space-Grotesk.ttf")),
        )
}
fn snapshot(paths: &Paths) -> Result<Value> {
    let config = paths.config()?;
    let mut builds = BTreeMap::new();
    let mut backup_items = BTreeMap::new();
    for app in APPS {
        if let Some(out) = builder::history(paths, app) {
            let metadata: model::BuildInfo = files::read_json(&out.join("build-info.json"))?;
            let bundle = out.join(model::executable_name(app));
            let version =
                platform::bundle_value(&bundle, "CFBundleShortVersionString").unwrap_or_default();
            builds.insert(
                app,
                json!({"path":out,"commit":metadata.commit,"version":version}),
            );
        }
        backup_items.insert(app, backups::list(paths, app)?.into_iter().map(|b| json!({"name":b.path.file_name().unwrap_or_default().to_string_lossy(),"source":b.source})).collect::<Vec<_>>());
    }
    let appearance: Appearance = files::read_or_default(&paths.at("ui-settings.json"))?;
    let prefs = paths.preferences()?;
    let checks = craft_apps_manager::hourly::read(paths)?;
    // Only report a check that matches the installed version, so a stale result never shows.
    let updates: BTreeMap<_, _> = config
        .apps
        .iter()
        .filter(|a| !a.path.is_empty())
        .filter_map(|a| {
            let check = checks.get(&craft_apps_manager::hourly::key(&prefs, &a.name))?;
            (check.installed == a.version).then(|| {
                (
                    a.name.clone(),
                    json!({"latest":check.latest,"checked":check.checked}),
                )
            })
        })
        .collect();
    let launches: BTreeMap<String, i64> = files::read_or_default(&paths.at(LAUNCHES))?;
    Ok(
        json!({"apps":config.apps,"updates":updates,"launches":launches,"builds":builds,"backups":backup_items,"preferences":prefs,"buildPreferences":paths.builder_preferences()?,"sparkle":null,"ui":{"theme":if appearance.theme == "light" {"light"} else {"dark"}},"auto":scheduler::enabled(false),"autoSource":scheduler::enabled(true),"version":env!("CARGO_PKG_VERSION")}),
    )
}
const LAUNCHES: &str = "runtime/launches.json";
fn launch(paths: &Paths, app: &str) -> Result<()> {
    apps::launch(paths, app)?;
    let mut launches: BTreeMap<String, i64> = files::read_or_default(&paths.at(LAUNCHES))?;
    launches.insert(app.into(), chrono::Utc::now().timestamp());
    files::write_json(&paths.at(LAUNCHES), &launches)
}
fn refresh(paths: &Paths, proxy: &EventLoopProxy<UiEvent>) {
    let paths = paths.clone();
    let proxy = proxy.clone();
    std::thread::spawn(move || match snapshot(&paths) {
        Ok(value) => post(&proxy, UiEvent::Snapshot(value)),
        Err(e) => post(
            &proxy,
            UiEvent::Feedback(format!("Could not read library: {e:#}")),
        ),
    });
}
fn theme(paths: &Paths, value: &str) -> Result<()> {
    if !["dark", "light"].contains(&value) {
        bail!("Unknown appearance");
    }
    files::write_json(
        &paths.at("ui-settings.json"),
        &Appearance {
            theme: value.into(),
        },
    )
}
fn save_settings(paths: &Paths, value: &Value) -> Result<()> {
    let mut prefs: Preferences = serde_json::from_value(value["preferences"].clone())?;
    prefs.validate()?;
    let build: BuilderPreferences = serde_json::from_value(value["buildPreferences"].clone())?;
    if !(1..=100).contains(&build.log_size_mb) || build.log_archives > 5 {
        bail!("Invalid log settings");
    }
    let desired = [
        value["auto"]
            .as_bool()
            .context("Missing app check preference")?,
        value["autoSource"]
            .as_bool()
            .context("Missing source check preference")?,
    ];
    let previous = [scheduler::status(false)?, scheduler::status(true)?];
    let old_prefs = paths.preferences()?;
    let old_build = paths.builder_preferences()?;
    let result = (|| -> Result<()> {
        for source in [false, true] {
            let i = usize::from(source);
            if desired[i] != previous[i] {
                scheduler::set(paths, source, desired[i])?;
            }
        }
        files::write_json(&paths.at("manager-settings.json"), &prefs)?;
        files::write_json(&paths.at("builder-settings.json"), &build)
    })();
    if let Err(error) = result {
        let mut failures = Vec::new();
        for source in [false, true] {
            let i = usize::from(source);
            if desired[i] != previous[i] {
                if let Err(e) = scheduler::set(paths, source, previous[i]) {
                    failures.push(e.to_string());
                }
            }
        }
        for rollback in [
            files::write_json(&paths.at("manager-settings.json"), &old_prefs),
            files::write_json(&paths.at("builder-settings.json"), &old_build),
        ] {
            if let Err(e) = rollback {
                failures.push(e.to_string());
            }
        }
        if !failures.is_empty() {
            bail!(
                "Settings were not saved: {error:#}. Rollback needs attention: {}",
                failures.join("; ")
            );
        }
        return Err(error).context("Settings were not saved; previous settings restored");
    }
    if let (Some(checks), Some(downloads)) = (
        value["sparkleChecks"].as_bool(),
        value["sparkleDownloads"].as_bool(),
    ) {
        self_update::configure(checks, downloads);
    }
    Ok(())
}
fn selected_backup(paths: &Paths, app: &str, value: &Value) -> Result<backups::Backup> {
    let name = value["backupName"]
        .as_str()
        .context("Missing backup selection")?;
    let source = value["backupSource"]
        .as_bool()
        .context("Missing backup kind")?;
    backups::list(paths, app)?
        .into_iter()
        .find(|b| b.source == source && b.path.file_name().is_some_and(|n| n == name))
        .context("Backup is no longer available")
}

fn operation(
    paths: &Paths,
    app: &str,
    value: &Value,
    job: &Job,
    proxy: &EventLoopProxy<UiEvent>,
) -> Result<()> {
    match value["action"].as_str().context("Missing action")? {
        "install-latest" => macos_build::install_latest(paths, app, job),
        "install-selected" => macos_build::install_selected(paths, job),
        "update-all" => macos_build::install_available(paths, job),
        "build" => builder::build(paths, app, true, job),
        "setup" => tools::setup(paths, app, job),
        "install-build" => macos_build::install_build(paths, app, job),
        "check-source" => macos_build::check_source(paths, app, job),
        "check-updates" => craft_apps_manager::hourly::check_installed(paths, job),
        "check-release" => {
            let result = updates::check_app(paths, app)?;
            craft_apps_manager::hourly::record(
                paths,
                app,
                &apps::installed(paths, app)?.version,
                result.clone(),
            )?;
            post(
                proxy,
                UiEvent::Feedback(
                    result
                        .map(|v| format!("{} {v} is available", model::title(app)))
                        .unwrap_or_else(|| "No newer compatible release".into()),
                ),
            );
            Ok(())
        }
        "uninstall" => apps::uninstall(paths, app),
        "restore" => backups::restore(paths, app, &selected_backup(paths, app, value)?, job),
        "delete-backup" => {
            backups::delete_selected(paths, app, &[selected_backup(paths, app, value)?])
        }
        "fetch-sources" => updates::sources(paths, &paths.preferences()?.selected_sources, job),
        _ => bail!("Unknown operation"),
    }
}
fn emit(web: &wry::WebView, value: Value) {
    let _ = web.evaluate_script(&format!("window.receive({value})"));
}

pub fn run(paths: Paths) -> Result<()> {
    let event_loop = EventLoopBuilder::<UiEvent>::with_user_event().build();
    let proxy = event_loop.create_proxy();
    let pixels = image::load_from_memory(include_bytes!("../assets/icon.png"))?.into_rgba8();
    let icon = Icon::from_rgba(pixels.clone().into_raw(), pixels.width(), pixels.height())?;
    let window = WindowBuilder::new()
        .with_title("Craft Library")
        .with_decorations(true)
        .with_titlebar_transparent(true)
        .with_title_hidden(true)
        .with_fullsize_content_view(true)
        .with_traffic_light_inset(tao::dpi::LogicalPosition::new(14.0, 16.0))
        .with_theme(Some(
            if files::read_or_default::<Appearance>(&paths.at("ui-settings.json"))?.theme == "light"
            {
                Theme::Light
            } else {
                Theme::Dark
            },
        ))
        .with_inner_size(LogicalSize::new(1240.0, 840.0))
        .with_min_inner_size(LogicalSize::new(980.0, 660.0))
        .with_window_icon(Some(icon))
        .build(&event_loop)?;
    self_update::initialize();
    let native_menu = crate::native_menu::create()?;
    let menu_proxy = proxy.clone();
    muda::MenuEvent::set_event_handler(Some(move |event: muda::MenuEvent| {
        if event.id.0 == "updates" {
            post(
                &menu_proxy,
                UiEvent::Message(json!({"action":"manager-check"})),
            );
        }
        if event.id.0 == "quit" {
            post(&menu_proxy, UiEvent::Message(json!({"action":"close"})));
        }
    }));
    let ipc = proxy.clone();
    let web = wry::WebViewBuilder::new()
        .with_html(html())
        .with_navigation_handler(|url| url == "about:blank")
        .with_ipc_handler(move |request| {
            if request.body().len() < 64_000 {
                if let Ok(value) = serde_json::from_str(request.body()) {
                    post(&ipc, UiEvent::Message(value));
                }
            }
        })
        .build(&window)?;
    let mut job = Job::new(paths.at("logs/updates.log"), &paths.builder_preferences()?);
    let mut last_job = String::new();
    let mut last_poll = Instant::now();
    let mut was_busy = false;
    let mut closing = false;
    let mut startup_pending = true;
    let initial_app = std::env::args()
        .collect::<Vec<_>>()
        .windows(2)
        .find(|a| a[0] == "--app")
        .map(|a| a[1].clone());
    event_loop.run(move |event, _, flow| {
        let _keep_menu_alive = &native_menu;
        *flow = ControlFlow::WaitUntil(Instant::now() + Duration::from_millis(100));
        match event {
            Event::WindowEvent { event: WindowEvent::CloseRequested, .. } => { if job.state.lock().unwrap().busy { closing = true; job.cancel.store(true, Ordering::Relaxed); } else { *flow = ControlFlow::Exit; } },
            Event::UserEvent(UiEvent::Snapshot(value)) => {
                let mut value = value; value["sparkle"] = serde_json::to_value(self_update::settings()).unwrap_or(Value::Null);
                emit(&web, json!({"type":"snapshot","data":value}));
                if startup_pending {
                    startup_pending = false;
                    if let Some(app) = &initial_app { if model::valid_app(app).is_ok() { let _ = web.evaluate_script(&format!("choose({})", json!(app))); } }
                    let prefs = paths.preferences().unwrap_or_default();
                    if prefs.check_installed_apps_on_startup {
                        let p = paths.clone(); let tx = proxy.clone();
                        std::thread::spawn(move || { if let Err(e) = craft_apps_manager::hourly::run(&p, &Job::new(p.at("logs/checks.log"), &Default::default())) { post(&tx, UiEvent::Feedback(format!("Startup check failed: {e:#}"))); } refresh(&p, &tx); });
                    }
                }
            },
            Event::UserEvent(UiEvent::Feedback(message)) => emit(&web, json!({"type":"feedback","message":message})),
            Event::UserEvent(UiEvent::Message(value)) => {
                let result = (|| -> Result<()> {
                    let action = value["action"].as_str().context("Missing action")?;
                    match action {
                        "ready" | "refresh" => { refresh(&paths, &proxy); return Ok(()); },
                        "drag" => { window.drag_window()?; return Ok(()); },
                        "minimize" => { window.set_minimized(true); return Ok(()); },
                        "maximize" => { window.set_maximized(!window.is_maximized()); return Ok(()); },
                        "close" => { if job.state.lock().unwrap().busy { closing = true; job.cancel.store(true, Ordering::Relaxed); } else { *flow = ControlFlow::Exit; } return Ok(()); },
                        "cancel" => { job.cancel.store(true, Ordering::Relaxed); return Ok(()); },
                        "theme" => { let selected_theme = value["theme"].as_str().context("Missing theme")?;
                            theme(&paths, selected_theme)?;
                            window.set_theme(Some(if selected_theme == "light" { Theme::Light } else { Theme::Dark })); return Ok(()); },
                        "open-library" => return platform::open(&paths.root),
                        "open-log" => return platform::open(&job.log_path),
                        "fork" => return platform::open(Path::new(self_update::REPOSITORY)),
                        "manager-check" => return self_update::check(),
                        _ => {},
                    }
                    if job.state.lock().unwrap().busy { bail!("Wait for the current operation to finish"); }
                    if action == "settings" { save_settings(&paths, &value)?; refresh(&paths, &proxy); return Ok(()); }
                    let app = value["app"].as_str().context("Missing app")?.to_owned(); model::valid_app(&app)?;
                    match action {
                        "repository" => return platform::open(Path::new(&format!("https://github.com/storytold/{}", model::repository(&app)))),
                        "launch" => { launch(&paths, &app)?; refresh(&paths, &proxy); return Ok(()); },
                        "open-app" => return platform::open(Path::new(&apps::installed(&paths, &app)?.path)),
                        "open-build" => return platform::open(&builder::history(&paths, &app).context("No local build exists")?),
                        "launch-settings" => { emit(&web, json!({"type":"launch-settings","settings":apps::settings(&paths,&app)?})); return Ok(()); },
                        "save-launch-settings" => { let mut settings = apps::settings(&paths,&app)?; settings.arguments = value["arguments"].as_str().context("Missing arguments")?.lines().filter(|s| !s.is_empty()).map(str::to_owned).collect(); apps::save(&paths,&app,&settings)?; return Ok(()); },
                        "install-latest" | "install-selected" | "build" | "setup" | "install-build" | "check-source" | "check-release" | "check-updates" | "update-all" | "uninstall" | "restore" | "delete-backup" | "fetch-sources" => {},
                        _ => bail!("Unknown action"),
                    }
                    job = Job::new(paths.at(format!("logs/{app}.log")), &paths.builder_preferences()?);
                    let p = paths.clone(); let tx = proxy.clone();
                    self_update::set_busy(true);
                    job.spawn(move |job| operation(&p, &app, &value, &job, &tx));
                    Ok(())
                })();
                if let Err(e) = result { post(&proxy, UiEvent::Feedback(format!("{e:#}"))); }
            },
            Event::MainEventsCleared if last_poll.elapsed() >= Duration::from_millis(200) => {
                last_poll = Instant::now(); let state = job.state.lock().unwrap().clone();
                let value = json!({"type":"job","data":{"busy":state.busy,"stage":state.stage,"detail":state.detail,"progress":state.progress,"log":state.log,"outcome":state.outcome}});
                let encoded = value.to_string(); if encoded != last_job { emit(&web,value); last_job = encoded; }
                if was_busy && !state.busy { refresh(&paths, &proxy); }
                self_update::set_busy(state.busy);
                was_busy = state.busy;
                if closing && !state.busy { *flow = ControlFlow::Exit; }
            },
            _ => {},
        }
    });
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn catalog_and_local_document_are_complete_and_contain_no_placeholders() {
        let data = catalog();
        let ids: Vec<_> = data
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v["id"].as_str().unwrap())
            .collect();
        assert_eq!(ids, APPS);
        let source = html();
        assert!(!source.contains("__CATALOG__"));
        assert!(!source.contains("__BODY_FONT__"));
        assert!(source.contains("connect-src 'none'"));
    }
    #[test]
    fn appearance_only_accepts_the_two_supported_themes() {
        let root = std::env::temp_dir().join(format!("craft-theme-{}", uuid::Uuid::new_v4()));
        let paths = Paths::new(root.clone(), None);
        assert!(theme(&paths, "remote").is_err());
        theme(&paths, "light").unwrap();
        let saved: Appearance = files::read_json(&paths.at("ui-settings.json")).unwrap();
        assert_eq!(saved.theme, "light");
        std::fs::remove_dir_all(root).unwrap();
    }
}
