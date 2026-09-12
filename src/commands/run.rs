use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::data::{AirplaneInfo, CarrierInfo};
use crate::db::{RecoveryDb, SharedDb};
use crate::tasks::{
    CarrierCandidate, HookSamplingConfig, HookSamplingMode, PilotKind, PlaneCandidate,
    RecoveryContext, RecoveryTelemetryMode, SessionLog, SharedRegistry, UnitRegistry,
    EVENT_FANOUT_CAPACITY,
};
use crate::utils::lock_unpoisoned;
use crate::utils::shutdown::ShutdownHandle;
use backoff::ExponentialBackoff;
use futures_util::future::{select, Either};
use futures_util::{StreamExt, TryFutureExt};
use stubs::coalition::v0::coalition_service_client::CoalitionServiceClient;
use stubs::common::v0::{Coalition, GroupCategory};
use stubs::group::v0::group_service_client::GroupServiceClient;
use stubs::mission::v0::stream_events_response::Event;
use stubs::unit::v0::unit_service_client::UnitServiceClient;
use stubs::{coalition, common, group, mission, unit};
use tokio::sync::{broadcast, mpsc};
use tonic::transport::{Channel, Endpoint, Uri};
use tonic::Status;

/// A generation that stayed connected at least this long is considered
/// healthy; when it later fails, reconnection starts from a fresh backoff
/// instead of the 30 s ceiling accumulated by earlier failures.
const HEALTHY_UPTIME: Duration = Duration::from_secs(60);

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

    /// Hook draw-argument polling rate used by the independent sampler (legacy acquisition only).
    #[clap(long, default_value_t = 4, value_parser = clap::value_parser!(u64).range(2..=4))]
    hook_sampling_hz: u64,

    /// Timeout for one hook draw-argument RPC (legacy acquisition only).
    #[clap(long, default_value_t = 300, value_parser = clap::value_parser!(u64).range(250..=300))]
    hook_timeout_ms: u64,

    /// A/B diagnostic mode: restore the old blocking hook read on every position tick.
    #[clap(long)]
    legacy_inline_hook_sampling: bool,

    /// Recovery telemetry source. Auto uses the atomic API when supported and
    /// falls back to the legacy transform calls only on UNIMPLEMENTED.
    #[clap(long, value_enum, default_value_t = RecoveryTelemetryMode::Auto)]
    recovery_telemetry_mode: RecoveryTelemetryMode,

    /// Timeout for one atomic recovery snapshot. Must remain below the 300 ms
    /// stale-observation threshold.
    #[clap(long, default_value_t = 250, value_parser = clap::value_parser!(u64).range(100..=299))]
    recovery_snapshot_timeout_ms: u64,

    /// Poll HookService.GetOwnshipHookState during CATOBAR recoveries as a
    /// diagnostic. It only returns data on a client DCS instance with a local
    /// cockpit; on a dedicated server it is always unavailable.
    #[clap(long)]
    ownship_hook_diagnostics: bool,

    /// Removed in 0.4.0: the greenie board is now the LSO page of the DCS Web
    /// Dashboard, which reads `<out-dir>/lso.db` directly. The flag stays
    /// hidden for one release so an old service definition fails with a clear
    /// message instead of clap's "unexpected argument".
    #[clap(long, hide = true)]
    web_port: Option<u16>,

    /// Removed in 0.4.0 together with the web server.
    #[clap(long, hide = true)]
    web_expose_ucid: bool,
}

/// The loopback web board was removed in 0.4.0. Refuse its flags loudly rather
/// than silently running without the board the operator expected.
fn reject_removed_web_flags(opts: &Opts) -> Result<(), crate::error::Error> {
    if opts.web_port.is_some() || opts.web_expose_ucid {
        return Err(crate::error::Error::RemovedOption(
            "--web-port and --web-expose-ucid were removed in LSO 0.4.0: the greenie board is now \
             the LSO page of the DCS Web Dashboard (set its LSO_DIR to this --out-dir)",
        ));
    }
    Ok(())
}

