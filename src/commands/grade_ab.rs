//! PROTOTYPE (branch `feature/ramp-aoa-grading-prototype`): read-only offline re-grading of
//! recorded JSON reports under the four `CatobarGradingPolicy` steps (baseline, then each of the
//! three candidate switches added cumulatively). Reuses persisted `datums` and the exact production
//! geometry helper, exactly like `groove-ab`; it never edits the input and never affects live
//! grading. Output is a Markdown table so it can be pasted into a review document as-is.

use std::path::{Path, PathBuf};

use crate::data::{AirplaneInfo, CarrierInfo, CarrierRecovery};
use crate::grading::{
    compute_catobar_assessment_with_policy, CatobarAssessment, CatobarEvidence,
    CatobarGradingPolicy, GradingAxis,
};
use crate::track::{
    replay_gate_trajectory_and_groove, Datum, GateCaptureMethod, GateDatum, GateDeviations,
    GateQuality, GateStatus, ReplaySample, TrajectoryDeviation,
};

use super::groove_ab::{collect_json_files, normalized_recorded_grade, parse_grading};

#[derive(clap::Parser)]
pub struct Opts {
    /// A JSON recovery report, or a directory searched recursively for JSON reports.
    input: PathBuf,
    /// Also list every grading episode of every report under each policy step (one indented
    /// line per episode after the table row).
    #[clap(long)]
    episodes: bool,
}

#[derive(serde::Deserialize)]
struct ReportInput {
    #[serde(default)]
    recording_started_at: String,
    aircraft_type: String,
    carrier_type: String,
    touchdown_time_dcs: Option<f64>,
    grading: serde_json::Value,
    pass_grade: String,
    #[serde(default)]
    outcome: String,
    #[serde(default)]
    dcs_grading: Option<String>,
    #[serde(default)]
    wind_reference_established: bool,
    groove_time_secs: Option<f64>,
    groove_entry: Option<GrooveEntryInput>,
    /// Recorded gates and continuous trajectory: the exact evidence the live grader saw. Used
    /// in preference to a geometry replay from `datums`, whose `alt` is clamped at zero in the
    /// report and therefore softens the last metres before touchdown.
    #[serde(default)]
    gate_deviations: Option<GateDeviationsInput>,
    #[serde(default)]
    trajectory_deviations: Vec<TrajectoryInput>,
    datums: Vec<DatumInput>,
}

#[derive(serde::Deserialize)]
struct GrooveEntryInput {
    timestamp_dcs: f64,
}

#[derive(serde::Deserialize)]
struct GateDeviationsInput {
    at_three_quarter_nm: Option<GateInput>,
    at_half_nm: Option<GateInput>,
    at_quarter_nm: Option<GateInput>,
    three_quarter_quality: GateQualityInput,
    half_quality: GateQualityInput,
    quarter_quality: GateQualityInput,
}

#[derive(serde::Deserialize)]
struct GateInput {
    gs_deviation_deg: f64,
    lineup_deg: f64,
    #[serde(default)]
    gs_deviation_ft: f64,
    #[serde(default)]
    lineup_ft: f64,
    timestamp_dcs: f64,
    distance_m: f64,
    #[serde(default)]
    sample_gap_ms: f64,
    #[serde(default)]
    skew_ms: f64,
    #[serde(default = "interpolated")]
    method: String,
}

fn interpolated() -> String {
    "interpolated".to_string()
}

#[derive(serde::Deserialize)]
struct GateQualityInput {
    status: String,
    #[serde(default)]
    bracket_gap_ms: Option<f64>,
    #[serde(default)]
    bracket_start_time_dcs: Option<f64>,
    #[serde(default)]
    bracket_end_time_dcs: Option<f64>,
}

