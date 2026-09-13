use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::sync::Mutex;

use stubs::mission::v0::stream_events_response::Event;
use tokio::sync::broadcast;

const EVENT_JOURNAL_CAPACITY: usize = 512;

#[derive(Debug, Clone)]
pub struct SessionEvent {
    pub sequence: u64,
    pub time_dcs: f64,
    pub event: Event,
}

#[derive(Debug, Clone)]
pub enum SessionEventMessage {
    Event {
        sequence: u64,
        time_dcs: f64,
        event: Box<Event>,
    },
    StreamUnavailable(String),
    StreamAvailable,
}

#[derive(Debug)]
struct State {
    next_sequence: u64,
    journal: VecDeque<SessionEvent>,
    available: bool,
    detail: Option<String>,
}

/// One reconnecting session-level event feed shared by every recovery. The bounded journal closes
/// the race between detector hand-off and recorder subscription without allowing events from an
/// earlier attempt to be consumed: subscriptions replay only entries at or after their DCS start
/// time, and `EventCorrelator` still requires exact aircraft/carrier IDs.
#[derive(Debug)]
pub struct SessionEventHub {
    state: Mutex<State>,
    sender: broadcast::Sender<SessionEventMessage>,
}

impl Default for SessionEventHub {
    fn default() -> Self {
        let (sender, _) = broadcast::channel(EVENT_JOURNAL_CAPACITY);
        Self {
            state: Mutex::new(State {
                next_sequence: 1,
                journal: VecDeque::with_capacity(EVENT_JOURNAL_CAPACITY),
                available: false,
                detail: Some("event_stream_connecting".to_string()),
            }),
            sender,
        }
    }
}

impl SessionEventHub {
    pub fn publish(&self, time_dcs: f64, event: Event) {
        let published = {
            let mut state = self.state.lock().expect("event hub mutex poisoned");
            let published = SessionEvent {
                sequence: state.next_sequence,
                time_dcs,
                event,
            };
            state.next_sequence = state.next_sequence.saturating_add(1);
            if state.journal.len() == EVENT_JOURNAL_CAPACITY {
                state.journal.pop_front();
            }
            state.journal.push_back(published.clone());
            published
        };
        let _ = self.sender.send(SessionEventMessage::Event {
            sequence: published.sequence,
            time_dcs: published.time_dcs,
            event: Box::new(published.event),
        });
    }

    pub fn mark_unavailable(&self, detail: impl Into<String>) {
        let detail = detail.into();
        let changed = {
            let mut state = self.state.lock().expect("event hub mutex poisoned");
            let changed = state.available || state.detail.as_deref() != Some(detail.as_str());
            state.available = false;
            state.detail = Some(detail.clone());
            changed
        };
        if changed {
            let _ = self
                .sender
                .send(SessionEventMessage::StreamUnavailable(detail));
        }
    }

    pub fn mark_available(&self) {
        let changed = {
            let mut state = self.state.lock().expect("event hub mutex poisoned");
            let changed = !state.available;
            state.available = true;
            state.detail = None;
            changed
        };
        if changed {
            let _ = self.sender.send(SessionEventMessage::StreamAvailable);
        }
    }

    pub fn subscribe(&self, started_at_dcs: f64) -> (SessionEventSubscription, Arc<AtomicU64>) {
        let receiver = self.sender.subscribe();
        let (replay, initial_status) = {
            let state = self.state.lock().expect("event hub mutex poisoned");
            let replay = state
                .journal
                .iter()
                .filter(|event| event.time_dcs >= started_at_dcs)
                .cloned()
                .map(|event| SessionEventMessage::Event {
                    sequence: event.sequence,
                    time_dcs: event.time_dcs,
                    event: Box::new(event.event),
                })
                .collect();
            let initial_status = (!state.available).then(|| {
                SessionEventMessage::StreamUnavailable(
                    state
                        .detail
                        .clone()
                        .unwrap_or_else(|| "event_stream_unavailable".to_string()),
                )
            });
            (replay, initial_status)
        };
        let progress = Arc::new(AtomicU64::new(0));
        (
            SessionEventSubscription {
                replay,
                initial_status,
                receiver,
                progress: progress.clone(),
            },
            progress,
        )
    }

    pub fn snapshot_since(&self, started_at_dcs: f64) -> Vec<SessionEvent> {
        self.state
            .lock()
            .expect("event hub mutex poisoned")
            .journal
            .iter()
            .filter(|event| event.time_dcs >= started_at_dcs)
            .cloned()
            .collect()
    }

    pub fn status(&self) -> (bool, Option<String>) {
        let state = self.state.lock().expect("event hub mutex poisoned");
        (state.available, state.detail.clone())
    }
}

#[derive(Debug)]
pub struct SessionEventSubscription {
    replay: VecDeque<SessionEventMessage>,
    initial_status: Option<SessionEventMessage>,
    receiver: broadcast::Receiver<SessionEventMessage>,
    progress: Arc<AtomicU64>,
}

impl SessionEventSubscription {
    pub async fn recv(&mut self) -> SessionEventMessage {
        loop {
            let message = if let Some(event) = self.replay.pop_front() {
                event
            } else if let Some(status) = self.initial_status.take() {
                status
            } else {
                match self.receiver.recv().await {
                    Ok(message) => message,
                    Err(broadcast::error::RecvError::Lagged(skipped)) => {
                        return SessionEventMessage::StreamUnavailable(format!(
                            "local_event_subscriber_lagged:{skipped}"
                        ));
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        return SessionEventMessage::StreamUnavailable(
                            "session_event_hub_closed".to_string(),
                        );
                    }
                }
            };
            match &message {
                SessionEventMessage::Event { sequence, .. }
                    if *sequence <= self.progress.load(Ordering::Relaxed) =>
                {
                    continue;
                }
                SessionEventMessage::Event { sequence, .. } => {
                    self.progress.store(*sequence, Ordering::Relaxed)
                }
                _ => {}
            }
            return message;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use stubs::mission::v0::stream_events_response::Event;

    #[tokio::test]
    async fn subscription_replays_only_current_attempt_then_receives_live_events() {
        let hub = SessionEventHub::default();
        hub.mark_available();
        hub.publish(10.0, Event::MissionStart(Default::default()));
        hub.publish(20.0, Event::MissionEnd(Default::default()));

        let (mut subscription, _) = hub.subscribe(15.0);
        let SessionEventMessage::Event {
            sequence: replayed_sequence,
            time_dcs: replayed_time,
            ..
        } = subscription.recv().await
        else {
            panic!("expected replayed event");
        };
        assert_eq!(replayed_time, 20.0);

        hub.publish(30.0, Event::MissionStart(Default::default()));
        let SessionEventMessage::Event {
            sequence: live_sequence,
            time_dcs: live_time,
            ..
        } = subscription.recv().await
        else {
            panic!("expected live event");
        };
        assert_eq!(live_time, 30.0);
        assert!(live_sequence > replayed_sequence);
    }
}
