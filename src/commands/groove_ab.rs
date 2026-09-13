//! Read-only offline comparison of a report's recorded groove entry with the current CATOBAR
//! Case I roll-out detector. It reuses persisted `datums` and the exact production geometry helper;
//! it never edits the input and cannot replay UTC mapping, RPC delivery, events or velocities.

use std::path::{Path, PathBuf};

use crate::data::{AirplaneInfo, CarrierInfo, CarrierRecovery};
use crate::grading::compute_pass_grade_with_reason;
use crate::track::{replay_gate_trajectory_and_groove, Grading, ReplaySample};

#[derive(clap::Parser)]
pub struct Opts {
    /// A JSON recovery report, or a directory searched recursively for JSON reports.
    input: PathBuf,
}

#[derive(serde::Deserialize)]
struct ReportInput {
    #[serde(default)]
    completed_at: String,
    #[serde(default)]
    recording_started_at: String,
    aircraft_type: String,
    carrier_type: String,
    touchdown_time_dcs: Option<f64>,
    groove_time_secs: Option<f64>,
    groove_entry: Option<RecordedGrooveEntry>,
    grading: serde_json::Value,
    pass_grade: String,
    #[serde(default)]
    trajectory_deviations: Vec<TrajectoryInput>,
    datums: Vec<DatumInput>,
}

#[derive(serde::Deserialize)]
struct RecordedGrooveEntry {
    timestamp_dcs: f64,
    distance_m: f64,
}

#[derive(serde::Deserialize)]
struct TrajectoryInput {
    timestamp_dcs: f64,
}

#[derive(serde::Deserialize)]
struct DatumInput {
    time: f64,
    x: f64,
    y: f64,
    alt: f64,
    #[serde(default = "default_true")]
    telemetry_valid: bool,
    #[serde(default)]
    skew_ms: f64,
    #[serde(default)]
    roll_deg: f64,
}

fn default_true() -> bool {
    true
}

pub fn execute(opts: Opts) -> Result<(), crate::error::Error> {
    let files = collect_json_files(&opts.input)?;
    println!(
        "report\trecorded_distance_m\tnew_distance_m\trecorded_entry_dcs\tnew_entry_dcs\trecorded_groove_s\tnew_groove_s\tgroove_delta_s\trecorded_grade\tnew_geometric_grade\tgrade_changed\tlineup_deg\tbank_deg\ttrack_deg\tinbound_progress_mps\trollout_started_dcs\trollout_s\tsamples\tport_corridor_reached\tadded_samples\tadded_max_abs_lineup_deg\tadded_max_abs_bank_deg\tadded_max_abs_track_deg"
    );
    for path in files {
        match analyze(&path) {
            Ok(Some(row)) => println!("{row}"),
            Ok(None) => eprintln!("{}: skipped (unsupported aircraft/carrier)", path.display()),
            Err(error) => eprintln!("{}: skipped ({error})", path.display()),
        }
    }
    Ok(())
}