#[derive(serde::Deserialize)]
struct TrajectoryInput {
    timestamp_dcs: f64,
    distance_m: f64,
    gs_deviation_deg: f64,
    lineup_deg: f64,
    #[serde(default)]
    lineup_deviation_m: f64,
    #[serde(default)]
    track_angle_deg: f64,
    #[serde(default)]
    alt_m: f64,
    #[serde(default)]
    bank_deg: f64,
    #[serde(default)]
    sink_rate_mps: f64,
}

impl GateInput {
    fn into_gate(self) -> GateDatum {
        GateDatum {
            gs_deviation_deg: self.gs_deviation_deg,
            lineup_deg: self.lineup_deg,
            gs_deviation_ft: self.gs_deviation_ft,
            lineup_ft: self.lineup_ft,
            timestamp_dcs: self.timestamp_dcs,
            distance_m: self.distance_m,
            sample_gap_ms: self.sample_gap_ms,
            skew_ms: self.skew_ms,
            method: if self.method == "measured" {
                GateCaptureMethod::Measured
            } else {
                GateCaptureMethod::Interpolated
            },
        }
    }
}

impl GateQualityInput {
    fn into_quality(self) -> GateQuality {
        GateQuality {
            status: match self.status.as_str() {
                "valid" => GateStatus::Valid,
                "late" => GateStatus::Late,
                "missing" => GateStatus::Missing,
                _ => GateStatus::Invalid,
            },
            reason: None,
            bracket_gap_ms: self.bracket_gap_ms,
            bracket_start_time_dcs: self.bracket_start_time_dcs,
            bracket_end_time_dcs: self.bracket_end_time_dcs,
            coverage_source: None,
        }
    }
}

impl GateDeviationsInput {
    fn into_gates(self) -> GateDeviations {
        GateDeviations {
            at_three_quarter_nm: self.at_three_quarter_nm.map(GateInput::into_gate),
            at_half_nm: self.at_half_nm.map(GateInput::into_gate),
            at_quarter_nm: self.at_quarter_nm.map(GateInput::into_gate),
            three_quarter_quality: self.three_quarter_quality.into_quality(),
            half_quality: self.half_quality.into_quality(),
            quarter_quality: self.quarter_quality.into_quality(),
        }
    }
}

#[derive(serde::Deserialize)]
struct DatumInput {
    time: f64,
    x: f64,
    y: f64,
    alt: f64,
    #[serde(default = "nan")]
    aoa: f64,
    #[serde(default = "default_true")]
    telemetry_valid: bool,
    #[serde(default)]
    skew_ms: f64,
    #[serde(default)]
    roll_deg: f64,
}

fn nan() -> f64 {
    f64::NAN
}

fn default_true() -> bool {
    true
}

/// The four cumulative policy steps compared by this command, in column order.
pub(crate) const STEPS: [(&str, CatobarGradingPolicy); 4] = [
    ("P0 baseline", CatobarGradingPolicy::BASELINE),
    (
        "P1 +touchdown ends correction",
        CatobarGradingPolicy {
            touchdown_ends_correction_assessment: true,
            aoa_min_episode_duration_s: 0.0,
            aoa_requires_calibrated_type: false,
        },
    ),
    (
        "P2 +AoA 1 s persistence",
        CatobarGradingPolicy {
            touchdown_ends_correction_assessment: true,
            aoa_min_episode_duration_s: 1.0,
            aoa_requires_calibrated_type: false,
        },
    ),
    (
        "P3 +AoA calibrated types only",
        CatobarGradingPolicy::PROTOTYPE,
    ),
];

pub fn execute(opts: Opts) -> Result<(), crate::error::Error> {
    let files = collect_json_files(&opts.input)?;
    println!("{}", table_header());
    for path in files {
        match analyze(&path, opts.episodes) {
            Ok(Some(row)) => println!("{row}"),
            Ok(None) => eprintln!("{}: skipped (unsupported aircraft/carrier)", path.display()),
            Err(error) => eprintln!("{}: skipped ({error})", path.display()),
        }
    }
    Ok(())
}

