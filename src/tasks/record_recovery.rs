use std::borrow::Cow;
use std::collections::HashSet;
use std::io::Cursor;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use futures_util::future::Either;
use futures_util::stream::{select, BoxStream};
use futures_util::{FutureExt, StreamExt};
use once_cell::sync::Lazy;
use serenity::builder::{CreateAttachment, CreateEmbed, ExecuteWebhook};
use serenity::http::Http;
use serenity::model::id::UserId;
use serenity::model::mention::Mention;
use stubs::common::v0::{initiator, Airbase, Coalition, Initiator};
use stubs::mission::v0::stream_events_response::{
    CrashEvent, DeadEvent, Event, LandEvent, LandingQualityMarkEvent, PlayerLeaveUnitEvent,
    RunwayTouchEvent, UnitLostEvent,
};
use tacview::record::{self, Color, Coords, GlobalProperty, Property, Record, Tag, Update};
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;
use tokio::sync::mpsc;

use crate::client::{HookClient, MissionClient, UnitClient};
use crate::grading::{PassGrade, SpotGrade};
use crate::telemetry::TelemetryInvalidReason;
use crate::track::{Datum, GateDeviations, Grading, HookSampleStatus, Track, TrajectoryDeviation};
use crate::transform::Transform;

use super::{CompletedPass, TaskParams};

/// Serialisable snapshot of a single recovery attempt, written to a `.json` file alongside
/// the ACMI and PNG chart.
#[derive(serde::Serialize)]
struct RecoveryReport<'a> {
    schema_version: u32,
    recovery_id: &'a str,
    pilot_name: &'a str,
    pilot_kind: super::PilotKind,
    aircraft_type: &'a str,
    aircraft_id: Option<i64>,
    carrier_id: u32,
    carrier_name: &'a str,
    carrier_type: &'a str,
    recovery_mode: &'a str,
    session_id: i64,
    generation: u64,
    grading: &'a Grading,
    /// Approach grade before the AV-8B touchdown-accuracy bonus.
    approach_grade: PassGrade,
    /// Final grade shown on the greenie board.
    pass_grade: PassGrade,
    grade_points: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    spot: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    intended_spot: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    actual_nearest_spot: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    spot_grade: Option<SpotGrade>,
    #[serde(skip_serializing_if = "Option::is_none")]
    spot_distance_m: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    spot_bonus_points: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    dcs_grading: Option<&'a str>,
    gate_deviations: &'a GateDeviations,
    /// Continuous groove-to-touchdown GS/lineup series (see `TrajectoryDeviation`), additive
    /// to `gate_deviations`. Empty for a pass that never entered the groove.
    trajectory_deviations: &'a [TrajectoryDeviation],
    /// Auditable CASE I CATOBAR deviation episodes and correction qualifications. Additive to
    /// schema-v3 and empty for V/STOL.
    grading_episodes: &'a [crate::grading::GradingEpisode],
    /// Wind at the carrier's position, queried once at report time (with one bounded retry and a
    /// fallback to the groove-entry high probe if `GetWind` keeps reading DCS's known
    /// `180deg/0.0 m/s` sentinel — see `wind_reading_is_groove_entry_fallback` and
    /// `tasking-roadmap.md`, P1). Contextual only: it never changes `pass_grade`/`grade_points`
    /// (see docs/GRADING_REFERENCE.md, "Wind"). Absent when every query attempt failed outright.
    #[serde(skip_serializing_if = "Option::is_none")]
    wind_heading_deg: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    wind_speed_mps: Option<f32>,
    /// `true` when `wind_heading_deg`/`wind_speed_mps` above come from the groove-entry high
    /// probe rather than this report's own `GetWind` query, because that query still looked like
    /// the sentinel after one retry. `false` otherwise, including when the query simply failed
    /// (in which case the two fields above are absent). Diagnostic only, never used for grading.
    wind_reading_is_groove_entry_fallback: bool,
    /// Whether a wind reference was established for the AoA correction in `datums`/
    /// `pattern_datums` (see docs/GRADING_REFERENCE.md, "AoA"). `false` means every recorded
    /// `aoa` value is the raw, wind-uncorrected geometric approximation for this recovery.
    wind_reference_established: bool,
    /// Raw `GetWind` probes behind `wind_reference_established`, kept for live diagnosis of a
    /// confirmed anomaly (the low-altitude probe intermittently reading DCS's `180deg/0.0 m/s`
    /// sentinel while the high probe stays coherent) — see `tasking-roadmap.md`, P1. Never used
    /// for grading; see `WindReferenceProbes::low_reading_overridden_by_high` for when the
    /// sentinel forced a fallback to the high probe for the AoA correction itself.
    #[serde(skip_serializing_if = "Option::is_none")]
    wind_reference_probes: Option<crate::track::WindReferenceProbes>,
    datums: &'a [Datum],
    /// Rendering-only segmentation of the full pattern history. This never
    /// changes track closure, telemetry completeness or grading.
    pattern_rendering: crate::draw::PatternRenderingDiagnostic,
    /// In-mission date/time from the DCS scenario clock (ISO-8601).
    #[serde(skip_serializing_if = "str::is_empty")]
    mission_datetime: &'a str,
    recording_started_at: &'a str,
    completed_at: &'a str,
    touchdown_time_dcs: Option<f64>,
    /// Seconds spent in the groove before touchdown (`entered_groove` to `touchdown_time_dcs`),
    /// `None` when either timestamp was never recorded. One of the two conditions for automatic
    /// `_OK_` (see docs/GRADING_REFERENCE.md, "Automatic `_OK_`") — previously computed but only
    /// ever surfaced in the Discord embed, making it impossible to audit `_OK_` eligibility from
    /// the JSON report alone when Discord is not configured.
    groove_time_secs: Option<f64>,
    /// Stable-axis measurements and exact PROJECT-DERIVED thresholds that latched CATOBAR groove
    /// entry. Absent from legacy reports and V/STOL box-only detection.
    #[serde(skip_serializing_if = "Option::is_none")]
    groove_entry: Option<&'a crate::track::GrooveEntryEvidence>,
    lso_version: &'static str,
    lso_commit: &'static str,
    lso_dirty: bool,
    dcs_grpc_version: &'a str,
    dcs_grpc_client_stubs: &'static str,
    dcs_grpc_compatibility: &'a str,
    acquisition_source: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    recovery_telemetry: Option<&'a super::position_collector::BufferedCollectionDiagnostics>,
    collection_profile: &'static str,
    target_frequency_hz: u32,
    missed_tick_behavior: &'static str,
    detectors_suspended: bool,
    detector_suspension_scope: &'static str,
    baseline_manifest: &'a super::BaselineManifest,
    outcome: &'a str,
    /// Legacy primary-cause alias retained for schema-v3 consumers.
    cause: &'a str,
    causes: ReportCauses<'a>,
    confidence: &'a str,
    grading_version: &'static str,
    grading_source: &'static str,
    wire_estimated: Option<u8>,
    wire_dcs: Option<u8>,
    wire_divergent: bool,
    wire_primary: &'static str,
    wire_estimation: &'a crate::track::WireEstimateEvidence,
    arrest_confirmation: &'a crate::track::ArrestConfirmationEvidence,
    /// Commanded hook state (`up`/`down`/`unknown`) latched from the pre-contact baseline.
    hook_state: crate::track::HookState,
    /// `dcs_wire`, `hook_transient`, `kinematic`, `unconfirmed` or `none`.
    arrest_evidence: &'static str,
    grading_availability: &'static str,
    assessment_scope: AssessmentScope,
    observed_from_distance_m: Option<f64>,
    missing_coverage: &'a [String],
    points_eligible: bool,
    fallback_source: FallbackSource,
    telemetry_quality: &'a crate::track::TelemetryQuality,
    events: &'a [crate::track::EventEvidence],
    spot_zone: &'a crate::track::SpotZoneObservation,
    touchdown_horizontal_speed_mps: Option<f64>,
    hook_observation: &'a crate::track::HookObservation,
    event_correlation: &'a super::event_correlator::EventCorrelationSummary,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
enum AssessmentScope {
    Full,
    Partial,
    OutcomeOnly,
    None,
}
impl AssessmentScope {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Full => "full",
            Self::Partial => "partial",
            Self::OutcomeOnly => "outcome_only",
            Self::None => "none",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
enum FallbackSource {
    Project,
    DcsLqm,
    Geometry,
    None,
}
impl FallbackSource {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Project => "project",
            Self::DcsLqm => "dcs_lqm",
            Self::Geometry => "geometry",
            Self::None => "none",
        }
    }
}

#[derive(serde::Serialize)]
struct ReportCauses<'a> {
    primary: &'a str,
    secondary: &'a [&'static str],
}

// v2: CATOBAR amplitude now also considers the continuous groove-to-touchdown trajectory
// (`trajectory_deviations`), not only the three point-in-time gates.
// v3: a worsening correction trend in the final seconds can cap an otherwise-Ok pass at (OK).
// v4: a moderate deviation inside the last 150 m before the ramp can cap an otherwise-Ok/(OK)
// pass at NoGrade.
// v5: lineup inside that window uses a fixed 150 m angular reference and exposes its raw offset
// in metres, so proximity to the ramp does not silently tighten the lateral threshold.
// v7: CASE I CATOBAR evaluates correction from the episode peak with zone-specific deadlines.
const GRADING_VERSION: &str = "project-derived-v7";
const GRADING_SOURCE: &str = "PROJECT-DERIVED";
#[derive(Debug)]
struct HookPoll {
    received_at: Instant,
    received_unix_ms: u64,
    raw: Option<f64>,
    status: HookSampleStatus,
    grpc_code: Option<String>,
}

struct AbortOnDrop(tokio::task::JoinHandle<()>);

impl Drop for AbortOnDrop {
    fn drop(&mut self) {
        self.0.abort();
    }
}

struct PriorityCollectorGuard {
    active: Option<std::sync::Arc<super::ActivePriorityPlanes>>,
    plane_id: u32,
}

impl PriorityCollectorGuard {
    fn new(params: &TaskParams<'_>) -> Self {
        if params.suspend_detectors_during_recovery {
            params.active_priority_planes.activate(params.plane_id);
            Self {
                active: Some(params.active_priority_planes.clone()),
                plane_id: params.plane_id,
            }
        } else {
            Self {
                active: None,
                plane_id: params.plane_id,
            }
        }
    }
}

impl Drop for PriorityCollectorGuard {
    fn drop(&mut self) {
        if let Some(active) = &self.active {
            active.deactivate(self.plane_id);
        }
    }
}

