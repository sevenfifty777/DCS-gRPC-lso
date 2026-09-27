//! Ordered recovery case (Case I / II / III) from the mission weather and time.
//!
//! Phase 2 of `docs/CASE_RECOVERY_DETECTION_PLAN_2026-09-26.md`: a pure classifier, no I/O.
//! Two classifications are made from the same inputs:
//!
//! - **ED's rule** (`dcs-ed-statement-v1`), the one DCS's Marshal applies, from the ED developer
//!   statement in `tools/case_recovery-detection/conditions.txt`:
//!   Case III when dark, or density > 8 with (ceiling < 1000 ft or precipitation), or fog
//!   visibility < 5 NM; else Case I when density < 6 or ceiling > 1000 ft; else Case II.
//! - **NATOPS minima** (`natops-minima-v1`), diagnostic only (decision D6), NAVAIR 00-80T-105
//!   §4.2/§6.4: Case III at night (sunset + 30 min to sunrise − 30 min) or with a ceiling below
//!   1,000 ft or visibility below 5 NM; else Case I with a ceiling of 3,000 ft or more; else
//!   Case II.
//!
//! Missing inputs follow three-valued logic: one sufficient Case III reason decides even when
//! other inputs are unknown; otherwise any unknown input gives `Indeterminate`, never a guess.
//! Several readings of DCS data are hypotheses awaiting calibration; they are listed in
//! [`ASSUMPTIONS`] and serialised with every assessment. Nothing here changes a grade.

use serde::Serialize;
use time::{Date, Duration, Month, PrimitiveDateTime, Time};

use crate::cloud_presets::{CloudPresetTable, CLOUD_PRESETS};
use crate::flown_approach::{FlownApproach, FlownApproachEvidence};
use crate::mission_weather::{MissionWeatherQuery, MissionWeatherSnapshot};

pub const ED_RULE_VERSION: &str = "dcs-ed-statement-v1";
pub const NATOPS_RULE_VERSION: &str = "natops-minima-v1";

/// PROJECT-DERIVED hypothesis for ED's "dark": sun below the end of civil twilight.
pub const DARK_SUN_ELEVATION_DEG: f64 = -6.0;
/// NATOPS night starts this long after sunset and ends this long before sunrise (`OFFICIAL`,
/// NAVAIR 00-80T-105 §4.2).
pub const NATOPS_NIGHT_MARGIN_MIN: f64 = 30.0;
/// NATOPS "ceiling": lowest layer covering at least this much of the sky, out of 10
/// (broken or overcast). `PROJECT-DERIVED`: 00-80T-105 does not define ceiling.
pub const NATOPS_CEILING_MIN_DENSITY: f64 = 5.0;

const FT_PER_M: f64 = 3.280_839_895;
const M_PER_NM: f64 = 1852.0;

/// Readings of DCS data that calibration (plan section 6) must confirm or replace.
pub const ASSUMPTIONS: &[&str] = &[
    "ed_dark_is_sun_elevation_below_minus_6_deg",
    "preset_density_from_ed_metar_description_of_lowest_layer",
    "preset_ceiling_is_mission_base_plus_offset_of_lowest_covered_layer",
    "fog_absent_when_runtime_visibility_or_thickness_is_zero",
    "theatre_utc_offset_from_moose_table",
];

// The Roman numerals are the doctrine's names for the cases, not acronyms.
#[allow(clippy::upper_case_acronyms)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum OrderedCase {
    #[serde(rename = "I")]
    I,
    #[serde(rename = "II")]
    II,
    #[serde(rename = "III")]
    III,
    #[serde(rename = "indeterminate")]
    Indeterminate,
}

/// Why a case was decided (reasons) or could not be (unknown inputs).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CaseReason {
    // ED rule
    Dark,
    DenseCloudLowCeiling,
    DenseCloudPrecipitation,
    FogBelow5Nm,
    CloudDensityBelow6,
    CeilingAbove1000Ft,
    NeitherCaseIIINorCaseI,
    // NATOPS minima
    NatopsNight,
    CeilingBelow1000Ft,
    VisibilityBelow5Nm,
    CeilingAtOrAbove3000Ft,
    CeilingBelow3000Ft,
    // Unknown inputs
    WeatherUnavailable,
    DynamicWeather,
    UnknownAtmosphereType,
    UnknownPreset,
    MissingCloudData,
    MissingPrecipitation,
    MissingFogData,
    MissingVisibility,
    MissingTime,
    UnknownTheatre,
    MissingCarrierPosition,
}

