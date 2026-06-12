/// Glide-slope deviation thresholds (feet, absolute value) for pass grading.
const GS_SLIGHT: f64 = 40.0;
const GS_SIGNIFICANT: f64 = 100.0;
const GS_EXTREME: f64 = 200.0;

/// Lineup deviation thresholds (feet, absolute value) for pass grading.
const LU_SLIGHT: f64 = 25.0;
const LU_SIGNIFICANT: f64 = 60.0;
const LU_EXTREME: f64 = 120.0;

/// A dangerously low GS deviation at the 1/4-nm gate indicates a cut pass.
const GS_CUT_THRESHOLD: f64 = -150.0;

use crate::track::{GateDeviations, Grading};

/// Simplified NAVAIR 00-80T-104 pass grade.
///
/// Grades (descending quality):
/// - `Ok`            — 4 pts: average pass, minor deviations
/// - `OkParentheses` — 3 pts: slightly below average, safe
/// - `Fair`          — 2 pts: below average
/// - `NoGrade`       — 1 pt:  substandard / unsafe deviation
/// - `Cut`           — 0 pts: dangerously low at the ramp
/// - `Bolter`        — bolter, no points
/// - `WaveoffPilot`  — waveoff, no points
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum PassGrade {
    Ok,
    OkParentheses,
    Fair,
    NoGrade,
    Cut,
    Bolter,
    WaveoffPilot,
}

impl PassGrade {
    /// Short display label used in charts and the greenie board.
    pub fn label(self) -> &'static str {
        match self {
            Self::Ok => "OK",
            Self::OkParentheses => "(OK)",
            Self::Fair => "Fair",
            Self::NoGrade => "NG",
            Self::Cut => "Cut",
            Self::Bolter => "B",
            Self::WaveoffPilot => "WO",
        }
    }

}

/// Derive a `PassGrade` from the overall `Grading` outcome and the gate deviation data.
pub fn compute_pass_grade(grading: &Grading, gates: &GateDeviations) -> PassGrade {
    match grading {
        Grading::Unknown => PassGrade::NoGrade,
        Grading::WaveoffPilot => PassGrade::WaveoffPilot,
        Grading::Bolter => PassGrade::Bolter,
        Grading::Recovered { .. } => grade_from_gates(gates),
    }
}

fn grade_from_gates(gates: &GateDeviations) -> PassGrade {
    // Dangerously low at the ramp (1/4 nm) → cut pass.
    if let Some(g) = gates.at_quarter_nm.as_ref() {
        if g.gs_deviation_ft < GS_CUT_THRESHOLD {
            return PassGrade::Cut;
        }
    }

    let worst_gs = [
        gates.at_three_quarter_nm.as_ref().map(|g| g.gs_deviation_ft.abs()),
        gates.at_half_nm.as_ref().map(|g| g.gs_deviation_ft.abs()),
        gates.at_quarter_nm.as_ref().map(|g| g.gs_deviation_ft.abs()),
    ]
    .into_iter()
    .flatten()
    .fold(0.0_f64, f64::max);

    let worst_lu = [
        gates.at_three_quarter_nm.as_ref().map(|g| g.lineup_ft.abs()),
        gates.at_half_nm.as_ref().map(|g| g.lineup_ft.abs()),
        gates.at_quarter_nm.as_ref().map(|g| g.lineup_ft.abs()),
    ]
    .into_iter()
    .flatten()
    .fold(0.0_f64, f64::max);

    if worst_gs >= GS_EXTREME || worst_lu >= LU_EXTREME {
        PassGrade::NoGrade
    } else if worst_gs >= GS_SIGNIFICANT || worst_lu >= LU_SIGNIFICANT {
        PassGrade::Fair
    } else if worst_gs >= GS_SLIGHT || worst_lu >= LU_SLIGHT {
        PassGrade::OkParentheses
    } else {
        PassGrade::Ok
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::track::{GateDatum, GateDeviations};

    fn gates(gs_3q: f64, lu_3q: f64, gs_h: f64, lu_h: f64, gs_q: f64, lu_q: f64) -> GateDeviations {
        GateDeviations {
            at_three_quarter_nm: Some(GateDatum { gs_deviation_ft: gs_3q, lineup_ft: lu_3q }),
            at_half_nm:          Some(GateDatum { gs_deviation_ft: gs_h,  lineup_ft: lu_h  }),
            at_quarter_nm:       Some(GateDatum { gs_deviation_ft: gs_q,  lineup_ft: lu_q  }),
        }
    }

    #[test]
    fn test_perfect_pass_is_ok() {
        let g = gates(10.0, 5.0, 8.0, 3.0, 5.0, 2.0);
        assert_eq!(grade_from_gates(&g), PassGrade::Ok);
    }

    #[test]
    fn test_slight_deviation_is_ok_parentheses() {
        let g = gates(45.0, 5.0, 8.0, 3.0, 5.0, 2.0);
        assert_eq!(grade_from_gates(&g), PassGrade::OkParentheses);
    }

    #[test]
    fn test_significant_deviation_is_fair() {
        let g = gates(110.0, 5.0, 8.0, 3.0, 5.0, 2.0);
        assert_eq!(grade_from_gates(&g), PassGrade::Fair);
    }

    #[test]
    fn test_extreme_deviation_is_no_grade() {
        let g = gates(210.0, 5.0, 8.0, 3.0, 5.0, 2.0);
        assert_eq!(grade_from_gates(&g), PassGrade::NoGrade);
    }

    #[test]
    fn test_dangerously_low_at_ramp_is_cut() {
        let g = gates(0.0, 0.0, 0.0, 0.0, -160.0, 0.0);
        assert_eq!(grade_from_gates(&g), PassGrade::Cut);
    }

    #[test]
    fn test_bolter_outcome() {
        let g = gates(10.0, 5.0, 8.0, 3.0, 5.0, 2.0);
        assert_eq!(
            compute_pass_grade(&Grading::Bolter, &g),
            PassGrade::Bolter
        );
    }

    #[test]
    fn test_waveoff_outcome() {
        let g = GateDeviations::default();
        assert_eq!(
            compute_pass_grade(&Grading::WaveoffPilot, &g),
            PassGrade::WaveoffPilot
        );
    }
}
