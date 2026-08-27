use std::ops::Neg;
use std::str::FromStr;

use ultraviolet::{DRotor3, DVec3};

use crate::data::{AirplaneInfo, CarrierInfo, CarrierRecovery};
use crate::grading::{compute_pass_grade, compute_vstol_approach_grade_points, compute_vstol_final_grade_from_points, PassGrade, SpotGrade};
use crate::transform::Transform;
use crate::utils::{m_to_ft, m_to_nm};

// ---------------------------------------------------------------------------
// LSO grading gates
//
// The three standard gates are fixed distances aft of the touchdown point at
// which the LSO samples glide-slope and lineup deviations.  They correspond
// to the ¾ nm, ½ nm, and ¼ nm "start of the ball" calls used in real-world
// NAVAIR 00-80T-104 grading.
//
// Each gate is recorded exactly once, on the first polling frame where the
// aircraft's angled-deck x-coordinate (distance along the deck angle axis,
// positive = behind the threshold) has decreased to or below that distance
// AND the aircraft is below 500 ft AGL (to exclude the overhead-pattern
// crossing of x = 0 at altitude, which would otherwise produce bogus ~90°
// deviation readings via atan2).
//
// | Constant                | nm   | meters | Purpose                          |
// |-------------------------|------|--------|----------------------------------|
// | GATE_THREE_QUARTER_NM   | ¾ nm | 1389 m | Early groove — first deviation   |
// |                         |      |        | sample; sets the tone for the    |
// |                         |      |        | whole pass.  Deviation here      |
// |                         |      |        | typically drives (H)/(L) notes.  |
// | GATE_HALF_NM            | ½ nm | 926 m  | Mid-groove — primary grading     |
// |                         |      |        | reference for OK/Fair/NoGrade.   |
// |                         |      |        | Correlates to the "in close"     |
// |                         |      |        | LSO observation window.          |
// | GATE_QUARTER_NM         | ¼ nm | 463 m  | At the ramp / "in close" — if   |
// |                         |      |        | GS deviation is ≤ GS_CUT_LOW_DEG|
// |                         |      |        | here, the pass is graded Cut.    |
//
// Deviation values stored per gate:
//   gs_deviation_deg  — glide-slope deviation in degrees (+ = high, − = low)
//   gs_deviation_ft   — same deviation expressed in feet (for chart labels)
//   lineup_deg        — lateral lineup deviation in degrees (+ = right of CL)
//   lineup_ft         — same deviation expressed in feet (for chart labels)
// ---------------------------------------------------------------------------

/// ¾ nm gate — first LSO grading sample (1 nm = 1 852 m → ¾ nm ≈ 1 389 m).
const GATE_THREE_QUARTER_NM: f64 = 1389.0;
/// ½ nm gate — primary grading reference.
const GATE_HALF_NM: f64 = 926.0;
/// ¼ nm gate — ramp / "in close"; dangerously low here triggers a Cut pass.
const GATE_QUARTER_NM: f64 = 463.0;

/// Exponential moving average (EMA) smoothing factor for the carrier position.
///
/// DCS updates the carrier's world position in discrete steps (~every 1.4 s at
/// 15 kts) rather than every simulation frame.  When polled at 100 ms, ~13 out
/// of 14 frames return the same stale position, then one frame jumps ahead by
/// ~10 m.  This creates a sawtooth in the approach datum `x` coordinate
/// (distance to landing point along the angled deck), which appears as periodic
/// stairstep drops of 10–20 ft on the side-view chart.
///
/// α = 0.15 spreads each position step over ~18 frames, introducing ~0.6 s of
/// positional lag (~4.6 m at 15 kts).  The resulting gate-distance error is
/// < 0.5 %, well within acceptable tolerance for LSO grading.
const CARRIER_POS_SMOOTH_ALPHA: f64 = 0.15;

