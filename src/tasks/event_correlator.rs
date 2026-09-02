use stubs::common::v0::Unit;

use crate::track::{Grading, Track};
use crate::transform::Transform;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EventStreamStatus {
    Available,
    Disabled,
    Unavailable,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct EventCorrelationSummary {
    pub stream_status: EventStreamStatus,
    pub detail: Option<String>,
    pub outcome_evidence_seen_before_unavailability: bool,
    pub outcome_confirmed: bool,
}

#[derive(Debug)]
pub struct TouchdownCorrelation {
    pub carrier: Option<Transform>,
    pub plane: Option<Transform>,
    pub accepted: bool,
}

/// Owns event identity/correlation state. It never changes positional telemetry
/// completeness: stream health is recorded only as an event diagnostic.
#[derive(Debug)]
pub struct EventCorrelator {
    plane_id: u32,
    carrier_id: u32,
    stream_status: EventStreamStatus,
    stream_detail: Option<String>,
    outcome_evidence_seen: bool,
}

impl EventCorrelator {
    pub fn new(plane_id: u32, carrier_id: u32) -> Self {
        Self {
            plane_id,
            carrier_id,
            stream_status: EventStreamStatus::Available,
            stream_detail: None,
            outcome_evidence_seen: false,
        }
    }

    pub fn disabled(plane_id: u32, carrier_id: u32) -> Self {
        Self {
            plane_id,
            carrier_id,
            stream_status: EventStreamStatus::Disabled,
            stream_detail: Some("positions_only".to_string()),
            outcome_evidence_seen: false,
        }
    }

    pub fn accepts_pair(&self, plane_id: u32, carrier_id: u32) -> bool {
        plane_id == self.plane_id && carrier_id == self.carrier_id
    }

    pub fn is_tracked_unit(&self, unit_id: u32) -> bool {
        unit_id == self.plane_id || unit_id == self.carrier_id
    }

    pub fn landing_quality_mark(&mut self, track: &mut Track, time: f64, comment: String) -> bool {
        let accepted = track.set_dcs_grading(comment);
        track.record_event(
            "landing_quality_mark",
            time,
            accepted,
            if accepted {
                "first_matching_event"
            } else {
                "duplicate_ignored"
            },
        );
        self.outcome_evidence_seen |= accepted;
        accepted
    }

    pub fn touchdown(
        &mut self,
        track: &mut Track,
        kind: &'static str,
        time: f64,
        carrier: Unit,
        plane: Unit,
    ) -> TouchdownCorrelation {
        let carrier = transform_from_event_unit(time, carrier);
        let plane = transform_from_event_unit(time, plane);
        let Some((carrier_transform, plane_transform)) = carrier.clone().zip(plane.clone()) else {
            track.record_event(kind, time, false, "missing_transform_evidence");
            return TouchdownCorrelation {
                carrier,
                plane,
                accepted: false,
            };
        };
        let accepted = track.landed(&carrier_transform, &plane_transform);
        track.record_event(
            kind,
            time,
            accepted,
            if accepted {
                "ids_and_geometry_correlated"
            } else {
                "duplicate_or_geometry_rejected"
            },
        );
        self.outcome_evidence_seen |= accepted;
        TouchdownCorrelation {
            carrier: Some(carrier_transform),
            plane: Some(plane_transform),
            accepted,
        }
    }

    pub fn stream_unavailable(&mut self, track: &mut Track, detail: impl Into<String>) {
        if self.stream_status != EventStreamStatus::Available {
            return;
        }
        let detail = detail.into();
        self.stream_status = EventStreamStatus::Unavailable;
        self.stream_detail = Some(detail.clone());
        track.mark_event_stream_unavailable(detail);
    }

    pub fn summary(&self, grading: &Grading) -> EventCorrelationSummary {
        let outcome_confirmed = match grading {
            Grading::Recovered { cable: Some(_), .. } => self.outcome_evidence_seen,
            Grading::Bolter | Grading::TouchAndGo { .. } => true,
            Grading::WaveoffUnknown | Grading::Unknown | Grading::Recovered { cable: None, .. } => {
                false
            }
        };
        EventCorrelationSummary {
            stream_status: self.stream_status.clone(),
            detail: self.stream_detail.clone(),
            outcome_evidence_seen_before_unavailability: self.outcome_evidence_seen,
            outcome_confirmed,
        }
    }
}

pub(crate) fn transform_from_event_unit(time: f64, unit: Unit) -> Option<Transform> {
    Some(Transform::from((
        time,
        unit.position?,
        unit.orientation?,
        unit.velocity.unwrap_or_default(),
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stream_failure_is_scoped_to_event_diagnostics() {
        let carrier = crate::data::CarrierInfo::by_type("CVN_71").unwrap();
        let plane = crate::data::AirplaneInfo::by_type("FA-18C_hornet").unwrap();
        let mut track = Track::new("pilot", carrier, plane);
        let mut correlator = EventCorrelator::new(10, 20);

        correlator.stream_unavailable(&mut track, "grpc unavailable");

        assert_eq!(
            track.finish().telemetry_quality.completeness,
            crate::track::Completeness::InsufficientGates
        );
    }
}
