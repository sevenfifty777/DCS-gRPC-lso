use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use once_cell::sync::Lazy;

pub static RUNTIME_METRICS: Lazy<RuntimeMetrics> = Lazy::new(RuntimeMetrics::default);

const LATENCY_BUCKETS_US: [u64; 15] = [
    1_000,
    2_000,
    5_000,
    10_000,
    20_000,
    50_000,
    100_000,
    200_000,
    300_000,
    500_000,
    1_000_000,
    2_000_000,
    5_000_000,
    10_000_000,
    u64::MAX,
];

#[derive(Debug, Clone, Copy)]
pub enum RpcKind {
    TransformOther,
    TransformCarrier,
    TransformPlane,
    Hook,
}

impl RpcKind {
    const ALL: [Self; 4] = [
        Self::TransformOther,
        Self::TransformCarrier,
        Self::TransformPlane,
        Self::Hook,
    ];

    const fn index(self) -> usize {
        match self {
            Self::TransformOther => 0,
            Self::TransformCarrier => 1,
            Self::TransformPlane => 2,
            Self::Hook => 3,
        }
    }

    const fn label(self) -> &'static str {
        match self {
            Self::TransformOther => "transform_other",
            Self::TransformCarrier => "transform_carrier",
            Self::TransformPlane => "transform_plane",
            Self::Hook => "hook",
        }
    }
}

struct LatencyStats {
    calls: AtomicU64,
    successes: AtomicU64,
    errors: AtomicU64,
    timeouts: AtomicU64,
    total_us: AtomicU64,
    max_us: AtomicU64,
    buckets: [AtomicU64; LATENCY_BUCKETS_US.len()],
}

impl Default for LatencyStats {
    fn default() -> Self {
        Self {
            calls: AtomicU64::new(0),
            successes: AtomicU64::new(0),
            errors: AtomicU64::new(0),
            timeouts: AtomicU64::new(0),
            total_us: AtomicU64::new(0),
            max_us: AtomicU64::new(0),
            buckets: std::array::from_fn(|_| AtomicU64::new(0)),
        }
    }
}

impl LatencyStats {
    fn observe(&self, elapsed_us: u64, outcome: TimerOutcome) {
        self.calls.fetch_add(1, Ordering::Relaxed);
        match outcome {
            TimerOutcome::Success => self.successes.fetch_add(1, Ordering::Relaxed),
            TimerOutcome::Error => self.errors.fetch_add(1, Ordering::Relaxed),
            TimerOutcome::Timeout => self.timeouts.fetch_add(1, Ordering::Relaxed),
        };
        self.total_us.fetch_add(elapsed_us, Ordering::Relaxed);
        self.max_us.fetch_max(elapsed_us, Ordering::Relaxed);
        let index = LATENCY_BUCKETS_US
            .iter()
            .position(|upper| elapsed_us <= *upper)
            .unwrap_or(LATENCY_BUCKETS_US.len() - 1);
        self.buckets[index].fetch_add(1, Ordering::Relaxed);
    }

    fn snapshot(&self) -> LatencySnapshot {
        let calls = self.calls.load(Ordering::Relaxed);
        let buckets = std::array::from_fn(|index| self.buckets[index].load(Ordering::Relaxed));
        LatencySnapshot {
            calls,
            successes: self.successes.load(Ordering::Relaxed),
            errors: self.errors.load(Ordering::Relaxed),
            timeouts: self.timeouts.load(Ordering::Relaxed),
            mean_ms: if calls == 0 {
                0.0
            } else {
                self.total_us.load(Ordering::Relaxed) as f64 / calls as f64 / 1_000.0
            },
            max_ms: self.max_us.load(Ordering::Relaxed) as f64 / 1_000.0,
            p50_ms: percentile_ms(calls, &buckets, 50),
            p95_ms: percentile_ms(calls, &buckets, 95),
            p99_ms: percentile_ms(calls, &buckets, 99),
        }
    }
}

struct LatencySnapshot {
    calls: u64,
    successes: u64,
    errors: u64,
    timeouts: u64,
    mean_ms: f64,
    max_ms: f64,
    p50_ms: f64,
    p95_ms: f64,
    p99_ms: f64,
}

fn percentile_ms(calls: u64, buckets: &[u64; LATENCY_BUCKETS_US.len()], percentile: u64) -> f64 {
    if calls == 0 {
        return 0.0;
    }
    let target = calls.saturating_mul(percentile).div_ceil(100).max(1);
    let mut cumulative = 0_u64;
    for (index, count) in buckets.iter().enumerate() {
        cumulative = cumulative.saturating_add(*count);
        if cumulative >= target {
            let upper = LATENCY_BUCKETS_US[index];
            return if upper == u64::MAX {
                LATENCY_BUCKETS_US[LATENCY_BUCKETS_US.len() - 2] as f64 / 1_000.0
            } else {
                upper as f64 / 1_000.0
            };
        }
    }
    0.0
}

