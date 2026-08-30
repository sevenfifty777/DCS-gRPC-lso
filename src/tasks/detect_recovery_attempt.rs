use std::time::Duration;

use futures_util::StreamExt;
use tonic::Code;

use crate::client::UnitClient;
use crate::transform::Transform;
use crate::utils::{m_to_ft, m_to_nm};

use super::TaskParams;

#[tracing::instrument(
    skip_all,
    fields(carrier_name = params.carrier_name, plane_name = params.plane_name)
)]
pub async fn detect_recovery_attempt(params: TaskParams<'_>) -> Result<(), crate::error::Error> {
    tracing::debug!("started observing for possible recovery attempts");

    let mut client1 = UnitClient::new(params.ch.clone());
    let mut client2 = UnitClient::new(params.ch.clone());
    let mut interval =
        crate::utils::interval::interval(Duration::from_secs(2), params.shutdown.clone());

    while interval.next().await.is_some() {
        let result = futures_util::future::try_join(
            client1.get_transform(params.carrier_name),
            client2.get_transform(params.plane_name),
        )
        .await;

        match result {
            Ok((carrier, plane)) => {
                if is_recovery_attempt(&carrier, &plane) {
                    // record_recovery runs to completion (landed / bolter / waveoff /
                    // crash) before we check again — no cooldown needed here.
                    if let Err(err) = super::record_recovery::record_recovery(params.clone()).await
                    {
                        tracing::error!(
                            %err,
                            "recovery failed locally; keeping this detector and other pairs alive"
                        );
                    }
                }
            }
            Err(status) if status.code() == Code::NotFound => {
                tracing::debug!("stop tracking as either carrier or plane doesn't exist anymore");
                return Ok(());
            }
            Err(err) => {
                tracing::warn!(
                    ?err,
                    "transient detector polling error; keeping pair isolated"
                );
                continue;
            }
        }
    }

    Ok(())
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
