mod db;
mod export;
mod i18n;
pub mod platform;
mod report;
mod tracker;

use chrono::NaiveDate;
use serde::Serialize;
use std::sync::Mutex;
use std::time::Duration;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter, Manager, RunEvent, WindowEvent};
use tauri_plugin_notification::NotificationExt;
use tracker::{Settings, Status, StatusView, TickEvent, Tracker};

struct AppState(Mutex<Tracker>);

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
    drop(tracker);
    update_tray(app, &view, texts);
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
fn start_work(app: AppHandle) -> CmdResult<StatusView> {
    with_tracker(&app, |t, now| t.start(now, None))
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
) -> CmdResult<report::Report> {
    let tracker = state.0.lock().unwrap();
    report::build(
        &tracker.conn,
        parse_date(&from)?,
        parse_date(&to)?,
        now_ms(),
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
) -> CmdResult<()> {
    let tracker = state.0.lock().unwrap();
    let data = report::build(
        &tracker.conn,
        parse_date(&from)?,
        parse_date(&to)?,
        now_ms(),
    )
    .map_err(err)?;
    let employee = tracker.settings.employee_name.clone();
    let texts = i18n::texts(tracker.lang());
    drop(tracker);
    match format.as_str() {
        "csv" => std::fs::write(&path, export::to_csv(&data, &employee, texts)).map_err(err),
        "xlsx" => {
            export::to_xlsx(&data, &employee, texts, std::path::Path::new(&path)).map_err(err)
        }
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
    if let (Some(tray), Ok(menu)) = (
        app.tray_by_id("main"),
        tray_menu(&app, i18n::texts(view.resolved_language)),
    ) {
        let _ = tray.set_menu(Some(menu));
    }
    Ok(view)
}

fn tray_menu(app: &AppHandle, t: &i18n::Texts) -> tauri::Result<Menu<tauri::Wry>> {
    let show = MenuItem::with_id(app, "show", t.tray_show, true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", t.tray_quit, true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;
    Menu::with_items(app, &[&show, &separator, &quit])
}

fn show_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

fn format_duration(ms: i64) -> String {
    let minutes = ms / 60_000;
    format!("{}:{:02}", minutes / 60, minutes % 60)
}

/// Stav v liště: na macOS text vedle ikony, na Windows tooltip.
fn update_tray(app: &AppHandle, view: &StatusView, t: &i18n::Texts) {
    let Some(tray) = app.tray_by_id("main") else {
        return;
    };
    let text = match view.status {
        Status::Off => None,
        Status::Working => Some(format!("▶ {}", format_duration(view.session_worked_ms))),
        Status::Paused | Status::AutoPaused => {
            Some(format!("⏸ {}", format_duration(view.session_worked_ms)))
        }
    };
    #[cfg(target_os = "macos")]
    let _ = tray.set_title(text.as_deref());
    let tooltip = match &text {
        Some(t) => format!("Home Office Tracker — {t}"),
        None => format!("Home Office Tracker — {}", t.tray_not_working),
    };
    let _ = tray.set_tooltip(Some(tooltip));
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
        drop(tracker);

        if let Some(view) = &view {
            update_tray(&app, view, texts);
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
        .setup(|app| {
            let dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&dir)?;
            let conn = db::open(&dir.join("tracker.sqlite"))?;
            app.manage(AppState(Mutex::new(Tracker::new(conn)?)));

            let lang = app.state::<AppState>().0.lock().unwrap().lang();
            let menu = tray_menu(app.handle(), i18n::texts(lang))?;
            let mut tray = TrayIconBuilder::with_id("main")
                .tooltip("Home Office Tracker")
                .menu(&menu)
                .on_menu_event(|app, event| match event.id().as_ref() {
                    "show" => show_main_window(app),
                    "quit" => app.exit(0),
                    _ => {}
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
            if let Some(icon) = app.default_window_icon() {
                tray = tray.icon(icon.clone());
            }
            tray.build(app)?;

            spawn_ticker(app.handle().clone());
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
            pause_work,
            resume_work,
            end_work,
            get_report,
            export_report,
            get_settings,
            save_settings
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application");

    app.run(|app, event| match event {
        RunEvent::Exit => {
            // Ukončení appky = konec práce, ať nezůstane otevřená akce.
            if let Some(state) = app.try_state::<AppState>() {
                let _ = state.0.lock().unwrap().end(now_ms());
            }
        }
        #[cfg(target_os = "macos")]
        RunEvent::Reopen { .. } => show_main_window(app),
        _ => {}
    });
}