#[derive(Debug, PartialEq, serde::Serialize)]
pub struct Datum {
    pub time: f64,
    pub x: f64,
    pub y: f64,
    pub aoa: f64,
    pub alt: f64,
}

/// Single frame of full-pattern position data recorded in the carrier BRC frame.
///
/// Origin is the carrier. Used to draw the overhead circuit chart (break → abeam →
/// ninety → final → touchdown).
#[derive(Debug, PartialEq, serde::Serialize)]
pub struct PatternDatum {
    pub time: f64,
    /// Distance astern of carrier along BRC (m). Positive = behind carrier (approach direction).
    pub astern_m: f64,
    /// Lateral distance from carrier BRC centerline (m). Positive = port (left) side.
    pub port_m: f64,
    /// Altitude MSL in feet.
    pub alt_ft: f64,
    /// Angle of Attack (degrees).
    pub aoa: f64,
}

pub struct Track {
    pilot_name: String,
    previous_distance: f64,
    previous_x: f64,
    datums: Vec<Datum>,
    pattern_datums: Vec<PatternDatum>,
    gate_deviations: GateDeviations,
    /// Set to `true` once the aircraft enters inside 3/4 nm and below 300 ft AGL.
    entered_groove: bool,
    /// DCS simulation time (seconds since scenario start) when groove entry was first detected.
    groove_entry_time: Option<f64>,
    /// DCS simulation time (seconds since scenario start) when touchdown was recorded.
    landing_time: Option<f64>,
    grading: Option<Grading>,
    dcs_grading: Option<String>,
    /// Horizontal deck-plane distance (m) between the AV-8B pilot-ground
    /// landing reference and the calibrated Tarawa spot 7.5 at the exact land event.
    spot_distance_m: Option<f64>,
    carrier_info: &'static CarrierInfo,
    plane_info: &'static AirplaneInfo,
    /// Carrier and aircraft transforms at the closest point of an arrested approach.
    min_distance_state: Option<(Transform, Transform)>,
    /// Exponentially smoothed carrier position used for approach geometry.
    /// Eliminates the sawtooth caused by DCS updating the carrier's world
    /// position in discrete steps rather than every frame.
    smoothed_carrier_pos: Option<DVec3>,
    /// Whether an arrested-recovery aircraft was observed with its hook raised.
    hook_was_up: bool,
}

/// GS and lineup deviation recorded at a key gate distance.
#[derive(Debug, PartialEq, serde::Serialize)]
pub struct GateDatum {
    /// Glide slope deviation from ideal glide path in degrees
    /// (positive = high, negative = low).
    pub gs_deviation_deg: f64,
    /// Lateral lineup deviation from the angled-deck centerline in degrees
    /// (positive = right of centerline / lined-up-left, negative = left).
    pub lineup_deg: f64,
    /// Glide slope deviation in feet — kept for the PNG chart display label.
    pub gs_deviation_ft: f64,
    /// Lineup deviation in feet — kept for the PNG chart display label.
    pub lineup_ft: f64,
}

/// Deviation scores sampled at the standard LSO grading gates.
#[derive(Debug, Default, PartialEq, serde::Serialize)]
pub struct GateDeviations {
    pub at_three_quarter_nm: Option<GateDatum>,
    pub at_half_nm: Option<GateDatum>,
    pub at_quarter_nm: Option<GateDatum>,
}

#[derive(Debug, PartialEq, Eq, serde::Serialize)]
pub enum Grading {
    Unknown,
    Bolter,
    IntentionalBolter {
        cable_estimated: Option<u8>,
    },
    /// Pilot broke off the approach after entering the groove (inside 3/4 nm, below 300 ft).
    WaveoffPilot,
    Recovered {
        cable: Option<u8>,
        cable_estimated: Option<u8>,
    },
}

