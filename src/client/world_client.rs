use stubs::world;
use stubs::world::v0::world_service_client::WorldServiceClient;
use tonic::transport::Channel;

use super::{request_with_deadline, GrpcResult};

pub struct WorldClient {
    svc: WorldServiceClient<Channel>,
}

impl WorldClient {
    pub fn new(ch: Channel) -> Self {
        Self {
            svc: WorldServiceClient::new(ch),
        }
    }

    /// Returns the DCS theatre name (e.g. `"Caucasus"`, `"PersianGulf"`, `"Syria"`).
    pub async fn get_theatre(&mut self) -> GrpcResult<String> {
        let res = self
            .svc
            .get_theatre(request_with_deadline(world::v0::GetTheatreRequest {}))
            .await
            .map_err(Box::new)?
            .into_inner();
        Ok(res.theatre)
    }
}
