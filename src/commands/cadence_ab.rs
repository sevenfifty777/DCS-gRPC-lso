//! Offline A/B diagnostic for the "adaptive sampling cadence" question (B.2 of the
//! notation/cadence work plan): does artificially thinning the samples recorded *before* the
//! groove (the pattern/break portion, never the scored window) change the gates, the continuous
//! trajectory, or the resulting grade tier, compared to the full cadence actually recorded?
//!
//! This never touches the fork or a live DCS session: it replays the `datums` array already
//! persisted in existing JSON reports through the exact same gate/trajectory geometry
//! `Track::next` uses live (`track::replay_gate_and_trajectory`), once unmodified (the full-
//! cadence baseline) and once per requested stride, and reports what changed. It is purely a
//! measurement tool — it does not decide whether a reduced cadence should ever be promoted to
//! the live capture path, and it never writes back to the input files.

use std::path::{Path, PathBuf};

use crate::data::{AirplaneInfo, CarrierInfo, CarrierRecovery};
use crate::grading::grade_from_gates;
use crate::track::{replay_gate_and_trajectory, GateDeviations, ReplaySample};

#[derive(clap::Parser)]
pub struct Opts {
    /// A single JSON recovery report, or a directory searched recursively for `.json` reports.
    input: PathBuf,

    /// Outside-the-groove subsample strides to test (1-in-N samples kept before groove entry).
    /// The unmodified full cadence is always included as the baseline, regardless of this list.
    #[clap(long, num_args = 1.., default_values_t = [2, 4])]
    stride: Vec<u32>,
}

#[derive(serde::Deserialize)]
struct ReportInput {
    aircraft_type: String,
    carrier_type: String,
    datums: Vec<DatumInput>,
}

#[derive(serde::Deserialize, Clone, Copy)]
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
    if files.is_empty() {
        println!("No .json files found under {}", opts.input.display());
        return Ok(());
    }

    let mut strides = opts.stride;
    strides.retain(|&s| s > 1);
    strides.sort_unstable();
    strides.dedup();
    if strides.is_empty() {
        strides = vec![2, 4];
    }

    let mut summary = Summary::default();

    for path in &files {
        match analyze_file(path, &strides) {
            Ok(Some(report)) => {
                report.print(path);
                summary.record(&report);
            }
            Ok(None) => {
                println!(
                    "{}: skipped (unknown aircraft/carrier type)",
                    path.display()
                );
            }
            Err(err) => {
                println!("{}: skipped ({err})", path.display());
            }
        }
    }

    summary.print(files.len(), &strides);

    Ok(())
}

fn collect_json_files(input: &Path) -> Result<Vec<PathBuf>, crate::error::Error> {
    let metadata = std::fs::metadata(input)?;
    if metadata.is_file() {
        return Ok(vec![input.to_path_buf()]);
    }

    let mut files = Vec::new();
    let mut dirs = vec![input.to_path_buf()];
    while let Some(dir) = dirs.pop() {
        for entry in std::fs::read_dir(&dir)? {
            let entry = entry?;
            let path = entry.path();
            let file_type = entry.file_type()?;
            if file_type.is_dir() {
                dirs.push(path);
            } else if path.extension().is_some_and(|ext| ext == "json") {
                files.push(path);
            }
        }
    }
    files.sort();
    Ok(files)
}

fn analyze_file(path: &Path, strides: &[u32]) -> Result<Option<FileReport>, crate::error::Error> {
    let bytes = std::fs::read(path).map_err(|source| crate::error::Error::file_at(path, source))?;
    let input: ReportInput = serde_json::from_slice(&bytes)
        .map_err(|source| crate::error::Error::json_at(path, source))?;

    let Some(plane_info) = AirplaneInfo::by_type(&input.aircraft_type) else {
        return Ok(None);
    };
    let Some(carrier_info) = CarrierInfo::by_type(&input.carrier_type) else {
        return Ok(None);
    };
    let ideal_base_alt = match &carrier_info.recovery {
        CarrierRecovery::Arrested => 0.0,
        CarrierRecovery::Vstol {
            target_altitude_ft, ..
        } => *target_altitude_ft / 3.28084,
    };
    let is_vstol = carrier_info.is_vstol();

    let baseline = replay(
        &input.datums,
        1,
        ideal_base_alt,
        plane_info.glide_slope,
        is_vstol,
    );
    let variants = strides
        .iter()
        .map(|&stride| {
            (
                stride,
                replay(
                    &input.datums,
                    stride,
                    ideal_base_alt,
                    plane_info.glide_slope,
                    is_vstol,
                ),
            )
        })
        .collect();

    Ok(Some(FileReport { baseline, variants }))
}

