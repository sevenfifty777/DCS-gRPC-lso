use std::borrow::Cow;
use std::ops::{Neg, Range};
use std::path::PathBuf;

use image::imageops::FilterType;
use image::ImageFormat;
use plotters::coord::combinators::WithKeyPoints;
use plotters::coord::ranged1d::ValueFormatter;
use plotters::coord::types::RangedCoordf64;
use plotters::coord::Shift;
use plotters::prelude::*;
use plotters::style::{Color, IntoFont, RGBColor, TextStyle};
use plotters_bitmap::bitmap_pixel::RGBPixel;
use plotters_bitmap::BitMapBackend;

use crate::data::{AirplaneInfo, Aoa, CarrierRecovery};
use crate::track::{Datum, GateDatum, GateQuality, GateStatus, Grading, PatternDatum, TrackResult};
use crate::utils::{ft_to_nm, m_to_ft, m_to_nm, nm_to_ft, nm_to_m};

const THEME_BG: RGBColor = RGBColor(31, 41, 55); // 1F2937
const THEME_FG: RGBColor = RGBColor(156, 163, 175); // 9CA3AF

const THEME_GUIDE_RED: RGBColor = RGBColor(239, 68, 68); // EF4444
const THEME_GUIDE_YELLOW: RGBColor = RGBColor(254, 240, 138); // FEF08A
const THEME_GUIDE_GREEN: RGBColor = RGBColor(34, 197, 94); // 22C55E
const THEME_GUIDE_GRAY: RGBColor = RGBColor(100, 116, 139); // 64748B

const THEME_AOA_FAST: RGBColor = RGBColor(239, 68, 68); // EF4444
const THEME_AOA_SLIGHTLY_FAST: RGBColor = RGBColor(239, 165, 68); // EFA544
const THEME_AOA_ON_SPEED: RGBColor = RGBColor(254, 240, 138); // FEF08A
const THEME_AOA_SLIGHTLY_SLOW: RGBColor = RGBColor(170, 197, 34); // AAC522
const THEME_AOA_SLOW: RGBColor = RGBColor(34, 197, 94); // 22C55E

const WIDTH: u32 = 1000;
const X_LABEL_AREA_SIZE: u32 = 30;
const RANGE_X: Range<f64> = -0.02..0.78;
const FINAL_APPROACH_ALT_RANGE: Range<f64> = 0.0..500.0;
const MIN_FINAL_SPAN_NM: f64 = 0.20;
// Give the Tarawa artwork real room to the left of x=0.  In V1.8 the
// calibrated bitmap anchor sat outside the -0.02 nm native CATOBAR viewport,
// so Plotters clipped the sprite at the left border even though the 7.5 pixel
// reference itself was correct.  CATOBAR keeps RANGE_X unchanged.
const VSTOL_RANGE_X: Range<f64> = -0.16..0.78;
const TOP_RANGE_Y: Range<f64> = -0.15..0.15;
const SIDE_RANGE_Y: Range<f64> = 0.0..350.0;
const VSTOL_SIDE_RANGE_Y: Range<f64> = 0.0..500.0;
// Keep the vertical-profile and lineup plots in independent panels. Overlapping
// them makes unrelated traces appear to join into a false vertical excursion.
const PANEL_GAP: u32 = 16;
const VSTOL_VERTICAL_PLOT_HEIGHT: u32 = 500;
const VSTOL_HORIZONTAL_PLOT_HEIGHT: u32 = 300;

fn chart_layout(is_vstol: bool) -> (u32, u32, u32) {
    if is_vstol {
        let root_height = VSTOL_VERTICAL_PLOT_HEIGHT
            + PANEL_GAP
            + VSTOL_HORIZONTAL_PLOT_HEIGHT
            + X_LABEL_AREA_SIZE;
        (
            root_height,
            VSTOL_VERTICAL_PLOT_HEIGHT,
            VSTOL_VERTICAL_PLOT_HEIGHT + PANEL_GAP,
        )
    } else {
        let side_height = ((ft_to_nm(SIDE_RANGE_Y.end - SIDE_RANGE_Y.start) * 5.0
            / (RANGE_X.end - RANGE_X.start))
            * f64::from(WIDTH))
        .floor() as u32;
        let top_height = (((TOP_RANGE_Y.end - TOP_RANGE_Y.start) / (RANGE_X.end - RANGE_X.start))
            * f64::from(WIDTH))
        .floor() as u32;

        (
            top_height + side_height + PANEL_GAP + X_LABEL_AREA_SIZE,
            side_height,
            side_height + PANEL_GAP,
        )
    }
}

// Tarawa recovery-artwork calibration. The *_REF_PX coordinates come from the
// exact user-supplied full-ship copies carrying the pink spot-7.5 marker.
// Recovery and pattern assets use separate filenames so changing one renderer
// cannot silently change the other.
const TARAWA_TOP_REF_PX: (f64, f64) = (210.5, 38.5);
const TARAWA_SIDE_REF_PX: (f64, f64) = (224.95, 46.55);

// V/STOL-only visual sizes. Both recovery sprites were prepared from 300 px
// long source images so keep a shared on-screen width for the side and top
// panels. This preserves the same longitudinal Tarawa scale across both
// V/STOL recovery views while keeping independent 7.5 anchor offsets.
const TARAWA_RECOVERY_DISPLAY_WIDTH_PX: u32 = 220;
const TARAWA_TOP_DISPLAY_WIDTH_PX: u32 = TARAWA_RECOVERY_DISPLAY_WIDTH_PX;
const TARAWA_SIDE_DISPLAY_WIDTH_PX: u32 = TARAWA_RECOVERY_DISPLAY_WIDTH_PX;

// Top-view visual calibration.  The Tarawa sprite is intentionally kept large
// enough to remain readable.  Therefore the artwork is treated as a schematic
// overlay in Y: the 7.5 pixel stays exactly on x=0, while the port deck edge and
// hover datum are separated using the same proportions as the real geometry.
// On the full top-view PNG, the 7.5 marker is centred at (210.5, 38.5) px and
// the port flight-deck edge at that station is about source y=62.5 px. The
// hover axis is one AV-8B wingspan (9.24 m) farther port.
const TARAWA_TOP_PORT_EDGE_PX_Y: f64 = 62.5;
const AV8B_WINGSPAN_M: f64 = 9.24;

// V/STOL terminal-phase display calibration. The upper panel keeps the long
// approach in real altitude, then remaps only the hover-to-deck phase so the
// user-marked 7.5 deck point sits 50 ft below the 120-ft hover reference.
const VSTOL_TERMINAL_DESCENT_DISPLAY_FT: f64 = 50.0;
const VSTOL_MARKER_RADIUS_PX: i32 = 3;

fn themed_png_from_bytes(bytes: &[u8]) -> Result<image::DynamicImage, DrawError> {
    // The user-provided Tarawa PNGs already contain an alpha channel. Composite
    // that alpha over the graph theme instead of chroma-keying black pixels;
    // this preserves the genuinely dark parts of the hull/deck.
    let img = image::load_from_memory_with_format(bytes, ImageFormat::Png)?.into_rgba8();
    let mut bg = image::RgbaImage::from_pixel(
        img.width(),
        img.height(),
        image::Rgba([THEME_BG.0, THEME_BG.1, THEME_BG.2, 255]),
    );
    image::imageops::overlay(&mut bg, &img, 0, 0);
    Ok(image::DynamicImage::ImageRgba8(bg))
}

/// Small owned copy helper for rendering-only approach selections.
fn copy_datum(d: &Datum) -> Datum {
    d.clone()
}

