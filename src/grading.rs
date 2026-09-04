use crate::track::{GateDeviations, Grading, TrajectoryDeviation};

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
    /// Perfect pass (official `_OK_` symbol). No automatic rule currently emits it.
    #[expect(
        dead_code,
        reason = "reserved for an explicit official/manual _OK_ grade"
    )]
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
pub fn compute_pass_grade(
    grading: &Grading,
    gates: &GateDeviations,
    trajectory: &[TrajectoryDeviation],
    _groove_time_secs: Option<f64>,
) -> PassGrade {
    match grading {
        Grading::Unknown => PassGrade::Incomplete,
        Grading::WaveoffUnknown => PassGrade::WaveoffUnknown,
        Grading::Bolter if gates.all_valid() => PassGrade::Bolter,
        Grading::Recovered { .. } if gates.all_valid() => grade_from_gates(gates, trajectory),
        // A qualification touch-and-go keeps the independently measured
        // approach grade, but can never receive a trap/wire-specific upgrade.
        Grading::TouchAndGo { .. } if gates.all_valid() => grade_from_gates(gates, trajectory),
        Grading::TouchAndGo { .. } | Grading::Bolter | Grading::Recovered { .. } => {
            PassGrade::Incomplete
        }
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
) -> (PassGrade, Option<f64>) {
    match grading {
        Grading::Unknown => (PassGrade::Incomplete, None),
        Grading::WaveoffUnknown => (PassGrade::WaveoffUnknown, None),
        Grading::Bolter if gates.all_valid() => (PassGrade::Bolter, PassGrade::Bolter.points()),
        Grading::TouchAndGo { .. } => (PassGrade::Incomplete, None),
        Grading::Recovered { .. } if gates.all_valid() => {
            let mut gate_scores = Vec::with_capacity(3);
            if let Some(gate) = gates.at_three_quarter_nm.as_ref() {
                gate_scores.push(grade_single_gate(gate, false).points().unwrap_or_default());
            }
            if let Some(gate) = gates.at_half_nm.as_ref() {
                gate_scores.push(grade_single_gate(gate, false).points().unwrap_or_default());
            }
            if let Some(gate) = gates.at_quarter_nm.as_ref() {
                gate_scores.push(grade_single_gate(gate, true).points().unwrap_or_default());
            }

            if gate_scores.is_empty() {
                let fallback = grade_from_gates(gates, &[]);
                (fallback, fallback.points())
            } else {
                let average_points = gate_scores.iter().sum::<f64>() / gate_scores.len() as f64;
                (
                    map_vstol_approach_points_to_grade(average_points),
                    Some(average_points),
                )
            }
        }
        Grading::Bolter | Grading::Recovered { .. } => (PassGrade::Incomplete, None),
    }
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
    if quarter_nm && gate.gs_deviation_deg < GS_CUT_LOW_DEG {
        return PassGrade::Cut;
    }

    let gs_high = gate.gs_deviation_deg.max(0.0);
    let gs_low = gate.gs_deviation_deg.min(0.0).abs();
    let lineup = gate.lineup_deg.abs();

    if gs_high >= GS_SIGNIFICANT || gs_low >= GS_SIGNIFICANT || lineup >= LU_MEDIUM {
        PassGrade::NoGrade
    } else if gs_high >= GS_SLIGHT_HIGH || gs_low >= GS_SLIGHT_LOW || lineup >= LU_SLIGHT {
        PassGrade::OkParentheses
    } else {
        PassGrade::Ok
    }
}

/// `trajectory` is the continuous groove-to-touchdown series (see
/// `TrajectoryDeviation`); passing `&[]` reproduces the historical
/// three-gates-only behaviour exactly (all folds below start from the same
/// gate-only values and an empty slice contributes nothing).
///
/// `pub(crate)` (rather than only reachable through `compute_pass_grade`) so the `cadence-ab`
/// diagnostic command (B.2 of the notation/cadence work) can grade a replayed gate/trajectory
/// pair directly, without needing to fabricate a `Grading` outcome the replay never computes.
pub(crate) fn grade_from_gates(
    gates: &GateDeviations,
    trajectory: &[TrajectoryDeviation],
) -> PassGrade {
    // Dangerously low at the 1/4-nm gate → Cut pass. GS_CUT_LOW_DEG is negative, so this
    // triggers when the hook is well below the ideal glide path at close range. Also checked
    // at every continuous sample inside the 1/4-nm gate distance, not only at the exact gate
    // crossing: a brief dip below threshold that recovers before crossing 463 m is just as
    // dangerous as one measured exactly at the gate.
    let quarter_nm_cut = gates
        .at_quarter_nm
        .as_ref()
        .is_some_and(|g| g.gs_deviation_deg < GS_CUT_LOW_DEG)
        || trajectory.iter().any(|d| {
            d.distance_m <= crate::track::GATE_QUARTER_NM && d.gs_deviation_deg < GS_CUT_LOW_DEG
        });
    if quarter_nm_cut {
        return PassGrade::Cut;
    }

    // Worst positive (high) and negative (low) GS deviation, and worst lineup, across the
    // three gates *and* the continuous trajectory. Combining both means a spike between two
    // gates can no longer be graded better than if a gate had happened to land on it — see
    // `docs/GRADING_REFERENCE.md`, CATOBAR score.
    let all_gs: Vec<f64> = [
        gates
            .at_three_quarter_nm
            .as_ref()
            .map(|g| g.gs_deviation_deg),
        gates.at_half_nm.as_ref().map(|g| g.gs_deviation_deg),
        gates.at_quarter_nm.as_ref().map(|g| g.gs_deviation_deg),
    ]
    .into_iter()
    .flatten()
    .chain(trajectory.iter().map(|d| d.gs_deviation_deg))
    .collect();

    let worst_gs_high = all_gs
        .iter()
        .copied()
        .filter(|&v| v > 0.0)
        .fold(0.0_f64, f64::max);
    let worst_gs_low = all_gs
        .iter()
        .copied()
        .filter(|&v| v < 0.0)
        .fold(0.0_f64, f64::min)
        .abs();

    let worst_lu = [
        gates
            .at_three_quarter_nm
            .as_ref()
            .map(|g| g.lineup_deg.abs()),
        gates.at_half_nm.as_ref().map(|g| g.lineup_deg.abs()),
        gates.at_quarter_nm.as_ref().map(|g| g.lineup_deg.abs()),
    ]
    .into_iter()
    .flatten()
    .chain(trajectory.iter().map(|d| d.lineup_deg.abs()))
    .fold(0.0_f64, f64::max);

    // Apply the PROJECT-DERIVED grade tiers.
    // GS uses the CATOBAR-derived tiers retained for both paths: slight at 0.5°, significant at 1.0°.
    // Lineup has three tiers: slight (1.0°) → (OK), medium (2.0°) → --, large (3.0°) → --
    let tier = if worst_gs_high >= GS_SIGNIFICANT
        || worst_gs_low >= GS_SIGNIFICANT
        || worst_lu >= LU_MEDIUM
    {
        PassGrade::NoGrade
    } else if worst_gs_high >= GS_SLIGHT_HIGH
        || worst_gs_low >= GS_SLIGHT_LOW
        || worst_lu >= LU_SLIGHT
    {
        PassGrade::OkParentheses
    } else if trend_worsening(trajectory) {
        // NATOPS distinguishes OK ("reasonable deviations with good corrections") from (OK)
        // ("fair — reasonable deviations") precisely on whether corrections were good, not on
        // amplitude alone (see docs/GRADING_REFERENCE.md, "Continuous trajectory"). Deviations
        // are already within OK margins at this point; a trajectory that is still measurably
        // worsening this close to touchdown cannot claim "good corrections", so it is capped at
        // (OK) instead. Trend is deliberately never used to raise a grade the amplitude rules
        // already placed below Ok — only to hold Ok back when it would otherwise be granted.
        PassGrade::OkParentheses
    } else {
        PassGrade::Ok
    };

    // A.3: weight the last moments before the ramp more heavily. Rather than reweighting
    // worst_gs_high/worst_gs_low/worst_lu above (which would also rescale the three named
    // gates and complicate every existing threshold), this is a separate, narrower,
    // downgrade-only check: a deviation that would only ever earn (OK) elsewhere in the
    // approach is treated as NoGrade if it happens inside LATE_WINDOW_DISTANCE_M, where there
    // is no distance left to correct it. It never raises a grade, and never touches Cut or an
    // already-NoGrade result. See docs/GRADING_REFERENCE.md, "Late-approach weighting".
    if matches!(tier, PassGrade::Ok | PassGrade::OkParentheses) && late_window_severe(trajectory) {
        PassGrade::NoGrade
    } else {
        tier
    }
}

/// Whether any continuous trajectory sample inside `LATE_WINDOW_DISTANCE_M` of touchdown
/// crossed the (stricter) late-window GS/lineup thresholds. See `LATE_WINDOW_GS_DEG`/
/// `LATE_WINDOW_LU_DEG` for why this is intentionally tighter than the general amplitude tiers.
fn late_window_severe(trajectory: &[TrajectoryDeviation]) -> bool {
    trajectory.iter().any(|d| {
        d.distance_m <= LATE_WINDOW_DISTANCE_M
            && (d.gs_deviation_deg.abs() >= LATE_WINDOW_GS_DEG
                || d.lineup_deg.abs() >= LATE_WINDOW_LU_DEG)
    })
}

/// Whether GS or lineup deviation was clearly getting worse, not better, in the final
/// `TREND_WINDOW_S` seconds of the recorded trajectory. A simple two-point slope of the
/// deviation's absolute value over that window, as prescribed by A.2 of the notation work: "no
/// sophisticated filtering at this stage". Returns `false` (no penalty) whenever there is not
/// enough data to judge a trend, which errs toward not penalizing rather than inventing a signal.
fn trend_worsening(trajectory: &[TrajectoryDeviation]) -> bool {
    let Some(reference_time) = trajectory.last().map(|d| d.timestamp_dcs) else {
        return false;
    };
    let window: Vec<&TrajectoryDeviation> = trajectory
        .iter()
        .filter(|d| d.timestamp_dcs >= reference_time - TREND_WINDOW_S)
        .collect();
    let (Some(first), Some(last)) = (window.first(), window.last()) else {
        return false;
    };
    let dt = last.timestamp_dcs - first.timestamp_dcs;
    if dt <= 0.0 {
        return false;
    }
    let gs_slope = (last.gs_deviation_deg.abs() - first.gs_deviation_deg.abs()) / dt;
    let lu_slope = (last.lineup_deg.abs() - first.lineup_deg.abs()) / dt;
    gs_slope >= TREND_WORSENING_DEG_PER_S || lu_slope >= TREND_WORSENING_DEG_PER_S
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
            },
            half_quality: GateQuality {
                status: GateStatus::Valid,
                reason: None,
                bracket_gap_ms: Some(100.0),
            },
            quarter_quality: GateQuality {
                status: GateStatus::Valid,
                reason: None,
                bracket_gap_ms: Some(100.0),
            },
        }
    }

    #[test]
    fn test_perfect_pass_is_ok() {
        // All deviations well within OK margins.
        let g = gates_deg(0.2, 0.3, 0.1, 0.2, 0.1, 0.1);
        assert_eq!(grade_from_gates(&g, &[]), PassGrade::Ok);
    }

    #[test]
    fn test_slight_gs_deviation_is_ok_parentheses() {
        // 0.6° high GS at 3/4 nm: exceeds GS_SLIGHT_HIGH (0.5°) → (OK).
        let g = gates_deg(0.6, 0.3, 0.1, 0.2, 0.1, 0.1);
        assert_eq!(grade_from_gates(&g, &[]), PassGrade::OkParentheses);
    }

    #[test]
    fn test_slight_gs_high_threshold_is_0_5() {
        // 0.9° high GS: still between GS_SLIGHT_HIGH and GS_SIGNIFICANT → (OK), not --.
        let g = gates_deg(0.9, 0.0, 0.0, 0.0, 0.0, 0.0);
        assert_eq!(grade_from_gates(&g, &[]), PassGrade::OkParentheses);
    }

    #[test]
    fn test_catobar_slight_gs_low_threshold_is_0_5() {
        // The boundary is inclusive: 0.5° low GS is (OK).
        let g = gates_deg(-0.5, 0.0, 0.0, 0.0, 0.0, 0.0);
        assert_eq!(grade_from_gates(&g, &[]), PassGrade::OkParentheses);
    }

    #[test]
    fn test_gs_high_below_new_threshold_is_ok() {
        // 0.4° high GS: below GS_SLIGHT_HIGH (0.5°) → OK.
        let g = gates_deg(0.4, 0.0, 0.0, 0.0, 0.0, 0.0);
        assert_eq!(grade_from_gates(&g, &[]), PassGrade::Ok);
    }

    #[test]
    fn test_slight_lu_deviation_is_ok_parentheses() {
        // 1.1° LU at half nm: exceeds LU_SLIGHT (1.0°) → (OK).
        let g = gates_deg(0.2, 0.3, 0.1, 1.1, 0.1, 0.1);
        assert_eq!(grade_from_gates(&g, &[]), PassGrade::OkParentheses);
    }

    #[test]
    fn test_catobar_significant_gs_threshold_is_1_0() {
        // The boundary is inclusive: 1.0° high GS is no-grade.
        let g = gates_deg(1.0, 0.3, 0.1, 0.2, 0.1, 0.1);
        assert_eq!(grade_from_gates(&g, &[]), PassGrade::NoGrade);
    }

    #[test]
    fn test_significant_lu_deviation_is_no_grade() {
        // 3.1° LU at 1/4 nm: exceeds LU_SIGNIFICANT (3.0°) → --.
        let g = gates_deg(0.2, 0.3, 0.1, 0.2, 0.1, 3.1);
        assert_eq!(grade_from_gates(&g, &[]), PassGrade::NoGrade);
    }

    #[test]
    fn test_medium_lu_deviation_is_no_grade() {
        // 2.1° LU at 3/4 nm: exceeds LU_MEDIUM (2.0°) → --.
        let g = gates_deg(0.0, 2.1, 0.0, 0.0, 0.0, 0.0);
        assert_eq!(grade_from_gates(&g, &[]), PassGrade::NoGrade);
    }

    #[test]
    fn test_below_medium_lu_is_ok_parentheses() {
        // 1.9° LU: above LU_SLIGHT but below LU_MEDIUM → (OK).
        let g = gates_deg(0.0, 1.9, 0.0, 0.0, 0.0, 0.0);
        assert_eq!(grade_from_gates(&g, &[]), PassGrade::OkParentheses);
    }

    #[test]
    fn test_dangerously_low_at_quarter_nm_is_cut() {
        // −2.6° GS at 1/4 nm: below GS_CUT_LOW_DEG (−2.5°) → Cut.
        let g = gates_deg(0.0, 0.0, 0.0, 0.0, -2.6, 0.0);
        assert_eq!(grade_from_gates(&g, &[]), PassGrade::Cut);
    }

    #[test]
    fn test_low_at_earlier_gates_not_cut() {
        // −2.6° GS only at 3/4 nm (not at 1/4 nm) → NoGrade, not Cut.
        let g = gates_deg(-2.6, 0.0, 0.0, 0.0, 0.0, 0.0);
        assert_eq!(grade_from_gates(&g, &[]), PassGrade::NoGrade);
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
        }
    }

    #[test]
    fn continuous_excursion_between_two_gates_is_not_missed() {
        // All three gates are clean (well within OK), but the trajectory records a
        // significant 1.2° high excursion at 700 m, strictly between the 1/2-nm (926 m) and
        // 1/4-nm (463 m) gates, which the three-gate-only computation could never see. The
        // continuous series must still catch it and downgrade the pass, exactly as if a gate
        // had landed on the spike.
        let g = gates_deg(0.1, 0.1, 0.1, 0.1, 0.1, 0.1);
        let trajectory = [trajectory_point(700.0, 1.2, 0.0)];
        assert_eq!(grade_from_gates(&g, &[]), PassGrade::Ok);
        assert_eq!(grade_from_gates(&g, &trajectory), PassGrade::NoGrade);
    }

    #[test]
    fn continuous_slight_lineup_excursion_is_not_missed() {
        let g = gates_deg(0.1, 0.1, 0.1, 0.1, 0.1, 0.1);
        let trajectory = [trajectory_point(700.0, 0.0, 1.5)];
        assert_eq!(grade_from_gates(&g, &trajectory), PassGrade::OkParentheses);
    }

    #[test]
    fn continuous_series_can_only_worsen_never_improve_the_grade() {
        // A trajectory sample milder than the worst gate must never pull the grade back up.
        let g = gates_deg(1.2, 0.0, 0.0, 0.0, 0.0, 0.0);
        let trajectory = [trajectory_point(700.0, 0.1, 0.0)];
        assert_eq!(grade_from_gates(&g, &trajectory), grade_from_gates(&g, &[]));
    }

    #[test]
    fn dangerously_low_trajectory_sample_inside_quarter_nm_is_cut_even_off_gate() {
        // The exact 1/4-nm gate crossing is clean, but the continuous series shows a brief
        // dip below GS_CUT_LOW_DEG at 400 m (inside the 463 m gate) that recovered before the
        // gate itself was crossed. This is exactly the kind of transient the discrete gate
        // alone would miss.
        let g = gates_deg(0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
        let trajectory = [trajectory_point(400.0, -2.6, 0.0)];
        assert_eq!(grade_from_gates(&g, &trajectory), PassGrade::Cut);
    }

    #[test]
    fn low_trajectory_sample_outside_quarter_nm_is_not_cut() {
        // Same dangerously-low value, but at 700 m (outside the 1/4-nm gate distance): the
        // Cut rule only ever applied "at the ramp", never earlier in the groove.
        let g = gates_deg(0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
        let trajectory = [trajectory_point(700.0, -2.6, 0.0)];
        assert_eq!(grade_from_gates(&g, &trajectory), PassGrade::NoGrade);
    }

    #[test]
    fn worsening_trend_caps_an_otherwise_ok_pass_at_ok_parentheses() {
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
        assert_eq!(grade_from_gates(&g, &trajectory), PassGrade::OkParentheses);
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
        assert_eq!(grade_from_gates(&g, &trajectory), PassGrade::Ok);
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
        assert_eq!(grade_from_gates(&g, &trajectory), PassGrade::Ok);
    }

    #[test]
    fn windowing_prevents_a_long_clean_history_from_masking_a_recent_worsening_trend() {
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
        assert_eq!(grade_from_gates(&g, &trajectory), PassGrade::OkParentheses);
    }

    #[test]
    fn worsening_trend_never_downgrades_a_pass_already_below_ok() {
        // Trend is only ever checked at the Ok/(OK) boundary; a pass already at (OK) or worse
        // from amplitude alone is not pushed down an additional tier by a worsening trend.
        let g = gates_deg(0.6, 0.0, 0.0, 0.0, 0.0, 0.0);
        let trajectory = [
            trajectory_point_at(0.0, 900.0, 0.05, 0.0),
            trajectory_point_at(4.0, 500.0, 0.45, 0.0),
        ];
        assert_eq!(grade_from_gates(&g, &trajectory), PassGrade::OkParentheses);
    }

    #[test]
    fn single_trajectory_sample_has_no_trend() {
        // A single sample cannot express a slope; trend must gracefully report "no signal"
        // rather than panic or divide by zero.
        let g = gates_deg(0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
        let trajectory = [trajectory_point_at(0.0, 700.0, 0.05, 0.0)];
        assert_eq!(grade_from_gates(&g, &trajectory), PassGrade::Ok);
    }

    #[test]
    fn late_window_gs_deviation_downgrades_an_otherwise_ok_grade_to_no_grade() {
        // 0.9 deg is above LATE_WINDOW_GS_DEG (0.8) but below GS_SIGNIFICANT (1.0), so amplitude
        // alone would only ever grant (OK) here. Happening at 100 m -- inside
        // LATE_WINDOW_DISTANCE_M, with no room left to correct -- caps it at NoGrade instead.
        let g = gates_deg(0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
        let trajectory = [trajectory_point(100.0, 0.9, 0.0)];
        assert_eq!(grade_from_gates(&g, &trajectory), PassGrade::NoGrade);
    }

    #[test]
    fn late_window_lineup_deviation_downgrades_to_no_grade() {
        // Same logic, lineup axis: 1.6 deg is above LATE_WINDOW_LU_DEG (1.5) but below
        // LU_MEDIUM (2.0).
        let g = gates_deg(0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
        let trajectory = [trajectory_point(100.0, 0.0, 1.6)];
        assert_eq!(grade_from_gates(&g, &trajectory), PassGrade::NoGrade);
    }

    #[test]
    fn the_same_deviation_earlier_in_the_approach_is_only_ok_parentheses() {
        // Identical 0.9 deg GS deviation to the case above, but at 700 m -- well outside
        // LATE_WINDOW_DISTANCE_M. This is exactly the "last moments matter more" effect A.3
        // adds: the same magnitude of error is graded differently depending on how much
        // distance is left to correct it.
        let g = gates_deg(0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
        let trajectory = [trajectory_point(700.0, 0.9, 0.0)];
        assert_eq!(grade_from_gates(&g, &trajectory), PassGrade::OkParentheses);
    }

    #[test]
    fn late_window_below_its_own_threshold_does_not_downgrade() {
        // 0.6 deg at 100 m crosses the general GS_SLIGHT_HIGH tier ((OK)) but not the stricter
        // LATE_WINDOW_GS_DEG (0.8) -- the late-window check must not fire on every deviation
        // found close to the ramp, only ones that cross its own, stricter threshold.
        let g = gates_deg(0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
        let trajectory = [trajectory_point(100.0, 0.6, 0.0)];
        assert_eq!(grade_from_gates(&g, &trajectory), PassGrade::OkParentheses);
    }

    #[test]
    fn late_window_never_downgrades_a_pass_already_at_no_grade_or_worse() {
        // A pass already at NoGrade from amplitude alone is not further affected; the
        // late-window check only ever holds back Ok/(OK), like trend does for Ok alone.
        let g = gates_deg(1.2, 0.0, 0.0, 0.0, 0.0, 0.0);
        let trajectory = [trajectory_point(100.0, 0.9, 0.0)];
        assert_eq!(grade_from_gates(&g, &trajectory), PassGrade::NoGrade);
    }

    #[test]
    fn late_window_never_overrides_cut() {
        // A genuine Cut (dangerously low inside the quarter-NM gate) returns before the
        // late-window check is ever reached; a coexisting late-window-severe sample elsewhere
        // must not somehow soften that to NoGrade.
        let g = gates_deg(0.0, 0.0, 0.0, 0.0, -2.6, 0.0);
        let trajectory = [trajectory_point(100.0, 0.9, 0.0)];
        assert_eq!(grade_from_gates(&g, &trajectory), PassGrade::Cut);
    }

    #[test]
    fn test_bolter_outcome() {
        let g = gates_deg(0.2, 0.3, 0.1, 0.2, 0.1, 0.1);
        assert_eq!(
            compute_pass_grade(&Grading::Bolter, &g, &[], None),
            PassGrade::Bolter
        );
    }

    #[test]
    fn test_waveoff_outcome() {
        let g = GateDeviations::default();
        assert_eq!(
            compute_pass_grade(&Grading::WaveoffUnknown, &g, &[], None),
            PassGrade::WaveoffUnknown
        );
    }

    #[test]
    fn legacy_wire3_time_window_does_not_upgrade_to_perfect() {
        // Former legacy trigger: it must remain OK, not become Perfect.
        let g = gates_deg(0.1, 0.1, 0.1, 0.1, 0.1, 0.1);
        let grading = Grading::Recovered {
            cable: Some(3),
            cable_estimated: Some(3),
        };
        assert_eq!(
            compute_pass_grade(&grading, &g, &[], Some(16.5)),
            PassGrade::Ok
        );
    }

    #[test]
    fn clean_wire4_pass_remains_ok() {
        // Zero deviations but wire 4: cable selection cannot emit Perfect.
        let g = gates_deg(0.1, 0.1, 0.1, 0.1, 0.1, 0.1);
        let grading = Grading::Recovered {
            cable: Some(4),
            cable_estimated: Some(4),
        };
        assert_eq!(
            compute_pass_grade(&grading, &g, &[], Some(16.5)),
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
            compute_pass_grade(&grading, &g, &[], Some(12.0)),
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
            compute_pass_grade(&grading, &g, &[], Some(22.0)),
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
            compute_pass_grade(&grading, &g, &[], Some(16.5)),
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
        assert_eq!(compute_pass_grade(&grading, &g, &[], None), PassGrade::Ok);
    }

    #[test]
    fn test_touch_and_go_keeps_the_measured_approach_grade() {
        let g = gates_deg(0.6, 0.0, 0.0, 0.0, 0.0, 0.0);
        let grading = Grading::TouchAndGo {
            cable_estimated: Some(3),
        };

        assert_eq!(
            compute_pass_grade(&grading, &g, &[], Some(16.5)),
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
            compute_pass_grade(&grading, &g, &[], Some(16.5)),
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

        let (grade, points) = compute_vstol_approach_grade_points(&grading, &gates);

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

        let (grade, points) = compute_vstol_approach_grade_points(&grading, &gates);

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

        let (grade, points) = compute_vstol_approach_grade_points(&grading, &gates);

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
                compute_vstol_approach_grade_points(&grading, &gates),
                (expected_grade, expected_points)
            );
        }
    }
}