#[derive(Debug, PartialEq)]
pub struct TrackResult {
    pub pilot_name: String,
    pub grading: Grading,
    /// Gate-only approach grade before any V/STOL touchdown bonus.
    pub approach_grade: PassGrade,
    /// Final display grade. For CATOBAR this is identical to approach_grade;
    /// for V/STOL it includes the spot-7.5 bonus.
    pub pass_grade: PassGrade,
    /// Final numeric score. Kept separately because V/STOL bonuses can produce
    /// quarter-point values (e.g. 4.75) while reusing the CATOBAR labels.
    pub grade_points: f64,
    pub spot_grade: Option<SpotGrade>,
    pub spot_distance_m: Option<f64>,
    pub dcs_grading: Option<String>,
    pub gate_deviations: GateDeviations,
    pub datums: Vec<Datum>,
    pub pattern_datums: Vec<PatternDatum>,
    pub plane_info: &'static AirplaneInfo,
    pub carrier_info: &'static CarrierInfo,
    /// Time from groove entry to touchdown in seconds, if both were recorded.
    pub groove_time_secs: Option<f64>,
}

impl Track {
    pub fn new(
        pilot_name: impl Into<String>,
        carrier_info: &'static CarrierInfo,
        plane_info: &'static AirplaneInfo,
    ) -> Self {
        Self {
            pilot_name: pilot_name.into(),
            previous_distance: f64::MAX,
            previous_x: f64::MAX,
            datums: Default::default(),
            pattern_datums: Default::default(),
            gate_deviations: GateDeviations::default(),
            entered_groove: false,
            groove_entry_time: None,
            landing_time: None,
            grading: None,
            dcs_grading: None,
            spot_distance_m: None,
            carrier_info,
            plane_info,
            min_distance_state: None,
            smoothed_carrier_pos: None,
            hook_was_up: false,
        }
    }

