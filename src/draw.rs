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

use crate::data::{AirplaneInfo, Aoa};
use crate::track::{Datum, GateDatum, Grading, PatternDatum, TrackResult};
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
const TOP_RANGE_Y: Range<f64> = -0.15..0.15;
const SIDE_RANGE_Y: Range<f64> = 0.0..350.0;
const OVERLAP_OFFSET: u32 = 130;

#[tracing::instrument(skip_all)]
pub fn draw_chart(
    out_dir: &std::path::Path,
    filename: &str,
    track: &TrackResult,
) -> Result<PathBuf, DrawError> {
    let side_height = ((ft_to_nm(SIDE_RANGE_Y.end - SIDE_RANGE_Y.start) * 5.0
        / (RANGE_X.end - RANGE_X.start))
        * (WIDTH as f64))
        .floor() as u32;

    let top_height = (((TOP_RANGE_Y.end - TOP_RANGE_Y.start) / (RANGE_X.end - RANGE_X.start))
        * (WIDTH as f64))
        .floor() as u32
        - OVERLAP_OFFSET;

    let path = out_dir.join(filename).with_extension("png");
    let root_drawing_area =
        BitMapBackend::new(&path, (WIDTH, top_height + side_height + X_LABEL_AREA_SIZE))
            .into_drawing_area();
    root_drawing_area.fill(&THEME_BG)?;

    let (side, _) = root_drawing_area.split_vertically(side_height);
    let (_, top) = root_drawing_area.split_vertically(side_height - OVERLAP_OFFSET);

    draw_side_view(track, side)?;
    draw_top_view(track, top)?;

    let text_style = TextStyle::from(("sans-serif", 24).into_font()).color(&THEME_FG);

    root_drawing_area.draw_text(
        &format!("Pilot: {}", track.pilot_name),
        &text_style,
        (16, 16),
    )?;

    root_drawing_area.draw_text(
        &format!("Grade: {}  ({:.1} pts)", track.pass_grade.label(), track.pass_grade.points()),
        &text_style,
        (16, 48),
    )?;

    root_drawing_area.draw_text(
        &format!("Aircraft: {}", track.plane_info.name),
        &text_style,
        (16, 80),
    )?;

    root_drawing_area.draw_text(
        &match track.grading {
            Grading::Unknown => Cow::Borrowed(""),
            Grading::Bolter => Cow::Borrowed("Bolter"),
            Grading::WaveoffPilot => Cow::Borrowed("Waveoff"),
            Grading::IntentionalBolter { .. } => Cow::Borrowed("Qualif Bolter"),
            Grading::Recovered { cable, .. } => cable
                .map(|c| Cow::Owned(format!("Cable {}", c)))
                .unwrap_or(Cow::Borrowed("(failed to detect cable)")),
        },
        &text_style,
        (16, 112),
    )?;

    let text_style_small = TextStyle::from(("sans-serif", 18).into_font()).color(&THEME_FG);
    for (index, (label, gate)) in [
        ("3/4nm", track.gate_deviations.at_three_quarter_nm.as_ref()),
        ("1/2nm", track.gate_deviations.at_half_nm.as_ref()),
        ("1/4nm", track.gate_deviations.at_quarter_nm.as_ref()),
    ]
    .iter()
    .enumerate()
    {
        let (label, gate) = (*label, *gate);
        let y_pos = 144 + (index as i32) * 28;
        root_drawing_area.draw_text(
            &format!("{}: {}", label, fmt_gate(gate)),
            &text_style_small,
            (16, y_pos),
        )?;
    }

    std::mem::drop(root_drawing_area);

    Ok(path)
}

