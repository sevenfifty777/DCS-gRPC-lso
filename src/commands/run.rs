use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::task::JoinHandle;

use crate::data::{AirplaneInfo, CarrierInfo};
use crate::db::{RecoveryDb, SharedDb};
use crate::tasks::{SessionLog, TaskParams};
use crate::utils::shutdown::ShutdownHandle;
use backoff::ExponentialBackoff;
use futures_util::future::select;
use futures_util::{StreamExt, TryFutureExt};
use stubs::coalition::v0::coalition_service_client::CoalitionServiceClient;
use stubs::common::v0::{Coalition, GroupCategory};
use stubs::group::v0::group_service_client::GroupServiceClient;
use stubs::mission::v0::mission_service_client::MissionServiceClient;
use stubs::mission::v0::stream_events_response::Event;
use stubs::unit::v0::unit_service_client::UnitServiceClient;
use stubs::{coalition, common, group, mission, unit};
use tokio::sync::mpsc;
use tonic::transport::{Channel, Endpoint, Uri};
use tonic::Status;

#[derive(clap::Parser)]
pub struct Opts {
    /// The directory the carrier recovery recordings should be saved to.
    #[clap(short = 'o', long, default_value = ".")]
    out_dir: PathBuf,

    /// The URI of DCS-gRPC.
    #[clap(long, default_value = "http://127.0.0.1:50051")]
    uri: Uri,

    /// A Discord webhook recovery recordings should be posted to.
    #[clap(long)]
    discord_webhook: Option<String>,

    /// A JSON file that maps player names to Discord user IDs.
    #[clap(long)]
    discord_users: Option<PathBuf>,

    /// Whether to also record carrier recoveries of KI units (mostly useful for testing/debugging).
    #[clap(long = "ki")]
    include_ki: bool,

    /// Disable saving of TacView ACMI files (PNG chart and JSON report are still saved).
    #[clap(long = "no-acmi")]
    no_acmi: bool,

    /// Port to serve the web greenie board on (e.g. 8080). Disabled if not specified.
    #[clap(long)]
    web_port: Option<u16>,
}

pub async fn execute(
    opts: Opts,
    shutdown_handle: ShutdownHandle,
) -> Result<(), crate::error::Error> {
    if opts.discord_webhook.is_some() {
        tracing::info!("Discord integration enabled.");
    }

    tracing::info!(uri = %opts.uri, "Connecting to gRPC server");

    let users: Arc<HashMap<String, u64>> =
        Arc::new(if let Some(path) = opts.discord_users.as_deref() {
            serde_json::from_slice(&tokio::fs::read(path).await?)?
        } else {
            Default::default()
        });

    let backoff = ExponentialBackoff {
        // never wait longer than 30s for a retry
        max_interval: Duration::from_secs(30),
        // never stop trying
        max_elapsed_time: None,
        ..Default::default()
    };

    let session_log: SessionLog = Arc::new(Mutex::new(Vec::new()));
    let db: SharedDb = Arc::new(RecoveryDb::open(&opts.out_dir.join("lso.db"))?);

    // Optionally start the web greenie board dashboard.
    if let Some(port) = opts.web_port {
        let db = db.clone();
        tokio::spawn(async move {
            if let Err(err) = crate::web::serve(db, port).await {
                tracing::error!(%err, "web dashboard server error");
            }
        });
    }

    select(
        Box::pin(backoff::future::retry_notify(
            backoff,
            // on each try, run the program and consider every error as transient (ie. worth
            // retrying)
            || async {
                run(&opts, users.clone(), shutdown_handle.clone(), session_log.clone(), db.clone())
                    .await
                    .map_err(backoff::Error::transient)
            },
            // error hook:
            |err, backoff: Duration| {
                tracing::debug!(
                    %err,
                    backoff = %format!("{:.2}s", backoff.as_secs_f64()),
                    "retrying after error"
                );
            },
        )),
        shutdown_handle.signal(),
    )
    .await;

    print_greenie_board(&session_log);

    Ok(())
}