    pub fn next(
        &mut self,
        carrier: &Transform,
        plane: &Transform,
        hook_state: Option<f64>,
    ) -> bool {
        // ---------------------------------------------------------------
        // Pattern datum — BRC frame, recorded every frame.
        // Origin = carrier position. x_chart = -port_m, y_chart = -astern_m
        // so the circuit appears with port on the left and the carrier at the
        // top of the overview PNG.
        // ---------------------------------------------------------------
        {
            let brc_rot =
                DRotor3::from_rotation_xz(carrier.heading.neg().to_radians());
            let brc_fwd  = DVec3::unit_z().rotated_by(brc_rot); // BRC forward
            let brc_stbd = DVec3::unit_x().rotated_by(brc_rot); // starboard

            // rel = vector from plane to carrier
            let rel = carrier.position - plane.position;
            // astern_m > 0 when plane is behind carrier (normal approach direction)
            let astern_m = rel.dot(brc_fwd);
            // port_m > 0 when plane is on the port (left) side of the carrier
            // (rel points toward the carrier; when the plane is to port, rel
            // points toward the starboard side → positive dot with brc_stbd)
            let port_m = rel.dot(brc_stbd);

            self.pattern_datums.push(PatternDatum {
                time: plane.time,
                astern_m,
                port_m,
                alt_ft: m_to_ft(plane.alt),
                aoa: plane.aoa,
            });
        }

        // Smooth carrier position to eliminate DCS quantisation sawtooth.
        // The carrier's world position updates in discrete jumps (~every 1.4 s);
        // between updates, the same stale position is returned.  EMA blends the
        // raw position toward the smoothed estimate each frame, producing a
        // steady progression instead of a stairstep.
        let smoothed_pos = match self.smoothed_carrier_pos {
            Some(prev) => {
                let s = prev + (carrier.position - prev) * CARRIER_POS_SMOOTH_ALPHA;
                self.smoothed_carrier_pos = Some(s);
                s
            }
            None => {
                self.smoothed_carrier_pos = Some(carrier.position);
                carrier.position
            }
        };

        let landing_pos_offset = self
            .carrier_info
            .approach_reference_offset(self.plane_info)
            .rotated_by(carrier.rotation);
        let landing_pos = smoothed_pos + landing_pos_offset;

        // Horizontal V/STOL lineup is an aircraft-centerline measurement.
        // The ideal axis itself is already positioned one AV-8B wingspan outside
        // the Tarawa port deck edge by approach_reference_offset().  Do not add
        // the touchdown reference here: the pilot-ground contact projection is
        // retained for the later hover/touchdown phase, not for parallel-approach lineup.
        let ray_from_plane_to_carrier = DVec3::new(
            landing_pos.x - plane.position.x,
            0.0, // ignore altitude
            landing_pos.z - plane.position.z,
        );

        // Stop once the plane leaves the wide pattern detection zone (RTB or go-around).
        // This prevents a recording from running forever when no landing is made.
        let carrier_distance = (smoothed_pos - plane.position).mag();
        if m_to_nm(carrier_distance) > 3.5 || m_to_ft(plane.alt) > 1100.0 {
            tracing::debug!("stop: plane exited pattern detection zone");
            return false;
        }

        let is_arrested_recovery = matches!(
            &self.carrier_info.recovery,
            CarrierRecovery::Arrested
        );
        if is_arrested_recovery && hook_state.is_some_and(|state| state < 0.5) {
            self.hook_was_up = true;
        }

        // Track the minimum distance to the touchdown point.
        let distance = ray_from_plane_to_carrier.mag();
        if distance < self.previous_distance {
            self.previous_distance = distance;
            if is_arrested_recovery {
                self.min_distance_state = Some((carrier.clone(), plane.clone()));
            }
        } else if distance - self.previous_distance > 150.0 {
            match &self.grading {
                Some(Grading::Recovered { .. }) => {
                    // An arrested-recovery aircraft with its hook raised is a
                    // qualification touch-and-go; V/STOL never enters this path.
                    if self.hook_was_up {
                        if let Some((min_carrier, min_plane)) = &self.min_distance_state {
                            let estimated = self.estimate_cable(min_carrier, min_plane);
                            tracing::debug!(
                                distance_in_m = distance,
                                "intentional bolter detected after touchdown"
                            );
                            self.grading = Some(Grading::IntentionalBolter {
                                cable_estimated: estimated,
                            });
                            return false;
                        }
                    }

                    // Landed and now moving away → normal bolter.
                    tracing::debug!(distance_in_m = distance, "bolter detected");
                    self.grading = Some(Grading::Bolter);
                    return false;
                }
                Some(_) => {
                    // Waveoff or other graded outcome, plane moving away → stop.
                    tracing::debug!(distance_in_m = distance, "stop tracking (graded, moving away)");
                    return false;
                }
                None if self.entered_groove => {
                    // An arrested aircraft that crossed the deck is a bolter or
                    // qualification touch-and-go; otherwise it is a waveoff.
                    if let Some((min_carrier, min_plane)) = &self.min_distance_state {
                        if self.hook_was_up {
                            let estimated = self.estimate_cable(min_carrier, min_plane);
                            tracing::debug!(
                                distance_in_m = distance,
                                "intentional bolter detected"
                            );
                            self.grading = Some(Grading::IntentionalBolter {
                                cable_estimated: estimated,
                            });
                            return false;
                        }

                        tracing::debug!(
                            distance_in_m = distance,
                            "bolter detected (no touchdown)"
                        );
                        self.grading = Some(Grading::Bolter);
                        return false;
                    }

                    tracing::debug!(distance_in_m = distance, "waveoff detected (entered groove, moving away)");
                    self.grading = Some(Grading::WaveoffPilot);
                    return false;
                }
                None => {
                    // No graded outcome yet and plane not in groove → still flying the overhead
                    // pattern (break turn, downwind, abeam).  Reset the distance floor so the
                    // next approaching leg is tracked from a fresh minimum instead of stopping.
                    self.previous_distance = distance;
                    tracing::trace!(distance_in_m = distance, "pattern: plane moving away, resetting distance tracker");
                }
            }
        }

        // Already landed, no need to actually record any more datums, but keep going to detect
        // bolters.
        if self.grading.is_some() {
            return true;
        }

        // Construct the x axis, which is aligned to the angled deck.
        let fb_rot = DRotor3::from_rotation_xz(
            (carrier.heading - self.carrier_info.deck_angle)
                .neg()
                .to_radians(),
        );
        let fb = DVec3::unit_z().rotated_by(fb_rot);

        let x = ray_from_plane_to_carrier.dot(fb);
        let mut y = (distance.powi(2) - x.powi(2)).sqrt();

        // Determine whether plane is left or right of the glide slope.
        let a = DVec3::unit_x().rotated_by(fb_rot);
        if ray_from_plane_to_carrier.dot(a) > 0.0 {
            y = y.neg();
        }

        let alt = match &self.carrier_info.recovery {
            CarrierRecovery::Arrested => {
                let hook_offset = self.plane_info.hook.rotated_by(plane.rotation);
                plane.alt - self.carrier_info.deck_altitude + hook_offset.y
            }
            CarrierRecovery::Vstol { .. } => {
                // V/STOL V1 vertical chart is referenced to the water/sea level,
                // matching the Harrier's 120 ft hover/approach altitude.
                // DCS plane.alt is MSL, so keep it directly instead of subtracting
                // the Tarawa deck height (which would shift the curve ~66 ft low).
                plane.alt
            }
        };

        // Gate sampling and groove entry only apply when the aircraft is on the approach side of
        // the threshold (x > 0).  When x ≤ 0 the aircraft is ahead of the touchdown point
        // (e.g., still in the break or flying the overhead pattern), and atan2 with a negative x
        // would produce a bogus ~177° deviation reading.
        if x > 0.0 {
            // Robust reset: if the aircraft flies outbound (e.g., into the pattern after a bolter),
            // clear any gates or groove entry that were captured so they can be freshly recorded
            // on the real final approach inbound.
            if x > GATE_THREE_QUARTER_NM {
                self.gate_deviations.at_three_quarter_nm = None;
                self.groove_entry_time = None;
                self.entered_groove = false;
            }
            if x > GATE_HALF_NM {
                self.gate_deviations.at_half_nm = None;
            }
            if x > GATE_QUARTER_NM {
                self.gate_deviations.at_quarter_nm = None;
            }

            // Only sample gates if the aircraft is flying inbound (x is decreasing).
            // This prevents capturing bogus ~90° deviations if the aircraft crosses the beam
            // outbound (from front to back) during a tight low-altitude bolter pattern.
            let is_inbound = x < self.previous_x;

            // Sample gate deviations at key distances on first crossing.
            let ideal_gs_alt = match &self.carrier_info.recovery {
                CarrierRecovery::Arrested => x * self.plane_info.glide_slope.to_radians().tan(),
                CarrierRecovery::Vstol { target_altitude_ft, .. } => {
                    // Same geometric principle as CATOBAR, but translated upward:
                    // the ideal V/STOL approach reaches 120 ft MSL/above-water at
                    // x = 0 (abeam the 7.5 longitudinal station).
                    (*target_altitude_ft / 3.28084)
                        + x * self.plane_info.glide_slope.to_radians().tan()
                },
            };
            let gs_deviation_m = alt - ideal_gs_alt;
            let gs_deviation_ft = m_to_ft(gs_deviation_m);
            let lineup_ft = m_to_ft(y);
            // Angular deviations: atan2 is valid here because x > 0, so the angle is in (−90°, +90°).
            let gs_deviation_deg = gs_deviation_m.atan2(x).to_degrees();
            let lineup_deg = y.atan2(x).to_degrees();
            // Gate altitude guard: on-glidepath at ¾ nm is ~278 ft; even a GS+3° deviation is
            // ~400 ft at that distance.  500 ft cleanly rejects the 600–1000 ft overhead-pattern
            // crossing of x = 0 while still capturing all realistic final-approach deviations.
            let in_approach = m_to_ft(alt) <= 500.0;
            // For V/STOL, do not capture a distance gate while the Harrier is
            // still on base/turning toward the parallel axis.  This avoids
            // bogus multi-thousand-foot LAT values from an earlier circuit pass.
            let gate_lined_up = !self.carrier_info.is_vstol() || lineup_deg.abs() <= 10.0;
            if in_approach && gate_lined_up && is_inbound && x <= GATE_THREE_QUARTER_NM && self.gate_deviations.at_three_quarter_nm.is_none() {
                self.gate_deviations.at_three_quarter_nm = Some(GateDatum {
                    gs_deviation_deg,
                    lineup_deg,
                    gs_deviation_ft,
                    lineup_ft,
                });
            }
            if in_approach && gate_lined_up && is_inbound && x <= GATE_HALF_NM && self.gate_deviations.at_half_nm.is_none() {
                self.gate_deviations.at_half_nm = Some(GateDatum {
                    gs_deviation_deg,
                    lineup_deg,
                    gs_deviation_ft,
                    lineup_ft,
                });
            }
            if in_approach && gate_lined_up && is_inbound && x <= GATE_QUARTER_NM && self.gate_deviations.at_quarter_nm.is_none() {
                self.gate_deviations.at_quarter_nm = Some(GateDatum {
                    gs_deviation_deg,
                    lineup_deg,
                    gs_deviation_ft,
                    lineup_ft,
                });
            }

            // Mark groove entry: inside 3/4 nm, below 300 ft AGL, and lined up (±10°).
            // The lateral constraint prevents the timer from starting prematurely while the
            // aircraft is still performing a wide turn to final on the base leg.
            if x <= GATE_THREE_QUARTER_NM && m_to_ft(alt) <= 300.0 && lineup_deg.abs() <= 10.0 {
                if self.groove_entry_time.is_none() {
                    self.groove_entry_time = Some(plane.time);
                }
                self.entered_groove = true;
            }
        }

        self.datums.push(Datum {
            time: plane.time,
            x,
            y,
            aoa: plane.aoa,
            alt: alt.max(0.0),
        });

        self.previous_x = x;

        true
    }

