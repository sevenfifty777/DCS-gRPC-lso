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
