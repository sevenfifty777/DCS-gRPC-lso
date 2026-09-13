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
                    cable_estimated: Some($cable_estimated)
                }
            );
        }
    };
}

test_recording!(
    wire_1_01,
    "../tests/recordings/wire_1_01_FA18C.zip.acmi",
    Some(1),
    1
);

test_recording!(
    wire_2_01,
    "../tests/recordings/wire_2_01_FA18C.zip.acmi",
    None,
    2
);

test_recording!(
    wire_3_01,
    "../tests/recordings/wire_3_01_T45.zip.acmi",
    Some(3),
    3
);

test_recording!(
    wire_4_01,
    "../tests/recordings/wire_4_01_FA18C.zip.acmi",
    None,
    4
);
test_recording!(
    wire_4_02,
    "../tests/recordings/wire_4_02_F14A.zip.acmi",
    None,
    4
);

/// Live recordings from the 2026-09-02/03 dedicated-server campaigns (T-45 and F-14B(U) on CVN,
/// DCS-gRPC 0.9.1 atomic acquisition), ported from `astra-review`. Each fixture is the
/// LSO-written ACMI plus a sidecar with the hook draw-argument timeline and the pilot's real hook
/// selection; the DCS `WIRE#` in the LSO comment is the label.
mod live_2026_09 {
    use std::io::Cursor;

    use crate::commands::file::{extract_recoveries_with_hook, ReplayOptions};
    use crate::grading::PassGrade;
    use crate::track::{Completeness, Grading, HookState, TrackResult};

    #[derive(serde::Deserialize)]
    struct Sidecar {
        aircraft_type: String,
        pilot_hook: String,
        dcs_wire: Option<u8>,
        hook_samples: Vec<(f64, f64)>,
    }

    fn replay_with(
        acmi: &[u8],
        sidecar: &str,
        hook_samples: bool,
        options: ReplayOptions,
    ) -> (TrackResult, Sidecar) {
        let sidecar: Sidecar = serde_json::from_str(sidecar).expect("fixture sidecar");
        let samples: &[(f64, f64)] = if hook_samples {
            &sidecar.hook_samples
        } else {
            &[]
        };
        let mut recoveries = extract_recoveries_with_hook(&mut Cursor::new(acmi), samples, options)
            .expect("replay fixture");
        // The labelled pass is the last attempt of the recording. Earlier attempts (a low pass
        // over the ship before the pattern, for instance) must never have produced a trap.
        let result = recoveries.pop().expect("at least one recovery attempt");
        for earlier in &recoveries {
            assert!(
                !matches!(earlier.grading, Grading::Recovered { .. }),
                "{}: earlier attempt graded {:?}",
                sidecar.aircraft_type,
                earlier.grading
            );
        }
        (result, sidecar)
    }

    fn replay(acmi: &[u8], sidecar: &str) -> (TrackResult, Sidecar) {
        replay_with(acmi, sidecar, true, ReplayOptions::default())
    }

    macro_rules! live_fixture {
        ($name:ident) => {
            mod $name {
                use super::*;

                const ACMI: &[u8] = include_bytes!(concat!(
                    "../tests/recordings/live_2026-09/",
                    stringify!($name),
                    ".zip.acmi"
                ));
                const SIDECAR: &str = include_str!(concat!(
                    "../tests/recordings/live_2026-09/",
                    stringify!($name),
                    ".hook.json"
                ));

                #[test]
                #[tracing_test::traced_test]
                fn with_dcs_wire_label() {
                    let (result, sidecar) = replay(ACMI, SIDECAR);
                    check(&result, &sidecar);
                }

                #[test]
                #[tracing_test::traced_test]
                fn without_dcs_wire_message_as_with_a_human_lso() {
                    let (result, sidecar) = replay_with(
                        ACMI,
                        SIDECAR,
                        true,
                        ReplayOptions {
                            ignore_dcs_grading: true,
                        },
                    );
                    check_without_dcs_wire(&result, &sidecar, "hook_transient");
                }

                #[test]
                #[tracing_test::traced_test]
                fn without_dcs_wire_message_and_without_hook_samples() {
                    let (result, sidecar) = replay_with(
                        ACMI,
                        SIDECAR,
                        false,
                        ReplayOptions {
                            ignore_dcs_grading: true,
                        },
                    );
                    check_without_dcs_wire(&result, &sidecar, "kinematic");
                }
            }
        };
    }

