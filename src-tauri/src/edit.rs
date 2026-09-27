//! Ruční úpravy pracovních akcí: změna časů, projektu a poznámky, přidání
//! zapomenuté práce a smazání. Upravené akce mají `edited = 1`, ručně
//! přidané `manual = 1` — obojí se ukazuje v přehledech i v exportu.
//!
//! Upravovat jde jen akce z tohoto počítače (ne synchronizované odjinud) a
//! u právě běžící akce jen poznámku. Chyby vrací kódy (`"overlap"`, …), které
//! UI přeloží.

use crate::{db, projects};
use rusqlite::{params, Connection, OptionalExtension};
use serde::Deserialize;

const MAX_NOTE: usize = 500;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionInput {
    pub started_at: i64,
    pub ended_at: i64,
    pub project: Option<i64>,
    pub note: Option<String>,
}

fn clean_note(note: Option<String>) -> Option<String> {
    note.map(|n| n.trim().chars().take(MAX_NOTE).collect::<String>())
        .filter(|n| !n.is_empty())
}

fn e(err: rusqlite::Error) -> String {
    err.to_string()
}

/// Ověří časy nové/upravené akce: začátek < konec, ne v budoucnu, žádné
/// překrytí s jinou akcí tohoto počítače.
fn validate(
    conn: &Connection,
    id: Option<i64>,
    start: i64,
    end: i64,
    now: i64,
) -> Result<(), String> {
    if start >= end {
        return Err("invalid-range".into());
    }
    if end > now {
        return Err("in-future".into());
    }
    let overlap: bool = conn
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sessions
              WHERE device_id IS NULL AND id != ?1
                AND started_at < ?3 AND COALESCE(ended_at, ?4) > ?2)",
            params![id.unwrap_or(-1), start, end, now],
            |r| r.get(0),
        )
        .map_err(e)?;
    if overlap {
        return Err("overlap".into());
    }
    Ok(())
}

/// Akce z tohoto počítače: (started_at, ended_at).
fn local_session(conn: &Connection, id: i64) -> Result<(i64, Option<i64>), String> {
    conn.query_row(
        "SELECT started_at, ended_at FROM sessions WHERE id = ?1 AND device_id IS NULL",
        [id],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )
    .optional()
    .map_err(e)?
    .ok_or_else(|| "not-editable".into())
}

pub fn update_session(
    conn: &Connection,
    id: i64,
    running: Option<i64>,
    input: SessionInput,
    now: i64,
) -> Result<(), String> {
    let (old_start, old_end) = local_session(conn, id)?;
    let note = clean_note(input.note);
    let project = projects::active_id(conn, input.project).map_err(e)?.or(
        // Archivovaný projekt, který už akce má, smí zůstat.
        conn.query_row(
            "SELECT project_id FROM sessions WHERE id = ?1 AND project_id = ?2",
            params![id, input.project],
            |r| r.get(0),
        )
        .optional()
        .map_err(e)?
        .flatten(),
    );

    if Some(id) == running {
        // Běžící akce: jen poznámka (projekt se mění přepnutím projektu).
        conn.execute(
            "UPDATE sessions SET note = ?2 WHERE id = ?1",
            params![id, note],
        )
        .map_err(e)?;
        db::mark_dirty(conn, old_start).map_err(e)?;
        return Ok(());
    }
    let old_end = old_end.ok_or("not-editable")?;
    let (start, end) = (input.started_at, input.ended_at);
    let times_changed = start != old_start || end != old_end;
    if times_changed {
        validate(conn, Some(id), start, end, now)?;
    }

    let tx = conn.unchecked_transaction().map_err(e)?;
    if times_changed {
        for table in ["segments", "app_usage"] {
            // Ořez na nový rozsah; úseky úplně mimo pryč.
            tx.execute(
                &format!(
                    "DELETE FROM {table} WHERE session_id = ?1
                     AND (COALESCE(ended_at, started_at) <= ?2 OR started_at >= ?3)"
                ),
                params![id, start, end],
            )
            .map_err(e)?;
            tx.execute(
                &format!(
                    "UPDATE {table} SET started_at = MAX(started_at, ?2),
                            ended_at = MIN(COALESCE(ended_at, ?3), ?3)
                     WHERE session_id = ?1"
                ),
                params![id, start, end],
            )
            .map_err(e)?;
        }
        // Prodloužení na začátku / konci = doplněná práce.
        if start < old_start {
            tx.execute(
                "INSERT INTO segments (session_id, kind, reason, started_at, ended_at)
                 VALUES (?1, 'work', 'manual', ?2, ?3)",
                params![id, start, old_start.min(end)],
            )
            .map_err(e)?;
        }
        if end > old_end {
            tx.execute(
                "INSERT INTO segments (session_id, kind, reason, started_at, ended_at)
                 VALUES (?1, 'work', 'manual', ?2, ?3)",
                params![id, old_end.max(start), end],
            )
            .map_err(e)?;
        }
    }
    tx.execute(
        "UPDATE sessions SET started_at = ?2, ended_at = ?3, last_seen = ?3,
                project_id = ?4, note = ?5, edited = MAX(edited, ?6)
         WHERE id = ?1",
        params![id, start, end, project, note, times_changed as i64],
    )
    .map_err(e)?;
    tx.commit().map_err(e)?;
    db::mark_dirty(conn, old_start).map_err(e)?;
    db::mark_dirty(conn, start).map_err(e)?;
    Ok(())
}

