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
    pub recovery_id: String,
    pub timestamp: String,
    pub pilot_name: String,
    pub pilot_ucid: Option<String>,
    pub aircraft_id: Option<i64>,
    /// The string label produced by `PassGrade::label()`, e.g. `"OK"` or `"(OK)"`.
    pub pass_grade_label: String,
    pub wire: Option<u8>,
    pub spot: Option<String>,
    pub spot_grade: Option<String>,
    pub spot_distance_m: Option<f64>,
    pub intended_spot: Option<String>,
    pub actual_nearest_spot: Option<String>,
    pub distance_to_intended_spot_m: Option<f64>,
    pub dcs_grading: Option<String>,
    pub aircraft_type: Option<String>,
    /// DCS theatre / map name (e.g. `"Caucasus"`, `"Syria"`, `"PersianGulf"`).
    pub map_name: Option<String>,
    /// UTC datetime of the recovery in ISO-8601 format (`YYYY-MM-DD HH:MM:SS`).
    pub grade_date: String,
    /// Numeric project score (e.g. 4.0 for OK, 3.0 for (OK)).
    pub grade_points: Option<f64>,
    /// Distinguishes a genuine zero-point grade from an incomplete/no-points result.
    pub points_awarded: bool,
    /// In-mission date/time from DCS scenario clock (ISO-8601 string).
    pub mission_datetime: String,
    pub outcome: String,
    pub pilot_kind: String,
    pub carrier_id: u32,
    pub carrier_name: String,
    pub carrier_type: String,
    pub recovery_mode: String,
    pub session_id: i64,
    pub generation: u64,
    pub completeness: String,
    pub max_sample_gap_ms: f64,
    pub max_skew_ms: f64,
    pub wire_estimated: Option<u8>,
    pub wire_dcs: Option<u8>,
    pub wire_divergent: bool,
    pub confidence: String,
    pub cause: String,
    pub grading_version: String,
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
    pub spot: Option<String>,
    pub spot_grade: Option<String>,
    pub spot_distance_m: Option<f64>,
    pub intended_spot: Option<String>,
    pub actual_nearest_spot: Option<String>,
    pub distance_to_intended_spot_m: Option<f64>,
    pub dcs_grading: Option<String>,
    pub aircraft_type: Option<String>,
    /// DCS theatre / map name.
    pub map_name: Option<String>,
    /// Plain-English translation of `dcs_grading`, computed at query time.
    pub lso_notes: Option<String>,
    pub grade_date: String,
    pub grade_points: Option<f64>,
    pub points_awarded: Option<bool>,
    /// In-mission date/time from DCS scenario clock.
    pub mission_datetime: String,
    pub outcome: String,
    pub recovery_id: Option<String>,
    pub pilot_kind: Option<String>,
    pub carrier_id: Option<i64>,
    pub carrier_name: Option<String>,
    pub carrier_type: Option<String>,
    pub recovery_mode: Option<String>,
    pub session_id: Option<i64>,
    pub generation: Option<i64>,
    pub completeness: Option<String>,
    pub max_sample_gap_ms: Option<f64>,
    pub max_skew_ms: Option<f64>,
    pub wire_estimated: Option<i64>,
    pub wire_dcs: Option<i64>,
    pub wire_divergent: Option<bool>,
    pub confidence: Option<String>,
    pub cause: Option<String>,
    pub grading_version: Option<String>,
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
                spot           TEXT,
                spot_grade     TEXT,
                spot_distance_m REAL,
                dcs_grading    TEXT,
                aircraft_type  TEXT,
                map_name       TEXT,
                grade_date         TEXT    NOT NULL DEFAULT '',
                grade_points       REAL    NOT NULL DEFAULT 0.0,
                mission_datetime   TEXT    NOT NULL DEFAULT '',
                outcome            TEXT    NOT NULL DEFAULT ''
            );",
        )?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS schema_migrations (
                version INTEGER PRIMARY KEY,
                applied_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
            );",
        )?;
        for (name, definition) in [
            ("aircraft_type", "TEXT"),
            ("map_name", "TEXT"),
            ("grade_date", "TEXT NOT NULL DEFAULT ''"),
            ("grade_points", "REAL NOT NULL DEFAULT 0.0"),
            ("pilot_ucid", "TEXT"),
            ("aircraft_id", "INTEGER"),
            ("mission_datetime", "TEXT NOT NULL DEFAULT ''"),
            ("spot", "TEXT"),
            ("spot_grade", "TEXT"),
            ("spot_distance_m", "REAL"),
            ("outcome", "TEXT NOT NULL DEFAULT ''"),
            ("recovery_id", "TEXT"),
            ("pilot_kind", "TEXT"),
            ("carrier_id", "INTEGER"),
            ("carrier_name", "TEXT"),
            ("carrier_type", "TEXT"),
            ("recovery_mode", "TEXT"),
            ("session_id", "INTEGER"),
            ("generation", "INTEGER"),
            ("completeness", "TEXT"),
            ("max_sample_gap_ms", "REAL"),
            ("max_skew_ms", "REAL"),
            ("wire_estimated", "INTEGER"),
            ("wire_dcs", "INTEGER"),
            ("wire_divergent", "INTEGER NOT NULL DEFAULT 0"),
            ("confidence", "TEXT"),
            ("cause", "TEXT"),
            ("grading_version", "TEXT"),
            // Existing rows predate optional points and historically always
            // represented an awarded numeric value.
            ("points_awarded", "INTEGER NOT NULL DEFAULT 1"),
            ("intended_spot", "TEXT"),
            ("actual_nearest_spot", "TEXT"),
            ("distance_to_intended_spot_m", "REAL"),
        ] {
            ensure_column(&conn, "passes", name, definition)?;
        }
        conn.execute_batch(
            "CREATE UNIQUE INDEX IF NOT EXISTS passes_recovery_id_unique
             ON passes(recovery_id) WHERE recovery_id IS NOT NULL;
             INSERT OR IGNORE INTO schema_migrations(version) VALUES (2);
             INSERT OR IGNORE INTO schema_migrations(version) VALUES (3);
             INSERT OR IGNORE INTO schema_migrations(version) VALUES (4);",
        )?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    /// Persist a completed recovery pass.
    pub fn insert(&self, pass: &DbPass) -> rusqlite::Result<bool> {
        let conn = self.conn.lock().expect("db mutex poisoned");
        let inserted = conn.execute(
            "INSERT OR IGNORE INTO passes \
                (timestamp, pilot_name, pilot_ucid, aircraft_id, pass_grade, wire, spot, spot_grade, spot_distance_m, dcs_grading, aircraft_type, \
                 map_name, grade_date, grade_points, mission_datetime, outcome, recovery_id, pilot_kind, carrier_id, carrier_name, carrier_type,
                 recovery_mode, session_id, generation, completeness, max_sample_gap_ms, max_skew_ms, wire_estimated, wire_dcs, wire_divergent,
                 confidence, cause, grading_version, points_awarded, intended_spot, actual_nearest_spot, distance_to_intended_spot_m) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20,
                     ?21, ?22, ?23, ?24, ?25, ?26, ?27, ?28, ?29, ?30, ?31, ?32, ?33, ?34, ?35, ?36, ?37)",
            params![
                &pass.timestamp,
                &pass.pilot_name,
                &pass.pilot_ucid,
                pass.aircraft_id,
                &pass.pass_grade_label,
                pass.wire.map(|w| w as i64),
                &pass.spot,
                &pass.spot_grade,
                pass.spot_distance_m,
                &pass.dcs_grading,
                &pass.aircraft_type,
                &pass.map_name,
                &pass.grade_date,
                pass.grade_points.unwrap_or_default(),
                &pass.mission_datetime,
                &pass.outcome,
                &pass.recovery_id,
                &pass.pilot_kind,
                pass.carrier_id as i64,
                &pass.carrier_name,
                &pass.carrier_type,
                &pass.recovery_mode,
                pass.session_id,
                pass.generation as i64,
                &pass.completeness,
                pass.max_sample_gap_ms,
                pass.max_skew_ms,
                pass.wire_estimated.map(i64::from),
                pass.wire_dcs.map(i64::from),
                pass.wire_divergent,
                &pass.confidence,
                &pass.cause,
                &pass.grading_version,
                pass.points_awarded,
                &pass.intended_spot,
                &pass.actual_nearest_spot,
                pass.distance_to_intended_spot_m,
            ],
        )?;
        Ok(inserted == 1)
    }

    /// Return all passes ordered newest-first.
    pub fn all_passes(&self) -> rusqlite::Result<Vec<StoredPass>> {
        let conn = self.conn.lock().expect("db mutex poisoned");
        let mut stmt = conn.prepare(
            "SELECT id, timestamp, pilot_name, pilot_ucid, aircraft_id, pass_grade, wire, spot, spot_grade, spot_distance_m, dcs_grading, aircraft_type, \
                    map_name, grade_date, grade_points, mission_datetime, outcome, recovery_id, pilot_kind, carrier_id, carrier_name, carrier_type,
                    recovery_mode, session_id, generation, completeness, max_sample_gap_ms, max_skew_ms, wire_estimated, wire_dcs, wire_divergent,
                    confidence, cause, grading_version, points_awarded, intended_spot, actual_nearest_spot, distance_to_intended_spot_m \
             FROM passes ORDER BY id DESC",
        )?;
        let rows = stmt.query_map([], |row| {
            let dcs_grading: Option<String> = row.get(10)?;
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
                spot: row.get(7)?,
                spot_grade: row.get(8)?,
                spot_distance_m: row.get(9)?,
                dcs_grading,
                aircraft_type: row.get(11)?,
                map_name: row.get(12)?,
                lso_notes,
                grade_date: row.get(13)?,
                grade_points: row.get(14)?,
                mission_datetime: row.get(15)?,
                outcome: row.get(16)?,
                recovery_id: row.get(17)?,
                pilot_kind: row.get(18)?,
                carrier_id: row.get(19)?,
                carrier_name: row.get(20)?,
                carrier_type: row.get(21)?,
                recovery_mode: row.get(22)?,
                session_id: row.get(23)?,
                generation: row.get(24)?,
                completeness: row.get(25)?,
                max_sample_gap_ms: row.get(26)?,
                max_skew_ms: row.get(27)?,
                wire_estimated: row.get(28)?,
                wire_dcs: row.get(29)?,
                wire_divergent: row.get(30)?,
                confidence: row.get(31)?,
                cause: row.get(32)?,
                grading_version: row.get(33)?,
                points_awarded: row.get(34)?,
                intended_spot: row.get(35)?,
                actual_nearest_spot: row.get(36)?,
                distance_to_intended_spot_m: row.get(37)?,
            })
        })?;
        rows.collect()
    }

    #[cfg(test)]
    pub(crate) fn force_query_failure_for_test(&self) {
        self.conn
            .lock()
            .expect("db mutex poisoned")
            .execute_batch("DROP TABLE passes;")
            .expect("invalidate test database");
    }
}

