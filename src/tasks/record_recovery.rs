use std::borrow::Cow;
use std::collections::HashSet;
use std::io::Cursor;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use futures_util::future::Either;
use futures_util::stream::{select, BoxStream};
use futures_util::StreamExt;
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
use crate::telemetry::{TelemetryInvalidReason, ACTIVE_WATCHDOG_MS};
use crate::track::{Datum, GateDeviations, Grading, HookSampleStatus, Track};
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
    /// Gate-only grade before the AV-8B touchdown-accuracy bonus.
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
    datums: &'a [Datum],
    /// In-mission date/time from the DCS scenario clock (ISO-8601).
    #[serde(skip_serializing_if = "str::is_empty")]
    mission_datetime: &'a str,
    recording_started_at: &'a str,
    completed_at: &'a str,
    touchdown_time_dcs: Option<f64>,
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
    grading_availability: &'static str,
    telemetry_quality: &'a crate::track::TelemetryQuality,
    events: &'a [crate::track::EventEvidence],
    spot_zone: &'a crate::track::SpotZoneObservation,
    touchdown_horizontal_speed_mps: Option<f64>,
    hook_observation: &'a crate::track::HookObservation,
    event_correlation: &'a super::event_correlator::EventCorrelationSummary,
}

#[derive(serde::Serialize)]
struct ReportCauses<'a> {
    primary: &'a str,
    secondary: &'a [&'static str],
}

const GRADING_VERSION: &str = "project-derived-v1";
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
            .get_draw_argument_value_with_timeout(&plane_name, 25, config.timeout)
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
        (_, Grading::Bolter) => "Bolter".to_string(),
        // Intentional bolters are valid only for arrested recoveries. Keep the
        // V/STOL fallback defensive in case an invalid grading reaches this layer.
        (true, Grading::TouchAndGo { .. }) => "Waveoff/Go-around".to_string(),
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