async fn sample_hook(
    channel: crate::client::GrpcChannel,
    plane_name: String,
    draw_argument: u32,
    config: super::HookSamplingConfig,
    tx: mpsc::Sender<HookPoll>,
) {
    let period = Duration::from_secs_f64(1.0 / config.frequency_hz as f64);
    let mut interval = tokio::time::interval(period);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut client = UnitClient::new(channel);
    loop {
        interval.tick().await;
        let (raw, status, grpc_code) = match client
            .get_draw_argument_value_with_timeout(&plane_name, draw_argument, config.timeout)
            .await
        {
            Ok(raw) if raw.is_finite() => (Some(raw), HookSampleStatus::Success, None),
            Ok(_) => (
                None,
                HookSampleStatus::Error,
                Some("non_finite_response".to_string()),
            ),
            Err(status) if status.code() == tonic::Code::DeadlineExceeded => (
                None,
                HookSampleStatus::Timeout,
                Some(grpc_code_name(status.code()).to_string()),
            ),
            Err(status) => (
                None,
                HookSampleStatus::Error,
                Some(grpc_code_name(status.code()).to_string()),
            ),
        };
        let poll = HookPoll {
            received_at: Instant::now(),
            received_unix_ms: unix_time_ms(),
            raw,
            status,
            grpc_code,
        };
        if tx.try_send(poll).is_err() {
            crate::metrics::RUNTIME_METRICS.hook_sample_dropped();
        }
    }
}

fn grpc_code_name(code: tonic::Code) -> &'static str {
    match code {
        tonic::Code::Ok => "ok",
        tonic::Code::Cancelled => "cancelled",
        tonic::Code::Unknown => "unknown",
        tonic::Code::InvalidArgument => "invalid_argument",
        tonic::Code::DeadlineExceeded => "deadline_exceeded",
        tonic::Code::NotFound => "not_found",
        tonic::Code::AlreadyExists => "already_exists",
        tonic::Code::PermissionDenied => "permission_denied",
        tonic::Code::ResourceExhausted => "resource_exhausted",
        tonic::Code::FailedPrecondition => "failed_precondition",
        tonic::Code::Aborted => "aborted",
        tonic::Code::OutOfRange => "out_of_range",
        tonic::Code::Unimplemented => "unimplemented",
        tonic::Code::Internal => "internal",
        tonic::Code::Unavailable => "unavailable",
        tonic::Code::DataLoss => "data_loss",
        tonic::Code::Unauthenticated => "unauthenticated",
    }
}

fn unix_time_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u64::MAX as u128) as u64
}

/// Query `GetWind`, retrying once (bounded — never a loop) if the first response looks like
/// DCS's known `180deg/0.0 m/s` sentinel (`tasking-roadmap.md`, P1). A single retry is enough to
/// ride out a transient hit without stacking extra `GetWind` calls onto the Lua-side telemetry
/// thread for the rest of the groove; a persistent sentinel after the retry is still returned
/// as-is; callers decide whether/how to fall back. `probe` only labels the tracing spans.
async fn query_wind_with_sentinel_retry(
    atmo: &mut crate::client::AtmosphereClient,
    lat: f64,
    lon: f64,
    alt: f64,
    probe: &'static str,
) -> Option<(u16, f32)> {
    let first = atmo.get_wind(lat, lon, alt).await;
    tracing::debug!(probe, alt_m = alt, result = ?first, "wind query");
    match first {
        Ok(w) if crate::track::is_wind_sentinel(w) => {
            let retried = atmo.get_wind(lat, lon, alt).await;
            tracing::debug!(
                probe,
                alt_m = alt,
                result = ?retried,
                "wind query retry after suspected DCS sentinel"
            );
            match retried {
                Ok(w) => Some(w),
                Err(err) => {
                    tracing::warn!(probe, ?err, "failed to query wind on sentinel retry");
                    None
                }
            }
        }
        Ok(w) => Some(w),
        Err(err) => {
            tracing::warn!(probe, ?err, "failed to query wind");
            None
        }
    }
}

fn drain_hook_samples(
    rx: &mut mpsc::Receiver<HookPoll>,
    track: &mut Track,
    associated_time_dcs: f64,
    frequency_hz: u64,
) {
    let max_age_ms = (2_000.0 / frequency_hz.max(1) as f64).max(750.0);
    while let Ok(poll) = rx.try_recv() {
        let age_ms = poll.received_at.elapsed().as_secs_f64() * 1_000.0;
        let status = if poll.status == HookSampleStatus::Success && age_ms > max_age_ms {
            HookSampleStatus::Stale
        } else {
            poll.status
        };
        track.observe_hook_sample_with_error(
            associated_time_dcs,
            poll.received_unix_ms,
            age_ms,
            poll.raw,
            status,
            poll.grpc_code,
        );
    }
}

pub static FILENAME_DATETIME_FORMAT: Lazy<Vec<time::format_description::FormatItem<'_>>> =
    Lazy::new(|| {
        time::format_description::parse("[year][month][day]-[hour][minute][second]").unwrap()
    });

/// ISO-8601 datetime format for the `grade_date` database column: `YYYY-MM-DD HH:MM:SS`.
pub static GRADE_DATE_FORMAT: Lazy<Vec<time::format_description::FormatItem<'_>>> =
    Lazy::new(|| {
        time::format_description::parse("[year]-[month]-[day] [hour]:[minute]:[second]").unwrap()
    });

fn recovery_outcome(grading: &Grading, is_vstol: bool) -> String {
    match (is_vstol, grading) {
        (_, Grading::Unknown) => "unknown".to_string(),
        (_, Grading::ApproachOnly) => "Approach only — outcome unknown".to_string(),
        (_, Grading::Bolter) => "Bolter".to_string(),
        // Intentional bolters are valid only for arrested recoveries. Keep the
        // V/STOL fallback defensive in case an invalid grading reaches this layer.
        (true, Grading::TouchAndGo { .. }) => "Waveoff/Go-around".to_string(),
        (
            false,
            Grading::TouchAndGo {
                cable_estimated: Some(estimated),
            },
        ) => format!("T&G (CQ) — would have caught wire {estimated}"),
        (false, Grading::TouchAndGo { .. }) => "T&G (CQ)".to_string(),
        (_, Grading::WaveoffUnknown) => "Waveoff/Go-around — initiator unknown".to_string(),
        (true, Grading::Recovered { .. }) => "Spot 7.5".to_string(),
        (
            false,
            Grading::Recovered {
                cable,
                cable_estimated,
            },
        ) => match (cable, cable_estimated) {
            (Some(dcs), Some(estimated)) if dcs == estimated => {
                format!("Arrested — wire {dcs} (DCS/LQM + Rust)")
            }
            (Some(dcs), Some(estimated)) => {
                format!("Arrested — DCS/LQM wire {dcs}; Rust estimate {estimated}")
            }
            (Some(dcs), None) => {
                format!("Arrested — DCS/LQM wire {dcs}; Rust estimate unavailable")
            }
            (None, Some(estimated)) => format!("Wire #{estimated} (Rust estimate)"),
            (None, None) => "Arrested — wire evidence unavailable".to_string(),
        },
    }
}

fn completeness_cause(completeness: crate::track::Completeness) -> &'static str {
    match completeness {
        crate::track::Completeness::InsufficientGates => "insufficient_gates",
        crate::track::Completeness::TelemetryGap => "telemetry_gap",
        crate::track::Completeness::InvalidTelemetry => "invalid_telemetry",
        crate::track::Completeness::UnconfirmedArrest => "unconfirmed_arrest",
        crate::track::Completeness::BufferLimit => "position_buffer_limit",
        crate::track::Completeness::Complete => "complete",
    }
}

fn correlate_late_event(
    correlator: &mut super::event_correlator::EventCorrelator,
    track: &mut Track,
    event: super::event_hub::SessionEvent,
) {
    match (event.time_dcs, event.event) {
        (
            time,
            Event::LandingQualityMark(LandingQualityMarkEvent {
                initiator:
                    Some(Initiator {
                        initiator: Some(initiator::Initiator::Unit(plane)),
                    }),
                place:
                    Some(Airbase {
                        unit: Some(carrier),
                        ..
                    }),
                comment,
            }),
        ) if correlator.accepts_pair(plane.id, carrier.id) => {
            correlator.landing_quality_mark(track, time, comment);
        }
        (
            time,
            Event::Land(LandEvent {
                initiator:
                    Some(Initiator {
                        initiator: Some(initiator::Initiator::Unit(plane)),
                    }),
                place:
                    Some(Airbase {
                        unit: Some(carrier),
                        ..
                    }),
            }),
        ) if correlator.accepts_pair(plane.id, carrier.id) => {
            correlator.touchdown(track, "late_land", time, carrier, plane);
        }
        (
            time,
            Event::RunwayTouch(RunwayTouchEvent {
                initiator:
                    Some(Initiator {
                        initiator: Some(initiator::Initiator::Unit(plane)),
                    }),
                place:
                    Some(Airbase {
                        unit: Some(carrier),
                        ..
                    }),
            }),
        ) if correlator.accepts_pair(plane.id, carrier.id) => {
            correlator.touchdown(track, "late_runway_touch", time, carrier, plane);
        }
        _ => {}
    }
}