/// Lowest relevant cloud base, or none at all.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Ceiling {
    NoCeiling,
    /// Feet above sea level, rounded to the foot.
    Ft(f64),
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CloudSource {
    Manual,
    Preset { name: String },
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Fog {
    Absent,
    Present {
        visibility_nm: f64,
        thickness_m: f64,
    },
}

/// Inputs of both classifiers, derived from the raw snapshot. Every field is `None` when it
/// cannot be known; `unknown_inputs` says why.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RecoveryConditions {
    /// Mission time converted to UTC (RFC 3339).
    pub utc: Option<String>,
    pub utc_offset_hours: Option<f64>,
    pub carrier_lat_deg: Option<f64>,
    pub carrier_lon_deg: Option<f64>,
    pub sun_elevation_deg: Option<f64>,
    /// ED "dark" under [`DARK_SUN_ELEVATION_DEG`].
    pub dark: Option<bool>,
    /// NATOPS night window.
    pub natops_night: Option<bool>,
    pub dynamic_weather: Option<bool>,
    pub cloud_source: CloudSource,
    pub cloud_density_0_10: Option<f64>,
    /// ED ceiling: base of the lowest cloud layer.
    pub ceiling: Option<Ceiling>,
    /// NATOPS ceiling: base of the lowest layer of at least 5/10.
    pub natops_ceiling: Option<Ceiling>,
    pub precipitation: Option<bool>,
    pub fog: Option<Fog>,
    /// Mission visibility (`weather.visibility.distance`), NM.
    pub visibility_nm: Option<f64>,
    pub unknown_inputs: Vec<CaseReason>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct NatopsAssessment {
    pub case: OrderedCase,
    pub rule_version: &'static str,
    pub reasons: Vec<CaseReason>,
    pub unknown_inputs: Vec<CaseReason>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CaseAssessment {
    /// The case DCS's Marshal is expected to order (ED's rule).
    pub ordered: OrderedCase,
    pub rule_version: &'static str,
    pub reasons: Vec<CaseReason>,
    pub unknown_inputs: Vec<CaseReason>,
    /// Doctrinal reading, diagnostic only: never chooses a detector or changes a grade.
    pub natops: NatopsAssessment,
    pub assumptions: &'static [&'static str],
    pub conditions: RecoveryConditions,
}

/// `recovery_case` block of a recording report: the assessments taken when the attempt started
/// and at groove entry (weather can change and dusk can pass in between), plus a summary. The
/// summary prefers the groove-entry assessment, the one closest to the graded groove. These
/// are LSO's reading of the conditions at those moments, not the call Marshal actually made at
/// check-in. Diagnostic only in this phase: no grading rule reads it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RecoveryCaseReport {
    pub ordered: OrderedCase,
    pub natops_case: OrderedCase,
    /// NATOPS night window at the carrier.
    pub night: Option<bool>,
    /// `groove_entry`, `attempt_start` or `none`.
    pub source: &'static str,
    pub at_attempt_start: Option<CaseAssessment>,
    pub at_groove_entry: Option<CaseAssessment>,
    /// What the pilot flew (`crate::flown_approach`); absent for V/STOL.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub flown_approach: Option<FlownApproachEvidence>,
    /// `true` when the flown approach contradicts the ordered case (overhead pattern in Case III
    /// conditions, straight-in in Case I/II conditions); `None` when either is unknown.
    pub mismatch: Option<bool>,
    /// `approach_does_not_match_ordered_case` when `mismatch` is true. Never a grade penalty.
    pub diagnostics: Vec<&'static str>,
}

pub const APPROACH_MISMATCH_DIAGNOSTIC: &str = "approach_does_not_match_ordered_case";

impl RecoveryCaseReport {
    pub fn new(
        at_attempt_start: Option<CaseAssessment>,
        at_groove_entry: Option<CaseAssessment>,
        flown_approach: Option<FlownApproachEvidence>,
    ) -> Self {
        let (summary, source) = match (&at_groove_entry, &at_attempt_start) {
            (Some(entry), _) => (Some(entry), "groove_entry"),
            (None, Some(start)) => (Some(start), "attempt_start"),
            (None, None) => (None, "none"),
        };
        let ordered = summary.map_or(OrderedCase::Indeterminate, |a| a.ordered);
        let mismatch = flown_approach
            .as_ref()
            .and_then(|flown| approach_mismatch(ordered, flown.kind));
        Self {
            ordered,
            natops_case: summary.map_or(OrderedCase::Indeterminate, |a| a.natops.case),
            night: summary.and_then(|a| a.conditions.natops_night),
            source,
            at_attempt_start,
            at_groove_entry,
            flown_approach,
            mismatch,
            diagnostics: if mismatch == Some(true) {
                vec![APPROACH_MISMATCH_DIAGNOSTIC]
            } else {
                Vec::new()
            },
        }
    }

    /// Short label for pilot-facing surfaces, e.g. `"Case III (night)"`; `None` when the case
    /// is indeterminate.
    pub fn label(&self) -> Option<String> {
        let case = match self.ordered {
            OrderedCase::I => "Case I",
            OrderedCase::II => "Case II",
            OrderedCase::III => "Case III",
            OrderedCase::Indeterminate => return None,
        };
        Some(if self.night == Some(true) {
            format!("{case} (night)")
        } else {
            case.to_string()
        })
    }
}

/// Whether the flown approach contradicts the ordered case; `None` when either is unknown.
pub fn approach_mismatch(ordered: OrderedCase, flown: FlownApproach) -> Option<bool> {
    match (ordered, flown) {
        (OrderedCase::Indeterminate, _) | (_, FlownApproach::Unknown) => None,
        (OrderedCase::III, FlownApproach::OverheadPattern) => Some(true),
        (OrderedCase::I | OrderedCase::II, FlownApproach::StraightIn) => Some(true),
        _ => Some(false),
    }
}

impl OrderedCase {
    /// Storage label: `I`, `II`, `III` or `indeterminate`.
    pub fn as_str(self) -> &'static str {
        match self {
            OrderedCase::I => "I",
            OrderedCase::II => "II",
            OrderedCase::III => "III",
            OrderedCase::Indeterminate => "indeterminate",
        }
    }
}

/// Assess the ordered case from a weather query and the carrier position (degrees).
pub fn assess_recovery_case(
    query: &MissionWeatherQuery,
    carrier_lat_lon: Option<(f64, f64)>,
) -> CaseAssessment {
    assess_with_presets(query, carrier_lat_lon, &CLOUD_PRESETS)
}

pub fn assess_with_presets(
    query: &MissionWeatherQuery,
    carrier_lat_lon: Option<(f64, f64)>,
    presets: &CloudPresetTable,
) -> CaseAssessment {
    let conditions = match query {
        MissionWeatherQuery::Available(snapshot) => {
            derive_conditions(snapshot, carrier_lat_lon, presets)
        }
        MissionWeatherQuery::Unavailable(_) => unavailable_conditions(carrier_lat_lon),
    };
    let (ordered, reasons) = classify_ed(&conditions);
    let (natops_case, natops_reasons) = classify_natops(&conditions);
    let unknown = |case: OrderedCase| {
        if case == OrderedCase::Indeterminate {
            conditions.unknown_inputs.clone()
        } else {
            Vec::new()
        }
    };
    CaseAssessment {
        ordered,
        rule_version: ED_RULE_VERSION,
        reasons,
        unknown_inputs: unknown(ordered),
        natops: NatopsAssessment {
            case: natops_case,
            rule_version: NATOPS_RULE_VERSION,
            reasons: natops_reasons,
            unknown_inputs: unknown(natops_case),
        },
        assumptions: ASSUMPTIONS,
        conditions,
    }
}

