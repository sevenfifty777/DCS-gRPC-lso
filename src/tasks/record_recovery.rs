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
use tacview::record::{self, Color, Coords, GlobalProperty, Property, Record, Tag, Update};
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;
use tokio::sync::mpsc;

use crate::client::{HookClient, MissionClient, UnitClient};
use crate::grading::{PassGrade, SpotGrade};
use crate::metrics::RpcKind;
use crate::telemetry::{TelemetryAligner, TelemetryInvalidReason, ACTIVE_WATCHDOG_MS};
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
    hook_observation: &'a crate::track::HookObservation,
}

const GRADING_VERSION: &str = "project-derived-v1";
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
            .get_draw_argument_value_with_timeout(&plane_name, 25, config.timeout)
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
fn transform_from_event_unit(time: f64, unit: Unit) -> Option<Transform> {
    Some(Transform::from((
        time,
        unit.position?,
        unit.orientation?,
        unit.velocity.unwrap_or_default(),
    )))
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
                cable_estimated, ..
            },
        ) => cable_estimated
            .map(|wire| format!("Wire #{}", wire))
            .unwrap_or_else(|| "-".to_string()),
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
    let mut mission = MissionClient::new(params.ch.clone());
    let mut hook = HookClient::new(params.ch.clone());
    let mut world = crate::client::WorldClient::new(params.ch.clone());
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

    let events = mission.stream_events().await?;
    let _event_stream_guard = crate::metrics::RUNTIME_METRICS.stream();
    let (hook_tx, mut hook_rx) = mpsc::channel(64);
    let _hook_sampler = (!params.carrier_info.is_vstol()
        && params.hook_sampling.mode == super::HookSamplingMode::Independent)
        .then(|| {
            AbortOnDrop(tokio::spawn(sample_hook(
                params.ch.clone(),
                params.plane_name.to_string(),
                params.hook_sampling,
                hook_tx,
            )))
        });

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
                let (carrier_observed, plane_observed) = match futures_util::future::try_join(
                    client1
                        .get_observed_transform_for(params.carrier_name, RpcKind::TransformCarrier),
                    client2.get_observed_transform_for(params.plane_name, RpcKind::TransformPlane),
                )
                .await
                {
                    Ok(pair) => pair,
                    Err(status) if status.code() == tonic::Code::NotFound => {
                        tracing::info!("stop tracking because a unit no longer exists");
                        return Ok(());
                    }
                    Err(status) => {
                        telemetry_aligner.reset();
                        let silent_for = last_telemetry_success.elapsed();
                        tracing::warn!(?status, ?silent_for, "transform polling failed");
                        if silent_for >= Duration::from_millis(ACTIVE_WATCHDOG_MS) {
                            datums.mark_telemetry_gap(TelemetryInvalidReason::TelemetryGap);
                            break;
                        }
                        continue;
                    }
                };
                let sample = telemetry_aligner.align(carrier_observed, plane_observed);
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
                let hook_state = if params.carrier_info.is_vstol()
                    || params.hook_sampling.mode == super::HookSamplingMode::Independent
                {
                    None
                } else {
                    client2
                        .get_draw_argument_value(params.plane_name, 25)
                        .await
                        .ok()
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
                let plane_update = Update {
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
                if params.hook_sampling.mode == super::HookSamplingMode::Independent {
                    drain_hook_samples(
                        &mut hook_rx,
                        &mut datums,
                        plane.time,
                        params.hook_sampling.frequency_hz,
                    );
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

            Either::Right(Err(status)) => {
                tracing::warn!(?status, "mission event stream ended during recovery");
                datums.mark_telemetry_gap(TelemetryInvalidReason::TelemetryGap);
                break;
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
                ) if plane.id == params.plane_id && carrier.id == params.carrier_id => {
                    let Some((carrier, plane)) = transform_from_event_unit(time, carrier)
                        .zip(transform_from_event_unit(time, plane))
                    else {
                        datums.record_event("land", time, false, "missing_transform_evidence");
                        continue;
                    };
                    if params.hook_sampling.mode == super::HookSamplingMode::Independent {
                        drain_hook_samples(
                            &mut hook_rx,
                            &mut datums,
                            plane.time,
                            params.hook_sampling.frequency_hz,
                        );
                    }
                    let accepted = datums.landed(&carrier, &plane);
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
                    // near-event wire-4 crossing and could also make the last
                    // post-touch hook value look like pre-touch evidence.
                    if params.hook_sampling.mode == super::HookSamplingMode::Independent {
                        drain_hook_samples(
                            &mut hook_rx,
                            &mut datums,
                            plane.time,
                            params.hook_sampling.frequency_hz,
                        );
                    }
                    let accepted = datums.landed(&carrier, &plane);
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
                ) if unit.id == params.plane_id || unit.id == params.carrier_id => {
                    tracing::info!("stop (either carrier or plane despawned)");
                    return Ok(());
                }

                _ => {}
            },
        }
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

    let outcome = recovery_outcome(&track.grading, track.carrier_info.is_vstol());
    let (wire_estimated, wire_dcs) = match track.grading {
        Grading::Recovered {
            cable,
            cable_estimated,
        } => (cable_estimated, cable),
        Grading::TouchAndGo { cable_estimated } => (cable_estimated, None),
        _ => (None, None),
    };
    let wire_divergent = matches!((wire_estimated, wire_dcs), (Some(a), Some(b)) if a != b);
    let confidence = match track.telemetry_quality.completeness {
        crate::track::Completeness::Complete
            if wire_estimated == wire_dcs && wire_dcs.is_some() =>
        {
            "high"
        }
        crate::track::Completeness::Complete => "medium",
        _ => "insufficient",
    };
    let cause = match track.telemetry_quality.completeness {
        crate::track::Completeness::UnconfirmedArrest => "unconfirmed_arrest",
        _ => match track.grading {
            Grading::WaveoffUnknown => "go_around_initiator_unknown",
            Grading::Bolter => "deck_crossing_without_arrest",
            Grading::TouchAndGo { .. } => "hook_up_near_deck",
            Grading::Recovered { .. } => "correlated_touchdown",
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
        dcs_grpc_version: params.dcs_grpc_version,
        outcome: &outcome,
        cause,
        confidence,
        grading_version: GRADING_VERSION,
        grading_source: GRADING_SOURCE,
        wire_estimated,
        wire_dcs,
        wire_divergent,
        wire_primary: "estimated",
        wire_estimation: &track.wire_estimation,
        grading_availability,
        telemetry_quality: &track.telemetry_quality,
        events: &track.events,
        spot_zone: &track.spot_zone,
        touchdown_horizontal_speed_mps: track.touchdown_horizontal_speed_mps,
        hook_observation: &track.hook_observation,
    };
    match serde_json::to_vec_pretty(&report) {
        Ok(json) => {
            if let Err(err) = write_atomic_if_absent(&json_path, &json).await {
                tracing::error!(?err, path = %json_path.display(), "failed to persist JSON report");
            }
        }
        Err(err) => tracing::error!(?err, "failed to serialise JSON report"),
    }

    let wire = wire_estimated;
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
    if let Ok(mut log) = params.session_log.lock() {
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
        drain_hook_samples, recovery_id, recovery_outcome, transform_from_event_unit,
        write_atomic_if_absent, HookPoll,
    };
    use crate::data::{AirplaneInfo, CarrierInfo};
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

        assert_eq!(recovery_outcome(&grading, false), "-");
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