pub(crate) fn table_header() -> String {
    let mut header = String::from("| report | type | outcome | DCS LSO | recorded |");
    let mut rule = String::from("|---|---|---|---|---|");
    for (label, _) in STEPS {
        header.push_str(&format!(" {label} |"));
        rule.push_str("---|");
    }
    format!("{header}\n{rule}")
}

/// Indented listing of a policy step's reason and every episode it produced, for `--episodes`
/// and the live-fixture table test.
pub(crate) fn episode_listing(
    policy: &CatobarGradingPolicy,
    assessment: &CatobarAssessment,
) -> String {
    let step = STEPS
        .iter()
        .find(|(_, candidate)| candidate == policy)
        .map_or("?", |(label, _)| label);
    let mut lines = format!("\n    {step}: {}", assessment.reason);
    for episode in &assessment.episodes {
        lines.push_str(&format!(
            "\n      {:?} {} max={:?} corr={:?} ({}) eff={:.1} dur={:.2}s peak={:.2}@{:.2} affects={}{}",
            episode.axis,
            episode.most_severe_zone.label(),
            episode.maximum_severity,
            episode.correction,
            episode.correction_reason,
            episode.effective_severity,
            episode.duration_s,
            episode.peak_value,
            episode.peak_at_dcs,
            episode.affects_grade,
            episode
                .diagnostic
                .map_or(String::new(), |diagnostic| format!(" [{diagnostic}]")),
        ));
    }
    lines
}

/// One Markdown cell per policy step: the grade, then the axis, zone and effective severity of
/// the episode that decided it (blank when no scoring episode remains).
pub(crate) fn policy_cells(
    mut grade_under: impl FnMut(&CatobarGradingPolicy) -> CatobarAssessment,
) -> String {
    let mut cells = String::new();
    for (_, policy) in STEPS {
        let assessment = grade_under(&policy);
        let worst = assessment
            .episodes
            .iter()
            .filter(|episode| episode.affects_grade)
            .max_by(|left, right| {
                left.effective_severity
                    .total_cmp(&right.effective_severity)
                    .then_with(|| left.maximum_severity.cmp(&right.maximum_severity))
            });
        let detail = worst.map_or(String::new(), |episode| {
            format!(
                " ({} {} {:.1})",
                match episode.axis {
                    GradingAxis::Glideslope => "GS",
                    GradingAxis::Lineup => "LU",
                    GradingAxis::Aoa => "AoA",
                },
                episode.most_severe_zone.label().to_lowercase(),
                episode.effective_severity
            )
        });
        cells.push_str(&format!(" `{}`{detail} |", assessment.grade.label()));
    }
    cells
}

