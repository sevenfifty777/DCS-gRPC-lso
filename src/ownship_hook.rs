use std::time::{Instant, SystemTime, UNIX_EPOCH};

use stubs::hook::v0::{GetOwnshipHookStateResponse, OwnshipHookObservationStatus};
use tokio::sync::mpsc;

use crate::client::HookClient;
use crate::tasks::HookSamplingConfig;

const MAX_EVIDENCE: usize = 512;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
enum SampleStatus {
    Observed,
    Unavailable,
    IdentityUnavailable,
    IdentityMismatch,
    Timeout,
    Unimplemented,
    Error,
    Stale,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
struct SampleEvidence {
    model_time_dcs: Option<f64>,
    observed_unix_ms: u64,
    age_ms: f64,
    aircraft_type: String,
    ownship_unit_id: Option<u32>,
    identity_matches: Option<bool>,
    status_value: Option<f64>,
    value: Option<f64>,
    status: SampleStatus,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub(crate) struct OwnshipHookObservation {
    evidence_role: &'static str,
    target_unit_id: u32,
    observed_samples: u32,
    unavailable_samples: u32,
    identity_unavailable_samples: u32,
    identity_mismatch_samples: u32,
    timeout_samples: u32,
    unimplemented_samples: u32,
    error_samples: u32,
    stale_samples: u32,
    compacted_samples: u32,
    timeline: Vec<SampleEvidence>,
}

impl OwnshipHookObservation {
    pub(crate) fn new(target_unit_id: u32) -> Self {
        Self {
            evidence_role: "diagnostic_only_pending_live_validation",
            target_unit_id,
            observed_samples: 0,
            unavailable_samples: 0,
            identity_unavailable_samples: 0,
            identity_mismatch_samples: 0,
            timeout_samples: 0,
            unimplemented_samples: 0,
            error_samples: 0,
            stale_samples: 0,
            compacted_samples: 0,
            timeline: Vec::new(),
        }
    }

    fn observe(&mut self, mut poll: Poll, frequency_hz: u64) {
        let age_ms = poll.received_at.elapsed().as_secs_f64() * 1_000.0;
        let max_age_ms = (2_000.0 / frequency_hz.max(1) as f64).max(750.0);
        if poll.status == SampleStatus::Observed && age_ms > max_age_ms {
            poll.status = SampleStatus::Stale;
        }
        self.count(poll.status);

        if self.timeline.len() == MAX_EVIDENCE {
            self.timeline.remove(0);
            self.compacted_samples += 1;
        }
        self.timeline.push(SampleEvidence {
            model_time_dcs: poll.model_time_dcs,
            observed_unix_ms: poll.received_unix_ms,
            age_ms,
            aircraft_type: poll.aircraft_type,
            ownship_unit_id: poll.ownship_unit_id,
            identity_matches: poll.identity_matches,
            status_value: poll.status_value,
            value: poll.value,
            status: poll.status,
        });
    }

    fn count(&mut self, status: SampleStatus) {
        match status {
            SampleStatus::Observed => self.observed_samples += 1,
            SampleStatus::Unavailable => self.unavailable_samples += 1,
            SampleStatus::IdentityUnavailable => self.identity_unavailable_samples += 1,
            SampleStatus::IdentityMismatch => self.identity_mismatch_samples += 1,
            SampleStatus::Timeout => self.timeout_samples += 1,
            SampleStatus::Unimplemented => self.unimplemented_samples += 1,
            SampleStatus::Error => self.error_samples += 1,
            SampleStatus::Stale => self.stale_samples += 1,
        }
    }
}

#[derive(Debug)]
struct Poll {
    received_at: Instant,
    received_unix_ms: u64,
    model_time_dcs: Option<f64>,
    aircraft_type: String,
    ownship_unit_id: Option<u32>,
    identity_matches: Option<bool>,
    status_value: Option<f64>,
    value: Option<f64>,
    status: SampleStatus,
}

pub(crate) struct OwnshipHookSampler {
    task: tokio::task::JoinHandle<()>,
    rx: mpsc::Receiver<Poll>,
    frequency_hz: u64,
}

impl OwnshipHookSampler {
    pub(crate) fn start(
        channel: tonic::transport::Channel,
        target_unit_id: u32,
        config: HookSamplingConfig,
    ) -> Self {
        let (tx, rx) = mpsc::channel(64);
        let task = tokio::spawn(sample(channel, target_unit_id, config, tx));
        Self {
            task,
            rx,
            frequency_hz: config.frequency_hz,
        }
    }

