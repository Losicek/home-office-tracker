mod db;
mod export;
mod i18n;
pub mod platform;
mod projects;
mod report;
mod sync;
mod tracker;
mod tray;

use chrono::NaiveDate;
use serde::Serialize;
use std::sync::Mutex;
use std::time::Duration;
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter, Manager, RunEvent, WindowEvent};
use tauri_plugin_notification::NotificationExt;
use tracker::{Settings, StatusView, TickEvent, Tracker};

struct AppState(Mutex<Tracker>);

/// Stav synchronizace přes iCloud (mimo tracker, aby se nečekalo na zámek).
#[derive(Default)]
struct SyncState {
    /// Kontejner iCloudu; `None` = zatím nezjištěno nebo nedostupné.
    root: Option<std::path::PathBuf>,
    /// Zjišťovalo se a kontejner není (iCloud vypnutý, nepodepsaná appka…).
    unavailable: bool,
    last_sync: Option<i64>,
    error: Option<String>,
}

struct SyncHandle(Mutex<SyncState>);

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DeviceView {
    name: String,
    updated_at: i64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SyncStatusView {
    /// Platforma synchronizaci umí (macOS).
    supported: bool,
    /// Kontejner iCloudu je k dispozici (None = ještě nezjištěno).
    available: Option<bool>,
    enabled: bool,
    last_sync: Option<i64>,
    error: Option<String>,
    devices: Vec<DeviceView>,
}

type CmdResult<T> = Result<T, String>;

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

fn parse_date(s: &str) -> CmdResult<NaiveDate> {
    NaiveDate::parse_from_str(s, "%Y-%m-%d").map_err(err)
}

/// Provede akci nad trackerem, dá vědět oknu a vrátí nový stav.
fn with_tracker(
    app: &AppHandle,
    f: impl FnOnce(&mut Tracker, i64) -> rusqlite::Result<()>,
) -> CmdResult<StatusView> {
    let state = app.state::<AppState>();
    let mut tracker = state.0.lock().unwrap();
    let now = now_ms();
    f(&mut tracker, now).map_err(err)?;
    let view = tracker.view(now).map_err(err)?;
    let texts = i18n::texts(tracker.lang());
    let names = projects::active_names(&tracker.conn).unwrap_or_default();
    drop(tracker);
    tray::refresh(app, &view, texts, &names);
    let _ = app.emit("tracker-changed", ());
    Ok(view)
}

#[tauri::command]
fn get_status(state: tauri::State<AppState>) -> CmdResult<StatusView> {
    state.0.lock().unwrap().view(now_ms()).map_err(err)
}

// Aplikaci v popředí doplní nejbližší tick (během kliknutí je vpředu stejně
// tahle appka).
#[tauri::command]
fn start_work(app: AppHandle, project: Option<i64>) -> CmdResult<StatusView> {
    with_tracker(&app, |t, now| t.start(now, None, project))
}

#[tauri::command]
fn switch_project(app: AppHandle, project: Option<i64>) -> CmdResult<StatusView> {
    with_tracker(&app, |t, now| t.switch_project(now, None, project))
}

#[tauri::command]
fn list_projects(state: tauri::State<AppState>) -> CmdResult<Vec<projects::Project>> {
    projects::list(&state.0.lock().unwrap().conn, now_ms()).map_err(err)
}

#[tauri::command]
fn create_project(app: AppHandle, name: String, color: String) -> CmdResult<i64> {
    let state = app.state::<AppState>();
    let id = projects::create(&state.0.lock().unwrap().conn, &name, &color, now_ms())?;
    refresh_tray_now(&app);
    let _ = app.emit("tracker-changed", ());
    Ok(id)
}

#[tauri::command]
fn update_project(
    app: AppHandle,
    id: i64,
    name: String,
    color: String,
    archived: bool,
) -> CmdResult<()> {
    let state = app.state::<AppState>();
    projects::update(
        &state.0.lock().unwrap().conn,
        id,
        &name,
        &color,
        archived,
        now_ms(),
    )?;
    let _ = app.emit("tracker-changed", ());
    Ok(())
}

#[tauri::command]
fn pause_work(app: AppHandle) -> CmdResult<StatusView> {
    with_tracker(&app, |t, now| t.pause(now))
}

#[tauri::command]
fn resume_work(app: AppHandle) -> CmdResult<StatusView> {
    with_tracker(&app, |t, now| t.resume(now, None))
}

#[tauri::command]
fn end_work(app: AppHandle) -> CmdResult<StatusView> {
    with_tracker(&app, |t, now| t.end(now))
}

#[tauri::command]
fn get_report(
    state: tauri::State<AppState>,
    from: String,
    to: String,
    project: Option<String>,
) -> CmdResult<report::Report> {
    let tracker = state.0.lock().unwrap();
    report::build(
        &tracker.conn,
        parse_date(&from)?,
        parse_date(&to)?,
        now_ms(),
        &report::ProjectFilter::parse(project.as_deref()),
    )
    .map_err(err)
}

#[tauri::command]
fn export_report(
    state: tauri::State<AppState>,
    path: String,
    format: String,
    from: String,
    to: String,
    project: Option<String>,
) -> CmdResult<()> {
    let tracker = state.0.lock().unwrap();
    let filter = report::ProjectFilter::parse(project.as_deref());
    let data = report::build(
        &tracker.conn,
        parse_date(&from)?,
        parse_date(&to)?,
        now_ms(),
        &filter,
    )
    .map_err(err)?;
    let employee = tracker.settings.employee_name.clone();
    let texts = i18n::texts(tracker.lang());
    let project_label = match filter {
        report::ProjectFilter::All => texts.all_projects.to_string(),
        report::ProjectFilter::NoProject => texts.no_project.to_string(),
        report::ProjectFilter::Project(id) => projects::get_ref(&tracker.conn, id)
            .map_err(err)?
            .map(|p| p.name)
            .unwrap_or_default(),
    };
    drop(tracker);
    match format.as_str() {
        "csv" => std::fs::write(&path, export::to_csv(&data, &employee, texts)).map_err(err),
        "xlsx" => export::to_xlsx(
            &data,
            &employee,
            &project_label,
            texts,
            std::path::Path::new(&path),
        )
        .map_err(err),
        other => Err(format!("Neznámý formát exportu: {other}")),
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SettingsView {
    #[serde(flatten)]
    settings: Settings,
    /// Jazyk, který se opravdu použije (i když je nastaveno „system“).
    resolved_language: &'static str,
}

fn settings_view(tracker: &Tracker) -> SettingsView {
    SettingsView {
        settings: tracker.settings.clone(),
        resolved_language: tracker.lang(),
    }
}

#[tauri::command]
fn sync_status(app: AppHandle) -> CmdResult<SyncStatusView> {
    let enabled = app
        .state::<AppState>()
        .0
        .lock()
        .unwrap()
        .settings
        .icloud_sync;
    let devices = {
        let state = app.state::<AppState>();
        let tracker = state.0.lock().unwrap();
        let mut stmt = tracker
            .conn
            .prepare("SELECT name, updated_at FROM devices ORDER BY name")
            .map_err(err)?;
        let rows = stmt
            .query_map([], |r| {
                Ok(DeviceView {
                    name: r.get(0)?,
                    updated_at: r.get(1)?,
                })
            })
            .map_err(err)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(err)?;
        rows
    };
    let sync = app.state::<SyncHandle>();
    let sync = sync.0.lock().unwrap();
    Ok(SyncStatusView {
        supported: cfg!(target_os = "macos"),
        available: if sync.root.is_some() {
            Some(true)
        } else if sync.unavailable {
            Some(false)
        } else {
            None
        },
        enabled,
        last_sync: sync.last_sync,
        error: sync.error.clone(),
        devices,
    })
}

/// Synchronizace hned (tlačítko v Nastavení); běží mimo hlavní vlákno.
#[tauri::command]
async fn sync_now(app: AppHandle) -> CmdResult<()> {
    tauri::async_runtime::spawn_blocking(move || run_sync(&app))
        .await
        .map_err(err)
}

/// Jeden průchod synchronizace: zjistit kontejner, zapsat svoje, načíst cizí.
fn run_sync(app: &AppHandle) {
    let enabled = app
        .state::<AppState>()
        .0
        .lock()
        .unwrap()
        .settings
        .icloud_sync;
    if !enabled {
        return;
    }
    let root = {
        let sync = app.state::<SyncHandle>();
        let known = sync.0.lock().unwrap().root.clone();
        match known {
            Some(root) => Some(root),
            None => {
                // Může trvat — proto bez držení zámku.
                let found = platform::icloud_container().map(|c| c.join("Data"));
                let mut s = sync.0.lock().unwrap();
                s.root = found.clone();
                s.unavailable = found.is_none();
                found
            }
        }
    };
    let Some(root) = root else { return };
    let device_name = platform::device_name();
    let result = {
        let state = app.state::<AppState>();
        let tracker = state.0.lock().unwrap();
        sync::export(&tracker.conn, &root, &device_name, now_ms())
            .and_then(|_| sync::import(&tracker.conn, &root))
    };
    let sync = app.state::<SyncHandle>();
    let mut s = sync.0.lock().unwrap();
    match result {
        Ok(changed) => {
            s.last_sync = Some(now_ms());
            s.error = None;
            drop(s);
            if changed {
                let _ = app.emit("tracker-changed", ());
            }
        }
        Err(e) => s.error = Some(e),
    }
}

fn spawn_sync(app: AppHandle) {
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(5));
        loop {
            run_sync(&app);
            std::thread::sleep(Duration::from_secs(60));
        }
    });
}