#[derive(Default)]
pub struct RuntimeMetrics {
    rpc_calls: AtomicU64,
    rpc: [LatencyStats; 4],
    recovery_loop: LatencyStats,
    tick_lag: LatencyStats,
    active_streams: AtomicU64,
    active_recoveries: AtomicU64,
    queue_high_watermark: AtomicU64,
    hook_samples_dropped: AtomicU64,
    io_bytes: AtomicU64,
    render_count: AtomicU64,
    render_time_us: AtomicU64,
}

impl RuntimeMetrics {
    pub fn count_rpc(&self) {
        self.rpc_calls.fetch_add(1, Ordering::Relaxed);
    }

    pub fn rpc(&self, kind: RpcKind) -> MetricTimer<'_> {
        MetricTimer::new(&self.rpc[kind.index()])
    }

    pub fn recovery_loop(&self) -> MetricTimer<'_> {
        MetricTimer::successful(&self.recovery_loop)
    }

    pub fn observe_tick_lag(&self, elapsed_us: u64) {
        self.tick_lag.observe(elapsed_us, TimerOutcome::Success);
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

    pub fn hook_sample_dropped(&self) {
        self.hook_samples_dropped.fetch_add(1, Ordering::Relaxed);
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
        let render_count = self.render_count.load(Ordering::Relaxed);
        let render_time_us = self.render_time_us.load(Ordering::Relaxed);
        let loop_stats = self.recovery_loop.snapshot();
        let tick_stats = self.tick_lag.snapshot();
        tracing::info!(
            rpc_calls,
            rpc_per_second = rpc_calls as f64 / elapsed_secs.max(0.001),
            active_streams = self.active_streams.load(Ordering::Relaxed),
            active_recoveries = self.active_recoveries.load(Ordering::Relaxed),
            queue_high_watermark = self.queue_high_watermark.load(Ordering::Relaxed),
            hook_samples_dropped = self.hook_samples_dropped.load(Ordering::Relaxed),
            loop_calls = loop_stats.calls,
            loop_mean_ms = loop_stats.mean_ms,
            loop_p50_ms = loop_stats.p50_ms,
            loop_p95_ms = loop_stats.p95_ms,
            loop_p99_ms = loop_stats.p99_ms,
            loop_max_ms = loop_stats.max_ms,
            tick_lag_calls = tick_stats.calls,
            tick_lag_mean_ms = tick_stats.mean_ms,
            tick_lag_p50_ms = tick_stats.p50_ms,
            tick_lag_p95_ms = tick_stats.p95_ms,
            tick_lag_p99_ms = tick_stats.p99_ms,
            tick_lag_max_ms = tick_stats.max_ms,
            io_bytes = self.io_bytes.load(Ordering::Relaxed),
            render_count,
            render_mean_ms = if render_count == 0 {
                0.0
            } else {
                render_time_us as f64 / render_count as f64 / 1_000.0
            },
            "runtime metrics snapshot"
        );

        for kind in RpcKind::ALL {
            let stats = self.rpc[kind.index()].snapshot();
            tracing::info!(
                rpc_method = kind.label(),
                calls = stats.calls,
                successes = stats.successes,
                errors = stats.errors,
                timeouts = stats.timeouts,
                mean_ms = stats.mean_ms,
                p50_ms = stats.p50_ms,
                p95_ms = stats.p95_ms,
                p99_ms = stats.p99_ms,
                max_ms = stats.max_ms,
                "RPC latency snapshot"
            );
        }
    }
}

#[derive(Clone, Copy)]
enum TimerOutcome {
    Success,
    Error,
    Timeout,
}

pub struct MetricTimer<'a> {
    stats: &'a LatencyStats,
    started: Instant,
    outcome: TimerOutcome,
}

impl<'a> MetricTimer<'a> {
    fn new(stats: &'a LatencyStats) -> Self {
        Self {
            stats,
            started: Instant::now(),
            outcome: TimerOutcome::Error,
        }
    }

    fn successful(stats: &'a LatencyStats) -> Self {
        Self {
            stats,
            started: Instant::now(),
            outcome: TimerOutcome::Success,
        }
    }

    pub fn success(mut self) {
        self.outcome = TimerOutcome::Success;
    }

    pub fn timeout(mut self) {
        self.outcome = TimerOutcome::Timeout;
    }
}

impl Drop for MetricTimer<'_> {
    fn drop(&mut self) {
        self.stats.observe(
            self.started.elapsed().as_micros().min(u64::MAX as u128) as u64,
            self.outcome,
        );
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