/// Outside the scoring-relevant window (before groove entry and beyond the ¾ nm / 500 ft
/// envelope), keep only 1 in `stride` samples. `stride == 1` keeps everything, reproducing the
/// full recorded cadence exactly. This mirrors `PATTERN_DATUM_STRIDE`'s own condition
/// (`track.rs`) but is independent of it: the recording this reads may already have been
/// subsampled at capture time, so `stride` here is *additional* thinning on top of whatever the
/// input already has, not a from-scratch full-rate baseline.
fn replay(
    datums: &[DatumInput],
    stride: u32,
    ideal_base_alt: f64,
    glide_slope_deg: f64,
    is_vstol: bool,
) -> Variant {
    let mut kept = 0usize;
    // Counts only non-scoring-relevant samples, exactly like `Track`'s own
    // `pattern_datum_counter` (`track.rs`, `PATTERN_DATUM_STRIDE`): a scoring-relevant stretch
    // must never shift the phase of which *pattern* samples get kept afterwards.
    let mut pattern_counter: u32 = 0;
    let samples = datums.iter().filter_map(|d| {
        let scoring_relevant = d.x > 0.0
            && d.x <= crate::track::GATE_THREE_QUARTER_NM
            && crate::utils::m_to_ft(d.alt) <= 500.0;
        let keep = if scoring_relevant {
            true
        } else {
            let keep = pattern_counter.is_multiple_of(stride);
            pattern_counter = pattern_counter.wrapping_add(1);
            keep
        };
        if !keep {
            return None;
        }
        kept += 1;
        Some(ReplaySample {
            time: d.time,
            x: d.x,
            y: d.y,
            alt: d.alt,
            valid: d.telemetry_valid,
            skew_ms: d.skew_ms,
            roll_deg: d.roll_deg,
        })
    });

    let (gates, trajectory) =
        replay_gate_and_trajectory(samples, ideal_base_alt, glide_slope_deg, is_vstol);
    let grade = grade_from_gates(&gates, &trajectory);

    Variant {
        samples_kept: kept,
        samples_total: datums.len(),
        gates,
        trajectory_len: trajectory.len(),
        grade,
    }
}

struct Variant {
    samples_kept: usize,
    samples_total: usize,
    gates: GateDeviations,
    trajectory_len: usize,
    grade: crate::grading::PassGrade,
}

struct FileReport {
    baseline: Variant,
    variants: Vec<(u32, Variant)>,
}

impl FileReport {
    fn print(&self, path: &Path) {
        println!("{}", path.display());
        println!(
            "  stride=1 (baseline): {}/{} samples kept, trajectory={}, grade={}",
            self.baseline.samples_kept,
            self.baseline.samples_total,
            self.baseline.trajectory_len,
            self.baseline.grade.label(),
        );
        print_gate("    3/4 nm", &self.baseline.gates.at_three_quarter_nm);
        print_gate("    1/2 nm", &self.baseline.gates.at_half_nm);
        print_gate("    1/4 nm", &self.baseline.gates.at_quarter_nm);

        for (stride, variant) in &self.variants {
            let grade_changed = variant.grade != self.baseline.grade;
            println!(
                "  stride={stride}: {}/{} samples kept, trajectory={}, grade={}{}",
                variant.samples_kept,
                variant.samples_total,
                variant.trajectory_len,
                variant.grade.label(),
                if grade_changed {
                    " *** CHANGED ***"
                } else {
                    ""
                },
            );
            print_gate_diff(
                "    3/4 nm",
                &self.baseline.gates.at_three_quarter_nm,
                &variant.gates.at_three_quarter_nm,
            );
            print_gate_diff(
                "    1/2 nm",
                &self.baseline.gates.at_half_nm,
                &variant.gates.at_half_nm,
            );
            print_gate_diff(
                "    1/4 nm",
                &self.baseline.gates.at_quarter_nm,
                &variant.gates.at_quarter_nm,
            );
        }
    }
}

