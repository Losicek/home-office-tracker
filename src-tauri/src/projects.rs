//! Projekty: uživatel si je vytváří sám, před začátkem práce jeden vybere a
//! pracovní akce se k němu přiřadí. Projekty se nemažou, jen archivují
//! (akce na ně dál odkazují a archivace se dá později synchronizovat).

use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;

/// Nabídka barev v UI; cokoli jiného se nahradí první barvou.
pub const PALETTE: [&str; 8] = [
    "#3b82c4", "#1f9d6b", "#d9822b", "#c2415d", "#7c5cc4", "#2aa3b8", "#b38a1d", "#6b7686",
];

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    pub id: i64,
    pub name: String,
    pub color: String,
    pub archived: bool,
    /// Odpracováno celkem (všechny akce, otevřená do `now`).
    pub total_ms: i64,
}

fn clean_name(name: &str) -> Result<String, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("empty-name".into());
    }
    Ok(name.chars().take(80).collect())
}

fn clean_color(color: &str) -> &'static str {
    PALETTE
        .iter()
        .find(|c| c.eq_ignore_ascii_case(color))
        .copied()
        .unwrap_or(PALETTE[0])
}

pub fn list(conn: &Connection, now: i64) -> rusqlite::Result<Vec<Project>> {
    conn.prepare(
        "SELECT p.id, p.name, p.color, p.archived,
                COALESCE((SELECT SUM(COALESCE(g.ended_at, ?1) - g.started_at)
                          FROM segments g JOIN sessions s ON s.id = g.session_id
                          WHERE s.project_id = p.id AND g.kind = 'work'), 0)
         FROM projects p
         ORDER BY p.archived, p.name COLLATE NOCASE",
    )?
    .query_map([now], |r| {
        Ok(Project {
            id: r.get(0)?,
            name: r.get(1)?,
            color: r.get(2)?,
            archived: r.get::<_, i64>(3)? != 0,
            total_ms: r.get(4)?,
        })
    })?
    .collect()
}

pub fn create(conn: &Connection, name: &str, color: &str, now: i64) -> Result<i64, String> {
    let name = clean_name(name)?;
    conn.execute(
        "INSERT INTO projects (uuid, name, color, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?4)",
        params![
            uuid::Uuid::new_v4().to_string(),
            name,
            clean_color(color),
            now
        ],
    )
    .map_err(|e| e.to_string())?;
    Ok(conn.last_insert_rowid())
}

pub fn update(
    conn: &Connection,
    id: i64,
    name: &str,
    color: &str,
    archived: bool,
    now: i64,
) -> Result<(), String> {
    let name = clean_name(name)?;
    let changed = conn
        .execute(
            "UPDATE projects SET name = ?2, color = ?3, archived = ?4, updated_at = ?5
             WHERE id = ?1",
            params![id, name, clean_color(color), archived as i64, now],
        )
        .map_err(|e| e.to_string())?;
    if changed == 0 {
        return Err("unknown-project".into());
    }
    Ok(())
}

/// Existující, nearchivovaný projekt — jinak `None` (= bez projektu).
pub fn active_id(conn: &Connection, id: Option<i64>) -> rusqlite::Result<Option<i64>> {
    let Some(id) = id else { return Ok(None) };
    conn.query_row(
        "SELECT id FROM projects WHERE id = ?1 AND archived = 0",
        [id],
        |r| r.get(0),
    )
    .optional()
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectRef {
    pub id: i64,
    pub name: String,
    pub color: String,
}

pub fn get_ref(conn: &Connection, id: i64) -> rusqlite::Result<Option<ProjectRef>> {
    conn.query_row(
        "SELECT id, name, color FROM projects WHERE id = ?1",
        [id],
        |r| {
            Ok(ProjectRef {
                id: r.get(0)?,
                name: r.get(1)?,
                color: r.get(2)?,
            })
        },
    )
    .optional()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    #[test]
    fn create_rename_archive() {
        let conn = db::open_in_memory().unwrap();
        let id = create(&conn, "  Klient A ", "#nonsense", 0).unwrap();
        assert!(create(&conn, "   ", PALETTE[1], 0).is_err());
        let p = &list(&conn, 0).unwrap()[0];
        assert_eq!(
            (p.name.as_str(), p.color.as_str()),
            ("Klient A", PALETTE[0])
        );

        update(&conn, id, "Klient B", PALETTE[2], true, 5).unwrap();
        assert_eq!(active_id(&conn, Some(id)).unwrap(), None);
        let p = &list(&conn, 0).unwrap()[0];
        assert!(p.archived);
        assert_eq!(p.name, "Klient B");
    }
}