fn analyze(path: &Path, list_episodes: bool) -> Result<Option<String>, crate::error::Error> {
    let bytes = std::fs::read(path).map_err(|source| crate::error::Error::file_at(path, source))?;
    let mut input: ReportInput = serde_json::from_slice(&bytes)
        .map_err(|source| crate::error::Error::json_at(path, source))?;
    let Some(plane) = AirplaneInfo::by_type(&input.aircraft_type) else {
        return Ok(None);
    };
    let Some(carrier) = CarrierInfo::by_type(&input.carrier_type) else {
        return Ok(None);
    };
    if carrier.is_vstol() {
        return Ok(None);
    }
    let ideal_base_alt = match carrier.recovery {
        CarrierRecovery::Arrested => 0.0,
        CarrierRecovery::Vstol {
            target_altitude_ft, ..
        } => target_altitude_ft / 3.28084,
    };
    // Recorded evidence first (exact live gates/trajectory/groove entry); geometry replay from
    // `datums` only for a report that carries no continuous trajectory at all.
    let (gates, trajectory, entry_time, groove_time_secs, evidence_source) = match (
        input.gate_deviations.take(),
        input.trajectory_deviations.is_empty(),
    ) {
        (Some(gates), false) => (
            gates.into_gates(),
            std::mem::take(&mut input.trajectory_deviations)
                .into_iter()
                .map(|sample| TrajectoryDeviation {
                    timestamp_dcs: sample.timestamp_dcs,
                    distance_m: sample.distance_m,
                    gs_deviation_deg: sample.gs_deviation_deg,
                    lineup_deg: sample.lineup_deg,
                    lineup_deviation_m: sample.lineup_deviation_m,
                    track_angle_deg: sample.track_angle_deg,
                    alt_m: sample.alt_m,
                    bank_deg: sample.bank_deg,
                    sink_rate_mps: sample.sink_rate_mps,
                })
                .collect::<Vec<_>>(),
            input.groove_entry.as_ref().map(|entry| entry.timestamp_dcs),
            input.groove_time_secs,
            "recorded",
        ),
        _ => {
            let samples = input.datums.iter().map(|datum| ReplaySample {
                time: datum.time,
                x: datum.x,
                y: datum.y,
                alt: datum.alt,
                valid: datum.telemetry_valid,
                skew_ms: datum.skew_ms,
                roll_deg: datum.roll_deg,
            });
            let (gates, trajectory, entry) = replay_gate_trajectory_and_groove(
                samples,
                ideal_base_alt,
                plane.glide_slope,
                carrier.is_vstol(),
            );
            let entry_time = entry.as_ref().map(|evidence| evidence.timestamp_dcs);
            let groove_time_secs = input
                .touchdown_time_dcs
                .zip(entry_time)
                .and_then(|(touchdown, entry)| (touchdown > entry).then_some(touchdown - entry));
            (gates, trajectory, entry_time, groove_time_secs, "replayed")
        }
    };
    // Only `time`, `x` and `aoa` feed the AoA classifier; the rest is filler.
    let datums = input
        .datums
        .iter()
        .map(|datum| Datum {
            time: datum.time,
            corrected_time_dcs: datum.time,
            x: datum.x,
            y: datum.y,
            aoa: datum.aoa,
            alt: datum.alt,
            roll_deg: datum.roll_deg,
            carrier_time: datum.time,
            plane_time: datum.time,
            carrier_received_unix_ms: 0,
            plane_received_unix_ms: 0,
            sample_gap_ms: 0.0,
            capture_gap_ms: 0.0,
            delivery_age_ms: 0.0,
            skew_ms: datum.skew_ms,
            alignment: Default::default(),
            telemetry_valid: datum.telemetry_valid,
            raw_carrier_position: [0.0; 3],
            corrected_carrier_position: [0.0; 3],
            filtered_carrier_position: [0.0; 3],
        })
        .collect::<Vec<_>>();
    let grading = parse_grading(&input.grading);
    let mut episode_lines = String::new();
    let cells = policy_cells(|policy| {
        let assessment = compute_catobar_assessment_with_policy(
            CatobarEvidence {
                grading: &grading,
                gates: &gates,
                trajectory: &trajectory,
                datums: &datums,
                plane_info: plane,
                aoa_reliable: input.wind_reference_established,
                groove_time_secs,
                groove_entry_time: entry_time,
            },
            policy,
        );
        if list_episodes {
            episode_lines.push_str(&episode_listing(policy, &assessment));
        }
        assessment
    });
    let report = if input.recording_started_at.is_empty() {
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("unknown")
            .to_string()
    } else {
        input.recording_started_at.clone()
    };
    let dcs = input
        .dcs_grading
        .as_deref()
        .and_then(|comment| comment.split_once("GRADE:"))
        .map(|(_, rest)| rest.split_whitespace().next().unwrap_or("").to_string())
        .unwrap_or_else(|| "none".to_string());
    Ok(Some(format!(
        "| {report} | {} | {} | {dcs} | `{}` ({evidence_source}) |{cells}{episode_lines}",
        input.aircraft_type,
        input.outcome,
        normalized_recorded_grade(&input.pass_grade),
    )))
}
