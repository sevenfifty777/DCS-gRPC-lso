use crate::track::{GateDeviations, Grading};

// ---------------------------------------------------------------------------
// Thresholds — NAVAIR 00-80T-104 / MOOSE Airboss CVN values (degrees)
// ---------------------------------------------------------------------------

/// Glideslope deviation thresholds (degrees).
/// Source: MOOSE Airboss `gle` table CVN defaults; NAVAIR 00-80T-104.
/// Thresholds are asymmetric: being high is penalised slightly later than being low.
const GS_SLIGHT_HIGH: f64 = 0.5;    // (H) — "slightly high"   NAVAIR ~+0.5°
<<<<<<< HEAD
const GS_SLIGHT_LOW: f64 = 0.5;     // (L) — "slightly low"    symmetric V/STOL/CATOBAR threshold
=======
const GS_SLIGHT_LOW: f64 = 0.5;     // (L) — "slightly low"    MOOSE  −0.8° (symmetric kept)
>>>>>>> upstream/main
const GS_SIGNIFICANT: f64 = 1.0;    // H / L — "high/low"      symmetric
/// Dangerously low at the 1/4-nm gate — triggers a Cut pass.
const GS_CUT_LOW_DEG: f64 = -2.5;

/// Lineup deviation thresholds (degrees, absolute value).
/// Source: MOOSE Airboss `lue` table, CVN defaults.
const LU_SLIGHT: f64 = 1.0;       // (LUL) / (LUR) — "slightly lined up left/right"
const LU_MEDIUM: f64 = 2.0;       // LUL / LUR     — "lined up left/right" (medium, MOOSE + NAVAIR)
// const LU_SIGNIFICANT: f64 = 3.0;  // LUL / LUR     — "lined up left/right" (large) — NoGrade already triggered at LU_MEDIUM

// ---------------------------------------------------------------------------
// PassGrade — NAVAIR 00-80T-104 aligned
// ---------------------------------------------------------------------------

/// NAVAIR 00-80T-104 pass grade.
///
/// Points and labels match the real-world LSO grade sheet:
///
/// | Label   | Points | Meaning |
/// |---------|--------|---------|
/// | `_OK_`  | 5.0    | Unicorn — zero deviations, groove time 15–19 s, wire 3 |
/// | `OK`    | 4.0    | Okay pass — no significant deviations |
/// | `(OK)`  | 3.0    | Fair pass — slight deviations only |
/// | `--`    | 2.0    | No grade — significant deviations |
/// | `C`     | 0.0    | Cut pass — dangerously low at the ramp |
/// | `B`     | 2.5    | Bolter |
/// | `WO`    | 1.0    | Waveoff |
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum PassGrade {
    /// Unicorn — perfect pass (NAVAIR label `_OK_`, 5.0 pts).
    Unicorn,
    Ok,
    OkParentheses,
    /// No grade — significant deviations (NAVAIR label `--`).
    NoGrade,
    /// Cut pass — dangerously low at the ramp, or landed after being waved off.
    Cut,
    Bolter,
    WaveoffPilot,
}

impl PassGrade {
    /// Short display label used in charts and the greenie board.
    /// These match the standard NAVAIR LSO grade sheet symbols.
    pub fn label(self) -> &'static str {
        match self {
            Self::Unicorn       => "_OK_",
            Self::Ok            => "OK",
            Self::OkParentheses => "(OK)",
            Self::NoGrade       => "--",
            Self::Cut           => "C",
            Self::Bolter        => "B",
            Self::WaveoffPilot  => "WO",
        }
    }

    /// Numeric score used for greenie-board averaging (NAVAIR 00-80T-104).
    pub fn points(self) -> f64 {
        match self {
            Self::Unicorn       => 5.0,
            Self::Ok            => 4.0,
            Self::OkParentheses => 3.0,
            Self::NoGrade       => 2.0,
            Self::Cut           => 0.0,
            Self::Bolter        => 2.5,
            Self::WaveoffPilot  => 1.0,
        }
    }
}

// ---------------------------------------------------------------------------
<<<<<<< HEAD
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