fn unavailable_conditions(carrier_lat_lon: Option<(f64, f64)>) -> RecoveryConditions {
    RecoveryConditions {
        utc: None,
        utc_offset_hours: None,
        carrier_lat_deg: carrier_lat_lon.map(|(lat, _)| lat),
        carrier_lon_deg: carrier_lat_lon.map(|(_, lon)| lon),
        sun_elevation_deg: None,
        dark: None,
        natops_night: None,
        dynamic_weather: None,
        cloud_source: CloudSource::Unknown,
        cloud_density_0_10: None,
        ceiling: None,
        natops_ceiling: None,
        precipitation: None,
        fog: None,
        visibility_nm: None,
        unknown_inputs: vec![CaseReason::WeatherUnavailable],
    }
}

// ---------------------------------------------------------------------------------------------
// Deriving the inputs
// ---------------------------------------------------------------------------------------------

pub fn derive_conditions(
    s: &MissionWeatherSnapshot,
    carrier_lat_lon: Option<(f64, f64)>,
    presets: &CloudPresetTable,
) -> RecoveryConditions {
    let mut unknown = Vec::new();

    // Time and sun.
    let utc_offset_hours = s.theatre.as_deref().and_then(theatre_utc_offset_hours);
    let local = mission_local_time(s);
    if local.is_none() {
        unknown.push(CaseReason::MissingTime);
    }
    if s.theatre.is_none() || utc_offset_hours.is_none() {
        unknown.push(CaseReason::UnknownTheatre);
    }
    let utc = match (local, utc_offset_hours) {
        (Some(local), Some(offset)) => local.checked_sub(Duration::seconds_f64(offset * 3600.0)),
        _ => None,
    };
    let carrier = carrier_lat_lon.filter(|(lat, lon)| {
        lat.is_finite() && lon.is_finite() && lat.abs() <= 90.0 && lon.abs() <= 180.0
    });
    if carrier.is_none() {
        unknown.push(CaseReason::MissingCarrierPosition);
    }
    let (sun_elevation_deg, natops_night) = match (utc, carrier) {
        (Some(utc), Some((lat, lon))) => (
            Some(solar_elevation_deg(utc, lat, lon)),
            Some(is_natops_night(utc, lat, lon)),
        ),
        _ => (None, None),
    };
    let dark = sun_elevation_deg.map(|elevation| elevation < DARK_SUN_ELEVATION_DEG);

    // Clouds and precipitation.
    let dynamic_weather = s.dynamic_weather;
    let mut cloud_source = CloudSource::Unknown;
    let mut cloud_density_0_10 = None;
    let mut ceiling = None;
    let mut natops_ceiling = None;
    let mut precipitation = None;
    match dynamic_weather {
        Some(true) => unknown.push(CaseReason::DynamicWeather),
        None => unknown.push(CaseReason::UnknownAtmosphereType),
        Some(false) => match &s.clouds.preset {
            Some(name) => {
                cloud_source = CloudSource::Preset { name: name.clone() };
                match presets.get(name) {
                    None => unknown.push(CaseReason::UnknownPreset),
                    Some(preset) => {
                        precipitation = Some(preset.has_precipitation());
                        let densities = preset.metar_cover_densities();
                        cloud_density_0_10 = densities.first().copied();
                        let lowest_covered = preset.lowest_layer_with_coverage(0.0, false);
                        let reference = preset.layers_by_altitude().first().copied();
                        let lowest_base = match (lowest_covered, reference, s.clouds.base_m) {
                            (Some(layer), Some(reference), Some(base_m)) => Some(Ceiling::Ft(
                                feet(base_m + layer.altitude_min_m - reference.altitude_min_m),
                            )),
                            (None, _, _) => Some(Ceiling::NoCeiling),
                            _ => None,
                        };
                        ceiling = lowest_base;
                        natops_ceiling = match densities.first() {
                            Some(&d) if d >= NATOPS_CEILING_MIN_DENSITY => lowest_base,
                            // A thin lowest layer under a denser one whose base is unknown.
                            Some(_)
                                if densities.iter().any(|&d| d >= NATOPS_CEILING_MIN_DENSITY) =>
                            {
                                None
                            }
                            Some(_) => Some(Ceiling::NoCeiling),
                            None => None,
                        };
                        if cloud_density_0_10.is_none() || ceiling.is_none() {
                            unknown.push(CaseReason::MissingCloudData);
                        }
                    }
                }
            }
            None => {
                cloud_source = CloudSource::Manual;
                cloud_density_0_10 = s.clouds.density_0_10;
                precipitation = s.clouds.precipitation_code.map(|code| code > 0);
                ceiling = match (cloud_density_0_10, s.clouds.base_m) {
                    (Some(d), _) if d <= 0.0 => Some(Ceiling::NoCeiling),
                    (Some(_), Some(base_m)) => Some(Ceiling::Ft(feet(base_m))),
                    _ => None,
                };
                natops_ceiling = match cloud_density_0_10 {
                    Some(d) if d >= NATOPS_CEILING_MIN_DENSITY => ceiling,
                    Some(_) => Some(Ceiling::NoCeiling),
                    None => None,
                };
                if cloud_density_0_10.is_none() || ceiling.is_none() {
                    unknown.push(CaseReason::MissingCloudData);
                }
                if precipitation.is_none() {
                    unknown.push(CaseReason::MissingPrecipitation);
                }
            }
        },
    }

    // Fog: the runtime values are authoritative; the mission's fog switches are not (first live
    // reading, 27 September 2026).
    let fog = match (s.runtime_fog_visibility_m, s.runtime_fog_thickness_m) {
        (Some(visibility_m), Some(thickness_m)) if visibility_m <= 0.0 || thickness_m <= 0.0 => {
            Some(Fog::Absent)
        }
        (Some(visibility_m), Some(thickness_m)) => Some(Fog::Present {
            visibility_nm: visibility_m / M_PER_NM,
            thickness_m,
        }),
        _ => {
            unknown.push(CaseReason::MissingFogData);
            None
        }
    };
    let visibility_nm = s.visibility_m.map(|m| m / M_PER_NM);
    if visibility_nm.is_none() {
        unknown.push(CaseReason::MissingVisibility);
    }

    RecoveryConditions {
        utc: utc.and_then(|utc| {
            utc.assume_utc()
                .format(&time::format_description::well_known::Rfc3339)
                .ok()
        }),
        utc_offset_hours,
        carrier_lat_deg: carrier.map(|(lat, _)| lat),
        carrier_lon_deg: carrier.map(|(_, lon)| lon),
        sun_elevation_deg,
        dark,
        natops_night,
        dynamic_weather,
        cloud_source,
        cloud_density_0_10,
        ceiling,
        natops_ceiling,
        precipitation,
        fog,
        visibility_nm,
        unknown_inputs: unknown,
    }
}

