use stubs::world;
use stubs::world::v0::world_service_client::WorldServiceClient;
use tonic::{transport::Channel, Status};

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
    pub async fn get_theatre(&mut self) -> Result<String, Status> {
        let res = self
            .svc
            .get_theatre(world::v0::GetTheatreRequest {})
            .await?
            .into_inner();
        Ok(res.theatre)
    }
}
