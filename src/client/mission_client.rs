use stubs::mission;
use stubs::mission::v0::mission_service_client::MissionServiceClient;

use super::{request_with_deadline, GrpcChannel, GrpcResult};

pub struct MissionClient {
    svc: MissionServiceClient<GrpcChannel>,
}

impl MissionClient {
    pub fn new(ch: GrpcChannel) -> Self {
        Self {
            svc: MissionServiceClient::new(ch),
        }
    }

    pub async fn get_scenario_start_time(&mut self) -> GrpcResult<String> {
        let res = self
            .svc
            .get_scenario_start_time(request_with_deadline(
                mission::v0::GetScenarioStartTimeRequest {},
            ))
            .await
            .map_err(Box::new)?
            .into_inner();
        Ok(res.datetime)
    }

    pub async fn get_scenario_current_time(&mut self) -> GrpcResult<String> {
        let res = self
            .svc
            .get_scenario_current_time(request_with_deadline(
                mission::v0::GetScenarioCurrentTimeRequest {},
            ))
            .await
            .map_err(Box::new)?
            .into_inner();
        Ok(res.datetime)
    }

    pub async fn get_session_id(&mut self) -> GrpcResult<i64> {
        let response = self
            .svc
            .get_session_id(request_with_deadline(mission::v0::GetSessionIdRequest {}))
            .await
            .map_err(Box::new)?
            .into_inner();
        Ok(response.session_id)
    }
}
