use stubs::atmosphere::v0::atmosphere_service_client::AtmosphereServiceClient;
use stubs::atmosphere::v0::GetWindRequest;
use stubs::common::v0::InputPosition;

use super::{request_with_deadline, GrpcChannel, GrpcResult};

pub struct AtmosphereClient {
    svc: AtmosphereServiceClient<GrpcChannel>,
}

impl AtmosphereClient {
    pub fn new(ch: GrpcChannel) -> Self {
        Self {
            svc: AtmosphereServiceClient::new(ch),
        }
    }

    /// Query wind at the given geodetic position.
    ///
    /// Returns `(direction_deg, speed_mps)` where `direction_deg` is the heading the wind is
    /// coming **from** (0-359 deg), and `speed_mps` is the raw wind speed in metres per second
    /// as reported by DCS. Callers convert to a display unit (e.g. knots) themselves; this
    /// keeps the client returning the same physical unit DCS reports, matching every other raw
    /// SI value in the JSON report (e.g. `touchdown_horizontal_speed_mps`).
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
        Ok((direction, res.strength))
    }
}
