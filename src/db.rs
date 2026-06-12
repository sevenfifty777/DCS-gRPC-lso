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
    /// The string label produced by `PassGrade::label()`, e.g. `"OK"` or `"(OK)"`.
    pub pass_grade_label: String,
    pub wire: Option<u8>,
    pub dcs_grading: Option<String>,
}

/// Pass record as returned from a database query (JSON-serialisable for the web API).
#[derive(Debug, serde::Serialize)]
pub struct StoredPass {
    pub id: i64,
    pub timestamp: String,
    pub pilot_name: String,
    pub pass_grade: String,
    pub wire: Option<i64>,
    pub dcs_grading: Option<String>,
}

impl RecoveryDb {
    /// Open (or create) the LSO database at `path` and apply the schema migration.
    pub fn open(path: &Path) -> rusqlite::Result<Self> {
        let conn = Connection::open(path)?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS passes (
                id          INTEGER PRIMARY KEY AUTOINCREMENT,
                timestamp   TEXT    NOT NULL,
                pilot_name  TEXT    NOT NULL,
                pass_grade  TEXT    NOT NULL,
                wire        INTEGER,
                dcs_grading TEXT
            );",
        )?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    /// Persist a completed recovery pass.
    pub fn insert(&self, pass: &DbPass) -> rusqlite::Result<()> {
        let conn = self.conn.lock().expect("db mutex poisoned");
        conn.execute(
            "INSERT INTO passes (timestamp, pilot_name, pass_grade, wire, dcs_grading)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                &pass.timestamp,
                &pass.pilot_name,
                &pass.pass_grade_label,
                pass.wire.map(|w| w as i64),
                &pass.dcs_grading,
            ],
        )?;
        Ok(())
    }

    /// Return all passes ordered newest-first.
    pub fn all_passes(&self) -> rusqlite::Result<Vec<StoredPass>> {
        let conn = self.conn.lock().expect("db mutex poisoned");
        let mut stmt = conn.prepare(
            "SELECT id, timestamp, pilot_name, pass_grade, wire, dcs_grading
             FROM passes ORDER BY id DESC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(StoredPass {
                id: row.get(0)?,
                timestamp: row.get(1)?,
                pilot_name: row.get(2)?,
                pass_grade: row.get(3)?,
                wire: row.get(4)?,
                dcs_grading: row.get(5)?,
            })
        })?;
        rows.collect()
    }
}
