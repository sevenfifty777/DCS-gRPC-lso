use std::collections::VecDeque;
use std::ops::Neg;

use ultraviolet::{DRotor3, DVec3};

use crate::data::{AirplaneInfo, CarrierInfo, CarrierRecovery};
use crate::grading::{
    compute_pass_grade, compute_vstol_approach_grade_points, compute_vstol_final_grade_from_points,
    PassGrade, SpotGrade,
};
use crate::telemetry::{
    AlignmentMethod, TelemetryInvalidReason, TelemetrySample, MAX_EXTRAPOLATION_MS,
    SAMPLE_GAP_WARNING_MS,
};
use crate::transform::Transform;
use crate::utils::{m_to_ft, m_to_nm};

// ---------------------------------------------------------------------------
// LSO grading gates
//
// The three standard gates are fixed distances aft of the touchdown point at
// which the LSO samples glide-slope and lineup deviations.  They correspond
// to the ¾ nm, ½ nm, and ¼ nm "start of the ball" calls used in real-world
// NAVAIR 00-80T-104 grading.
//
// Each gate is recorded exactly once, on the first polling frame where the
// aircraft's angled-deck x-coordinate (distance along the deck angle axis,
// positive = behind the threshold) has decreased to or below that distance
// AND the aircraft is below 500 ft AGL (to exclude the overhead-pattern
// crossing of x = 0 at altitude, which would otherwise produce bogus ~90°
// deviation readings via atan2).
//
// | Constant                | nm   | meters | Purpose                          |
// |-------------------------|------|--------|----------------------------------|
// | GATE_THREE_QUARTER_NM   | ¾ nm | 1389 m | Early groove — first deviation   |
// |                         |      |        | sample; sets the tone for the    |
// |                         |      |        | whole pass.  Deviation here      |
// |                         |      |        | typically drives (H)/(L) notes.  |
// | GATE_HALF_NM            | ½ nm | 926 m  | Mid-groove — primary grading     |
// |                         |      |        | reference for OK/Fair/NoGrade.   |
// |                         |      |        | Correlates to the "in close"     |
// |                         |      |        | LSO observation window.          |
// | GATE_QUARTER_NM         | ¼ nm | 463 m  | At the ramp / "in close" — if   |
// |                         |      |        | GS deviation is ≤ GS_CUT_LOW_DEG|
// |                         |      |        | here, the pass is graded Cut.    |
//
// Deviation values stored per gate:
//   gs_deviation_deg  — glide-slope deviation in degrees (+ = high, − = low)
//   gs_deviation_ft   — same deviation expressed in feet (for chart labels)
//   lineup_deg        — lateral lineup deviation in degrees (+ = right of CL)
//   lineup_ft         — same deviation expressed in feet (for chart labels)
// ---------------------------------------------------------------------------

/// ¾ nm gate — first LSO grading sample (1 nm = 1 852 m → ¾ nm ≈ 1 389 m).
const GATE_THREE_QUARTER_NM: f64 = 1389.0;
/// ½ nm gate — primary grading reference.
const GATE_HALF_NM: f64 = 926.0;
/// ¼ nm gate — ramp / "in close"; dangerously low here triggers a Cut pass.
const GATE_QUARTER_NM: f64 = 463.0;

/// Exponential moving average (EMA) smoothing factor for the carrier position.
///
/// DCS updates the carrier's world position in discrete steps (~every 1.4 s at
/// 15 kts) rather than every simulation frame.  When polled at 100 ms, ~13 out
/// of 14 frames return the same stale position, then one frame jumps ahead by
/// ~10 m.  This creates a sawtooth in the approach datum `x` coordinate
/// (distance to landing point along the angled deck), which appears as periodic
/// stairstep drops of 10–20 ft on the side-view chart.
///
/// α = 0.15 spreads each position step over ~18 frames, introducing ~0.6 s of
/// positional lag (~4.6 m at 15 kts).  The resulting gate-distance error is
/// < 0.5 %, well within acceptable tolerance for LSO grading.
const CARRIER_POS_SMOOTH_ALPHA: f64 = 0.15;

const MAX_TRACK_SAMPLES: usize = 72_000;
const MAX_EVENT_EVIDENCE: usize = 256;
const MAX_HOOK_EVIDENCE: usize = 512;
const GATE_BUFFER_WINDOW_S: f64 = 2.0;
/// PROJECT-DERIVED limits validated against the September 2026 T-45/F-14
/// labelled recovery corpus. They deliberately require a complete cable-load
/// transient rather than interpreting a steady external draw-argument value.
const HOOK_DOWN_STABLE_MIN: f64 = 0.8;
const HOOK_DEFLECTED_MAX: f64 = 0.7;
const MIN_HOOK_DOWN_STABLE_S: f64 = 0.2;
const MAX_HOOK_DEFLECTION_RECOVERY_S: f64 = 8.0;
const MAX_HOOK_DEFLECTION_TOUCH_OFFSET_S: f64 = 2.0;
const MAX_HOOK_DEFLECTION_WIRE_LAG_MS: f64 = 200.0;
/// Reject an infinite-plane crossing when the hook is not physically near the
/// finite pendant. This prevents an early overhead/pattern crossing from
/// suppressing the real deck crossing later in the pass.
const MAX_WIRE_VERTICAL_SEPARATION_M: f64 = 3.0;
/// PROJECT-DERIVED provisional observation radius. It is informational only
/// until the Tarawa spot geometry is validated against the future live corpus.
const VSTOL_SPOT_OBSERVATION_RADIUS_M: f64 = 15.0;

// ---------------------------------------------------------------------------
// Commanded hook state (PROJECT-DERIVED, live corpus 2026-09-02/03).
// The external draw argument is the animated hook position. A real arrestment
// drives it from the down band into the up band starting up to ~1.4 s before
// `RunwayTouch` (T-45: 0.5-1.1 s, F-14: 1.3-1.4 s) and back 1.7-6.5 s later.
// The pilot-commanded state is therefore latched from the stable baseline
// observed in the groove *before* that excursion.
// ---------------------------------------------------------------------------
/// The baseline window ends this long before the earliest contact evidence.
const HOOK_BASELINE_GUARD_S: f64 = 1.5;
/// The baseline is the latest stable run of at most this length.
const HOOK_BASELINE_WINDOW_S: f64 = 3.0;
const HOOK_BASELINE_MIN_SAMPLES: usize = 5;
const HOOK_BASELINE_MIN_SPAN_S: f64 = 0.5;

// ---------------------------------------------------------------------------
// Arrest kinematics (PROJECT-DERIVED, campaign B 2026-09-02): a trapped
// aircraft's carrier-relative horizontal speed fell below ~5 m/s within 2 s of
// touchdown and stayed there, while bolters and hook-up touch-and-go passes
// left the deck at ~47-50 m/s. This confirms an arrest when DCS supplies no
// `WIRE#` (human LSO, DCS waveoff ignored); it never names a wire.
// ---------------------------------------------------------------------------
const ARREST_MAX_RELATIVE_SPEED_MPS: f64 = 6.0;
/// Hysteresis applied while checking that the aircraft stays arrested.
const ARREST_HOLD_MAX_RELATIVE_SPEED_MPS: f64 = 8.0;
/// The slow window must start within this time after the contact reference.
/// Live traps settle 4-5 s after touchdown once the cable pull-back ends.
const ARREST_DETECTION_WINDOW_S: f64 = 8.0;
const ARREST_HOLD_S: f64 = 2.0;
const ARREST_MAX_SAMPLE_GAP_MS: f64 = 300.0;
/// Deck run-out band along the angled deck, relative to the ideal touchdown
/// point (`x > 0` is short of it, `x < 0` is beyond it).
const ARREST_MIN_X_M: f64 = -160.0;
const ARREST_MAX_X_M: f64 = 60.0;
/// Deck kinematics are recorded once the aircraft is within this distance of
/// the ideal touchdown point while in the groove.
const DECK_KINEMATICS_START_X_M: f64 = 60.0;
const MAX_DECK_KINEMATIC_SAMPLES: usize = 600;
/// Relative speed is the carrier-relative displacement over at least this
/// window, which spans one DCS ship-position step.
const KINEMATIC_SPEED_WINDOW_S: f64 = 1.0;

#[derive(Debug, Clone, Default, PartialEq, serde::Serialize)]
pub struct Datum {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub observation_sequence: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_round_trip_ms: Option<f64>,
    /// Time the snapshot request waited in the DCS-gRPC mission queue.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub queue_wait_ms: Option<f64>,
    /// Time the snapshot spent inside the Lua callback.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lua_exec_ms: Option<f64>,
    /// Mission IPC queue depth when the snapshot request was enqueued.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub queue_depth: Option<u32>,
    /// Legacy display time; equal to corrected aircraft DCS time.
    pub time: f64,
    pub corrected_time_dcs: f64,
    pub x: f64,
    pub y: f64,
    pub aoa: f64,
    pub alt: f64,
    pub carrier_time: f64,
    pub plane_time: f64,
    pub carrier_received_unix_ms: u64,
    pub plane_received_unix_ms: u64,
    pub sample_gap_ms: f64,
    pub skew_ms: f64,
    pub alignment: AlignmentMethod,
    pub telemetry_valid: bool,
    pub raw_carrier_position: [f64; 3],
    pub corrected_carrier_position: [f64; 3],
    pub filtered_carrier_position: [f64; 3],
    /// Raw carrier velocity (m/s, world frame). Recorded so the carrier
    /// smoothing filter can be re-evaluated offline against dead reckoning.
    #[serde(default)]
    pub raw_carrier_velocity: [f64; 3],
}