/// Select the single continuous final-approach branch used by both plots.
///
/// The endpoint is interpolated exactly at x=0 when the aircraft crosses the
/// touchdown/reference station. Keeping this as one time- and position-continuous
/// run prevents an earlier overhead crossing from being joined to the real final.
fn select_final_approach_datums(track: &TrackResult) -> Vec<Datum> {
    if track.carrier_info.is_vstol() {
        select_vstol_final_datums(&track.datums)
    } else {
        select_catobar_final_datums(&track.datums)
    }
}

/// Preserve the original V/STOL policy: prefer the longest continuous branch
/// that reaches the abeam-7.5 station, with the latest branch winning a span tie.
fn select_vstol_final_datums(datums: &[Datum]) -> Vec<Datum> {
    continuous_final_runs(datums)
        .into_iter()
        .enumerate()
        .filter(|(_, run)| {
            inbound_span_nm(run) >= MIN_FINAL_SPAN_NM
                && run
                    .last()
                    .map(|datum| m_to_nm(datum.x))
                    .unwrap_or(f64::INFINITY)
                    <= 0.01
        })
        .max_by(|(index_a, run_a), (index_b, run_b)| {
            inbound_span_nm(run_a)
                .partial_cmp(&inbound_span_nm(run_b))
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| index_a.cmp(index_b))
        })
        .map(|(_, run)| run)
        .unwrap_or_default()
}

/// CATOBAR grading belongs to the terminal recovery, so prefer the latest
/// substantial inbound branch and retain a shorter fallback for early waveoffs.
fn select_catobar_final_datums(datums: &[Datum]) -> Vec<Datum> {
    let mut runs = continuous_final_runs(datums);
    let selected_index = runs
        .iter()
        .rposition(|run| inbound_span_nm(run) >= MIN_FINAL_SPAN_NM)
        .or_else(|| runs.iter().rposition(|run| inbound_span_nm(run) > 0.0));

    selected_index
        .map(|index| runs.remove(index))
        .unwrap_or_default()
}

fn inbound_span_nm(run: &[Datum]) -> f64 {
    run.first()
        .zip(run.last())
        .map(|(first, last)| m_to_nm(first.x - last.x))
        .unwrap_or(0.0)
}

fn continuous_final_runs(datums: &[Datum]) -> Vec<Vec<Datum>> {
    const MAX_DT_S: f64 = 1.0;
    const MAX_STEP_M: f64 = 60.0;
    const MAX_X_BACKTRACK_M: f64 = 20.0;

    let mut runs: Vec<Vec<Datum>> = Vec::new();
    let mut current: Vec<Datum> = Vec::new();

    let finish_run = |current: &mut Vec<Datum>, runs: &mut Vec<Vec<Datum>>| {
        if current.len() >= 2 {
            runs.push(std::mem::take(current));
        } else {
            current.clear();
        }
    };

    for d in datums {
        let x_nm = m_to_nm(d.x);
        let y_nm = m_to_nm(d.y);
        let alt_ft = m_to_ft(d.alt);
        let common_window = x_nm <= RANGE_X.end
            && TOP_RANGE_Y.contains(&y_nm)
            && FINAL_APPROACH_ALT_RANGE.contains(&alt_ft);

        if current.is_empty() {
            // A final branch must start on the approach side of the abeam station.
            if common_window && x_nm >= 0.0 {
                current.push(copy_datum(d));
            }
            continue;
        }

        let prev = current.last().unwrap();
        let dt = d.time - prev.time;
        let dx = d.x - prev.x;
        let dy = d.y - prev.y;
        let step_m = (dx * dx + dy * dy).sqrt();
        let continuous = common_window
            && dt > 0.0
            && dt <= MAX_DT_S
            && step_m <= MAX_STEP_M
            // On final x should decrease. A small increase is tolerated for
            // carrier-position smoothing / DCS sampling jitter.
            && dx <= MAX_X_BACKTRACK_M;

        if !continuous {
            finish_run(&mut current, &mut runs);
            if common_window && x_nm >= 0.0 {
                current.push(copy_datum(d));
            }
            continue;
        }

        // Exact crossing of the reference station. Keep a synthetic sample at
        // x=0 using linear interpolation between the last inbound sample and
        // the first sample at/past the station, then close the final branch.
        if prev.x > 0.0 && d.x <= 0.0 {
            // Copy the previous sample values before mutating `current`.
            let prev_time = prev.time;
            let prev_x = prev.x;
            let prev_y = prev.y;
            let prev_aoa = prev.aoa;
            let prev_alt = prev.alt;
            let denom = prev_x - d.x;
            let t = if denom.abs() > f64::EPSILON {
                (prev_x / denom).clamp(0.0, 1.0)
            } else {
                0.0
            };
            current.push(Datum {
                time: prev_time + (d.time - prev_time) * t,
                x: 0.0,
                y: prev_y + (d.y - prev_y) * t,
                aoa: prev_aoa + (d.aoa - prev_aoa) * t,
                alt: prev_alt + (d.alt - prev_alt) * t,
                ..copy_datum(prev)
            });
            finish_run(&mut current, &mut runs);
            continue;
        }

        if d.x >= 0.0 {
            current.push(copy_datum(d));
        } else {
            finish_run(&mut current, &mut runs);
        }
    }
    finish_run(&mut current, &mut runs);
    runs
}

/// Post-x=0 V/STOL lateral translation, for the horizontal chart only.
///
/// A good side-step toward the deck should appear as an almost vertical trace
/// close to x=0: lateral position changes while longitudinal station remains
/// nearly constant. Keeping this phase separate avoids reintroducing the old
/// artificial "spike" at the beginning of final while still letting the pilot
/// inspect whether the hover translation was straight and stable.
fn select_vstol_translation_datums(track: &TrackResult, final_run: &[Datum]) -> Vec<Datum> {
    const MAX_DT_S: f64 = 1.0;
    const MAX_STEP_M: f64 = 30.0;
    const MAX_ABS_X_NM: f64 = 0.03;

    let Some(endpoint) = final_run.last() else {
        return Vec::new();
    };

    let mut out = vec![copy_datum(endpoint)];
    let mut prev_time = endpoint.time;
    let mut prev_x = endpoint.x;
    let mut prev_y = endpoint.y;

    for d in track.datums.iter().filter(|d| d.time > endpoint.time) {
        let x_nm = m_to_nm(d.x);
        let y_nm = m_to_nm(d.y);
        let dt = d.time - prev_time;
        let dx = d.x - prev_x;
        let dy = d.y - prev_y;
        let step_m = (dx * dx + dy * dy).sqrt();

        let in_translation_window = x_nm.abs() <= MAX_ABS_X_NM
            && TOP_RANGE_Y.contains(&y_nm)
            && dt > 0.0
            && dt <= MAX_DT_S
            && step_m <= MAX_STEP_M;

        if !in_translation_window {
            // Once the translation has started, any discontinuity / large
            // longitudinal departure ends this display segment.
            if out.len() > 1 {
                break;
            }
            continue;
        }

        out.push(copy_datum(d));
        prev_time = d.time;
        prev_x = d.x;
        prev_y = d.y;
    }

    if out.len() <= 1 {
        return Vec::new();
    }

    // If the track ends with the synthetic DCS land-event datum, use that as
    // the authoritative terminal point even when its timestamp is not strictly
    // greater than the immediately preceding sample (rounding can produce a
    // tiny non-monotonic tail such as 279.87 -> 279.86). Also clamp any
    // overshoot beyond that touchdown x-position so the displayed side-step
    // stays smooth and ends at the true final spot rather than on a jitter
    // excursion a little farther forward.
    if let Some(last_raw) = track.datums.last() {
        let x_nm = m_to_nm(last_raw.x);
        let y_nm = m_to_nm(last_raw.y);
        let close_enough = out
            .last()
            .map(|last| {
                let dx = last_raw.x - last.x;
                let dy = last_raw.y - last.y;
                (dx * dx + dy * dy).sqrt() <= MAX_STEP_M * 2.0
            })
            .unwrap_or(false);

        if last_raw.time >= endpoint.time
            && x_nm.abs() <= MAX_ABS_X_NM
            && TOP_RANGE_Y.contains(&y_nm)
            && close_enough
        {
            for datum in out.iter_mut().skip(1) {
                if datum.x < last_raw.x {
                    datum.x = last_raw.x;
                }
            }
            if let Some(last) = out.last_mut() {
                *last = copy_datum(last_raw);
            }
        }
    }

    out
}