pub fn add_session(conn: &Connection, input: SessionInput, now: i64) -> Result<i64, String> {
    let (start, end) = (input.started_at, input.ended_at);
    validate(conn, None, start, end, now)?;
    let project = projects::active_id(conn, input.project).map_err(e)?;
    let tx = conn.unchecked_transaction().map_err(e)?;
    tx.execute(
        "INSERT INTO sessions (started_at, ended_at, last_seen, project_id, uuid, note, manual)
         VALUES (?1, ?2, ?2, ?3, ?4, ?5, 1)",
        params![
            start,
            end,
            project,
            uuid::Uuid::new_v4().to_string(),
            clean_note(input.note)
        ],
    )
    .map_err(e)?;
    let id = tx.last_insert_rowid();
    tx.execute(
        "INSERT INTO segments (session_id, kind, reason, started_at, ended_at)
         VALUES (?1, 'work', 'manual', ?2, ?3)",
        params![id, start, end],
    )
    .map_err(e)?;
    tx.commit().map_err(e)?;
    db::mark_dirty(conn, start).map_err(e)?;
    Ok(id)
}

pub fn delete_session(conn: &Connection, id: i64, running: Option<i64>) -> Result<(), String> {
    if Some(id) == running {
        return Err("running".into());
    }
    let (start, _) = local_session(conn, id)?;
    let tx = conn.unchecked_transaction().map_err(e)?;
    tx.execute("DELETE FROM app_usage WHERE session_id = ?1", [id])
        .map_err(e)?;
    tx.execute("DELETE FROM segments WHERE session_id = ?1", [id])
        .map_err(e)?;
    tx.execute("DELETE FROM sessions WHERE id = ?1", [id])
        .map_err(e)?;
    tx.commit().map_err(e)?;
    db::mark_dirty(conn, start).map_err(e)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tracker::Tracker;

    const MIN: i64 = 60_000;
    const NOW: i64 = 10_000 * MIN;

    fn worked(conn: &Connection, id: i64) -> i64 {
        conn.query_row(
            "SELECT COALESCE(SUM(ended_at - started_at), 0) FROM segments
             WHERE session_id = ?1 AND kind = 'work'",
            [id],
            |r| r.get(0),
        )
        .unwrap()
    }

    fn session(t: &mut Tracker, start: i64, end: i64) -> i64 {
        t.start(start, Some("Excel".into()), None).unwrap();
        t.end(end).unwrap();
        t.conn
            .query_row("SELECT MAX(id) FROM sessions", [], |r| r.get(0))
            .unwrap()
    }

    fn input(start: i64, end: i64) -> SessionInput {
        SessionInput {
            started_at: start,
            ended_at: end,
            project: None,
            note: Some(" Klient A ".into()),
        }
    }

    #[test]
    fn extend_and_shrink_session() {
        let mut t = Tracker::new(db::open_in_memory().unwrap()).unwrap();
        let id = session(&mut t, 100 * MIN, 160 * MIN);

        // Zapomenutý konec: prodloužit o 30 min.
        update_session(&t.conn, id, None, input(100 * MIN, 190 * MIN), NOW).unwrap();
        assert_eq!(worked(&t.conn, id), 90 * MIN);
        let (edited, note): (i64, String) = t
            .conn
            .query_row(
                "SELECT edited, note FROM sessions WHERE id = ?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!((edited, note.as_str()), (1, "Klient A"));

        // Zkrátit z obou stran; aplikace se ořízne taky.
        update_session(&t.conn, id, None, input(110 * MIN, 150 * MIN), NOW).unwrap();
        assert_eq!(worked(&t.conn, id), 40 * MIN);
        let apps: i64 = t
            .conn
            .query_row(
                "SELECT SUM(ended_at - started_at) FROM app_usage WHERE session_id = ?1",
                [id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(apps, 40 * MIN);
    }

    #[test]
    fn manual_add_validates_and_delete_works() {
        let mut t = Tracker::new(db::open_in_memory().unwrap()).unwrap();
        session(&mut t, 100 * MIN, 160 * MIN);
        assert_eq!(
            add_session(&t.conn, input(150 * MIN, 200 * MIN), NOW).unwrap_err(),
            "overlap"
        );
        assert_eq!(
            add_session(&t.conn, input(300 * MIN, 200 * MIN), NOW).unwrap_err(),
            "invalid-range"
        );
        assert_eq!(
            add_session(&t.conn, input(NOW, NOW + MIN), NOW).unwrap_err(),
            "in-future"
        );
        let id = add_session(&t.conn, input(200 * MIN, 260 * MIN), NOW).unwrap();
        assert_eq!(worked(&t.conn, id), 60 * MIN);
        delete_session(&t.conn, id, None).unwrap();
        let count: i64 = t
            .conn
            .query_row("SELECT COUNT(*) FROM sessions", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn running_session_only_note() {
        let mut t = Tracker::new(db::open_in_memory().unwrap()).unwrap();
        t.start(100 * MIN, None, None).unwrap();
        let id: i64 = t
            .conn
            .query_row("SELECT id FROM sessions", [], |r| r.get(0))
            .unwrap();
        update_session(&t.conn, id, Some(id), input(0, 1), NOW).unwrap();
        let start: i64 = t
            .conn
            .query_row("SELECT started_at FROM sessions", [], |r| r.get(0))
            .unwrap();
        assert_eq!(start, 100 * MIN);
        assert_eq!(
            delete_session(&t.conn, id, Some(id)).unwrap_err(),
            "running"
        );
    }
}
