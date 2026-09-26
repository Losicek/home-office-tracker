//! Menu v liště (macOS horní lišta, Windows oznamovací oblast): stav s časem
//! a ovládání práce bez otevírání okna.
//!
//! Menu se přestaví jen při změně stavu, projektu, seznamu projektů nebo
//! jazyka; řádek s časem se jinak jen přepisuje (každou sekundu), aby se
//! otevřené menu nezavíralo.

use crate::i18n::Texts;
use crate::tracker::{Status, StatusView};
use std::sync::Mutex;
use tauri::menu::{IsMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::{AppHandle, Manager, Wry};

#[derive(Default)]
pub struct TrayState {
    key: Mutex<String>,
    status_item: Mutex<Option<MenuItem<Wry>>>,
}

/// Akce z menu, kterou provede `lib.rs`.
pub enum Action {
    Start(Option<i64>),
    StartLast,
    Switch(Option<i64>),
    Pause,
    Resume,
    End,
    Show,
    Quit,
}

pub fn parse_action(id: &str) -> Option<Action> {
    let project = |s: &str| if s == "none" { None } else { s.parse().ok() };
    Some(match id {
        "start" => Action::StartLast,
        "pause" => Action::Pause,
        "resume" => Action::Resume,
        "end" => Action::End,
        "show" => Action::Show,
        "quit" => Action::Quit,
        _ => {
            if let Some(p) = id.strip_prefix("start:") {
                Action::Start(project(p))
            } else if let Some(p) = id.strip_prefix("switch:") {
                Action::Switch(project(p))
            } else {
                return None;
            }
        }
    })
}

fn hms(ms: i64) -> String {
    let s = ms.max(0) / 1000;
    format!("{}:{:02}:{:02}", s / 3600, (s % 3600) / 60, s % 60)
}

fn hm(ms: i64) -> String {
    let m = ms.max(0) / 60_000;
    format!("{}:{:02}", m / 60, m % 60)
}

fn status_line(view: &StatusView, t: &Texts) -> String {
    let label = match view.status {
        Status::Off => return t.status_off.to_string(),
        Status::Working => t.status_working,
        Status::Paused => t.status_paused,
        Status::AutoPaused => t.status_auto_paused,
    };
    let mut line = format!("{label} · {}", hms(view.session_worked_ms));
    if let Some(p) = &view.project {
        line.push_str(" · ");
        line.push_str(&p.name);
    }
    line
}

fn build_menu(
    app: &AppHandle,
    view: &StatusView,
    t: &Texts,
    projects: &[(i64, String)],
) -> tauri::Result<(Menu<Wry>, MenuItem<Wry>)> {
    let status = MenuItem::with_id(app, "status", status_line(view, t), false, None::<&str>)?;
    let mut items: Vec<Box<dyn IsMenuItem<Wry>>> = vec![
        Box::new(status.clone()),
        Box::new(PredefinedMenuItem::separator(app)?),
    ];

    let project_submenu = |title: &str, prefix: &str| -> tauri::Result<Submenu<Wry>> {
        let submenu = Submenu::new(app, title, true)?;
        submenu.append(&MenuItem::with_id(
            app,
            format!("{prefix}:none"),
            t.no_project,
            true,
            None::<&str>,
        )?)?;
        for (id, name) in projects {
            let current = view.project.as_ref().is_some_and(|p| p.id == *id);
            let label = if current {
                format!("✓ {name}")
            } else {
                name.clone()
            };
            submenu.append(&MenuItem::with_id(
                app,
                format!("{prefix}:{id}"),
                label,
                true,
                None::<&str>,
            )?)?;
        }
        Ok(submenu)
    };

    match view.status {
        Status::Off => {
            // „Začít pracovat“ = s naposledy použitým projektem.
            let label = match &view.project {
                Some(p) => format!("{} · {}", t.tray_start, p.name),
                None => t.tray_start.to_string(),
            };
            items.push(Box::new(MenuItem::with_id(
                app,
                "start",
                label,
                true,
                None::<&str>,
            )?));
            if !projects.is_empty() {
                items.push(Box::new(project_submenu(t.tray_start_on, "start")?));
            }
        }
        Status::Working => {
            items.push(Box::new(MenuItem::with_id(
                app,
                "pause",
                t.tray_pause,
                true,
                None::<&str>,
            )?));
            items.push(Box::new(MenuItem::with_id(
                app,
                "end",
                t.tray_end,
                true,
                None::<&str>,
            )?));
            if !projects.is_empty() {
                items.push(Box::new(project_submenu(t.tray_switch, "switch")?));
            }
        }
        Status::Paused | Status::AutoPaused => {
            items.push(Box::new(MenuItem::with_id(
                app,
                "resume",
                t.tray_resume,
                true,
                None::<&str>,
            )?));
            items.push(Box::new(MenuItem::with_id(
                app,
                "end",
                t.tray_end,
                true,
                None::<&str>,
            )?));
        }
    }

    items.push(Box::new(PredefinedMenuItem::separator(app)?));
    items.push(Box::new(MenuItem::with_id(
        app,
        "show",
        t.tray_show,
        true,
        None::<&str>,
    )?));
    items.push(Box::new(PredefinedMenuItem::separator(app)?));
    items.push(Box::new(MenuItem::with_id(
        app,
        "quit",
        t.tray_quit,
        true,
        None::<&str>,
    )?));

    let refs: Vec<&dyn IsMenuItem<Wry>> = items.iter().map(|i| i.as_ref()).collect();
    Ok((Menu::with_items(app, &refs)?, status))
}

/// Obnoví menu, text vedle ikony (macOS) a tooltip.
pub fn refresh(app: &AppHandle, view: &StatusView, t: &Texts, projects: &[(i64, String)]) {
    let Some(tray) = app.tray_by_id("main") else {
        return;
    };
    let Some(state) = app.try_state::<TrayState>() else {
        return;
    };

    let key = format!(
        "{:?}|{:?}|{}|{:?}",
        view.status,
        view.project.as_ref().map(|p| (&p.id, &p.name)),
        t.tray_show,
        projects
    );
    let mut stored = state.key.lock().unwrap();
    if *stored != key {
        if let Ok((menu, status)) = build_menu(app, view, t, projects) {
            let _ = tray.set_menu(Some(menu));
            *state.status_item.lock().unwrap() = Some(status);
            *stored = key;
        }
    } else if let Some(item) = state.status_item.lock().unwrap().as_ref() {
        let _ = item.set_text(status_line(view, t));
    }
    drop(stored);

    let title = match view.status {
        Status::Off => None,
        Status::Working => Some(format!("▶ {}", hm(view.session_worked_ms))),
        Status::Paused | Status::AutoPaused => Some(format!("⏸ {}", hm(view.session_worked_ms))),
    };
    #[cfg(target_os = "macos")]
    let _ = tray.set_title(title.as_deref());
    let _ = tray.set_tooltip(Some(format!(
        "Home Office Tracker — {}",
        status_line(view, t)
    )));
    let _ = title;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_menu_ids() {
        assert!(matches!(parse_action("start"), Some(Action::StartLast)));
        assert!(matches!(
            parse_action("start:none"),
            Some(Action::Start(None))
        ));
        assert!(matches!(
            parse_action("start:7"),
            Some(Action::Start(Some(7)))
        ));
        assert!(matches!(
            parse_action("switch:3"),
            Some(Action::Switch(Some(3)))
        ));
        assert!(parse_action("status").is_none());
        assert_eq!(hms(3_725_000), "1:02:05");
    }
}
