//! Přehledy za libovolné období (den / týden / měsíc = jen jiný rozsah dní).
//! Úseky přes půlnoc se rozdělí mezi dny podle místního času.

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
) -> rusqlite::Result<Report> {
    let range_start = day_start_ms(from);
    let range_end = day_start_ms(to + Duration::days(1));

    let days: Vec<(NaiveDate, i64, i64)> = from
        .iter_days()
        .take_while(|d| *d <= to)
        .map(|d| (d, day_start_ms(d), day_start_ms(d + Duration::days(1))))
        .collect();

    // Segmenty zasahující do období; otevřené počítáme do teď.
    let segments: Vec<(i64, String, Option<String>, i64, i64)> = conn
        .prepare(
            "SELECT session_id, kind, reason, started_at, COALESCE(ended_at, ?3)
             FROM segments
             WHERE started_at < ?2 AND COALESCE(ended_at, ?3) > ?1
             ORDER BY started_at",
        )?
        .query_map(params![range_start, range_end, now], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
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

    for (session, kind, reason, start, end) in &segments {
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
        .prepare(
            "SELECT id, started_at, ended_at FROM sessions
             WHERE started_at < ?2 AND COALESCE(ended_at, ?3) > ?1
             ORDER BY started_at",
        )?
        .query_map(params![range_start, range_end, now], |r| {
            Ok(SessionRow {
                id: r.get(0)?,
                started_at: r.get(1)?,
                ended_at: r.get(2)?,
                worked_ms: 0,
                paused_ms: 0,
                auto_pauses: 0,
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
        .prepare(
            "SELECT app_name,
                    SUM(MIN(COALESCE(ended_at, ?3), ?2) - MAX(started_at, ?1)) AS ms
             FROM app_usage
             WHERE started_at < ?2 AND COALESCE(ended_at, ?3) > ?1
             GROUP BY app_name
             HAVING ms > 0
             ORDER BY ms DESC",
        )?
        .query_map(params![range_start, range_end, now], |r| {
            Ok(AppRow {
                name: r.get(0)?,
                ms: r.get(1)?,
            })
        })?
        .collect::<Result<_, _>>()?;

    Ok(Report {
        from: from.format("%Y-%m-%d").to_string(),
        to: to.format("%Y-%m-%d").to_string(),
        worked_ms: day_rows.iter().map(|d| d.worked_ms).sum(),
        paused_ms: day_rows.iter().map(|d| d.paused_ms).sum(),
        days: day_rows,
        sessions,
        apps,
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
        t.start(midnight - 2 * HOUR, Some("Excel".into())).unwrap();
        t.end(midnight + HOUR).unwrap();

        let r = build(&t.conn, d1, d2, midnight + 5 * HOUR).unwrap();
        assert_eq!(r.days[0].worked_ms, 2 * HOUR);
        assert_eq!(r.days[1].worked_ms, HOUR);
        assert_eq!(r.worked_ms, 3 * HOUR);
        assert_eq!(r.sessions.len(), 1);
        assert_eq!(r.apps[0].ms, 3 * HOUR);

        let only_second = build(&t.conn, d2, d2, midnight + 5 * HOUR).unwrap();
        assert_eq!(only_second.worked_ms, HOUR);
        assert_eq!(only_second.apps[0].ms, HOUR);
    }
}
