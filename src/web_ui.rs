//! Local HTML interface. The webview never loads remote pages or executes shell input.
use anyhow::{bail, Context, Result};
use base64::{engine::general_purpose::STANDARD, Engine};
use craft_apps_manager::{
    activity, apps, backups, builder, dock, files,
    jobs::Job,
    library, macos_build,
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
    window: Option<Frame>,
}
/// Window position and size in points, restored on the next launch.
#[derive(Clone, Copy, Default, Serialize, Deserialize)]
struct Frame {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    maximized: bool,
}
#[derive(Debug)]
enum UiEvent {
    Message(Value),
    Snapshot(Value),
    Feedback(String),
    Running(Vec<String>),
    /// A ready-made message for the page.
    Emit(Value),
}
fn post(proxy: &EventLoopProxy<UiEvent>, event: UiEvent) {
    let _ = proxy.send_event(event);
}
fn png(bytes: &[u8]) -> String {
    format!("data:image/png;base64,{}", STANDARD.encode(bytes))
}
/// App id, category, description and icon, in library order.
const CATALOG: [(&str, &str, &str, &[u8]); 14] = [
    (
        "designcraft",
        "Design & layout",
        "Arrange layouts, pages and creative projects.",
        include_bytes!("../assets/app-icons/designcraft.png"),
    ),
    (
        "effectcraft",
        "Motion & video",
        "Compose visual effects and motion graphics.",
        include_bytes!("../assets/app-icons/effectcraft.png"),
    ),
    (
        "filmcraft",
        "Motion & video",
        "Cut, edit and assemble your next film.",
        include_bytes!("../assets/app-icons/filmcraft.png"),
    ),
    (
        "lightcraft",
        "Photography",
        "Develop photographs and shape light and color.",
        include_bytes!("../assets/app-icons/lightcraft.png"),
    ),
    (
        "photocraft",
        "Photography",
        "Edit images and bring your photos into focus.",
        include_bytes!("../assets/app-icons/photocraft.png"),
    ),
    (
        "printcraft",
        "Documents",
        "Read, edit and work with PDF documents.",
        include_bytes!("../assets/app-icons/pdfcraft.png"),
    ),
    (
        "vectorcraft",
        "Design & layout",
        "Draw and refine precise vector artwork.",
        include_bytes!("../assets/app-icons/vectorcraft.png"),
    ),
    (
        "wordcraft",
        "Documents",
        "Write, format and compose your documents.",
        include_bytes!("../assets/app-icons/wordcraft.png"),
    ),
    (
        "gridcraft",
        "Documents",
        "Organize data and explore spreadsheets.",
        include_bytes!("../assets/app-icons/gridcraft.png"),
    ),
    (
        "deckcraft",
        "Documents",
        "Build presentations and share your ideas.",
        include_bytes!("../assets/app-icons/deckcraft.png"),
    ),
    (
        "cadcraft",
        "3D & modeling",
        "Model precise shapes and structures.",
        include_bytes!("../assets/app-icons/cadcraft.png"),
    ),
    (
        "soundcraft",
        "Audio",
        "Record, edit and arrange audio.",
        include_bytes!("../assets/app-icons/soundcraft.png"),
    ),
    (
        "artcraft",
        "Creative tools",
        "Create with ArtCraft’s creative toolkit.",
        include_bytes!("../assets/icon.png"),
    ),
    (
        "artcraftx",
        "Creative tools",
        "Explore the experimental ArtCraft X app.",
        include_bytes!("../assets/icon.png"),
    ),
];
fn catalog() -> Value {
    Value::Array(CATALOG.iter().map(|(id, category, description, bytes)| json!({"id":id,"name":model::title(id),"category":category,"description":description,"icon":png(bytes)})).collect())
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
                    json!({"latest":check.latest,"checked":check.checked,"notes":check.notes}),
                )
            })
        })
        .collect();
    let launches: BTreeMap<String, i64> = files::read_or_default(&paths.at(LAUNCHES))?;
    Ok(
        json!({"apps":config.apps,"updates":updates,"launches":launches,"activity":activity::read(paths).unwrap_or_default(),"startAtLogin":dock::login_enabled(),"library":paths.root,"defaultLibrary":model::Locations::home()?,"installFolder":paths.install_folder()?,"builds":builds,"backups":backup_items,"preferences":prefs,"buildPreferences":paths.builder_preferences()?,"sparkle":null,"ui":{"theme":if appearance.theme == "light" {"light"} else {"dark"}},"auto":scheduler::enabled(false),"autoSource":scheduler::enabled(true),"version":env!("CARGO_PKG_VERSION")}),
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
    let file = paths.at("ui-settings.json");
    let mut appearance: Appearance = files::read_or_default(&file)?;
    appearance.theme = value.into();
    files::write_json(&file, &appearance)
}
/// Remembers where the window was, for the next launch.
fn save_frame(paths: &Paths, window: &tao::window::Window) {
    let scale = window.scale_factor();
    let (Ok(position), size) = (window.outer_position(), window.inner_size()) else {
        return;
    };
    let file = paths.at("ui-settings.json");
    let mut appearance: Appearance = files::read_or_default(&file).unwrap_or_default();
    let maximized = window.is_maximized();
    let previous = appearance.window.filter(|_| maximized);
    appearance.window = Some(previous.map_or(
        Frame {
            x: f64::from(position.x) / scale,
            y: f64::from(position.y) / scale,
            width: f64::from(size.width) / scale,
            height: f64::from(size.height) / scale,
            maximized,
        },
        |f| Frame { maximized, ..f },
    ));
    let _ = files::write_json(&file, &appearance);
}
fn save_settings(paths: &Paths, value: &Value) -> Result<()> {
    let mut prefs: Preferences = serde_json::from_value(value["preferences"].clone())?;
    let old_prefs = paths.preferences()?;
    if !old_prefs.install_folder.is_empty()
        && old_prefs.install_folder != prefs.install_folder.trim()
    {
        prefs
            .previous_install_folders
            .insert(0, old_prefs.install_folder.clone());
    }
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
            let update = updates::newer_release(paths, app)?;
            let result = update.as_ref().map(|u| u.version.clone());
            craft_apps_manager::hourly::record(
                paths,
                app,
                &apps::installed(paths, app)?.version,
                update,
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
        "delete-build" => {
            let folder = builder::history(paths, app).context("No local build exists")?;
            builder::delete_local(paths, app, &folder)?;
            job.log(&format!("Deleted the local build of {}", model::title(app)));
            Ok(())
        }
        "restore" => backups::restore(paths, app, &selected_backup(paths, app, value)?, job),
        "delete-backup" => {
            backups::delete_selected(paths, app, &[selected_backup(paths, app, value)?])
        }
        "fetch-sources" => updates::sources(paths, &paths.preferences()?.selected_sources, job),
        "move-app" => macos_build::move_app(paths, app, job).map(|_| ()),
        "clean-builds" => {
            builder::clean(paths)?;
            job.log("Deleted build caches and extracted source");
            Ok(())
        }
        "clear-backups" => {
            backups::clear(paths)?;
            job.log("Deleted app and source backups");
            Ok(())
        }
        "clear-downloads" => {
            library::clear_downloads(paths)?;
            job.log("Deleted downloaded installers");
            Ok(())
        }
        "move-apps" => macos_build::move_apps(paths, job),
        "move-library" => {
            let to = value["path"].as_str().context("Missing folder")?;
            library::move_library(paths, Path::new(to), job)
        }
        _ => bail!("Unknown operation"),
    }
}
/// Mirrors the library in the Dock: installed apps in its menu, update count on its badge.
fn update_dock(value: &Value) {
    let installed: Vec<&str> = value["apps"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|a| a["path"].as_str().is_some_and(|p| !p.is_empty()))
        .filter_map(|a| a["name"].as_str())
        .collect();
    let apps: Vec<_> = CATALOG
        .iter()
        .filter(|(id, ..)| installed.contains(id))
        .map(|(id, _, _, bytes)| (*id, model::title(id), *bytes))
        .collect();
    dock::set_apps(&apps);
    let updates = value["updates"].as_object().map_or(0, |u| {
        u.values().filter(|c| c["latest"].is_string()).count()
    });
    dock::set_badge(updates);
}
/// Starts a fresh Craft Library once this one has exited, so a new library folder takes effect.
fn relaunch() -> Result<()> {
    let exe = std::env::current_exe()?;
    let target = exe
        .ancestors()
        .find(|p| p.extension().is_some_and(|e| e == "app"))
        .map_or(exe.clone(), Path::to_path_buf);
    let opener = if target == exe {
        "\"$1\""
    } else {
        "/usr/bin/open -n \"$1\""
    };
    std::process::Command::new("/bin/sh")
        .arg("-c")
        .arg(format!("sleep 1; {opener}"))
        .arg("sh")
        .arg(&target)
        .spawn()?;
    Ok(())
}
fn emit(web: &wry::WebView, value: Value) {
    let _ = web.evaluate_script(&format!("window.receive({value})"));
}