/// Vestavěné aktualizace (GitHub); ve verzi pro App Store vypnuté.
#[tauri::command]
fn updates_enabled() -> bool {
    cfg!(not(feature = "app-store"))
}

#[tauri::command]
fn get_settings(state: tauri::State<AppState>) -> SettingsView {
    settings_view(&state.0.lock().unwrap())
}

#[tauri::command]
fn save_settings(app: AppHandle, settings: Settings) -> CmdResult<SettingsView> {
    let state = app.state::<AppState>();
    let mut tracker = state.0.lock().unwrap();
    tracker.save_settings(settings).map_err(err)?;
    let view = settings_view(&tracker);
    drop(tracker);
    if view.settings.icloud_sync {
        let app = app.clone();
        std::thread::spawn(move || run_sync(&app));
    }
    refresh_tray_now(&app);
    Ok(view)
}

fn show_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

/// Obnoví menu v liště podle aktuálního stavu (např. po změně jazyka nebo
/// projektů).
fn refresh_tray_now(app: &AppHandle) {
    let state = app.state::<AppState>();
    let tracker = state.0.lock().unwrap();
    let Ok(view) = tracker.view(now_ms()) else {
        return;
    };
    let texts = i18n::texts(tracker.lang());
    let names = projects::active_names(&tracker.conn).unwrap_or_default();
    drop(tracker);
    tray::refresh(app, &view, texts, &names);
}