#[tracing::instrument(
    skip_all,
    fields(carrier_name = params.carrier_name, plane_name = params.plane_name)
)]
pub async fn record_recovery(
    params: TaskParams<'_>,
    attempt_started_at_dcs: f64,
) -> Result<(), crate::error::Error> {
    let recording_started = Instant::now();
    let _recovery_guard = crate::metrics::RUNTIME_METRICS.recovery();
    let _priority_guard = PriorityCollectorGuard::new(&params);
    tracing::debug!("started recording");
    if params.suspend_detectors_during_recovery {
        let concurrent = params.active_priority_planes.active_count();
        if concurrent > 1 {
            // Never observed live before (tasking-roadmap.md, "Robustesse multi-recoveries
            // simultanées"): two or more aircraft are being recorded at the same instant,
            // possibly on different carriers -- this pass is not tracked in isolation.
            tracing::info!(
                concurrent_recoveries = concurrent,
                "recovery overlap: another aircraft is already being recorded"
            );
        }
    }

    // Identity was resolved against the occupied network slot when the task was
    // created. Re-resolving by display name here would mix homonyms or a pilot
    // who changed slot while this recovery was active.
    let pilot_name = params.pilot_name.to_string();
    debug_assert!(!params.pilot_identity.is_empty());

    // Tacview-20211111-143727-DCS-grpc-lso.zip
    let now = OffsetDateTime::now_local().unwrap_or_else(|_| OffsetDateTime::now_utc());
    let now_utc = now.to_offset(time::UtcOffset::UTC);
    let recovery_timestamp = now_utc.format(&Rfc3339).unwrap_or_default();
    let safe_pilot_name = pilot_name
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect::<String>();

    let mut acmi_unit_client = params
        .record_acmi
        .then(|| UnitClient::new(params.ch.clone()));
    let mut legacy_hook_client = (params.hook_sampling.mode
        == super::HookSamplingMode::LegacyInline)
        .then(|| UnitClient::new(params.ch.clone()));
    let mut position_collector = super::position_collector::PositionCollector::start(
        params.ch.clone(),
        params.position_source,
        params.session_id,
        params.generation,
        params.carrier_id,
        params.carrier_name,
        params.plane_id,
        params.plane_name,
    )
    .await?;
    let mut mission = (!params.positions_only).then(|| MissionClient::new(params.ch.clone()));
    let interval =
        crate::utils::interval::interval(Duration::from_millis(100), params.shutdown.clone());

    let mut acmi = Cursor::new(Vec::new());
    let mut recording = if params.record_acmi {
        Some(tacview::Writer::new_compressed(&mut acmi)?)
    } else {
        None
    };
    macro_rules! write_acmi {
        ($record:expr) => {
            if let Some(writer) = recording.as_mut() {
                writer.write($record)?;
            }
        };
    }
    let mut datums = Track::new(pilot_name.clone(), params.carrier_info, params.plane_info);
    let mut event_correlator = if params.positions_only {
        super::event_correlator::EventCorrelator::disabled(params.plane_id, params.carrier_id)
    } else {
        super::event_correlator::EventCorrelator::new(params.plane_id, params.carrier_id)
    };
    let mut last_telemetry_success = Instant::now();

    if params.record_acmi {
        let reference_time = mission
            .as_mut()
            .expect("mission client enabled with ACMI")
            .get_scenario_start_time()
            .await?;
        write_acmi!(GlobalProperty::ReferenceTime(reference_time));
        write_acmi!(GlobalProperty::RecordingTime(
            OffsetDateTime::now_utc().format(&Rfc3339).unwrap(),
        ));

        let mission_name = HookClient::new(params.ch.clone())
            .get_mission_name()
            .await?;
        write_acmi!(GlobalProperty::Title(format!(
            "Carrier Recovery during {}",
            mission_name
        )));
        write_acmi!(GlobalProperty::Author(format!(
            "dcs-grpc-lso v{}",
            env!("CARGO_PKG_VERSION")
        )));
    }

    // Query the theatre (map) name once at the start of the recording.
    let map_name: String = if params.positions_only {
        String::new()
    } else {
        match crate::client::WorldClient::new(params.ch.clone())
            .get_theatre()
            .await
        {
            Ok(t) => t,
            Err(err) => {
                tracing::warn!(?err, "failed to query theatre name");
                String::new()
            }
        }
    };

    let mut ref_written = false;
    let mut lat_ref = 0.0;
    let mut lon_ref = 0.0;

    if params.record_acmi {
        let client = acmi_unit_client.as_mut().expect("ACMI unit client enabled");
        write_acmi!(create_initial_update(client, 1, params.carrier_name).await?);
        write_acmi!(create_initial_update(client, 2, params.plane_name).await?);
    }

    let (events, event_progress): (
        BoxStream<'_, super::event_hub::SessionEventMessage>,
        Arc<AtomicU64>,
    ) = if params.positions_only {
        (
            futures_util::stream::pending().boxed(),
            Arc::new(AtomicU64::new(0)),
        )
    } else {
        let (subscription, event_progress) = params
            .event_hub
            .as_ref()
            .expect("session event hub enabled outside positions-only")
            .subscribe(attempt_started_at_dcs);
        (
            futures_util::stream::unfold(subscription, |mut subscription| async move {
                let message = subscription.recv().await;
                Some((message, subscription))
            })
            .boxed(),
            event_progress,
        )
    };
    let (mut hook_rx, _hook_sampler) = match (
        params.carrier_info.is_vstol(),
        params.hook_sampling.mode == super::HookSamplingMode::Independent,
        params.plane_info.hook_draw_argument,
    ) {
        (false, true, Some(draw_argument)) => {
            let (hook_tx, hook_rx) = mpsc::channel(64);
            (
                Some(hook_rx),
                Some(AbortOnDrop(tokio::spawn(sample_hook(
                    params.ch.clone(),
                    params.plane_name.to_string(),
                    draw_argument,
                    params.hook_sampling,
                    hook_tx,
                )))),
            )
        }
        _ => (None, None),
    };

    let mut known_carrier_coords = None;
    let mut known_plane_coords = None;
    let mut track_stopped: Option<Instant> = None;
    let mut lowest_altitude = f64::MAX;
    // Last known carrier geodetic position, used for the wind query at pass completion.
    let mut last_carrier_lat: f64 = 0.0;
    let mut last_carrier_lon: f64 = 0.0;
    let mut last_carrier_alt: f64 = 0.0;
    let mut warning_window_started = Instant::now();
    let mut warning_count = 0_u32;
    let mut warning_max_gap_ms = 0.0_f64;
    let mut last_invalid_source_warning: Option<Instant> = None;
    let mut pending_invalid_batches = 0_u64;
    let mut pending_invalid_snapshots = 0_u64;
    // Set once the wind reference query has been attempted (successfully or not), so it is
    // never repeated for the rest of the recovery. See docs/GRADING_REFERENCE.md, "AoA".
    let mut wind_reference_queried = false;

    // The whole merged stream ends on shutdown, not only the tick half of it: the event half
    // never ends on its own (every recorder holds the hub alive), so without this wrapper a
    // Ctrl-C parked the loop on `events` forever and finalization below was never reached
    // (review finding F01).
    let mut stream = params.shutdown.wrap_stream(select(
        interval.map(Either::Left),
        events.map(Either::Right),
    ));

    'recording: while let Some(next) = stream.next().await {
        match next {
            // next interval
            Either::Left(scheduled_tick) => {
                let _loop_timer = crate::metrics::RUNTIME_METRICS.recovery_loop();
                crate::metrics::RUNTIME_METRICS.observe_tick_lag(
                    Instant::now()
                        .saturating_duration_since(scheduled_tick.into())
                        .as_micros()
                        .min(u64::MAX as u128) as u64,
                );
                let batch = match position_collector
                    .poll(params.carrier_name, params.plane_name)
                    .await
                {
                    Ok(batch) => batch,
                    Err(status) if status.code() == tonic::Code::NotFound => {
                        tracing::info!("stop tracking because a unit no longer exists");
                        break 'recording;
                    }
                    Err(status) => {
                        position_collector.reset();
                        let silent_for = last_telemetry_success.elapsed();
                        if status.code() == tonic::Code::ResourceExhausted {
                            tracing::warn!(
                                ?status,
                                ?silent_for,
                                "server read quota exceeded despite the client budget; lower \
                                 --buffered-read-budget-per-second or raise recoveryTelemetry.readsPerSecond"
                            );
                        } else {
                            tracing::warn!(?status, ?silent_for, "transform polling failed");
                        }
                        if silent_for >= position_collector.recovery_watchdog() {
                            datums.mark_telemetry_gap(TelemetryInvalidReason::TelemetryGap);
                            break 'recording;
                        }
                        continue;
                    }
                };
                if batch.lost_snapshots > 0 {
                    tracing::warn!(
                        lost_snapshots = batch.lost_snapshots,
                        "source recovery telemetry reported retained-position loss"
                    );
                    datums.mark_source_buffer_loss(batch.lost_snapshots);
                }
                let identity_mismatch =
                    super::position_collector::identity_mismatch(&batch.invalid_observations);
                if batch.invalid_snapshots > 0 {
                    datums.record_invalid_source_observations(batch.invalid_observations);
                    pending_invalid_batches = pending_invalid_batches.saturating_add(1);
                    pending_invalid_snapshots =
                        pending_invalid_snapshots.saturating_add(batch.invalid_snapshots);
                    let should_report = last_invalid_source_warning.is_none_or(|last_reported| {
                        last_reported.elapsed() >= Duration::from_secs(10)
                    });
                    if should_report {
                        tracing::warn!(
                            invalid_batches = pending_invalid_batches,
                            invalid_snapshots = pending_invalid_snapshots,
                            "source recovery telemetry contained invalid unit observations"
                        );
                        last_invalid_source_warning = Some(Instant::now());
                        pending_invalid_batches = 0;
                        pending_invalid_snapshots = 0;
                    }
                }
                if let Some((entity, capture_time_dcs)) = identity_mismatch {
                    // The name now resolves to another unit incarnation (slot re-occupied,
                    // respawn): everything recorded so far belongs to the previous one, and
                    // nothing that follows does. End the attempt with that typed reason and
                    // finalise the evidence in hand (review finding F14).
                    let entity_name = match entity {
                        crate::telemetry::SourceObservationEntity::Aircraft => "aircraft",
                        crate::telemetry::SourceObservationEntity::Carrier => "carrier",
                    };
                    tracing::warn!(
                        entity = entity_name,
                        ?capture_time_dcs,
                        "source telemetry reports a different unit incarnation; ending the attempt"
                    );
                    datums.record_event(
                        "unit_identity_mismatch",
                        capture_time_dcs
                            .or_else(|| datums.last_observed_time_dcs())
                            .unwrap_or_default(),
                        false,
                        format!("source_reports_{entity_name}_id_mismatch"),
                    );
                    break 'recording;
                }
                let buffered_source = position_collector.is_buffered();
                let sample_count = batch.samples.len();
                for sample in batch.samples {
                    if sample.is_valid()
                        && (buffered_source || sample.source_age_ms <= f64::EPSILON)
                    {
                        last_telemetry_success = Instant::now();
                    }
                    if last_telemetry_success.elapsed() >= position_collector.recovery_watchdog() {
                        tracing::warn!(
                            silent_for = ?last_telemetry_success.elapsed(),
                            source_age_ms = sample.source_age_ms,
                            "active telemetry watchdog expired without source advancement"
                        );
                        datums.mark_telemetry_gap(TelemetryInvalidReason::TelemetryGap);
                        break 'recording;
                    }
                    if sample.has_warning() {
                        warning_count += 1;
                        warning_max_gap_ms =
                            warning_max_gap_ms.max(sample.sample_gap_ms.max(sample.source_age_ms));
                        if warning_window_started.elapsed() >= Duration::from_secs(10) {
                            tracing::warn!(
                                warning_count,
                                warning_max_gap_ms,
                                "telemetry quality degraded during reporting window"
                            );
                            warning_count = 0;
                            warning_max_gap_ms = 0.0;
                            warning_window_started = Instant::now();
                        }
                    }
                    let carrier = &sample.carrier;
                    let plane = &sample.plane;
                    let hook_state = match (
                        params.carrier_info.is_vstol(),
                        params.hook_sampling.mode == super::HookSamplingMode::LegacyInline,
                        params.plane_info.hook_draw_argument,
                    ) {
                        (false, true, Some(draw_argument)) => legacy_hook_client
                            .as_mut()
                            .expect("legacy hook client enabled")
                            .get_draw_argument_value(params.plane_name, draw_argument)
                            .await
                            .ok(),
                        _ => None,
                    };

                    if params.record_acmi {
                        if !ref_written {
                            lat_ref = carrier.lat;
                            lon_ref = carrier.lon;
                            write_acmi!(GlobalProperty::ReferenceLatitude(lat_ref));
                            write_acmi!(GlobalProperty::ReferenceLongitude(lon_ref));
                            ref_written = true;
                        }

                        let carrier_update = Update {
                            id: 1,
                            props: vec![Property::T(remove_unchanged(
                                Coords::default()
                                    .position(
                                        carrier.lat - lat_ref,
                                        carrier.lon - lon_ref,
                                        carrier.alt,
                                    )
                                    .uv(carrier.position.x, carrier.position.z)
                                    .orientation(carrier.yaw, carrier.pitch, carrier.roll)
                                    .heading(carrier.heading),
                                &mut known_carrier_coords,
                            ))],
                        };
                        let plane_update = Update {
                            id: 2,
                            props: vec![
                                Property::T(remove_unchanged(
                                    Coords::default()
                                        .position(
                                            plane.lat - lat_ref,
                                            plane.lon - lon_ref,
                                            plane.alt,
                                        )
                                        .uv(plane.position.x, plane.position.z)
                                        .orientation(plane.yaw, plane.pitch, plane.roll)
                                        .heading(plane.heading),
                                    &mut known_plane_coords,
                                )),
                                Property::AOA(plane.aoa),
                            ],
                        };

                        if (carrier.time - plane.time).abs() < 0.01 {
                            write_acmi!(Record::Frame(carrier.time));
                            write_acmi!(carrier_update);
                            write_acmi!(plane_update);
                        } else if carrier.time < plane.time {
                            write_acmi!(Record::Frame(carrier.time));
                            write_acmi!(carrier_update);
                            write_acmi!(Record::Frame(plane.time));
                            write_acmi!(plane_update);
                        } else {
                            write_acmi!(Record::Frame(plane.time));
                            write_acmi!(plane_update);
                            write_acmi!(Record::Frame(carrier.time));
                            write_acmi!(carrier_update);
                        }
                    }

                    last_carrier_lat = carrier.lat;
                    last_carrier_lon = carrier.lon;
                    last_carrier_alt = carrier.alt;

                    lowest_altitude = lowest_altitude.min(plane.alt);

                    let keep_tracking = datums.next_sample(&sample, hook_state);

                    // Establish the wind reference for AoA correction exactly once, as soon as
                    // the aircraft enters the groove (see docs/GRADING_REFERENCE.md, "AoA").
                    // DCS wind is deterministic and altitude-dependent, not time-varying, so two
                    // readings taken now -- at the aircraft's current altitude and near the deck
                    // -- are enough for the rest of the recovery; skipped in --positions-only,
                    // which never queries output-only DCS metadata.
                    if !wind_reference_queried && !params.positions_only && datums.entered_groove()
                    {
                        wind_reference_queried = true;
                        let mut atmo = crate::client::AtmosphereClient::new(params.ch.clone());
                        // The low probe (near the deck) is the one seen hitting DCS's sentinel in
                        // practice; each query is individually retried once on that sentinel so a
                        // live anomaly can still be traced to a specific probe/attempt below.
                        let high = query_wind_with_sentinel_retry(
                            &mut atmo, plane.lat, plane.lon, plane.alt, "high",
                        )
                        .await;
                        let low = query_wind_with_sentinel_retry(
                            &mut atmo,
                            plane.lat,
                            plane.lon,
                            carrier.alt,
                            "low",
                        )
                        .await;
                        match (high, low) {
                            (Some((high_dir, high_speed)), Some((low_dir, low_speed))) => {
                                let high_is_sentinel =
                                    crate::track::is_wind_sentinel((high_dir, high_speed));
                                let low_is_sentinel =
                                    crate::track::is_wind_sentinel((low_dir, low_speed));
                                // Still sentinel after the bounded retry above: reuse the high
                                // probe's wind vector for the low-altitude point too (DCS wind is
                                // altitude-dependent but static over time, and the high probe has
                                // stayed coherent in every corpus report reviewed so far) rather
                                // than feed a known-bad zero into the AoA correction. The raw
                                // `low` reading is still recorded as-is in `WindProbe` below.
                                let use_high_for_low = low_is_sentinel && !high_is_sentinel;
                                let (reference_low_dir, reference_low_speed) = if use_high_for_low {
                                    (high_dir, high_speed)
                                } else {
                                    (low_dir, low_speed)
                                };
                                datums.set_wind_reference(
                                    plane.alt,
                                    crate::track::wind_velocity_vector(high_dir, high_speed),
                                    carrier.alt,
                                    crate::track::wind_velocity_vector(
                                        reference_low_dir,
                                        reference_low_speed,
                                    ),
                                );
                                datums.set_wind_reference_probes(
                                    crate::track::WindProbe {
                                        alt_m: plane.alt,
                                        heading_deg: high_dir,
                                        speed_mps: high_speed,
                                    },
                                    crate::track::WindProbe {
                                        alt_m: carrier.alt,
                                        heading_deg: low_dir,
                                        speed_mps: low_speed,
                                    },
                                    use_high_for_low,
                                );
                            }
                            (high, low) => {
                                tracing::warn!(
                                    ?high,
                                    ?low,
                                    "failed to establish a wind reference for AoA correction; \
                                     the raw geometric approximation will be used for this recovery"
                                );
                            }
                        }
                    }

                    if let Some(hook_rx) = hook_rx.as_mut() {
                        drain_hook_samples(
                            hook_rx,
                            &mut datums,
                            plane.time,
                            params.hook_sampling.frequency_hz,
                        );
                    }
                    if !keep_tracking {
                        break 'recording;
                    }
                }
                // Evaluated once per tick, outside the per-sample loop: a unit that vanished
                // right after touchdown only ever produces empty or invalid buffered batches, and
                // the cutoff used to sit inside that loop where it was never reached (review
                // finding F02, buffered half).
                if post_touchdown_window_elapsed(track_stopped) {
                    tracing::debug!("post-touchdown window elapsed; finalising the recording");
                    break 'recording;
                }
                if sample_count == 0
                    && last_telemetry_success.elapsed() >= position_collector.recovery_watchdog()
                {
                    tracing::warn!(
                        silent_for = ?last_telemetry_success.elapsed(),
                        "active telemetry watchdog expired without buffered snapshots"
                    );
                    datums.mark_telemetry_gap(TelemetryInvalidReason::TelemetryGap);
                    break 'recording;
                }
            }

            Either::Right(super::event_hub::SessionEventMessage::StreamUnavailable(detail)) => {
                tracing::warn!(%detail, "mission event stream unavailable during recovery");
                event_correlator.stream_unavailable(&mut datums, detail);
            }

            Either::Right(super::event_hub::SessionEventMessage::StreamAvailable) => {
                event_correlator.stream_available(&mut datums);
            }

            // DCS landing grade
            Either::Right(super::event_hub::SessionEventMessage::Event {
                time_dcs, event, ..
            }) => match (time_dcs, *event) {
                (
                    time,
                    Event::LandingQualityMark(LandingQualityMarkEvent {
                        initiator:
                            Some(Initiator {
                                initiator: Some(initiator::Initiator::Unit(plane)),
                            }),
                        place:
                            Some(Airbase {
                                unit: Some(carrier),
                                ..
                            }),
                        comment,
                    }),
                ) if event_correlator.accepts_pair(plane.id, carrier.id) => {
                    tracing::info!(%comment, "landing quality mark event");
                    event_correlator.landing_quality_mark(&mut datums, time, comment.clone());
                    write_acmi!(Record::Frame(time));

                    let carrier = Transform::from((
                        time,
                        carrier.position.unwrap_or_default(),
                        carrier.orientation.unwrap_or_default(),
                        carrier.velocity.unwrap_or_default(),
                    ));
                    write_acmi!(Update {
                        id: 1,
                        props: vec![Property::T(remove_unchanged(
                            Coords::default()
                                .position(carrier.lat - lat_ref, carrier.lon - lon_ref, carrier.alt)
                                .uv(carrier.position.x, carrier.position.z)
                                .orientation(carrier.yaw, carrier.pitch, carrier.roll)
                                .heading(carrier.heading),
                            &mut known_carrier_coords,
                        ))],
                    });

                    let plane = Transform::from((
                        time,
                        plane.position.unwrap_or_default(),
                        plane.orientation.unwrap_or_default(),
                        plane.velocity.unwrap_or_default(),
                    ));
                    write_acmi!(Update {
                        id: 2,
                        props: vec![
                            Property::T(remove_unchanged(
                                Coords::default()
                                    .position(plane.lat - lat_ref, plane.lon - lon_ref, plane.alt)
                                    .uv(plane.position.x, plane.position.z)
                                    .orientation(plane.yaw, plane.pitch, plane.roll)
                                    .heading(plane.heading),
                                &mut known_plane_coords,
                            )),
                            Property::AOA(plane.aoa),
                        ],
                    });

                    write_acmi!(record::Event {
                        kind: record::EventKind::Message,
                        params: vec!["2".to_string(), "1".to_string()],
                        text: Some(comment),
                    });
                }

                // Generic DCS land event. It is correlated with exact unit IDs
                // and touchdown geometry, then deduplicated against RunwayTouch.
                (
                    time,
                    Event::Land(LandEvent {
                        initiator:
                            Some(Initiator {
                                initiator: Some(initiator::Initiator::Unit(plane)),
                            }),
                        place:
                            Some(Airbase {
                                unit: Some(carrier),
                                ..
                            }),
                    }),
                ) if event_correlator.accepts_pair(plane.id, carrier.id) => {
                    if let Some(hook_rx) = hook_rx.as_mut() {
                        drain_hook_samples(
                            hook_rx,
                            &mut datums,
                            time,
                            params.hook_sampling.frequency_hz,
                        );
                    }
                    let correlation =
                        event_correlator.touchdown(&mut datums, "land", time, carrier, plane);
                    if correlation.accepted {
                        track_stopped = Some(Instant::now());
                    }
                }

                // DCS runway-touch event
                (
                    time,
                    Event::RunwayTouch(RunwayTouchEvent {
                        initiator:
                            Some(Initiator {
                                initiator: Some(initiator::Initiator::Unit(plane)),
                            }),
                        place:
                            Some(Airbase {
                                unit: Some(carrier),
                                ..
                            }),
                    }),
                ) if event_correlator.accepts_pair(plane.id, carrier.id) => {
                    tracing::info!("land event");

                    if let Some(hook_rx) = hook_rx.as_mut() {
                        drain_hook_samples(
                            hook_rx,
                            &mut datums,
                            time,
                            params.hook_sampling.frequency_hz,
                        );
                    }
                    let correlation = event_correlator.touchdown(
                        &mut datums,
                        "runway_touch",
                        time,
                        carrier,
                        plane,
                    );
                    let Some((carrier, plane)) = correlation.carrier.zip(correlation.plane) else {
                        continue;
                    };

                    write_acmi!(Record::Frame(time));
                    write_acmi!(Update {
                        id: 1,
                        props: vec![Property::T(remove_unchanged(
                            Coords::default()
                                .position(carrier.lat - lat_ref, carrier.lon - lon_ref, carrier.alt)
                                .uv(carrier.position.x, carrier.position.z)
                                .orientation(carrier.yaw, carrier.pitch, carrier.roll)
                                .heading(carrier.heading),
                            &mut known_carrier_coords,
                        ))],
                    });

                    write_acmi!(Update {
                        id: 2,
                        props: vec![
                            Property::T(remove_unchanged(
                                Coords::default()
                                    .position(plane.lat - lat_ref, plane.lon - lon_ref, plane.alt)
                                    .uv(plane.position.x, plane.position.z)
                                    .orientation(plane.yaw, plane.pitch, plane.roll)
                                    .heading(plane.heading),
                                &mut known_plane_coords,
                            )),
                            Property::AOA(plane.aoa),
                        ],
                    });

                    write_acmi!(record::Event {
                        kind: record::EventKind::Landed,
                        params: vec!["2".to_string(), "1".to_string()],
                        text: None,
                    });

                    // Do not feed the possibly late event transform back into
                    // the continuous trajectory. It used to manufacture a
                    // near-event wire-4 crossing and could also make the last
                    // post-touch hook value look like pre-touch evidence.
                    // don't stop right away, track a couple of more seconds
                    if correlation.accepted {
                        track_stopped = Some(Instant::now());
                    }
                }

                // Any event indicating that either the carrier or plane do not exist anymore
                (
                    _,
                    Event::Crash(CrashEvent {
                        initiator:
                            Some(Initiator {
                                initiator: Some(initiator::Initiator::Unit(unit)),
                            }),
                    })
                    | Event::Dead(DeadEvent {
                        initiator:
                            Some(Initiator {
                                initiator: Some(initiator::Initiator::Unit(unit)),
                            }),
                    })
                    | Event::PlayerLeaveUnit(PlayerLeaveUnitEvent {
                        initiator:
                            Some(Initiator {
                                initiator: Some(initiator::Initiator::Unit(unit)),
                            }),
                    })
                    | Event::UnitLost(UnitLostEvent {
                        initiator:
                            Some(Initiator {
                                initiator: Some(initiator::Initiator::Unit(unit)),
                            }),
                    }),
                ) if event_correlator.is_tracked_unit(unit.id) => {
                    tracing::info!("stop (either carrier or plane despawned)");
                    break 'recording;
                }

                _ => {}
            },
        }
    }

    if params.shutdown.signal().now_or_never().is_some() {
        tracing::info!("shutdown requested; finalising the recording with the evidence so far");
        datums.record_event(
            "shutdown",
            datums.last_observed_time_dcs().unwrap_or_default(),
            false,
            "recording_finalised_on_shutdown",
        );
    }

    if let Err(status) = position_collector.stop().await {
        tracing::warn!(
            ?status,
            "failed to stop source-buffered recovery telemetry cleanly"
        );
    }

    // Geometry can close a pass just before DCS publishes its LQM/contact event. Keep a short,
    // bounded grace period, then replay only newer entries from the session journal. This closes
    // the hand-off race without holding the next track open or accepting an event from an older
    // attempt (the journal is additionally filtered by attempt DCS time and exact unit IDs).
    if !params.positions_only && datums.has_recognisable_approach() {
        tokio::time::sleep(Duration::from_secs(2)).await;
        if let Some(event_hub) = params.event_hub.as_ref() {
            for event in event_hub.snapshot_since(attempt_started_at_dcs) {
                if event.sequence <= event_progress.load(Ordering::Relaxed) {
                    continue;
                }
                correlate_late_event(&mut event_correlator, &mut datums, event);
            }
            let (available, detail) = event_hub.status();
            if available {
                event_correlator.stream_available(&mut datums);
            } else {
                event_correlator.stream_unavailable(
                    &mut datums,
                    detail.unwrap_or_else(|| "event_stream_unavailable".to_string()),
                );
            }
        }
    }

    // If the plane was never below 100 m MSL, discard as a non-attempt.
    // Waveoffs and bolters still pass this check since they require being in the groove.
    if lowest_altitude > 100.0 {
        // Promoted from DEBUG to INFO (tasking-roadmap.md P2, "tentatives d'approche avorties
        // avant le groove, invisibles hors logs DEBUG"): this is the abandon path a false-start
        // detection takes, and its cost (open stream time, hook sampler start/stop) was only
        // visible in DEBUG before.
        tracing::info!(
            elapsed_secs = recording_started.elapsed().as_secs_f64(),
            lowest_altitude_m = lowest_altitude,
            "discard as plane was never below 100m MSL"
        );
        return Ok(());
    }

    if warning_count > 0 {
        tracing::warn!(
            warning_count,
            warning_max_gap_ms,
            "telemetry quality degraded during final reporting window"
        );
    }

    if let Some(writer) = recording.take() {
        let _ = writer.into_inner();
    }
    drop(recording);
    let data = if params.record_acmi {
        acmi.into_inner()
    } else {
        Vec::new()
    };
    let acquisition_source = position_collector.acquisition_source();
    let buffered_diagnostics = position_collector.buffered_diagnostics().cloned();
    datums.set_position_collector_metrics(position_collector.metrics());
    let track = std::sync::Arc::new(datums.finish());
    let event_correlation = event_correlator.summary(&track.grading);

    // Discard if no recognisable outcome was established (e.g. plane flew through the zone
    // without ever entering the groove).
    if track.grading == Grading::Unknown {
        // Same rationale as the 100m-MSL discard above: this is a false-start abandon, promoted
        // to INFO for the same reason (tasking-roadmap.md P2).
        tracing::info!(
            elapsed_secs = recording_started.elapsed().as_secs_f64(),
            lowest_altitude_m = lowest_altitude,
            "discard: no recovery outcome (Unknown grading)"
        );
        return Ok(());
    }

    let recovery_time_ms = track
        .touchdown_time_dcs
        .or_else(|| track.datums.last().map(|datum| datum.time))
        .map(|time| (time * 1_000.0).round() as i64)
        .unwrap_or_default();
    let recovery_id = recovery_id(
        params.session_id,
        params.generation,
        params.plane_id,
        params.carrier_id,
        recovery_time_ms,
    );
    let filename = format!(
        "LSO-{}-{}-{}",
        now.format(&FILENAME_DATETIME_FORMAT).unwrap_or_default(),
        if safe_pilot_name.is_empty() {
            "unknown"
        } else {
            safe_pilot_name.as_str()
        },
        recovery_id,
    );
    // Query in-mission date/time from the DCS scenario clock (non-fatal).
    let mission_datetime: String = if params.positions_only {
        String::new()
    } else {
        match mission
            .as_mut()
            .expect("mission client enabled outside positions-only")
            .get_scenario_current_time()
            .await
        {
            Ok(dt) => dt,
            Err(err) => {
                tracing::warn!(?err, "failed to query in-mission datetime");
                String::new()
            }
        }
    };

    let outcome = recovery_outcome(&track.grading, track.carrier_info.is_vstol());
    // Pilot-facing surfaces (Discord, PNG chart, SQLite/greenie-board log) use a simplified
    // headline that never contradicts what the pilot saw in DCS: see
    // Grading::pilot_facing_outcome for the rationale. The full `outcome` string above (which can
    // show a diverging Rust estimate) is reserved for the JSON report.
    let outcome_headline = track
        .grading
        .pilot_facing_outcome(track.carrier_info.is_vstol());
    let (wire_estimated, wire_dcs) = match track.grading {
        Grading::Recovered {
            cable,
            cable_estimated,
        } => (cable_estimated, cable),
        Grading::TouchAndGo { cable_estimated } => (cable_estimated, None),
        _ => (None, None),
    };
    let wire_divergent = matches!((wire_estimated, wire_dcs), (Some(a), Some(b)) if a != b);
    let wire_primary = match (wire_dcs, wire_estimated) {
        (Some(dcs), Some(estimated)) if dcs == estimated => "agreement",
        (Some(_), _) => "dcs_lqm",
        // A hook-up pass: the wire the hook would have caught, never an arrestment.
        (None, Some(_)) if matches!(track.grading, Grading::TouchAndGo { .. }) => {
            "rust_hypothetical"
        }
        (None, Some(_)) => "rust_estimated",
        (None, None) => "none",
    };
    let event_outcome_unavailable = matches!(
        &event_correlation.stream_status,
        super::event_correlator::EventStreamStatus::Unavailable
    ) && !event_correlation.outcome_confirmed;
    let confidence = match track.telemetry_quality.completeness {
        _ if event_outcome_unavailable => "insufficient",
        crate::track::Completeness::Complete
            if wire_estimated == wire_dcs && wire_dcs.is_some() =>
        {
            "high"
        }
        crate::track::Completeness::Complete => "medium",
        _ => "insufficient",
    };
    let cause = match track.telemetry_quality.completeness {
        cause @ (crate::track::Completeness::InsufficientGates
        | crate::track::Completeness::TelemetryGap
        | crate::track::Completeness::InvalidTelemetry
        | crate::track::Completeness::UnconfirmedArrest
        | crate::track::Completeness::BufferLimit) => completeness_cause(cause),
        crate::track::Completeness::Complete => match track.grading {
            Grading::WaveoffUnknown => "go_around_initiator_unknown",
            Grading::ApproachOnly => "approach_only_outcome_unknown",
            Grading::Bolter => "deck_crossing_without_arrest",
            Grading::TouchAndGo { .. } => "hook_up_near_deck",
            Grading::Recovered { .. } => match track.arrest_evidence {
                "hook_transient" => "hook_transient_arrest_without_dcs_wire",
                "kinematic" => "kinematic_arrest_without_wire",
                _ => "correlated_touchdown",
            },
            Grading::Unknown => "unknown",
        },
    };
    let mut secondary_causes = track
        .telemetry_quality
        .unavailability_causes
        .iter()
        .copied()
        .filter(|secondary| *secondary != track.telemetry_quality.completeness)
        .map(completeness_cause)
        .collect::<Vec<_>>();
    secondary_causes.extend(
        track
            .telemetry_quality
            .diagnostics
            .iter()
            .map(|cause| match cause {
                crate::track::DiagnosticCause::PatternHistoryTruncated => {
                    "pattern_history_truncated"
                }
                crate::track::DiagnosticCause::HookHistoryTruncated => "hook_history_truncated",
                crate::track::DiagnosticCause::EventHistoryTruncated => "event_history_truncated",
                crate::track::DiagnosticCause::EventStreamUnavailable => "event_stream_unavailable",
            }),
    );
    // Availability describes whether Rust had enough evidence to apply the
    // rules, independently from whether the resulting performance earns points.
    let grading_availability =
        if track.telemetry_quality.completeness != crate::track::Completeness::Complete {
            "unavailable_technical"
        } else if matches!(track.grading, Grading::ApproachOnly) {
            "available_approach_only"
        } else if event_outcome_unavailable {
            "unavailable_event_outcome"
        } else {
            "available"
        };
    let aircraft_id = crate::data::get_aircraft_id(params.plane_type);
    let completed_at = OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_default();

    let pipeline = super::report_pipeline::ReportPipeline::new(params.out_dir, &filename);
    let Some(mut recovery_claim) = pipeline.claim_recovery(&recovery_id) else {
        tracing::info!(
            recovery_id,
            "recovery already owned by another producer in this session/generation"
        );
        return Ok(());
    };
    let json_path = pipeline.json_path();

    // Query wind at the carrier's last known position, once per recovery (with one bounded retry
    // on DCS's known sentinel — see `query_wind_with_sentinel_retry`). Purely contextual: see
    // docs/GRADING_REFERENCE.md, "Wind" — it never affects pass_grade/grade_points. Skipped in
    // `--positions-only`, which documents that it never queries output-only DCS metadata.
    let (wind_mps, wind_reading_is_groove_entry_fallback): (Option<(u16, f32)>, bool) = if params
        .positions_only
    {
        (None, false)
    } else {
        let mut atmo = crate::client::AtmosphereClient::new(params.ch.clone());
        match query_wind_with_sentinel_retry(
            &mut atmo,
            last_carrier_lat,
            last_carrier_lon,
            last_carrier_alt,
            "report_time",
        )
        .await
        {
            Some(w) if crate::track::is_wind_sentinel(w) => {
                // Still the sentinel after the retry: fall back to this recovery's
                // groove-entry high probe (queried at the aircraft's own altitude, stayed
                // coherent in every corpus report reviewed so far) instead of surfacing a
                // known-bad zero, when one was established.
                match track.wind_reference_probes.map(|p| p.high) {
                    Some(high)
                        if !crate::track::is_wind_sentinel((high.heading_deg, high.speed_mps)) =>
                    {
                        (Some((high.heading_deg, high.speed_mps)), true)
                    }
                    _ => (Some(w), false),
                }
            }
            other => (other, false),
        }
    };

    // `spot` is retained as the legacy phase-1 alias. New consumers must use the
    // independent intended/nearest fields below.
    let spot_label = track.intended_spot;
    let observed_from_distance_m = track
        .trajectory_deviations
        .iter()
        .map(|sample| sample.distance_m)
        .chain(track.datums.iter().map(|sample| sample.x))
        .filter(|distance| distance.is_finite() && *distance >= 0.0)
        .reduce(f64::max);
    let has_approach_evidence = !track.trajectory_deviations.is_empty()
        || track.gate_deviations.at_three_quarter_nm.is_some()
        || track.gate_deviations.at_half_nm.is_some()
        || track.gate_deviations.at_quarter_nm.is_some();
    let has_outcome = !matches!(track.grading, Grading::Unknown | Grading::ApproachOnly);
    let assessment_scope = if track.telemetry_quality.completeness
        == crate::track::Completeness::Complete
        && has_approach_evidence
    {
        AssessmentScope::Full
    } else if has_approach_evidence {
        AssessmentScope::Partial
    } else if has_outcome {
        AssessmentScope::OutcomeOnly
    } else {
        AssessmentScope::None
    };
    let missing_coverage = track
        .telemetry_quality
        .unavailability_causes
        .iter()
        .map(|cause| completeness_cause(*cause).to_string())
        .collect::<Vec<_>>();
    let fallback_source = if has_approach_evidence {
        FallbackSource::Project
    } else if track.dcs_grading.is_some() {
        FallbackSource::DcsLqm
    } else if has_outcome {
        FallbackSource::Geometry
    } else {
        FallbackSource::None
    };
    let pattern_rendering = crate::draw::pattern_branch_diagnostic(
        &track.pattern_datums,
        track.groove_entry.as_ref().map(|entry| entry.timestamp_dcs),
        track.touchdown_time_dcs,
    );
    let report = RecoveryReport {
        // Schema 9 is the first number of the merged lineage: it follows both
        // the astra-review reports (schema 8) and the refonte reports (schema 3)
        // so a consumer can never confuse the three field layouts.
        schema_version: 9,
        recovery_id: &recovery_id,
        pilot_name: &track.pilot_name,
        pilot_kind: params.pilot_kind,
        aircraft_type: params.plane_type,
        aircraft_id,
        carrier_id: params.carrier_id,
        carrier_name: params.carrier_name,
        carrier_type: params.carrier_type,
        recovery_mode: if track.carrier_info.is_vstol() {
            "vstol"
        } else {
            "arrested"
        },
        session_id: params.session_id,
        generation: params.generation,
        grading: &track.grading,
        approach_grade: track.approach_grade,
        pass_grade: track.pass_grade,
        grade_points: track.grade_points,
        spot: spot_label,
        intended_spot: track.intended_spot,
        actual_nearest_spot: track.actual_nearest_spot,
        spot_grade: track.spot_grade,
        spot_distance_m: track.spot_distance_m,
        spot_bonus_points: track.spot_grade.map(|g| g.bonus_points()),
        dcs_grading: track.dcs_grading.as_deref(),
        gate_deviations: &track.gate_deviations,
        trajectory_deviations: &track.trajectory_deviations,
        grading_episodes: &track.grading_episodes,
        wind_heading_deg: wind_mps.map(|(heading, _)| heading),
        wind_speed_mps: wind_mps.map(|(_, speed)| speed),
        wind_reading_is_groove_entry_fallback,
        wind_reference_established: track.wind_reference_established,
        wind_reference_probes: track.wind_reference_probes,
        datums: &track.datums,
        pattern_rendering,
        mission_datetime: &mission_datetime,
        recording_started_at: &recovery_timestamp,
        completed_at: &completed_at,
        touchdown_time_dcs: track.touchdown_time_dcs,
        groove_time_secs: track.groove_time_secs,
        groove_entry: track.groove_entry.as_ref(),
        lso_version: env!("CARGO_PKG_VERSION"),
        lso_commit: option_env!("GIT_COMMIT_HASH").unwrap_or("unknown"),
        lso_dirty: option_env!("GIT_DIRTY") == Some("true"),
        dcs_grpc_version: params.dcs_grpc_version,
        dcs_grpc_client_stubs: crate::client::DCS_GRPC_STUBS_VERSION,
        dcs_grpc_compatibility: params.dcs_grpc_compatibility,
        acquisition_source,
        recovery_telemetry: buffered_diagnostics.as_ref(),
        collection_profile: if params.positions_only {
            "positions_only"
        } else {
            "normal"
        },
        target_frequency_hz: if params.position_source == super::PositionSource::Buffered {
            20
        } else {
            10
        },
        missed_tick_behavior: "skip",
        detectors_suspended: params.suspend_detectors_during_recovery,
        detector_suspension_scope: if params.suspend_detectors_during_recovery {
            "same_aircraft"
        } else {
            "none"
        },
        baseline_manifest: &params.baseline_manifest,
        outcome: &outcome,
        cause,
        causes: ReportCauses {
            primary: cause,
            secondary: &secondary_causes,
        },
        confidence,
        grading_version: GRADING_VERSION,
        grading_source: GRADING_SOURCE,
        wire_estimated,
        wire_dcs,
        wire_divergent,
        wire_primary,
        wire_estimation: &track.wire_estimation,
        arrest_confirmation: &track.arrest_confirmation,
        hook_state: track.hook_state,
        arrest_evidence: track.arrest_evidence,
        grading_availability,
        assessment_scope,
        observed_from_distance_m,
        missing_coverage: &missing_coverage,
        points_eligible: track.grade_points.is_some(),
        fallback_source,
        telemetry_quality: &track.telemetry_quality,
        events: &track.events,
        spot_zone: &track.spot_zone,
        touchdown_horizontal_speed_mps: track.touchdown_horizontal_speed_mps,
        hook_observation: &track.hook_observation,
        event_correlation: &event_correlation,
    };
    let json = serde_json::to_vec_pretty(&report)?;
    match pipeline.publish_json(&json).await {
        Ok(super::report_pipeline::Publication::Created) => recovery_claim.commit(),
        Ok(super::report_pipeline::Publication::AlreadyExists) => {
            recovery_claim.commit();
            tracing::info!(
                recovery_id,
                path = %json_path.display(),
                "recovery already published; concurrent producer is not allowed to replace it"
            );
            return Ok(());
        }
        Err(source) => return Err(crate::error::Error::file_at(json_path, source)),
    }

    let acmi_path = if params.record_acmi {
        let path = pipeline.acmi_path();
        match pipeline.publish_acmi(&data).await {
            Ok(super::report_pipeline::Publication::Created) => Some(path),
            Ok(super::report_pipeline::Publication::AlreadyExists) => {
                tracing::error!(
                    recovery_id,
                    path = %path.display(),
                    "ACMI coherence conflict: an artifact already exists and was not replaced"
                );
                None
            }
            Err(err) => {
                tracing::error!(error = %err, path = %path.display(), "failed to persist ACMI output");
                None
            }
        }
    } else {
        None
    };

    let display_type = match aircraft_id {
        Some(2) => "F-14A/B",
        Some(3) => "F-14B(U)",
        _ => params.plane_info.name,
    };

    let completed = CompletedPass {
        timestamp: filename.clone(),
        pilot_name: track.pilot_name.clone(),
        pass_grade: track.pass_grade,
        grade_points: track.grade_points,
        wire: wire_dcs.or(wire_estimated),
        spot: spot_label.map(|s| s.to_string()),
        spot_grade: track.spot_grade,
        spot_distance_m: track.spot_distance_m,
        dcs_grading: track.dcs_grading.clone(),
        aircraft_type: display_type.to_string(),
        aircraft_id,
        map_name: map_name.clone(),
        outcome: outcome_headline,
        pilot_kind: params.pilot_kind,
        carrier_name: params.carrier_name.to_string(),
        carrier_type: params.carrier_type.to_string(),
        recovery_mode: if track.carrier_info.is_vstol() {
            "vstol".to_string()
        } else {
            "arrested".to_string()
        },
        session_id: params.session_id,
        generation: params.generation,
    };

    // Append to in-memory session greenie board log.
    if !params.positions_only {
        if let Ok(mut log) = params.session_log.lock() {
            if !log.iter().any(|pass| pass.timestamp == completed.timestamp) {
                log.push(completed.clone());
            }
        }
    }

    // Persist to SQLite database (non-fatal — a write failure must not abort the recovery).
    let db_inserted = if params.positions_only {
        None
    } else if let Some(db) = params.db.clone() {
        let db_path = db.path().to_path_buf();
        let entry = crate::db::DbPass {
            recovery_id: recovery_id.clone(),
            timestamp: completed.timestamp.clone(),
            pilot_name: completed.pilot_name.clone(),
            pilot_ucid: params.pilot_ucid.clone(),
            aircraft_id: completed.aircraft_id,
            pass_grade_label: completed.pass_grade.label().to_string(),
            wire: completed.wire,
            spot: completed.spot.clone(),
            spot_grade: completed.spot_grade.map(|g| g.label().to_string()),
            spot_distance_m: completed.spot_distance_m,
            intended_spot: track.intended_spot.map(str::to_string),
            actual_nearest_spot: track.actual_nearest_spot.map(str::to_string),
            distance_to_intended_spot_m: track.spot_distance_m,
            dcs_grading: completed.dcs_grading.clone(),
            aircraft_type: Some(completed.aircraft_type.clone()),
            map_name: if completed.map_name.is_empty() {
                None
            } else {
                Some(completed.map_name.clone())
            },
            grade_date: now_utc.format(&GRADE_DATE_FORMAT).unwrap_or_default(),
            grade_points: completed.grade_points,
            points_awarded: completed.grade_points.is_some(),
            mission_datetime: mission_datetime.clone(),
            outcome: completed.outcome.clone(),
            pilot_kind: format!("{:?}", completed.pilot_kind).to_lowercase(),
            carrier_id: params.carrier_id,
            carrier_name: completed.carrier_name.clone(),
            carrier_type: completed.carrier_type.clone(),
            recovery_mode: completed.recovery_mode.clone(),
            session_id: completed.session_id,
            generation: completed.generation,
            completeness: track.telemetry_quality.completeness.as_str().to_string(),
            max_sample_gap_ms: track.telemetry_quality.max_sample_gap_ms,
            max_scoring_sample_gap_ms: track.telemetry_quality.max_scoring_sample_gap_ms,
            max_skew_ms: track.telemetry_quality.max_skew_ms,
            telemetry_health: format!("{:?}", track.telemetry_quality.health).to_lowercase(),
            wire_estimated,
            wire_dcs,
            wire_divergent,
            confidence: confidence.to_string(),
            cause: cause.to_string(),
            secondary_causes_json: serde_json::to_string(&secondary_causes)
                .unwrap_or_else(|_| "[]".to_string()),
            grading_version: GRADING_VERSION.to_string(),
            wire_estimation_confidence: track.wire_estimation.confidence.to_string(),
            grading_availability: grading_availability.to_string(),
            assessment_scope: assessment_scope.as_str().to_string(),
            observed_from_distance_m,
            missing_coverage_json: serde_json::to_string(&missing_coverage)
                .unwrap_or_else(|_| "[]".to_string()),
            points_eligible: track.grade_points.is_some(),
            fallback_source: fallback_source.as_str().to_string(),
            arrest_evidence: track.arrest_evidence.to_string(),
            hook_state: track.hook_state.as_str().to_string(),
        };
        match tokio::task::spawn_blocking(move || db.insert(&entry)).await {
            Ok(Ok(inserted)) => Some(inserted),
            Ok(Err(err)) => {
                tracing::error!(error = %err, path = %db_path.display(), "failed to persist pass to database");
                None
            }
            Err(err) => {
                tracing::error!(?err, "database task panicked");
                None
            }
        }
    } else {
        tracing::error!("SQLite unavailable outside positions-only mode");
        None
    };

    let rendered = if params.positions_only {
        None
    } else {
        let render_track = track.clone();
        let render_pipeline = pipeline.clone();
        match tokio::task::spawn_blocking(move || {
            let started = Instant::now();
            let rendered = render_pipeline.render_and_publish(&render_track)?;
            crate::metrics::RUNTIME_METRICS
                .observe_render(started.elapsed().as_micros().min(u64::MAX as u128) as u64);
            Ok::<_, crate::error::Error>(rendered)
        })
        .await
        {
            Ok(Ok(paths)) => Some(paths),
            Ok(Err(err)) => {
                tracing::error!(error = %err, error_chain = ?err, "PNG rendering failed after persistence");
                None
            }
            Err(err) => {
                tracing::error!(?err, "PNG rendering task panicked after persistence");
                None
            }
        }
    };

    if let (Some(discord_webhook), Some((chart_path, pattern_chart_path)), Some(true)) = (
        params.discord_webhook.as_deref(),
        rendered.as_ref(),
        db_inserted,
    ) {
        let publish_result: Result<(), crate::error::Error> = async {
            let http = Http::new("token");
            let webhook = http.get_webhook_from_url(discord_webhook).await?;

            let mut embed = CreateEmbed::new()
                .field("Aircraft", params.plane_info.name, false)
                .field(
                    "Map",
                    if map_name.is_empty() {
                        "-"
                    } else {
                        map_name.as_str()
                    },
                    false,
                )
                .field("Date / Time (UTC)", recovery_timestamp.as_str(), false);
            if !mission_datetime.is_empty() {
                embed = embed.field("Mission Date/Time", mission_datetime.as_str(), false);
            }
            embed = embed
                .field(
                    "Pilot",
                    params
                        .users
                        .get(track.pilot_name.as_str())
                        .map(|id| Cow::Owned(Mention::from(UserId::new(*id)).to_string()))
                        .unwrap_or(Cow::Borrowed(track.pilot_name.as_str())),
                    true,
                )
                .field(
                    "Grade",
                    match track.grade_points {
                        Some(points) if track.carrier_info.is_vstol() => {
                            format!("{} ({points:.2} pts)", track.pass_grade.label())
                        }
                        Some(points) => format!("{} ({points:.1} pts)", track.pass_grade.label()),
                        None => format!("{} (no points)", track.pass_grade.label()),
                    },
                    true,
                )
                .field("Outcome", completed.outcome.clone(), true)
                .field(
                    "Gates (GS / LU)",
                    {
                        let fmt = |g: Option<&crate::track::GateDatum>,
                                   q: &crate::track::GateQuality|
                         -> String {
                            match g {
                                Some(d) => format!(
                                    "{:+.1}° / {:+.1}°",
                                    d.gs_deviation_deg, d.lineup_deg
                                ),
                                None
                                    if q.coverage_source
                                        == Some("continuous_trajectory_bracket") =>
                                {
                                    "covered by continuous trajectory".to_string()
                                }
                                None => "-".to_string(),
                            }
                        };
                        Cow::Owned(format!(
                            "3/4nm: {}\n1/2nm: {}\n1/4nm: {}",
                            fmt(
                                track.gate_deviations.at_three_quarter_nm.as_ref(),
                                &track.gate_deviations.three_quarter_quality
                            ),
                            fmt(
                                track.gate_deviations.at_half_nm.as_ref(),
                                &track.gate_deviations.half_quality
                            ),
                            fmt(
                                track.gate_deviations.at_quarter_nm.as_ref(),
                                &track.gate_deviations.quarter_quality
                            ),
                        ))
                    },
                    false,
                );

            // Succinct, transparent explanation of the grade -- or, when grading itself was
            // unavailable, why (that takes priority: `grade_reason` still describes whatever
            // amplitude/etc. rule the code path reached internally, but `pass_grade` was
            // overridden to `Incomplete` regardless, so showing it here would be misleading).
            let why_this_grade = match assessment_scope {
                AssessmentScope::Full => track.grade_reason.clone(),
                AssessmentScope::Partial => format!(
                    "Partial project assessment (no points): {} Missing coverage: {}.",
                    track.grade_reason,
                    missing_coverage.join(", ")
                ),
                AssessmentScope::OutcomeOnly if track.dcs_grading.is_some() => format!(
                    "Outcome only; raw DCS LQM fallback: {}. No project points.",
                    track.dcs_grading.as_deref().unwrap_or_default()
                ),
                AssessmentScope::OutcomeOnly =>
                    "Outcome only; approach coverage is insufficient. No project points.".to_string(),
                AssessmentScope::None => format!(
                    "Grading unavailable: {:?}. This is a measurement limitation, not a pilot failure.",
                    track.telemetry_quality.completeness
                ),
            };
            embed = embed.field("Why This Grade", why_this_grade, false);

            if track.carrier_info.is_vstol() {
                if let (Some(spot_grade), Some(distance_m)) =
                    (track.spot_grade, track.spot_distance_m)
                {
                    embed = embed.field(
                        "Spot 7.5",
                        format!(
                            "{} — {:.2} m — +{:.2} pt",
                            spot_grade.label(),
                            distance_m,
                            spot_grade.bonus_points()
                        ),
                        false,
                    );
                }
            }

            // LSO notation and plain-English notes from DCS grading string. DCS never emits this
            // for a touch-and-go (only for an arrested pass), so fall back to a plain-language
            // summary of our own measured deviations -- explicitly labelled as such, never
            // presented as a DCS/NATOPS comment (see `describe_measured_deviations`, src/grading.rs).
            if let Some(ref notation) = track.dcs_grading {
                embed = embed.field("LSO Notation", notation.as_str(), false);
                let notes = crate::lso_notation::to_english(notation);
                if !notes.is_empty() {
                    embed = embed.field("LSO Notes", notes, false);
                }
            } else {
                let notes = crate::grading::describe_measured_deviations(
                    &track.gate_deviations,
                    &track.trajectory_deviations,
                );
                if !notes.is_empty() {
                    embed = embed.field("LSO Notes (measured by LSO, not a DCS comment)", notes, false);
                }
            }

            // Wind and groove time — Discord-only fields. The JSON report already carries the
            // same wind reading in raw m/s (`wind_heading_deg`/`wind_speed_mps`); knots here are
            // purely a display convenience for this embed.
            if let Some((dir, speed_mps)) = wind_mps {
                const MPS_TO_KNOTS: f32 = 1.944;
                embed = embed.field(
                    "Wind",
                    format!("{}° at {:.0} kts", dir, speed_mps * MPS_TO_KNOTS),
                    true,
                );
            }
            if let Some(secs) = track.groove_time_secs {
                embed = embed.field("Groove Time", format!("{:.1} s", secs), true);
            }

            let mut execute = ExecuteWebhook::new()
                .embeds(vec![embed])
                .add_file(CreateAttachment::path(&chart_path).await?)
                .add_file(CreateAttachment::path(&pattern_chart_path).await?);
            if let Some(ref path) = acmi_path {
                execute = execute.add_file(CreateAttachment::path(path).await?);
            }
            webhook.execute(&http, false, execute).await?;
            Ok(())
        }
        .await;
        if let Err(err) = publish_result {
            tracing::error!(error = %err, error_chain = ?err, "Discord publication failed after local persistence");
        }
    } else if params.discord_webhook.is_some() && db_inserted != Some(true) {
        tracing::warn!("Discord publication skipped because this recovery was not newly persisted");
    }

    Ok(())
}

