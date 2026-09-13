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
    /// A position, altitude, orientation or velocity component is `NaN`/infinite (see
    /// `Transform::has_finite_geometry`). Checked on every acquisition path, so the tracker
    /// never has to defend against it.
    NonFiniteValue,
    TimeWentBackwards,
    MissingHistory,
    ExcessiveSkew,
    TelemetryGap,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceObservationEntity {
    Aircraft,
    Carrier,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ScoringSegmentAttribution {
    BeforeGroove,
    InScoredSegment,
    AfterTouchdown,
    IndeterminateMissingSourceTime,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceTimeAttributionBasis {
    CaptureTime,
    SequenceAndCaptureTickBounds,
    Unresolved,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum InvalidSourceVerdictEffect {
    DiagnosticOutsideScoredSegment,
    DiagnosticCoveredShortGap,
    BlockingCoverageGap,
    BlockingIndeterminateMissingSourceTime,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SourceCaptureAnchor {
    pub sequence: u64,
    pub capture_tick: u64,
    pub capture_time_dcs: f64,
}

/// One source-side unit observation that could not produce a paired position sample. The source
/// capture clock and client receipt clock stay separate; attribution is filled only after the
/// final groove/touchdown bounds are known.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct InvalidSourceObservation {
    pub sequence: u64,
    pub capture_tick: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capture_time_dcs: Option<f64>,
    pub entity: SourceObservationEntity,
    pub status_code: i32,
    pub status: String,
    pub reason: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_read_time_dcs: Option<f64>,
    pub received_unix_ms: u64,
    pub attribution: ScoringSegmentAttribution,
    pub attribution_basis: SourceTimeAttributionBasis,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_time_lower_bound_dcs: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_time_upper_bound_dcs: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub coverage_gap_ms: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous_valid_sequence: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_valid_sequence: Option<u64>,
    pub verdict_effect: InvalidSourceVerdictEffect,
    pub affects_scoring: bool,
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
    /// Buffered-source identity used only to prove real capture coverage. Replay and unary
    /// samples have no source sequence/tick and therefore never manufacture such an anchor.
    pub source_sequence: Option<u64>,
    pub source_capture_tick: Option<u64>,
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
        } else if !carrier.has_finite_geometry() || !plane.has_finite_geometry() {
            Some(TelemetryInvalidReason::NonFiniteValue)
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
            source_sequence: None,
            source_capture_tick: None,
        }
    }

    pub fn from_source_pair(
        carrier: Transform,
        plane: Transform,
        previous_time: Option<f64>,
        source_age_ms: f64,
    ) -> Self {
        let received = crate::transform::ObservedTransform::now(carrier.clone());
        let received_unix_ms = received.received_unix_ms;
        let sample_gap_ms = previous_time
            .map(|previous| ((carrier.time.max(plane.time) - previous).max(0.0)) * 1_000.0)
            .unwrap_or_default();
        let skew_ms = (carrier.time - plane.time).abs() * 1_000.0;
        let invalid_reason =
            if !carrier.time.is_finite() || !plane.time.is_finite() || !source_age_ms.is_finite() {
                Some(TelemetryInvalidReason::NonFiniteTimestamp)
            } else if !carrier.has_finite_geometry() || !plane.has_finite_geometry() {
                Some(TelemetryInvalidReason::NonFiniteValue)
            } else if previous_time.is_some_and(|previous| carrier.time.max(plane.time) < previous)
            {
                Some(TelemetryInvalidReason::TimeWentBackwards)
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
            carrier_received_unix_ms: received_unix_ms,
            plane_received_unix_ms: received_unix_ms,
            skew_ms,
            sample_gap_ms,
            source_age_ms,
            method: if invalid_reason.is_some() {
                AlignmentMethod::Invalid
            } else {
                AlignmentMethod::Direct
            },
            invalid_reason,
            source_sequence: None,
            source_capture_tick: None,
        }
    }
}

#[derive(Debug, Default)]
pub struct TelemetryAligner {
    previous_carrier: Option<ObservedTransform>,
    previous_plane: Option<ObservedTransform>,
    previous_sample_at: Option<Instant>,
    carrier_last_advanced_at: Option<Instant>,
    plane_last_advanced_at: Option<Instant>,
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
        let carrier_source_age_ms = source_age_ms(
            carrier_raw.time,
            self.previous_carrier.as_ref(),
            self.carrier_last_advanced_at,
            latest_received,
        );
        let plane_source_age_ms = source_age_ms(
            plane_raw.time,
            self.previous_plane.as_ref(),
            self.plane_last_advanced_at,
            latest_received,
        );
        let source_age_ms = carrier_source_age_ms.max(plane_source_age_ms);
        let skew_secs = (carrier_raw.time - plane_raw.time).abs();
        let skew_ms = skew_secs * 1_000.0;
        let history_valid = self.previous_sample_valid
            && sample_gap_ms <= SAMPLE_GAP_WARNING_MS
            && source_age_ms <= SAMPLE_GAP_WARNING_MS;

        let timestamps_finite = carrier_raw.time.is_finite() && plane_raw.time.is_finite();
        let geometry_finite = carrier_raw.has_finite_geometry() && plane_raw.has_finite_geometry();
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
        } else if !geometry_finite {
            // Checked before any extrapolation: a NaN velocity would otherwise be folded into
            // the corrected position of an otherwise plausible sample.
            (
                AlignmentMethod::Invalid,
                Some(TelemetryInvalidReason::NonFiniteValue),
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

        let carrier_advanced = self
            .previous_carrier
            .as_ref()
            .is_none_or(|previous| carrier_raw.time > previous.value.time);
        let plane_advanced = self
            .previous_plane
            .as_ref()
            .is_none_or(|previous| plane_raw.time > previous.value.time);

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
            source_sequence: None,
            source_capture_tick: None,
        };

        if carrier_advanced {
            self.carrier_last_advanced_at = Some(latest_received);
        }
        if plane_advanced {
            self.plane_last_advanced_at = Some(latest_received);
        }
        self.previous_carrier = Some(carrier_observed);
        self.previous_plane = Some(plane_observed);
        self.previous_sample_at = Some(latest_received);
        self.previous_sample_valid = sample.is_valid() && !sample.has_warning();
        sample
    }

    pub fn reset(&mut self) {
        *self = Self::new();
    }
}

