use stubs::net::v0::get_players_response::GetPlayerInfo;
use stubs::net::v0::net_service_client::NetServiceClient;
use tonic::{transport::Channel, Status};

pub struct NetClient {
    svc: NetServiceClient<Channel>,
}

impl NetClient {
    pub fn new(ch: Channel) -> Self {
        Self {
            svc: NetServiceClient::new(ch),
        }
    }

    pub async fn get_players(&mut self) -> Result<Vec<GetPlayerInfo>, Status> {
        let res = self
            .svc
            .get_players(stubs::net::v0::GetPlayersRequest {})
            .await?
            .into_inner();
        Ok(res.players)
    }
}