fn recovery_id(
    session_id: i64,
    generation: u64,
    plane_id: u32,
    carrier_id: u32,
    dcs_time_ms: i64,
) -> String {
    format!("s{session_id}-g{generation}-p{plane_id}-c{carrier_id}-t{dcs_time_ms}")
}

async fn create_initial_update(
    client: &mut UnitClient,
    id: u64,
    unit_name: &str,
) -> crate::client::GrpcResult<Update> {
    let unit = client.get_unit(unit_name).await?;
    let attrs = client.get_descriptor(unit_name).await?;

    let coalition = Coalition::try_from(unit.coalition).unwrap_or(Coalition::Neutral);
    let mut props = vec![
        Property::Type(tags(attrs)),
        Property::Name(unit.r#type.unwrap_or_default()),
        Property::Group(unit.group.unwrap_or_default().name),
        Property::Color(color(coalition)),
    ];
    if let Some(player_name) = &unit.player_name {
        props.push(Property::Pilot(player_name.to_string()))
    }

    Ok(Update { id, props })
}

fn tags<I: AsRef<str>>(attrs: impl IntoIterator<Item = I>) -> HashSet<Tag> {
    let mut tags = HashSet::with_capacity(2);
    for attr in attrs.into_iter() {
        match attr.as_ref() {
            "Ships" => {
                tags.insert(Tag::Sea);
                tags.insert(Tag::Watercraft);
            }
            "AircraftCarrier" => {
                tags.insert(Tag::AircraftCarrier);
            }
            "Air" => {
                tags.insert(Tag::Air);
            }
            "Planes" => {
                tags.insert(Tag::FixedWing);
            }
            _ => {}
        }
    }
    tags
}

fn color(coalition: Coalition) -> Color {
    match coalition {
        Coalition::All | Coalition::Neutral => Color::Grey,
        Coalition::Red => Color::Red,
        Coalition::Blue => Color::Blue,
    }
}

fn remove_unchanged(mut coords: Coords, known: &mut Option<Coords>) -> Coords {
    if let Some(known) = known {
        if changed_precision(coords.longitude, known.longitude, 0.0000001) {
            known.longitude = coords.longitude;
        } else {
            coords.longitude = None;
        }

        if changed_precision(coords.latitude, known.latitude, 0.0000001) {
            known.latitude = coords.latitude;
        } else {
            coords.latitude = None;
        }

        if changed_precision(coords.altitude, known.altitude, 0.01) {
            known.altitude = coords.altitude;
        } else {
            coords.altitude = None;
        }

        if changed_precision(coords.u, known.u, 0.01) {
            known.u = coords.u;
        } else {
            coords.u = None;
        }

        if changed_precision(coords.v, known.v, 0.01) {
            known.v = coords.v;
        } else {
            coords.v = None;
        }

        if changed_precision(coords.roll, known.roll, 0.1) {
            known.roll = coords.roll;
        } else {
            coords.roll = None;
        }

        if changed_precision(coords.pitch, known.pitch, 0.1) {
            known.pitch = coords.pitch;
        } else {
            coords.pitch = None;
        }

        if changed_precision(coords.yaw, known.yaw, 0.1) {
            known.yaw = coords.yaw;
        } else {
            coords.yaw = None;
        }

        if changed_precision(coords.heading, known.heading, 0.1) {
            known.heading = coords.heading;
        } else {
            coords.heading = None;
        }
    } else {
        *known = Some(coords.clone());
    }

    coords
}

fn changed_precision(a: Option<f64>, b: Option<f64>, theta: f64) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => (a - b).abs() >= theta,
        (None, None) => false,
        _ => true,
    }
}

