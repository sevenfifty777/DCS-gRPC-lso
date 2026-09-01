use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use stubs::recovery;
use stubs::recovery::v0::recovery_service_client::RecoveryServiceClient as GrpcRecoveryClient;
use stubs::recovery::v0::DrawArgumentStatus;
use tonic::transport::Channel;

use crate::metrics::RpcKind;
use crate::transform::{ObservedTransform, Transform};

use super::{request_with_timeout, GrpcResult};

#[derive(Debug)]
pub struct RecoverySnapshot {
    pub sequence: u64,
    pub round_trip_ms: f64,
    pub carrier: ObservedTransform,
    pub plane: ObservedTransform,
    pub draw_argument_status: DrawArgumentStatus,
    pub draw_argument_value: Option<f64>,
}

pub struct RecoveryClient {
    svc: GrpcRecoveryClient<Channel>,
}

impl RecoveryClient {
    pub fn new(channel: Channel) -> Self {
        Self {
            svc: GrpcRecoveryClient::new(channel),
        }
    }

    pub async fn get_snapshot(
        &mut self,
        carrier_name: &str,
        aircraft_name: &str,
        aircraft_draw_argument: Option<u32>,
        sequence: u64,
        timeout: Duration,
    ) -> GrpcResult<RecoverySnapshot> {
        let timer = crate::metrics::RUNTIME_METRICS.rpc(RpcKind::RecoverySnapshot);
        let started_at = Instant::now();
        let response = self
            .svc
            .get_recovery_snapshot(request_with_timeout(
                recovery::v0::GetRecoverySnapshotRequest {
                    carrier_name: carrier_name.to_string(),
                    aircraft_name: aircraft_name.to_string(),
                    aircraft_draw_argument,
                    sequence,
                },
                timeout,
            ))
            .await;

        let response = match response {
            Ok(response) => response.into_inner(),
            Err(status) if status.code() == tonic::Code::DeadlineExceeded => {
                timer.timeout();
                return Err(Box::new(status));
            }
            Err(status) => return Err(Box::new(status)),
        };
        if response.sequence != sequence {
            return Err(Box::new(tonic::Status::data_loss(format!(
                "recovery snapshot sequence mismatch: requested {sequence}, received {}",
                response.sequence
            ))));
        }

        let carrier = response
            .carrier
            .ok_or_else(|| tonic::Status::data_loss("recovery snapshot omitted carrier"))?;
        let plane = response
            .aircraft
            .ok_or_else(|| tonic::Status::data_loss("recovery snapshot omitted aircraft"))?;
        let carrier = transform(response.time, carrier, "carrier")?;
        let plane = transform(response.time, plane, "aircraft")?;
        let received_at = Instant::now();
        let received_unix_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
            .min(u64::MAX as u128) as u64;
        let observed = |value| ObservedTransform {
            value,
            received_at,
            received_unix_ms,
        };
        let draw_argument = response.aircraft_draw_argument.unwrap_or_default();
        let draw_argument_status = DrawArgumentStatus::try_from(draw_argument.status)
            .unwrap_or(DrawArgumentStatus::Unspecified);
        timer.success();

        Ok(RecoverySnapshot {
            sequence: response.sequence,
            round_trip_ms: started_at.elapsed().as_secs_f64() * 1_000.0,
            carrier: observed(carrier),
            plane: observed(plane),
            draw_argument_status,
            draw_argument_value: draw_argument.value,
        })
    }
}

fn transform(
    time: f64,
    value: recovery::v0::RecoveryTransform,
    label: &str,
) -> Result<Transform, Box<tonic::Status>> {
    let position = value.position.ok_or_else(|| {
        Box::new(tonic::Status::data_loss(format!(
            "{label} position is missing"
        )))
    })?;
    let orientation = value.orientation.ok_or_else(|| {
        Box::new(tonic::Status::data_loss(format!(
            "{label} orientation is missing"
        )))
    })?;
    let velocity = value.velocity.ok_or_else(|| {
        Box::new(tonic::Status::data_loss(format!(
            "{label} velocity is missing"
        )))
    })?;
    Ok(Transform::from((time, position, orientation, velocity)))
}
