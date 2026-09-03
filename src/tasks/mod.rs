use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use futures_util::Stream;
use tokio::sync::{broadcast, mpsc};
use tonic::transport::Channel;
use tonic::Status;

use crate::data::{AirplaneInfo, CarrierInfo};
use crate::db::SharedDb;
use crate::grading::{PassGrade, SpotGrade};
use crate::utils::shutdown::ShutdownHandle;

pub mod detect_recovery_attempt;
pub mod record_recovery;

/// One DCS mission event with its DCS timestamp, fanned out from the single
/// `MissionService.StreamEvents` subscription of the current generation.
pub type MissionEvent = (f64, stubs::mission::v0::stream_events_response::Event);
pub type EventSender = broadcast::Sender<Arc<MissionEvent>>;

/// Capacity of the shared event fan-out. Mission events are rare compared to
/// the 10 Hz telemetry loop; a lagging subscriber is reported, never blocked.
pub const EVENT_FANOUT_CAPACITY: usize = 256;

/// Adapts a broadcast subscription into the stream shape used by the
/// recovery loop. A lagged subscriber yields a `DATA_LOSS` status carrying the
/// number of skipped events; a closed sender ends the stream.
pub fn event_stream(
    rx: broadcast::Receiver<Arc<MissionEvent>>,
) -> impl Stream<Item = Result<Arc<MissionEvent>, Status>> {
    futures_util::stream::unfold(rx, |mut rx| async move {
        match rx.recv().await {
            Ok(event) => Some((Ok(event), rx)),
            Err(broadcast::error::RecvError::Lagged(skipped)) => Some((
                Err(Status::data_loss(format!(
                    "mission event fan-out lagged by {skipped} events"
                ))),
                rx,
            )),
            Err(broadcast::error::RecvError::Closed) => None,
        }
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PilotKind {
    Human,
    Ai,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HookSamplingMode {
    Independent,
    LegacyInline,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum RecoveryTelemetryMode {
    Auto,
    Legacy,
    Atomic,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AcquisitionMode {
    Legacy,
    Atomic,
}

impl AcquisitionMode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Legacy => "legacy",
            Self::Atomic => "atomic",
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct HookSamplingConfig {
    pub mode: HookSamplingMode,
    pub frequency_hz: u64,
    pub timeout: std::time::Duration,
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

/// A plane eligible for recovery monitoring, resolved once at discovery /
/// birth. Identity is tied to the occupied network slot, never to the
/// display name.
#[derive(Debug, Clone)]
pub struct PlaneCandidate {
    pub id: u32,
    pub name: String,
    pub pilot_name: String,
    pub plane_type: String,
    pub plane_info: &'static AirplaneInfo,
    pub pilot_kind: PilotKind,
    pub pilot_identity: String,
    pub pilot_ucid: Option<String>,
}

#[derive(Debug, Clone)]
pub struct CarrierCandidate {
    pub id: u32,
    pub name: String,
    pub carrier_type: String,
    pub carrier_info: &'static CarrierInfo,
}

/// Units known to the current generation, keyed by DCS unit id. Birth events
/// insert or replace entries; a `NOT_FOUND` transform removes them.
#[derive(Debug, Default)]
pub struct UnitRegistry {
    pub planes: HashMap<u32, PlaneCandidate>,
    pub carriers: HashMap<u32, CarrierCandidate>,
}

pub type SharedRegistry = Arc<Mutex<UnitRegistry>>;

/// Configuration and shared handles common to every recovery recorded in one
/// generation.
pub struct RecoveryContext {
    pub out_dir: PathBuf,
    pub discord_webhook: Option<String>,
    pub record_acmi: bool,
    pub hook_sampling: HookSamplingConfig,
    pub recovery_telemetry_mode: RecoveryTelemetryMode,
    pub recovery_snapshot_timeout: std::time::Duration,
    /// Poll `HookService.GetOwnshipHookState` as a diagnostic. Only useful on a
    /// client DCS instance with a local cockpit; always `unavailable` on a
    /// dedicated server.
    pub ownship_hook_diagnostics: bool,
    pub users: Arc<HashMap<String, u64>>,
    pub ch: Channel,
    pub shutdown: ShutdownHandle,
    pub session_log: SessionLog,
    pub db: SharedDb,
    pub session_id: i64,
    pub generation: u64,
    pub dcs_grpc_version: String,
    pub events: EventSender,
    /// Errors that must end the generation (e.g. forced atomic mode on a
    /// server without `GetRecoverySnapshot`).
    pub fatal: mpsc::Sender<crate::error::Error>,
}

#[derive(Clone)]
pub struct TaskParams<'a> {
    pub out_dir: &'a Path,
    pub discord_webhook: Option<String>,
    pub record_acmi: bool,
    pub hook_sampling: HookSamplingConfig,
    pub recovery_telemetry_mode: RecoveryTelemetryMode,
    pub recovery_snapshot_timeout: std::time::Duration,
    pub ownship_hook_diagnostics: bool,
    pub users: Arc<HashMap<String, u64>>,
    pub ch: Channel,
    pub events: EventSender,
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
    pub dcs_grpc_version: &'a str,
}

impl<'a> TaskParams<'a> {
    pub fn new(
        context: &'a RecoveryContext,
        plane: &'a PlaneCandidate,
        carrier: &'a CarrierCandidate,
    ) -> Self {
        Self {
            out_dir: &context.out_dir,
            discord_webhook: context.discord_webhook.clone(),
            record_acmi: context.record_acmi,
            hook_sampling: context.hook_sampling,
            recovery_telemetry_mode: context.recovery_telemetry_mode,
            recovery_snapshot_timeout: context.recovery_snapshot_timeout,
            ownship_hook_diagnostics: context.ownship_hook_diagnostics,
            users: context.users.clone(),
            ch: context.ch.clone(),
            events: context.events.clone(),
            carrier_id: carrier.id,
            carrier_name: &carrier.name,
            carrier_type: &carrier.carrier_type,
            plane_id: plane.id,
            plane_name: &plane.name,
            plane_type: &plane.plane_type,
            pilot_name: &plane.pilot_name,
            pilot_kind: plane.pilot_kind,
            pilot_identity: &plane.pilot_identity,
            pilot_ucid: plane.pilot_ucid.clone(),
            carrier_info: carrier.carrier_info,
            plane_info: plane.plane_info,
            shutdown: context.shutdown.clone(),
            session_log: context.session_log.clone(),
            db: context.db.clone(),
            session_id: context.session_id,
            generation: context.generation,
            dcs_grpc_version: &context.dcs_grpc_version,
        }
    }
}