fn side_range_y(track: &TrackResult) -> Range<f64> {
    if track.carrier_info.is_vstol() {
        VSTOL_SIDE_RANGE_Y
    } else {
        SIDE_RANGE_Y
    }
}

fn recovery_label(grading: &Grading, is_vstol: bool) -> Cow<'static, str> {
    match grading {
        Grading::Unknown => Cow::Borrowed(""),
        Grading::Bolter => Cow::Borrowed("Bolter"),
        Grading::WaveoffUnknown => Cow::Borrowed("Waveoff (initiator unknown)"),
        Grading::TouchAndGo { .. } => Cow::Borrowed("T&G (CQ)"),
        Grading::Recovered {
            cable,
            cable_estimated,
        } => {
            if is_vstol {
                Cow::Borrowed("V/STOL recovery")
            } else {
                match crate::track::select_wire_for_display(*cable_estimated, *cable) {
                    (Some(wire), "estimated") => Cow::Owned(format!("Wire {} (estimated)", wire)),
                    (Some(wire), "dcs") => Cow::Owned(format!("Wire {} (DCS)", wire)),
                    _ => Cow::Borrowed("(failed to detect cable)"),
                }
            }
        }
    }
}

#[tracing::instrument(skip_all)]
pub fn draw_chart(
    out_dir: &std::path::Path,
    filename: &str,
    track: &TrackResult,
) -> Result<PathBuf, DrawError> {
    let path = out_dir.join(filename).with_extension("png");

    // Both recovery types use independent vertical-profile and lineup panels.
    // Their plot heights retain the original axis aspect ratios.
    let (root_height, side_height, top_start) = chart_layout(track.carrier_info.is_vstol());

    let root_drawing_area = BitMapBackend::new(&path, (WIDTH, root_height)).into_drawing_area();
    root_drawing_area.fill(&THEME_BG)?;

    let (side, _) = root_drawing_area.split_vertically(side_height);
    let (_, top) = root_drawing_area.split_vertically(top_start);

    // Both functions already branch internally on CarrierRecovery. For CATOBAR
    // they therefore retain the original carrier images and guide geometry; for
    // V/STOL they use the Tarawa / parallel-axis references.
    draw_side_view(track, side)?;
    draw_top_view(track, top)?;

    let sep_y = side_height + PANEL_GAP / 2;
    root_drawing_area.draw(&PathElement::new(
        vec![(0, sep_y as i32), (WIDTH as i32, sep_y as i32)],
        THEME_GUIDE_GRAY.mix(0.35),
    ))?;

    let text_style = TextStyle::from(("sans-serif", 24).into_font()).color(&THEME_FG);

    root_drawing_area.draw_text(
        &format!("Pilot: {}", track.pilot_name),
        &text_style,
        (16, 16),
    )?;

    let grade_points_text = match track.grade_points {
        Some(points) if track.carrier_info.is_vstol() => format!("{points:.2}"),
        Some(points) => format!("{points:.1}"),
        None => "no".to_string(),
    };
    root_drawing_area.draw_text(
        &format!(
            "Grade: {}  ({} pts)",
            track.pass_grade.label(),
            grade_points_text
        ),
        &text_style,
        (16, 48),
    )?;

    root_drawing_area.draw_text(
        &format!("Aircraft: {}", track.plane_info.name),
        &text_style,
        (16, 80),
    )?;

    root_drawing_area.draw_text(
        &recovery_label(&track.grading, track.carrier_info.is_vstol()),
        &text_style,
        (16, 112),
    )?;

    let text_style_small = TextStyle::from(("sans-serif", 18).into_font()).color(&THEME_FG);
    for (index, (label, gate, quality)) in [
        (
            "3/4nm",
            track.gate_deviations.at_three_quarter_nm.as_ref(),
            &track.gate_deviations.three_quarter_quality,
        ),
        (
            "1/2nm",
            track.gate_deviations.at_half_nm.as_ref(),
            &track.gate_deviations.half_quality,
        ),
        (
            "1/4nm",
            track.gate_deviations.at_quarter_nm.as_ref(),
            &track.gate_deviations.quarter_quality,
        ),
    ]
    .iter()
    .enumerate()
    {
        let (label, gate, quality) = (*label, *gate, *quality);
        let y_pos = 144 + (index as i32) * 28;
        root_drawing_area.draw_text(
            &format!(
                "{}: {}",
                label,
                fmt_gate(
                    gate,
                    quality,
                    track.telemetry_quality.max_sample_gap_ms,
                    track.carrier_info.is_vstol(),
                )
            ),
            &text_style_small,
            (16, y_pos),
        )?;
    }

    if !track.carrier_info.is_vstol() {
        let fragment_count = select_catobar_display_runs(&track.datums).len();
        if fragment_count > 1 {
            root_drawing_area.draw_text(
                &format!(
                    "TRACE PARTIAL — {} fragments, gaps non raccordés",
                    fragment_count
                ),
                &text_style_small,
                (16, 232),
            )?;
        }
    }

    if track.carrier_info.is_vstol() {
        if let (Some(spot_grade), Some(distance_m)) = (track.spot_grade, track.spot_distance_m) {
            root_drawing_area.draw_text(
                &format!(
                    "Spot 7.5: {}  {:.2}m  +{:.2}pt",
                    spot_grade.label(),
                    distance_m,
                    spot_grade.bonus_points()
                ),
                &text_style_small,
                (16, 228),
            )?;
        }
    }

    std::mem::drop(root_drawing_area);

    Ok(path)
}

