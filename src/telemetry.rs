use std::time::Instant;

use crate::transform::{ObservedTransform, Transform};

/// SOURCE: PROJECT-DERIVED telemetry contract v1.
pub const DIRECT_SKEW_MS: f64 = 100.0;
/// SOURCE: PROJECT-DERIVED telemetry contract v1.
pub const MAX_EXTRAPOLATION_MS: f64 = 300.0;
/// SOURCE: PROJECT-DERIVED telemetry contract v1.
pub const SAMPLE_GAP_WARNING_MS: f64 = 300.0;
/// SOURCE: PROJECT-DERIVED telemetry contract v1.
pub const SAMPLE_GAP_INCOMPLETE_MS: f64 = 1_000.0;
/// SOURCE: PROJECT-DERIVED telemetry contract v1.
pub const ACTIVE_WATCHDOG_MS: u64 = 2_000;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AlignmentMethod {
    #[default]
    Direct,
    ExtrapolatedCarrier,
    ExtrapolatedPlane,
    Invalid,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TelemetryInvalidReason {
    NonFiniteTimestamp,
    TimeWentBackwards,
    MissingHistory,
    ExcessiveSkew,
    TelemetryGap,
}

#[derive(Debug, Clone)]
pub struct TelemetrySample {
    pub carrier_raw: Transform,
    pub plane_raw: Transform,
    pub carrier: Transform,
    pub plane: Transform,
    pub carrier_received_unix_ms: u64,
    pub plane_received_unix_ms: u64,
    pub skew_ms: f64,
    pub sample_gap_ms: f64,
    pub source_age_ms: f64,
    pub method: AlignmentMethod,
    pub invalid_reason: Option<TelemetryInvalidReason>,
}

impl TelemetrySample {
    pub fn is_valid(&self) -> bool {
        self.invalid_reason.is_none()
    }

    pub fn has_warning(&self) -> bool {
        self.sample_gap_ms > SAMPLE_GAP_WARNING_MS || self.source_age_ms > SAMPLE_GAP_WARNING_MS
    }

    pub fn from_replay(carrier: Transform, plane: Transform, previous_time: Option<f64>) -> Self {
        let sample_gap_ms = previous_time
            .map(|previous| ((carrier.time.max(plane.time) - previous).max(0.0)) * 1_000.0)
            .unwrap_or_default();
        let skew_ms = (carrier.time - plane.time).abs() * 1_000.0;
        let invalid_reason = if !carrier.time.is_finite() || !plane.time.is_finite() {
            Some(TelemetryInvalidReason::NonFiniteTimestamp)
        } else if skew_ms > MAX_EXTRAPOLATION_MS {
            Some(TelemetryInvalidReason::ExcessiveSkew)
        } else if sample_gap_ms > SAMPLE_GAP_INCOMPLETE_MS {
            Some(TelemetryInvalidReason::TelemetryGap)
        } else {
            None
        };
        Self {
            carrier_raw: carrier.clone(),
            plane_raw: plane.clone(),
            carrier,
            plane,
            carrier_received_unix_ms: 0,
            plane_received_unix_ms: 0,
            skew_ms,
            sample_gap_ms,
            source_age_ms: 0.0,
            method: if invalid_reason.is_some() {
                AlignmentMethod::Invalid
            } else {
                AlignmentMethod::Direct
            },
            invalid_reason,
        }
    }
}

#[derive(Debug, Default)]
pub struct TelemetryAligner {
    previous_carrier: Option<ObservedTransform>,
    previous_plane: Option<ObservedTransform>,
    previous_sample_at: Option<Instant>,
    previous_dcs_time: Option<f64>,
    previous_sample_valid: bool,
}

impl TelemetryAligner {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn align(
        &mut self,
        carrier_observed: ObservedTransform,
        plane_observed: ObservedTransform,
    ) -> TelemetrySample {
        let carrier_raw = carrier_observed.value.clone();
        let plane_raw = plane_observed.value.clone();
        let latest_received = carrier_observed.received_at.max(plane_observed.received_at);
        let sample_gap_ms = self
            .previous_sample_at
            .map(|previous| latest_received.duration_since(previous).as_secs_f64() * 1_000.0)
            .unwrap_or_default();
        let source_time = carrier_raw.time.max(plane_raw.time);
        let source_age_ms = match self.previous_dcs_time {
            Some(previous) if source_time <= previous => sample_gap_ms,
            _ => 0.0,
        };
        let skew_secs = (carrier_raw.time - plane_raw.time).abs();
        let skew_ms = skew_secs * 1_000.0;
        let history_valid = self.previous_sample_valid
            && sample_gap_ms <= SAMPLE_GAP_WARNING_MS
            && source_age_ms <= SAMPLE_GAP_WARNING_MS;

        let timestamps_finite = carrier_raw.time.is_finite() && plane_raw.time.is_finite();
        let time_went_backwards = self
            .previous_carrier
            .as_ref()
            .is_some_and(|previous| carrier_raw.time < previous.value.time)
            || self
                .previous_plane
                .as_ref()
                .is_some_and(|previous| plane_raw.time < previous.value.time);

        let mut carrier = carrier_raw.clone();
        let mut plane = plane_raw.clone();
        let (method, mut invalid_reason) = if !timestamps_finite {
            (
                AlignmentMethod::Invalid,
                Some(TelemetryInvalidReason::NonFiniteTimestamp),
            )
        } else if time_went_backwards {
            (
                AlignmentMethod::Invalid,
                Some(TelemetryInvalidReason::TimeWentBackwards),
            )
        } else if skew_ms <= DIRECT_SKEW_MS + 1.0e-6 {
            (AlignmentMethod::Direct, None)
        } else if skew_ms <= MAX_EXTRAPOLATION_MS {
            if carrier_raw.time < plane_raw.time {
                if self.previous_carrier.is_some() && history_valid {
                    extrapolate_position(&mut carrier, skew_secs);
                    carrier.time = plane_raw.time;
                    (AlignmentMethod::ExtrapolatedCarrier, None)
                } else {
                    (
                        AlignmentMethod::Invalid,
                        Some(TelemetryInvalidReason::MissingHistory),
                    )
                }
            } else if self.previous_plane.is_some() && history_valid {
                extrapolate_position(&mut plane, skew_secs);
                plane.time = carrier_raw.time;
                (AlignmentMethod::ExtrapolatedPlane, None)
            } else {
                (
                    AlignmentMethod::Invalid,
                    Some(TelemetryInvalidReason::MissingHistory),
                )
            }
        } else {
            (
                AlignmentMethod::Invalid,
                Some(TelemetryInvalidReason::ExcessiveSkew),
            )
        };

        if invalid_reason.is_none()
            && (sample_gap_ms > SAMPLE_GAP_INCOMPLETE_MS
                || source_age_ms > SAMPLE_GAP_INCOMPLETE_MS)
        {
            invalid_reason = Some(TelemetryInvalidReason::TelemetryGap);
        }

        let sample = TelemetrySample {
            carrier_raw,
            plane_raw,
            carrier,
            plane,
            carrier_received_unix_ms: carrier_observed.received_unix_ms,
            plane_received_unix_ms: plane_observed.received_unix_ms,
            skew_ms,
            sample_gap_ms,
            source_age_ms,
            method: if invalid_reason.is_some() {
                AlignmentMethod::Invalid
            } else {
                method
            },
            invalid_reason,
        };

        self.previous_carrier = Some(carrier_observed);
        self.previous_plane = Some(plane_observed);
        self.previous_sample_at = Some(latest_received);
        self.previous_dcs_time = Some(source_time);
        self.previous_sample_valid = sample.is_valid() && !sample.has_warning();
        sample
    }

    pub fn reset(&mut self) {
        *self = Self::new();
    }
}

fn extrapolate_position(transform: &mut Transform, seconds: f64) {
    transform.position += transform.velocity * seconds;
    transform.alt += transform.velocity.y * seconds;
}

#[cfg(test)]
mod tests {
    use super::*;
    use ultraviolet::DVec3;

    fn observed(time: f64, velocity_x: f64) -> ObservedTransform {
        let value = Transform {
            time,
            velocity: DVec3::new(velocity_x, 0.0, 0.0),
            ..Transform::default()
        };
        ObservedTransform::now(value)
    }

    #[test]
    fn direct_alignment_accepts_100_ms_boundary() {
        let mut aligner = TelemetryAligner::new();
        let sample = aligner.align(observed(10.0, 0.0), observed(10.1, 0.0));
        assert!(sample.is_valid());
        assert_eq!(sample.method, AlignmentMethod::Direct);
    }

    #[test]
    fn short_extrapolation_requires_history() {
        let mut aligner = TelemetryAligner::new();
        let first = aligner.align(observed(10.0, 10.0), observed(10.2, 0.0));
        assert_eq!(
            first.invalid_reason,
            Some(TelemetryInvalidReason::MissingHistory)
        );

        let history = aligner.align(observed(10.3, 10.0), observed(10.3, 0.0));
        assert!(history.is_valid());

        let second = aligner.align(observed(10.4, 10.0), observed(10.6, 0.0));
        assert!(second.is_valid());
        assert_eq!(second.method, AlignmentMethod::ExtrapolatedCarrier);
        assert!((second.carrier.position.x - 2.0).abs() < 1.0e-9);
    }

    #[test]
    fn exact_300_ms_skew_is_allowed_only_with_valid_history() {
        let mut aligner = TelemetryAligner::new();
        assert!(aligner
            .align(observed(1.0, 1.0), observed(1.0, 0.0))
            .is_valid());
        let sample = aligner.align(observed(2.0, 1.0), observed(2.3, 0.0));
        assert!(sample.is_valid());
        assert_eq!(sample.method, AlignmentMethod::ExtrapolatedCarrier);
    }

    #[test]
    fn time_reversal_invalidates_the_sample() {
        let mut aligner = TelemetryAligner::new();
        assert!(aligner
            .align(observed(2.0, 0.0), observed(2.0, 0.0))
            .is_valid());
        let sample = aligner.align(observed(1.9, 0.0), observed(2.1, 0.0));
        assert_eq!(
            sample.invalid_reason,
            Some(TelemetryInvalidReason::TimeWentBackwards)
        );
    }

    #[test]
    fn reset_forbids_extrapolation_across_a_cut_or_session_change() {
        let mut aligner = TelemetryAligner::new();
        assert!(aligner
            .align(observed(1.0, 5.0), observed(1.0, 0.0))
            .is_valid());
        aligner.reset();
        let sample = aligner.align(observed(2.0, 5.0), observed(2.2, 0.0));
        assert_eq!(
            sample.invalid_reason,
            Some(TelemetryInvalidReason::MissingHistory)
        );
    }

    #[test]
    fn replay_gap_above_one_second_is_incomplete_evidence() {
        let carrier = Transform {
            time: 2.1,
            ..Transform::default()
        };
        let plane = carrier.clone();
        let sample = TelemetrySample::from_replay(carrier, plane, Some(1.0));
        assert_eq!(
            sample.invalid_reason,
            Some(TelemetryInvalidReason::TelemetryGap)
        );
    }

    #[test]
    fn skew_above_300_ms_is_invalid() {
        let mut aligner = TelemetryAligner::new();
        let sample = aligner.align(observed(10.0, 0.0), observed(10.301, 0.0));
        assert_eq!(
            sample.invalid_reason,
            Some(TelemetryInvalidReason::ExcessiveSkew)
        );
    }
}
