use std::future::ready;

use futures_util::{Stream, StreamExt};
use stubs::mission;
use stubs::mission::v0::mission_service_client::MissionServiceClient;
use stubs::mission::v0::stream_events_response::Event;
use tonic::{transport::Channel, Status};

use super::{request_with_deadline, GrpcResult};

pub struct MissionClient {
    svc: MissionServiceClient<Channel>,
}

impl MissionClient {
    pub fn new(ch: Channel) -> Self {
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

    pub async fn stream_events(
        &mut self,
    ) -> GrpcResult<impl Stream<Item = Result<(f64, Event), Status>>> {
        let events = self
            .svc
            .stream_events(request_with_deadline(mission::v0::StreamEventsRequest {}))
            .await
            .map_err(Box::new)?
            .into_inner()
            .filter_map(|event| {
                ready(match event {
                    Ok(stubs::mission::v0::StreamEventsResponse {
                        time,
                        event: Some(event),
                        ..
                    }) => Some(Ok((time, event))),
                    Err(err) => Some(Err(err)),
                    Ok(_) => None,
                })
            });
        Ok(events)
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