/// How long the recorder keeps sampling after an accepted touchdown so the hook transient can
/// recover and the deck kinematics can settle (see `Track`'s `POST_ARREST_EVIDENCE_WINDOW_S` and
/// the replay's `REPLAY_POST_LANDING_S`, both 10 s).
const POST_TOUCHDOWN_WINDOW: Duration = Duration::from_secs(10);

/// Whether an accepted touchdown happened more than `POST_TOUCHDOWN_WINDOW` ago. Pure, so it can
/// be evaluated on every tick regardless of whether the tick carried any sample.
fn post_touchdown_window_elapsed(track_stopped: Option<Instant>) -> bool {
    track_stopped.is_some_and(|stopped| stopped.elapsed() > POST_TOUCHDOWN_WINDOW)
}

#[cfg(test)]
mod tests {
    use super::{
        correlate_late_event, drain_hook_samples, grpc_code_name, post_touchdown_window_elapsed,
        recovery_id, recovery_outcome, HookPoll, POST_TOUCHDOWN_WINDOW,
    };
    use std::time::Instant;

    #[test]
    fn post_touchdown_window_is_evaluated_without_samples() {
        assert!(!post_touchdown_window_elapsed(None));
        assert!(!post_touchdown_window_elapsed(Some(Instant::now())));
        let long_ago = Instant::now()
            .checked_sub(POST_TOUCHDOWN_WINDOW + std::time::Duration::from_secs(1))
            .expect("instant arithmetic");
        assert!(post_touchdown_window_elapsed(Some(long_ago)));
    }
    use crate::data::{AirplaneInfo, CarrierInfo};
    use crate::tasks::event_correlator::transform_from_event_unit;
    use crate::track::{Grading, HookSampleStatus, Track};
    use stubs::common::v0::{initiator, Airbase, Initiator, Orientation, Position, Unit};
    use stubs::mission::v0::stream_events_response::{Event, LandingQualityMarkEvent};

