macro_rules! test_recording {
    ($name:ident, $path:expr, $cable:expr, $cable_estimated:expr) => {
        #[test]
        #[tracing_test::traced_test]
        fn $name() {
            use std::io::Cursor;

            use crate::commands::file::extract_recoveries;
            use crate::track::{Grading, TrackResult};

            let acmi = include_bytes!($path);
            let recoveries = extract_recoveries(&mut Cursor::new(acmi)).unwrap();
            let [recovery]: [TrackResult; 1] = recoveries.try_into().unwrap();
            assert_eq!(
                recovery.grading,
                Grading::Recovered {
                    cable: $cable,
                    cable_estimated: $cable_estimated
                }
            );
        }
    };
}

test_recording!(
    wire_1_01,
    "../tests/recordings/wire_1_01_FA18C.zip.acmi",
    Some(1),
    None
);

test_recording!(
    wire_2_01,
    "../tests/recordings/wire_2_01_FA18C.zip.acmi",
    None,
    None
);

test_recording!(
    wire_3_01,
    "../tests/recordings/wire_3_01_T45.zip.acmi",
    Some(3),
    None
);

test_recording!(
    wire_4_01,
    "../tests/recordings/wire_4_01_FA18C.zip.acmi",
    None,
    None
);
test_recording!(
    wire_4_02,
    "../tests/recordings/wire_4_02_F14A.zip.acmi",
    None,
    None
);

/// Live recordings from the 2026-09-02/03 dedicated-server campaigns (T-45 and
/// F-14B(U) on CVN, DCS-gRPC 0.9.1 atomic acquisition). Each fixture is the
/// LSO-written ACMI plus a sidecar with the hook draw-argument timeline and the
/// pilot's real hook selection; the DCS `WIRE#` in the LSO comment is the label.
mod live_2026_09 {
    use std::io::Cursor;

    use crate::commands::file::extract_recoveries_with_hook;
    use crate::grading::PassGrade;
    use crate::track::{Grading, HookState, TrackResult};

    #[derive(serde::Deserialize)]
    struct Sidecar {
        aircraft_type: String,
        pilot_hook: String,
        dcs_wire: Option<u8>,
        hook_samples: Vec<(f64, f64)>,
    }

    fn replay(acmi: &[u8], sidecar: &str) -> (TrackResult, Sidecar) {
        let sidecar: Sidecar = serde_json::from_str(sidecar).expect("fixture sidecar");
        let recoveries =
            extract_recoveries_with_hook(&mut Cursor::new(acmi), &sidecar.hook_samples)
                .expect("replay fixture");
        assert_eq!(
            recoveries.len(),
            1,
            "{} recoveries in {}",
            recoveries.len(),
            sidecar.aircraft_type
        );
        (recoveries.into_iter().next().unwrap(), sidecar)
    }

    macro_rules! live_fixture {
        ($name:ident) => {
            #[test]
            #[tracing_test::traced_test]
            fn $name() {
                let (result, sidecar) = replay(
                    include_bytes!(concat!(
                        "../tests/recordings/live_2026-09/",
                        stringify!($name),
                        ".zip.acmi"
                    )),
                    include_str!(concat!(
                        "../tests/recordings/live_2026-09/",
                        stringify!($name),
                        ".hook.json"
                    )),
                );
                check(&result, &sidecar);
            }
        };
    }

    /// Ground-truth rules shared by every live fixture:
    /// - the commanded hook state must match what the pilot selected;
    /// - a DCS-labelled trap must be recovered on that wire, and the
    ///   independent estimate must agree with DCS;
    /// - a hook-up deck contact is a qualification touch-and-go, never a bolter.
    fn check(result: &TrackResult, sidecar: &Sidecar) {
        let expected_hook = match sidecar.pilot_hook.as_str() {
            "up" => HookState::Up,
            "down" => HookState::Down,
            other => panic!("unknown pilot hook state {other}"),
        };
        assert_eq!(
            result.hook_state, expected_hook,
            "{}",
            sidecar.aircraft_type
        );

        match (sidecar.dcs_wire, expected_hook) {
            (Some(wire), _) => {
                assert_eq!(
                    result.grading,
                    Grading::Recovered {
                        cable: Some(wire),
                        cable_estimated: Some(wire),
                    },
                    "{} DCS wire {wire}",
                    sidecar.aircraft_type
                );
                assert_eq!(result.arrest_evidence, "dcs_wire");
                assert!(
                    result.arrest_kinematics.confirmed,
                    "{:?}",
                    result.arrest_kinematics
                );
                assert_ne!(result.pass_grade, PassGrade::Incomplete);
            }
            (None, HookState::Up) => {
                assert!(
                    matches!(result.grading, Grading::TouchAndGo { .. }),
                    "{} hook-up pass graded {:?}",
                    sidecar.aircraft_type,
                    result.grading
                );
                assert_eq!(result.arrest_evidence, "none");
            }
            (None, _) => {
                assert_eq!(result.grading, Grading::Bolter, "{}", sidecar.aircraft_type);
                assert!(!result.arrest_kinematics.confirmed);
            }
        }
    }