#[tracing::instrument(skip_all)]
pub fn draw_top_view(
    track: &TrackResult,
    canvas: DrawingArea<BitMapBackend<'_, RGBPixel>, Shift>,
) -> Result<(), DrawError> {
    let mut chart = ChartBuilder::on(&canvas)
        .margin(0u32)
        .x_label_area_size(X_LABEL_AREA_SIZE)
        .y_label_area_size(0u32)
        .build_cartesian_2d(
            CustomRange(RANGE_X.with_key_points(vec![0.25f64, 0.5, 0.75, 1.0])),
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
        // 0.25degree on center line
        (0.25f64, THEME_GUIDE_GRAY),
        // orange
        (0.75, THEME_GUIDE_GREEN),
        // red
        (3.0, THEME_GUIDE_YELLOW),
        // red
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

    let mut track_in_nm = track
        .datums
        .iter()
        .map(|d| Datum {
            time: d.time,
            x: m_to_nm(d.x),
            y: m_to_nm(d.y),
            aoa: d.aoa,
            alt: d.alt,
        })
        .filter(|d| RANGE_X.contains(&d.x) && TOP_RANGE_Y.contains(&d.y));

    // filter out datums with an x that is not continuously getting smaller (as drawing the series
    // will explode otherwise)
    let mut x_before = f64::MAX;
    let track_in_nm = std::iter::from_fn(move || {
        for datum in &mut track_in_nm {
            if datum.x < x_before {
                x_before = datum.x;
                return Some(datum);
            }
        }

        None
    });

    // draw approach shadow
    chart.draw_series(LineSeries::new(
        track_in_nm.clone().map(|d| (d.x, d.y)),
        THEME_BG.stroke_width(4),
    ))?;

    // draw approach
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
    Ok(())
}

#[tracing::instrument(skip_all)]
pub fn draw_side_view(
    track: &TrackResult,
    canvas: DrawingArea<BitMapBackend<'_, RGBPixel>, Shift>,
) -> Result<(), DrawError> {
    let mut chart = ChartBuilder::on(&canvas)
        .margin(0u32)
        .x_label_area_size(0u32)
        .y_label_area_size(0u32)
        .build_cartesian_2d(
            CustomRange(RANGE_X.with_key_points(vec![0.25f64, 0.5, 0.75, 1.0])),
            SIDE_RANGE_Y,
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

    let mut track_descent = track
        .datums
        .iter()
        .map(|d| Datum {
            time: d.time,
            x: m_to_nm(d.x),
            y: d.y,
            aoa: d.aoa,
            alt: m_to_ft(d.alt),
        })
        .filter(|d| RANGE_X.contains(&d.x) && SIDE_RANGE_Y.contains(&d.alt));

    // filter out datums with an x that is not continuously getting smaller (as drawing the series
    // will explode otherwise)
    let mut x_before = f64::MAX;
    let track_descent = std::iter::from_fn(move || {
        for datum in &mut track_descent {
            if datum.x < x_before {
                x_before = datum.x;
                return Some(datum);
            }
        }

        None
    });

    // draw approach shadow
    chart.draw_series(LineSeries::new(
        track_descent.clone().map(|d| (d.x, d.alt)),
        THEME_BG.stroke_width(4),
    ))?;

    // draw approach
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

    Ok(())
}

fn text_style() -> TextStyle<'static> {
    TextStyle::from(("sans-serif", 20).into_font()).color(&THEME_FG)
}

fn fmt_gate(gate: Option<&GateDatum>) -> String {
    match gate {
        Some(g) => format!(
            "GS {:+.1}\u{00B0}  LU {:+.1}\u{00B0}",
            g.gs_deviation_deg, g.lineup_deg
        ),
        None => "-".to_string(),
    }
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
// ---------------------------------------------------------------------------
// Pattern (circuit) overview chart
// ---------------------------------------------------------------------------

/// Width in nm of the pattern chart (port–starboard).
const PAT_WIDTH_NM: f64 = 5.0;
/// Height in nm of the pattern chart (ahead–astern, 0 = carrier).
const PAT_ASTERN_NM: f64 = 3.0; // astern (bottom of chart)
const PAT_AHEAD_NM: f64 = 3.0;  // ahead  (top of chart)
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
    let path = out_dir.join(format!("{filename}-pattern")).with_extension("png");

    let root =
        BitMapBackend::new(&path, (PAT_IMG_W, PAT_IMG_H)).into_drawing_area();
    root.fill(&THEME_BG)?;

    // Title
    let title_style = TextStyle::from(("sans-serif", 22).into_font()).color(&THEME_FG);
    root.draw_text(
        &format!(
            "Pattern — {}  {} pts",
            track.pass_grade.label(),
            track.pass_grade.points()
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
        [( 0.0, y_range.start), (0.0, y_range.end)],
        THEME_GUIDE_GRAY.mix(0.3).stroke_width(1),
    ))?;

    // carrier-top-full-transp.png rotated: bow=top, port=left, stbd=right.
    // Composite onto THEME_BG before drawing (BitMapBackend has no alpha support).
    // Drawn at 4.5× visual scale so it is readable at 5 nm chart width.
    {
        let carrier_len_m = 333.0_f64;
        let carrier_wid_m = 99.0_f64;
        let vs = 4.5_f64;

        let data_w = PAT_IMG_W as f64 - 2.0 * 48.0 - 52.0;
        let data_h = PAT_IMG_H as f64 - 2.0 * 48.0 - 30.0;
        let m2px_x = data_w / nm_to_m(PAT_WIDTH_NM);
        let m2px_y = data_h / nm_to_m(PAT_ASTERN_NM + PAT_AHEAD_NM);

        let img_w = ((carrier_wid_m * vs * m2px_x) as u32).max(1);
        let img_h = ((carrier_len_m * vs * m2px_y) as u32).max(1);

        let img = image::load_from_memory_with_format(
            include_bytes!("../img/carrier-top-full-transp.png"),
            ImageFormat::Png,
        )?
        .rotate90()
        .resize_exact(img_w, img_h, FilterType::CatmullRom)
        .into_rgba8();
        let mut bg = image::RgbaImage::from_pixel(
            img_w, img_h,
            image::Rgba([THEME_BG.0, THEME_BG.1, THEME_BG.2, 255]),
        );
        image::imageops::overlay(&mut bg, &img, 0, 0);

        let anchor_x = -m_to_nm(carrier_wid_m * vs / 2.0);
        let anchor_y =  m_to_nm(carrier_len_m * vs / 2.0);
        let elem: BitMapElement<_> = (
            (anchor_x, anchor_y),
            image::DynamicImage::ImageRgba8(bg),
        )
            .into();
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
            astern_m: -m_to_nm(d.astern_m),  // chart_y = -astern_m
            port_m:   -m_to_nm(d.port_m),    // chart_x = -port_m
            alt_ft: d.alt_ft,
            aoa: d.aoa,
        })
        .filter(|d| {
            d.port_m  >= -PAT_WIDTH_NM / 2.0 && d.port_m  <= PAT_WIDTH_NM / 2.0
                && d.astern_m >= -PAT_ASTERN_NM   && d.astern_m <= PAT_AHEAD_NM
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
                seg_pts.drain(..).collect::<Vec<_>>(),
                seg_color.stroke_width(2),
            ))?;
            seg_color = color;
        }
        seg_pts.push(pt);
    }
    if !seg_pts.is_empty() {
        chart.draw_series(LineSeries::new(
            seg_pts,
            seg_color.stroke_width(2),
        ))?;
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
