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

/// Record of a single completed recovery attempt, accumulated for the greenie board.
#[derive(Debug, Clone)]
pub struct CompletedPass {
    pub timestamp: String,
    pub pilot_name: String,
    pub pass_grade: PassGrade,
    pub grade_points: f64,
    pub wire: Option<u8>,
    /// V/STOL target spot label (currently fixed to 7.5); None for CATOBAR.
    pub spot: Option<String>,
    pub spot_grade: Option<SpotGrade>,
    pub spot_distance_m: Option<f64>,
    pub dcs_grading: Option<String>,
    pub aircraft_type: String,
    pub aircraft_id: Option<i64>,
    pub map_name: String,
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
    pub plane_id: u32,
    pub plane_name: &'a str,
    pub plane_type: &'a str,
    pub pilot_name: &'a str,
    pub carrier_info: &'static CarrierInfo,
    pub plane_info: &'static AirplaneInfo,
    pub shutdown: ShutdownHandle,
    pub session_log: SessionLog,
    pub db: SharedDb,
}