fn print_gate(label: &str, gate: &Option<crate::track::GateDatum>) {
    match gate {
        Some(g) => println!(
            "{label}: gs={:.3} deg, lu={:.3} deg, gap={:.0} ms",
            g.gs_deviation_deg, g.lineup_deg, g.sample_gap_ms
        ),
        None => println!("{label}: unavailable"),
    }
}

fn print_gate_diff(
    label: &str,
    baseline: &Option<crate::track::GateDatum>,
    variant: &Option<crate::track::GateDatum>,
) {
    match (baseline, variant) {
        (Some(b), Some(v)) => {
            let gs_delta = v.gs_deviation_deg - b.gs_deviation_deg;
            let lu_delta = v.lineup_deg - b.lineup_deg;
            let gap_flag = if v.sample_gap_ms > crate::telemetry::SAMPLE_GAP_WARNING_MS {
                " (gap exceeds 300 ms)"
            } else {
                ""
            };
            println!(
                "{label}: gs {:+.3} deg, lu {:+.3} deg, gap={:.0} ms{gap_flag}",
                gs_delta, lu_delta, v.sample_gap_ms
            );
        }
        (Some(_), None) => println!("{label}: became unavailable"),
        (None, Some(_)) => println!("{label}: became available (was unavailable at baseline)"),
        (None, None) => println!("{label}: still unavailable"),
    }
}

#[derive(Default)]
struct Summary {
    files_analyzed: usize,
    grade_changed: Vec<(usize, u32)>,
    gate_invalidated: Vec<(usize, u32)>,
}

impl Summary {
    fn record(&mut self, report: &FileReport) {
        let index = self.files_analyzed;
        self.files_analyzed += 1;
        for (stride, variant) in &report.variants {
            if variant.grade != report.baseline.grade {
                self.grade_changed.push((index, *stride));
            }
            let three_quarter_broke = report.baseline.gates.at_three_quarter_nm.is_some()
                && variant.gates.at_three_quarter_nm.is_none();
            if three_quarter_broke {
                self.gate_invalidated.push((index, *stride));
            }
        }
    }

