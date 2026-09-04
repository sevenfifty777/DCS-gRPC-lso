use std::borrow::Cow;
use std::collections::HashSet;
use std::io::Cursor;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use futures_util::future::Either;
use futures_util::stream::select;
use futures_util::StreamExt;
use once_cell::sync::Lazy;
use serenity::builder::{CreateAttachment, CreateEmbed, ExecuteWebhook};
use serenity::http::Http;
use serenity::model::id::UserId;
use serenity::model::mention::Mention;
use stubs::common::v0::{initiator, Airbase, Coalition, Initiator, Unit};
use stubs::mission::v0::stream_events_response::{
    CrashEvent, DeadEvent, Event, LandEvent, LandingQualityMarkEvent, PlayerLeaveUnitEvent,
    RunwayTouchEvent, UnitLostEvent,
};
use stubs::recovery::v0::DrawArgumentStatus;
use tacview::record::{self, Color, Coords, GlobalProperty, Property, Record, Tag, Update};
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;
use tokio::sync::mpsc;

use crate::client::{HookClient, MissionClient, RecoveryClient, UnitClient};
use crate::grading::{PassGrade, SpotGrade};
use crate::metrics::RpcKind;
use crate::ownship_hook::{OwnshipHookObservation, OwnshipHookSampler};
use crate::telemetry::{TelemetryAligner, TelemetryInvalidReason, ACTIVE_WATCHDOG_MS};
use crate::track::{Datum, GateDeviations, Grading, HookSampleStatus, Track};
use crate::transform::Transform;

use super::{AcquisitionMode, CompletedPass, RecoveryTelemetryMode, TaskParams};

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
    acquisition_mode: AcquisitionMode,
    session_id: i64,
    generation: u64,
    grading: &'a Grading,
    /// Gate-only grade before the AV-8B touchdown-accuracy bonus. Omitted when
    /// the pass is technically incomplete so consumers cannot read a grade the
    /// system considers unavailable.
    #[serde(skip_serializing_if = "Option::is_none")]
    approach_grade: Option<PassGrade>,
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
    dcs_grpc_version: &'a str,
    outcome: &'a str,
    cause: &'a str,
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
    hook_observation: HookObservationReport<'a>,
    /// Commanded hook state latched from the pre-contact baseline.
    hook_state: crate::track::HookState,
    /// `dcs_wire`, `hook_transient`, `kinematic`, `unconfirmed` or `none`.
    arrest_evidence: &'static str,
    arrest_kinematics: &'a crate::track::ArrestKinematicsEvidence,
    #[serde(skip_serializing_if = "Option::is_none")]
    dcs_lso: Option<&'a crate::track::DcsLsoGrade>,
    ownship_hook_observation: &'a OwnshipHookObservation,
}

#[derive(serde::Serialize)]
struct HookObservationReport<'a> {
    evidence_source: &'static str,
    draw_argument: Option<u32>,
    #[serde(flatten)]
    observation: &'a crate::track::HookObservation,
}

const GRADING_VERSION: &str = "project-derived-v1";
/// Custom Tacview property carrying the raw hook draw argument per frame.
pub const ACMI_HOOK_PROPERTY: &str = "LSOHook";
const GRADING_SOURCE: &str = "PROJECT-DERIVED";
static OUTPUT_TEMP_SEQUENCE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

#[derive(Debug)]
struct HookPoll {
    received_at: Instant,
    received_unix_ms: u64,
    raw: Option<f64>,
    status: HookSampleStatus,
}

struct AbortOnDrop(tokio::task::JoinHandle<()>);

impl Drop for AbortOnDrop {
    fn drop(&mut self) {
        self.0.abort();
    }
}

async fn sample_hook(
    channel: tonic::transport::Channel,
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
        let (raw, status) = match client
            .get_draw_argument_value_with_timeout(&plane_name, draw_argument, config.timeout)
            .await
        {
            Ok(raw) if raw.is_finite() => (Some(raw), HookSampleStatus::Success),
            Ok(_) => (None, HookSampleStatus::Error),
            Err(status) if status.code() == tonic::Code::DeadlineExceeded => {
                (None, HookSampleStatus::Timeout)
            }
            Err(_) => (None, HookSampleStatus::Error),
        };
        let poll = HookPoll {
            received_at: Instant::now(),
            received_unix_ms: unix_time_ms(),
            raw,
            status,
        };
        if tx.try_send(poll).is_err() {
            crate::metrics::RUNTIME_METRICS.hook_sample_dropped();
        }
    }
}

fn unix_time_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u64::MAX as u128) as u64
}

fn may_fallback_to_legacy(requested: RecoveryTelemetryMode, code: tonic::Code) -> bool {
    requested == RecoveryTelemetryMode::Auto && code == tonic::Code::Unimplemented
}

fn hook_evidence_source(draw_argument: Option<u32>) -> &'static str {
    if draw_argument.is_some() {
        "external_draw_argument"
    } else {
        "not_requested"
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
        track.observe_hook_sample(
            associated_time_dcs,
            poll.received_unix_ms,
            age_ms,
            poll.raw,
            status,
        );
    }
}

/// Builds an event transform only when DCS supplied the position and orientation
/// required to correlate the evidence geometrically. A zero/default transform is
/// not evidence of a touchdown.
fn transform_from_event_unit(time: f64, unit: &Unit) -> Option<Transform> {
    Some(Transform::from((
        time,
        unit.position?,
        unit.orientation?,
        unit.velocity.unwrap_or_default(),
    )))
}