    pub fn landed(&mut self, carrier: &Transform, plane: &Transform) {
        // For V/STOL, the DCS land event contains the most accurate final
        // aircraft/carrier transforms. The normal sampling loop can stop a few
        // frames before that event, which made the terminal trace appear to end
        // slightly before the actual touchdown point. Append one exact terminal
        // datum from the land-event transforms so both V/STOL plots can finish
        // at the real touchdown position.
        if matches!(&self.carrier_info.recovery, CarrierRecovery::Vstol { .. }) {
            // Exact touchdown accuracy relative to Tarawa spot 7.5.  The AV-8B
            // pilot-ground reference is transformed into carrier-local coordinates,
            // then compared to the calibrated landing point using only the deck
            // plane axes (local X/Z).  Vertical compression/gear animation therefore
            // cannot distort the touchdown accuracy score.
            if let CarrierRecovery::Vstol { landing_point, .. } = &self.carrier_info.recovery {
                let spot_ref_world = plane.position
                    + self.plane_info.landing_reference.rotated_by(plane.rotation);
                let spot_ref_local = (spot_ref_world - carrier.position)
                    .rotated_by(carrier.rotation.reversed());
                let dx = spot_ref_local.x - landing_point.x;
                let dz = spot_ref_local.z - landing_point.z;
                let spot_distance_m = (dx * dx + dz * dz).sqrt();
                self.spot_distance_m = Some(spot_distance_m);
            }

            let landing_pos_offset = self
                .carrier_info
                .approach_reference_offset(self.plane_info)
                .rotated_by(carrier.rotation);
            let landing_pos = carrier.position + landing_pos_offset;

            let ray_from_plane_to_carrier = DVec3::new(
                landing_pos.x - plane.position.x,
                0.0,
                landing_pos.z - plane.position.z,
            );

            let fb_rot = DRotor3::from_rotation_xz(
                (carrier.heading - self.carrier_info.deck_angle)
                    .neg()
                    .to_radians(),
            );
            let fb = DVec3::unit_z().rotated_by(fb_rot);
            let distance = ray_from_plane_to_carrier.mag();
            let x = ray_from_plane_to_carrier.dot(fb);
            let mut y = (distance.powi(2) - x.powi(2)).max(0.0).sqrt();

            let a = DVec3::unit_x().rotated_by(fb_rot);
            if ray_from_plane_to_carrier.dot(a) > 0.0 {
                y = y.neg();
            }

            let should_push = self
                .datums
                .last()
                .map(|d| (plane.time - d.time).abs() > 1.0e-6)
                .unwrap_or(true);

            if should_push {
                self.datums.push(Datum {
                    time: plane.time,
                    x,
                    y,
                    aoa: plane.aoa,
                    alt: plane.alt.max(0.0),
                });
            }
        }

        let cable = match &self.carrier_info.recovery {
            CarrierRecovery::Arrested => self.estimate_cable(carrier, plane),
            CarrierRecovery::Vstol { .. } => None,
        };
        self.grading = Some(Grading::Recovered {
            cable,
            cable_estimated: cable,
        });
        self.landing_time = Some(plane.time);
        tracing::debug!(?cable, "landed, stop tracking");
    }