    #[test]
    fn late_journal_lqm_is_correlated_to_the_current_pair() {
        let carrier_info = CarrierInfo::by_type("CVN_71").unwrap();
        let plane_info = AirplaneInfo::by_type("FA-18C_hornet").unwrap();
        let mut track = Track::new("pilot", carrier_info, plane_info);
        let mut correlator = crate::tasks::event_correlator::EventCorrelator::new(10, 20);
        correlate_late_event(
            &mut correlator,
            &mut track,
            crate::tasks::event_hub::SessionEvent {
                sequence: 7,
                time_dcs: 42.0,
                event: Event::LandingQualityMark(LandingQualityMarkEvent {
                    initiator: Some(Initiator {
                        initiator: Some(initiator::Initiator::Unit(Unit {
                            id: 10,
                            ..Unit::default()
                        })),
                    }),
                    place: Some(Airbase {
                        unit: Some(Unit {
                            id: 20,
                            ..Unit::default()
                        }),
                        ..Airbase::default()
                    }),
                    comment: "LSO: GRADE:OK : WIRE# 3 [BC]".to_string(),
                }),
            },
        );

        let result = track.finish();
        assert_eq!(
            result.grading,
            Grading::Recovered {
                cable: Some(3),
                cable_estimated: None,
            }
        );
        assert!(correlator.summary(&result.grading).outcome_confirmed);
    }