#[tracing::instrument(skip_all)]
pub fn draw_top_view(
    track: &TrackResult,
    canvas: DrawingArea<BitMapBackend<'_, RGBPixel>, Shift>,
) -> Result<(), DrawError> {
    let chart_range_x = if track.carrier_info.is_vstol() {
        VSTOL_RANGE_X
    } else {
        RANGE_X
    };
    let mut chart = ChartBuilder::on(&canvas)
        .margin(0u32)
        .x_label_area_size(X_LABEL_AREA_SIZE)
        .y_label_area_size(0u32)
        .build_cartesian_2d(
            CustomRange(
                chart_range_x
                    .clone()
                    .with_key_points(vec![0.25f64, 0.5, 0.75, 1.0]),
            ),
            TOP_RANGE_Y,
        )?;

    // Then we can draw a mesh
    chart
        .configure_mesh()
        .disable_mesh()
        .disable_y_axis()
        .axis_style(THEME_FG)
        .x_label_style(text_style())
        .draw()?;

    let mut vstol_visual_ref_y_nm: Option<f64> = None;

    if let CarrierRecovery::Vstol {
        landing_point,
        approach_axis_port_m,
        ..
    } = &track.carrier_info.recovery
    {
        // V/STOL keeps the native CATOBAR visual grammar but uses the user-provided
        // Tarawa top-down artwork.  The pink-square reference supplied by the user
        // was used off-line to calibrate the spot-7.5 pixel location; the production
        // image added to /img is the clean version without the marker.
        let _ship_axis_y = m_to_nm(*approach_axis_port_m);

        let (plot_w, plot_h) = chart.plotting_area().dim_in_pixel();
        let x_nm_per_px = (chart_range_x.end - chart_range_x.start) / f64::from(plot_w.max(1));
        let y_nm_per_px = (TOP_RANGE_Y.end - TOP_RANGE_Y.start) / f64::from(plot_h.max(1));

        let img = themed_png_from_bytes(include_bytes!("../img/tarawa-vstol-recovery-top.png"))?;
        let src_w = img.width() as f64;
        let src_h = img.height() as f64;
        let display_w = TARAWA_TOP_DISPLAY_WIDTH_PX;
        let display_h = ((src_h / src_w) * f64::from(display_w)).round() as u32;
        let resized = img.resize_exact(display_w, display_h.max(1), FilterType::CatmullRom);

        // Longitudinal calibration remains exact: the user-marked 7.5 pixel is
        // placed on x=0.  Laterally, keep the readable sprite size and position
        // it so the y=0 convergence point is visibly over the sea, one AV-8B
        // wingspan outside the port deck edge, while remaining perpendicular to
        // the 7.5 station.
        let scale = f64::from(display_h.max(1)) / src_h;
        let ref_x_px = TARAWA_TOP_REF_PX.0 / src_w * f64::from(display_w);
        let ref_y_px = TARAWA_TOP_REF_PX.1 * scale;
        let ref_to_port_edge_px = (TARAWA_TOP_PORT_EDGE_PX_Y - TARAWA_TOP_REF_PX.1) * scale;
        // The V/STOL approach axis is one AV-8B wingspan outside the 18 m
        // port flight-deck edge.  Because the calibrated 7.5 point itself is
        // at landing_point.x = -3.10 m, its actual distance to that edge is
        // 18.0 - 3.10 = 14.90 m.  Derive this from the recovery geometry so
        // the artwork and the scoring reference cannot drift apart.
        let ref_to_port_edge_m = (*approach_axis_port_m - AV8B_WINGSPAN_M + landing_point.x).abs();
        let port_edge_to_hover_px = if ref_to_port_edge_m > 1.0e-9 {
            ref_to_port_edge_px * (AV8B_WINGSPAN_M / ref_to_port_edge_m)
        } else {
            0.0
        };
        let ref_to_hover_px = ref_to_port_edge_px + port_edge_to_hover_px;

        // In chart coordinates y=0 is the hover/parallel-approach axis.  Place
        // the 7.5 point above it by the visual distance calculated above.
        let visual_ref_y = ref_to_hover_px * y_nm_per_px;
        vstol_visual_ref_y_nm = Some(visual_ref_y);
        let anchor_x = -ref_x_px * x_nm_per_px;
        let anchor_y = visual_ref_y + ref_y_px * y_nm_per_px;
        let elem: BitMapElement<_> = ((anchor_x, anchor_y), resized).into();
        chart.draw_series(std::iter::once(elem))?;

        // Explicit ideal axis: y=0 is the AV-8B parallel approach line.
        chart.draw_series(LineSeries::new(
            [(0.0, 0.0), (chart_range_x.end, 0.0)],
            THEME_GUIDE_GRAY.mix(0.70),
        ))?;

        // Same converging lineup corridors as the CATOBAR chart, but centred
        // on the parallel AV-8B approach line instead of the angled deck.
        let lines = [
            (0.25f64, THEME_GUIDE_GRAY),
            (0.75, THEME_GUIDE_GREEN),
            (3.0, THEME_GUIDE_YELLOW),
            (6.0, THEME_GUIDE_RED),
        ];
        for (deg, color) in lines {
            let y = deg.to_radians().tan() * chart_range_x.end;
            chart.draw_series(LineSeries::new(
                [(0.0, 0.0), (chart_range_x.end, y)],
                color.mix(0.4),
            ))?;
            chart.draw_series(LineSeries::new(
                [(0.0, 0.0), (chart_range_x.end, y.neg())],
                color.mix(0.4),
            ))?;
        }
    } else {
        // carrier top image is 300x300px which corresponds to 115x115m
        let (w, _h) = canvas.dim_in_pixel();
        let a = nm_to_m(RANGE_X.end - RANGE_X.start);
        let m2px = f64::from(w) / a;
        let img_size = ((115.0 * m2px) as u32, (115.0 * m2px) as u32);
        let img_carrier_top = image::load_from_memory_with_format(
            include_bytes!("../img/carrier-top.png"),
            ImageFormat::Png,
        )?
        .resize_exact(img_size.0, img_size.1, FilterType::Nearest);
        let elem: BitMapElement<_> = (
            (-m_to_nm(115.0 * 1.0 / 3.0), m_to_nm(115.0 / 2.0)),
            img_carrier_top,
        )
            .into();
        chart.draw_series(std::iter::once(elem))?;

        // draw centerline
        // Source: A Review and Analysis of Precision Approach and Landing System (PALS) Certifification
        // Procedures, Figure 5
        let lines = [
            (0.25f64, THEME_GUIDE_GRAY),
            (0.75, THEME_GUIDE_GREEN),
            (3.0, THEME_GUIDE_YELLOW),
            (6.0, THEME_GUIDE_RED),
        ];

        for (deg, color) in lines {
            let y = deg.to_radians().tan() * RANGE_X.end;
            chart.draw_series(LineSeries::new(
                [(0.0, 0.0), (RANGE_X.end, y)],
                color.mix(0.4),
            ))?;
            chart.draw_series(LineSeries::new(
                [(0.0, 0.0), (RANGE_X.end, y.neg())],
                color.mix(0.4),
            ))?;
        }
    }

    let vstol = track.carrier_info.is_vstol();
    let final_run = select_final_approach_datums(track);
    let track_runs_in_nm: Vec<Vec<Datum>> = if vstol {
        let mut combined = final_run
            .iter()
            .map(|d| Datum {
                x: m_to_nm(d.x),
                y: m_to_nm(d.y),
                ..copy_datum(d)
            })
            .collect::<Vec<_>>();

        // Horizontal terminal phase: keep the real side-step shape, but remap
        // only its lateral display scale so the *physical* Tarawa 7.5 axis
        // (approach_axis_port_m from the hover axis) lands exactly on the
        // user-calibrated yellow 7.5 point in the readable top-down sprite.
        // The terminal phase uses a fixed physical-to-visual lateral transform
        // between the hover axis and the calibrated 7.5 marker.
        let translation = select_vstol_translation_datums(track, &final_run);
        if translation.len() > 1 {
            if let (
                Some(_endpoint),
                CarrierRecovery::Vstol {
                    landing_point,
                    approach_axis_port_m,
                    ..
                },
            ) = (final_run.last(), &track.carrier_info.recovery)
            {
                // The recorded V/STOL datums use plane.position (DCS aircraft
                // origin) for the horizontal trace, while spot 7.5 is calibrated
                // against the AV-8B pilot-ground landing_reference. Therefore the
                // physical lateral value of the origin at an exact 7.5 touchdown
                // is shorter than the pilot-ground displacement.
                //
                // Current calibration:
                //   hover axis -> pilot-ground 7.5 = 27.24 - 3.10 = 24.14 m
                //   pilot-ground offset from origin = 3.43 m
                //   hover axis -> aircraft origin at exact 7.5 = 20.71 m
                //
                // IMPORTANT: use a FIXED transform between those two known
                // references. Older builds anchored the transform on endpoint.y
                // (the aircraft's actual lateral position at x=0). If the pilot
                // had already started the side-step before crossing x=0, that
                // endpoint could sit very close to physical_75_y_nm. The resulting
                // tiny denominator made the display scale explode and produced the
                // long vertical green/red spike seen in V1.20.
                //
                // y=0 (hover axis) is fixed to visual y=0 and the physical aircraft
                // origin at exact spot 7.5 is fixed to the yellow 7.5 marker. This
                // transform therefore cannot become singular and is independent of
                // when the pilot starts the lateral translation.
                let physical_75_y_m =
                    *approach_axis_port_m + landing_point.x - track.plane_info.landing_reference.x;
                let physical_75_y_nm = m_to_nm(physical_75_y_m);
                let visual_75_y_nm = vstol_visual_ref_y_nm.unwrap_or(physical_75_y_nm);
                let scale = if physical_75_y_nm.abs() > 1.0e-9 {
                    visual_75_y_nm / physical_75_y_nm
                } else {
                    1.0
                };

                // Once the terminal translation begins, keep its entire segment in
                // the same fixed visual coordinate system, including the synthetic
                // x=0 endpoint already stored at the end of `combined`. This avoids
                // connecting an unscaled x=0 point to a scaled next sample, which
                // would otherwise create a smaller vertical hook at the transition.
                if let Some(last) = combined.last_mut() {
                    last.y *= scale;
                }

                combined.extend(translation.into_iter().skip(1).map(|d| {
                    let real_y_nm = m_to_nm(d.y);
                    Datum {
                        x: m_to_nm(d.x),
                        y: real_y_nm * scale,
                        ..copy_datum(&d)
                    }
                }));
            }
        }
        vec![combined]
    } else {
        select_catobar_display_runs(&track.datums)
            .into_iter()
            .map(|run| {
                run.iter()
                    .map(|d| Datum {
                        x: m_to_nm(d.x),
                        y: m_to_nm(d.y),
                        ..copy_datum(d)
                    })
                    .collect()
            })
            .collect()
    };

    // draw approach shadow
    for track_in_nm in &track_runs_in_nm {
        chart.draw_series(LineSeries::new(
            track_in_nm.iter().map(|d| (d.x, d.y)),
            THEME_BG.stroke_width(4),
        ))?;
    }

    // draw approach
    for track_in_nm in &track_runs_in_nm {
        let mut points = Vec::new();
        let mut color = THEME_AOA_ON_SPEED;
        for datum in track_in_nm {
            let next_color = aoa_color(datum.aoa, track.plane_info);
            let point = (datum.x, datum.y);

            if points.is_empty() {
                color = next_color;
            }

            if next_color != color {
                points.push(point);
                chart.draw_series(LineSeries::new(
                    points.iter().cloned(),
                    color.stroke_width(2),
                ))?;
                points.clear();
                color = next_color;
            }
            points.push(point);
        }

        if !points.is_empty() {
            chart.draw_series(LineSeries::new(
                points.iter().cloned(),
                color.stroke_width(2),
            ))?;
        }
    }

    // Draw the calibrated V/STOL 7.5 marker last so the terminal trace or
    // the Tarawa artwork can never hide it.
    if let Some(visual_ref_y) = vstol_visual_ref_y_nm {
        chart.draw_series(std::iter::once(Circle::new(
            (0.0, visual_ref_y),
            VSTOL_MARKER_RADIUS_PX,
            THEME_GUIDE_YELLOW.filled(),
        )))?;
    }

    Ok(())
}

