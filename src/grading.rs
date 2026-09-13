use crate::data::{AirplaneInfo, Aoa};
use crate::track::{Datum, GateDeviations, Grading, TrajectoryDeviation};

// ---------------------------------------------------------------------------
// SOURCE: PROJECT-DERIVED grading contract v1.
// Thresholds retained from the historical module pending live validation;
// they are not represented as NAVAIR-prescribed numerical boundaries.
// ---------------------------------------------------------------------------

/// Glideslope deviation thresholds (degrees).
/// PROJECT-DERIVED: historical module/MOOSE-inspired values, not a NAVAIR table.
const GS_SLIGHT_HIGH: f64 = 0.5;
const GS_SLIGHT_LOW: f64 = 0.5;
const GS_SIGNIFICANT: f64 = 1.0;
/// Dangerously low at the 1/4-nm gate — triggers a Cut pass.
const GS_CUT_LOW_DEG: f64 = -2.5;

/// Lineup deviation thresholds (degrees, absolute value).
/// PROJECT-DERIVED: historical module/MOOSE-inspired values, not a NAVAIR table.
const LU_SLIGHT: f64 = 1.0;
const LU_MEDIUM: f64 = 2.0;
// const LU_SIGNIFICANT: f64 = 3.0;  // LUL / LUR     — "lined up left/right" (large) — NoGrade already triggered at LU_MEDIUM

/// How far back from the last recorded trajectory sample the trend check looks.
/// PROJECT-DERIVED: a correction takes roughly 1-2 s to fly (per the project's own analysis of
/// realistic approach dynamics), so 4 s gives enough samples to see a real trend rather than
/// single-sample noise, without reaching back into an earlier, unrelated part of the approach.
const TREND_WINDOW_S: f64 = 4.0;
/// PROJECT-DERIVED. The trend check only ever runs once amplitude alone already places the
/// pass at Ok (every sample under `GS_SLIGHT_HIGH`/`GS_SLIGHT_LOW`/`LU_SLIGHT`), so the largest
/// physically reachable slope within the window is bounded by that ceiling divided by
/// `TREND_WINDOW_S` (about 0.125 deg/s here). This threshold sits well inside that reachable
/// range — a sustained drift of roughly a third of the OK margin over the window — while staying
/// clearly above ordinary aim-point noise. Not a NAVAIR value: no published doctrine quantifies
/// "worsening" numerically.
const TREND_WORSENING_DEG_PER_S: f64 = 0.075;

/// How close to touchdown (metres) a deviation counts as "the last moments before the ramp",
/// where the same magnitude of error carries more risk because there is no distance left to
/// correct it. PROJECT-DERIVED: deliberately narrower than the 463 m quarter-NM Cut gate, so
/// this only ever tightens the very end of the approach the Cut rule already treats specially,
/// never the whole final segment. At a typical ~70-75 m/s approach speed this is roughly the
/// last 2 seconds.
const LATE_WINDOW_DISTANCE_M: f64 = 150.0;
/// PROJECT-DERIVED thresholds for the late-window check, deliberately set between
/// `*_SLIGHT`/`*_SIGNIFICANT`: a deviation in this range would only ever earn `(OK)` if it
/// happened earlier in the approach, but this close to the ramp there is no time left to
/// correct it, so it is treated as if it had crossed the stricter NoGrade threshold. Not NAVAIR
/// values — no published doctrine quantifies this numerically.
const LATE_WINDOW_GS_DEG: f64 = 0.8;
const LATE_WINDOW_LU_DEG: f64 = 1.5;

/// A.1 robustness guard: how many consecutive continuous-trajectory samples must cross
/// `GS_SLIGHT_*`/`LU_SLIGHT` (in the same direction) before that excursion counts toward the
/// worst-deviation search below. PROJECT-DERIVED, deliberately the smallest value that rules out
/// a single aberrant telemetry frame: 1 would accept a lone spike, 2 requires the excursion to
/// still be present on the very next sample (well under a second at scoring cadence), so a real,
/// if brief, correction-worthy excursion is still caught immediately. Never applied to the
/// `GS_CUT_LOW_DEG` safety check or the late-window check — both stay maximally sensitive to a
/// single dangerous sample, on purpose.
const PERSISTENCE_MIN_CONSECUTIVE_SAMPLES: usize = 2;

/// A.4 (NATOPS `OC` — overcontrolled): how many trailing seconds of the continuous trajectory to
/// scan for alternating corrections. Reuses `TREND_WINDOW_S`'s own rationale (a correction takes
/// roughly 1-2 s to fly) rather than introducing an unrelated window length.
const OSCILLATION_WINDOW_S: f64 = TREND_WINDOW_S;
/// PROJECT-DERIVED. A direction reversal only counts once consecutive samples swing by at least
/// this much — comfortably above ordinary aim-point/telemetry noise, well under `GS_SLIGHT_HIGH`/
/// `GS_SLIGHT_LOW`/`LU_SLIGHT` so a real correction reversal is still caught before amplitude
/// alone would already have downgraded the pass.
const OSCILLATION_MIN_SWING_DEG: f64 = 0.3;
/// PROJECT-DERIVED. Two reversals (three legs: out, back, out again) is the minimum shape that
/// distinguishes a genuine oscillation from a single correction overshoot — one reversal is just
/// "corrected, then held", which `trend_worsening` and the amplitude tiers already grade fairly.
const OSCILLATION_MIN_REVERSALS: usize = 2;

/// Danger-cut sink-rate threshold (m/s, positive = descending). PROJECT-DERIVED, and unlike
/// `GS_CUT_LOW_DEG` this has **no NATOPS-numeric backing at all**: NAVAIR 00-80T-104 §6.6.4 and
/// the `TMRD` ("Too Much Rate of Descent") comment code in NAVAIR 00-80T-105 both leave excessive
/// sink rate to the controlling LSO's judgment ("aircraft/engine performance, approach dynamics,
/// and environmental conditions"), never a number. Publicly documented CATOBAR approach/touchdown
/// sink rate is roughly 600-800 ft/min (~3.0-4.1 m/s) by design (no flare); this threshold sits at
/// roughly double that nominal corridor so a normal, intentional no-flare approach or touchdown
/// never trips it, while a real dive or late correction well outside that corridor does.
const SINK_RATE_CUT_MPS: f64 = 8.0;
/// Danger-cut bank-angle threshold (degrees, absolute value). PROJECT-DERIVED, same rationale as
/// `SINK_RATE_CUT_MPS`: NATOPS has no numeric criterion for "excessive" bank either (the "Level
/// your wings" imperative call and the `W`/`TMA`/`DLW`/`DRW` comment codes are qualitative only).
/// Set at double `GROOVE_ROLLOUT_MAX_BANK_DEG` (`src/track.rs`, 15 deg) — well beyond any ordinary
/// groove correction, in the range where a real risk of a wingtip/deck-edge strike or a genuine
/// loss of control margin this close to the ship becomes plausible.
const BANK_ANGLE_CUT_DEG: f64 = 30.0;
/// Reinforced persistence guard for the two danger-cut checks above, deliberately stricter than
/// `PERSISTENCE_MIN_CONSECUTIVE_SAMPLES` (2): a Cut is the harshest verdict available (0 points),
/// so a single hard telemetry bump or one noisy frame must never trigger it on its own. Three
/// consecutive samples is still well under a second at scoring cadence.
const DANGER_CUT_MIN_CONSECUTIVE_SAMPLES: usize = 3;

