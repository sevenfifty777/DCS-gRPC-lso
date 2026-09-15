use std::collections::VecDeque;
use std::ops::Neg;

use ultraviolet::{DRotor3, DVec3};

use crate::data::{AirplaneInfo, CarrierInfo, CarrierRecovery};
use crate::grading::{
    compute_catobar_assessment, compute_vstol_approach_grade_points,
    compute_vstol_final_grade_from_points, CatobarEvidence, GradingEpisode, PassGrade, SpotGrade,
};
use crate::telemetry::{
    AlignmentMethod, InvalidSourceObservation, InvalidSourceVerdictEffect,
    ScoringSegmentAttribution, SourceCaptureAnchor, SourceTimeAttributionBasis,
    TelemetryInvalidReason, TelemetrySample, MAX_EXTRAPOLATION_MS, SAMPLE_GAP_WARNING_MS,
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
pub(crate) const GATE_THREE_QUARTER_NM: f64 = 1389.0;
/// ½ nm gate — primary grading reference.
pub(crate) const GATE_HALF_NM: f64 = 926.0;
/// ¼ nm gate — ramp / "in close"; dangerously low here triggers a Cut pass.
pub(crate) const GATE_QUARTER_NM: f64 = 463.0;

/// Altitude cap (relative to deck level, hook offset included) below which an x=0 threshold
/// crossing is treated as a real deck crossing (bolter/touch-and-go candidate) rather than a
/// high fly-over during a go-around. Confirmed live: a genuine waveoff crossed x=0 at ~140 m
/// (~460 ft), well above any plausible deck contact, and was misclassified as `Bolter`. Deck
/// heave/pitch and hook geometry can put a real touch a few feet off exact zero, so this stays
/// tighter than the 300 ft groove-entry guard rather than reusing it.
const DECK_CROSSING_ALT_CAP_FT: f64 = 50.0;

/// Hook altitude (relative to deck level) at the exact tick the aircraft's longitudinal position
/// first crosses the deck threshold, at or below which that crossing counts as **confirmed
/// physical contact** rather than merely a low fly-over. Deliberately much tighter than
/// `DECK_CROSSING_ALT_CAP_FT` above (which only rejects a *high* go-around's threshold crossing):
/// confirmed live, a genuine survol at 8.7 m (~28 ft, comfortably inside the 50 ft cap) produced
/// no contact at all and was still misclassified as `Bolter` with zero proof of a touch (5
/// September 2026, human test). PROJECT-DERIVED: chosen to tolerate deck heave/telemetry noise
/// around a genuine touch, not calibrated on a large corpus.
const DECK_CONTACT_CONFIRMATION_ALT_M: f64 = 1.0;

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
/// Upper bound on `trajectory_deviations`, the continuous groove-to-touchdown GS/lineup
/// series. A groove is a few tens of seconds at 10-20 Hz, so this is a defensive cap, not
/// an expected limit.
const MAX_TRAJECTORY_SAMPLES: usize = 4_000;
/// Minimum horizontal distance-to-ship (`x`, metres) below which a `trajectory_deviations`
/// sample is no longer pushed. `gs_deviation_deg`/`lineup_deg` are `atan2(offset_m, x)`: as `x`
/// approaches 0 in the final metres before touchdown, an ordinary few-decimetre flare produces
/// an angle of several tens of degrees with no geometric meaning (confirmed live: 70.3 deg at
/// x=0.30 m, 32.5 deg at x=1.47 m, neither reflecting a real deviation). PROJECT-DERIVED: a few
/// metres comfortably clears that blow-up region while still covering the in-flight approach
/// down to just short of the ramp.
const TRAJECTORY_MIN_DISTANCE_M: f64 = 3.0;
/// Fixed reference distance (metres) substituted for the real, shrinking `x` when computing
/// `gs_deviation_deg` for a `trajectory_deviations` sample closer to the ship than this.
/// `TRAJECTORY_MIN_DISTANCE_M` above stops the outright blow-up as `x -> 0`, but it does not
/// stop a much more moderate, still-misleading version of the same effect between that floor and
/// several tens of metres out: an ordinary, essentially constant flare offset of a few
/// decimetres — confirmed live on 5 September 2026 to be a near-constant few tenths of a metre,
/// both vertically and laterally, from at least 50 m out to touchdown across every recovery
/// observed, on clean and Cut passes alike — produces a rapidly growing angle purely because the
/// denominator `x` keeps shrinking, not because the underlying offset is getting worse. Holding
/// the denominator at this reference distance for any sample closer than it (`x.max(this)`)
/// removes that purely geometric growth while leaving every sample farther out untouched (`x`
/// already exceeds it there, so `max` is a no-op) and leaving a genuinely large offset just as
/// able to cross a tier or Cut threshold as before — it just takes a real number of metres to do
/// it, not merely a small number of metres of remaining distance. PROJECT-DERIVED, chosen as
/// roughly one second of flight at a typical CATOBAR approach speed (~75 m/s): comfortably larger
/// than the confirmed-live flare offsets divided by the Ok/Cut angular thresholds (see
/// docs/GRADING_REFERENCE.md, "Continuous trajectory, near-touchdown geometry" for the derivation
/// and live numbers), while short enough to still evaluate a deviation that only develops in the
/// final seconds using its own reasonably real geometry rather than one held all the way back to
/// a gate distance.
const NEAR_TOUCHDOWN_ANGLE_REFERENCE_M: f64 = 75.0;
/// Lineup uses a longer fixed reference inside the complete late-grading window. Human F-14B(U)
/// telemetry from 8 September 2026 showed that the former 75 m denominator made the 1.5 degree
/// late threshold tighten from 3.93 m at 150 m to 1.96 m in close even when the aircraft's real
/// lateral displacement was stable or converging. Holding the denominator at 150 m makes that
/// angular threshold represent one consistent lateral error throughout the window while still
/// exposing the raw measured displacement in `TrajectoryDeviation::lineup_deviation_m`.
const NEAR_TOUCHDOWN_LINEUP_REFERENCE_M: f64 = 150.0;
const MAX_EVENT_EVIDENCE: usize = 256;
const MAX_INVALID_SOURCE_EVIDENCE: usize = 512;
/// At the maximum supported 4 Hz hook cadence this retains about 8.5 minutes, comfortably beyond
/// the roughly three-minute detection-to-contact sessions observed live. If it is still exceeded,
/// the recent-ring policy evicts only the oldest samples so the last quarter nautical mile and
/// contact-adjacent evidence remain available.
const MAX_HOOK_EVIDENCE: usize = 2_048;
const GATE_BUFFER_WINDOW_S: f64 = 2.0;
const HEALTH_WINDOW_S: f64 = 10.0;

// ---------------------------------------------------------------------------
// CATOBAR Case I groove-entry roll-out detection.
//
// SOURCE: NAVAIR 00-80T-105 (CV NATOPS) 6.2.4.2/6.2.4.3 defines Case I groove entry as an
// event, not a geometric threshold: after the 180-to-90-to-start approach turn, "the aircraft
// should roll wings level on centerline with a centered ball" before a 15-18 second groove to
// touchdown. NAVAIR 00-80T-104 (LSO NATOPS) 6.6.3.1 separately states that "3/4 nm" is the
// LSO-control transition distance for a *Case III* precision approach, not a Case I groove
// distance -- the box below (GATE_THREE_QUARTER_NM / 300 ft / +/-10 deg lineup) already
// documented itself as an engineering proxy, but that proxy borrows its radius from a
// different Case entirely, and cannot by itself distinguish a real roll-out from the aircraft
// transiently sweeping through the box mid-turn (e.g., cutting inside the corner from the 90).
//
// PROJECT-DERIVED thresholds below recognize the physical final turn from the port-side Case I
// pattern, then confirm roll-out from bank alone while requiring continuous inbound progress.
// Lineup, ground track and lineup trend remain auditable quality diagnostics, but never delay the
// scored portion: an undershoot, overshoot or active correction is part of the groove rather than
// evidence that the groove does not exist. None of these numerical thresholds is NAVAIR-specified.
//
// CATOBAR only. V/STOL (Tarawa AV-8B) keeps the box-only behaviour unchanged: its approach
// profile (hover/cross/VL, see VSTOL.md) has no CATOBAR-style final turn to distinguish from,
// and this refinement was designed and reasoned about against CATOBAR Case I geometry only.

/// Maximum bank accepted while confirming CATOBAR Case I roll-out. It remains looser than a
/// literal zero-bank test so a small correction does not suppress entry indefinitely.
const GROOVE_ROLLOUT_MAX_BANK_DEG: f64 = 10.0;
/// Legacy stable-axis diagnostic threshold. Retained additively in `groove_entry.criteria`, but
/// no longer gates Case I groove entry.
const GROOVE_ROLLOUT_MAX_TRACK_ANGLE_DEG: f64 = 10.0;
/// Legacy stable-axis diagnostic threshold. Retained for schema-v3 compatibility only.
const GROOVE_ENTRY_MAX_LINEUP_DEG: f64 = 2.0;
/// Minimum time span (seconds) the buffered `gate_samples` window must already cover before
/// the ground-track angle above is trusted at all. PROJECT-DERIVED: half of
/// `GATE_BUFFER_WINDOW_S`, chosen so a too-short, noise-dominated baseline (e.g., the first
/// sample right after crossing into the detection box) never manufactures a false track-angle
/// reading -- errs toward waiting one more sample rather than inventing a signal, the same
/// posture `trend_worsening` takes in `grading.rs` when it lacks enough data.
const GROOVE_ROLLOUT_MIN_TRACK_WINDOW_S: f64 = GATE_BUFFER_WINDOW_S / 2.0;
/// Window used to fit lineup evolution independently of the two-second ground-track fit.
const GROOVE_LINEUP_TREND_WINDOW_S: f64 = 1.0;
/// Maximum absolute lineup trend while the entry criteria are held. At 0.5 deg/s the fitted
/// lineup can change by at most 0.375 degrees during the 0.75-second confirmation. This was the
/// tightest tested threshold that still found a later stable interval on every corpus pass that
/// entered the CATOBAR box; it guards against rapid convergence independently of whether lineup
/// happens to cross the two-degree boundary on one frame.
const GROOVE_ENTRY_MAX_LINEUP_RATE_DEG_PER_S: f64 = 0.5;
/// Maximum relative altitude at which a port-side final turn may arm the Case I roll-out search.
const CASE_I_LAST_TURN_ARM_MAX_ALTITUDE_FT: f64 = 600.0;
/// Port-side lineup corridor used as an interception diagnostic. Reaching or crossing it is not
/// mandatory: a genuine undershoot may roll wings level while remaining farther to port.
const CASE_I_PORT_LINEUP_CORRIDOR_DEG: f64 = -0.75;
/// Outbound growth proving that a later sample belongs to a new circuit branch.
const CASE_I_BRANCH_RESET_OUTBOUND_GROWTH_M: f64 = 150.0;
/// Minimum source-time duration for which bank and inbound progress must remain continuously true.
/// Time, rather than a sample count, keeps the contract stable at both the 10 Hz unary rollback
/// and the 20 Hz buffered source. A capture gap above the 300 ms contract resets this interval.
const GROOVE_ENTRY_STABILITY_DURATION_S: f64 = 0.75;
/// PROJECT-DERIVED provisional observation radius. It is informational only
/// until the Tarawa spot geometry is validated against the future live corpus.
const VSTOL_SPOT_OBSERVATION_RADIUS_M: f64 = 15.0;

// ---------------------------------------------------------------------------
// Wire-estimate arrest deceleration proxy.
//
// `wire_estimate_at` used to always take the last (highest-numbered) hook-plane crossing
// recorded before the correlated touchdown event. On a real trap the hook can geometrically
// sweep across several wire thresholds while the aircraft is still airborne on short final
// (still at approach speed, no deceleration), then a late DCS `Land`/`RunwayTouch` correlation
// let the estimator pick whichever of those was numerically highest, rather than the wire
// actually caught -- confirmed live as a systematic bias (see tasking-roadmap.md, P1). The
// proxy below distinguishes "still airborne, sweeping over wire thresholds" from "already
// arrested, decelerating on deck" directly from the continuous horizontal ground speed, without
// depending on the event correlation at all.

/// Horizontal deceleration rate (m/s^2, ground speed only) above which the aircraft is treated
/// as actively decelerating after an arrestment, rather than merely varying speed in normal
/// flight (throttle changes, turbulence, natural approach-speed bleed). PROJECT-DERIVED, not
/// NATOPS-numbered: a CATOBAR trap sheds most of its ~65-75 m/s approach speed within a ~2-3 s,
/// ~90-150 m roll-out, averaging well above 20 m/s^2 -- this sits far enough below that peak
/// rate to catch the onset promptly while staying well above plausible in-flight speed noise.
/// Confirmed live on 2 DCS-confirmed wire-1 arrests (6 September 2026, same pilot/aircraft type,
/// F-14B(U)): observed horizontal speed stayed within +/-1.2 m/s^2 of noise for ~0.6-0.7 s after
/// the hook geometrically swept past all four wire thresholds, then climbed sharply through this
/// threshold over 2-3 consecutive samples both times -- see tasking-roadmap.md for the full trace
/// and `WIRE_ARREST_ONSET_TOLERANCE_S` below for what this live data changed.
const WIRE_ARREST_DECELERATION_MPS2: f64 = 5.0;
/// How many consecutive samples must show deceleration above `WIRE_ARREST_DECELERATION_MPS2`
/// before the onset is accepted. PROJECT-DERIVED, the same value and rationale as
/// other persistence guards in this module: it rules out a single noisy/skewed telemetry frame
/// without meaningfully delaying detection at scoring cadence.
const WIRE_ARREST_DECELERATION_MIN_CONSECUTIVE_SAMPLES: u32 = 2;
/// Tolerance (seconds) by which a wire crossing may precede the detected deceleration onset and
/// still count as the wire that was actually caught. PROJECT-DERIVED. Originally 0.5 s (absorbing
/// only the onset's own detection lag: about one sample interval plus a brief load build-up), but
/// confirmed live on 6 September 2026 to be roughly half of what real DCS cable-arrest data needs:
/// on the 2 DCS-confirmed wire-1 arrests available (same pilot/aircraft type, F-14B(U)), the
/// aircraft coasted at essentially constant speed for **0.895 s and 0.990 s** after the hook
/// geometrically crossed wire 1's threshold -- and had already geometrically crossed *all four*
/// wire thresholds (spanning only ~0.62-0.66 s) -- before deceleration became measurable. Widened
/// to 1.2 s to comfortably cover this "free-rollout before the cable loads" phase and correctly
/// select the earliest (actually caught) crossing rather than a later one the hook merely slid
/// past. Calibrated on only 2 samples from a single pilot/aircraft type; needs reconfirmation on a
/// broader live corpus (other aircraft types, other pilots) before this value is trusted generally
/// -- see tasking-roadmap.md, P1.
const WIRE_ARREST_ONSET_TOLERANCE_S: f64 = 1.2;
/// Diagnostic-only kinematic arrest signature. These PROJECT-DERIVED thresholds never make a
/// pass gradable: they intentionally require a correlated DCS contact, a nearby sustained
/// deceleration onset, then at least two seconds nearly stationary relative to the carrier with
/// uninterrupted source capture and no bounce/departure signature.
const ARREST_LOW_RELATIVE_SPEED_MPS: f64 = 5.0;
const ARREST_LOW_SPEED_HOLD_S: f64 = 2.0;
const ARREST_MIN_LOW_SPEED_SAMPLES: u32 = 3;
const ARREST_MAX_CONTACT_ONSET_DELTA_S: f64 = 1.2;
const ARREST_MAX_ON_DECK_HOOK_HEIGHT_M: f64 = 3.0;
const ARREST_DEPARTURE_RELATIVE_SPEED_MPS: f64 = 10.0;

// ---------------------------------------------------------------------------
// Hook-transient wire estimate (PROJECT-DERIVED, validated against the September 2026 T-45 and
// F-14B(U) labelled live corpus in `tests/recordings/live_2026-09/`). A real arrestment drives the
// animated external hook draw argument from its stable "down" band (`>= HOOK_DOWN_STABLE_MIN`)
// sharply into the deflected band (`<= HOOK_DEFLECTED_MAX`) within two seconds of the touchdown
// reference, then back to the down band within eight seconds once the cable pull-back ends. The
// wire is the last finite pendant crossing no more than 200 ms before that deflection. A steady
// value, a hook-up baseline or an incomplete transient never names a wire.
// ---------------------------------------------------------------------------
const HOOK_DOWN_STABLE_MIN: f64 = 0.8;
const HOOK_DEFLECTED_MAX: f64 = 0.7;
const MIN_HOOK_DOWN_STABLE_S: f64 = 0.2;
const MAX_HOOK_DEFLECTION_RECOVERY_S: f64 = 8.0;
const MAX_HOOK_DEFLECTION_TOUCH_OFFSET_S: f64 = 2.0;
const MAX_HOOK_DEFLECTION_WIRE_LAG_MS: f64 = 200.0;
/// Reject a hook-plane crossing when the hook is not physically near the finite pendant: outside
/// the two pendant end points, or more than this far above/below it. Prevents an early
/// overhead-pattern crossing of the infinite wire plane from suppressing the real deck crossing.
const MAX_WIRE_VERTICAL_SEPARATION_M: f64 = 3.0;
/// Pilot-commanded hook state is latched from the stable baseline observed this long before the
/// earliest contact evidence, so the arrestment excursion of the animated hook (which starts up to
/// ~1.4 s before the DCS touchdown event) never flips a real trap to "hook up".
const HOOK_BASELINE_GUARD_S: f64 = 1.5;
const HOOK_BASELINE_WINDOW_S: f64 = 3.0;

// ---------------------------------------------------------------------------
// Deck-kinematics arrest confirmation (PROJECT-DERIVED, campaign B 2026-09-02/03, ported from
// `astra-review`): a trapped aircraft's carrier-relative horizontal speed fell below ~5 m/s
// within 8 s of the contact reference and stayed there for 2 s, while bolters and hook-up
// touch-and-go passes left the deck at ~47-50 m/s. Unlike `observe_arrest_kinematics` above,
// this uses displacement relative to the *raw* carrier position over a one-second window, so it
// works on ACMI replay (which carries no velocity) and is immune to DCS's ~1.4 s stepped ship
// position. This is the evidence allowed to confirm an arrest when DCS supplies no `WIRE#`
// (human LSO, DCS waveoff ignored); it never names a wire and never earns "high" confidence.
// ---------------------------------------------------------------------------
const DECK_ARREST_MAX_RELATIVE_SPEED_MPS: f64 = 6.0;
/// Hysteresis applied while checking that the aircraft stays arrested.
const DECK_ARREST_HOLD_MAX_RELATIVE_SPEED_MPS: f64 = 8.0;
/// The slow window must start within this time after the contact reference. Live traps settle
/// 4-5 s after touchdown once the cable pull-back ends.
const DECK_ARREST_DETECTION_WINDOW_S: f64 = 8.0;
const DECK_ARREST_HOLD_S: f64 = 2.0;
const DECK_ARREST_MAX_SAMPLE_GAP_MS: f64 = 300.0;
/// Deck run-out band along the angled deck, relative to the ideal touchdown point (`x > 0` is
/// short of it, `x < 0` is beyond it).
const DECK_ARREST_MIN_X_M: f64 = -160.0;
const DECK_ARREST_MAX_X_M: f64 = 60.0;
/// Deck kinematics are recorded once the aircraft is within this distance of the ideal touchdown
/// point while in the groove.
const DECK_KINEMATICS_START_X_M: f64 = 60.0;
const MAX_DECK_KINEMATIC_SAMPLES: usize = 600;
/// Relative speed is the carrier-relative displacement over at least this window, which spans one
/// DCS ship-position step.
const KINEMATIC_SPEED_WINDOW_S: f64 = 1.0;
/// An eventless arrest (no `Land`/`RunwayTouch` at all, `Recovered` established purely from deck
/// kinematics) keeps recording this long after the aircraft first went slow, so the hook
/// transient can recover and the hold can be measured, then stops. Mirrors the live recorder's
/// 10 s post-touchdown window and replaces the unreachable `held_s >= 8 s` exit of the original
/// implementation (review finding F04).
const POST_ARREST_EVIDENCE_WINDOW_S: f64 = 10.0;
/// `WireEstimateEvidence::reason` when the estimate came from the hook transient; the only reason
/// that also counts as arrest evidence (`arrest_evidence == "hook_transient"`).
const HOOK_TRANSIENT_ESTIMATE_REASON: &str = "hook_deflection_correlated_with_wire_crossing";
/// `WireEstimateEvidence::reason` when the wire was named from where the aircraft came to rest
/// (`Track::wire_estimate_from_stop_position`).
const STOP_POSITION_ESTIMATE_REASON: &str = "stop_position_run_out";
/// `WireEstimateEvidence::reason` on a pass flown with the hook up: the crossing sequence says
/// which wire the hook would have caught, useful feedback on an intentional bolter, but nothing
/// was caught and the number must never read as an arrestment.
const HYPOTHETICAL_HOOK_UP_REASON: &str = "hypothetical_hook_up_plane_crossing";
/// Largest distance between `stop position + run-out` and the nearest recorded hook-plane
/// crossing for the stop-position estimate to name that wire: half the 12 m pendant spacing.
/// The residual was 0 to 2 m on the eight Tomcat traps it was calibrated on.
const WIRE_STOP_POSITION_MAX_RESIDUAL_M: f64 = 6.0;
/// How far from a crossing's timestamp the aircraft position sample may be to give that
/// crossing its `x`.
const WIRE_CROSSING_POSITION_TOLERANCE_S: f64 = 0.3;

#[derive(Debug, Clone, Default, PartialEq, serde::Serialize)]
pub struct Datum {
    /// Legacy display time; equal to corrected aircraft DCS time.
    pub time: f64,
    pub corrected_time_dcs: f64,
    pub x: f64,
    pub y: f64,
    pub aoa: f64,
    pub alt: f64,
    /// Aircraft roll/bank in degrees, copied from the raw telemetry sample. Informational only —
    /// carried here (not just on `TrajectoryDeviation`) so the `cadence-ab` replay path can
    /// reconstruct `TrajectoryDeviation::bank_deg` from a persisted report exactly as `Track::next`
    /// computed it live.
    pub roll_deg: f64,
    pub carrier_time: f64,
    pub plane_time: f64,
    pub carrier_received_unix_ms: u64,
    pub plane_received_unix_ms: u64,
    /// Legacy worst-of capture gap and delivery age; retained for existing schema-v3 consumers.
    pub sample_gap_ms: f64,
    /// Source capture-time spacing, distinct from delivery latency.
    pub capture_gap_ms: f64,
    /// Age of the source snapshot when the batch was read, distinct from capture continuity.
    pub delivery_age_ms: f64,
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
    /// DCS time of the sharp hook deflection that started the completed arrestment transient
    /// (see `HOOK_DEFLECTED_MAX`), when one was found near the touchdown reference.
    pub hook_deflection_time_dcs: Option<f64>,
    /// DCS time at which the deflected hook returned to its stable down band.
    pub hook_recovered_time_dcs: Option<f64>,
    /// `hook_deflection_time_dcs` minus the selected crossing time, when the estimate was
    /// anchored on the hook transient.
    pub correlation_lag_ms: Option<f64>,
    pub crossings: Vec<WireCrossingEvidence>,
    /// Diagnostic only, never used for grading: the DCS simulation time at which a sustained
    /// post-arrest horizontal deceleration was first detected (see
    /// `WIRE_ARREST_DECELERATION_MPS2`), if any. Lets a report reader see directly whether
    /// `wire` was chosen via the deceleration-onset proxy or the older last-crossing-before-event
    /// fallback, to monitor the effect of this correction on the still-open wire-bias question
    /// (see tasking-roadmap.md, P1).
    pub arrest_deceleration_onset_time: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct ArrestKinematicEvidence {
    pub accepted: bool,
    pub reason: &'static str,
    pub verdict_effect: &'static str,
    pub contact_event_correlated: bool,
    pub contact_time_dcs: Option<f64>,
    pub deceleration_onset_time_dcs: Option<f64>,
    pub deceleration_contact_delta_ms: Option<f64>,
    pub post_contact_valid_samples: u32,
    pub minimum_relative_speed_mps: Option<f64>,
    pub low_speed_hold_s: f64,
    pub low_speed_hold_samples: u32,
    pub maximum_capture_gap_ms: f64,
    pub outcome_conflicts_with_arrest: bool,
    pub bounce_detected: bool,
    pub forward_departure_detected: bool,
    pub telemetry_ended_before_conclusion: bool,
    pub contact_source: &'static str,
    pub velocity_source: &'static str,
    pub low_relative_speed_threshold_mps: f64,
    pub required_low_speed_hold_s: f64,
    pub required_low_speed_samples: u32,
    pub maximum_contact_onset_delta_s: f64,
    pub maximum_contiguous_capture_gap_ms: f64,
    pub maximum_on_deck_hook_height_m: f64,
}

/// Carrier-relative post-contact deck kinematics used to confirm an arrest without a DCS wire
/// (see the `DECK_ARREST_*` constants). Never identifies the wire.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize)]
pub struct DeckArrestKinematicsEvidence {
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
    /// Aircraft position relative to the *raw* carrier position (world frame). DCS steps ship
    /// positions every ~1.4 s, so instantaneous velocities and smoothed positions both produce
    /// spikes; a displacement over `KINEMATIC_SPEED_WINDOW_S` is stable for an aircraft carried
    /// by the deck.
    relative_position: DVec3,
    x: f64,
}

#[derive(Debug, Clone, Copy)]
struct WindowedDeckSample {
    time: f64,
    relative_speed_mps: f64,
    x: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct CompletedHookDeflection {
    deflected_at_dcs: f64,
    recovered_at_dcs: f64,
}

/// What proved (or failed to prove) that an arrested-carrier contact was an arrest.
///
/// `source` is one of `dcs_lqm` (a parsed `WIRE#`), `hook_transient` (a completed hook
/// deflection correlated with a finite pendant crossing), `kinematic` (deck kinematics show the
/// aircraft stopped relative to the ship), `kinematic_diagnostic` (only the velocity-based
/// signature below accepted; it remains diagnostic) or `unconfirmed`. The first three make the
/// pass gradable; the last two leave it `unconfirmed_arrest`.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct ArrestConfirmationEvidence {
    pub source: &'static str,
    pub confidence: &'static str,
    pub verdict_effect: &'static str,
    pub dcs_wire: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub missing_dcs_lqm_reason: Option<&'static str>,
    /// Velocity-based signature (live path only; diagnostic, never confirms on its own).
    pub kinematic: ArrestKinematicEvidence,
    /// Displacement-based signature (live and replay); the confirming kinematic evidence.
    pub deck_kinematics: DeckArrestKinematicsEvidence,
}

#[derive(Debug, Clone, Default)]
struct ArrestKinematicState {
    post_contact_valid_samples: u32,
    minimum_relative_speed_mps: Option<f64>,
    low_speed_run_start_time: Option<f64>,
    low_speed_run_samples: u32,
    best_low_speed_hold_s: f64,
    best_low_speed_hold_samples: u32,
    maximum_capture_gap_ms: f64,
    low_speed_was_reached: bool,
    bounce_detected: bool,
    forward_departure_detected: bool,
    last_valid_time_dcs: Option<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize)]
pub struct GrooveEntryCriteria {
    pub detection_box_max_distance_m: f64,
    pub detection_box_max_altitude_ft: f64,
    pub max_lineup_deg: f64,
    pub max_bank_deg: f64,
    pub max_track_angle_deg: f64,
    pub track_window_s: f64,
    pub minimum_track_window_s: f64,
    pub lineup_trend_window_s: f64,
    pub max_lineup_rate_deg_per_s: f64,
    pub required_stability_duration_s: f64,
    pub max_contiguous_sample_gap_ms: f64,
    pub last_turn_arm_max_altitude_ft: f64,
    pub port_lineup_corridor_deg: f64,
    pub requires_positive_approach_x: bool,
    pub requires_inbound_progress: bool,
    pub lineup_blocks_entry: bool,
    pub track_angle_blocks_entry: bool,
    pub lineup_rate_blocks_entry: bool,
    pub detection_box_blocks_catobar_entry: bool,
    pub last_turn_min_abs_bank_exclusive_deg: f64,
    pub branch_reset_outbound_growth_m: f64,
    pub source_time_must_increase: bool,
}

impl Default for GrooveEntryCriteria {
    fn default() -> Self {
        Self {
            detection_box_max_distance_m: GATE_THREE_QUARTER_NM,
            detection_box_max_altitude_ft: 300.0,
            max_lineup_deg: GROOVE_ENTRY_MAX_LINEUP_DEG,
            max_bank_deg: GROOVE_ROLLOUT_MAX_BANK_DEG,
            max_track_angle_deg: GROOVE_ROLLOUT_MAX_TRACK_ANGLE_DEG,
            track_window_s: GATE_BUFFER_WINDOW_S,
            minimum_track_window_s: GROOVE_ROLLOUT_MIN_TRACK_WINDOW_S,
            lineup_trend_window_s: GROOVE_LINEUP_TREND_WINDOW_S,
            max_lineup_rate_deg_per_s: GROOVE_ENTRY_MAX_LINEUP_RATE_DEG_PER_S,
            required_stability_duration_s: GROOVE_ENTRY_STABILITY_DURATION_S,
            max_contiguous_sample_gap_ms: SAMPLE_GAP_WARNING_MS,
            last_turn_arm_max_altitude_ft: CASE_I_LAST_TURN_ARM_MAX_ALTITUDE_FT,
            port_lineup_corridor_deg: CASE_I_PORT_LINEUP_CORRIDOR_DEG,
            requires_positive_approach_x: true,
            requires_inbound_progress: true,
            lineup_blocks_entry: false,
            track_angle_blocks_entry: false,
            lineup_rate_blocks_entry: false,
            detection_box_blocks_catobar_entry: false,
            last_turn_min_abs_bank_exclusive_deg: GROOVE_ROLLOUT_MAX_BANK_DEG,
            branch_reset_outbound_growth_m: CASE_I_BRANCH_RESET_OUTBOUND_GROWTH_M,
            source_time_must_increase: true,
        }
    }
}

/// Auditable snapshot of the CATOBAR Case I roll-out decision that latched groove entry.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct GrooveEntryEvidence {
    pub timestamp_dcs: f64,
    /// An exact DCS-time/UTC anchor is not provided by the current RPC contract. The client
    /// receipt time below is therefore kept distinct and is never presented as capture UTC.
    pub utc_mapping_status: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confirmation_received_unix_ms: Option<u64>,
    pub distance_m: f64,
    pub lineup_deg: f64,
    pub bank_deg: f64,
    pub track_angle_deg: f64,
    pub lineup_rate_deg_per_s: f64,
    /// Schema-v3 compatibility field: now the uninterrupted roll-out confirmation duration,
    /// not a duration for which lineup/route/trend were stable.
    pub stability_duration_s: f64,
    /// Schema-v3 compatibility field: now the number of roll-out confirmation samples.
    pub stability_sample_count: u32,
    pub trigger: &'static str,
    pub criteria: GrooveEntryCriteria,
    pub rollout_started_at_dcs: f64,
    pub altitude_relative_ft: f64,
    pub inbound_progress_mps: f64,
    pub approach_side: &'static str,
    pub port_lineup_corridor_reached: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub port_lineup_corridor_crossing_time_dcs: Option<f64>,
    pub last_turn_arm_state: &'static str,
    pub last_turn_arm_reason: &'static str,
    pub decision_semantics: &'static str,
}

#[derive(Debug, Clone, Copy)]
struct GrooveQualityMeasurement {
    track_angle_deg: f64,
    lineup_rate_deg_per_s: f64,
    inbound_progress_mps: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CaseIGrooveState {
    SearchingPattern,
    PatternObserved,
    LastTurnArmed,
    RolloutConfirming,
    GrooveConfirmed,
}

#[derive(Debug, Clone)]
struct CaseIGrooveDetector {
    state: CaseIGrooveState,
    last_time_dcs: Option<f64>,
    branch_min_x: f64,
    previous_lineup_deg: Option<f64>,
    rollout_started_at_dcs: Option<f64>,
    rollout_sample_count: u32,
    corridor_reached: bool,
    corridor_crossing_time_dcs: Option<f64>,
}

impl Default for CaseIGrooveDetector {
    fn default() -> Self {
        Self {
            state: CaseIGrooveState::SearchingPattern,
            last_time_dcs: None,
            branch_min_x: f64::MAX,
            previous_lineup_deg: None,
            rollout_started_at_dcs: None,
            rollout_sample_count: 0,
            corridor_reached: false,
            corridor_crossing_time_dcs: None,
        }
    }
}

/// Pilot-commanded arresting-hook position, latched from the pre-contact baseline of the
/// module's validated external draw argument (see `calibrated_hook_state`).
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
    pub interpreted_state: &'static str,
    pub timeline: VecDeque<HookSampleEvidence>,
    pub timeline_capacity: usize,
    pub timeline_retention_policy: &'static str,
    pub timeline_truncated: bool,
    pub timeline_dropped_samples: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub truncation_reason: Option<&'static str>,
    pub retained_from_dcs: Option<f64>,
    pub retained_through_dcs: Option<f64>,
    /// Calibration is module-specific; unknown modules are never inferred.
    pub polarity: &'static str,
}

/// Two altitude/wind samples used to correct the geometric AoA approximation for DCS's wind
/// model. DCS wind is deterministic and does not vary over time once a mission is running (it
/// only varies with altitude, per the two-layer profile configured in the mission editor), so a
/// single pair of readings taken once at groove entry — one at the aircraft's own altitude, one
/// near the deck — is enough to interpolate the wind at any altitude for the rest of the
/// approach. See `docs/GRADING_REFERENCE.md`, "AoA", for the full rationale.
#[derive(Debug, Clone, Copy)]
struct WindReference {
    alt_a_m: f64,
    wind_a: DVec3,
    alt_b_m: f64,
    wind_b: DVec3,
}

/// One raw `AtmosphereService.GetWind` response kept for diagnosis, alongside the altitude it was
/// queried at. Purely observational: never used for grading, and never replaces
/// `WindReference`/`wind_velocity_vector` in the AoA correction itself. Added to investigate a
/// confirmed live anomaly (5 September 2026: two reports out of eight reported `180deg/0.0 m/s`
/// against `95deg/0.99-1.42 m/s` on the other six, same ship/mission/timeframe) — this exposes the
/// two individual probes behind a `wind_reference_established` reference instead of only the
/// already-derived boolean, so a future live capture can show whether one specific probe (as
/// opposed to the other, or the separate report-time query) is the source of the degenerate value.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize)]
pub struct WindProbe {
    pub alt_m: f64,
    pub heading_deg: u16,
    pub speed_mps: f32,
}

