use stubs::common::v0::Unit;
use stubs::unit;
use stubs::unit::v0::unit_service_client::UnitServiceClient;
use tonic::transport::Channel;

use crate::transform::{ObservedTransform, Transform};

use super::{request_with_deadline, GrpcResult};

pub struct UnitClient {
    svc: UnitServiceClient<Channel>,
}

impl UnitClient {
    pub fn new(ch: Channel) -> Self {
        Self {
            svc: UnitServiceClient::new(ch),
        }
    }

    pub async fn get_transform(&mut self, unit_name: impl Into<String>) -> GrpcResult<Transform> {
        let timer = crate::metrics::RUNTIME_METRICS.transform_rpc();
        let res = self
            .svc
            .get_transform(request_with_deadline(unit::v0::GetTransformRequest {
                name: unit_name.into(),
            }))
            .await
            .map_err(Box::new)?
            .into_inner();
        timer.success();

        Ok((
            res.time,
            res.position.unwrap_or_default(),
            res.orientation.unwrap_or_default(),
            res.velocity.unwrap_or_default(),
        )
            .into())
    }

    pub async fn get_observed_transform(
        &mut self,
        unit_name: impl Into<String>,
    ) -> GrpcResult<ObservedTransform> {
        self.get_transform(unit_name)
            .await
            .map(ObservedTransform::now)
    }

    pub async fn get_unit(&mut self, unit_name: &str) -> GrpcResult<Unit> {
        let unit = self
            .svc
            .get(request_with_deadline(unit::v0::GetRequest {
                name: unit_name.to_string(),
            }))
            .await
            .map_err(Box::new)?
            .into_inner()
            .unit
            .ok_or_else(|| {
                Box::new(tonic::Status::not_found(format!(
                    "received empty response for unit `{}`",
                    unit_name
                )))
            })?;
        Ok(unit)
    }

    pub async fn get_descriptor(&mut self, unit_name: &str) -> GrpcResult<Vec<String>> {
        let descriptor = self
            .svc
            .get_descriptor(request_with_deadline(unit::v0::GetDescriptorRequest {
                name: unit_name.to_string(),
            }))
            .await
            .map_err(Box::new)?
            .into_inner()
            .attributes;
        Ok(descriptor)
    }

    pub async fn get_draw_argument_value(
        &mut self,
        unit_name: &str,
        argument: u32,
    ) -> GrpcResult<f64> {
        let value = self
            .svc
            .get_draw_argument_value(request_with_deadline(
                unit::v0::GetDrawArgumentValueRequest {
                    name: unit_name.to_string(),
                    argument,
                },
            ))
            .await
            .map_err(Box::new)?
            .into_inner()
            .value;
        Ok(value)
    }
}