#[tracing::instrument(skip_all)]
pub fn draw_side_view(
    track: &TrackResult,
    canvas: DrawingArea<BitMapBackend<'_, RGBPixel>, Shift>,
) -> Result<(), DrawError> {
    let side_range = side_range_y(track);
    let chart_range_x = if track.carrier_info.is_vstol() {
        VSTOL_RANGE_X
    } else {
        RANGE_X
    };
    let mut chart = ChartBuilder::on(&canvas)
        .margin(0u32)
        .x_label_area_size(0u32)
        .y_label_area_size(0u32)
        .build_cartesian_2d(
            CustomRange(
                chart_range_x
                    .clone()
                    .with_key_points(vec![0.25f64, 0.5, 0.75, 1.0]),
            ),
            side_range.clone(),
        )?;

    // Then we can draw a mesh
    chart
        .configure_mesh()
        .disable_mesh()
        .disable_x_axis()
        .disable_y_axis()
        .axis_style(THEME_FG)
        .x_label_style(text_style())
        .draw()?;

    if let CarrierRecovery::Vstol {
        target_altitude_ft, ..
    } = &track.carrier_info.recovery
    {
        // Use the user-provided Tarawa side profile.  The clean asset is stored
        // under /img as a recovery-only asset while the pink-square copy was used
        // only to calibrate the spot-7.5 pixel reference.
        let visual_deck_alt_ft = *target_altitude_ft - VSTOL_TERMINAL_DESCENT_DISPLAY_FT;
        let (plot_w, plot_h) = chart.plotting_area().dim_in_pixel();
        let x_nm_per_px = (chart_range_x.end - chart_range_x.start) / f64::from(plot_w.max(1));
        let y_ft_per_px = (side_range.end - side_range.start) / f64::from(plot_h.max(1));

        let img = themed_png_from_bytes(include_bytes!("../img/tarawa-vstol-recovery-side.png"))?;
        let src_w = img.width() as f64;
        let src_h = img.height() as f64;
        let display_w = TARAWA_SIDE_DISPLAY_WIDTH_PX;
        let display_h = ((src_h / src_w) * f64::from(display_w)).round() as u32;
        let resized = img.resize_exact(display_w, display_h.max(1), FilterType::CatmullRom);

        // The side-view pink square marks the deck-level 7.5 location. Align its
        // x coordinate with x=0 and place it exactly 50 ft below the 120-ft
        // hover reference. This is a V/STOL display transform only; the raw
        // aircraft altitude remains unchanged in the recorded data.
        let ref_x_px = TARAWA_SIDE_REF_PX.0 / src_w * f64::from(display_w);
        let ref_y_px = TARAWA_SIDE_REF_PX.1 / src_h * f64::from(display_h.max(1));
        let anchor_x = -ref_x_px * x_nm_per_px;
        let anchor_y = visual_deck_alt_ft + ref_y_px * y_ft_per_px;
        let elem: BitMapElement<_> = ((anchor_x, anchor_y), resized).into();
        chart.draw_series(std::iter::once(elem))?;

        // CATOBAR-style glide-path fan translated upward to the V/STOL target.
        // The centreline reaches 120 ft above the water at x=0 (abeam 7.5).
        // AV-8B V/STOL guide fan centred on the aircraft-specific 3.0-degree
        // glide slope.  The +/-0.5 and +/-1.0 degree boundaries deliberately
        // match the CATOBAR-derived grading thresholds used for GS deviations.
        // The +/-0.25 green lines are visual inner references only.
        let lines = [
            (track.plane_info.glide_slope - 1.0, THEME_GUIDE_RED),
            (track.plane_info.glide_slope - 0.5, THEME_GUIDE_YELLOW),
            (track.plane_info.glide_slope - 0.25, THEME_GUIDE_GREEN),
            (track.plane_info.glide_slope, THEME_GUIDE_GRAY),
            (track.plane_info.glide_slope + 0.25, THEME_GUIDE_GREEN),
            (track.plane_info.glide_slope + 0.5, THEME_GUIDE_YELLOW),
            (track.plane_info.glide_slope + 1.0, THEME_GUIDE_RED),
        ];

        for (deg, color) in lines {
            let mut x = chart_range_x.end;
            let mut y = *target_altitude_ft + nm_to_ft(deg.to_radians().tan() * chart_range_x.end);
            if y > side_range.end {
                x = ft_to_nm(side_range.end - *target_altitude_ft) / deg.to_radians().tan();
                y = side_range.end;
            }
            chart.draw_series(LineSeries::new(
                [(0.0, *target_altitude_ft), (x, y)],
                color.mix(0.4),
            ))?;
        }

        // Keep the guide fan converging on the 120-ft hover reference, but
        // dissociate the yellow final-position marker from that hover datum.
        // The yellow point represents the deck-level 7.5 touchdown location,
        // visually 50 ft below the hover reference, matching the top-view logic
        // where the guide convergence and the physical 7.5 spot are distinct.
    } else {
        // carrier side image is 300x150px which corresponds to 115x57.5m
        let (w, _h) = canvas.dim_in_pixel();
        let a = nm_to_m(RANGE_X.end - RANGE_X.start);
        let m2px = f64::from(w) / a;
        let img_size = ((115.0 * m2px) as u32, (57.5 * m2px) as u32);
        let img_carrier_side = image::load_from_memory_with_format(
            include_bytes!("../img/carrier-side.png"),
            ImageFormat::Png,
        )?
        .resize_exact(img_size.0, img_size.1, FilterType::Nearest);
        let elem: BitMapElement<_> = ((-m_to_nm(115.0 * 1.0 / 3.0), 24.0), img_carrier_side).into();
        chart.draw_series(std::iter::once(elem))?;

        // draw centerline
        let lines = [
            (track.plane_info.glide_slope - 0.9, THEME_GUIDE_RED),
            (track.plane_info.glide_slope - 0.6, THEME_GUIDE_YELLOW),
            (track.plane_info.glide_slope - 0.25, THEME_GUIDE_GREEN),
            (track.plane_info.glide_slope, THEME_GUIDE_GRAY),
            (track.plane_info.glide_slope + 0.25, THEME_GUIDE_GREEN),
            (track.plane_info.glide_slope + 0.7, THEME_GUIDE_YELLOW),
            (track.plane_info.glide_slope + 1.5, THEME_GUIDE_RED),
        ];

        for (deg, color) in lines {
            let mut x = RANGE_X.end;
            let mut y = nm_to_ft(deg.to_radians().tan() * RANGE_X.end);
            if y > SIDE_RANGE_Y.end {
                x = ft_to_nm(SIDE_RANGE_Y.end) / deg.to_radians().tan();
                y = SIDE_RANGE_Y.end;
            }
            chart.draw_series(LineSeries::new([(0.0, 0.0), (x, y)], color.mix(0.4)))?;
        }
    }

    let vstol = track.carrier_info.is_vstol();
    let final_run = select_final_approach_datums(track);
    let track_descent_runs: Vec<Vec<Datum>> = if vstol {
        let mut combined = final_run
            .iter()
            .map(|d| Datum {
                x: m_to_nm(d.x),
                alt: m_to_ft(d.alt),
                ..copy_datum(d)
            })
            .collect::<Vec<_>>();

        // Vertical terminal phase: after x=0, continue the trace through the
        // hover-to-deck descent.  The real AV-8B origin altitude at touchdown
        // contact is deck altitude minus the (negative) landing-reference Y.
        // Remap only this terminal segment so that a correct touchdown reaches
        // the user-marked 7.5 deck point, visually 50 ft below the hover datum.
        let translation = select_vstol_translation_datums(track, &final_run);
        if translation.len() > 1 {
            if let (
                Some(endpoint),
                CarrierRecovery::Vstol {
                    target_altitude_ft, ..
                },
            ) = (final_run.last(), &track.carrier_info.recovery)
            {
                let start_alt_ft = m_to_ft(endpoint.alt);
                // The exact land-event datum appended by Track::landed() is
                // the authoritative touchdown altitude for the terminal display.
                // This removes the small remaining gap caused by estimating the
                // touchdown altitude from static aircraft geometry.
                let physical_touchdown_alt_ft = translation
                    .last()
                    .map(|d| m_to_ft(d.alt))
                    .unwrap_or_else(|| {
                        m_to_ft(
                            track.carrier_info.deck_altitude - track.plane_info.landing_reference.y,
                        )
                    });
                let visual_deck_alt_ft = *target_altitude_ft - VSTOL_TERMINAL_DESCENT_DISPLAY_FT;
                let denom = physical_touchdown_alt_ft - start_alt_ft;
                let scale = if denom.abs() > 1.0e-9 {
                    (visual_deck_alt_ft - start_alt_ft) / denom
                } else {
                    1.0
                };

                combined.extend(translation.into_iter().skip(1).map(|d| {
                    let real_alt_ft = m_to_ft(d.alt);
                    Datum {
                        x: m_to_nm(d.x),
                        alt: start_alt_ft + (real_alt_ft - start_alt_ft) * scale,
                        ..copy_datum(&d)
                    }
                }));
            }
        }
        vec![combined]
    } else {
        select_catobar_display_runs(&track.datums)
            .into_iter()
            .map(|run| {
                run.iter()
                    .map(|d| Datum {
                        x: m_to_nm(d.x),
                        alt: m_to_ft(d.alt),
                        ..copy_datum(d)
                    })
                    .collect()
            })
            .collect()
    };

    // draw approach shadow
    for track_descent in &track_descent_runs {
        chart.draw_series(LineSeries::new(
            track_descent.iter().map(|d| (d.x, d.alt)),
            THEME_BG.stroke_width(4),
        ))?;
    }

    // draw approach
    for track_descent in &track_descent_runs {
        let mut points = Vec::new();
        let mut color = THEME_AOA_ON_SPEED;
        for datum in track_descent {
            let next_color = aoa_color(datum.aoa, track.plane_info);
            let point = (datum.x, datum.alt);

            if points.is_empty() {
                color = next_color;
            }

            if next_color != color {
                points.push(point);
                chart.draw_series(LineSeries::new(
                    points.iter().cloned(),
                    color.stroke_width(2),
                ))?;
                points.clear();
                color = next_color;
            }
            points.push(point);
        }

        if !points.is_empty() {
            chart.draw_series(LineSeries::new(
                points.iter().cloned(),
                color.stroke_width(2),
            ))?;
        }
    }

    // Draw the deck-level 7.5 marker last so it remains visible even when the
    // touchdown trace terminates on the exact same point.
    if let CarrierRecovery::Vstol {
        target_altitude_ft, ..
    } = &track.carrier_info.recovery
    {
        let visual_deck_alt_ft = *target_altitude_ft - VSTOL_TERMINAL_DESCENT_DISPLAY_FT;
        chart.draw_series(std::iter::once(Circle::new(
            (0.0, visual_deck_alt_ft),
            VSTOL_MARKER_RADIUS_PX,
            THEME_GUIDE_YELLOW.filled(),
        )))?;
    }

    Ok(())
}