/// The two raw probes behind a successfully established `WindReference`.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize)]
pub struct WindReferenceProbes {
    /// Queried at the aircraft's own altitude at groove entry.
    pub high: WindProbe,
    /// Queried at the carrier's deck altitude. Always the raw reading actually observed (after
    /// at most one bounded retry), even when `low_reading_overridden_by_high` is `true` below —
    /// this field never lies about what was measured.
    pub low: WindProbe,
    /// `true` when `low` above still looked like DCS's known `180deg/0.0 m/s` sentinel
    /// (`is_wind_sentinel`) after one retry, and the `WindReference` used for AoA correction
    /// reused `high`'s wind vector for the low-altitude point instead — see `tasking-roadmap.md`,
    /// P1. `low` keeps reporting the raw, still-suspect reading either way.
    pub low_reading_overridden_by_high: bool,
}

/// Whether a `GetWind` reading matches the exact degenerate output produced when DCS returns a
/// zero wind vector (`heading = atan2(0,0) + 180 = 180`, `speed = 0.0`) — a known intermittent
/// anomaly near sea level, not distinguishable by value alone from a genuinely calm mission (see
/// `tasking-roadmap.md`, P1). Used only to decide whether to retry/fall back; never to reject a
/// reading outright.
pub(crate) fn is_wind_sentinel(reading: (u16, f32)) -> bool {
    reading == (180, 0.0)
}

impl WindReference {
    /// Linearly interpolate (or extrapolate, clamped to the two samples) the wind vector at the
    /// given altitude. Assumes DCS interpolates linearly between its two configured wind layers,
    /// which matches the two-point (ground level / 2000 m) profile exposed in the mission editor;
    /// not verified against the engine's own interpolation curve.
    fn at_altitude(&self, alt_m: f64) -> DVec3 {
        let span = self.alt_b_m - self.alt_a_m;
        if span.abs() < f64::EPSILON {
            return self.wind_a;
        }
        let t = ((alt_m - self.alt_a_m) / span).clamp(0.0, 1.0);
        self.wind_a + (self.wind_b - self.wind_a) * t
    }
}

/// Convert a "heading the wind is coming from" + speed (as returned by
/// `AtmosphereClient::get_wind`) into a world-frame wind velocity vector (the direction and rate
/// the air itself is moving), using the same yaw convention as `Transform`'s `forward` vector
/// (yaw 0 = +z, yaw 90 = +x).
pub(crate) fn wind_velocity_vector(direction_from_deg: u16, speed_mps: f32) -> DVec3 {
    let direction_to_rad = ((f64::from(direction_from_deg) + 180.0) % 360.0).to_radians();
    DVec3::new(direction_to_rad.sin(), 0.0, direction_to_rad.cos()) * f64::from(speed_mps)
}

/// Wind-corrected, pitch-plane-only AoA approximation (`PROJECT-DERIVED`; see
/// `docs/GRADING_REFERENCE.md`, "AoA"). The plain geometric angle this replaces (nose vs. ground
/// velocity) is biased by wind — always present during carrier ops, since deck wind is standard
/// procedure — and mixes sideslip/crab into what should be a purely vertical measurement.
///
/// Subtracts the wind vector from ground velocity to approximate true airspeed, transforms it
/// into the aircraft's own body frame, and keeps only the vertical component, discarding the
/// lateral (crab) one — the same decomposition a real vane-based indicator performs by only
/// sensing airflow in the aircraft's vertical plane of symmetry.
fn corrected_aoa_deg(velocity: DVec3, wind: DVec3, rotation: DRotor3) -> f64 {
    let true_airspeed = velocity - wind;
    if true_airspeed.mag_sq() <= f64::EPSILON {
        return f64::NAN;
    }
    let body = true_airspeed.rotated_by(rotation.reversed());
    (-body.y).atan2(body.z).to_degrees()
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
    /// Continuous GS/lineup series from groove entry to touchdown (see `TrajectoryDeviation`).
    /// Cleared on each fresh groove entry, alongside `wire_crossings`, so an earlier bolter's
    /// trajectory never leaks into the scored attempt.
    trajectory_deviations: Vec<TrajectoryDeviation>,
    /// Set once a CATOBAR Case I final turn from port has rolled out, or once the unchanged
    /// V/STOL detection box is entered.
    entered_groove: bool,
    /// CATOBAR Case I pattern/last-turn/roll-out state. Unused for V/STOL.
    case_i_groove_detector: CaseIGrooveDetector,
    /// DCS simulation time (seconds since scenario start) when groove entry was first detected.
    groove_entry_time: Option<f64>,
    groove_entry_evidence: Option<GrooveEntryEvidence>,
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
    /// Set alongside `crossed_deck_threshold`, at the same single tick: whether the hook altitude
    /// at that exact threshold crossing was at or below `DECK_CONTACT_CONFIRMATION_ALT_M`, i.e.
    /// whether the crossing represents confirmed physical contact rather than merely a low
    /// fly-over. Gates the geometry-only `Bolter` path in `Track::next` (see that constant).
    deck_crossing_confirmed_contact: bool,
    /// DCS simulation time of the first sample where the hook geometrically reaches deck level
    /// (`alt <= 0.0`) for an arrested recovery. Used instead of the event-correlated
    /// `landing_time` (which can lag the physical contact by ~0.2-1 s) to freeze which hook
    /// samples `calibrated_hook_state` may interpret: a raw reading taken while the crosse is
    /// pressed against the deck reads as `0.0` regardless of true hook position and must never be
    /// mistaken for "hook up" (confirmed live 5 September 2026: this contamination would have
    /// invented a `TouchAndGo` on a real trap without the DCS LQM as a safety net).
    first_hook_ground_contact_time: Option<f64>,
    /// DCS time of the first inbound crossing of the ideal touchdown point while in the groove.
    /// Contact reference for the deck-kinematics arrest confirmation and the hook baseline
    /// guard when no `Land`/`RunwayTouch` event was correlated.
    deck_crossing_time: Option<f64>,
    /// Carrier-relative positions recorded from `DECK_KINEMATICS_START_X_M` inbound, for
    /// `evaluate_deck_arrest_kinematics`.
    deck_kinematics: Vec<DeckKinematicSample>,
    telemetry_quality: TelemetryQuality,
    /// Valid buffered captures used only as real temporal bounds for source-side errors. No
    /// position is reconstructed from these anchors.
    source_capture_anchors: Vec<SourceCaptureAnchor>,
    events: Vec<EventEvidence>,
    spot_zone: SpotZoneObservation,
    touchdown_horizontal_speed_mps: Option<f64>,
    health_red_announced: bool,
    previous_wire_plane: [Option<(f64, f64)>; 4],
    wire_crossings: Vec<WireCrossingEvidence>,
    /// Last observed (horizontal ground speed, time) pair, used by
    /// `observe_horizontal_deceleration` to compute a per-sample deceleration rate. Updated on
    /// every valid arrested-recovery sample, independent of `wire_crossings`/hook evidence.
    previous_horizontal_speed: Option<(f64, f64)>,
    /// Consecutive samples so far showing deceleration at or above
    /// `WIRE_ARREST_DECELERATION_MPS2`, reset to 0 whenever a sample falls below it.
    deceleration_run_count: u32,
    /// Time of the first sample in the current consecutive deceleration run (the last known
    /// "still fast" sample before the run started), cleared alongside `deceleration_run_count`.
    deceleration_run_start_time: Option<f64>,
    /// Frozen the first time `deceleration_run_count` reaches
    /// `WIRE_ARREST_DECELERATION_MIN_CONSECUTIVE_SAMPLES`; never overwritten afterwards, the same
    /// "first occurrence wins" posture as `first_hook_ground_contact_time`. `None` until a
    /// sustained post-arrest deceleration is observed (including for a bolter/touch-and-go/
    /// waveoff, which never decelerates this way).
    arrest_deceleration_onset_time: Option<f64>,
    arrest_kinematic_state: ArrestKinematicState,
    recent_health_samples: VecDeque<(f64, f64, f64)>,
    telemetry_gap_stats: OnlineMetricStats,
    capture_gap_stats: OnlineMetricStats,
    delivery_age_stats: OnlineMetricStats,
    first_sample_time: Option<f64>,
    last_sample_time: Option<f64>,
    /// Set once at groove entry (see `set_wind_reference`); `None` until then, or for the whole
    /// recovery if the wind query failed. `aoa` falls back to the raw geometric approximation
    /// whenever this is `None`.
    wind_reference: Option<WindReference>,
    /// Raw probes behind `wind_reference`, kept only for diagnosis (see `WindReferenceProbes`).
    wind_reference_probes: Option<WindReferenceProbes>,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bracket_start_time_dcs: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bracket_end_time_dcs: Option<f64>,
    /// Additive provenance for coverage recovered from the continuous groove trajectory when
    /// the point-in-time gate object itself was not retained. `None` means the normal gate
    /// capture path was used.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub coverage_source: Option<&'static str>,
}

impl Default for GateQuality {
    fn default() -> Self {
        Self {
            status: GateStatus::Missing,
            reason: Some("not_observed".to_string()),
            bracket_gap_ms: None,
            bracket_start_time_dcs: None,
            bracket_end_time_dcs: None,
            coverage_source: None,
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
    /// Whether the 3/4 NM gate counts at all, for both completeness (`all_valid`) and grading
    /// amplitude (`grade_from_gates`, `src/grading.rs`), given `groove_entry_time` (the DCS
    /// simulation time of roll-out-confirmed groove entry, CATOBAR only,
    /// `Track::groove_entry_time`). `None` always counts it -- the historical rule, and the one
    /// V/STOL keeps (see the callers in `src/grading.rs`).
    ///
    /// When `groove_entry_time` is known and the 3/4 NM gate's timestamp precedes it (or the gate
    /// was never captured at all -- groove entry's own box condition, `x <=
    /// GATE_THREE_QUARTER_NM`, guarantees the crossing already happened by the time groove entry
    /// is confirmed, present gate or not), the 3/4 NM gate **does not count**: on a real Case I
    /// pattern the aircraft is typically still turning final at that distance
    /// (`GATE_THREE_QUARTER_NM` sits well inside the base-to-final turn for a human abeam
    /// position), so a GS/lineup reading captured there reflects the turn, not a deviation from
    /// the groove the pass is actually being judged on. Confirmed live 5 September 2026 (evening,
    /// human test): the 3/4 NM gate was captured before roll-out confirmation on 7 of 8 passes,
    /// with lineup readings up to -10.5°, imposing `--` on otherwise-clean approaches regardless
    /// of what followed in the groove. `PROJECT-DERIVED`: NATOPS never makes any fixed gate
    /// crossing a qualification requirement in the first place (see `AGENTS.md`, "Gates, outcomes
    /// et câble") -- only the 1/2 NM and 1/4 NM gates, which land inside the groove on a real Case
    /// I pattern far more reliably, are kept as an unconditional requirement. Non revalidé en
    /// mission live (voir tasking-roadmap.md).
    pub(crate) fn three_quarter_counts(&self, groove_entry_time: Option<f64>) -> bool {
        let counts = match (&self.at_three_quarter_nm, groove_entry_time) {
            (Some(gate), Some(entry)) => gate.timestamp_dcs >= entry,
            (None, Some(_)) => false,
            _ => true,
        };
        tracing::debug!(
            three_quarter_nm_timestamp = self.at_three_quarter_nm.as_ref().map(|g| g.timestamp_dcs),
            groove_entry_time,
            counts,
            "3/4 NM gate eligibility (excluded when it precedes confirmed groove entry)"
        );
        counts
    }

    /// Whether the gate evidence is sufficient to award a favourable grade. See
    /// `three_quarter_counts` for what `groove_entry_time` does here.
    pub fn all_valid(&self, groove_entry_time: Option<f64>) -> bool {
        if !self.three_quarter_counts(groove_entry_time) {
            return gate_evidence_time(&self.at_half_nm, &self.half_quality)
                .zip(gate_evidence_time(
                    &self.at_quarter_nm,
                    &self.quarter_quality,
                ))
                .is_some_and(|(half, quarter)| half < quarter)
                && self.half_quality.status == GateStatus::Valid
                && self.quarter_quality.status == GateStatus::Valid;
        }

        let ordered = gate_evidence_time(&self.at_three_quarter_nm, &self.three_quarter_quality)
            .zip(gate_evidence_time(&self.at_half_nm, &self.half_quality))
            .zip(gate_evidence_time(
                &self.at_quarter_nm,
                &self.quarter_quality,
            ))
            .is_some_and(|((three_quarter, half), quarter)| three_quarter < half && half < quarter);
        ordered
            && self.three_quarter_quality.status == GateStatus::Valid
            && self.half_quality.status == GateStatus::Valid
            && self.quarter_quality.status == GateStatus::Valid
    }
}

fn gate_evidence_time(datum: &Option<GateDatum>, quality: &GateQuality) -> Option<f64> {
    datum.as_ref().map(|gate| gate.timestamp_dcs).or_else(|| {
        quality
            .bracket_start_time_dcs
            .zip(quality.bracket_end_time_dcs)
            .map(|(start, end)| start + (end - start) / 2.0)
    })
}

fn recover_gate_coverage_from_trajectory(
    trajectory: &[TrajectoryDeviation],
    gate: f64,
    datum: &Option<GateDatum>,
    quality: &mut GateQuality,
) {
    if datum.is_some() && quality.status == GateStatus::Valid {
        return;
    }

    let bracket = trajectory.windows(2).find(|pair| {
        let previous = &pair[0];
        let current = &pair[1];
        previous.distance_m > gate
            && current.distance_m <= gate
            && current.timestamp_dcs > previous.timestamp_dcs
            && (current.timestamp_dcs - previous.timestamp_dcs) * 1_000.0 <= SAMPLE_GAP_WARNING_MS
    });
    let Some([previous, current]) = bracket else {
        return;
    };

    quality.status = GateStatus::Valid;
    quality.reason = Some("covered_by_continuous_trajectory".to_string());
    quality.bracket_gap_ms = Some((current.timestamp_dcs - previous.timestamp_dcs) * 1_000.0);
    quality.bracket_start_time_dcs = Some(previous.timestamp_dcs);
    quality.bracket_end_time_dcs = Some(current.timestamp_dcs);
    quality.coverage_source = Some("continuous_trajectory_bracket");
}

/// GS/lineup deviation computed at one aircraft position along the final approach, using the
/// same geometry as a gate crossing (see `capture_gate`) but evaluated at the aircraft's own
/// along-track distance instead of a fixed gate distance.
///
/// Populated once per accepted sample from groove entry to touchdown (see
/// `Track::trajectory_deviations`), so grading can see the full approach instead of only the
/// three point-in-time gates in `GateDeviations`. `PROJECT-DERIVED`, additive to the JSON
/// contract; `gate_deviations` is unchanged and remains the primary chart/diagnostic evidence.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct TrajectoryDeviation {
    pub timestamp_dcs: f64,
    pub distance_m: f64,
    pub gs_deviation_deg: f64,
    pub lineup_deg: f64,
    /// Signed lateral displacement from the landing-area centreline in metres. Additive raw
    /// diagnostic retained alongside the normalized angular lineup used by grading.
    pub lineup_deviation_m: f64,
    /// Ground-track angle relative to the landing-area axis (0° = directly inbound). Additive
    /// diagnostic; it preserves imperfect routing and corrections after physical roll-out but is
    /// not a grading input.
    pub track_angle_deg: f64,
    /// Deck-relative altitude (metres) at this sample. Additive; lets a consumer reconstruct
    /// `sink_rate_mps` or overlay a vertical profile without needing `gs_deviation_deg` and the
    /// ideal glidepath to back it out.
    pub alt_m: f64,
    /// Aircraft roll/bank in degrees, copied from the raw telemetry sample. Informational only —
    /// a proxy for the NATOPS "attitude/wing" call, never scored. See
    /// `docs/GRADING_REFERENCE.md`, "Continuous trajectory".
    pub bank_deg: f64,
    /// Rate of altitude loss (m/s, positive = descending) since the previous continuous-trajectory
    /// sample; `0.0` for the first sample of a run. NATOPS `TMRD` proxy, informational only — see
    /// `AGENTS.md`, "Gates, outcomes et câble" (sink rate is never notated).
    pub sink_rate_mps: f64,
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
    PatternHistoryTruncated,
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
    /// Legacy worst-of capture gap and delivery age, retained for schema-v3 consumers.
    pub max_sample_gap_ms: f64,
    pub max_capture_gap_ms: f64,
    pub max_delivery_age_ms: f64,
    pub max_skew_ms: f64,
    pub warning_samples: u32,
    pub invalid_samples: u32,
    pub pattern_invalid_samples: u32,
    pub scoring_invalid_samples: u32,
    pub post_touchdown_invalid_source_observations: u32,
    pub indeterminate_invalid_source_observations: u32,
    pub outside_segment_invalid_source_observations: u32,
    pub covered_short_gap_invalid_source_observations: u32,
    pub blocking_invalid_source_observations: u32,
    pub invalid_source_observation_timeline_truncated: bool,
    pub invalid_source_observation_timeline_dropped: u32,
    pub invalid_source_observations: Vec<InvalidSourceObservation>,
    pub max_scoring_sample_gap_ms: f64,
    pub max_scoring_capture_gap_ms: f64,
    pub max_scoring_delivery_age_ms: f64,
    pub dropped_samples: u32,
    pub dropped_position_samples: u32,
    pub dropped_hook_samples: u32,
    pub dropped_event_samples: u32,
    pub sample_count: u32,
    pub effective_frequency_hz: f64,
    pub degraded_sample_ratio: f64,
    pub capture_gap_warning_ratio: f64,
    pub late_delivery_warning_ratio: f64,
    pub gap_p50_ms: f64,
    pub gap_p90_ms: f64,
    pub gap_p95_ms: f64,
    pub gap_p99_ms: f64,
    pub capture_gap_p50_ms: f64,
    pub capture_gap_p95_ms: f64,
    pub capture_gap_p99_ms: f64,
    pub delivery_age_p50_ms: f64,
    pub delivery_age_p95_ms: f64,
    pub delivery_age_p99_ms: f64,
    pub capture_time_monotonic: bool,
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
            max_capture_gap_ms: 0.0,
            max_delivery_age_ms: 0.0,
            max_skew_ms: 0.0,
            warning_samples: 0,
            invalid_samples: 0,
            pattern_invalid_samples: 0,
            scoring_invalid_samples: 0,
            post_touchdown_invalid_source_observations: 0,
            indeterminate_invalid_source_observations: 0,
            outside_segment_invalid_source_observations: 0,
            covered_short_gap_invalid_source_observations: 0,
            blocking_invalid_source_observations: 0,
            invalid_source_observation_timeline_truncated: false,
            invalid_source_observation_timeline_dropped: 0,
            invalid_source_observations: Vec::new(),
            max_scoring_sample_gap_ms: 0.0,
            max_scoring_capture_gap_ms: 0.0,
            max_scoring_delivery_age_ms: 0.0,
            dropped_samples: 0,
            dropped_position_samples: 0,
            dropped_hook_samples: 0,
            dropped_event_samples: 0,
            sample_count: 0,
            effective_frequency_hz: 0.0,
            degraded_sample_ratio: 0.0,
            capture_gap_warning_ratio: 0.0,
            late_delivery_warning_ratio: 0.0,
            gap_p50_ms: 0.0,
            gap_p90_ms: 0.0,
            gap_p95_ms: 0.0,
            gap_p99_ms: 0.0,
            capture_gap_p50_ms: 0.0,
            capture_gap_p95_ms: 0.0,
            capture_gap_p99_ms: 0.0,
            delivery_age_p50_ms: 0.0,
            delivery_age_p95_ms: 0.0,
            delivery_age_p99_ms: 0.0,
            capture_time_monotonic: true,
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

/// Measure the entry quality at the current sample. These values are diagnostics only: the Case I
/// detector below never makes lineup, track angle or lineup trend prerequisites for roll-out.
fn groove_quality_measurement(
    gate_samples: &VecDeque<ApproachSample>,
) -> Option<GrooveQualityMeasurement> {
    let (Some(oldest), Some(newest)) = (gate_samples.front(), gate_samples.back()) else {
        return None;
    };
    if newest.time <= oldest.time {
        return None;
    }
    let (vx, vy) = track_velocity_regression(gate_samples)?;
    // Track angle relative to the groove axis: 0 deg when travel is straight down -x (inbound,
    // toward the ship) with no lateral (y) drift.
    let track_angle_deg = vy.atan2(-vx).to_degrees();
    let trend_start = newest.time - GROOVE_LINEUP_TREND_WINDOW_S;
    let trend_window = gate_samples
        .iter()
        .filter(|sample| sample.time >= trend_start && sample.valid)
        .collect::<Vec<_>>();
    let lineup_rate_deg_per_s = linear_regression_slope(&trend_window, |sample| {
        sample.y.atan2(sample.x).to_degrees()
    })
    .unwrap_or(0.0);
    let lineup_deg = newest.y.atan2(newest.x).to_degrees();
    tracing::trace!(
        track_angle_deg,
        lineup_deg,
        lineup_rate_deg_per_s,
        inbound_progress_mps = -vx,
        "CATOBAR groove entry quality diagnostics"
    );
    Some(GrooveQualityMeasurement {
        track_angle_deg,
        lineup_rate_deg_per_s,
        inbound_progress_mps: -vx,
    })
}

#[derive(Debug, Clone, Copy)]
struct CaseIGrooveObservation {
    time_dcs: f64,
    x: f64,
    altitude_relative_ft: f64,
    lineup_deg: f64,
    bank_deg: f64,
    valid: bool,
    inbound: bool,
    capture_gap_ms: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CaseIGrooveUpdate {
    None,
    BranchReset,
    Confirmed,
}

impl CaseIGrooveDetector {
    fn reset_confirmation(&mut self) {
        self.rollout_started_at_dcs = None;
        self.rollout_sample_count = 0;
        if self.state == CaseIGrooveState::RolloutConfirming {
            self.state = CaseIGrooveState::LastTurnArmed;
        }
    }

    fn reset_branch(&mut self) {
        *self = Self::default();
    }

    fn observe(&mut self, observation: CaseIGrooveObservation) -> CaseIGrooveUpdate {
        if self
            .last_time_dcs
            .is_some_and(|previous| observation.time_dcs <= previous)
        {
            self.reset_branch();
            self.last_time_dcs = Some(observation.time_dcs);
            return CaseIGrooveUpdate::BranchReset;
        }
        self.last_time_dcs = Some(observation.time_dcs);

        if !observation.valid || observation.x <= 0.0 {
            self.reset_confirmation();
            return CaseIGrooveUpdate::None;
        }

        self.branch_min_x = self.branch_min_x.min(observation.x);
        if !observation.inbound
            && observation.x - self.branch_min_x > CASE_I_BRANCH_RESET_OUTBOUND_GROWTH_M
        {
            self.reset_branch();
            self.last_time_dcs = Some(observation.time_dcs);
            if observation.lineup_deg <= CASE_I_PORT_LINEUP_CORRIDOR_DEG {
                self.state = CaseIGrooveState::PatternObserved;
                self.previous_lineup_deg = Some(observation.lineup_deg);
            }
            return CaseIGrooveUpdate::BranchReset;
        }

        if observation.lineup_deg <= CASE_I_PORT_LINEUP_CORRIDOR_DEG
            && self.state == CaseIGrooveState::SearchingPattern
        {
            self.state = CaseIGrooveState::PatternObserved;
        }

        if matches!(
            self.state,
            CaseIGrooveState::LastTurnArmed | CaseIGrooveState::RolloutConfirming
        ) {
            let reached_now = observation.lineup_deg >= CASE_I_PORT_LINEUP_CORRIDOR_DEG;
            let crossed_now = self.previous_lineup_deg.is_some_and(|previous| {
                previous < CASE_I_PORT_LINEUP_CORRIDOR_DEG
                    && observation.lineup_deg >= CASE_I_PORT_LINEUP_CORRIDOR_DEG
            });
            if !self.corridor_reached && (reached_now || crossed_now) {
                self.corridor_reached = true;
                self.corridor_crossing_time_dcs = Some(observation.time_dcs);
            }
        }
        self.previous_lineup_deg = Some(observation.lineup_deg);

        if self.state == CaseIGrooveState::PatternObserved
            && observation.inbound
            && observation.altitude_relative_ft <= CASE_I_LAST_TURN_ARM_MAX_ALTITUDE_FT
            && observation.bank_deg.abs() > GROOVE_ROLLOUT_MAX_BANK_DEG
        {
            self.state = CaseIGrooveState::LastTurnArmed;
        }

        if !matches!(
            self.state,
            CaseIGrooveState::LastTurnArmed | CaseIGrooveState::RolloutConfirming
        ) {
            return CaseIGrooveUpdate::None;
        }

        if !observation.inbound
            || observation.altitude_relative_ft > CASE_I_LAST_TURN_ARM_MAX_ALTITUDE_FT
            || observation.capture_gap_ms > SAMPLE_GAP_WARNING_MS
            || observation.bank_deg.abs() > GROOVE_ROLLOUT_MAX_BANK_DEG
        {
            self.reset_confirmation();
            return CaseIGrooveUpdate::None;
        }

        if self.state == CaseIGrooveState::LastTurnArmed {
            self.state = CaseIGrooveState::RolloutConfirming;
            self.rollout_started_at_dcs = Some(observation.time_dcs);
            self.rollout_sample_count = 1;
            return CaseIGrooveUpdate::None;
        }

        self.rollout_sample_count += 1;
        let rollout_started_at = self
            .rollout_started_at_dcs
            .expect("roll-out confirmation state always has a start time");
        if observation.time_dcs - rollout_started_at >= GROOVE_ENTRY_STABILITY_DURATION_S {
            self.state = CaseIGrooveState::GrooveConfirmed;
            CaseIGrooveUpdate::Confirmed
        } else {
            CaseIGrooveUpdate::None
        }
    }

    fn evidence(
        &self,
        observation: CaseIGrooveObservation,
        quality: GrooveQualityMeasurement,
        utc_mapping_status: &'static str,
        confirmation_received_unix_ms: Option<u64>,
    ) -> GrooveEntryEvidence {
        let rollout_started_at_dcs = self
            .rollout_started_at_dcs
            .expect("confirmed roll-out always has a start time");
        GrooveEntryEvidence {
            timestamp_dcs: observation.time_dcs,
            utc_mapping_status,
            confirmation_received_unix_ms,
            distance_m: observation.x,
            lineup_deg: observation.lineup_deg,
            bank_deg: observation.bank_deg,
            track_angle_deg: quality.track_angle_deg,
            lineup_rate_deg_per_s: quality.lineup_rate_deg_per_s,
            stability_duration_s: observation.time_dcs - rollout_started_at_dcs,
            stability_sample_count: self.rollout_sample_count,
            trigger: "case_i_port_final_turn_rollout_sustained",
            criteria: GrooveEntryCriteria::default(),
            rollout_started_at_dcs,
            altitude_relative_ft: observation.altitude_relative_ft,
            inbound_progress_mps: quality.inbound_progress_mps,
            approach_side: "port",
            port_lineup_corridor_reached: self.corridor_reached,
            port_lineup_corridor_crossing_time_dcs: self.corridor_crossing_time_dcs,
            last_turn_arm_state: "last_turn_armed",
            last_turn_arm_reason: "port_pattern_turn_observed_below_600_ft_while_inbound",
            decision_semantics:
                "physical_rollout_bank_and_inbound_only_quality_diagnostics_do_not_block",
        }
    }
}

/// Least-squares linear regression of `x` and `y` against `time` over every *valid* sample in
/// `window`, returning the fitted ground-track velocity `(dx/dt, dy/dt)`.
///
/// Replaces comparing only the two endpoint samples of the buffer: a single noisy or skewed
/// sample sitting right at either edge used to swing the whole track-angle estimate on its own,
/// since it was one of only two points consulted. Fitting a trend line through every sample in
/// the window means one noisy frame is outweighed by the rest of the window instead of dictating
/// the result outright -- a precision improvement only, not a change to what "wings level,
/// tracking down the groove" means (see the constants block above); it introduces no new
/// `PROJECT-DERIVED` threshold, and does not change `GROOVE_ROLLOUT_MAX_TRACK_ANGLE_DEG` itself.
/// Only valid, inbound, approach-side samples contribute to the regression.
fn track_velocity_regression(window: &VecDeque<ApproachSample>) -> Option<(f64, f64)> {
    let valid = window
        .iter()
        .filter(|sample| sample.valid)
        .collect::<Vec<_>>();
    Some((
        linear_regression_slope(&valid, |sample| sample.x)?,
        linear_regression_slope(&valid, |sample| sample.y)?,
    ))
}

fn linear_regression_slope(
    samples: &[&ApproachSample],
    value: impl Fn(&ApproachSample) -> f64,
) -> Option<f64> {
    if samples.len() < 2 {
        return None;
    }
    let count = samples.len() as f64;
    let t_mean = samples.iter().map(|sample| sample.time).sum::<f64>() / count;
    let value_mean = samples.iter().map(|sample| value(sample)).sum::<f64>() / count;

    let mut s_tt = 0.0;
    let mut s_tv = 0.0;
    for sample in samples {
        let dt = sample.time - t_mean;
        s_tt += dt * dt;
        s_tv += dt * (value(sample) - value_mean);
    }
    if s_tt <= 0.0 {
        return None;
    }
    Some(s_tv / s_tt)
}

#[derive(Debug, Default, PartialEq, Eq, serde::Serialize)]
pub enum Grading {
    #[default]
    Unknown,
    /// A geometrically recognisable final approach whose terminal outcome could not be proven.
    /// The measured approach may still be graded; this is distinct from both a waveoff and a
    /// false start that never became an approach.
    ApproachOnly,
    Bolter,
    TouchAndGo {
        cable_estimated: Option<u8>,
    },
    /// Pilot broke off the approach after entering the detected groove.
    WaveoffUnknown,
    Recovered {
        cable: Option<u8>,
        cable_estimated: Option<u8>,
    },
}

impl Grading {
    /// Human-readable outcome summary for pilot-facing surfaces (Discord embed, PNG chart,
    /// SQLite/greenie-board log).
    ///
    /// Unlike the full JSON `outcome` field (built separately, alongside the raw
    /// `wire_estimated`/`wire_dcs`/`wire_divergent` fields, for anyone deliberately inspecting the
    /// report), this never places a Rust-estimated wire number next to a DCS/LQM one: DCS
    /// evidence is shown alone whenever available. A pilot who saw wire 3 called out in DCS
    /// should never read a contradicting "Rust estimate 4" in the same headline they glance at
    /// right after landing — DCS is always the authority for anything the pilot sees at a glance,
    /// exactly as it already is for grading (see `Completeness::UnconfirmedArrest`).
    pub fn pilot_facing_outcome(&self, is_vstol: bool) -> String {
        match (is_vstol, self) {
            (_, Self::Unknown) => String::new(),
            (_, Self::ApproachOnly) => "Approach only — outcome unknown".to_string(),
            (_, Self::Bolter) => "Bolter".to_string(),
            // Intentional bolters are valid only for arrested recoveries. Keep the
            // V/STOL fallback defensive in case an invalid grading reaches this layer.
            (true, Self::TouchAndGo { .. }) => "Waveoff/Go-around".to_string(),
            // An intentional bolter still tells the pilot which wire the hook would have caught;
            // the wording keeps it apart from an arrestment.
            (
                false,
                Self::TouchAndGo {
                    cable_estimated: Some(estimated),
                },
            ) => format!("T&G (CQ) — would have caught wire {estimated}"),
            (false, Self::TouchAndGo { .. }) => "T&G (CQ)".to_string(),
            (_, Self::WaveoffUnknown) => "Waveoff/Go-around — initiator unknown".to_string(),
            (true, Self::Recovered { .. }) => "Spot 7.5".to_string(),
            (
                false,
                Self::Recovered {
                    cable,
                    cable_estimated,
                },
            ) => match (cable, cable_estimated) {
                (Some(dcs), _) => format!("Arrested — wire {dcs}"),
                (None, Some(estimated)) => format!("Wire #{estimated} (Rust estimate)"),
                (None, None) => "Arrested — wire evidence unavailable".to_string(),
            },
        }
    }
}

#[derive(Debug, PartialEq)]
pub struct TrackResult {
    pub pilot_name: String,
    pub grading: Grading,
    /// Approach grade before any V/STOL touchdown bonus. CASE I CATOBAR uses the episode
    /// classifier; V/STOL retains its gate-average model.
    pub approach_grade: PassGrade,
    /// Final display grade. For CATOBAR this is identical to approach_grade;
    /// for V/STOL it includes the spot-7.5 bonus.
    pub pass_grade: PassGrade,
    /// Final numeric score. Kept separately because V/STOL bonuses can produce
    /// quarter-point values (e.g. 4.75) while reusing the CATOBAR labels.
    pub grade_points: Option<f64>,
    /// Short, plain-language explanation of the specific rule that produced `approach_grade`
    /// (see `grading::grade_from_gates_with_reason`/`compute_pass_grade_with_reason`) -- built
    /// alongside the grade itself, not reconstructed afterward, so it can never disagree with
    /// it. Used for the Discord "Why This Grade" field alongside the telemetry-availability
    /// message when grading itself was unavailable (that message takes priority; see
    /// `record_recovery.rs`). A short generic placeholder for V/STOL, which does not yet have a
    /// detailed per-gate breakdown of its own.
    pub grade_reason: String,
    /// Additive schema-v3 audit trail for the PROJECT-DERIVED CASE I CATOBAR classifier.
    /// Empty for V/STOL and for tracks without a continuous groove trajectory.
    pub grading_episodes: Vec<GradingEpisode>,
    pub spot_grade: Option<SpotGrade>,
    pub spot_distance_m: Option<f64>,
    pub intended_spot: Option<&'static str>,
    pub actual_nearest_spot: Option<&'static str>,
    pub dcs_grading: Option<String>,
    pub gate_deviations: GateDeviations,
    pub trajectory_deviations: Vec<TrajectoryDeviation>,
    pub datums: Vec<Datum>,
    pub pattern_datums: Vec<PatternDatum>,
    pub plane_info: &'static AirplaneInfo,
    pub carrier_info: &'static CarrierInfo,
    /// Time from groove entry to touchdown in seconds, if both were recorded.
    pub groove_time_secs: Option<f64>,
    /// Exact CATOBAR Case I roll-out criteria and measurements that latched groove entry.
    pub groove_entry: Option<GrooveEntryEvidence>,
    pub touchdown_time_dcs: Option<f64>,
    pub telemetry_quality: TelemetryQuality,
    pub events: Vec<EventEvidence>,
    pub spot_zone: SpotZoneObservation,
    /// Raw horizontal speed at the first accepted touchdown evidence. No
    /// VL/RVL threshold is applied before the live corpus is validated.
    pub touchdown_horizontal_speed_mps: Option<f64>,
    pub hook_observation: HookObservation,
    pub wire_estimation: WireEstimateEvidence,
    pub arrest_confirmation: ArrestConfirmationEvidence,
    /// Commanded hook state used for the bolter / touch-and-go decision.
    pub hook_state: HookState,
    /// What proved the arrest: `dcs_wire`, `hook_transient`, `kinematic`, `unconfirmed` for an
    /// arrested-carrier contact nothing confirmed, or `none` for non-arrest outcomes and V/STOL.
    pub arrest_evidence: &'static str,
    /// Whether a wind reference (see `WindReference`) was established for this recovery, i.e.
    /// whether `datums[].aoa`/`pattern_datums[].aoa` are wind-corrected or fell back to the raw
    /// geometric approximation for the whole recovery (query failure, or the aircraft never
    /// entered the groove).
    pub wind_reference_established: bool,
    /// Raw probes behind `wind_reference_established` when it is `true` (see
    /// `WindReferenceProbes`); `None` both when the reference was never established and when it
    /// was established through a path that predates this diagnostic (kept purely for live
    /// investigation, never for grading).
    pub wind_reference_probes: Option<WindReferenceProbes>,
}

impl Track {
    pub(crate) fn last_observed_time_dcs(&self) -> Option<f64> {
        self.datums
            .last()
            .map(|datum| datum.time)
            .or(self.previous_sample_time)
    }

    pub(crate) fn has_recognisable_approach(&self) -> bool {
        self.entered_groove
            || self.gate_deviations.at_three_quarter_nm.is_some()
            || self.gate_deviations.at_half_nm.is_some()
            || self.gate_deviations.at_quarter_nm.is_some()
            || !self.trajectory_deviations.is_empty()
    }

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
            trajectory_deviations: Default::default(),
            entered_groove: false,
            case_i_groove_detector: CaseIGrooveDetector::default(),
            groove_entry_time: None,
            groove_entry_evidence: None,
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
                // "test_corpus" for the F/A-18C: its `<= 0.2` up / `>= 0.8` down mapping was
                // empirically confirmed first. The T-45 and F-14B(U) share the same draw-argument
                // convention (a single 0..1 hook animation) and were confirmed live in both
                // directions on 5 September 2026 (human touch-and-go and arrested traps).
                // F-14A/F-14B share the same draw-argument index as the F-14B(U) but remain an
                // unconfirmed extrapolation pending their own human recording.
                polarity: match (plane_info.name, plane_info.hook_draw_argument) {
                    ("F/A-18C Hornet", Some(_)) => "fa18c_zero_up_one_down_test_corpus",
                    ("T-45C Goshawk" | "F-14B(U) Tomcat", Some(_)) => {
                        "zero_up_one_down_confirmed_live_20260905"
                    }
                    (_, Some(_)) => "assumed_zero_up_one_down_pending_live_validation",
                    (_, None) => "unknown_pending_live_validation",
                },
                interpreted_state: "unknown",
                timeline_capacity: MAX_HOOK_EVIDENCE,
                timeline_retention_policy:
                    "bounded_recent_ring_prioritizes_final_window_and_contact",
                ..HookObservation::default()
            },
            crossed_deck_threshold: false,
            deck_crossing_confirmed_contact: false,
            first_hook_ground_contact_time: None,
            deck_crossing_time: None,
            deck_kinematics: Vec::new(),
            telemetry_quality: TelemetryQuality::default(),
            source_capture_anchors: Vec::new(),
            events: Vec::new(),
            spot_zone: SpotZoneObservation::default(),
            touchdown_horizontal_speed_mps: None,
            health_red_announced: false,
            previous_wire_plane: [None; 4],
            wire_crossings: Vec::new(),
            previous_horizontal_speed: None,
            deceleration_run_count: 0,
            deceleration_run_start_time: None,
            arrest_deceleration_onset_time: None,
            arrest_kinematic_state: ArrestKinematicState::default(),
            recent_health_samples: VecDeque::new(),
            telemetry_gap_stats: OnlineMetricStats::default(),
            capture_gap_stats: OnlineMetricStats::default(),
            delivery_age_stats: OnlineMetricStats::default(),
            first_sample_time: None,
            last_sample_time: None,
            wind_reference: None,
            wind_reference_probes: None,
        }
    }