    fn print(&self, files_found: usize, strides: &[u32]) {
        println!("---");
        println!(
            "Analyzed {}/{} file(s), strides tested: {:?}",
            self.files_analyzed, files_found, strides
        );
        println!(
            "Grade changed vs full-cadence baseline: {} occurrence(s)",
            self.grade_changed.len()
        );
        println!(
            "3/4-nm gate lost to the stride (was valid at baseline, unavailable after thinning): {} occurrence(s)",
            self.gate_invalidated.len()
        );
        if self.grade_changed.is_empty() && self.gate_invalidated.is_empty() {
            println!(
                "No change observed for the tested strides on this corpus. This is not proof \
                 a reduced cadence is safe in general -- only that it did not measurably affect \
                 these specific recordings."
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pattern_datum(time: f64, x: f64) -> DatumInput {
        // Outside the scoring-relevant window: far out and high, well beyond both the 1389 m
        // gate and the 500 ft approach-altitude ceiling.
        DatumInput {
            time,
            x,
            y: 0.0,
            alt: 400.0,
            telemetry_valid: true,
            skew_ms: 0.0,
            roll_deg: 0.0,
        }
    }

    fn scoring_datum(time: f64, x: f64) -> DatumInput {
        // Inside the scoring-relevant window: on-glideslope altitude for a 3.5 deg approach,
        // comfortably under both the 1389 m gate and the 500 ft ceiling.
        DatumInput {
            time,
            x,
            y: 0.0,
            alt: x * 3.5_f64.to_radians().tan(),
            telemetry_valid: true,
            skew_ms: 0.0,
            roll_deg: 0.0,
        }
    }

    #[test]
    fn stride_one_keeps_every_sample() {
        let datums: Vec<DatumInput> = (0..20)
            .map(|i| pattern_datum(i as f64 * 0.1, 2000.0 - i as f64 * 10.0))
            .collect();
        let variant = replay(&datums, 1, 0.0, 3.5, false);
        assert_eq!(variant.samples_kept, datums.len());
    }

    #[test]
    fn stride_never_thins_the_scoring_relevant_window() {
        // 10 pattern-phase samples (x > 1389 m, high) followed by 10 scoring-relevant samples
        // (x <= 1389 m, on glideslope). At stride 4, only the pattern samples should be thinned;
        // every scoring-relevant sample must survive regardless of the stride.
        let mut datums = Vec::new();
        for i in 0..10 {
            datums.push(pattern_datum(i as f64 * 0.1, 2000.0 - i as f64 * 20.0));
        }
        for i in 0..10 {
            datums.push(scoring_datum(
                1.0 + i as f64 * 0.1,
                1300.0 - i as f64 * 100.0,
            ));
        }

        let variant = replay(&datums, 4, 0.0, 3.5, false);

        // 1-in-4 of the 10 pattern samples (indices 0, 4, 8) plus all 10 scoring samples.
        assert_eq!(variant.samples_kept, 3 + 10);
    }

    #[test]
    fn a_scoring_relevant_stretch_does_not_shift_the_pattern_thinning_phase() {
        // Pattern samples before and after a scoring-relevant stretch must be thinned as if the
        // scoring-relevant samples were never there -- the pattern counter only advances on
        // non-scoring-relevant samples (see `pattern_counter` in `replay`).
        let mut datums = Vec::new();
        for i in 0..4 {
            datums.push(pattern_datum(i as f64 * 0.1, 2000.0 - i as f64 * 20.0));
        }
        for i in 0..50 {
            datums.push(scoring_datum(
                1.0 + i as f64 * 0.05,
                1300.0 - i as f64 * 20.0,
            ));
        }
        for i in 0..4 {
            datums.push(pattern_datum(
                10.0 + i as f64 * 0.1,
                1600.0 + i as f64 * 20.0,
            ));
        }

        let variant = replay(&datums, 2, 0.0, 3.5, false);

        // 1-in-2 of each 4-sample pattern group (indices 0,2 in each) = 2 + 2, plus all 50
        // scoring samples, regardless of how many scoring samples came between the two groups.
        assert_eq!(variant.samples_kept, 2 + 50 + 2);
    }

    #[test]
    fn collect_json_files_finds_nested_json_and_ignores_other_extensions() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("lso-cadence-ab-{unique}"));
        let nested = root.join("session-1");
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::write(root.join("a.json"), b"{}").unwrap();
        std::fs::write(nested.join("b.json"), b"{}").unwrap();
        std::fs::write(nested.join("notes.txt"), b"not json").unwrap();

        let mut files = collect_json_files(&root).unwrap();
        files.sort();

        assert_eq!(files, vec![root.join("a.json"), nested.join("b.json")]);

        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn collect_json_files_accepts_a_single_file() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("lso-cadence-ab-single-{unique}.json"));
        std::fs::write(&path, b"{}").unwrap();

        let files = collect_json_files(&path).unwrap();

        assert_eq!(files, vec![path.clone()]);

        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn unknown_aircraft_or_carrier_type_is_skipped_not_an_error() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("lso-cadence-ab-unknown-{unique}.json"));
        std::fs::write(
            &path,
            br#"{"aircraft_type":"NotARealJet","carrier_type":"CVN_71","datums":[]}"#,
        )
        .unwrap();

        let result = analyze_file(&path, &[2, 4]).unwrap();
        assert!(result.is_none());

        std::fs::remove_file(&path).unwrap();
    }
}
