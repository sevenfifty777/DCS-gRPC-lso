use std::ops::Neg;
use std::str::FromStr;

use ultraviolet::{DRotor3, DVec3};

use crate::data::{AirplaneInfo, CarrierInfo};
use crate::grading::{compute_pass_grade, PassGrade};
use crate::transform::Transform;
use crate::utils::m_to_ft;

/// Gate distances in meters at which LSO deviations are sampled.
const GATE_THREE_QUARTER_NM: f64 = 1389.0;
const GATE_HALF_NM: f64 = 926.0;
const GATE_QUARTER_NM: f64 = 463.0;

#[derive(Debug, PartialEq, serde::Serialize)]
pub struct Datum {
    pub x: f64,
    pub y: f64,
    pub aoa: f64,
    pub alt: f64,
}

pub struct Track {
    pilot_name: String,
    previous_distance: f64,
    datums: Vec<Datum>,
    gate_deviations: GateDeviations,
    /// Set to `true` once the aircraft enters inside 3/4 nm and below 300 ft AGL.
    entered_groove: bool,
    grading: Option<Grading>,
    dcs_grading: Option<String>,
    carrier_info: &'static CarrierInfo,
    plane_info: &'static AirplaneInfo,
}

/// GS and lineup deviation recorded at a key gate distance.
#[derive(Debug, PartialEq, serde::Serialize)]
pub struct GateDatum {
    /// Glide slope deviation from ideal path in feet (positive = high, negative = low).
    pub gs_deviation_ft: f64,
    /// Lateral lineup deviation in feet (positive = right of centerline, negative = left).
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
    pub pass_grade: PassGrade,
    pub dcs_grading: Option<String>,
    pub gate_deviations: GateDeviations,
    pub datums: Vec<Datum>,
    pub plane_info: &'static AirplaneInfo,
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
            datums: Default::default(),
            gate_deviations: GateDeviations::default(),
            entered_groove: false,
            grading: None,
            dcs_grading: None,
            carrier_info,
            plane_info,
        }
    }

    pub fn next(&mut self, carrier: &Transform, plane: &Transform) -> bool {
        let landing_pos_offset = self
            .carrier_info
            .optimal_landing_offset(self.plane_info)
            .rotated_by(carrier.rotation);
        let landing_pos = carrier.position + landing_pos_offset;

        let ray_from_plane_to_carrier = DVec3::new(
            landing_pos.x - plane.position.x,
            0.0, // ignore altitude
            landing_pos.z - plane.position.z,
        );

        // Stop tracking once the distance from the plane to the landing position is increasing and
        // has increased more than 100m (since the last time the distance was decreasing).
        let distance = ray_from_plane_to_carrier.mag();
        if distance < self.previous_distance {
            self.previous_distance = distance;
        } else if distance - self.previous_distance > 150.0 {
            if self.grading.is_some() {
                tracing::debug!(distance_in_m = distance, "bolter detected");
                self.grading = Some(Grading::Bolter);
            }

            tracing::debug!(distance_in_m = distance, "stop tracking");

            return false;
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

        let hook_offset = self.plane_info.hook.rotated_by(plane.rotation);
        let alt = plane.alt - self.carrier_info.deck_altitude + hook_offset.y;

        // Sample gate deviations at key distances on first crossing.
        let ideal_gs_alt = x * self.plane_info.glide_slope.to_radians().tan();
        let gs_deviation_ft = m_to_ft(alt - ideal_gs_alt);
        let lineup_ft = m_to_ft(y);
        if x <= GATE_THREE_QUARTER_NM && self.gate_deviations.at_three_quarter_nm.is_none() {
            self.gate_deviations.at_three_quarter_nm =
                Some(GateDatum { gs_deviation_ft, lineup_ft });
        }
        if x <= GATE_HALF_NM && self.gate_deviations.at_half_nm.is_none() {
            self.gate_deviations.at_half_nm = Some(GateDatum { gs_deviation_ft, lineup_ft });
        }
        if x <= GATE_QUARTER_NM && self.gate_deviations.at_quarter_nm.is_none() {
            self.gate_deviations.at_quarter_nm = Some(GateDatum { gs_deviation_ft, lineup_ft });
        }

        // Mark groove entry: inside 3/4 nm and below 300 ft AGL.
        if x <= GATE_THREE_QUARTER_NM && m_to_ft(alt) <= 300.0 {
            self.entered_groove = true;
        }

        self.datums.push(Datum {
            x,
            y,
            aoa: plane.aoa,
            alt: alt.max(0.0),
        });

        true
    }

    pub fn landed(&mut self, carrier: &Transform, plane: &Transform) {
        let cable = self.estimate_cable(carrier, plane);
        self.grading = Some(Grading::Recovered {
            cable,
            cable_estimated: cable,
        });
        tracing::debug!(?cable, "landed, stop tracking");
    }

    pub fn finish(mut self) -> TrackResult {
        // If the plane entered the groove but never landed and no other grading was set,
        // it performed a waveoff.
        if self.grading.is_none() && self.entered_groove {
            self.grading = Some(Grading::WaveoffPilot);
        }

        // If DCS grading is set, use its reported wire instead of the estimated one.
        let grading = if let Some(dcs_wire) = self.dcs_grading.as_ref().and_then(|s| {
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
        };

        let pass_grade = compute_pass_grade(&grading, &self.gate_deviations);

        TrackResult {
            pilot_name: self.pilot_name,
            grading,
            pass_grade,
            dcs_grading: self.dcs_grading,
            gate_deviations: self.gate_deviations,
            datums: self.datums,
            plane_info: self.plane_info,
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
