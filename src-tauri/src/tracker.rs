//! Stavový automat pracovní akce: Nepracuje → Pracuje ⇄ Pauza → Konec.
//!
//! Tracker nečte hodiny ani systém sám — `tick` dostane aktuální čas, dobu
//! nečinnosti a aplikaci v popředí zvenku, takže je celý testovatelný.

use crate::db;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

/// Mezera mezi dvěma ticky delší než tohle znamená, že počítač spal / byl
/// uspaný — takový čas se nepočítá jako práce.
const SLEEP_GAP_MS: i64 = 60_000;
const HEARTBEAT_MS: i64 = 10_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Off,
    Working,
    Paused,
    AutoPaused,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub idle_minutes: u32,
    pub employee_name: String,
    /// "system" nebo kód z `i18n::SUPPORTED`
    pub language: String,
    /// "system" | "light" | "dark"
    pub theme: String,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            idle_minutes: 10,
            employee_name: String::new(),
            language: "system".into(),
            theme: "system".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatusView {
    pub status: Status,
    pub now: i64,
    pub session_started_at: Option<i64>,
    /// Začátek aktuálního úseku (práce nebo pauzy).
    pub segment_started_at: Option<i64>,
    pub session_worked_ms: i64,
    pub session_paused_ms: i64,
    pub current_app: Option<String>,
}

#[derive(Debug, PartialEq)]
pub enum TickEvent {
    /// Automatická pauza kvůli nečinnosti / uspání, pauza začíná v `since`.
    AutoPaused { since: i64 },
}

pub struct Tracker {
    pub conn: Connection,
    pub settings: Settings,
    status: Status,
    session_id: Option<i64>,
    session_started_at: Option<i64>,
    segment_started_at: Option<i64>,
    app: Option<(i64, String)>,
    last_tick: i64,
    last_heartbeat: i64,
}

impl Tracker {
    pub fn new(conn: Connection) -> rusqlite::Result<Self> {
        db::close_dangling_sessions(&conn)?;
        let mut settings = Settings::default();
        if let Some(v) = db::get_setting(&conn, "idle_minutes")? {
            settings.idle_minutes = v.parse().unwrap_or(settings.idle_minutes);
        }
        if let Some(v) = db::get_setting(&conn, "employee_name")? {
            settings.employee_name = v;
        }
        if let Some(v) = db::get_setting(&conn, "language")? {
            settings.language = v;
        }
        if let Some(v) = db::get_setting(&conn, "theme")? {
            settings.theme = v;
        }
        Ok(Tracker {
            conn,
            settings,
            status: Status::Off,
            session_id: None,
            session_started_at: None,
            segment_started_at: None,
            app: None,
            last_tick: 0,
            last_heartbeat: 0,
        })
    }

    #[cfg(test)]
    pub fn status(&self) -> Status {
        self.status
    }