pub fn run(paths: Paths) -> Result<()> {
    let event_loop = EventLoopBuilder::<UiEvent>::with_user_event().build();
    let proxy = event_loop.create_proxy();
    let pixels = image::load_from_memory(include_bytes!("../assets/icon.png"))?.into_rgba8();
    let icon = Icon::from_rgba(pixels.clone().into_raw(), pixels.width(), pixels.height())?;
    let appearance: Appearance = files::read_or_default(&paths.at("ui-settings.json"))?;
    // Only reuse a saved position that still lands on a connected display.
    let frame = appearance.window.filter(|f| {
        f.width >= 980.0
            && f.height >= 660.0
            && event_loop.available_monitors().any(|m| {
                let scale = m.scale_factor();
                let (p, s) = (m.position(), m.size());
                let (left, top) = (f64::from(p.x) / scale, f64::from(p.y) / scale);
                let (right, bottom) = (
                    left + f64::from(s.width) / scale,
                    top + f64::from(s.height) / scale,
                );
                f.x + 100.0 < right
                    && f.x + f.width - 100.0 > left
                    && f.y >= top - 1.0
                    && f.y + 40.0 < bottom
            })
    });
    let mut builder = WindowBuilder::new();
    if let Some(f) = frame {
        builder = builder
            .with_position(tao::dpi::LogicalPosition::new(f.x, f.y))
            .with_maximized(f.maximized);
    }
    let window = builder
        .with_title("Craft Library")
        .with_decorations(true)
        .with_titlebar_transparent(true)
        .with_title_hidden(true)
        .with_fullsize_content_view(true)
        .with_traffic_light_inset(tao::dpi::LogicalPosition::new(14.0, 16.0))
        .with_theme(Some(if appearance.theme == "light" {
            Theme::Light
        } else {
            Theme::Dark
        }))
        .with_inner_size(frame.map_or(LogicalSize::new(1240.0, 840.0), |f| {
            LogicalSize::new(f.width, f.height)
        }))
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
    let mut restart_after_job = false;
    let mut current = (String::new(), String::new());
    let mut closing = false;
    let mut startup_pending = true;
    let mut running: Vec<String> = Vec::new();
    let mut last_running_poll = Instant::now() - Duration::from_secs(10);
    let polling = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
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
                update_dock(&value);
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
            Event::UserEvent(UiEvent::Emit(value)) => emit(&web, value),
            Event::LoopDestroyed => save_frame(&paths, &window),
            Event::UserEvent(UiEvent::Running(apps)) => if apps != running { emit(&web, json!({"type":"running","apps":apps})); running = apps; },
            Event::NewEvents(tao::event::StartCause::Init) => {
                let launcher = std::sync::Mutex::new(proxy.clone());
                dock::start(move |app| {
                    let message = if app == "__show" { json!({"action":"show-window"}) } else { json!({"action":"launch","app":app}) };
                    if let Ok(proxy) = launcher.lock() { post(&proxy, UiEvent::Message(message)); }
                });
                dock::set_menu_bar(paths.preferences().is_ok_and(|p| p.menu_bar_icon));
            },
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
                        "show-window" => { window.set_visible(true); window.set_minimized(false); window.set_focus(); return Ok(()); },
                        "disk-usage" => {
                            let p = paths.clone(); let tx = proxy.clone();
                            std::thread::spawn(move || {
                                let parts: Vec<_> = library::usage(&p).into_iter().map(|(name, bytes)| json!({"name":name,"bytes":bytes})).collect();
                                post(&tx, UiEvent::Emit(json!({"type":"disk-usage","parts":parts})));
                            });
                            return Ok(());
                        },
                        "theme" => { let selected_theme = value["theme"].as_str().context("Missing theme")?;
                            theme(&paths, selected_theme)?;
                            window.set_theme(Some(if selected_theme == "light" { Theme::Light } else { Theme::Dark })); return Ok(()); },
                        "open-library" => return platform::open(&paths.root),
                        "choose-folder" => {
                            let purpose = value["purpose"].as_str().context("Missing purpose")?;
                            let (message, initial) = match purpose {
                                "library" => ("Choose a folder for the Craft Library library. Downloads, sources, builds, backups and logs go here.", paths.root.clone()),
                                "apps" => ("Choose where Craft apps are installed.", paths.install_folder()?),
                                _ => bail!("Unknown folder"),
                            };
                            if let Some(path) = dock::choose_folder(message, &initial) {
                                emit(&web, json!({"type":"folder-chosen","purpose":purpose,"path":path}));
                            }
                            return Ok(());
                        },
                        "restart" => { relaunch()?; *flow = ControlFlow::Exit; return Ok(()); },
                        "open-log" => return platform::open(&job.log_path),
                        "fork" => return platform::open(Path::new(self_update::REPOSITORY)),
                        "manager-check" => return self_update::check(),
                        _ => {},
                    }
                    if job.state.lock().unwrap().busy { bail!("Wait for the current operation to finish"); }
                    if action == "settings" {
                        save_settings(&paths, &value)?;
                        dock::set_menu_bar(paths.preferences()?.menu_bar_icon);
                        if let Some(on) = value["startAtLogin"].as_bool() { if on != dock::login_enabled() { dock::set_login(on)?; } }
                        refresh(&paths, &proxy);
                        return Ok(());
                    }
                    if action == "use-library" { library::use_library(Path::new(value["path"].as_str().context("Missing folder")?))?; relaunch()?; *flow = ControlFlow::Exit; return Ok(()); }
                    let app = value["app"].as_str().context("Missing app")?.to_owned(); model::valid_app(&app)?;
                    match action {
                        "repository" => return platform::open(Path::new(&format!("https://github.com/storytold/{}", model::repository(&app)))),
                        "launch" => { launch(&paths, &app)?; refresh(&paths, &proxy); return Ok(()); },
                        "open-app" => return platform::open(Path::new(&apps::installed(&paths, &app)?.path)),
                        "open-build" => return platform::open(&builder::history(&paths, &app).context("No local build exists")?),
                        "launch-settings" => { emit(&web, json!({"type":"launch-settings","settings":apps::settings(&paths,&app)?})); return Ok(()); },
                        "launch-build" => return builder::launch_local(&paths, &app),
                        "build-launch-settings" => { emit(&web, json!({"type":"launch-settings","build":true,"settings":builder::launch_options(&paths,&app)?})); return Ok(()); },
                        "save-build-launch-settings" => { let mut settings = builder::launch_options(&paths,&app)?; settings.arguments = value["arguments"].as_str().context("Missing arguments")?.lines().filter(|s| !s.is_empty()).map(str::to_owned).collect(); builder::save_launch_options(&paths,&app,&settings)?; return Ok(()); },
                        "release-notes" => return platform::open(Path::new(&format!("https://github.com/storytold/{}/releases/latest", model::repository(&app)))),
                        "save-launch-settings" => { let mut settings = apps::settings(&paths,&app)?; settings.arguments = value["arguments"].as_str().context("Missing arguments")?.lines().filter(|s| !s.is_empty()).map(str::to_owned).collect(); apps::save(&paths,&app,&settings)?; return Ok(()); },
                        "install-latest" | "install-selected" | "build" | "setup" | "install-build" | "check-source" | "check-release" | "check-updates" | "update-all" | "delete-build" | "move-app" | "move-apps" | "move-library" | "clean-builds" | "clear-backups" | "clear-downloads" | "uninstall" | "restore" | "delete-backup" | "fetch-sources" => {},
                        _ => bail!("Unknown action"),
                    }
                    // The library's own logs move with it, so its move logs to the temporary folder.
                    job = Job::new(if action == "move-library" { std::env::temp_dir().join("craft-library-move.log") } else { paths.at(format!("logs/{app}.log")) }, &paths.builder_preferences()?);
                    restart_after_job = action == "move-library";
                    current = (action.to_owned(), app.clone());
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
                if was_busy && !state.busy {
                    // Quick lookups that found nothing stay out of the history.
                    if !["check-release", "check-source"].contains(&current.0.as_str()) || state.stage != "Complete" {
                        let entry = activity::Entry { time: chrono::Utc::now().timestamp(), action: current.0.clone(), app: current.1.clone(), stage: state.stage.clone(), lines: activity::summary(&state.log), error: if state.stage == "Failed" { state.outcome.clone() } else { String::new() } };
                        // After a library move, the history file lives in the new library.
                        let target = if current.0 == "move-library" && state.stage == "Complete" { None } else { Some(&paths) };
                        if let Some(target) = target { if let Err(e) = activity::record(target, entry) { post(&proxy, UiEvent::Feedback(format!("Could not save activity: {e:#}"))); } }
                    }
                    if restart_after_job && state.outcome == "Success" {
                        if let Err(e) = relaunch() { post(&proxy, UiEvent::Feedback(format!("Restart Craft Library to finish: {e:#}"))); } else { *flow = ControlFlow::Exit; }
                    } else {
                        refresh(&paths, &proxy);
                    }
                    restart_after_job = false;
                }
                self_update::set_busy(state.busy);
                was_busy = state.busy;
                if closing && !state.busy { *flow = ControlFlow::Exit; }
                // Which Craft apps are open, for the Running dots; one process listing every few seconds.
                if last_running_poll.elapsed() >= Duration::from_secs(3) && !polling.swap(true, Ordering::AcqRel) {
                    last_running_poll = Instant::now();
                    let tx = proxy.clone(); let polling = polling.clone();
                    std::thread::spawn(move || { if let Ok(apps) = platform::running_apps(APPS) { post(&tx, UiEvent::Running(apps)); } polling.store(false, Ordering::Release); });
                }
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