fn text_style() -> TextStyle<'static> {
    TextStyle::from(("sans-serif", 20).into_font()).color(&THEME_FG)
}

fn fmt_gate(
    gate: Option<&GateDatum>,
    quality: &GateQuality,
    max_sample_gap_ms: f64,
    vstol: bool,
) -> String {
    match gate {
        Some(g) if vstol => format!("ALT {:+.0}ft  LAT {:+.0}ft", g.gs_deviation_ft, g.lineup_ft),
        Some(g) => format!(
            "GS {:+.1}\u{00B0}  LU {:+.1}\u{00B0}",
            g.gs_deviation_deg, g.lineup_deg
        ),
        None if quality.status == GateStatus::Late => "LATE".to_string(),
        None if quality.status == GateStatus::Invalid => {
            if quality.reason.as_deref() == Some("stale_gate_bracket") {
                format!("INVALID — gap up to {:.0} ms", max_sample_gap_ms)
            } else {
                format!(
                    "INVALID — {}",
                    quality.reason.as_deref().unwrap_or("telemetry unavailable")
                )
            }
        }
        None => "TELEMETRY UNAVAILABLE".to_string(),
    }
}

/// Preserve every continuous fragment belonging to the latest CATOBAR final.
/// Fragments are deliberately returned separately so the renderer never draws
/// a line through a telemetry outage.
fn select_catobar_display_runs(datums: &[Datum]) -> Vec<Vec<Datum>> {
    const MAX_FRAGMENT_GAP_S: f64 = 2.0;
    const MAX_FINAL_DURATION_S: f64 = 45.0;

    let runs = continuous_final_runs(datums);
    let Some(selected_index) = runs
        .iter()
        .rposition(|run| inbound_span_nm(run) >= MIN_FINAL_SPAN_NM)
        .or_else(|| runs.iter().rposition(|run| inbound_span_nm(run) > 0.0))
    else {
        return Vec::new();
    };

    let final_time = runs[selected_index]
        .last()
        .map(|datum| datum.time)
        .unwrap_or_default();
    let mut first_index = selected_index;
    while first_index > 0 {
        let previous = &runs[first_index - 1];
        let current = &runs[first_index];
        let Some((previous_end, current_start)) = previous.last().zip(current.first()) else {
            break;
        };
        let gap = current_start.time - previous_end.time;
        let still_inbound = previous_end.x + 20.0 >= current_start.x;
        let recent = final_time - previous_end.time <= MAX_FINAL_DURATION_S;
        if gap <= 0.0 || gap > MAX_FRAGMENT_GAP_S || !still_inbound || !recent {
            break;
        }
        first_index -= 1;
    }
    runs.into_iter()
        .skip(first_index)
        .take(selected_index - first_index + 1)
        .collect()
}

