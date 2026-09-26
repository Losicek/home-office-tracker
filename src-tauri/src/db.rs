//! SQLite úložiště. Všechny časy jsou UTC milisekundy od epochy; převod na
//! místní čas dělají až přehledy a exporty.
//!
//! - `sessions`  — jedna „pracovní akce“ (Začít pracovat → Konec práce)
//! - `segments`  — souvislé úseky práce (`work`) a pauz (`pause`) uvnitř akce
//! - `app_usage` — úseky, kdy byla v popředí daná aplikace (jen během práce)

use rusqlite::{params, Connection, OptionalExtension};
use std::path::Path;

const SCHEMA_VERSION: i32 = 1;

pub fn open(path: &Path) -> rusqlite::Result<Connection> {
    let conn = Connection::open(path)?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    migrate(&conn)?;
    Ok(conn)
}

#[cfg(test)]
pub fn open_in_memory() -> rusqlite::Result<Connection> {
    let conn = Connection::open_in_memory()?;
    migrate(&conn)?;
    Ok(conn)
}

fn migrate(conn: &Connection) -> rusqlite::Result<()> {
    let version: i32 = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
    if version < 1 {
        conn.execute_batch(
            "CREATE TABLE sessions (
                 id         INTEGER PRIMARY KEY,
                 started_at INTEGER NOT NULL,
                 ended_at   INTEGER,
                 last_seen  INTEGER NOT NULL
             );
             CREATE TABLE segments (
                 id         INTEGER PRIMARY KEY,
                 session_id INTEGER NOT NULL REFERENCES sessions(id),
                 kind       TEXT NOT NULL CHECK (kind IN ('work', 'pause')),
                 reason     TEXT,
                 started_at INTEGER NOT NULL,
                 ended_at   INTEGER
             );
             CREATE TABLE app_usage (
                 id         INTEGER PRIMARY KEY,
                 session_id INTEGER NOT NULL REFERENCES sessions(id),
                 app_name   TEXT NOT NULL,
                 started_at INTEGER NOT NULL,
                 ended_at   INTEGER
             );
             CREATE TABLE settings (
                 key   TEXT PRIMARY KEY,
                 value TEXT NOT NULL
             );
             CREATE INDEX segments_time ON segments(started_at, ended_at);
             CREATE INDEX app_usage_time ON app_usage(started_at, ended_at);
             CREATE INDEX sessions_time ON sessions(started_at, ended_at);",
        )?;
    }
    conn.pragma_update(None, "user_version", SCHEMA_VERSION)?;
    Ok(())
}

/// Po pádu / vypnutí počítače zůstane akce otevřená. Uzavřeme ji k poslednímu
/// okamžiku, kdy o ní appka věděla (`last_seen`), ať se nepočítá čas, kdy
/// appka vůbec neběžela.
pub fn close_dangling_sessions(conn: &Connection) -> rusqlite::Result<usize> {
    let open: Vec<(i64, i64)> = conn
        .prepare("SELECT id, last_seen FROM sessions WHERE ended_at IS NULL")?
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<Result<_, _>>()?;
    for (id, last_seen) in &open {
        close_open_rows(conn, *id, *last_seen)?;
        conn.execute(
            "UPDATE sessions SET ended_at = MAX(started_at, ?2) WHERE id = ?1",
            params![id, last_seen],
        )?;
    }
    Ok(open.len())
}

/// Uzavře všechny otevřené segmenty i úseky aplikací dané akce k času `at`
/// (nikdy ne dřív, než úsek začal).
pub fn close_open_rows(conn: &Connection, session_id: i64, at: i64) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE segments SET ended_at = MAX(started_at, ?2)
         WHERE session_id = ?1 AND ended_at IS NULL",
        params![session_id, at],
    )?;
    conn.execute(
        "UPDATE app_usage SET ended_at = MAX(started_at, ?2)
         WHERE session_id = ?1 AND ended_at IS NULL",
        params![session_id, at],
    )?;
    Ok(())
}

pub fn get_setting(conn: &Connection, key: &str) -> rusqlite::Result<Option<String>> {
    conn.query_row("SELECT value FROM settings WHERE key = ?1", [key], |r| {
        r.get(0)
    })
    .optional()
}

pub fn set_setting(conn: &Connection, key: &str, value: &str) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO settings (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![key, value],
    )?;
    Ok(())
}
