//! Synchronizace mezi počítači přes sdílenou složku (na macOS kontejner
//! iCloudu appky).
//!
//! Princip: **každý počítač zapisuje jen do své podsložky** a soubory
//! ostatních jen čte. Nikdy tedy dva počítače nepíšou do stejného souboru,
//! takže nehrozí konflikty ani poškozená databáze (SQLite se do iCloudu
//! nedává, synchronizují se JSON soubory).
//!
//! ```text
//! <root>/devices/<device-id>/device.json            název počítače
//!                           /projects.json          všechny známé projekty
//!                           /sessions-2026-09.json  akce tohoto počítače
//!                                                   začaté v daném měsíci (UTC)
//! ```
//!
//! Projekty se slučují podle `uuid`, vyhrává novější `updated_at`. Akce
//! z ostatních počítačů se ukládají s `sessions.device_id`; otevřené akce se
//! počítají jen do jejich `last_seen`.

use crate::db;
use chrono::{Datelike, NaiveDate, TimeZone, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

const FORMAT_VERSION: u32 = 1;

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DeviceFile {
    id: String,
    name: String,
    updated_at: i64,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProjectEntry {
    uuid: String,
    name: String,
    color: String,
    archived: bool,
    created_at: i64,
    updated_at: i64,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProjectsFile {
    version: u32,
    projects: Vec<ProjectEntry>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SegmentEntry {
    kind: String,
    reason: Option<String>,
    started_at: i64,
    ended_at: Option<i64>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AppEntry {
    app: String,
    started_at: i64,
    ended_at: Option<i64>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SessionEntry {
    uuid: String,
    started_at: i64,
    ended_at: Option<i64>,
    last_seen: i64,
    /// uuid projektu
    project: Option<String>,
    #[serde(default)]
    note: Option<String>,
    #[serde(default)]
    edited: bool,
    #[serde(default)]
    manual: bool,
    segments: Vec<SegmentEntry>,
    apps: Vec<AppEntry>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SessionsFile {
    version: u32,
    sessions: Vec<SessionEntry>,
}

fn e(err: impl std::fmt::Display) -> String {
    err.to_string()
}

fn device_dir(root: &Path, id: &str) -> PathBuf {
    root.join("devices").join(id)
}

/// Zápis přes dočasný soubor a přejmenování — čtenář nikdy neuvidí půlku.
fn write_atomic(path: &Path, value: &impl Serialize) -> Result<(), String> {
    let bytes = serde_json::to_vec(value).map_err(e)?;
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, bytes).map_err(e)?;
    std::fs::rename(&tmp, path).map_err(e)
}

use db::month_key;

/// [začátek, začátek dalšího měsíce) v UTC ms.
fn month_range(key: &str) -> Option<(i64, i64)> {
    let first = NaiveDate::parse_from_str(&format!("{key}-01"), "%Y-%m-%d").ok()?;
    let next = if first.month() == 12 {
        NaiveDate::from_ymd_opt(first.year() + 1, 1, 1)?
    } else {
        NaiveDate::from_ymd_opt(first.year(), first.month() + 1, 1)?
    };
    let ms = |d: NaiveDate| {
        Utc.from_utc_datetime(&d.and_hms_opt(0, 0, 0).unwrap())
            .timestamp_millis()
    };
    Some((ms(first), ms(next)))
}

/// Zapíše změněná data tohoto počítače. Vrací, zda se něco zapsalo.
pub fn export(conn: &Connection, root: &Path, device_name: &str, now: i64) -> Result<bool, String> {
    let id = db::device_id(conn).map_err(e)?;
    let dir = device_dir(root, &id);
    std::fs::create_dir_all(&dir).map_err(e)?;
    let last: i64 = db::get_setting(conn, "sync_last_export")
        .map_err(e)?
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    let mut wrote = false;

    let known_name = db::get_setting(conn, "sync_device_name").map_err(e)?;
    if last == 0 || known_name.as_deref() != Some(device_name) {
        write_atomic(
            &dir.join("device.json"),
            &DeviceFile {
                id: id.clone(),
                name: device_name.to_string(),
                updated_at: now,
            },
        )?;
        db::set_setting(conn, "sync_device_name", device_name).map_err(e)?;
        wrote = true;
    }

    let projects_changed: bool = conn
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM projects WHERE updated_at >= ?1)",
            [last],
            |r| r.get(0),
        )
        .map_err(e)?;
    if last == 0 || projects_changed {
        let projects = conn
            .prepare("SELECT uuid, name, color, archived, created_at, updated_at FROM projects")
            .map_err(e)?
            .query_map([], |r| {
                Ok(ProjectEntry {
                    uuid: r.get(0)?,
                    name: r.get(1)?,
                    color: r.get(2)?,
                    archived: r.get::<_, i64>(3)? != 0,
                    created_at: r.get(4)?,
                    updated_at: r.get(5)?,
                })
            })
            .map_err(e)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(e)?;
        write_atomic(
            &dir.join("projects.json"),
            &ProjectsFile {
                version: FORMAT_VERSION,
                projects,
            },
        )?;
        wrote = true;
    }

    // Měsíce, ve kterých se od posledního exportu něco změnilo.
    let mut months: BTreeSet<String> = conn
        .prepare(
            "SELECT started_at FROM sessions
             WHERE device_id IS NULL
               AND (?1 = 0 OR ended_at IS NULL OR last_seen >= ?1 OR ended_at >= ?1)",
        )
        .map_err(e)?
        .query_map([last], |r| r.get::<_, i64>(0))
        .map_err(e)?
        .map(|r| r.map(month_key))
        .collect::<Result<_, _>>()
        .map_err(e)?;

    let dirty: Vec<String> = conn
        .prepare("SELECT month FROM sync_dirty")
        .map_err(e)?
        .query_map([], |r| r.get(0))
        .map_err(e)?
        .collect::<Result<_, _>>()
        .map_err(e)?;
    months.extend(dirty);

    for month in months {
        let Some((from, to)) = month_range(&month) else {
            continue;
        };
        let sessions = local_sessions(conn, from, to)?;
        write_atomic(
            &dir.join(format!("sessions-{month}.json")),
            &SessionsFile {
                version: FORMAT_VERSION,
                sessions,
            },
        )?;
        wrote = true;
    }

    conn.execute("DELETE FROM sync_dirty", []).map_err(e)?;
    db::set_setting(conn, "sync_last_export", &now.to_string()).map_err(e)?;
    Ok(wrote)
}

fn local_sessions(conn: &Connection, from: i64, to: i64) -> Result<Vec<SessionEntry>, String> {
    let mut sessions: Vec<(i64, SessionEntry)> = conn
        .prepare(
            "SELECT s.id, s.uuid, s.started_at, s.ended_at, s.last_seen, p.uuid,
                    s.note, s.edited, s.manual
             FROM sessions s LEFT JOIN projects p ON p.id = s.project_id
             WHERE s.device_id IS NULL AND s.started_at >= ?1 AND s.started_at < ?2
             ORDER BY s.started_at",
        )
        .map_err(e)?
        .query_map([from, to], |r| {
            Ok((
                r.get(0)?,
                SessionEntry {
                    uuid: r.get(1)?,
                    started_at: r.get(2)?,
                    ended_at: r.get(3)?,
                    last_seen: r.get(4)?,
                    project: r.get(5)?,
                    note: r.get(6)?,
                    edited: r.get::<_, i64>(7)? != 0,
                    manual: r.get::<_, i64>(8)? != 0,
                    segments: Vec::new(),
                    apps: Vec::new(),
                },
            ))
        })
        .map_err(e)?
        .collect::<Result<_, _>>()
        .map_err(e)?;

    let mut segments = conn
        .prepare(
            "SELECT kind, reason, started_at, ended_at FROM segments
             WHERE session_id = ?1 ORDER BY started_at",
        )
        .map_err(e)?;
    let mut apps = conn
        .prepare(
            "SELECT app_name, started_at, ended_at FROM app_usage
             WHERE session_id = ?1 ORDER BY started_at",
        )
        .map_err(e)?;
    for (id, s) in &mut sessions {
        s.segments = segments
            .query_map([*id], |r| {
                Ok(SegmentEntry {
                    kind: r.get(0)?,
                    reason: r.get(1)?,
                    started_at: r.get(2)?,
                    ended_at: r.get(3)?,
                })
            })
            .map_err(e)?
            .collect::<Result<_, _>>()
            .map_err(e)?;
        s.apps = apps
            .query_map([*id], |r| {
                Ok(AppEntry {
                    app: r.get(0)?,
                    started_at: r.get(1)?,
                    ended_at: r.get(2)?,
                })
            })
            .map_err(e)?
            .collect::<Result<_, _>>()
            .map_err(e)?;
    }
    Ok(sessions.into_iter().map(|(_, s)| s).collect())
}

/// Načte nové nebo změněné soubory ostatních počítačů. Vrací, zda se něco
/// změnilo (UI se má obnovit).
pub fn import(conn: &Connection, root: &Path) -> Result<bool, String> {
    let own = db::device_id(conn).map_err(e)?;
    let devices_dir = root.join("devices");
    let Ok(entries) = std::fs::read_dir(&devices_dir) else {
        return Ok(false);
    };
    let mut device_dirs: Vec<PathBuf> = entries.flatten().map(|d| d.path()).collect();
    device_dirs.sort();

    let mut changed = false;
    for dir in device_dirs {
        let Some(device) = dir.file_name().map(|n| n.to_string_lossy().into_owned()) else {
            continue;
        };
        if device == own || !dir.is_dir() {
            continue;
        }
        let mut files: Vec<PathBuf> = match std::fs::read_dir(&dir) {
            Ok(entries) => entries.flatten().map(|f| f.path()).collect(),
            Err(_) => continue,
        };
        // device.json → projects.json → sessions-*.json (projekty před akcemi)
        files.sort();
        for path in files {
            let name = path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            if name.starts_with('.') && name.ends_with(".icloud") {
                crate::platform::request_download(&path);
                continue;
            }
            if !name.ends_with(".json") {
                continue;
            }
            let Ok(meta) = std::fs::metadata(&path) else {
                continue;
            };
            let modified = meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_millis() as i64)
                .unwrap_or(0);
            let size = meta.len() as i64;
            let key = format!("devices/{device}/{name}");
            let seen: Option<(i64, i64)> = conn
                .query_row(
                    "SELECT modified, size FROM sync_files WHERE path = ?1",
                    [&key],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .optional()
                .map_err(e)?;
            if seen == Some((modified, size)) {
                continue;
            }
            let Ok(bytes) = std::fs::read(&path) else {
                continue;
            };
            // Nečitelný soubor (třeba rozpracovaný) zkusíme příště znovu.
            let applied = if name == "device.json" {
                serde_json::from_slice::<DeviceFile>(&bytes)
                    .ok()
                    .map(|f| apply_device(conn, &device, &f))
            } else if name == "projects.json" {
                serde_json::from_slice::<ProjectsFile>(&bytes)
                    .ok()
                    .map(|f| apply_projects(conn, &f.projects))
            } else if let Some(month) = name
                .strip_prefix("sessions-")
                .and_then(|n| n.strip_suffix(".json"))
            {
                serde_json::from_slice::<SessionsFile>(&bytes)
                    .ok()
                    .map(|f| apply_sessions(conn, &device, month, &f.sessions))
            } else {
                None
            };
            match applied {
                Some(result) => result?,
                None => continue,
            }
            conn.execute(
                "INSERT INTO sync_files (path, modified, size) VALUES (?1, ?2, ?3)
                 ON CONFLICT(path) DO UPDATE SET modified = excluded.modified, size = excluded.size",
                params![key, modified, size],
            )
            .map_err(e)?;
            changed = true;
        }
    }
    Ok(changed)
}

fn apply_device(conn: &Connection, device: &str, f: &DeviceFile) -> Result<(), String> {
    conn.execute(
        "INSERT INTO devices (id, name, updated_at) VALUES (?1, ?2, ?3)
         ON CONFLICT(id) DO UPDATE SET name = excluded.name, updated_at = excluded.updated_at",
        params![device, f.name, f.updated_at],
    )
    .map_err(e)?;
    Ok(())
}

fn apply_projects(conn: &Connection, projects: &[ProjectEntry]) -> Result<(), String> {
    for p in projects {
        let local: Option<(i64, i64)> = conn
            .query_row(
                "SELECT id, updated_at FROM projects WHERE uuid = ?1",
                [&p.uuid],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()
            .map_err(e)?;
        match local {
            None => {
                conn.execute(
                    "INSERT INTO projects (uuid, name, color, archived, created_at, updated_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    params![
                        p.uuid,
                        p.name,
                        p.color,
                        p.archived as i64,
                        p.created_at,
                        p.updated_at
                    ],
                )
                .map_err(e)?;
            }
            Some((id, updated_at)) if p.updated_at > updated_at => {
                conn.execute(
                    "UPDATE projects SET name = ?2, color = ?3, archived = ?4, updated_at = ?5
                     WHERE id = ?1",
                    params![id, p.name, p.color, p.archived as i64, p.updated_at],
                )
                .map_err(e)?;
            }
            Some(_) => {}
        }
    }
    Ok(())
}

fn apply_sessions(
    conn: &Connection,
    device: &str,
    month: &str,
    sessions: &[SessionEntry],
) -> Result<(), String> {
    let tx = conn.unchecked_transaction().map_err(e)?;
    // Akce, které v souboru daného měsíce už nejsou, byly na druhém počítači
    // smazány (nebo přesunuty do jiného měsíce) → pryč.
    if let Some((from, to)) = month_range(month) {
        let keep: std::collections::HashSet<&str> =
            sessions.iter().map(|s| s.uuid.as_str()).collect();
        let existing: Vec<(i64, String)> = tx
            .prepare(
                "SELECT id, uuid FROM sessions
                 WHERE device_id = ?1 AND started_at >= ?2 AND started_at < ?3",
            )
            .map_err(e)?
            .query_map(params![device, from, to], |r| Ok((r.get(0)?, r.get(1)?)))
            .map_err(e)?
            .collect::<Result<_, _>>()
            .map_err(e)?;
        for (id, uuid) in existing {
            if !keep.contains(uuid.as_str()) {
                tx.execute("DELETE FROM app_usage WHERE session_id = ?1", [id])
                    .map_err(e)?;
                tx.execute("DELETE FROM segments WHERE session_id = ?1", [id])
                    .map_err(e)?;
                tx.execute("DELETE FROM sessions WHERE id = ?1", [id])
                    .map_err(e)?;
            }
        }
    }
    for s in sessions {
        let existing: Option<(i64, Option<String>)> = tx
            .query_row(
                "SELECT id, device_id FROM sessions WHERE uuid = ?1",
                [&s.uuid],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()
            .map_err(e)?;
        match existing {
            // Vlastní akce tohoto počítače nikdy nepřepisujeme.
            Some((_, None)) => continue,
            Some((id, Some(_))) => {
                tx.execute("DELETE FROM app_usage WHERE session_id = ?1", [id])
                    .map_err(e)?;
                tx.execute("DELETE FROM segments WHERE session_id = ?1", [id])
                    .map_err(e)?;
                tx.execute("DELETE FROM sessions WHERE id = ?1", [id])
                    .map_err(e)?;
            }
            None => {}
        }
        let project: Option<i64> = match &s.project {
            Some(uuid) => tx
                .query_row("SELECT id FROM projects WHERE uuid = ?1", [uuid], |r| {
                    r.get(0)
                })
                .optional()
                .map_err(e)?,
            None => None,
        };
        // Akce, která na druhém počítači ještě běží, končí (zatím) v last_seen.
        let end = s.ended_at.unwrap_or(s.last_seen);
        tx.execute(
            "INSERT INTO sessions (started_at, ended_at, last_seen, project_id, uuid, device_id,
                                   note, edited, manual)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                s.started_at,
                end,
                s.last_seen,
                project,
                s.uuid,
                device,
                s.note,
                s.edited as i64,
                s.manual as i64
            ],
        )
        .map_err(e)?;
        let id = tx.last_insert_rowid();
        for g in &s.segments {
            tx.execute(
                "INSERT INTO segments (session_id, kind, reason, started_at, ended_at)
                 VALUES (?1, ?2, ?3, ?4, MAX(?4, MIN(?5, ?6)))",
                params![
                    id,
                    g.kind,
                    g.reason,
                    g.started_at,
                    g.ended_at.unwrap_or(end),
                    end
                ],
            )
            .map_err(e)?;
        }
        for a in &s.apps {
            tx.execute(
                "INSERT INTO app_usage (session_id, app_name, started_at, ended_at)
                 VALUES (?1, ?2, ?3, MAX(?3, MIN(?4, ?5)))",
                params![id, a.app, a.started_at, a.ended_at.unwrap_or(end), end],
            )
            .map_err(e)?;
        }
    }
    tx.commit().map_err(e)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::report::{build, ProjectFilter};
    use crate::{projects, tracker::Tracker};

    const MIN: i64 = 60_000;

    fn temp_root() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("hot-sync-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn sessions_and_projects_flow_between_devices() {
        let root = temp_root();
        // 2026-09-25 08:00 UTC
        let t0 = Utc
            .with_ymd_and_hms(2026, 9, 25, 8, 0, 0)
            .unwrap()
            .timestamp_millis();
        let day = NaiveDate::from_ymd_opt(2026, 9, 25).unwrap();

        let mut a = Tracker::new(db::open_in_memory().unwrap()).unwrap();
        let mut b = Tracker::new(db::open_in_memory().unwrap()).unwrap();

        let pa = projects::create(&a.conn, "Klient", projects::PALETTE[1], t0).unwrap();
        a.start(t0, Some("Excel".into()), Some(pa)).unwrap();
        a.end(t0 + 60 * MIN).unwrap();
        b.start(t0 + 2 * 60 * MIN, Some("Mail".into()), None)
            .unwrap();
        b.end(t0 + 3 * 60 * MIN).unwrap();

        assert!(export(&a.conn, &root, "Mac Studio", t0 + 61 * MIN).unwrap());
        assert!(export(&b.conn, &root, "MacBook", t0 + 181 * MIN).unwrap());
        assert!(import(&b.conn, &root).unwrap());
        assert!(import(&a.conn, &root).unwrap());
        // Podruhé už není co načítat.
        assert!(!import(&b.conn, &root).unwrap());

        for t in [&a, &b] {
            let r = build(&t.conn, day, day, t0 + 5 * 60 * MIN, &ProjectFilter::All).unwrap();
            assert_eq!(r.worked_ms, 2 * 60 * MIN);
            assert_eq!(r.sessions.len(), 2);
            let klient = r
                .projects
                .iter()
                .find(|p| p.name.as_deref() == Some("Klient"))
                .unwrap();
            assert_eq!(klient.worked_ms, 60 * MIN);
        }
        let on_b = build(&b.conn, day, day, t0 + 5 * 60 * MIN, &ProjectFilter::All).unwrap();
        assert_eq!(on_b.sessions[0].device_name.as_deref(), Some("Mac Studio"));

        // Přejmenování projektu na B se po synchronizaci projeví na A.
        let pb = projects::list(&b.conn, 0).unwrap()[0].id;
        projects::update(
            &b.conn,
            pb,
            "Klient s.r.o.",
            projects::PALETTE[1],
            false,
            t0 + 200 * MIN,
        )
        .unwrap();
        export(&b.conn, &root, "MacBook", t0 + 201 * MIN).unwrap();
        import(&a.conn, &root).unwrap();
        assert_eq!(projects::list(&a.conn, 0).unwrap()[0].name, "Klient s.r.o.");

        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn edits_and_deletions_propagate() {
        let root = temp_root();
        let t0 = Utc
            .with_ymd_and_hms(2026, 9, 25, 8, 0, 0)
            .unwrap()
            .timestamp_millis();
        let now = t0 + 10 * 60 * MIN;
        let mut a = Tracker::new(db::open_in_memory().unwrap()).unwrap();
        let b = Tracker::new(db::open_in_memory().unwrap()).unwrap();
        a.start(t0, None, None).unwrap();
        a.end(t0 + 60 * MIN).unwrap();
        a.start(t0 + 2 * 60 * MIN, None, None).unwrap();
        a.end(t0 + 3 * 60 * MIN).unwrap();
        export(&a.conn, &root, "Mac Studio", t0 + 181 * MIN).unwrap();
        import(&b.conn, &root).unwrap();
        let count = |t: &Tracker| -> i64 {
            t.conn
                .query_row("SELECT COUNT(*) FROM sessions", [], |r| r.get(0))
                .unwrap()
        };
        assert_eq!(count(&b), 2);

        // Na A: poznámka k první, druhá smazána. Export pošle měsíc znovu.
        let ids: Vec<i64> = a
            .conn
            .prepare("SELECT id FROM sessions ORDER BY started_at")
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        crate::edit::update_session(
            &a.conn,
            ids[0],
            None,
            crate::edit::SessionInput {
                started_at: t0,
                ended_at: t0 + 60 * MIN,
                project: None,
                note: Some("Porada".into()),
            },
            now,
        )
        .unwrap();
        crate::edit::delete_session(&a.conn, ids[1], None).unwrap();
        // Posun mtime, aby import soubor poznal jako změněný i ve stejné ms.
        std::thread::sleep(std::time::Duration::from_millis(20));
        assert!(export(&a.conn, &root, "Mac Studio", now).unwrap());
        assert!(import(&b.conn, &root).unwrap());
        assert_eq!(count(&b), 1);
        let note: Option<String> = b
            .conn
            .query_row("SELECT note FROM sessions", [], |r| r.get(0))
            .unwrap();
        assert_eq!(note.as_deref(), Some("Porada"));
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn running_session_on_other_device_counts_until_last_seen() {
        let root = temp_root();
        let t0 = Utc
            .with_ymd_and_hms(2026, 9, 25, 8, 0, 0)
            .unwrap()
            .timestamp_millis();
        let day = NaiveDate::from_ymd_opt(2026, 9, 25).unwrap();
        let mut a = Tracker::new(db::open_in_memory().unwrap()).unwrap();
        let b = Tracker::new(db::open_in_memory().unwrap()).unwrap();

        a.start(t0, Some("Excel".into()), None).unwrap();
        for i in 1..=360 {
            a.tick(t0 + i * 5_000, 0, Some("Excel".into())).unwrap();
        }
        export(&a.conn, &root, "Mac Studio", t0 + 30 * MIN).unwrap();
        import(&b.conn, &root).unwrap();

        let r = build(&b.conn, day, day, t0 + 5 * 60 * MIN, &ProjectFilter::All).unwrap();
        assert_eq!(r.worked_ms, 30 * MIN);
        std::fs::remove_dir_all(&root).unwrap();
    }
}