fn handle_tray_action(app: &AppHandle, action: tray::Action) {
    use tray::Action;
    let result = match action {
        Action::StartLast => with_tracker(app, |t, now| {
            let last = t.view(now)?.project.map(|p| p.id);
            t.start(now, None, last)
        }),
        Action::Start(project) => with_tracker(app, |t, now| t.start(now, None, project)),
        Action::Switch(project) => with_tracker(app, |t, now| t.switch_project(now, None, project)),
        Action::Pause => with_tracker(app, |t, now| t.pause(now)),
        Action::Resume => with_tracker(app, |t, now| t.resume(now, None)),
        Action::End => with_tracker(app, |t, now| t.end(now)),
        Action::Show => {
            show_main_window(app);
            return;
        }
        Action::Quit => {
            app.exit(0);
            return;
        }
    };
    if let Err(e) = result {
        eprintln!("akce z lišty selhala: {e}");
    }
}

/// Na macOS drží `NSWorkspace` aktuální aplikaci v popředí jen díky run loopu
/// hlavního vlákna, takže se ptáme tam.
fn frontmost_app_on_main(app: &AppHandle) -> Option<String> {
    let (tx, rx) = std::sync::mpsc::channel();
    app.run_on_main_thread(move || {
        let _ = tx.send(platform::frontmost_app());
    })
    .ok()?;
    rx.recv_timeout(Duration::from_secs(2)).ok().flatten()
}