    /// Whether the aircraft has entered the groove.
    /// Used by the caller to know when to query and supply a wind reference (see
    /// `set_wind_reference`) for the AoA correction.
    pub fn entered_groove(&self) -> bool {
        self.entered_groove
    }

    /// Supply the two altitude/wind samples used to correct the AoA approximation from now on
    /// (see `WindReference`). Call once, as soon as `entered_groove()` becomes `true`; a second
    /// call is a no-op only in the sense that it simply replaces the reference — in practice the
    /// caller should only ever call this once per recovery.
    pub fn set_wind_reference(&mut self, alt_a_m: f64, wind_a: DVec3, alt_b_m: f64, wind_b: DVec3) {
        self.wind_reference = Some(WindReference {
            alt_a_m,
            wind_a,
            alt_b_m,
            wind_b,
        });
    }

    /// Record the two raw `GetWind` responses behind the just-established `WindReference`, purely
    /// for diagnosis (see `WindReferenceProbes`). Call alongside `set_wind_reference`, with the
    /// same two probes before they were converted to velocity vectors.
    pub fn set_wind_reference_probes(
        &mut self,
        high: WindProbe,
        low: WindProbe,
        low_reading_overridden_by_high: bool,
    ) {
        self.wind_reference_probes = Some(WindReferenceProbes {
            high,
            low,
            low_reading_overridden_by_high,
        });
    }

