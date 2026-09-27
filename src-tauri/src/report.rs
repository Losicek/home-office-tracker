//! Přehledy za libovolné období (den / týden / měsíc = jen jiný rozsah dní).
//! Úseky přes půlnoc se rozdělí mezi dny podle místního času. Přehled jde
//! zúžit na jeden projekt (nebo na akce bez projektu).

use chrono::{Duration, Local, NaiveDate, TimeZone};
use rusqlite::{params, Connection};
use serde::Serialize;
use std::collections::HashMap;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DayRow {
    pub date: String,
    pub worked_ms: i64,
    pub paused_ms: i64,
    pub sessions: u32,
    pub first_start: Option<i64>,
    pub last_end: Option<i64>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionRow {
    pub id: i64,
    pub started_at: i64,
    pub ended_at: Option<i64>,
    pub worked_ms: i64,
    pub paused_ms: i64,
    pub auto_pauses: u32,
    pub project_id: Option<i64>,
    pub project_name: Option<String>,
    pub project_color: Option<String>,
    /// Počítač, na kterém akce proběhla (jen u akcí z jiných počítačů).
    pub device_name: Option<String>,
    pub note: Option<String>,
    /// Časy upravené ručně.
    pub edited: bool,
    /// Celá akce přidaná ručně.
    pub manual: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectRow {
    /// `None` = akce bez projektu
    pub id: Option<i64>,
    pub name: Option<String>,
    pub color: Option<String>,
    pub worked_ms: i64,
    pub sessions: u32,
}

/// Filtr přehledu: "all", "none" (bez projektu) nebo id projektu.
#[derive(Debug, Clone, PartialEq)]
pub enum ProjectFilter {
    All,
    NoProject,
    Project(i64),
}

impl ProjectFilter {
    pub fn parse(s: Option<&str>) -> Self {
        match s {
            None | Some("all") | Some("") => ProjectFilter::All,
            Some("none") => ProjectFilter::NoProject,
            Some(id) => id
                .parse()
                .map(ProjectFilter::Project)
                .unwrap_or(ProjectFilter::All),
        }
    }

    /// SQL podmínka nad aliasem `s` (tabulka sessions).
    fn sql(&self) -> String {
        match self {
            ProjectFilter::All => "1".into(),
            ProjectFilter::NoProject => "s.project_id IS NULL".into(),
            ProjectFilter::Project(id) => format!("s.project_id = {id}"),
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppRow {
    pub name: String,
    pub ms: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub from: String,
    pub to: String,
    pub worked_ms: i64,
    pub paused_ms: i64,
    pub days: Vec<DayRow>,
    pub sessions: Vec<SessionRow>,
    pub apps: Vec<AppRow>,
    pub projects: Vec<ProjectRow>,
}

/// Místní půlnoc daného dne jako UTC ms.
pub fn day_start_ms(date: NaiveDate) -> i64 {
    let midnight = date.and_hms_opt(0, 0, 0).unwrap();
    Local
        .from_local_datetime(&midnight)
        .earliest()
        // Den, kdy půlnoc kvůli změně času neexistuje — vezmeme 1:00.
        .or_else(|| {
            Local
                .from_local_datetime(&(midnight + Duration::hours(1)))
                .earliest()
        })
        .map(|dt| dt.timestamp_millis())
        .unwrap_or(0)
}

fn overlap(start: i64, end: i64, from: i64, to: i64) -> i64 {
    (end.min(to) - start.max(from)).max(0)
}

/// `from`/`to` jsou místní data včetně (např. celý týden Po–Ne).
pub fn build(
    conn: &Connection,
    from: NaiveDate,
    to: NaiveDate,
    now: i64,
    filter: &ProjectFilter,
) -> rusqlite::Result<Report> {
    let cond = filter.sql();
    let range_start = day_start_ms(from);
    let range_end = day_start_ms(to + Duration::days(1));

    let days: Vec<(NaiveDate, i64, i64)> = from
        .iter_days()
        .take_while(|d| *d <= to)
        .map(|d| (d, day_start_ms(d), day_start_ms(d + Duration::days(1))))
        .collect();

    // Segmenty zasahující do období; otevřené počítáme do teď.
    let segments: Vec<(i64, String, Option<String>, i64, i64, Option<i64>)> = conn
        .prepare(&format!(
            "SELECT g.session_id, g.kind, g.reason, g.started_at,
                    COALESCE(g.ended_at, ?3), s.project_id
             FROM segments g JOIN sessions s ON s.id = g.session_id
             WHERE g.started_at < ?2 AND COALESCE(g.ended_at, ?3) > ?1 AND {cond}
             ORDER BY g.started_at"
        ))?
        .query_map(params![range_start, range_end, now], |r| {
            Ok((
                r.get(0)?,
                r.get(1)?,
                r.get(2)?,
                r.get(3)?,
                r.get(4)?,
                r.get(5)?,
            ))
        })?
        .collect::<Result<_, _>>()?;

    let mut day_rows: Vec<DayRow> = days
        .iter()
        .map(|(d, _, _)| DayRow {
            date: d.format("%Y-%m-%d").to_string(),
            worked_ms: 0,
            paused_ms: 0,
            sessions: 0,
            first_start: None,
            last_end: None,
        })
        .collect();
    let mut day_sessions: Vec<Vec<i64>> = vec![Vec::new(); days.len()];
    let mut per_session: HashMap<i64, (i64, i64, u32)> = HashMap::new();
    let mut per_project: HashMap<Option<i64>, (i64, Vec<i64>)> = HashMap::new();

    for (session, kind, reason, start, end, project) in &segments {
        if kind == "work" {
            let p = per_project.entry(*project).or_default();
            p.0 += overlap(*start, *end, range_start, range_end);
            if !p.1.contains(session) {
                p.1.push(*session);
            }
        }
        let entry = per_session.entry(*session).or_default();
        if kind == "pause" && reason.as_deref() == Some("idle") && *start >= range_start {
            entry.2 += 1;
        }
        for (i, (_, d_start, d_end)) in days.iter().enumerate() {
            let ms = overlap(*start, *end, *d_start, *d_end);
            if ms == 0 {
                continue;
            }
            let row = &mut day_rows[i];
            if kind == "work" {
                row.worked_ms += ms;
                entry.0 += ms;
                let clipped_start = (*start).max(*d_start);
                let clipped_end = (*end).min(*d_end);
                row.first_start = Some(
                    row.first_start
                        .map_or(clipped_start, |v| v.min(clipped_start)),
                );
                row.last_end = Some(row.last_end.map_or(clipped_end, |v| v.max(clipped_end)));
                if !day_sessions[i].contains(session) {
                    day_sessions[i].push(*session);
                }
            } else {
                row.paused_ms += ms;
                entry.1 += ms;
            }
        }
    }
    for (row, sessions) in day_rows.iter_mut().zip(&day_sessions) {
        row.sessions = sessions.len() as u32;
    }

    let mut sessions: Vec<SessionRow> = conn
        .prepare(&format!(
            "SELECT s.id, s.started_at, s.ended_at, s.project_id, p.name, p.color, d.name,
                    s.note, s.edited, s.manual
             FROM sessions s LEFT JOIN projects p ON p.id = s.project_id
             LEFT JOIN devices d ON d.id = s.device_id
             WHERE s.started_at < ?2 AND COALESCE(s.ended_at, ?3) > ?1 AND {cond}
             ORDER BY s.started_at"
        ))?
        .query_map(params![range_start, range_end, now], |r| {
            Ok(SessionRow {
                id: r.get(0)?,
                started_at: r.get(1)?,
                ended_at: r.get(2)?,
                worked_ms: 0,
                paused_ms: 0,
                auto_pauses: 0,
                project_id: r.get(3)?,
                project_name: r.get(4)?,
                project_color: r.get(5)?,
                device_name: r.get(6)?,
                note: r.get(7)?,
                edited: r.get::<_, i64>(8)? != 0,
                manual: r.get::<_, i64>(9)? != 0,
            })
        })?
        .collect::<Result<_, _>>()?;
    for s in &mut sessions {
        if let Some((worked, paused, auto)) = per_session.get(&s.id) {
            s.worked_ms = *worked;
            s.paused_ms = *paused;
            s.auto_pauses = *auto;
        }
    }

    let apps: Vec<AppRow> = conn
        .prepare(&format!(
            "SELECT a.app_name,
                    SUM(MIN(COALESCE(a.ended_at, ?3), ?2) - MAX(a.started_at, ?1)) AS ms
             FROM app_usage a JOIN sessions s ON s.id = a.session_id
             WHERE a.started_at < ?2 AND COALESCE(a.ended_at, ?3) > ?1 AND {cond}
             GROUP BY a.app_name
             HAVING ms > 0
             ORDER BY ms DESC"
        ))?
        .query_map(params![range_start, range_end, now], |r| {
            Ok(AppRow {
                name: r.get(0)?,
                ms: r.get(1)?,
            })
        })?
        .collect::<Result<_, _>>()?;

    let mut projects: Vec<ProjectRow> = Vec::new();
    for (id, (worked_ms, sessions)) in per_project {
        let info = match id {
            Some(id) => crate::projects::get_ref(conn, id)?,
            None => None,
        };
        projects.push(ProjectRow {
            id,
            name: info.as_ref().map(|p| p.name.clone()),
            color: info.map(|p| p.color),
            worked_ms,
            sessions: sessions.len() as u32,
        });
    }
    projects.retain(|p| p.worked_ms > 0);
    projects.sort_by(|a, b| b.worked_ms.cmp(&a.worked_ms));

    Ok(Report {
        from: from.format("%Y-%m-%d").to_string(),
        to: to.format("%Y-%m-%d").to_string(),
        worked_ms: day_rows.iter().map(|d| d.worked_ms).sum(),
        paused_ms: day_rows.iter().map(|d| d.paused_ms).sum(),
        days: day_rows,
        sessions,
        apps,
        projects,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{db, tracker::Tracker};

    const HOUR: i64 = 3_600_000;

    #[test]
    fn work_across_midnight_is_split_between_days() {
        let d1 = NaiveDate::from_ymd_opt(2026, 9, 25).unwrap();
        let d2 = d1 + Duration::days(1);
        let midnight = day_start_ms(d2);
        let mut t = Tracker::new(db::open_in_memory().unwrap()).unwrap();
        t.start(midnight - 2 * HOUR, Some("Excel".into()), None)
            .unwrap();
        t.end(midnight + HOUR).unwrap();

        let r = build(&t.conn, d1, d2, midnight + 5 * HOUR, &ProjectFilter::All).unwrap();
        assert_eq!(r.days[0].worked_ms, 2 * HOUR);
        assert_eq!(r.days[1].worked_ms, HOUR);
        assert_eq!(r.worked_ms, 3 * HOUR);
        assert_eq!(r.sessions.len(), 1);
        assert_eq!(r.apps[0].ms, 3 * HOUR);

        let only_second = build(&t.conn, d2, d2, midnight + 5 * HOUR, &ProjectFilter::All).unwrap();
        assert_eq!(only_second.worked_ms, HOUR);
        assert_eq!(only_second.apps[0].ms, HOUR);
    }

    #[test]
    fn per_project_totals_and_filter() {
        let d = NaiveDate::from_ymd_opt(2026, 9, 25).unwrap();
        let start = day_start_ms(d) + 8 * HOUR;
        let mut t = Tracker::new(db::open_in_memory().unwrap()).unwrap();
        let a = crate::projects::create(&t.conn, "A", "#3b82c4", 0).unwrap();
        t.start(start, Some("Excel".into()), Some(a)).unwrap();
        t.end(start + 2 * HOUR).unwrap();
        t.start(start + 3 * HOUR, Some("Mail".into()), None)
            .unwrap();
        t.end(start + 4 * HOUR).unwrap();

        let all = build(&t.conn, d, d, start + 5 * HOUR, &ProjectFilter::All).unwrap();
        assert_eq!(all.worked_ms, 3 * HOUR);
        assert_eq!(all.projects.len(), 2);
        assert_eq!(all.projects[0].id, Some(a));
        assert_eq!(all.projects[0].worked_ms, 2 * HOUR);
        assert_eq!(all.projects[1].id, None);

        let only_a = build(&t.conn, d, d, start + 5 * HOUR, &ProjectFilter::Project(a)).unwrap();
        assert_eq!(only_a.worked_ms, 2 * HOUR);
        assert_eq!(only_a.sessions.len(), 1);
        assert_eq!(only_a.apps.len(), 1);
        assert_eq!(only_a.sessions[0].project_name.as_deref(), Some("A"));

        let none = build(&t.conn, d, d, start + 5 * HOUR, &ProjectFilter::NoProject).unwrap();
        assert_eq!(none.worked_ms, HOUR);
        assert_eq!(none.apps[0].name, "Mail");
    }
}