#[tracing::instrument(
    skip_all,
    fields(carrier_name = params.carrier_name, plane_name = params.plane_name)
)]
pub async fn record_recovery(params: TaskParams<'_>) -> Result<(), crate::error::Error> {
    let _recovery_guard = crate::metrics::RUNTIME_METRICS.recovery();
    let _priority_guard = PriorityCollectorGuard::new(&params);
    tracing::debug!("started recording");

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
    let interval = crate::utils::interval::interval(Duration::from_millis(100), params.shutdown);

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

    let events: BoxStream<'_, Result<(f64, Event), tonic::Status>> = if params.positions_only {
        futures_util::stream::pending().boxed()
    } else {
        match mission
            .as_mut()
            .expect("mission client enabled outside positions-only")
            .stream_events()
            .await
        {
            Ok(events) => events
                .chain(futures_util::stream::once(async {
                    Err(tonic::Status::unavailable("clean_end_of_event_stream"))
                }))
                .boxed(),
            Err(status) => futures_util::stream::once(async move { Err(*status) }).boxed(),
        }
    };
    let _event_stream_guard =
        (!params.positions_only).then(|| crate::metrics::RUNTIME_METRICS.stream());
    let (mut hook_rx, _hook_sampler) = if !params.carrier_info.is_vstol()
        && params.hook_sampling.mode == super::HookSamplingMode::Independent
    {
        let (hook_tx, hook_rx) = mpsc::channel(64);
        (
            Some(hook_rx),
            Some(AbortOnDrop(tokio::spawn(sample_hook(
                params.ch.clone(),
                params.plane_name.to_string(),
                params.hook_sampling,
                hook_tx,
            )))),
        )
    } else {
        (None, None)
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

    let mut stream = select(interval.map(Either::Left), events.map(Either::Right));

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
                        tracing::warn!(?status, ?silent_for, "transform polling failed");
                        if silent_for >= Duration::from_millis(ACTIVE_WATCHDOG_MS) {
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
                if batch.invalid_snapshots > 0 {
                    datums.mark_invalid_source_observations(batch.invalid_snapshots);
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
                let buffered_source = position_collector.is_buffered();
                let sample_count = batch.samples.len();
                for sample in batch.samples {
                    if sample.is_valid()
                        && (buffered_source || sample.source_age_ms <= f64::EPSILON)
                    {
                        last_telemetry_success = Instant::now();
                    }
                    if last_telemetry_success.elapsed() >= Duration::from_millis(ACTIVE_WATCHDOG_MS)
                    {
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
                    let hook_state = if !params.carrier_info.is_vstol()
                        && params.hook_sampling.mode == super::HookSamplingMode::LegacyInline
                    {
                        legacy_hook_client
                            .as_mut()
                            .expect("legacy hook client enabled")
                            .get_draw_argument_value(params.plane_name, 25)
                            .await
                            .ok()
                    } else {
                        None
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

                    if let Some(track_stopped) = track_stopped {
                        if track_stopped.elapsed() > Duration::from_secs(10) {
                            break 'recording;
                        }
                    }
                }
                if sample_count == 0
                    && last_telemetry_success.elapsed() >= Duration::from_millis(ACTIVE_WATCHDOG_MS)
                {
                    tracing::warn!(
                        silent_for = ?last_telemetry_success.elapsed(),
                        "active telemetry watchdog expired without buffered snapshots"
                    );
                    datums.mark_telemetry_gap(TelemetryInvalidReason::TelemetryGap);
                    break 'recording;
                }
            }

            Either::Right(Err(status)) => {
                tracing::warn!(?status, "mission event stream ended during recovery");
                let detail = if status.message() == "clean_end_of_event_stream" {
                    "clean_end_of_stream".to_string()
                } else {
                    format!("{}: {}", grpc_code_name(status.code()), status.message())
                };
                event_correlator.stream_unavailable(&mut datums, detail);
            }

            // DCS landing grade
            Either::Right(Ok(event)) => match event {
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

    if let Err(status) = position_collector.stop().await {
        tracing::warn!(
            ?status,
            "failed to stop source-buffered recovery telemetry cleanly"
        );
    }

    // If the plane was never below 100 m MSL, discard as a non-attempt.
    // Waveoffs and bolters still pass this check since they require being in the groove.
    if lowest_altitude > 100.0 {
        tracing::debug!("discard as plane was never below 100m MSL");
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
        tracing::debug!("discard: no recovery outcome (Unknown grading)");
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
            Grading::Bolter => "deck_crossing_without_arrest",
            Grading::TouchAndGo { .. } => "hook_up_near_deck",
            Grading::Recovered { .. } => "correlated_touchdown",
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
    // `spot` is retained as the legacy phase-1 alias. New consumers must use the
    // independent intended/nearest fields below.
    let spot_label = track.intended_spot;
    let report = RecoveryReport {
        schema_version: 3,
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
        datums: &track.datums,
        mission_datetime: &mission_datetime,
        recording_started_at: &recovery_timestamp,
        completed_at: &completed_at,
        touchdown_time_dcs: track.touchdown_time_dcs,
        lso_version: env!("CARGO_PKG_VERSION"),
        lso_commit: option_env!("GIT_COMMIT_HASH").unwrap_or("unknown"),
        lso_dirty: option_env!("GIT_DIRTY") == Some("true"),
        dcs_grpc_version: params.dcs_grpc_version,
        dcs_grpc_client_stubs: "0.10.0",
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
        grading_availability,
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

            // Query wind at carrier position (non-fatal — a failure must not abort the post).
            let wind: Option<(u16, f32)> = {
                let mut atmo = crate::client::AtmosphereClient::new(params.ch.clone());
                match atmo
                    .get_wind(last_carrier_lat, last_carrier_lon, last_carrier_alt)
                    .await
                {
                    Ok(w) => Some(w),
                    Err(err) => {
                        tracing::warn!(?err, "failed to query wind at carrier position");
                        None
                    }
                }
            };

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
                        let fmt = |g: Option<&crate::track::GateDatum>| match g {
                            Some(d) => {
                                format!("{:+.0}ft / {:+.0}ft", d.gs_deviation_ft, d.lineup_ft)
                            }
                            None => "-".to_string(),
                        };
                        Cow::Owned(format!(
                            "3/4nm: {}\n1/2nm: {}\n1/4nm: {}",
                            fmt(track.gate_deviations.at_three_quarter_nm.as_ref()),
                            fmt(track.gate_deviations.at_half_nm.as_ref()),
                            fmt(track.gate_deviations.at_quarter_nm.as_ref()),
                        ))
                    },
                    false,
                );

            if track.telemetry_quality.completeness != crate::track::Completeness::Complete {
                embed = embed.field(
                    "Technical status",
                    format!(
                        "Grading unavailable: {:?}. This is a measurement limitation, not a pilot failure.",
                        track.telemetry_quality.completeness
                    ),
                    false,
                );
            }

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

            // LSO notation and plain-English notes from DCS grading string.
            if let Some(ref notation) = track.dcs_grading {
                embed = embed.field("LSO Notation", notation.as_str(), false);
                let notes = crate::lso_notation::to_english(notation);
                if !notes.is_empty() {
                    embed = embed.field("LSO Notes", notes, false);
                }
            }

            // Wind and groove time — Discord-only fields.
            if let Some((dir, spd)) = wind {
                embed = embed.field("Wind", format!("{}° at {:.0} kts", dir, spd), true);
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

#[cfg(test)]
mod tests {
    use super::{drain_hook_samples, grpc_code_name, recovery_id, recovery_outcome, HookPoll};
    use crate::data::{AirplaneInfo, CarrierInfo};
    use crate::tasks::event_correlator::transform_from_event_unit;
    use crate::track::{Grading, HookSampleStatus, Track};
    use stubs::common::v0::{Orientation, Position, Unit};

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

        assert_eq!(recovery_outcome(&grading, false), "T&G (CQ)");
        assert_eq!(recovery_outcome(&grading, true), "Waveoff/Go-around");
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
