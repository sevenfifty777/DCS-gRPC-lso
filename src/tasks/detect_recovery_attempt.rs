use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use futures_util::StreamExt;
use tokio::task::JoinHandle;
use tonic::Code;

use crate::client::UnitClient;
use crate::transform::Transform;
use crate::utils::{lock_unpoisoned, m_to_ft, m_to_nm};

use super::{
    CarrierCandidate, PlaneCandidate, RecoveryContext, RecoveryTelemetryMode, SharedRegistry,
    TaskParams,
};

/// How often the supervisor samples every known unit while no recovery is
/// active for it.
pub const DETECTION_POLL_INTERVAL: Duration = Duration::from_secs(2);
/// Maximum number of `GetTransform` requests in flight per supervisor tick so
/// a large mission never bursts the DCS-gRPC mission queue.
pub const TRANSFORM_CONCURRENCY: usize = 4;

/// Single detection loop for the whole generation.
///
/// Each tick fetches every known carrier and every plane that is not being
/// recorded exactly once (`carriers + idle planes` RPCs instead of
/// `2 × carriers × planes`), pairs each plane with the nearest compatible
/// carrier inside the detection envelope, and records at most one recovery per
/// plane at a time.
pub async fn supervise_recoveries(
    registry: SharedRegistry,
    context: Arc<RecoveryContext>,
) -> Result<(), crate::error::Error> {
    let mut active: HashMap<u32, JoinHandle<()>> = HashMap::new();
    let mut ticks =
        crate::utils::interval::interval(DETECTION_POLL_INTERVAL, context.shutdown.clone());
    let client = UnitClient::new(context.ch.clone());

    while ticks.next().await.is_some() {
        active.retain(|_, handle| !handle.is_finished());
        crate::metrics::RUNTIME_METRICS.observe_active_recordings(active.len());

        let (planes, carriers) = {
            let registry = lock_unpoisoned(&registry);
            (
                registry
                    .planes
                    .values()
                    .filter(|plane| !active.contains_key(&plane.id))
                    .cloned()
                    .collect::<Vec<_>>(),
                registry.carriers.values().cloned().collect::<Vec<_>>(),
            )
        };
        if planes.is_empty() || carriers.is_empty() {
            continue;
        }

        let carrier_units = carriers
            .iter()
            .map(|carrier| (carrier.id, carrier.name.clone()))
            .collect::<Vec<_>>();
        let carrier_transforms =
            fetch_transforms(&client, carrier_units, &registry, "carrier").await;
        if carrier_transforms.is_empty() {
            continue;
        }
        let plane_units = planes
            .iter()
            .map(|plane| (plane.id, plane.name.clone()))
            .collect::<Vec<_>>();
        let plane_transforms = fetch_transforms(&client, plane_units, &registry, "plane").await;

        for plane in &planes {
            let Some(plane_transform) = plane_transforms.get(&plane.id) else {
                continue;
            };
            let nearest = carriers
                .iter()
                .filter(|carrier| {
                    carrier
                        .carrier_info
                        .supports_aircraft_type(&plane.plane_type)
                })
                .filter_map(|carrier| {
                    carrier_transforms
                        .get(&carrier.id)
                        .map(|transform| (carrier, transform))
                })
                .filter(|(_, carrier_transform)| {
                    is_recovery_attempt(carrier_transform, plane_transform)
                })
                .min_by(|(_, left), (_, right)| {
                    let left = (left.position - plane_transform.position).mag();
                    let right = (right.position - plane_transform.position).mag();
                    left.total_cmp(&right)
                });
            if let Some((carrier, _)) = nearest {
                let handle = spawn_recording(context.clone(), plane.clone(), carrier.clone());
                active.insert(plane.id, handle);
            }
        }
    }

    for (_, handle) in active.drain() {
        handle.abort();
    }
    Ok(())
}

async fn fetch_transforms(
    client: &UnitClient,
    units: Vec<(u32, String)>,
    registry: &SharedRegistry,
    kind: &'static str,
) -> HashMap<u32, Transform> {
    let results = futures_util::stream::iter(units)
        .map(|(id, name)| {
            let mut client = client.clone();
            async move { (id, client.get_transform(name).await) }
        })
        .buffer_unordered(TRANSFORM_CONCURRENCY)
        .collect::<Vec<_>>()
        .await;

    let mut transforms = HashMap::with_capacity(results.len());
    let mut transient_errors = 0_u32;
    let mut last_error = None;
    for (id, result) in results {
        match result {
            Ok(transform) => {
                transforms.insert(id, transform);
            }
            Err(status) if status.code() == Code::NotFound => {
                tracing::debug!(unit_id = id, kind, "unit no longer exists; forgetting it");
                let mut registry = lock_unpoisoned(registry);
                registry.planes.remove(&id);
                registry.carriers.remove(&id);
            }
            Err(status) => {
                transient_errors += 1;
                last_error = Some(status);
            }
        }
    }
    if let Some(status) = last_error {
        tracing::warn!(
            transient_errors,
            kind,
            ?status,
            "transient detector polling errors; units stay registered"
        );
    }
    transforms
}

fn spawn_recording(
    context: Arc<RecoveryContext>,
    plane: PlaneCandidate,
    carrier: CarrierCandidate,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let params = TaskParams::new(&context, &plane, &carrier);
        let plane_id = plane.id;
        let carrier_id = carrier.id;
        if let Err(err) = super::record_recovery::record_recovery(params).await {
            let unimplemented_forced_atomic = matches!(
                &err,
                crate::error::Error::Grpc(status) if status.code() == Code::Unimplemented
            ) && context.recovery_telemetry_mode
                == RecoveryTelemetryMode::Atomic;
            if unimplemented_forced_atomic {
                tracing::error!(
                    "forced atomic recovery telemetry is not implemented by this DCS-gRPC server; \
                     ending the generation instead of retrying every recovery"
                );
                context.fatal.send(err).await.ok();
            } else {
                tracing::error!(
                    %err,
                    plane_id,
                    carrier_id,
                    session_id = context.session_id,
                    generation = context.generation,
                    "recovery failed locally; keeping other recoveries alive"
                );
            }
        }
    })
}

pub fn is_recovery_attempt(carrier: &Transform, plane: &Transform) -> bool {
    // Pattern entry: within 3.5 nm and below 1100 ft.
    // The nose-pointing check is intentionally removed — during the break the
    // aircraft is abeam the carrier with its nose perpendicular to BRC.
    if m_to_ft(plane.alt) > 1100.0 {
        tracing::trace!(alt_in_ft = m_to_ft(plane.alt), "ignore planes above 1100ft");
        return false;
    }

    let ray_from_plane_to_carrier = carrier.position - plane.position;
    let distance = ray_from_plane_to_carrier.mag();

    // ignore planes farther away than 3.5nm
    if m_to_nm(distance) > 3.5 {
        tracing::trace!(
            distance_in_nm = m_to_nm(distance),
            "ignore planes farther away than 3.5nm"
        );
        return false;
    }

    // ignore takeoffs / aircraft on deck
    if distance < 200.0 {
        tracing::trace!(distance_in_m = distance, "ignore takeoffs");
        return false;
    }

    // No rear-hemisphere check: the overhead pattern takes the aircraft ahead of
    // the carrier (initial / break), so we must capture all quadrants within the
    // distance + altitude envelope above.

    tracing::debug!(
        at = plane.time,
        distance_in_m = distance,
        distance_in_nm = m_to_nm(distance),
        "found pattern / recovery attempt",
    );
    true
}