/// Combine the CATOBAR-style approach grade with the AV-8B spot bonus.
///
/// The numeric result is capped at 5.0 and mapped back onto the same display
/// labels used by the native CATOBAR greenie board.  This function is only
/// called for a successfully recovered V/STOL pass; CATOBAR grading is left
/// completely unchanged.
/// Combine an arbitrary V/STOL approach point value with the spot bonus.
///
/// This variant is used by the averaged-gates V/STOL approach logic, where the
/// approach score can be fractional before the spot bonus is added.
pub fn compute_vstol_final_grade_from_points(
    approach_points: f64,
    spot_grade: SpotGrade,
) -> (PassGrade, f64) {
    let points = (approach_points + spot_grade.bonus_points()).min(5.0);
    let final_grade = if points >= 5.0 {
        PassGrade::Unicorn
    } else if points >= 4.0 {
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
=======
>>>>>>> upstream/main
// Grade computation
// ---------------------------------------------------------------------------

/// NAVAIR groove-time window for a Unicorn pass (seconds, Case I/II CVN).
/// Source: NAVAIR 00-80T-104. MOOSE Airboss original values: 15.0–18.99 s.
const UNICORN_GROOVE_MIN: f64 = 15.0;
const UNICORN_GROOVE_MAX: f64 = 18.99;

/// Derive a `PassGrade` from the overall `Grading` outcome, gate deviations,
/// and the groove time (seconds from groove entry to touchdown).
///
/// `groove_time_secs` is `None` when either timestamp was not recorded
/// (e.g. the aircraft never entered the 3/4-nm gate before landing).
pub fn compute_pass_grade(
    grading: &Grading,
    gates: &GateDeviations,
    groove_time_secs: Option<f64>,
) -> PassGrade {
    match grading {
        Grading::Unknown      => PassGrade::NoGrade,
        Grading::WaveoffPilot => PassGrade::WaveoffPilot,
        Grading::Bolter       => PassGrade::Bolter,
<<<<<<< HEAD
        Grading::Recovered { cable, .. } => {
=======
        Grading::Recovered { cable, .. } | Grading::IntentionalBolter { cable_estimated: cable } => {
            if let Grading::IntentionalBolter { cable_estimated: None } = grading {
                // For a qualification touch-and-go, if they miss the wires completely,
                // it is graded as a Bolter (B), but the outcome still reflects it was a Qualif Bolter.
                return PassGrade::Bolter;
            }

>>>>>>> upstream/main
            let base = grade_from_gates(gates);
            // Unicorn: zero deviations (base == Ok), wire 3, groove time in window.
            if base == PassGrade::Ok
                && *cable == Some(3)
                && groove_time_secs
                    .map(|t| t >= UNICORN_GROOVE_MIN && t <= UNICORN_GROOVE_MAX)
                    .unwrap_or(false)
            {
                PassGrade::Unicorn
            } else {
                base
            }
        }
    }
}

<<<<<<< HEAD
/// Compute the AV-8B V/STOL approach grade using the same gate-deviation
/// thresholds as the CATOBAR grader, but without CATOBAR-only wire/groove
/// bonuses.  `GateDeviations` are already measured in `track.rs` relative to
/// the aircraft-specific ideal glide slope; for AV-8B that reference is 3.0°.
///
/// Deliberately NOT part of this score: AOA.  The AV-8B 10-12° target is used
/// only by `aoa_rating` for trace colouring / indication.
///
/// The V/STOL final score is obtained later by adding the calibrated spot-7.5
/// bonus to this approach grade.
/// Compute the AV-8B V/STOL approach grade and its numeric point value.
///
/// Unlike CATOBAR, the V/STOL approach score is the arithmetic mean of the
/// three gate grades (3/4 nm, 1/2 nm, 1/4 nm), each gate being assessed with
/// the same GS/LU thresholds as the CATOBAR logic. This preserves the original
/// severity per gate while avoiding a pure “worst gate wins” outcome for VTOL.
pub fn compute_vstol_approach_grade_points(
    grading: &Grading,
    gates: &GateDeviations,
) -> (PassGrade, f64) {
    match grading {
        Grading::Unknown => (PassGrade::NoGrade, PassGrade::NoGrade.points()),
        Grading::WaveoffPilot => (PassGrade::WaveoffPilot, PassGrade::WaveoffPilot.points()),
        Grading::Bolter => (PassGrade::Bolter, PassGrade::Bolter.points()),
        Grading::Recovered { .. } => {
            let mut gate_scores = Vec::with_capacity(3);
            if let Some(g) = gates.at_three_quarter_nm.as_ref() {
                gate_scores.push(grade_single_gate(g, false).points());
            }
            if let Some(g) = gates.at_half_nm.as_ref() {
                gate_scores.push(grade_single_gate(g, false).points());
            }
            if let Some(g) = gates.at_quarter_nm.as_ref() {
                gate_scores.push(grade_single_gate(g, true).points());
            }

            if gate_scores.is_empty() {
                let fallback = grade_from_gates(gates);
                (fallback, fallback.points())
            } else {
                let avg_points = gate_scores.iter().sum::<f64>() / gate_scores.len() as f64;
                (map_vstol_approach_points_to_grade(avg_points), avg_points)
            }
        }
    }
}

fn map_vstol_approach_points_to_grade(points: f64) -> PassGrade {
    if points >= PassGrade::Ok.points() {
        PassGrade::Ok
    } else if points >= PassGrade::OkParentheses.points() {
        PassGrade::OkParentheses
    } else if points >= PassGrade::NoGrade.points() {
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
    let lu = gate.lineup_deg.abs();

    if gs_high >= GS_SIGNIFICANT || gs_low >= GS_SIGNIFICANT || lu >= LU_MEDIUM {
        PassGrade::NoGrade
    } else if gs_high >= GS_SLIGHT_HIGH || gs_low >= GS_SLIGHT_LOW || lu >= LU_SLIGHT {
        PassGrade::OkParentheses
    } else {
        PassGrade::Ok
    }
}

=======
>>>>>>> upstream/main
fn grade_from_gates(gates: &GateDeviations) -> PassGrade {
    // Dangerously low at the 1/4-nm gate → Cut pass.
    // GS_CUT_LOW_DEG is negative, so this triggers when the hook is well below
    // the ideal glide path at close range.
    if let Some(g) = gates.at_quarter_nm.as_ref() {
        if g.gs_deviation_deg < GS_CUT_LOW_DEG {
            return PassGrade::Cut;
        }
    }

    // Worst positive (high) and negative (low) GS deviation across all three gates.
    // Tracked separately because NAVAIR/MOOSE use asymmetric thresholds.
    let worst_gs_high = [
        gates.at_three_quarter_nm.as_ref().map(|g| g.gs_deviation_deg),
        gates.at_half_nm.as_ref().map(|g| g.gs_deviation_deg),
        gates.at_quarter_nm.as_ref().map(|g| g.gs_deviation_deg),
    ]
    .into_iter()
    .flatten()
    .filter(|&v| v > 0.0)
    .fold(0.0_f64, f64::max);

    let worst_gs_low = [
        gates.at_three_quarter_nm.as_ref().map(|g| g.gs_deviation_deg),
        gates.at_half_nm.as_ref().map(|g| g.gs_deviation_deg),
        gates.at_quarter_nm.as_ref().map(|g| g.gs_deviation_deg),
    ]
    .into_iter()
    .flatten()
    .filter(|&v| v < 0.0)
    .fold(0.0_f64, f64::min)
    .abs();

    let worst_lu = [
        gates.at_three_quarter_nm.as_ref().map(|g| g.lineup_deg.abs()),
        gates.at_half_nm.as_ref().map(|g| g.lineup_deg.abs()),
        gates.at_quarter_nm.as_ref().map(|g| g.lineup_deg.abs()),
    ]
    .into_iter()
    .flatten()
    .fold(0.0_f64, f64::max);

    // Apply NAVAIR/MOOSE grade tiers.
<<<<<<< HEAD
    // GS uses the CATOBAR-derived tiers retained for both paths: slight at 0.5°, significant at 1.0°.
=======
    // GS is asymmetric: slight high at 0.5° (NAVAIR), slight low at 0.8° (MOOSE).
>>>>>>> upstream/main
    // Lineup has three tiers: slight (1.0°) → (OK), medium (2.0°) → --, large (3.0°) → --
    if worst_gs_high >= GS_SIGNIFICANT || worst_gs_low >= GS_SIGNIFICANT || worst_lu >= LU_MEDIUM {
        PassGrade::NoGrade
    } else if worst_gs_high >= GS_SLIGHT_HIGH || worst_gs_low >= GS_SLIGHT_LOW || worst_lu >= LU_SLIGHT {
        PassGrade::OkParentheses
    } else {
        PassGrade::Ok
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::track::{GateDatum, GateDeviations};

    /// Build a `GateDeviations` from degree values (the unit used for grading).
    /// Foot values are set to 0.0 as they are not used in grading logic.
    fn gates_deg(
        gs_3q: f64, lu_3q: f64,
        gs_h:  f64, lu_h:  f64,
        gs_q:  f64, lu_q:  f64,
    ) -> GateDeviations {
        let datum = |gs_deg, lu_deg| Some(GateDatum {
            gs_deviation_deg: gs_deg,
            lineup_deg: lu_deg,
            gs_deviation_ft: 0.0,
            lineup_ft: 0.0,
        });
        GateDeviations {
            at_three_quarter_nm: datum(gs_3q, lu_3q),
            at_half_nm:          datum(gs_h,  lu_h),
            at_quarter_nm:       datum(gs_q,  lu_q),
        }
    }

    #[test]
    fn test_perfect_pass_is_ok() {
        // All deviations well within OK margins.
        let g = gates_deg(0.2, 0.3, 0.1, 0.2, 0.1, 0.1);
        assert_eq!(grade_from_gates(&g), PassGrade::Ok);
    }

    #[test]
    fn test_slight_gs_deviation_is_ok_parentheses() {
        // 0.6° high GS at 3/4 nm: exceeds GS_SLIGHT_HIGH (0.5°) → (OK).
        let g = gates_deg(0.6, 0.3, 0.1, 0.2, 0.1, 0.1);
        assert_eq!(grade_from_gates(&g), PassGrade::OkParentheses);
    }

    #[test]
    fn test_slight_gs_high_threshold_is_0_5() {
        // 0.9° high GS: still between GS_SLIGHT_HIGH and GS_SIGNIFICANT → (OK), not --.
        let g = gates_deg(0.9, 0.0, 0.0, 0.0, 0.0, 0.0);
        assert_eq!(grade_from_gates(&g), PassGrade::OkParentheses);
    }

    #[test]
<<<<<<< HEAD
    fn test_slight_gs_low_threshold_is_0_5() {
        // 0.6° low GS: exceeds GS_SLIGHT_LOW (0.5°) → (OK).
        let g = gates_deg(-0.6, 0.0, 0.0, 0.0, 0.0, 0.0);
=======
    fn test_slight_gs_low_threshold_is_0_8() {
        // 0.9° low GS: exceeds GS_SLIGHT_LOW (0.8°) → (OK).
        let g = gates_deg(-0.9, 0.0, 0.0, 0.0, 0.0, 0.0);
>>>>>>> upstream/main
        assert_eq!(grade_from_gates(&g), PassGrade::OkParentheses);
    }

    #[test]
    fn test_gs_high_below_new_threshold_is_ok() {
        // 0.4° high GS: below GS_SLIGHT_HIGH (0.5°) → OK.
        let g = gates_deg(0.4, 0.0, 0.0, 0.0, 0.0, 0.0);
        assert_eq!(grade_from_gates(&g), PassGrade::Ok);
    }

    #[test]
    fn test_slight_lu_deviation_is_ok_parentheses() {
        // 1.1° LU at half nm: exceeds LU_SLIGHT (1.0°) → (OK).
        let g = gates_deg(0.2, 0.3, 0.1, 1.1, 0.1, 0.1);
        assert_eq!(grade_from_gates(&g), PassGrade::OkParentheses);
    }

    #[test]
    fn test_significant_gs_deviation_is_no_grade() {
<<<<<<< HEAD
        // 1.1° GS at 3/4 nm: exceeds GS_SIGNIFICANT (1.0°) → --.
        let g = gates_deg(1.1, 0.3, 0.1, 0.2, 0.1, 0.1);
=======
        // 1.6° GS at 3/4 nm: exceeds GS_SIGNIFICANT (1.5°) → --.
        let g = gates_deg(1.6, 0.3, 0.1, 0.2, 0.1, 0.1);
>>>>>>> upstream/main
        assert_eq!(grade_from_gates(&g), PassGrade::NoGrade);
    }

    #[test]
    fn test_significant_lu_deviation_is_no_grade() {
        // 3.1° LU at 1/4 nm: exceeds LU_SIGNIFICANT (3.0°) → --.
        let g = gates_deg(0.2, 0.3, 0.1, 0.2, 0.1, 3.1);
        assert_eq!(grade_from_gates(&g), PassGrade::NoGrade);
    }

    #[test]
    fn test_medium_lu_deviation_is_no_grade() {
        // 2.1° LU at 3/4 nm: exceeds LU_MEDIUM (2.0°) → --.
        let g = gates_deg(0.0, 2.1, 0.0, 0.0, 0.0, 0.0);
        assert_eq!(grade_from_gates(&g), PassGrade::NoGrade);
    }

    #[test]
    fn test_below_medium_lu_is_ok_parentheses() {
        // 1.9° LU: above LU_SLIGHT but below LU_MEDIUM → (OK).
        let g = gates_deg(0.0, 1.9, 0.0, 0.0, 0.0, 0.0);
        assert_eq!(grade_from_gates(&g), PassGrade::OkParentheses);
    }

    #[test]
    fn test_dangerously_low_at_quarter_nm_is_cut() {
        // −2.6° GS at 1/4 nm: below GS_CUT_LOW_DEG (−2.5°) → Cut.
        let g = gates_deg(0.0, 0.0, 0.0, 0.0, -2.6, 0.0);
        assert_eq!(grade_from_gates(&g), PassGrade::Cut);
    }

    #[test]
    fn test_low_at_earlier_gates_not_cut() {
        // −2.6° GS only at 3/4 nm (not at 1/4 nm) → NoGrade, not Cut.
        let g = gates_deg(-2.6, 0.0, 0.0, 0.0, 0.0, 0.0);
        assert_eq!(grade_from_gates(&g), PassGrade::NoGrade);
    }

    #[test]
    fn test_bolter_outcome() {
        let g = gates_deg(0.2, 0.3, 0.1, 0.2, 0.1, 0.1);
        assert_eq!(compute_pass_grade(&Grading::Bolter, &g, None), PassGrade::Bolter);
    }

    #[test]
    fn test_waveoff_outcome() {
        let g = GateDeviations::default();
        assert_eq!(compute_pass_grade(&Grading::WaveoffPilot, &g, None), PassGrade::WaveoffPilot);
    }

    #[test]
    fn test_unicorn_wire3_correct_time() {
        // Zero deviations, wire 3, groove time in window → Unicorn.
        let g = gates_deg(0.1, 0.1, 0.1, 0.1, 0.1, 0.1);
        let grading = Grading::Recovered { cable: Some(3), cable_estimated: Some(3) };
        assert_eq!(compute_pass_grade(&grading, &g, Some(16.5)), PassGrade::Unicorn);
    }

    #[test]
    fn test_unicorn_wrong_wire() {
        // Zero deviations but wire 4 → OK, not Unicorn.
        let g = gates_deg(0.1, 0.1, 0.1, 0.1, 0.1, 0.1);
        let grading = Grading::Recovered { cable: Some(4), cable_estimated: Some(4) };
        assert_eq!(compute_pass_grade(&grading, &g, Some(16.5)), PassGrade::Ok);
    }

    #[test]
    fn test_unicorn_groove_time_too_short() {
        // Wire 3, no deviations, but groove time too short → OK.
        let g = gates_deg(0.1, 0.1, 0.1, 0.1, 0.1, 0.1);
        let grading = Grading::Recovered { cable: Some(3), cable_estimated: Some(3) };
        assert_eq!(compute_pass_grade(&grading, &g, Some(12.0)), PassGrade::Ok);
    }

    #[test]
    fn test_unicorn_groove_time_too_long() {
        // Wire 3, no deviations, but groove time too long → OK.
        let g = gates_deg(0.1, 0.1, 0.1, 0.1, 0.1, 0.1);
        let grading = Grading::Recovered { cable: Some(3), cable_estimated: Some(3) };
        assert_eq!(compute_pass_grade(&grading, &g, Some(22.0)), PassGrade::Ok);
    }

    #[test]
    fn test_unicorn_with_deviation_is_ok_not_unicorn() {
        // Slight deviation → (OK) base grade, can never be Unicorn.
        let g = gates_deg(0.9, 0.1, 0.1, 0.1, 0.1, 0.1);
        let grading = Grading::Recovered { cable: Some(3), cable_estimated: Some(3) };
        assert_eq!(compute_pass_grade(&grading, &g, Some(16.5)), PassGrade::OkParentheses);
    }

    #[test]
    fn test_unicorn_no_groove_time() {
        // groove_time_secs = None → OK, not Unicorn.
        let g = gates_deg(0.1, 0.1, 0.1, 0.1, 0.1, 0.1);
        let grading = Grading::Recovered { cable: Some(3), cable_estimated: Some(3) };
        assert_eq!(compute_pass_grade(&grading, &g, None), PassGrade::Ok);
    }

    #[test]
    fn test_points_bolter() {
        assert_eq!(PassGrade::Bolter.points(), 2.5);
    }

    #[test]
    fn test_points_waveoff() {
        assert_eq!(PassGrade::WaveoffPilot.points(), 1.0);
    }

    #[test]
    fn test_points_no_grade() {
        assert_eq!(PassGrade::NoGrade.points(), 2.0);
    }
<<<<<<< HEAD

    #[test]
    fn test_vstol_spot_thresholds() {
        assert_eq!(SpotGrade::from_distance_m(0.99), SpotGrade::A);
        assert_eq!(SpotGrade::from_distance_m(1.0), SpotGrade::B);
        assert_eq!(SpotGrade::from_distance_m(2.99), SpotGrade::B);
        assert_eq!(SpotGrade::from_distance_m(3.0), SpotGrade::C);
        assert_eq!(SpotGrade::from_distance_m(4.99), SpotGrade::C);
        assert_eq!(SpotGrade::from_distance_m(5.0), SpotGrade::D);
    }

    #[test]
    fn test_vstol_bonus_maps_to_catobar_labels() {
        assert_eq!(
            compute_vstol_final_grade_from_points(PassGrade::Ok.points(), SpotGrade::A),
            (PassGrade::Unicorn, 5.0)
        );
        assert_eq!(
            compute_vstol_final_grade_from_points(PassGrade::Ok.points(), SpotGrade::B),
            (PassGrade::Ok, 4.75)
        );
        assert_eq!(
            compute_vstol_final_grade_from_points(PassGrade::OkParentheses.points(), SpotGrade::A),
            (PassGrade::Ok, 4.0)
        );
        assert_eq!(
            compute_vstol_final_grade_from_points(PassGrade::NoGrade.points(), SpotGrade::C),
            (PassGrade::NoGrade, 2.5)
        );
    }

    #[test]
    fn test_vstol_approach_average_all_ok_is_ok() {
        let grading = Grading::Recovered { cable: None, cable_estimated: None };
        let ok = gates_deg(0.2, 0.3, -0.2, 0.4, 0.1, 0.2);
        let (grade, points) = compute_vstol_approach_grade_points(&grading, &ok);
        assert_eq!(grade, PassGrade::Ok);
        assert!((points - 4.0).abs() < 1e-9);
    }

    #[test]
    fn test_vstol_approach_average_single_significant_gate_stays_ok_parentheses() {
        let grading = Grading::Recovered { cable: None, cable_estimated: None };
        // 3/4 nm = -- (2.0), other two gates = OK (4.0) => average = 10 / 3.
        let mixed = gates_deg(1.1, 0.2, 0.1, 0.2, 0.1, 0.1);
        let (grade, points) = compute_vstol_approach_grade_points(&grading, &mixed);
        assert_eq!(grade, PassGrade::OkParentheses);
        assert!((points - (10.0 / 3.0)).abs() < 1e-9);
    }

    #[test]
    fn test_vstol_approach_average_single_slight_gate_stays_ok() {
        let grading = Grading::Recovered { cable: None, cable_estimated: None };
        // 3/4 nm = (OK) (3.0), other two gates = OK (4.0) => average = 11 / 3.
        let slight = gates_deg(0.6, 0.2, 0.1, 0.2, 0.1, 0.1);
        let (grade, points) = compute_vstol_approach_grade_points(&grading, &slight);
        assert_eq!(grade, PassGrade::Ok);
        assert!((points - (11.0 / 3.0)).abs() < 1e-9);
    }

    #[test]
    fn test_vstol_final_grade_uses_averaged_approach_points() {
        let (grade, points) = compute_vstol_final_grade_from_points(10.0 / 3.0, SpotGrade::B);
        assert_eq!(grade, PassGrade::Ok);
        assert!((points - ((10.0 / 3.0) + 0.75)).abs() < 1e-9);
    }

=======
>>>>>>> upstream/main
}
