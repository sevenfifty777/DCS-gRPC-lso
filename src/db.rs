use std::path::Path;
use std::sync::{Arc, Mutex};

use rusqlite::{params, Connection};

/// A shared, thread-safe handle to the LSO SQLite database.
pub type SharedDb = Arc<RecoveryDb>;

/// Thin wrapper around a SQLite connection safe to share across async tasks.
///
/// All methods take `&self` and lock the inner `Mutex` for the duration of the
/// operation. Callers in async code should use `tokio::task::spawn_blocking`.
pub struct RecoveryDb {
    conn: Mutex<Connection>,
}

/// Pass record ready for insertion, with all values owned (required for `spawn_blocking`).
pub struct DbPass {
    pub timestamp: String,
    pub pilot_name: String,
    pub pilot_ucid: Option<String>,
    pub aircraft_id: Option<i64>,
    /// The string label produced by `PassGrade::label()`, e.g. `"OK"` or `"(OK)"`.
    pub pass_grade_label: String,
    pub wire: Option<u8>,
    pub dcs_grading: Option<String>,
    pub aircraft_type: Option<String>,
    /// DCS theatre / map name (e.g. `"Caucasus"`, `"Syria"`, `"PersianGulf"`).
    pub map_name: Option<String>,
    /// UTC datetime of the recovery in ISO-8601 format (`YYYY-MM-DD HH:MM:SS`).
    pub grade_date: String,
    /// Numeric NAVAIR grade points (e.g. 4.0 for OK, 3.0 for (OK)).
    pub grade_points: f64,
    /// In-mission date/time from DCS scenario clock (ISO-8601 string).
    pub mission_datetime: String,
    /// Outcome of the pass (e.g. "Landed", "Bolter", "Qualif Bolter", "Waveoff").
    pub outcome: String,
}

/// Pass record as returned from a database query (JSON-serialisable for the web API).
#[derive(Debug, serde::Serialize)]
pub struct StoredPass {
    pub id: i64,
    pub timestamp: String,
    pub pilot_name: String,
    pub pilot_ucid: Option<String>,
    pub aircraft_id: Option<i64>,
    pub pass_grade: String,
    pub wire: Option<i64>,
    pub dcs_grading: Option<String>,
    pub aircraft_type: Option<String>,
    /// DCS theatre / map name.
    pub map_name: Option<String>,
    /// Plain-English translation of `dcs_grading`, computed at query time.
    pub lso_notes: Option<String>,
    pub grade_date: String,
    pub grade_points: f64,
    /// In-mission date/time from DCS scenario clock.
    pub mission_datetime: String,
    pub outcome: String,
}

impl RecoveryDb {
    /// Open (or create) the LSO database at `path` and apply the schema migration.
    pub fn open(path: &Path) -> rusqlite::Result<Self> {
        let conn = Connection::open(path)?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS passes (
                id             INTEGER PRIMARY KEY AUTOINCREMENT,
                timestamp      TEXT    NOT NULL,
                pilot_name     TEXT    NOT NULL,
                pilot_ucid     TEXT,
                aircraft_id    INTEGER,
                pass_grade     TEXT    NOT NULL,
                wire           INTEGER,
                dcs_grading    TEXT,
                aircraft_type  TEXT,
                map_name       TEXT,
                grade_date         TEXT    NOT NULL DEFAULT '',
                grade_points       REAL    NOT NULL DEFAULT 0.0,
                mission_datetime   TEXT    NOT NULL DEFAULT '',
                outcome            TEXT    NOT NULL DEFAULT ''
            );",
        )?;
        // Migrations: add columns to pre-existing databases that lack them.
        // Each ALTER TABLE is silently ignored when the column already exists
        // (SQLite does not support IF NOT EXISTS on ALTER TABLE ADD COLUMN).
        let _ = conn.execute_batch("ALTER TABLE passes ADD COLUMN aircraft_type  TEXT;");
        let _ = conn.execute_batch("ALTER TABLE passes ADD COLUMN map_name       TEXT;");
        let _ = conn.execute_batch("ALTER TABLE passes ADD COLUMN grade_date     TEXT    NOT NULL DEFAULT '';");
        let _ = conn.execute_batch("ALTER TABLE passes ADD COLUMN grade_points   REAL    NOT NULL DEFAULT 0.0;");
        let _ = conn.execute_batch("ALTER TABLE passes ADD COLUMN pilot_ucid     TEXT;");
        let _ = conn.execute_batch("ALTER TABLE passes ADD COLUMN aircraft_id    INTEGER;");
        let _ = conn.execute_batch("ALTER TABLE passes ADD COLUMN mission_datetime TEXT NOT NULL DEFAULT '';");
        let _ = conn.execute_batch("ALTER TABLE passes ADD COLUMN outcome        TEXT NOT NULL DEFAULT '';");
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    /// Persist a completed recovery pass.
    pub fn insert(&self, pass: &DbPass) -> rusqlite::Result<()> {
        let conn = self.conn.lock().expect("db mutex poisoned");
        conn.execute(
            "INSERT INTO passes \
                (timestamp, pilot_name, pilot_ucid, aircraft_id, pass_grade, wire, dcs_grading, aircraft_type, \
                 map_name, grade_date, grade_points, mission_datetime, outcome) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
            params![
                &pass.timestamp,
                &pass.pilot_name,
                &pass.pilot_ucid,
                pass.aircraft_id,
                &pass.pass_grade_label,
                pass.wire.map(|w| w as i64),
                &pass.dcs_grading,
                &pass.aircraft_type,
                &pass.map_name,
                &pass.grade_date,
                pass.grade_points,
                &pass.mission_datetime,
                &pass.outcome,
            ],
        )?;
        Ok(())
    }

    /// Return all passes ordered newest-first.
    pub fn all_passes(&self) -> rusqlite::Result<Vec<StoredPass>> {
        let conn = self.conn.lock().expect("db mutex poisoned");
        let mut stmt = conn.prepare(
            "SELECT id, timestamp, pilot_name, pilot_ucid, aircraft_id, pass_grade, wire, dcs_grading, aircraft_type, \
                    map_name, grade_date, grade_points, mission_datetime, outcome \
             FROM passes ORDER BY id DESC",
        )?;
        let rows = stmt.query_map([], |row| {
            let dcs_grading: Option<String> = row.get(7)?;
            let lso_notes = dcs_grading
                .as_deref()
                .map(crate::lso_notation::to_english)
                .filter(|s| !s.is_empty());
            Ok(StoredPass {
                id: row.get(0)?,
                timestamp: row.get(1)?,
                pilot_name: row.get(2)?,
                pilot_ucid: row.get(3)?,
                aircraft_id: row.get(4)?,
                pass_grade: row.get(5)?,
                wire: row.get(6)?,
                dcs_grading,
                aircraft_type: row.get(8)?,
                map_name: row.get(9)?,
                lso_notes,
                grade_date: row.get(10)?,
                grade_points: row.get(11)?,
                mission_datetime: row.get(12)?,
                outcome: row.get(13)?,
            })
        })?;
        rows.collect()
    }
}
