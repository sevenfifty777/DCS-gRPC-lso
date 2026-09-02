use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tokio::task::JoinHandle;

use crate::data::{AirplaneInfo, CarrierInfo};
use crate::db::{RecoveryDb, SharedDb};
use crate::tasks::{
    ActivePriorityPlanes, BaselineManifest, HookSamplingConfig, HookSamplingMode, PilotKind,
    SessionLog, TaskParams,
};
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

type RecoveryTaskMap = Arc<Mutex<RecoveryTaskRegistry>>;
const DCS_GRPC_CLIENT_STUB_VERSION: &str = "0.9.0";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct RecoveryTaskKey {
    session_id: i64,
    generation: u64,
    plane_id: u32,
    carrier_id: u32,
}

struct RegisteredRecoveryTask {
    key: RecoveryTaskKey,
    plane_name: String,
    carrier_name: String,
    handle: JoinHandle<()>,
}

#[derive(Default)]
struct RecoveryTaskRegistry {
    tasks: HashMap<RecoveryTaskKey, RegisteredRecoveryTask>,
}

impl RecoveryTaskRegistry {
    fn register(
        &mut self,
        key: RecoveryTaskKey,
        plane_name: String,
        carrier_name: String,
        handle: JoinHandle<()>,
    ) {
        self.tasks.retain(|_, existing| {
            let same_generation = existing.key.session_id == key.session_id
                && existing.key.generation == key.generation;
            let replaced_plane =
                existing.plane_name == plane_name && existing.key.plane_id != key.plane_id;
            let replaced_carrier =
                existing.carrier_name == carrier_name && existing.key.carrier_id != key.carrier_id;
            let same_pair = existing.key == key;
            let keep = !(same_generation && (replaced_plane || replaced_carrier || same_pair));
            if !keep {
                tracing::debug!(
                    old_plane_id = existing.key.plane_id,
                    old_carrier_id = existing.key.carrier_id,
                    new_plane_id = key.plane_id,
                    new_carrier_id = key.carrier_id,
                    session_id = key.session_id,
                    generation = key.generation,
                    "aborting stale recovery task after unit respawn"
                );
                existing.handle.abort();
            }
            keep
        });
        self.tasks.insert(
            key,
            RegisteredRecoveryTask {
                key,
                plane_name,
                carrier_name,
                handle,
            },
        );
    }

    fn abort_all(&mut self) {
        for (_, task) in self.tasks.drain() {
            task.handle.abort();
        }
    }
}

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

    /// Hook draw-argument polling rate used by the independent sampler.
    #[clap(long, default_value_t = 4, value_parser = clap::value_parser!(u64).range(2..=4))]
    hook_sampling_hz: u64,

    /// Timeout for one hook draw-argument RPC.
    #[clap(long, default_value_t = 300, value_parser = clap::value_parser!(u64).range(250..=300))]
    hook_timeout_ms: u64,

    /// A/B diagnostic mode: restore the old blocking hook read on every position tick.
    #[clap(long)]
    legacy_inline_hook_sampling: bool,

    /// Diagnostic mode: collect transforms and JSON metrics only; disables hook, ACMI, DB, PNG and Discord.
    #[clap(long)]
    positions_only: bool,

    /// Suspend redundant detectors for an aircraft while that aircraft is being collected.
    #[clap(long)]
    suspend_detectors_during_recovery: bool,

    /// JSON manifest describing the DCS build, mission, modules and deployed DLL/Lua hashes.
    #[clap(long)]
    baseline_manifest: Option<PathBuf>,

    /// Port to serve the web greenie board on (e.g. 8080). Disabled if not specified.
    #[clap(long)]
    web_port: Option<u16>,
}