    pub(crate) fn drain(&mut self, observation: &mut OwnshipHookObservation) {
        while let Ok(poll) = self.rx.try_recv() {
            observation.observe(poll, self.frequency_hz);
        }
    }
}

impl Drop for OwnshipHookSampler {
    fn drop(&mut self) {
        self.task.abort();
    }
}

async fn sample(
    channel: tonic::transport::Channel,
    target_unit_id: u32,
    config: HookSamplingConfig,
    tx: mpsc::Sender<Poll>,
) {
    let period = std::time::Duration::from_secs_f64(1.0 / config.frequency_hz as f64);
    let mut interval = tokio::time::interval(period);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut client = HookClient::new(channel);
    loop {
        interval.tick().await;
        let poll = poll_once(&mut client, target_unit_id, config).await;
        let stop = poll.status == SampleStatus::Unimplemented;
        if tx.try_send(poll).is_err() {
            crate::metrics::RUNTIME_METRICS.hook_sample_dropped();
        }
        if stop {
            break;
        }
    }
}

async fn poll_once(
    client: &mut HookClient,
    target_unit_id: u32,
    config: HookSamplingConfig,
) -> Poll {
    match client
        .get_ownship_hook_state_with_timeout(config.timeout)
        .await
    {
        Ok(response) => Poll::from_response(response, target_unit_id),
        Err(error) if error.code() == tonic::Code::DeadlineExceeded => {
            Poll::failure(SampleStatus::Timeout)
        }
        Err(error) if error.code() == tonic::Code::Unimplemented => {
            Poll::failure(SampleStatus::Unimplemented)
        }
        Err(_) => Poll::failure(SampleStatus::Error),
    }
}

impl Poll {
    fn from_response(response: GetOwnshipHookStateResponse, target_unit_id: u32) -> Self {
        let status_value = response.status_value.filter(|value| value.is_finite());
        let value = response.value.filter(|value| value.is_finite());
        let identity_matches = response
            .ownship_unit_id
            .map(|ownship_unit_id| ownship_unit_id == target_unit_id);
        let observation_status =
            OwnshipHookObservationStatus::try_from(response.observation_status)
                .unwrap_or(OwnshipHookObservationStatus::Unspecified);
        let status = classify(observation_status, identity_matches, status_value, value);
        Self {
            received_at: Instant::now(),
            received_unix_ms: unix_time_ms(),
            model_time_dcs: response
                .model_time
                .is_finite()
                .then_some(response.model_time),
            aircraft_type: response.aircraft_type,
            ownship_unit_id: response.ownship_unit_id,
            identity_matches,
            status_value,
            value,
            status,
        }
    }

    fn failure(status: SampleStatus) -> Self {
        Self {
            received_at: Instant::now(),
            received_unix_ms: unix_time_ms(),
            model_time_dcs: None,
            aircraft_type: String::new(),
            ownship_unit_id: None,
            identity_matches: None,
            status_value: None,
            value: None,
            status,
        }
    }
}

fn classify(
    observation_status: OwnshipHookObservationStatus,
    identity_matches: Option<bool>,
    status_value: Option<f64>,
    value: Option<f64>,
) -> SampleStatus {
    match observation_status {
        OwnshipHookObservationStatus::Observed if status_value.is_none() && value.is_none() => {
            SampleStatus::Error
        }
        OwnshipHookObservationStatus::Observed if identity_matches == Some(true) => {
            SampleStatus::Observed
        }
        OwnshipHookObservationStatus::Observed if identity_matches == Some(false) => {
            SampleStatus::IdentityMismatch
        }
        OwnshipHookObservationStatus::Observed => SampleStatus::IdentityUnavailable,
        OwnshipHookObservationStatus::Unavailable => SampleStatus::Unavailable,
        OwnshipHookObservationStatus::Unspecified => SampleStatus::Error,
    }
}

fn unix_time_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u64::MAX as u128) as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn observation_rejects_mismatch_and_stale_samples() {
        let mut observation = OwnshipHookObservation::new(42);
        let mut stale = Poll::failure(SampleStatus::Observed);
        stale.received_at = Instant::now() - std::time::Duration::from_secs(2);
        stale.ownship_unit_id = Some(42);
        stale.identity_matches = Some(true);
        stale.status_value = Some(1.0);
        observation.observe(stale, 4);

        let mut mismatch = Poll::failure(SampleStatus::IdentityMismatch);
        mismatch.ownship_unit_id = Some(99);
        mismatch.identity_matches = Some(false);
        observation.observe(mismatch, 4);

        assert_eq!(observation.observed_samples, 0);
        assert_eq!(observation.stale_samples, 1);
        assert_eq!(observation.identity_mismatch_samples, 1);
        assert_eq!(observation.timeline[0].status, SampleStatus::Stale);
        assert_eq!(
            observation.timeline[1].status,
            SampleStatus::IdentityMismatch
        );
    }

    #[test]
    fn response_requires_values_and_matching_ownship_identity() {
        assert_eq!(
            classify(
                OwnshipHookObservationStatus::Observed,
                Some(true),
                Some(0.0),
                Some(1.0),
            ),
            SampleStatus::Observed
        );
        assert_eq!(
            classify(
                OwnshipHookObservationStatus::Observed,
                Some(true),
                None,
                None,
            ),
            SampleStatus::Error
        );
        assert_eq!(
            classify(
                OwnshipHookObservationStatus::Observed,
                None,
                Some(0.0),
                Some(1.0),
            ),
            SampleStatus::IdentityUnavailable
        );
    }
}