    live_fixture!(t45_hookdown_bolter);
    live_fixture!(t45_hookdown_wire4);
    live_fixture!(t45_hookup_2);
    live_fixture!(t45_hookup_3);
    live_fixture!(t45_hookdown_wire1);
    live_fixture!(f14bu_hookup_3);
    live_fixture!(f14bu_hookup_4);
    live_fixture!(f14bu_hookdown_wire2);
    live_fixture!(f14bu_hookdown_wire4);
    live_fixture!(f14bu_hookup_1);
    live_fixture!(f14bu_hookup_2_dcs_waveoff);
    live_fixture!(f14bu_hookdown_wire1);
    live_fixture!(t45_hookup_1);
    live_fixture!(t45_hookdown_wire3);

    #[test]
    fn dcs_waveoff_ignored_by_a_hook_up_touch_and_go_is_a_cut() {
        let (result, _) = replay(
            include_bytes!("../tests/recordings/live_2026-09/f14bu_hookup_2_dcs_waveoff.zip.acmi"),
            include_str!("../tests/recordings/live_2026-09/f14bu_hookup_2_dcs_waveoff.hook.json"),
        );
        assert!(result
            .dcs_lso
            .as_ref()
            .is_some_and(|grade| grade.waveoff_ordered));
        assert_eq!(result.pass_grade, PassGrade::Cut);
    }

    /// Prints one line per fixture; run with `--nocapture` when tuning thresholds.
    #[test]
    fn live_fixture_summary() {
        for (name, acmi, sidecar) in [
            (
                "t45_hookdown_wire4",
                &include_bytes!("../tests/recordings/live_2026-09/t45_hookdown_wire4.zip.acmi")[..],
                include_str!("../tests/recordings/live_2026-09/t45_hookdown_wire4.hook.json"),
            ),
            (
                "f14bu_hookdown_wire2",
                &include_bytes!("../tests/recordings/live_2026-09/f14bu_hookdown_wire2.zip.acmi")[..],
                include_str!("../tests/recordings/live_2026-09/f14bu_hookdown_wire2.hook.json"),
            ),
        ] {
            let (result, _) = replay(acmi, sidecar);
            let series = result
                .deck_speed_series
                .iter()
                .map(|(t, v, x)| format!("{t:.1}:{v:.1}@{x:.0}"))
                .collect::<Vec<_>>()
                .join(" ");
            println!("{name} series: {series}");
            println!(
                "{name}: grading={:?} hook={:?} arrest={} kinematics={:?} wire_estimation={:?} grade={:?} completeness={:?}",
                result.grading,
                result.hook_state,
                result.arrest_evidence,
                result.arrest_kinematics,
                result.wire_estimation.reason,
                result.pass_grade,
                result.telemetry_quality.completeness
            );
        }
    }
}

/// Generate approach + pattern PNGs for every test recording and write them to
/// `target/test-charts/`. Run with:
///
///   cargo test generate_chart_images -- --nocapture
///
/// Then open the files in `target/test-charts/` to inspect the output visually.
#[test]
fn generate_chart_images() {
    use std::io::Cursor;

    use crate::commands::file::extract_recoveries;
    use crate::draw::{draw_chart, draw_pattern_chart};

    let recordings: &[(&str, &[u8])] = &[
        (
            "wire_1_01_FA18C",
            include_bytes!("../tests/recordings/wire_1_01_FA18C.zip.acmi"),
        ),
        (
            "wire_2_01_FA18C",
            include_bytes!("../tests/recordings/wire_2_01_FA18C.zip.acmi"),
        ),
        (
            "wire_3_01_T45",
            include_bytes!("../tests/recordings/wire_3_01_T45.zip.acmi"),
        ),
        (
            "wire_4_01_FA18C",
            include_bytes!("../tests/recordings/wire_4_01_FA18C.zip.acmi"),
        ),
        (
            "wire_4_02_F14A",
            include_bytes!("../tests/recordings/wire_4_02_F14A.zip.acmi"),
        ),
    ];

    let out_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join("test-charts");
    std::fs::create_dir_all(&out_dir).expect("failed to create target/test-charts/");

    for (name, acmi) in recordings {
        let recoveries = extract_recoveries(&mut Cursor::new(acmi))
            .unwrap_or_else(|e| panic!("failed to parse {name}: {e}"));

        for (i, track) in recoveries.iter().enumerate() {
            let filename = if recoveries.len() == 1 {
                name.to_string()
            } else {
                format!("{name}_{i}")
            };

            let approach_path = draw_chart(&out_dir, &filename, track)
                .unwrap_or_else(|e| panic!("draw_chart failed for {filename}: {e}"));
            println!("approach : {}", approach_path.display());

            let pattern_path = draw_pattern_chart(&out_dir, &filename, track)
                .unwrap_or_else(|e| panic!("draw_pattern_chart failed for {filename}: {e}"));
            println!("pattern  : {}", pattern_path.display());
        }
    }
}