fn feet(metres: f64) -> f64 {
    (metres * FT_PER_M).round()
}

/// Mission local date and time: start date plus `timer.getAbsTime()` (seconds since midnight
/// of the start day, possibly more than a day).
fn mission_local_time(s: &MissionWeatherSnapshot) -> Option<PrimitiveDateTime> {
    let date = s.date?;
    let month = Month::try_from(date.month).ok()?;
    let day = Date::from_calendar_date(date.year, month, date.day).ok()?;
    let abs_time_s = s.abs_time_s?;
    PrimitiveDateTime::new(day, Time::MIDNIGHT).checked_add(Duration::seconds_f64(abs_time_s))
}

/// DCS mission time is local theatre time with a fixed offset per map. Values from MOOSE
/// `UTILS.GMTToLocalTimeDifference` (hypothesis `theatre_utc_offset_from_moose_table`), to be
/// checked against DCS before being trusted.
pub fn theatre_utc_offset_hours(theatre: &str) -> Option<f64> {
    Some(match theatre {
        "Caucasus" => 4.0,
        "PersianGulf" => 4.0,
        "Nevada" => -8.0,
        "Normandy" => 0.0,
        "TheChannel" => 2.0,
        "Syria" => 3.0,
        "MarianaIslands" => 10.0,
        "Falklands" => -3.0,
        "SinaiMap" => 2.0,
        "Kola" => 3.0,
        "Afghanistan" => 4.5,
        "Iraq" => 3.0,
        "GermanyCW" => 1.0,
        _ => return None,
    })
}

// ---------------------------------------------------------------------------------------------
// Sun position (NOAA general solar position equations, accuracy about a minute)
// ---------------------------------------------------------------------------------------------

struct SolarTerms {
    /// Equation of time, minutes.
    eqtime_min: f64,
    /// Declination, radians.
    declination_rad: f64,
}

fn solar_terms(date: Date, hour_utc: f64) -> SolarTerms {
    let days_in_year = if time::util::is_leap_year(date.year()) {
        366.0
    } else {
        365.0
    };
    let gamma = 2.0 * std::f64::consts::PI / days_in_year
        * (f64::from(date.ordinal()) - 1.0 + (hour_utc - 12.0) / 24.0);
    let eqtime_min = 229.18
        * (0.000075 + 0.001868 * gamma.cos()
            - 0.032077 * gamma.sin()
            - 0.014615 * (2.0 * gamma).cos()
            - 0.040849 * (2.0 * gamma).sin());
    let declination_rad = 0.006918 - 0.399912 * gamma.cos() + 0.070257 * gamma.sin()
        - 0.006758 * (2.0 * gamma).cos()
        + 0.000907 * (2.0 * gamma).sin()
        - 0.002697 * (3.0 * gamma).cos()
        + 0.00148 * (3.0 * gamma).sin();
    SolarTerms {
        eqtime_min,
        declination_rad,
    }
}

fn minutes_of_day(utc: PrimitiveDateTime) -> f64 {
    f64::from(utc.hour()) * 60.0
        + f64::from(utc.minute())
        + f64::from(utc.second()) / 60.0
        + f64::from(utc.nanosecond()) / 60e9
}

