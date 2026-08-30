use stubs::hook;
use stubs::hook::v0::hook_service_client::HookServiceClient;
use tonic::transport::Channel;

use super::{request_with_deadline, GrpcResult};

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
}