/// DCS time of the most recent recorded sample, for evidence that has no
/// timestamp of its own.
fn datums_last_time(track: &Track) -> f64 {
    track.last_sample_time().unwrap_or_default()
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

/// Whether a `Crash`/`Dead`/`PlayerLeaveUnit`/`UnitLost` event for the plane
/// or the carrier ends the recording with a graded pass (`true`) or discards it
/// (`false`). Before any accepted deck contact there is nothing to grade. After
/// one, the pass is graded from the evidence recorded so far: a pilot who leaves
/// the slot inside the post-touchdown window (7.5 s after `Land` in the
/// 2026-09-04 Foothold session, DCS `WIRE# 2` already received) must not lose
/// the trap.
fn finish_recording_on_despawn(touchdown_accepted: bool) -> bool {
    touchdown_accepted
}

/// Discord "Wire" field for an arrested recovery: the DCS wire next to the
/// independent estimate with an agreement marker, then the proof of the arrest.
/// `None` for every other outcome (bolter, T&G, waveoff, V/STOL), which carry
/// no wire evidence worth a field.
fn wire_evidence_field(
    grading: &Grading,
    is_vstol: bool,
    arrest_evidence: &str,
    arrest_held_s: Option<f64>,
) -> Option<String> {
    let Grading::Recovered {
        cable,
        cable_estimated,
    } = grading
    else {
        return None;
    };
    if is_vstol {
        return None;
    }
    let wire = |wire: Option<u8>| wire.map_or_else(|| "-".to_string(), |wire| wire.to_string());
    let marker = match (cable, cable_estimated) {
        (Some(dcs), Some(estimated)) if dcs == estimated => " ✓",
        (Some(_), Some(_)) => " ⚠ mismatch",
        _ => "",
    };
    let arrest = match arrest_evidence {
        "dcs_wire" => "DCS wire".to_string(),
        "hook_transient" => "hook transient (estimated wire)".to_string(),
        "kinematic" => match arrest_held_s {
            Some(held) => format!("deck kinematics (stopped {held:.1} s)"),
            None => "deck kinematics".to_string(),
        },
        "unconfirmed" => "unconfirmed".to_string(),
        other => other.to_string(),
    };
    Some(format!(
        "DCS: {}
Estimated: {}{marker}
Arrest: {arrest}",
        wire(*cable),
        wire(*cable_estimated),
    ))
}

pub(crate) fn recovery_outcome(grading: &Grading, is_vstol: bool, arrest_evidence: &str) -> String {
    match (is_vstol, grading) {
        (_, Grading::Unknown) => "unknown".to_string(),
        (_, Grading::Bolter) => "Bolter".to_string(),
        // Intentional bolters are valid only for arrested recoveries. Keep the
        // V/STOL fallback defensive in case an invalid grading reaches this layer.
        (true, Grading::TouchAndGo { .. }) => "Waveoff/Go-around".to_string(),
        (false, Grading::TouchAndGo { .. }) => "T&G (CQ)".to_string(),
        (_, Grading::WaveoffUnknown) => "Waveoff/Go-around — initiator unknown".to_string(),
        (_, Grading::WaveoffDcs) => "Waveoff (DCS LSO)".to_string(),
        (true, Grading::Recovered { .. }) => "Spot 7.5".to_string(),
        (
            false,
            Grading::Recovered {
                cable,
                cable_estimated,
            },
        ) => cable
            .or(*cable_estimated)
            .map(|wire| format!("Wire #{}", wire))
            .unwrap_or_else(|| {
                if arrest_evidence == "kinematic" {
                    "Arrested (wire unknown)".to_string()
                } else {
                    "-".to_string()
                }
            }),
    }
}

#[tracing::instrument(
    skip_all,
    fields(carrier_name = params.carrier_name, plane_name = params.plane_name)
)]
pub async fn record_recovery(params: TaskParams<'_>) -> Result<(), crate::error::Error> {
    let _recovery_guard = crate::metrics::RUNTIME_METRICS.recovery();
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

    let mut client1 = UnitClient::new(params.ch.clone());
    let mut client2 = UnitClient::new(params.ch.clone());
    let mut recovery_client = RecoveryClient::new(params.ch.clone());
    let mut mission = MissionClient::new(params.ch.clone());
    let mut hook = HookClient::new(params.ch.clone());
    let mut world = crate::client::WorldClient::new(params.ch.clone());
    let draw_argument = params.plane_info.hook_argument.map(|argument| argument.id);
    let acquisition_mode = match params.recovery_telemetry_mode {
        RecoveryTelemetryMode::Legacy => AcquisitionMode::Legacy,
        RecoveryTelemetryMode::Auto | RecoveryTelemetryMode::Atomic => {
            match recovery_client
                .get_snapshot(
                    params.carrier_name,
                    params.plane_name,
                    draw_argument,
                    0,
                    params.recovery_snapshot_timeout,
                )
                .await
            {
                Ok(_) => AcquisitionMode::Atomic,
                Err(status)
                    if may_fallback_to_legacy(params.recovery_telemetry_mode, status.code()) =>
                {
                    tracing::info!("atomic recovery API unavailable; using legacy telemetry");
                    AcquisitionMode::Legacy
                }
                Err(status) => return Err(status.into()),
            }
        }
    };
    tracing::info!(
        acquisition_mode = acquisition_mode.as_str(),
        aircraft_type = params.plane_type,
        hook_evidence_source = hook_evidence_source(draw_argument),
        hook_draw_argument = ?draw_argument,
        "selected recovery telemetry mode"
    );
    let interval =
        crate::utils::interval::recovery_interval(Duration::from_millis(100), params.shutdown);

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
    let mut telemetry_aligner = TelemetryAligner::new();
    let mut last_telemetry_success = Instant::now();

    if params.record_acmi {
        let reference_time = mission.get_scenario_start_time().await?;
        write_acmi!(GlobalProperty::ReferenceTime(reference_time));
        write_acmi!(GlobalProperty::RecordingTime(
            OffsetDateTime::now_utc().format(&Rfc3339).unwrap(),
        ));

        let mission_name = hook.get_mission_name().await?;
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
    let map_name: String = match world.get_theatre().await {
        Ok(t) => t,
        Err(err) => {
            tracing::warn!(?err, "failed to query theatre name");
            String::new()
        }
    };

    let mut ref_written = false;
    let mut lat_ref = 0.0;
    let mut lon_ref = 0.0;

    if params.record_acmi {
        write_acmi!(create_initial_update(&mut client1, 1, params.carrier_name).await?);
        write_acmi!(create_initial_update(&mut client1, 2, params.plane_name).await?);
    }

    // Subscribe to the generation's single mission event stream instead of
    // opening another server-side StreamEvents per recovery.
    let events = Box::pin(super::event_stream(params.events.subscribe()));
    let (hook_tx, mut hook_rx) = mpsc::channel(64);
    let _hook_sampler = (acquisition_mode == AcquisitionMode::Legacy
        && params.hook_sampling.mode == super::HookSamplingMode::Independent)
        .then_some(draw_argument)
        .flatten()
        .map(|draw_argument| {
            AbortOnDrop(tokio::spawn(sample_hook(
                params.ch.clone(),
                params.plane_name.to_string(),
                draw_argument,
                params.hook_sampling,
                hook_tx,
            )))
        });
    let mut ownship_hook_observation = OwnshipHookObservation::new(params.plane_id);
    // Diagnostic only: `LoGetMechInfo` needs a local cockpit, which a dedicated
    // server never has (0/6164 observed in the live corpus), so it is opt-in.
    let mut ownship_hook_sampler = (params.ownship_hook_diagnostics
        && !params.carrier_info.is_vstol())
    .then(|| OwnshipHookSampler::start(params.ch.clone(), params.plane_id, params.hook_sampling));
    let mut rpc_failures = 0_u32;

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
    let mut observation_sequence = 1_u64;

    let mut stream = select(interval.map(Either::Left), events.map(Either::Right));

    while let Some(next) = stream.next().await {
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
                let acquired = match acquisition_mode {
                    AcquisitionMode::Legacy => futures_util::future::try_join(
                        client1.get_observed_transform_for(
                            params.carrier_name,
                            RpcKind::TransformCarrier,
                        ),
                        client2
                            .get_observed_transform_for(params.plane_name, RpcKind::TransformPlane),
                    )
                    .await
                    .map(|(carrier, plane)| (carrier, plane, None, None, None, None)),
                    AcquisitionMode::Atomic => {
                        let sequence = observation_sequence;
                        observation_sequence = observation_sequence.saturating_add(1);
                        recovery_client
                            .get_snapshot(
                                params.carrier_name,
                                params.plane_name,
                                draw_argument,
                                sequence,
                                params.recovery_snapshot_timeout,
                            )
                            .await
                            .map(|snapshot| {
                                (
                                    snapshot.carrier,
                                    snapshot.plane,
                                    Some((
                                        snapshot.draw_argument_status,
                                        snapshot.draw_argument_value,
                                    )),
                                    Some(snapshot.sequence),
                                    Some(snapshot.round_trip_ms),
                                    Some((
                                        snapshot.queue_wait_ms,
                                        snapshot.lua_exec_ms,
                                        snapshot.queue_depth,
                                    )),
                                )
                            })
                    }
                };
                let (
                    carrier_observed,
                    plane_observed,
                    atomic_hook,
                    sequence,
                    request_round_trip_ms,
                    server_timing,
                ) = match acquired {
                    Ok(observation) => observation,
                    Err(status) if status.code() == tonic::Code::NotFound => {
                        tracing::info!("stop tracking because a unit no longer exists");
                        return Ok(());
                    }
                    Err(status) => {
                        // Keep the last sample time so the outage is measured by
                        // the next sample's gap; only extrapolation history is lost.
                        telemetry_aligner.invalidate_history();
                        rpc_failures += 1;
                        let silent_for = last_telemetry_success.elapsed();
                        tracing::warn!(?status, ?silent_for, "transform polling failed");
                        if silent_for >= Duration::from_millis(ACTIVE_WATCHDOG_MS) {
                            datums.mark_telemetry_gap(TelemetryInvalidReason::TelemetryGap);
                            break;
                        }
                        continue;
                    }
                };
                let mut sample = telemetry_aligner.align(carrier_observed, plane_observed);
                sample.observation_sequence = sequence;
                sample.request_round_trip_ms = request_round_trip_ms;
                if let Some((queue_wait_ms, lua_exec_ms, queue_depth)) = server_timing {
                    sample.queue_wait_ms = queue_wait_ms;
                    sample.lua_exec_ms = lua_exec_ms;
                    sample.queue_depth = queue_depth;
                    crate::metrics::RUNTIME_METRICS
                        .observe_snapshot_timing(queue_wait_ms, lua_exec_ms);
                }
                if sample.is_valid() && sample.source_age_ms <= f64::EPSILON {
                    last_telemetry_success = Instant::now();
                }
                if last_telemetry_success.elapsed() >= Duration::from_millis(ACTIVE_WATCHDOG_MS) {
                    tracing::warn!(
                        silent_for = ?last_telemetry_success.elapsed(),
                        source_age_ms = sample.source_age_ms,
                        "active telemetry watchdog expired without source advancement"
                    );
                    datums.mark_telemetry_gap(TelemetryInvalidReason::TelemetryGap);
                    break;
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
                let hook_state = match atomic_hook {
                    Some((DrawArgumentStatus::Observed, Some(raw))) if raw.is_finite() => Some(raw),
                    Some((DrawArgumentStatus::Observed, _))
                    | Some((DrawArgumentStatus::Unavailable, _))
                    | Some((DrawArgumentStatus::Unspecified, _)) => {
                        datums.observe_hook_sample(
                            sample.plane.time,
                            sample.plane_received_unix_ms,
                            0.0,
                            None,
                            HookSampleStatus::Error,
                        );
                        None
                    }
                    Some((DrawArgumentStatus::NotRequested, _)) => None,
                    None if draw_argument.is_none()
                        || params.hook_sampling.mode == super::HookSamplingMode::Independent =>
                    {
                        None
                    }
                    None => client2
                        .get_draw_argument_value(
                            params.plane_name,
                            draw_argument.expect("guarded draw argument"),
                        )
                        .await
                        .ok(),
                };

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
                            .position(carrier.lat - lat_ref, carrier.lon - lon_ref, carrier.alt)
                            .uv(carrier.position.x, carrier.position.z)
                            .orientation(carrier.yaw, carrier.pitch, carrier.roll)
                            .heading(carrier.heading),
                        &mut known_carrier_coords,
                    ))],
                };
                let mut plane_props = vec![
                    Property::T(remove_unchanged(
                        Coords::default()
                            .position(plane.lat - lat_ref, plane.lon - lon_ref, plane.alt)
                            .uv(plane.position.x, plane.position.z)
                            .orientation(plane.yaw, plane.pitch, plane.roll)
                            .heading(plane.heading),
                        &mut known_plane_coords,
                    )),
                    Property::AOA(plane.aoa),
                ];
                // Raw hook draw argument as a custom property so offline replay
                // can reproduce the hook classifier and the wire estimator.
                if let Some(raw) = hook_state {
                    plane_props.push(Property::Unknown(
                        ACMI_HOOK_PROPERTY.to_string(),
                        format!("{raw:.3}"),
                    ));
                }
                let plane_update = Update {
                    id: 2,
                    props: plane_props,
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

                last_carrier_lat = carrier.lat;
                last_carrier_lon = carrier.lon;
                last_carrier_alt = carrier.alt;

                lowest_altitude = lowest_altitude.min(plane.alt);

                let keep_tracking = datums.next_sample(&sample, hook_state);
                if acquisition_mode == AcquisitionMode::Legacy
                    && params.hook_sampling.mode == super::HookSamplingMode::Independent
                {
                    drain_hook_samples(
                        &mut hook_rx,
                        &mut datums,
                        plane.time,
                        params.hook_sampling.frequency_hz,
                    );
                }
                if let Some(sampler) = ownship_hook_sampler.as_mut() {
                    sampler.drain(&mut ownship_hook_observation);
                }
                if !keep_tracking {
                    break;
                }

                if let Some(track_stopped) = track_stopped {
                    if track_stopped.elapsed() > Duration::from_secs(10) {
                        break;
                    }
                }
            }

            Either::Right(Err(status)) if status.code() == tonic::Code::DataLoss => {
                // The shared fan-out dropped events for this slow subscriber.
                // A touchdown event may be among them, so record it as evidence
                // but keep the telemetry loop running.
                tracing::warn!(?status, "mission event fan-out lagged during recovery");
                datums.record_event(
                    "event_stream_lagged",
                    datums_last_time(&datums),
                    false,
                    status.message().to_string(),
                );
            }

            Either::Right(Err(status)) => {
                tracing::warn!(?status, "mission event stream ended during recovery");
                datums.mark_telemetry_gap(TelemetryInvalidReason::TelemetryGap);
                break;
            }

            // DCS landing grade
            Either::Right(Ok(event)) => match (event.0, &event.1) {
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
                ) if plane.id == params.plane_id && carrier.id == params.carrier_id => {
                    tracing::info!(%comment, "landing quality mark event");
                    let accepted = datums.set_dcs_grading(comment.clone());
                    datums.record_event(
                        "landing_quality_mark",
                        time,
                        accepted,
                        if accepted {
                            "first_matching_event"
                        } else {
                            "duplicate_ignored"
                        },
                    );
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
                        text: Some(comment.clone()),
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
                ) if plane.id == params.plane_id && carrier.id == params.carrier_id => {
                    let Some((carrier, plane)) = transform_from_event_unit(time, carrier)
                        .zip(transform_from_event_unit(time, plane))
                    else {
                        datums.record_event("land", time, false, "missing_transform_evidence");
                        continue;
                    };
                    // Record the touchdown first so hook samples drained
                    // afterwards are classified against the landing time.
                    let accepted = datums.landed(&carrier, &plane);
                    if acquisition_mode == AcquisitionMode::Legacy
                        && params.hook_sampling.mode == super::HookSamplingMode::Independent
                    {
                        drain_hook_samples(
                            &mut hook_rx,
                            &mut datums,
                            plane.time,
                            params.hook_sampling.frequency_hz,
                        );
                    }
                    datums.record_event(
                        "land",
                        time,
                        accepted,
                        if accepted {
                            "ids_and_geometry_correlated"
                        } else {
                            "duplicate_or_geometry_rejected"
                        },
                    );
                    if accepted {
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
                ) if plane.id == params.plane_id && carrier.id == params.carrier_id => {
                    tracing::info!("land event");

                    let Some((carrier, plane)) = transform_from_event_unit(time, carrier)
                        .zip(transform_from_event_unit(time, plane))
                    else {
                        datums.record_event(
                            "runway_touch",
                            time,
                            false,
                            "missing_transform_evidence",
                        );
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
                    // near-event wire-4 crossing. Record the touchdown first so
                    // hook samples drained afterwards are classified against
                    // the landing time instead of looking like pre-touch evidence.
                    let accepted = datums.landed(&carrier, &plane);
                    if acquisition_mode == AcquisitionMode::Legacy
                        && params.hook_sampling.mode == super::HookSamplingMode::Independent
                    {
                        drain_hook_samples(
                            &mut hook_rx,
                            &mut datums,
                            plane.time,
                            params.hook_sampling.frequency_hz,
                        );
                    }
                    datums.record_event(
                        "runway_touch",
                        time,
                        accepted,
                        if accepted {
                            "ids_and_geometry_correlated"
                        } else {
                            "duplicate_or_geometry_rejected"
                        },
                    );

                    // don't stop right away, track a couple of more seconds
                    if accepted {
                        track_stopped = Some(Instant::now());
                    }
                }

                // Any event indicating that either the carrier or plane do not exist anymore
                (
                    time,
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
                ) if unit.id == params.plane_id || unit.id == params.carrier_id => {
                    if finish_recording_on_despawn(track_stopped.is_some()) {
                        // The deck contact is already in hand (and usually the
                        // DCS `WIRE#` too): grade with the evidence recorded so
                        // far instead of erasing a completed pass.
                        tracing::info!(
                            "plane or carrier despawned after touchdown; grading the recorded evidence"
                        );
                        datums.record_event(
                            "despawn_after_touchdown",
                            time,
                            true,
                            "recording_finalised_early",
                        );
                        break;
                    }
                    tracing::info!("stop (either carrier or plane despawned)");
                    return Ok(());
                }

                _ => {}
            },
        }
    }

    tracing::info!(
        acquisition_mode = acquisition_mode.as_str(),
        rpc_failures,
        telemetry_warnings = datums.telemetry_quality().warning_samples,
        invalid_samples = datums.telemetry_quality().invalid_samples,
        max_sample_gap_ms = datums.telemetry_quality().max_sample_gap_ms,
        max_scoring_sample_gap_ms = datums.telemetry_quality().max_scoring_sample_gap_ms,
        completeness = ?datums.telemetry_quality().completeness,
        "recording loop ended"
    );

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
    if let Some(sampler) = ownship_hook_sampler.as_mut() {
        sampler.drain(&mut ownship_hook_observation);
    }
    let track = std::sync::Arc::new(datums.finish());

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
    let acmi_path = if params.record_acmi {
        let path = params.out_dir.join(&filename).with_extension("zip.acmi");
        match write_atomic_if_absent(&path, &data).await {
            Ok(()) => Some(path),
            Err(err) => {
                tracing::error!(?err, path = %path.display(), "failed to persist ACMI output");
                None
            }
        }
    } else {
        None
    };

    // Query in-mission date/time from the DCS scenario clock (non-fatal).
    let mission_datetime: String = match mission.get_scenario_current_time().await {
        Ok(dt) => dt,
        Err(err) => {
            tracing::warn!(?err, "failed to query in-mission datetime");
            String::new()
        }
    };

    let outcome = recovery_outcome(
        &track.grading,
        track.carrier_info.is_vstol(),
        track.arrest_evidence,
    );
    let (wire_estimated, wire_dcs) = match track.grading {
        Grading::Recovered {
            cable,
            cable_estimated,
        } => (cable_estimated, cable),
        Grading::TouchAndGo { cable_estimated } => (cable_estimated, None),
        _ => (None, None),
    };
    let wire_divergent = matches!((wire_estimated, wire_dcs), (Some(a), Some(b)) if a != b);
    let (wire, wire_primary) = crate::track::select_wire_for_display(wire_estimated, wire_dcs);
    let confidence = match track.telemetry_quality.completeness {
        crate::track::Completeness::Complete
            if wire_estimated == wire_dcs && wire_dcs.is_some() =>
        {
            "high"
        }
        crate::track::Completeness::Complete => "medium",
        _ => "insufficient",
    };
    let dcs_waveoff_ordered = track
        .dcs_lso
        .as_ref()
        .is_some_and(|grade| grade.waveoff_ordered);
    let cause = match track.telemetry_quality.completeness {
        crate::track::Completeness::UnconfirmedArrest => "unconfirmed_arrest",
        _ if dcs_waveoff_ordered && track.grading.touched_deck() => {
            "deck_contact_after_dcs_waveoff"
        }
        _ => match track.grading {
            Grading::WaveoffUnknown => "go_around_initiator_unknown",
            Grading::WaveoffDcs => "dcs_lso_waveoff",
            Grading::Bolter => "deck_crossing_without_arrest",
            Grading::TouchAndGo { .. } => "hook_up_near_deck",
            Grading::Recovered { .. } => match track.arrest_evidence {
                "kinematic" => "kinematic_arrest_without_wire",
                _ => "correlated_touchdown",
            },
            Grading::Unknown => "unknown",
        },
    };
    // Availability describes whether Rust had enough evidence to apply the
    // rules, independently from whether the resulting performance earns points.
    let grading_availability =
        if track.telemetry_quality.completeness == crate::track::Completeness::Complete {
            "available"
        } else {
            "unavailable_technical"
        };
    let aircraft_id = crate::data::get_aircraft_id(params.plane_type);
    let completed_at = OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_default();

    // Write JSON report.
    let json_path = params.out_dir.join(&filename).with_extension("json");
    // `spot` is retained as the legacy phase-1 alias. New consumers must use the
    // independent intended/nearest fields below.
    let spot_label = track.intended_spot;
    let report = RecoveryReport {
        schema_version: 8,
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
        acquisition_mode,
        session_id: params.session_id,
        generation: params.generation,
        grading: &track.grading,
        approach_grade: (track.pass_grade != PassGrade::Incomplete).then_some(track.approach_grade),
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
        dcs_grpc_version: params.dcs_grpc_version,
        outcome: &outcome,
        cause,
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
        hook_observation: HookObservationReport {
            evidence_source: hook_evidence_source(draw_argument),
            draw_argument,
            observation: &track.hook_observation,
        },
        hook_state: track.hook_state,
        arrest_evidence: track.arrest_evidence,
        arrest_kinematics: &track.arrest_kinematics,
        dcs_lso: track.dcs_lso.as_ref(),
        ownship_hook_observation: &ownship_hook_observation,
    };
    match serde_json::to_vec_pretty(&report) {
        Ok(json) => {
            if let Err(err) = write_atomic_if_absent(&json_path, &json).await {
                tracing::error!(?err, path = %json_path.display(), "failed to persist JSON report");
            }
        }
        Err(err) => tracing::error!(?err, "failed to serialise JSON report"),
    }

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
        wire,
        spot: spot_label.map(|s| s.to_string()),
        spot_grade: track.spot_grade,
        spot_distance_m: track.spot_distance_m,
        dcs_grading: track.dcs_grading.clone(),
        aircraft_type: display_type.to_string(),
        aircraft_id,
        map_name: map_name.clone(),
        outcome: outcome.clone(),
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
    {
        let mut log = crate::utils::lock_unpoisoned(&params.session_log);
        if !log.iter().any(|pass| pass.timestamp == completed.timestamp) {
            log.push(completed.clone());
        }
    }

    // Persist to SQLite database (non-fatal — a write failure must not abort the recovery).
    let db_inserted = {
        let db = params.db.clone();
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
            completeness: format!("{:?}", track.telemetry_quality.completeness).to_lowercase(),
            max_sample_gap_ms: track.telemetry_quality.max_sample_gap_ms,
            max_scoring_sample_gap_ms: track.telemetry_quality.max_scoring_sample_gap_ms,
            max_skew_ms: track.telemetry_quality.max_skew_ms,
            telemetry_health: format!("{:?}", track.telemetry_quality.health).to_lowercase(),
            wire_estimated,
            wire_dcs,
            wire_divergent,
            confidence: confidence.to_string(),
            cause: cause.to_string(),
            grading_version: GRADING_VERSION.to_string(),
            wire_estimation_confidence: track.wire_estimation.confidence.to_string(),
            grading_availability: grading_availability.to_string(),
            arrest_evidence: track.arrest_evidence.to_string(),
            hook_state: track.hook_state.as_str().to_string(),
        };
        match tokio::task::spawn_blocking(move || db.insert(&entry)).await {
            Ok(Ok(inserted)) => Some(inserted),
            Ok(Err(err)) => {
                tracing::error!(?err, "failed to persist pass to database");
                None
            }
            Err(err) => {
                tracing::error!(?err, "database task panicked");
                None
            }
        }
    };

    let render_track = track.clone();
    let render_dir = params.out_dir.to_path_buf();
    let render_filename = filename.clone();
    let rendered = match tokio::task::spawn_blocking(move || {
        let started = Instant::now();
        let chart = crate::draw::draw_chart(&render_dir, &render_filename, &render_track)?;
        let pattern =
            crate::draw::draw_pattern_chart(&render_dir, &render_filename, &render_track)?;
        crate::metrics::RUNTIME_METRICS
            .observe_render(started.elapsed().as_micros().min(u64::MAX as u128) as u64);
        Ok::<_, crate::error::Error>((chart, pattern))
    })
    .await
    {
        Ok(Ok(paths)) => Some(paths),
        Ok(Err(err)) => {
            tracing::error!(?err, "PNG rendering failed after persistence");
            None
        }
        Err(err) => {
            tracing::error!(?err, "PNG rendering task panicked after persistence");
            None
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
                .field("Outcome", completed.outcome.clone(), true);
            if let Some(wire_field) = wire_evidence_field(
                &track.grading,
                track.carrier_info.is_vstol(),
                track.arrest_evidence,
                track.arrest_kinematics.held_s,
            ) {
                embed = embed.field("Wire", wire_field, true);
            }
            embed = embed
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
            tracing::error!(?err, "Discord publication failed after local persistence");
        }
    } else if params.discord_webhook.is_some() && db_inserted != Some(true) {
        tracing::warn!("Discord publication skipped because this recovery was not newly persisted");
    }

    Ok(())
}