    pub fn finish(mut self) -> TrackResult {
        // If the plane entered the groove but never landed and no other grading was set,
        // it performed a waveoff.
        if self.grading.is_none() && self.entered_groove {
            self.grading = Some(Grading::WaveoffPilot);
        }

        // If DCS grading is set, use its reported wire for arrested recoveries only.
        let grading = if matches!(&self.carrier_info.recovery, CarrierRecovery::Arrested) {
            if let Some(dcs_wire) = self.dcs_grading.as_ref().and_then(|s| {
                s.split_once("WIRE# ")
                    .and_then(|(_, w)| u8::from_str(&w[0..1]).ok())
            }) {
                match self.grading {
                    Some(Grading::Recovered {
                        cable_estimated, ..
                    }) => Grading::Recovered {
                        cable: Some(dcs_wire),
                        cable_estimated,
                    },
                    _ => Grading::Recovered {
                        cable: Some(dcs_wire),
                        cable_estimated: None,
                    },
                }
            } else {
                self.grading.unwrap_or_default()
            }
        } else {
            self.grading.unwrap_or_default()
        };
        let grading = normalize_grading_for_recovery(grading, &self.carrier_info.recovery);

        let groove_time_secs = match (self.groove_entry_time, self.landing_time) {
            (Some(entry), Some(land)) if land > entry => Some(land - entry),
            _ => None,
        };

        // CATOBAR keeps the native wire/groove grading path.  AV-8B V/STOL
        // deliberately reuses the same GS/LU gate tiers, referenced to its 3.0°
        // glide slope, but excludes CATOBAR-only wire/groove bonuses.  AOA is
        // visual information only and is not part of the points calculation.
        let (approach_grade, approach_points) = if self.carrier_info.is_vstol() {
            compute_vstol_approach_grade_points(&grading, &self.gate_deviations)
        } else {
            let grade = compute_pass_grade(&grading, &self.gate_deviations, groove_time_secs);
            (grade, grade.points())
        };
        let spot_grade = self.spot_distance_m.map(SpotGrade::from_distance_m);

        // CATOBAR is intentionally untouched. Only a successfully recovered V/STOL
        // pass receives the spot-accuracy bonus and is then mapped back to the same
        // greenie-board labels used by CATOBAR (_OK_/OK/(OK)/--/C).
        let (pass_grade, grade_points) = if self.carrier_info.is_vstol()
            && matches!(&grading, Grading::Recovered { .. })
        {
            match spot_grade {
                Some(spot) => compute_vstol_final_grade_from_points(approach_points, spot),
                None => (approach_grade, approach_points),
            }
        } else {
            (approach_grade, approach_points)
        };

        TrackResult {
            pilot_name: self.pilot_name,
            grading,
            approach_grade,
            pass_grade,
            grade_points,
            spot_grade,
            spot_distance_m: self.spot_distance_m,
            dcs_grading: self.dcs_grading,
            gate_deviations: self.gate_deviations,
            datums: self.datums,
            pattern_datums: self.pattern_datums,
            plane_info: self.plane_info,
            carrier_info: self.carrier_info,
            groove_time_secs,
        }
    }