/// CASE I CATOBAR episode model. Every threshold, zone boundary, weight and correction rule in
/// this block is PROJECT-DERIVED and requires comparison with human-LSO assessments before it can
/// be treated as operationally calibrated.
const EPISODE_STABLE_SAMPLES: usize = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GradingAxis {
    Glideslope,
    Lineup,
    Aoa,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EpisodeSeverity {
    None,
    Small,
    Medium,
    Large,
}

impl EpisodeSeverity {
    const fn level(self) -> u8 {
        match self {
            Self::None => 0,
            Self::Small => 1,
            Self::Medium => 2,
            Self::Large => 3,
        }
    }

    const fn from_level(level: u8) -> Self {
        match level {
            0 => Self::None,
            1 => Self::Small,
            2 => Self::Medium,
            _ => Self::Large,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ApproachZone {
    Start,
    Middle,
    InClose,
    Ramp,
}

impl ApproachZone {
    const fn weight(self) -> f64 {
        match self {
            Self::Start => 1.0,
            Self::Middle => 1.2,
            Self::InClose => 1.5,
            Self::Ramp => 2.0,
        }
    }

    const fn label(self) -> &'static str {
        match self {
            Self::Start => "START",
            Self::Middle => "MIDDLE",
            Self::InClose => "IN CLOSE",
            Self::Ramp => "RAMP",
        }
    }

    const fn good_correction_deadline_s(self) -> f64 {
        match self {
            Self::Start => 3.0,
            Self::Middle => 2.5,
            Self::InClose => 1.5,
            Self::Ramp => 0.75,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EpisodeEvolution {
    TowardTarget,
    AwayFromTarget,
    Stagnant,
    Mixed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CorrectionQuality {
    Good,
    Average,
    Poor,
    NotAssessed,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct GradingEpisode {
    pub axis: GradingAxis,
    pub started_at_dcs: f64,
    pub ended_at_dcs: f64,
    pub duration_s: f64,
    pub most_severe_zone: ApproachZone,
    pub zone_weight: f64,
    pub maximum_severity: EpisodeSeverity,
    pub corrected_severity: EpisodeSeverity,
    pub effective_severity: f64,
    pub peak_value: f64,
    pub peak_normalized_error: f64,
    pub peak_at_dcs: f64,
    pub peak_zone: ApproachZone,
    pub peak_classification: &'static str,
    pub evolution: EpisodeEvolution,
    pub returned_to_less_severe_band: bool,
    pub oscillation_reversals: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub first_durable_improvement_delay_s: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub return_to_none_delay_s: Option<f64>,
    pub stabilized_severity: EpisodeSeverity,
    pub stabilization_samples: usize,
    pub post_correction_aggravation: bool,
    pub correction_reason: &'static str,
    pub correction: CorrectionQuality,
    pub affects_grade: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub diagnostic: Option<&'static str>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CatobarAssessment {
    pub grade: PassGrade,
    pub reason: String,
    pub episodes: Vec<GradingEpisode>,
}

pub struct CatobarEvidence<'a> {
    pub grading: &'a Grading,
    pub gates: &'a GateDeviations,
    pub trajectory: &'a [TrajectoryDeviation],
    pub datums: &'a [Datum],
    pub plane_info: &'a AirplaneInfo,
    pub aoa_reliable: bool,
    pub groove_time_secs: Option<f64>,
    pub groove_entry_time: Option<f64>,
}

/// `_OK_` ("Okay underline", NAVAIR 00-80T-104 §11.4.1, `OFFICIAL` symbol meaning "Perfect
/// pass") amplitude tolerance, degrees. The *symbol* and its meaning are official; these
/// specific numbers are not — NATOPS documents no numerical criterion for awarding it. Borrowed
/// instead from a real, currently-maintained open-source LSO grading implementation, MOOSE
/// `Ops.Airboss` (`Airboss.lua`, `AIRBOSS.GLE`/`AIRBOSS.LUE` `_max`/`_min` fields) — the same
/// historical/MOOSE-inspired lineage already behind `GS_SLIGHT_*`/`LU_SLIGHT` above, tightened
/// to the band Airboss itself reserves for a zero-deviation "Unicorn" pass. Deliberately
/// asymmetric on GS, matching the source: very slightly more tolerance for a hair high than for
/// a hair low, which stays the more dangerous side close to the ramp.
const OK_PERFECT_GS_HIGH_DEG: f64 = 0.4;
const OK_PERFECT_GS_LOW_DEG: f64 = 0.3;
const OK_PERFECT_LU_ABS_DEG: f64 = 0.5;
/// `_OK_` groove-time window, seconds. Unlike the amplitude band above, this one **is**
/// `OFFICIAL`: NAVAIR 00-80T-105 §6.2.4.3 states a standard Case I groove should run "15 - 18
/// seconds" from wings-level/centered-ball to touchdown. Applied identically to every CATOBAR
/// type (F-14, F/A-18, T-45) despite the T-45 flying a different glide slope (3.0 deg vs 3.5
/// deg for the others) that likely implies a different real approach speed and therefore a
/// different natural groove length — the module has no per-type approach-speed reference to
/// adjust for this, so the raw NATOPS window is used unadjusted for every type. A known,
/// documented limitation (see `tasking-roadmap.md`), not an oversight. Deliberately **not**
/// coupled to which wire is caught: the historical MOOSE "wire 3 + 15-18.99 s" combination this
/// module's own values descend from was already reviewed and left disabled (see
/// `docs/GRADING_REFERENCE.md`) precisely because no NATOPS text ties a specific wire to a
/// grade.
const OK_PERFECT_GROOVE_TIME_MIN_S: f64 = 15.0;
const OK_PERFECT_GROOVE_TIME_MAX_S: f64 = 18.0;

// ---------------------------------------------------------------------------
// PassGrade — project score using selected official display symbols
// ---------------------------------------------------------------------------

/// Project pass grade. See `docs/GRADING_REFERENCE.md` for rule provenance.
///
/// Display labels follow the documented LSO symbols; automatic classification and score
/// computation remain PROJECT-DERIVED:
///
/// | Label   | Points | Meaning |
/// |---------|--------|---------|
/// | `_OK_`  | 5.0    | Reserved for an explicit official/manual perfect grade |
/// | `OK`    | 4.0    | Okay pass — no significant deviations |
/// | `(OK)`  | 3.0    | Fair pass — slight deviations only |
/// | `--`    | 2.0    | No grade — significant deviations |
/// | `C`     | 0.0    | Cut pass — dangerously low at the ramp |
/// | `B`     | 2.5    | Bolter |
/// | `WO`    | 1.0    | Waveoff |
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum PassGrade {
    /// Perfect pass (`OFFICIAL` `_OK_` symbol, NAVAIR 00-80T-104 §11.4.1). Emitted by
    /// `grade_from_gates` when amplitude is within `OK_PERFECT_*` and groove time falls in
    /// `OK_PERFECT_GROOVE_TIME_MIN_S..=OK_PERFECT_GROOVE_TIME_MAX_S` — see its doc comment.
    Perfect,
    Ok,
    OkParentheses,
    /// No grade — significant deviations (NAVAIR label `--`).
    NoGrade,
    /// Cut pass — dangerously low at the ramp, or landed after being waved off.
    Cut,
    Bolter,
    WaveoffUnknown,
    /// Project use of `NC` for insufficient telemetry; never carries points.
    Incomplete,
}

impl PassGrade {
    /// Short display label used in charts and the greenie board.
    /// These are the documented LSO display symbols plus project `NC`/`WO?` states.
    pub fn label(self) -> &'static str {
        match self {
            Self::Perfect => "_OK_",
            Self::Ok => "OK",
            Self::OkParentheses => "(OK)",
            Self::NoGrade => "--",
            Self::Cut => "C",
            Self::Bolter => "B",
            Self::WaveoffUnknown => "WO?",
            Self::Incomplete => "NC",
        }
    }

    /// Numeric project score used for greenie-board averaging.
    pub fn points(self) -> Option<f64> {
        match self {
            Self::Perfect => Some(5.0),
            Self::Ok => Some(4.0),
            Self::OkParentheses => Some(3.0),
            Self::NoGrade => Some(2.0),
            Self::Cut => Some(0.0),
            Self::Bolter => Some(2.5),
            Self::WaveoffUnknown | Self::Incomplete => None,
        }
    }
}

// ---------------------------------------------------------------------------
// V/STOL spot accuracy
// ---------------------------------------------------------------------------

/// AV-8B touchdown accuracy grade relative to the calibrated Tarawa spot 7.5.
///
/// The distance is measured on the carrier deck plane from the AV-8B pilot-ground
/// landing reference to the calibrated 7.5 point at the exact DCS land event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum SpotGrade {
    A,
    B,
    C,
    D,
}

impl SpotGrade {
    /// Convert a touchdown distance (metres) into the V/STOL spot grade.
    pub fn from_distance_m(distance_m: f64) -> Self {
        if distance_m < 1.0 {
            Self::A
        } else if distance_m < 3.0 {
            Self::B
        } else if distance_m < 5.0 {
            Self::C
        } else {
            Self::D
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::A => "A",
            Self::B => "B",
            Self::C => "C",
            Self::D => "D",
        }
    }

    /// Bonus added to the approach score for a recovered AV-8B pass.
    pub fn bonus_points(self) -> f64 {
        match self {
            Self::A => 1.00,
            Self::B => 0.75,
            Self::C => 0.50,
            Self::D => 0.00,
        }
    }
}

/// Combine an arbitrary V/STOL approach point value with the spot bonus.
///
/// The numeric result is capped at 5.0 and mapped onto the display labels used
/// by the CATOBAR greenie board. This function is only for a successfully
/// recovered V/STOL pass; CATOBAR grading remains in `compute_pass_grade`.
pub fn compute_vstol_final_grade_from_points(
    approach_points: f64,
    spot_grade: SpotGrade,
) -> (PassGrade, f64) {
    let points = (approach_points + spot_grade.bonus_points()).min(5.0);
    let final_grade = if points >= 4.0 {
        PassGrade::Ok
    } else if points >= 3.0 {
        PassGrade::OkParentheses
    } else if points >= 2.0 {
        PassGrade::NoGrade
    } else {
        PassGrade::Cut
    };
    (final_grade, points)
}

// ---------------------------------------------------------------------------
// Grade computation
// ---------------------------------------------------------------------------

/// Derive a `PassGrade` from the overall `Grading` outcome, gate deviations,
/// the continuous groove-to-touchdown trajectory, and the groove time
/// (seconds from groove entry to touchdown).
///
/// `trajectory` supplements the three point-in-time `gates`: its worst
/// amplitude is combined with the gates' (see `grade_from_gates`), so a
/// transient excursion between two gates cannot be graded better than it
/// would be if a gate had happened to land on it. Availability is still
/// governed by `gates.all_valid()` alone; `trajectory` never makes an
/// otherwise-incomplete pass gradable, only a gradable pass's amplitude more
/// accurate.
///
/// `groove_time_secs` is `None` when either timestamp was not recorded
/// (e.g. the aircraft never entered the 3/4-nm gate before landing).
///
/// PROJECT-DERIVED plain-language summary of this module's own measured GS/lineup deviations, for
/// display when no DCS LSO comment exists to show instead (`Track::dcs_grading`) -- DCS never
/// emits a `LandingQualityMark` comment for a touch-and-go (confirmed live 6 September 2026: 3/3
/// T&G passes in one session had no `dcs_grading` at all, only the single arrested pass in the
/// same session did), so this is the only way to show *why* a T&G graded the way it did. This is
/// explicitly a summary of our own measurements, not a DCS/NATOPS LSO comment and never phrased to
/// resemble one (no NATOPS shorthand codes, no invented callouts) -- see AGENTS.md, "Règles de
/// vérité", on never fabricating an authoritative-sounding result without evidence behind it.
/// Reuses the same amplitude/late-window thresholds as `grade_from_gates` so a phrase only appears
/// for a deviation that could actually have affected the grade. Returns an empty string when every
/// gate and the late-window trajectory check are all clean.
pub fn describe_measured_deviations(
    gates: &GateDeviations,
    trajectory: &[TrajectoryDeviation],
) -> String {
    let mut phrases = Vec::new();
    for (label, gate) in [
        ("3/4 NM", &gates.at_three_quarter_nm),
        ("1/2 NM", &gates.at_half_nm),
        ("1/4 NM", &gates.at_quarter_nm),
    ] {
        let Some(g) = gate else { continue };
        if g.gs_deviation_deg >= GS_SLIGHT_HIGH {
            phrases.push(format!(
                "high on glideslope at {label} ({:+.1}°)",
                g.gs_deviation_deg
            ));
        } else if g.gs_deviation_deg <= -GS_SLIGHT_LOW {
            phrases.push(format!(
                "low on glideslope at {label} ({:+.1}°)",
                g.gs_deviation_deg
            ));
        }
        if g.lineup_deg.abs() >= LU_SLIGHT {
            let side = if g.lineup_deg > 0.0 { "right" } else { "left" };
            phrases.push(format!(
                "{side} of centerline at {label} ({:+.1}°)",
                g.lineup_deg
            ));
        }
    }

    // The single closest late-window excursion (see `LATE_WINDOW_*` above) -- the most
    // operationally relevant one, since it is what a real LSO would have called out loudest.
    if let Some(d) = trajectory.iter().rev().find(|d| {
        d.distance_m <= LATE_WINDOW_DISTANCE_M
            && (d.gs_deviation_deg.abs() >= LATE_WINDOW_GS_DEG
                || d.lineup_deg.abs() >= LATE_WINDOW_LU_DEG)
    }) {
        phrases.push(format!(
            "still off in close ({:.0} m out: GS {:+.1}°, lineup {:+.1}°)",
            d.distance_m, d.gs_deviation_deg, d.lineup_deg
        ));
    }

    if phrases.is_empty() {
        return String::new();
    }
    let mut result = phrases.join(", ");
    if let Some(c) = result.get_mut(0..1) {
        c.make_ascii_uppercase();
    }
    result
}

/// `groove_entry_time` (DCS simulation time of roll-out-confirmed groove entry, CATOBAR only) is
/// forwarded to `GateDeviations::all_valid` so the 3/4 NM gate is not required when it was
/// captured before that instant -- see that method's doc comment. Returns the grade plus a short,
/// plain-language explanation of the specific rule that produced it -- see
/// `grade_from_gates_with_reason`'s doc comment for why this is the single source of truth
/// rather than a second, independent reconstruction. Used for the Discord "Why This Grade"
/// field.
pub fn compute_pass_grade_with_reason(
    grading: &Grading,
    gates: &GateDeviations,
    trajectory: &[TrajectoryDeviation],
    groove_time_secs: Option<f64>,
    groove_entry_time: Option<f64>,
) -> (PassGrade, String) {
    match grading {
        Grading::Unknown => (
            PassGrade::Incomplete,
            "Grading unavailable: no recognisable approach was recorded.".to_string(),
        ),
        Grading::ApproachOnly => {
            if gates.all_valid(groove_entry_time) {
                let (grade, reason) = grade_from_gates_with_reason(
                    gates,
                    trajectory,
                    groove_time_secs,
                    groove_entry_time,
                );
                (
                    grade,
                    format!("Approach only (outcome unknown): {reason}"),
                )
            } else {
                (
                    PassGrade::Incomplete,
                    "Grading unavailable: required gate coverage was not captured in valid chronological brackets. A positioning/detection limitation, not a pilot failure.".to_string(),
                )
            }
        }
        Grading::WaveoffUnknown => (
            PassGrade::WaveoffUnknown,
            "WO?: went around; can't tell from the data who or what caused it.".to_string(),
        ),
        Grading::Bolter if gates.all_valid(groove_entry_time) => (
            PassGrade::Bolter,
            "B: hook touched down but didn't catch a wire; the approach itself was on track."
                .to_string(),
        ),
        Grading::Recovered { .. } if gates.all_valid(groove_entry_time) => {
            grade_from_gates_with_reason(gates, trajectory, groove_time_secs, groove_entry_time)
        }
        // A qualification touch-and-go keeps the independently measured
        // approach grade, but can never receive a trap/wire-specific upgrade.
        // `_OK_` ("Perfect pass") is reserved for a real trap: a touch-and-go is a deliberate
        // hook-up practice pass, never a full stop, so it is capped one tier down instead (see
        // docs/GRADING_REFERENCE.md: "A touch-and-go cannot receive `_OK_` or points").
        Grading::TouchAndGo { .. } if gates.all_valid(groove_entry_time) => {
            match grade_from_gates_with_reason(gates, trajectory, groove_time_secs, groove_entry_time)
            {
                (PassGrade::Perfect, _) => (
                    PassGrade::Ok,
                    "OK: textbook approach, but capped one tier down — a touch-and-go can't receive a perfect pass.".to_string(),
                ),
                other => other,
            }
        }
        Grading::TouchAndGo { .. } | Grading::Bolter | Grading::Recovered { .. } => (
            PassGrade::Incomplete,
            "Grading unavailable: required gate coverage was not captured in valid chronological brackets. A positioning/detection limitation, not a pilot failure.".to_string(),
        ),
    }
}

/// Compute the AV-8B V/STOL approach grade and its numeric point value.
///
/// Unlike CATOBAR, the V/STOL approach score is the arithmetic mean of the
/// available 3/4-nm, 1/2-nm, and 1/4-nm gate grades. Each gate uses the same
/// deviation thresholds as CATOBAR, but wire and groove-time bonuses do not
/// apply. AOA is handled separately by `aoa_rating` for trace presentation.
pub fn compute_vstol_approach_grade_points(
    grading: &Grading,
    gates: &GateDeviations,
    trajectory: &[TrajectoryDeviation],
) -> (PassGrade, Option<f64>) {
    match grading {
        Grading::Unknown => (PassGrade::Incomplete, None),
        Grading::ApproachOnly if gates.all_valid(None) => {
            vstol_grade_from_coverage(gates, trajectory)
        }
        Grading::ApproachOnly => (PassGrade::Incomplete, None),
        Grading::WaveoffUnknown => (PassGrade::WaveoffUnknown, None),
        // V/STOL never relaxes the 3/4 NM gate requirement: unlike CATOBAR, it has no
        // roll-out-confirmed groove entry to distinguish a mid-turn reading from a real one (see
        // `AGENTS.md`, "Gates, outcomes et câble") -- `None` here always keeps the historical,
        // unconditional three-gates rule.
        Grading::Bolter if gates.all_valid(None) => (PassGrade::Bolter, PassGrade::Bolter.points()),
        Grading::TouchAndGo { .. } => (PassGrade::Incomplete, None),
        Grading::Recovered { .. } if gates.all_valid(None) => {
            vstol_grade_from_coverage(gates, trajectory)
        }
        Grading::Bolter | Grading::Recovered { .. } => (PassGrade::Incomplete, None),
    }
}

/// Production CASE I CATOBAR assessment. This is the only path that adds aircraft-specific AoA
/// episodes; callers without trustworthy wind-referenced AoA should pass `false`, which keeps the
/// episodes auditable while forcing `affects_grade = false` and zero effective severity.
pub fn compute_catobar_assessment(evidence: CatobarEvidence<'_>) -> CatobarAssessment {
    let CatobarEvidence {
        grading,
        gates,
        trajectory,
        datums,
        plane_info,
        aoa_reliable,
        groove_time_secs,
        groove_entry_time,
    } = evidence;
    let (base_grade, base_reason) = compute_pass_grade_with_reason(
        grading,
        gates,
        trajectory,
        groove_time_secs,
        groove_entry_time,
    );
    let episodes = classify_catobar_episodes(trajectory, datums, Some(plane_info), aoa_reliable);
    let eligible = !trajectory.is_empty()
        && gates.all_valid(groove_entry_time)
        && matches!(
            grading,
            Grading::Recovered { .. } | Grading::TouchAndGo { .. } | Grading::ApproachOnly
        )
        && base_grade != PassGrade::Cut;
    if !eligible {
        return CatobarAssessment {
            grade: base_grade,
            reason: base_reason,
            episodes,
        };
    }

    let (episode_grade, episode_reason) = grade_from_episode_set(&episodes);
    let has_scoring_episode = episodes.iter().any(|episode| episode.affects_grade);
    let mut grade = episode_grade;
    let mut reason = episode_reason;
    if base_grade == PassGrade::Perfect && !has_scoring_episode {
        grade = PassGrade::Perfect;
        reason = base_reason;
    }
    if matches!(grading, Grading::TouchAndGo { .. }) && grade == PassGrade::Perfect {
        grade = PassGrade::Ok;
        reason = "OK: trajectoire parfaite, plafonnée pour un touch-and-go.".to_string();
    }
    if matches!(grading, Grading::ApproachOnly) {
        reason = format!("Approach only (outcome unknown): {reason}");
    }
    CatobarAssessment {
        grade,
        reason,
        episodes,
    }
}

fn vstol_grade_from_coverage(
    gates: &GateDeviations,
    trajectory: &[TrajectoryDeviation],
) -> (PassGrade, Option<f64>) {
    let gate_scores = [
        vstol_gate_points(
            gates.at_three_quarter_nm.as_ref(),
            trajectory,
            crate::track::GATE_THREE_QUARTER_NM,
            false,
        ),
        vstol_gate_points(
            gates.at_half_nm.as_ref(),
            trajectory,
            crate::track::GATE_HALF_NM,
            false,
        ),
        vstol_gate_points(
            gates.at_quarter_nm.as_ref(),
            trajectory,
            crate::track::GATE_QUARTER_NM,
            true,
        ),
    ];
    let Some(gate_scores) = gate_scores.into_iter().collect::<Option<Vec<_>>>() else {
        return (PassGrade::Incomplete, None);
    };
    let average_points = gate_scores.iter().sum::<f64>() / gate_scores.len() as f64;
    (
        map_vstol_approach_points_to_grade(average_points),
        Some(average_points),
    )
}

fn vstol_gate_points(
    gate: Option<&crate::track::GateDatum>,
    trajectory: &[TrajectoryDeviation],
    distance_m: f64,
    is_quarter: bool,
) -> Option<f64> {
    if let Some(gate) = gate {
        return grade_single_gate(gate, is_quarter).points();
    }
    let [outer, inner] = trajectory.windows(2).find_map(|pair| {
        (pair[0].distance_m > distance_m && pair[1].distance_m <= distance_m)
            .then(|| [pair[0].clone(), pair[1].clone()])
    })?;
    let span = outer.distance_m - inner.distance_m;
    if span <= f64::EPSILON {
        return None;
    }
    let ratio = (outer.distance_m - distance_m) / span;
    let interpolate = |a: f64, b: f64| a + (b - a) * ratio;
    grade_gate_values(
        interpolate(outer.gs_deviation_deg, inner.gs_deviation_deg),
        interpolate(outer.lineup_deg, inner.lineup_deg),
        is_quarter,
    )
    .points()
}

fn map_vstol_approach_points_to_grade(points: f64) -> PassGrade {
    // Gate scores are averaged, so map at the midpoint between adjacent grade
    // values. This lets one slight gate (3, 4, 4 => 3.67) remain an OK while
    // one significant gate (2, 4, 4 => 3.33) becomes (OK).
    const OK_MIDPOINT: f64 = 3.5;
    const OK_PARENTHESES_MIDPOINT: f64 = 2.5;
    const NO_GRADE_MIDPOINT: f64 = 1.0;

    if points >= OK_MIDPOINT {
        PassGrade::Ok
    } else if points >= OK_PARENTHESES_MIDPOINT {
        PassGrade::OkParentheses
    } else if points >= NO_GRADE_MIDPOINT {
        PassGrade::NoGrade
    } else {
        PassGrade::Cut
    }
}

fn grade_single_gate(gate: &crate::track::GateDatum, quarter_nm: bool) -> PassGrade {
    grade_gate_values(gate.gs_deviation_deg, gate.lineup_deg, quarter_nm)
}

fn grade_gate_values(gs_deviation_deg: f64, lineup_deg: f64, quarter_nm: bool) -> PassGrade {
    if quarter_nm && gs_deviation_deg < GS_CUT_LOW_DEG {
        return PassGrade::Cut;
    }

    let gs_high = gs_deviation_deg.max(0.0);
    let gs_low = gs_deviation_deg.min(0.0).abs();
    let lineup = lineup_deg.abs();

    if gs_high >= GS_SIGNIFICANT || gs_low >= GS_SIGNIFICANT || lineup >= LU_MEDIUM {
        PassGrade::NoGrade
    } else if gs_high >= GS_SLIGHT_HIGH || gs_low >= GS_SLIGHT_LOW || lineup >= LU_SLIGHT {
        PassGrade::OkParentheses
    } else {
        PassGrade::Ok
    }
}

#[derive(Debug, Clone, Copy)]
struct AxisObservation {
    time: f64,
    distance_m: f64,
    value: f64,
    normalized_error: f64,
    severity: EpisodeSeverity,
    classification: &'static str,
}

fn approach_zone(distance_m: f64) -> ApproachZone {
    if distance_m > crate::track::GATE_HALF_NM {
        ApproachZone::Start
    } else if distance_m > crate::track::GATE_QUARTER_NM {
        ApproachZone::Middle
    } else if distance_m > LATE_WINDOW_DISTANCE_M {
        ApproachZone::InClose
    } else {
        ApproachZone::Ramp
    }
}

fn gs_severity(value: f64) -> EpisodeSeverity {
    match value.abs() {
        v if v < 0.5 => EpisodeSeverity::None,
        v if v < 1.0 => EpisodeSeverity::Small,
        v if v < 2.5 => EpisodeSeverity::Medium,
        _ => EpisodeSeverity::Large,
    }
}

fn lineup_severity(value: f64) -> EpisodeSeverity {
    match value.abs() {
        v if v < 1.0 => EpisodeSeverity::None,
        v if v < 2.0 => EpisodeSeverity::Small,
        v if v < 3.0 => EpisodeSeverity::Medium,
        _ => EpisodeSeverity::Large,
    }
}

fn aoa_severity(rating: Aoa) -> EpisodeSeverity {
    match rating {
        Aoa::OnSpeed => EpisodeSeverity::None,
        Aoa::SlightlyFast | Aoa::SlightlySlow => EpisodeSeverity::Small,
        Aoa::Fast | Aoa::Slow => EpisodeSeverity::Medium,
    }
}

/// Signed distance to the nearest edge of the aircraft's existing OnSpeed band. The band is
/// discovered through `AirplaneInfo::aoa_rating`; no parallel aircraft table or AoA threshold is
/// introduced here. Negative is Fast, positive is Slow.
fn normalized_aoa_error(plane: &AirplaneInfo, aoa: f64) -> Option<f64> {
    let rating = (plane.aoa_rating)(aoa);
    if rating == Aoa::OnSpeed {
        return Some(0.0);
    }
    let direction = match rating {
        Aoa::Fast | Aoa::SlightlyFast => 1.0,
        Aoa::Slow | Aoa::SlightlySlow => -1.0,
        Aoa::OnSpeed => unreachable!(),
    };
    let mut outside = aoa;
    let mut step = 0.25;
    let mut inside = None;
    for _ in 0..32 {
        let candidate = aoa + direction * step;
        if (plane.aoa_rating)(candidate) == Aoa::OnSpeed {
            inside = Some(candidate);
            break;
        }
        outside = candidate;
        step *= 2.0;
    }
    let mut inside = inside?;
    for _ in 0..48 {
        let midpoint = (outside + inside) / 2.0;
        if (plane.aoa_rating)(midpoint) == Aoa::OnSpeed {
            inside = midpoint;
        } else {
            outside = midpoint;
        }
    }
    Some(aoa - inside)
}

fn build_axis_episodes(
    axis: GradingAxis,
    observations: &[AxisObservation],
    affects_grade: bool,
    diagnostic: Option<&'static str>,
) -> Vec<GradingEpisode> {
    let mut episodes = Vec::new();
    let mut start = 0;
    while start < observations.len() {
        while start < observations.len() && observations[start].severity == EpisodeSeverity::None {
            start += 1;
        }
        if start == observations.len() {
            break;
        }
        let mut end = start;
        let mut none_run = 0;
        while end + 1 < observations.len() {
            end += 1;
            if observations[end].severity == EpisodeSeverity::None {
                none_run += 1;
                if none_run == EPISODE_STABLE_SAMPLES {
                    break;
                }
            } else {
                none_run = 0;
            }
        }

        // A single ordinary anomaly is treated as telemetry/aim-point noise. Safety Cut checks
        // are evaluated separately before this classifier and deliberately do not use this guard.
        if end > start
            && observations[start..=end]
                .iter()
                .filter(|s| s.severity != EpisodeSeverity::None)
                .count()
                >= PERSISTENCE_MIN_CONSECUTIVE_SAMPLES
        {
            let slice = &observations[start..=end];
            let maximum_severity = slice
                .iter()
                .map(|sample| sample.severity)
                .max()
                .unwrap_or(EpisodeSeverity::None);
            // Earliest sample wins a complete tie, making repeated equal maxima deterministic.
            let (peak_index, peak) = slice
                .iter()
                .enumerate()
                .filter(|(_, sample)| sample.severity == maximum_severity)
                .max_by(|(left_index, left), (right_index, right)| {
                    left.normalized_error
                        .abs()
                        .total_cmp(&right.normalized_error.abs())
                        .then_with(|| right_index.cmp(left_index))
                })
                .unwrap_or((0, &slice[0]));
            let peak_value = peak.value;
            let peak_zone = approach_zone(peak.distance_m);
            let post_peak = &slice[peak_index..];
            let reversals = count_reversals(post_peak.iter().map(|sample| sample.normalized_error));
            let max_level = maximum_severity.level();
            let duration_s = (slice[slice.len() - 1].time - slice[0].time).max(0.0);
            let trend_end = post_peak.last().unwrap_or(peak);
            let trend_start = trend_end.time - TREND_WINDOW_S;
            let trend_first = post_peak
                .iter()
                .find(|sample| sample.time >= trend_start)
                .unwrap_or(peak);
            let trend_dt = trend_end.time - trend_first.time;
            let trend_slope = (trend_dt > 0.0).then(|| {
                (trend_end.normalized_error.abs() - trend_first.normalized_error.abs()) / trend_dt
            });
            let improving_by_trend =
                trend_slope.is_some_and(|slope| slope <= -TREND_WORSENING_DEG_PER_S);
            let durable_improvement_index = (1..post_peak.len()).find(|&index| {
                post_peak[index].severity.level() < max_level
                    && post_peak[index..]
                        .iter()
                        .take(EPISODE_STABLE_SAMPLES)
                        .filter(|sample| sample.severity.level() < max_level)
                        .count()
                        == EPISODE_STABLE_SAMPLES
            });
            let first_improvement_s =
                durable_improvement_index.map(|index| post_peak[index].time - peak.time);
            let return_to_none_index = (1..post_peak.len()).find(|&index| {
                post_peak[index..]
                    .iter()
                    .take(EPISODE_STABLE_SAMPLES)
                    .filter(|sample| sample.severity == EpisodeSeverity::None)
                    .count()
                    == EPISODE_STABLE_SAMPLES
            });
            let return_to_none_s =
                return_to_none_index.map(|index| post_peak[index].time - peak.time);
            let stabilization_start = return_to_none_index.or(durable_improvement_index);
            let (stabilized_severity, stabilization_samples) = stabilization_start.map_or(
                (post_peak.last().unwrap_or(peak).severity, 0),
                |index| {
                    let severity = post_peak[index].severity;
                    let count = post_peak[index..]
                        .iter()
                        .take_while(|sample| sample.severity == severity)
                        .count();
                    (severity, count)
                },
            );
            let post_correction_aggravation = stabilization_start.is_some_and(|index| {
                post_peak[index + stabilization_samples..]
                    .iter()
                    .any(|sample| sample.severity.level() > stabilized_severity.level())
            });
            let improved = first_improvement_s.is_some() || improving_by_trend;
            let evolution = if reversals >= OSCILLATION_MIN_REVERSALS || post_correction_aggravation
            {
                EpisodeEvolution::Mixed
            } else if !improved {
                EpisodeEvolution::AwayFromTarget
            } else if improved {
                EpisodeEvolution::TowardTarget
            } else {
                EpisodeEvolution::Stagnant
            };
            let (correction, correction_reason) = if !affects_grade {
                (CorrectionQuality::NotAssessed, "aoa_reference_unreliable")
            } else if reversals >= OSCILLATION_MIN_REVERSALS {
                (CorrectionQuality::Poor, "repeated_significant_inversions")
            } else if post_correction_aggravation {
                (CorrectionQuality::Poor, "aggravation_after_improvement")
            } else if !improved {
                (CorrectionQuality::Poor, "no_real_post_peak_improvement")
            } else if peak_zone == ApproachZone::Ramp
                && stabilization_samples < EPISODE_STABLE_SAMPLES
            {
                (
                    CorrectionQuality::Poor,
                    "ramp_correction_not_stabilized_before_trajectory_end",
                )
            } else if first_improvement_s
                .is_some_and(|elapsed| elapsed <= peak_zone.good_correction_deadline_s())
                && stabilization_samples >= EPISODE_STABLE_SAMPLES
                && (return_to_none_s.is_some() || stabilized_severity.level() < max_level)
            {
                (
                    CorrectionQuality::Good,
                    "post_peak_improvement_within_zone_deadline_and_stabilized",
                )
            } else {
                (
                    CorrectionQuality::Average,
                    "real_post_peak_improvement_late_incomplete_or_insufficiently_stabilized",
                )
            };
            let corrected_level = match correction {
                CorrectionQuality::Good => max_level.saturating_sub(1),
                CorrectionQuality::Average | CorrectionQuality::NotAssessed => max_level,
                CorrectionQuality::Poor => (max_level + 1).min(3),
            };
            let effective_severity = if affects_grade {
                f64::from(corrected_level) * peak_zone.weight()
            } else {
                0.0
            };
            episodes.push(GradingEpisode {
                axis,
                started_at_dcs: slice[0].time,
                ended_at_dcs: slice[slice.len() - 1].time,
                duration_s,
                most_severe_zone: peak_zone,
                zone_weight: peak_zone.weight(),
                maximum_severity,
                corrected_severity: EpisodeSeverity::from_level(corrected_level),
                effective_severity,
                peak_value,
                peak_normalized_error: peak.normalized_error,
                peak_at_dcs: peak.time,
                peak_zone,
                peak_classification: peak.classification,
                evolution,
                returned_to_less_severe_band: first_improvement_s.is_some(),
                oscillation_reversals: reversals,
                first_durable_improvement_delay_s: first_improvement_s,
                return_to_none_delay_s: return_to_none_s,
                stabilized_severity,
                stabilization_samples,
                post_correction_aggravation,
                correction_reason,
                correction,
                affects_grade,
                diagnostic,
            });
        }
        start = end + 1;
    }
    episodes
}

fn classify_catobar_episodes(
    trajectory: &[TrajectoryDeviation],
    datums: &[Datum],
    plane_info: Option<&AirplaneInfo>,
    aoa_reliable: bool,
) -> Vec<GradingEpisode> {
    let gs = trajectory
        .iter()
        .filter(|sample| sample.gs_deviation_deg.is_finite())
        .map(|sample| AxisObservation {
            time: sample.timestamp_dcs,
            distance_m: sample.distance_m,
            value: sample.gs_deviation_deg,
            normalized_error: sample.gs_deviation_deg,
            severity: gs_severity(sample.gs_deviation_deg),
            classification: if sample.gs_deviation_deg >= 0.0 {
                "high"
            } else {
                "low"
            },
        })
        .collect::<Vec<_>>();
    let lineup = trajectory
        .iter()
        .filter(|sample| sample.lineup_deg.is_finite())
        .map(|sample| AxisObservation {
            time: sample.timestamp_dcs,
            distance_m: sample.distance_m,
            value: sample.lineup_deg,
            normalized_error: sample.lineup_deg,
            severity: lineup_severity(sample.lineup_deg),
            classification: if sample.lineup_deg >= 0.0 {
                "right"
            } else {
                "left"
            },
        })
        .collect::<Vec<_>>();
    let mut episodes = build_axis_episodes(GradingAxis::Glideslope, &gs, true, None);
    episodes.extend(build_axis_episodes(
        GradingAxis::Lineup,
        &lineup,
        true,
        None,
    ));

    if let (Some(plane), Some(first), Some(last)) =
        (plane_info, trajectory.first(), trajectory.last())
    {
        let aoa = datums
            .iter()
            .filter(|sample| {
                sample.time >= first.timestamp_dcs
                    && sample.time <= last.timestamp_dcs
                    && sample.x > 0.0
                    && sample.aoa.is_finite()
            })
            .filter_map(|sample| {
                let rating = (plane.aoa_rating)(sample.aoa);
                Some(AxisObservation {
                    time: sample.time,
                    distance_m: sample.x,
                    value: sample.aoa,
                    normalized_error: normalized_aoa_error(plane, sample.aoa)?,
                    severity: aoa_severity(rating),
                    classification: match rating {
                        Aoa::Fast => "fast",
                        Aoa::SlightlyFast => "slightly_fast",
                        Aoa::OnSpeed => "on_speed",
                        Aoa::SlightlySlow => "slightly_slow",
                        Aoa::Slow => "slow",
                    },
                })
            })
            .collect::<Vec<_>>();
        episodes.extend(build_axis_episodes(
            GradingAxis::Aoa,
            &aoa,
            aoa_reliable,
            (!aoa_reliable).then_some("aoa_reference_unavailable_no_grade_effect"),
        ));
    }
    episodes.sort_by(|left, right| left.started_at_dcs.total_cmp(&right.started_at_dcs));
    episodes
}

/// `trajectory` is the continuous groove-to-touchdown series (see
/// `TrajectoryDeviation`); passing `&[]` reproduces the historical
/// three-gates-only behaviour exactly (all folds below start from the same
/// gate-only values and an empty slice contributes nothing).
///
/// `pub(crate)` (rather than only reachable through `compute_pass_grade`) so the `cadence-ab`
/// diagnostic command (B.2 of the notation/cadence work) can grade a replayed gate/trajectory
/// pair directly, without needing to fabricate a `Grading` outcome the replay never computes.
/// `cadence-ab` always passes `None` for `groove_entry_time` (it never re-derives groove entry
/// from the replay), which keeps its historical, unconditional three-gates behaviour.
///
/// `groove_entry_time` also excludes the 3/4 NM gate's own GS/lineup values from the amplitude
/// computed below when it was captured before that instant — not just from `all_valid`'s
/// completeness check. Counting a turn-artifact reading toward "worst GS/lineup" would defeat the
/// point of no longer requiring it: see `GateDeviations::all_valid`'s doc comment for why that
/// reading is not representative of the approach.
pub(crate) fn grade_from_gates(
    gates: &GateDeviations,
    trajectory: &[TrajectoryDeviation],
    groove_time_secs: Option<f64>,
    groove_entry_time: Option<f64>,
) -> PassGrade {
    grade_from_gates_with_reason(gates, trajectory, groove_time_secs, groove_entry_time).0
}

/// Same result as `grade_from_gates`, plus a short, plain-language explanation of the specific
/// rule that produced it -- for the Discord "Why This Grade" field. Built inline, as the single
/// source of truth for both the grade and its reason, rather than reconstructed afterward from
/// the final grade alone: this project has already been bitten once by two code paths computing
/// the same answer independently and drifting apart (`wire_estimated`/`wire_estimation`, fixed 6
/// September 2026), so `grade_from_gates` above is now a thin wrapper over this function instead
/// of a parallel implementation. Deliberately worded for a pilot, not a developer: measured
/// values are given, but internal constant names never are.
pub(crate) fn grade_from_gates_with_reason(
    gates: &GateDeviations,
    trajectory: &[TrajectoryDeviation],
    groove_time_secs: Option<f64>,
    groove_entry_time: Option<f64>,
) -> (PassGrade, String) {
    // Dangerously low at the 1/4-nm gate → Cut pass. GS_CUT_LOW_DEG is negative, so this
    // triggers when the hook is well below the ideal glide path at close range. Also checked
    // at every continuous sample inside the 1/4-nm gate distance, not only at the exact gate
    // crossing: a brief dip below threshold that recovers before crossing 463 m is just as
    // dangerous as one measured exactly at the gate. A sustained excessive sink rate or bank
    // angle in that same zone (`dangerous_sink_rate_or_bank`) is graded the same way — see its
    // doc comment for why those thresholds carry no NATOPS-numeric backing, unlike GS_CUT_LOW_DEG.
    if let Some(g) = gates
        .at_quarter_nm
        .as_ref()
        .filter(|g| g.gs_deviation_deg < GS_CUT_LOW_DEG)
    {
        return (
            PassGrade::Cut,
            format!(
                "C: glideslope {:.1}° low at the 1/4 NM gate — dangerously low this close to the ship.",
                g.gs_deviation_deg.abs()
            ),
        );
    }
    if let Some(d) = trajectory.iter().find(|d| {
        d.distance_m <= crate::track::GATE_QUARTER_NM && d.gs_deviation_deg < GS_CUT_LOW_DEG
    }) {
        return (
            PassGrade::Cut,
            format!(
                "C: glideslope {:.1}° low inside 1/4 NM — dangerously low this close to the ship.",
                d.gs_deviation_deg.abs()
            ),
        );
    }
    if let Some(reason) = dangerous_sink_rate_or_bank_reason(trajectory) {
        return (PassGrade::Cut, reason);
    }

    let episodes = classify_catobar_episodes(trajectory, &[], None, false);
    let (tier, reason) = if trajectory.is_empty() {
        // Legacy/offline fallback for schema-v3 reports that contain gates but no continuous
        // samples. Live CASE I CATOBAR tracks always use the episode classifier below.
        legacy_gate_only_grade(gates, groove_entry_time)
    } else {
        grade_from_episode_set(&episodes)
    };

    // `_OK_` (NAVAIR 00-80T-104 §11.4.1, "Perfect pass"): only reachable from a pass that has
    // already cleared every check above and landed on plain `Ok` — this is a strict tightening
    // of `Ok`, never an alternate path, so trend/oscillation/late-window already vouch for the
    // approach before this even runs. See `OK_PERFECT_*` for why the amplitude band is
    // PROJECT-DERIVED (MOOSE Airboss-sourced) while the groove-time window is NATOPS `OFFICIAL`.
    if tier == PassGrade::Ok
        && episodes.is_empty()
        && is_amplitude_perfect(gates, trajectory)
        && trend_worsening_detail(trajectory).is_none()
        && oscillation_detail(trajectory).is_none()
        && groove_time_secs.is_some_and(|t| {
            (OK_PERFECT_GROOVE_TIME_MIN_S..=OK_PERFECT_GROOVE_TIME_MAX_S).contains(&t)
        })
    {
        (
            PassGrade::Perfect,
            format!(
                "_OK_: textbook pass on every gate and the continuous approach, groove time {:.1} s.",
                groove_time_secs.unwrap_or_default()
            ),
        )
    } else {
        (tier, reason)
    }
}

fn legacy_gate_only_grade(
    gates: &GateDeviations,
    groove_entry_time: Option<f64>,
) -> (PassGrade, String) {
    let three_quarter = if gates.three_quarter_counts(groove_entry_time) {
        gates.at_three_quarter_nm.as_ref()
    } else {
        None
    };
    let all_gs = [
        three_quarter.map(|g| g.gs_deviation_deg),
        gates.at_half_nm.as_ref().map(|g| g.gs_deviation_deg),
        gates.at_quarter_nm.as_ref().map(|g| g.gs_deviation_deg),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>();
    let worst_gs_high = all_gs.iter().copied().fold(0.0_f64, f64::max);
    let worst_gs_low = all_gs.iter().copied().fold(0.0_f64, f64::min).abs();
    let worst_lu = [
        three_quarter.map(|g| g.lineup_deg.abs()),
        gates.at_half_nm.as_ref().map(|g| g.lineup_deg.abs()),
        gates.at_quarter_nm.as_ref().map(|g| g.lineup_deg.abs()),
    ]
    .into_iter()
    .flatten()
    .fold(0.0_f64, f64::max);
    if worst_gs_high >= GS_SIGNIFICANT {
        (
            PassGrade::NoGrade,
            format!("--: glideslope {worst_gs_high:.1}° high — well outside tolerance."),
        )
    } else if worst_gs_low >= GS_SIGNIFICANT {
        (
            PassGrade::NoGrade,
            format!("--: glideslope {worst_gs_low:.1}° low — well outside tolerance."),
        )
    } else if worst_lu >= LU_MEDIUM {
        (
            PassGrade::NoGrade,
            format!("--: lineup off by {worst_lu:.1}° — more than double what OK allows."),
        )
    } else if worst_gs_high >= GS_SLIGHT_HIGH {
        (
            PassGrade::OkParentheses,
            format!(
                "(OK): drifted {worst_gs_high:.1}° high on glideslope — OK needs better than {GS_SLIGHT_HIGH:.1}°."
            ),
        )
    } else if worst_gs_low >= GS_SLIGHT_LOW {
        (
            PassGrade::OkParentheses,
            format!(
                "(OK): drifted {worst_gs_low:.1}° low on glideslope — OK needs better than {GS_SLIGHT_LOW:.1}°."
            ),
        )
    } else if worst_lu >= LU_SLIGHT {
        (
            PassGrade::OkParentheses,
            format!("(OK): lineup off by {worst_lu:.1}° — OK needs better than {LU_SLIGHT:.1}°."),
        )
    } else {
        (
            PassGrade::Ok,
            "OK: within tolerance on every gate and the continuous approach.".to_string(),
        )
    }
}

fn grade_from_episode_set(episodes: &[GradingEpisode]) -> (PassGrade, String) {
    let Some(worst) = episodes
        .iter()
        .filter(|episode| episode.affects_grade)
        .max_by(|left, right| {
            left.effective_severity
                .total_cmp(&right.effective_severity)
                .then_with(|| left.maximum_severity.cmp(&right.maximum_severity))
        })
    else {
        return (
            PassGrade::Ok,
            "OK: trajectoire stable, sans épisode significatif.".to_string(),
        );
    };
    let grade = if worst.effective_severity < 1.5 {
        PassGrade::Ok
    } else if worst.effective_severity < 3.0 {
        PassGrade::OkParentheses
    } else {
        PassGrade::NoGrade
    };
    (grade, episode_reason(grade, worst))
}

fn episode_reason(grade: PassGrade, episode: &GradingEpisode) -> String {
    let axis = match episode.axis {
        GradingAxis::Glideslope => "écart de pente".to_string(),
        GradingAxis::Lineup => "lineup".to_string(),
        GradingAxis::Aoa => format!(
            "AoA {}",
            match episode.peak_classification {
                "fast" | "slightly_fast" => "rapide",
                "slow" | "slightly_slow" => "lent",
                _ => "hors vitesse",
            }
        ),
    };
    let severity = match episode.maximum_severity {
        EpisodeSeverity::None => "nul",
        EpisodeSeverity::Small => "léger",
        EpisodeSeverity::Medium => "moyen",
        EpisodeSeverity::Large => "gros",
    };
    let correction = match episode.correction {
        CorrectionQuality::Good => "corrigé rapidement après le pic et stabilisé",
        CorrectionQuality::Average => "correction réelle après le pic, mais tardive ou incomplète",
        CorrectionQuality::Poor => "sans retour stable vers la cible après le pic",
        CorrectionQuality::NotAssessed => "conservé à titre diagnostique",
    };
    format!(
        "{}: {axis} {severity} en {}, {correction}.",
        grade.label(),
        episode.most_severe_zone.label()
    )
}

/// Whether every gate and every continuous-trajectory sample stayed inside the `_OK_` tolerance
/// band (`OK_PERFECT_GS_HIGH_DEG`/`OK_PERFECT_GS_LOW_DEG`/`OK_PERFECT_LU_ABS_DEG`). Gates are
/// already bracket/skew-validated single points of trusted evidence, so each one is checked
/// directly, with no pardon for a single reading. The continuous trajectory gets the same
/// noise-pardon as everywhere else in this module (`PERSISTENCE_MIN_CONSECUTIVE_SAMPLES`): one
/// isolated, non-repeating frame outside the band does not by itself deny `_OK_`, since a real
/// perfect pass should not be punished for a single unrelated telemetry hiccup.
fn is_amplitude_perfect(gates: &GateDeviations, trajectory: &[TrajectoryDeviation]) -> bool {
    let gate_within_band = |gate: &Option<crate::track::GateDatum>| match gate {
        Some(g) => {
            g.gs_deviation_deg <= OK_PERFECT_GS_HIGH_DEG
                && g.gs_deviation_deg >= -OK_PERFECT_GS_LOW_DEG
                && g.lineup_deg.abs() <= OK_PERFECT_LU_ABS_DEG
        }
        None => true,
    };
    if !gate_within_band(&gates.at_three_quarter_nm)
        || !gate_within_band(&gates.at_half_nm)
        || !gate_within_band(&gates.at_quarter_nm)
    {
        return false;
    }

    let elevated: Vec<bool> = trajectory
        .iter()
        .map(|d| {
            d.gs_deviation_deg > OK_PERFECT_GS_HIGH_DEG
                || d.gs_deviation_deg < -OK_PERFECT_GS_LOW_DEG
                || d.lineup_deg.abs() > OK_PERFECT_LU_ABS_DEG
        })
        .collect();
    !persistent_mask(&elevated, PERSISTENCE_MIN_CONSECUTIVE_SAMPLES)
        .iter()
        .any(|&keep| keep)
}

/// Whether GS or lineup deviation was clearly getting worse, not better, in the final
/// `TREND_WINDOW_S` seconds of the recorded trajectory, and which axis/slope did so. A simple
/// two-point slope of the deviation's absolute value over that window, as prescribed by A.2 of
/// the notation work: "no sophisticated filtering at this stage". Returns `None` (no penalty)
/// whenever there is not enough data to judge a trend, which errs toward not penalizing rather
/// than inventing a signal.
/// used to build a plain-language reason (see `grade_from_gates_with_reason`). Checks GS before
/// lineup, matching the order the original boolean OR evaluated them in.
fn trend_worsening_detail(trajectory: &[TrajectoryDeviation]) -> Option<(&'static str, f64)> {
    let reference_time = trajectory.last()?.timestamp_dcs;
    let window: Vec<&TrajectoryDeviation> = trajectory
        .iter()
        .filter(|d| d.timestamp_dcs >= reference_time - TREND_WINDOW_S)
        .collect();
    let (Some(first), Some(last)) = (window.first(), window.last()) else {
        return None;
    };
    let dt = last.timestamp_dcs - first.timestamp_dcs;
    if dt <= 0.0 {
        return None;
    }
    let gs_slope = (last.gs_deviation_deg.abs() - first.gs_deviation_deg.abs()) / dt;
    let lu_slope = (last.lineup_deg.abs() - first.lineup_deg.abs()) / dt;
    if gs_slope >= TREND_WORSENING_DEG_PER_S {
        Some(("glideslope", gs_slope))
    } else if lu_slope >= TREND_WORSENING_DEG_PER_S {
        Some(("lineup", lu_slope))
    } else {
        None
    }
}

/// For each run of consecutive `true` values in `elevated`, marks the whole run `true` in the
/// result only if the run is at least `min_run` samples long; shorter runs (a lone spike) come
/// back `false`. `min_run` is a parameter (rather than always `PERSISTENCE_MIN_CONSECUTIVE_SAMPLES`)
/// so the danger-cut checks below can require a stricter run length
/// (`DANGER_CUT_MIN_CONSECUTIVE_SAMPLES`) than the ordinary amplitude guard.
fn persistent_mask(elevated: &[bool], min_run: usize) -> Vec<bool> {
    let mut keep = vec![false; elevated.len()];
    let mut i = 0;
    while i < elevated.len() {
        if elevated[i] {
            let start = i;
            while i < elevated.len() && elevated[i] {
                i += 1;
            }
            if i - start >= min_run {
                keep[start..i].fill(true);
            }
        } else {
            i += 1;
        }
    }
    keep
}

/// Danger cut: a sustained excessive sink rate or bank angle at or inside the quarter-NM gate —
/// the same "no distance left to correct it" zone as `GS_CUT_LOW_DEG` — is graded as a Cut,
/// exactly like a dangerously low glideslope. See `SINK_RATE_CUT_MPS`/`BANK_ANGLE_CUT_DEG` for why
/// neither threshold is NATOPS-numeric. Guarded by `DANGER_CUT_MIN_CONSECUTIVE_SAMPLES` so a
/// single spike or noisy frame never cuts a pass on its own. Returns a ready-made plain-language
/// reason quoting the peak value within the persisting run when tripped -- used by
/// `grade_from_gates_with_reason`. Checks sink rate before bank, matching the order the original
/// boolean OR evaluated them in.
fn dangerous_sink_rate_or_bank_reason(trajectory: &[TrajectoryDeviation]) -> Option<String> {
    let near_ramp: Vec<&TrajectoryDeviation> = trajectory
        .iter()
        .filter(|d| d.distance_m <= crate::track::GATE_QUARTER_NM)
        .collect();
    let sink_elevated: Vec<bool> = near_ramp
        .iter()
        .map(|d| d.sink_rate_mps >= SINK_RATE_CUT_MPS)
        .collect();
    let sink_keep = persistent_mask(&sink_elevated, DANGER_CUT_MIN_CONSECUTIVE_SAMPLES);
    if let Some(peak) = near_ramp
        .iter()
        .zip(&sink_keep)
        .filter(|(_, &keep)| keep)
        .map(|(d, _)| d.sink_rate_mps)
        .fold(None, |acc: Option<f64>, v| {
            Some(acc.map_or(v, |a| a.max(v)))
        })
    {
        return Some(format!(
            "C: sink rate {peak:.1} m/s sustained inside 1/4 NM — descending too fast this close to the ramp."
        ));
    }

    let bank_elevated: Vec<bool> = near_ramp
        .iter()
        .map(|d| d.bank_deg.abs() >= BANK_ANGLE_CUT_DEG)
        .collect();
    let bank_keep = persistent_mask(&bank_elevated, DANGER_CUT_MIN_CONSECUTIVE_SAMPLES);
    if let Some(peak) = near_ramp
        .iter()
        .zip(&bank_keep)
        .filter(|(_, &keep)| keep)
        .map(|(d, _)| d.bank_deg.abs())
        .fold(None, |acc: Option<f64>, v| {
            Some(acc.map_or(v, |a| a.max(v)))
        })
    {
        return Some(format!(
            "C: bank angle {peak:.0}° sustained inside 1/4 NM — too steep a bank this close to the ramp."
        ));
    }

    None
}

/// A.4 (NATOPS `OC` — overcontrolled): whether GS or (signed) lineup deviation reversed
/// direction at least `OSCILLATION_MIN_REVERSALS` times, by at least `OSCILLATION_MIN_SWING_DEG`
/// each time, within the final `OSCILLATION_WINDOW_S` seconds of the trajectory. Unlike
/// `trend_worsening` (a net two-point slope), this looks at every consecutive pair in the
/// window, so a pilot correcting back and forth around the aim point is caught even when the
/// net slope over the window is near zero.
/// Returns which axis oscillated and how many reversals it made, if any -- used to build a
/// plain-language reason (see `grade_from_gates_with_reason`). Checks GS before lineup, matching
/// the order the original boolean OR evaluated them in.
fn oscillation_detail(trajectory: &[TrajectoryDeviation]) -> Option<(&'static str, usize)> {
    let reference_time = trajectory.last()?.timestamp_dcs;
    let window: Vec<&TrajectoryDeviation> = trajectory
        .iter()
        .filter(|d| d.timestamp_dcs >= reference_time - OSCILLATION_WINDOW_S)
        .collect();
    let gs_reversals = count_reversals(window.iter().map(|d| d.gs_deviation_deg));
    if gs_reversals >= OSCILLATION_MIN_REVERSALS {
        return Some(("glideslope", gs_reversals));
    }
    let lu_reversals = count_reversals(window.iter().map(|d| d.lineup_deg));
    if lu_reversals >= OSCILLATION_MIN_REVERSALS {
        return Some(("lineup", lu_reversals));
    }
    None
}

/// Counts direction reversals in a signed series, ignoring any step smaller than
/// `OSCILLATION_MIN_SWING_DEG` (telemetry/aim-point noise never counts as a leg). The reference
/// point only advances on a significant move, so a run of sub-threshold jitter around the same
/// spot cannot mask a real swing by repeatedly resetting the baseline.
fn count_reversals(values: impl Iterator<Item = f64>) -> usize {
    let mut reference: Option<f64> = None;
    let mut last_sign: Option<f64> = None;
    let mut reversals = 0;
    for value in values {
        let Some(previous) = reference else {
            reference = Some(value);
            continue;
        };
        let delta = value - previous;
        if delta.abs() < OSCILLATION_MIN_SWING_DEG {
            continue;
        }
        let sign = delta.signum();
        if let Some(previous_sign) = last_sign {
            if sign != previous_sign {
                reversals += 1;
            }
        }
        last_sign = Some(sign);
        reference = Some(value);
    }
    reversals
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::track::{GateCaptureMethod, GateDatum, GateDeviations, GateQuality, GateStatus};

    /// Build a `GateDeviations` from degree values (the unit used for grading).
    /// Foot values are set to 0.0 as they are not used in grading logic.
    fn gates_deg(
        gs_3q: f64,
        lu_3q: f64,
        gs_h: f64,
        lu_h: f64,
        gs_q: f64,
        lu_q: f64,
    ) -> GateDeviations {
        let datum = |gs_deg, lu_deg, timestamp_dcs| {
            Some(GateDatum {
                gs_deviation_deg: gs_deg,
                lineup_deg: lu_deg,
                gs_deviation_ft: 0.0,
                lineup_ft: 0.0,
                timestamp_dcs,
                distance_m: 0.0,
                sample_gap_ms: 100.0,
                skew_ms: 0.0,
                method: GateCaptureMethod::Interpolated,
            })
        };
        GateDeviations {
            at_three_quarter_nm: datum(gs_3q, lu_3q, 1.0),
            at_half_nm: datum(gs_h, lu_h, 2.0),
            at_quarter_nm: datum(gs_q, lu_q, 3.0),
            three_quarter_quality: GateQuality {
                status: GateStatus::Valid,
                reason: None,
                bracket_gap_ms: Some(100.0),
                ..GateQuality::default()
            },
            half_quality: GateQuality {
                status: GateStatus::Valid,
                reason: None,
                bracket_gap_ms: Some(100.0),
                ..GateQuality::default()
            },
            quarter_quality: GateQuality {
                status: GateStatus::Valid,
                reason: None,
                bracket_gap_ms: Some(100.0),
                ..GateQuality::default()
            },
        }
    }

    #[test]
    fn test_clean_pass_without_groove_time_evidence_is_ok_not_perfect() {
        // All deviations well within OK margins (in fact within the tighter `_OK_` band too),
        // but no groove time is known here (`None`): `_OK_` is never granted absent that
        // evidence, so this stays `Ok`, not `Perfect`. See
        // `clean_pass_with_groove_time_in_window_reaches_perfect_regardless_of_wire` for the
        // same amplitude with groove time actually supplied.
        let g = gates_deg(0.2, 0.3, 0.1, 0.2, 0.1, 0.1);
        assert_eq!(grade_from_gates(&g, &[], None, None), PassGrade::Ok);
    }

    #[test]
    fn test_slight_gs_deviation_is_ok_parentheses() {
        // 0.6° high GS at 3/4 nm: exceeds GS_SLIGHT_HIGH (0.5°) → (OK).
        let g = gates_deg(0.6, 0.3, 0.1, 0.2, 0.1, 0.1);
        assert_eq!(
            grade_from_gates(&g, &[], None, None),
            PassGrade::OkParentheses
        );
    }

    #[test]
    fn test_slight_gs_high_threshold_is_0_5() {
        // 0.9° high GS: still between GS_SLIGHT_HIGH and GS_SIGNIFICANT → (OK), not --.
        let g = gates_deg(0.9, 0.0, 0.0, 0.0, 0.0, 0.0);
        assert_eq!(
            grade_from_gates(&g, &[], None, None),
            PassGrade::OkParentheses
        );
    }

    #[test]
    fn test_catobar_slight_gs_low_threshold_is_0_5() {
        // The boundary is inclusive: 0.5° low GS is (OK).
        let g = gates_deg(-0.5, 0.0, 0.0, 0.0, 0.0, 0.0);
        assert_eq!(
            grade_from_gates(&g, &[], None, None),
            PassGrade::OkParentheses
        );
    }

    #[test]
    fn test_gs_high_below_new_threshold_is_ok() {
        // 0.4° high GS: below GS_SLIGHT_HIGH (0.5°) → OK.
        let g = gates_deg(0.4, 0.0, 0.0, 0.0, 0.0, 0.0);
        assert_eq!(grade_from_gates(&g, &[], None, None), PassGrade::Ok);
    }

    #[test]
    fn test_slight_lu_deviation_is_ok_parentheses() {
        // 1.1° LU at half nm: exceeds LU_SLIGHT (1.0°) → (OK).
        let g = gates_deg(0.2, 0.3, 0.1, 1.1, 0.1, 0.1);
        assert_eq!(
            grade_from_gates(&g, &[], None, None),
            PassGrade::OkParentheses
        );
    }

    #[test]
    fn test_catobar_significant_gs_threshold_is_1_0() {
        // The boundary is inclusive: 1.0° high GS is no-grade.
        let g = gates_deg(1.0, 0.3, 0.1, 0.2, 0.1, 0.1);
        assert_eq!(grade_from_gates(&g, &[], None, None), PassGrade::NoGrade);
    }

    #[test]
    fn test_significant_lu_deviation_is_no_grade() {
        // 3.1° LU at 1/4 nm: exceeds LU_SIGNIFICANT (3.0°) → --.
        let g = gates_deg(0.2, 0.3, 0.1, 0.2, 0.1, 3.1);
        assert_eq!(grade_from_gates(&g, &[], None, None), PassGrade::NoGrade);
    }

    #[test]
    fn test_medium_lu_deviation_is_no_grade() {
        // 2.1° LU at 3/4 nm: exceeds LU_MEDIUM (2.0°) → --.
        let g = gates_deg(0.0, 2.1, 0.0, 0.0, 0.0, 0.0);
        assert_eq!(grade_from_gates(&g, &[], None, None), PassGrade::NoGrade);
    }

    #[test]
    fn test_below_medium_lu_is_ok_parentheses() {
        // 1.9° LU: above LU_SLIGHT but below LU_MEDIUM → (OK).
        let g = gates_deg(0.0, 1.9, 0.0, 0.0, 0.0, 0.0);
        assert_eq!(
            grade_from_gates(&g, &[], None, None),
            PassGrade::OkParentheses
        );
    }

    #[test]
    fn test_dangerously_low_at_quarter_nm_is_cut() {
        // −2.6° GS at 1/4 nm: below GS_CUT_LOW_DEG (−2.5°) → Cut.
        let g = gates_deg(0.0, 0.0, 0.0, 0.0, -2.6, 0.0);
        assert_eq!(grade_from_gates(&g, &[], None, None), PassGrade::Cut);
    }

    #[test]
    fn test_low_at_earlier_gates_not_cut() {
        // −2.6° GS only at 3/4 nm (not at 1/4 nm) → NoGrade, not Cut.
        let g = gates_deg(-2.6, 0.0, 0.0, 0.0, 0.0, 0.0);
        assert_eq!(grade_from_gates(&g, &[], None, None), PassGrade::NoGrade);
    }

    fn trajectory_point(
        distance_m: f64,
        gs_deviation_deg: f64,
        lineup_deg: f64,
    ) -> TrajectoryDeviation {
        TrajectoryDeviation {
            timestamp_dcs: 0.0,
            distance_m,
            gs_deviation_deg,
            lineup_deg,
            lineup_deviation_m: 0.0,
            track_angle_deg: 0.0,
            alt_m: 0.0,
            bank_deg: 0.0,
            sink_rate_mps: 0.0,
        }
    }

    /// Same as `trajectory_point`, but with an explicit timestamp, for trend tests where the
    /// order and spacing of samples in time is what's being exercised.
    fn trajectory_point_at(
        timestamp_dcs: f64,
        distance_m: f64,
        gs_deviation_deg: f64,
        lineup_deg: f64,
    ) -> TrajectoryDeviation {
        TrajectoryDeviation {
            timestamp_dcs,
            distance_m,
            gs_deviation_deg,
            lineup_deg,
            lineup_deviation_m: 0.0,
            track_angle_deg: 0.0,
            alt_m: 0.0,
            bank_deg: 0.0,
            sink_rate_mps: 0.0,
        }
    }

    #[test]
    fn continuous_excursion_between_two_gates_is_not_missed() {
        // All three gates are clean (well within OK), but the trajectory records a
        // significant 1.2° high excursion spanning two consecutive samples around 700 m,
        // strictly between the 1/2-nm (926 m) and 1/4-nm (463 m) gates, which the three-gate-only
        // computation could never see. The continuous series must still catch it and downgrade
        // the pass, exactly as if a gate had landed on the spike. Two samples (not one) so the
        // A.1 persistence guard (see `continuous_single_frame_spike_is_not_a_false_positive`)
        // confirms this is a real excursion, not an aberrant single frame.
        let g = gates_deg(0.1, 0.1, 0.1, 0.1, 0.1, 0.1);
        let trajectory = [
            trajectory_point(710.0, 1.2, 0.0),
            trajectory_point(700.0, 1.2, 0.0),
        ];
        assert_eq!(grade_from_gates(&g, &[], None, None), PassGrade::Ok);
        assert_eq!(
            grade_from_gates(&g, &trajectory, None, None),
            PassGrade::NoGrade
        );
    }

    #[test]
    fn continuous_single_frame_spike_is_not_a_false_positive() {
        // A.1 robustness guard (piste 6 of the notation work): the exact excursion from
        // `continuous_excursion_between_two_gates_is_not_missed`, but recorded on a single
        // sample only. A lone aberrant telemetry frame must not by itself cap an otherwise
        // clean approach — see `PERSISTENCE_MIN_CONSECUTIVE_SAMPLES`.
        let g = gates_deg(0.1, 0.1, 0.1, 0.1, 0.1, 0.1);
        let trajectory = [trajectory_point(700.0, 1.2, 0.0)];
        assert_eq!(grade_from_gates(&g, &trajectory, None, None), PassGrade::Ok);
    }

    #[test]
    fn continuous_slight_lineup_excursion_is_not_missed() {
        let g = gates_deg(0.1, 0.1, 0.1, 0.1, 0.1, 0.1);
        let trajectory = [
            trajectory_point(710.0, 0.0, 1.5),
            trajectory_point(700.0, 0.0, 1.5),
        ];
        assert_eq!(
            grade_from_gates(&g, &trajectory, None, None),
            PassGrade::OkParentheses
        );
    }

    #[test]
    fn continuous_series_is_the_catobar_episode_source_when_present() {
        // Gates remain coverage evidence, but their duplicate amplitude is not double-counted
        // once the continuous CASE I trajectory is available to build episodes.
        let g = gates_deg(1.2, 0.0, 0.0, 0.0, 0.0, 0.0);
        let trajectory = [trajectory_point(700.0, 0.1, 0.0)];
        assert_eq!(grade_from_gates(&g, &trajectory, None, None), PassGrade::Ok);
    }

    #[test]
    fn dangerously_low_trajectory_sample_inside_quarter_nm_is_cut_even_off_gate() {
        // The exact 1/4-nm gate crossing is clean, but the continuous series shows a brief
        // dip below GS_CUT_LOW_DEG at 400 m (inside the 463 m gate) that recovered before the
        // gate itself was crossed. This is exactly the kind of transient the discrete gate
        // alone would miss.
        let g = gates_deg(0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
        let trajectory = [trajectory_point(400.0, -2.6, 0.0)];
        assert_eq!(
            grade_from_gates(&g, &trajectory, None, None),
            PassGrade::Cut
        );
    }

    #[test]
    fn low_trajectory_sample_outside_quarter_nm_is_not_cut() {
        // Same dangerously-low value, but at 700 m (outside the 1/4-nm gate distance): the
        // Cut rule only ever applied "at the ramp", never earlier in the groove. Two consecutive
        // samples so the A.1 persistence guard confirms the excursion before the general
        // amplitude tier downgrades to NoGrade.
        let g = gates_deg(0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
        let trajectory = [
            trajectory_point(710.0, -2.6, 0.0),
            trajectory_point(700.0, -2.6, 0.0),
        ];
        assert_eq!(
            grade_from_gates(&g, &trajectory, None, None),
            PassGrade::NoGrade
        );
    }

    fn danger_point(distance_m: f64, sink_rate_mps: f64, bank_deg: f64) -> TrajectoryDeviation {
        TrajectoryDeviation {
            timestamp_dcs: 0.0,
            distance_m,
            gs_deviation_deg: 0.0,
            lineup_deg: 0.0,
            lineup_deviation_m: 0.0,
            track_angle_deg: 0.0,
            alt_m: 0.0,
            bank_deg,
            sink_rate_mps,
        }
    }

    #[test]
    fn sustained_excessive_sink_rate_near_the_ramp_is_a_cut() {
        // Otherwise-clean gates and GS/lineup, but the aircraft is descending at
        // SINK_RATE_CUT_MPS (8.0 m/s, roughly double the nominal ~3-4 m/s no-flare CATOBAR
        // approach rate) for DANGER_CUT_MIN_CONSECUTIVE_SAMPLES (3) consecutive samples inside
        // the quarter-NM gate: graded the same as a dangerously low glideslope.
        let g = gates_deg(0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
        let trajectory = [
            danger_point(400.0, 8.0, 0.0),
            danger_point(380.0, 8.5, 0.0),
            danger_point(360.0, 8.2, 0.0),
        ];
        assert_eq!(
            grade_from_gates(&g, &trajectory, None, None),
            PassGrade::Cut
        );
    }

    #[test]
    fn momentary_sink_rate_spike_near_the_ramp_is_not_a_cut() {
        // Same magnitude as the sustained case above, but only two consecutive samples: below
        // DANGER_CUT_MIN_CONSECUTIVE_SAMPLES (3), so a single noisy telemetry bump must not cut
        // an otherwise clean pass.
        let g = gates_deg(0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
        let trajectory = [danger_point(400.0, 8.5, 0.0), danger_point(380.0, 8.5, 0.0)];
        assert_eq!(grade_from_gates(&g, &trajectory, None, None), PassGrade::Ok);
    }

    #[test]
    fn excessive_sink_rate_outside_quarter_nm_is_not_a_cut() {
        // Same sustained excessive sink rate, but entirely outside the quarter-NM danger zone
        // (700 m): the Cut rule only ever applies "at the ramp", same scoping as GS_CUT_LOW_DEG.
        let g = gates_deg(0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
        let trajectory = [
            danger_point(720.0, 9.0, 0.0),
            danger_point(710.0, 9.0, 0.0),
            danger_point(700.0, 9.0, 0.0),
        ];
        assert_eq!(grade_from_gates(&g, &trajectory, None, None), PassGrade::Ok);
    }

    #[test]
    fn sustained_excessive_bank_angle_near_the_ramp_is_a_cut() {
        // Same mechanism as the sink-rate danger cut, but for BANK_ANGLE_CUT_DEG (30 deg,
        // double the CATOBAR groove roll-out "wings level" threshold): a sustained hard bank
        // this close to the ramp is graded as a Cut regardless of otherwise-clean GS/lineup.
        let g = gates_deg(0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
        let trajectory = [
            danger_point(400.0, 0.0, -32.0),
            danger_point(380.0, 0.0, -31.0),
            danger_point(360.0, 0.0, -33.0),
        ];
        assert_eq!(
            grade_from_gates(&g, &trajectory, None, None),
            PassGrade::Cut
        );
    }

    #[test]
    fn ordinary_groove_correction_bank_is_not_a_cut() {
        // 12 deg is a realistic lineup-correction bank, well under BANK_ANGLE_CUT_DEG (30 deg):
        // must never cut an otherwise clean pass.
        let g = gates_deg(0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
        let trajectory = [
            danger_point(400.0, 0.0, 12.0),
            danger_point(380.0, 0.0, 12.0),
            danger_point(360.0, 0.0, 12.0),
        ];
        assert_eq!(grade_from_gates(&g, &trajectory, None, None), PassGrade::Ok);
    }

    #[test]
    fn worsening_inside_the_none_band_does_not_create_an_episode() {
        // Pass B from the design discussion: nickel at the start, drifting to worse (but still
        // within OK amplitude margins) by the end. NATOPS reserves OK for "reasonable deviations
        // WITH GOOD CORRECTIONS" -- a trajectory that is still getting worse this close to
        // touchdown cannot make that claim, even though the raw amplitude alone stayed under the
        // OK threshold throughout.
        let g = gates_deg(0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
        let trajectory = [
            trajectory_point_at(0.0, 900.0, 0.05, 0.0),
            trajectory_point_at(4.0, 500.0, 0.45, 0.0),
        ];
        assert_eq!(grade_from_gates(&g, &trajectory, None, None), PassGrade::Ok);
    }

    #[test]
    fn improving_trend_keeps_an_ok_pass_at_ok() {
        // Pass A from the design discussion: high at the start, corrected to nickel by the end.
        // Trend must never be used to raise a grade, only to hold one back -- so this case,
        // unlike the worsening one, stays a plain Ok.
        let g = gates_deg(0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
        let trajectory = [
            trajectory_point_at(0.0, 900.0, 0.45, 0.0),
            trajectory_point_at(4.0, 500.0, 0.05, 0.0),
        ];
        assert_eq!(grade_from_gates(&g, &trajectory, None, None), PassGrade::Ok);
    }

    #[test]
    fn small_oscillation_is_not_a_worsening_trend() {
        // Ordinary aim-point noise (a few hundredths of a degree) must not be mistaken for a
        // sustained worsening trend.
        let g = gates_deg(0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
        let trajectory = [
            trajectory_point_at(0.0, 900.0, 0.10, 0.0),
            trajectory_point_at(4.0, 500.0, 0.15, 0.0),
        ];
        assert_eq!(grade_from_gates(&g, &trajectory, None, None), PassGrade::Ok);
    }

    #[test]
    fn worsening_inside_the_none_band_remains_noise_even_in_the_last_four_seconds() {
        // Clean from groove entry (t=0) through t=17, then a real worsening drift in the final
        // 4 s (t=17 -> t=21). Comparing only the true first and last samples of the whole
        // approach would dilute this recent drift into an unremarkable 0.35/21 =~ 0.017 deg/s
        // and miss it entirely; restricting the slope to TREND_WINDOW_S correctly surfaces it.
        let g = gates_deg(0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
        let trajectory = [
            trajectory_point_at(0.0, 1300.0, 0.05, 0.0),
            trajectory_point_at(17.0, 700.0, 0.05, 0.0),
            trajectory_point_at(21.0, 500.0, 0.40, 0.0),
        ];
        assert_eq!(grade_from_gates(&g, &trajectory, None, None), PassGrade::Ok);
    }

    #[test]
    fn gate_amplitude_is_not_double_counted_when_continuous_evidence_exists() {
        // Trend is only ever checked at the Ok/(OK) boundary; a pass already at (OK) or worse
        // from amplitude alone is not pushed down an additional tier by a worsening trend.
        let g = gates_deg(0.6, 0.0, 0.0, 0.0, 0.0, 0.0);
        let trajectory = [
            trajectory_point_at(0.0, 900.0, 0.05, 0.0),
            trajectory_point_at(4.0, 500.0, 0.45, 0.0),
        ];
        assert_eq!(grade_from_gates(&g, &trajectory, None, None), PassGrade::Ok);
    }

    #[test]
    fn single_trajectory_sample_has_no_trend() {
        // A single sample cannot express a slope; trend must gracefully report "no signal"
        // rather than panic or divide by zero.
        let g = gates_deg(0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
        let trajectory = [trajectory_point_at(0.0, 700.0, 0.05, 0.0)];
        assert_eq!(grade_from_gates(&g, &trajectory, None, None), PassGrade::Ok);
    }

    #[test]
    fn alternating_corrections_cap_an_otherwise_ok_pass_at_ok_parentheses() {
        // A.4 (NATOPS `OC` -- overcontrolled): the pilot corrects back and forth around the aim
        // point (+0.4/-0.4/+0.4/-0.4 deg), each swing well inside the OK amplitude margin and
        // the net slope across the window near zero -- `trend_worsening` alone would miss this
        // entirely. Two direction reversals within `OSCILLATION_WINDOW_S` is enough to flag it.
        let g = gates_deg(0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
        let trajectory = [
            trajectory_point_at(0.0, 900.0, 0.6, 0.0),
            trajectory_point_at(1.0, 800.0, -0.6, 0.0),
            trajectory_point_at(2.0, 700.0, 0.6, 0.0),
            trajectory_point_at(3.0, 600.0, -0.6, 0.0),
        ];
        assert_eq!(
            grade_from_gates(&g, &trajectory, None, None),
            PassGrade::OkParentheses
        );
    }

    #[test]
    fn a_single_correction_and_return_is_not_yet_an_oscillation() {
        // One reversal (correct, then hold) is just an ordinary correction -- already graded
        // fairly by the amplitude tiers and `trend_worsening`. `OSCILLATION_MIN_REVERSALS`
        // requires at least two reversals before calling it a genuine back-and-forth pattern.
        let g = gates_deg(0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
        let trajectory = [
            trajectory_point_at(0.0, 900.0, 0.1, 0.0),
            trajectory_point_at(1.0, 800.0, 0.4, 0.0),
            trajectory_point_at(2.0, 700.0, 0.1, 0.0),
        ];
        assert_eq!(grade_from_gates(&g, &trajectory, None, None), PassGrade::Ok);
    }

    #[test]
    fn small_lineup_oscillation_below_the_swing_threshold_is_noise() {
        // Alternating but tiny (0.1 deg) swings never reach `OSCILLATION_MIN_SWING_DEG`, so they
        // never register as a leg at all -- ordinary aim-point jitter is not overcontrol.
        let g = gates_deg(0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
        let trajectory = [
            trajectory_point_at(0.0, 900.0, 0.0, 0.1),
            trajectory_point_at(1.0, 800.0, 0.0, -0.1),
            trajectory_point_at(2.0, 700.0, 0.0, 0.1),
            trajectory_point_at(3.0, 600.0, 0.0, -0.1),
        ];
        assert_eq!(grade_from_gates(&g, &trajectory, None, None), PassGrade::Ok);
    }

    #[test]
    fn late_window_gs_deviation_downgrades_an_otherwise_ok_grade_to_no_grade() {
        // 0.9 deg is above LATE_WINDOW_GS_DEG (0.8) but below GS_SIGNIFICANT (1.0), so amplitude
        // alone would only ever grant (OK) here. Happening at 100 m -- inside
        // LATE_WINDOW_DISTANCE_M, with no room left to correct -- caps it at NoGrade instead.
        let g = gates_deg(0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
        let trajectory = [
            trajectory_point(110.0, 0.9, 0.0),
            trajectory_point(100.0, 0.9, 0.0),
        ];
        assert_eq!(
            grade_from_gates(&g, &trajectory, None, None),
            PassGrade::NoGrade
        );
    }

    #[test]
    fn late_window_lineup_deviation_downgrades_to_no_grade() {
        // Same logic, lineup axis: 1.6 deg is above LATE_WINDOW_LU_DEG (1.5) but below
        // LU_MEDIUM (2.0).
        let g = gates_deg(0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
        let trajectory = [
            trajectory_point(110.0, 0.0, 1.6),
            trajectory_point(100.0, 0.0, 1.6),
        ];
        assert_eq!(
            grade_from_gates(&g, &trajectory, None, None),
            PassGrade::NoGrade
        );
    }

    #[test]
    fn the_same_deviation_earlier_in_the_approach_is_only_ok_parentheses() {
        // Identical 0.9 deg GS deviation to the case above, but at 700 m -- well outside
        // LATE_WINDOW_DISTANCE_M. This is exactly the "last moments matter more" effect A.3
        // adds: the same magnitude of error is graded differently depending on how much
        // distance is left to correct it. Two consecutive samples to satisfy the A.1
        // persistence guard.
        let g = gates_deg(0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
        let trajectory = [
            trajectory_point(710.0, 0.9, 0.0),
            trajectory_point(700.0, 0.9, 0.0),
        ];
        assert_eq!(
            grade_from_gates(&g, &trajectory, None, None),
            PassGrade::OkParentheses
        );
    }

    #[test]
    fn ramp_weight_applies_to_every_nonzero_episode_tier() {
        // 0.6 deg at 100 m crosses the general GS_SLIGHT_HIGH tier ((OK)) but not the stricter
        // LATE_WINDOW_GS_DEG (0.8) -- the late-window check must not fire on every deviation
        // found close to the ramp, only ones that cross its own, stricter threshold. Two
        // consecutive samples to satisfy the A.1 persistence guard.
        let g = gates_deg(0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
        let trajectory = [
            trajectory_point(110.0, 0.6, 0.0),
            trajectory_point(100.0, 0.6, 0.0),
        ];
        assert_eq!(
            grade_from_gates(&g, &trajectory, None, None),
            PassGrade::NoGrade
        );
    }

    #[test]
    fn late_window_never_downgrades_a_pass_already_at_no_grade_or_worse() {
        // A pass already at NoGrade from amplitude alone is not further affected; the
        // late-window check only ever holds back Ok/(OK), like trend does for Ok alone.
        let g = gates_deg(1.2, 0.0, 0.0, 0.0, 0.0, 0.0);
        let trajectory = [
            trajectory_point(110.0, 1.2, 0.0),
            trajectory_point(100.0, 1.2, 0.0),
        ];
        assert_eq!(
            grade_from_gates(&g, &trajectory, None, None),
            PassGrade::NoGrade
        );
    }

    #[test]
    fn late_window_never_overrides_cut() {
        // A genuine Cut (dangerously low inside the quarter-NM gate) returns before the
        // late-window check is ever reached; a coexisting late-window-severe sample elsewhere
        // must not somehow soften that to NoGrade.
        let g = gates_deg(0.0, 0.0, 0.0, 0.0, -2.6, 0.0);
        let trajectory = [trajectory_point(100.0, 0.9, 0.0)];
        assert_eq!(
            grade_from_gates(&g, &trajectory, None, None),
            PassGrade::Cut
        );
    }

    #[test]
    fn test_bolter_outcome() {
        let g = gates_deg(0.2, 0.3, 0.1, 0.2, 0.1, 0.1);
        assert_eq!(
            compute_pass_grade_with_reason(&Grading::Bolter, &g, &[], None, None).0,
            PassGrade::Bolter
        );
    }

    #[test]
    fn test_waveoff_outcome() {
        let g = GateDeviations::default();
        assert_eq!(
            compute_pass_grade_with_reason(&Grading::WaveoffUnknown, &g, &[], None, None).0,
            PassGrade::WaveoffUnknown
        );
    }

    #[test]
    fn clean_pass_with_groove_time_in_window_reaches_perfect_regardless_of_wire() {
        // `_OK_` depends only on amplitude (`OK_PERFECT_*`) and groove time
        // (`OK_PERFECT_GROOVE_TIME_MIN_S..=OK_PERFECT_GROOVE_TIME_MAX_S`), never on which wire
        // is caught. The historical "wire 3 + 15-18.99 s" Unicorn coupling this module's own
        // thresholds descend from was deliberately not revived (see docs/GRADING_REFERENCE.md).
        // Wire 3 here plays no special role -- see the next test for wire 4 producing the same
        // result.
        let g = gates_deg(0.1, 0.1, 0.1, 0.1, 0.1, 0.1);
        let grading = Grading::Recovered {
            cable: Some(3),
            cable_estimated: Some(3),
        };
        assert_eq!(
            compute_pass_grade_with_reason(&grading, &g, &[], Some(16.5), None).0,
            PassGrade::Perfect
        );
    }

    #[test]
    fn clean_wire4_pass_also_reaches_perfect() {
        // Same clean amplitude and in-window groove time as the wire-3 case above, but a
        // different wire: proves wire number plays no role at all in `_OK_`, exactly as
        // documented (docs/GRADING_REFERENCE.md: "Groove time and estimated wire cannot produce
        // `_OK_`" refers to the *disabled coupling*, not to groove time or wire individually --
        // groove time alone genuinely does gate `_OK_`, per NAVAIR 00-80T-105 §6.2.4.3).
        let g = gates_deg(0.1, 0.1, 0.1, 0.1, 0.1, 0.1);
        let grading = Grading::Recovered {
            cable: Some(4),
            cable_estimated: Some(4),
        };
        assert_eq!(
            compute_pass_grade_with_reason(&grading, &g, &[], Some(16.5), None).0,
            PassGrade::Perfect
        );
    }

    #[test]
    fn groove_time_exactly_at_either_boundary_still_reaches_perfect() {
        // NAVAIR 00-80T-105 §6.2.4.3's "15 - 18 second groove" is inclusive at both ends in this
        // module's implementation (`OK_PERFECT_GROOVE_TIME_MIN_S..=OK_PERFECT_GROOVE_TIME_MAX_S`).
        let g = gates_deg(0.1, 0.1, 0.1, 0.1, 0.1, 0.1);
        let grading = Grading::Recovered {
            cable: Some(3),
            cable_estimated: Some(3),
        };
        assert_eq!(
            compute_pass_grade_with_reason(&grading, &g, &[], Some(15.0), None).0,
            PassGrade::Perfect
        );
        assert_eq!(
            compute_pass_grade_with_reason(&grading, &g, &[], Some(18.0), None).0,
            PassGrade::Perfect
        );
    }

    #[test]
    fn groove_time_just_outside_either_boundary_denies_perfect() {
        let g = gates_deg(0.1, 0.1, 0.1, 0.1, 0.1, 0.1);
        let grading = Grading::Recovered {
            cable: Some(3),
            cable_estimated: Some(3),
        };
        assert_eq!(
            compute_pass_grade_with_reason(&grading, &g, &[], Some(14.99), None).0,
            PassGrade::Ok
        );
        assert_eq!(
            compute_pass_grade_with_reason(&grading, &g, &[], Some(18.01), None).0,
            PassGrade::Ok
        );
    }

    #[test]
    fn touch_and_go_never_reaches_perfect_even_with_perfect_amplitude_and_groove_time() {
        // docs/GRADING_REFERENCE.md: "A touch-and-go cannot receive `_OK_` or points" -- a
        // touch-and-go is a deliberate hook-up qualification pass, never a full stop, so even a
        // textbook-perfect approach caps at `Ok`, one tier below what a real trap would earn
        // with identical numbers (see `clean_pass_with_groove_time_in_window_reaches_perfect_regardless_of_wire`).
        let g = gates_deg(0.1, 0.1, 0.1, 0.1, 0.1, 0.1);
        let grading = Grading::TouchAndGo {
            cable_estimated: Some(3),
        };
        assert_eq!(
            compute_pass_grade_with_reason(&grading, &g, &[], Some(16.5), None).0,
            PassGrade::Ok
        );
    }

    #[test]
    fn a_single_gate_reading_outside_the_perfect_band_denies_perfect_without_affecting_ok() {
        // Gates are trusted, bracket/skew-validated single points -- unlike the continuous
        // trajectory, a gate reading gets no noise pardon. 0.45 deg is inside the ordinary `Ok`
        // slight-threshold (0.5 deg, so the base tier is unaffected) but outside the tighter
        // `_OK_` band (`OK_PERFECT_GS_HIGH_DEG` = 0.4 deg).
        let g = gates_deg(0.45, 0.1, 0.1, 0.1, 0.1, 0.1);
        let grading = Grading::Recovered {
            cable: Some(3),
            cable_estimated: Some(3),
        };
        assert_eq!(
            compute_pass_grade_with_reason(&grading, &g, &[], Some(16.5), None).0,
            PassGrade::Ok
        );
    }

    #[test]
    fn a_single_isolated_trajectory_spike_outside_the_perfect_band_is_pardoned() {
        // Same noise pardon requested for `_OK_` as everywhere else in this module: one
        // non-repeating frame outside the tight `_OK_` band, surrounded by perfectly clean
        // samples, must not by itself cost a real perfect pass its `_OK_`.
        let g = gates_deg(0.1, 0.1, 0.1, 0.1, 0.1, 0.1);
        let grading = Grading::Recovered {
            cable: Some(3),
            cable_estimated: Some(3),
        };
        let trajectory = [
            trajectory_point(700.0, 0.1, 0.1),
            trajectory_point(650.0, 0.45, 0.1), // isolated: outside _OK_ band, inside Ok band
            trajectory_point(600.0, 0.1, 0.1),
        ];
        assert_eq!(
            compute_pass_grade_with_reason(&grading, &g, &trajectory, Some(16.5), None).0,
            PassGrade::Perfect
        );
    }

    #[test]
    fn two_consecutive_trajectory_samples_outside_the_perfect_band_deny_perfect_but_not_ok() {
        // Same magnitude as the isolated-spike test above, but sustained over
        // PERSISTENCE_MIN_CONSECUTIVE_SAMPLES (2) samples: no longer pardoned for `_OK_`. Still
        // inside the ordinary `Ok` slight threshold (0.5 deg), so the base tier is unaffected --
        // only `_OK_` is denied.
        let g = gates_deg(0.1, 0.1, 0.1, 0.1, 0.1, 0.1);
        let grading = Grading::Recovered {
            cable: Some(3),
            cable_estimated: Some(3),
        };
        let trajectory = [
            trajectory_point(700.0, 0.1, 0.1),
            trajectory_point(650.0, 0.45, 0.1),
            trajectory_point(600.0, 0.45, 0.1),
            trajectory_point(550.0, 0.1, 0.1),
        ];
        assert_eq!(
            compute_pass_grade_with_reason(&grading, &g, &trajectory, Some(16.5), None).0,
            PassGrade::Ok
        );
    }

    #[test]
    fn clean_short_groove_remains_ok() {
        // Wire 3, no deviations, but groove time too short → OK.
        let g = gates_deg(0.1, 0.1, 0.1, 0.1, 0.1, 0.1);
        let grading = Grading::Recovered {
            cable: Some(3),
            cable_estimated: Some(3),
        };
        assert_eq!(
            compute_pass_grade_with_reason(&grading, &g, &[], Some(12.0), None).0,
            PassGrade::Ok
        );
    }

    #[test]
    fn clean_long_groove_remains_ok() {
        // Wire 3, no deviations, but groove time too long → OK.
        let g = gates_deg(0.1, 0.1, 0.1, 0.1, 0.1, 0.1);
        let grading = Grading::Recovered {
            cable: Some(3),
            cable_estimated: Some(3),
        };
        assert_eq!(
            compute_pass_grade_with_reason(&grading, &g, &[], Some(22.0), None).0,
            PassGrade::Ok
        );
    }

    #[test]
    fn slight_deviation_remains_ok_parentheses() {
        // Slight deviation keeps the (OK) project grade.
        let g = gates_deg(0.9, 0.1, 0.1, 0.1, 0.1, 0.1);
        let grading = Grading::Recovered {
            cable: Some(3),
            cable_estimated: Some(3),
        };
        assert_eq!(
            compute_pass_grade_with_reason(&grading, &g, &[], Some(16.5), None).0,
            PassGrade::OkParentheses
        );
    }

    #[test]
    fn clean_pass_without_groove_time_remains_ok() {
        // Missing groove time does not affect this clean OK project grade.
        let g = gates_deg(0.1, 0.1, 0.1, 0.1, 0.1, 0.1);
        let grading = Grading::Recovered {
            cable: Some(3),
            cable_estimated: Some(3),
        };
        assert_eq!(
            compute_pass_grade_with_reason(&grading, &g, &[], None, None).0,
            PassGrade::Ok
        );
    }

    #[test]
    fn test_touch_and_go_keeps_the_measured_approach_grade() {
        let g = gates_deg(0.6, 0.0, 0.0, 0.0, 0.0, 0.0);
        let grading = Grading::TouchAndGo {
            cable_estimated: Some(3),
        };

        assert_eq!(
            compute_pass_grade_with_reason(&grading, &g, &[], Some(16.5), None).0,
            PassGrade::OkParentheses
        );
    }

    #[test]
    fn test_touch_and_go_does_not_require_an_estimated_wire() {
        let g = gates_deg(0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
        let grading = Grading::TouchAndGo {
            cable_estimated: None,
        };

        assert_eq!(
            compute_pass_grade_with_reason(&grading, &g, &[], Some(16.5), None).0,
            PassGrade::Ok
        );
    }

    #[test]
    fn test_points_bolter() {
        assert_eq!(PassGrade::Bolter.points(), Some(2.5));
    }

    #[test]
    fn test_points_waveoff() {
        assert_eq!(PassGrade::WaveoffUnknown.points(), None);
    }

    #[test]
    fn test_points_no_grade() {
        assert_eq!(PassGrade::NoGrade.points(), Some(2.0));
    }

    #[test]
    fn test_vstol_spot_grade_boundaries() {
        assert_eq!(SpotGrade::from_distance_m(0.99), SpotGrade::A);
        assert_eq!(SpotGrade::from_distance_m(1.0), SpotGrade::B);
        assert_eq!(SpotGrade::from_distance_m(2.99), SpotGrade::B);
        assert_eq!(SpotGrade::from_distance_m(3.0), SpotGrade::C);
        assert_eq!(SpotGrade::from_distance_m(4.99), SpotGrade::C);
        assert_eq!(SpotGrade::from_distance_m(5.0), SpotGrade::D);
    }

    #[test]
    fn test_vstol_spot_grade_labels_and_bonus_points() {
        let cases = [
            (SpotGrade::A, "A", 1.0),
            (SpotGrade::B, "B", 0.75),
            (SpotGrade::C, "C", 0.5),
            (SpotGrade::D, "D", 0.0),
        ];

        for (spot_grade, expected_label, expected_bonus) in cases {
            assert_eq!(spot_grade.label(), expected_label);
            assert_eq!(spot_grade.bonus_points(), expected_bonus);
        }
    }

    #[test]
    fn test_vstol_spot_bonus_maps_to_display_grades() {
        assert_eq!(
            compute_vstol_final_grade_from_points(PassGrade::Ok.points().unwrap(), SpotGrade::A),
            (PassGrade::Ok, 5.0)
        );
        assert_eq!(
            compute_vstol_final_grade_from_points(PassGrade::Ok.points().unwrap(), SpotGrade::B),
            (PassGrade::Ok, 4.75)
        );
        assert_eq!(
            compute_vstol_final_grade_from_points(
                PassGrade::OkParentheses.points().unwrap(),
                SpotGrade::A
            ),
            (PassGrade::Ok, 4.0)
        );
        assert_eq!(
            compute_vstol_final_grade_from_points(
                PassGrade::NoGrade.points().unwrap(),
                SpotGrade::C
            ),
            (PassGrade::NoGrade, 2.5)
        );
    }

    #[test]
    fn test_vstol_approach_with_all_ok_gates_is_ok() {
        let grading = Grading::Recovered {
            cable: None,
            cable_estimated: None,
        };
        let gates = gates_deg(0.2, 0.3, -0.2, 0.4, 0.1, 0.2);

        let (grade, points) = compute_vstol_approach_grade_points(&grading, &gates, &[]);

        assert_eq!(grade, PassGrade::Ok);
        assert!((points.unwrap() - 4.0).abs() < 1e-9);
    }

    #[test]
    fn test_vstol_approach_averages_one_significant_gate() {
        let grading = Grading::Recovered {
            cable: None,
            cable_estimated: None,
        };
        // 3/4 nm = -- (2.0), other gates = OK (4.0): average = 10 / 3.
        let gates = gates_deg(1.0, 0.2, 0.1, 0.2, 0.1, 0.1);

        let (grade, points) = compute_vstol_approach_grade_points(&grading, &gates, &[]);

        assert_eq!(grade, PassGrade::OkParentheses);
        assert!((points.unwrap() - (10.0 / 3.0)).abs() < 1e-9);
    }

    #[test]
    fn test_vstol_approach_averages_one_slight_gate() {
        let grading = Grading::Recovered {
            cable: None,
            cable_estimated: None,
        };
        // 3/4 nm = (OK) (3.0), other gates = OK (4.0): average = 11 / 3.
        let gates = gates_deg(0.5, 0.2, 0.1, 0.2, 0.1, 0.1);

        let (grade, points) = compute_vstol_approach_grade_points(&grading, &gates, &[]);

        assert_eq!(grade, PassGrade::Ok);
        assert!((points.unwrap() - (11.0 / 3.0)).abs() < 1e-9);
    }

    #[test]
    fn test_vstol_approach_points_map_at_grade_midpoints() {
        let cases = [
            (3.5, PassGrade::Ok),
            (3.49, PassGrade::OkParentheses),
            (2.5, PassGrade::OkParentheses),
            (2.49, PassGrade::NoGrade),
            (1.0, PassGrade::NoGrade),
            (0.99, PassGrade::Cut),
        ];

        for (points, expected_grade) in cases {
            assert_eq!(map_vstol_approach_points_to_grade(points), expected_grade);
        }
    }

    #[test]
    fn test_vstol_final_grade_uses_fractional_approach_points() {
        let (grade, points) = compute_vstol_final_grade_from_points(10.0 / 3.0, SpotGrade::B);

        assert_eq!(grade, PassGrade::Ok);
        assert!((points - ((10.0 / 3.0) + 0.75)).abs() < 1e-9);
    }

    #[test]
    fn test_vstol_non_recovery_outcomes_never_gain_points_from_incomplete_gates() {
        let gates = GateDeviations::default();
        let cases = [
            (Grading::Unknown, PassGrade::Incomplete, None),
            (Grading::WaveoffUnknown, PassGrade::WaveoffUnknown, None),
            (Grading::Bolter, PassGrade::Incomplete, None),
        ];

        for (grading, expected_grade, expected_points) in cases {
            assert_eq!(
                compute_vstol_approach_grade_points(&grading, &gates, &[]),
                (expected_grade, expected_points)
            );
        }
    }

    #[test]
    fn describe_measured_deviations_is_empty_when_every_gate_and_trajectory_sample_is_clean() {
        let gates = gates_deg(0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
        let trajectory = [trajectory_point(200.0, 0.1, 0.1)];
        assert_eq!(describe_measured_deviations(&gates, &trajectory), "");
    }

    #[test]
    fn describe_measured_deviations_reports_high_and_left_by_side() {
        // GS_SLIGHT_HIGH/LOW = 0.5, LU_SLIGHT = 1.0.
        let gates = gates_deg(0.7, -1.5, 0.0, 0.0, -0.6, 0.0);
        let description = describe_measured_deviations(&gates, &[]);
        assert!(
            description.starts_with("High on glideslope at 3/4 NM (+0.7°)"),
            "{description}"
        );
        assert!(
            description.contains("left of centerline at 3/4 NM (-1.5°)"),
            "{description}"
        );
        assert!(
            description.contains("low on glideslope at 1/4 NM (-0.6°)"),
            "{description}"
        );
    }

    #[test]
    fn describe_measured_deviations_reports_right_of_centerline() {
        let gates = gates_deg(0.0, 1.4, 0.0, 0.0, 0.0, 0.0);
        let description = describe_measured_deviations(&gates, &[]);
        assert_eq!(description, "Right of centerline at 3/4 NM (+1.4°)");
    }

    #[test]
    fn describe_measured_deviations_reports_a_late_window_excursion() {
        let gates = gates_deg(0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
        // LATE_WINDOW_LU_DEG = 1.5, LATE_WINDOW_DISTANCE_M = 150.
        let trajectory = [
            trajectory_point(200.0, 0.0, 0.0),
            trajectory_point(100.0, 0.0, 1.8),
        ];
        let description = describe_measured_deviations(&gates, &trajectory);
        assert_eq!(
            description,
            "Still off in close (100 m out: GS +0.0°, lineup +1.8°)"
        );
    }

    // ── `_with_reason` variants: Discord "Why This Grade" field ────────────────────────────

    #[test]
    fn reason_cut_from_quarter_nm_gate_glideslope() {
        // GS_CUT_LOW_DEG = -2.5.
        let g = gates_deg(0.0, 0.0, 0.0, 0.0, -2.8, 0.0);
        let (grade, reason) = grade_from_gates_with_reason(&g, &[], None, None);
        assert_eq!(grade, PassGrade::Cut);
        assert_eq!(
            reason,
            "C: glideslope 2.8° low at the 1/4 NM gate — dangerously low this close to the ship."
        );
    }

    #[test]
    fn reason_cut_from_sustained_sink_rate() {
        // SINK_RATE_CUT_MPS = 8.0, DANGER_CUT_MIN_CONSECUTIVE_SAMPLES = 3.
        let g = gates_deg(0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
        let mut trajectory = vec![
            trajectory_point(400.0, 8.4, 0.0),
            trajectory_point(400.0, 8.4, 0.0),
            trajectory_point(400.0, 8.4, 0.0),
        ];
        for d in &mut trajectory {
            d.distance_m = 300.0;
            d.sink_rate_mps = 8.4;
        }
        let (grade, reason) = grade_from_gates_with_reason(&g, &trajectory, None, None);
        assert_eq!(grade, PassGrade::Cut);
        assert_eq!(
            reason,
            "C: sink rate 8.4 m/s sustained inside 1/4 NM — descending too fast this close to the ramp."
        );
    }

    #[test]
    fn reason_cut_from_sustained_bank_angle() {
        // BANK_ANGLE_CUT_DEG = 30.0, DANGER_CUT_MIN_CONSECUTIVE_SAMPLES = 3.
        let g = gates_deg(0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
        let mut trajectory = vec![
            trajectory_point(300.0, 0.0, 0.0),
            trajectory_point(300.0, 0.0, 0.0),
            trajectory_point(300.0, 0.0, 0.0),
        ];
        for d in &mut trajectory {
            d.bank_deg = -33.0;
        }
        let (grade, reason) = grade_from_gates_with_reason(&g, &trajectory, None, None);
        assert_eq!(grade, PassGrade::Cut);
        assert_eq!(
            reason,
            "C: bank angle 33° sustained inside 1/4 NM — too steep a bank this close to the ramp."
        );
    }

    #[test]
    fn reason_nograde_from_significant_glideslope() {
        // GS_SIGNIFICANT = 1.0.
        let g = gates_deg(0.0, 0.0, 0.0, 0.0, 1.4, 0.0);
        let (grade, reason) = grade_from_gates_with_reason(&g, &[], None, None);
        assert_eq!(grade, PassGrade::NoGrade);
        assert_eq!(reason, "--: glideslope 1.4° high — well outside tolerance.");
    }

    #[test]
    fn reason_nograde_from_medium_lineup() {
        // LU_MEDIUM = 2.0.
        let g = gates_deg(0.0, 0.0, 0.0, 2.3, 0.0, 0.0);
        let (grade, reason) = grade_from_gates_with_reason(&g, &[], None, None);
        assert_eq!(grade, PassGrade::NoGrade);
        assert_eq!(
            reason,
            "--: lineup off by 2.3° — more than double what OK allows."
        );
    }

    #[test]
    fn reason_nograde_from_late_window() {
        // LATE_WINDOW_LU_DEG = 1.5, LATE_WINDOW_DISTANCE_M = 150.
        let g = gates_deg(0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
        let trajectory = [
            trajectory_point(110.0, 0.0, 1.6),
            trajectory_point(100.0, 0.0, 1.6),
        ];
        let (grade, reason) = grade_from_gates_with_reason(&g, &trajectory, None, None);
        assert_eq!(grade, PassGrade::NoGrade);
        assert_eq!(
            reason,
            "--: lineup léger en RAMP, sans retour stable vers la cible après le pic."
        );
    }

    #[test]
    fn reason_okparentheses_from_slight_glideslope() {
        // GS_SLIGHT_HIGH = 0.5.
        let g = gates_deg(0.0, 0.0, 0.0, 0.0, 0.6, 0.0);
        let (grade, reason) = grade_from_gates_with_reason(&g, &[], None, None);
        assert_eq!(grade, PassGrade::OkParentheses);
        assert_eq!(
            reason,
            "(OK): drifted 0.6° high on glideslope — OK needs better than 0.5°."
        );
    }

    #[test]
    fn reason_does_not_invent_an_episode_inside_the_none_band() {
        let g = gates_deg(0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
        // TREND_WORSENING_DEG_PER_S = 0.075, over TREND_WINDOW_S = 4s: 0.0 -> 0.4 deg lineup.
        let trajectory = [
            trajectory_point_at(0.0, 300.0, 0.0, 0.0),
            trajectory_point_at(4.0, 250.0, 0.0, 0.4),
        ];
        let (grade, reason) = grade_from_gates_with_reason(&g, &trajectory, None, None);
        assert_eq!(grade, PassGrade::Ok);
        assert_eq!(reason, "OK: trajectoire stable, sans épisode significatif.");
    }

    #[test]
    fn reason_okparentheses_from_oscillation() {
        let g = gates_deg(0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
        // OSCILLATION_MIN_REVERSALS = 2, OSCILLATION_MIN_SWING_DEG = 0.3.
        let trajectory = [
            trajectory_point_at(0.0, 300.0, 0.0, 1.2),
            trajectory_point_at(1.0, 280.0, 0.0, -1.2),
            trajectory_point_at(2.0, 260.0, 0.0, 1.2),
            trajectory_point_at(3.0, 240.0, 0.0, -1.2),
        ];
        let (grade, reason) = grade_from_gates_with_reason(&g, &trajectory, None, None);
        assert_eq!(grade, PassGrade::NoGrade);
        assert_eq!(
            reason,
            "--: lineup léger en IN CLOSE, sans retour stable vers la cible après le pic."
        );
    }

    #[test]
    fn reason_ok_when_everything_is_clean() {
        let g = gates_deg(0.1, 0.1, 0.1, 0.1, 0.1, 0.1);
        let (grade, reason) = grade_from_gates_with_reason(&g, &[], None, None);
        assert_eq!(grade, PassGrade::Ok);
        assert_eq!(
            reason,
            "OK: within tolerance on every gate and the continuous approach."
        );
    }

    #[test]
    fn reason_perfect_when_amplitude_and_groove_time_both_qualify() {
        let g = gates_deg(0.1, 0.1, 0.1, 0.1, 0.1, 0.1);
        let (grade, reason) = grade_from_gates_with_reason(&g, &[], Some(16.5), None);
        assert_eq!(grade, PassGrade::Perfect);
        assert_eq!(
            reason,
            "_OK_: textbook pass on every gate and the continuous approach, groove time 16.5 s."
        );
    }

    #[test]
    fn reason_touch_and_go_caps_perfect_to_ok_with_its_own_message() {
        let g = gates_deg(0.1, 0.1, 0.1, 0.1, 0.1, 0.1);
        let grading = Grading::TouchAndGo {
            cable_estimated: None,
        };
        let (grade, reason) = compute_pass_grade_with_reason(&grading, &g, &[], Some(16.5), None);
        assert_eq!(grade, PassGrade::Ok);
        assert_eq!(
            reason,
            "OK: textbook approach, but capped one tier down — a touch-and-go can't receive a perfect pass."
        );
    }

    #[test]
    fn reason_bolter_message() {
        let g = gates_deg(0.2, 0.3, 0.1, 0.2, 0.1, 0.1);
        let (grade, reason) = compute_pass_grade_with_reason(&Grading::Bolter, &g, &[], None, None);
        assert_eq!(grade, PassGrade::Bolter);
        assert_eq!(
            reason,
            "B: hook touched down but didn't catch a wire; the approach itself was on track."
        );
    }

    #[test]
    fn reason_waveoff_message() {
        let g = GateDeviations::default();
        let (grade, reason) =
            compute_pass_grade_with_reason(&Grading::WaveoffUnknown, &g, &[], None, None);
        assert_eq!(grade, PassGrade::WaveoffUnknown);
        assert_eq!(
            reason,
            "WO?: went around; can't tell from the data who or what caused it."
        );
    }

    #[test]
    fn reason_incomplete_from_insufficient_gates() {
        let g = GateDeviations::default();
        let grading = Grading::Recovered {
            cable: Some(3),
            cable_estimated: Some(3),
        };
        let (grade, reason) = compute_pass_grade_with_reason(&grading, &g, &[], None, None);
        assert_eq!(grade, PassGrade::Incomplete);
        assert_eq!(
            reason,
            "Grading unavailable: required gate coverage was not captured in valid chronological brackets. A positioning/detection limitation, not a pilot failure."
        );
    }

    fn observations(
        values: &[(f64, f64, f64)],
        severity: fn(f64) -> EpisodeSeverity,
    ) -> Vec<AxisObservation> {
        values
            .iter()
            .map(|&(time, distance_m, value)| AxisObservation {
                time,
                distance_m,
                value,
                normalized_error: value,
                severity: severity(value),
                classification: if value >= 0.0 { "positive" } else { "negative" },
            })
            .collect()
    }

    #[test]
    fn identical_small_error_is_weighted_in_each_of_the_four_zones() {
        let cases = [
            (1_100.0, ApproachZone::Start, 2.0),
            (700.0, ApproachZone::Middle, 2.4),
            (300.0, ApproachZone::InClose, 3.0),
            (100.0, ApproachZone::Ramp, 4.0),
        ];
        for (distance, zone, effective) in cases {
            let samples = observations(
                &[(0.0, distance, 0.6), (1.0, distance - 1.0, 0.6)],
                gs_severity,
            );
            let episodes = build_axis_episodes(GradingAxis::Glideslope, &samples, true, None);
            assert_eq!(episodes.len(), 1);
            assert_eq!(episodes[0].most_severe_zone, zone);
            assert!((episodes[0].effective_severity - effective).abs() < 1e-9);
        }
    }

    #[test]
    fn good_average_and_poor_corrections_adjust_one_level_only() {
        let good = observations(
            &[
                (0.0, 1_200.0, 1.2),
                (0.5, 1_150.0, 0.8),
                (1.0, 1_100.0, 0.2),
                (1.5, 1_050.0, 0.1),
            ],
            gs_severity,
        );
        let average = observations(
            &[
                (0.0, 1_200.0, 1.2),
                (3.1, 1_100.0, 0.8),
                (4.0, 1_050.0, 0.2),
                (5.0, 1_000.0, 0.1),
            ],
            gs_severity,
        );
        let poor = observations(
            &[
                (0.0, 1_200.0, 1.2),
                (1.0, 1_100.0, 1.2),
                (2.0, 1_000.0, 1.2),
            ],
            gs_severity,
        );
        let episode = |samples: &[AxisObservation]| {
            build_axis_episodes(GradingAxis::Glideslope, samples, true, None).remove(0)
        };
        let good = episode(&good);
        let average = episode(&average);
        let poor = episode(&poor);
        assert_eq!(good.correction, CorrectionQuality::Good);
        assert_eq!(good.corrected_severity, EpisodeSeverity::Small);
        assert_eq!(grade_from_episode_set(&[good]).0, PassGrade::Ok);
        assert_eq!(average.correction, CorrectionQuality::Average);
        assert_eq!(average.corrected_severity, EpisodeSeverity::Medium);
        assert_eq!(
            grade_from_episode_set(&[average]).0,
            PassGrade::OkParentheses
        );
        assert_eq!(poor.correction, CorrectionQuality::Poor);
        assert_eq!(poor.corrected_severity, EpisodeSeverity::Large);
        assert_eq!(grade_from_episode_set(&[poor]).0, PassGrade::NoGrade);
    }

    #[test]
    fn stagnation_aggravation_and_oscillation_are_poor_corrections() {
        let cases = [
            observations(&[(0.0, 1_200.0, 0.6), (1.0, 1_100.0, 0.6)], gs_severity),
            observations(&[(0.0, 1_200.0, 0.6), (1.0, 1_100.0, 1.2)], gs_severity),
            observations(
                &[
                    (0.0, 1_200.0, 0.6),
                    (1.0, 1_100.0, -0.6),
                    (2.0, 1_000.0, 0.6),
                    (3.0, 950.0, -0.6),
                ],
                gs_severity,
            ),
        ];
        for samples in cases {
            let episode =
                build_axis_episodes(GradingAxis::Glideslope, &samples, true, None).remove(0);
            assert_eq!(episode.correction, CorrectionQuality::Poor);
        }
    }

    #[test]
    fn isolated_noise_is_ignored_and_multiple_axes_are_not_summed() {
        let isolated = observations(&[(0.0, 1_200.0, 1.2)], gs_severity);
        assert!(build_axis_episodes(GradingAxis::Glideslope, &isolated, true, None).is_empty());

        let gs = observations(&[(0.0, 1_200.0, 0.6), (1.0, 1_100.0, 0.6)], gs_severity);
        let lu = observations(&[(0.0, 1_200.0, 1.2), (1.0, 1_100.0, 1.2)], lineup_severity);
        let mut episodes = build_axis_episodes(GradingAxis::Glideslope, &gs, true, None);
        episodes.extend(build_axis_episodes(GradingAxis::Lineup, &lu, true, None));
        assert_eq!(
            grade_from_episode_set(&episodes).0,
            PassGrade::OkParentheses
        );
    }

    #[test]
    fn aircraft_specific_aoa_tables_feed_episodes_and_unreliable_aoa_never_penalizes() {
        let trajectory = [
            trajectory_point_at(0.0, 1_200.0, 0.0, 0.0),
            trajectory_point_at(2.0, 1_000.0, 0.0, 0.0),
        ];
        for (aircraft, aoa) in [("FA-18C_hornet", 7.0), ("F-14B", 10.0), ("T-45", 6.2)] {
            let datums = [
                Datum {
                    time: 0.0,
                    x: 1_200.0,
                    aoa,
                    ..Datum::default()
                },
                Datum {
                    time: 1.0,
                    x: 1_100.0,
                    aoa,
                    ..Datum::default()
                },
            ];
            let plane = AirplaneInfo::by_type(aircraft).unwrap();
            let episodes = classify_catobar_episodes(&trajectory, &datums, Some(plane), true);
            let aoa_episode = episodes
                .iter()
                .find(|episode| episode.axis == GradingAxis::Aoa)
                .unwrap();
            assert_eq!(aoa_episode.maximum_severity, EpisodeSeverity::Small);
            assert!(aoa_episode.affects_grade);

            let diagnostic = classify_catobar_episodes(&trajectory, &datums, Some(plane), false);
            let aoa_episode = diagnostic
                .iter()
                .find(|episode| episode.axis == GradingAxis::Aoa)
                .unwrap();
            assert!(!aoa_episode.affects_grade);
            assert_eq!(aoa_episode.effective_severity, 0.0);
            assert_eq!(grade_from_episode_set(&diagnostic).0, PassGrade::Ok);
        }
    }

    #[test]
    fn safety_cut_remains_prioritary_and_cannot_be_corrected_away() {
        let gates = gates_deg(0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
        let trajectory = [trajectory_point_at(0.0, 300.0, -2.6, 0.0)];
        let grading = Grading::Recovered {
            cable: Some(3),
            cable_estimated: None,
        };
        let assessment = compute_catobar_assessment(CatobarEvidence {
            grading: &grading,
            gates: &gates,
            trajectory: &trajectory,
            datums: &[],
            plane_info: AirplaneInfo::by_type("FA-18C_hornet").unwrap(),
            aoa_reliable: false,
            groove_time_secs: Some(16.0),
            groove_entry_time: None,
        });
        assert_eq!(assessment.grade, PassGrade::Cut);
    }

    #[test]
    fn production_catobar_path_preserves_perfect_tg_bolter_and_waveoff_outcomes() {
        let gates = gates_deg(0.1, 0.1, 0.1, 0.1, 0.1, 0.1);
        let trajectory = [
            trajectory_point_at(0.0, 1_100.0, 0.1, 0.1),
            trajectory_point_at(1.0, 900.0, 0.1, 0.1),
        ];
        let plane = AirplaneInfo::by_type("FA-18C_hornet").unwrap();
        let assess = |grading: &Grading| {
            compute_catobar_assessment(CatobarEvidence {
                grading,
                gates: &gates,
                trajectory: &trajectory,
                datums: &[],
                plane_info: plane,
                aoa_reliable: false,
                groove_time_secs: Some(16.0),
                groove_entry_time: None,
            })
            .grade
        };
        assert_eq!(
            assess(&Grading::Recovered {
                cable: Some(3),
                cable_estimated: None,
            }),
            PassGrade::Perfect
        );
        assert_eq!(
            assess(&Grading::TouchAndGo {
                cable_estimated: None,
            }),
            PassGrade::Ok
        );
        assert_eq!(assess(&Grading::Bolter), PassGrade::Bolter);
        assert_eq!(assess(&Grading::WaveoffUnknown), PassGrade::WaveoffUnknown);
    }

    #[test]
    fn episode_json_is_additive_and_auditable() {
        let samples = observations(&[(0.0, 700.0, 1.2), (0.1, 690.0, 1.2)], gs_severity);
        let episode = build_axis_episodes(GradingAxis::Glideslope, &samples, true, None).remove(0);
        let json = serde_json::to_value(episode).unwrap();
        assert_eq!(json["axis"], "glideslope");
        assert_eq!(json["most_severe_zone"], "middle");
        assert_eq!(json["correction"], "poor");
        assert_eq!(json["affects_grade"], true);
        assert!(json.get("effective_severity").is_some());
        assert!(json.get("peak_classification").is_some());
        assert!(json.get("peak_at_dcs").is_some());
        assert!(json.get("peak_zone").is_some());
        assert!(json.get("peak_normalized_error").is_some());
        assert!(json.get("stabilization_samples").is_some());
        assert!(json.get("correction_reason").is_some());
    }

    #[test]
    fn initial_aggravation_can_be_good_when_post_peak_recovery_is_fast_and_stable() {
        let samples = observations(
            &[
                (0.0, 1_200.0, 0.6),
                (0.5, 1_150.0, 1.4),
                (1.0, 1_100.0, 0.8),
                (1.5, 1_050.0, 0.2),
                (2.0, 1_000.0, 0.1),
            ],
            gs_severity,
        );
        let episode = build_axis_episodes(GradingAxis::Glideslope, &samples, true, None).remove(0);
        assert_eq!(episode.peak_at_dcs, 0.5);
        assert_eq!(episode.correction, CorrectionQuality::Good);
        assert_eq!(episode.first_durable_improvement_delay_s, Some(0.5));
        assert_eq!(episode.return_to_none_delay_s, Some(1.0));
    }

    #[test]
    fn zone_deadlines_are_measured_from_the_peak() {
        for (distance, deadline) in [(1_100.0, 3.0), (700.0, 2.5), (300.0, 1.5), (100.0, 0.75)] {
            let good = observations(
                &[
                    (0.0, distance, 1.2),
                    (deadline, distance - 1.0, 0.8),
                    (deadline + 0.1, distance - 2.0, 0.8),
                ],
                gs_severity,
            );
            assert_eq!(
                build_axis_episodes(GradingAxis::Glideslope, &good, true, None)[0].correction,
                CorrectionQuality::Good
            );
            let late = observations(
                &[
                    (0.0, distance, 1.2),
                    (deadline + 0.01, distance - 1.0, 0.8),
                    (deadline + 0.11, distance - 2.0, 0.8),
                ],
                gs_severity,
            );
            assert_eq!(
                build_axis_episodes(GradingAxis::Glideslope, &late, true, None)[0].correction,
                CorrectionQuality::Average
            );
        }
    }

    #[test]
    fn peak_zone_not_later_completion_zone_sets_weight_but_later_higher_peak_replaces_it() {
        let corrected_across_zone = observations(
            &[(0.0, 500.0, 1.2), (0.5, 450.0, 0.8), (1.0, 400.0, 0.8)],
            gs_severity,
        );
        let episode =
            build_axis_episodes(GradingAxis::Glideslope, &corrected_across_zone, true, None)[0]
                .clone();
        assert_eq!(episode.peak_zone, ApproachZone::Middle);
        assert_eq!(episode.zone_weight, 1.2);

        let later_peak = observations(
            &[(0.0, 500.0, 1.2), (0.5, 400.0, 1.8), (1.0, 350.0, 0.8)],
            gs_severity,
        );
        assert_eq!(
            build_axis_episodes(GradingAxis::Glideslope, &later_peak, true, None)[0].peak_zone,
            ApproachZone::InClose
        );
    }

    #[test]
    fn isolated_reversal_is_average_but_two_significant_reversals_are_poor() {
        let one = observations(
            &[(0.0, 700.0, 1.4), (0.5, 650.0, 0.8), (1.0, 600.0, 1.2)],
            gs_severity,
        );
        assert_eq!(
            build_axis_episodes(GradingAxis::Glideslope, &one, true, None)[0].correction,
            CorrectionQuality::Average
        );
        let two = observations(
            &[
                (0.0, 700.0, 1.4),
                (0.5, 650.0, 0.8),
                (1.0, 600.0, 1.2),
                (1.5, 550.0, 0.8),
            ],
            gs_severity,
        );
        let episode = &build_axis_episodes(GradingAxis::Glideslope, &two, true, None)[0];
        assert!(episode.oscillation_reversals >= 2);
        assert_eq!(episode.correction, CorrectionQuality::Poor);
    }

    #[test]
    fn ramp_requires_post_peak_stabilization_before_the_trajectory_ends() {
        let insufficient = observations(&[(0.0, 100.0, 1.2), (0.5, 80.0, 0.8)], gs_severity);
        assert_eq!(
            build_axis_episodes(GradingAxis::Glideslope, &insufficient, true, None)[0].correction,
            CorrectionQuality::Poor
        );
        let complete = observations(
            &[(0.0, 100.0, 1.2), (0.5, 80.0, 0.8), (0.6, 70.0, 0.8)],
            gs_severity,
        );
        assert_eq!(
            build_axis_episodes(GradingAxis::Glideslope, &complete, true, None)[0].correction,
            CorrectionQuality::Good
        );
    }

    #[test]
    fn aoa_normalization_tracks_fast_and_slow_toward_each_existing_onspeed_table() {
        for aircraft in ["FA-18C_hornet", "F-14B", "T-45"] {
            let plane = AirplaneInfo::by_type(aircraft).unwrap();
            let mut fast = None;
            let mut slow = None;
            for step in -400..=400 {
                let aoa = f64::from(step) / 10.0;
                match (plane.aoa_rating)(aoa) {
                    Aoa::SlightlyFast => fast = Some(aoa),
                    Aoa::SlightlySlow if slow.is_none() => slow = Some(aoa),
                    _ => {}
                }
            }
            let fast = fast.unwrap();
            let slow = slow.unwrap();
            assert!(normalized_aoa_error(plane, fast).unwrap() < 0.0);
            assert!(normalized_aoa_error(plane, slow).unwrap() > 0.0);
            let toward_fast = normalized_aoa_error(plane, fast + 0.1).unwrap().abs();
            let toward_slow = normalized_aoa_error(plane, slow - 0.1).unwrap().abs();
            assert!(toward_fast < normalized_aoa_error(plane, fast).unwrap().abs());
            assert!(toward_slow < normalized_aoa_error(plane, slow).unwrap().abs());
        }
    }
}
