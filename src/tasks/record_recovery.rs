use std::borrow::Cow;
use std::collections::HashSet;
use std::io::Cursor;
use std::time::{Duration, Instant};

use futures_util::future::Either;
use futures_util::stream::select;
use futures_util::StreamExt;
use once_cell::sync::Lazy;
use serenity::builder::{CreateAttachment, CreateEmbed, ExecuteWebhook};
use serenity::http::Http;
use serenity::model::id::UserId;
use serenity::model::mention::Mention;
use stubs::common::v0::{initiator, Airbase, Coalition, Initiator};
use stubs::mission::v0::stream_events_response::{
    CrashEvent, DeadEvent, Event, LandingQualityMarkEvent, PlayerLeaveUnitEvent, RunwayTouchEvent,
    UnitLostEvent,
};
use tacview::record::{self, Color, Coords, GlobalProperty, Property, Record, Tag, Update};
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;
use tonic::Status;

use crate::client::{HookClient, MissionClient, UnitClient};
use crate::grading::PassGrade;
use crate::track::{Datum, GateDeviations, Grading, Track};
use crate::transform::Transform;

use super::{CompletedPass, TaskParams};

/// Serialisable snapshot of a single recovery attempt, written to a `.json` file alongside
/// the ACMI and PNG chart.
#[derive(serde::Serialize)]
struct RecoveryReport<'a> {
    pilot_name: &'a str,
    grading: &'a Grading,
    pass_grade: PassGrade,
    #[serde(skip_serializing_if = "Option::is_none")]
    dcs_grading: Option<&'a str>,
    gate_deviations: &'a GateDeviations,
    datums: &'a [Datum],
    /// In-mission date/time from the DCS scenario clock (ISO-8601).
    #[serde(skip_serializing_if = "str::is_empty")]
    mission_datetime: &'a str,
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

