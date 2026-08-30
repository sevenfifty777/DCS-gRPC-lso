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

#[derive(Debug, Clone, Default, PartialEq, serde::Serialize)]
pub struct HookObservation {
    pub samples_in_groove: u32,
    pub samples_in_final_window: u32,
    pub min_raw: Option<f64>,
    pub max_raw: Option<f64>,
    pub final_raw: Option<f64>,
    /// Polarity is intentionally not interpreted before module/live validation.
    pub polarity: &'static str,
}

pub struct Track {
    pilot_name: String,
    previous_distance: f64,
    previous_x: f64,
    previous_sample_time: Option<f64>,
    previous_gate_sample: Option<ApproachSample>,
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
    BufferLimit,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct TelemetryQuality {
    pub completeness: Completeness,
    pub max_sample_gap_ms: f64,
    pub max_skew_ms: f64,
    pub warning_samples: u32,
    pub invalid_samples: u32,
    pub dropped_samples: u32,
    pub reasons: Vec<TelemetryInvalidReason>,
}

impl Default for TelemetryQuality {
    fn default() -> Self {
        Self {
            completeness: Completeness::Complete,
            max_sample_gap_ms: 0.0,
            max_skew_ms: 0.0,
            warning_samples: 0,
            invalid_samples: 0,
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
    sample_gap_ms: f64,
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
            previous_gate_sample: None,
            datums: Default::default(),
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
                polarity: "unknown_pending_live_validation",
                ..HookObservation::default()
            },
            crossed_deck_threshold: false,
            telemetry_quality: TelemetryQuality::default(),
            events: Vec::new(),
            spot_zone: SpotZoneObservation::default(),
            touchdown_horizontal_speed_mps: None,
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
        if let Some(reason) = sample.invalid_reason {
            self.telemetry_quality.invalid_samples += 1;
            if !self.telemetry_quality.reasons.contains(&reason) {
                self.telemetry_quality.reasons.push(reason);
            }
            self.telemetry_quality.completeness = match reason {
                TelemetryInvalidReason::TelemetryGap => Completeness::TelemetryGap,
                _ if self.telemetry_quality.completeness != Completeness::TelemetryGap => {
                    Completeness::InvalidTelemetry
                }
                _ => self.telemetry_quality.completeness,
            };
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
        if sample.is_valid() {
            self.observe_vstol_spot_zone(carrier, plane);
        }
        let is_arrested_recovery = matches!(&self.carrier_info.recovery, CarrierRecovery::Arrested);
        if is_arrested_recovery {
            if let Some(raw) = hook_state.filter(|raw| raw.is_finite()) {
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
                if self.entered_groove {
                    self.hook_observation.samples_in_groove += 1;
                }
                if self.entered_groove && distance <= GATE_QUARTER_NM {
                    self.hook_observation.samples_in_final_window += 1;
                    self.hook_observation.final_raw = Some(raw);
                }
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
                sample_gap_ms: sample.sample_gap_ms.max(sample.source_age_ms),
                skew_ms: sample.skew_ms,
            };

            if let Some(previous) = self.previous_gate_sample.as_ref() {
                capture_gate(
                    previous,
                    &current,
                    GATE_THREE_QUARTER_NM,
                    ideal_base_alt,
                    self.plane_info.glide_slope,
                    &mut self.gate_deviations.at_three_quarter_nm,
                    &mut self.gate_deviations.three_quarter_quality,
                );
                capture_gate(
                    previous,
                    &current,
                    GATE_HALF_NM,
                    ideal_base_alt,
                    self.plane_info.glide_slope,
                    &mut self.gate_deviations.at_half_nm,
                    &mut self.gate_deviations.half_quality,
                );
                capture_gate(
                    previous,
                    &current,
                    GATE_QUARTER_NM,
                    ideal_base_alt,
                    self.plane_info.glide_slope,
                    &mut self.gate_deviations.at_quarter_nm,
                    &mut self.gate_deviations.quarter_quality,
                );
            } else {
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
            }
            self.previous_gate_sample = Some(current);

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
            CarrierRecovery::Arrested => self.estimate_cable(carrier, plane),
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

        if !self.gate_deviations.all_valid()
            && self.telemetry_quality.completeness == Completeness::Complete
        {
            self.telemetry_quality.completeness = Completeness::InsufficientGates;
        }
        if matches!(self.carrier_info.recovery, CarrierRecovery::Arrested)
            && matches!(grading, Grading::Recovered { cable: None, .. })
        {
            // RunwayTouch/Land prove contact, not an arrest. Until sustained
            // kinematics or a DCS wire/LQM confirms the trap, the pass cannot
            // receive a favourable grade.
            self.telemetry_quality.completeness = Completeness::InvalidTelemetry;
        }
        if self.telemetry_quality.completeness != Completeness::Complete {
            pass_grade = PassGrade::Incomplete;
            grade_points = None;
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

    fn estimate_cable(&self, carrier: &Transform, plane: &Transform) -> Option<u8> {
        let hook_offset = self.plane_info.hook.rotated_by(plane.rotation);
        let touchdown = plane.position + hook_offset;
        let forward = carrier.forward.rotated_by(DRotor3::from_rotation_xz(
            -self.carrier_info.deck_angle.to_radians(),
        ));

        // The land event fires slightly after the wire is caught, so the hook has already
        // passed the wire. Move the touchdown 3.0 m forward to compensate.
        let touchdown = touchdown + (forward * 3.0);

        [
            (1, &self.carrier_info.cable1),
            (2, &self.carrier_info.cable2),
            (3, &self.carrier_info.cable3),
            (4, &self.carrier_info.cable4),
        ]
        .into_iter()
        .map(|(nr, pendants)| {
            // Calculate the mid position between both cable pendants:
            // o-----------o
            //       ^
            //       |
            let mid_cable = (pendants.0 - pendants.1) / 2.0;
            let mid_cable = pendants.0 - mid_cable;
            let mid_cable = carrier.position + mid_cable.rotated_by(carrier.rotation);

            (nr, mid_cable)
        })
        .map(|(nr, mid_cable)| {
            let ray_to_cable = touchdown - mid_cable;
            tracing::trace!(
                cable = nr,
                distance = ray_to_cable.mag(),
                dot = ray_to_cable.dot(forward),
                "cable candidate"
            );
            (nr, ray_to_cable.mag_sq())
        })
        .min_by(|left, right| left.1.total_cmp(&right.1))
        .map(|(nr, _)| nr)
    }

    pub fn mark_telemetry_gap(&mut self, reason: TelemetryInvalidReason) {
        self.telemetry_quality.invalid_samples += 1;
        self.telemetry_quality.completeness = Completeness::TelemetryGap;
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
    if previous.sample_gap_ms.max(current.sample_gap_ms) > SAMPLE_GAP_WARNING_MS {
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
    let sample_gap_ms = previous.sample_gap_ms.max(current.sample_gap_ms);
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

fn parse_dcs_wire(comment: &str) -> Option<u8> {
    let (_, suffix) = comment.split_once("WIRE#")?;
    suffix
        .trim_start()
        .chars()
        .take_while(char::is_ascii_digit)
        .collect::<String>()
        .parse()
        .ok()
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
            sample_gap_ms: 100.0,
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
        let current = approach_sample(11.0, 900.0);
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
        assert!((datum.timestamp_dcs - 10.74).abs() < 1.0e-9);
    }

    #[test]
    fn tracking_started_inside_gate_is_late_and_never_backfilled() {
        let mut quality = GateQuality::default();
        mark_started_inside(900.0, GATE_HALF_NM, &mut quality);
        assert_eq!(quality.status, GateStatus::Late);

        let mut datum = None;
        capture_gate(
            &approach_sample(1.0, 1_000.0),
            &approach_sample(2.0, 900.0),
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
            (400.0, 0.0, 2.0, "stale_gate_bracket"),
            (100.0, 301.0, 2.0, "excessive_skew_at_gate"),
            (100.0, 0.0, 0.5, "non_monotonic_gate_bracket"),
        ];
        for (gap, skew, current_time, reason) in cases {
            let previous = approach_sample(1.0, 1_000.0);
            let mut current = approach_sample(current_time, 900.0);
            current.sample_gap_ms = gap;
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
            let plane = Transform {
                position: midpoint_world
                    - plane_info.hook.rotated_by(plane_rotation)
                    - forward * 3.0,
                rotation: plane_rotation,
                ..Transform::default()
            };
            let track = Track::new("pilot", carrier_info, plane_info);
            assert_eq!(track.estimate_cable(&carrier, &plane), Some(3));
        }
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
}