fn aoa_color(aoa: f64, plane_info: &'static AirplaneInfo) -> RGBColor {
    match (plane_info.aoa_rating)(aoa) {
        Aoa::Fast => THEME_AOA_FAST,
        Aoa::SlightlyFast => THEME_AOA_SLIGHTLY_FAST,
        Aoa::OnSpeed => THEME_AOA_ON_SPEED,
        Aoa::SlightlySlow => THEME_AOA_SLIGHTLY_SLOW,
        Aoa::Slow => THEME_AOA_SLOW,
    }
}

struct CustomRange(WithKeyPoints<RangedCoordf64>);

impl Ranged for CustomRange {
    type ValueType = <plotters::coord::types::RangedCoordf64 as Ranged>::ValueType;
    type FormatOption = plotters::coord::ranged1d::NoDefaultFormatting;

    fn map(&self, value: &Self::ValueType, limit: (i32, i32)) -> i32 {
        self.0.map(value, limit)
    }

    fn key_points<Hint: plotters::coord::ranged1d::KeyPointHint>(
        &self,
        hint: Hint,
    ) -> Vec<Self::ValueType> {
        self.0.key_points(hint)
    }

    fn range(&self) -> std::ops::Range<Self::ValueType> {
        self.0.range()
    }

    fn axis_pixel_range(&self, limit: (i32, i32)) -> std::ops::Range<i32> {
        self.0.axis_pixel_range(limit)
    }
}

impl ValueFormatter<f64> for CustomRange {
    fn format(v: &f64) -> String {
        match *v {
            v if (v - 0.25).abs() < f64::EPSILON => "¼ nm".to_string(),
            v if (v - 0.50).abs() < f64::EPSILON => "½ nm".to_string(),
            v if (v - 0.75).abs() < f64::EPSILON => "¾ nm".to_string(),
            _ => format!("{}nm", v),
        }
    }
}

#[cfg(test)]
mod layout_tests {
    use super::{
        chart_layout, recovery_label, select_catobar_display_runs, select_catobar_final_datums,
        select_vstol_final_datums, Datum, Grading, PANEL_GAP,
    };

    #[test]
    fn catobar_recovery_label_uses_dcs_wire_when_estimate_is_unavailable() {
        let grading = Grading::Recovered {
            cable: Some(4),
            cable_estimated: None,
        };

        assert_eq!(recovery_label(&grading, false), "Wire 4 (DCS)");
    }

    #[test]
    fn catobar_recovery_label_keeps_dcs_wire_authoritative() {
        let grading = Grading::Recovered {
            cable: Some(4),
            cable_estimated: Some(3),
        };

        assert_eq!(recovery_label(&grading, false), "Wire 4 (DCS)");
    }

    fn two_complete_final_approach_runs() -> Vec<Datum> {
        let mut datums = Vec::new();

        // The earlier branch deliberately spans farther than the later branch.
        for (index, x_m) in (0..=24).map(|index| (index, 1_200.0 - index as f64 * 50.0)) {
            datums.push(Datum {
                time: index as f64 * 0.1,
                x: x_m,
                y: 180.0,
                aoa: 2.0,
                alt: 100.0,
                ..Datum::default()
            });
        }

        // Leave the shared chart window before commencing the later final.
        datums.push(Datum {
            time: 10.0,
            x: 1_300.0,
            y: 400.0,
            aoa: 2.0,
            alt: 100.0,
            ..Datum::default()
        });

        for (index, x_m) in (0..=22).map(|index| (index, 1_100.0 - index as f64 * 50.0)) {
            datums.push(Datum {
                time: 100.0 + index as f64 * 0.1,
                x: x_m,
                y: 4.0,
                aoa: 7.0,
                alt: 100.0,
                ..Datum::default()
            });
        }

        datums
    }

    #[test]
    fn catobar_chart_panels_do_not_overlap() {
        let (_root_height, side_height, top_start) = chart_layout(false);

        assert_eq!(top_start - side_height, PANEL_GAP);
    }

    #[test]
    fn vstol_chart_panels_do_not_overlap() {
        let (_root_height, side_height, top_start) = chart_layout(true);

        assert_eq!(top_start - side_height, PANEL_GAP);
    }

    #[test]
    fn catobar_final_approach_selection_uses_latest_continuous_inbound_run() {
        let selected = select_catobar_final_datums(&two_complete_final_approach_runs());

        assert!(!selected.is_empty());
        assert!(selected.iter().all(|datum| datum.time >= 100.0));
        assert!(selected.iter().all(|datum| datum.y == 4.0));
        assert_eq!(selected.last().map(|datum| datum.x), Some(0.0));
    }

    #[test]
    fn catobar_display_preserves_fragments_without_connecting_them() {
        let mut datums = Vec::new();
        for (time, x) in [
            (1.0, 1_300.0),
            (1.1, 1_240.0),
            (2.0, 1_100.0),
            (2.1, 1_040.0),
        ] {
            datums.push(Datum {
                time,
                x,
                y: 0.0,
                alt: 80.0,
                ..Datum::default()
            });
        }
        let runs = select_catobar_display_runs(&datums);
        assert_eq!(runs.len(), 2);
        assert_eq!(runs[0].last().unwrap().x, 1_240.0);
        assert_eq!(runs[1].first().unwrap().x, 1_100.0);
    }

    #[test]
    fn vstol_final_approach_selection_preserves_longest_completed_branch_policy() {
        let selected = select_vstol_final_datums(&two_complete_final_approach_runs());

        assert!(!selected.is_empty());
        assert!(selected.iter().all(|datum| datum.time < 100.0));
        assert!(selected.iter().all(|datum| datum.y == 180.0));
        assert_eq!(selected.last().map(|datum| datum.x), Some(0.0));
    }
}

// ---------------------------------------------------------------------------
// Pattern (circuit) overview chart
// ---------------------------------------------------------------------------

/// Width in nm of the pattern chart (port–starboard).
const PAT_WIDTH_NM: f64 = 5.0;
/// Height in nm of the pattern chart (ahead–astern, 0 = carrier).
const PAT_ASTERN_NM: f64 = 3.0; // astern (bottom of chart)
const PAT_AHEAD_NM: f64 = 3.0; // ahead  (top of chart)
/// Physical size of the pattern PNG.
const PAT_IMG_W: u32 = 900;
const PAT_IMG_H: u32 = 900;

