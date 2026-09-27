//! What the pilot actually flew, from the recorded geometry: an overhead pattern (break and
//! port turn to final) or a long straight-in final.
//!
//! Phase 5 of `docs/CASE_RECOVERY_DETECTION_PLAN_2026-09-26.md`. Pure, no I/O. Compared with the
//! ordered case to raise the `approach_does_not_match_ordered_case` diagnostic; it never changes
//! a grade (NATOPS does not grade the arrival). All thresholds are `PROJECT-DERIVED`.

use serde::Serialize;

use crate::track::Datum;

/// Wings level: the Case I roll-out bank limit (`GROOVE_ROLLOUT_MAX_BANK_DEG`).
pub const STRAIGHT_IN_MAX_ABS_BANK_DEG: f64 = 10.0;
/// On the extended centerline.
pub const STRAIGHT_IN_MAX_ABS_LINEUP_DEG: f64 = 5.0;
/// The wings-level final must already be established this far out: 2 NM.
pub const STRAIGHT_IN_MIN_START_DISTANCE_M: f64 = 3704.0;
/// ...and still be wings level here: 3/4 NM, where Case III grading starts.
pub const STRAIGHT_IN_MAX_END_DISTANCE_M: f64 = 1389.0;
/// A capture gap longer than this breaks the segment rather than bridging it.
pub const STRAIGHT_IN_MAX_SAMPLE_GAP_S: f64 = 1.0;
/// Outbound motion tolerated between two samples before the segment breaks (noise).
const INBOUND_TOLERANCE_M: f64 = 1.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FlownApproach {
    /// The Case I detector saw the port pattern, armed the last turn below 600 ft and confirmed
    /// the roll-out.
    OverheadPattern,
    /// A continuous wings-level, inbound, on-centerline final from beyond 2 NM to 3/4 NM.
    StraightIn,
    Unknown,
}