/// Sun elevation above the horizon, degrees, without refraction.
pub fn solar_elevation_deg(utc: PrimitiveDateTime, lat_deg: f64, lon_deg: f64) -> f64 {
    let minutes = minutes_of_day(utc);
    let terms = solar_terms(utc.date(), minutes / 60.0);
    let true_solar_time = minutes + terms.eqtime_min + 4.0 * lon_deg;
    let hour_angle = (true_solar_time / 4.0 - 180.0).to_radians();
    let lat = lat_deg.to_radians();
    let cos_zenith = lat.sin() * terms.declination_rad.sin()
        + lat.cos() * terms.declination_rad.cos() * hour_angle.cos();
    90.0 - cos_zenith.clamp(-1.0, 1.0).acos().to_degrees()
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum SunEvents {
    /// Sunrise and sunset, minutes after 00:00 UTC of the date (may fall outside 0..1440).
    RiseAndSet {
        sunrise_min: f64,
        sunset_min: f64,
    },
    PolarNight,
    MidnightSun,
}

fn sun_events(date: Date, lat_deg: f64, lon_deg: f64) -> SunEvents {
    let terms = solar_terms(date, 12.0);
    let lat = lat_deg.to_radians();
    let decl = terms.declination_rad;
    let cos_ha = 90.833_f64.to_radians().cos() / (lat.cos() * decl.cos()) - lat.tan() * decl.tan();
    if cos_ha > 1.0 {
        return SunEvents::PolarNight;
    }
    if cos_ha < -1.0 {
        return SunEvents::MidnightSun;
    }
    let ha_deg = cos_ha.acos().to_degrees();
    SunEvents::RiseAndSet {
        sunrise_min: 720.0 - 4.0 * (lon_deg + ha_deg) - terms.eqtime_min,
        sunset_min: 720.0 - 4.0 * (lon_deg - ha_deg) - terms.eqtime_min,
    }
}

/// NATOPS night: from 30 minutes after sunset to 30 minutes before sunrise.
pub fn is_natops_night(utc: PrimitiveDateTime, lat_deg: f64, lon_deg: f64) -> bool {
    match sun_events(utc.date(), lat_deg, lon_deg) {
        SunEvents::PolarNight => true,
        SunEvents::MidnightSun => false,
        SunEvents::RiseAndSet {
            sunrise_min,
            sunset_min,
        } => {
            let now = minutes_of_day(utc);
            let day_start = sunrise_min - NATOPS_NIGHT_MARGIN_MIN;
            let day_end = sunset_min + NATOPS_NIGHT_MARGIN_MIN;
            let in_day = [-1440.0, 0.0, 1440.0]
                .iter()
                .any(|shift| (day_start..=day_end).contains(&(now + shift)));
            !in_day
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Classifiers (three-valued logic: `Some(true)`, `Some(false)`, `None` = unknown)
// ---------------------------------------------------------------------------------------------

fn or3(values: &[Option<bool>]) -> Option<bool> {
    if values.contains(&Some(true)) {
        Some(true)
    } else if values.iter().all(|v| *v == Some(false)) {
        Some(false)
    } else {
        None
    }
}

fn and3(a: Option<bool>, b: Option<bool>) -> Option<bool> {
    match (a, b) {
        (Some(false), _) | (_, Some(false)) => Some(false),
        (Some(true), Some(true)) => Some(true),
        _ => None,
    }
}

fn ceiling_below(ceiling: Option<Ceiling>, ft: f64) -> Option<bool> {
    ceiling.map(|c| matches!(c, Ceiling::Ft(base) if base < ft))
}

fn ceiling_above(ceiling: Option<Ceiling>, ft: f64) -> Option<bool> {
    ceiling.map(|c| match c {
        Ceiling::NoCeiling => true,
        Ceiling::Ft(base) => base > ft,
    })
}

fn fog_below_5nm(fog: Option<Fog>) -> Option<bool> {
    fog.map(|f| matches!(f, Fog::Present { visibility_nm, .. } if visibility_nm < 5.0))
}

fn true_reasons(clauses: &[(Option<bool>, CaseReason)]) -> Vec<CaseReason> {
    clauses
        .iter()
        .filter(|(value, _)| *value == Some(true))
        .map(|(_, reason)| *reason)
        .collect()
}

fn values(clauses: &[(Option<bool>, CaseReason)]) -> Vec<Option<bool>> {
    clauses.iter().map(|(value, _)| *value).collect()
}

/// ED's rule.
pub fn classify_ed(c: &RecoveryConditions) -> (OrderedCase, Vec<CaseReason>) {
    let dense = c.cloud_density_0_10.map(|d| d > 8.0);
    let case_iii = [
        (c.dark, CaseReason::Dark),
        (
            and3(dense, ceiling_below(c.ceiling, 1000.0)),
            CaseReason::DenseCloudLowCeiling,
        ),
        (
            and3(dense, c.precipitation),
            CaseReason::DenseCloudPrecipitation,
        ),
        (fog_below_5nm(c.fog), CaseReason::FogBelow5Nm),
    ];
    match or3(&values(&case_iii)) {
        Some(true) => (OrderedCase::III, true_reasons(&case_iii)),
        None => (OrderedCase::Indeterminate, Vec::new()),
        Some(false) => {
            let case_i = [
                (
                    c.cloud_density_0_10.map(|d| d < 6.0),
                    CaseReason::CloudDensityBelow6,
                ),
                (
                    ceiling_above(c.ceiling, 1000.0),
                    CaseReason::CeilingAbove1000Ft,
                ),
            ];
            match or3(&values(&case_i)) {
                Some(true) => (OrderedCase::I, true_reasons(&case_i)),
                Some(false) => (OrderedCase::II, vec![CaseReason::NeitherCaseIIINorCaseI]),
                None => (OrderedCase::Indeterminate, Vec::new()),
            }
        }
    }
}

/// NATOPS minima, diagnostic only.
pub fn classify_natops(c: &RecoveryConditions) -> (OrderedCase, Vec<CaseReason>) {
    let case_iii = [
        (c.natops_night, CaseReason::NatopsNight),
        (
            ceiling_below(c.natops_ceiling, 1000.0),
            CaseReason::CeilingBelow1000Ft,
        ),
        (
            or3(&[fog_below_5nm(c.fog), c.visibility_nm.map(|v| v < 5.0)]),
            CaseReason::VisibilityBelow5Nm,
        ),
    ];
    match or3(&values(&case_iii)) {
        Some(true) => (OrderedCase::III, true_reasons(&case_iii)),
        None => (OrderedCase::Indeterminate, Vec::new()),
        Some(false) => match c.natops_ceiling {
            Some(Ceiling::NoCeiling) => (OrderedCase::I, vec![CaseReason::CeilingAtOrAbove3000Ft]),
            Some(Ceiling::Ft(base)) if base >= 3000.0 => {
                (OrderedCase::I, vec![CaseReason::CeilingAtOrAbove3000Ft])
            }
            Some(Ceiling::Ft(_)) => (OrderedCase::II, vec![CaseReason::CeilingBelow3000Ft]),
            None => (OrderedCase::Indeterminate, Vec::new()),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cloud_presets::parse_clouds_lua;
    use crate::mission_weather::{parse_mission_weather, MissionWeatherQuery};

    /// Carrier off Batumi, Caucasus (UTC+4).
    const CARRIER: Option<(f64, f64)> = Some((41.6, 41.0));

    fn utc(y: i32, m: u8, d: u8, h: u8, min: u8) -> PrimitiveDateTime {
        PrimitiveDateTime::new(
            Date::from_calendar_date(y, Month::try_from(m).unwrap(), d).unwrap(),
            Time::from_hms(h, min, 0).unwrap(),
        )
    }

    /// A weather reply: Caucasus, 15 June 2024 at `local_time_s`, no fog, 80 km visibility.
    /// Keys in `extra` override the defaults (JSON last-key-wins).
    fn reply(local_time_s: f64, extra: &str) -> MissionWeatherQuery {
        let json = format!(
            r#"{{"snippet_version": 1, "abs_time_s": {local_time_s}, "mission_start_time_s": 43200,
                "theatre": "Caucasus", "atmosphere_type": 0,
                "date": {{"year": 2024, "month": 6, "day": 15}},
                "visibility_m": 80000,
                "runtime_fog_visibility_m": 0, "runtime_fog_thickness_m": 0
                {extra}}}"#
        );
        parse_mission_weather(&json)
    }

    fn manual(density: f64, base_m: f64, precip: i64) -> String {
        format!(
            r#", "clouds": {{"base_m": {base_m}, "thickness_m": 500, "density": {density}, "iprecptns": {precip}}}"#
        )
    }

    /// 12:00 local.
    const NOON: f64 = 12.0 * 3600.0;
    /// 02:00 local the next day.
    const NEXT_DAY_0200: f64 = 26.0 * 3600.0;

    fn cases(query: &MissionWeatherQuery) -> (OrderedCase, OrderedCase) {
        let a = assess_recovery_case(query, CARRIER);
        (a.ordered, a.natops.case)
    }

    #[test]
    fn first_live_sample_fog_gives_case_iii() {
        // 27 September 2026 dedicated-server reading: clouds 5/10 at 3,000 m, fog 2,000 m.
        let q = reply(
            43561.41,
            &(manual(5.0, 3000.0, 0)
                + r#", "runtime_fog_visibility_m": 2000, "runtime_fog_thickness_m": 1000"#),
        );
        let a = assess_recovery_case(&q, CARRIER);
        assert_eq!(a.ordered, OrderedCase::III);
        assert_eq!(a.reasons, vec![CaseReason::FogBelow5Nm]);
        assert_eq!(a.natops.case, OrderedCase::III);
        assert_eq!(a.natops.reasons, vec![CaseReason::VisibilityBelow5Nm]);
        assert_eq!(a.conditions.dark, Some(false));
    }

    #[test]
    fn same_sky_without_fog_is_case_i() {
        let q = reply(NOON, &manual(5.0, 3000.0, 0));
        assert_eq!(cases(&q), (OrderedCase::I, OrderedCase::I));
    }

    #[test]
    fn dense_low_overcast_is_case_iii() {
        let q = reply(NOON, &manual(9.0, 250.0, 0));
        let a = assess_recovery_case(&q, CARRIER);
        assert_eq!(a.ordered, OrderedCase::III);
        assert_eq!(a.reasons, vec![CaseReason::DenseCloudLowCeiling]);
        assert_eq!(a.natops.case, OrderedCase::III);
    }

    #[test]
    fn dense_overcast_with_rain_is_case_iii_even_high() {
        let q = reply(NOON, &manual(9.0, 2000.0, 1));
        let a = assess_recovery_case(&q, CARRIER);
        assert_eq!(a.ordered, OrderedCase::III);
        assert_eq!(a.reasons, vec![CaseReason::DenseCloudPrecipitation]);
    }

    #[test]
    fn overcast_at_1500_ft_without_rain_is_ed_case_i_but_natops_case_ii() {
        let q = reply(NOON, &manual(9.0, 457.2, 0));
        assert_eq!(cases(&q), (OrderedCase::I, OrderedCase::II));
    }

    #[test]
    fn broken_below_1000_ft_is_ed_case_ii_natops_case_iii() {
        let q = reply(NOON, &manual(7.0, 280.0, 0));
        assert_eq!(cases(&q), (OrderedCase::II, OrderedCase::III));
    }

    #[test]
    fn ceiling_of_exactly_1000_ft_with_density_7_is_case_ii() {
        // 304.8 m is 1,000 ft: neither below 1,000 nor above it.
        let q = reply(NOON, &manual(7.0, 304.8, 0));
        let a = assess_recovery_case(&q, CARRIER);
        assert_eq!(a.conditions.ceiling, Some(Ceiling::Ft(1000.0)));
        assert_eq!(a.ordered, OrderedCase::II);
    }

    #[test]
    fn density_boundaries_6_and_8() {
        // Density 6 is not "< 6"; with a low ceiling it is not Case I, and not "> 8" either.
        assert_eq!(
            cases(&reply(NOON, &manual(6.0, 200.0, 0))).0,
            OrderedCase::II
        );
        assert_eq!(
            cases(&reply(NOON, &manual(8.0, 200.0, 0))).0,
            OrderedCase::II
        );
        assert_eq!(
            cases(&reply(NOON, &manual(5.9, 200.0, 0))).0,
            OrderedCase::I
        );
    }

    #[test]
    fn fog_visibility_boundary_at_5_nm() {
        let fog = |m: f64| {
            reply(
                NOON,
                &(manual(0.0, 1000.0, 0)
                    + &format!(
                        r#", "runtime_fog_visibility_m": {m}, "runtime_fog_thickness_m": 300"#
                    )),
            )
        };
        assert_eq!(cases(&fog(9000.0)).0, OrderedCase::III);
        assert_eq!(cases(&fog(5.0 * 1852.0)).0, OrderedCase::I);
    }

    #[test]
    fn night_is_case_iii_whatever_the_sky() {
        let q = reply(NEXT_DAY_0200, &manual(0.0, 1000.0, 0));
        let a = assess_recovery_case(&q, CARRIER);
        assert_eq!(a.ordered, OrderedCase::III);
        assert_eq!(a.reasons, vec![CaseReason::Dark]);
        assert_eq!(a.natops.reasons, vec![CaseReason::NatopsNight]);
    }

    #[test]
    fn dynamic_weather_is_indeterminate_by_day_and_case_iii_at_night() {
        let dynamic = r#", "atmosphere_type": 1"#;
        let day = assess_recovery_case(&reply(NOON, dynamic), CARRIER);
        assert_eq!(day.ordered, OrderedCase::Indeterminate);
        assert!(day.unknown_inputs.contains(&CaseReason::DynamicWeather));
        let night = assess_recovery_case(&reply(NEXT_DAY_0200, dynamic), CARRIER);
        assert_eq!(night.ordered, OrderedCase::III);
    }

    #[test]
    fn missing_carrier_position_is_indeterminate_by_day() {
        let q = reply(NOON, &manual(0.0, 1000.0, 0));
        let a = assess_recovery_case(&q, None);
        assert_eq!(a.ordered, OrderedCase::Indeterminate);
        assert!(a
            .unknown_inputs
            .contains(&CaseReason::MissingCarrierPosition));
    }

    #[test]
    fn missing_fog_data_blocks_case_i_but_not_a_sufficient_case_iii() {
        let json = r#"{"snippet_version": 1, "abs_time_s": 43200, "theatre": "Caucasus",
            "atmosphere_type": 0, "date": {"year": 2024, "month": 6, "day": 15},
            "visibility_m": 80000, "clouds": {"base_m": 1000, "density": 0, "iprecptns": 0}}"#;
        let a = assess_recovery_case(&parse_mission_weather(json), CARRIER);
        assert_eq!(a.ordered, OrderedCase::Indeterminate);
        assert!(a.unknown_inputs.contains(&CaseReason::MissingFogData));
        let json = json.replace(
            r#""density": 0, "iprecptns": 0"#,
            r#""density": 9, "iprecptns": 1"#,
        );
        let a = assess_recovery_case(&parse_mission_weather(&json), CARRIER);
        assert_eq!(a.ordered, OrderedCase::III);
    }

    #[test]
    fn weather_unavailable_is_indeterminate() {
        let q = parse_mission_weather("not json");
        let a = assess_recovery_case(&q, CARRIER);
        assert_eq!(a.ordered, OrderedCase::Indeterminate);
        assert_eq!(a.natops.case, OrderedCase::Indeterminate);
        assert_eq!(a.unknown_inputs, vec![CaseReason::WeatherUnavailable]);
    }

    #[test]
    fn unknown_theatre_leaves_dark_unknown() {
        let q = parse_mission_weather(
            r#"{"snippet_version": 1, "abs_time_s": 43200, "theatre": "Moon",
                "atmosphere_type": 0, "date": {"year": 2024, "month": 6, "day": 15}}"#,
        );
        let a = assess_recovery_case(&q, CARRIER);
        assert_eq!(a.conditions.dark, None);
        assert!(a.unknown_inputs.contains(&CaseReason::UnknownTheatre));
    }

    fn preset_table() -> CloudPresetTable {
        parse_clouds_lua(
            r#"clouds = { presets = {
                Thin = { readableName = 'x METAR: FEW/SCT 7/8', precipitationPower = -1,
                         presetAltMin = 800, presetAltMax = 4000,
                         layers = { { altitudeMin = 2520, altitudeMax = 3000, coverage = 0.36 },
                                    { altitudeMin = 6000, altitudeMax = 7000, coverage = 0.3 } } },
                Wet = { readableName = 'x METAR: VIS 3-5KM RA OVC 3/15', precipitationPower = 0.3,
                        presetAltMin = 400, presetAltMax = 3000,
                        layers = { { altitudeMin = 2940, altitudeMax = 5000, coverage = 0.61 } } },
                ThinUnderBroken = { readableName = 'x METAR: SCT 8 BKN 20', precipitationPower = -1,
                        presetAltMin = 400, presetAltMax = 3000,
                        layers = { { altitudeMin = 2000, altitudeMax = 2500, coverage = 0.4 },
                                   { altitudeMin = 6000, altitudeMax = 7000, coverage = 0.7 } } },
                NoMetar = { readableName = 'Low clouds', precipitationPower = -1,
                        presetAltMin = 400, presetAltMax = 3000,
                        layers = { { altitudeMin = 600, altitudeMax = 900, coverage = 0.6 } } },
            } }"#,
            Some("test".into()),
        )
        .unwrap()
    }

    fn preset_reply(local_time_s: f64, preset: &str, base_m: f64) -> MissionWeatherQuery {
        reply(
            local_time_s,
            &format!(
                r#", "clouds": {{"preset": "{preset}", "base_m": {base_m}, "thickness_m": 200, "density": 0, "iprecptns": 0}}"#
            ),
        )
    }

    #[test]
    fn thin_preset_by_day_is_case_i() {
        let a = assess_with_presets(
            &preset_reply(NOON, "Thin", 2520.0),
            CARRIER,
            &preset_table(),
        );
        assert_eq!(a.conditions.cloud_density_0_10, Some(3.0));
        assert_eq!(a.ordered, OrderedCase::I);
        assert_eq!(a.natops.case, OrderedCase::I);
    }

    #[test]
    fn overcast_rain_preset_is_ed_case_iii_and_base_follows_the_mission() {
        let a = assess_with_presets(&preset_reply(NOON, "Wet", 600.0), CARRIER, &preset_table());
        assert_eq!(a.conditions.precipitation, Some(true));
        assert_eq!(a.conditions.ceiling, Some(Ceiling::Ft(feet(600.0))));
        assert_eq!(a.ordered, OrderedCase::III);
        assert_eq!(a.reasons, vec![CaseReason::DenseCloudPrecipitation]);
        // NATOPS has no precipitation clause: a 1,969 ft ceiling is Case II.
        assert_eq!(a.natops.case, OrderedCase::II);
    }

    #[test]
    fn thin_lowest_layer_under_a_broken_one_leaves_natops_ceiling_unknown() {
        let a = assess_with_presets(
            &preset_reply(NOON, "ThinUnderBroken", 2000.0),
            CARRIER,
            &preset_table(),
        );
        assert_eq!(a.conditions.natops_ceiling, None);
        assert_eq!(a.natops.case, OrderedCase::Indeterminate);
        assert_eq!(a.ordered, OrderedCase::I);
    }

    #[test]
    fn preset_without_metar_or_unknown_preset_is_indeterminate_by_day() {
        let table = preset_table();
        // Unknown density, but a 1,969 ft base with no precipitation cannot be Case III and is
        // Case I whatever the density.
        let a = assess_with_presets(&preset_reply(NOON, "NoMetar", 600.0), CARRIER, &table);
        assert_eq!(a.ordered, OrderedCase::I);
        assert_eq!(a.reasons, vec![CaseReason::CeilingAbove1000Ft]);
        // Under 1,000 ft the unknown density decides between Case II and Case III.
        let a = assess_with_presets(&preset_reply(NOON, "NoMetar", 200.0), CARRIER, &table);
        assert_eq!(a.ordered, OrderedCase::Indeterminate);
        assert!(a.unknown_inputs.contains(&CaseReason::MissingCloudData));
        let a = assess_with_presets(&preset_reply(NOON, "Preset999", 600.0), CARRIER, &table);
        assert_eq!(a.ordered, OrderedCase::Indeterminate);
        assert!(a.unknown_inputs.contains(&CaseReason::UnknownPreset));
    }

    #[test]
    fn checked_in_atmos_x_preset_is_classified() {
        // "Heavy rain with thick overcast cloud layer" at a 400 m base: dense, wet, low.
        let a = assess_recovery_case(&preset_reply(NOON, "Preset65", 400.0), CARRIER);
        assert_eq!(a.ordered, OrderedCase::III);
    }

    #[test]
    fn solar_noon_elevation_on_the_june_solstice() {
        // London, 21 June 2024, solar noon about 12:02 UTC: 90 - 51.5 + 23.44 = 61.9 degrees.
        let elevation = solar_elevation_deg(utc(2024, 6, 21, 12, 2), 51.5074, -0.1278);
        assert!((elevation - 61.9).abs() < 0.3, "{elevation}");
    }

    #[test]
    fn sunrise_and_sunset_match_published_times() {
        // London, 21 June 2024: sunrise 03:43 UTC, sunset 20:21 UTC.
        let SunEvents::RiseAndSet {
            sunrise_min,
            sunset_min,
        } = sun_events(
            Date::from_calendar_date(2024, Month::June, 21).unwrap(),
            51.5074,
            -0.1278,
        )
        else {
            panic!("expected sunrise and sunset");
        };
        assert!(
            (sunrise_min - (3.0 * 60.0 + 43.0)).abs() < 3.0,
            "{sunrise_min}"
        );
        assert!(
            (sunset_min - (20.0 * 60.0 + 21.0)).abs() < 3.0,
            "{sunset_min}"
        );
    }

    #[test]
    fn natops_night_starts_30_minutes_after_sunset() {
        // London sunset 20:21 UTC on 21 June 2024.
        assert!(!is_natops_night(utc(2024, 6, 21, 20, 41), 51.5074, -0.1278));
        assert!(is_natops_night(utc(2024, 6, 21, 21, 1), 51.5074, -0.1278));
        // ...and ends 30 minutes before sunrise (03:43 UTC).
        assert!(is_natops_night(utc(2024, 6, 21, 3, 3), 51.5074, -0.1278));
        assert!(!is_natops_night(utc(2024, 6, 21, 3, 23), 51.5074, -0.1278));
    }

    #[test]
    fn polar_night_and_midnight_sun() {
        assert!(is_natops_night(utc(2024, 12, 21, 12, 0), 78.0, 15.0));
        assert!(!is_natops_night(utc(2024, 6, 21, 0, 0), 78.0, 15.0));
    }

    #[test]
    fn report_summary_prefers_groove_entry_and_labels_night() {
        let day = assess_recovery_case(&reply(NOON, &manual(5.0, 3000.0, 0)), CARRIER);
        let night = assess_recovery_case(&reply(NEXT_DAY_0200, &manual(5.0, 3000.0, 0)), CARRIER);
        let report = RecoveryCaseReport::new(Some(day.clone()), Some(night), None);
        assert_eq!(report.source, "groove_entry");
        assert_eq!(report.ordered, OrderedCase::III);
        assert_eq!(report.night, Some(true));
        assert_eq!(report.label().as_deref(), Some("Case III (night)"));

        let report = RecoveryCaseReport::new(Some(day), None, None);
        assert_eq!(report.source, "attempt_start");
        assert_eq!(report.label().as_deref(), Some("Case I"));

        let report = RecoveryCaseReport::new(None, None, None);
        assert_eq!(report.source, "none");
        assert_eq!(report.ordered, OrderedCase::Indeterminate);
        assert_eq!(report.label(), None);
        assert_eq!(report.mismatch, None);
    }

    #[test]
    fn approach_mismatch_matrix() {
        use FlownApproach::*;
        use OrderedCase::*;
        assert_eq!(approach_mismatch(III, OverheadPattern), Some(true));
        assert_eq!(approach_mismatch(I, StraightIn), Some(true));
        assert_eq!(approach_mismatch(II, StraightIn), Some(true));
        assert_eq!(approach_mismatch(III, StraightIn), Some(false));
        assert_eq!(approach_mismatch(I, OverheadPattern), Some(false));
        assert_eq!(approach_mismatch(II, OverheadPattern), Some(false));
        assert_eq!(approach_mismatch(Indeterminate, StraightIn), None);
        assert_eq!(approach_mismatch(III, Unknown), None);
    }

    #[test]
    fn straight_in_in_case_i_weather_raises_the_mismatch_diagnostic() {
        let day = assess_recovery_case(&reply(NOON, &manual(5.0, 3000.0, 0)), CARRIER);
        let flown = crate::flown_approach::FlownApproachEvidence {
            kind: FlownApproach::StraightIn,
            case_i_groove_entry_confirmed: false,
            longest_straight_segment: None,
            criteria: Default::default(),
        };
        let report = RecoveryCaseReport::new(None, Some(day), Some(flown));
        assert_eq!(report.mismatch, Some(true));
        assert_eq!(report.diagnostics, vec![APPROACH_MISMATCH_DIAGNOSTIC]);
    }

    #[test]
    fn serialized_assessment_uses_case_labels() {
        let a = assess_recovery_case(&reply(NOON, &manual(5.0, 3000.0, 0)), CARRIER);
        let json = serde_json::to_value(&a).unwrap();
        assert_eq!(json["ordered"], "I");
        assert_eq!(json["natops"]["case"], "I");
        assert_eq!(json["conditions"]["cloud_source"]["kind"], "manual");
        assert_eq!(json["conditions"]["fog"]["kind"], "absent");
        assert!(json["assumptions"].as_array().unwrap().len() >= 5);
    }
}
