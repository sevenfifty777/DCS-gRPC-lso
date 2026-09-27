//! Raw mission weather and time, read from the mission scripting environment through
//! `CustomService.Eval`.
//!
//! This is the single weather source of the recovery-case work
//! (`docs/CASE_RECOVERY_DETECTION_PLAN_2026-09-26.md`, section 4). The Lua chunk below only copies
//! raw values out of `env.mission`, `timer` and `world.weather`; it contains no case logic, which
//! stays in Rust as AGENTS.md requires. Parsing is strict: a value of the wrong type or outside its
//! physical range becomes `None` plus a [`WeatherFieldIssue`], never a guessed value, and a reply
//! that is not the expected object makes the whole query [`MissionWeatherQuery::Unavailable`].
//!
//! Nothing here interprets the weather (preset density, which fog is active, what "dark" is):
//! that is the phase 2 classifier's job, calibrated against DCS first.

use serde::Serialize;
use serde_json::{Map, Value};

use crate::client::{CustomClient, GrpcResult};

/// Version of [`MISSION_WEATHER_LUA`]'s reply layout. Bump it with any change to the chunk.
pub const MISSION_WEATHER_SNIPPET_VERSION: u32 = 1;

/// Fixed, read-only Lua chunk sent through `CustomService.Eval`. It must run in the mission
/// environment (`CustomService`), not the GUI one (`HookService`), where `env.mission` and
/// `world.weather` do not exist. `nil` fields are simply absent from the JSON reply. The two
/// `world.weather` fog functions are called under `pcall` because older DCS builds lack them.
pub const MISSION_WEATHER_LUA: &str = r#"
local m = env.mission or {}
local w = m.weather or {}
local c = w.clouds or {}
local f = w.fog or {}
local out = {
  snippet_version = 1,
  abs_time_s = timer.getAbsTime(),
  mission_start_time_s = m.start_time,
  theatre = m.theatre,
  atmosphere_type = w.atmosphere_type,
  clouds = {
    preset = c.preset,
    base_m = c.base,
    thickness_m = c.thickness,
    density = c.density,
    iprecptns = c.iprecptns,
  },
  enable_fog = w.enable_fog,
  fog = { visibility_m = f.visibility, thickness_m = f.thickness },
  fog2 = w.fog2,
  visibility_m = w.visibility and w.visibility.distance or nil,
  enable_dust = w.enable_dust,
  dust_density = w.dust_density,
}
if m.date then
  out.date = { year = m.date.Year, month = m.date.Month, day = m.date.Day }
end
if world.weather then
  local ok, v = pcall(world.weather.getFogVisibilityDistance)
  if ok then out.runtime_fog_visibility_m = v end
  local ok2, t = pcall(world.weather.getFogThickness)
  if ok2 then out.runtime_fog_thickness_m = t end
end
return out
"#;

/// Mission start date as written in the mission file (`env.mission.date`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct MissionDate {
    pub year: i32,
    pub month: u8,
    pub day: u8,
}

/// Raw cloud block of the mission (`env.mission.weather.clouds`). With a preset, DCS leaves
/// `density` and `iprecptns` at 0 and the preset table decides; that lookup is phase 2.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct MissionClouds {
    pub preset: Option<String>,
    /// Cloud base, metres above sea level.
    pub base_m: Option<f64>,
    pub thickness_m: Option<f64>,
    /// Manual cloud density, 0 to 10.
    pub density_0_10: Option<f64>,
    /// DCS precipitation code (`iprecptns`): 0 none; other values are rain/snow/thunderstorm.
    pub precipitation_code: Option<i64>,
}

/// Legacy fog block (`env.mission.weather.fog`), meaningful only when `enable_fog` is true.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct MissionLegacyFog {
    pub visibility_m: Option<f64>,
    pub thickness_m: Option<f64>,
}

/// One value the Lua reply carried but that failed validation. The field is left `None`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WeatherFieldIssue {
    /// Dotted path of the field in the Lua reply, e.g. `clouds.density`.
    pub field: String,
    pub problem: String,
}

