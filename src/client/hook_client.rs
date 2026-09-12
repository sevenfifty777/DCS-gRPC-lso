use stubs::hook;
use stubs::hook::v0::hook_service_client::HookServiceClient;
use tonic::transport::Channel;

use super::{request_with_deadline, request_with_timeout, GrpcResult};
use crate::metrics::RpcKind;

pub struct HookClient {
    svc: HookServiceClient<Channel>,
}

impl HookClient {
    pub fn new(ch: Channel) -> Self {
        Self {
            svc: HookServiceClient::new(ch),
        }
    }

    pub async fn get_mission_name(&mut self) -> GrpcResult<String> {
        let res = self
            .svc
            .get_mission_name(request_with_deadline(hook::v0::GetMissionNameRequest {}))
            .await
            .map_err(Box::new)?
            .into_inner();
        Ok(res.name)
    }

    pub async fn get_ownship_hook_state_with_timeout(
        &mut self,
        timeout: std::time::Duration,
    ) -> GrpcResult<hook::v0::GetOwnshipHookStateResponse> {
        let timer = crate::metrics::RUNTIME_METRICS.rpc(RpcKind::Hook);
        let response = self
            .svc
            .get_ownship_hook_state(request_with_timeout(
                hook::v0::GetOwnshipHookStateRequest {},
                timeout,
            ))
            .await;
        match response {
            Ok(response) => {
                timer.success();
                Ok(response.into_inner())
            }
            Err(status) if status.code() == tonic::Code::DeadlineExceeded => {
                timer.timeout();
                Err(Box::new(status))
            }
            Err(status) => Err(Box::new(status)),
        }
    }
}