async fn run(
    opts: &Opts,
    users: Arc<HashMap<String, u64>>,
    shutdown_handle: ShutdownHandle,
    session_log: SessionLog,
    db: SharedDb,
) -> Result<(), crate::error::Error> {
    let out_dir = opts.out_dir.clone();
    let channel = Endpoint::from(opts.uri.clone())
        .keep_alive_while_idle(true)
        .connect()
        .await?;
    tracing::info!("Connected");
    let mut coalition_svc = CoalitionServiceClient::new(channel.clone());
    let group_svc = GroupServiceClient::new(channel.clone());
    let mut unit_svc = UnitServiceClient::new(channel.clone());
    let mut mission_svc = MissionServiceClient::new(channel.clone());

    // initial full-sync of all current units inside of the mission
    let groups = coalition_svc
        .get_groups(coalition::v0::GetGroupsRequest {
            coalition: Coalition::All.into(),
            category: 0,
        })
        .map_ok(|res| res.into_inner().groups)
        .await?;

    let group_units = futures_util::future::try_join_all(
        groups
            .into_iter()
            .filter(|group| {
                if let Ok(category) = GroupCategory::try_from(group.category) {
                    matches!(category, GroupCategory::Airplane | GroupCategory::Ship)
                } else {
                    false
                }
            })
            .map(|group| {
                let mut group_svc = group_svc.clone();
                async move {
                    group_svc
                        .get_units(group::v0::GetUnitsRequest {
                            group_name: group.name,
                            active: Some(true),
                        })
                        .map_ok(|res| res.into_inner().units)
                        .await
                }
            }),
    )
    .await?;

    let mut planes: HashMap<String, (u32, String, String, &'static AirplaneInfo)> = HashMap::new();
    let mut carriers: HashMap<String, (u32, &'static CarrierInfo)> = HashMap::new();

    for units in group_units {
        for unit in units {
            match check_candidate(&mut unit_svc, &unit, opts.include_ki).await? {
                Some(Candidate::Plane {
                    info: plane_info,
                    unit_type,
                }) => {
                    planes.insert(
                        unit.name.clone(),
                        (
                            unit.id,
                            unit.player_name.unwrap_or_else(|| String::from("KI")),
                            unit_type,
                            plane_info,
                        ),
                    );
                }
                Some(Candidate::Carrier(carrier_info)) => {
                    carriers.insert(unit.name, (unit.id, carrier_info));
                }
                None => {}
            }
        }
    }

    let (tx, mut rx) = mpsc::channel(1);

    // Tracks the active detect_recovery_attempt task for each (plane_id, carrier_id) pair.
    // When a Birth event re-spawns a known unit the old task is aborted before a new one starts,
    // preventing duplicate recordings.
    let active_tasks: Arc<Mutex<HashMap<(u32, u32), JoinHandle<()>>>> =
        Arc::new(Mutex::new(HashMap::new()));

    let discord_webhook = opts.discord_webhook.clone();
    let record_acmi = !opts.no_acmi;
    let tx2 = tx.clone();
    let active_tasks2 = active_tasks.clone();
    let spawn_detect_recovery_attempt =
        move |carrier_id: u32,
              carrier_name: String,
              carrier_info: &'static CarrierInfo,
              plane_id: u32,
              plane_name: String,
              plane_type: String,
              plane_info: &'static AirplaneInfo,
              pilot_name: String| {
            let out_dir = out_dir.clone();
            let discord_webhook = discord_webhook.clone();
            let record_acmi = record_acmi;
            let users = users.clone();
            let channel = channel.clone();
            let tx = tx2.clone();
            let shutdown_handle = shutdown_handle.clone();
            let session_log = session_log.clone();
            let db = db.clone();
            let active_tasks = active_tasks2.clone();
            let handle = tokio::spawn(async move {
                if let Err(err) =
                    crate::tasks::detect_recovery_attempt::detect_recovery_attempt(TaskParams {
                        out_dir: &out_dir,
                        discord_webhook,
                        record_acmi,
                        users,
                        ch: channel,
                        carrier_id,
                        carrier_name: &carrier_name,
                        plane_id,
                        plane_name: &plane_name,
                        plane_type: &plane_type,
                        pilot_name: &pilot_name,
                        carrier_info,
                        plane_info,
                        shutdown: shutdown_handle,
                        session_log,
                        db,
                    })
                    .await
                {
                    tx.send(err).await.ok();
                }
                // Remove ourselves from the active map when done.
                if let Ok(mut map) = active_tasks.lock() {
                    map.remove(&(plane_id, carrier_id));
                }
            });
            // Abort any existing task for this pair before registering the new one.
            if let Ok(mut map) = active_tasks2.lock() {
                if let Some(old) = map.insert((plane_id, carrier_id), handle) {
                    tracing::debug!(
                        plane_id, carrier_id,
                        "aborting stale detect_recovery_attempt task for re-spawned unit"
                    );
                    old.abort();
                }
            }
        };

    for (carrier_name, (carrier_id, carrier_info)) in &carriers {
        for (plane_name, (plane_id, pilot_name, plane_type, plane_info)) in &planes {
            spawn_detect_recovery_attempt(
                *carrier_id,
                carrier_name.clone(),
                carrier_info,
                *plane_id,
                plane_name.clone(),
                plane_type.clone(),
                plane_info,
                pilot_name.clone(),
            );
        }
    }

    // listen for birth events to track carriers and planes spawned at a later point in time
    let mut events = mission_svc
        .stream_events(mission::v0::StreamEventsRequest {})
        .await?
        .into_inner();
    let tx_events = tx.clone();
    let include_ki = opts.include_ki;
    tokio::spawn(async move {
        while let Some(event) = events.next().await {
            let event = match event {
                Ok(stubs::mission::v0::StreamEventsResponse {
                    event: Some(event), ..
                }) => event,
                Ok(_) => continue,
                Err(err) => {
                    tx_events.send(err.into()).await.ok();
                    return;
                }
            };

            if let Event::Birth(mission::v0::stream_events_response::BirthEvent {
                initiator:
                    Some(common::v0::Initiator {
                        initiator: Some(common::v0::initiator::Initiator::Unit(unit)),
                    }),
                ..
            }) = event
            {
                match check_candidate(&mut unit_svc, &unit, include_ki).await {
                    Ok(Some(Candidate::Plane {
                        info: plane_info,
                        unit_type,
                    })) => {
                        for (carrier_name, (carrier_id, carrier_info)) in &carriers {
                            spawn_detect_recovery_attempt(
                                *carrier_id,
                                carrier_name.clone(),
                                carrier_info,
                                unit.id,
                                unit.name.clone(),
                                unit_type.clone(),
                                plane_info,
                                unit.player_name
                                    .clone()
                                    .unwrap_or_else(|| String::from("KI")),
                            );
                        }
                    }
                    Ok(Some(Candidate::Carrier(carrier_info))) => {
                        for (plane_name, (plane_id, pilot_name, plane_type, plane_info)) in &planes {
                            spawn_detect_recovery_attempt(
                                unit.id,
                                unit.name.clone(),
                                carrier_info,
                                *plane_id,
                                plane_name.clone(),
                                plane_type.clone(),
                                plane_info,
                                pilot_name.clone(),
                            );
                        }
                    }
                    Ok(None) => {}
                    Err(err) => {
                        tracing::error!(
                            unit_name = %unit.name,
                            %err,
                            "ignoring unit due to an error while checking its eligibility",
                        );
                    }
                }
            }
        }
        tx_events.send(tonic::Status::aborted("Mission event stream ended").into()).await.ok();
    });

    drop(tx);

    match rx.recv().await {
        Some(err) => Err(err),
        None => Err(tonic::Status::aborted("All tasks finished unexpectedly").into()),
    }

    // This point is reached after shutdown or fatal error; print the greenie board.
}

#[derive(Debug)]
enum Candidate {
    Carrier(&'static CarrierInfo),
    Plane {
        info: &'static AirplaneInfo,
        unit_type: String,
    },
}

async fn check_candidate(
    svc: &mut UnitServiceClient<Channel>,
    unit: &common::v0::Unit,
    include_ki: bool,
) -> Result<Option<Candidate>, Status> {
    let Some(unit_type) = unit.r#type.as_deref() else {
        tracing::debug!(
            unit_id = unit.id,
            unit_name = %unit.name,
            "ignoring unit without a DCS type"
        );
        return Ok(None);
    };

    match GroupCategory::try_from(unit.group.as_ref().map(|g| g.category).unwrap_or(-1)) {
        Ok(GroupCategory::Airplane) if unit.player_name.is_some() || include_ki => {
            return Ok(
                AirplaneInfo::by_type(unit_type).map(|info| Candidate::Plane {
                    info,
                    unit_type: unit_type.to_owned(),
                }),
            );
        }
        Ok(GroupCategory::Ship) => {
            let attrs = svc
                .get_descriptor(unit::v0::GetDescriptorRequest {
                    name: unit.name.clone(),
                })
                .await?
                .into_inner()
                .attributes;

            if attrs
                .iter()
                .any(|a| a.as_str() == "AircraftCarrier With Arresting Gear")
            {
                return Ok(CarrierInfo::by_type(unit_type).map(Candidate::Carrier));
            }
        }
        _ => {}
    }

    Ok(None)
}

/// Print a session greenie board to stdout.
fn print_greenie_board(session_log: &SessionLog) {
    let passes = match session_log.lock() {
        Ok(log) => log.clone(),
        Err(_) => return,
    };
    if passes.is_empty() {
        return;
    }

    println!();
    println!("╔══════════════════════════════════════════════════════════╗");
    println!("║              SESSION GREENIE BOARD                       ║");
    println!("╠═══════════════════════╦══════╦══════╦════════════════════╣");
    println!("║ Pilot                 ║ Wire ║ Grd  ║ DCS Grade          ║");
    println!("╠═══════════════════════╬══════╬══════╬════════════════════╣");
    for pass in &passes {
        let wire = pass
            .wire
            .map(|w| format!("  {}  ", w))
            .unwrap_or_else(|| "  -  ".to_string());
        let dcs = pass
            .dcs_grading
            .as_deref()
            .unwrap_or("-")
            .chars()
            .take(18)
            .collect::<String>();
        println!(
            "║ {:<21} ║{:^6}║ {:<4} ║ {:<18} ║",
            pass.pilot_name.chars().take(21).collect::<String>(),
            wire,
            pass.pass_grade.label(),
            dcs,
        );
    }
    println!("╚═══════════════════════╩══════╩══════╩════════════════════╝");
    println!();
}