impl FlownApproach {
    pub fn as_str(self) -> &'static str {
        match self {
            FlownApproach::OverheadPattern => "overhead_pattern",
            FlownApproach::StraightIn => "straight_in",
            FlownApproach::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct StraightSegment {
    pub start_time_dcs: f64,
    pub end_time_dcs: f64,
    pub start_distance_m: f64,
    pub end_distance_m: f64,
    pub max_abs_bank_deg: f64,
    pub max_abs_lineup_deg: f64,
    pub sample_count: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct FlownApproachCriteria {
    pub max_abs_bank_deg: f64,
    pub max_abs_lineup_deg: f64,
    pub min_start_distance_m: f64,
    pub max_end_distance_m: f64,
    pub max_sample_gap_s: f64,
}

impl Default for FlownApproachCriteria {
    fn default() -> Self {
        Self {
            max_abs_bank_deg: STRAIGHT_IN_MAX_ABS_BANK_DEG,
            max_abs_lineup_deg: STRAIGHT_IN_MAX_ABS_LINEUP_DEG,
            min_start_distance_m: STRAIGHT_IN_MIN_START_DISTANCE_M,
            max_end_distance_m: STRAIGHT_IN_MAX_END_DISTANCE_M,
            max_sample_gap_s: STRAIGHT_IN_MAX_SAMPLE_GAP_S,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FlownApproachEvidence {
    pub kind: FlownApproach,
    pub case_i_groove_entry_confirmed: bool,
    /// The longest wings-level, inbound, on-centerline segment of the recording, qualifying or
    /// not; absent when no sample met those conditions.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub longest_straight_segment: Option<StraightSegment>,
    pub criteria: FlownApproachCriteria,
}

/// Classify the approach flown. A qualifying straight-in final takes precedence over a Case I
/// groove confirmation: the Case I detector could arm on a correction turn inside 3/4 NM, after
/// the long wings-level final that defines a straight-in.
pub fn classify_flown_approach(
    datums: &[Datum],
    case_i_groove_entry_confirmed: bool,
) -> FlownApproachEvidence {
    let segments = straight_segments(datums);
    let qualifies = |s: &StraightSegment| {
        s.start_distance_m >= STRAIGHT_IN_MIN_START_DISTANCE_M
            && s.end_distance_m <= STRAIGHT_IN_MAX_END_DISTANCE_M
    };
    let kind = if segments.iter().any(qualifies) {
        FlownApproach::StraightIn
    } else if case_i_groove_entry_confirmed {
        FlownApproach::OverheadPattern
    } else {
        FlownApproach::Unknown
    };
    let longest_straight_segment = segments.into_iter().max_by(|a, b| {
        (a.start_distance_m - a.end_distance_m).total_cmp(&(b.start_distance_m - b.end_distance_m))
    });
    FlownApproachEvidence {
        kind,
        case_i_groove_entry_confirmed,
        longest_straight_segment,
        criteria: FlownApproachCriteria::default(),
    }
}

fn is_straight(datum: &Datum) -> bool {
    datum.telemetry_valid
        && datum.x > 0.0
        && datum.roll_deg.abs() <= STRAIGHT_IN_MAX_ABS_BANK_DEG
        && lineup_deg(datum).abs() <= STRAIGHT_IN_MAX_ABS_LINEUP_DEG
}

fn lineup_deg(datum: &Datum) -> f64 {
    datum.y.atan2(datum.x).to_degrees()
}

/// Maximal runs of consecutive straight, inbound samples without a long capture gap.
fn straight_segments(datums: &[Datum]) -> Vec<StraightSegment> {
    let mut segments = Vec::new();
    let mut current: Option<StraightSegment> = None;
    let mut previous: Option<&Datum> = None;
    for datum in datums {
        let straight = is_straight(datum);
        let continues = previous.is_some_and(|p| {
            datum.time > p.time
                && datum.time - p.time <= STRAIGHT_IN_MAX_SAMPLE_GAP_S
                && datum.x <= p.x + INBOUND_TOLERANCE_M
        });
        if !straight || !continues {
            if let Some(segment) = current.take() {
                segments.push(segment);
            }
        }
        if straight {
            let bank = datum.roll_deg.abs();
            let lineup = lineup_deg(datum).abs();
            match current.as_mut() {
                Some(segment) => {
                    segment.end_time_dcs = datum.time;
                    segment.end_distance_m = datum.x;
                    segment.max_abs_bank_deg = segment.max_abs_bank_deg.max(bank);
                    segment.max_abs_lineup_deg = segment.max_abs_lineup_deg.max(lineup);
                    segment.sample_count += 1;
                }
                None => {
                    current = Some(StraightSegment {
                        start_time_dcs: datum.time,
                        end_time_dcs: datum.time,
                        start_distance_m: datum.x,
                        end_distance_m: datum.x,
                        max_abs_bank_deg: bank,
                        max_abs_lineup_deg: lineup,
                        sample_count: 1,
                    })
                }
            }
        }
        previous = Some(datum);
    }
    if let Some(segment) = current {
        segments.push(segment);
    }
    segments
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Inbound samples at 10 Hz and 70 m/s from `from_m` to `to_m`, with `bank(x)` and a lateral
    /// offset `y(x)`.
    fn final_approach(
        from_m: f64,
        to_m: f64,
        bank: impl Fn(f64) -> f64,
        y: impl Fn(f64) -> f64,
    ) -> Vec<Datum> {
        let step = 7.0;
        let mut datums = Vec::new();
        let mut x = from_m;
        let mut t = 100.0;
        while x >= to_m {
            datums.push(Datum {
                time: t,
                x,
                y: y(x),
                roll_deg: bank(x),
                telemetry_valid: true,
                ..Datum::default()
            });
            x -= step;
            t += 0.1;
        }
        datums
    }

    #[test]
    fn long_wings_level_final_is_straight_in() {
        let datums = final_approach(5000.0, 300.0, |_| 2.0, |_| 10.0);
        let evidence = classify_flown_approach(&datums, false);
        assert_eq!(evidence.kind, FlownApproach::StraightIn);
        let segment = evidence.longest_straight_segment.unwrap();
        assert!(segment.start_distance_m >= 4990.0);
        assert!(segment.end_distance_m <= 310.0);
    }

    #[test]
    fn straight_in_wins_over_a_late_case_i_arming() {
        // Wings level from 5,000 m, then a correction turn inside 3/4 NM.
        let datums = final_approach(
            5000.0,
            300.0,
            |x| if x < 900.0 && x > 700.0 { 15.0 } else { 2.0 },
            |_| 0.0,
        );
        assert_eq!(
            classify_flown_approach(&datums, true).kind,
            FlownApproach::StraightIn
        );
    }

    #[test]
    fn short_final_after_a_turn_is_overhead_when_case_i_confirmed() {
        // Turning final until 1,200 m, then wings level: no straight segment from 2 NM.
        let datums = final_approach(
            3000.0,
            200.0,
            |x| if x > 1200.0 { 25.0 } else { 1.0 },
            |_| 0.0,
        );
        assert_eq!(
            classify_flown_approach(&datums, true).kind,
            FlownApproach::OverheadPattern
        );
        assert_eq!(
            classify_flown_approach(&datums, false).kind,
            FlownApproach::Unknown
        );
    }

    #[test]
    fn a_turn_between_2_nm_and_3_4_nm_breaks_the_straight_in() {
        let datums = final_approach(
            5000.0,
            300.0,
            |x| {
                if (2000.0..2500.0).contains(&x) {
                    20.0
                } else {
                    2.0
                }
            },
            |_| 0.0,
        );
        assert_eq!(
            classify_flown_approach(&datums, false).kind,
            FlownApproach::Unknown
        );
    }

    #[test]
    fn off_centerline_final_is_not_straight_in() {
        // 8 degrees left of centerline all the way.
        let datums = final_approach(5000.0, 300.0, |_| 0.0, |x| x * 8_f64.to_radians().tan());
        assert_eq!(
            classify_flown_approach(&datums, false).kind,
            FlownApproach::Unknown
        );
    }

    #[test]
    fn final_starting_inside_2_nm_is_not_straight_in() {
        let datums = final_approach(3000.0, 300.0, |_| 0.0, |_| 0.0);
        assert_eq!(
            classify_flown_approach(&datums, false).kind,
            FlownApproach::Unknown
        );
    }

    #[test]
    fn a_capture_gap_breaks_the_segment() {
        let mut datums = final_approach(5000.0, 300.0, |_| 0.0, |_| 0.0);
        let mid = datums.len() / 2;
        for datum in &mut datums[mid..] {
            datum.time += 2.0;
        }
        assert_eq!(
            classify_flown_approach(&datums, false).kind,
            FlownApproach::Unknown
        );
    }

    #[test]
    fn invalid_samples_break_the_segment() {
        let mut datums = final_approach(5000.0, 300.0, |_| 0.0, |_| 0.0);
        let mid = datums.len() / 2;
        datums[mid].telemetry_valid = false;
        assert_eq!(
            classify_flown_approach(&datums, false).kind,
            FlownApproach::Unknown
        );
    }

    #[test]
    fn empty_recording_is_unknown() {
        let evidence = classify_flown_approach(&[], false);
        assert_eq!(evidence.kind, FlownApproach::Unknown);
        assert_eq!(evidence.longest_straight_segment, None);
    }
}