fn ensure_column(
    conn: &Connection,
    table: &str,
    column: &str,
    definition: &str,
) -> rusqlite::Result<()> {
    let mut statement = conn.prepare(&format!("PRAGMA table_info({table})"))?;
    let columns = statement
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    if !columns.iter().any(|existing| existing == column) {
        conn.execute_batch(&format!(
            "ALTER TABLE {table} ADD COLUMN {column} {definition};"
        ))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn outcome_round_trips_through_sqlite() {
        let db = RecoveryDb::open(Path::new(":memory:")).expect("open in-memory database");
        let entry = DbPass {
            recovery_id: "test-recovery".to_string(),
            timestamp: "LSO-test".to_string(),
            pilot_name: "Pilot".to_string(),
            pilot_ucid: None,
            aircraft_id: Some(1),
            pass_grade_label: "(OK)".to_string(),
            wire: Some(3),
            spot: None,
            spot_grade: None,
            spot_distance_m: None,
            intended_spot: Some("7.5".to_string()),
            actual_nearest_spot: Some("7.5".to_string()),
            distance_to_intended_spot_m: Some(1.25),
            dcs_grading: None,
            aircraft_type: Some("F/A-18C".to_string()),
            map_name: Some("Caucasus".to_string()),
            grade_date: "2026-08-26 00:00:00".to_string(),
            grade_points: Some(3.0),
            points_awarded: true,
            mission_datetime: "2026-08-26T00:00:00Z".to_string(),
            outcome: "Qualif Bolter".to_string(),
            pilot_kind: "human".to_string(),
            carrier_id: 1,
            carrier_name: "CVN".to_string(),
            carrier_type: "CVN_71".to_string(),
            recovery_mode: "arrested".to_string(),
            session_id: 42,
            generation: 1,
            completeness: "complete".to_string(),
            max_sample_gap_ms: 100.0,
            max_skew_ms: 0.0,
            wire_estimated: Some(3),
            wire_dcs: Some(3),
            wire_divergent: false,
            confidence: "high".to_string(),
            cause: "correlated_touchdown".to_string(),
            grading_version: "project-derived-v1".to_string(),
        };
        assert!(db.insert(&entry).expect("insert pass"));
        assert!(!db.insert(&entry).expect("duplicate is idempotent"));

        let passes = db.all_passes().expect("query passes");

        assert_eq!(passes.len(), 1);
        assert_eq!(passes[0].outcome, "Qualif Bolter");
        assert_eq!(passes[0].points_awarded, Some(true));
        assert_eq!(passes[0].intended_spot.as_deref(), Some("7.5"));
        assert_eq!(passes[0].actual_nearest_spot.as_deref(), Some("7.5"));
        assert_eq!(passes[0].distance_to_intended_spot_m, Some(1.25));
    }

    #[test]
    fn additive_migration_preserves_a_version_one_database() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "dcs-grpc-lso-migration-{}-{unique}.db",
            std::process::id()
        ));
        {
            let conn = Connection::open(&path).expect("create old database");
            conn.execute_batch(
                "CREATE TABLE passes (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    timestamp TEXT NOT NULL,
                    pilot_name TEXT NOT NULL,
                    pass_grade TEXT NOT NULL,
                    wire INTEGER,
                    dcs_grading TEXT
                );
                INSERT INTO passes(timestamp, pilot_name, pass_grade, wire)
                VALUES ('legacy', 'Legacy Pilot', 'OK', 3);",
            )
            .expect("create legacy schema");
        }

        let db = RecoveryDb::open(&path).expect("migrate legacy database");
        let passes = db.all_passes().expect("read migrated database");
        assert_eq!(passes.len(), 1);
        assert_eq!(passes[0].pilot_name, "Legacy Pilot");
        assert_eq!(passes[0].wire, Some(3));
        assert_eq!(passes[0].points_awarded, Some(true));
        assert_eq!(passes[0].intended_spot, None);
        assert_eq!(passes[0].actual_nearest_spot, None);
        drop(db);
        std::fs::remove_file(path).expect("remove isolated migration fixture");
    }
}
