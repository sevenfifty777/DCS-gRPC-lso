use stubs::custom::v0::custom_service_client::CustomServiceClient;
use stubs::custom::v0::EvalRequest;

use super::{request_with_deadline, GrpcChannel, GrpcResult};

pub struct CustomClient {
    svc: CustomServiceClient<GrpcChannel>,
}

impl CustomClient {
    pub fn new(ch: GrpcChannel) -> Self {
        Self {
            svc: CustomServiceClient::new(ch),
        }
    }

    /// Run a Lua chunk in the mission scripting environment and return the JSON encoding of the
    /// value it returns (`net.lua2json` on the server side).
    ///
    /// The server refuses the call with `PERMISSION_DENIED` unless `evalEnabled = true`. LSO only
    /// sends fixed, read-only chunks defined in its own source (see `crate::mission_weather`);
    /// never forward user- or network-supplied Lua here.
    pub async fn eval(&mut self, lua: &str) -> GrpcResult<String> {
        let res = self
            .svc
            .eval(request_with_deadline(EvalRequest {
                lua: lua.to_string(),
            }))
            .await
            .map_err(Box::new)?
            .into_inner();
        Ok(res.json)
    }
}