/// Single frame of full-pattern position data recorded in the carrier BRC frame.
///
/// Origin is the carrier. Used to draw the overhead circuit chart (break → abeam →
/// ninety → final → touchdown).
#[derive(Debug, PartialEq, serde::Serialize)]
pub struct PatternDatum {
    pub time: f64,
    /// Distance astern of carrier along BRC (m). Positive = behind carrier (approach direction).
    pub astern_m: f64,
    /// Lateral distance from carrier BRC centerline (m). Positive = port (left) side.
    pub port_m: f64,
    /// Altitude MSL in feet.
    pub alt_ft: f64,
    /// Angle of Attack (degrees).
    pub aoa: f64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct EventEvidence {
    pub sequence: u32,
    pub kind: String,
    pub timestamp_dcs: f64,
    pub source: &'static str,
    pub confidence: &'static str,
    pub accepted: bool,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct SpotZoneObservation {
    pub intended_spot: &'static str,
    pub radius_m: f64,
    pub entered_at_dcs: Option<f64>,
    pub last_present_at_dcs: Option<f64>,
    pub exited_at_dcs: Option<f64>,
}

impl Default for SpotZoneObservation {
    fn default() -> Self {
        Self {
            intended_spot: "7.5",
            radius_m: VSTOL_SPOT_OBSERVATION_RADIUS_M,
            entered_at_dcs: None,
            last_present_at_dcs: None,
            exited_at_dcs: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HookSampleStatus {
    Success,
    Timeout,
    Error,
    Stale,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct HookSampleEvidence {
    pub associated_time_dcs: f64,
    pub observed_unix_ms: u64,
    pub age_ms: f64,
    pub raw: Option<f64>,
    pub status: HookSampleStatus,
    pub in_groove: bool,
    pub in_final_window: bool,
    pub before_touchdown: bool,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct WireCrossingEvidence {
    pub wire: u8,
    pub timestamp_dcs: f64,
    pub bracket_gap_ms: f64,
    pub method: &'static str,
}

#[derive(Debug, Clone, Default, PartialEq, serde::Serialize)]
pub struct WireEstimateEvidence {
    pub wire: Option<u8>,
    pub confidence: &'static str,
    pub reason: &'static str,
    pub hook_deflection_time_dcs: Option<f64>,
    pub hook_recovered_time_dcs: Option<f64>,
    pub correlation_lag_ms: Option<f64>,
    pub crossings: Vec<WireCrossingEvidence>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct CompletedHookDeflection {
    deflected_at_dcs: f64,
    recovered_at_dcs: f64,
}

/// Pilot-commanded arresting-hook position, latched from the pre-contact
/// baseline of the validated external draw argument.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HookState {
    Up,
    Down,
    #[default]
    Unknown,
}

impl HookState {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Up => "up",
            Self::Down => "down",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, serde::Serialize)]
pub struct HookObservation {
    pub samples_in_groove: u32,
    pub samples_in_final_window: u32,
    pub min_raw: Option<f64>,
    pub max_raw: Option<f64>,
    pub final_raw: Option<f64>,
    pub successful_samples: u32,
    pub timeout_samples: u32,
    pub error_samples: u32,
    pub stale_samples: u32,
    /// Number of oldest diagnostic samples compacted to keep the bounded
    /// timeline focused on the most recent recovery evidence.
    pub compacted_samples: u32,
    /// Legacy string form of `baseline_state`.
    pub interpreted_state: &'static str,
    /// Commanded hook state latched from the pre-contact baseline.
    pub baseline_state: HookState,
    /// Mean raw value over the baseline window.
    pub baseline_value: Option<f64>,
    pub baseline_samples: u32,
    pub baseline_start_dcs: Option<f64>,
    pub baseline_end_dcs: Option<f64>,
    /// Why the baseline could not be latched (empty when it was).
    pub baseline_reason: &'static str,
    pub timeline: VecDeque<HookSampleEvidence>,
    /// Calibration is module-specific; unknown modules are never inferred.
    pub polarity: &'static str,
}

/// Carrier-relative post-contact kinematics used to confirm an arrest without
/// a DCS wire. Never identifies the wire.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize)]
pub struct ArrestKinematicsEvidence {
    pub confirmed: bool,
    pub reason: &'static str,
    /// Touchdown event time, or the deck-threshold crossing when no event exists.
    pub reference_time_dcs: Option<f64>,
    pub slow_since_dcs: Option<f64>,
    pub held_s: Option<f64>,
    pub min_relative_speed_mps: Option<f64>,
    pub x_at_slow_m: Option<f64>,
    pub samples: u32,
}

#[derive(Debug, Clone, Copy)]
struct DeckKinematicSample {
    time: f64,
    /// Aircraft position relative to the *raw* carrier position (world frame).
    /// DCS steps ship positions every ~1.4 s, so instantaneous velocities and
    /// smoothed positions both produce spikes; a displacement over
    /// `KINEMATIC_SPEED_WINDOW_S` is stable for an aircraft carried by the deck.
    relative_position: DVec3,
    x: f64,
}

#[derive(Debug, Clone, Copy)]
struct WindowedDeckSample {
    time: f64,
    relative_speed_mps: f64,
    x: f64,
}

/// Parsed DCS `LandingQualityMark` comment.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize)]
pub struct DcsLsoGrade {
    /// Grade token following `GRADE:` (`---`, `C`, `WO`, `(OK)`, `_OK_`, ...).
    pub grade: Option<String>,
    pub wire: Option<u8>,
    /// The DCS LSO called a waveoff (`GRADE:WO`, `WO(AFU)IC`, `WOFDIC`, ...).
    pub waveoff_ordered: bool,
}

pub struct Track {
    pilot_name: String,
    previous_distance: f64,
    previous_x: f64,
    previous_sample_time: Option<f64>,
    gate_samples: VecDeque<ApproachSample>,
    datums: Vec<Datum>,
    pattern_datums: Vec<PatternDatum>,
    gate_deviations: GateDeviations,
    /// Set to `true` once the aircraft enters inside 3/4 nm and below 300 ft AGL.
    entered_groove: bool,
    /// DCS simulation time (seconds since scenario start) when groove entry was first detected.
    groove_entry_time: Option<f64>,
    /// DCS simulation time (seconds since scenario start) when touchdown was recorded.
    landing_time: Option<f64>,
    grading: Option<Grading>,
    dcs_grading: Option<String>,
    /// Horizontal deck-plane distance (m) between the AV-8B pilot-ground
    /// landing reference and the calibrated Tarawa spot 7.5 at the exact land event.
    spot_distance_m: Option<f64>,
    /// Nearest entry in the active geometric spot catalog. This is independent from the
    /// phase-1 intended spot and does not select or alter scoring policy.
    actual_nearest_spot: Option<&'static str>,
    carrier_info: &'static CarrierInfo,
    plane_info: &'static AirplaneInfo,
    /// Carrier and aircraft transforms at the closest point of an arrested approach.
    min_distance_state: Option<(Transform, Transform)>,
    /// Exponentially smoothed carrier position used for approach geometry.
    /// Eliminates the sawtooth caused by DCS updating the carrier's world
    /// position in discrete steps rather than every frame.
    smoothed_carrier_pos: Option<DVec3>,
    hook_observation: HookObservation,
    crossed_deck_threshold: bool,
    telemetry_quality: TelemetryQuality,
    events: Vec<EventEvidence>,
    spot_zone: SpotZoneObservation,
    touchdown_horizontal_speed_mps: Option<f64>,
    health_red_announced: bool,
    previous_wire_plane: [Option<(f64, f64)>; 4],
    wire_crossings: Vec<WireCrossingEvidence>,
    /// Along-deck distance of the most recent sample, updated even after a
    /// graded outcome (unlike `previous_x`, which drives the gate logic).
    last_x: f64,
    /// First inbound crossing of the ideal touchdown point while in the groove.
    deck_crossing_time: Option<f64>,
    deck_kinematics: Vec<DeckKinematicSample>,
    dcs_lso: Option<DcsLsoGrade>,
}

/// GS and lineup deviation recorded at a key gate distance.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct GateDatum {
    /// Glide slope deviation from ideal glide path in degrees
    /// (positive = high, negative = low).
    pub gs_deviation_deg: f64,
    /// Lateral lineup deviation from the angled-deck centerline in degrees
    /// (positive = right of centerline / lined-up-left, negative = left).
    pub lineup_deg: f64,
    /// Glide slope deviation in feet — kept for the PNG chart display label.
    pub gs_deviation_ft: f64,
    /// Lineup deviation in feet — kept for the PNG chart display label.
    pub lineup_ft: f64,
    pub timestamp_dcs: f64,
    pub distance_m: f64,
    pub sample_gap_ms: f64,
    pub skew_ms: f64,
    pub method: GateCaptureMethod,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GateCaptureMethod {
    Measured,
    Interpolated,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GateStatus {
    Valid,
    Late,
    Missing,
    Invalid,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct GateQuality {
    pub status: GateStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

impl Default for GateQuality {
    fn default() -> Self {
        Self {
            status: GateStatus::Missing,
            reason: Some("not_observed".to_string()),
        }
    }
}

/// Deviation scores sampled at the standard LSO grading gates.
#[derive(Debug, Default, PartialEq, serde::Serialize)]
pub struct GateDeviations {
    pub at_three_quarter_nm: Option<GateDatum>,
    pub at_half_nm: Option<GateDatum>,
    pub at_quarter_nm: Option<GateDatum>,
    pub three_quarter_quality: GateQuality,
    pub half_quality: GateQuality,
    pub quarter_quality: GateQuality,
}

impl GateDeviations {
    pub fn all_valid(&self) -> bool {
        let ordered = self
            .at_three_quarter_nm
            .as_ref()
            .zip(self.at_half_nm.as_ref())
            .zip(self.at_quarter_nm.as_ref())
            .is_some_and(|((three_quarter, half), quarter)| {
                three_quarter.timestamp_dcs < half.timestamp_dcs
                    && half.timestamp_dcs < quarter.timestamp_dcs
            });
        ordered
            && self.three_quarter_quality.status == GateStatus::Valid
            && self.half_quality.status == GateStatus::Valid
            && self.quarter_quality.status == GateStatus::Valid
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Completeness {
    Complete,
    InsufficientGates,
    TelemetryGap,
    InvalidTelemetry,
    UnconfirmedArrest,
    BufferLimit,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TelemetryHealth {
    #[default]
    Green,
    Orange,
    Red,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct TelemetryQuality {
    pub completeness: Completeness,
    pub health: TelemetryHealth,
    pub health_reason: &'static str,
    pub max_sample_gap_ms: f64,
    pub max_skew_ms: f64,
    pub warning_samples: u32,
    pub invalid_samples: u32,
    pub pattern_invalid_samples: u32,
    pub scoring_invalid_samples: u32,
    pub max_scoring_sample_gap_ms: f64,
    pub dropped_samples: u32,
    pub reasons: Vec<TelemetryInvalidReason>,
}

impl Default for TelemetryQuality {
    fn default() -> Self {
        Self {
            completeness: Completeness::Complete,
            health: TelemetryHealth::Green,
            health_reason: "nominal",
            max_sample_gap_ms: 0.0,
            max_skew_ms: 0.0,
            warning_samples: 0,
            invalid_samples: 0,
            pattern_invalid_samples: 0,
            scoring_invalid_samples: 0,
            max_scoring_sample_gap_ms: 0.0,
            dropped_samples: 0,
            reasons: Vec::new(),
        }
    }
}

#[derive(Debug, Clone)]
struct ApproachSample {
    time: f64,
    x: f64,
    y: f64,
    alt: f64,
    valid: bool,
    in_approach: bool,
    lined_up: bool,
    skew_ms: f64,
}

#[derive(Debug, Default, PartialEq, Eq, serde::Serialize)]
pub enum Grading {
    #[default]
    Unknown,
    Bolter,
    TouchAndGo {
        cable_estimated: Option<u8>,
    },
    /// Pilot broke off the approach after entering the groove (inside 3/4 nm, below 300 ft).
    WaveoffUnknown,
    /// The DCS LSO ordered a waveoff and the aircraft never touched the deck.
    WaveoffDcs,
    Recovered {
        cable: Option<u8>,
        cable_estimated: Option<u8>,
    },
}

impl Grading {
    /// Whether the aircraft made deck contact under this outcome.
    pub fn touched_deck(&self) -> bool {
        matches!(
            self,
            Self::Bolter | Self::TouchAndGo { .. } | Self::Recovered { .. }
        )
    }
}

/// Select the wire shown by presentation and legacy summary consumers while
/// keeping the independently persisted estimated and DCS values unchanged.
/// DCS is authoritative when it reports a wire. The independent estimate is a
/// fallback for recoveries flown without the DCS radio/AI LSO workflow.
pub(crate) fn select_wire_for_display(
    wire_estimated: Option<u8>,
    wire_dcs: Option<u8>,
) -> (Option<u8>, &'static str) {
    if let Some(wire) = wire_dcs {
        (Some(wire), "dcs")
    } else if let Some(wire) = wire_estimated {
        (Some(wire), "estimated")
    } else {
        (None, "unavailable")
    }
}

#[derive(Debug, PartialEq)]
pub struct TrackResult {
    pub pilot_name: String,
    pub grading: Grading,
    /// Gate-only approach grade before any V/STOL touchdown bonus.
    pub approach_grade: PassGrade,
    /// Final display grade. For CATOBAR this is identical to approach_grade;
    /// for V/STOL it includes the spot-7.5 bonus.
    pub pass_grade: PassGrade,
    /// Final numeric score. Kept separately because V/STOL bonuses can produce
    /// quarter-point values (e.g. 4.75) while reusing the CATOBAR labels.
    pub grade_points: Option<f64>,
    pub spot_grade: Option<SpotGrade>,
    pub spot_distance_m: Option<f64>,
    pub intended_spot: Option<&'static str>,
    pub actual_nearest_spot: Option<&'static str>,
    pub dcs_grading: Option<String>,
    pub gate_deviations: GateDeviations,
    pub datums: Vec<Datum>,
    pub pattern_datums: Vec<PatternDatum>,
    pub plane_info: &'static AirplaneInfo,
    pub carrier_info: &'static CarrierInfo,
    /// Time from groove entry to touchdown in seconds, if both were recorded.
    pub groove_time_secs: Option<f64>,
    pub touchdown_time_dcs: Option<f64>,
    pub telemetry_quality: TelemetryQuality,
    pub events: Vec<EventEvidence>,
    pub spot_zone: SpotZoneObservation,
    /// Raw horizontal speed at the first accepted touchdown evidence. No
    /// VL/RVL threshold is applied before the live corpus is validated.
    pub touchdown_horizontal_speed_mps: Option<f64>,
    pub hook_observation: HookObservation,
    pub wire_estimation: WireEstimateEvidence,
    /// Commanded hook state used for the bolter / touch-and-go decision.
    pub hook_state: HookState,
    pub arrest_kinematics: ArrestKinematicsEvidence,
    /// What proved the arrest: `dcs_wire`, `hook_transient`, `kinematic`, or
    /// `none` for non-arrested outcomes / unconfirmed contact.
    pub arrest_evidence: &'static str,
    pub dcs_lso: Option<DcsLsoGrade>,
    #[cfg(test)]
    pub deck_speed_series: Vec<(f64, f64, f64)>,
}

impl Track {
    pub fn new(
        pilot_name: impl Into<String>,
        carrier_info: &'static CarrierInfo,
        plane_info: &'static AirplaneInfo,
    ) -> Self {
        Self {
            pilot_name: pilot_name.into(),
            previous_distance: f64::MAX,
            previous_x: f64::MAX,
            previous_sample_time: None,
            gate_samples: VecDeque::new(),
            // A typical pass records 1-2k samples; avoid repeated regrowth.
            datums: Vec::with_capacity(2_048),
            pattern_datums: Vec::with_capacity(2_048),
            gate_deviations: GateDeviations::default(),
            entered_groove: false,
            groove_entry_time: None,
            landing_time: None,
            grading: None,
            dcs_grading: None,
            spot_distance_m: None,
            actual_nearest_spot: None,
            carrier_info,
            plane_info,
            min_distance_state: None,
            smoothed_carrier_pos: None,
            hook_observation: HookObservation {
                polarity: plane_info
                    .hook_argument
                    .map_or("no_validated_hook_argument", |argument| argument.polarity),
                interpreted_state: "unknown",
                baseline_reason: "not_evaluated",
                ..HookObservation::default()
            },
            crossed_deck_threshold: false,
            telemetry_quality: TelemetryQuality::default(),
            events: Vec::new(),
            spot_zone: SpotZoneObservation::default(),
            touchdown_horizontal_speed_mps: None,
            health_red_announced: false,
            previous_wire_plane: [None; 4],
            wire_crossings: Vec::new(),
            last_x: f64::MAX,
            deck_crossing_time: None,
            deck_kinematics: Vec::new(),
            dcs_lso: None,
        }
    }

    pub fn next(
        &mut self,
        carrier: &Transform,
        plane: &Transform,
        hook_state: Option<f64>,
    ) -> bool {
        let sample =
            TelemetrySample::from_replay(carrier.clone(), plane.clone(), self.previous_sample_time);
        self.next_sample(&sample, hook_state)
    }

    pub fn next_sample(&mut self, sample: &TelemetrySample, hook_state: Option<f64>) -> bool {
        let carrier = &sample.carrier;
        let plane = &sample.plane;
        self.previous_sample_time = Some(carrier.time.max(plane.time));
        self.telemetry_quality.max_sample_gap_ms = self
            .telemetry_quality
            .max_sample_gap_ms
            .max(sample.sample_gap_ms.max(sample.source_age_ms));
        self.telemetry_quality.max_skew_ms = self.telemetry_quality.max_skew_ms.max(sample.skew_ms);
        if sample.has_warning() {
            self.telemetry_quality.warning_samples += 1;
        }
        let (health, health_reason) = if sample.invalid_reason.is_some()
            || sample.sample_gap_ms > crate::telemetry::SAMPLE_GAP_INCOMPLETE_MS
            || sample.source_age_ms > crate::telemetry::SAMPLE_GAP_INCOMPLETE_MS
        {
            (TelemetryHealth::Red, "invalid_or_incomplete_sample")
        } else if sample.has_warning() {
            (TelemetryHealth::Orange, "degraded_cadence_or_freshness")
        } else {
            (TelemetryHealth::Green, "nominal")
        };
        self.telemetry_quality.health = health;
        self.telemetry_quality.health_reason = health_reason;
        if health == TelemetryHealth::Red && !self.health_red_announced {
            tracing::warn!(
                before_groove = !self.entered_groove,
                health_reason,
                "live grading health is red"
            );
            self.health_red_announced = true;
        }
        if let Some(reason) = sample.invalid_reason {
            self.telemetry_quality.invalid_samples += 1;
            if !self.telemetry_quality.reasons.contains(&reason) {
                self.telemetry_quality.reasons.push(reason);
            }
        }

        // ---------------------------------------------------------------
        // Pattern datum — BRC frame, recorded every frame.
        // Origin = carrier position. x_chart = -port_m, y_chart = -astern_m
        // so the circuit appears with port on the left and the carrier at the
        // top of the overview PNG.
        // ---------------------------------------------------------------
        {
            let brc_rot = DRotor3::from_rotation_xz(carrier.heading.neg().to_radians());
            let brc_fwd = DVec3::unit_z().rotated_by(brc_rot); // BRC forward
            let brc_stbd = DVec3::unit_x().rotated_by(brc_rot); // starboard

            // rel = vector from plane to carrier
            let rel = carrier.position - plane.position;
            // astern_m > 0 when plane is behind carrier (normal approach direction)
            let astern_m = rel.dot(brc_fwd);
            // port_m > 0 when plane is on the port (left) side of the carrier
            // (rel points toward the carrier; when the plane is to port, rel
            // points toward the starboard side → positive dot with brc_stbd)
            let port_m = rel.dot(brc_stbd);

            if self.pattern_datums.len() < MAX_TRACK_SAMPLES {
                self.pattern_datums.push(PatternDatum {
                    time: plane.time,
                    astern_m,
                    port_m,
                    alt_ft: m_to_ft(plane.alt),
                    aoa: plane.aoa,
                });
            } else {
                self.telemetry_quality.dropped_samples += 1;
                self.telemetry_quality.completeness = Completeness::BufferLimit;
            }
        }

        // Smooth carrier position to eliminate DCS quantisation sawtooth.
        // The carrier's world position updates in discrete jumps (~every 1.4 s);
        // between updates, the same stale position is returned.  EMA blends the
        // raw position toward the smoothed estimate each frame, producing a
        // steady progression instead of a stairstep.
        let smoothed_pos = match self.smoothed_carrier_pos {
            Some(prev) => {
                let s = prev + (carrier.position - prev) * CARRIER_POS_SMOOTH_ALPHA;
                self.smoothed_carrier_pos = Some(s);
                s
            }
            None => {
                self.smoothed_carrier_pos = Some(carrier.position);
                carrier.position
            }
        };

        let landing_pos_offset = self
            .carrier_info
            .approach_reference_offset(self.plane_info)
            .rotated_by(carrier.rotation);
        let landing_pos = smoothed_pos + landing_pos_offset;

        // Horizontal V/STOL lineup is an aircraft-centerline measurement.
        // The ideal axis itself is already positioned one AV-8B wingspan outside
        // the Tarawa port deck edge by approach_reference_offset().  Do not add
        // the touchdown reference here: the pilot-ground contact projection is
        // retained for the later hover/touchdown phase, not for parallel-approach lineup.
        let ray_from_plane_to_carrier = DVec3::new(
            landing_pos.x - plane.position.x,
            0.0, // ignore altitude
            landing_pos.z - plane.position.z,
        );

        // Stop once the plane leaves the wide pattern detection zone (RTB or go-around).
        // This prevents a recording from running forever when no landing is made.
        let carrier_distance = (smoothed_pos - plane.position).mag();
        if m_to_nm(carrier_distance) > 3.5 || m_to_ft(plane.alt) > 1100.0 {
            tracing::debug!("stop: plane exited pattern detection zone");
            return false;
        }

        let distance = ray_from_plane_to_carrier.mag();

        // Construct the x axis, which is aligned to the angled deck. This is
        // computed before any outcome logic so that hook samples, deck
        // kinematics and post-outcome evidence all see the current position.
        let fb_rot = DRotor3::from_rotation_xz(
            (carrier.heading - self.carrier_info.deck_angle)
                .neg()
                .to_radians(),
        );
        let fb = DVec3::unit_z().rotated_by(fb_rot);

        let x = ray_from_plane_to_carrier.dot(fb);
        let mut y = (distance.powi(2) - x.powi(2)).max(0.0).sqrt();

        // Determine whether plane is left or right of the glide slope.
        let a = DVec3::unit_x().rotated_by(fb_rot);
        if ray_from_plane_to_carrier.dot(a) > 0.0 {
            y = y.neg();
        }
        let previous_last_x = self.last_x;
        self.last_x = x;

        if sample.is_valid() {
            self.observe_vstol_spot_zone(carrier, plane);
        }
        let is_arrested_recovery = matches!(&self.carrier_info.recovery, CarrierRecovery::Arrested);
        if is_arrested_recovery {
            if let Some(raw) = hook_state.filter(|raw| raw.is_finite()) {
                self.observe_hook_sample(
                    plane.time,
                    sample.plane_received_unix_ms,
                    0.0,
                    Some(raw),
                    HookSampleStatus::Success,
                );
            }
            if sample.is_valid() {
                self.observe_wire_crossings(carrier, plane, sample.sample_gap_ms);
            }
        }

        // First inbound crossing of the ideal touchdown point. Evaluated before
        // any early return so it also works when the touchdown event arrived
        // while the hook was still short of the reference point.
        if sample.is_valid() && previous_last_x > 0.0 && x <= 0.0 {
            self.crossed_deck_threshold = true;
            if self.entered_groove {
                self.deck_crossing_time.get_or_insert(plane.time);
            }
        }

        // Carrier-relative deck kinematics for arrest confirmation.
        if is_arrested_recovery
            && sample.is_valid()
            && self.entered_groove
            && x <= DECK_KINEMATICS_START_X_M
        {
            let relative_position = plane.position - sample.carrier_raw.position;
            if relative_position.x.is_finite()
                && relative_position.z.is_finite()
                && self.deck_kinematics.len() < MAX_DECK_KINEMATIC_SAMPLES
            {
                self.deck_kinematics.push(DeckKinematicSample {
                    time: plane.time,
                    relative_position,
                    x,
                });
            }
        }

        // Track the minimum distance to the touchdown point.
        if distance < self.previous_distance {
            self.previous_distance = distance;
            if is_arrested_recovery {
                self.min_distance_state = Some((carrier.clone(), plane.clone()));
            }
        } else if distance - self.previous_distance > 150.0 {
            match &self.grading {
                Some(Grading::Recovered { .. }) => {
                    if self.carrier_info.is_vstol() {
                        tracing::debug!(
                            distance_in_m = distance,
                            "V/STOL contact followed by departure"
                        );
                        self.grading = Some(Grading::TouchAndGo {
                            cable_estimated: None,
                        });
                        return false;
                    }
                    // A kinematically confirmed arrest cannot become a bolter:
                    // the aircraft is simply taxiing / moving with the deck.
                    if self.evaluate_arrest_kinematics().confirmed {
                        tracing::debug!(
                            distance_in_m = distance,
                            "arrested aircraft moving with the deck; stop tracking"
                        );
                        return false;
                    }
                    if self.commanded_hook_state() == HookState::Up {
                        let cable_estimated = match self.grading.as_ref() {
                            Some(Grading::Recovered {
                                cable_estimated, ..
                            }) => *cable_estimated,
                            _ => None,
                        };
                        tracing::debug!("qualification touch-and-go detected");
                        self.grading = Some(Grading::TouchAndGo { cable_estimated });
                        return false;
                    }
                    // Landed and now moving away → normal bolter.
                    tracing::debug!(distance_in_m = distance, "bolter detected");
                    self.grading = Some(Grading::Bolter);
                    return false;
                }
                Some(_) => {
                    // Waveoff or other graded outcome, plane moving away → stop.
                    tracing::debug!(
                        distance_in_m = distance,
                        "stop tracking (graded, moving away)"
                    );
                    return false;
                }
                None if self.entered_groove => {
                    // A deck crossing without an arrest is a bolter, unless the
                    // commanded hook state proves a qualification touch-and-go.
                    if self.crossed_deck_threshold && self.min_distance_state.is_some() {
                        if self.commanded_hook_state() == HookState::Up {
                            let cable_estimated = self.wire_estimate_at(plane.time).wire;
                            tracing::debug!("qualification touch-and-go detected");
                            self.grading = Some(Grading::TouchAndGo { cable_estimated });
                            return false;
                        }
                        tracing::debug!(
                            distance_in_m = distance,
                            "bolter detected (deck crossing, no arrest)"
                        );
                        self.grading = Some(Grading::Bolter);
                        return false;
                    }

                    tracing::debug!(
                        distance_in_m = distance,
                        "waveoff detected (initiator unknown)"
                    );
                    self.grading = Some(Grading::WaveoffUnknown);
                    return false;
                }
                None => {
                    // No graded outcome yet and plane not in groove → still flying the overhead
                    // pattern (break turn, downwind, abeam).  Reset the distance floor so the
                    // next approaching leg is tracked from a fresh minimum instead of stopping.
                    self.previous_distance = distance;
                    tracing::trace!(
                        distance_in_m = distance,
                        "pattern: plane moving away, resetting distance tracker"
                    );
                }
            }
        }

        // Arrest confirmed purely from deck kinematics (no Land/RunwayTouch
        // event, e.g. missing DCS events). Keep tracking long enough for the
        // hook transient to recover, then stop.
        if self.grading.is_none()
            && is_arrested_recovery
            && self.entered_groove
            && self.crossed_deck_threshold
        {
            let kinematics = self.evaluate_arrest_kinematics();
            if kinematics.confirmed {
                tracing::debug!(?kinematics, "arrest confirmed from deck kinematics");
                self.grading = Some(Grading::Recovered {
                    cable: None,
                    cable_estimated: None,
                });
                return true;
            }
        }
        if let Some(Grading::Recovered { .. }) = &self.grading {
            if self.landing_time.is_none() {
                let kinematics = self.evaluate_arrest_kinematics();
                if kinematics.confirmed
                    && kinematics
                        .held_s
                        .is_some_and(|held| held >= MAX_HOOK_DEFLECTION_RECOVERY_S)
                {
                    tracing::debug!("kinematic arrest held; stop tracking");
                    return false;
                }
            }
        }

        // Already landed, no need to actually record any more datums, but keep going to detect
        // bolters.
        if self.grading.is_some() {
            return true;
        }

        let alt = match &self.carrier_info.recovery {
            CarrierRecovery::Arrested => {
                let hook_offset = self.plane_info.hook.rotated_by(plane.rotation);
                plane.alt - self.carrier_info.deck_altitude + hook_offset.y
            }
            CarrierRecovery::Vstol { .. } => {
                // V/STOL V1 vertical chart is referenced to the water/sea level,
                // matching the Harrier's 120 ft hover/approach altitude.
                // DCS plane.alt is MSL, so keep it directly instead of subtracting
                // the Tarawa deck height (which would shift the curve ~66 ft low).
                plane.alt
            }
        };

        let scoring_relevant =
            self.entered_groove || (x > 0.0 && x <= GATE_THREE_QUARTER_NM && m_to_ft(alt) <= 500.0);
        if scoring_relevant {
            self.telemetry_quality.max_scoring_sample_gap_ms = self
                .telemetry_quality
                .max_scoring_sample_gap_ms
                .max(sample.sample_gap_ms.max(sample.source_age_ms));
        }
        if let Some(reason) = sample.invalid_reason {
            if scoring_relevant {
                self.telemetry_quality.scoring_invalid_samples += 1;
                self.telemetry_quality.completeness = match reason {
                    TelemetryInvalidReason::TelemetryGap => Completeness::TelemetryGap,
                    _ if self.telemetry_quality.completeness != Completeness::TelemetryGap => {
                        Completeness::InvalidTelemetry
                    }
                    _ => self.telemetry_quality.completeness,
                };
            } else {
                self.telemetry_quality.pattern_invalid_samples += 1;
            }
        }

        // Gate sampling and groove entry only apply when the aircraft is on the approach side of
        // the threshold (x > 0).  When x ≤ 0 the aircraft is ahead of the touchdown point
        // (e.g., still in the break or flying the overhead pattern), and atan2 with a negative x
        // would produce a bogus ~177° deviation reading.
        if x > 0.0 {
            // Robust reset: if the aircraft flies outbound (e.g., into the pattern after a bolter),
            // clear any gates or groove entry that were captured so they can be freshly recorded
            // on the real final approach inbound. Wire-crossing and deck evidence from the
            // previous approach is discarded with it so the next trap can be attributed.
            if x > GATE_THREE_QUARTER_NM {
                self.gate_deviations.at_three_quarter_nm = None;
                self.gate_deviations.three_quarter_quality = GateQuality::default();
                self.groove_entry_time = None;
                self.entered_groove = false;
                self.crossed_deck_threshold = false;
                self.deck_crossing_time = None;
                self.deck_kinematics.clear();
                self.wire_crossings.clear();
                self.previous_wire_plane = [None; 4];
            }
            if x > GATE_HALF_NM {
                self.gate_deviations.at_half_nm = None;
                self.gate_deviations.half_quality = GateQuality::default();
            }
            if x > GATE_QUARTER_NM {
                self.gate_deviations.at_quarter_nm = None;
                self.gate_deviations.quarter_quality = GateQuality::default();
            }

            // Only sample gates if the aircraft is flying inbound (x is decreasing).
            // This prevents capturing bogus ~90° deviations if the aircraft crosses the beam
            // outbound (from front to back) during a tight low-altitude bolter pattern.
            let is_inbound = x < self.previous_x;

            let ideal_base_alt = match &self.carrier_info.recovery {
                CarrierRecovery::Arrested => 0.0,
                CarrierRecovery::Vstol {
                    target_altitude_ft, ..
                } => *target_altitude_ft / 3.28084,
            };
            let lineup_deg = y.atan2(x).to_degrees();
            // Gate altitude guard: on-glidepath at ¾ nm is ~278 ft; even a GS+3° deviation is
            // ~400 ft at that distance.  500 ft cleanly rejects the 600–1000 ft overhead-pattern
            // crossing of x = 0 while still capturing all realistic final-approach deviations.
            let in_approach = m_to_ft(alt) <= 500.0;
            // For V/STOL, do not capture a distance gate while the Harrier is
            // still on base/turning toward the parallel axis.  This avoids
            // bogus multi-thousand-foot LAT values from an earlier circuit pass.
            let gate_lined_up = !self.carrier_info.is_vstol() || lineup_deg.abs() <= 10.0;
            let current = ApproachSample {
                time: plane.time,
                x,
                y,
                alt,
                valid: sample.is_valid() && is_inbound,
                in_approach,
                lined_up: gate_lined_up,
                skew_ms: sample.skew_ms,
            };

            if self.gate_samples.is_empty() {
                mark_started_inside(
                    x,
                    GATE_THREE_QUARTER_NM,
                    &mut self.gate_deviations.three_quarter_quality,
                );
                mark_started_inside(x, GATE_HALF_NM, &mut self.gate_deviations.half_quality);
                mark_started_inside(
                    x,
                    GATE_QUARTER_NM,
                    &mut self.gate_deviations.quarter_quality,
                );
            } else {
                capture_gate_from_window(
                    &self.gate_samples,
                    &current,
                    GATE_THREE_QUARTER_NM,
                    ideal_base_alt,
                    self.plane_info.glide_slope,
                    &mut self.gate_deviations.at_three_quarter_nm,
                    &mut self.gate_deviations.three_quarter_quality,
                );
                capture_gate_from_window(
                    &self.gate_samples,
                    &current,
                    GATE_HALF_NM,
                    ideal_base_alt,
                    self.plane_info.glide_slope,
                    &mut self.gate_deviations.at_half_nm,
                    &mut self.gate_deviations.half_quality,
                );
                capture_gate_from_window(
                    &self.gate_samples,
                    &current,
                    GATE_QUARTER_NM,
                    ideal_base_alt,
                    self.plane_info.glide_slope,
                    &mut self.gate_deviations.at_quarter_nm,
                    &mut self.gate_deviations.quarter_quality,
                );
            }
            self.gate_samples.push_back(current);
            while self
                .gate_samples
                .front()
                .is_some_and(|sample| plane.time - sample.time > GATE_BUFFER_WINDOW_S)
            {
                self.gate_samples.pop_front();
            }

            // Mark groove entry: inside 3/4 nm, below 300 ft AGL, and lined up (±10°).
            // The lateral constraint prevents the timer from starting prematurely while the
            // aircraft is still performing a wide turn to final on the base leg.
            if x <= GATE_THREE_QUARTER_NM && m_to_ft(alt) <= 300.0 && lineup_deg.abs() <= 10.0 {
                if self.groove_entry_time.is_none() {
                    self.groove_entry_time = Some(plane.time);
                }
                self.entered_groove = true;
            }
        }

        if self.datums.len() < MAX_TRACK_SAMPLES {
            self.datums.push(Datum {
                observation_sequence: sample.observation_sequence,
                request_round_trip_ms: sample.request_round_trip_ms,
                queue_wait_ms: sample.queue_wait_ms,
                lua_exec_ms: sample.lua_exec_ms,
                queue_depth: sample.queue_depth,
                time: plane.time,
                corrected_time_dcs: plane.time.max(carrier.time),
                x,
                y,
                aoa: plane.aoa,
                alt: alt.max(0.0),
                carrier_time: sample.carrier_raw.time,
                plane_time: sample.plane_raw.time,
                carrier_received_unix_ms: sample.carrier_received_unix_ms,
                plane_received_unix_ms: sample.plane_received_unix_ms,
                sample_gap_ms: sample.sample_gap_ms.max(sample.source_age_ms),
                skew_ms: sample.skew_ms,
                alignment: sample.method,
                telemetry_valid: sample.is_valid(),
                raw_carrier_position: vec3_array(sample.carrier_raw.position),
                corrected_carrier_position: vec3_array(sample.carrier.position),
                filtered_carrier_position: vec3_array(smoothed_pos),
                raw_carrier_velocity: vec3_array(sample.carrier_raw.velocity),
            });
        } else {
            self.telemetry_quality.dropped_samples += 1;
            self.telemetry_quality.completeness = Completeness::BufferLimit;
        }

        self.previous_x = x;

        true
    }

    pub fn landed(&mut self, carrier: &Transform, plane: &Transform) -> bool {
        let plane_reference = plane.position
            + match self.carrier_info.recovery {
                CarrierRecovery::Arrested => self.plane_info.hook.rotated_by(plane.rotation),
                CarrierRecovery::Vstol { .. } => {
                    self.plane_info.landing_reference.rotated_by(plane.rotation)
                }
            };
        let carrier_reference = carrier.position
            + self
                .carrier_info
                .approach_reference_offset(self.plane_info)
                .rotated_by(carrier.rotation);
        let horizontal_distance = DVec3::new(
            plane_reference.x - carrier_reference.x,
            0.0,
            plane_reference.z - carrier_reference.z,
        )
        .mag();
        if !horizontal_distance.is_finite() || horizontal_distance > 200.0 {
            tracing::warn!(horizontal_distance, "touchdown event rejected by geometry");
            return false;
        }
        self.observe_vstol_spot_zone(carrier, plane);
        // For V/STOL, the DCS land event contains the most accurate final
        // aircraft/carrier transforms. The normal sampling loop can stop a few
        // frames before that event, which made the terminal trace appear to end
        // slightly before the actual touchdown point. Append one exact terminal
        // datum from the land-event transforms so both V/STOL plots can finish
        // at the real touchdown position.
        if matches!(&self.carrier_info.recovery, CarrierRecovery::Vstol { .. }) {
            // Exact touchdown accuracy relative to Tarawa spot 7.5.  The AV-8B
            // pilot-ground reference is transformed into carrier-local coordinates,
            // then compared to the calibrated landing point using only the deck
            // plane axes (local X/Z).  Vertical compression/gear animation therefore
            // cannot distort the touchdown accuracy score.
            if let CarrierRecovery::Vstol { landing_point, .. } = &self.carrier_info.recovery {
                let spot_ref_world =
                    plane.position + self.plane_info.landing_reference.rotated_by(plane.rotation);
                let spot_ref_local =
                    (spot_ref_world - carrier.position).rotated_by(carrier.rotation.reversed());
                self.actual_nearest_spot = self
                    .carrier_info
                    .nearest_active_vstol_spot(spot_ref_local)
                    .map(|(label, _)| label);
                let dx = spot_ref_local.x - landing_point.x;
                let dz = spot_ref_local.z - landing_point.z;
                let spot_distance_m = (dx * dx + dz * dz).sqrt();
                self.spot_distance_m = Some(spot_distance_m);
            }

            let landing_pos_offset = self
                .carrier_info
                .approach_reference_offset(self.plane_info)
                .rotated_by(carrier.rotation);
            let landing_pos = carrier.position + landing_pos_offset;

            let ray_from_plane_to_carrier = DVec3::new(
                landing_pos.x - plane.position.x,
                0.0,
                landing_pos.z - plane.position.z,
            );

            let fb_rot = DRotor3::from_rotation_xz(
                (carrier.heading - self.carrier_info.deck_angle)
                    .neg()
                    .to_radians(),
            );
            let fb = DVec3::unit_z().rotated_by(fb_rot);
            let distance = ray_from_plane_to_carrier.mag();
            let x = ray_from_plane_to_carrier.dot(fb);
            let mut y = (distance.powi(2) - x.powi(2)).max(0.0).sqrt();

            let a = DVec3::unit_x().rotated_by(fb_rot);
            if ray_from_plane_to_carrier.dot(a) > 0.0 {
                y = y.neg();
            }

            let should_push = self
                .datums
                .last()
                .map(|d| (plane.time - d.time).abs() > 1.0e-6)
                .unwrap_or(true);

            if should_push {
                self.datums.push(Datum {
                    observation_sequence: None,
                    request_round_trip_ms: None,
                    queue_wait_ms: None,
                    lua_exec_ms: None,
                    queue_depth: None,
                    time: plane.time,
                    corrected_time_dcs: plane.time.max(carrier.time),
                    x,
                    y,
                    aoa: plane.aoa,
                    alt: plane.alt.max(0.0),
                    carrier_time: carrier.time,
                    plane_time: plane.time,
                    carrier_received_unix_ms: 0,
                    plane_received_unix_ms: 0,
                    sample_gap_ms: 0.0,
                    skew_ms: (carrier.time - plane.time).abs() * 1_000.0,
                    alignment: AlignmentMethod::Direct,
                    telemetry_valid: true,
                    raw_carrier_position: vec3_array(carrier.position),
                    corrected_carrier_position: vec3_array(carrier.position),
                    filtered_carrier_position: vec3_array(carrier.position),
                    raw_carrier_velocity: vec3_array(carrier.velocity),
                });
            }
        }

        let cable = match &self.carrier_info.recovery {
            CarrierRecovery::Arrested => self.wire_estimate_at(plane.time).wire,
            CarrierRecovery::Vstol { .. } => None,
        };
        if !matches!(self.grading, Some(Grading::Recovered { .. })) {
            self.grading = Some(Grading::Recovered {
                cable: None,
                cable_estimated: cable,
            });
            self.landing_time = Some(plane.time);
            self.touchdown_horizontal_speed_mps = Some(
                (plane.velocity.x * plane.velocity.x + plane.velocity.z * plane.velocity.z).sqrt(),
            );
            tracing::debug!(?cable, "first correlated touchdown recorded");
        } else {
            tracing::warn!(at = plane.time, "duplicate touchdown ignored");
            return false;
        }
        true
    }

    pub fn finish(mut self) -> TrackResult {
        let is_arrested = matches!(&self.carrier_info.recovery, CarrierRecovery::Arrested);
        let dcs_lso = self.dcs_lso.clone();
        let dcs_waveoff_ordered = dcs_lso.as_ref().is_some_and(|grade| grade.waveoff_ordered);

        // If the plane entered the groove but never landed and no other grading was set,
        // it performed a waveoff: DCS-ordered when the LSO comment says so, otherwise the
        // initiator is unknown.
        if self.grading.is_none() && self.entered_groove {
            self.grading = Some(if dcs_waveoff_ordered {
                Grading::WaveoffDcs
            } else {
                Grading::WaveoffUnknown
            });
        }

        let hook_state = if is_arrested {
            self.commanded_hook_state()
        } else {
            HookState::Unknown
        };
        self.hook_observation.interpreted_state = hook_state.as_str();
        let arrest_kinematics = if is_arrested {
            self.evaluate_arrest_kinematics()
        } else {
            ArrestKinematicsEvidence::default()
        };
        #[cfg(test)]
        let deck_speed_series = self.deck_speed_series();

        let touchdown_reference = self
            .landing_time
            .or(self.deck_crossing_time)
            .or_else(|| self.datums.last().map(|datum| datum.time))
            .unwrap_or_default();
        let wire_estimation = self.wire_estimate_at(touchdown_reference);
        if let (
            Some(wire),
            Some(Grading::Recovered {
                cable_estimated, ..
            }),
        ) = (wire_estimation.wire, self.grading.as_mut())
        {
            *cable_estimated = Some(wire);
        }

        // If DCS grading is set, use its reported wire for arrested recoveries only.
        let grading = if is_arrested {
            if let Some(dcs_wire) = dcs_lso.as_ref().and_then(|grade| grade.wire) {
                match self.grading {
                    Some(Grading::Recovered {
                        cable_estimated, ..
                    }) => Grading::Recovered {
                        cable: Some(dcs_wire),
                        cable_estimated,
                    },
                    _ => Grading::Recovered {
                        cable: Some(dcs_wire),
                        cable_estimated: None,
                    },
                }
            } else {
                self.grading.unwrap_or_default()
            }
        } else {
            self.grading.unwrap_or_default()
        };
        let grading = normalize_grading_for_recovery(grading, &self.carrier_info.recovery);

        let groove_time_secs = match (self.groove_entry_time, self.landing_time) {
            (Some(entry), Some(land)) if land > entry => Some(land - entry),
            _ => None,
        };

        // CATOBAR keeps the native wire/groove grading path.  AV-8B V/STOL
        // deliberately reuses the same GS/LU gate tiers, referenced to its 3.0°
        // glide slope, but excludes CATOBAR-only wire/groove bonuses.  AOA is
        // visual information only and is not part of the points calculation.
        let (approach_grade, approach_points) = if self.carrier_info.is_vstol() {
            compute_vstol_approach_grade_points(&grading, &self.gate_deviations)
        } else {
            let grade = compute_pass_grade(&grading, &self.gate_deviations, groove_time_secs);
            (grade, grade.points())
        };
        let spot_grade = self.spot_distance_m.map(SpotGrade::from_distance_m);

        // CATOBAR is intentionally untouched. Only a successfully recovered V/STOL
        // pass receives the spot-accuracy bonus and is then mapped back to the same
        // greenie-board labels used by CATOBAR (_OK_/OK/(OK)/--/C).
        let (mut pass_grade, mut grade_points) =
            if self.carrier_info.is_vstol() && matches!(&grading, Grading::Recovered { .. }) {
                match (spot_grade, approach_points) {
                    (Some(spot), Some(points)) => {
                        let (grade, points) = compute_vstol_final_grade_from_points(points, spot);
                        (grade, Some(points))
                    }
                    _ => (approach_grade, approach_points),
                }
            } else {
                (approach_grade, approach_points)
            };

        // A DCS-ordered waveoff is an outcome grade (WO), not an approach
        // grade, so it does not require three valid gates.
        if !self.gate_deviations.all_valid()
            && self.telemetry_quality.completeness == Completeness::Complete
            && grading != Grading::WaveoffDcs
        {
            self.telemetry_quality.completeness = Completeness::InsufficientGates;
        }
        // RunwayTouch/Land prove contact, not an arrest. A DCS wire, the completed
        // hook-deflection/cable-crossing correlation, or the deck kinematics
        // (aircraft stopped relative to the carrier) confirm the trap; without any
        // of them the pass cannot receive a favourable grade.
        let arrest_evidence = match &grading {
            Grading::Recovered { cable: Some(_), .. } if is_arrested => "dcs_wire",
            Grading::Recovered {
                cable_estimated: Some(_),
                ..
            } if is_arrested => "hook_transient",
            Grading::Recovered { .. } if is_arrested && arrest_kinematics.confirmed => "kinematic",
            Grading::Recovered { .. } if is_arrested => "unconfirmed",
            _ => "none",
        };
        if arrest_evidence == "unconfirmed" {
            self.telemetry_quality.completeness = Completeness::UnconfirmedArrest;
        }
        if self.telemetry_quality.completeness != Completeness::Complete {
            pass_grade = PassGrade::Incomplete;
            grade_points = None;
        }
        // Deck contact after a DCS-ordered waveoff is a cut pass regardless of
        // the approach quality (NAVAIR 00-80T-104: landing after a waveoff).
        if dcs_waveoff_ordered && grading.touched_deck() {
            pass_grade = PassGrade::Cut;
            grade_points = PassGrade::Cut.points();
        }

        TrackResult {
            pilot_name: self.pilot_name,
            grading,
            approach_grade,
            pass_grade,
            grade_points,
            spot_grade,
            spot_distance_m: self.spot_distance_m,
            intended_spot: self.carrier_info.is_vstol().then_some("7.5"),
            actual_nearest_spot: self.actual_nearest_spot,
            dcs_grading: self.dcs_grading,
            gate_deviations: self.gate_deviations,
            datums: self.datums,
            pattern_datums: self.pattern_datums,
            plane_info: self.plane_info,
            carrier_info: self.carrier_info,
            groove_time_secs,
            touchdown_time_dcs: self.landing_time,
            telemetry_quality: self.telemetry_quality,
            events: self.events,
            spot_zone: self.spot_zone,
            touchdown_horizontal_speed_mps: self.touchdown_horizontal_speed_mps,
            hook_observation: self.hook_observation,
            wire_estimation,
            hook_state,
            arrest_kinematics,
            arrest_evidence,
            dcs_lso,
            #[cfg(test)]
            deck_speed_series,
        }
    }

    /// Windowed carrier-relative speed series after the contact reference
    /// (time, speed m/s, x), for threshold tuning against recorded passes.
    #[cfg(test)]
    pub fn deck_speed_series(&self) -> Vec<(f64, f64, f64)> {
        let Some(reference) = self.contact_reference_time() else {
            return Vec::new();
        };
        self.deck_kinematics
            .iter()
            .enumerate()
            .filter(|(_, sample)| sample.time >= reference - 1.0)
            .filter_map(|(index, sample)| {
                let earlier = self.deck_kinematics[..index]
                    .iter()
                    .rev()
                    .find(|past| sample.time - past.time >= KINEMATIC_SPEED_WINDOW_S)?;
                let delta = sample.relative_position - earlier.relative_position;
                let horizontal = (delta.x * delta.x + delta.z * delta.z).sqrt();
                Some((
                    sample.time,
                    horizontal / (sample.time - earlier.time),
                    sample.x,
                ))
            })
            .collect()
    }

    /// Telemetry quality accumulated so far (read-only view for diagnostics).
    pub fn telemetry_quality(&self) -> &TelemetryQuality {
        &self.telemetry_quality
    }

    /// DCS time of the most recent processed sample.
    pub fn last_sample_time(&self) -> Option<f64> {
        self.previous_sample_time
    }

    /// Set the track's dcs grading.
    pub fn set_dcs_grading(&mut self, dcs_grading: String) -> bool {
        if self.dcs_grading.is_none() {
            self.dcs_lso = Some(parse_dcs_lso_grade(&dcs_grading));
            self.dcs_grading = Some(dcs_grading);
            true
        } else {
            false
        }
    }

    /// Earliest evidence of deck contact: the touchdown event, else the first
    /// inbound crossing of the ideal touchdown point while in the groove.
    fn contact_reference_time(&self) -> Option<f64> {
        [self.landing_time, self.deck_crossing_time]
            .into_iter()
            .flatten()
            .reduce(f64::min)
    }

    /// Confirms an arrest from carrier-relative deck kinematics. See the
    /// `ARREST_*` constants for the PROJECT-DERIVED thresholds.
    fn evaluate_arrest_kinematics(&self) -> ArrestKinematicsEvidence {
        let samples_total = self.deck_kinematics.len() as u32;
        let Some(reference) = self.contact_reference_time() else {
            return ArrestKinematicsEvidence {
                reason: "no_contact_reference",
                samples: samples_total,
                ..ArrestKinematicsEvidence::default()
            };
        };
        // Windowed carrier-relative horizontal speed per sample at or after
        // the reference; samples without a window partner are skipped.
        let samples = self
            .deck_kinematics
            .iter()
            .enumerate()
            .filter(|(_, sample)| sample.time >= reference)
            .filter_map(|(index, sample)| {
                let earlier = self.deck_kinematics[..index]
                    .iter()
                    .rev()
                    .find(|past| sample.time - past.time >= KINEMATIC_SPEED_WINDOW_S)?;
                let delta = sample.relative_position - earlier.relative_position;
                let horizontal = (delta.x * delta.x + delta.z * delta.z).sqrt();
                let speed = horizontal / (sample.time - earlier.time);
                speed.is_finite().then_some(WindowedDeckSample {
                    time: sample.time,
                    relative_speed_mps: speed,
                    x: sample.x,
                })
            })
            .collect::<Vec<_>>();
        if samples.is_empty() {
            return ArrestKinematicsEvidence {
                reason: "no_deck_samples_after_reference",
                reference_time_dcs: Some(reference),
                samples: samples_total,
                ..ArrestKinematicsEvidence::default()
            };
        }
        let min_relative_speed_mps = samples
            .iter()
            .map(|sample| sample.relative_speed_mps)
            .fold(f64::INFINITY, f64::min);

        // Earliest run of consecutive slow samples (no gap, inside the run-out
        // band) that lasts ARREST_HOLD_S and starts within the detection
        // window. The arresting cable pulls the aircraft back at 10-15 m/s for
        // ~1.5 s after the run-out, so an earlier short slow spell followed by
        // that pull-back must not end the search.
        let mut run_start: Option<&WindowedDeckSample> = None;
        let mut previous_time = None;
        let mut gap_seen = false;
        let mut best_held = 0.0_f64;
        let mut best_start: Option<&WindowedDeckSample> = None;
        for sample in &samples {
            let gap_ms =
                previous_time.map_or(0.0, |previous: f64| (sample.time - previous) * 1_000.0);
            previous_time = Some(sample.time);
            let slow = sample.relative_speed_mps <= ARREST_HOLD_MAX_RELATIVE_SPEED_MPS
                && (ARREST_MIN_X_M..=ARREST_MAX_X_M).contains(&sample.x);
            if gap_ms > ARREST_MAX_SAMPLE_GAP_MS {
                gap_seen = true;
                run_start = None;
            }
            if !slow {
                run_start = None;
                continue;
            }
            let start = match run_start {
                Some(start) => start,
                None => {
                    // A run must begin with a genuinely slow sample.
                    if sample.relative_speed_mps > ARREST_MAX_RELATIVE_SPEED_MPS {
                        continue;
                    }
                    run_start = Some(sample);
                    sample
                }
            };
            let held = sample.time - start.time;
            if held > best_held {
                best_held = held;
                best_start = Some(start);
            }
            if held >= ARREST_HOLD_S && start.time - reference <= ARREST_DETECTION_WINDOW_S {
                return ArrestKinematicsEvidence {
                    confirmed: true,
                    reason: "confirmed",
                    reference_time_dcs: Some(reference),
                    slow_since_dcs: Some(start.time),
                    held_s: Some(held),
                    min_relative_speed_mps: Some(min_relative_speed_mps),
                    x_at_slow_m: Some(start.x),
                    samples: samples_total,
                };
            }
        }
        ArrestKinematicsEvidence {
            confirmed: false,
            reason: match best_start {
                None => "never_slow_within_window",
                Some(_) if gap_seen => "telemetry_gap_in_arrest_window",
                Some(_) => "slow_but_not_held",
            },
            reference_time_dcs: Some(reference),
            slow_since_dcs: best_start.map(|start| start.time),
            held_s: best_start.map(|_| best_held),
            min_relative_speed_mps: Some(min_relative_speed_mps),
            x_at_slow_m: best_start.map(|start| start.x),
            samples: samples_total,
        }
    }

    fn observe_wire_crossings(
        &mut self,
        carrier: &Transform,
        plane: &Transform,
        bracket_gap_ms: f64,
    ) {
        if !self.entered_groove {
            self.previous_wire_plane = [None; 4];
            return;
        }

        let hook_offset = self.plane_info.hook.rotated_by(plane.rotation);
        let hook = plane.position + hook_offset;
        let forward = carrier.forward.rotated_by(DRotor3::from_rotation_xz(
            -self.carrier_info.deck_angle.to_radians(),
        ));

        let cables = [
            (1, &self.carrier_info.cable1),
            (2, &self.carrier_info.cable2),
            (3, &self.carrier_info.cable3),
            (4, &self.carrier_info.cable4),
        ];
        for (index, (wire, pendants)) in cables.into_iter().enumerate() {
            let left = carrier.position + pendants.0.rotated_by(carrier.rotation);
            let right = carrier.position + pendants.1.rotated_by(carrier.rotation);
            let across_wire = right - left;
            let across_wire_length_sq = across_wire.mag_sq();
            if across_wire_length_sq <= f64::EPSILON {
                self.previous_wire_plane[index] = None;
                continue;
            }
            let across_fraction = (hook - left).dot(across_wire) / across_wire_length_sq;
            let nearest_wire_point = left + across_wire * across_fraction.clamp(0.0, 1.0);
            let vertical_separation = (hook.y - nearest_wire_point.y).abs();
            if !(0.0..=1.0).contains(&across_fraction)
                || vertical_separation > MAX_WIRE_VERTICAL_SEPARATION_M
            {
                self.previous_wire_plane[index] = None;
                continue;
            }
            let midpoint = (left + right) / 2.0;
            let signed_distance = (hook - midpoint).dot(forward);
            if let Some((previous_distance, previous_time)) = self.previous_wire_plane[index] {
                if previous_distance < 0.0
                    && signed_distance >= 0.0
                    && plane.time > previous_time
                    && !self
                        .wire_crossings
                        .iter()
                        .any(|crossing| crossing.wire == wire)
                {
                    let ratio = (-previous_distance / (signed_distance - previous_distance))
                        .clamp(0.0, 1.0);
                    self.wire_crossings.push(WireCrossingEvidence {
                        wire,
                        timestamp_dcs: previous_time + (plane.time - previous_time) * ratio,
                        bracket_gap_ms,
                        method: "finite_hook_plane_crossing",
                    });
                }
            }
            self.previous_wire_plane[index] = Some((signed_distance, plane.time));
        }
    }

    fn wire_estimate_at(&self, event_time: f64) -> WireEstimateEvidence {
        let Some(deflection) = self.completed_hook_deflection_near(event_time) else {
            return WireEstimateEvidence {
                wire: None,
                confidence: "insufficient",
                reason: "no_complete_hook_deflection_near_touchdown",
                hook_deflection_time_dcs: None,
                hook_recovered_time_dcs: None,
                correlation_lag_ms: None,
                crossings: self.wire_crossings.clone(),
            };
        };
        let mut eligible = self
            .wire_crossings
            .iter()
            .filter(|crossing| {
                crossing.timestamp_dcs <= deflection.deflected_at_dcs
                    && crossing.bracket_gap_ms <= SAMPLE_GAP_WARNING_MS
            })
            .cloned()
            .collect::<Vec<_>>();
        eligible.sort_by(|left, right| left.timestamp_dcs.total_cmp(&right.timestamp_dcs));
        tracing::debug!(event_time, ?deflection, crossings = ?eligible, "wire crossing evidence at hook deflection");
        let Some(last) = eligible.last() else {
            return WireEstimateEvidence {
                wire: None,
                confidence: "insufficient",
                reason: "hook_deflection_not_correlated_with_wire_crossing",
                hook_deflection_time_dcs: Some(deflection.deflected_at_dcs),
                hook_recovered_time_dcs: Some(deflection.recovered_at_dcs),
                correlation_lag_ms: None,
                crossings: self.wire_crossings.clone(),
            };
        };
        let correlation_lag_ms = (deflection.deflected_at_dcs - last.timestamp_dcs) * 1_000.0;
        if !(0.0..=MAX_HOOK_DEFLECTION_WIRE_LAG_MS).contains(&correlation_lag_ms) {
            return WireEstimateEvidence {
                wire: None,
                confidence: "insufficient",
                reason: "hook_deflection_not_correlated_with_wire_crossing",
                hook_deflection_time_dcs: Some(deflection.deflected_at_dcs),
                hook_recovered_time_dcs: Some(deflection.recovered_at_dcs),
                correlation_lag_ms: Some(correlation_lag_ms),
                crossings: self.wire_crossings.clone(),
            };
        }
        WireEstimateEvidence {
            wire: Some(last.wire),
            confidence: if last.bracket_gap_ms <= 150.0 && correlation_lag_ms <= 150.0 {
                "high"
            } else {
                "medium"
            },
            reason: "hook_deflection_correlated_with_wire_crossing",
            hook_deflection_time_dcs: Some(deflection.deflected_at_dcs),
            hook_recovered_time_dcs: Some(deflection.recovered_at_dcs),
            correlation_lag_ms: Some(correlation_lag_ms),
            crossings: self.wire_crossings.clone(),
        }
    }

    fn completed_hook_deflection_near(&self, event_time: f64) -> Option<CompletedHookDeflection> {
        let samples = self
            .hook_observation
            .timeline
            .iter()
            .filter(|sample| {
                sample.status == HookSampleStatus::Success
                    && sample.in_final_window
                    && sample.raw.is_some_and(f64::is_finite)
            })
            .collect::<Vec<_>>();

        for deflected_index in 1..samples.len() {
            let before = samples[deflected_index - 1];
            let deflected = samples[deflected_index];
            let Some(before_raw) = before.raw else {
                continue;
            };
            let Some(deflected_raw) = deflected.raw else {
                continue;
            };
            let transition_gap_ms =
                (deflected.associated_time_dcs - before.associated_time_dcs) * 1_000.0;
            if before_raw < HOOK_DOWN_STABLE_MIN
                || deflected_raw > HOOK_DEFLECTED_MAX
                || !(0.0..=SAMPLE_GAP_WARNING_MS).contains(&transition_gap_ms)
                || (deflected.associated_time_dcs - event_time).abs()
                    > MAX_HOOK_DEFLECTION_TOUCH_OFFSET_S
            {
                continue;
            }

            let mut stable_start_dcs = before.associated_time_dcs;
            let mut newer_time_dcs = before.associated_time_dcs;
            for sample in samples[..deflected_index - 1].iter().rev() {
                let gap_ms = (newer_time_dcs - sample.associated_time_dcs) * 1_000.0;
                if sample.raw.is_none_or(|raw| raw < HOOK_DOWN_STABLE_MIN)
                    || !(0.0..=SAMPLE_GAP_WARNING_MS).contains(&gap_ms)
                {
                    break;
                }
                stable_start_dcs = sample.associated_time_dcs;
                newer_time_dcs = sample.associated_time_dcs;
            }
            if before.associated_time_dcs - stable_start_dcs < MIN_HOOK_DOWN_STABLE_S {
                continue;
            }

            let recovered = samples[deflected_index + 1..].iter().find(|sample| {
                let elapsed = sample.associated_time_dcs - deflected.associated_time_dcs;
                (0.0..=MAX_HOOK_DEFLECTION_RECOVERY_S).contains(&elapsed)
                    && sample.raw.is_some_and(|raw| raw >= HOOK_DOWN_STABLE_MIN)
            });
            if let Some(recovered) = recovered {
                return Some(CompletedHookDeflection {
                    deflected_at_dcs: deflected.associated_time_dcs,
                    recovered_at_dcs: recovered.associated_time_dcs,
                });
            }
        }
        None
    }

    pub fn observe_hook_sample(
        &mut self,
        associated_time_dcs: f64,
        observed_unix_ms: u64,
        age_ms: f64,
        raw: Option<f64>,
        status: HookSampleStatus,
    ) {
        if !matches!(self.carrier_info.recovery, CarrierRecovery::Arrested) {
            return;
        }
        let in_groove = self.entered_groove;
        let in_final_window = in_groove && self.last_x <= GATE_QUARTER_NM;
        let before_touchdown = self
            .landing_time
            .is_none_or(|landing| associated_time_dcs < landing);
        match status {
            HookSampleStatus::Success => self.hook_observation.successful_samples += 1,
            HookSampleStatus::Timeout => self.hook_observation.timeout_samples += 1,
            HookSampleStatus::Error => self.hook_observation.error_samples += 1,
            HookSampleStatus::Stale => self.hook_observation.stale_samples += 1,
        }

        if status == HookSampleStatus::Success {
            if let Some(raw) = raw.filter(|value| value.is_finite()) {
                self.hook_observation.min_raw = Some(
                    self.hook_observation
                        .min_raw
                        .map_or(raw, |value| value.min(raw)),
                );
                self.hook_observation.max_raw = Some(
                    self.hook_observation
                        .max_raw
                        .map_or(raw, |value| value.max(raw)),
                );
                if in_groove {
                    self.hook_observation.samples_in_groove += 1;
                }
                if in_final_window {
                    self.hook_observation.samples_in_final_window += 1;
                    self.hook_observation.final_raw = Some(raw);
                }
            }
        }

        if self.hook_observation.timeline.len() == MAX_HOOK_EVIDENCE {
            self.hook_observation.timeline.pop_front();
            self.hook_observation.compacted_samples += 1;
        }
        self.hook_observation
            .timeline
            .push_back(HookSampleEvidence {
                associated_time_dcs,
                observed_unix_ms,
                age_ms,
                raw,
                status,
                in_groove,
                in_final_window,
                before_touchdown,
            });
    }

    /// Pilot-commanded hook state, latched from the latest stable run of
    /// in-groove samples that ends `HOOK_BASELINE_GUARD_S` before the earliest
    /// contact evidence. The arrestment excursion of the animated hook is
    /// therefore excluded and a real trap keeps reading `Down`. Modules without
    /// a validated argument are never interpreted.
    fn commanded_hook_state(&mut self) -> HookState {
        let observation = &mut self.hook_observation;
        observation.baseline_state = HookState::Unknown;
        observation.baseline_value = None;
        observation.baseline_samples = 0;
        observation.baseline_start_dcs = None;
        observation.baseline_end_dcs = None;
        let Some(argument) = self.plane_info.hook_argument else {
            observation.baseline_reason = "no_validated_hook_argument";
            return HookState::Unknown;
        };
        let guard_end = [self.landing_time, self.deck_crossing_time]
            .into_iter()
            .flatten()
            .reduce(f64::min)
            .map(|contact| contact - HOOK_BASELINE_GUARD_S);
        let usable = observation
            .timeline
            .iter()
            .filter(|sample| {
                sample.status == HookSampleStatus::Success
                    && sample.in_groove
                    && sample.raw.is_some_and(f64::is_finite)
                    && guard_end.is_none_or(|end| sample.associated_time_dcs <= end)
            })
            .collect::<Vec<_>>();
        let Some(latest) = usable.last() else {
            observation.baseline_reason = "no_in_groove_samples_before_contact_guard";
            return HookState::Unknown;
        };
        let band = |raw: f64| {
            if raw <= argument.up_max {
                Some(HookState::Up)
            } else if raw >= argument.down_min {
                Some(HookState::Down)
            } else {
                None
            }
        };
        let Some(state) = latest.raw.and_then(band) else {
            observation.baseline_reason = "latest_baseline_sample_between_bands";
            return HookState::Unknown;
        };
        let window_start = latest.associated_time_dcs - HOOK_BASELINE_WINDOW_S;
        let recent = usable
            .iter()
            .rev()
            .take_while(|sample| sample.associated_time_dcs >= window_start)
            .collect::<Vec<_>>();
        if recent.len() < HOOK_BASELINE_MIN_SAMPLES {
            observation.baseline_reason = "too_few_baseline_samples";
            return HookState::Unknown;
        }
        if !recent
            .iter()
            .all(|sample| sample.raw.and_then(band) == Some(state))
        {
            observation.baseline_reason = "baseline_not_stable";
            return HookState::Unknown;
        }
        let first = recent[recent.len() - 1];
        let span = latest.associated_time_dcs - first.associated_time_dcs;
        if span < HOOK_BASELINE_MIN_SPAN_S {
            observation.baseline_reason = "baseline_span_too_short";
            return HookState::Unknown;
        }
        let mean = recent.iter().filter_map(|sample| sample.raw).sum::<f64>() / recent.len() as f64;
        observation.baseline_state = state;
        observation.baseline_value = Some(mean);
        observation.baseline_samples = recent.len() as u32;
        observation.baseline_start_dcs = Some(first.associated_time_dcs);
        observation.baseline_end_dcs = Some(latest.associated_time_dcs);
        observation.baseline_reason = "";
        state
    }

    pub fn mark_telemetry_gap(&mut self, reason: TelemetryInvalidReason) {
        self.telemetry_quality.invalid_samples += 1;
        if self.entered_groove
            || (self.previous_x > 0.0 && self.previous_x <= GATE_THREE_QUARTER_NM)
        {
            self.telemetry_quality.scoring_invalid_samples += 1;
            self.telemetry_quality.completeness = Completeness::TelemetryGap;
        } else {
            self.telemetry_quality.pattern_invalid_samples += 1;
        }
        if !self.telemetry_quality.reasons.contains(&reason) {
            self.telemetry_quality.reasons.push(reason);
        }
    }

    pub fn record_event(
        &mut self,
        kind: impl Into<String>,
        timestamp_dcs: f64,
        accepted: bool,
        reason: impl Into<String>,
    ) {
        if self.events.len() < MAX_EVENT_EVIDENCE {
            let accepted_confidence = if accepted { "correlated" } else { "rejected" };
            self.events.push(EventEvidence {
                sequence: self.events.len() as u32 + 1,
                kind: kind.into(),
                timestamp_dcs,
                source: "dcs-grpc-event-stream",
                confidence: accepted_confidence,
                accepted,
                reason: reason.into(),
            });
        } else {
            self.telemetry_quality.dropped_samples += 1;
            self.telemetry_quality.completeness = Completeness::BufferLimit;
        }
    }

    fn observe_vstol_spot_zone(&mut self, carrier: &Transform, plane: &Transform) {
        let CarrierRecovery::Vstol { landing_point, .. } = &self.carrier_info.recovery else {
            return;
        };
        let plane_reference =
            plane.position + self.plane_info.landing_reference.rotated_by(plane.rotation);
        let local = (plane_reference - carrier.position).rotated_by(carrier.rotation.reversed());
        let dx = local.x - landing_point.x;
        let dz = local.z - landing_point.z;
        let inside = (dx * dx + dz * dz).sqrt() <= self.spot_zone.radius_m;
        if inside {
            self.spot_zone.entered_at_dcs.get_or_insert(plane.time);
            self.spot_zone.last_present_at_dcs = Some(plane.time);
        } else if self.spot_zone.entered_at_dcs.is_some()
            && self.spot_zone.last_present_at_dcs.is_some()
            && self.spot_zone.exited_at_dcs.is_none()
        {
            self.spot_zone.exited_at_dcs = Some(plane.time);
        }
    }
}

fn vec3_array(value: DVec3) -> [f64; 3] {
    [value.x, value.y, value.z]
}

fn mark_started_inside(x: f64, gate: f64, quality: &mut GateQuality) {
    if x <= gate && quality.status == GateStatus::Missing {
        quality.status = GateStatus::Late;
        quality.reason = Some("tracking_started_inside_gate".to_string());
    }
}

fn capture_gate_from_window(
    samples: &VecDeque<ApproachSample>,
    current: &ApproachSample,
    gate: f64,
    ideal_base_alt: f64,
    glide_slope_deg: f64,
    datum: &mut Option<GateDatum>,
    quality: &mut GateQuality,
) {
    if datum.is_some() || quality.status == GateStatus::Valid || current.x > gate {
        return;
    }

    let mut best_failure = None;
    for previous in samples.iter().rev().filter(|sample| sample.x > gate) {
        let mut candidate = None;
        let mut candidate_quality = GateQuality::default();
        capture_gate(
            previous,
            current,
            gate,
            ideal_base_alt,
            glide_slope_deg,
            &mut candidate,
            &mut candidate_quality,
        );
        if candidate_quality.status == GateStatus::Valid {
            *datum = candidate;
            *quality = candidate_quality;
            return;
        }
        best_failure.get_or_insert(candidate_quality);
    }
    if let Some(failure) = best_failure {
        *quality = failure;
    }
}

fn capture_gate(
    previous: &ApproachSample,
    current: &ApproachSample,
    gate: f64,
    ideal_base_alt: f64,
    glide_slope_deg: f64,
    datum: &mut Option<GateDatum>,
    quality: &mut GateQuality,
) {
    if datum.is_some() || quality.status == GateStatus::Valid {
        return;
    }
    if !(previous.x > gate && current.x <= gate) {
        return;
    }
    if !previous.valid || !current.valid {
        quality.status = GateStatus::Invalid;
        quality.reason = Some("invalid_or_non_inbound_bracketing_sample".to_string());
        return;
    }
    if current.time <= previous.time {
        quality.status = GateStatus::Invalid;
        quality.reason = Some("non_monotonic_gate_bracket".to_string());
        return;
    }
    let bracket_gap_ms = (current.time - previous.time) * 1_000.0;
    if bracket_gap_ms > SAMPLE_GAP_WARNING_MS {
        quality.status = GateStatus::Invalid;
        quality.reason = Some("stale_gate_bracket".to_string());
        return;
    }
    if previous.skew_ms.max(current.skew_ms) > MAX_EXTRAPOLATION_MS {
        quality.status = GateStatus::Invalid;
        quality.reason = Some("excessive_skew_at_gate".to_string());
        return;
    }
    if !previous.in_approach || !current.in_approach {
        quality.status = GateStatus::Invalid;
        quality.reason = Some("outside_approach_altitude".to_string());
        return;
    }
    if !previous.lined_up || !current.lined_up {
        quality.status = GateStatus::Invalid;
        quality.reason = Some("outside_approach_lineup".to_string());
        return;
    }

    let span = previous.x - current.x;
    if !span.is_finite() || span <= f64::EPSILON {
        quality.status = GateStatus::Invalid;
        quality.reason = Some("invalid_gate_bracket".to_string());
        return;
    }
    let ratio = ((previous.x - gate) / span).clamp(0.0, 1.0);
    let interpolate = |a: f64, b: f64| a + (b - a) * ratio;
    let alt = interpolate(previous.alt, current.alt);
    let y = interpolate(previous.y, current.y);
    let timestamp_dcs = interpolate(previous.time, current.time);
    let ideal_alt = ideal_base_alt + gate * glide_slope_deg.to_radians().tan();
    let gs_deviation_m = alt - ideal_alt;
    let sample_gap_ms = bracket_gap_ms;
    let skew_ms = previous.skew_ms.max(current.skew_ms);

    *datum = Some(GateDatum {
        gs_deviation_deg: gs_deviation_m.atan2(gate).to_degrees(),
        lineup_deg: y.atan2(gate).to_degrees(),
        gs_deviation_ft: m_to_ft(gs_deviation_m),
        lineup_ft: m_to_ft(y),
        timestamp_dcs,
        distance_m: gate,
        sample_gap_ms,
        skew_ms,
        method: if (current.x - gate).abs() <= 0.5 {
            GateCaptureMethod::Measured
        } else {
            GateCaptureMethod::Interpolated
        },
    });
    quality.status = GateStatus::Valid;
    quality.reason = None;
}

/// Parses the wire number from a DCS LSO comment, tolerating `WIRE# 3`,
/// `WIRE#3`, `WIRE #3` and `WIRE# 4[BC]`.
fn parse_dcs_wire(comment: &str) -> Option<u8> {
    let (_, suffix) = comment.split_once("WIRE")?;
    suffix
        .trim_start()
        .trim_start_matches('#')
        .trim_start()
        .chars()
        .take_while(char::is_ascii_digit)
        .collect::<String>()
        .parse()
        .ok()
}

/// Parses a DCS `LandingQualityMark` comment such as
/// `LSO: GRADE:--- : _SLOX_  WIRE# 4[BC]` or
/// `LSO: GRADE:WO  _LOIC_  WO(AFU)IC [BC]`.
fn parse_dcs_lso_grade(comment: &str) -> DcsLsoGrade {
    let grade = comment
        .split_once("GRADE:")
        .map(|(_, rest)| {
            rest.trim_start()
                .split(|c: char| c.is_whitespace() || c == ':')
                .next()
                .unwrap_or_default()
                .to_string()
        })
        .filter(|grade| !grade.is_empty());
    let waveoff_ordered = grade.as_deref() == Some("WO")
        || comment
            .split_whitespace()
            .any(|token| token.trim_start_matches(['_', '(']).starts_with("WO"));
    DcsLsoGrade {
        grade,
        wire: parse_dcs_wire(comment),
        waveoff_ordered,
    }
}

fn normalize_grading_for_recovery(grading: Grading, recovery: &CarrierRecovery) -> Grading {
    match (recovery, grading) {
        // Intentional bolters are hook-up qualification passes and only exist
        // for arrested recoveries. Never expose this outcome on V/STOL.
        (CarrierRecovery::Vstol { .. }, Grading::TouchAndGo { .. }) => Grading::WaveoffUnknown,
        (_, grading) => grading,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_wire_falls_back_to_dcs_when_estimate_is_unavailable() {
        assert_eq!(select_wire_for_display(None, Some(4)), (Some(4), "dcs"));
    }

    #[test]
    fn display_wire_keeps_dcs_authoritative_when_both_sources_exist() {
        assert_eq!(select_wire_for_display(Some(2), Some(3)), (Some(3), "dcs"));
    }

    #[test]
    fn display_wire_is_unavailable_without_evidence() {
        assert_eq!(select_wire_for_display(None, None), (None, "unavailable"));
    }

    fn approach_sample(time: f64, x: f64) -> ApproachSample {
        ApproachSample {
            time,
            x,
            y: 0.0,
            alt: x * 3.5_f64.to_radians().tan(),
            valid: true,
            in_approach: true,
            lined_up: true,
            skew_ms: 0.0,
        }
    }

    fn gate(timestamp_dcs: f64) -> GateDatum {
        GateDatum {
            gs_deviation_deg: 0.0,
            lineup_deg: 0.0,
            gs_deviation_ft: 0.0,
            lineup_ft: 0.0,
            timestamp_dcs,
            distance_m: 0.0,
            sample_gap_ms: 100.0,
            skew_ms: 0.0,
            method: GateCaptureMethod::Interpolated,
        }
    }

    fn valid_quality() -> GateQuality {
        GateQuality {
            status: GateStatus::Valid,
            reason: None,
        }
    }

    #[test]
    fn gate_requires_two_valid_bracketing_samples_and_interpolates() {
        let previous = approach_sample(10.0, 1_000.0);
        let current = approach_sample(10.1, 900.0);
        let mut datum = None;
        let mut quality = GateQuality::default();

        capture_gate(
            &previous,
            &current,
            GATE_HALF_NM,
            0.0,
            3.5,
            &mut datum,
            &mut quality,
        );

        let datum = datum.expect("interpolated gate");
        assert_eq!(quality.status, GateStatus::Valid);
        assert_eq!(datum.method, GateCaptureMethod::Interpolated);
        assert_eq!(datum.distance_m, GATE_HALF_NM);
        assert!((datum.timestamp_dcs - 10.074).abs() < 1.0e-9);
    }

    #[test]
    fn tracking_started_inside_gate_is_late_and_never_backfilled() {
        let mut quality = GateQuality::default();
        mark_started_inside(900.0, GATE_HALF_NM, &mut quality);
        assert_eq!(quality.status, GateStatus::Late);

        let mut datum = None;
        capture_gate(
            &approach_sample(1.0, 1_000.0),
            &approach_sample(1.1, 900.0),
            GATE_HALF_NM,
            0.0,
            3.5,
            &mut datum,
            &mut quality,
        );
        assert!(datum.is_some());
        // The later bracket is a real observation and may become valid; the
        // original late startup alone never created a datum.
        assert_eq!(quality.status, GateStatus::Valid);
    }

    #[test]
    fn stale_skewed_or_reordered_gate_brackets_are_invalid() {
        let cases = [
            (0.0, 1.4, "stale_gate_bracket"),
            (301.0, 1.1, "excessive_skew_at_gate"),
            (0.0, 0.5, "non_monotonic_gate_bracket"),
        ];
        for (skew, current_time, reason) in cases {
            let previous = approach_sample(1.0, 1_000.0);
            let mut current = approach_sample(current_time, 900.0);
            current.skew_ms = skew;
            let mut datum = None;
            let mut quality = GateQuality::default();
            capture_gate(
                &previous,
                &current,
                GATE_HALF_NM,
                0.0,
                3.5,
                &mut datum,
                &mut quality,
            );
            assert!(datum.is_none());
            assert_eq!(quality.status, GateStatus::Invalid);
            assert_eq!(quality.reason.as_deref(), Some(reason));
        }
    }

    #[test]
    fn corpus_false_gate_rejections_use_only_the_real_endpoints() {
        // Exact endpoint intervals observed at 14:19:14 (1/2), 14:22:09
        // (1/2) and 16:31:37 (3/4), after older 591/689/421 ms gaps.
        for (gate_distance, old_gap_ms, endpoint_gap_ms) in [
            (GATE_HALF_NM, 591.0, 90.0),
            (GATE_HALF_NM, 689.0, 60.0),
            (GATE_THREE_QUARTER_NM, 421.0, 60.0),
        ] {
            let first_time = 1.0;
            let outside_time = first_time + old_gap_ms / 1_000.0;
            let samples = VecDeque::from([
                approach_sample(first_time, gate_distance + 200.0),
                approach_sample(outside_time, gate_distance + 50.0),
            ]);
            let current = approach_sample(
                outside_time + endpoint_gap_ms / 1_000.0,
                gate_distance - 50.0,
            );
            let mut datum = None;
            let mut quality = GateQuality::default();
            capture_gate_from_window(
                &samples,
                &current,
                gate_distance,
                0.0,
                3.5,
                &mut datum,
                &mut quality,
            );
            assert_eq!(quality.status, GateStatus::Valid);
            assert!((datum.expect("gate").sample_gap_ms - endpoint_gap_ms).abs() < 0.001);
        }
    }

    #[test]
    fn zero_one_or_two_gates_are_incomplete_and_three_ordered_gates_are_valid() {
        let mut gates = GateDeviations::default();
        assert!(!gates.all_valid());
        gates.at_three_quarter_nm = Some(gate(1.0));
        gates.three_quarter_quality = valid_quality();
        assert!(!gates.all_valid());
        gates.at_half_nm = Some(gate(2.0));
        gates.half_quality = valid_quality();
        assert!(!gates.all_valid());
        gates.at_quarter_nm = Some(gate(3.0));
        gates.quarter_quality = valid_quality();
        assert!(gates.all_valid());
        gates.at_quarter_nm.as_mut().unwrap().timestamp_dcs = 1.5;
        assert!(!gates.all_valid());
    }

    #[test]
    fn telemetry_gap_only_invalidates_the_scored_segment() {
        let carrier = CarrierInfo::by_type("CVN_71").unwrap();
        let plane = AirplaneInfo::by_type("FA-18C_hornet").unwrap();

        let mut pattern = Track::new("pilot", carrier, plane);
        pattern.previous_x = GATE_THREE_QUARTER_NM + 500.0;
        pattern.mark_telemetry_gap(TelemetryInvalidReason::TelemetryGap);
        assert_eq!(
            pattern.telemetry_quality.completeness,
            Completeness::Complete
        );
        assert_eq!(pattern.telemetry_quality.pattern_invalid_samples, 1);
        assert_eq!(pattern.telemetry_quality.scoring_invalid_samples, 0);

        let mut groove = Track::new("pilot", carrier, plane);
        groove.entered_groove = true;
        groove.mark_telemetry_gap(TelemetryInvalidReason::TelemetryGap);
        assert_eq!(
            groove.telemetry_quality.completeness,
            Completeness::TelemetryGap
        );
        assert_eq!(groove.telemetry_quality.scoring_invalid_samples, 1);
    }

    #[test]
    fn touchdown_without_arrest_confirmation_is_explicitly_unavailable() {
        let carrier = CarrierInfo::by_type("CVN_71").unwrap();
        let plane = AirplaneInfo::by_type("FA-18C_hornet").unwrap();
        let mut track = Track::new("pilot", carrier, plane);
        track.gate_deviations = GateDeviations {
            at_three_quarter_nm: Some(gate(1.0)),
            at_half_nm: Some(gate(2.0)),
            at_quarter_nm: Some(gate(3.0)),
            three_quarter_quality: valid_quality(),
            half_quality: valid_quality(),
            quarter_quality: valid_quality(),
        };
        track.grading = Some(Grading::Recovered {
            cable: None,
            cable_estimated: None,
        });

        let result = track.finish();
        assert_eq!(
            result.telemetry_quality.completeness,
            Completeness::UnconfirmedArrest
        );
        assert_eq!(result.pass_grade, PassGrade::Incomplete);
        assert_eq!(result.grade_points, None);
    }

    #[test]
    fn event_evidence_preserves_arrival_order_and_first_touchdown() {
        let carrier = CarrierInfo::by_type("LHA_Tarawa").unwrap();
        let plane_info = AirplaneInfo::by_type("AV8BNA").unwrap();
        let mut track = Track::new("pilot", carrier, plane_info);
        let carrier_transform = Transform::default();
        let first_plane = Transform {
            time: 10.0,
            velocity: DVec3::new(0.0, 0.0, 0.0),
            ..Transform::default()
        };
        let second_plane = Transform {
            time: 10.2,
            velocity: DVec3::new(20.0, 0.0, 0.0),
            ..Transform::default()
        };
        assert!(track.landed(&carrier_transform, &first_plane));
        track.record_event("runway_touch", 10.0, true, "first");
        assert!(!track.landed(&carrier_transform, &second_plane));
        track.record_event("land", 10.2, false, "duplicate");
        let result = track.finish();
        assert_eq!(result.events[0].sequence, 1);
        assert_eq!(result.events[1].sequence, 2);
        assert_eq!(result.touchdown_time_dcs, Some(10.0));
        assert_eq!(result.touchdown_horizontal_speed_mps, Some(0.0));
    }

    #[test]
    fn event_buffer_is_bounded_and_overload_marks_result_incomplete() {
        let carrier = CarrierInfo::by_type("CVN_71").unwrap();
        let plane = AirplaneInfo::by_type("FA-18C_hornet").unwrap();
        let mut track = Track::new("pilot", carrier, plane);
        for sequence in 0..(MAX_EVENT_EVIDENCE + 10) {
            track.record_event("synthetic", sequence as f64, false, "robustness_test");
        }
        let result = track.finish();
        assert_eq!(result.events.len(), MAX_EVENT_EVIDENCE);
        assert_eq!(result.telemetry_quality.dropped_samples, 10);
        assert_eq!(
            result.telemetry_quality.completeness,
            Completeness::BufferLimit
        );
        assert_eq!(result.grade_points, None);
    }

    #[test]
    fn simulated_vl_and_rvl_keep_raw_speed_without_inventing_a_threshold() {
        let carrier = CarrierInfo::by_type("LHA_Tarawa").unwrap();
        let plane_info = AirplaneInfo::by_type("AV8BNA").unwrap();
        let carrier_transform = Transform::default();
        for speed in [0.0, 25.0] {
            let mut track = Track::new("pilot", carrier, plane_info);
            let plane = Transform {
                time: 1.0,
                velocity: DVec3::new(speed, 0.0, 0.0),
                ..Transform::default()
            };
            assert!(track.landed(&carrier_transform, &plane));
            assert_eq!(track.finish().touchdown_horizontal_speed_mps, Some(speed));
        }
    }

    #[test]
    fn simulated_vstol_touch_and_go_is_neutral_not_a_bolter() {
        // Robustness simulation only; it does not prove real Tarawa event order.
        let carrier_info = CarrierInfo::by_type("LHA_Tarawa").unwrap();
        let plane_info = AirplaneInfo::by_type("AV8BNA").unwrap();
        let carrier = Transform::default();
        let contact = Transform {
            time: 1.0,
            ..Transform::default()
        };
        let mut track = Track::new("pilot", carrier_info, plane_info);
        assert!(track.next(&carrier, &contact, None));
        assert!(track.landed(&carrier, &contact));
        let departure = Transform {
            time: 2.0,
            position: DVec3::new(0.0, 0.0, 300.0),
            ..Transform::default()
        };
        assert!(!track.next(&carrier, &departure, None));
        assert_eq!(track.finish().grading, Grading::WaveoffUnknown);
    }

    #[test]
    fn simulated_bounce_and_reordered_land_events_keep_first_contact() {
        // Robustness simulation only; duplicate/reordered DCS delivery remains
        // a deferred live-validation item.
        let carrier_info = CarrierInfo::by_type("LHA_Tarawa").unwrap();
        let plane_info = AirplaneInfo::by_type("AV8BNA").unwrap();
        let carrier = Transform::default();
        let second_arrival = Transform {
            time: 20.0,
            ..Transform::default()
        };
        let delayed_earlier_event = Transform {
            time: 19.5,
            ..Transform::default()
        };
        let mut track = Track::new("pilot", carrier_info, plane_info);
        assert!(track.landed(&carrier, &second_arrival));
        assert!(!track.landed(&carrier, &delayed_earlier_event));
        assert_eq!(track.finish().touchdown_time_dcs, Some(20.0));
    }

    #[test]
    fn wire_estimation_is_stable_across_zero_and_360_degree_headings() {
        let carrier_info = CarrierInfo::by_type("CVN_71").unwrap();
        let plane_info = AirplaneInfo::by_type("FA-18C_hornet").unwrap();
        for heading in [0.0_f64, 359.999] {
            let rotation = DRotor3::from_euler_angles(0.0, 0.0, -heading.to_radians());
            let forward = DVec3::new(heading.to_radians().sin(), 0.0, heading.to_radians().cos());
            let carrier = Transform {
                heading,
                rotation,
                forward,
                ..Transform::default()
            };
            let midpoint = (carrier_info.cable3.0 + carrier_info.cable3.1) / 2.0;
            let midpoint_world = midpoint.rotated_by(carrier.rotation);
            let plane_rotation = carrier.rotation;
            let hook_offset = plane_info.hook.rotated_by(plane_rotation);
            let mut track = Track::new("pilot", carrier_info, plane_info);
            track.entered_groove = true;
            track.last_x = 100.0;
            let before = Transform {
                position: midpoint_world - hook_offset - forward,
                rotation: plane_rotation,
                time: 1.0,
                ..Transform::default()
            };
            let after = Transform {
                position: midpoint_world - hook_offset + forward,
                rotation: plane_rotation,
                time: 1.1,
                ..Transform::default()
            };
            track.observe_wire_crossings(&carrier, &before, 100.0);
            track.observe_wire_crossings(&carrier, &after, 100.0);
            observe_test_hook_sample(&mut track, 0.65, 1.0);
            observe_test_hook_sample(&mut track, 0.90, 1.0);
            observe_test_hook_sample(&mut track, 1.15, 0.0);
            observe_test_hook_sample(&mut track, 1.80, 1.0);
            assert_eq!(track.wire_estimate_at(1.4).wire, Some(3));
        }
    }

    #[test]
    fn crossing_without_complete_hook_deflection_does_not_name_a_wire() {
        let carrier_info = CarrierInfo::by_type("CVN_71").unwrap();
        let plane_info = AirplaneInfo::by_type("FA-18C_hornet").unwrap();
        let carrier = Transform {
            forward: DVec3::unit_z(),
            ..Transform::default()
        };
        let midpoint = (carrier_info.cable3.0 + carrier_info.cable3.1) / 2.0;
        let hook_offset = plane_info.hook;
        let mut track = Track::new("pilot", carrier_info, plane_info);
        track.entered_groove = true;
        track.last_x = 100.0;
        track.observe_wire_crossings(
            &carrier,
            &Transform {
                position: midpoint - hook_offset - DVec3::unit_z(),
                time: 1.0,
                ..Transform::default()
            },
            100.0,
        );
        track.observe_wire_crossings(
            &carrier,
            &Transform {
                position: midpoint - hook_offset + DVec3::unit_z(),
                time: 1.1,
                ..Transform::default()
            },
            100.0,
        );

        observe_test_hook_sample(&mut track, 0.65, 1.0);
        observe_test_hook_sample(&mut track, 0.90, 1.0);
        observe_test_hook_sample(&mut track, 1.15, 0.0);
        let estimate = track.wire_estimate_at(1.4);
        assert_eq!(estimate.wire, None);
        assert_eq!(
            estimate.reason,
            "no_complete_hook_deflection_near_touchdown"
        );
    }

    #[test]
    fn labelled_t45_and_f14_hook_transients_select_the_dcs_wire() {
        struct LabelledTrap {
            module: &'static str,
            expected_wire: u8,
            touchdown_time: f64,
            deflection_time: f64,
            recovery_time: f64,
            crossings: &'static [(u8, f64)],
        }

        let cases = [
            LabelledTrap {
                module: "T-45",
                expected_wire: 4,
                touchdown_time: 1881.79,
                deflection_time: 1881.27,
                recovery_time: 1884.18,
                crossings: &[
                    (1, 1_880.385_933),
                    (2, 1_880.629_196),
                    (3, 1_880.882_638),
                    (4, 1_881.139_835),
                ],
            },
            LabelledTrap {
                module: "T-45",
                expected_wire: 1,
                touchdown_time: 2941.25,
                deflection_time: 2940.18,
                recovery_time: 2942.97,
                crossings: &[
                    (1, 2_940.104_828),
                    (2, 2_940.345_659),
                    (3, 2_940.591_212),
                    (4, 2_940.847_226),
                ],
            },
            LabelledTrap {
                module: "F-14BU",
                expected_wire: 2,
                touchdown_time: 5871.96,
                deflection_time: 5870.70,
                recovery_time: 5877.09,
                crossings: &[
                    (1, 5_870.483_342),
                    (2, 5_870.690_110),
                    (3, 5_870.901_822),
                    (4, 5_871.117_252),
                ],
            },
            LabelledTrap {
                module: "F-14BU",
                expected_wire: 4,
                touchdown_time: 6599.68,
                deflection_time: 6598.53,
                recovery_time: 6605.01,
                // The stored schema-6 crossing was an early infinite-plane
                // false positive. This is the finite deck crossing recomputed
                // from the recorded geometry.
                crossings: &[(4, 6598.500)],
            },
        ];

        for case in cases {
            let mut track = Track::new(
                "pilot",
                CarrierInfo::by_type("CVN_71").unwrap(),
                AirplaneInfo::by_type(case.module).unwrap(),
            );
            track.entered_groove = true;
            track.last_x = 100.0;
            track.wire_crossings = case
                .crossings
                .iter()
                .map(|(wire, timestamp_dcs)| WireCrossingEvidence {
                    wire: *wire,
                    timestamp_dcs: *timestamp_dcs,
                    bracket_gap_ms: 100.0,
                    method: "finite_hook_plane_crossing",
                })
                .collect();
            observe_test_hook_sample(&mut track, case.deflection_time - 0.35, 1.0);
            observe_test_hook_sample(&mut track, case.deflection_time - 0.10, 1.0);
            observe_test_hook_sample(&mut track, case.deflection_time, 0.0);
            observe_test_hook_sample(&mut track, case.recovery_time, 1.0);

            let estimate = track.wire_estimate_at(case.touchdown_time);
            assert_eq!(estimate.wire, Some(case.expected_wire), "{}", case.module);
            assert_eq!(estimate.confidence, "high", "{}", case.module);
            assert_eq!(
                estimate.reason,
                "hook_deflection_correlated_with_wire_crossing"
            );
        }
    }

    #[test]
    fn stable_hook_up_and_unrecovered_transition_never_estimate_a_wire() {
        let carrier_info = CarrierInfo::by_type("CVN_71").unwrap();
        let plane_info = AirplaneInfo::by_type("F-14BU").unwrap();

        let mut stable_up = Track::new("pilot", carrier_info, plane_info);
        stable_up.entered_groove = true;
        stable_up.last_x = 100.0;
        stable_up.wire_crossings.push(WireCrossingEvidence {
            wire: 3,
            timestamp_dcs: 9.95,
            bracket_gap_ms: 100.0,
            method: "finite_hook_plane_crossing",
        });
        for time in [9.4, 9.7, 10.0, 10.3] {
            observe_test_hook_sample(&mut stable_up, time, 0.0);
        }
        assert_eq!(stable_up.wire_estimate_at(10.2).wire, None);

        let mut unrecovered = Track::new("pilot", carrier_info, plane_info);
        unrecovered.entered_groove = true;
        unrecovered.last_x = 100.0;
        unrecovered.wire_crossings = stable_up.wire_crossings.clone();
        observe_test_hook_sample(&mut unrecovered, 9.4, 1.0);
        observe_test_hook_sample(&mut unrecovered, 9.7, 1.0);
        observe_test_hook_sample(&mut unrecovered, 10.0, 0.0);
        observe_test_hook_sample(&mut unrecovered, 18.1, 1.0);
        assert_eq!(unrecovered.wire_estimate_at(10.2).wire, None);
    }

    #[test]
    fn wire_crossing_requires_groove_and_finite_pendant_proximity() {
        let carrier_info = CarrierInfo::by_type("CVN_71").unwrap();
        let plane_info = AirplaneInfo::by_type("FA-18C_hornet").unwrap();
        let carrier = Transform {
            forward: DVec3::unit_z(),
            ..Transform::default()
        };
        let midpoint = (carrier_info.cable3.0 + carrier_info.cable3.1) / 2.0;
        let hook_offset = plane_info.hook;
        let before = |time, offset| Transform {
            position: midpoint - hook_offset - DVec3::unit_z() + offset,
            time,
            ..Transform::default()
        };
        let after = |time, offset| Transform {
            position: midpoint - hook_offset + DVec3::unit_z() + offset,
            time,
            ..Transform::default()
        };
        let mut track = Track::new("pilot", carrier_info, plane_info);

        track.observe_wire_crossings(&carrier, &before(1.0, DVec3::zero()), 100.0);
        track.observe_wire_crossings(&carrier, &after(1.1, DVec3::zero()), 100.0);
        assert!(track.wire_crossings.is_empty());

        track.entered_groove = true;
        let outside_pendant = DVec3::new(100.0, 0.0, 0.0);
        track.observe_wire_crossings(&carrier, &before(2.0, outside_pendant), 100.0);
        track.observe_wire_crossings(&carrier, &after(2.1, outside_pendant), 100.0);
        let above_deck = DVec3::new(0.0, MAX_WIRE_VERTICAL_SEPARATION_M + 1.0, 0.0);
        track.observe_wire_crossings(&carrier, &before(3.0, above_deck), 100.0);
        track.observe_wire_crossings(&carrier, &after(3.1, above_deck), 100.0);
        assert!(track.wire_crossings.is_empty());

        track.observe_wire_crossings(&carrier, &before(4.0, DVec3::zero()), 100.0);
        track.observe_wire_crossings(&carrier, &after(4.1, DVec3::zero()), 100.0);
        assert_eq!(track.wire_crossings.len(), 1);
        assert_eq!(track.wire_crossings[0].wire, 3);
    }

    #[test]
    fn finish_applies_post_touchdown_hook_estimate_to_recovered_grading() {
        let mut track = Track::new(
            "pilot",
            CarrierInfo::by_type("CVN_71").unwrap(),
            AirplaneInfo::by_type("F-14BU").unwrap(),
        );
        track.entered_groove = true;
        track.last_x = 100.0;
        track.landing_time = Some(10.2);
        track.grading = Some(Grading::Recovered {
            cable: None,
            cable_estimated: None,
        });
        track.gate_deviations = GateDeviations {
            at_three_quarter_nm: Some(gate(9.0)),
            at_half_nm: Some(gate(9.5)),
            at_quarter_nm: Some(gate(9.8)),
            three_quarter_quality: valid_quality(),
            half_quality: valid_quality(),
            quarter_quality: valid_quality(),
        };
        track.wire_crossings.push(WireCrossingEvidence {
            wire: 3,
            timestamp_dcs: 9.95,
            bracket_gap_ms: 100.0,
            method: "finite_hook_plane_crossing",
        });
        observe_test_hook_sample(&mut track, 9.65, 1.0);
        observe_test_hook_sample(&mut track, 9.90, 1.0);
        observe_test_hook_sample(&mut track, 10.0, 0.0);
        observe_test_hook_sample(&mut track, 11.0, 1.0);

        let result = track.finish();
        assert_eq!(
            result.grading,
            Grading::Recovered {
                cable: None,
                cable_estimated: Some(3),
            }
        );
        assert_eq!(
            result.telemetry_quality.completeness,
            Completeness::Complete
        );
        assert_eq!(result.pass_grade, PassGrade::Ok);
    }

    fn observe_test_hook_sample(track: &mut Track, time: f64, raw: f64) {
        track.observe_hook_sample(time, 0, 0.0, Some(raw), HookSampleStatus::Success);
    }

    #[test]
    fn replay_and_live_paths_share_geometry_and_outcome_for_common_data() {
        let carrier_info = CarrierInfo::by_type("CVN_71").unwrap();
        let plane_info = AirplaneInfo::by_type("FA-18C_hornet").unwrap();
        let carrier = Transform {
            forward: DVec3::unit_z(),
            ..Transform::default()
        };
        let landing = carrier_info.approach_reference_offset(plane_info);
        let fb = DVec3::unit_z().rotated_by(DRotor3::from_rotation_xz(
            carrier_info.deck_angle.to_radians(),
        ));
        let mut replay = Track::new("pilot", carrier_info, plane_info);
        let mut live = Track::new("pilot", carrier_info, plane_info);
        let mut aligner = crate::telemetry::TelemetryAligner::new();

        for (index, distance) in [1_500.0, 1_300.0, 800.0, 400.0].into_iter().enumerate() {
            let time = 1.0 + index as f64 * 0.1;
            let mut carrier_frame = carrier.clone();
            carrier_frame.time = time;
            let altitude = distance * plane_info.glide_slope.to_radians().tan();
            let plane = Transform {
                time,
                position: landing - fb * distance,
                alt: altitude,
                ..Transform::default()
            };
            assert!(replay.next(&carrier_frame, &plane, Some(1.0)));
            let sample = aligner.align(
                crate::transform::ObservedTransform::now(carrier_frame),
                crate::transform::ObservedTransform::now(plane),
            );
            assert!(live.next_sample(&sample, Some(1.0)));
        }

        let replay = replay.finish();
        let live = live.finish();
        assert_eq!(replay.grading, live.grading);
        assert_eq!(replay.pass_grade, live.pass_grade);
        assert_eq!(replay.datums.len(), live.datums.len());
        for (replay, live) in replay.datums.iter().zip(&live.datums) {
            assert!((replay.x - live.x).abs() < 1.0e-9);
            assert!((replay.y - live.y).abs() < 1.0e-9);
            assert!((replay.alt - live.alt).abs() < 1.0e-9);
        }
    }

    #[test]
    fn intentional_bolter_is_preserved_for_arrested_recovery() {
        let grading = Grading::TouchAndGo {
            cable_estimated: Some(3),
        };

        assert_eq!(
            normalize_grading_for_recovery(grading, &CarrierRecovery::Arrested),
            Grading::TouchAndGo {
                cable_estimated: Some(3)
            }
        );
    }

    #[test]
    fn intentional_bolter_is_never_exposed_for_vstol_recovery() {
        let grading = Grading::TouchAndGo {
            cable_estimated: Some(3),
        };
        let recovery = CarrierRecovery::Vstol {
            landing_point: DVec3::zero(),
            approach_axis_port_m: 27.24,
            target_altitude_ft: 120.0,
        };

        assert_eq!(
            normalize_grading_for_recovery(grading, &recovery),
            Grading::WaveoffUnknown
        );
    }

    /// Feeds `count` in-groove hook samples at 10 Hz starting at `start`.
    fn hook_run(track: &mut Track, start: f64, count: usize, raw: f64) {
        for index in 0..count {
            track.observe_hook_sample(
                start + index as f64 * 0.1,
                index as u64,
                0.0,
                Some(raw),
                HookSampleStatus::Success,
            );
        }
    }

    #[test]
    fn commanded_hook_state_is_latched_for_every_validated_module() {
        let carrier = CarrierInfo::by_type("CVN_71").unwrap();
        for module in ["FA-18C_hornet", "T-45", "F-14B", "F-14BU", "F-14A-135-GR"] {
            let plane = AirplaneInfo::by_type(module).unwrap();

            let mut up = Track::new("pilot", carrier, plane);
            up.entered_groove = true;
            up.last_x = 400.0;
            hook_run(&mut up, 10.0, 12, 0.0);
            assert_eq!(up.commanded_hook_state(), HookState::Up, "{module}");
            assert_eq!(up.hook_observation.baseline_samples, 12);

            let mut down = Track::new("pilot", carrier, plane);
            down.entered_groove = true;
            down.last_x = 400.0;
            hook_run(&mut down, 10.0, 12, 1.0);
            assert_eq!(down.commanded_hook_state(), HookState::Down, "{module}");
        }

        let harrier = AirplaneInfo::by_type("AV8BNA").unwrap();
        let mut unknown = Track::new("pilot", carrier, harrier);
        unknown.entered_groove = true;
        hook_run(&mut unknown, 10.0, 12, 1.0);
        assert_eq!(unknown.commanded_hook_state(), HookState::Unknown);
        assert_eq!(
            unknown.hook_observation.baseline_reason,
            "no_validated_hook_argument"
        );
    }

    #[test]
    fn arrestment_excursion_does_not_flip_a_hook_down_baseline() {
        // Live corpus 2026-09-02: on real traps the external argument drops to
        // the "up" band 0.5-1.4 s before RunwayTouch and recovers seconds later.
        let carrier = CarrierInfo::by_type("CVN_71").unwrap();
        for module in ["T-45", "F-14BU"] {
            let plane = AirplaneInfo::by_type(module).unwrap();
            let mut track = Track::new("pilot", carrier, plane);
            track.entered_groove = true;
            track.last_x = 300.0;
            // Stable hook down from t=10.0 to t=18.6 ...
            hook_run(&mut track, 10.0, 87, 1.0);
            // ... excursion into the up band 1.4 s before the 20.0 s touchdown ...
            hook_run(&mut track, 18.6, 40, 0.0);
            track.landing_time = Some(20.0);
            // ... and recovery afterwards.
            hook_run(&mut track, 22.6, 10, 1.0);
            assert_eq!(track.commanded_hook_state(), HookState::Down, "{module}");
            assert!(track
                .hook_observation
                .baseline_end_dcs
                .is_some_and(|end| end <= 20.0 - HOOK_BASELINE_GUARD_S));
        }
    }

    #[test]
    fn post_touch_samples_never_rewrite_a_hook_up_baseline() {
        let carrier = CarrierInfo::by_type("CVN_71").unwrap();
        let hornet = AirplaneInfo::by_type("FA-18C_hornet").unwrap();
        let mut track = Track::new("pilot", carrier, hornet);
        track.entered_groove = true;
        track.last_x = 400.0;
        hook_run(&mut track, 10.0, 20, 0.0);
        track.landing_time = Some(13.0);
        hook_run(&mut track, 13.0, 10, 1.0);
        assert_eq!(track.commanded_hook_state(), HookState::Up);
        assert!(track
            .hook_observation
            .timeline
            .iter()
            .filter(|sample| sample.associated_time_dcs >= 13.0)
            .all(|sample| !sample.before_touchdown));
    }

    #[test]
    fn unstable_or_short_hook_baselines_stay_unknown() {
        let carrier = CarrierInfo::by_type("CVN_71").unwrap();
        let tomcat = AirplaneInfo::by_type("F-14B").unwrap();

        let mut short = Track::new("pilot", carrier, tomcat);
        short.entered_groove = true;
        short.last_x = 400.0;
        hook_run(&mut short, 10.0, 3, 0.0);
        assert_eq!(short.commanded_hook_state(), HookState::Unknown);
        assert_eq!(
            short.hook_observation.baseline_reason,
            "too_few_baseline_samples"
        );

        let mut flipping = Track::new("pilot", carrier, tomcat);
        flipping.entered_groove = true;
        flipping.last_x = 400.0;
        hook_run(&mut flipping, 10.0, 6, 0.0);
        hook_run(&mut flipping, 10.6, 6, 1.0);
        assert_eq!(flipping.commanded_hook_state(), HookState::Unknown);
        assert_eq!(
            flipping.hook_observation.baseline_reason,
            "baseline_not_stable"
        );
    }

    #[test]
    fn dcs_lso_comments_parse_grade_wire_and_waveoff() {
        let trap = parse_dcs_lso_grade("LSO: GRADE:--- : _SLOX_  WIRE# 4[BC]");
        assert_eq!(trap.grade.as_deref(), Some("---"));
        assert_eq!(trap.wire, Some(4));
        assert!(!trap.waveoff_ordered);

        let cut = parse_dcs_lso_grade("LSO: GRADE:C : _LULIC_  WO(AFU)TL  (EGIW)  WIRE# 3[BC]");
        assert_eq!(cut.grade.as_deref(), Some("C"));
        assert_eq!(cut.wire, Some(3));
        assert!(cut.waveoff_ordered);

        let waveoff = parse_dcs_lso_grade("LSO: GRADE:WO  _DRX_  _LOIC_  WOFDIC [BC]");
        assert_eq!(waveoff.grade.as_deref(), Some("WO"));
        assert_eq!(waveoff.wire, None);
        assert!(waveoff.waveoff_ordered);

        let perfect = parse_dcs_lso_grade("LSO: GRADE:_OK_ : WIRE# 3");
        assert_eq!(perfect.grade.as_deref(), Some("_OK_"));
        assert_eq!(perfect.wire, Some(3));

        assert_eq!(parse_dcs_wire("__H__IC WIRE #2"), Some(2));
        assert_eq!(parse_dcs_wire("WIRE#1"), Some(1));
        assert_eq!(parse_dcs_wire("_WX_ (NX) _DRX_"), None);
        assert!(!parse_dcs_lso_grade("LSO: GRADE:--- : _WX_  (NX)  WIRE# 3").waveoff_ordered);
    }

    /// Appends a deck sample whose carrier-relative position advances at
    /// `relative_speed_mps` along +z since the previous sample.
    fn deck_sample(track: &mut Track, time: f64, relative_speed_mps: f64, x: f64) {
        let relative_position = match track.deck_kinematics.last() {
            Some(previous) => {
                previous.relative_position
                    + DVec3::new(0.0, 0.0, relative_speed_mps * (time - previous.time))
            }
            None => DVec3::zero(),
        };
        track.deck_kinematics.push(DeckKinematicSample {
            time,
            relative_position,
            x,
        });
    }

    #[test]
    fn deck_kinematics_confirm_an_arrest_only_when_the_aircraft_stops_and_holds() {
        let carrier = CarrierInfo::by_type("CVN_71").unwrap();
        let plane = AirplaneInfo::by_type("T-45").unwrap();

        // Campaign B pass 4: relative speed ~49 m/s at touchdown, below 5 m/s
        // after 2 s, then moving with the deck.
        let mut arrested = Track::new("pilot", carrier, plane);
        arrested.landing_time = Some(100.0);
        for step in 0..15 {
            deck_sample(
                &mut arrested,
                98.5 + step as f64 * 0.1,
                49.0,
                60.0 - step as f64 * 4.9,
            );
        }
        for step in 0..60 {
            let time = 100.0 + step as f64 * 0.1;
            let speed = (49.0 - step as f64 * 2.5).max(0.5);
            deck_sample(&mut arrested, time, speed, -step as f64 * 2.0);
        }
        let evidence = arrested.evaluate_arrest_kinematics();
        assert!(evidence.confirmed, "{evidence:?}");
        assert_eq!(evidence.reason, "confirmed");
        assert!(evidence.held_s.is_some_and(|held| held >= ARREST_HOLD_S));

        // Bolter: never below ~45 m/s.
        let mut bolter = Track::new("pilot", carrier, plane);
        bolter.landing_time = Some(100.0);
        for step in 0..15 {
            deck_sample(
                &mut bolter,
                98.5 + step as f64 * 0.1,
                47.0,
                60.0 - step as f64 * 4.7,
            );
        }
        for step in 0..40 {
            deck_sample(
                &mut bolter,
                100.0 + step as f64 * 0.1,
                47.0,
                -step as f64 * 4.7,
            );
        }
        let evidence = bolter.evaluate_arrest_kinematics();
        assert!(!evidence.confirmed);
        assert_eq!(evidence.reason, "never_slow_within_window");

        // Slow for one second then a telemetry gap: not confirmed.
        let mut gapped = Track::new("pilot", carrier, plane);
        gapped.landing_time = Some(100.0);
        for step in 0..15 {
            deck_sample(&mut gapped, 98.5 + step as f64 * 0.1, 2.0, -50.0);
        }
        for step in 0..10 {
            deck_sample(&mut gapped, 100.0 + step as f64 * 0.1, 2.0, -50.0);
        }
        deck_sample(&mut gapped, 103.0, 1.0, -50.0);
        let evidence = gapped.evaluate_arrest_kinematics();
        assert!(!evidence.confirmed);
        assert_eq!(evidence.reason, "telemetry_gap_in_arrest_window");

        // No contact reference at all.
        let mut idle = Track::new("pilot", carrier, plane);
        deck_sample(&mut idle, 1.0, 0.0, 0.0);
        assert_eq!(
            idle.evaluate_arrest_kinematics().reason,
            "no_contact_reference"
        );
    }

    #[test]
    fn kinematic_arrest_without_a_wire_is_complete_and_named_as_such() {
        let carrier = CarrierInfo::by_type("CVN_71").unwrap();
        let plane = AirplaneInfo::by_type("T-45").unwrap();
        let mut track = Track::new("pilot", carrier, plane);
        track.entered_groove = true;
        track.landing_time = Some(100.0);
        track.grading = Some(Grading::Recovered {
            cable: None,
            cable_estimated: None,
        });
        for step in 0..15 {
            deck_sample(
                &mut track,
                98.5 + step as f64 * 0.1,
                49.0,
                60.0 - step as f64 * 4.9,
            );
        }
        track.gate_deviations = GateDeviations {
            at_three_quarter_nm: Some(gate(90.0)),
            at_half_nm: Some(gate(95.0)),
            at_quarter_nm: Some(gate(98.0)),
            three_quarter_quality: valid_quality(),
            half_quality: valid_quality(),
            quarter_quality: valid_quality(),
        };
        for step in 0..60 {
            let time = 100.0 + step as f64 * 0.1;
            deck_sample(
                &mut track,
                time,
                (49.0 - step as f64 * 2.5).max(0.5),
                -step as f64 * 2.0,
            );
        }

        let result = track.finish();
        assert_eq!(result.arrest_evidence, "kinematic");
        assert_eq!(
            result.telemetry_quality.completeness,
            Completeness::Complete
        );
        assert_eq!(result.pass_grade, PassGrade::Ok);
        assert!(result.arrest_kinematics.confirmed);
    }

    #[test]
    fn dcs_waveoff_is_a_wo_grade_when_complied_with_and_a_cut_when_ignored() {
        let carrier = CarrierInfo::by_type("CVN_71").unwrap();
        let plane = AirplaneInfo::by_type("F-14BU").unwrap();

        let mut complied = Track::new("pilot", carrier, plane);
        complied.entered_groove = true;
        assert!(complied.set_dcs_grading("LSO: GRADE:WO  _LOIC_  WO(AFU)IC [BC]".to_string()));
        let result = complied.finish();
        assert_eq!(result.grading, Grading::WaveoffDcs);
        assert_eq!(result.pass_grade, PassGrade::Waveoff);
        assert_eq!(result.grade_points, Some(1.0));

        let mut ignored = Track::new("pilot", carrier, plane);
        ignored.entered_groove = true;
        ignored.grading = Some(Grading::TouchAndGo {
            cable_estimated: None,
        });
        ignored.gate_deviations = GateDeviations {
            at_three_quarter_nm: Some(gate(1.0)),
            at_half_nm: Some(gate(2.0)),
            at_quarter_nm: Some(gate(3.0)),
            three_quarter_quality: valid_quality(),
            half_quality: valid_quality(),
            quarter_quality: valid_quality(),
        };
        assert!(
            ignored.set_dcs_grading("LSO: GRADE:WO  LULX  WO(AFU)TL  WO(AFU)IC [BC]".to_string())
        );
        let result = ignored.finish();
        assert_eq!(
            result.grading,
            Grading::TouchAndGo {
                cable_estimated: None
            }
        );
        assert_eq!(result.pass_grade, PassGrade::Cut);
        assert_eq!(result.grade_points, Some(0.0));
    }

    #[test]
    fn outbound_reset_clears_previous_approach_wire_evidence() {
        let carrier_info = CarrierInfo::by_type("CVN_71").unwrap();
        let plane_info = AirplaneInfo::by_type("FA-18C_hornet").unwrap();
        let carrier = Transform {
            forward: DVec3::unit_z(),
            ..Transform::default()
        };
        let landing = carrier_info.approach_reference_offset(plane_info);
        let fb = DVec3::unit_z().rotated_by(DRotor3::from_rotation_xz(
            carrier_info.deck_angle.to_radians(),
        ));
        let mut track = Track::new("pilot", carrier_info, plane_info);
        track.entered_groove = true;
        track.wire_crossings.push(WireCrossingEvidence {
            wire: 4,
            timestamp_dcs: 1.0,
            bracket_gap_ms: 100.0,
            method: "finite_hook_plane_crossing",
        });
        track.deck_crossing_time = Some(1.0);

        // Fly back out past the 3/4 nm gate.
        let outbound = Transform {
            time: 30.0,
            position: landing - fb * (GATE_THREE_QUARTER_NM + 200.0),
            alt: 150.0,
            ..Transform::default()
        };
        assert!(track.next(&carrier, &outbound, None));
        assert!(track.wire_crossings.is_empty());
        assert!(!track.entered_groove);
        assert_eq!(track.deck_crossing_time, None);
    }

    #[test]
    fn hook_timeline_compaction_preserves_recent_final_evidence_without_buffer_limit() {
        let carrier = CarrierInfo::by_type("CVN_71").unwrap();
        let hornet = AirplaneInfo::by_type("FA-18C_hornet").unwrap();
        let mut track = Track::new("pilot", carrier, hornet);

        for index in 0..MAX_HOOK_EVIDENCE {
            track.observe_hook_sample(
                index as f64 * 0.25,
                index as u64,
                0.0,
                Some(0.0),
                HookSampleStatus::Success,
            );
        }

        track.entered_groove = true;
        track.last_x = 400.0;
        for index in 0..6 {
            let sequence = MAX_HOOK_EVIDENCE + index;
            track.observe_hook_sample(
                sequence as f64 * 0.25,
                sequence as u64,
                0.0,
                Some(1.0),
                HookSampleStatus::Success,
            );
        }

        assert_eq!(track.hook_observation.timeline.len(), MAX_HOOK_EVIDENCE);
        assert_eq!(track.hook_observation.compacted_samples, 6);
        assert_eq!(
            track
                .hook_observation
                .timeline
                .front()
                .unwrap()
                .observed_unix_ms,
            6
        );
        assert_eq!(
            track.hook_observation.timeline.back().unwrap().raw,
            Some(1.0)
        );
        assert_eq!(track.commanded_hook_state(), HookState::Down);
        assert_ne!(
            track.telemetry_quality.completeness,
            Completeness::BufferLimit
        );
        assert_eq!(track.telemetry_quality.dropped_samples, 0);
    }
}