#[tracing::instrument(
    skip_all,
    fields(carrier_name = params.carrier_name, plane_name = params.plane_name)
)]
pub async fn record_recovery(params: TaskParams<'_>) -> Result<(), crate::error::Error> {
    tracing::debug!("started recording");

    // Tacview-20211111-143727-DCS-grpc-lso.zip
    let now = OffsetDateTime::now_local().unwrap_or_else(|_| OffsetDateTime::now_utc());
    let now_utc = now.to_offset(time::UtcOffset::UTC);
    let recovery_timestamp = now_utc.format(&Rfc3339).unwrap_or_default();
    let filename = format!(
        "LSO-{}-{}",
        now.format(&FILENAME_DATETIME_FORMAT).unwrap_or_default(),
        params
            .pilot_name
            .chars()
            .filter(|c| c.is_ascii_alphanumeric())
            .collect::<String>()
    );

    let mut client1 = UnitClient::new(params.ch.clone());
    let mut client2 = UnitClient::new(params.ch.clone());
    let mut mission = MissionClient::new(params.ch.clone());
    let mut hook = HookClient::new(params.ch.clone());
    let mut world = crate::client::WorldClient::new(params.ch.clone());
    let interval = crate::utils::interval::interval(Duration::from_millis(100), params.shutdown);

    let mut acmi = Cursor::new(Vec::new());
    let mut recording = tacview::Writer::new_compressed(&mut acmi)?;
    let mut datums = Track::new(params.pilot_name, params.carrier_info, params.plane_info);

    let reference_time = mission.get_scenario_start_time().await?;
    recording.write(GlobalProperty::ReferenceTime(reference_time))?;
    recording.write(GlobalProperty::RecordingTime(
        OffsetDateTime::now_utc().format(&Rfc3339).unwrap(),
    ))?;

    let mission_name = hook.get_mission_name().await?;
    recording.write(GlobalProperty::Title(format!(
        "Carrier Recovery during {}",
        mission_name
    )))?;
    recording.write(GlobalProperty::Author(format!(
        "dcs-grpc-lso v{}",
        env!("CARGO_PKG_VERSION")
    )))?;

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

    recording.write(create_initial_update(&mut client1, 1, params.carrier_name).await?)?;
    recording.write(create_initial_update(&mut client1, 2, params.plane_name).await?)?;

    let events = mission.stream_events().await?;

    let mut known_carrier_coords = None;
    let mut known_plane_coords = None;
    let mut track_stopped: Option<Instant> = None;
    let mut lowest_altitude = f64::MAX;
    // Last known carrier geodetic position, used for the wind query at pass completion.
    let mut last_carrier_lat: f64 = 0.0;
    let mut last_carrier_lon: f64 = 0.0;
    let mut last_carrier_alt: f64 = 0.0;

    let mut stream = select(interval.map(Either::Left), events.map(Either::Right));

    while let Some(next) = stream.next().await {
        match next {
            // next interval
            Either::Left(_) => {
                let (carrier, plane) = futures_util::future::try_join(
                    client1.get_transform(params.carrier_name),
                    client2.get_transform(params.plane_name),
                )
                .await?;
                let hook_state = client2
                    .get_draw_argument_value(params.plane_name, 25)
                    .await
                    .unwrap_or(1.0);

                if !ref_written {
                    lat_ref = carrier.lat;
                    lon_ref = carrier.lon;
                    recording.write(GlobalProperty::ReferenceLatitude(lat_ref))?;
                    recording.write(GlobalProperty::ReferenceLongitude(lon_ref))?;
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
                    recording.write(Record::Frame(carrier.time))?;
                    recording.write(carrier_update)?;
                    recording.write(plane_update)?;
                } else if carrier.time < plane.time {
                    recording.write(Record::Frame(carrier.time))?;
                    recording.write(carrier_update)?;
                    recording.write(Record::Frame(plane.time))?;
                    recording.write(plane_update)?;
                } else {
                    recording.write(Record::Frame(plane.time))?;
                    recording.write(plane_update)?;
                    recording.write(Record::Frame(carrier.time))?;
                    recording.write(carrier_update)?;
                }

                last_carrier_lat = carrier.lat;
                last_carrier_lon = carrier.lon;
                last_carrier_alt = carrier.alt;

                lowest_altitude = lowest_altitude.min(plane.alt);

                if !datums.next(&carrier, &plane, hook_state) {
                    if let Some(stop_time) = track_stopped {
                        if stop_time.elapsed() > std::time::Duration::from_secs(10) {
                            tracing::info!("stop (10s passed since pass completed)");
                            break;
                        }
                    } else {
                        // Track told us to stop but hasn't set `track_stopped` yet
                        // (happens on Bolter or WaveoffPilot).
                        track_stopped = Some(Instant::now());
                    }
                }
            }

            // DCS landing grade
            Either::Right(event) => match event? {
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
                    datums.set_dcs_grading(comment.clone());
                    recording.write(Record::Frame(time))?;

                    let carrier = Transform::from((
                        time,
                        carrier.position.unwrap_or_default(),
                        carrier.orientation.unwrap_or_default(),
                        carrier.velocity.unwrap_or_default(),
                    ));
                    recording.write(Update {
                        id: 1,
                        props: vec![Property::T(remove_unchanged(
                            Coords::default()
                                .position(carrier.lat - lat_ref, carrier.lon - lon_ref, carrier.alt)
                                .uv(carrier.position.x, carrier.position.z)
                                .orientation(carrier.yaw, carrier.pitch, carrier.roll)
                                .heading(carrier.heading),
                            &mut known_carrier_coords,
                        ))],
                    })?;

                    let plane = Transform::from((
                        time,
                        plane.position.unwrap_or_default(),
                        plane.orientation.unwrap_or_default(),
                        plane.velocity.unwrap_or_default(),
                    ));
                    recording.write(Update {
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
                    })?;

                    recording.write(record::Event {
                        kind: record::EventKind::Message,
                        params: vec!["2".to_string(), "1".to_string()],
                        text: Some(comment),
                    })?;
                }

                // DCS land event
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
                    recording.write(Record::Frame(time))?;

                    let carrier = Transform::from((
                        time,
                        carrier.position.unwrap_or_default(),
                        carrier.orientation.unwrap_or_default(),
                        carrier.velocity.unwrap_or_default(),
                    ));
                    recording.write(Update {
                        id: 1,
                        props: vec![Property::T(remove_unchanged(
                            Coords::default()
                                .position(carrier.lat - lat_ref, carrier.lon - lon_ref, carrier.alt)
                                .uv(carrier.position.x, carrier.position.z)
                                .orientation(carrier.yaw, carrier.pitch, carrier.roll)
                                .heading(carrier.heading),
                            &mut known_carrier_coords,
                        ))],
                    })?;

                    let plane = Transform::from((
                        time,
                        plane.position.unwrap_or_default(),
                        plane.orientation.unwrap_or_default(),
                        plane.velocity.unwrap_or_default(),
                    ));
                    recording.write(Update {
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
                    })?;

                    recording.write(record::Event {
                        kind: record::EventKind::Landed,
                        params: vec!["2".to_string(), "1".to_string()],
                        text: None,
                    })?;

                    let hook_state = client2
                        .get_draw_argument_value(params.plane_name, 25)
                        .await
                        .unwrap_or(1.0);
                    datums.next(&carrier, &plane, hook_state);
                    datums.landed(&carrier, &plane);

                    // don't stop right away, track a couple of more seconds
                    track_stopped = Some(Instant::now());
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

    recording.into_inner();
    let data = acmi.into_inner();
    let acmi_path = if params.record_acmi {
        let path = params.out_dir.join(&filename).with_extension("zip.acmi");
        tokio::fs::write(&path, &data).await?;
        Some(path)
    } else {
        None
    };
    let track = datums.finish();

    // Discard if no recognisable outcome was established (e.g. plane flew through the zone
    // without ever entering the groove).
    if track.grading == Grading::Unknown {
        tracing::debug!("discard: no recovery outcome (Unknown grading)");
        return Ok(());
    }

    // Query in-mission date/time from the DCS scenario clock (non-fatal).
    let mission_datetime: String = match mission.get_scenario_current_time().await {
        Ok(dt) => dt,
        Err(err) => {
            tracing::warn!(?err, "failed to query in-mission datetime");
            String::new()
        }
    };

    // Write JSON report.
    let json_path = params.out_dir.join(&filename).with_extension("json");
    let report = RecoveryReport {
        pilot_name: &track.pilot_name,
        grading: &track.grading,
        pass_grade: track.pass_grade,
        dcs_grading: track.dcs_grading.as_deref(),
        gate_deviations: &track.gate_deviations,
        datums: &track.datums,
        mission_datetime: &mission_datetime,
    };
    tokio::fs::write(&json_path, serde_json::to_vec_pretty(&report)?).await?;

    let wire = match track.grading {
        Grading::Recovered { cable, .. } => cable,
        _ => None,
    };
    let aircraft_id = crate::data::get_aircraft_id(params.plane_type);
    let display_type = match aircraft_id {
        Some(2) => "F-14A/B",
        Some(3) => "F-14B(U)",
        _ => params.plane_info.name,
    };

    let completed = CompletedPass {
        timestamp: filename.clone(),
        pilot_name: track.pilot_name.clone(),
        pass_grade: track.pass_grade,
        wire,
        dcs_grading: track.dcs_grading.clone(),
        aircraft_type: display_type.to_string(),
        aircraft_id,
        map_name: map_name.clone(),
    };

    // Append to in-memory session greenie board log.
    if let Ok(mut log) = params.session_log.lock() {
        log.push(completed.clone());
    }

    // Query pilot UCID
    let pilot_ucid = {
        let mut net = crate::client::NetClient::new(params.ch.clone());
        match net.get_players().await {
            Ok(players) => players
                .into_iter()
                .find(|p| p.name == track.pilot_name)
                .map(|p| p.ucid)
                .filter(|u| !u.is_empty()),
            Err(err) => {
                tracing::warn!(?err, "failed to query players for UCID");
                None
            }
        }
    };

    // Persist to SQLite database (non-fatal — a write failure must not abort the recovery).
    {
        let db = params.db.clone();
        let entry = crate::db::DbPass {
            timestamp: completed.timestamp.clone(),
            pilot_name: completed.pilot_name.clone(),
            pilot_ucid,
            aircraft_id: completed.aircraft_id,
            pass_grade_label: completed.pass_grade.label().to_string(),
            wire: completed.wire,
            dcs_grading: completed.dcs_grading.clone(),
            aircraft_type: Some(completed.aircraft_type.clone()),
            map_name: if completed.map_name.is_empty() { None } else { Some(completed.map_name.clone()) },
            grade_date: now_utc
                .format(&GRADE_DATE_FORMAT)
                .unwrap_or_default(),
            grade_points: completed.pass_grade.points(),
            mission_datetime: mission_datetime.clone(),
        };
        match tokio::task::spawn_blocking(move || db.insert(&entry)).await {
            Ok(Ok(())) => {}
            Ok(Err(err)) => tracing::error!(?err, "failed to persist pass to database"),
            Err(err) => tracing::error!(?err, "database task panicked"),
        }
    }

    let chart_path = crate::draw::draw_chart(params.out_dir, &filename, &track)?;
    let pattern_chart_path =
        crate::draw::draw_pattern_chart(params.out_dir, &filename, &track)?;

    if let Some(discord_webhook) = params.discord_webhook.as_deref() {
        let http = Http::new("token");
        let webhook = http.get_webhook_from_url(discord_webhook).await?;

        // Query wind at carrier position (non-fatal — a failure must not abort the post).
        let wind: Option<(u16, f32)> = {
            let mut atmo = crate::client::AtmosphereClient::new(params.ch.clone());
            match atmo.get_wind(last_carrier_lat, last_carrier_lon, last_carrier_alt).await {
                Ok(w) => Some(w),
                Err(err) => {
                    tracing::warn!(?err, "failed to query wind at carrier position");
                    None
                }
            }
        };

        let mut embed = CreateEmbed::new()
            .field("Aircraft", params.plane_info.name, false)
            .field("Map", if map_name.is_empty() { "-" } else { map_name.as_str() }, false)
            .field("Date / Time (UTC)", recovery_timestamp.as_str(), false);
        if !mission_datetime.is_empty() {
            embed = embed.field("Mission Date/Time", mission_datetime.as_str(), false);
        }
        embed = embed
            .field(
                "Pilot",
                params
                    .users
                    .get(params.pilot_name)
                    .map(|id| Cow::Owned(Mention::from(UserId::new(*id)).to_string()))
                    .unwrap_or(Cow::Borrowed(params.pilot_name)),
                true,
            )
            .field(
                "Grade",
                format!("{} ({:.1} pts)", track.pass_grade.label(), track.pass_grade.points()),
                true,
            )
            .field(
                "Outcome",
                match track.grading {
                    Grading::Unknown => Cow::Borrowed("unknown"),
                    Grading::Bolter => Cow::Borrowed("Bolter"),
                    Grading::WaveoffPilot => Cow::Borrowed("Waveoff"),
                    Grading::IntentionalBolter { .. } => Cow::Borrowed("Qualif Bolter"),
                    Grading::Recovered { cable, .. } => cable
                        .map(|c| Cow::Owned(format!("Wire #{}", c)))
                        .unwrap_or(Cow::Borrowed("Landed")),
                },
                true,
            )
            .field(
                "Gates (GS / LU)",
                {
                    let fmt = |g: Option<&crate::track::GateDatum>| match g {
                        Some(d) => format!("{:+.0}ft / {:+.0}ft", d.gs_deviation_ft, d.lineup_ft),
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
    }

    Ok(())
}

async fn create_initial_update(
    client: &mut UnitClient,
    id: u64,
    unit_name: &str,
) -> Result<Update, Status> {
    let unit = client.get_unit(unit_name).await?;
    let attrs = client.get_descriptor(unit_name).await?;

    let coalition = Coalition::try_from(unit.coalition).unwrap_or(Coalition::Neutral);
    let mut props = vec![
        Property::Type(tags(attrs)),
        Property::Name(unit.r#type),
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
