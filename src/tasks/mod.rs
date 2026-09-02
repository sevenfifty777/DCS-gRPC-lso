use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

use tonic::transport::Channel;

use crate::data::{AirplaneInfo, CarrierInfo};
use crate::db::SharedDb;
use crate::grading::{PassGrade, SpotGrade};
use crate::utils::shutdown::ShutdownHandle;

pub mod detect_recovery_attempt;
pub mod event_correlator;
pub mod position_collector;
pub mod record_recovery;
pub mod report_pipeline;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PilotKind {
    Human,
    Ai,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HookSamplingMode {
    Disabled,
    Independent,
    LegacyInline,
}

#[derive(Debug, Clone, Copy)]
pub struct HookSamplingConfig {
    pub mode: HookSamplingMode,
    pub frequency_hz: u64,
    pub timeout: std::time::Duration,
}

#[derive(Debug, Clone, Default, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct BaselineManifest {
    pub dcs_build: Option<String>,
    pub mission: Option<String>,
    pub mission_sha256: Option<String>,
    pub dcs_grpc_dll_sha256: Option<String>,
    pub dcs_grpc_lua_sha256: Option<String>,
    #[serde(default)]
    pub module_versions: std::collections::BTreeMap<String, String>,
}

impl BaselineManifest {
    pub fn validate(&self) -> Result<(), String> {
        for (name, value) in [
            ("dcs_build", self.dcs_build.as_deref()),
            ("mission", self.mission.as_deref()),
        ] {
            if value.is_some_and(|value| value.trim().is_empty()) {
                return Err(format!(
                    "baseline manifest field `{name}` must not be empty"
                ));
            }
        }
        if self
            .module_versions
            .iter()
            .any(|(name, version)| name.trim().is_empty() || version.trim().is_empty())
        {
            return Err(
                "baseline manifest module names and versions must not be empty".to_string(),
            );
        }
        for (name, value) in [
            ("mission_sha256", self.mission_sha256.as_deref()),
            ("dcs_grpc_dll_sha256", self.dcs_grpc_dll_sha256.as_deref()),
            ("dcs_grpc_lua_sha256", self.dcs_grpc_lua_sha256.as_deref()),
        ] {
            if let Some(value) = value {
                if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
                    return Err(format!(
                        "baseline manifest field `{name}` must be a 64-character hexadecimal SHA-256"
                    ));
                }
            }
        }
        if self.dcs_build.is_none()
            && self.mission.is_none()
            && self.mission_sha256.is_none()
            && self.dcs_grpc_dll_sha256.is_none()
            && self.dcs_grpc_lua_sha256.is_none()
            && self.module_versions.is_empty()
        {
            return Err("baseline manifest must contain at least one provenance field".to_string());
        }
        Ok(())
    }
}

#[derive(Debug, Default)]
pub struct ActivePriorityPlanes(Mutex<HashMap<u32, u32>>);

impl ActivePriorityPlanes {
    pub fn activate(&self, plane_id: u32) {
        if let Ok(mut active) = self.0.lock() {
            *active.entry(plane_id).or_default() += 1;
        }
    }

    pub fn deactivate(&self, plane_id: u32) {
        if let Ok(mut active) = self.0.lock() {
            if let Some(count) = active.get_mut(&plane_id) {
                *count -= 1;
                if *count == 0 {
                    active.remove(&plane_id);
                }
            }
        }
    }

    pub fn contains(&self, plane_id: u32) -> bool {
        self.0
            .lock()
            .is_ok_and(|active| active.contains_key(&plane_id))
    }
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
    pub hook_sampling: HookSamplingConfig,
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
    pub db: Option<SharedDb>,
    pub session_id: i64,
    pub generation: u64,
    pub dcs_grpc_version: &'a str,
    pub dcs_grpc_compatibility: &'a str,
    pub positions_only: bool,
    pub suspend_detectors_during_recovery: bool,
    pub active_priority_planes: Arc<ActivePriorityPlanes>,
    pub baseline_manifest: Arc<BaselineManifest>,
}

#[cfg(test)]
mod tests {
    use super::{ActivePriorityPlanes, BaselineManifest};

    #[test]
    fn detector_suspension_is_scoped_to_one_aircraft_and_reference_counted() {
        let active = ActivePriorityPlanes::default();
        active.activate(10);
        active.activate(10);
        assert!(active.contains(10));
        assert!(!active.contains(20));
        active.deactivate(10);
        assert!(active.contains(10));
        active.deactivate(10);
        assert!(!active.contains(10));
    }

    #[test]
    fn baseline_manifest_rejects_empty_unknown_and_invalid_hash_values() {
        let empty: BaselineManifest = serde_json::from_str("{}").unwrap();
        assert!(empty.validate().is_err());
        assert!(serde_json::from_str::<BaselineManifest>(r#"{"misson":"typo"}"#).is_err());

        let invalid: BaselineManifest =
            serde_json::from_str(r#"{"mission_sha256":"replace-with-sha256"}"#).unwrap();
        assert!(invalid.validate().is_err());

        let valid: BaselineManifest = serde_json::from_str(&format!(
            r#"{{"mission":"cq.miz","mission_sha256":"{}"}}"#,
            "a".repeat(64)
        ))
        .unwrap();
        assert!(valid.validate().is_ok());
    }
}
