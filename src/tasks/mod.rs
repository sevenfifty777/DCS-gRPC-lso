use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

use tonic::transport::Channel;

use crate::data::{AirplaneInfo, CarrierInfo};
use crate::db::SharedDb;
use crate::grading::{PassGrade, SpotGrade};
use crate::utils::shutdown::ShutdownHandle;

pub mod detect_recovery_attempt;
pub mod record_recovery;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PilotKind {
    Human,
    Ai,
}

/// Record of a single completed recovery attempt, accumulated for the greenie board.
#[derive(Debug, Clone)]
pub struct CompletedPass {
    pub timestamp: String,
    pub pilot_name: String,
    pub pass_grade: PassGrade,
    pub grade_points: Option<f64>,
    pub wire: Option<u8>,
    /// V/STOL target spot label (currently fixed to 7.5); None for CATOBAR.
    pub spot: Option<String>,
    pub spot_grade: Option<SpotGrade>,
    pub spot_distance_m: Option<f64>,
    pub dcs_grading: Option<String>,
    pub aircraft_type: String,
    pub aircraft_id: Option<i64>,
    pub map_name: String,
    pub outcome: String,
    pub pilot_kind: PilotKind,
    pub carrier_name: String,
    pub carrier_type: String,
    pub recovery_mode: String,
    pub session_id: i64,
    pub generation: u64,
}

/// Shared log of all completed passes in this session.
pub type SessionLog = Arc<Mutex<Vec<CompletedPass>>>;

#[derive(Clone)]
pub struct TaskParams<'a> {
    pub out_dir: &'a Path,
    pub discord_webhook: Option<String>,
    pub record_acmi: bool,
    pub users: Arc<HashMap<String, u64>>,
    pub ch: Channel,
    pub carrier_id: u32,
    pub carrier_name: &'a str,
    pub carrier_type: &'a str,
    pub plane_id: u32,
    pub plane_name: &'a str,
    pub plane_type: &'a str,
    pub pilot_name: &'a str,
    pub pilot_kind: PilotKind,
    /// Stable private identity for the lifetime of this recovery task. For a
    /// human this is the UCID when available; for AI it is session/unit based.
    /// It must never be emitted to public reports or logs.
    pub pilot_identity: &'a str,
    /// Human UCID, persisted only in the private SQLite/API data path.
    pub pilot_ucid: Option<String>,
    pub carrier_info: &'static CarrierInfo,
    pub plane_info: &'static AirplaneInfo,
    pub shutdown: ShutdownHandle,
    pub session_log: SessionLog,
    pub db: SharedDb,
    pub session_id: i64,
    pub generation: u64,
}