/// Raw weather and time snapshot, validated but not interpreted.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MissionWeatherSnapshot {
    pub snippet_version: u32,
    /// `timer.getAbsTime()`: seconds since midnight of the mission start day (may exceed 86 400).
    pub abs_time_s: Option<f64>,
    /// `env.mission.start_time`: seconds since midnight at mission start.
    pub mission_start_time_s: Option<f64>,
    pub date: Option<MissionDate>,
    pub theatre: Option<String>,
    /// Raw `atmosphere_type` (0 static weather, 1 dynamic weather).
    pub atmosphere_type: Option<i64>,
    /// `Some(true)` for dynamic weather; `None` when `atmosphere_type` is missing or unknown.
    pub dynamic_weather: Option<bool>,
    pub clouds: MissionClouds,
    pub legacy_fog_enabled: Option<bool>,
    pub legacy_fog: MissionLegacyFog,
    /// `fog2.mode` of the newer fog system, uninterpreted.
    pub fog2_mode: Option<i64>,
    /// The full `fog2` table as DCS returned it, kept for calibration until its layout is known.
    pub fog2_raw: Option<Value>,
    /// Mission visibility (`weather.visibility.distance`), metres.
    pub visibility_m: Option<f64>,
    pub dust_enabled: Option<bool>,
    pub dust_density: Option<f64>,
    /// `world.weather.getFogVisibilityDistance()` at query time, metres.
    pub runtime_fog_visibility_m: Option<f64>,
    /// `world.weather.getFogThickness()` at query time, metres.
    pub runtime_fog_thickness_m: Option<f64>,
    pub issues: Vec<WeatherFieldIssue>,
}