pub async fn execute(
    opts: Opts,
    shutdown_handle: ShutdownHandle,
) -> Result<(), crate::error::Error> {
    reject_removed_web_flags(&opts)?;
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

    let session_log: SessionLog = Arc::new(Mutex::new(Vec::new()));
    let db: SharedDb = Arc::new(RecoveryDb::open(&opts.out_dir.join("lso.db"))?);
    let generation_counter = Arc::new(AtomicU64::new(0));
    let metrics_started = Instant::now();
    let metrics_shutdown = shutdown_handle.clone();
    let metrics_handle = tokio::spawn(async move {
        let mut ticks = crate::utils::interval::interval(Duration::from_secs(10), metrics_shutdown);
        while ticks.next().await.is_some() {
            crate::metrics::RUNTIME_METRICS.log_snapshot(metrics_started.elapsed().as_secs_f64());
        }
    });

    loop {
        let backoff = ExponentialBackoff {
            // never wait longer than 30s for a retry
            max_interval: Duration::from_secs(30),
            // never stop trying
            max_elapsed_time: None,
            ..Default::default()
        };
        let attempt = backoff::future::retry_notify(
            backoff,
            // on each try, run the program; failures of a young connection are transient
            // (retry with growing delay), failures after a healthy uptime restart the
            // outer loop with a fresh backoff.
            || async {
                let generation = generation_counter.fetch_add(1, Ordering::Relaxed) + 1;
                let started = Instant::now();
                match run(
                    &opts,
                    users.clone(),
                    shutdown_handle.clone(),
                    session_log.clone(),
                    db.clone(),
                    generation,
                )
                .await
                {
                    Ok(()) => Ok(()),
                    Err(err) if started.elapsed() >= HEALTHY_UPTIME => {
                        Err(backoff::Error::Permanent(err))
                    }
                    Err(err) => Err(backoff::Error::transient(err)),
                }
            },
            // error hook:
            |err, backoff: Duration| {
                tracing::debug!(
                    %err,
                    backoff = %format!("{:.2}s", backoff.as_secs_f64()),
                    "retrying after error"
                );
            },
        );
        match select(Box::pin(attempt), shutdown_handle.signal()).await {
            Either::Left((Err(err), _)) => {
                tracing::info!(%err, "connection lost after a healthy uptime; reconnecting");
                continue;
            }
            Either::Left((Ok(()), _)) | Either::Right(_) => break,
        }
    }
    metrics_handle.abort();

    print_greenie_board(&session_log);

    Ok(())
}