    /// Ground-truth rules shared by every live fixture replayed as recorded:
    /// - the commanded hook state must match what the pilot selected;
    /// - a DCS-labelled trap must be recovered on that wire, the independent estimate must agree
    ///   with DCS, and the deck kinematics must independently confirm the stop;
    /// - a hook-up deck contact is a qualification touch-and-go, never a bolter.
    fn check(result: &TrackResult, sidecar: &Sidecar) {
        let expected_hook = expected_hook(sidecar);
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
                    "{} DCS wire {wire}: {:?}",
                    sidecar.aircraft_type,
                    result.wire_estimation
                );
                assert_eq!(result.arrest_evidence, "dcs_wire");
                assert_eq!(
                    result.wire_estimation.reason,
                    "hook_deflection_correlated_with_wire_crossing"
                );
                assert!(
                    result.arrest_confirmation.deck_kinematics.confirmed,
                    "{:?}",
                    result.arrest_confirmation.deck_kinematics
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
                assert!(!result.arrest_confirmation.deck_kinematics.confirmed);
            }
        }
    }

    /// The human-LSO case: same recording, no DCS `WIRE#`. A labelled trap must still be graded
    /// from the independent evidence named by `expected_evidence` (`hook_transient` when the hook
    /// timeline is available, `kinematic` from deck kinematics alone), at medium confidence, and
    /// must never invent a wire the evidence does not support. Non-trap passes are unchanged.
    fn check_without_dcs_wire(result: &TrackResult, sidecar: &Sidecar, expected_evidence: &str) {
        match (sidecar.dcs_wire, expected_hook(sidecar)) {
            (Some(wire), _) => {
                assert_eq!(
                    result.arrest_evidence, expected_evidence,
                    "{} wire {wire}: {:?}",
                    sidecar.aircraft_type, result.arrest_confirmation
                );
                assert_eq!(result.arrest_confirmation.confidence, "medium");
                assert_eq!(
                    result.telemetry_quality.completeness,
                    Completeness::Complete,
                    "{:?}",
                    result.telemetry_quality.unavailability_causes
                );
                assert!(result.grade_points.is_some(), "{:?}", result.pass_grade);
                match result.grading {
                    Grading::Recovered {
                        cable: None,
                        cable_estimated,
                    } => {
                        if expected_evidence == "hook_transient" {
                            assert_eq!(cable_estimated, Some(wire), "{}", sidecar.aircraft_type);
                        } else if let Some(estimated) = cable_estimated {
                            assert_eq!(estimated, wire, "{}", sidecar.aircraft_type);
                        }
                    }
                    ref other => panic!("{} graded {other:?}", sidecar.aircraft_type),
                }
            }
            (None, HookState::Up) if expected_evidence == "hook_transient" => {
                assert!(
                    matches!(result.grading, Grading::TouchAndGo { .. }),
                    "{} hook-up pass graded {:?}",
                    sidecar.aircraft_type,
                    result.grading
                );
                assert_eq!(result.arrest_evidence, "none");
            }
            (None, HookState::Up) => {
                // Without any hook evidence a hook-up deck contact cannot be told apart from a
                // bolter, and a recording cut right after the contact leaves it `unconfirmed`;
                // what matters is that it is never promoted to a graded trap.
                assert!(!result.arrest_confirmation.deck_kinematics.confirmed);
                assert!(
                    matches!(result.arrest_evidence, "none" | "unconfirmed"),
                    "{} hook-up pass graded {:?} with evidence {}",
                    sidecar.aircraft_type,
                    result.grading,
                    result.arrest_evidence
                );
                if matches!(result.grading, Grading::Recovered { .. }) {
                    assert_eq!(
                        result.telemetry_quality.completeness,
                        Completeness::UnconfirmedArrest
                    );
                    assert_eq!(result.grade_points, None);
                }
            }
            (None, _) => {
                assert_eq!(result.grading, Grading::Bolter, "{}", sidecar.aircraft_type);
                assert_eq!(result.arrest_evidence, "none");
            }
        }
    }

    fn expected_hook(sidecar: &Sidecar) -> HookState {
        match sidecar.pilot_hook.as_str() {
            "up" => HookState::Up,
            "down" => HookState::Down,
            other => panic!("unknown pilot hook state {other}"),
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