    #[tokio::test]
    async fn independent_hook_work_does_not_delay_position_ticks() {
        let slow_hook = tokio::spawn(async {
            tokio::time::sleep(std::time::Duration::from_millis(400)).await;
        });
        let started = std::time::Instant::now();
        let mut ticks = tokio::time::interval(std::time::Duration::from_millis(10));
        for _ in 0..5 {
            ticks.tick().await;
        }
        assert!(started.elapsed() < std::time::Duration::from_millis(150));
        slow_hook.abort();
    }

    #[test]
    fn hook_grpc_codes_use_documented_snake_case_names() {
        assert_eq!(
            grpc_code_name(tonic::Code::DeadlineExceeded),
            "deadline_exceeded"
        );
        assert_eq!(
            grpc_code_name(tonic::Code::ResourceExhausted),
            "resource_exhausted"
        );
        assert_eq!(
            grpc_code_name(tonic::Code::FailedPrecondition),
            "failed_precondition"
        );
    }

    #[test]
    fn stale_and_timed_out_hook_polls_never_become_certain_state() {
        let (tx, mut rx) = tokio::sync::mpsc::channel(4);
        tx.try_send(HookPoll {
            received_at: std::time::Instant::now() - std::time::Duration::from_secs(2),
            received_unix_ms: 1,
            raw: Some(1.0),
            status: HookSampleStatus::Success,
            grpc_code: None,
        })
        .unwrap();
        tx.try_send(HookPoll {
            received_at: std::time::Instant::now(),
            received_unix_ms: 2,
            raw: None,
            status: HookSampleStatus::Timeout,
            grpc_code: Some("deadline_exceeded".to_string()),
        })
        .unwrap();

        let carrier = CarrierInfo::by_type("CVN_71").unwrap();
        let plane = AirplaneInfo::by_type("FA-18C_hornet").unwrap();
        let mut track = Track::new("pilot", carrier, plane);
        drain_hook_samples(&mut rx, &mut track, 42.0, 4);
        let result = track.finish();

        assert_eq!(result.hook_observation.stale_samples, 1);
        assert_eq!(result.hook_observation.timeout_samples, 1);
        assert_eq!(result.hook_observation.successful_samples, 0);
        assert_eq!(result.hook_observation.interpreted_state, "unknown");
        assert_eq!(
            result
                .hook_observation
                .timeline
                .back()
                .unwrap()
                .grpc_code
                .as_deref(),
            Some("deadline_exceeded")
        );
    }