async fn run(
    opts: &Opts,
    users: Arc<HashMap<String, u64>>,
    shutdown_handle: ShutdownHandle,
    session_log: SessionLog,
    db: SharedDb,
    generation: u64,
) -> Result<(), crate::error::Error> {
    let channel = Endpoint::from(opts.uri.clone())
        .connect_timeout(crate::client::RPC_DEADLINE)
        .keep_alive_while_idle(true)
        .connect()
        .await?;
    tracing::info!("Connected");
    let mut coalition_svc = CoalitionServiceClient::new(channel.clone());
    let group_svc = GroupServiceClient::new(channel.clone());
    let mut unit_svc = UnitServiceClient::new(channel.clone());
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

    let registry: SharedRegistry = Arc::new(Mutex::new(UnitRegistry::default()));
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
                    let plane = plane_candidate(&unit, plane_info, &players, session_id);
                    lock_unpoisoned(&registry).planes.insert(unit.id, plane);
                }
                Some(Candidate::Carrier(carrier_info)) => {
                    lock_unpoisoned(&registry).carriers.insert(
                        unit.id,
                        CarrierCandidate {
                            id: unit.id,
                            name: unit.name,
                            carrier_type: unit.r#type.unwrap_or_default(),
                            carrier_info,
                        },
                    );
                }
                None => {}
            }
        }
    }
    {
        let registry = lock_unpoisoned(&registry);
        tracing::info!(
            planes = registry.planes.len(),
            carriers = registry.carriers.len(),
            "initial unit discovery complete"
        );
    }

    let (tx, mut rx) = mpsc::channel(16);
    let (event_tx, _event_rx) = broadcast::channel(EVENT_FANOUT_CAPACITY);

    let context = Arc::new(RecoveryContext {
        out_dir: opts.out_dir.clone(),
        discord_webhook: opts.discord_webhook.clone(),
        record_acmi: !opts.no_acmi,
        hook_sampling: HookSamplingConfig {
            mode: if opts.legacy_inline_hook_sampling {
                HookSamplingMode::LegacyInline
            } else {
                HookSamplingMode::Independent
            },
            frequency_hz: opts.hook_sampling_hz,
            timeout: Duration::from_millis(opts.hook_timeout_ms),
        },
        recovery_telemetry_mode: opts.recovery_telemetry_mode,
        recovery_snapshot_timeout: Duration::from_millis(opts.recovery_snapshot_timeout_ms),
        ownship_hook_diagnostics: opts.ownship_hook_diagnostics,
        users,
        ch: channel.clone(),
        shutdown: shutdown_handle.clone(),
        session_log,
        db,
        session_id,
        generation,
        dcs_grpc_version,
        events: event_tx.clone(),
        fatal: tx.clone(),
    });

    // Single detection supervisor for every known plane/carrier.
    let supervisor_registry = registry.clone();
    let supervisor_context = context.clone();
    let tx_supervisor = tx.clone();
    let supervisor_handle = tokio::spawn(async move {
        if let Err(err) = crate::tasks::detect_recovery_attempt::supervise_recoveries(
            supervisor_registry,
            supervisor_context,
        )
        .await
        {
            tx_supervisor.send(err).await.ok();
        }
    });

    // The single mission event stream: fans every event out to the active
    // recorders and registers carriers and planes spawned later.
    let mut events = Box::pin(mission_client.stream_events().await?);
    let tx_events = tx.clone();
    let include_ki = opts.include_ki;
    let event_registry = registry.clone();
    let event_handle = tokio::spawn(async move {
        let _stream_guard = crate::metrics::RUNTIME_METRICS.stream();
        while let Some(event) = events.next().await {
            let (time, event) = match event {
                Ok(event) => event,
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
            }) = &event
            {
                match check_candidate(&mut unit_svc, unit, include_ki).await {
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
                        let plane = plane_candidate(unit, plane_info, &players, session_id);
                        tracing::debug!(unit_id = unit.id, name = %unit.name, "plane registered at birth");
                        lock_unpoisoned(&event_registry)
                            .planes
                            .insert(unit.id, plane);
                    }
                    Ok(Some(Candidate::Carrier(carrier_info))) => {
                        tracing::debug!(unit_id = unit.id, name = %unit.name, "carrier registered at birth");
                        lock_unpoisoned(&event_registry).carriers.insert(
                            unit.id,
                            CarrierCandidate {
                                id: unit.id,
                                name: unit.name.clone(),
                                carrier_type: unit.r#type.clone().unwrap_or_default(),
                                carrier_info,
                            },
                        );
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

            // No receiver is fine: nothing is being recorded right now.
            let _ = event_tx.send(Arc::new((time, event)));
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
    let session_channel = channel.clone();
    let session_shutdown = shutdown_handle.clone();
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
    supervisor_handle.abort();
    event_handle.abort();
    session_watchdog.abort();
    result
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
        name: unit.name.clone(),
        pilot_name,
        plane_type: unit.r#type.clone().unwrap_or_default(),
        plane_info,
        pilot_kind,
        pilot_identity,
        pilot_ucid,
    }
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
    let passes = lock_unpoisoned(session_log).clone();
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
    println!("╚══════════════════════╩══════╩══════╩════════════════════╝");
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
        assert_eq!(opts.recovery_telemetry_mode, RecoveryTelemetryMode::Auto);
        assert_eq!(opts.recovery_snapshot_timeout_ms, 250);
        assert!(!opts.ownship_hook_diagnostics);
    }

    #[test]
    fn removed_web_flags_are_refused_with_guidance() {
        let opts = Opts::try_parse_from(["lso-run", "--web-port", "8080"])
            .expect("the hidden flag still parses so the refusal can explain itself");
        let err = reject_removed_web_flags(&opts).expect_err("removed flag must be refused");
        assert!(err.to_string().contains("DCS Web Dashboard"), "{err}");

        let opts = Opts::try_parse_from(["lso-run", "--web-expose-ucid"]).expect("parses");
        assert!(reject_removed_web_flags(&opts).is_err());

        let opts = Opts::try_parse_from(["lso-run"]).expect("parses");
        assert!(reject_removed_web_flags(&opts).is_ok());
    }

    #[test]
    fn registry_replaces_units_by_id_and_keeps_both_kinds() {
        let mut registry = UnitRegistry::default();
        let info = AirplaneInfo::by_type("FA-18C_hornet").unwrap();
        registry.planes.insert(
            1,
            plane_candidate(&unit(1, "Hornet-1", Some("A")), info, &[], 1),
        );
        registry.planes.insert(
            1,
            plane_candidate(&unit(1, "Hornet-1", Some("B")), info, &[], 1),
        );
        registry.carriers.insert(
            5,
            CarrierCandidate {
                id: 5,
                name: "CVN".to_string(),
                carrier_type: "CVN_71".to_string(),
                carrier_info: CarrierInfo::by_type("CVN_71").unwrap(),
            },
        );
        assert_eq!(registry.planes.len(), 1);
        assert_eq!(registry.planes[&1].pilot_name, "B");
        assert_eq!(registry.carriers.len(), 1);
    }
}
