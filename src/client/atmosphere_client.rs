use stubs::atmosphere::v0::atmosphere_service_client::AtmosphereServiceClient;
use stubs::atmosphere::v0::GetWindRequest;
use stubs::common::v0::InputPosition;
use tonic::transport::Channel;

use super::{request_with_deadline, GrpcResult};

/// Conversion factor from metres per second to knots.
const MPS_TO_KNOTS: f32 = 1.944;

pub struct AtmosphereClient {
    svc: AtmosphereServiceClient<Channel>,
}

impl AtmosphereClient {
    pub fn new(ch: Channel) -> Self {
        Self {
            svc: AtmosphereServiceClient::new(ch),
        }
    }

    /// Query wind at the given geodetic position.
    ///
    /// Returns `(direction_deg, speed_kts)` where `direction_deg` is the heading
    /// the wind is coming **from** (0–359°), and `speed_kts` is the wind speed in knots.
    pub async fn get_wind(&mut self, lat: f64, lon: f64, alt: f64) -> GrpcResult<(u16, f32)> {
        let res = self
            .svc
            .get_wind(request_with_deadline(GetWindRequest {
                position: Some(InputPosition { lat, lon, alt }),
            }))
            .await
            .map_err(Box::new)?
            .into_inner();

        let direction = (res.heading.round() as u16) % 360;
        let speed_kts = res.strength * MPS_TO_KNOTS;
        Ok((direction, speed_kts))
    }
}