    /// Skutečně používaný jazyk (při „system“ podle systému).
    pub fn lang(&self) -> &'static str {
        crate::i18n::resolve(&self.settings.language)
    }

    pub fn save_settings(&mut self, settings: Settings) -> rusqlite::Result<()> {
        let valid_language = settings.language == "system"
            || crate::i18n::SUPPORTED.contains(&settings.language.as_str());
        let valid_theme = ["system", "light", "dark"].contains(&settings.theme.as_str());
        let settings = Settings {
            idle_minutes: settings.idle_minutes.clamp(1, 240),
            employee_name: settings.employee_name.trim().to_string(),
            language: if valid_language {
                settings.language
            } else {
                "system".into()
            },
            theme: if valid_theme {
                settings.theme
            } else {
                "system".into()
            },
        };
        db::set_setting(
            &self.conn,
            "idle_minutes",
            &settings.idle_minutes.to_string(),
        )?;
        db::set_setting(&self.conn, "employee_name", &settings.employee_name)?;
        db::set_setting(&self.conn, "language", &settings.language)?;
        db::set_setting(&self.conn, "theme", &settings.theme)?;
        self.settings = settings;
        Ok(())
    }

    pub fn start(&mut self, now: i64, app: Option<String>) -> rusqlite::Result<()> {
        if self.status != Status::Off {
            return Ok(());
        }
        self.conn.execute(
            "INSERT INTO sessions (started_at, last_seen) VALUES (?1, ?1)",
            [now],
        )?;
        let session = self.conn.last_insert_rowid();
        self.session_id = Some(session);
        self.session_started_at = Some(now);
        self.last_tick = now;
        self.last_heartbeat = now;
        self.open_segment("work", None, now)?;
        self.switch_app(app, now)?;
        self.status = Status::Working;
        Ok(())
    }

    pub fn pause(&mut self, now: i64) -> rusqlite::Result<()> {
        if self.status == Status::Working {
            self.enter_pause(now, "manual")?;
            self.status = Status::Paused;
        }
        Ok(())
    }

    pub fn resume(&mut self, now: i64, app: Option<String>) -> rusqlite::Result<()> {
        if matches!(self.status, Status::Paused | Status::AutoPaused) {
            self.close_segment(now)?;
            self.open_segment("work", None, now)?;
            self.switch_app(app, now)?;
            self.status = Status::Working;
            self.last_tick = now;
        }
        Ok(())
    }

    pub fn end(&mut self, now: i64) -> rusqlite::Result<()> {
        let Some(session) = self.session_id else {
            return Ok(());
        };
        db::close_open_rows(&self.conn, session, now)?;
        self.conn.execute(
            "UPDATE sessions SET ended_at = ?2, last_seen = ?2 WHERE id = ?1",
            params![session, now],
        )?;
        self.status = Status::Off;
        self.session_id = None;
        self.session_started_at = None;
        self.segment_started_at = None;
        self.app = None;
        Ok(())
    }

    /// Volá se zhruba jednou za sekundu z vlákna na pozadí.
    pub fn tick(
        &mut self,
        now: i64,
        idle_secs: u64,
        app: Option<String>,
    ) -> rusqlite::Result<Option<TickEvent>> {
        let Some(session) = self.session_id else {
            return Ok(None);
        };
        let previous_tick = self.last_tick;
        self.last_tick = now;

        if now - self.last_heartbeat >= HEARTBEAT_MS {
            self.conn.execute(
                "UPDATE sessions SET last_seen = ?2 WHERE id = ?1",
                params![session, now],
            )?;
            self.last_heartbeat = now;
        }

        if self.status != Status::Working {
            return Ok(None);
        }

        // Počítač spal: nečinnost od posledního ticku, bez ohledu na to, co
        // hlásí systém (probuzení klávesou nečinnost vynuluje).
        if previous_tick > 0 && now - previous_tick > SLEEP_GAP_MS {
            return self.auto_pause(previous_tick).map(Some);
        }

        let limit_secs = self.settings.idle_minutes as u64 * 60;
        if idle_secs >= limit_secs {
            let since = now - idle_secs as i64 * 1000;
            return self.auto_pause(since).map(Some);
        }

        let changed = match (&self.app, &app) {
            (Some((_, current)), Some(new)) => current != new,
            (None, None) => false,
            _ => true,
        };
        if changed {
            self.switch_app(app, now)?;
        }
        Ok(None)
    }

    pub fn view(&self, now: i64) -> rusqlite::Result<StatusView> {
        let (worked, paused) = match self.session_id {
            Some(session) => self.conn.query_row(
                "SELECT
                    COALESCE(SUM(CASE WHEN kind = 'work'
                        THEN COALESCE(ended_at, ?2) - started_at END), 0),
                    COALESCE(SUM(CASE WHEN kind = 'pause'
                        THEN COALESCE(ended_at, ?2) - started_at END), 0)
                 FROM segments WHERE session_id = ?1",
                params![session, now],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )?,
            None => (0, 0),
        };
        Ok(StatusView {
            status: self.status,
            now,
            session_started_at: self.session_started_at,
            segment_started_at: self.segment_started_at,
            session_worked_ms: worked,
            session_paused_ms: paused,
            current_app: match self.status {
                Status::Working => self.app.as_ref().map(|(_, name)| name.clone()),
                _ => None,
            },
        })
    }

    fn auto_pause(&mut self, since: i64) -> rusqlite::Result<TickEvent> {
        // Pauza nesmí začít dřív než aktuální úsek práce.
        let since = since.max(self.segment_started_at.unwrap_or(since));
        self.enter_pause(since, "idle")?;
        self.status = Status::AutoPaused;
        Ok(TickEvent::AutoPaused { since })
    }

    fn enter_pause(&mut self, at: i64, reason: &str) -> rusqlite::Result<()> {
        self.close_segment(at)?;
        self.switch_app(None, at)?;
        self.open_segment("pause", Some(reason), at)
    }

    fn open_segment(&mut self, kind: &str, reason: Option<&str>, at: i64) -> rusqlite::Result<()> {
        let session = self.session_id.expect("segment bez akce");
        self.conn.execute(
            "INSERT INTO segments (session_id, kind, reason, started_at) VALUES (?1, ?2, ?3, ?4)",
            params![session, kind, reason, at],
        )?;
        self.segment_started_at = Some(at);
        Ok(())
    }

    fn close_segment(&mut self, at: i64) -> rusqlite::Result<()> {
        let session = self.session_id.expect("segment bez akce");
        self.conn.execute(
            "UPDATE segments SET ended_at = MAX(started_at, ?2)
             WHERE session_id = ?1 AND ended_at IS NULL",
            params![session, at],
        )?;
        Ok(())
    }

    fn switch_app(&mut self, app: Option<String>, at: i64) -> rusqlite::Result<()> {
        if let Some((row, _)) = self.app.take() {
            self.conn.execute(
                "UPDATE app_usage SET ended_at = MAX(started_at, ?2) WHERE id = ?1",
                params![row, at],
            )?;
        }
        if let (Some(name), Some(session)) = (app, self.session_id) {
            self.conn.execute(
                "INSERT INTO app_usage (session_id, app_name, started_at) VALUES (?1, ?2, ?3)",
                params![session, name, at],
            )?;
            self.app = Some((self.conn.last_insert_rowid(), name));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MIN: i64 = 60_000;

    fn tracker() -> Tracker {
        Tracker::new(db::open_in_memory().unwrap()).unwrap()
    }

    fn app(name: &str) -> Option<String> {
        Some(name.to_string())
    }

    #[test]
    fn pause_is_not_counted_as_work() {
        let mut t = tracker();
        t.start(0, app("Excel")).unwrap();
        t.pause(30 * MIN).unwrap();
        t.resume(45 * MIN, app("Excel")).unwrap();
        let v = t.view(60 * MIN).unwrap();
        assert_eq!(v.session_worked_ms, 45 * MIN);
        assert_eq!(v.session_paused_ms, 15 * MIN);
    }

    #[test]
    fn idle_triggers_retroactive_auto_pause() {
        let mut t = tracker();
        t.start(0, app("Excel")).unwrap();
        for s in 1..=(20 * 60) {
            let now = s * 1000;
            // Uživatel přestal hýbat myší v 5. minutě.
            let idle = if now > 5 * MIN {
                ((now - 5 * MIN) / 1000) as u64
            } else {
                0
            };
            if let Some(ev) = t.tick(now, idle, app("Excel")).unwrap() {
                assert_eq!(ev, TickEvent::AutoPaused { since: 5 * MIN });
                break;
            }
        }
        assert_eq!(t.status(), Status::AutoPaused);
        let v = t.view(20 * MIN).unwrap();
        assert_eq!(v.session_worked_ms, 5 * MIN);
    }

    #[test]
    fn sleep_gap_pauses_at_last_tick() {
        let mut t = tracker();
        t.start(0, app("Excel")).unwrap();
        t.tick(1000, 0, app("Excel")).unwrap();
        let ev = t.tick(2 * 60 * MIN, 0, app("Excel")).unwrap();
        assert_eq!(ev, Some(TickEvent::AutoPaused { since: 1000 }));
    }

    #[test]
    fn app_switches_are_recorded() {
        let mut t = tracker();
        t.start(0, app("Excel")).unwrap();
        t.tick(10 * MIN, 0, app("Chrome")).unwrap();
        t.end(15 * MIN).unwrap();
        let rows: Vec<(String, i64)> = t
            .conn
            .prepare("SELECT app_name, ended_at - started_at FROM app_usage ORDER BY id")
            .unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(
            rows,
            vec![("Excel".into(), 10 * MIN), ("Chrome".into(), 5 * MIN)]
        );
    }

    #[test]
    fn dangling_session_is_closed_at_last_seen() {
        let conn = db::open_in_memory().unwrap();
        let mut t = Tracker::new(conn).unwrap();
        t.start(0, app("Excel")).unwrap();
        t.tick(20_000, 0, app("Excel")).unwrap(); // heartbeat
                                                  // Appka spadla — nový tracker nad stejnou DB.
        let conn = std::mem::replace(&mut t.conn, db::open_in_memory().unwrap());
        let t = Tracker::new(conn).unwrap();
        let ended: i64 = t
            .conn
            .query_row("SELECT ended_at FROM sessions", [], |r| r.get(0))
            .unwrap();
        assert_eq!(ended, 20_000);
    }
}