    /// The AoA to record for this sample: wind-corrected if a reference is available, otherwise
    /// the raw geometric approximation already computed on `plane.aoa` (see `Transform::from`).
    fn effective_aoa(&self, plane: &Transform) -> f64 {
        match &self.wind_reference {
            Some(reference) => corrected_aoa_deg(
                plane.velocity,
                reference.at_altitude(plane.alt),
                plane.rotation,
            ),
            None => plane.aoa,
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
        if sample.is_valid()
            && sample_time.is_finite()
            && self.source_capture_anchors.len() < MAX_TRACK_SAMPLES
        {
            if let (Some(sequence), Some(capture_tick)) =
                (sample.source_sequence, sample.source_capture_tick)
            {
                self.source_capture_anchors.push(SourceCaptureAnchor {
                    sequence,
                    capture_tick,
                    capture_time_dcs: sample_time,
                });
            }
        }
        let observed_gap_ms = sample.sample_gap_ms.max(sample.source_age_ms);
        self.previous_sample_time = Some(sample_time);
        self.first_sample_time.get_or_insert(sample_time);
        self.last_sample_time = Some(sample_time);
        self.telemetry_gap_stats.observe(observed_gap_ms);
        self.capture_gap_stats.observe(sample.sample_gap_ms);
        self.delivery_age_stats.observe(sample.source_age_ms);
        self.telemetry_quality.sample_count =
            self.telemetry_gap_stats.count().min(u64::from(u32::MAX)) as u32;
        self.telemetry_quality.max_source_age_ms = self
            .telemetry_quality
            .max_source_age_ms
            .max(sample.source_age_ms);
        self.recent_health_samples.push_back((
            sample_time,
            sample.sample_gap_ms,
            sample.source_age_ms,
        ));
        while self
            .recent_health_samples
            .front()
            .is_some_and(|(time, _, _)| sample_time - time > HEALTH_WINDOW_S)
        {
            self.recent_health_samples.pop_front();
        }
        self.telemetry_quality.max_sample_gap_ms = self
            .telemetry_quality
            .max_sample_gap_ms
            .max(observed_gap_ms);
        self.telemetry_quality.max_capture_gap_ms = self
            .telemetry_quality
            .max_capture_gap_ms
            .max(sample.sample_gap_ms);
        self.telemetry_quality.max_delivery_age_ms = self
            .telemetry_quality
            .max_delivery_age_ms
            .max(sample.source_age_ms);
        self.telemetry_quality.max_skew_ms = self.telemetry_quality.max_skew_ms.max(sample.skew_ms);
        if sample.has_warning() {
            self.telemetry_quality.warning_samples += 1;
        }
        let window_span_s = self
            .recent_health_samples
            .front()
            .map_or(0.0, |(time, _, _)| sample_time - time);
        let window_frequency_hz = if window_span_s > 0.0 {
            (self.recent_health_samples.len().saturating_sub(1)) as f64 / window_span_s
        } else {
            0.0
        };
        let (capture_gap_ratio, late_delivery_ratio) = if self.recent_health_samples.is_empty() {
            (0.0, 0.0)
        } else {
            let count = self.recent_health_samples.len() as f64;
            (
                self.recent_health_samples
                    .iter()
                    .filter(|(_, gap, _)| *gap > SAMPLE_GAP_WARNING_MS)
                    .count() as f64
                    / count,
                self.recent_health_samples
                    .iter()
                    .filter(|(_, _, age)| *age > SAMPLE_GAP_WARNING_MS)
                    .count() as f64
                    / count,
            )
        };
        let (current_health, current_health_reason) = if sample.invalid_reason.is_some()
            || sample.sample_gap_ms > crate::telemetry::SAMPLE_GAP_INCOMPLETE_MS
            || sample.source_age_ms > crate::telemetry::SAMPLE_GAP_INCOMPLETE_MS
        {
            (TelemetryHealth::Red, "invalid_or_incomplete_sample")
        } else if window_span_s >= 5.0 && (window_frequency_hz < 6.0 || capture_gap_ratio >= 0.15) {
            (TelemetryHealth::Red, "sustained_capture_gap")
        } else if window_span_s >= 5.0 && late_delivery_ratio >= 0.15 {
            (TelemetryHealth::Red, "sustained_delivery_latency")
        } else if window_span_s >= 5.0 && (window_frequency_hz < 8.0 || capture_gap_ratio >= 0.05) {
            (TelemetryHealth::Orange, "degraded_capture_cadence")
        } else if window_span_s >= 5.0 && late_delivery_ratio >= 0.05 {
            (TelemetryHealth::Orange, "degraded_delivery_latency")
        } else if sample.sample_gap_ms > SAMPLE_GAP_WARNING_MS {
            (TelemetryHealth::Orange, "capture_gap_warning")
        } else if sample.source_age_ms > SAMPLE_GAP_WARNING_MS {
            (TelemetryHealth::Orange, "late_delivery_warning")
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
                capture_gap_ratio,
                late_delivery_ratio,
                "live grading health is red"
            );
            self.health_red_announced = true;
        }
        if let Some(reason) = sample.invalid_reason {
            self.telemetry_quality.invalid_samples += 1;
            if !self.telemetry_quality.reasons.contains(&reason) {
                self.telemetry_quality.reasons.push(reason);
            }
            if reason == TelemetryInvalidReason::TimeWentBackwards {
                self.telemetry_quality.capture_time_monotonic = false;
            }
        }

        // ---------------------------------------------------------------
        // Invalid-sample boundary. Everything above this line is telemetry-quality accounting,
        // which an invalid sample must contribute to (its gap, its reason, its health effect).
        // Everything below that mutates geometric state -- pattern datums, the carrier
        // smoothing, the exit-zone test, hook evidence, the distance minima and the outcome
        // decision, deck contact -- is gated on `sample_valid`. Gate windows, the roll-out
        // detector and the datum record carry the validity flag themselves and still see the
        // sample, so gap attribution and chart gaps keep working (review finding F05).
        // ---------------------------------------------------------------
        let sample_valid = sample.is_valid();

        // ---------------------------------------------------------------
        // Pattern datum — BRC frame, recorded every frame.
        // Origin = carrier position. x_chart = -port_m, y_chart = -astern_m
        // so the circuit appears with port on the left and the carrier at the
        // top of the overview PNG.
        // ---------------------------------------------------------------
        if sample_valid {
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
                    aoa: self.effective_aoa(plane),
                });
            } else {
                // Only the oldest, non-scoring overview history is compacted. This is a chart
                // limitation, not loss of source telemetry or scoring evidence.
                self.pattern_datums.drain(..MAX_TRACK_SAMPLES / 4);
                self.pattern_datums.push(PatternDatum {
                    time: plane.time,
                    astern_m,
                    port_m,
                    alt_ft: m_to_ft(plane.alt),
                    aoa: self.effective_aoa(plane),
                });
                if !self
                    .telemetry_quality
                    .diagnostics
                    .contains(&DiagnosticCause::PatternHistoryTruncated)
                {
                    self.telemetry_quality
                        .diagnostics
                        .push(DiagnosticCause::PatternHistoryTruncated);
                }
            }
        }

        // Smooth carrier position to eliminate DCS quantisation sawtooth.
        // The carrier's world position updates in discrete jumps (~every 1.4 s);
        // between updates, the same stale position is returned.  EMA blends the
        // raw position toward the smoothed estimate each frame, producing a
        // steady progression instead of a stairstep.
        let smoothed_pos = match (self.smoothed_carrier_pos, sample_valid) {
            (Some(prev), true) => {
                let s = prev + (carrier.position - prev) * CARRIER_POS_SMOOTH_ALPHA;
                self.smoothed_carrier_pos = Some(s);
                s
            }
            // An invalid sample neither advances nor seeds the smoothing.
            (Some(prev), false) => prev,
            (None, true) => {
                self.smoothed_carrier_pos = Some(carrier.position);
                carrier.position
            }
            (None, false) => carrier.position,
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
        if sample_valid && (m_to_nm(carrier_distance) > 3.5 || m_to_ft(plane.alt) > 1100.0) {
            tracing::debug!("stop: plane exited pattern detection zone");
            return false;
        }

        let distance = ray_from_plane_to_carrier.mag();
        if sample.is_valid() {
            self.observe_vstol_spot_zone(carrier, plane);
        }
        let is_arrested_recovery = matches!(&self.carrier_info.recovery, CarrierRecovery::Arrested);
        if is_arrested_recovery && sample_valid {
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
                self.observe_horizontal_deceleration(plane);
                let hook_altitude_m = plane.alt - self.carrier_info.deck_altitude
                    + self.plane_info.hook.rotated_by(plane.rotation).y;
                self.observe_arrest_kinematics(sample, carrier, plane, hook_altitude_m);
                // Carrier-relative deck kinematics for the displacement-based arrest
                // confirmation (see `evaluate_deck_arrest_kinematics`). Relative to the *raw*
                // carrier position on purpose: the smoothed one lags every ship-position step.
                if self.entered_groove && self.deck_kinematics.len() < MAX_DECK_KINEMATIC_SAMPLES {
                    let x = self.deck_axis_x(carrier, ray_from_plane_to_carrier);
                    let relative_position = plane.position - sample.carrier_raw.position;
                    if x <= DECK_KINEMATICS_START_X_M
                        && relative_position.x.is_finite()
                        && relative_position.z.is_finite()
                    {
                        self.deck_kinematics.push(DeckKinematicSample {
                            time: plane.time,
                            relative_position,
                            x,
                        });
                    }
                }
            }
        }

        // Track the minimum distance to the touchdown point. Only a valid sample may move the
        // floor or decide that the aircraft is leaving.
        if sample_valid && distance < self.previous_distance {
            self.previous_distance = distance;
            if is_arrested_recovery {
                self.min_distance_state = Some((carrier.clone(), plane.clone()));
            }
        } else if sample_valid && distance - self.previous_distance > 150.0 {
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
                    // A kinematically confirmed arrest cannot become a bolter or a touch-and-go:
                    // the aircraft is simply taxiing / moving with the deck after the trap.
                    if self.evaluate_deck_arrest_kinematics().confirmed {
                        tracing::debug!(
                            distance_in_m = distance,
                            "arrested aircraft moving with the deck; stop tracking"
                        );
                        return false;
                    }
                    if self.calibrated_hook_state() == HookState::Up {
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
                    // A deck crossing without an arrest is a bolter -- but only once physical
                    // contact is actually confirmed (`deck_crossing_confirmed_contact`). This
                    // branch never has an event-correlated touchdown (that case is handled by the
                    // `Some(Grading::Recovered { .. })` arm above); relying on
                    // `crossed_deck_threshold` alone let a plain low fly-over (confirmed live at
                    // 8.7 m / ~28 ft, comfortably inside the 50 ft go-around guard) or a DCS
                    // `GRADE:WO` waveoff be misclassified as `Bolter` with zero proof of a touch.
                    // Hook draw arguments are retained as raw evidence but not interpreted until
                    // polarity is validated for the deployed modules.
                    if self.crossed_deck_threshold && self.min_distance_state.is_some() {
                        if self.calibrated_hook_state() == HookState::Up {
                            tracing::debug!("qualification touch-and-go detected");
                            // `cable_estimated` is reconciled once in `finish()` from
                            // `wire_estimation`, against the complete wire-crossing history
                            // rather than a snapshot that may still be missing a crossing the
                            // current position tick has not observed yet.
                            self.grading = Some(Grading::TouchAndGo {
                                cable_estimated: None,
                            });
                            return false;
                        }
                        if self.deck_crossing_confirmed_contact {
                            tracing::debug!(
                                distance_in_m = distance,
                                "bolter detected (deck crossing, confirmed contact, no arrest)"
                            );
                            self.grading = Some(Grading::Bolter);
                        } else {
                            tracing::debug!(
                                distance_in_m = distance,
                                "waveoff detected (deck crossing without confirmed contact)"
                            );
                            self.grading = Some(Grading::WaveoffUnknown);
                        }
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

        if sample_valid
            && is_arrested_recovery
            && self.entered_groove
            && self.crossed_deck_threshold
        {
            // Arrest confirmed purely from deck kinematics (no `Land`/`RunwayTouch` at all, e.g.
            // missing DCS events): establish the outcome instead of letting the parked aircraft
            // read `ApproachOnly`, or `Bolter` once it taxis.
            if self.grading.is_none() {
                let kinematics = self.evaluate_deck_arrest_kinematics();
                if kinematics.confirmed {
                    tracing::debug!(?kinematics, "arrest confirmed from deck kinematics");
                    self.grading = Some(Grading::Recovered {
                        cable: None,
                        cable_estimated: None,
                    });
                    return true;
                }
            }
            // Eventless arrest: nothing else will ever end this track (the aircraft is stationary
            // relative to the ship), so stop after a bounded evidence window measured from the
            // moment it first went slow.
            if matches!(self.grading, Some(Grading::Recovered { .. }))
                && self.landing_time.is_none()
            {
                let kinematics = self.evaluate_deck_arrest_kinematics();
                if kinematics.confirmed
                    && kinematics
                        .slow_since_dcs
                        .is_some_and(|slow| plane.time - slow >= POST_ARREST_EVIDENCE_WINDOW_S)
                {
                    tracing::debug!("kinematic arrest evidence window elapsed; stop tracking");
                    return false;
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

        if sample_valid
            && matches!(self.carrier_info.recovery, CarrierRecovery::Arrested)
            && self.first_hook_ground_contact_time.is_none()
            && alt <= 0.0
        {
            self.first_hook_ground_contact_time = Some(plane.time);
        }

        let scoring_relevant =
            self.entered_groove || (x > 0.0 && x <= GATE_THREE_QUARTER_NM && m_to_ft(alt) <= 500.0);
        if scoring_relevant {
            self.telemetry_quality.max_scoring_sample_gap_ms = self
                .telemetry_quality
                .max_scoring_sample_gap_ms
                .max(sample.sample_gap_ms.max(sample.source_age_ms));
            self.telemetry_quality.max_scoring_capture_gap_ms = self
                .telemetry_quality
                .max_scoring_capture_gap_ms
                .max(sample.sample_gap_ms);
            self.telemetry_quality.max_scoring_delivery_age_ms = self
                .telemetry_quality
                .max_scoring_delivery_age_ms
                .max(sample.source_age_ms);
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
        if sample.is_valid()
            && self.previous_x > 0.0
            && x <= 0.0
            && m_to_ft(alt) <= DECK_CROSSING_ALT_CAP_FT
        {
            self.crossed_deck_threshold = true;
            self.deck_crossing_confirmed_contact = alt <= DECK_CONTACT_CONFIRMATION_ALT_M;
            if self.entered_groove {
                self.deck_crossing_time.get_or_insert(plane.time);
            }
        }

        if x > 0.0 {
            // Robust reset: if the aircraft flies outbound (e.g., into the pattern after a bolter),
            // clear any gates or groove entry that were captured so they can be freshly recorded
            // on the real final approach inbound.
            if x > GATE_THREE_QUARTER_NM {
                self.gate_deviations.at_three_quarter_nm = None;
                self.gate_deviations.three_quarter_quality = GateQuality::default();
                self.crossed_deck_threshold = false;
                self.deck_crossing_confirmed_contact = false;
                self.first_hook_ground_contact_time = None;
                self.deck_crossing_time = None;
                self.deck_kinematics.clear();
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
            // V/STOL keeps its historical box-only detector. CATOBAR Case I instead recognizes
            // the port-side pattern and final turn, then confirms physical roll-out from bank
            // and inbound progress. The 3/4 NM gate, lineup, route and lineup trend do not block
            // that decision; their values remain diagnostics and subsequent deviations are kept.
            if self.carrier_info.is_vstol()
                && x <= GATE_THREE_QUARTER_NM
                && m_to_ft(alt) <= 300.0
                && lineup_deg.abs() <= 10.0
            {
                if !self.entered_groove {
                    self.mark_fresh_groove_entry(plane.time);
                }
                self.entered_groove = true;
            }

            if !self.carrier_info.is_vstol() {
                let observation = CaseIGrooveObservation {
                    time_dcs: plane.time,
                    x,
                    altitude_relative_ft: m_to_ft(alt),
                    lineup_deg,
                    bank_deg: plane.roll,
                    valid: sample.is_valid(),
                    inbound: is_inbound,
                    capture_gap_ms: sample.sample_gap_ms,
                };
                match self.case_i_groove_detector.observe(observation) {
                    CaseIGrooveUpdate::Confirmed if !self.entered_groove => {
                        let quality = groove_quality_measurement(&self.gate_samples).unwrap_or(
                            GrooveQualityMeasurement {
                                track_angle_deg: 0.0,
                                lineup_rate_deg_per_s: 0.0,
                                inbound_progress_mps: 0.0,
                            },
                        );
                        self.groove_entry_evidence = Some(
                            self.case_i_groove_detector.evidence(
                                observation,
                                quality,
                                "unavailable_no_exact_dcs_utc_anchor",
                                (sample.plane_received_unix_ms != 0)
                                    .then_some(sample.plane_received_unix_ms),
                            ),
                        );
                        self.mark_fresh_groove_entry(plane.time);
                        self.entered_groove = true;
                    }
                    CaseIGrooveUpdate::BranchReset if !self.entered_groove => {
                        self.groove_entry_time = None;
                        self.groove_entry_evidence = None;
                    }
                    _ => {}
                }
            }

            // Continuous GS/lineup series, from groove entry to touchdown, using the same
            // geometry as a gate crossing but evaluated at the aircraft's own distance `x`
            // instead of a fixed gate. See `TrajectoryDeviation` for why this exists: it lets
            // grading see excursions between the three point-in-time gates, not just at them.
            if self.entered_groove
                && sample.is_valid()
                && is_inbound
                && (!self.carrier_info.is_vstol() || in_approach)
                && gate_lined_up
                && x >= TRAJECTORY_MIN_DISTANCE_M
                && self.trajectory_deviations.len() < MAX_TRAJECTORY_SAMPLES
            {
                let ideal_alt = ideal_base_alt + x * self.plane_info.glide_slope.to_radians().tan();
                let gs_deviation_m = alt - ideal_alt;
                let (gs_deviation_deg, trajectory_lineup_deg) =
                    trajectory_deviation_angles_deg(gs_deviation_m, y, x);
                let sink_rate_mps =
                    sink_rate_since(self.trajectory_deviations.last(), alt, plane.time);
                self.trajectory_deviations.push(TrajectoryDeviation {
                    timestamp_dcs: plane.time,
                    distance_m: x,
                    gs_deviation_deg,
                    lineup_deg: trajectory_lineup_deg,
                    lineup_deviation_m: y,
                    track_angle_deg: groove_quality_measurement(&self.gate_samples)
                        .map_or(0.0, |quality| quality.track_angle_deg),
                    alt_m: alt,
                    bank_deg: plane.roll,
                    sink_rate_mps,
                });
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
                aoa: self.effective_aoa(plane),
                alt: alt.max(0.0),
                roll_deg: plane.roll,
                carrier_time: sample.carrier_raw.time,
                plane_time: sample.plane_raw.time,
                carrier_received_unix_ms: sample.carrier_received_unix_ms,
                plane_received_unix_ms: sample.plane_received_unix_ms,
                sample_gap_ms: sample.sample_gap_ms.max(sample.source_age_ms),
                capture_gap_ms: sample.sample_gap_ms,
                delivery_age_ms: sample.source_age_ms,
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

    /// Admissibility is decided before anything is mutated (review finding F03): a duplicate
    /// `Land`/`RunwayTouch` (DCS delivers two for one V/STOL landing) or an event whose geometry
    /// does not belong to this pair must leave the spot evidence, the terminal datum and the
    /// outcome exactly as the first accepted event left them.
    pub fn landed(&mut self, carrier: &Transform, plane: &Transform) -> bool {
        if matches!(self.grading, Some(Grading::Recovered { .. })) {
            tracing::warn!(at = plane.time, "duplicate touchdown ignored");
            return false;
        }
        if !carrier.has_finite_geometry() || !plane.has_finite_geometry() {
            tracing::warn!(
                at = plane.time,
                "touchdown event rejected: non-finite transform"
            );
            return false;
        }
        // Arrested: the hook must be near the ideal touchdown point. V/STOL: the pilot-ground
        // reference must be near the *nearest active spot*, not the approach reference (which is
        // an axis one wingspan outside the port deck edge), so a legitimate landing on another
        // spot of the same ship is never rejected as foreign geometry.
        let horizontal_distance = match &self.carrier_info.recovery {
            CarrierRecovery::Arrested => {
                let plane_reference =
                    plane.position + self.plane_info.hook.rotated_by(plane.rotation);
                let carrier_reference = carrier.position
                    + self
                        .carrier_info
                        .approach_reference_offset(self.plane_info)
                        .rotated_by(carrier.rotation);
                DVec3::new(
                    plane_reference.x - carrier_reference.x,
                    0.0,
                    plane_reference.z - carrier_reference.z,
                )
                .mag()
            }
            CarrierRecovery::Vstol { .. } => {
                let spot_ref_world =
                    plane.position + self.plane_info.landing_reference.rotated_by(plane.rotation);
                let spot_ref_local =
                    (spot_ref_world - carrier.position).rotated_by(carrier.rotation.reversed());
                self.carrier_info
                    .nearest_active_vstol_spot(spot_ref_local)
                    .map_or(f64::INFINITY, |(_, distance)| distance)
            }
        };
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
                    aoa: self.effective_aoa(plane),
                    alt: plane.alt.max(0.0),
                    roll_deg: plane.roll,
                    carrier_time: carrier.time,
                    plane_time: plane.time,
                    carrier_received_unix_ms: 0,
                    plane_received_unix_ms: 0,
                    sample_gap_ms: 0.0,
                    capture_gap_ms: 0.0,
                    delivery_age_ms: 0.0,
                    skew_ms: (carrier.time - plane.time).abs() * 1_000.0,
                    alignment: AlignmentMethod::Direct,
                    telemetry_valid: true,
                    raw_carrier_position: vec3_array(carrier.position),
                    corrected_carrier_position: vec3_array(carrier.position),
                    filtered_carrier_position: vec3_array(carrier.position),
                });
            }
        }

        self.grading = Some(Grading::Recovered {
            cable: None,
            // Left unset here on purpose: reconciled once in `finish()` from
            // `wire_estimation`, against the complete wire-crossing history, rather than
            // computed here against whatever crossings the position-tick path has managed
            // to observe by the time this event-correlated `Land` fires. The two paths are
            // independent and can race, which previously let this diverge from
            // `wire_estimation.wire` in the same report.
            cable_estimated: None,
        });
        self.landing_time = Some(plane.time);
        self.touchdown_horizontal_speed_mps = Some(
            (plane.velocity.x * plane.velocity.x + plane.velocity.z * plane.velocity.z).sqrt(),
        );
        tracing::debug!("first correlated touchdown recorded");
        true
    }

    /// Reset the per-attempt evidence that must never leak from an earlier groove entry (e.g. a
    /// bolter that re-attempts) into the one that ends up being scored: the wire-crossing plane
    /// history and the continuous GS/lineup trajectory (see `TrajectoryDeviation`). Called only
    /// on a `false -> true` transition of `entered_groove`.
    fn mark_fresh_groove_entry(&mut self, entry_time: f64) {
        self.groove_entry_time = Some(entry_time);
        self.previous_wire_plane = [None; 4];
        self.wire_crossings.clear();
        self.trajectory_deviations.clear();
        self.previous_horizontal_speed = None;
        self.deceleration_run_count = 0;
        self.deceleration_run_start_time = None;
        self.arrest_deceleration_onset_time = None;
        self.arrest_kinematic_state = ArrestKinematicState::default();
        self.deck_crossing_time = None;
        self.deck_kinematics.clear();
    }

    pub fn finish(mut self) -> TrackResult {
        // A point-in-time gate object is a convenient measurement, not the only possible proof
        // that the required distance was observed. The continuous groove trajectory contains
        // the same valid, inbound source samples. When two adjacent samples bracket a gate in at
        // most 300 ms, retain that bracket as coverage evidence. No position or deviation is
        // invented: grading amplitude continues to use the trajectory samples themselves.
        recover_gate_coverage_from_trajectory(
            &self.trajectory_deviations,
            GATE_THREE_QUARTER_NM,
            &self.gate_deviations.at_three_quarter_nm,
            &mut self.gate_deviations.three_quarter_quality,
        );
        recover_gate_coverage_from_trajectory(
            &self.trajectory_deviations,
            GATE_HALF_NM,
            &self.gate_deviations.at_half_nm,
            &mut self.gate_deviations.half_quality,
        );
        recover_gate_coverage_from_trajectory(
            &self.trajectory_deviations,
            GATE_QUARTER_NM,
            &self.gate_deviations.at_quarter_nm,
            &mut self.gate_deviations.quarter_quality,
        );

        // Source-invalid observations can be delivered in a later batch. Attribute them only
        // now, from their own capture timestamp against the final scored-segment bounds; client
        // receipt time is retained for diagnosis but is never substituted for source time.
        self.attribute_invalid_source_observations();

        // Entering the groove proves an approach, not its terminal outcome. Without contact,
        // LQM waveoff evidence or the geometric departure guard, keep that distinction explicit
        // instead of inventing a waveoff.
        if self.grading.is_none()
            && (self.entered_groove
                || self.gate_deviations.at_half_nm.is_some()
                || self.gate_deviations.at_quarter_nm.is_some())
        {
            self.grading = Some(Grading::ApproachOnly);
        }

        // Still correlated on the event-reported touchdown time (`landing_time`) for the upper
        // eligibility bound and the event-lag sanity check, but the wire number itself now
        // prefers the earliest crossing at/after a detected arrest-deceleration onset over the
        // plain last-before-event crossing (see `WIRE_ARREST_DECELERATION_MPS2` and
        // `wire_estimate_at`) -- this is what lets a same-approach fixture (`wire_4_01_FA18C`, a
        // clean straight-in trap with no bolter/bounce) still resolve to the correct wire 4 even
        // though the hook sweeps geometrically across wires 1-3 while still airborne on short
        // final: those earlier crossings occur before any deceleration is observed, and are no
        // longer preferred. An earlier attempt to filter on `first_hook_ground_contact_time`
        // alone (a single geometric instant, not a deceleration proxy) discarded the correct wire
        // 4 crossing on this same fixture and was reverted; see tasking-roadmap.md for the
        // decision history. This does not change the second, independent half of the fix: a wire
        // estimate can still no longer read "high" confidence without a DCS-confirmed arrest.
        let dcs_wire = self.dcs_grading.as_deref().and_then(parse_dcs_wire);
        let is_arrested = matches!(self.carrier_info.recovery, CarrierRecovery::Arrested);
        let touchdown_reference = self
            .landing_time
            .or(self.deck_crossing_time)
            .or_else(|| self.datums.last().map(|datum| datum.time))
            .unwrap_or_default();
        let deck_kinematics = if is_arrested {
            self.evaluate_deck_arrest_kinematics()
        } else {
            DeckArrestKinematicsEvidence::default()
        };
        let hook_state = if is_arrested {
            self.calibrated_hook_state()
        } else {
            HookState::Unknown
        };
        self.hook_observation.interpreted_state = hook_state.as_str();
        // The stop position names the wire only once the deck kinematics have confirmed a stop
        // inside the run-out band; a hook-up pass gets the hypothetical estimate instead.
        let stop_x = deck_kinematics
            .confirmed
            .then_some(deck_kinematics.x_at_slow_m)
            .flatten();
        let wire_estimation = self.wire_estimate_with(
            touchdown_reference,
            dcs_wire.is_some(),
            stop_x,
            hook_state == HookState::Up,
        );
        let arrest_confirmation = self.arrest_confirmation_evidence_with(
            dcs_wire,
            wire_estimation.reason == HOOK_TRANSIENT_ESTIMATE_REASON,
            deck_kinematics,
        );

        // If DCS grading is set, use its reported wire for arrested recoveries only.
        let grading = if matches!(&self.carrier_info.recovery, CarrierRecovery::Arrested) {
            if let Some(dcs_wire) = dcs_wire {
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

        // A `Bolter` derived only from our own geometry is refused when the DCS LQM itself
        // starts with `GRADE:WO` (a waveoff/go-around grade): this is not inventing a waveoff
        // author, it is refusing a bolter that DCS's own grading directly contradicts. Confirmed
        // live 5 September 2026: an LQM `GRADE:WO ... WO(AFU)IC` was received on a pass our
        // geometry alone had called `Bolter`, with no `runway_touch`/`land` event at all.
        let grading = match grading {
            Grading::Bolter if dcs_grade_is_waveoff(self.dcs_grading.as_deref()) => {
                tracing::info!(
                    dcs_grading = ?self.dcs_grading,
                    "geometric Bolter refused: DCS LQM opens on GRADE:WO"
                );
                Grading::WaveoffUnknown
            }
            other => other,
        };

        // Reconcile `cable_estimated` with `wire_estimation`: both must describe the same
        // wire-crossing evidence, and `wire_estimation` above is the single, complete-history
        // computation (unlike the touchdown/deck-crossing call sites earlier in `Track::next`,
        // which could see a not-yet-complete `self.wire_crossings` depending on event/position
        // race ordering). Never overrides a DCS-confirmed `cable` above; only the independent
        // Rust estimate.
        let grading = if matches!(self.carrier_info.recovery, CarrierRecovery::Arrested) {
            match grading {
                Grading::Recovered { cable, .. } => Grading::Recovered {
                    cable,
                    cable_estimated: wire_estimation.wire,
                },
                Grading::TouchAndGo { .. } => Grading::TouchAndGo {
                    cable_estimated: wire_estimation.wire,
                },
                other => other,
            }
        } else {
            grading
        };

        let groove_time_secs = match (self.groove_entry_time, self.landing_time) {
            (Some(entry), Some(land)) if land > entry => Some(land - entry),
            _ => None,
        };
        tracing::debug!(
            groove_entry_time = ?self.groove_entry_time,
            landing_time = ?self.landing_time,
            first_hook_ground_contact_time = ?self.first_hook_ground_contact_time,
            hook_contact_to_dcs_event_delta_secs =
                match (self.first_hook_ground_contact_time, self.landing_time) {
                    (Some(contact), Some(land)) => Some(land - contact),
                    _ => None,
                },
            groove_time_secs,
            "groove/touchdown timing summary"
        );

        // CATOBAR keeps the native wire/groove grading path.  AV-8B V/STOL
        // deliberately reuses the same GS/LU gate tiers, referenced to its 3.0°
        // glide slope, but excludes CATOBAR-only wire/groove bonuses.  AOA is
        // visual information only and is not part of the points calculation.
        let (approach_grade, approach_points, grade_reason, grading_episodes) =
            if self.carrier_info.is_vstol() {
                let (grade, points) = compute_vstol_approach_grade_points(
                    &grading,
                    &self.gate_deviations,
                    &self.trajectory_deviations,
                );
                // V/STOL does not yet have a detailed per-gate rationale of its own (see
                // `grade_reason`'s doc comment) -- a short, generic placeholder naming the final
                // grade is still more useful than an empty field.
                (
                    grade,
                    points,
                    format!(
                        "{}: V/STOL approach grade averaged from gate scores.",
                        grade.label()
                    ),
                    Vec::new(),
                )
            } else {
                let assessment = compute_catobar_assessment(CatobarEvidence {
                    grading: &grading,
                    gates: &self.gate_deviations,
                    trajectory: &self.trajectory_deviations,
                    datums: &self.datums,
                    plane_info: self.plane_info,
                    aoa_reliable: self.wind_reference.is_some(),
                    groove_time_secs,
                    groove_entry_time: self.groove_entry_time,
                });
                (
                    assessment.grade,
                    assessment.grade.points(),
                    assessment.reason,
                    assessment.episodes,
                )
            };
        let spot_grade = self.spot_distance_m.map(SpotGrade::from_distance_m);

        // Only a successfully recovered V/STOL pass receives the spot-accuracy bonus and is then
        // mapped back to the same greenie-board labels used by CATOBAR (_OK_/OK/(OK)/--/C).
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

        // V/STOL keeps the unconditional three-gates rule (`None`): it has no roll-out-confirmed
        // groove entry to distinguish a mid-turn 3/4 NM reading from a real one.
        let three_quarter_groove_entry_time = (!self.carrier_info.is_vstol())
            .then_some(self.groove_entry_time)
            .flatten();
        if !self
            .gate_deviations
            .all_valid(three_quarter_groove_entry_time)
        {
            self.telemetry_quality
                .add_unavailability_cause(Completeness::InsufficientGates);
        }
        // A deck contact flown with the hook commanded up cannot be an arrest. When nothing
        // confirms one (no DCS wire, no hook transient, no deck-kinematics stop), classify it as
        // a qualification touch-and-go here too: the live moving-away decision in `next_sample`
        // never fires when the recording ends right after the contact (confirmed on the
        // F-14B(U) hook-up live fixtures, whose ACMI stops ~2 s after `Land`).
        let grading = match grading {
            Grading::Recovered {
                cable: None,
                cable_estimated,
            } if is_arrested
                && hook_state == HookState::Up
                && !arrest_confirmation.deck_kinematics.confirmed
                && arrest_confirmation.source != "hook_transient" =>
            {
                tracing::debug!("hook-up deck contact without arrest evidence: touch-and-go");
                Grading::TouchAndGo { cable_estimated }
            }
            other => other,
        };

        // RunwayTouch/Land prove contact, not an arrest. A DCS wire, a completed hook-deflection
        // transient correlated with a pendant crossing, or the deck kinematics (aircraft stopped
        // relative to the carrier) confirm the trap; without any of them the pass cannot receive
        // a favourable grade. Precedence is strict and mirrors `arrest_confirmation.source`.
        let arrest_evidence = match &grading {
            Grading::Recovered { cable: Some(_), .. } if is_arrested => "dcs_wire",
            Grading::Recovered {
                cable_estimated: Some(_),
                ..
            } if is_arrested && arrest_confirmation.source == "hook_transient" => "hook_transient",
            Grading::Recovered { .. }
                if is_arrested && arrest_confirmation.deck_kinematics.confirmed =>
            {
                "kinematic"
            }
            Grading::Recovered { .. } if is_arrested => "unconfirmed",
            _ => "none",
        };
        if arrest_evidence == "unconfirmed" {
            self.telemetry_quality
                .add_unavailability_cause(Completeness::UnconfirmedArrest);
        }
        if self.telemetry_quality.completeness != Completeness::Complete {
            // Preserve a measured approach assessment even when its coverage is only partial.
            // It is never point-eligible in this state; `Incomplete` remains the compatibility
            // value only when no meaningful approach segment was observed.
            let has_approach_evidence = !self.trajectory_deviations.is_empty()
                || self.gate_deviations.at_three_quarter_nm.is_some()
                || self.gate_deviations.at_half_nm.is_some()
                || self.gate_deviations.at_quarter_nm.is_some();
            if !has_approach_evidence {
                pass_grade = PassGrade::Incomplete;
            }
            grade_points = None;
        }

        self.telemetry_quality.gap_p50_ms = self.telemetry_gap_stats.percentile(0.50);
        self.telemetry_quality.gap_p90_ms = self.telemetry_gap_stats.percentile(0.90);
        self.telemetry_quality.gap_p95_ms = self.telemetry_gap_stats.percentile(0.95);
        self.telemetry_quality.gap_p99_ms = self.telemetry_gap_stats.percentile(0.99);
        self.telemetry_quality.degraded_sample_ratio =
            self.telemetry_gap_stats.ratio_above_warning();
        self.telemetry_quality.capture_gap_warning_ratio =
            self.capture_gap_stats.ratio_above_warning();
        self.telemetry_quality.late_delivery_warning_ratio =
            self.delivery_age_stats.ratio_above_warning();
        self.telemetry_quality.capture_gap_p50_ms = self.capture_gap_stats.percentile(0.50);
        self.telemetry_quality.capture_gap_p95_ms = self.capture_gap_stats.percentile(0.95);
        self.telemetry_quality.capture_gap_p99_ms = self.capture_gap_stats.percentile(0.99);
        self.telemetry_quality.delivery_age_p50_ms = self.delivery_age_stats.percentile(0.50);
        self.telemetry_quality.delivery_age_p95_ms = self.delivery_age_stats.percentile(0.95);
        self.telemetry_quality.delivery_age_p99_ms = self.delivery_age_stats.percentile(0.99);
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
            grade_reason,
            grading_episodes,
            spot_grade,
            spot_distance_m: self.spot_distance_m,
            intended_spot: self.carrier_info.is_vstol().then_some("7.5"),
            actual_nearest_spot: self.actual_nearest_spot,
            dcs_grading: self.dcs_grading,
            gate_deviations: self.gate_deviations,
            trajectory_deviations: self.trajectory_deviations,
            datums: self.datums,
            pattern_datums: self.pattern_datums,
            plane_info: self.plane_info,
            carrier_info: self.carrier_info,
            groove_time_secs,
            groove_entry: self.groove_entry_evidence,
            touchdown_time_dcs: self.landing_time,
            telemetry_quality: self.telemetry_quality,
            events: self.events,
            spot_zone: self.spot_zone,
            touchdown_horizontal_speed_mps: self.touchdown_horizontal_speed_mps,
            hook_observation: self.hook_observation,
            wire_estimation,
            arrest_confirmation,
            hook_state,
            arrest_evidence,
            wind_reference_established: self.wind_reference.is_some(),
            wind_reference_probes: self.wind_reference_probes,
        }
    }

    /// Set the track's dcs grading.
    pub fn set_dcs_grading(&mut self, dcs_grading: String) -> bool {
        if self.dcs_grading.is_none() {
            // A matching DCS LQM that opens on GRADE:WO is outcome evidence for the current
            // attempt, not merely a comment to retain until some later touchdown.  Latching the
            // waveoff here lets `next()` close the track once the aircraft moves away from its
            // closest point, so a subsequent circuit starts with a fresh Track instead of
            // inheriting this LQM, groove and wire-crossing history.
            //
            // Do not overwrite a positional/contact outcome already established on this same
            // track.  `finish()` still resolves the narrower geometry-only Bolter contradiction
            // below, while a correlated touchdown remains stronger evidence than a late WO
            // comment.
            let marks_current_attempt_as_waveoff =
                dcs_grade_is_waveoff(Some(dcs_grading.as_str())) && self.grading.is_none();
            self.dcs_grading = Some(dcs_grading);
            if marks_current_attempt_as_waveoff {
                self.grading = Some(Grading::WaveoffUnknown);
                tracing::info!(
                    "DCS GRADE:WO established the terminal outcome for the current attempt"
                );
            }
            true
        } else {
            false
        }
    }

    pub(crate) fn has_dcs_waveoff_evidence(&self) -> bool {
        dcs_grade_is_waveoff(self.dcs_grading.as_deref())
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
            // Finite pendant: the hook must be between the two end points and physically near
            // the wire (`MAX_WIRE_VERTICAL_SEPARATION_M`), otherwise the sample does not bracket
            // a crossing at all. Confirmed on the F-14B(U) wire-4 live fixture, where an early
            // infinite-plane crossing used to suppress the real deck crossing.
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

    /// Distance along the angled-deck axis from the aircraft to the ideal touchdown point
    /// (`x > 0` short of it, `x < 0` beyond it), the same axis the gates and datums use.
    fn deck_axis_x(&self, carrier: &Transform, ray_from_plane_to_carrier: DVec3) -> f64 {
        let fb_rot = DRotor3::from_rotation_xz(
            (carrier.heading - self.carrier_info.deck_angle)
                .neg()
                .to_radians(),
        );
        ray_from_plane_to_carrier.dot(DVec3::unit_z().rotated_by(fb_rot))
    }

    /// Earliest evidence of deck contact: the touchdown event, else the first inbound crossing of
    /// the ideal touchdown point while in the groove.
    fn contact_reference_time(&self) -> Option<f64> {
        [self.landing_time, self.deck_crossing_time]
            .into_iter()
            .flatten()
            .reduce(f64::min)
    }

    /// Confirms an arrest from carrier-relative deck kinematics. See the `DECK_ARREST_*`
    /// constants for the PROJECT-DERIVED thresholds. Pure: the confirmation is a property of the
    /// samples recorded so far and never mutates the track.
    fn evaluate_deck_arrest_kinematics(&self) -> DeckArrestKinematicsEvidence {
        let samples_total = self.deck_kinematics.len() as u32;
        let Some(reference) = self.contact_reference_time() else {
            return DeckArrestKinematicsEvidence {
                reason: "no_contact_reference",
                samples: samples_total,
                ..DeckArrestKinematicsEvidence::default()
            };
        };
        // Windowed carrier-relative horizontal speed per sample at or after the reference;
        // samples without a window partner are skipped.
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
            return DeckArrestKinematicsEvidence {
                reason: "no_deck_samples_after_reference",
                reference_time_dcs: Some(reference),
                samples: samples_total,
                ..DeckArrestKinematicsEvidence::default()
            };
        }
        let min_relative_speed_mps = samples
            .iter()
            .map(|sample| sample.relative_speed_mps)
            .fold(f64::INFINITY, f64::min);

        // Earliest run of consecutive slow samples (no gap, inside the run-out band) that lasts
        // `DECK_ARREST_HOLD_S` and starts within the detection window. The arresting cable pulls
        // the aircraft back at 10-15 m/s for ~1.5 s after the run-out, so an earlier short slow
        // spell followed by that pull-back must not end the search.
        let mut run_start: Option<&WindowedDeckSample> = None;
        let mut previous_time = None;
        let mut gap_seen = false;
        let mut best_held = 0.0_f64;
        let mut best_start: Option<&WindowedDeckSample> = None;
        for sample in &samples {
            let gap_ms =
                previous_time.map_or(0.0, |previous: f64| (sample.time - previous) * 1_000.0);
            previous_time = Some(sample.time);
            let slow = sample.relative_speed_mps <= DECK_ARREST_HOLD_MAX_RELATIVE_SPEED_MPS
                && (DECK_ARREST_MIN_X_M..=DECK_ARREST_MAX_X_M).contains(&sample.x);
            if gap_ms > DECK_ARREST_MAX_SAMPLE_GAP_MS {
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
                    if sample.relative_speed_mps > DECK_ARREST_MAX_RELATIVE_SPEED_MPS {
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
            if held >= DECK_ARREST_HOLD_S
                && start.time - reference <= DECK_ARREST_DETECTION_WINDOW_S
            {
                return DeckArrestKinematicsEvidence {
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
        DeckArrestKinematicsEvidence {
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

    /// The hook-transient wire estimate: a completed arrestment deflection of the animated hook
    /// near `event_time`, correlated with the last finite pendant crossing no more than
    /// `MAX_HOOK_DEFLECTION_WIRE_LAG_MS` before it. `None` when no complete transient exists,
    /// so the caller can fall back to the deceleration-onset / last-crossing selection.
    fn wire_estimate_from_hook_transient(&self, event_time: f64) -> Option<WireEstimateEvidence> {
        let deflection = self.completed_hook_deflection_near(event_time)?;
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
        tracing::debug!(
            event_time,
            ?deflection,
            crossings = ?eligible,
            "wire crossing evidence at hook deflection"
        );
        let Some(last) = eligible.last() else {
            return Some(WireEstimateEvidence {
                wire: None,
                confidence: "insufficient",
                reason: "hook_deflection_not_correlated_with_wire_crossing",
                hook_deflection_time_dcs: Some(deflection.deflected_at_dcs),
                hook_recovered_time_dcs: Some(deflection.recovered_at_dcs),
                correlation_lag_ms: None,
                crossings: self.wire_crossings.clone(),
                arrest_deceleration_onset_time: self.arrest_deceleration_onset_time,
            });
        };
        let correlation_lag_ms = (deflection.deflected_at_dcs - last.timestamp_dcs) * 1_000.0;
        if !(0.0..=MAX_HOOK_DEFLECTION_WIRE_LAG_MS).contains(&correlation_lag_ms) {
            return Some(WireEstimateEvidence {
                wire: None,
                confidence: "insufficient",
                reason: "hook_deflection_not_correlated_with_wire_crossing",
                hook_deflection_time_dcs: Some(deflection.deflected_at_dcs),
                hook_recovered_time_dcs: Some(deflection.recovered_at_dcs),
                correlation_lag_ms: Some(correlation_lag_ms),
                crossings: self.wire_crossings.clone(),
                arrest_deceleration_onset_time: self.arrest_deceleration_onset_time,
            });
        }
        Some(WireEstimateEvidence {
            wire: Some(last.wire),
            // The completed transient is itself arrest evidence, so unlike the deceleration-onset
            // path below "high" does not additionally require a DCS wire.
            confidence: if last.bracket_gap_ms <= 150.0 && correlation_lag_ms <= 150.0 {
                "high"
            } else {
                "medium"
            },
            reason: HOOK_TRANSIENT_ESTIMATE_REASON,
            hook_deflection_time_dcs: Some(deflection.deflected_at_dcs),
            hook_recovered_time_dcs: Some(deflection.recovered_at_dcs),
            correlation_lag_ms: Some(correlation_lag_ms),
            crossings: self.wire_crossings.clone(),
            arrest_deceleration_onset_time: self.arrest_deceleration_onset_time,
        })
    }

    /// Find a complete arrestment transient of the animated hook near `event_time`: a stable
    /// hook-down value for at least `MIN_HOOK_DOWN_STABLE_S`, then a sharp deflection on the very
    /// next sample within `MAX_HOOK_DEFLECTION_TOUCH_OFFSET_S` of the event, then a return to the
    /// down band within `MAX_HOOK_DEFLECTION_RECOVERY_S`.
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
            let (Some(before_raw), Some(deflected_raw)) = (before.raw, deflected.raw) else {
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

    fn observe_arrest_kinematics(
        &mut self,
        sample: &TelemetrySample,
        carrier: &Transform,
        plane: &Transform,
        hook_altitude_m: f64,
    ) {
        let Some(contact_time_dcs) = self.landing_time else {
            return;
        };
        // A batch can arrive after the event while still containing older source snapshots.
        // Receipt order must not turn those pre-contact positions into post-contact stop proof.
        if plane.time < contact_time_dcs {
            return;
        }
        let relative_velocity = plane.velocity - carrier.velocity;
        let relative_speed_mps = (relative_velocity.x * relative_velocity.x
            + relative_velocity.z * relative_velocity.z)
            .sqrt();
        if !relative_speed_mps.is_finite() || !hook_altitude_m.is_finite() {
            return;
        }
        let state = &mut self.arrest_kinematic_state;
        state.post_contact_valid_samples = state.post_contact_valid_samples.saturating_add(1);
        state.last_valid_time_dcs = Some(plane.time);
        state.minimum_relative_speed_mps = Some(
            state
                .minimum_relative_speed_mps
                .map_or(relative_speed_mps, |minimum| {
                    minimum.min(relative_speed_mps)
                }),
        );
        state.maximum_capture_gap_ms = state.maximum_capture_gap_ms.max(sample.sample_gap_ms);

        if hook_altitude_m > ARREST_MAX_ON_DECK_HOOK_HEIGHT_M {
            state.bounce_detected = true;
        }
        if state.low_speed_was_reached && relative_speed_mps > ARREST_DEPARTURE_RELATIVE_SPEED_MPS {
            state.forward_departure_detected = true;
        }

        let continuous = sample.sample_gap_ms <= SAMPLE_GAP_WARNING_MS;
        let retained_on_deck = hook_altitude_m <= ARREST_MAX_ON_DECK_HOOK_HEIGHT_M;
        if continuous && retained_on_deck && relative_speed_mps <= ARREST_LOW_RELATIVE_SPEED_MPS {
            state.low_speed_was_reached = true;
            let start = *state.low_speed_run_start_time.get_or_insert(plane.time);
            state.low_speed_run_samples = state.low_speed_run_samples.saturating_add(1);
            let hold = plane.time - start;
            if hold >= state.best_low_speed_hold_s {
                state.best_low_speed_hold_s = hold;
                state.best_low_speed_hold_samples = state.low_speed_run_samples;
            }
        } else {
            state.low_speed_run_start_time = None;
            state.low_speed_run_samples = 0;
        }
    }

    /// Velocity-signature-only view, kept for the diagnostic unit tests: no hook transient, and
    /// whatever deck kinematics the track has observed so far.
    #[cfg(test)]
    fn arrest_confirmation_evidence(&self, dcs_wire: Option<u8>) -> ArrestConfirmationEvidence {
        self.arrest_confirmation_evidence_with(
            dcs_wire,
            false,
            self.evaluate_deck_arrest_kinematics(),
        )
    }

    fn arrest_confirmation_evidence_with(
        &self,
        dcs_wire: Option<u8>,
        hook_transient: bool,
        deck_kinematics: DeckArrestKinematicsEvidence,
    ) -> ArrestConfirmationEvidence {
        let contact_time_dcs = self.landing_time;
        let onset_delta_ms = self
            .arrest_deceleration_onset_time
            .zip(contact_time_dcs)
            .map(|(onset, contact)| (onset - contact) * 1_000.0);
        let onset_correlated = onset_delta_ms
            .is_some_and(|delta| delta.abs() <= ARREST_MAX_CONTACT_ONSET_DELTA_S * 1_000.0);
        let state = &self.arrest_kinematic_state;
        let outcome_conflicts_with_arrest = matches!(
            self.grading,
            Some(
                Grading::Bolter
                    | Grading::TouchAndGo { .. }
                    | Grading::WaveoffUnknown
                    | Grading::ApproachOnly,
            )
        );
        let hold_complete = state.best_low_speed_hold_s >= ARREST_LOW_SPEED_HOLD_S
            && state.best_low_speed_hold_samples >= ARREST_MIN_LOW_SPEED_SAMPLES;
        let telemetry_ended_before_conclusion = !hold_complete
            && contact_time_dcs.is_some_and(|contact| {
                state
                    .last_valid_time_dcs
                    .is_none_or(|last| last - contact < ARREST_LOW_SPEED_HOLD_S)
            });
        let (accepted, reason) = if contact_time_dcs.is_none() {
            (false, "no_correlated_contact_event")
        } else if self.arrest_deceleration_onset_time.is_none() {
            (false, "no_sustained_deceleration_onset")
        } else if !onset_correlated {
            (false, "deceleration_onset_not_correlated_with_contact")
        } else if outcome_conflicts_with_arrest {
            (false, "observed_outcome_conflicts_with_arrest")
        } else if state.bounce_detected {
            (false, "post_contact_bounce_detected")
        } else if state.forward_departure_detected {
            (false, "post_contact_forward_departure_detected")
        } else if !hold_complete {
            if telemetry_ended_before_conclusion {
                (false, "telemetry_ended_before_low_speed_hold_completed")
            } else {
                (false, "low_relative_speed_not_sustained")
            }
        } else {
            (
                true,
                "correlated_contact_deceleration_and_sustained_deck_relative_stop",
            )
        };
        let kinematic = ArrestKinematicEvidence {
            accepted,
            reason,
            verdict_effect: "diagnostic_only_no_grading_change",
            contact_event_correlated: contact_time_dcs.is_some(),
            contact_time_dcs,
            deceleration_onset_time_dcs: self.arrest_deceleration_onset_time,
            deceleration_contact_delta_ms: onset_delta_ms,
            post_contact_valid_samples: state.post_contact_valid_samples,
            minimum_relative_speed_mps: state.minimum_relative_speed_mps,
            low_speed_hold_s: state.best_low_speed_hold_s,
            low_speed_hold_samples: state.best_low_speed_hold_samples,
            maximum_capture_gap_ms: state.maximum_capture_gap_ms,
            outcome_conflicts_with_arrest,
            bounce_detected: state.bounce_detected,
            forward_departure_detected: state.forward_departure_detected,
            telemetry_ended_before_conclusion,
            contact_source: "dcs_grpc_event_stream_land_or_runway_touch",
            velocity_source: "paired_aircraft_carrier_source_velocity",
            low_relative_speed_threshold_mps: ARREST_LOW_RELATIVE_SPEED_MPS,
            required_low_speed_hold_s: ARREST_LOW_SPEED_HOLD_S,
            required_low_speed_samples: ARREST_MIN_LOW_SPEED_SAMPLES,
            maximum_contact_onset_delta_s: ARREST_MAX_CONTACT_ONSET_DELTA_S,
            maximum_contiguous_capture_gap_ms: SAMPLE_GAP_WARNING_MS,
            maximum_on_deck_hook_height_m: ARREST_MAX_ON_DECK_HOOK_HEIGHT_M,
        };
        let missing_dcs_lqm_reason = if dcs_wire.is_some() {
            None
        } else if self.dcs_grading.is_some() {
            Some("landing_quality_mark_without_valid_wire")
        } else {
            Some("landing_quality_mark_absent")
        };
        // Policy (see primer.md, "Le verdict", and tasking-roadmap.md, P0): a DCS wire is
        // authoritative; a completed hook transient or a deck-kinematics stop confirms the arrest
        // at medium confidence without inventing a wire number; the velocity-based signature
        // alone stays diagnostic.
        let (source, confidence, verdict_effect) = if dcs_wire.is_some() {
            ("dcs_lqm", "high", "authoritative_dcs_confirmation")
        } else if hook_transient {
            (
                "hook_transient",
                "medium",
                "confirmed_arrest_medium_confidence",
            )
        } else if deck_kinematics.confirmed {
            ("kinematic", "medium", "confirmed_arrest_medium_confidence")
        } else if accepted {
            (
                "kinematic_diagnostic",
                "medium",
                "diagnostic_only_no_grading_change",
            )
        } else {
            ("unconfirmed", "insufficient", "no_confirmation")
        };
        ArrestConfirmationEvidence {
            source,
            confidence,
            verdict_effect,
            dcs_wire,
            missing_dcs_lqm_reason,
            kinematic,
            deck_kinematics,
        }
    }

    /// Track the onset of a sustained horizontal-speed deceleration (see
    /// `WIRE_ARREST_DECELERATION_MPS2`), used by `wire_estimate_at` to tell a wire crossing
    /// recorded while still airborne apart from the one actually caught. Frozen at first
    /// detection, like `first_hook_ground_contact_time`; a no-op once already set.
    ///
    /// Relies on `plane.velocity`, which only the live DCS-gRPC path populates
    /// (`Transform::from` the gRPC `Velocity` message). `lso.exe file` (ACMI/Tacview replay,
    /// including the `tests/recordings/*.zip.acmi` fixtures) never sets it, the same
    /// pre-existing gap `touchdown_horizontal_speed_mps` already has there (see its own test
    /// asserting `0.0`): every offline-replayed recovery therefore falls back to the old
    /// last-crossing-before-event selection below, onset never detected, no regression either
    /// way. This proxy is only exercised by the live path and by the dedicated unit tests
    /// (`wire_estimate_prefers_the_crossing_at_deceleration_onset_over_a_later_stretch_crossing`/
    /// `wire_estimate_ignores_a_crossing_recorded_while_still_airborne_before_any_deceleration`)
    /// that call it directly with synthetic velocity; it still needs live revalidation (see
    /// tasking-roadmap.md).
    fn observe_horizontal_deceleration(&mut self, plane: &Transform) {
        if self.arrest_deceleration_onset_time.is_some() {
            return;
        }
        let speed =
            (plane.velocity.x * plane.velocity.x + plane.velocity.z * plane.velocity.z).sqrt();
        if let Some((previous_speed, previous_time)) = self.previous_horizontal_speed {
            let dt = plane.time - previous_time;
            if dt > 0.0 {
                let deceleration = (previous_speed - speed) / dt;
                tracing::trace!(
                    time = plane.time,
                    speed,
                    dt,
                    deceleration,
                    run_count = self.deceleration_run_count,
                    "horizontal deceleration sample"
                );
                if deceleration >= WIRE_ARREST_DECELERATION_MPS2 {
                    if self.deceleration_run_count == 0 {
                        self.deceleration_run_start_time = Some(previous_time);
                    }
                    self.deceleration_run_count += 1;
                    if self.deceleration_run_count
                        >= WIRE_ARREST_DECELERATION_MIN_CONSECUTIVE_SAMPLES
                    {
                        self.arrest_deceleration_onset_time = self.deceleration_run_start_time;
                        tracing::debug!(
                            onset = ?self.arrest_deceleration_onset_time,
                            deceleration_mps2 = deceleration,
                            "arrest deceleration onset detected"
                        );
                    }
                } else {
                    self.deceleration_run_count = 0;
                    self.deceleration_run_start_time = None;
                }
            }
        }
        self.previous_horizontal_speed = Some((speed, plane.time));
    }

    /// Aircraft `x` (landing-area frame of `Datum::x`) at `time`, from the nearest position
    /// sample within `WIRE_CROSSING_POSITION_TOLERANCE_S`.
    fn aircraft_x_at(&self, time: f64) -> Option<f64> {
        self.datums
            .iter()
            .filter(|datum| (datum.time - time).abs() <= WIRE_CROSSING_POSITION_TOLERANCE_S)
            .min_by(|left, right| {
                (left.time - time)
                    .abs()
                    .total_cmp(&(right.time - time).abs())
            })
            .map(|datum| datum.x)
    }

    /// Which wire was caught, from where the aircraft came to rest. The arresting gear's run-out
    /// past the engaged wire is a constant of the type (`AirplaneInfo::arresting_run_out_m`, 85
    /// to 89 m on seven Tomcat traps at entry speeds from 53 to 76 m/s), so `stop_x + run_out`
    /// is the engaged wire's position, matched against the recorded hook-plane crossings (the
    /// aircraft's `x` at each crossing time). The crossing sequence alone cannot name the wire
    /// on a trap without a landing mark: the deceleration onset is detected 53 to 59 m past the
    /// wire, later than the 0.6 s the hook takes to sweep all four planes, so the onset-anchored
    /// pick below always returned the 1-wire (15 September 2026, 19:17: a 2-wire, wheels down
    /// beyond the 4-wire; `docs/RECOVERY_REVIEW_2026-09-15.md`, section 6).
    fn wire_estimate_from_stop_position(&self, stop_x: f64) -> Option<WireEstimateEvidence> {
        let run_out = self.plane_info.arresting_run_out_m?;
        let engaged_x = stop_x + run_out;
        let (crossing, residual) = self
            .wire_crossings
            .iter()
            .filter_map(|crossing| {
                let x = self.aircraft_x_at(crossing.timestamp_dcs)?;
                Some((crossing, (x - engaged_x).abs()))
            })
            .min_by(|left, right| left.1.total_cmp(&right.1))?;
        if residual > WIRE_STOP_POSITION_MAX_RESIDUAL_M {
            tracing::debug!(
                stop_x,
                run_out,
                engaged_x,
                nearest_wire = crossing.wire,
                residual_m = residual,
                "wire estimate: stop position matches no recorded crossing"
            );
            return None;
        }
        tracing::debug!(
            wire = crossing.wire,
            stop_x,
            run_out,
            residual_m = residual,
            "wire estimate from stop position"
        );
        Some(WireEstimateEvidence {
            wire: Some(crossing.wire),
            confidence: "medium",
            reason: STOP_POSITION_ESTIMATE_REASON,
            hook_deflection_time_dcs: None,
            hook_recovered_time_dcs: None,
            correlation_lag_ms: None,
            crossings: self.wire_crossings.clone(),
            arrest_deceleration_onset_time: self.arrest_deceleration_onset_time,
        })
    }

    #[cfg(test)]
    fn wire_estimate_at(&self, event_time: f64, arrest_confirmed: bool) -> WireEstimateEvidence {
        self.wire_estimate_with(event_time, arrest_confirmed, None, false)
    }

    /// The wire estimate at `event_time`. `stop_x` is where the aircraft came to rest on a
    /// confirmed arrestment (`DeckArrestKinematicsEvidence::x_at_slow_m`), `hook_up` whether the
    /// pass was flown with the hook up, in which case the estimate is the wire the hook would
    /// have caught and is labelled `HYPOTHETICAL_HOOK_UP_REASON`.
    fn wire_estimate_with(
        &self,
        event_time: f64,
        arrest_confirmed: bool,
        stop_x: Option<f64>,
        hook_up: bool,
    ) -> WireEstimateEvidence {
        // A completed hook transient is the strongest independent evidence of which wire was
        // caught (validated on the 2026-09 live corpus); then the stop position on a confirmed
        // arrestment; the deceleration-onset / last-crossing selection below is the fallback
        // when neither exists (no hook sampler, batch-delayed hook timestamps, or simply no
        // arrest).
        if let Some(estimate) = self.wire_estimate_from_hook_transient(event_time) {
            return estimate;
        }
        if !hook_up {
            if let Some(estimate) = stop_x.and_then(|x| self.wire_estimate_from_stop_position(x)) {
                return estimate;
            }
        }
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
            tracing::debug!(
                reason = "no_fresh_hook_plane_crossing",
                "wire estimate: none"
            );
            return WireEstimateEvidence {
                wire: None,
                confidence: "insufficient",
                reason: "no_fresh_hook_plane_crossing",
                hook_deflection_time_dcs: None,
                hook_recovered_time_dcs: None,
                correlation_lag_ms: None,
                crossings: self.wire_crossings.clone(),
                arrest_deceleration_onset_time: self.arrest_deceleration_onset_time,
            };
        };
        // A late RunwayTouch position is not moved backwards by a magic offset. If the event
        // does not closely correlate with fresh evidence, keep every crossing but decline to
        // name a wire. Anchored on the arrest deceleration onset when one was observed, rather
        // than on the last raw crossing: confirmed live 6 September 2026 that the last geometric
        // crossing (the hook merely sweeping past a wire threshold) sits 505-1953 ms before the
        // DCS touchdown event on every pass in a 6-recovery human session -- always outside
        // `SAMPLE_GAP_WARNING_MS`, blocking every single estimate including both DCS-confirmed
        // arrests -- while the onset itself sat only 180-280 ms before that same event on those
        // two arrests, comfortably inside it. Falls back to the last-crossing anchor whenever no
        // onset was observed (every bolter/touch-and-go/waveoff, and any arrest whose
        // deceleration signature was lost to a telemetry gap), unchanged from before.
        let event_lag_ms = match self.arrest_deceleration_onset_time {
            Some(onset) => (event_time - onset) * 1_000.0,
            None => (event_time - last.timestamp_dcs) * 1_000.0,
        };
        if !(0.0..=SAMPLE_GAP_WARNING_MS).contains(&event_lag_ms) {
            tracing::debug!(
                event_lag_ms,
                arrest_deceleration_onset_time = ?self.arrest_deceleration_onset_time,
                reason = "wire_crossing_not_time_correlated_with_event",
                "wire estimate: none"
            );
            return WireEstimateEvidence {
                wire: None,
                confidence: "insufficient",
                reason: "wire_crossing_not_time_correlated_with_event",
                hook_deflection_time_dcs: None,
                hook_recovered_time_dcs: None,
                correlation_lag_ms: None,
                crossings: self.wire_crossings.clone(),
                arrest_deceleration_onset_time: self.arrest_deceleration_onset_time,
            };
        }
        // Prefer the earliest crossing at or after the arrest deceleration onset (see
        // `WIRE_ARREST_DECELERATION_MPS2`) over the plain last-before-event crossing: cable
        // stretch can carry the hook geometrically past the wire actually caught and into the
        // next one's threshold while the aircraft is already decelerating, and a crossing
        // recorded before any deceleration at all is still airborne on short final, not a catch.
        // Falls back to the last eligible crossing (previous behaviour) whenever no onset was
        // observed -- including every bolter/touch-and-go/waveoff, which never decelerates this
        // way, and any arrest whose deceleration signature was lost to a telemetry gap.
        let selected = match self.arrest_deceleration_onset_time {
            Some(onset) => eligible
                .iter()
                .find(|crossing| crossing.timestamp_dcs >= onset - WIRE_ARREST_ONSET_TOLERANCE_S)
                .unwrap_or(last),
            None => last,
        };
        // "high" additionally requires a DCS-confirmed arrest (a parsed LQM wire): tight
        // brackets alone describe how well the crossing was *measured*, not whether the
        // aircraft actually stopped, and a late `runway_touch`/`Land` correlation used to let
        // this read "high" on a bolter/touch-and-go whose last geometric crossing was simply
        // the highest-numbered wire the hook happened to pass before the event fired.
        let confidence =
            if arrest_confirmed && selected.bracket_gap_ms <= 150.0 && event_lag_ms <= 150.0 {
                "high"
            } else {
                "medium"
            };
        tracing::debug!(
            wire = selected.wire,
            confidence,
            arrest_confirmed,
            bracket_gap_ms = selected.bracket_gap_ms,
            event_lag_ms,
            arrest_deceleration_onset_time = ?self.arrest_deceleration_onset_time,
            selected_via_deceleration_onset = selected.timestamp_dcs != last.timestamp_dcs,
            "wire estimate"
        );
        WireEstimateEvidence {
            wire: Some(selected.wire),
            confidence,
            reason: if hook_up {
                HYPOTHETICAL_HOOK_UP_REASON
            } else {
                "continuous_hook_plane_crossing"
            },
            hook_deflection_time_dcs: None,
            hook_recovered_time_dcs: None,
            correlation_lag_ms: None,
            crossings: self.wire_crossings.clone(),
            arrest_deceleration_onset_time: self.arrest_deceleration_onset_time,
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
        // Frozen at the first *geometric* hook contact (`alt <= 0.0`), not the event-correlated
        // `landing_time`: the DCS touchdown event lags the physical contact by ~0.2-1 s, a window
        // in which the raw draw argument reflects the crosse pressed against the deck (reads as
        // `0.0`, i.e. "up") rather than its true position. Whichever signal fires first freezes
        // interpretation, since either is proof contact has already happened.
        let before_touchdown =
            self.first_hook_ground_contact_time.is_none() && self.landing_time.is_none();
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
            self.hook_observation.timeline_truncated = true;
            self.hook_observation.timeline_dropped_samples = self
                .hook_observation
                .timeline_dropped_samples
                .saturating_add(1);
            self.hook_observation.truncation_reason =
                Some("capacity_reached_oldest_observation_evicted");
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
        self.hook_observation.retained_from_dcs = self
            .hook_observation
            .timeline
            .front()
            .map(|sample| sample.associated_time_dcs);
        self.hook_observation.retained_through_dcs = self
            .hook_observation
            .timeline
            .back()
            .map(|sample| sample.associated_time_dcs);
        self.hook_observation.interpreted_state = match self.calibrated_hook_state() {
            HookState::Up => "up",
            HookState::Down => "down",
            HookState::Unknown => "unknown",
        };
    }

    fn calibrated_hook_state(&self) -> HookState {
        // Only a type with a known hook draw-argument index (`AirplaneInfo::hook_draw_argument`)
        // is ever interpreted; every other type (or a future type added without one) stays
        // `Unknown`, never inferred. The up/down thresholds below (`<= 0.2`/`>= 0.8`) were only
        // empirically confirmed for the F/A-18C, T-45 and F-14B(U) -- see
        // `HookObservation::polarity` (`Track::new`) for which F-14 variants remain an unverified
        // assumption of the same convention.
        if self.plane_info.hook_draw_argument.is_none() {
            return HookState::Unknown;
        }
        // Preferred baseline: in-groove samples that end `HOOK_BASELINE_GUARD_S` before the
        // earliest contact evidence, so the arrestment excursion of the animated hook (up to
        // ~1.4 s before the DCS touchdown event on the 2026-09 T-45/F-14B(U) corpus) is excluded
        // and a real trap keeps reading `Down`. When no sample predates that guard (short
        // timelines, unit tests), fall back to the older freeze at first contact.
        let guard_end = [
            self.landing_time,
            self.deck_crossing_time,
            self.first_hook_ground_contact_time,
        ]
        .into_iter()
        .flatten()
        .reduce(f64::min)
        .map(|contact| contact - HOOK_BASELINE_GUARD_S);
        let in_groove_success = |sample: &&HookSampleEvidence| {
            sample.status == HookSampleStatus::Success
                && sample.in_groove
                && sample.raw.is_some_and(f64::is_finite)
        };
        let mut valid = self
            .hook_observation
            .timeline
            .iter()
            .filter(in_groove_success)
            .filter(|sample| guard_end.is_some_and(|end| sample.associated_time_dcs <= end))
            .collect::<Vec<_>>();
        if valid.is_empty() {
            valid = self
                .hook_observation
                .timeline
                .iter()
                .filter(in_groove_success)
                .filter(|sample| sample.before_touchdown)
                .collect::<Vec<_>>();
        }
        let Some(latest) = valid.last() else {
            return HookState::Unknown;
        };
        let latest_state = match latest.raw {
            Some(raw) if raw <= 0.2 => HookState::Up,
            Some(raw) if raw >= 0.8 => HookState::Down,
            _ => return HookState::Unknown,
        };
        let recent_start = latest.associated_time_dcs - HOOK_BASELINE_WINDOW_S;
        let stable = valid
            .iter()
            .rev()
            .take_while(|sample| sample.associated_time_dcs >= recent_start)
            .take_while(|sample| match (latest_state, sample.raw) {
                (HookState::Up, Some(raw)) => raw <= 0.2,
                (HookState::Down, Some(raw)) => raw >= 0.8,
                _ => false,
            })
            .collect::<Vec<_>>();
        let duration = stable.last().map_or(0.0, |first| {
            latest.associated_time_dcs - first.associated_time_dcs
        });
        match latest_state {
            HookState::Down if stable.len() >= 2 && duration >= 0.2 => HookState::Down,
            HookState::Up if stable.len() >= 3 && duration >= 0.4 => HookState::Up,
            _ => HookState::Unknown,
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

    pub fn record_invalid_source_observations(
        &mut self,
        observations: Vec<InvalidSourceObservation>,
    ) {
        let invalid = observations.len().min(u32::MAX as usize) as u32;
        self.telemetry_quality.invalid_samples = self
            .telemetry_quality
            .invalid_samples
            .saturating_add(invalid);
        if invalid > 0 {
            self.telemetry_quality.health = TelemetryHealth::Red;
            self.telemetry_quality.health_reason = "source_invalid_observation";
        }
        for observation in observations {
            if self.telemetry_quality.invalid_source_observations.len()
                < MAX_INVALID_SOURCE_EVIDENCE
            {
                self.telemetry_quality
                    .invalid_source_observations
                    .push(observation);
            } else {
                self.telemetry_quality
                    .invalid_source_observation_timeline_truncated = true;
                self.telemetry_quality
                    .invalid_source_observation_timeline_dropped = self
                    .telemetry_quality
                    .invalid_source_observation_timeline_dropped
                    .saturating_add(1);
            }
        }
    }

    fn attribute_invalid_source_observations(&mut self) {
        let summary = assess_invalid_source_observation_coverage(
            &mut self.telemetry_quality.invalid_source_observations,
            &self.source_capture_anchors,
            self.groove_entry_time,
            self.landing_time,
            &self.gate_deviations,
        );
        self.telemetry_quality.pattern_invalid_samples = self
            .telemetry_quality
            .pattern_invalid_samples
            .saturating_add(summary.before_groove);
        self.telemetry_quality.scoring_invalid_samples = self
            .telemetry_quality
            .scoring_invalid_samples
            .saturating_add(summary.in_scored_segment);
        self.telemetry_quality
            .post_touchdown_invalid_source_observations = summary.after_touchdown;
        self.telemetry_quality
            .indeterminate_invalid_source_observations = summary.indeterminate;
        self.telemetry_quality
            .outside_segment_invalid_source_observations = summary.outside_segment;
        self.telemetry_quality
            .covered_short_gap_invalid_source_observations = summary.covered_short_gap;
        self.telemetry_quality.blocking_invalid_source_observations = summary.blocking;
        if summary.blocking > 0 {
            self.telemetry_quality
                .add_unavailability_cause(Completeness::InvalidTelemetry);
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

/// Rate of altitude loss (m/s, positive = descending) between `previous` (the last recorded
/// continuous-trajectory sample, if any) and the current `alt_m`/`time`. Shared by `Track::next`
/// and `replay_gate_and_trajectory` so a persisted report's `sink_rate_mps` can always be
/// reconstructed identically from either path. Returns `0.0` for the first sample of a run, or
/// whenever time did not advance (never divides by a non-positive interval).
fn sink_rate_since(previous: Option<&TrajectoryDeviation>, alt_m: f64, time: f64) -> f64 {
    previous
        .filter(|prev| time > prev.timestamp_dcs)
        .map(|prev| (prev.alt_m - alt_m) / (time - prev.timestamp_dcs))
        .unwrap_or(0.0)
}

/// `gs_deviation_deg`/`lineup_deg` for a `trajectory_deviations` sample. Vertical geometry keeps
/// the 75 m flare guard; lateral geometry uses the 150 m late-window reference so a fixed offset
/// has a fixed severity throughout that window.
fn trajectory_deviation_angles_deg(
    gs_deviation_m: f64,
    lateral_offset_m: f64,
    x: f64,
) -> (f64, f64) {
    (
        gs_deviation_m
            .atan2(x.max(NEAR_TOUCHDOWN_ANGLE_REFERENCE_M))
            .to_degrees(),
        lateral_offset_m
            .atan2(x.max(NEAR_TOUCHDOWN_LINEUP_REFERENCE_M))
            .to_degrees(),
    )
}

#[derive(Debug, Default, PartialEq, Eq)]
struct InvalidSourceCoverageSummary {
    before_groove: u32,
    in_scored_segment: u32,
    after_touchdown: u32,
    indeterminate: u32,
    outside_segment: u32,
    covered_short_gap: u32,
    blocking: u32,
}

/// Decide whether retained source-side errors materially remove coverage from the final scored
/// segment. This function classifies evidence only: it never interpolates a transform or adds a
/// trajectory sample. A non-blocking hole must be exactly one missing source sequence, bounded by
/// the immediately adjacent valid source sequences/ticks, span at most 300 ms, and not intersect
/// the real bracket of a valid gate.
fn assess_invalid_source_observation_coverage(
    observations: &mut [InvalidSourceObservation],
    anchors: &[SourceCaptureAnchor],
    groove_entry_time: Option<f64>,
    touchdown_time: Option<f64>,
    gates: &GateDeviations,
) -> InvalidSourceCoverageSummary {
    let mut summary = InvalidSourceCoverageSummary::default();
    let mut invalid_sequences = observations
        .iter()
        .map(|observation| observation.sequence)
        .collect::<Vec<_>>();
    invalid_sequences.sort_unstable();
    invalid_sequences.dedup();

    let mut sorted_anchors = anchors
        .iter()
        .copied()
        .filter(|anchor| anchor.capture_time_dcs.is_finite())
        .collect::<Vec<_>>();
    sorted_anchors.sort_by_key(|anchor| anchor.sequence);

    let gate_brackets = [
        &gates.three_quarter_quality,
        &gates.half_quality,
        &gates.quarter_quality,
    ];

    for observation in observations {
        let anchor_index =
            sorted_anchors.partition_point(|anchor| anchor.sequence < observation.sequence);
        let previous = anchor_index
            .checked_sub(1)
            .and_then(|index| sorted_anchors.get(index));
        let next = sorted_anchors.get(anchor_index);
        let ordered_bounds = previous.zip(next).filter(|(previous, next)| {
            previous.capture_time_dcs < next.capture_time_dcs
                && previous.capture_tick < observation.capture_tick
                && observation.capture_tick < next.capture_tick
        });
        let adjacent_bounds = ordered_bounds.filter(|(previous, next)| {
            previous.sequence.checked_add(1) == Some(observation.sequence)
                && observation.sequence.checked_add(1) == Some(next.sequence)
        });

        if let Some((previous, next)) = ordered_bounds {
            observation.source_time_lower_bound_dcs = Some(previous.capture_time_dcs);
            observation.source_time_upper_bound_dcs = Some(next.capture_time_dcs);
            observation.coverage_gap_ms =
                Some((next.capture_time_dcs - previous.capture_time_dcs).max(0.0) * 1_000.0);
            observation.previous_valid_sequence = Some(previous.sequence);
            observation.next_valid_sequence = Some(next.sequence);
        }

        let (attribution, basis) = if let Some(time) = observation.capture_time_dcs {
            (
                classify_source_time(time, groove_entry_time, touchdown_time),
                SourceTimeAttributionBasis::CaptureTime,
            )
        } else if let Some((previous, next)) = ordered_bounds {
            (
                classify_source_time_bounds(
                    previous.capture_time_dcs,
                    next.capture_time_dcs,
                    groove_entry_time,
                    touchdown_time,
                ),
                SourceTimeAttributionBasis::SequenceAndCaptureTickBounds,
            )
        } else if groove_entry_time.is_none() {
            (
                ScoringSegmentAttribution::BeforeGroove,
                SourceTimeAttributionBasis::Unresolved,
            )
        } else {
            (
                ScoringSegmentAttribution::IndeterminateMissingSourceTime,
                SourceTimeAttributionBasis::Unresolved,
            )
        };

        let isolated_sequence = !invalid_sequences
            .contains(&observation.sequence.saturating_sub(1))
            && observation
                .sequence
                .checked_add(1)
                .is_none_or(|next| !invalid_sequences.contains(&next));
        let exact_time_inside_bounds = observation.capture_time_dcs.is_none_or(|time| {
            adjacent_bounds.is_some_and(|(previous, next)| {
                previous.capture_time_dcs <= time && time <= next.capture_time_dcs
            })
        });
        let coverage_gap_ms = adjacent_bounds.map(|(previous, next)| {
            (next.capture_time_dcs - previous.capture_time_dcs).max(0.0) * 1_000.0
        });
        let short_real_gap = isolated_sequence
            && exact_time_inside_bounds
            && coverage_gap_ms.is_some_and(|gap_ms| gap_ms <= SAMPLE_GAP_WARNING_MS + 1.0e-6);
        let touches_gate = match observation.capture_time_dcs {
            Some(time) => gate_brackets
                .iter()
                .any(|quality| gate_contains_time(quality, time)),
            None => ordered_bounds.is_some_and(|(previous, next)| {
                gate_brackets.iter().any(|quality| {
                    gate_intersects_interval(
                        quality,
                        previous.capture_time_dcs,
                        next.capture_time_dcs,
                    )
                })
            }),
        };

        let verdict_effect = match attribution {
            ScoringSegmentAttribution::BeforeGroove | ScoringSegmentAttribution::AfterTouchdown => {
                InvalidSourceVerdictEffect::DiagnosticOutsideScoredSegment
            }
            ScoringSegmentAttribution::InScoredSegment if short_real_gap && !touches_gate => {
                InvalidSourceVerdictEffect::DiagnosticCoveredShortGap
            }
            ScoringSegmentAttribution::InScoredSegment => {
                InvalidSourceVerdictEffect::BlockingCoverageGap
            }
            ScoringSegmentAttribution::IndeterminateMissingSourceTime => {
                InvalidSourceVerdictEffect::BlockingIndeterminateMissingSourceTime
            }
        };
        let affects_scoring = matches!(
            verdict_effect,
            InvalidSourceVerdictEffect::BlockingCoverageGap
                | InvalidSourceVerdictEffect::BlockingIndeterminateMissingSourceTime
        );

        observation.attribution = attribution;
        observation.attribution_basis = basis;
        observation.verdict_effect = verdict_effect;
        observation.affects_scoring = affects_scoring;

        match attribution {
            ScoringSegmentAttribution::BeforeGroove => {
                summary.before_groove = summary.before_groove.saturating_add(1)
            }
            ScoringSegmentAttribution::InScoredSegment => {
                summary.in_scored_segment = summary.in_scored_segment.saturating_add(1)
            }
            ScoringSegmentAttribution::AfterTouchdown => {
                summary.after_touchdown = summary.after_touchdown.saturating_add(1)
            }
            ScoringSegmentAttribution::IndeterminateMissingSourceTime => {
                summary.indeterminate = summary.indeterminate.saturating_add(1)
            }
        }
        match verdict_effect {
            InvalidSourceVerdictEffect::DiagnosticOutsideScoredSegment => {
                summary.outside_segment = summary.outside_segment.saturating_add(1)
            }
            InvalidSourceVerdictEffect::DiagnosticCoveredShortGap => {
                summary.covered_short_gap = summary.covered_short_gap.saturating_add(1)
            }
            InvalidSourceVerdictEffect::BlockingCoverageGap
            | InvalidSourceVerdictEffect::BlockingIndeterminateMissingSourceTime => {
                summary.blocking = summary.blocking.saturating_add(1)
            }
        }
    }
    summary
}

fn classify_source_time(
    time: f64,
    groove_entry_time: Option<f64>,
    touchdown_time: Option<f64>,
) -> ScoringSegmentAttribution {
    match groove_entry_time {
        None => ScoringSegmentAttribution::BeforeGroove,
        Some(entry) if time < entry => ScoringSegmentAttribution::BeforeGroove,
        Some(_) if touchdown_time.is_some_and(|touchdown| time > touchdown) => {
            ScoringSegmentAttribution::AfterTouchdown
        }
        Some(_) => ScoringSegmentAttribution::InScoredSegment,
    }
}

fn classify_source_time_bounds(
    lower: f64,
    upper: f64,
    groove_entry_time: Option<f64>,
    touchdown_time: Option<f64>,
) -> ScoringSegmentAttribution {
    let Some(entry) = groove_entry_time else {
        return ScoringSegmentAttribution::BeforeGroove;
    };
    if upper < entry {
        ScoringSegmentAttribution::BeforeGroove
    } else if touchdown_time.is_some_and(|touchdown| lower > touchdown) {
        ScoringSegmentAttribution::AfterTouchdown
    } else if lower >= entry && touchdown_time.is_none_or(|touchdown| upper <= touchdown) {
        ScoringSegmentAttribution::InScoredSegment
    } else {
        ScoringSegmentAttribution::IndeterminateMissingSourceTime
    }
}

fn gate_contains_time(quality: &GateQuality, time: f64) -> bool {
    quality.status == GateStatus::Valid
        && quality
            .bracket_start_time_dcs
            .zip(quality.bracket_end_time_dcs)
            .is_some_and(|(start, end)| start <= time && time <= end)
}

fn gate_intersects_interval(quality: &GateQuality, lower: f64, upper: f64) -> bool {
    quality.status == GateStatus::Valid
        && quality
            .bracket_start_time_dcs
            .zip(quality.bracket_end_time_dcs)
            .is_some_and(|(start, end)| lower <= end && start <= upper)
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
    quality.bracket_start_time_dcs = Some(previous.time);
    quality.bracket_end_time_dcs = Some(current.time);
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

/// One sample's worth of input for `replay_gate_and_trajectory`: an already-converted
/// approach-frame position, matching a persisted `Datum`'s `time`/`x`/`y`/`alt`/`roll_deg`/
/// `telemetry_valid`/`skew_ms` fields exactly.
pub(crate) struct ReplaySample {
    pub time: f64,
    pub x: f64,
    pub y: f64,
    pub alt: f64,
    pub valid: bool,
    pub skew_ms: f64,
    pub roll_deg: f64,
}

/// Replay a sequence of already-converted approach-frame samples through the same gate-capture
/// and continuous-trajectory logic `Track::next` applies live, without needing full DCS
/// Transform pairs or a live `Track`. Used by the `cadence-ab` diagnostic command to test an
/// artificially reduced sampling cadence against a corpus of already-recorded runs (JSON
/// `datums`) — see B.2 of the notation/cadence work plan.
///
/// This mirrors the gate/groove/trajectory logic of `Track::next` (as of this writing,
/// including the shared CATOBAR Case I roll-out detector);
/// keep the two in sync if that section changes. It intentionally does not replay
/// telemetry-quality bookkeeping, wire estimation or touchdown detection — the diagnostic only
/// ever needs gate/trajectory geometry, not a full recovery outcome.
pub(crate) fn replay_gate_and_trajectory(
    samples: impl IntoIterator<Item = ReplaySample>,
    ideal_base_alt: f64,
    glide_slope_deg: f64,
    carrier_is_vstol: bool,
) -> (GateDeviations, Vec<TrajectoryDeviation>) {
    let (gates, trajectory, _) = replay_gate_trajectory_and_groove(
        samples,
        ideal_base_alt,
        glide_slope_deg,
        carrier_is_vstol,
    );
    (gates, trajectory)
}

/// Full deterministic geometry replay used by `groove-ab`. Unlike the historical cadence A/B
/// wrapper above, this also returns the Case I roll-out evidence. It can reproduce the geometry
/// exactly from schema-v3 `datums`, but not UTC mapping or event/velocity-derived evidence.
pub(crate) fn replay_gate_trajectory_and_groove(
    samples: impl IntoIterator<Item = ReplaySample>,
    ideal_base_alt: f64,
    glide_slope_deg: f64,
    carrier_is_vstol: bool,
) -> (
    GateDeviations,
    Vec<TrajectoryDeviation>,
    Option<GrooveEntryEvidence>,
) {
    let mut gate_deviations = GateDeviations::default();
    let mut trajectory_deviations = Vec::new();
    let mut gate_samples: VecDeque<ApproachSample> = VecDeque::new();
    let mut previous_x = f64::MAX;
    let mut entered_groove = false;
    let mut case_i_groove_detector = CaseIGrooveDetector::default();
    let mut groove_entry = None;

    for ReplaySample {
        time,
        x,
        y,
        alt,
        valid,
        skew_ms,
        roll_deg,
    } in samples
    {
        if x <= 0.0 {
            previous_x = x;
            continue;
        }

        if x > GATE_THREE_QUARTER_NM {
            gate_deviations.at_three_quarter_nm = None;
            gate_deviations.three_quarter_quality = GateQuality::default();
        }
        if x > GATE_HALF_NM {
            gate_deviations.at_half_nm = None;
            gate_deviations.half_quality = GateQuality::default();
        }
        if x > GATE_QUARTER_NM {
            gate_deviations.at_quarter_nm = None;
            gate_deviations.quarter_quality = GateQuality::default();
        }

        let is_inbound = x < previous_x;
        let lineup_deg = y.atan2(x).to_degrees();
        let in_approach = m_to_ft(alt) <= 500.0;
        let gate_lined_up = !carrier_is_vstol || lineup_deg.abs() <= 10.0;
        let current = ApproachSample {
            time,
            x,
            y,
            alt,
            valid: valid && is_inbound,
            in_approach,
            lined_up: gate_lined_up,
            skew_ms,
        };

        if gate_samples.is_empty() {
            mark_started_inside(
                x,
                GATE_THREE_QUARTER_NM,
                &mut gate_deviations.three_quarter_quality,
            );
            mark_started_inside(x, GATE_HALF_NM, &mut gate_deviations.half_quality);
            mark_started_inside(x, GATE_QUARTER_NM, &mut gate_deviations.quarter_quality);
        } else {
            capture_gate_from_window(
                &gate_samples,
                &current,
                GATE_THREE_QUARTER_NM,
                ideal_base_alt,
                glide_slope_deg,
                &mut gate_deviations.at_three_quarter_nm,
                &mut gate_deviations.three_quarter_quality,
            );
            capture_gate_from_window(
                &gate_samples,
                &current,
                GATE_HALF_NM,
                ideal_base_alt,
                glide_slope_deg,
                &mut gate_deviations.at_half_nm,
                &mut gate_deviations.half_quality,
            );
            capture_gate_from_window(
                &gate_samples,
                &current,
                GATE_QUARTER_NM,
                ideal_base_alt,
                glide_slope_deg,
                &mut gate_deviations.at_quarter_nm,
                &mut gate_deviations.quarter_quality,
            );
        }

        gate_samples.push_back(current);
        while gate_samples
            .front()
            .is_some_and(|s| time - s.time > GATE_BUFFER_WINDOW_S)
        {
            gate_samples.pop_front();
        }

        // V/STOL remains box-only; CATOBAR uses the same pure Case I detector as `Track::next`.
        if carrier_is_vstol
            && x <= GATE_THREE_QUARTER_NM
            && m_to_ft(alt) <= 300.0
            && lineup_deg.abs() <= 10.0
        {
            if !entered_groove {
                trajectory_deviations.clear();
            }
            entered_groove = true;
        }

        if !carrier_is_vstol {
            let capture_gap_ms = gate_samples
                .iter()
                .rev()
                .nth(1)
                .map_or(0.0, |previous| (time - previous.time) * 1_000.0);
            let observation = CaseIGrooveObservation {
                time_dcs: time,
                x,
                altitude_relative_ft: m_to_ft(alt),
                lineup_deg,
                bank_deg: roll_deg,
                valid,
                inbound: is_inbound,
                capture_gap_ms,
            };
            match case_i_groove_detector.observe(observation) {
                CaseIGrooveUpdate::Confirmed => {
                    let quality = groove_quality_measurement(&gate_samples).unwrap_or(
                        GrooveQualityMeasurement {
                            track_angle_deg: 0.0,
                            lineup_rate_deg_per_s: 0.0,
                            inbound_progress_mps: 0.0,
                        },
                    );
                    groove_entry = Some(case_i_groove_detector.evidence(
                        observation,
                        quality,
                        "unavailable_offline_replay",
                        None,
                    ));
                    trajectory_deviations.clear();
                    entered_groove = true;
                }
                CaseIGrooveUpdate::BranchReset => {
                    entered_groove = false;
                    groove_entry = None;
                    trajectory_deviations.clear();
                }
                CaseIGrooveUpdate::None => {}
            }
        }

        if entered_groove
            && valid
            && is_inbound
            && (!carrier_is_vstol || in_approach)
            && gate_lined_up
            && x >= TRAJECTORY_MIN_DISTANCE_M
            && trajectory_deviations.len() < MAX_TRAJECTORY_SAMPLES
        {
            let ideal_alt = ideal_base_alt + x * glide_slope_deg.to_radians().tan();
            let gs_deviation_m = alt - ideal_alt;
            let (gs_deviation_deg, trajectory_lineup_deg) =
                trajectory_deviation_angles_deg(gs_deviation_m, y, x);
            let sink_rate_mps = sink_rate_since(trajectory_deviations.last(), alt, time);
            trajectory_deviations.push(TrajectoryDeviation {
                timestamp_dcs: time,
                distance_m: x,
                gs_deviation_deg,
                lineup_deg: trajectory_lineup_deg,
                lineup_deviation_m: y,
                track_angle_deg: groove_quality_measurement(&gate_samples)
                    .map_or(0.0, |quality| quality.track_angle_deg),
                alt_m: alt,
                bank_deg: roll_deg,
                sink_rate_mps,
            });
        }

        previous_x = x;
    }

    (gate_deviations, trajectory_deviations, groove_entry)
}

/// Whether a DCS LQM comment opens with a `GRADE:WO` waveoff/go-around grade (e.g. `LSO:
/// GRADE:WO : SLOX DRX (LURIM) DRIM (LURIC) WO(AFU)IC`). This confirms that a waveoff occurred,
/// while its `WaveoffUnknown` representation deliberately makes no claim about who initiated it.
fn dcs_grade_is_waveoff(comment: Option<&str>) -> bool {
    comment.is_some_and(|comment| comment.contains("GRADE:WO"))
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
    use crate::telemetry::SourceObservationEntity;

    #[test]
    fn wind_velocity_vector_matches_expected_world_frame_direction() {
        // Wind FROM north (0 deg) blows TOWARD south: -z, matching the `forward` convention
        // (yaw 0 = +z) used throughout this file.
        let from_north = wind_velocity_vector(0, 10.0);
        assert!((from_north.x).abs() < 1.0e-9);
        assert!((from_north.z + 10.0).abs() < 1.0e-9);

        // Wind FROM east (90 deg) blows TOWARD west: -x.
        let from_east = wind_velocity_vector(90, 5.0);
        assert!((from_east.x + 5.0).abs() < 1.0e-9);
        assert!((from_east.z).abs() < 1.0e-9);
    }

    #[test]
    fn wind_reference_interpolates_linearly_between_two_altitudes() {
        let reference = WindReference {
            alt_a_m: 100.0,
            wind_a: DVec3::new(1.0, 0.0, 0.0),
            alt_b_m: 0.0,
            wind_b: DVec3::new(0.0, 0.0, 0.0),
        };

        let midpoint = reference.at_altitude(50.0);
        assert!((midpoint.x - 0.5).abs() < 1.0e-9);

        // Above the higher sample and below the lower one: clamp to the nearest known reading
        // rather than extrapolating past what was actually measured.
        assert!((reference.at_altitude(150.0).x - 1.0).abs() < 1.0e-9);
        assert!((reference.at_altitude(-50.0).x).abs() < 1.0e-9);
    }

    #[test]
    fn wind_reference_handles_two_samples_at_the_same_altitude() {
        let reference = WindReference {
            alt_a_m: 50.0,
            wind_a: DVec3::new(2.0, 0.0, 0.0),
            alt_b_m: 50.0,
            wind_b: DVec3::new(4.0, 0.0, 0.0),
        };
        // Degenerate span: must not divide by zero or panic, and must return a defined value.
        assert!((reference.at_altitude(50.0).x - 2.0).abs() < 1.0e-9);
    }

    #[test]
    fn corrected_aoa_is_zero_for_level_flight_with_no_wind() {
        let aoa = corrected_aoa_deg(
            DVec3::new(0.0, 0.0, 50.0),
            DVec3::zero(),
            DRotor3::identity(),
        );
        assert!(aoa.abs() < 1.0e-9);
    }

    #[test]
    fn corrected_aoa_is_positive_when_sinking_relative_to_the_nose() {
        // Classic high-AoA case: nose level (forward = body z), but the aircraft is actually
        // moving somewhat downward relative to it (body y < 0).
        let aoa = corrected_aoa_deg(
            DVec3::new(0.0, -10.0, 50.0),
            DVec3::zero(),
            DRotor3::identity(),
        );
        assert!(aoa > 0.0, "expected a positive AoA, got {aoa}");
        assert!((aoa - 10.0_f64.atan2(50.0).to_degrees()).abs() < 1.0e-9);
    }

    #[test]
    fn headwind_correction_lowers_the_computed_aoa_for_the_same_sink_rate() {
        // Same sink rate (velocity.y = -5) in both cases; only the headwind differs. A headwind
        // raises true airspeed above ground speed, so the same sink rate corresponds to a
        // shallower (smaller) angle once corrected -- this is the whole point of subtracting
        // wind instead of using raw ground velocity.
        let velocity = DVec3::new(0.0, -5.0, 40.0);
        let raw = corrected_aoa_deg(velocity, DVec3::zero(), DRotor3::identity());
        let headwind = DVec3::new(0.0, 0.0, -20.0); // blowing against the aircraft's travel
        let corrected = corrected_aoa_deg(velocity, headwind, DRotor3::identity());
        assert!(
            corrected < raw,
            "expected headwind-corrected AoA ({corrected}) < raw ({raw})"
        );
    }

    #[test]
    fn corrected_aoa_is_nan_when_true_airspeed_is_zero() {
        let velocity = DVec3::new(0.0, 0.0, 20.0);
        let wind = DVec3::new(0.0, 0.0, 20.0); // exactly matches ground velocity
        assert!(corrected_aoa_deg(velocity, wind, DRotor3::identity()).is_nan());
    }

    #[test]
    fn effective_aoa_falls_back_to_the_raw_geometric_value_without_a_wind_reference() {
        let carrier = CarrierInfo::by_type("CVN_71").unwrap();
        let plane_info = AirplaneInfo::by_type("FA-18C_hornet").unwrap();
        let track = Track::new("pilot", carrier, plane_info);
        let plane = Transform {
            aoa: 7.5,
            velocity: DVec3::new(0.0, -5.0, 40.0),
            rotation: DRotor3::identity(),
            ..Transform::default()
        };
        assert_eq!(track.effective_aoa(&plane), 7.5);
    }

    #[test]
    fn effective_aoa_uses_the_corrected_value_once_a_wind_reference_is_set() {
        let carrier = CarrierInfo::by_type("CVN_71").unwrap();
        let plane_info = AirplaneInfo::by_type("FA-18C_hornet").unwrap();
        let mut track = Track::new("pilot", carrier, plane_info);
        track.set_wind_reference(300.0, DVec3::zero(), 0.0, DVec3::new(0.0, 0.0, -20.0));

        let plane = Transform {
            aoa: 999.0, // must be ignored once a wind reference is set
            alt: 0.0,   // interpolates to the wind_b reading exactly
            velocity: DVec3::new(0.0, -5.0, 40.0),
            rotation: DRotor3::identity(),
            ..Transform::default()
        };
        let expected =
            corrected_aoa_deg(plane.velocity, DVec3::new(0.0, 0.0, -20.0), plane.rotation);
        assert_eq!(track.effective_aoa(&plane), expected);
        assert_ne!(track.effective_aoa(&plane), 999.0);
    }

    #[test]
    fn wind_reference_probes_are_surfaced_for_diagnosis_when_established() {
        // Purely diagnostic (see `WindReferenceProbes`): must round-trip the exact raw responses
        // through `finish()`, independent of whatever velocity vectors were derived from them.
        let carrier = CarrierInfo::by_type("CVN_71").unwrap();
        let plane_info = AirplaneInfo::by_type("FA-18C_hornet").unwrap();
        let mut track = Track::new("pilot", carrier, plane_info);
        track.set_wind_reference(300.0, DVec3::zero(), 0.0, DVec3::new(0.0, 0.0, -20.0));
        let high = WindProbe {
            alt_m: 300.0,
            heading_deg: 180,
            speed_mps: 0.0,
        };
        let low = WindProbe {
            alt_m: 0.0,
            heading_deg: 95,
            speed_mps: 1.2,
        };
        track.set_wind_reference_probes(high, low, false);

        let result = track.finish();
        assert!(result.wind_reference_established);
        assert_eq!(
            result.wind_reference_probes,
            Some(WindReferenceProbes {
                high,
                low,
                low_reading_overridden_by_high: false,
            })
        );
    }

    #[test]
    fn low_wind_probe_sentinel_falls_back_to_high_probe_after_bounded_retry() {
        // Regression for tasking-roadmap.md P1: the low-altitude probe intermittently reads
        // DCS's `180deg/0.0 m/s` sentinel while the high probe stays coherent. Simulates the
        // caller-side decision `record_recovery.rs` makes after one retry still looks suspect --
        // this test only pins the `Track`-level bookkeeping, not the retry itself (no I/O here).
        let carrier = CarrierInfo::by_type("CVN_71").unwrap();
        let plane_info = AirplaneInfo::by_type("FA-18C_hornet").unwrap();
        let mut track = Track::new("pilot", carrier, plane_info);
        let high = WindProbe {
            alt_m: 300.0,
            heading_deg: 95,
            speed_mps: 1.2,
        };
        let low = WindProbe {
            alt_m: 0.0,
            heading_deg: 180,
            speed_mps: 0.0,
        };
        assert!(is_wind_sentinel((low.heading_deg, low.speed_mps)));
        assert!(!is_wind_sentinel((high.heading_deg, high.speed_mps)));
        // The AoA reference vector uses `high`'s wind for both points -- `low`'s raw sentinel
        // reading is kept only in `WindReferenceProbes` for diagnosis, never for correction.
        track.set_wind_reference(
            high.alt_m,
            wind_velocity_vector(high.heading_deg, high.speed_mps),
            low.alt_m,
            wind_velocity_vector(high.heading_deg, high.speed_mps),
        );
        track.set_wind_reference_probes(high, low, true);

        let result = track.finish();
        assert!(result.wind_reference_established);
        let probes = result.wind_reference_probes.unwrap();
        assert_eq!(probes.low, low);
        assert!(probes.low_reading_overridden_by_high);
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

    fn invalid_source_observation(
        sequence: u64,
        capture_time_dcs: Option<f64>,
        entity: crate::telemetry::SourceObservationEntity,
        status: &str,
        received_unix_ms: u64,
    ) -> InvalidSourceObservation {
        InvalidSourceObservation {
            sequence,
            capture_tick: sequence,
            capture_time_dcs,
            entity,
            status_code: 4,
            status: status.to_string(),
            reason: status.to_string(),
            source_read_time_dcs: Some(99.0),
            received_unix_ms,
            attribution: ScoringSegmentAttribution::IndeterminateMissingSourceTime,
            attribution_basis: SourceTimeAttributionBasis::Unresolved,
            source_time_lower_bound_dcs: None,
            source_time_upper_bound_dcs: None,
            coverage_gap_ms: None,
            previous_valid_sequence: None,
            next_valid_sequence: None,
            verdict_effect: InvalidSourceVerdictEffect::BlockingIndeterminateMissingSourceTime,
            affects_scoring: false,
        }
    }

    fn source_anchor(
        sequence: u64,
        capture_tick: u64,
        capture_time_dcs: f64,
    ) -> SourceCaptureAnchor {
        SourceCaptureAnchor {
            sequence,
            capture_tick,
            capture_time_dcs,
        }
    }

    fn assess_source_errors(
        observations: &mut [InvalidSourceObservation],
        anchors: &[SourceCaptureAnchor],
        gates: &GateDeviations,
    ) -> InvalidSourceCoverageSummary {
        assess_invalid_source_observation_coverage(
            observations,
            anchors,
            Some(10.0),
            Some(20.0),
            gates,
        )
    }

    #[test]
    fn isolated_source_error_bounded_inside_300_ms_is_diagnostic_only() {
        let mut observations = vec![invalid_source_observation(
            10,
            Some(15.1),
            SourceObservationEntity::Aircraft,
            "read_error",
            90_000,
        )];
        let summary = assess_source_errors(
            &mut observations,
            &[source_anchor(9, 9, 15.0), source_anchor(11, 11, 15.2)],
            &GateDeviations::default(),
        );

        assert_eq!(summary.covered_short_gap, 1);
        assert_eq!(summary.blocking, 0);
        assert!(observations[0]
            .coverage_gap_ms
            .is_some_and(|gap_ms| (gap_ms - 200.0).abs() < 1.0e-6));
        assert_eq!(
            observations[0].verdict_effect,
            InvalidSourceVerdictEffect::DiagnosticCoveredShortGap
        );
        assert!(!observations[0].affects_scoring);
    }

    #[test]
    fn isolated_source_error_accepts_exactly_300_ms_of_real_coverage() {
        let mut observations = vec![invalid_source_observation(
            10,
            Some(15.15),
            SourceObservationEntity::Carrier,
            "read_error",
            0,
        )];
        let summary = assess_source_errors(
            &mut observations,
            &[source_anchor(9, 9, 15.0), source_anchor(11, 11, 15.3)],
            &GateDeviations::default(),
        );
        assert_eq!(summary.covered_short_gap, 1);
        assert_eq!(summary.blocking, 0);
    }

    #[test]
    fn isolated_source_error_above_300_ms_blocks() {
        let mut observations = vec![invalid_source_observation(
            10,
            Some(15.151),
            SourceObservationEntity::Aircraft,
            "read_error",
            0,
        )];
        let summary = assess_source_errors(
            &mut observations,
            &[source_anchor(9, 9, 15.0), source_anchor(11, 11, 15.301)],
            &GateDeviations::default(),
        );
        assert_eq!(summary.covered_short_gap, 0);
        assert_eq!(summary.blocking, 1);
        assert_eq!(
            observations[0].verdict_effect,
            InvalidSourceVerdictEffect::BlockingCoverageGap
        );
    }

    #[test]
    fn consecutive_source_error_sequences_are_blocking() {
        let mut observations = vec![
            invalid_source_observation(
                10,
                Some(15.1),
                SourceObservationEntity::Aircraft,
                "read_error",
                0,
            ),
            invalid_source_observation(
                11,
                Some(15.15),
                SourceObservationEntity::Carrier,
                "read_error",
                0,
            ),
        ];
        let summary = assess_source_errors(
            &mut observations,
            &[source_anchor(9, 9, 15.0), source_anchor(12, 12, 15.2)],
            &GateDeviations::default(),
        );
        assert_eq!(summary.blocking, 2);
        assert!(observations
            .iter()
            .all(|observation| observation.affects_scoring));
    }

    #[test]
    fn source_errors_before_groove_and_after_touchdown_are_explicitly_outside() {
        let mut observations = vec![
            invalid_source_observation(
                10,
                Some(9.5),
                SourceObservationEntity::Aircraft,
                "read_error",
                0,
            ),
            invalid_source_observation(
                20,
                Some(20.5),
                SourceObservationEntity::Carrier,
                "read_error",
                0,
            ),
        ];
        let summary = assess_source_errors(&mut observations, &[], &GateDeviations::default());
        assert_eq!(summary.outside_segment, 2);
        assert_eq!(summary.blocking, 0);
        assert_eq!(
            observations[0].attribution,
            ScoringSegmentAttribution::BeforeGroove
        );
        assert_eq!(
            observations[1].attribution,
            ScoringSegmentAttribution::AfterTouchdown
        );
        assert!(observations.iter().all(|observation| {
            observation.verdict_effect == InvalidSourceVerdictEffect::DiagnosticOutsideScoredSegment
        }));
    }

    #[test]
    fn source_error_inside_a_valid_gate_bracket_remains_blocking() {
        let mut observations = vec![invalid_source_observation(
            10,
            Some(15.1),
            SourceObservationEntity::Aircraft,
            "read_error",
            0,
        )];
        let gates = GateDeviations {
            half_quality: GateQuality {
                status: GateStatus::Valid,
                reason: None,
                bracket_gap_ms: Some(200.0),
                bracket_start_time_dcs: Some(15.0),
                bracket_end_time_dcs: Some(15.2),
                coverage_source: None,
            },
            ..GateDeviations::default()
        };
        let summary = assess_source_errors(
            &mut observations,
            &[source_anchor(9, 9, 15.0), source_anchor(11, 11, 15.2)],
            &gates,
        );
        assert_eq!(summary.blocking, 1);
        assert_eq!(
            observations[0].verdict_effect,
            InvalidSourceVerdictEffect::BlockingCoverageGap
        );
    }

    #[test]
    fn missing_source_time_can_be_bounded_by_adjacent_sequence_and_tick() {
        let mut observation = invalid_source_observation(
            10,
            None,
            SourceObservationEntity::Carrier,
            "invalid_data",
            999_999,
        );
        observation.capture_tick = 100;
        let mut observations = vec![observation];
        let summary = assess_source_errors(
            &mut observations,
            &[source_anchor(9, 99, 15.0), source_anchor(11, 101, 15.2)],
            &GateDeviations::default(),
        );
        assert_eq!(summary.covered_short_gap, 1);
        assert_eq!(
            observations[0].attribution,
            ScoringSegmentAttribution::InScoredSegment
        );
        assert_eq!(
            observations[0].attribution_basis,
            SourceTimeAttributionBasis::SequenceAndCaptureTickBounds
        );
        assert_eq!(observations[0].source_time_lower_bound_dcs, Some(15.0));
        assert_eq!(observations[0].source_time_upper_bound_dcs, Some(15.2));
        assert_eq!(observations[0].received_unix_ms, 999_999);
    }

    #[test]
    fn missing_source_time_without_reliable_bounds_stays_indeterminate_and_blocking() {
        let mut observations = vec![invalid_source_observation(
            10,
            None,
            SourceObservationEntity::Carrier,
            "invalid_data",
            123_456,
        )];
        let summary = assess_source_errors(
            &mut observations,
            &[source_anchor(9, 99, 15.0)],
            &GateDeviations::default(),
        );
        assert_eq!(summary.indeterminate, 1);
        assert_eq!(summary.blocking, 1);
        assert_eq!(
            observations[0].attribution_basis,
            SourceTimeAttributionBasis::Unresolved
        );
        assert_eq!(
            observations[0].verdict_effect,
            InvalidSourceVerdictEffect::BlockingIndeterminateMissingSourceTime
        );
    }

    #[test]
    fn covered_short_error_coexists_with_an_independent_completeness_cause() {
        let carrier = CarrierInfo::by_type("CVN_71").unwrap();
        let plane = AirplaneInfo::by_type("FA-18C_hornet").unwrap();
        let mut track = Track::new("pilot", carrier, plane);
        track.entered_groove = true;
        track.groove_entry_time = Some(10.0);
        track.landing_time = Some(20.0);
        track.source_capture_anchors = vec![source_anchor(9, 9, 15.0), source_anchor(11, 11, 15.2)];
        track.record_invalid_source_observations(vec![invalid_source_observation(
            10,
            Some(15.1),
            SourceObservationEntity::Aircraft,
            "read_error",
            0,
        )]);
        track.mark_telemetry_gap(TelemetryInvalidReason::TelemetryGap);

        let result = track.finish();
        assert!(result
            .telemetry_quality
            .unavailability_causes
            .contains(&Completeness::TelemetryGap));
        assert!(!result
            .telemetry_quality
            .unavailability_causes
            .contains(&Completeness::InvalidTelemetry));
        assert_eq!(
            result
                .telemetry_quality
                .covered_short_gap_invalid_source_observations,
            1
        );
    }

    fn observe_kinematic_point(
        track: &mut Track,
        time: f64,
        relative_speed_mps: f64,
        hook_altitude_m: f64,
        gap_ms: f64,
    ) {
        let carrier = Transform {
            time,
            ..Transform::default()
        };
        let plane = Transform {
            time,
            velocity: DVec3::new(0.0, 0.0, relative_speed_mps),
            ..Transform::default()
        };
        let mut sample = TelemetrySample::from_replay(carrier, plane, None);
        sample.sample_gap_ms = gap_ms;
        track.observe_arrest_kinematics(&sample, &sample.carrier, &sample.plane, hook_altitude_m);
    }

    fn case_i_sample(
        time: f64,
        x: f64,
        lineup_deg: f64,
        altitude_ft: f64,
        bank: f64,
    ) -> ReplaySample {
        ReplaySample {
            time,
            x,
            y: x * lineup_deg.to_radians().tan(),
            alt: altitude_ft / 3.28084,
            valid: true,
            skew_ms: 0.0,
            roll_deg: bank,
        }
    }

    fn case_i_turn_and_rollout(
        rollout_samples: usize,
        rollout_lineup: impl Fn(usize) -> f64,
    ) -> Vec<ReplaySample> {
        let mut samples = (0..5)
            .map(|index| {
                let time = index as f64 * 0.05;
                case_i_sample(time, 1_700.0 - 20.0 * index as f64, -4.0, 550.0, 22.0)
            })
            .collect::<Vec<_>>();
        samples.extend((0..rollout_samples).map(|index| {
            let time = 0.25 + index as f64 * 0.05;
            case_i_sample(
                time,
                1_600.0 - 20.0 * index as f64,
                rollout_lineup(index),
                500.0,
                8.0,
            )
        }));
        samples
    }

    #[test]
    fn nominal_port_final_turn_confirms_physical_rollout() {
        let samples = case_i_turn_and_rollout(16, |index| -2.0 + index as f64 * 0.15);
        let (_, _, entry) = replay_gate_trajectory_and_groove(samples, 0.0, 3.5, false);
        let entry = entry.expect("nominal Case I roll-out should confirm");
        assert_eq!(entry.rollout_started_at_dcs, 0.25);
        assert!((entry.timestamp_dcs - 1.0).abs() < 1.0e-9);
        assert_eq!(entry.stability_sample_count, 16);
        assert!(entry.port_lineup_corridor_reached);
        assert_eq!(entry.approach_side, "port");
    }

    #[test]
    fn undershoot_remaining_outside_port_corridor_still_enters_groove() {
        let samples = case_i_turn_and_rollout(16, |_| -3.0);
        let (_, trajectory, entry) = replay_gate_trajectory_and_groove(samples, 0.0, 3.5, false);
        let entry = entry.expect("a real undershoot roll-out must not wait for lineup");
        assert!(!entry.port_lineup_corridor_reached);
        assert!(entry.lineup_deg < CASE_I_PORT_LINEUP_CORRIDOR_DEG);
        assert!(trajectory
            .first()
            .is_some_and(|sample| sample.lineup_deg < -2.0));
    }

    #[test]
    fn rapid_overshoot_crossing_axis_is_recorded_at_rollout() {
        let samples = case_i_turn_and_rollout(20, |index| -3.0 + index as f64 * 0.5);
        let (_, trajectory, entry) = replay_gate_trajectory_and_groove(samples, 0.0, 3.5, false);
        let entry = entry.expect("overshoot must not block physical roll-out");
        assert!(entry.port_lineup_corridor_reached);
        assert!(entry.lineup_deg > 2.0);
        assert!(trajectory.iter().any(|sample| sample.lineup_deg > 2.0));
    }

    #[test]
    fn poor_lineup_and_corrections_do_not_delay_entry_and_remain_in_trajectory() {
        let mut samples =
            case_i_turn_and_rollout(16, |index| if index % 2 == 0 { -4.0 } else { 3.0 });
        samples.extend((0..6).map(|index| {
            case_i_sample(
                1.05 + index as f64 * 0.05,
                1_280.0 - 20.0 * index as f64,
                if index % 2 == 0 { -5.0 } else { 4.0 },
                450.0,
                6.0,
            )
        }));
        let (_, trajectory, entry) = replay_gate_trajectory_and_groove(samples, 0.0, 3.5, false);
        let entry = entry.expect("quality errors are not entry prerequisites");
        assert!((entry.timestamp_dcs - 1.0).abs() < 1.0e-9);
        assert!(trajectory
            .iter()
            .any(|sample| sample.lineup_deg.abs() >= 4.0));
        assert!(trajectory
            .iter()
            .any(|sample| sample.track_angle_deg.abs() > 10.0));
    }

    #[test]
    fn bank_below_limit_for_less_than_point_75_seconds_does_not_confirm() {
        let samples = case_i_turn_and_rollout(15, |_| -1.0);
        let (_, _, entry) = replay_gate_trajectory_and_groove(samples, 0.0, 3.5, false);
        assert!(entry.is_none());
    }

    #[test]
    fn temporary_bank_dip_during_turn_does_not_confirm_rollout() {
        let mut samples = case_i_turn_and_rollout(8, |_| -1.0);
        samples.push(case_i_sample(0.65, 1_430.0, -1.0, 500.0, 10.1));
        samples.extend((0..16).map(|index| {
            case_i_sample(
                0.70 + index as f64 * 0.05,
                1_410.0 - 20.0 * index as f64,
                -1.0,
                500.0,
                9.0,
            )
        }));
        let (_, _, entry) = replay_gate_trajectory_and_groove(samples, 0.0, 3.5, false);
        let entry = entry.expect("a complete roll-out after the oscillation should confirm");
        assert_eq!(entry.rollout_started_at_dcs, 0.70);
        assert!((entry.timestamp_dcs - 1.45).abs() < 1.0e-9);
    }

    #[test]
    fn imperfect_route_confirms_after_point_75_seconds_when_inbound() {
        let samples = case_i_turn_and_rollout(16, |index| -8.0 + index as f64 * 2.0);
        let (_, _, entry) = replay_gate_trajectory_and_groove(samples, 0.0, 3.5, false);
        let entry = entry.expect("route quality must not gate entry");
        assert!(entry.inbound_progress_mps > 0.0);
        assert!(entry.track_angle_deg.abs() > GROOVE_ROLLOUT_MAX_TRACK_ANGLE_DEG);
    }

    #[test]
    fn wings_level_but_outbound_never_confirms() {
        let mut samples = case_i_turn_and_rollout(1, |_| -2.0);
        samples.extend((0..20).map(|index| {
            case_i_sample(
                0.30 + index as f64 * 0.05,
                1_600.0 + 20.0 * index as f64,
                -1.0,
                500.0,
                5.0,
            )
        }));
        let (_, _, entry) = replay_gate_trajectory_and_groove(samples, 0.0, 3.5, false);
        assert!(entry.is_none());
    }

    #[test]
    fn capture_gap_above_300_ms_resets_rollout_confirmation() {
        let mut samples = case_i_turn_and_rollout(8, |_| -1.0);
        samples.extend((0..17).map(|index| {
            case_i_sample(
                1.0 + index as f64 * 0.05,
                1_350.0 - 20.0 * index as f64,
                -1.0,
                480.0,
                5.0,
            )
        }));
        let (_, _, entry) = replay_gate_trajectory_and_groove(samples, 0.0, 3.5, false);
        let entry = entry.expect("a fresh full confirmation after the gap should succeed");
        assert_eq!(entry.rollout_started_at_dcs, 1.05);
        assert!((entry.timestamp_dcs - 1.80).abs() < 1.0e-9);
    }

    #[test]
    fn source_time_return_resets_the_case_i_branch() {
        let mut detector = CaseIGrooveDetector::default();
        let turn = CaseIGrooveObservation {
            time_dcs: 10.0,
            x: 1_600.0,
            altitude_relative_ft: 500.0,
            lineup_deg: -4.0,
            bank_deg: 20.0,
            valid: true,
            inbound: true,
            capture_gap_ms: 0.0,
        };
        assert_eq!(detector.observe(turn), CaseIGrooveUpdate::None);
        assert_eq!(detector.state, CaseIGrooveState::LastTurnArmed);
        assert_eq!(
            detector.observe(CaseIGrooveObservation {
                time_dcs: 9.5,
                bank_deg: 5.0,
                ..turn
            }),
            CaseIGrooveUpdate::BranchReset
        );
        assert_eq!(detector.state, CaseIGrooveState::SearchingPattern);
        assert!(detector.rollout_started_at_dcs.is_none());
    }

    #[test]
    fn initial_break_downwind_and_case_ii_straight_in_do_not_trigger() {
        let mut samples = (0..20)
            .map(|index| {
                case_i_sample(
                    index as f64 * 0.05,
                    2_000.0 - 10.0 * index as f64,
                    -2.0,
                    800.0,
                    25.0,
                )
            })
            .collect::<Vec<_>>();
        samples.extend((0..20).map(|index| {
            case_i_sample(
                1.0 + index as f64 * 0.05,
                1_800.0 + 10.0 * index as f64,
                -8.0,
                550.0,
                0.0,
            )
        }));
        samples.extend((0..30).map(|index| {
            case_i_sample(
                2.0 + index as f64 * 0.05,
                1_900.0 - 15.0 * index as f64,
                0.0,
                450.0,
                0.0,
            )
        }));
        let (_, _, entry) = replay_gate_trajectory_and_groove(samples, 0.0, 3.5, false);
        assert!(
            entry.is_none(),
            "no observed port final turn means no implicit Case II/III activation"
        );
    }

    #[test]
    fn prior_waveoff_branch_cannot_contaminate_new_case_i_final() {
        let mut samples = case_i_turn_and_rollout(16, |_| -1.0);
        samples.extend((0..12).map(|index| {
            case_i_sample(
                1.05 + index as f64 * 0.05,
                1_300.0 + 30.0 * index as f64,
                2.0,
                500.0,
                0.0,
            )
        }));
        samples.extend((0..5).map(|index| {
            case_i_sample(
                1.65 + index as f64 * 0.05,
                1_700.0 - 20.0 * index as f64,
                -4.0,
                550.0,
                22.0,
            )
        }));
        samples.extend((0..16).map(|index| {
            case_i_sample(
                1.90 + index as f64 * 0.05,
                1_600.0 - 20.0 * index as f64,
                -1.0,
                500.0,
                5.0,
            )
        }));
        let (_, trajectory, entry) = replay_gate_trajectory_and_groove(samples, 0.0, 3.5, false);
        let entry = entry.expect("second final should independently confirm");
        assert_eq!(entry.rollout_started_at_dcs, 1.90);
        assert!(trajectory.iter().all(|sample| sample.timestamp_dcs >= 2.65));
    }

    #[test]
    fn vstol_box_only_entry_is_unchanged() {
        let samples = (0..3).map(|index| {
            case_i_sample(
                index as f64 * 0.1,
                1_300.0 - 20.0 * index as f64,
                0.0,
                250.0,
                0.0,
            )
        });
        let (_, trajectory, entry) = replay_gate_trajectory_and_groove(samples, 0.0, 3.5, true);
        assert!(entry.is_none(), "V/STOL has no CATOBAR groove evidence");
        assert!(
            !trajectory.is_empty(),
            "historical V/STOL box still starts its trajectory"
        );
    }

    #[test]
    fn detector_and_groove_ab_replay_share_the_same_confirmation() {
        let samples = case_i_turn_and_rollout(16, |_| -1.0);
        let mut detector = CaseIGrooveDetector::default();
        let mut previous_x = f64::MAX;
        let mut direct_confirmation = None;
        for sample in &samples {
            let observation = CaseIGrooveObservation {
                time_dcs: sample.time,
                x: sample.x,
                altitude_relative_ft: m_to_ft(sample.alt),
                lineup_deg: sample.y.atan2(sample.x).to_degrees(),
                bank_deg: sample.roll_deg,
                valid: sample.valid,
                inbound: sample.x < previous_x,
                capture_gap_ms: detector
                    .last_time_dcs
                    .map_or(0.0, |previous| (sample.time - previous) * 1_000.0),
            };
            if detector.observe(observation) == CaseIGrooveUpdate::Confirmed {
                direct_confirmation = Some(sample.time);
            }
            previous_x = sample.x;
        }
        let (_, _, replay_entry) = replay_gate_trajectory_and_groove(samples, 0.0, 3.5, false);
        assert_eq!(
            direct_confirmation,
            replay_entry.map(|entry| entry.timestamp_dcs)
        );
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
            ..GateQuality::default()
        }
    }

    fn trajectory_point(timestamp_dcs: f64, distance_m: f64) -> TrajectoryDeviation {
        TrajectoryDeviation {
            timestamp_dcs,
            distance_m,
            gs_deviation_deg: 0.1,
            lineup_deg: 0.1,
            lineup_deviation_m: 0.0,
            track_angle_deg: 0.0,
            alt_m: 10.0,
            bank_deg: 0.0,
            sink_rate_mps: 0.0,
        }
    }

    #[test]
    fn continuous_trajectory_bracket_recovers_missing_required_gate_coverage() {
        let carrier = CarrierInfo::by_type("CVN_71").unwrap();
        let plane = AirplaneInfo::by_type("FA-18C_hornet").unwrap();
        let mut track = Track::new("pilot", carrier, plane);
        track.groove_entry_time = Some(1.0);
        track.grading = Some(Grading::Bolter);
        track.gate_deviations = GateDeviations {
            at_three_quarter_nm: Some(gate(1.2)),
            at_half_nm: None,
            at_quarter_nm: Some(gate(3.0)),
            three_quarter_quality: valid_quality(),
            half_quality: GateQuality::default(),
            quarter_quality: valid_quality(),
        };
        track.trajectory_deviations = vec![
            trajectory_point(1.1, 1_400.0),
            trajectory_point(1.2, 1_380.0),
            trajectory_point(1.9, 940.0),
            trajectory_point(2.0, 910.0),
            trajectory_point(2.9, 480.0),
            trajectory_point(3.0, 450.0),
        ];

        let result = track.finish();
        assert_eq!(
            result.telemetry_quality.completeness,
            Completeness::Complete
        );
        assert_eq!(result.pass_grade, PassGrade::Bolter);
        assert_eq!(
            result.gate_deviations.half_quality.coverage_source,
            Some("continuous_trajectory_bracket")
        );
        assert!(
            (result
                .gate_deviations
                .half_quality
                .bracket_gap_ms
                .expect("trajectory bracket gap")
                - 100.0)
                .abs()
                < 1.0e-6
        );
    }

    #[test]
    fn trajectory_gap_over_300_ms_cannot_recover_gate_coverage() {
        let mut quality = GateQuality::default();
        recover_gate_coverage_from_trajectory(
            &[trajectory_point(1.0, 940.0), trajectory_point(1.31, 910.0)],
            GATE_HALF_NM,
            &None,
            &mut quality,
        );
        assert_eq!(quality.status, GateStatus::Missing);
        assert_eq!(quality.coverage_source, None);
    }

    #[test]
    fn unfinished_recognisable_final_is_approach_only_and_keeps_measured_points() {
        let carrier = CarrierInfo::by_type("CVN_71").unwrap();
        let plane = AirplaneInfo::by_type("FA-18C_hornet").unwrap();
        let mut track = Track::new("pilot", carrier, plane);
        track.entered_groove = true;
        track.groove_entry_time = Some(1.0);
        track.gate_deviations = GateDeviations {
            at_three_quarter_nm: Some(gate(1.2)),
            at_half_nm: Some(gate(2.0)),
            at_quarter_nm: Some(gate(3.0)),
            three_quarter_quality: valid_quality(),
            half_quality: valid_quality(),
            quarter_quality: valid_quality(),
        };

        let result = track.finish();
        assert_eq!(result.grading, Grading::ApproachOnly);
        assert_eq!(result.pass_grade, PassGrade::Ok);
        assert_eq!(result.grade_points, Some(4.0));
    }

    #[test]
    fn significant_inner_gate_distinguishes_a_final_from_a_false_start() {
        let carrier = CarrierInfo::by_type("CVN_71").unwrap();
        let plane = AirplaneInfo::by_type("FA-18C_hornet").unwrap();

        let mut final_without_outcome = Track::new("pilot", carrier, plane);
        final_without_outcome.gate_deviations.at_half_nm = Some(gate(2.0));
        final_without_outcome.gate_deviations.half_quality = valid_quality();
        let result = final_without_outcome.finish();
        assert_eq!(result.grading, Grading::ApproachOnly);
        assert_eq!(result.pass_grade, PassGrade::Incomplete);
        assert_eq!(result.grade_points, None);

        let mut outer_pattern_only = Track::new("pilot", carrier, plane);
        outer_pattern_only.gate_deviations.at_three_quarter_nm = Some(gate(1.0));
        outer_pattern_only.gate_deviations.three_quarter_quality = valid_quality();
        assert_eq!(outer_pattern_only.finish().grading, Grading::Unknown);
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
        assert!(!gates.all_valid(None));
        gates.at_three_quarter_nm = Some(gate(1.0));
        gates.three_quarter_quality = valid_quality();
        assert!(!gates.all_valid(None));
        gates.at_half_nm = Some(gate(2.0));
        gates.half_quality = valid_quality();
        assert!(!gates.all_valid(None));
        gates.at_quarter_nm = Some(gate(3.0));
        gates.quarter_quality = valid_quality();
        assert!(gates.all_valid(None));
        gates.at_quarter_nm.as_mut().unwrap().timestamp_dcs = 1.5;
        assert!(!gates.all_valid(None));
    }

    #[test]
    fn three_quarter_gate_before_groove_entry_is_not_required() {
        // Regression for the relaxed rule: on a real Case I pattern the 3/4 NM gate is commonly
        // captured while the aircraft is still turning final, well before roll-out-confirmed
        // groove entry (confirmed live 5 September 2026, evening: 7 of 8 human passes). A pass
        // with only 1/2 NM and 1/4 NM valid and ordered is still gradable when the 3/4 NM gate --
        // present or not, valid or not -- was captured before groove entry.
        let mut gates = GateDeviations {
            at_half_nm: Some(gate(2.0)),
            half_quality: valid_quality(),
            at_quarter_nm: Some(gate(3.0)),
            quarter_quality: valid_quality(),
            ..Default::default()
        };
        // No 3/4 NM gate at all yet still gradable, as long as groove entry precedes it being
        // required.
        assert!(gates.all_valid(Some(1.5)));

        // A 3/4 NM gate that failed validation (turn-induced lineup, say) but was captured before
        // groove entry must not block grading either.
        gates.at_three_quarter_nm = Some(gate(1.0));
        gates.three_quarter_quality = GateQuality {
            status: GateStatus::Invalid,
            reason: Some("stale_skewed_or_reordered_gate_bracket".to_string()),
            bracket_gap_ms: None,
            ..GateQuality::default()
        };
        assert!(gates.all_valid(Some(1.5)));

        // The same gate, once captured *after* groove entry, is required again.
        gates.at_three_quarter_nm.as_mut().unwrap().timestamp_dcs = 1.8;
        assert!(!gates.all_valid(Some(1.5)));
    }

    #[test]
    fn three_quarter_gate_after_groove_entry_is_still_required() {
        // Companion test: when the 3/4 NM gate genuinely lands inside the groove (a tight
        // roll-out) and was captured but failed validation, it still blocks the pass -- being
        // *after* groove entry is what matters, not merely being present.
        let mut gates = GateDeviations {
            at_half_nm: Some(gate(2.0)),
            half_quality: valid_quality(),
            at_quarter_nm: Some(gate(3.0)),
            quarter_quality: valid_quality(),
            at_three_quarter_nm: Some(gate(1.0)),
            three_quarter_quality: GateQuality {
                status: GateStatus::Invalid,
                reason: Some("stale_skewed_or_reordered_gate_bracket".to_string()),
                bracket_gap_ms: None,
                ..GateQuality::default()
            },
        };
        // Captured at t=1.0, after a groove entry at t=0.5: still required, and invalid.
        assert!(!gates.all_valid(Some(0.5)));
        // With no groove-entry timestamp at all, the historical unconditional rule applies too.
        assert!(!gates.all_valid(None));

        gates.three_quarter_quality = valid_quality();
        assert!(gates.all_valid(Some(0.5)));
        assert!(gates.all_valid(None));
    }

    #[test]
    fn three_quarter_gate_missing_before_groove_entry_is_not_required() {
        // A 3/4 NM gate that was never captured at all (e.g. a telemetry gap during the turn)
        // must not be treated more strictly than one that was captured but turn-corrupted: groove
        // entry's own box condition (`x <= GATE_THREE_QUARTER_NM`) guarantees the crossing already
        // happened by the time groove entry is confirmed, present gate or not.
        let gates = GateDeviations {
            at_half_nm: Some(gate(2.0)),
            half_quality: valid_quality(),
            at_quarter_nm: Some(gate(3.0)),
            quarter_quality: valid_quality(),
            ..Default::default()
        };
        assert!(gates.all_valid(Some(1.5)));
        // But an unknown groove-entry timestamp still falls back to the unconditional rule.
        assert!(!gates.all_valid(None));
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
    fn source_invalid_inside_groove_withholds_grading() {
        let carrier = CarrierInfo::by_type("CVN_71").unwrap();
        let plane = AirplaneInfo::by_type("FA-18C_hornet").unwrap();
        let mut track = Track::new("pilot", carrier, plane);
        track.entered_groove = true;
        track.groove_entry_time = Some(10.0);
        track.landing_time = Some(20.0);
        track.record_invalid_source_observations(vec![invalid_source_observation(
            1,
            Some(15.0),
            crate::telemetry::SourceObservationEntity::Aircraft,
            "read_error",
            50_000,
        )]);
        let result = track.finish();
        assert_eq!(result.telemetry_quality.health, TelemetryHealth::Red);
        assert_eq!(
            result.telemetry_quality.health_reason,
            "source_invalid_observation"
        );
        assert!(result
            .telemetry_quality
            .unavailability_causes
            .contains(&Completeness::InvalidTelemetry));
        assert_eq!(result.telemetry_quality.scoring_invalid_samples, 1);
        assert_eq!(
            result.telemetry_quality.invalid_source_observations[0].attribution,
            ScoringSegmentAttribution::InScoredSegment
        );
    }

    #[test]
    fn source_invalid_before_groove_and_after_touchdown_are_diagnostic_only_even_if_delivered_late()
    {
        let carrier = CarrierInfo::by_type("CVN_71").unwrap();
        let plane = AirplaneInfo::by_type("FA-18C_hornet").unwrap();
        let mut track = Track::new("pilot", carrier, plane);
        track.entered_groove = true;
        track.groove_entry_time = Some(10.0);
        track.landing_time = Some(20.0);
        track.record_invalid_source_observations(vec![
            invalid_source_observation(
                1,
                Some(9.0),
                crate::telemetry::SourceObservationEntity::Aircraft,
                "not_found",
                90_000,
            ),
            invalid_source_observation(
                2,
                Some(21.0),
                crate::telemetry::SourceObservationEntity::Carrier,
                "id_mismatch",
                90_000,
            ),
        ]);
        let result = track.finish();
        assert!(!result
            .telemetry_quality
            .unavailability_causes
            .contains(&Completeness::InvalidTelemetry));
        assert_eq!(result.telemetry_quality.pattern_invalid_samples, 1);
        assert_eq!(
            result
                .telemetry_quality
                .post_touchdown_invalid_source_observations,
            1
        );
        let observations = &result.telemetry_quality.invalid_source_observations;
        assert_eq!(
            observations[0].attribution,
            ScoringSegmentAttribution::BeforeGroove
        );
        assert_eq!(
            observations[1].attribution,
            ScoringSegmentAttribution::AfterTouchdown
        );
        assert_eq!(observations[0].status, "not_found");
        assert_eq!(observations[1].status, "id_mismatch");
        assert_ne!(observations[0].reason, "time_went_backwards");
    }

    #[test]
    fn source_invalid_without_capture_time_is_indeterminate_and_conservative() {
        let carrier = CarrierInfo::by_type("CVN_71").unwrap();
        let plane = AirplaneInfo::by_type("FA-18C_hornet").unwrap();
        let mut track = Track::new("pilot", carrier, plane);
        track.entered_groove = true;
        track.groove_entry_time = Some(10.0);
        track.landing_time = Some(20.0);
        track.record_invalid_source_observations(vec![invalid_source_observation(
            1,
            None,
            crate::telemetry::SourceObservationEntity::Carrier,
            "invalid_data",
            123_000,
        )]);
        let result = track.finish();
        assert!(result
            .telemetry_quality
            .unavailability_causes
            .contains(&Completeness::InvalidTelemetry));
        assert_eq!(
            result
                .telemetry_quality
                .indeterminate_invalid_source_observations,
            1
        );
        assert_eq!(
            result.telemetry_quality.invalid_source_observations[0].attribution,
            ScoringSegmentAttribution::IndeterminateMissingSourceTime
        );
        assert_eq!(result.telemetry_quality.scoring_invalid_samples, 0);
    }

    #[test]
    fn kinematic_arrest_signature_accepts_only_a_sustained_deck_relative_stop() {
        let carrier = CarrierInfo::by_type("CVN_71").unwrap();
        let plane = AirplaneInfo::by_type("FA-18C_hornet").unwrap();
        let mut track = Track::new("pilot", carrier, plane);
        track.landing_time = Some(10.0);
        track.arrest_deceleration_onset_time = Some(9.8);
        // A late-delivered batch may contain source samples older than the contact event; they
        // must not contribute to the low-speed hold merely because they were received later.
        observe_kinematic_point(&mut track, 9.9, 1.0, 0.0, 100.0);
        for index in 0..=21 {
            observe_kinematic_point(&mut track, 10.0 + index as f64 * 0.1, 3.0, 0.0, 100.0);
        }
        let evidence = track.arrest_confirmation_evidence(None);
        assert!(evidence.kinematic.accepted);
        assert_eq!(evidence.source, "kinematic_diagnostic");
        assert_eq!(evidence.confidence, "medium");
        assert_eq!(evidence.verdict_effect, "diagnostic_only_no_grading_change");
        assert_eq!(evidence.kinematic.post_contact_valid_samples, 22);
        assert!(evidence.kinematic.low_speed_hold_s >= 2.0);
    }

    #[test]
    fn kinematic_arrest_signature_rejects_bolter_touch_and_go_and_transient_deceleration() {
        let carrier = CarrierInfo::by_type("CVN_71").unwrap();
        let plane = AirplaneInfo::by_type("FA-18C_hornet").unwrap();
        for grading in [
            Grading::Bolter,
            Grading::TouchAndGo {
                cable_estimated: None,
            },
        ] {
            let mut track = Track::new("pilot", carrier, plane);
            track.grading = Some(grading);
            track.landing_time = Some(10.0);
            track.arrest_deceleration_onset_time = Some(10.1);
            observe_kinematic_point(&mut track, 10.0, 3.0, 0.0, 100.0);
            let evidence = track.arrest_confirmation_evidence(None);
            assert!(!evidence.kinematic.accepted);
            assert!(evidence.kinematic.outcome_conflicts_with_arrest);
            assert_eq!(
                evidence.kinematic.reason,
                "observed_outcome_conflicts_with_arrest"
            );
        }

        // A touchdown followed only by a brief slowdown and renewed forward travel is not an
        // arrest even when it has not yet been classified as a bolter/touch-and-go.
        let mut transient = Track::new("pilot", carrier, plane);
        transient.grading = Some(Grading::Recovered {
            cable: None,
            cable_estimated: None,
        });
        transient.landing_time = Some(10.0);
        transient.arrest_deceleration_onset_time = Some(10.1);
        for index in 0..=5 {
            observe_kinematic_point(
                &mut transient,
                10.0 + index as f64 * 0.1,
                if index < 3 { 3.0 } else { 25.0 },
                0.0,
                100.0,
            );
        }
        let evidence = transient.arrest_confirmation_evidence(None);
        assert!(!evidence.kinematic.accepted);
        assert!(evidence.kinematic.forward_departure_detected);
    }

    #[test]
    fn kinematic_arrest_signature_rejects_a_bounce_and_incomplete_post_contact_telemetry() {
        let carrier = CarrierInfo::by_type("CVN_71").unwrap();
        let plane = AirplaneInfo::by_type("FA-18C_hornet").unwrap();

        let mut bounce = Track::new("pilot", carrier, plane);
        bounce.landing_time = Some(10.0);
        bounce.arrest_deceleration_onset_time = Some(10.0);
        observe_kinematic_point(&mut bounce, 10.0, 3.0, 0.0, 100.0);
        observe_kinematic_point(&mut bounce, 10.1, 3.0, 4.0, 100.0);
        assert_eq!(
            bounce.arrest_confirmation_evidence(None).kinematic.reason,
            "post_contact_bounce_detected"
        );

        let mut interrupted = Track::new("pilot", carrier, plane);
        interrupted.landing_time = Some(10.0);
        interrupted.arrest_deceleration_onset_time = Some(10.0);
        for index in 0..=5 {
            observe_kinematic_point(&mut interrupted, 10.0 + index as f64 * 0.1, 3.0, 0.0, 100.0);
        }
        let evidence = interrupted.arrest_confirmation_evidence(None);
        assert!(!evidence.kinematic.accepted);
        assert!(evidence.kinematic.telemetry_ended_before_conclusion);
        assert_eq!(
            evidence.kinematic.reason,
            "telemetry_ended_before_low_speed_hold_completed"
        );
    }

    #[test]
    fn kinematic_diagnostic_never_lifts_unconfirmed_arrest() {
        let carrier = CarrierInfo::by_type("CVN_71").unwrap();
        let plane = AirplaneInfo::by_type("FA-18C_hornet").unwrap();
        let mut track = Track::new("pilot", carrier, plane);
        track.grading = Some(Grading::Recovered {
            cable: None,
            cable_estimated: None,
        });
        track.landing_time = Some(10.0);
        track.arrest_deceleration_onset_time = Some(9.9);
        for index in 0..=21 {
            observe_kinematic_point(&mut track, 10.0 + index as f64 * 0.1, 2.0, 0.0, 100.0);
        }
        let result = track.finish();
        assert!(result.arrest_confirmation.kinematic.accepted);
        assert_eq!(result.arrest_confirmation.confidence, "medium");
        assert!(result
            .telemetry_quality
            .unavailability_causes
            .contains(&Completeness::UnconfirmedArrest));
        assert_eq!(result.grade_points, None);
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
        assert_eq!(result.pass_grade, result.approach_grade);
        assert_eq!(result.grade_points, None);
        assert_eq!(result.grade_points, None);
    }

    #[test]
    fn gate_before_groove_entry_does_not_block_an_otherwise_clean_catobar_pass() {
        // End-to-end regression for the relaxed rule, through `Track::finish()`: a 3/4 NM gate
        // with a turn-artifact lineup (-10.5 deg, matching a confirmed live human pass) captured
        // before groove entry must no longer force `--`/`NoGrade` on an otherwise clean approach.
        let carrier = CarrierInfo::by_type("CVN_71").unwrap();
        let plane = AirplaneInfo::by_type("FA-18C_hornet").unwrap();
        let mut track = Track::new("pilot", carrier, plane);
        track.groove_entry_time = Some(1.5);
        track.gate_deviations = GateDeviations {
            at_three_quarter_nm: Some(GateDatum {
                lineup_deg: -10.5,
                timestamp_dcs: 1.0,
                ..gate(1.0)
            }),
            at_half_nm: Some(gate(2.0)),
            at_quarter_nm: Some(gate(3.0)),
            three_quarter_quality: valid_quality(),
            half_quality: valid_quality(),
            quarter_quality: valid_quality(),
        };
        track.grading = Some(Grading::Recovered {
            cable: Some(3),
            cable_estimated: Some(3),
        });

        let result = track.finish();
        assert_eq!(result.pass_grade, PassGrade::Ok);
        assert!(result.telemetry_quality.completeness == Completeness::Complete);

        // Companion: the same gate captured *after* groove entry still blocks the pass.
        let mut late_track = Track::new("pilot", carrier, plane);
        late_track.groove_entry_time = Some(0.5);
        late_track.gate_deviations = GateDeviations {
            at_three_quarter_nm: Some(GateDatum {
                lineup_deg: -10.5,
                timestamp_dcs: 1.0,
                ..gate(1.0)
            }),
            at_half_nm: Some(gate(2.0)),
            at_quarter_nm: Some(gate(3.0)),
            three_quarter_quality: valid_quality(),
            half_quality: valid_quality(),
            quarter_quality: valid_quality(),
        };
        late_track.grading = Some(Grading::Recovered {
            cable: Some(3),
            cable_estimated: Some(3),
        });
        let late_result = late_track.finish();
        assert_eq!(late_result.pass_grade, PassGrade::NoGrade);
    }

    #[test]
    fn trajectory_deviation_recorded_between_gates_reaches_the_final_grade() {
        // All three gates are clean, but a continuous-series spike between the 1/2-nm and
        // 1/4-nm gates should still downgrade the final grade produced by `finish()` — the
        // wiring from Track::trajectory_deviations through to compute_pass_grade, not just
        // grade_from_gates in isolation (covered separately in grading.rs).
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
            cable: Some(3),
            cable_estimated: Some(3),
        });
        // Two consecutive elevated samples: satisfies the A.1 persistence guard
        // (`PERSISTENCE_MIN_CONSECUTIVE_SAMPLES` in grading.rs) that now requires a spike to
        // repeat on at least one neighboring sample before it counts, ruling out a single
        // aberrant telemetry frame.
        track.trajectory_deviations = vec![
            TrajectoryDeviation {
                timestamp_dcs: 2.5,
                distance_m: 700.0,
                gs_deviation_deg: 1.5,
                lineup_deg: 0.0,
                lineup_deviation_m: 0.0,
                track_angle_deg: 0.0,
                alt_m: 0.0,
                bank_deg: 0.0,
                sink_rate_mps: 0.0,
            },
            TrajectoryDeviation {
                timestamp_dcs: 2.6,
                distance_m: 690.0,
                gs_deviation_deg: 1.5,
                lineup_deg: 0.0,
                lineup_deviation_m: 0.0,
                track_angle_deg: 0.0,
                alt_m: 0.0,
                bank_deg: 0.0,
                sink_rate_mps: 0.0,
            },
        ];

        // A 1.5 deg medium in the middle zone (weight 1.2) is (OK) under the production policy;
        // the baseline added a level because the series ends inside the excursion.
        let result = track.finish();
        assert_eq!(result.pass_grade, PassGrade::OkParentheses);
        assert_eq!(result.trajectory_deviations.len(), 2);
    }

    #[test]
    fn an_entered_groove_populates_a_continuous_trajectory_series() {
        // This test isolates the continuous-series path; groove-entry persistence has dedicated
        // boundary tests below.
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

        // `Track::next` derives the deck-relative altitude as
        // `plane.alt - carrier_info.deck_altitude + hook.rotated_by(plane.rotation).y`; with the
        // identity rotation from `Transform::default()`, an on-glideslope `plane.alt` must add
        // that bias back so `alt_offset_m` below is a real, controlled deviation in metres.
        let on_glideslope_bias = carrier_info.deck_altitude - plane_info.hook.y;
        let fly = |track: &mut Track, time: f64, distance: f64, alt_offset_m: f64| {
            let mut carrier_frame = carrier.clone();
            carrier_frame.time = time;
            let ideal_alt =
                distance * plane_info.glide_slope.to_radians().tan() + on_glideslope_bias;
            let plane = Transform {
                time,
                position: landing - fb * distance,
                alt: ideal_alt + alt_offset_m,
                ..Transform::default()
            };
            track.next(&carrier_frame, &plane, Some(1.0));
        };

        // Establish ordinary inbound geometry, then latch groove entry explicitly because this
        // test covers continuous deviations rather than entry timing.
        fly(&mut track, 0.0, 1450.0, 25.0);
        fly(&mut track, 0.90, 900.0, 25.0);
        // 25 m stays under the 300 ft groove-entry altitude ceiling here while producing a
        // clearly significant (~1.8°) GS deviation.
        fly(&mut track, 1.15, 850.0, 25.0);
        fly(&mut track, 1.40, 800.0, 25.0);
        track.entered_groove = true;
        track.mark_fresh_groove_entry(1.65);
        fly(&mut track, 1.65, 750.0, 25.0);
        assert!(
            track
                .trajectory_deviations
                .iter()
                .any(|d| d.gs_deviation_deg >= 1.5),
            "noisy approach should have recorded a significant deviation, found: {:?}",
            track.trajectory_deviations
        );
    }

    #[test]
    fn trajectory_deviations_stop_before_the_atan2_blow_up_near_touchdown() {
        // Regression for the confirmed live bug: `gs_deviation_deg`/`lineup_deg` are
        // `atan2(offset_m, x)`, so as `x` (distance-to-ship) approaches 0 in the final metres
        // before contact, an ordinary few-decimetre flare produces an angle of several tens of
        // degrees with no geometric meaning (live repro: 70.3 deg at x=0.30 m). No pushed
        // sample should have `distance_m` below `TRAJECTORY_MIN_DISTANCE_M`, and none should
        // show a manufactured huge-angle deviation from a tiny, realistic vertical offset.
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
        let on_glideslope_bias = carrier_info.deck_altitude - plane_info.hook.y;
        let fly = |track: &mut Track, time: f64, distance: f64, alt_offset_m: f64| {
            let mut carrier_frame = carrier.clone();
            carrier_frame.time = time;
            let ideal_alt =
                distance * plane_info.glide_slope.to_radians().tan() + on_glideslope_bias;
            let plane = Transform {
                time,
                position: landing - fb * distance,
                alt: ideal_alt + alt_offset_m,
                ..Transform::default()
            };
            track.next(&carrier_frame, &plane, Some(1.0));
        };

        // Wings-level, on-centerline warm-up so the CATOBAR roll-out check confirms groove
        // entry (see `groove_entry_populates_a_continuous_trajectory_series` for the same
        // pattern), then fly a clean approach all the way down to a realistic touchdown flare
        // (a small, few-decimetre vertical offset at the last, sub-3-m samples).
        fly(&mut track, 0.0, 1450.0, 0.0);
        fly(&mut track, 0.90, 900.0, 0.0);
        fly(&mut track, 1.15, 850.0, 0.0);
        fly(&mut track, 1.40, 800.0, 0.0);
        fly(&mut track, 1.65, 750.0, 0.0);
        track.entered_groove = true;
        track.mark_fresh_groove_entry(2.0);
        fly(&mut track, 2.0, 300.0, 0.0);
        fly(&mut track, 2.5, 100.0, 0.0);
        fly(&mut track, 2.9, 10.0, 0.0);
        fly(&mut track, 3.0, 3.0, 0.1);
        fly(&mut track, 3.1, 1.47, 0.3);
        fly(&mut track, 3.2, 0.30, 0.3);

        assert!(
            !track.trajectory_deviations.is_empty(),
            "clean approach should still have recorded trajectory samples"
        );
        for deviation in &track.trajectory_deviations {
            assert!(
                deviation.distance_m >= TRAJECTORY_MIN_DISTANCE_M,
                "sample below the distance floor should never have been pushed: {deviation:?}"
            );
            assert!(
                deviation.gs_deviation_deg.abs() < 45.0,
                "a realistic offset must never produce a manufactured huge angle: {deviation:?}"
            );
        }
    }

    #[test]
    fn near_touchdown_flare_offset_no_longer_manufactures_a_large_angle() {
        // Regression for the confirmed-live-on-5-September-2026 near-touchdown geometry defect
        // (distinct from the atan2 blow-up above, which only guards `x < TRAJECTORY_MIN_DISTANCE_M`):
        // between that floor and several tens of metres out, an ordinary, essentially constant
        // flare/reference offset (confirmed live at ~0.75-1.1 m, both vertically and laterally,
        // on otherwise-clean passes) was still amplified into a double-digit-degree deviation
        // purely because `x` shrinks, pushing a pass three clean gates would have graded `Ok`/
        // `(Ok)` down to `NoGrade`. With `NEAR_TOUCHDOWN_ANGLE_REFERENCE_M` substituted for `x`
        // in this zone, the same constant 0.8 m offset held from 50 m down to 4 m must stay a
        // small, stable angle throughout, never approaching the old atan2(0.8, 4) ~= 11.3 deg.
        let carrier_info = CarrierInfo::by_type("CVN_71").unwrap();
        let plane_info = AirplaneInfo::by_type("FA-18C_hornet").unwrap();
        let carrier = Transform {
            forward: DVec3::unit_z(),
            ..Transform::default()
        };
        let landing = carrier_info.approach_reference_offset(plane_info);
        let fb_rot = DRotor3::from_rotation_xz(carrier_info.deck_angle.to_radians());
        let fb = DVec3::unit_z().rotated_by(fb_rot);
        let lateral_axis = DVec3::unit_x().rotated_by(fb_rot);
        let mut track = Track::new("pilot", carrier_info, plane_info);
        let on_glideslope_bias = carrier_info.deck_altitude - plane_info.hook.y;
        let fly = |track: &mut Track,
                   time: f64,
                   distance: f64,
                   vertical_offset_m: f64,
                   lateral_offset_m: f64| {
            let mut carrier_frame = carrier.clone();
            carrier_frame.time = time;
            let ideal_alt =
                distance * plane_info.glide_slope.to_radians().tan() + on_glideslope_bias;
            let plane = Transform {
                time,
                position: landing - fb * distance + lateral_axis * lateral_offset_m,
                alt: ideal_alt + vertical_offset_m,
                ..Transform::default()
            };
            track.next(&carrier_frame, &plane, Some(1.0));
        };

        // Wings-level, on-centerline warm-up so the CATOBAR roll-out check confirms groove entry,
        // then hold a small, constant vertical+lateral offset from 50 m down to 4 m — the same
        // shape observed live (a roughly constant few-decimetre offset, not a growing one).
        fly(&mut track, 0.0, 1450.0, 0.0, 0.0);
        fly(&mut track, 0.90, 900.0, 0.0, 0.0);
        fly(&mut track, 1.15, 850.0, 0.0, 0.0);
        fly(&mut track, 1.40, 800.0, 0.0, 0.0);
        fly(&mut track, 1.65, 750.0, 0.0, 0.0);
        track.entered_groove = true;
        track.mark_fresh_groove_entry(2.0);
        fly(&mut track, 2.0, 300.0, 0.0, 0.0);
        fly(&mut track, 2.5, 100.0, 0.0, 0.0);
        fly(&mut track, 3.0, 50.0, 0.8, 0.8);
        fly(&mut track, 3.1, 30.0, 0.8, 0.8);
        fly(&mut track, 3.2, 15.0, 0.8, 0.8);
        fly(&mut track, 3.3, 8.0, 0.8, 0.8);
        fly(&mut track, 3.4, 4.0, 0.8, 0.8);

        let near_touchdown: Vec<_> = track
            .trajectory_deviations
            .iter()
            .filter(|d| d.distance_m <= 50.0)
            .collect();
        assert!(
            !near_touchdown.is_empty(),
            "the near-touchdown offset samples should have been recorded"
        );
        for deviation in &near_touchdown {
            assert!(
                deviation.gs_deviation_deg.abs() < 1.0,
                "a constant, realistic flare offset must not be amplified into a large angle \
                 purely by a shrinking x: {deviation:?}"
            );
            assert!(
                deviation.lineup_deg.abs() < 0.5,
                "a constant, realistic lateral offset must not be amplified into a large angle \
                 purely by a shrinking x: {deviation:?}"
            );
            assert!((deviation.lineup_deviation_m - 0.8).abs() < 1e-6);
        }
    }

    #[test]
    fn late_window_lineup_uses_one_consistent_lateral_severity() {
        // The human F-14B(U) corpus exposed the discontinuity in meaning caused by the former
        // 75 m floor: 1.5 degrees meant 3.93 m at 150 m but only 1.96 m in close. A constant
        // lateral displacement must now retain the same normalized angle throughout the entire
        // 150 m late window, while a genuinely larger displacement still crosses the threshold.
        let threshold_offset_m = 150.0 * 1.5_f64.to_radians().tan();
        let at_window = trajectory_deviation_angles_deg(0.0, threshold_offset_m, 150.0).1;
        let at_ramp = trajectory_deviation_angles_deg(0.0, threshold_offset_m, 4.0).1;
        assert!((at_window - 1.5).abs() < 1e-12);
        assert!((at_ramp - at_window).abs() < 1e-12);

        let real_large_offset = trajectory_deviation_angles_deg(0.0, 5.0, 4.0).1;
        assert!(real_large_offset > 1.5);
    }

    #[test]
    fn near_touchdown_large_offset_still_crosses_the_cut_threshold() {
        // Companion to the test above: the fixed reference distance must not blind the module to
        // a genuinely large, sustained low excursion near the ramp — it should still be able to
        // cross the -2.5 deg GS Cut threshold, just from a real number of metres of deviation
        // (with `NEAR_TOUCHDOWN_ANGLE_REFERENCE_M = 75`, atan2(-3.5, 75) ~= -2.67 deg) rather than
        // from an ordinary flare's decimetres divided by an almost-zero remaining distance.
        let carrier_info = CarrierInfo::by_type("CVN_71").unwrap();
        let plane_info = AirplaneInfo::by_type("FA-18C_hornet").unwrap();
        let carrier = Transform {
            forward: DVec3::unit_z(),
            ..Transform::default()
        };
        let landing = carrier_info.approach_reference_offset(plane_info);
        let fb_rot = DRotor3::from_rotation_xz(carrier_info.deck_angle.to_radians());
        let fb = DVec3::unit_z().rotated_by(fb_rot);
        let mut track = Track::new("pilot", carrier_info, plane_info);
        let on_glideslope_bias = carrier_info.deck_altitude - plane_info.hook.y;
        let fly = |track: &mut Track, time: f64, distance: f64, vertical_offset_m: f64| {
            let mut carrier_frame = carrier.clone();
            carrier_frame.time = time;
            let ideal_alt =
                distance * plane_info.glide_slope.to_radians().tan() + on_glideslope_bias;
            let plane = Transform {
                time,
                position: landing - fb * distance,
                alt: ideal_alt + vertical_offset_m,
                ..Transform::default()
            };
            track.next(&carrier_frame, &plane, Some(1.0));
        };

        fly(&mut track, 0.0, 1450.0, 0.0);
        fly(&mut track, 0.90, 900.0, 0.0);
        fly(&mut track, 1.15, 850.0, 0.0);
        fly(&mut track, 1.40, 800.0, 0.0);
        fly(&mut track, 1.65, 750.0, 0.0);
        track.entered_groove = true;
        track.mark_fresh_groove_entry(2.0);
        fly(&mut track, 2.0, 300.0, 0.0);
        fly(&mut track, 2.5, 100.0, 0.0);
        fly(&mut track, 3.0, 50.0, -3.5);
        fly(&mut track, 3.1, 30.0, -3.5);
        fly(&mut track, 3.2, 15.0, -3.5);
        fly(&mut track, 3.3, 8.0, -3.5);
        fly(&mut track, 3.4, 4.0, -3.5);

        assert!(
            track
                .trajectory_deviations
                .iter()
                .any(|d| d.distance_m <= 50.0 && d.gs_deviation_deg <= -2.5),
            "a genuinely large, sustained low excursion must still cross the Cut threshold \
             near touchdown: {:?}",
            track.trajectory_deviations
        );
    }

    #[test]
    fn continuous_trajectory_carries_sink_rate_and_bank_informationally() {
        // Piste 3 (sink rate / NATOPS TMRD) and piste 5 (bank angle) of the notation work:
        // both are surfaced on `TrajectoryDeviation` for context, never used by
        // `compute_pass_grade` (see AGENTS.md: sink rate is never notated).
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

        let on_glideslope_bias = carrier_info.deck_altitude - plane_info.hook.y;
        let fly = |track: &mut Track, time: f64, distance: f64, roll: f64| {
            let mut carrier_frame = carrier.clone();
            carrier_frame.time = time;
            let ideal_alt =
                distance * plane_info.glide_slope.to_radians().tan() + on_glideslope_bias;
            let plane = Transform {
                time,
                position: landing - fb * distance,
                alt: ideal_alt,
                roll,
                ..Transform::default()
            };
            track.next(&carrier_frame, &plane, Some(1.0));
        };

        // Warm up geometry, then latch groove entry explicitly before the two samples this test
        // actually cares about; entry timing has its own persistence and boundary tests.
        fly(&mut track, 0.0, 1450.0, 0.0);
        fly(&mut track, 0.35, 1000.0, 0.0);
        fly(&mut track, 0.60, 950.0, 0.0);
        fly(&mut track, 0.85, 900.0, 0.0);

        track.entered_groove = true;
        track.mark_fresh_groove_entry(1.10);
        fly(&mut track, 1.10, 800.0, 3.0);
        fly(&mut track, 1.20, 700.0, -12.5);

        assert_eq!(
            track.trajectory_deviations.len(),
            2,
            "found: {:?}",
            track.trajectory_deviations
        );
        let first = &track.trajectory_deviations[0];
        let second = &track.trajectory_deviations[1];

        // First sample of a run has nothing to compare against.
        assert_eq!(first.sink_rate_mps, 0.0);
        // On-glideslope descent between two closer samples: altitude must have decreased, so the
        // sink rate is positive and matches the raw alt/time delta exactly (same formula used by
        // both `Track::next` and `replay_gate_and_trajectory`, see `sink_rate_since`).
        assert!(second.alt_m < first.alt_m);
        let expected_sink_rate =
            (first.alt_m - second.alt_m) / (second.timestamp_dcs - first.timestamp_dcs);
        assert!((second.sink_rate_mps - expected_sink_rate).abs() < 1.0e-9);
        assert!(second.sink_rate_mps > 0.0);

        // Bank is a direct, per-sample passthrough of the raw telemetry roll.
        assert_eq!(first.bank_deg, 3.0);
        assert_eq!(second.bank_deg, -12.5);
    }

    #[test]
    fn mark_fresh_groove_entry_clears_stale_wire_and_trajectory_evidence() {
        // A bolter re-attempt calls this only on the false -> true transition of
        // entered_groove; its job is to make sure nothing from the discarded first attempt
        // (wire-plane history, wire crossings, continuous trajectory) leaks into the one that
        // ends up scored.
        let carrier = CarrierInfo::by_type("CVN_71").unwrap();
        let plane = AirplaneInfo::by_type("FA-18C_hornet").unwrap();
        let mut track = Track::new("pilot", carrier, plane);
        track.previous_wire_plane = [Some((1.0, 2.0)); 4];
        track.wire_crossings.push(WireCrossingEvidence {
            wire: 3,
            timestamp_dcs: 5.0,
            bracket_gap_ms: 50.0,
            method: "test",
        });
        track.trajectory_deviations.push(TrajectoryDeviation {
            timestamp_dcs: 5.0,
            distance_m: 700.0,
            gs_deviation_deg: 5.0,
            lineup_deg: 0.0,
            lineup_deviation_m: 0.0,
            track_angle_deg: 0.0,
            alt_m: 0.0,
            bank_deg: 0.0,
            sink_rate_mps: 0.0,
        });

        track.mark_fresh_groove_entry(12.0);

        assert_eq!(track.groove_entry_time, Some(12.0));
        assert_eq!(track.previous_wire_plane, [None; 4]);
        assert!(track.wire_crossings.is_empty());
        assert!(track.trajectory_deviations.is_empty());
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
        assert_eq!(result.grading, Grading::ApproachOnly);
        assert_eq!(result.pass_grade, PassGrade::Ok);
        assert_eq!(result.grade_points, Some(4.0));
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
    fn pilot_facing_outcome_never_mentions_a_diverging_rust_estimate() {
        // DCS/LQM evidence disagrees with the Rust geometric estimate: the pilot-facing headline
        // must show only the DCS wire, never a contradicting "estimate 4" next to "wire 3" — see
        // Grading::pilot_facing_outcome. The full JSON `outcome` field (recovery_outcome in
        // record_recovery.rs, not exercised here) is the only place allowed to show both.
        let grading = Grading::Recovered {
            cable: Some(3),
            cable_estimated: Some(4),
        };
        assert_eq!(
            grading.pilot_facing_outcome(false),
            "Arrested — wire 3",
            "must not mention the diverging Rust estimate"
        );
    }

    #[test]
    fn pilot_facing_outcome_shows_dcs_wire_alone_even_when_it_agrees_with_the_estimate() {
        let grading = Grading::Recovered {
            cable: Some(3),
            cable_estimated: Some(3),
        };
        assert_eq!(grading.pilot_facing_outcome(false), "Arrested — wire 3");
    }

    #[test]
    fn pilot_facing_outcome_falls_back_to_the_rust_estimate_only_when_dcs_said_nothing() {
        let grading = Grading::Recovered {
            cable: None,
            cable_estimated: Some(3),
        };
        assert_eq!(
            grading.pilot_facing_outcome(false),
            "Wire #3 (Rust estimate)"
        );
    }

    #[test]
    fn pilot_facing_outcome_reports_missing_wire_evidence() {
        let grading = Grading::Recovered {
            cable: None,
            cable_estimated: None,
        };
        assert_eq!(
            grading.pilot_facing_outcome(false),
            "Arrested — wire evidence unavailable"
        );
    }

    #[test]
    fn pilot_facing_outcome_other_grading_variants() {
        assert_eq!(Grading::Bolter.pilot_facing_outcome(false), "Bolter");
        assert_eq!(
            Grading::WaveoffUnknown.pilot_facing_outcome(false),
            "Waveoff/Go-around — initiator unknown"
        );
        assert_eq!(
            Grading::TouchAndGo {
                cable_estimated: None
            }
            .pilot_facing_outcome(false),
            "T&G (CQ)"
        );
        assert_eq!(
            Grading::TouchAndGo {
                cable_estimated: None
            }
            .pilot_facing_outcome(true),
            "Waveoff/Go-around"
        );
        assert_eq!(
            Grading::Recovered {
                cable: Some(1),
                cable_estimated: Some(1)
            }
            .pilot_facing_outcome(true),
            "Spot 7.5"
        );
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
            assert_eq!(result.pass_grade, result.approach_grade, "{invalid}");
            assert_eq!(result.grade_points, None, "{invalid}");
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
        assert!(result.hook_observation.timeline_truncated);
        assert_eq!(result.hook_observation.timeline_dropped_samples, 88);
        assert_eq!(
            result.hook_observation.truncation_reason,
            Some("capacity_reached_oldest_observation_evicted")
        );
        assert_ne!(
            result.telemetry_quality.completeness,
            Completeness::BufferLimit
        );
    }

    #[test]
    fn hook_history_capacity_covers_three_minutes_at_four_hz_without_truncation() {
        let carrier = CarrierInfo::by_type("CVN_71").unwrap();
        let plane = AirplaneInfo::by_type("FA-18C_hornet").unwrap();
        let mut track = Track::new("pilot", carrier, plane);
        for sequence in 0..720 {
            track.observe_hook_sample(
                sequence as f64 / 4.0,
                sequence,
                0.0,
                Some(1.0),
                HookSampleStatus::Success,
            );
        }
        let result = track.finish();
        assert_eq!(result.hook_observation.timeline.len(), 720);
        assert!(!result.hook_observation.timeline_truncated);
        assert_eq!(result.hook_observation.timeline_dropped_samples, 0);
        assert_eq!(result.hook_observation.timeline_capacity, 2_048);
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
        assert_eq!(quality.health_reason, "sustained_capture_gap");
        assert!((quality.effective_frequency_hz - 5.0).abs() < 0.01);
        assert!((quality.gap_p99_ms - 200.0).abs() < 0.01);
        assert!((quality.capture_gap_p99_ms - 200.0).abs() < 0.01);
        assert_eq!(quality.delivery_age_p99_ms, 0.0);
    }

    #[test]
    fn continuous_capture_delivered_late_is_reported_separately_from_a_capture_gap() {
        let carrier_info = CarrierInfo::by_type("CVN_71").unwrap();
        let plane_info = AirplaneInfo::by_type("FA-18C_hornet").unwrap();
        let mut track = Track::new("pilot", carrier_info, plane_info);
        let mut previous = None;
        for sequence in 0..=120 {
            let time = sequence as f64 * 0.05;
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
            let sample = TelemetrySample::from_source_pair(carrier, plane, previous, 700.0);
            previous = Some(time);
            assert!(track.next_sample(&sample, None));
        }
        let quality = track.finish().telemetry_quality;
        assert_eq!(quality.health, TelemetryHealth::Red);
        assert_eq!(quality.health_reason, "sustained_delivery_latency");
        assert_eq!(quality.capture_gap_p99_ms, 50.0);
        assert_eq!(quality.delivery_age_p99_ms, 700.0);
        assert!(quality.capture_gap_warning_ratio < 0.01);
        assert!(quality.late_delivery_warning_ratio > 0.99);
    }

    #[test]
    fn duplicate_vstol_touchdown_never_rewrites_the_accepted_spot_evidence() {
        // Review finding F03: DCS delivers two `Land` events for one V/STOL landing. The second
        // one, arriving after the aircraft has rolled a few metres, used to overwrite the spot
        // distance measured at the first, accepted, contact.
        let carrier_info = CarrierInfo::by_type("LHA_Tarawa").unwrap();
        let plane_info = AirplaneInfo::by_type("AV8BNA").unwrap();
        let carrier = Transform::default();
        let spot = match &carrier_info.recovery {
            CarrierRecovery::Vstol { landing_point, .. } => *landing_point,
            CarrierRecovery::Arrested => unreachable!(),
        };
        let at_spot = Transform {
            time: 10.0,
            position: spot - plane_info.landing_reference,
            ..Transform::default()
        };
        let rolled_12_m = Transform {
            time: 10.2,
            position: spot - plane_info.landing_reference + DVec3::new(0.0, 0.0, 12.0),
            ..Transform::default()
        };
        let mut track = Track::new("pilot", carrier_info, plane_info);
        assert!(track.landed(&carrier, &at_spot));
        let accepted_distance = track.spot_distance_m.expect("spot distance measured");
        assert!(accepted_distance < 0.01, "{accepted_distance}");
        let datums_after_first = track.datums.len();

        assert!(!track.landed(&carrier, &rolled_12_m));
        assert_eq!(track.spot_distance_m, Some(accepted_distance));
        assert_eq!(track.landing_time, Some(10.0));
        assert_eq!(track.datums.len(), datums_after_first);
    }

    #[test]
    fn non_finite_touchdown_transform_is_rejected_before_any_mutation() {
        let carrier_info = CarrierInfo::by_type("CVN_71").unwrap();
        let plane_info = AirplaneInfo::by_type("FA-18C_hornet").unwrap();
        let mut track = Track::new("pilot", carrier_info, plane_info);
        let poisoned = Transform {
            time: 10.0,
            position: DVec3::new(f64::NAN, 0.0, 0.0),
            ..Transform::default()
        };
        assert!(!track.landed(&Transform::default(), &poisoned));
        assert_eq!(track.grading, None);
        assert_eq!(track.landing_time, None);
    }

    #[test]
    fn invalid_sample_only_feeds_telemetry_quality_never_geometry() {
        // Review finding F05: a sample the boundary marked invalid must still count towards the
        // quality accounting, but must not move the carrier smoothing, the pattern trace, the
        // distance floor or any outcome.
        let carrier_info = CarrierInfo::by_type("CVN_71").unwrap();
        let plane_info = AirplaneInfo::by_type("FA-18C_hornet").unwrap();
        let mut track = Track::new("pilot", carrier_info, plane_info);
        let carrier = Transform {
            time: 10.0,
            ..Transform::default()
        };
        let plane = Transform {
            time: 10.0,
            position: DVec3::new(0.0, 100.0, 1_500.0),
            alt: 100.0,
            ..Transform::default()
        };
        assert!(track.next(&carrier, &plane, None));
        let smoothed = track.smoothed_carrier_pos;
        let pattern_datums = track.pattern_datums.len();
        let previous_distance = track.previous_distance;
        let invalid_samples = track.telemetry_quality.invalid_samples;

        let poisoned_carrier = Transform {
            time: 10.1,
            position: DVec3::new(f64::NAN, 0.0, 0.0),
            ..Transform::default()
        };
        let poisoned_plane = Transform {
            time: 10.1,
            position: DVec3::new(f64::NAN, f64::NAN, f64::NAN),
            alt: f64::NAN,
            ..plane.clone()
        };
        let sample = TelemetrySample::from_replay(poisoned_carrier, poisoned_plane, Some(10.0));
        assert_eq!(
            sample.invalid_reason,
            Some(TelemetryInvalidReason::NonFiniteValue)
        );
        assert!(track.next_sample(&sample, Some(1.0)));

        assert_eq!(track.smoothed_carrier_pos, smoothed);
        assert_eq!(track.pattern_datums.len(), pattern_datums);
        assert_eq!(track.previous_distance, previous_distance);
        assert_eq!(track.grading, None);
        assert_eq!(track.hook_observation.timeline.len(), 0);
        assert_eq!(track.telemetry_quality.invalid_samples, invalid_samples + 1);
        assert!(track
            .telemetry_quality
            .reasons
            .contains(&TelemetryInvalidReason::NonFiniteValue));
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
        // The carrier clock must follow the aircraft clock: a carrier frozen at t=0 makes every
        // sample an `ExcessiveSkew` invalid sample, which no longer drives any outcome.
        let carrier_at = |time: f64| Transform {
            time,
            ..Transform::default()
        };
        let contact = Transform {
            time: 1.0,
            ..Transform::default()
        };
        let mut track = Track::new("pilot", carrier_info, plane_info);
        assert!(track.next(&carrier_at(1.0), &contact, None));
        assert!(track.landed(&carrier_at(1.0), &contact));
        let departure = Transform {
            time: 2.0,
            position: DVec3::new(0.0, 0.0, 300.0),
            ..Transform::default()
        };
        assert!(!track.next(&carrier_at(2.0), &departure, None));
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
            assert_eq!(track.wire_estimate_at(1.1, true).wire, Some(3));
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

        let estimate = track.wire_estimate_at(2.0, true);
        assert_eq!(estimate.wire, None);
        assert_eq!(
            estimate.reason,
            "wire_crossing_not_time_correlated_with_event"
        );
    }

    /// A Tomcat with the four hook-plane crossings of a real pass (aircraft `x` +14, +3, -10,
    /// -22 m at the wires, 0.2 s apart) and no hook transient.
    fn tomcat_with_four_crossings() -> Track {
        let carrier_info = CarrierInfo::by_type("CVN_71").unwrap();
        let plane_info = AirplaneInfo::by_type("F-14B(U)").unwrap();
        let mut track = Track::new("pilot", carrier_info, plane_info);
        for (wire, x) in [(1u8, 14.0), (2, 3.0), (3, -10.0), (4, -22.0)] {
            let timestamp_dcs = 10.0 + f64::from(wire) * 0.2;
            track.wire_crossings.push(WireCrossingEvidence {
                wire,
                timestamp_dcs,
                bracket_gap_ms: 50.0,
                method: "finite_hook_plane_crossing",
            });
            track.datums.push(Datum {
                time: timestamp_dcs,
                x,
                ..Datum::default()
            });
        }
        track
    }

    #[test]
    fn stop_position_names_the_wire_the_run_out_points_at() {
        // 15 September 2026, 19:17: no landing mark, stopped at x = -85.2 m. The Tomcat's
        // run-out is 87 m, so the engaged wire sat at about +2 m: the 2-wire (+3 m). The
        // onset-anchored crossing pick would have said 1.
        let track = tomcat_with_four_crossings();
        let estimate = track.wire_estimate_with(11.0, false, Some(-85.2), false);
        assert_eq!(estimate.wire, Some(2));
        assert_eq!(estimate.reason, STOP_POSITION_ESTIMATE_REASON);
        assert_eq!(estimate.confidence, "medium");
        // A 4-wire stops 26 m further down the deck.
        assert_eq!(
            track
                .wire_estimate_with(11.0, false, Some(-111.5), false)
                .wire,
            Some(4)
        );
        // A stop that matches no crossing within half a pendant spacing names nothing from the
        // stop position and falls back to the crossing selection.
        let unmatched = track.wire_estimate_with(11.0, false, Some(-60.0), false);
        assert_ne!(unmatched.reason, STOP_POSITION_ESTIMATE_REASON);
    }

    #[test]
    fn stop_position_is_ignored_for_a_type_without_a_measured_run_out() {
        let carrier_info = CarrierInfo::by_type("CVN_71").unwrap();
        let plane_info = AirplaneInfo::by_type("T-45").unwrap();
        let mut track = Track::new("pilot", carrier_info, plane_info);
        track.wire_crossings.push(WireCrossingEvidence {
            wire: 3,
            timestamp_dcs: 10.0,
            bracket_gap_ms: 50.0,
            method: "finite_hook_plane_crossing",
        });
        track.datums.push(Datum {
            time: 10.0,
            x: -10.0,
            ..Datum::default()
        });
        let estimate = track.wire_estimate_with(10.1, false, Some(-66.0), false);
        assert_ne!(estimate.reason, STOP_POSITION_ESTIMATE_REASON);
    }

    #[test]
    fn hook_up_pass_gets_a_hypothetical_wire_never_a_stop_position_one() {
        // Same crossings, hook up: the stop position is not consulted (nothing was caught) and
        // the crossing-based estimate is labelled as the wire the hook would have caught.
        let track = tomcat_with_four_crossings();
        let estimate = track.wire_estimate_with(10.9, false, Some(-85.2), true);
        assert_eq!(estimate.reason, HYPOTHETICAL_HOOK_UP_REASON);
        assert!(estimate.wire.is_some());
        assert_eq!(estimate.confidence, "medium");
        let outcome = Grading::TouchAndGo {
            cable_estimated: estimate.wire,
        }
        .pilot_facing_outcome(false);
        assert!(
            outcome.starts_with("T&G (CQ) — would have caught wire "),
            "{outcome}"
        );
    }

    #[test]
    fn wire_estimate_prefers_the_crossing_at_deceleration_onset_over_a_later_stretch_crossing() {
        // Regression for the confirmed live bias: cable stretch can carry the hook
        // geometrically past the wire actually caught (wire 3 here) into the next wire's
        // threshold (wire 4) while the aircraft is already decelerating on deck. The old
        // last-crossing-before-event logic would have reported wire 4; the deceleration-onset
        // proxy must instead recognise wire 3 as the earliest crossing at/after the onset.
        let carrier_info = CarrierInfo::by_type("CVN_71").unwrap();
        let plane_info = AirplaneInfo::by_type("FA-18C_hornet").unwrap();
        let carrier = Transform {
            forward: DVec3::unit_z(),
            ..Transform::default()
        };
        let hook_offset = plane_info.hook;
        let midpoint3 = (carrier_info.cable3.0 + carrier_info.cable3.1) / 2.0;
        let midpoint4 = (carrier_info.cable4.0 + carrier_info.cable4.1) / 2.0;
        let mut track = Track::new("pilot", carrier_info, plane_info);

        // Approach speed is steady up to the catch.
        track.observe_horizontal_deceleration(&Transform {
            velocity: DVec3::new(0.0, 0.0, 70.0),
            time: 4.9,
            ..Transform::default()
        });
        track.observe_horizontal_deceleration(&Transform {
            velocity: DVec3::new(0.0, 0.0, 70.0),
            time: 5.0,
            ..Transform::default()
        });

        // Wire 3 crossing at ~5.05, right as the cable catches.
        track.observe_wire_crossings(
            &carrier,
            &Transform {
                position: midpoint3 - hook_offset - DVec3::unit_z(),
                time: 5.0,
                ..Transform::default()
            },
            100.0,
        );
        track.observe_wire_crossings(
            &carrier,
            &Transform {
                position: midpoint3 - hook_offset + DVec3::unit_z(),
                time: 5.1,
                ..Transform::default()
            },
            100.0,
        );

        // Sharp, sustained deceleration confirms the onset at 5.0.
        track.observe_horizontal_deceleration(&Transform {
            velocity: DVec3::new(0.0, 0.0, 20.0),
            time: 5.1,
            ..Transform::default()
        });
        track.observe_horizontal_deceleration(&Transform {
            velocity: DVec3::new(0.0, 0.0, 5.0),
            time: 5.2,
            ..Transform::default()
        });

        // Cable stretch then carries the hook across wire 4's threshold too, at ~5.25, while
        // already decelerating.
        track.observe_wire_crossings(
            &carrier,
            &Transform {
                position: midpoint4 - hook_offset - DVec3::unit_z(),
                time: 5.2,
                ..Transform::default()
            },
            100.0,
        );
        track.observe_wire_crossings(
            &carrier,
            &Transform {
                position: midpoint4 - hook_offset + DVec3::unit_z(),
                time: 5.3,
                ..Transform::default()
            },
            100.0,
        );

        let estimate = track.wire_estimate_at(5.3, true);
        assert_eq!(
            estimate.wire,
            Some(3),
            "must report the wire actually caught, not the one stretch carried the hook past"
        );
        assert_eq!(estimate.arrest_deceleration_onset_time, Some(5.0));
    }

    #[test]
    fn wire_estimate_correlates_via_deceleration_onset_when_the_last_crossing_lags_the_event_too_far(
    ) {
        // Regression reproducing the shape of a confirmed live human trap (6 September 2026,
        // F-14B(U), DCS-confirmed WIRE# 1 both times available): the aircraft coasts at
        // essentially constant speed for close to a second after the hook geometrically crosses
        // wire 1 -- and after it has already swept past all four wire thresholds too -- before a
        // sustained deceleration becomes measurable. The DCS touchdown event correlates tightly
        // with that late deceleration onset (event_lag well under 300 ms), but the *last* raw
        // crossing (wire 4) sits far outside 300 ms of the event -- confirmed live on 6/6 passes
        // in that session, blocking every wire estimate that evening, including both confirmed
        // arrests. Anchoring the correlation check on the onset instead of the last crossing must
        // let this resolve to wire 1, not `None`.
        let carrier_info = CarrierInfo::by_type("CVN_71").unwrap();
        let plane_info = AirplaneInfo::by_type("FA-18C_hornet").unwrap();
        let carrier = Transform {
            forward: DVec3::unit_z(),
            ..Transform::default()
        };
        let hook_offset = plane_info.hook;
        let midpoints = [
            (carrier_info.cable1.0 + carrier_info.cable1.1) / 2.0,
            (carrier_info.cable2.0 + carrier_info.cable2.1) / 2.0,
            (carrier_info.cable3.0 + carrier_info.cable3.1) / 2.0,
            (carrier_info.cable4.0 + carrier_info.cable4.1) / 2.0,
        ];
        let mut track = Track::new("pilot", carrier_info, plane_info);

        // All four wires are swept geometrically within ~0.66 s, well before any deceleration.
        for (i, midpoint) in midpoints.iter().enumerate() {
            let base = i as f64 * 0.22;
            track.observe_wire_crossings(
                &carrier,
                &Transform {
                    position: *midpoint - hook_offset - DVec3::unit_z(),
                    time: base,
                    ..Transform::default()
                },
                100.0,
            );
            track.observe_wire_crossings(
                &carrier,
                &Transform {
                    position: *midpoint - hook_offset + DVec3::unit_z(),
                    time: base + 0.05,
                    ..Transform::default()
                },
                100.0,
            );
        }

        // Steady coast (real cable load has not built up yet) until just before 0.9 s.
        for t in [0.0_f64, 0.2, 0.4, 0.6, 0.8, 0.85] {
            track.observe_horizontal_deceleration(&Transform {
                velocity: DVec3::new(0.0, 0.0, 65.0),
                time: t,
                ..Transform::default()
            });
        }
        // Sustained deceleration becomes measurable close to 0.9 s.
        track.observe_horizontal_deceleration(&Transform {
            velocity: DVec3::new(0.0, 0.0, 20.0),
            time: 0.95,
            ..Transform::default()
        });
        track.observe_horizontal_deceleration(&Transform {
            velocity: DVec3::new(0.0, 0.0, 5.0),
            time: 1.0,
            ..Transform::default()
        });

        // The DCS touchdown event correlates tightly with the onset, not with the long-past last
        // crossing (wire 4 at ~0.71 s, over 400 ms away).
        let estimate = track.wire_estimate_at(1.05, true);
        assert_eq!(
            estimate.wire,
            Some(1),
            "must resolve via the deceleration onset instead of bailing out on the stale last crossing"
        );
        assert_eq!(estimate.arrest_deceleration_onset_time, Some(0.85));
    }

    #[test]
    fn wire_estimate_ignores_a_crossing_recorded_while_still_airborne_before_any_deceleration() {
        // Same fixture-derived shape as `wire_4_01_FA18C`: an early wire is crossed
        // geometrically while the aircraft is still airborne on short final (no deceleration
        // yet at all), and only a later wire is crossed at the real, decelerating catch.
        let carrier_info = CarrierInfo::by_type("CVN_71").unwrap();
        let plane_info = AirplaneInfo::by_type("FA-18C_hornet").unwrap();
        let carrier = Transform {
            forward: DVec3::unit_z(),
            ..Transform::default()
        };
        let hook_offset = plane_info.hook;
        let midpoint3 = (carrier_info.cable3.0 + carrier_info.cable3.1) / 2.0;
        let midpoint4 = (carrier_info.cable4.0 + carrier_info.cable4.1) / 2.0;
        let mut track = Track::new("pilot", carrier_info, plane_info);

        // Wire 3's threshold is swept over while still airborne, well before any deceleration.
        track.observe_wire_crossings(
            &carrier,
            &Transform {
                position: midpoint3 - hook_offset - DVec3::unit_z(),
                time: 1.0,
                ..Transform::default()
            },
            100.0,
        );
        track.observe_wire_crossings(
            &carrier,
            &Transform {
                position: midpoint3 - hook_offset + DVec3::unit_z(),
                time: 1.1,
                ..Transform::default()
            },
            100.0,
        );

        // Steady approach speed until the real catch, much later.
        track.observe_horizontal_deceleration(&Transform {
            velocity: DVec3::new(0.0, 0.0, 70.0),
            time: 9.9,
            ..Transform::default()
        });
        track.observe_horizontal_deceleration(&Transform {
            velocity: DVec3::new(0.0, 0.0, 70.0),
            time: 10.0,
            ..Transform::default()
        });

        // Wire 4 is the real catch, at ~10.05.
        track.observe_wire_crossings(
            &carrier,
            &Transform {
                position: midpoint4 - hook_offset - DVec3::unit_z(),
                time: 10.0,
                ..Transform::default()
            },
            100.0,
        );
        track.observe_wire_crossings(
            &carrier,
            &Transform {
                position: midpoint4 - hook_offset + DVec3::unit_z(),
                time: 10.1,
                ..Transform::default()
            },
            100.0,
        );

        track.observe_horizontal_deceleration(&Transform {
            velocity: DVec3::new(0.0, 0.0, 20.0),
            time: 10.1,
            ..Transform::default()
        });
        track.observe_horizontal_deceleration(&Transform {
            velocity: DVec3::new(0.0, 0.0, 5.0),
            time: 10.2,
            ..Transform::default()
        });

        let estimate = track.wire_estimate_at(10.2, true);
        assert_eq!(
            estimate.wire,
            Some(4),
            "must not report the wire swept over while still airborne"
        );
    }

    #[test]
    fn wire_estimated_matches_wire_estimation_even_when_the_event_path_races_ahead_of_the_crossing()
    {
        // Regression for the confirmed live desync: the event-correlated `Land` (here
        // simulated by setting `grading`/`landing_time` directly, as `landed()` would) can be
        // processed before the position-tick path (`observe_wire_crossings`) has recorded the
        // corresponding wire crossing. `cable_estimated` must never freeze that incomplete
        // snapshot -- it has to agree with `wire_estimation.wire`, which `finish()` computes
        // once against the complete crossing history.
        let carrier_info = CarrierInfo::by_type("CVN_71").unwrap();
        let plane_info = AirplaneInfo::by_type("FA-18C_hornet").unwrap();
        let carrier = Transform {
            forward: DVec3::unit_z(),
            ..Transform::default()
        };
        let midpoint = (carrier_info.cable3.0 + carrier_info.cable3.1) / 2.0;
        let hook_offset = plane_info.hook;
        let mut track = Track::new("pilot", carrier_info, plane_info);

        // Position path observes the "before" sample (still short of the wire).
        track.observe_wire_crossings(
            &carrier,
            &Transform {
                position: midpoint - hook_offset - DVec3::unit_z(),
                time: 1.0,
                ..Transform::default()
            },
            100.0,
        );

        // Event path correlates `Land` right here, with `wire_crossings` still empty --
        // exactly the race from the live report: no fresh crossing yet, same as `landed()`
        // would leave it with the eager-computation removed.
        track.grading = Some(Grading::Recovered {
            cable: None,
            cable_estimated: None,
        });
        track.landing_time = Some(1.05);

        // Position path only now observes the "after" sample that completes the crossing.
        track.observe_wire_crossings(
            &carrier,
            &Transform {
                position: midpoint - hook_offset + DVec3::unit_z(),
                time: 1.1,
                ..Transform::default()
            },
            100.0,
        );

        let result = track.finish();
        assert_eq!(result.wire_estimation.wire, Some(3));
        match result.grading {
            Grading::Recovered {
                cable_estimated, ..
            } => {
                assert_eq!(
                    cable_estimated, result.wire_estimation.wire,
                    "cable_estimated must match wire_estimation.wire, not an earlier snapshot"
                );
            }
            other => panic!("expected Grading::Recovered, got {other:?}"),
        }
    }

    #[test]
    fn wire_estimate_never_reports_high_confidence_without_a_dcs_confirmed_arrest() {
        // A tightly bracketed, well-correlated crossing is still not proof the aircraft actually
        // stopped -- only a DCS-confirmed wire (LQM `WIRE#`) is. Without it, confidence must never
        // exceed "medium", even when every timing bracket is tight.
        let carrier_info = CarrierInfo::by_type("CVN_71").unwrap();
        let plane_info = AirplaneInfo::by_type("FA-18C_hornet").unwrap();
        let carrier = Transform {
            forward: DVec3::unit_z(),
            ..Transform::default()
        };
        let hook_offset = plane_info.hook;
        let midpoint3 = (carrier_info.cable3.0 + carrier_info.cable3.1) / 2.0;
        let mut track = Track::new("pilot", carrier_info, plane_info);
        track.observe_wire_crossings(
            &carrier,
            &Transform {
                position: midpoint3 - hook_offset - DVec3::unit_z(),
                time: 1.0,
                ..Transform::default()
            },
            100.0,
        );
        track.observe_wire_crossings(
            &carrier,
            &Transform {
                position: midpoint3 - hook_offset + DVec3::unit_z(),
                time: 1.05,
                ..Transform::default()
            },
            100.0,
        );
        track.first_hook_ground_contact_time = Some(1.05);
        track.landing_time = Some(1.05);
        track.grading = Some(Grading::Recovered {
            cable: None,
            cable_estimated: None,
        });
        // No `set_dcs_grading` call at all: no LQM confirmation of an arrest.

        let result = track.finish();
        assert_eq!(result.wire_estimation.wire, Some(3));
        assert_eq!(
            result.wire_estimation.confidence, "medium",
            "confidence must not be \"high\" without a DCS-confirmed arrest"
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

        // The cadence-ab diagnostic command (B.2) rebuilds gate/trajectory geometry from a
        // persisted run's `datums` using `replay_gate_and_trajectory` instead of a live `Track`.
        // It must reproduce the exact same gate_deviations/trajectory_deviations Track::next
        // itself produced for those datums, or a "no cadence change" run would show a false
        // diff before any artificial subsampling is even applied.
        let (replayed_gates, replayed_trajectory) = replay_gate_and_trajectory(
            live.datums.iter().map(|d| ReplaySample {
                time: d.time,
                x: d.x,
                y: d.y,
                alt: d.alt,
                valid: d.telemetry_valid,
                skew_ms: d.skew_ms,
                roll_deg: d.roll_deg,
            }),
            0.0,
            plane_info.glide_slope,
            carrier_info.is_vstol(),
        );
        assert_eq!(replayed_gates, live.gate_deviations);
        assert_eq!(replayed_trajectory, live.trajectory_deviations);
    }

    #[test]
    fn high_altitude_flyover_is_a_waveoff_not_a_bolter() {
        // Regression for a confirmed live bug: a go-around that overflies the deck without
        // touching it (crossing x=0 at ~140 m / ~460 ft, well above any plausible deck contact)
        // was misclassified as `Bolter` because the threshold crossing ignored altitude.
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
        let hook_offset_y = plane_info.hook.y;
        let mut track = Track::new("pilot", carrier_info, plane_info);

        // Descending inbound warm-up keeps carrier and plane time aligned. Groove entry is
        // latched explicitly below so this test isolates flyover classification.
        for (index, distance) in [1400.0, 1300.0, 1100.0, 900.0, 700.0, 300.0, 100.0]
            .into_iter()
            .enumerate()
        {
            let time = [0.0, 0.4, 0.65, 0.9, 1.15, 1.25, 1.35][index];
            let mut carrier_frame = carrier.clone();
            carrier_frame.time = time;
            let altitude = distance * plane_info.glide_slope.to_radians().tan()
                + carrier_info.deck_altitude
                - hook_offset_y;
            let plane = Transform {
                time,
                position: landing - fb * distance,
                alt: altitude,
                ..Transform::default()
            };
            assert!(track.next(&carrier_frame, &plane, None));
        }
        // Outcome classification is the subject of this regression; Case I roll-out timing is
        // covered independently.
        track.entered_groove = true;
        track.mark_fresh_groove_entry(1.35);
        assert!(
            track.entered_groove,
            "warm-up should have confirmed groove entry before the deck crossing"
        );

        // Crosses x=0 (deck threshold) at ~140 m relative altitude: a high fly-over, not a touch.
        let crossing_alt = carrier_info.deck_altitude - hook_offset_y + 140.0;
        let mut carrier_frame = carrier.clone();
        carrier_frame.time = 1.6;
        let crossing = Transform {
            time: 1.6,
            position: landing - fb * -5.0,
            alt: crossing_alt,
            ..Transform::default()
        };
        assert!(track.next(&carrier_frame, &crossing, None));

        // Continues away, past the >150 m distance-growth threshold that finalizes an outcome.
        carrier_frame.time = 1.8;
        let departure = Transform {
            time: 1.8,
            position: landing - fb * -200.0,
            alt: crossing_alt,
            ..Transform::default()
        };
        assert!(!track.next(&carrier_frame, &departure, None));
        assert_eq!(track.finish().grading, Grading::WaveoffUnknown);
    }

    #[test]
    fn low_altitude_deck_crossing_is_still_a_bolter() {
        // Companion to `high_altitude_flyover_is_a_waveoff_not_a_bolter`: a real deck crossing
        // near hook height must still be graded `Bolter`, not swept up by the altitude guard.
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
        let hook_offset_y = plane_info.hook.y;
        let mut track = Track::new("pilot", carrier_info, plane_info);

        // Descending inbound warm-up keeps carrier and plane time aligned. Groove entry is
        // latched explicitly below so this test isolates deck-contact classification.
        for (index, distance) in [1400.0, 1300.0, 1100.0, 900.0, 700.0, 300.0, 100.0]
            .into_iter()
            .enumerate()
        {
            let time = [0.0, 0.4, 0.65, 0.9, 1.15, 1.25, 1.35][index];
            let mut carrier_frame = carrier.clone();
            carrier_frame.time = time;
            let altitude = distance * plane_info.glide_slope.to_radians().tan()
                + carrier_info.deck_altitude
                - hook_offset_y;
            let plane = Transform {
                time,
                position: landing - fb * distance,
                alt: altitude,
                ..Transform::default()
            };
            assert!(track.next(&carrier_frame, &plane, None));
        }
        track.entered_groove = true;
        track.mark_fresh_groove_entry(1.35);
        assert!(
            track.entered_groove,
            "warm-up should have confirmed groove entry before the deck crossing"
        );

        // Crosses x=0 essentially at deck level (relative alt ~= 0).
        let crossing_alt = carrier_info.deck_altitude - hook_offset_y;
        let mut carrier_frame = carrier.clone();
        carrier_frame.time = 1.6;
        let crossing = Transform {
            time: 1.6,
            position: landing - fb * -5.0,
            alt: crossing_alt,
            ..Transform::default()
        };
        assert!(track.next(&carrier_frame, &crossing, None));

        carrier_frame.time = 1.8;
        let departure = Transform {
            time: 1.8,
            position: landing - fb * -200.0,
            alt: crossing_alt,
            ..Transform::default()
        };
        assert!(!track.next(&carrier_frame, &departure, None));
        assert_eq!(track.finish().grading, Grading::Bolter);
    }

    #[test]
    fn low_flyover_without_confirmed_contact_is_a_waveoff_not_a_bolter() {
        // Regression for a confirmed live bug (5 September 2026, human test): a survol at ~8.7 m
        // (comfortably inside the 50 ft/`DECK_CROSSING_ALT_CAP_FT` go-around guard, but far above
        // `DECK_CONTACT_CONFIRMATION_ALT_M`) never touched the deck and had no correlated DCS
        // event, yet was still classified `Bolter` by the altitude-cap check alone.
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
        let hook_offset_y = plane_info.hook.y;
        let mut track = Track::new("pilot", carrier_info, plane_info);

        for (index, distance) in [1400.0, 1300.0, 1100.0, 900.0, 700.0, 300.0, 100.0]
            .into_iter()
            .enumerate()
        {
            let time = [0.0, 0.4, 0.65, 0.9, 1.15, 1.25, 1.35][index];
            let mut carrier_frame = carrier.clone();
            carrier_frame.time = time;
            let altitude = distance * plane_info.glide_slope.to_radians().tan()
                + carrier_info.deck_altitude
                - hook_offset_y;
            let plane = Transform {
                time,
                position: landing - fb * distance,
                alt: altitude,
                ..Transform::default()
            };
            assert!(track.next(&carrier_frame, &plane, None));
        }
        track.entered_groove = true;
        track.mark_fresh_groove_entry(1.35);
        assert!(
            track.entered_groove,
            "warm-up should have confirmed groove entry before the deck crossing"
        );

        // Crosses x=0 at 8.7 m relative altitude: below the 50 ft cap, but well above the 1 m
        // confirmed-contact threshold -- no actual touch happened.
        let crossing_alt = carrier_info.deck_altitude - hook_offset_y + 8.7;
        let mut carrier_frame = carrier.clone();
        carrier_frame.time = 1.6;
        let crossing = Transform {
            time: 1.6,
            position: landing - fb * -5.0,
            alt: crossing_alt,
            ..Transform::default()
        };
        assert!(track.next(&carrier_frame, &crossing, None));

        carrier_frame.time = 1.8;
        let departure = Transform {
            time: 1.8,
            position: landing - fb * -200.0,
            alt: crossing_alt,
            ..Transform::default()
        };
        assert!(!track.next(&carrier_frame, &departure, None));
        assert_eq!(track.finish().grading, Grading::WaveoffUnknown);
    }

    #[test]
    fn dcs_waveoff_grade_overrides_a_geometry_only_bolter() {
        // Regression for a confirmed live bug (5 September 2026, human test): the DCS LQM itself
        // opened with `GRADE:WO ... WO(AFU)IC` (the LSO ordered a waveoff) on a pass our own
        // geometry alone had called `Bolter`, with no `runway_touch`/`land` event at all. Refusing
        // a bolter DCS's own grading directly contradicts is not inventing a waveoff author.
        let carrier = CarrierInfo::by_type("CVN_71").unwrap();
        let hornet = AirplaneInfo::by_type("FA-18C_hornet").unwrap();
        let mut track = Track::new("pilot", carrier, hornet);
        track.grading = Some(Grading::Bolter);
        track
            .set_dcs_grading("LSO: GRADE:WO : SLOX DRX (LURIM) DRIM (LURIC) WO(AFU)IC".to_string());

        assert_eq!(track.finish().grading, Grading::WaveoffUnknown);
    }

    #[test]
    fn dcs_waveoff_grade_closes_the_track_after_departure() {
        // Regression for the 7 September 2026 human F-14B(U) session: a GRADE:WO was retained as
        // text but did not establish an outcome, so the recorder followed two more circuits and
        // consumed their LQMs as duplicates. Once DCS has identified this attempt as a waveoff,
        // the ordinary >150 m departure guard must close it.
        let carrier_info = CarrierInfo::by_type("CVN_71").unwrap();
        let plane_info = AirplaneInfo::by_type("FA-18C_hornet").unwrap();
        // The carrier clock must follow the aircraft clock (see
        // `simulated_vstol_touch_and_go_is_neutral_not_a_bolter`).
        let carrier_at = |time: f64| Transform {
            time,
            forward: DVec3::unit_z(),
            ..Transform::default()
        };
        let landing = carrier_info.approach_reference_offset(plane_info);
        let fb = DVec3::unit_z().rotated_by(DRotor3::from_rotation_xz(
            carrier_info.deck_angle.to_radians(),
        ));
        let mut track = Track::new("pilot", carrier_info, plane_info);

        let inbound = Transform {
            time: 1.0,
            position: landing - fb * 100.0,
            alt: 50.0,
            ..Transform::default()
        };
        assert!(track.next(&carrier_at(1.0), &inbound, None));
        assert!(track.set_dcs_grading("LSO: GRADE:WO _LULIM_ _LULIC_ WO(AFU)IC [BC]".to_string()));

        let departure = Transform {
            time: 2.0,
            position: landing - fb * -300.0,
            alt: 50.0,
            ..Transform::default()
        };
        assert!(!track.next(&carrier_at(2.0), &departure, None));
        assert_eq!(track.finish().grading, Grading::WaveoffUnknown);
    }

    #[test]
    fn dcs_ok_grade_never_overrides_a_confirmed_bolter() {
        // Companion to the above: an unrelated `GRADE:` prefix (not a waveoff) must never touch
        // an already-confirmed `Bolter`.
        let carrier = CarrierInfo::by_type("CVN_71").unwrap();
        let hornet = AirplaneInfo::by_type("FA-18C_hornet").unwrap();
        let mut track = Track::new("pilot", carrier, hornet);
        track.grading = Some(Grading::Bolter);
        // Deliberately no `WIRE#` token: that would independently promote the grading to
        // `Recovered` regardless of this fix (a DCS-reported wire is itself stronger evidence of
        // an arrest), which is not what this test is checking.
        track.set_dcs_grading("LSO: GRADE:--- : _H_IC".to_string());

        assert_eq!(track.finish().grading, Grading::Bolter);
    }

    #[test]
    fn hook_reading_is_frozen_at_first_geometric_contact_not_at_the_late_dcs_event() {
        // Regression for a confirmed live bug (5 September 2026, human test): the DCS touchdown
        // event lags physical contact by ~0.2-1 s. A raw sample taken in that window, while the
        // crosse is pressed against the deck, reads `0.0` ("up") regardless of true hook
        // position. `first_hook_ground_contact_time` (set from geometry, not the event) must
        // freeze interpretation before that contaminated window, exactly like `landing_time` did.
        let carrier = CarrierInfo::by_type("CVN_71").unwrap();
        let hornet = AirplaneInfo::by_type("FA-18C_hornet").unwrap();
        let mut track = Track::new("pilot", carrier, hornet);
        track.entered_groove = true;
        track.previous_x = 400.0;
        for index in 0..3 {
            track.observe_hook_sample(
                20.0 + index as f64 * 0.25,
                index,
                0.0,
                Some(1.0),
                HookSampleStatus::Success,
            );
        }
        assert_eq!(track.calibrated_hook_state(), HookState::Down);

        // Geometric contact fires here -- before any DCS event ever correlates a touchdown.
        track.first_hook_ground_contact_time = Some(20.8);

        // Contaminated post-contact samples: crosse crushed against the deck reads as 0.0.
        for index in 0..3 {
            track.observe_hook_sample(
                21.0 + index as f64 * 0.25,
                10 + index,
                0.0,
                Some(0.0),
                HookSampleStatus::Success,
            );
        }
        assert_eq!(
            track.calibrated_hook_state(),
            HookState::Down,
            "post-contact samples must not rewrite the pre-contact evidence"
        );
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
        assert_eq!(touch_and_go.calibrated_hook_state(), HookState::Up);
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
            HookState::Up,
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
        assert_eq!(arrested.calibrated_hook_state(), HookState::Down);
    }

    #[test]
    fn f14_hook_is_calibrated_via_its_own_draw_argument() {
        // Regression for the user-supplied calibration: the F-14 (all variants) uses draw
        // argument 1305, distinct from the F/A-18C and T-45's 25
        // (`F14_HOOK_DRAW_ARGUMENT`, `src/data.rs`) -- but once a raw value is captured, it is
        // interpreted with the same up/down thresholds, since no different polarity was
        // supplied. `calibrated_hook_state` itself never touches which index was polled (that
        // happens upstream, in `src/tasks/record_recovery.rs`); this only confirms the F-14 is
        // no longer forced to `Unknown` the way it was before this calibration was supplied.
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
        assert_eq!(track.calibrated_hook_state(), HookState::Up);
    }

    #[test]
    fn type_without_a_known_hook_draw_argument_remains_unknown() {
        // A type with no `hook_draw_argument` at all (e.g. AV-8B, or any future type added
        // before its index is known) must never have a raw value interpreted, no matter how
        // clean the readings look -- `calibrated_hook_state` gates on the index being known,
        // not on the readings themselves.
        static UNCALIBRATED: AirplaneInfo = AirplaneInfo {
            name: "Future Type",
            arresting_run_out_m: None,
            hook: DVec3 {
                x: 0.0,
                y: 0.0,
                z: 0.0,
            },
            landing_reference: DVec3 {
                x: 0.0,
                y: 0.0,
                z: 0.0,
            },
            glide_slope: 3.5,
            hook_draw_argument: None,
            aoa_grading_calibrated: false,
            aoa_rating: |_| crate::data::Aoa::OnSpeed,
        };
        let carrier = CarrierInfo::by_type("CVN_71").unwrap();
        let mut track = Track::new("pilot", carrier, &UNCALIBRATED);
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
        assert_eq!(track.calibrated_hook_state(), HookState::Unknown);
    }
}