fn spawn_ticker(app: AppHandle) {
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_secs(1));
        let idle = platform::idle_seconds();
        let current = frontmost_app_on_main(&app);
        let now = now_ms();

        let state = app.state::<AppState>();
        let mut tracker = state.0.lock().unwrap();
        let event = match tracker.tick(now, idle, current) {
            Ok(event) => event,
            Err(e) => {
                eprintln!("tick selhal: {e}");
                continue;
            }
        };
        let view = tracker.view(now).ok();
        let idle_minutes = tracker.settings.idle_minutes;
        let texts = i18n::texts(tracker.lang());
        let names = projects::active_names(&tracker.conn).unwrap_or_default();
        drop(tracker);

        if let Some(view) = &view {
            tray::refresh(&app, view, texts, &names);
        }
        if let Some(TickEvent::AutoPaused { since }) = event {
            let since = chrono::DateTime::from_timestamp_millis(since)
                .map(|t| t.with_timezone(&chrono::Local).format("%H:%M").to_string())
                .unwrap_or_default();
            let _ = app
                .notification()
                .builder()
                .title(texts.notif_title)
                .body(texts.notification(idle_minutes, &since))
                .show();
            show_main_window(&app);
            let _ = app.emit("tracker-changed", ());
        }
    });
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            show_main_window(app);
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_process::init())
        .setup(|app| {
            #[cfg(not(feature = "app-store"))]
            app.handle()
                .plugin(tauri_plugin_updater::Builder::new().build())?;
            let dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&dir)?;
            let conn = db::open(&dir.join("tracker.sqlite"))?;
            app.manage(AppState(Mutex::new(Tracker::new(conn)?)));

            app.manage(tray::TrayState::default());
            let mut tray = TrayIconBuilder::with_id("main")
                .tooltip("Home Office Tracker")
                .on_menu_event(|app, event| {
                    if let Some(action) = tray::parse_action(event.id().as_ref()) {
                        handle_tray_action(app, action);
                    }
                });
            // Windows: levé kliknutí na ikonu otevře okno, menu je na pravém
            // (na macOS zůstává zvyk menu na levé kliknutí).
            #[cfg(windows)]
            {
                use tauri::tray::{MouseButton, MouseButtonState, TrayIconEvent};
                tray = tray
                    .show_menu_on_left_click(false)
                    .on_tray_icon_event(|tray, event| {
                        if let TrayIconEvent::Click {
                            button: MouseButton::Left,
                            button_state: MouseButtonState::Up,
                            ..
                        } = event
                        {
                            show_main_window(tray.app_handle());
                        }
                    });
            }
            // macOS: jednobarevná ikonka-šablona (přizpůsobí se světlé/tmavé
            // liště); Windows: barevná ikona appky.
            #[cfg(target_os = "macos")]
            {
                tray = tray
                    .icon(tauri::image::Image::from_bytes(include_bytes!(
                        "../icons/tray-template.png"
                    ))?)
                    .icon_as_template(true);
            }
            #[cfg(not(target_os = "macos"))]
            if let Some(icon) = app.default_window_icon() {
                tray = tray.icon(icon.clone());
            }
            tray.build(app)?;
            refresh_tray_now(app.handle());

            app.manage(SyncHandle(Mutex::new(SyncState::default())));
            spawn_ticker(app.handle().clone());
            spawn_sync(app.handle().clone());
            Ok(())
        })
        .on_window_event(|window, event| {
            // Zavření okna appku neukončí — sledování běží dál v liště.
            if let WindowEvent::CloseRequested { api, .. } = event {
                let _ = window.hide();
                api.prevent_close();
            }
        })
        .invoke_handler(tauri::generate_handler![
            get_status,
            start_work,
            switch_project,
            list_projects,
            create_project,
            update_project,
            pause_work,
            resume_work,
            end_work,
            get_report,
            export_report,
            get_settings,
            updates_enabled,
            sync_status,
            sync_now,
            save_settings
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application");

    app.run(|app, event| match event {
        RunEvent::Exit => {
            // Ukončení appky = konec práce, ať nezůstane otevřená akce.
            if let Some(state) = app.try_state::<AppState>() {
                let _ = state.0.lock().unwrap().end(now_ms());
                // Ať ostatní počítače hned vidí ukončenou akci.
                run_sync(app);
            }
        }
        #[cfg(target_os = "macos")]
        RunEvent::Reopen { .. } => show_main_window(app),
        _ => {}
    });
}