async fn write_atomic_if_absent(path: &std::path::Path, bytes: &[u8]) -> std::io::Result<()> {
    if tokio::fs::try_exists(path).await? {
        return Ok(());
    }
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("output");
    let sequence = OUTPUT_TEMP_SEQUENCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let temporary =
        path.with_extension(format!("{extension}.tmp-{}-{sequence}", std::process::id()));
    tokio::fs::write(&temporary, bytes).await?;
    crate::metrics::RUNTIME_METRICS.add_io_bytes(bytes.len());
    match tokio::fs::rename(&temporary, path).await {
        Ok(()) => Ok(()),
        Err(_err) if tokio::fs::try_exists(path).await.unwrap_or(false) => {
            let _ = tokio::fs::remove_file(&temporary).await;
            Ok(())
        }
        Err(err) => {
            let _ = tokio::fs::remove_file(&temporary).await;
            Err(err)
        }
    }
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
    use super::{
        drain_hook_samples, finish_recording_on_despawn, hook_evidence_source,
        may_fallback_to_legacy, recovery_id, recovery_outcome, transform_from_event_unit,
        wire_evidence_field, write_atomic_if_absent, HookObservationReport, HookPoll,
    };
    use crate::data::{AirplaneInfo, CarrierInfo};
    use crate::track::{Grading, HookSampleStatus, Track};
    use stubs::common::v0::{Orientation, Position, Unit};

    #[test]
    fn auto_falls_back_only_when_snapshot_rpc_is_unimplemented() {
        use crate::tasks::RecoveryTelemetryMode;

        assert!(may_fallback_to_legacy(
            RecoveryTelemetryMode::Auto,
            tonic::Code::Unimplemented
        ));
        assert!(!may_fallback_to_legacy(
            RecoveryTelemetryMode::Atomic,
            tonic::Code::Unimplemented
        ));
        assert!(!may_fallback_to_legacy(
            RecoveryTelemetryMode::Auto,
            tonic::Code::Unavailable
        ));
    }

    #[test]
    fn despawn_discards_only_before_deck_contact() {
        // Pilot leaves the slot (or the unit is lost) while still in the pattern
        // or in the groove: nothing to grade.
        assert!(!finish_recording_on_despawn(false));
        // Same event inside the post-touchdown window: the pass is graded from
        // the evidence already recorded (2026-09-04 T-45 trap, WIRE# 2, pilot
        // left the unit 7.5 s after the land event).
        assert!(finish_recording_on_despawn(true));
    }

    #[test]
    fn discord_wire_field_marks_agreement_and_names_the_proof() {
        let recovered = |cable, cable_estimated| Grading::Recovered {
            cable,
            cable_estimated,
        };
        assert_eq!(
            wire_evidence_field(&recovered(Some(1), Some(1)), false, "dcs_wire", Some(2.0)),
            Some(
                "DCS: 1
Estimated: 1 ✓
Arrest: DCS wire"
                    .to_string()
            )
        );
        assert_eq!(
            wire_evidence_field(&recovered(Some(2), Some(3)), false, "dcs_wire", Some(2.0)),
            Some(
                "DCS: 2
Estimated: 3 ⚠ mismatch
Arrest: DCS wire"
                    .to_string()
            )
        );
        // Human LSO, no DCS comment: the estimate stands alone and the proof is named.
        assert_eq!(
            wire_evidence_field(&recovered(None, Some(3)), false, "hook_transient", None),
            Some(
                "DCS: -
Estimated: 3
Arrest: hook transient (estimated wire)"
                    .to_string()
            )
        );
        assert_eq!(
            wire_evidence_field(&recovered(None, None), false, "kinematic", Some(2.3)),
            Some(
                "DCS: -
Estimated: -
Arrest: deck kinematics (stopped 2.3 s)"
                    .to_string()
            )
        );
        assert_eq!(
            wire_evidence_field(&recovered(None, None), false, "unconfirmed", None),
            Some(
                "DCS: -
Estimated: -
Arrest: unconfirmed"
                    .to_string()
            )
        );
    }

    #[test]
    fn discord_wire_field_is_absent_without_an_arrest() {
        for grading in [
            Grading::Bolter,
            Grading::TouchAndGo {
                cable_estimated: Some(2),
            },
            Grading::WaveoffDcs,
            Grading::WaveoffUnknown,
            Grading::Unknown,
        ] {
            assert_eq!(wire_evidence_field(&grading, false, "none", None), None);
        }
        let vstol = Grading::Recovered {
            cable: None,
            cable_estimated: None,
        };
        assert_eq!(wire_evidence_field(&vstol, true, "none", None), None);
    }

    #[test]
    fn external_hook_arguments_follow_modelviewer_validation() {
        let argument = |plane_type: &str| {
            AirplaneInfo::by_type(plane_type)
                .and_then(|info| info.hook_argument)
                .map(|argument| argument.id)
        };
        for plane_type in [
            "F-14A-135-GR",
            "F-14A-135-GR-Early",
            "F-14A-95-GR",
            "F-14B",
            "F-14A/B",
            "F-14B(U)",
            "F-14BU",
        ] {
            assert_eq!(argument(plane_type), Some(1305));
        }
        assert_eq!(argument("FA-18C_hornet"), Some(25));
        assert_eq!(argument("T-45"), Some(25));
        assert_eq!(argument("AV8BNA"), None);
    }

    #[test]
    fn hook_observation_report_persists_external_argument_provenance() {
        let observation = crate::track::HookObservation::default();
        for draw_argument in [25, 1305] {
            let report = HookObservationReport {
                evidence_source: hook_evidence_source(Some(draw_argument)),
                draw_argument: Some(draw_argument),
                observation: &observation,
            };

            let json = serde_json::to_value(report).unwrap();
            assert_eq!(json["evidence_source"], "external_draw_argument");
            assert_eq!(json["draw_argument"], draw_argument);
            assert!(json.get("successful_samples").is_some());
        }
    }

    #[test]
    fn hook_observation_report_marks_unrequested_argument() {
        let observation = crate::track::HookObservation::default();
        let report = HookObservationReport {
            evidence_source: hook_evidence_source(None),
            draw_argument: None,
            observation: &observation,
        };

        let json = serde_json::to_value(report).unwrap();
        assert_eq!(json["evidence_source"], "not_requested");
        assert!(json["draw_argument"].is_null());
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
    fn stale_and_timed_out_hook_polls_never_become_certain_state() {
        let (tx, mut rx) = tokio::sync::mpsc::channel(4);
        tx.try_send(HookPoll {
            received_at: std::time::Instant::now() - std::time::Duration::from_secs(2),
            received_unix_ms: 1,
            raw: Some(1.0),
            status: HookSampleStatus::Success,
        })
        .unwrap();
        tx.try_send(HookPoll {
            received_at: std::time::Instant::now(),
            received_unix_ms: 2,
            raw: None,
            status: HookSampleStatus::Timeout,
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
    }

    #[test]
    fn arrested_recovery_without_detected_wire_uses_dash_outcome() {
        let grading = Grading::Recovered {
            cable: None,
            cable_estimated: None,
        };

        assert_eq!(recovery_outcome(&grading, false, "unconfirmed"), "-");
        assert_eq!(
            recovery_outcome(&grading, false, "kinematic"),
            "Arrested (wire unknown)"
        );
    }

    #[test]
    fn arrested_recovery_outcome_prefers_dcs_wire() {
        let grading = Grading::Recovered {
            cable: Some(4),
            cable_estimated: Some(2),
        };

        assert_eq!(recovery_outcome(&grading, false, "dcs_wire"), "Wire #4");
    }

    #[test]
    fn arrested_recovery_outcome_uses_estimate_without_dcs_wire() {
        let grading = Grading::Recovered {
            cable: None,
            cable_estimated: Some(2),
        };

        assert_eq!(
            recovery_outcome(&grading, false, "hook_transient"),
            "Wire #2"
        );
    }

    #[test]
    fn vstol_recovery_uses_spot_outcome() {
        let grading = Grading::Recovered {
            cable: None,
            cable_estimated: None,
        };

        assert_eq!(recovery_outcome(&grading, true, "none"), "Spot 7.5");
    }

    #[test]
    fn touch_and_go_is_not_exposed_as_bolter_for_vstol() {
        let grading = Grading::TouchAndGo {
            cable_estimated: Some(3),
        };

        assert_eq!(recovery_outcome(&grading, false, "none"), "T&G (CQ)");
        assert_eq!(
            recovery_outcome(&grading, true, "none"),
            "Waveoff/Go-around"
        );
        assert_eq!(
            recovery_outcome(&Grading::WaveoffDcs, false, "none"),
            "Waveoff (DCS LSO)"
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
        assert!(transform_from_event_unit(1.0, &Unit::default()).is_none());

        let position_only = Unit {
            position: Some(Position::default()),
            ..Unit::default()
        };
        assert!(transform_from_event_unit(1.0, &position_only).is_none());

        let complete = Unit {
            position: Some(Position::default()),
            orientation: Some(Orientation::default()),
            ..Unit::default()
        };
        assert!(transform_from_event_unit(1.0, &complete).is_some());
    }

    #[tokio::test]
    async fn simultaneous_output_writers_do_not_collide_or_overwrite_partially() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory = std::env::temp_dir().join(format!(
            "dcs-grpc-lso-output-collision-{}-{unique}",
            std::process::id()
        ));
        std::fs::create_dir(&directory).expect("create isolated output fixture");
        let path = directory.join("same-recovery.json");

        let (first, second) = tokio::join!(
            write_atomic_if_absent(&path, b"first-complete-payload"),
            write_atomic_if_absent(&path, b"second-complete-payload")
        );
        first.expect("first writer");
        second.expect("second writer is idempotent");

        let bytes = std::fs::read(&path).expect("read final output");
        assert!(
            bytes == b"first-complete-payload" || bytes == b"second-complete-payload",
            "the target must contain one complete payload"
        );
        let entries = std::fs::read_dir(&directory)
            .expect("list output fixture")
            .collect::<Result<Vec<_>, _>>()
            .expect("read output entries");
        assert_eq!(entries.len(), 1, "temporary files must be cleaned up");

        std::fs::remove_file(path).expect("remove output fixture file");
        std::fs::remove_dir(directory).expect("remove output fixture directory");
    }

    #[tokio::test]
    async fn failed_atomic_output_is_reported_without_leaving_a_partial_target() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir()
            .join(format!(
                "dcs-grpc-lso-missing-parent-{}-{unique}",
                std::process::id()
            ))
            .join("report.json");
        let result = write_atomic_if_absent(&path, b"payload").await;
        assert!(result.is_err());
        assert!(!path.exists());
    }
}
