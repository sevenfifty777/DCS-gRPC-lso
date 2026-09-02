//! Priority acquisition of paired carrier/aircraft transforms.
//!
//! This module deliberately knows nothing about gates, hook state, events,
//! persistence or rendering. It is the polling implementation behind the
//! collection boundary and can later be replaced by a buffered source without
//! changing the consumers.

use tonic::transport::Channel;

use crate::client::{GrpcResult, UnitClient};
use crate::metrics::RpcKind;
use crate::telemetry::{TelemetryAligner, TelemetrySample};
use crate::track::{OnlineMetricStats, PositionCollectionMetrics};

pub struct PositionCollector {
    carrier: UnitClient,
    plane: UnitClient,
    aligner: TelemetryAligner,
    poll_latency_stats: OnlineMetricStats,
    errors: u32,
    timeouts: u32,
}

impl PositionCollector {
    pub fn new(channel: Channel) -> Self {
        Self {
            carrier: UnitClient::new(channel.clone()),
            plane: UnitClient::new(channel),
            aligner: TelemetryAligner::new(),
            poll_latency_stats: OnlineMetricStats::default(),
            errors: 0,
            timeouts: 0,
        }
    }

    pub async fn poll(
        &mut self,
        carrier_name: &str,
        plane_name: &str,
    ) -> GrpcResult<TelemetrySample> {
        let started = std::time::Instant::now();
        let result = futures_util::future::try_join(
            self.carrier
                .get_observed_transform_for(carrier_name, RpcKind::TransformCarrier),
            self.plane
                .get_observed_transform_for(plane_name, RpcKind::TransformPlane),
        )
        .await;
        self.poll_latency_stats
            .observe(started.elapsed().as_secs_f64() * 1_000.0);
        let (carrier, plane) = match result {
            Ok(pair) => pair,
            Err(error) => {
                self.errors += 1;
                if error.code() == tonic::Code::DeadlineExceeded {
                    self.timeouts += 1;
                }
                return Err(error);
            }
        };
        Ok(self.aligner.align(carrier, plane))
    }

    pub fn reset(&mut self) {
        self.aligner.reset();
    }

    pub fn metrics(&self) -> PositionCollectionMetrics {
        PositionCollectionMetrics {
            polls: self.poll_latency_stats.count().min(u64::from(u32::MAX)) as u32,
            errors: self.errors,
            timeouts: self.timeouts,
            mean_latency_ms: self.poll_latency_stats.mean(),
            p50_latency_ms: self.poll_latency_stats.percentile(0.50),
            p95_latency_ms: self.poll_latency_stats.percentile(0.95),
            p99_latency_ms: self.poll_latency_stats.percentile(0.99),
            max_latency_ms: self.poll_latency_stats.max(),
        }
    }
}