    #[test]
    fn arrested_recovery_without_wire_evidence_is_explicit() {
        let grading = Grading::Recovered {
            cable: None,
            cable_estimated: None,
        };

        assert_eq!(
            recovery_outcome(&grading, false),
            "Arrested — wire evidence unavailable"
        );
    }

    #[test]
    fn vstol_recovery_uses_spot_outcome() {
        let grading = Grading::Recovered {
            cable: None,
            cable_estimated: None,
        };

        assert_eq!(recovery_outcome(&grading, true), "Spot 7.5");
    }

    #[test]
    fn touch_and_go_is_not_exposed_as_bolter_for_vstol() {
        let grading = Grading::TouchAndGo {
            cable_estimated: Some(3),
        };

        assert_eq!(
            recovery_outcome(&grading, false),
            "T&G (CQ) — would have caught wire 3"
        );
        assert_eq!(recovery_outcome(&grading, true), "Waveoff/Go-around");
        assert_eq!(
            recovery_outcome(
                &Grading::TouchAndGo {
                    cable_estimated: None
                },
                false
            ),
            "T&G (CQ)"
        );
    }

    #[test]
    fn simultaneous_passes_and_new_generations_have_distinct_ids() {
        let first = recovery_id(10, 1, 100, 1, 42_000);
        let simultaneous = recovery_id(10, 1, 101, 1, 42_000);
        let regenerated = recovery_id(10, 2, 100, 1, 42_000);
        assert_ne!(first, simultaneous);
        assert_ne!(first, regenerated);
    }

    #[test]
    fn touchdown_event_without_a_complete_transform_is_not_evidence() {
        assert!(transform_from_event_unit(1.0, Unit::default()).is_none());

        let position_only = Unit {
            position: Some(Position::default()),
            ..Unit::default()
        };
        assert!(transform_from_event_unit(1.0, position_only).is_none());

        let complete = Unit {
            position: Some(Position::default()),
            orientation: Some(Orientation::default()),
            ..Unit::default()
        };
        assert!(transform_from_event_unit(1.0, complete).is_some());
    }
}
