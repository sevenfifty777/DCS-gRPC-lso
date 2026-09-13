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
    pub unavailability_count: u32,
    pub reconnection_count: u32,
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
    dcs_waveoff_evidence_seen: bool,
    unavailability_count: u32,
    reconnection_count: u32,
}

impl EventCorrelator {
    pub fn new(plane_id: u32, carrier_id: u32) -> Self {
        Self {
            plane_id,
            carrier_id,
            stream_status: EventStreamStatus::Available,
            stream_detail: None,
            outcome_evidence_seen: false,
            dcs_waveoff_evidence_seen: false,
            unavailability_count: 0,
            reconnection_count: 0,
        }
    }

    pub fn disabled(plane_id: u32, carrier_id: u32) -> Self {
        Self {
            plane_id,
            carrier_id,
            stream_status: EventStreamStatus::Disabled,
            stream_detail: Some("positions_only".to_string()),
            outcome_evidence_seen: false,
            dcs_waveoff_evidence_seen: false,
            unavailability_count: 0,
            reconnection_count: 0,
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
        self.dcs_waveoff_evidence_seen |= accepted && track.has_dcs_waveoff_evidence();
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
        self.unavailability_count = self.unavailability_count.saturating_add(1);
        track.mark_event_stream_unavailable(detail);
    }

    pub fn stream_available(&mut self, track: &mut Track) {
        if self.stream_status != EventStreamStatus::Unavailable {
            return;
        }
        self.stream_status = EventStreamStatus::Available;
        self.stream_detail = None;
        self.reconnection_count = self.reconnection_count.saturating_add(1);
        let timestamp = track.last_observed_time_dcs().unwrap_or_default();
        track.record_event(
            "event_stream_reconnected",
            timestamp,
            true,
            "session_event_stream_restored",
        );
    }

    pub fn summary(&self, grading: &Grading) -> EventCorrelationSummary {
        let outcome_confirmed = match grading {
            Grading::Recovered { cable: Some(_), .. } => self.outcome_evidence_seen,
            Grading::Bolter | Grading::TouchAndGo { .. } => true,
            Grading::WaveoffUnknown => self.dcs_waveoff_evidence_seen,
            Grading::Unknown | Grading::ApproachOnly | Grading::Recovered { cable: None, .. } => {
                false
            }
        };
        EventCorrelationSummary {
            stream_status: self.stream_status.clone(),
            detail: self.stream_detail.clone(),
            outcome_evidence_seen_before_unavailability: self.outcome_evidence_seen,
            outcome_confirmed,
            unavailability_count: self.unavailability_count,
            reconnection_count: self.reconnection_count,
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

    fn catobar_track() -> Track {
        let carrier = crate::data::CarrierInfo::by_type("CVN_71").unwrap();
        let plane = crate::data::AirplaneInfo::by_type("FA-18C_hornet").unwrap();
        Track::new("pilot", carrier, plane)
    }

    #[test]
    fn stream_failure_is_scoped_to_event_diagnostics() {
        let mut track = catobar_track();
        let mut correlator = EventCorrelator::new(10, 20);

        correlator.stream_unavailable(&mut track, "grpc unavailable");

        assert_eq!(
            track.finish().telemetry_quality.completeness,
            crate::track::Completeness::InsufficientGates
        );
    }

    #[test]
    fn stream_reconnection_is_explicit_and_preserves_prior_outcome_evidence() {
        let mut track = catobar_track();
        let mut correlator = EventCorrelator::new(10, 20);
        correlator.stream_unavailable(&mut track, "unavailable");
        correlator.stream_available(&mut track);

        let summary = correlator.summary(&Grading::ApproachOnly);
        assert_eq!(summary.stream_status, EventStreamStatus::Available);
        assert_eq!(summary.unavailability_count, 1);
        assert_eq!(summary.reconnection_count, 1);
        assert!(track
            .finish()
            .events
            .iter()
            .any(|event| event.kind == "event_stream_reconnected"));
    }

    #[test]
    fn separate_waveoff_tracks_do_not_consume_a_later_wire_lqm() {
        // Regression for the 7 September 2026 human F-14B(U) session: one recorder retained two
        // GRADE:WO comments and then a WIRE# 2 trap.  Because the first LQM won forever, both the
        // second waveoff and the authoritative wire were marked as duplicates in one report.
        for (time, comment) in [
            (
                100.0,
                "LSO: GRADE:WO _LOIC_ _LOAR_ _LULIM_ (EGTL) (DLIM) WO(AFU)IC [BC]",
            ),
            (200.0, "LSO: GRADE:WO _LULIM_ _LULIC_ WO(AFU)IC [BC]"),
        ] {
            let mut track = catobar_track();
            let mut correlator = EventCorrelator::new(10, 20);
            assert!(correlator.landing_quality_mark(&mut track, time, comment.to_string()));

            let result = track.finish();
            assert_eq!(result.grading, Grading::WaveoffUnknown);
            assert!(correlator.summary(&result.grading).outcome_confirmed);
            assert_eq!(result.events.len(), 1);
            assert!(result.events[0].accepted);
        }

        let mut trap = catobar_track();
        let mut trap_correlator = EventCorrelator::new(10, 20);
        assert!(trap_correlator.landing_quality_mark(
            &mut trap,
            300.0,
            "LSO: GRADE:C : WX WIRE# 2 _EGIW_ [BC]".to_string(),
        ));

        let result = trap.finish();
        assert_eq!(
            result.grading,
            Grading::Recovered {
                cable: Some(2),
                cable_estimated: None,
            }
        );
        assert!(trap_correlator.summary(&result.grading).outcome_confirmed);
        assert_eq!(result.events.len(), 1);
        assert!(result.events[0].accepted);
    }
}
