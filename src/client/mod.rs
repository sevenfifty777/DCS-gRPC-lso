mod atmosphere_client;
mod hook_client;
mod metadata_client;
mod mission_client;
mod net_client;
mod recovery_client;
mod unit_client;
mod world_client;

use std::time::Duration;

use tonic::{Request, Status};

/// Deadline applied to unary DCS-gRPC calls. A timed-out pass is diagnosed
/// locally instead of being allowed to block every other recovery forever.
pub const RPC_DEADLINE: Duration = Duration::from_secs(2);

pub type GrpcResult<T> = Result<T, Box<Status>>;

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
pub use recovery_client::*;
pub use unit_client::*;
pub use world_client::*;
