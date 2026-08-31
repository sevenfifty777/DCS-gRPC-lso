use stubs::net::v0::get_players_response::GetPlayerInfo;
use stubs::net::v0::net_service_client::NetServiceClient;
use tonic::transport::Channel;

use super::{request_with_deadline, GrpcResult};

pub struct NetClient {
    svc: NetServiceClient<Channel>,
}

impl NetClient {
    pub fn new(ch: Channel) -> Self {
        Self {
            svc: NetServiceClient::new(ch),
        }
    }

    pub async fn get_players(&mut self) -> GrpcResult<Vec<GetPlayerInfo>> {
        let res = self
            .svc
            .get_players(request_with_deadline(stubs::net::v0::GetPlayersRequest {}))
            .await
            .map_err(Box::new)?
            .into_inner();
        Ok(res.players)
    }
}