    /// Set the track's dcs grading.
    pub fn set_dcs_grading(&mut self, dcs_grading: String) {
        self.dcs_grading = Some(dcs_grading);
    }

    fn estimate_cable(&self, carrier: &Transform, plane: &Transform) -> Option<u8> {
        let hook_offset = self.plane_info.hook.rotated_by(plane.rotation);
        let touchdown = plane.position + hook_offset;
        let forward = carrier
            .forward
            .rotated_by(DRotor3::from_rotation_xz(-self.carrier_info.deck_angle));

        // The land event fires slightly after the wire is caught, so the hook has already
        // passed the wire. Move the touchdown 3.0 m forward to compensate.
        let touchdown = touchdown + (forward * 3.0);

        let cables = [
            (1, &self.carrier_info.cable1),
            (2, &self.carrier_info.cable2),
            (3, &self.carrier_info.cable3),
            (4, &self.carrier_info.cable4),
        ]
        .into_iter()
        .map(|(nr, pendants)| {
            // Calculate the mid position between both cable pendants:
            // o-----------o
            //       ^
            //       |
            let mid_cable = (pendants.0 - pendants.1) / 2.0;
            let mid_cable = pendants.0 - mid_cable;
            let mid_cable = carrier.position + mid_cable.rotated_by(carrier.rotation);

            (nr, mid_cable)
        })
        .collect::<Vec<_>>();

        for (nr, mid_cable) in cables {
            // If the cable is in front of the touchdown position, consider it the one the plane
            // catches.
            let ray_to_cable = touchdown - mid_cable;
            tracing::trace!(
                cable = nr,
                distance = ray_to_cable.mag(),
                dot = ray_to_cable.dot(forward),
                "cable candidate"
            );
            if ray_to_cable.dot(forward) > 0.0 {
                return Some(nr);
            }
        }

        None
    }
}

