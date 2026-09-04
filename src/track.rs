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
/// Outside the scoring-relevant window (before groove entry and beyond the
/// ¾ nm / 500 ft envelope), only one in this many samples is kept in `datums`.
/// The scoring zone itself is always recorded at full rate; this only trims
/// the pattern/break portion, which the JSON report keeps solely for the
/// pattern chart, waveoff diagnosis and telemetry-quality accounting, never
/// for gate/grading evidence.
const PATTERN_DATUM_STRIDE: u32 = 4;
const MAX_EVENT_EVIDENCE: usize = 256;
const MAX_HOOK_EVIDENCE: usize = 512;
const GATE_BUFFER_WINDOW_S: f64 = 2.0;
const HEALTH_WINDOW_S: f64 = 10.0;
/// PROJECT-DERIVED provisional observation radius. It is informational only
/// until the Tarawa spot geometry is validated against the future live corpus.
const VSTOL_SPOT_OBSERVATION_RADIUS_M: f64 = 15.0;

#[derive(Debug, Clone, Default, PartialEq, serde::Serialize)]
pub struct Datum {
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub grpc_code: Option<String>,
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
    pub crossings: Vec<WireCrossingEvidence>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CalibratedHookState {
    Up,
    Down,
    Unknown,
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
    pub interpreted_state: &'static str,
    pub timeline: VecDeque<HookSampleEvidence>,
    /// Calibration is module-specific; unknown modules are never inferred.
    pub polarity: &'static str,
}

pub struct Track {
    pilot_name: String,
    previous_distance: f64,
    previous_x: f64,
    previous_sample_time: Option<f64>,
    gate_samples: VecDeque<ApproachSample>,
    datums: Vec<Datum>,
    /// Counts samples recorded outside the scoring-relevant window, used to
    /// subsample `datums` there (see `PATTERN_DATUM_STRIDE`).
    pattern_datum_counter: u32,
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
    recent_health_samples: VecDeque<(f64, f64)>,
    telemetry_gap_stats: OnlineMetricStats,
    first_sample_time: Option<f64>,
    last_sample_time: Option<f64>,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bracket_gap_ms: Option<f64>,
}

impl Default for GateQuality {
    fn default() -> Self {
        Self {
            status: GateStatus::Missing,
            reason: Some("not_observed".to_string()),
            bracket_gap_ms: None,
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

impl Completeness {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Complete => "complete",
            Self::InsufficientGates => "insufficient_gates",
            Self::TelemetryGap => "telemetry_gap",
            Self::InvalidTelemetry => "invalid_telemetry",
            Self::UnconfirmedArrest => "unconfirmed_arrest",
            Self::BufferLimit => "buffer_limit",
        }
    }

    const fn priority(self) -> u8 {
        match self {
            Self::BufferLimit => 0,
            Self::TelemetryGap => 1,
            Self::InvalidTelemetry => 2,
            Self::InsufficientGates => 3,
            Self::UnconfirmedArrest => 4,
            Self::Complete => u8::MAX,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TelemetryHealth {
    #[default]
    Green,
    Orange,
    Red,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticCause {
    HookHistoryTruncated,
    EventHistoryTruncated,
    EventStreamUnavailable,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct PositionCollectionMetrics {
    pub polls: u32,
    pub errors: u32,
    pub timeouts: u32,
    pub mean_latency_ms: f64,
    pub p50_latency_ms: f64,
    pub p95_latency_ms: f64,
    pub p99_latency_ms: f64,
    pub max_latency_ms: f64,
}

const METRIC_HISTOGRAM_MAX_MS: usize = 10_000;

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct OnlineMetricStats {
    bins: Vec<u32>,
    count: u64,
    sum: f64,
    max: f64,
    above_warning: u64,
}

impl Default for OnlineMetricStats {
    fn default() -> Self {
        Self {
            bins: vec![0; METRIC_HISTOGRAM_MAX_MS + 1],
            count: 0,
            sum: 0.0,
            max: 0.0,
            above_warning: 0,
        }
    }
}

impl OnlineMetricStats {
    pub(crate) fn observe(&mut self, value_ms: f64) {
        if !value_ms.is_finite() || value_ms < 0.0 {
            return;
        }
        let bin = value_ms.round().min(METRIC_HISTOGRAM_MAX_MS as f64) as usize;
        self.bins[bin] = self.bins[bin].saturating_add(1);
        self.count = self.count.saturating_add(1);
        self.sum += value_ms;
        self.max = self.max.max(value_ms);
        if value_ms > SAMPLE_GAP_WARNING_MS {
            self.above_warning = self.above_warning.saturating_add(1);
        }
    }

    pub(crate) fn count(&self) -> u64 {
        self.count
    }

    pub(crate) fn mean(&self) -> f64 {
        if self.count == 0 {
            0.0
        } else {
            self.sum / self.count as f64
        }
    }

    pub(crate) fn max(&self) -> f64 {
        self.max
    }

    pub(crate) fn ratio_above_warning(&self) -> f64 {
        if self.count == 0 {
            0.0
        } else {
            self.above_warning as f64 / self.count as f64
        }
    }

    pub(crate) fn percentile(&self, quantile: f64) -> f64 {
        if self.count == 0 {
            return 0.0;
        }
        let rank = (((self.count - 1) as f64 * quantile).ceil() as u64) + 1;
        let mut cumulative = 0_u64;
        for (value_ms, count) in self.bins.iter().enumerate() {
            cumulative += u64::from(*count);
            if cumulative >= rank {
                return value_ms as f64;
            }
        }
        METRIC_HISTOGRAM_MAX_MS as f64
    }
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
    pub dropped_position_samples: u32,
    pub dropped_hook_samples: u32,
    pub dropped_event_samples: u32,
    pub sample_count: u32,
    pub effective_frequency_hz: f64,
    pub degraded_sample_ratio: f64,
    pub gap_p50_ms: f64,
    pub gap_p90_ms: f64,
    pub gap_p95_ms: f64,
    pub gap_p99_ms: f64,
    pub max_source_age_ms: f64,
    pub position_polls: u32,
    pub position_poll_errors: u32,
    pub position_poll_timeouts: u32,
    pub position_poll_mean_latency_ms: f64,
    pub position_poll_p50_latency_ms: f64,
    pub position_poll_p95_latency_ms: f64,
    pub position_poll_p99_latency_ms: f64,
    pub position_poll_max_latency_ms: f64,
    pub reasons: Vec<TelemetryInvalidReason>,
    pub diagnostics: Vec<DiagnosticCause>,
    pub unavailability_causes: Vec<Completeness>,
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
            dropped_position_samples: 0,
            dropped_hook_samples: 0,
            dropped_event_samples: 0,
            sample_count: 0,
            effective_frequency_hz: 0.0,
            degraded_sample_ratio: 0.0,
            gap_p50_ms: 0.0,
            gap_p90_ms: 0.0,
            gap_p95_ms: 0.0,
            gap_p99_ms: 0.0,
            max_source_age_ms: 0.0,
            position_polls: 0,
            position_poll_errors: 0,
            position_poll_timeouts: 0,
            position_poll_mean_latency_ms: 0.0,
            position_poll_p50_latency_ms: 0.0,
            position_poll_p95_latency_ms: 0.0,
            position_poll_p99_latency_ms: 0.0,
            position_poll_max_latency_ms: 0.0,
            reasons: Vec::new(),
            diagnostics: Vec::new(),
            unavailability_causes: Vec::new(),
        }
    }
}

impl TelemetryQuality {
    fn add_unavailability_cause(&mut self, cause: Completeness) {
        if cause == Completeness::Complete {
            return;
        }
        if !self.unavailability_causes.contains(&cause) {
            self.unavailability_causes.push(cause);
            self.unavailability_causes
                .sort_by_key(|cause| cause.priority());
        }
        self.completeness = self.unavailability_causes[0];
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
    Recovered {
        cable: Option<u8>,
        cable_estimated: Option<u8>,
    },
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
            datums: Default::default(),
            pattern_datum_counter: 0,
            pattern_datums: Default::default(),
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
                polarity: if plane_info.name == "F/A-18C Hornet" {
                    "fa18c_zero_up_one_down_test_corpus"
                } else {
                    "unknown_pending_live_validation"
                },
                interpreted_state: "unknown",
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
            recent_health_samples: VecDeque::new(),
            telemetry_gap_stats: OnlineMetricStats::default(),
            first_sample_time: None,
            last_sample_time: None,
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
        let sample_time = carrier.time.max(plane.time);
        let observed_gap_ms = sample.sample_gap_ms.max(sample.source_age_ms);
        self.previous_sample_time = Some(sample_time);
        self.first_sample_time.get_or_insert(sample_time);
        self.last_sample_time = Some(sample_time);
        self.telemetry_gap_stats.observe(observed_gap_ms);
        self.telemetry_quality.sample_count =
            self.telemetry_gap_stats.count().min(u64::from(u32::MAX)) as u32;
        self.telemetry_quality.max_source_age_ms = self
            .telemetry_quality
            .max_source_age_ms
            .max(sample.source_age_ms);
        self.recent_health_samples
            .push_back((sample_time, observed_gap_ms));
        while self
            .recent_health_samples
            .front()
            .is_some_and(|(time, _)| sample_time - time > HEALTH_WINDOW_S)
        {
            self.recent_health_samples.pop_front();
        }
        self.telemetry_quality.max_sample_gap_ms = self
            .telemetry_quality
            .max_sample_gap_ms
            .max(observed_gap_ms);
        self.telemetry_quality.max_skew_ms = self.telemetry_quality.max_skew_ms.max(sample.skew_ms);
        if sample.has_warning() {
            self.telemetry_quality.warning_samples += 1;
        }
        let window_span_s = self
            .recent_health_samples
            .front()
            .map_or(0.0, |(time, _)| sample_time - time);
        let window_frequency_hz = if window_span_s > 0.0 {
            (self.recent_health_samples.len().saturating_sub(1)) as f64 / window_span_s
        } else {
            0.0
        };
        let degraded_ratio = if self.recent_health_samples.is_empty() {
            0.0
        } else {
            self.recent_health_samples
                .iter()
                .filter(|(_, gap)| *gap > SAMPLE_GAP_WARNING_MS)
                .count() as f64
                / self.recent_health_samples.len() as f64
        };
        let (current_health, current_health_reason) = if sample.invalid_reason.is_some()
            || sample.sample_gap_ms > crate::telemetry::SAMPLE_GAP_INCOMPLETE_MS
            || sample.source_age_ms > crate::telemetry::SAMPLE_GAP_INCOMPLETE_MS
        {
            (TelemetryHealth::Red, "invalid_or_incomplete_sample")
        } else if window_span_s >= 5.0 && (window_frequency_hz < 6.0 || degraded_ratio >= 0.15) {
            (TelemetryHealth::Red, "sustained_gate_capture_risk")
        } else if window_span_s >= 5.0 && (window_frequency_hz < 8.0 || degraded_ratio >= 0.05) {
            (TelemetryHealth::Orange, "degraded_window_cadence")
        } else if sample.has_warning() {
            (TelemetryHealth::Orange, "degraded_cadence_or_freshness")
        } else {
            (TelemetryHealth::Green, "nominal")
        };
        if health_rank(current_health) >= health_rank(self.telemetry_quality.health) {
            self.telemetry_quality.health = current_health;
            self.telemetry_quality.health_reason = current_health_reason;
        }
        if current_health == TelemetryHealth::Red && !self.health_red_announced {
            tracing::warn!(
                before_groove = !self.entered_groove,
                health_reason = current_health_reason,
                window_frequency_hz,
                degraded_ratio,
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
                self.telemetry_quality.dropped_position_samples += 1;
                self.telemetry_quality
                    .add_unavailability_cause(Completeness::BufferLimit);
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
                    if self.calibrated_hook_state() == CalibratedHookState::Up {
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
                    // A deck crossing without an arrest is a bolter. Hook draw
                    // arguments are retained as raw evidence but not interpreted
                    // until polarity is validated for the deployed modules.
                    if self.crossed_deck_threshold && self.min_distance_state.is_some() {
                        if self.calibrated_hook_state() == CalibratedHookState::Up {
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

        // Already landed, no need to actually record any more datums, but keep going to detect
        // bolters.
        if self.grading.is_some() {
            return true;
        }

        // Construct the x axis, which is aligned to the angled deck.
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
                let cause = match reason {
                    TelemetryInvalidReason::TelemetryGap => Completeness::TelemetryGap,
                    _ => Completeness::InvalidTelemetry,
                };
                self.telemetry_quality.add_unavailability_cause(cause);
            } else {
                self.telemetry_quality.pattern_invalid_samples += 1;
            }
        }

        // Gate sampling and groove entry only apply when the aircraft is on the approach side of
        // the threshold (x > 0).  When x ≤ 0 the aircraft is ahead of the touchdown point
        // (e.g., still in the break or flying the overhead pattern), and atan2 with a negative x
        // would produce a bogus ~177° deviation reading.
        if sample.is_valid() && self.previous_x > 0.0 && x <= 0.0 {
            self.crossed_deck_threshold = true;
        }

        if x > 0.0 {
            // Robust reset: if the aircraft flies outbound (e.g., into the pattern after a bolter),
            // clear any gates or groove entry that were captured so they can be freshly recorded
            // on the real final approach inbound.
            if x > GATE_THREE_QUARTER_NM {
                self.gate_deviations.at_three_quarter_nm = None;
                self.gate_deviations.three_quarter_quality = GateQuality::default();
                self.groove_entry_time = None;
                self.entered_groove = false;
                self.crossed_deck_threshold = false;
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
                if !self.entered_groove {
                    self.groove_entry_time = Some(plane.time);
                    // Start a new wire-evidence segment for the final inbound
                    // branch. An earlier overhead/pattern deck crossing must not
                    // reserve a wire number for the actual recovery.
                    self.previous_wire_plane = [None; 4];
                    self.wire_crossings.clear();
                }
                self.entered_groove = true;
            }
        }

        // Subsample only the non-scoring pattern portion. The skip below is a
        // deliberate report-size reduction, not data loss, so it must never
        // touch the dropped/BufferLimit counters that describe real capacity
        // loss (those still fire below if the scoring-relevant window itself
        // ever exceeds MAX_TRACK_SAMPLES).
        let record_this_datum = if scoring_relevant {
            true
        } else {
            let keep = self
                .pattern_datum_counter
                .is_multiple_of(PATTERN_DATUM_STRIDE);
            self.pattern_datum_counter = self.pattern_datum_counter.wrapping_add(1);
            keep
        };

        if record_this_datum && self.datums.len() < MAX_TRACK_SAMPLES {
            self.datums.push(Datum {
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
            });
        } else if record_this_datum {
            self.telemetry_quality.dropped_samples += 1;
            self.telemetry_quality.dropped_position_samples += 1;
            self.telemetry_quality
                .add_unavailability_cause(Completeness::BufferLimit);
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
        // If the plane entered the groove but never landed and no other grading was set,
        // it performed a waveoff.
        if self.grading.is_none() && self.entered_groove {
            self.grading = Some(Grading::WaveoffUnknown);
        }

        let wire_estimation = self.wire_estimate_at(
            self.landing_time
                .or_else(|| self.datums.last().map(|datum| datum.time))
                .unwrap_or_default(),
        );

        // If DCS grading is set, use its reported wire for arrested recoveries only.
        let grading = if matches!(&self.carrier_info.recovery, CarrierRecovery::Arrested) {
            if let Some(dcs_wire) = self.dcs_grading.as_deref().and_then(parse_dcs_wire) {
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

        if !self.gate_deviations.all_valid() {
            self.telemetry_quality
                .add_unavailability_cause(Completeness::InsufficientGates);
        }
        if matches!(self.carrier_info.recovery, CarrierRecovery::Arrested)
            && matches!(grading, Grading::Recovered { cable: None, .. })
        {
            // RunwayTouch/Land prove contact, not an arrest. Until sustained
            // kinematics or a DCS wire/LQM confirms the trap, the pass cannot
            // receive a favourable grade.
            self.telemetry_quality
                .add_unavailability_cause(Completeness::UnconfirmedArrest);
        }
        if self.telemetry_quality.completeness != Completeness::Complete {
            pass_grade = PassGrade::Incomplete;
            grade_points = None;
        }

        self.telemetry_quality.gap_p50_ms = self.telemetry_gap_stats.percentile(0.50);
        self.telemetry_quality.gap_p90_ms = self.telemetry_gap_stats.percentile(0.90);
        self.telemetry_quality.gap_p95_ms = self.telemetry_gap_stats.percentile(0.95);
        self.telemetry_quality.gap_p99_ms = self.telemetry_gap_stats.percentile(0.99);
        self.telemetry_quality.degraded_sample_ratio =
            self.telemetry_gap_stats.ratio_above_warning();
        self.telemetry_quality.effective_frequency_hz =
            match (self.first_sample_time, self.last_sample_time) {
                (Some(first), Some(last))
                    if last > first && self.telemetry_gap_stats.count() > 1 =>
                {
                    (self.telemetry_gap_stats.count() - 1) as f64 / (last - first)
                }
                _ => 0.0,
            };

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
        }
    }

    /// Set the track's dcs grading.
    pub fn set_dcs_grading(&mut self, dcs_grading: String) -> bool {
        if self.dcs_grading.is_none() {
            self.dcs_grading = Some(dcs_grading);
            true
        } else {
            false
        }
    }

    fn observe_wire_crossings(
        &mut self,
        carrier: &Transform,
        plane: &Transform,
        bracket_gap_ms: f64,
    ) {
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
                        method: "hook_plane_crossing",
                    });
                }
            }
            self.previous_wire_plane[index] = Some((signed_distance, plane.time));
        }
    }

    fn wire_estimate_at(&self, event_time: f64) -> WireEstimateEvidence {
        let mut eligible = self
            .wire_crossings
            .iter()
            .filter(|crossing| {
                crossing.timestamp_dcs <= event_time
                    && crossing.bracket_gap_ms <= SAMPLE_GAP_WARNING_MS
            })
            .cloned()
            .collect::<Vec<_>>();
        eligible.sort_by(|left, right| left.timestamp_dcs.total_cmp(&right.timestamp_dcs));
        tracing::debug!(event_time, crossings = ?eligible, "wire crossing evidence at event");
        let Some(last) = eligible.last() else {
            return WireEstimateEvidence {
                wire: None,
                confidence: "insufficient",
                reason: "no_fresh_hook_plane_crossing",
                crossings: self.wire_crossings.clone(),
            };
        };
        let event_lag_ms = (event_time - last.timestamp_dcs) * 1_000.0;
        // A late RunwayTouch position is not moved backwards by a magic offset.
        // If the event does not closely correlate with a continuously observed
        // crossing, keep every crossing as evidence but decline to name a wire.
        if !(0.0..=SAMPLE_GAP_WARNING_MS).contains(&event_lag_ms) {
            return WireEstimateEvidence {
                wire: None,
                confidence: "insufficient",
                reason: "wire_crossing_not_time_correlated_with_event",
                crossings: self.wire_crossings.clone(),
            };
        }
        WireEstimateEvidence {
            wire: Some(last.wire),
            confidence: if last.bracket_gap_ms <= 150.0 && event_lag_ms <= 150.0 {
                "high"
            } else {
                "medium"
            },
            reason: "continuous_hook_plane_crossing",
            crossings: self.wire_crossings.clone(),
        }
    }

    pub fn observe_hook_sample(
        &mut self,
        associated_time_dcs: f64,
        observed_unix_ms: u64,
        age_ms: f64,
        raw: Option<f64>,
        status: HookSampleStatus,
    ) {
        self.observe_hook_sample_with_error(
            associated_time_dcs,
            observed_unix_ms,
            age_ms,
            raw,
            status,
            None,
        );
    }

    pub fn observe_hook_sample_with_error(
        &mut self,
        associated_time_dcs: f64,
        observed_unix_ms: u64,
        age_ms: f64,
        raw: Option<f64>,
        status: HookSampleStatus,
        grpc_code: Option<String>,
    ) {
        if !matches!(self.carrier_info.recovery, CarrierRecovery::Arrested) {
            return;
        }
        let in_groove = self.entered_groove;
        let in_final_window = in_groove && self.previous_x <= GATE_QUARTER_NM;
        let before_touchdown = self.landing_time.is_none();
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
            self.telemetry_quality.dropped_samples += 1;
            self.telemetry_quality.dropped_hook_samples += 1;
            if !self
                .telemetry_quality
                .diagnostics
                .contains(&DiagnosticCause::HookHistoryTruncated)
            {
                self.telemetry_quality
                    .diagnostics
                    .push(DiagnosticCause::HookHistoryTruncated);
            }
        }
        self.hook_observation
            .timeline
            .push_back(HookSampleEvidence {
                associated_time_dcs,
                observed_unix_ms,
                age_ms,
                raw,
                status,
                grpc_code,
                in_groove,
                in_final_window,
                before_touchdown,
            });
        self.hook_observation.interpreted_state = match self.calibrated_hook_state() {
            CalibratedHookState::Up => "up",
            CalibratedHookState::Down => "down",
            CalibratedHookState::Unknown => "unknown",
        };
    }

    fn calibrated_hook_state(&self) -> CalibratedHookState {
        if self.plane_info.name != "F/A-18C Hornet" {
            return CalibratedHookState::Unknown;
        }
        let valid = self
            .hook_observation
            .timeline
            .iter()
            .filter(|sample| {
                sample.status == HookSampleStatus::Success
                    && sample.in_final_window
                    && sample.before_touchdown
            })
            .collect::<Vec<_>>();
        let Some(latest) = valid.last() else {
            return CalibratedHookState::Unknown;
        };
        let latest_state = match latest.raw {
            Some(raw) if raw <= 0.2 => CalibratedHookState::Up,
            Some(raw) if raw >= 0.8 => CalibratedHookState::Down,
            _ => return CalibratedHookState::Unknown,
        };
        let recent_start = latest.associated_time_dcs - 3.0;
        let stable = valid
            .iter()
            .rev()
            .take_while(|sample| sample.associated_time_dcs >= recent_start)
            .take_while(|sample| match (latest_state, sample.raw) {
                (CalibratedHookState::Up, Some(raw)) => raw <= 0.2,
                (CalibratedHookState::Down, Some(raw)) => raw >= 0.8,
                _ => false,
            })
            .collect::<Vec<_>>();
        let duration = stable.last().map_or(0.0, |first| {
            latest.associated_time_dcs - first.associated_time_dcs
        });
        match latest_state {
            CalibratedHookState::Down if stable.len() >= 2 && duration >= 0.2 => {
                CalibratedHookState::Down
            }
            CalibratedHookState::Up if stable.len() >= 3 && duration >= 0.4 => {
                CalibratedHookState::Up
            }
            _ => CalibratedHookState::Unknown,
        }
    }

    pub fn mark_telemetry_gap(&mut self, reason: TelemetryInvalidReason) {
        self.telemetry_quality.invalid_samples += 1;
        if self.entered_groove
            || (self.previous_x > 0.0 && self.previous_x <= GATE_THREE_QUARTER_NM)
        {
            self.telemetry_quality.scoring_invalid_samples += 1;
            self.telemetry_quality
                .add_unavailability_cause(Completeness::TelemetryGap);
        } else {
            self.telemetry_quality.pattern_invalid_samples += 1;
        }
        if !self.telemetry_quality.reasons.contains(&reason) {
            self.telemetry_quality.reasons.push(reason);
        }
    }

    pub fn mark_source_buffer_loss(&mut self, lost_samples: u64) {
        let lost = lost_samples.min(u64::from(u32::MAX)) as u32;
        self.telemetry_quality.dropped_samples =
            self.telemetry_quality.dropped_samples.saturating_add(lost);
        self.telemetry_quality.dropped_position_samples = self
            .telemetry_quality
            .dropped_position_samples
            .saturating_add(lost);
        if self.entered_groove
            || (self.previous_x > 0.0 && self.previous_x <= GATE_THREE_QUARTER_NM)
        {
            self.telemetry_quality
                .add_unavailability_cause(Completeness::BufferLimit);
        }
    }

    pub fn mark_invalid_source_observations(&mut self, invalid_samples: u64) {
        let invalid = invalid_samples.min(u64::from(u32::MAX)) as u32;
        self.telemetry_quality.invalid_samples = self
            .telemetry_quality
            .invalid_samples
            .saturating_add(invalid);
        if self.entered_groove
            || (self.previous_x > 0.0 && self.previous_x <= GATE_THREE_QUARTER_NM)
        {
            self.telemetry_quality.scoring_invalid_samples = self
                .telemetry_quality
                .scoring_invalid_samples
                .saturating_add(invalid);
            self.telemetry_quality
                .add_unavailability_cause(Completeness::InvalidTelemetry);
        } else {
            self.telemetry_quality.pattern_invalid_samples = self
                .telemetry_quality
                .pattern_invalid_samples
                .saturating_add(invalid);
        }
    }

    pub fn set_position_collector_metrics(&mut self, metrics: PositionCollectionMetrics) {
        self.telemetry_quality.position_polls = metrics.polls;
        self.telemetry_quality.position_poll_errors = metrics.errors;
        self.telemetry_quality.position_poll_timeouts = metrics.timeouts;
        self.telemetry_quality.position_poll_mean_latency_ms = metrics.mean_latency_ms;
        self.telemetry_quality.position_poll_p50_latency_ms = metrics.p50_latency_ms;
        self.telemetry_quality.position_poll_p95_latency_ms = metrics.p95_latency_ms;
        self.telemetry_quality.position_poll_p99_latency_ms = metrics.p99_latency_ms;
        self.telemetry_quality.position_poll_max_latency_ms = metrics.max_latency_ms;
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
            self.telemetry_quality.dropped_event_samples += 1;
            if !self
                .telemetry_quality
                .diagnostics
                .contains(&DiagnosticCause::EventHistoryTruncated)
            {
                self.telemetry_quality
                    .diagnostics
                    .push(DiagnosticCause::EventHistoryTruncated);
            }
        }
    }

    pub fn mark_event_stream_unavailable(&mut self, reason: impl Into<String>) {
        if !self
            .telemetry_quality
            .diagnostics
            .contains(&DiagnosticCause::EventStreamUnavailable)
        {
            self.telemetry_quality
                .diagnostics
                .push(DiagnosticCause::EventStreamUnavailable);
        }
        let timestamp_dcs = self
            .datums
            .last()
            .map(|datum| datum.time)
            .or(self.previous_sample_time)
            .unwrap_or_default();
        self.record_event("event_stream_unavailable", timestamp_dcs, false, reason);
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

fn health_rank(health: TelemetryHealth) -> u8 {
    match health {
        TelemetryHealth::Green => 0,
        TelemetryHealth::Orange => 1,
        TelemetryHealth::Red => 2,
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
    quality.bracket_gap_ms = Some(bracket_gap_ms);
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
    quality.bracket_gap_ms = Some(bracket_gap_ms);
}

fn parse_dcs_wire(comment: &str) -> Option<u8> {
    let (_, suffix) = comment.split_once("WIRE#")?;
    let suffix = suffix.trim_start();
    let digit_count = suffix.bytes().take_while(u8::is_ascii_digit).count();
    if digit_count == 0 {
        return None;
    }
    let (digits, remainder) = suffix.split_at(digit_count);
    if !remainder.is_empty()
        && !remainder
            .chars()
            .next()
            .is_some_and(|next| next.is_ascii_whitespace() || next == '[')
    {
        return None;
    }
    let wire = digits.parse::<u64>().ok()?;
    u8::try_from(wire)
        .ok()
        .filter(|wire| (1..=4).contains(wire))
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
            bracket_gap_ms: Some(100.0),
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
            if reason == "stale_gate_bracket" {
                assert!((quality.bracket_gap_ms.expect("bracket gap") - 400.0).abs() < 0.001);
            }
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
    fn event_stream_failure_before_touchdown_preserves_gates_without_favourable_outcome() {
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
        track.entered_groove = true;
        let mut correlator = crate::tasks::event_correlator::EventCorrelator::new(10, 20);
        correlator.stream_unavailable(&mut track, "unavailable: before touchdown");

        let result = track.finish();
        assert_eq!(
            result.telemetry_quality.completeness,
            Completeness::Complete
        );
        assert!(result
            .telemetry_quality
            .diagnostics
            .contains(&DiagnosticCause::EventStreamUnavailable));
        assert_eq!(result.grading, Grading::WaveoffUnknown);
        assert_eq!(result.grade_points, None);
    }

    #[test]
    fn event_stream_failure_after_confirmed_touchdown_does_not_revoke_position_evidence() {
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
            cable_estimated: Some(3),
        });
        let mut correlator = crate::tasks::event_correlator::EventCorrelator::new(10, 20);
        assert!(correlator.landing_quality_mark(
            &mut track,
            12.0,
            "LSO: GRADE:OK : WIRE# 3".to_string()
        ));
        correlator.stream_unavailable(&mut track, "clean_end_of_stream");

        let result = track.finish();
        assert_eq!(
            result.telemetry_quality.completeness,
            Completeness::Complete
        );
        assert!(result
            .telemetry_quality
            .diagnostics
            .contains(&DiagnosticCause::EventStreamUnavailable));
        assert_ne!(result.pass_grade, PassGrade::Incomplete);
        assert!(result.grade_points.is_some());
        let summary = correlator.summary(&result.grading);
        assert!(summary.outcome_confirmed);
    }

    #[test]
    fn dcs_wire_parser_accepts_only_wires_one_through_four() {
        for wire in 1..=4 {
            assert_eq!(
                parse_dcs_wire(&format!("LSO: GRADE:OK : WIRE# {wire}[BC]")),
                Some(wire)
            );
        }
        for malformed in [
            "WIRE# 0",
            "WIRE# 5",
            "WIRE# 99",
            "WIRE# 255",
            "WIRE# 256",
            "WIRE# 184467440737095516160",
            "WIRE# -1",
            "WIRE# 1foo",
            "WIRE#",
        ] {
            assert_eq!(parse_dcs_wire(malformed), None, "{malformed}");
        }
    }

    #[test]
    fn invalid_dcs_wire_never_confirms_an_arrest_or_awards_points() {
        let carrier = CarrierInfo::by_type("CVN_71").unwrap();
        let plane = AirplaneInfo::by_type("FA-18C_hornet").unwrap();
        for invalid in [
            "WIRE# 0",
            "WIRE# 5",
            "WIRE# 255",
            "WIRE# 999999999999999999999",
        ] {
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
                cable_estimated: Some(3),
            });
            assert!(track.set_dcs_grading(invalid.to_string()));
            let result = track.finish();
            assert_eq!(
                result.telemetry_quality.completeness,
                Completeness::UnconfirmedArrest,
                "{invalid}"
            );
            assert_eq!(result.pass_grade, PassGrade::Incomplete, "{invalid}");
            assert_eq!(result.grade_points, None, "{invalid}");
        }
    }

    #[test]
    fn unconfirmed_arrest_does_not_overwrite_telemetry_cause() {
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
        track.entered_groove = true;
        track.mark_telemetry_gap(TelemetryInvalidReason::TelemetryGap);
        track.grading = Some(Grading::Recovered {
            cable: None,
            cable_estimated: None,
        });

        let result = track.finish();
        assert_eq!(
            result.telemetry_quality.completeness,
            Completeness::TelemetryGap
        );
        assert_eq!(
            result.telemetry_quality.unavailability_causes,
            [Completeness::TelemetryGap, Completeness::UnconfirmedArrest]
        );
    }

    #[test]
    fn completeness_database_names_match_json_names() {
        for completeness in [
            Completeness::Complete,
            Completeness::InsufficientGates,
            Completeness::TelemetryGap,
            Completeness::InvalidTelemetry,
            Completeness::UnconfirmedArrest,
            Completeness::BufferLimit,
        ] {
            assert_eq!(
                serde_json::to_string(&completeness).unwrap(),
                format!("\"{}\"", completeness.as_str())
            );
        }
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
    fn event_buffer_is_bounded_without_masking_primary_completeness() {
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
            Completeness::InsufficientGates
        );
        assert_eq!(result.telemetry_quality.dropped_event_samples, 10);
        assert!(result
            .telemetry_quality
            .diagnostics
            .contains(&DiagnosticCause::EventHistoryTruncated));
        assert_eq!(result.grade_points, None);
    }

    #[test]
    fn hook_history_is_a_recent_ring_and_never_changes_position_completeness() {
        let carrier = CarrierInfo::by_type("CVN_71").unwrap();
        let plane = AirplaneInfo::by_type("FA-18C_hornet").unwrap();
        let mut track = Track::new("pilot", carrier, plane);
        track.entered_groove = true;
        track.previous_x = 400.0;
        for sequence in 0..(MAX_HOOK_EVIDENCE + 88) {
            track.observe_hook_sample(
                sequence as f64,
                sequence as u64,
                0.0,
                Some(1.0),
                HookSampleStatus::Success,
            );
        }
        let result = track.finish();
        assert_eq!(result.hook_observation.timeline.len(), MAX_HOOK_EVIDENCE);
        assert_eq!(
            result
                .hook_observation
                .timeline
                .front()
                .unwrap()
                .associated_time_dcs,
            88.0
        );
        assert_eq!(
            result
                .hook_observation
                .timeline
                .back()
                .unwrap()
                .associated_time_dcs,
            (MAX_HOOK_EVIDENCE + 87) as f64
        );
        assert_eq!(result.telemetry_quality.dropped_hook_samples, 88);
        assert_ne!(
            result.telemetry_quality.completeness,
            Completeness::BufferLimit
        );
    }

    #[test]
    fn sustained_five_hz_collection_turns_health_red_and_reports_percentiles() {
        let carrier_info = CarrierInfo::by_type("CVN_71").unwrap();
        let plane_info = AirplaneInfo::by_type("FA-18C_hornet").unwrap();
        let mut track = Track::new("pilot", carrier_info, plane_info);
        for sequence in 0..=30 {
            let time = sequence as f64 * 0.2;
            let carrier = Transform {
                time,
                forward: DVec3::unit_z(),
                ..Transform::default()
            };
            let plane = Transform {
                time,
                position: DVec3::new(0.0, 50.0, -1_000.0),
                alt: 50.0,
                ..Transform::default()
            };
            assert!(track.next(&carrier, &plane, None));
        }
        let quality = track.finish().telemetry_quality;
        assert_eq!(quality.health, TelemetryHealth::Red);
        assert_eq!(quality.health_reason, "sustained_gate_capture_risk");
        assert!((quality.effective_frequency_hz - 5.0).abs() < 0.01);
        assert!((quality.gap_p99_ms - 200.0).abs() < 0.01);
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
            assert_eq!(track.wire_estimate_at(1.1).wire, Some(3));
        }
    }