fn analyze(path: &Path) -> Result<Option<String>, crate::error::Error> {
    let bytes = std::fs::read(path).map_err(|source| crate::error::Error::file_at(path, source))?;
    let input: ReportInput = serde_json::from_slice(&bytes)
        .map_err(|source| crate::error::Error::json_at(path, source))?;
    let Some(plane) = AirplaneInfo::by_type(&input.aircraft_type) else {
        return Ok(None);
    };
    let Some(carrier) = CarrierInfo::by_type(&input.carrier_type) else {
        return Ok(None);
    };
    let ideal_base_alt = match carrier.recovery {
        CarrierRecovery::Arrested => 0.0,
        CarrierRecovery::Vstol {
            target_altitude_ft, ..
        } => target_altitude_ft / 3.28084,
    };
    let recorded_entry = input
        .groove_entry
        .as_ref()
        .map(|entry| entry.timestamp_dcs)
        .or_else(|| {
            input
                .touchdown_time_dcs
                .zip(input.groove_time_secs)
                .map(|(touchdown, duration)| touchdown - duration)
        })
        .or_else(|| {
            input
                .trajectory_deviations
                .first()
                .map(|sample| sample.timestamp_dcs)
        });
    let recorded_distance = input
        .groove_entry
        .as_ref()
        .map(|entry| entry.distance_m)
        .or_else(|| {
            recorded_entry.and_then(|entry_time| {
                input
                    .datums
                    .iter()
                    .min_by(|left, right| {
                        (left.time - entry_time)
                            .abs()
                            .total_cmp(&(right.time - entry_time).abs())
                    })
                    .map(|datum| datum.x)
            })
        });
    let samples = input.datums.into_iter().map(|datum| ReplaySample {
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
    let report = if !input.recording_started_at.is_empty() {
        input.recording_started_at.clone()
    } else if input.completed_at.is_empty() {
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("unknown")
            .to_string()
    } else {
        input.completed_at
    };
    let fmt = |value: Option<f64>| {
        value
            .map(|value| format!("{value:.2}"))
            .unwrap_or_else(|| "N/A".to_string())
    };
    let grading = parse_grading(&input.grading);
    let new_entry_time = entry.as_ref().map(|evidence| evidence.timestamp_dcs);
    let new_duration = input
        .touchdown_time_dcs
        .zip(new_entry_time)
        .and_then(|(touchdown, entry)| (touchdown > entry).then_some(touchdown - entry));
    let (new_grade, _) =
        compute_pass_grade_with_reason(&grading, &gates, &trajectory, new_duration, new_entry_time);
    let recorded_grade = normalized_recorded_grade(&input.pass_grade);
    let duration_delta = new_duration
        .zip(input.groove_time_secs)
        .map(|(new, old)| new - old);
    let added = recorded_entry.map(|old_entry| {
        trajectory
            .iter()
            .filter(|sample| sample.timestamp_dcs < old_entry)
            .collect::<Vec<_>>()
    });
    let added_count = added.as_ref().map_or(0, |samples| samples.len());
    let added_max = |value: fn(&crate::track::TrajectoryDeviation) -> f64| {
        added.as_ref().and_then(|samples| {
            samples
                .iter()
                .map(|sample| value(sample).abs())
                .reduce(f64::max)
        })
    };
    let added_lineup = added_max(|sample| sample.lineup_deg);
    let added_bank = added_max(|sample| sample.bank_deg);
    let added_track = added_max(|sample| sample.track_angle_deg);
    let Some(entry) = entry else {
        return Ok(Some(format!(
            "{report}\t{}\tN/A\t{}\tN/A\t{}\tN/A\tN/A\t{}\t{}\t{}\tN/A\tN/A\tN/A\tN/A\tN/A\tN/A\tN/A\tN/A\t0\tN/A\tN/A\tN/A",
            fmt(recorded_distance),
            fmt(recorded_entry),
            fmt(input.groove_time_secs),
            recorded_grade,
            new_grade.label(),
            recorded_grade != new_grade.label(),
        )));
    };
    Ok(Some(format!(
        "{report}\t{}\t{:.1}\t{}\t{:.2}\t{}\t{}\t{}\t{}\t{}\t{}\t{:.2}\t{:.2}\t{:.2}\t{:.2}\t{:.2}\t{:.2}\t{}\t{}\t{}\t{}\t{}\t{}",
        fmt(recorded_distance),
        entry.distance_m,
        fmt(recorded_entry),
        entry.timestamp_dcs,
        fmt(input.groove_time_secs),
        fmt(new_duration),
        fmt(duration_delta),
        recorded_grade,
        new_grade.label(),
        recorded_grade != new_grade.label(),
        entry.lineup_deg,
        entry.bank_deg,
        entry.track_angle_deg,
        entry.inbound_progress_mps,
        entry.rollout_started_at_dcs,
        entry.stability_duration_s,
        entry.stability_sample_count,
        entry.port_lineup_corridor_reached,
        added_count,
        fmt(added_lineup),
        fmt(added_bank),
        fmt(added_track),
    )))
}

fn normalized_recorded_grade(grade: &str) -> &str {
    match grade {
        "Perfect" => "_OK_",
        "Ok" => "OK",
        "OkParentheses" => "(OK)",
        "NoGrade" => "--",
        "Cut" => "C",
        "Bolter" => "B",
        "WaveoffUnknown" => "WO?",
        "Incomplete" | "Unknown" => "NC",
        grade => grade,
    }
}

fn parse_grading(value: &serde_json::Value) -> Grading {
    if let Some(kind) = value.as_str() {
        return match kind {
            "Bolter" => Grading::Bolter,
            "WaveoffUnknown" => Grading::WaveoffUnknown,
            "ApproachOnly" => Grading::ApproachOnly,
            _ => Grading::Unknown,
        };
    }
    let Some(object) = value.as_object() else {
        return Grading::Unknown;
    };
    if let Some(recovered) = object.get("Recovered") {
        return Grading::Recovered {
            cable: recovered
                .get("cable")
                .and_then(serde_json::Value::as_u64)
                .and_then(|wire| u8::try_from(wire).ok()),
            cable_estimated: recovered
                .get("cable_estimated")
                .and_then(serde_json::Value::as_u64)
                .and_then(|wire| u8::try_from(wire).ok()),
        };
    }
    if let Some(touch_and_go) = object.get("TouchAndGo") {
        return Grading::TouchAndGo {
            cable_estimated: touch_and_go
                .get("cable_estimated")
                .and_then(serde_json::Value::as_u64)
                .and_then(|wire| u8::try_from(wire).ok()),
        };
    }
    Grading::Unknown
}

fn collect_json_files(input: &Path) -> Result<Vec<PathBuf>, crate::error::Error> {
    let metadata = std::fs::metadata(input)?;
    if metadata.is_file() {
        return Ok(vec![input.to_path_buf()]);
    }
    let mut files = Vec::new();
    let mut directories = vec![input.to_path_buf()];
    while let Some(directory) = directories.pop() {
        for entry in std::fs::read_dir(directory)? {
            let entry = entry?;
            let path = entry.path();
            if entry.file_type()?.is_dir() {
                directories.push(path);
            } else if path
                .extension()
                .is_some_and(|extension| extension == "json")
            {
                files.push(path);
            }
        }
    }
    files.sort();
    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_grading_preserves_the_outcome_needed_by_geometric_replay() {
        assert_eq!(parse_grading(&serde_json::json!("Bolter")), Grading::Bolter);
        assert_eq!(
            parse_grading(&serde_json::json!({
                "Recovered": { "cable": 3, "cable_estimated": 2 }
            })),
            Grading::Recovered {
                cable: Some(3),
                cable_estimated: Some(2),
            }
        );
        assert_eq!(
            parse_grading(&serde_json::json!({
                "TouchAndGo": { "cable_estimated": null }
            })),
            Grading::TouchAndGo {
                cable_estimated: None,
            }
        );
    }
}