pub async fn execute(
    opts: Opts,
    shutdown_handle: ShutdownHandle,
) -> Result<(), crate::error::Error> {
    if !opts.positions_only && opts.discord_webhook.is_some() {
        tracing::info!("Discord integration enabled.");
    }

    tracing::info!(uri = %opts.uri, "Connecting to gRPC server");

    let users = Arc::new(load_discord_users(&opts).await?);
    let baseline_manifest = Arc::new(if let Some(path) = opts.baseline_manifest.as_deref() {
        let manifest = read_json_file::<BaselineManifest>(path).await?;
        manifest
            .validate()
            .map_err(crate::error::Error::InvalidBaselineManifest)?;
        manifest
    } else {
        BaselineManifest::default()
    });

    let backoff = ExponentialBackoff {
        // never wait longer than 30s for a retry
        max_interval: Duration::from_secs(30),
        // never stop trying
        max_elapsed_time: None,
        ..Default::default()
    };

    let session_log: SessionLog = Arc::new(Mutex::new(Vec::new()));
    let db = open_recovery_db(&opts.out_dir, opts.positions_only)?;
    let generation_counter = Arc::new(AtomicU64::new(0));
    let metrics_started = Instant::now();
    let metrics_shutdown = shutdown_handle.clone();
    let metrics_handle = tokio::spawn(async move {
        let mut ticks = crate::utils::interval::interval(Duration::from_secs(10), metrics_shutdown);
        while ticks.next().await.is_some() {
            crate::metrics::RUNTIME_METRICS.log_snapshot(metrics_started.elapsed().as_secs_f64());
        }
    });

    // Optionally start the web greenie board dashboard.
    if let (Some(port), Some(db)) = (opts.web_port, db.clone()) {
        tokio::spawn(async move {
            if let Err(err) = crate::web::serve(db, port).await {
                tracing::error!(%err, "web dashboard server error");
            }
        });
    } else if opts.web_port.is_some() {
        tracing::warn!("web dashboard disabled in positions-only mode because SQLite is disabled");
    }

    select(
        Box::pin(backoff::future::retry_notify(
            backoff,
            // on each try, run the program and consider every error as transient (ie. worth
            // retrying)
            || async {
                let generation = generation_counter.fetch_add(1, Ordering::Relaxed) + 1;
                run(
                    &opts,
                    users.clone(),
                    shutdown_handle.clone(),
                    session_log.clone(),
                    db.clone(),
                    baseline_manifest.clone(),
                    generation,
                )
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
    metrics_handle.abort();

    if !opts.positions_only {
        print_greenie_board(&session_log);
    }

    Ok(())
}

async fn read_json_file<T: serde::de::DeserializeOwned>(
    path: &std::path::Path,
) -> Result<T, crate::error::Error> {
    let bytes = tokio::fs::read(path)
        .await
        .map_err(|source| crate::error::Error::file_at(path, source))?;
    serde_json::from_slice(&bytes).map_err(|source| crate::error::Error::json_at(path, source))
}

async fn load_discord_users(opts: &Opts) -> Result<HashMap<String, u64>, crate::error::Error> {
    if opts.positions_only {
        return Ok(HashMap::new());
    }
    match opts.discord_users.as_deref() {
        Some(path) => read_json_file(path).await,
        None => Ok(HashMap::new()),
    }
}

async fn run(
    opts: &Opts,
    users: Arc<HashMap<String, u64>>,
    shutdown_handle: ShutdownHandle,
    session_log: SessionLog,
    db: Option<SharedDb>,
    baseline_manifest: Arc<BaselineManifest>,
    generation: u64,
) -> Result<(), crate::error::Error> {
    let out_dir = opts.out_dir.clone();
    let channel = Endpoint::from(opts.uri.clone())
        .connect_timeout(crate::client::RPC_DEADLINE)
        .keep_alive_while_idle(true)
        .connect()
        .await?;
    tracing::info!("Connected");
    let mut coalition_svc = CoalitionServiceClient::new(channel.clone());
    let group_svc = GroupServiceClient::new(channel.clone());
    let mut unit_svc = UnitServiceClient::new(channel.clone());
    let mut mission_svc = MissionServiceClient::new(channel.clone());
    let mut mission_client = crate::client::MissionClient::new(channel.clone());
    let mut metadata_client = crate::client::MetadataClient::new(channel.clone());
    let dcs_grpc_version = match metadata_client.get_version().await {
        Ok(version) => {
            tracing::info!(%version, "DCS-gRPC server version reported");
            version
        }
        Err(err) => {
            tracing::warn!(?err, "DCS-gRPC server version unavailable");
            "unknown".to_string()
        }
    };
    let dcs_grpc_compatibility = dcs_grpc_compatibility(&dcs_grpc_version);
    match dcs_grpc_compatibility {
        "exact" => tracing::info!(
            client = DCS_GRPC_CLIENT_STUB_VERSION,
            server = %dcs_grpc_version,
            "DCS-gRPC client/server versions match"
        ),
        "compatible_same_api_line" => tracing::warn!(
            client = DCS_GRPC_CLIENT_STUB_VERSION,
            server = %dcs_grpc_version,
            "DCS-gRPC patch versions differ; same 0.9 API line accepted pending live validation"
        ),
        compatibility => tracing::warn!(
            client = DCS_GRPC_CLIENT_STUB_VERSION,
            server = %dcs_grpc_version,
            compatibility,
            "DCS-gRPC compatibility is not established"
        ),
    }
    let session_id = match mission_client.get_session_id().await {
        Ok(id) => id,
        Err(err) => {
            tracing::warn!(
                ?err,
                generation,
                "DCS session ID unavailable; using generation fallback"
            );
            -(generation as i64)
        }
    };
    tracing::info!(
        session_id,
        generation,
        "recovery supervisor generation started"
    );

    // initial full-sync of all current units inside of the mission
    let groups = coalition_svc
        .get_groups(crate::client::request_with_deadline(
            coalition::v0::GetGroupsRequest {
                coalition: Coalition::All.into(),
                category: 0,
            },
        ))
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
                        .get_units(crate::client::request_with_deadline(
                            group::v0::GetUnitsRequest {
                                group_name: group.name,
                                active: Some(true),
                            },
                        ))
                        .map_ok(|res| res.into_inner().units)
                        .await
                }
            }),
    )
    .await?;

    let planes: Arc<Mutex<HashMap<String, PlaneCandidate>>> = Arc::new(Mutex::new(HashMap::new()));
    let carriers: Arc<Mutex<HashMap<String, CarrierCandidate>>> =
        Arc::new(Mutex::new(HashMap::new()));
    let mut net_client = crate::client::NetClient::new(channel.clone());
    let players = match net_client.get_players().await {
        Ok(players) => players,
        Err(err) => {
            tracing::warn!(?err, "initial UCID/slot snapshot unavailable");
            Vec::new()
        }
    };

    for units in group_units {
        for unit in units {
            match check_candidate(&mut unit_svc, &unit, opts.include_ki).await? {
                Some(Candidate::Plane(plane_info)) => {
                    planes.lock().expect("planes mutex poisoned").insert(
                        unit.name.clone(),
                        plane_candidate(&unit, plane_info, &players, session_id),
                    );
                }
                Some(Candidate::Carrier(carrier_info)) => {
                    carriers.lock().expect("carriers mutex poisoned").insert(
                        unit.name,
                        CarrierCandidate {
                            id: unit.id,
                            carrier_type: unit.r#type.unwrap_or_default(),
                            carrier_info,
                        },
                    );
                }
                None => {}
            }
        }
    }

    let (tx, mut rx) = mpsc::channel(16);

    // Tracks the active detect_recovery_attempt task for each (plane_id, carrier_id) pair.
    // When a Birth event re-spawns a known unit the old task is aborted before a new one starts,
    // preventing duplicate recordings.
    let active_tasks: RecoveryTaskMap = Arc::new(Mutex::new(RecoveryTaskRegistry::default()));
    let active_priority_planes = Arc::new(ActivePriorityPlanes::default());

    let positions_only = opts.positions_only;
    let discord_webhook = if positions_only {
        None
    } else {
        opts.discord_webhook.clone()
    };
    let record_acmi = !opts.no_acmi && !positions_only;
    let suspend_detectors_during_recovery =
        opts.suspend_detectors_during_recovery || positions_only;
    let hook_sampling = HookSamplingConfig {
        mode: if positions_only {
            HookSamplingMode::Disabled
        } else if opts.legacy_inline_hook_sampling {
            HookSamplingMode::LegacyInline
        } else {
            HookSamplingMode::Independent
        },
        frequency_hz: opts.hook_sampling_hz,
        timeout: Duration::from_millis(opts.hook_timeout_ms),
    };
    let active_tasks2 = active_tasks.clone();
    let priority_planes = active_priority_planes.clone();
    let baseline_manifest_for_tasks = baseline_manifest.clone();
    let session_channel = channel.clone();
    let session_shutdown = shutdown_handle.clone();
    let dcs_grpc_version = dcs_grpc_version.clone();
    let dcs_grpc_compatibility = dcs_grpc_compatibility.to_string();
    let spawn_detect_recovery_attempt =
        move |carrier_id: u32,
              carrier_name: String,
              carrier_type: String,
              carrier_info: &'static CarrierInfo,
              plane_id: u32,
              plane_name: String,
              plane_type: String,
              plane_info: &'static AirplaneInfo,
              pilot_name: String,
              pilot_kind: PilotKind,
              pilot_identity: String,
              pilot_ucid: Option<String>| {
            if !carrier_info.supports_aircraft_type(&plane_type) {
                tracing::trace!(%carrier_name, %plane_name, %plane_type, "unsupported recovery pair rejected");
                return;
            }
            let out_dir = out_dir.clone();
            let discord_webhook = discord_webhook.clone();
            let record_acmi = record_acmi;
            let hook_sampling = hook_sampling;
            let users = users.clone();
            let channel = channel.clone();
            let shutdown_handle = shutdown_handle.clone();
            let session_log = session_log.clone();
            let db = db.clone();
            let dcs_grpc_version = dcs_grpc_version.clone();
            let dcs_grpc_compatibility = dcs_grpc_compatibility.clone();
            let active_priority_planes = priority_planes.clone();
            let baseline_manifest = baseline_manifest_for_tasks.clone();
            let registry_plane_name = plane_name.clone();
            let registry_carrier_name = carrier_name.clone();
            let handle = tokio::spawn(async move {
                if let Err(err) =
                    crate::tasks::detect_recovery_attempt::detect_recovery_attempt(TaskParams {
                        out_dir: &out_dir,
                        discord_webhook,
                        record_acmi,
                        hook_sampling,
                        users,
                        ch: channel,
                        carrier_id,
                        carrier_name: &carrier_name,
                        carrier_type: &carrier_type,
                        plane_id,
                        plane_name: &plane_name,
                        plane_type: &plane_type,
                        pilot_name: &pilot_name,
                        pilot_kind,
                        pilot_identity: &pilot_identity,
                        pilot_ucid,
                        carrier_info,
                        plane_info,
                        shutdown: shutdown_handle,
                        session_log,
                        db,
                        session_id,
                        generation,
                        dcs_grpc_version: &dcs_grpc_version,
                        dcs_grpc_compatibility: &dcs_grpc_compatibility,
                        positions_only,
                        suspend_detectors_during_recovery,
                        active_priority_planes,
                        baseline_manifest,
                    })
                    .await
                {
                    tracing::error!(%err, plane_id, carrier_id, session_id, generation, "recovery pair stopped after isolated error");
                }
            });
            if let Ok(mut registry) = active_tasks2.lock() {
                registry.register(
                    RecoveryTaskKey {
                        session_id,
                        generation,
                        plane_id,
                        carrier_id,
                    },
                    registry_plane_name,
                    registry_carrier_name,
                    handle,
                );
            }
        };

    let carrier_snapshot = carriers.lock().expect("carriers mutex poisoned").clone();
    let plane_snapshot = planes.lock().expect("planes mutex poisoned").clone();
    for (carrier_name, carrier) in &carrier_snapshot {
        for (plane_name, plane) in &plane_snapshot {
            spawn_detect_recovery_attempt(
                carrier.id,
                carrier_name.clone(),
                carrier.carrier_type.clone(),
                carrier.carrier_info,
                plane.id,
                plane_name.clone(),
                plane.plane_type.clone(),
                plane.plane_info,
                plane.pilot_name.clone(),
                plane.pilot_kind,
                plane.pilot_identity.clone(),
                plane.pilot_ucid.clone(),
            );
        }
    }

    // listen for birth events to track carriers and planes spawned at a later point in time
    let mut events = mission_svc
        .stream_events(crate::client::request_with_deadline(
            mission::v0::StreamEventsRequest {},
        ))
        .await?
        .into_inner();
    let tx_events = tx.clone();
    let include_ki = opts.include_ki;
    let event_handle = tokio::spawn(async move {
        let _stream_guard = crate::metrics::RUNTIME_METRICS.stream();
        while let Some(event) = events.next().await {
            let event = match event {
                Ok(stubs::mission::v0::StreamEventsResponse {
                    event: Some(event), ..
                }) => event,
                Ok(_) => continue,
                Err(err) => {
                    crate::metrics::RUNTIME_METRICS.observe_queue_depth(
                        tx_events
                            .max_capacity()
                            .saturating_sub(tx_events.capacity())
                            + 1,
                    );
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
                    Ok(Some(Candidate::Plane(plane_info))) => {
                        let players = match net_client.get_players().await {
                            Ok(players) => players,
                            Err(err) => {
                                tracing::warn!(
                                    ?err,
                                    unit_id = unit.id,
                                    "UCID/slot snapshot unavailable at birth"
                                );
                                Vec::new()
                            }
                        };
                        let plane = plane_candidate(&unit, plane_info, &players, session_id);
                        planes
                            .lock()
                            .expect("planes mutex poisoned")
                            .insert(unit.name.clone(), plane.clone());
                        let carrier_snapshot =
                            carriers.lock().expect("carriers mutex poisoned").clone();
                        for (carrier_name, carrier) in carrier_snapshot {
                            spawn_detect_recovery_attempt(
                                carrier.id,
                                carrier_name,
                                carrier.carrier_type,
                                carrier.carrier_info,
                                plane.id,
                                unit.name.clone(),
                                plane.plane_type.clone(),
                                plane.plane_info,
                                plane.pilot_name.clone(),
                                plane.pilot_kind,
                                plane.pilot_identity.clone(),
                                plane.pilot_ucid.clone(),
                            );
                        }
                    }
                    Ok(Some(Candidate::Carrier(carrier_info))) => {
                        let carrier = CarrierCandidate {
                            id: unit.id,
                            carrier_type: unit.r#type.clone().unwrap_or_default(),
                            carrier_info,
                        };
                        carriers
                            .lock()
                            .expect("carriers mutex poisoned")
                            .insert(unit.name.clone(), carrier.clone());
                        let plane_snapshot = planes.lock().expect("planes mutex poisoned").clone();
                        for (plane_name, plane) in plane_snapshot {
                            spawn_detect_recovery_attempt(
                                carrier.id,
                                unit.name.clone(),
                                carrier.carrier_type.clone(),
                                carrier.carrier_info,
                                plane.id,
                                plane_name,
                                plane.plane_type,
                                plane.plane_info,
                                plane.pilot_name,
                                plane.pilot_kind,
                                plane.pilot_identity,
                                plane.pilot_ucid,
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
        crate::metrics::RUNTIME_METRICS.observe_queue_depth(
            tx_events
                .max_capacity()
                .saturating_sub(tx_events.capacity())
                + 1,
        );
        tx_events
            .send(tonic::Status::aborted("Mission event stream ended").into())
            .await
            .ok();
    });

    let tx_session = tx.clone();
    let session_watchdog = tokio::spawn(async move {
        let mut client = crate::client::MissionClient::new(session_channel);
        let mut ticks = crate::utils::interval::interval(Duration::from_secs(1), session_shutdown);
        let mut last_success = Instant::now();
        while ticks.next().await.is_some() {
            match client.get_session_id().await {
                Ok(current) if current != session_id => {
                    let status = tonic::Status::aborted(format!(
                        "DCS session changed from {session_id} to {current}"
                    ));
                    crate::metrics::RUNTIME_METRICS.observe_queue_depth(
                        tx_session
                            .max_capacity()
                            .saturating_sub(tx_session.capacity())
                            + 1,
                    );
                    tx_session.send(status.into()).await.ok();
                    return;
                }
                Ok(_) => last_success = Instant::now(),
                Err(err) => {
                    let silent_for = last_success.elapsed();
                    tracing::warn!(
                        ?err,
                        ?silent_for,
                        session_id,
                        "session watchdog query failed"
                    );
                    if silent_for >= Duration::from_millis(crate::telemetry::ACTIVE_WATCHDOG_MS) {
                        let status = tonic::Status::unavailable(format!(
                            "DCS-gRPC channel silent for {:.3}s",
                            silent_for.as_secs_f64()
                        ));
                        crate::metrics::RUNTIME_METRICS.observe_queue_depth(
                            tx_session
                                .max_capacity()
                                .saturating_sub(tx_session.capacity())
                                + 1,
                        );
                        tx_session.send(status.into()).await.ok();
                        return;
                    }
                }
            }
        }
    });

    drop(tx);

    let result = match rx.recv().await {
        Some(err) => Err(err),
        None => Err(tonic::Status::aborted("All tasks finished unexpectedly").into()),
    };
    if let Ok(mut tasks) = active_tasks.lock() {
        tasks.abort_all();
    }
    event_handle.abort();
    session_watchdog.abort();
    result

    // This point is reached after shutdown or fatal error; print the greenie board.
}

fn dcs_grpc_compatibility(server: &str) -> &'static str {
    if server == DCS_GRPC_CLIENT_STUB_VERSION {
        return "exact";
    }
    let api_line = |version: &str| {
        let mut parts = version.trim_start_matches('v').split('.');
        Some((
            parts.next()?.parse::<u64>().ok()?,
            parts.next()?.parse::<u64>().ok()?,
        ))
    };
    match (api_line(DCS_GRPC_CLIENT_STUB_VERSION), api_line(server)) {
        (Some(client), Some(server)) if client == server => "compatible_same_api_line",
        (Some(_), Some(_)) => "incompatible_api_line",
        _ => "unknown",
    }
}

fn open_recovery_db(
    out_dir: &std::path::Path,
    positions_only: bool,
) -> Result<Option<SharedDb>, crate::error::Error> {
    if positions_only {
        Ok(None)
    } else {
        let path = out_dir.join("lso.db");
        RecoveryDb::open(&path)
            .map(|db| Some(Arc::new(db)))
            .map_err(|source| crate::error::Error::db_at(path, source))
    }
}

#[derive(Debug, Clone)]
struct PlaneCandidate {
    id: u32,
    pilot_name: String,
    plane_type: String,
    plane_info: &'static AirplaneInfo,
    pilot_kind: PilotKind,
    pilot_identity: String,
    pilot_ucid: Option<String>,
}

fn plane_candidate(
    unit: &common::v0::Unit,
    plane_info: &'static AirplaneInfo,
    players: &[stubs::net::v0::get_players_response::GetPlayerInfo],
    session_id: i64,
) -> PlaneCandidate {
    // Slot is the only network field tied to the occupied DCS unit. Deliberately
    // avoid name-only matching: duplicate display names are legal and must not
    // exchange UCIDs.
    let unit_id = unit.id.to_string();
    let player = players
        .iter()
        .find(|player| player.slot == unit_id || player.slot == unit.name);
    let pilot_kind = if unit.player_name.is_some() || player.is_some() {
        PilotKind::Human
    } else {
        PilotKind::Ai
    };
    let pilot_name = player
        .map(|player| player.name.trim())
        .filter(|name| !name.is_empty())
        .map(ToOwned::to_owned)
        .or_else(|| unit.player_name.clone())
        .unwrap_or_else(|| unit.name.clone());
    let pilot_ucid = player
        .map(|player| player.ucid.trim())
        .filter(|ucid| !ucid.is_empty())
        .map(ToOwned::to_owned);
    let pilot_identity = match (pilot_kind, pilot_ucid.as_deref()) {
        (PilotKind::Human, Some(ucid)) => ucid.to_string(),
        (PilotKind::Human, None) => format!("human-unresolved:{session_id}:{}", unit.id),
        (PilotKind::Ai, _) => format!("ai:{session_id}:{}", unit.id),
    };

    PlaneCandidate {
        id: unit.id,
        pilot_name,
        plane_type: unit.r#type.clone().unwrap_or_default(),
        plane_info,
        pilot_kind,
        pilot_identity,
        pilot_ucid,
    }
}

#[derive(Debug, Clone)]
struct CarrierCandidate {
    id: u32,
    carrier_type: String,
    carrier_info: &'static CarrierInfo,
}

#[derive(Debug)]
enum Candidate {
    Carrier(&'static CarrierInfo),
    Plane(&'static AirplaneInfo),
}

async fn check_candidate(
    svc: &mut UnitServiceClient<Channel>,
    unit: &common::v0::Unit,
    include_ki: bool,
) -> Result<Option<Candidate>, Box<Status>> {
    match GroupCategory::try_from(unit.group.as_ref().map(|g| g.category).unwrap_or(-1)) {
        Ok(GroupCategory::Airplane) if unit.player_name.is_some() || include_ki => {
            return Ok(unit
                .r#type
                .as_deref()
                .and_then(AirplaneInfo::by_type)
                .map(Candidate::Plane))
        }
        Ok(GroupCategory::Ship) => {
            let attrs = svc
                .get_descriptor(crate::client::request_with_deadline(
                    unit::v0::GetDescriptorRequest {
                        name: unit.name.clone(),
                    },
                ))
                .await
                .map_err(Box::new)?
                .into_inner()
                .attributes;

            if attrs.iter().any(|a| {
                matches!(
                    a.as_str(),
                    "AircraftCarrier With Arresting Gear" | "AircraftCarrier With Tramplin"
                )
            }) {
                return Ok(unit
                    .r#type
                    .as_deref()
                    .and_then(CarrierInfo::by_type)
                    .map(Candidate::Carrier));
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

    // Preserve the native CATOBAR board when the session contains only
    // arrested recoveries.  V/STOL sessions relabel the same column to Spot;
    // a mixed session uses W/S without changing the table width.
    let has_spot = passes.iter().any(|p| p.spot.is_some());
    let has_wire = passes.iter().any(|p| p.spot.is_none());
    let point_header = match (has_wire, has_spot) {
        (true, true) => "W/S",
        (false, true) => "Spot",
        _ => "Wire",
    };

    println!();
    println!("╔══════════════════════════════════════════════════════════╗");
    println!("║              SESSION GREENIE BOARD                       ║");
    println!("╠═══════════════════════╦══════╦══════╦════════════════════╣");
    println!(
        "║ Pilot                 ║ {:^4} ║ Grd  ║ DCS Grade          ║",
        point_header
    );
    println!("╠═══════════════════════╬══════╬══════╬════════════════════╣");
    for pass in &passes {
        let recovery_point = pass
            .spot
            .clone()
            .or_else(|| pass.wire.map(|w| w.to_string()))
            .unwrap_or_else(|| "-".to_string());
        let dcs = pass
            .dcs_grading
            .as_deref()
            .unwrap_or("-")
            .chars()
            .take(18)
            .collect::<String>();
        println!(
            "║ {:<21} ║ {:^4} ║ {:<4} ║ {:<18} ║",
            pass.pilot_name.chars().take(21).collect::<String>(),
            recovery_point,
            pass.pass_grade.label(),
            dcs,
        );
    }
    println!("╚═══════════════════════╩══════╩══════╩════════════════════╝");
    println!();
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;
    use stubs::net::v0::get_players_response::GetPlayerInfo;

    fn unit(id: u32, name: &str, player_name: Option<&str>) -> common::v0::Unit {
        common::v0::Unit {
            id,
            name: name.to_string(),
            r#type: Some("FA-18C_hornet".to_string()),
            player_name: player_name.map(ToOwned::to_owned),
            ..Default::default()
        }
    }

    #[test]
    fn homonyms_are_resolved_by_slot_not_display_name() {
        let players = [
            GetPlayerInfo {
                name: "SameName".to_string(),
                ucid: "ucid-a".to_string(),
                slot: "101".to_string(),
                ..Default::default()
            },
            GetPlayerInfo {
                name: "SameName".to_string(),
                ucid: "ucid-b".to_string(),
                slot: "102".to_string(),
                ..Default::default()
            },
        ];
        let info = AirplaneInfo::by_type("FA-18C_hornet").unwrap();

        let first = plane_candidate(&unit(101, "Hornet-1", Some("SameName")), info, &players, 7);
        let second = plane_candidate(&unit(102, "Hornet-2", Some("SameName")), info, &players, 7);

        assert_eq!(first.pilot_ucid.as_deref(), Some("ucid-a"));
        assert_eq!(second.pilot_ucid.as_deref(), Some("ucid-b"));
        assert_ne!(first.pilot_identity, second.pilot_identity);
    }

    #[test]
    fn ai_and_respawns_get_session_scoped_internal_identities() {
        let info = AirplaneInfo::by_type("FA-18C_hornet").unwrap();
        let ai = plane_candidate(&unit(200, "AI-1", None), info, &[], 7);
        let respawn = plane_candidate(&unit(201, "AI-1", None), info, &[], 7);

        assert_eq!(ai.pilot_kind, PilotKind::Ai);
        assert!(ai.pilot_ucid.is_none());
        assert_ne!(ai.pilot_identity, respawn.pilot_identity);
    }

    #[test]
    fn human_slot_change_keeps_ucid_but_changes_unit_correlation() {
        let info = AirplaneInfo::by_type("FA-18C_hornet").unwrap();
        let first_players = [GetPlayerInfo {
            name: "Pilot".to_string(),
            ucid: "stable-ucid".to_string(),
            slot: "301".to_string(),
            ..Default::default()
        }];
        let second_players = [GetPlayerInfo {
            name: "Pilot".to_string(),
            ucid: "stable-ucid".to_string(),
            slot: "302".to_string(),
            ..Default::default()
        }];
        let first = plane_candidate(
            &unit(301, "Hornet-1", Some("Pilot")),
            info,
            &first_players,
            7,
        );
        let second = plane_candidate(
            &unit(302, "Hornet-2", Some("Pilot")),
            info,
            &second_players,
            7,
        );

        assert_eq!(first.pilot_identity, second.pilot_identity);
        assert_ne!(first.id, second.id);
    }

    #[test]
    fn no_acmi_and_hook_ab_configuration_are_accepted() {
        let opts = Opts::try_parse_from([
            "lso-run",
            "--no-acmi",
            "--hook-sampling-hz",
            "2",
            "--hook-timeout-ms",
            "250",
            "--legacy-inline-hook-sampling",
        ])
        .expect("valid run options");
        assert!(opts.no_acmi);
        assert_eq!(opts.hook_sampling_hz, 2);
        assert_eq!(opts.hook_timeout_ms, 250);
        assert!(opts.legacy_inline_hook_sampling);
    }

    #[test]
    fn positions_only_and_detector_suspension_are_explicit_modes() {
        let opts = Opts::try_parse_from([
            "lso-run",
            "--positions-only",
            "--suspend-detectors-during-recovery",
        ])
        .expect("valid diagnostic options");
        assert!(opts.positions_only);
        assert!(opts.suspend_detectors_during_recovery);
    }

    #[test]
    fn dcs_grpc_compatibility_is_checked_by_api_line() {
        assert_eq!(dcs_grpc_compatibility("0.9.0"), "exact");
        assert_eq!(dcs_grpc_compatibility("0.9.1"), "compatible_same_api_line");
        assert_eq!(dcs_grpc_compatibility("0.10.0"), "incompatible_api_line");
        assert_eq!(dcs_grpc_compatibility("unknown"), "unknown");
    }

    #[test]
    fn positions_only_does_not_open_or_create_sqlite() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let out_dir = std::env::temp_dir().join(format!("lso-positions-only-{unique}"));
        let db = open_recovery_db(&out_dir, true).expect("positions-only database selection");
        assert!(db.is_none());
        assert!(!out_dir.join("lso.db").exists());
    }

    #[tokio::test]
    async fn airplane_respawn_aborts_every_old_id_pair_for_the_same_name() {
        let mut registry = RecoveryTaskRegistry::default();
        for carrier_id in [20, 21] {
            registry.register(
                RecoveryTaskKey {
                    session_id: 7,
                    generation: 3,
                    plane_id: 10,
                    carrier_id,
                },
                "Hornet-1".to_string(),
                format!("Carrier-{carrier_id}"),
                tokio::spawn(std::future::pending()),
            );
        }
        registry.register(
            RecoveryTaskKey {
                session_id: 7,
                generation: 3,
                plane_id: 11,
                carrier_id: 20,
            },
            "Hornet-1".to_string(),
            "Carrier-20".to_string(),
            tokio::spawn(std::future::pending()),
        );

        assert_eq!(registry.tasks.len(), 1);
        assert!(registry.tasks.keys().all(|key| key.plane_id == 11));
        registry.abort_all();
    }

    #[tokio::test]
    async fn carrier_respawn_aborts_every_old_id_pair_for_the_same_name() {
        let mut registry = RecoveryTaskRegistry::default();
        for plane_id in [10, 11] {
            registry.register(
                RecoveryTaskKey {
                    session_id: 7,
                    generation: 3,
                    plane_id,
                    carrier_id: 20,
                },
                format!("Plane-{plane_id}"),
                "CVN-71".to_string(),
                tokio::spawn(std::future::pending()),
            );
        }
        registry.register(
            RecoveryTaskKey {
                session_id: 7,
                generation: 3,
                plane_id: 10,
                carrier_id: 22,
            },
            "Plane-10".to_string(),
            "CVN-71".to_string(),
            tokio::spawn(std::future::pending()),
        );

        assert_eq!(registry.tasks.len(), 1);
        assert!(registry.tasks.keys().all(|key| key.carrier_id == 22));
        registry.abort_all();
    }

    #[tokio::test]
    async fn respawn_cleanup_never_crosses_session_or_generation() {
        let mut registry = RecoveryTaskRegistry::default();
        for generation in [3, 4] {
            registry.register(
                RecoveryTaskKey {
                    session_id: 7,
                    generation,
                    plane_id: 10 + generation as u32,
                    carrier_id: 20,
                },
                "Hornet-1".to_string(),
                "CVN-71".to_string(),
                tokio::spawn(std::future::pending()),
            );
        }
        assert_eq!(registry.tasks.len(), 2);
        registry.abort_all();
    }

    #[tokio::test]
    async fn positions_only_ignores_missing_and_invalid_discord_user_files() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let missing = std::env::temp_dir().join(format!("missing-discord-{unique}.json"));
        let opts = Opts::try_parse_from([
            "lso-run",
            "--positions-only",
            "--discord-users",
            missing.to_str().unwrap(),
        ])
        .unwrap();
        assert!(load_discord_users(&opts).await.unwrap().is_empty());

        let invalid = std::env::temp_dir().join(format!("invalid-discord-{unique}.json"));
        std::fs::write(&invalid, b"not-json").unwrap();
        let opts = Opts::try_parse_from([
            "lso-run",
            "--positions-only",
            "--discord-users",
            invalid.to_str().unwrap(),
        ])
        .unwrap();
        assert!(load_discord_users(&opts).await.unwrap().is_empty());
        std::fs::remove_file(invalid).unwrap();
    }

    #[tokio::test]
    async fn manifest_errors_include_path_json_location_and_unknown_key() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("lso-manifest-errors-{unique}"));
        std::fs::create_dir(&dir).unwrap();
        let missing = dir.join("missing.json");
        let error = read_json_file::<BaselineManifest>(&missing)
            .await
            .unwrap_err()
            .to_string();
        assert!(error.contains("missing.json"));
        assert!(error.contains("I/O error"));

        let invalid = dir.join("invalid.json");
        std::fs::write(&invalid, b"{\n  \"mission\":\n}").unwrap();
        let error = read_json_file::<BaselineManifest>(&invalid)
            .await
            .unwrap_err()
            .to_string();
        assert!(error.contains("invalid.json"));
        assert!(error.contains("line 3"));
        assert!(error.contains("column"));

        let unknown = dir.join("unknown.json");
        std::fs::write(&unknown, b"{\"misson\":\"cq.miz\"}").unwrap();
        let error = read_json_file::<BaselineManifest>(&unknown)
            .await
            .unwrap_err()
            .to_string();
        assert!(error.contains("unknown.json"));
        assert!(error.contains("unknown field"));
        std::fs::remove_dir_all(dir).unwrap();
    }
}
