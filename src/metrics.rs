use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use once_cell::sync::Lazy;

pub static RUNTIME_METRICS: Lazy<RuntimeMetrics> = Lazy::new(RuntimeMetrics::default);

#[derive(Default)]
pub struct RuntimeMetrics {
    rpc_calls: AtomicU64,
    transform_rpc_calls: AtomicU64,
    transform_rpc_errors: AtomicU64,
    transform_rpc_latency_us: AtomicU64,
    active_streams: AtomicU64,
    active_recoveries: AtomicU64,
    queue_high_watermark: AtomicU64,
    io_bytes: AtomicU64,
    render_count: AtomicU64,
    render_time_us: AtomicU64,
}

impl RuntimeMetrics {
    pub fn count_rpc(&self) {
        self.rpc_calls.fetch_add(1, Ordering::Relaxed);
    }

    pub fn transform_rpc(&self) -> TransformRpcTimer<'_> {
        self.transform_rpc_calls.fetch_add(1, Ordering::Relaxed);
        TransformRpcTimer {
            metrics: self,
            started: Instant::now(),
            succeeded: false,
        }
    }

    pub fn stream(&self) -> GaugeGuard<'_> {
        GaugeGuard::new(&self.active_streams)
    }

    pub fn recovery(&self) -> GaugeGuard<'_> {
        GaugeGuard::new(&self.active_recoveries)
    }

    pub fn observe_queue_depth(&self, depth: usize) {
        self.queue_high_watermark
            .fetch_max(depth as u64, Ordering::Relaxed);
    }

    pub fn add_io_bytes(&self, bytes: usize) {
        self.io_bytes.fetch_add(bytes as u64, Ordering::Relaxed);
    }

    pub fn observe_render(&self, elapsed_us: u64) {
        self.render_count.fetch_add(1, Ordering::Relaxed);
        self.render_time_us.fetch_add(elapsed_us, Ordering::Relaxed);
    }

    pub fn log_snapshot(&self, elapsed_secs: f64) {
        let rpc_calls = self.rpc_calls.load(Ordering::Relaxed);
        let transform_calls = self.transform_rpc_calls.load(Ordering::Relaxed);
        let transform_errors = self.transform_rpc_errors.load(Ordering::Relaxed);
        let transform_latency_us = self.transform_rpc_latency_us.load(Ordering::Relaxed);
        let render_count = self.render_count.load(Ordering::Relaxed);
        let render_time_us = self.render_time_us.load(Ordering::Relaxed);
        tracing::info!(
            rpc_calls,
            rpc_per_second = rpc_calls as f64 / elapsed_secs.max(0.001),
            transform_calls,
            transform_errors,
            transform_mean_latency_ms = if transform_calls == 0 {
                0.0
            } else {
                transform_latency_us as f64 / transform_calls as f64 / 1_000.0
            },
            active_streams = self.active_streams.load(Ordering::Relaxed),
            active_recoveries = self.active_recoveries.load(Ordering::Relaxed),
            queue_high_watermark = self.queue_high_watermark.load(Ordering::Relaxed),
            io_bytes = self.io_bytes.load(Ordering::Relaxed),
            render_count,
            render_mean_ms = if render_count == 0 {
                0.0
            } else {
                render_time_us as f64 / render_count as f64 / 1_000.0
            },
            "runtime metrics snapshot"
        );
    }
}

pub struct TransformRpcTimer<'a> {
    metrics: &'a RuntimeMetrics,
    started: Instant,
    succeeded: bool,
}

impl TransformRpcTimer<'_> {
    pub fn success(mut self) {
        self.succeeded = true;
    }
}

impl Drop for TransformRpcTimer<'_> {
    fn drop(&mut self) {
        self.metrics.transform_rpc_latency_us.fetch_add(
            self.started.elapsed().as_micros().min(u64::MAX as u128) as u64,
            Ordering::Relaxed,
        );
        if !self.succeeded {
            self.metrics
                .transform_rpc_errors
                .fetch_add(1, Ordering::Relaxed);
        }
    }
}

pub struct GaugeGuard<'a> {
    gauge: &'a AtomicU64,
}

impl<'a> GaugeGuard<'a> {
    fn new(gauge: &'a AtomicU64) -> Self {
        gauge.fetch_add(1, Ordering::Relaxed);
        Self { gauge }
    }
}

impl Drop for GaugeGuard<'_> {
    fn drop(&mut self) {
        self.gauge.fetch_sub(1, Ordering::Relaxed);
    }
}