    #[test]
    fn late_touch_event_does_not_turn_an_old_crossing_into_a_wire() {
        let carrier_info = CarrierInfo::by_type("CVN_71").unwrap();
        let plane_info = AirplaneInfo::by_type("FA-18C_hornet").unwrap();
        let carrier = Transform {
            forward: DVec3::unit_z(),
            ..Transform::default()
        };
        let midpoint = (carrier_info.cable3.0 + carrier_info.cable3.1) / 2.0;
        let hook_offset = plane_info.hook;
        let mut track = Track::new("pilot", carrier_info, plane_info);
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

        let estimate = track.wire_estimate_at(2.0);
        assert_eq!(estimate.wire, None);
        assert_eq!(
            estimate.reason,
            "wire_crossing_not_time_correlated_with_event"
        );
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

    #[test]
    fn fa18_hook_calibration_uses_stable_final_window_evidence() {
        let carrier = CarrierInfo::by_type("CVN_71").unwrap();
        let hornet = AirplaneInfo::by_type("FA-18C_hornet").unwrap();
        let mut touch_and_go = Track::new("pilot", carrier, hornet);
        touch_and_go.entered_groove = true;
        touch_and_go.previous_x = 400.0;
        for index in 0..4 {
            touch_and_go.observe_hook_sample(
                10.0 + index as f64 * 0.25,
                index,
                0.0,
                Some(0.0),
                HookSampleStatus::Success,
            );
        }
        assert_eq!(
            touch_and_go.calibrated_hook_state(),
            CalibratedHookState::Up
        );
        touch_and_go.landing_time = Some(11.0);
        for index in 0..3 {
            touch_and_go.observe_hook_sample(
                11.0 + index as f64 * 0.25,
                10 + index,
                0.0,
                Some(1.0),
                HookSampleStatus::Success,
            );
        }
        assert_eq!(
            touch_and_go.calibrated_hook_state(),
            CalibratedHookState::Up,
            "post-touch samples must not rewrite the pre-touch CQ evidence"
        );

        let mut arrested = Track::new("pilot", carrier, hornet);
        arrested.entered_groove = true;
        arrested.previous_x = 400.0;
        for (index, raw) in [0.0, 1.0, 1.0].into_iter().enumerate() {
            arrested.observe_hook_sample(
                20.0 + index as f64 * 0.25,
                index as u64,
                0.0,
                Some(raw),
                HookSampleStatus::Success,
            );
        }
        assert_eq!(arrested.calibrated_hook_state(), CalibratedHookState::Down);
    }

    #[test]
    fn uncalibrated_f14_hook_values_remain_unknown() {
        let carrier = CarrierInfo::by_type("CVN_71").unwrap();
        let tomcat = AirplaneInfo::by_type("F-14B").unwrap();
        let mut track = Track::new("pilot", carrier, tomcat);
        track.entered_groove = true;
        track.previous_x = 400.0;
        for index in 0..4 {
            track.observe_hook_sample(
                10.0 + index as f64 * 0.25,
                index,
                0.0,
                Some(0.0),
                HookSampleStatus::Success,
            );
        }
        assert_eq!(track.calibrated_hook_state(), CalibratedHookState::Unknown);
    }
}
