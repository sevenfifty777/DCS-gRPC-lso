mod atmosphere_client;
mod hook_client;
mod metadata_client;
mod mission_client;
mod net_client;
mod unit_client;
mod world_client;

use std::time::Duration;

use tonic::metadata::AsciiMetadataValue;
use tonic::service::interceptor::InterceptedService;
use tonic::service::Interceptor;
use tonic::transport::Channel;
use tonic::{Request, Status};

/// Deadline applied to unary DCS-gRPC calls. A timed-out pass is diagnosed
/// locally instead of being allowed to block every other recovery forever.
pub const RPC_DEADLINE: Duration = Duration::from_secs(2);

pub type GrpcResult<T> = Result<T, Box<Status>>;

pub type GrpcChannel = InterceptedService<Channel, ApiKeyInterceptor>;

#[derive(Clone)]
pub struct ApiKeyInterceptor {
    value: Option<AsciiMetadataValue>,
}

impl ApiKeyInterceptor {
    pub fn new(token: Option<&str>) -> Result<Self, crate::error::Error> {
        let value = match token {
            Some(token) => {
                let mut value = token.parse::<AsciiMetadataValue>().map_err(|_| {
                    crate::error::Error::InvalidConfiguration(
                        "the DCS-gRPC API key is not valid ASCII metadata".to_string(),
                    )
                })?;
                value.set_sensitive(true);
                Some(value)
            }
            None => None,
        };
        Ok(Self { value })
    }
}

impl Interceptor for ApiKeyInterceptor {
    fn call(&mut self, mut request: Request<()>) -> Result<Request<()>, Status> {
        if let Some(value) = &self.value {
            request.metadata_mut().insert("x-api-key", value.clone());
        }
        Ok(request)
    }
}

pub fn authenticated_channel(channel: Channel, interceptor: ApiKeyInterceptor) -> GrpcChannel {
    InterceptedService::new(channel, interceptor)
}

pub(crate) fn request_with_deadline<T>(message: T) -> Request<T> {
    request_with_timeout(message, RPC_DEADLINE)
}

pub(crate) fn request_with_timeout<T>(message: T, timeout: Duration) -> Request<T> {
    crate::metrics::RUNTIME_METRICS.count_rpc();
    let mut request = Request::new(message);
    request.set_timeout(timeout);
    request
}

pub use atmosphere_client::*;
pub use hook_client::*;
pub use metadata_client::*;
pub use mission_client::*;
pub use net_client::*;
pub use unit_client::*;
pub use world_client::*;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn api_key_interceptor_adds_sensitive_metadata() {
        let mut interceptor = ApiKeyInterceptor::new(Some("test-token")).unwrap();
        let request = interceptor.call(Request::new(())).unwrap();
        let value = request.metadata().get("x-api-key").unwrap();
        assert_eq!(value, "test-token");
        assert!(value.is_sensitive());
    }

    #[test]
    fn api_key_interceptor_allows_an_explicitly_unauthenticated_channel() {
        let mut interceptor = ApiKeyInterceptor::new(None).unwrap();
        let request = interceptor.call(Request::new(())).unwrap();
        assert!(request.metadata().get("x-api-key").is_none());
    }
}