/// Draw a top-down bird's-eye chart of the full recovery circuit and save it
/// next to the approach chart as `<filename>-pattern.png`.
///
/// Coordinate conventions (carrier = origin):
/// - chart X axis: right = starboard, left = port  (chart_x = -port_m)
/// - chart Y axis: top = ahead of carrier, bottom = astern  (chart_y = -astern_m)
///
/// A standard Case I left-hand circuit will appear with the break/abeam
/// positions on the left side and the approach from the bottom.
#[tracing::instrument(skip_all)]
pub fn draw_pattern_chart(
    out_dir: &std::path::Path,
    filename: &str,
    track: &TrackResult,
) -> Result<PathBuf, DrawError> {
    let path = out_dir
        .join(format!("{filename}-pattern"))
        .with_extension("png");

    let root = BitMapBackend::new(&path, (PAT_IMG_W, PAT_IMG_H)).into_drawing_area();
    root.fill(&THEME_BG)?;

    // Title
    let title_style = TextStyle::from(("sans-serif", 22).into_font()).color(&THEME_FG);
    root.draw_text(
        &format!(
            "Pattern — {}  {} pts",
            track.pass_grade.label(),
            match track.grade_points {
                Some(points) if track.carrier_info.is_vstol() => format!("{points:.2}"),
                Some(points) => format!("{points:.1}"),
                None => "no".to_string(),
            }
        ),
        &title_style,
        (16, 12),
    )?;

    let x_range = (-PAT_WIDTH_NM / 2.0)..(PAT_WIDTH_NM / 2.0);
    // chart_y = -astern_m: carrier at 0, ahead = positive, astern = negative
    let y_range = (-PAT_ASTERN_NM)..PAT_AHEAD_NM;

    let mut chart = ChartBuilder::on(&root)
        .margin(48u32)
        .x_label_area_size(30u32)
        .y_label_area_size(52u32)
        .build_cartesian_2d(x_range, y_range.clone())?;

    chart
        .configure_mesh()
        .light_line_style(THEME_BG.mix(0.0))
        .bold_line_style(THEME_GUIDE_GRAY.mix(0.15))
        .axis_style(THEME_FG)
        .x_label_style(text_style())
        .y_label_style(text_style())
        .x_desc("← Port                 Starboard →")
        .y_desc("nm")
        .draw()?;

    // BRC reference line through carrier
    chart.draw_series(LineSeries::new(
        [(0.0, y_range.start), (0.0, y_range.end)],
        THEME_GUIDE_GRAY.mix(0.3).stroke_width(1),
    ))?;

    // Draw the ship image at the chart centre.
    // CATOBAR keeps the original carrier image/scale from the fork.
    // V/STOL swaps only the centre ship image to the Tarawa while keeping
    // the rest of the pattern chart behaviour unchanged.
    {
        let (ship_len_m, ship_wid_m, ship_png) = if track.carrier_info.is_vstol() {
            (
                233.0_f64,
                48.0_f64,
                include_bytes!("../img/tarawa-vstol-pattern-top.png").as_slice(),
            )
        } else {
            (
                333.0_f64,
                99.0_f64,
                include_bytes!("../img/carrier-top-full-transp.png").as_slice(),
            )
        };
        let vs = 4.5_f64;

        let data_w = PAT_IMG_W as f64 - 2.0 * 48.0 - 52.0;
        let data_h = PAT_IMG_H as f64 - 2.0 * 48.0 - 30.0;
        let m2px_x = data_w / nm_to_m(PAT_WIDTH_NM);
        let m2px_y = data_h / nm_to_m(PAT_ASTERN_NM + PAT_AHEAD_NM);

        let img_w = ((ship_wid_m * vs * m2px_x) as u32).max(1);
        let img_h = ((ship_len_m * vs * m2px_y) as u32).max(1);

        let img = image::load_from_memory_with_format(ship_png, ImageFormat::Png)?
            .rotate90()
            .resize_exact(img_w, img_h, FilterType::CatmullRom)
            .into_rgba8();
        let mut bg = image::RgbaImage::from_pixel(
            img_w,
            img_h,
            image::Rgba([THEME_BG.0, THEME_BG.1, THEME_BG.2, 255]),
        );
        image::imageops::overlay(&mut bg, &img, 0, 0);

        let anchor_x = -m_to_nm(ship_wid_m * vs / 2.0);
        let anchor_y = m_to_nm(ship_len_m * vs / 2.0);
        let elem: BitMapElement<_> =
            ((anchor_x, anchor_y), image::DynamicImage::ImageRgba8(bg)).into();
        chart.draw_series(std::iter::once(elem))?;
    }

    // Gate rings (1/4, 1/2, 3/4 nm astern)
    for (gate_nm, label) in [(0.25, "¼"), (0.5, "½"), (0.75, "¾")] {
        chart.draw_series(std::iter::once(Circle::new(
            (0.0_f64, 0.0_f64),
            // radius in pixels = gate_nm / x_range_span * img_width_minus_margins
            ((gate_nm / PAT_WIDTH_NM) * (PAT_IMG_W as f64 - 96.0)) as u32,
            THEME_GUIDE_GRAY.mix(0.25).stroke_width(1),
        )))?;
        chart.draw_series(std::iter::once(Text::new(
            format!("{gate_nm} nm"),
            (m_to_nm(25.0), -gate_nm + m_to_nm(20.0)),
            text_style().color(&THEME_GUIDE_GRAY.mix(0.6)),
        )))?;
        let _ = label; // label kept for reference
    }

    // Pattern track coloured by AoA
    let datums_nm: Vec<_> = track
        .pattern_datums
        .iter()
        .map(|d| PatternDatum {
            time: d.time,
            // chart coords: port on left (negate port_m), ahead at top (negate astern_m)
            astern_m: -m_to_nm(d.astern_m), // chart_y = -astern_m
            port_m: -m_to_nm(d.port_m),     // chart_x = -port_m
            alt_ft: d.alt_ft,
            aoa: d.aoa,
        })
        .filter(|d| {
            d.port_m >= -PAT_WIDTH_NM / 2.0
                && d.port_m <= PAT_WIDTH_NM / 2.0
                && d.astern_m >= -PAT_ASTERN_NM
                && d.astern_m <= PAT_AHEAD_NM
        })
        .collect();

    let mut iter = datums_nm.iter().peekable();
    let mut seg_pts: Vec<(f64, f64)> = Vec::new();
    let mut seg_color = THEME_AOA_ON_SPEED;

    while let Some(d) = iter.next() {
        let pt = (d.port_m, d.astern_m); // (chart_x, chart_y)
        let color = aoa_color(d.aoa, track.plane_info);
        if seg_pts.is_empty() {
            seg_color = color;
        }
        if color != seg_color || iter.peek().is_none() {
            if color != seg_color {
                seg_pts.push(pt);
            }
            chart.draw_series(LineSeries::new(
                std::mem::take(&mut seg_pts),
                seg_color.stroke_width(2),
            ))?;
            seg_color = color;
        }
        seg_pts.push(pt);
    }
    if !seg_pts.is_empty() {
        chart.draw_series(LineSeries::new(seg_pts, seg_color.stroke_width(2)))?;
    }

    // Touchdown marker (last datum)
    if let Some(last) = datums_nm.last() {
        chart.draw_series(std::iter::once(Circle::new(
            (last.port_m, last.astern_m),
            5,
            THEME_FG.filled(),
        )))?;
    }

    std::mem::drop(chart);
    std::mem::drop(root);
    Ok(path)
}

#[derive(Debug, thiserror::Error)]
pub enum DrawError {
    #[error(transparent)]
    Plotter(#[from] DrawingAreaErrorKind<<BitMapBackend<'static> as DrawingBackend>::ErrorType>),
    #[error(transparent)]
    Image(#[from] image::ImageError),
}