fn source_age_ms(
    current_time: f64,
    previous: Option<&ObservedTransform>,
    last_advanced_at: Option<Instant>,
    received_at: Instant,
) -> f64 {
    match previous {
        Some(previous) if current_time <= previous.value.time => last_advanced_at
            .map(|advanced| received_at.duration_since(advanced).as_secs_f64() * 1_000.0)
            .unwrap_or_default(),
        _ => 0.0,
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

    #[test]
    fn non_finite_geometry_is_rejected_on_every_acquisition_path() {
        let good = Transform {
            time: 10.0,
            position: DVec3::new(1.0, 2.0, 3.0),
            ..Transform::default()
        };
        let nan_position = Transform {
            position: DVec3::new(f64::NAN, 2.0, 3.0),
            ..good.clone()
        };
        let infinite_velocity = Transform {
            velocity: DVec3::new(0.0, f64::INFINITY, 0.0),
            ..good.clone()
        };
        // AoA alone may legitimately be NaN (zero velocity) and must not invalidate a sample.
        let nan_aoa = Transform {
            aoa: f64::NAN,
            ..good.clone()
        };

        assert_eq!(
            TelemetrySample::from_replay(good.clone(), nan_position.clone(), None).invalid_reason,
            Some(TelemetryInvalidReason::NonFiniteValue)
        );
        assert_eq!(
            TelemetrySample::from_source_pair(infinite_velocity.clone(), good.clone(), None, 0.0)
                .invalid_reason,
            Some(TelemetryInvalidReason::NonFiniteValue)
        );
        assert_eq!(
            TelemetrySample::from_replay(good.clone(), nan_aoa, None).invalid_reason,
            None
        );

        let mut aligner = TelemetryAligner::new();
        let sample = aligner.align(
            ObservedTransform::now(good.clone()),
            ObservedTransform::now(nan_position),
        );
        assert_eq!(
            sample.invalid_reason,
            Some(TelemetryInvalidReason::NonFiniteValue)
        );
        assert_eq!(sample.method, AlignmentMethod::Invalid);
        // A skewed pair with a NaN velocity must be rejected before extrapolation is attempted.
        let mut aligner = TelemetryAligner::new();
        aligner.align(
            ObservedTransform::now(good.clone()),
            ObservedTransform::now(good.clone()),
        );
        let skewed_plane = Transform {
            time: 10.15,
            velocity: DVec3::new(f64::NAN, 0.0, 0.0),
            ..good.clone()
        };
        let sample = aligner.align(
            ObservedTransform::now(good),
            ObservedTransform::now(skewed_plane),
        );
        assert_eq!(
            sample.invalid_reason,
            Some(TelemetryInvalidReason::NonFiniteValue)
        );
        assert!(sample.plane.position.x.is_finite());
    }

    fn observed(time: f64, velocity_x: f64) -> ObservedTransform {
        let value = Transform {
            time,
            velocity: DVec3::new(velocity_x, 0.0, 0.0),
            ..Transform::default()
        };
        ObservedTransform::now(value)
    }

    fn observed_at(time: f64, received_at: Instant) -> ObservedTransform {
        ObservedTransform {
            value: Transform {
                time,
                ..Transform::default()
            },
            received_at,
            received_unix_ms: 0,
        }
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

    #[test]
    fn frozen_source_age_accumulates_across_successful_rpc_responses() {
        let mut aligner = TelemetryAligner::new();
        let start = Instant::now();
        assert!(aligner
            .align(observed_at(10.0, start), observed_at(10.0, start))
            .is_valid());
        let after_400_ms = aligner.align(
            observed_at(10.0, start + std::time::Duration::from_millis(400)),
            observed_at(10.0, start + std::time::Duration::from_millis(400)),
        );
        assert_eq!(after_400_ms.source_age_ms, 400.0);
        assert!(after_400_ms.has_warning());

        let after_1100_ms = aligner.align(
            observed_at(10.0, start + std::time::Duration::from_millis(1_100)),
            observed_at(10.0, start + std::time::Duration::from_millis(1_100)),
        );
        assert_eq!(after_1100_ms.source_age_ms, 1_100.0);
        assert_eq!(
            after_1100_ms.invalid_reason,
            Some(TelemetryInvalidReason::TelemetryGap)
        );
    }

    #[test]
    fn source_pair_uses_capture_gap_and_delivery_age() {
        let carrier = Transform {
            time: 10.2,
            ..Transform::default()
        };
        let plane = carrier.clone();
        let sample = TelemetrySample::from_source_pair(carrier, plane, Some(10.0), 750.0);
        assert!((sample.sample_gap_ms - 200.0).abs() < 1.0e-6);
        assert_eq!(sample.source_age_ms, 750.0);
        assert!(sample.is_valid());
        assert!(sample.has_warning());
    }

    #[test]
    fn buffered_delivery_age_above_one_second_does_not_invalidate_contiguous_capture() {
        let carrier = Transform {
            time: 10.05,
            ..Transform::default()
        };
        let plane = carrier.clone();
        let sample = TelemetrySample::from_source_pair(carrier, plane, Some(10.0), 1_200.0);
        assert!((sample.sample_gap_ms - 50.0).abs() < 1.0e-6);
        assert_eq!(sample.source_age_ms, 1_200.0);
        assert!(sample.is_valid());
        assert!(sample.has_warning());
    }
}