impl Default for Grading {
    fn default() -> Self {
        Self::Unknown
    }
}

fn normalize_grading_for_recovery(
    grading: Grading,
    recovery: &CarrierRecovery,
) -> Grading {
    match (recovery, grading) {
        // Intentional bolters are hook-up qualification passes and only exist
        // for arrested recoveries. Never expose this outcome on V/STOL.
        (CarrierRecovery::Vstol { .. }, Grading::IntentionalBolter { .. }) => Grading::Bolter,
        (_, grading) => grading,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn intentional_bolter_is_preserved_for_arrested_recovery() {
        let grading = Grading::IntentionalBolter {
            cable_estimated: Some(3),
        };

        assert_eq!(
            normalize_grading_for_recovery(grading, &CarrierRecovery::Arrested),
            Grading::IntentionalBolter {
                cable_estimated: Some(3)
            }
        );
    }

    #[test]
    fn intentional_bolter_is_never_exposed_for_vstol_recovery() {
        let grading = Grading::IntentionalBolter {
            cable_estimated: Some(3),
        };
        let recovery = CarrierRecovery::Vstol {
            landing_point: DVec3::zero(),
            approach_axis_port_m: 27.24,
            target_altitude_ft: 120.0,
        };

        assert_eq!(
            normalize_grading_for_recovery(grading, &recovery),
            Grading::Bolter
        );
    }
}