/// Why no snapshot could be obtained. Every variant leads to an `Indeterminate` case downstream,
/// never to an error that stops a recording.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WeatherUnavailableReason {
    /// `PERMISSION_DENIED`: the server runs with `evalEnabled = false`.
    EvalDisabled,
    /// `UNIMPLEMENTED`: the server does not offer the call.
    NotImplemented,
    /// `DEADLINE_EXCEEDED`.
    Timeout,
    /// `UNAVAILABLE`: the channel or the mission environment was not reachable.
    ServerUnavailable,
    /// The Lua chunk failed to load or raised an error (`UNKNOWN`/`INTERNAL`).
    ScriptError,
    /// The reply was not the JSON object this chunk returns.
    MalformedResponse,
    /// Any other gRPC status.
    OtherRpcError,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct WeatherUnavailable {
    pub reason: WeatherUnavailableReason,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum MissionWeatherQuery {
    Available(Box<MissionWeatherSnapshot>),
    Unavailable(WeatherUnavailable),
}

/// The single weather source: one `Eval` round trip, then strict parsing. Never fails; any
/// problem is reported as [`MissionWeatherQuery::Unavailable`]. Also returns the unparsed reply
/// when the call itself succeeded, for the `lso.exe weather` calibration dump.
pub async fn query_mission_weather(
    client: &mut CustomClient,
) -> (MissionWeatherQuery, Option<String>) {
    let result = client.eval(MISSION_WEATHER_LUA).await;
    let query = classify_eval_result(&result);
    if let MissionWeatherQuery::Unavailable(unavailable) = &query {
        tracing::warn!(
            reason = ?unavailable.reason,
            detail = %unavailable.detail,
            "mission weather unavailable"
        );
    }
    (query, result.ok())
}

/// Turn the raw `Eval` result into a query outcome. Pure, so every failure path is testable.
pub fn classify_eval_result(result: &GrpcResult<String>) -> MissionWeatherQuery {
    match result {
        Ok(json) => parse_mission_weather(json),
        Err(status) => MissionWeatherQuery::Unavailable(WeatherUnavailable {
            reason: reason_for_status(status.code()),
            detail: status.message().to_string(),
        }),
    }
}

fn reason_for_status(code: tonic::Code) -> WeatherUnavailableReason {
    match code {
        tonic::Code::PermissionDenied => WeatherUnavailableReason::EvalDisabled,
        tonic::Code::Unimplemented => WeatherUnavailableReason::NotImplemented,
        tonic::Code::DeadlineExceeded => WeatherUnavailableReason::Timeout,
        tonic::Code::Unavailable => WeatherUnavailableReason::ServerUnavailable,
        tonic::Code::Unknown | tonic::Code::Internal => WeatherUnavailableReason::ScriptError,
        _ => WeatherUnavailableReason::OtherRpcError,
    }
}

fn malformed(detail: impl Into<String>) -> MissionWeatherQuery {
    MissionWeatherQuery::Unavailable(WeatherUnavailable {
        reason: WeatherUnavailableReason::MalformedResponse,
        detail: detail.into(),
    })
}

/// Parse the JSON reply of [`MISSION_WEATHER_LUA`].
pub fn parse_mission_weather(json: &str) -> MissionWeatherQuery {
    let value: Value = match serde_json::from_str(json) {
        Ok(value) => value,
        Err(err) => return malformed(format!("reply is not valid JSON: {err}")),
    };
    let Value::Object(root) = value else {
        return malformed("reply is not a JSON object");
    };
    match root.get("snippet_version").and_then(Value::as_f64) {
        Some(v) if v == f64::from(MISSION_WEATHER_SNIPPET_VERSION) => {}
        Some(v) => {
            return malformed(format!(
                "snippet_version {v} does not match {MISSION_WEATHER_SNIPPET_VERSION}"
            ))
        }
        None => return malformed("reply has no numeric snippet_version"),
    }

    let mut p = FieldParser::default();
    let clouds = p.object(&root, "clouds");
    let fog = p.object(&root, "fog");
    let fog2_raw = root.get("fog2").cloned();
    let fog2_mode = match &fog2_raw {
        Some(Value::Object(fog2)) => p.integer(fog2, "fog2.mode", "mode", 0, i64::MAX),
        Some(_) => {
            p.issue("fog2", "not an object");
            None
        }
        None => None,
    };
    let atmosphere_type = p.integer(&root, "atmosphere_type", "atmosphere_type", 0, i64::MAX);
    let dynamic_weather = match atmosphere_type {
        Some(0) => Some(false),
        Some(1) => Some(true),
        Some(other) => {
            p.issue(
                "atmosphere_type",
                format!("unknown value {other}; dynamic weather not decided"),
            );
            None
        }
        None => None,
    };

    let snapshot = MissionWeatherSnapshot {
        snippet_version: MISSION_WEATHER_SNIPPET_VERSION,
        abs_time_s: p.number(&root, "abs_time_s", "abs_time_s", 0.0, f64::MAX),
        mission_start_time_s: p.number(
            &root,
            "mission_start_time_s",
            "mission_start_time_s",
            0.0,
            f64::MAX,
        ),
        date: p.date(&root),
        theatre: p.string(&root, "theatre", "theatre"),
        atmosphere_type,
        dynamic_weather,
        clouds: MissionClouds {
            preset: clouds
                .as_ref()
                .and_then(|c| p.string(c, "clouds.preset", "preset")),
            base_m: clouds
                .as_ref()
                .and_then(|c| p.number(c, "clouds.base_m", "base_m", 0.0, f64::MAX)),
            thickness_m: clouds
                .as_ref()
                .and_then(|c| p.number(c, "clouds.thickness_m", "thickness_m", 0.0, f64::MAX)),
            density_0_10: clouds
                .as_ref()
                .and_then(|c| p.number(c, "clouds.density", "density", 0.0, 10.0)),
            precipitation_code: clouds
                .as_ref()
                .and_then(|c| p.integer(c, "clouds.iprecptns", "iprecptns", 0, i64::MAX)),
        },
        legacy_fog_enabled: p.boolean(&root, "enable_fog", "enable_fog"),
        legacy_fog: MissionLegacyFog {
            visibility_m: fog
                .as_ref()
                .and_then(|f| p.number(f, "fog.visibility_m", "visibility_m", 0.0, f64::MAX)),
            thickness_m: fog
                .as_ref()
                .and_then(|f| p.number(f, "fog.thickness_m", "thickness_m", 0.0, f64::MAX)),
        },
        fog2_mode,
        fog2_raw,
        visibility_m: p.number(&root, "visibility_m", "visibility_m", 0.0, f64::MAX),
        dust_enabled: p.boolean(&root, "enable_dust", "enable_dust"),
        dust_density: p.number(&root, "dust_density", "dust_density", 0.0, f64::MAX),
        runtime_fog_visibility_m: p.number(
            &root,
            "runtime_fog_visibility_m",
            "runtime_fog_visibility_m",
            0.0,
            f64::MAX,
        ),
        runtime_fog_thickness_m: p.number(
            &root,
            "runtime_fog_thickness_m",
            "runtime_fog_thickness_m",
            0.0,
            f64::MAX,
        ),
        issues: std::mem::take(&mut p.issues),
    };
    MissionWeatherQuery::Available(Box::new(snapshot))
}

/// Collects validation issues while extracting typed fields. An absent field is not an issue
/// (Lua drops `nil`); a present field of the wrong type or out of range is.
#[derive(Default)]
struct FieldParser {
    issues: Vec<WeatherFieldIssue>,
}

impl FieldParser {
    fn issue(&mut self, field: &str, problem: impl Into<String>) {
        self.issues.push(WeatherFieldIssue {
            field: field.to_string(),
            problem: problem.into(),
        });
    }

    fn object(&mut self, obj: &Map<String, Value>, key: &str) -> Option<Map<String, Value>> {
        match obj.get(key)? {
            Value::Object(inner) => Some(inner.clone()),
            // `net.lua2json` may encode an empty Lua table as an array.
            Value::Array(items) if items.is_empty() => Some(Map::new()),
            _ => {
                self.issue(key, "not an object");
                None
            }
        }
    }

    fn number(
        &mut self,
        obj: &Map<String, Value>,
        path: &str,
        key: &str,
        min: f64,
        max: f64,
    ) -> Option<f64> {
        let value = obj.get(key)?;
        let Some(n) = value.as_f64() else {
            self.issue(path, "not a number");
            return None;
        };
        if !n.is_finite() || n < min || n > max {
            self.issue(path, format!("{n} outside {min}..={max}"));
            return None;
        }
        Some(n)
    }

    fn integer(
        &mut self,
        obj: &Map<String, Value>,
        path: &str,
        key: &str,
        min: i64,
        max: i64,
    ) -> Option<i64> {
        let value = obj.get(key)?;
        let Some(n) = value.as_f64() else {
            self.issue(path, "not a number");
            return None;
        };
        if !n.is_finite() || n.fract() != 0.0 || n < min as f64 || n > max as f64 {
            self.issue(path, format!("{n} is not an integer in {min}..={max}"));
            return None;
        }
        Some(n as i64)
    }

    fn boolean(&mut self, obj: &Map<String, Value>, path: &str, key: &str) -> Option<bool> {
        let value = obj.get(key)?;
        match value.as_bool() {
            Some(b) => Some(b),
            None => {
                self.issue(path, "not a boolean");
                None
            }
        }
    }

    fn string(&mut self, obj: &Map<String, Value>, path: &str, key: &str) -> Option<String> {
        let value = obj.get(key)?;
        match value.as_str() {
            Some(s) if !s.trim().is_empty() => Some(s.to_string()),
            Some(_) => {
                self.issue(path, "empty string");
                None
            }
            None => {
                self.issue(path, "not a string");
                None
            }
        }
    }

    fn date(&mut self, root: &Map<String, Value>) -> Option<MissionDate> {
        let date = self.object(root, "date")?;
        let year = self.integer(&date, "date.year", "year", 1900, 2200);
        let month = self.integer(&date, "date.month", "month", 1, 12);
        let day = self.integer(&date, "date.day", "day", 1, 31);
        let (year, month, day) = (year?, month?, day?);
        let calendar_month = time::Month::try_from(month as u8).ok()?;
        if time::Date::from_calendar_date(year as i32, calendar_month, day as u8).is_err() {
            self.issue(
                "date",
                format!("{year}-{month}-{day} is not a calendar date"),
            );
            return None;
        }
        Some(MissionDate {
            year: year as i32,
            month: month as u8,
            day: day as u8,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MODERN_PRESET_REPLY: &str = r#"{
        "snippet_version": 1,
        "abs_time_s": 43215.5,
        "mission_start_time_s": 43200,
        "theatre": "PersianGulf",
        "atmosphere_type": 0,
        "clouds": {"preset": "Preset1", "base_m": 4200, "thickness_m": 200, "density": 0, "iprecptns": 0},
        "enable_fog": false,
        "fog": {"visibility_m": 0, "thickness_m": 0},
        "fog2": {"mode": 2},
        "visibility_m": 80000,
        "enable_dust": false,
        "dust_density": 0,
        "date": {"year": 2024, "month": 6, "day": 21},
        "runtime_fog_visibility_m": 0,
        "runtime_fog_thickness_m": 0
    }"#;

    fn available(json: &str) -> MissionWeatherSnapshot {
        match parse_mission_weather(json) {
            MissionWeatherQuery::Available(snapshot) => *snapshot,
            other => panic!("expected a snapshot, got {other:?}"),
        }
    }

    fn unavailable_reason(query: MissionWeatherQuery) -> WeatherUnavailableReason {
        match query {
            MissionWeatherQuery::Unavailable(u) => u.reason,
            other => panic!("expected unavailable, got {other:?}"),
        }
    }

    #[test]
    fn modern_preset_mission_parses_every_field() {
        let s = available(MODERN_PRESET_REPLY);
        assert!(s.issues.is_empty(), "{:?}", s.issues);
        assert_eq!(s.abs_time_s, Some(43215.5));
        assert_eq!(s.mission_start_time_s, Some(43200.0));
        assert_eq!(
            s.date,
            Some(MissionDate {
                year: 2024,
                month: 6,
                day: 21
            })
        );
        assert_eq!(s.theatre.as_deref(), Some("PersianGulf"));
        assert_eq!(s.dynamic_weather, Some(false));
        assert_eq!(s.clouds.preset.as_deref(), Some("Preset1"));
        assert_eq!(s.clouds.base_m, Some(4200.0));
        assert_eq!(s.clouds.density_0_10, Some(0.0));
        assert_eq!(s.clouds.precipitation_code, Some(0));
        assert_eq!(s.legacy_fog_enabled, Some(false));
        assert_eq!(s.fog2_mode, Some(2));
        assert_eq!(s.fog2_raw, Some(serde_json::json!({"mode": 2})));
        assert_eq!(s.visibility_m, Some(80000.0));
        assert_eq!(s.runtime_fog_visibility_m, Some(0.0));
    }

    #[test]
    fn legacy_manual_cloud_mission_without_fog2_or_preset() {
        let s = available(
            r#"{"snippet_version": 1, "abs_time_s": 5, "atmosphere_type": 0,
                "clouds": {"base_m": 2900, "thickness_m": 200, "density": 6, "iprecptns": 0},
                "enable_fog": false, "fog": {"visibility_m": 0, "thickness_m": 0},
                "visibility_m": 80000, "date": {"year": 2016, "month": 6, "day": 21}}"#,
        );
        assert!(s.issues.is_empty(), "{:?}", s.issues);
        assert_eq!(s.clouds.preset, None);
        assert_eq!(s.clouds.density_0_10, Some(6.0));
        assert_eq!(s.fog2_mode, None);
        assert_eq!(s.fog2_raw, None);
        // Absent because the Lua `pcall` failed or the functions do not exist: not an issue.
        assert_eq!(s.runtime_fog_visibility_m, None);
    }

    #[test]
    fn dynamic_weather_flag() {
        let s = available(r#"{"snippet_version": 1, "atmosphere_type": 1}"#);
        assert_eq!(s.dynamic_weather, Some(true));
        let s = available(r#"{"snippet_version": 1, "atmosphere_type": 7}"#);
        assert_eq!(s.dynamic_weather, None);
        assert_eq!(s.atmosphere_type, Some(7));
        assert_eq!(s.issues.len(), 1);
    }

    #[test]
    fn missing_fields_are_none_without_issues() {
        let s = available(r#"{"snippet_version": 1}"#);
        assert!(s.issues.is_empty());
        assert_eq!(s.abs_time_s, None);
        assert_eq!(s.date, None);
        assert_eq!(s.clouds, MissionClouds::default());
        assert_eq!(s.dynamic_weather, None);
    }

    #[test]
    fn out_of_range_and_wrong_type_values_become_issues_not_values() {
        let s = available(
            r#"{"snippet_version": 1, "abs_time_s": -1,
                "clouds": {"density": 11, "base_m": "high", "iprecptns": 1.5, "preset": ""},
                "enable_fog": "yes", "visibility_m": -5,
                "date": {"year": 2023, "month": 2, "day": 30}}"#,
        );
        assert_eq!(s.abs_time_s, None);
        assert_eq!(s.clouds.density_0_10, None);
        assert_eq!(s.clouds.base_m, None);
        assert_eq!(s.clouds.precipitation_code, None);
        assert_eq!(s.clouds.preset, None);
        assert_eq!(s.legacy_fog_enabled, None);
        assert_eq!(s.visibility_m, None);
        assert_eq!(s.date, None);
        let fields: Vec<&str> = s.issues.iter().map(|i| i.field.as_str()).collect();
        for expected in [
            "abs_time_s",
            "clouds.density",
            "clouds.base_m",
            "clouds.iprecptns",
            "clouds.preset",
            "enable_fog",
            "visibility_m",
            "date",
        ] {
            assert!(
                fields.contains(&expected),
                "missing issue for {expected}: {fields:?}"
            );
        }
    }

    #[test]
    fn empty_lua_table_encoded_as_array_is_accepted() {
        let s = available(r#"{"snippet_version": 1, "clouds": [], "fog": []}"#);
        assert!(s.issues.is_empty(), "{:?}", s.issues);
        assert_eq!(s.clouds, MissionClouds::default());
    }

    #[test]
    fn malformed_replies_are_unavailable() {
        for json in [
            "not json",
            "[1, 2]",
            "null",
            r#"{"abs_time_s": 5}"#,
            r#"{"snippet_version": 2}"#,
            r#"{"snippet_version": "1"}"#,
        ] {
            assert_eq!(
                unavailable_reason(parse_mission_weather(json)),
                WeatherUnavailableReason::MalformedResponse,
                "{json}"
            );
        }
    }

    #[test]
    fn grpc_errors_map_to_reasons() {
        let cases = [
            (
                tonic::Status::permission_denied("eval operation is disabled"),
                WeatherUnavailableReason::EvalDisabled,
            ),
            (
                tonic::Status::unimplemented(""),
                WeatherUnavailableReason::NotImplemented,
            ),
            (
                tonic::Status::deadline_exceeded(""),
                WeatherUnavailableReason::Timeout,
            ),
            (
                tonic::Status::unavailable(""),
                WeatherUnavailableReason::ServerUnavailable,
            ),
            (
                tonic::Status::unknown("Failed to execute Lua code: boom"),
                WeatherUnavailableReason::ScriptError,
            ),
            (
                tonic::Status::internal(""),
                WeatherUnavailableReason::ScriptError,
            ),
            (
                tonic::Status::unauthenticated(""),
                WeatherUnavailableReason::OtherRpcError,
            ),
        ];
        for (status, expected) in cases {
            let result: GrpcResult<String> = Err(Box::new(status));
            assert_eq!(unavailable_reason(classify_eval_result(&result)), expected);
        }
    }

    #[test]
    fn eval_disabled_keeps_the_server_message() {
        let result: GrpcResult<String> = Err(Box::new(tonic::Status::permission_denied(
            "eval operation is disabled",
        )));
        match classify_eval_result(&result) {
            MissionWeatherQuery::Unavailable(u) => {
                assert_eq!(u.detail, "eval operation is disabled")
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn snippet_declares_the_version_the_parser_expects() {
        assert!(MISSION_WEATHER_LUA.contains(&format!(
            "snippet_version = {MISSION_WEATHER_SNIPPET_VERSION},"
        )));
    }

    #[test]
    fn serialized_query_is_tagged_by_status() {
        let json = serde_json::to_value(parse_mission_weather(MODERN_PRESET_REPLY)).unwrap();
        assert_eq!(json["status"], "available");
        let json = serde_json::to_value(parse_mission_weather("oops")).unwrap();
        assert_eq!(json["status"], "unavailable");
        assert_eq!(json["reason"], "malformed_response");
    }
}
