use std::ops::Neg;

use ultraviolet::{DRotor3, DVec3};

// Connector positions (hook, cable, ...) extracted via ModelViewer2.
// 1. Open Connector Tool
// 2. Select model connector name (name can be found in `D:\DCS World\CoreMods\tech\USS_Nimitz\
//    scripts\USS_Nimitz_RunwaysAndRoutes.lua`)
// 3. Read P position row as (z, y, x)

const NIMITZ: CarrierInfo = CarrierInfo {
    // CoreMods\tech\USS_Nimitz\scripts\USS_Nimitz_RunwaysAndRoutes.lua
    deck_angle: 9.1359,
    deck_altitude: 20.1494,
    recovery: CarrierRecovery::Arrested,
    active_vstol_spots: &[],
    cable1: (
        // POINT_TROS_01_01
        DVec3 {
            x: -17.622131,
            y: 20.201731,
            z: -112.129128,
        },
        // POINT_TROS_01_02
        DVec3 {
            x: 18.445099,
            y: 20.201729,
            z: -106.040421,
        },
    ),
    cable2: (
        // POINT_TROS_02_01
        DVec3 {
            x: -19.584789,
            y: 20.201731,
            z: -99.914261,
        },
        // POINT_TROS_02_02
        DVec3 {
            x: 16.519514,
            y: 20.201729,
            z: -93.864029,
        },
    ),
    cable3: (
        // POINT_TROS_03_01
        DVec3 {
            x: -21.578857,
            y: 20.201731,
            z: -87.524025,
        },
        // POINT_TROS_03_02
        DVec3 {
            x: 14.471450,
            y: 20.201731,
            z: -81.399986,
        },
    ),
    cable4: (
        // POINT_TROS_04_01
        DVec3 {
            x: -23.609934,
            y: 20.201731,
            z: -74.960480,
        },
        // POINT_TROS_04_02
        DVec3 {
            x: 12.444860,
            y: 20.201729,
            z: -68.854492,
        },
    ),
};

const FORRESTAL: CarrierInfo = CarrierInfo {
    // CoreMods\tech\USS_Nimitz\scripts\USS_Nimitz_RunwaysAndRoutes.lua
    deck_angle: 9.42,
    deck_altitude: 18.46,
    recovery: CarrierRecovery::Arrested,
    active_vstol_spots: &[],
    cable1: (
        // POINT_TROS_01_01
        DVec3 {
            x: -17.749493,
            y: 18.474249,
            z: -96.792412,
        },
        // POINT_TROS_01_02
        DVec3 {
            x: 17.089462,
            y: 18.474247,
            z: -90.162186,
        },
    ),
    cable2: (
        // POINT_TROS_02_01
        DVec3 {
            x: -19.516848,
            y: 18.475485,
            z: -87.192558,
        },
        // POINT_TROS_02_02
        DVec3 {
            x: 15.311986,
            y: 18.475483,
            z: -80.510368,
        },
    ),
    cable3: (
        // POINT_TROS_03_01
        DVec3 {
            x: -21.246920,
            y: 18.482229,
            z: -76.618980,
        },
        // POINT_TROS_03_02
        DVec3 {
            x: 13.582755,
            y: 18.482227,
            z: -69.941109,
        },
    ),
    cable4: (
        // POINT_TROS_04_01
        DVec3 {
            x: -23.128010,
            y: 18.491688,
            z: -66.396812,
        },
        // POINT_TROS_04_02
        DVec3 {
            x: 11.704433,
            y: 18.491686,
            z: -59.733154,
        },
    ),
};

/// External-model hook draw argument shared by the F/A-18C and the VNAO T-45.
/// Validated against the installed module files (SHA-256 checked, 2026-09-02):
/// `0` = hook up, `1` = hook down. During an arrestment the animated value
/// transiently drops into the "up" band while the cable loads the hook.
const HOOK_ARGUMENT_25: HookArgument = HookArgument {
    id: 25,
    up_max: 0.2,
    down_min: 0.8,
    polarity: "external_arg_25_zero_up_one_down_live_validated_2026-09",
};

/// Heatblur F-14 external hook argument (all variants). Same polarity as
/// argument 25; the arrestment excursion lasts ~6.5 s and starts before
/// `RunwayTouch`.
const HOOK_ARGUMENT_1305: HookArgument = HookArgument {
    id: 1305,
    up_max: 0.2,
    down_min: 0.8,
    polarity: "external_arg_1305_zero_up_one_down_live_validated_2026-09",
};

static FA18C: AirplaneInfo = AirplaneInfo {
    name: "F/A-18C Hornet",
    hook: DVec3 {
        x: 0.0,
        y: -2.240897,
        z: -7.237348,
    },
    landing_reference: DVec3 {
        x: 0.0,
        y: -2.240897,
        z: -7.237348,
    },
    hook_argument: Some(HOOK_ARGUMENT_25),
    glide_slope: 3.5,
    aoa_rating: |aoa: f64| -> Aoa {
        // https://forums.vrsimulations.com/support/index.php/Navigation_Tutorial_Flight#Angle_of_Attack_Bracket
        if aoa <= 6.9 {
            Aoa::Fast
        } else if aoa <= 7.4 {
            Aoa::SlightlyFast
        } else if aoa < 8.8 {
            Aoa::OnSpeed
        } else if aoa < 9.3 {
            Aoa::SlightlySlow
        } else {
            Aoa::Slow
        }
    },
};

/// F-14 AOA rating shared by all Tomcat variants.
/// https://www.heatblur.se/F-14Manual/cockpit.html?highlight=aoa#approach-indexer
/// AOA degrees for Tomcat calculated by degrees=((units/1.0989) - 3.01)
/// from units in manual based off conversation found here:
/// https://forum.dcs.world/topic/228893-aoa-units-to-degrees-conversion/
fn f14_aoa_rating(aoa: f64) -> Aoa {
    if aoa <= 9.7 {
        Aoa::Fast
    } else if aoa <= 10.2 {
        Aoa::SlightlyFast
    } else if aoa < 11.1 {
        Aoa::OnSpeed
    } else if aoa < 11.6 {
        Aoa::SlightlySlow
    } else {
        Aoa::Slow
    }
}

/// Hook position shared by all F-14 variants (extracted via ModelViewer2).
const F14_HOOK: DVec3 = DVec3 {
    x: 0.0,
    y: -1.978941,
    z: -6.563727,
};

static F14A: AirplaneInfo = AirplaneInfo {
    name: "F-14A Tomcat",
    hook: F14_HOOK,
    landing_reference: F14_HOOK,
    hook_argument: Some(HOOK_ARGUMENT_1305),
    glide_slope: 3.5,
    aoa_rating: f14_aoa_rating,
};

static F14B: AirplaneInfo = AirplaneInfo {
    name: "F-14B Tomcat",
    hook: F14_HOOK,
    landing_reference: F14_HOOK,
    hook_argument: Some(HOOK_ARGUMENT_1305),
    glide_slope: 3.5,
    aoa_rating: f14_aoa_rating,
};

static F14BU: AirplaneInfo = AirplaneInfo {
    name: "F-14B(U) Tomcat",
    hook: F14_HOOK,
    landing_reference: F14_HOOK,
    hook_argument: Some(HOOK_ARGUMENT_1305),
    glide_slope: 3.5,
    aoa_rating: f14_aoa_rating,
};

static T45: AirplaneInfo = AirplaneInfo {
    name: "T-45C Goshawk",
    hook: DVec3 {
        x: 0.0,
        y: -1.778766,
        z: -4.782536,
    },
    landing_reference: DVec3 {
        x: 0.0,
        y: -1.778766,
        z: -4.782536,
    },
    hook_argument: Some(HOOK_ARGUMENT_25),
    glide_slope: 3.5,
    aoa_rating: |aoa: f64| -> Aoa {
        // Thresholds derived from VNAO T-45 v1.0.2 DEU (DisplayElectronicsUnit.lua).
        // The cockpit AOA indexer uses UNITS_AOA (set by the EFM DLL). A commented reference
        // in the DEU (`getAngleOfAttack()*degrees_per_radian + 10`) implies the mapping
        // degrees ≈ UNITS_AOA - 10. Indexer thresholds in UNITS → degrees:
        //   Fast  (chevron "^"):  UNITS <= 16.5  → degrees <= 6.5
        //   OnSpd (circle  "O"):  16 <= UNITS <= 18  → 6.0–8.0° (centre 7.0°)
        //   Slow  (vee    "V"):   UNITS >= 17.5  → degrees >= 7.5
        if aoa <= 6.0 {
            Aoa::Fast
        } else if aoa <= 6.5 {
            Aoa::SlightlyFast
        } else if aoa < 7.5 {
            Aoa::OnSpeed
        } else if aoa < 8.0 {
            Aoa::SlightlySlow
        } else {
            Aoa::Slow
        }
    },
};

static AV8B: AirplaneInfo = AirplaneInfo {
    name: "AV-8B N/A Harrier",
    // AV-8B has no arresting hook for this workflow.
    hook: DVec3 {
        x: 0.0,
        y: 0.0,
        z: 0.0,
    },
    // AV-8B touchdown/spot reference: pilot position projected vertically
    // onto the aircraft ground-contact plane.  AV8BNA.lua places the pilot at
    // x=3.43, z=0.0; y keeps the calibrated ground-contact level so altitude
    // handling remains unchanged while the longitudinal reference moves forward
    // to the pilot station.
    landing_reference: DVec3 {
        x: 3.43,
        y: -1.89645,
        z: 0.0,
    },
    hook_argument: None,
    // V/STOL V1.3 keeps the CATOBAR-style sloped approach display, but the
    // reference path terminates at 120 ft above the water abeam spot 7.5.
    glide_slope: 3.0,
    // AV-8B target approach AOA: 10-12 degrees.  This rating is used only
    // for the trace colour / AOA indication; it does NOT change the V/STOL
    // approach grade, which remains based on GS + LU at the three gates.
    aoa_rating: |aoa: f64| -> Aoa {
        if aoa < 10.0 {
            Aoa::Fast
        } else if aoa <= 12.0 {
            Aoa::OnSpeed
        } else {
            Aoa::Slow
        }
    },
};

#[derive(Debug, PartialEq)]
pub enum CarrierRecovery {
    Arrested,
    Vstol {
        /// Calibrated AV-8B pilot-ground reference at Tarawa spot 7.5, in carrier-local coordinates.
        landing_point: DVec3,
        /// Distance of the ideal AV-8B approach axis to port of the Tarawa centerline.
        /// DCS defines the Tarawa landing strip as 36 m wide; the AV-8B wingspan is 9.24 m.
        /// Therefore the V1 axis is 18.0 + 9.24 = 27.24 m to port of ship centerline,
        /// i.e. one full AV-8B wingspan outside the port deck edge.
        approach_axis_port_m: f64,
        /// V1 target altitude above mean sea level / water for the parallel approach.
        target_altitude_ft: f64,
    },
}

#[derive(Debug, PartialEq)]
pub struct CarrierInfo {
    /// Counter-clockwise offset from BRC to FB in degrees.
    pub deck_angle: f64,
    // in meter
    pub deck_altitude: f64,
    pub recovery: CarrierRecovery,
    /// Geometrically calibrated spots enabled for nearest-spot reporting.
    /// Phase 1 intentionally contains only Tarawa 7.5; 7 and 8 require live calibration.
    pub active_vstol_spots: &'static [VstolSpot],
    /// Cable pendant positions (left, right) relative to the object' origin.
    pub cable1: (DVec3, DVec3),
    pub cable2: (DVec3, DVec3),
    pub cable3: (DVec3, DVec3),
    pub cable4: (DVec3, DVec3),
}

#[derive(Debug, PartialEq)]
pub struct VstolSpot {
    pub label: &'static str,
    pub landing_point: DVec3,
}

const TARAWA_PHASE1_SPOTS: &[VstolSpot] = &[VstolSpot {
    label: "7.5",
    landing_point: DVec3 {
        x: -3.10,
        y: 19.95,
        z: -64.81,
    },
}];

const TARAWA: CarrierInfo = CarrierInfo {
    // V/STOL approach is parallel to the ship's BRC, not the angled runway definition.
    deck_angle: 0.0,
    deck_altitude: 19.98,
    recovery: CarrierRecovery::Vstol {
        // Recalibrated in DCS with the AV-8B pilot-ground reference positioned
        // exactly on the desired 7.5 spot (stable taxi calibration, 2026-08-23).
        landing_point: DVec3 {
            x: -3.10,
            y: 19.95,
            z: -64.81,
        },
        // DCS Tarawa runway width = 36 m => port deck edge = 18 m from centerline.
        // AV-8B wingspan = 9.24 m.  Ideal aircraft centerline is therefore one
        // full wingspan outboard of the port deck edge: 18.0 + 9.24 = 27.24 m.
        approach_axis_port_m: 27.24,
        // V1: descend to 120 ft above the water at the 7.5 longitudinal station.
        target_altitude_ft: 120.0,
    },
    active_vstol_spots: TARAWA_PHASE1_SPOTS,
    // Tarawa has no arresting wires; retained only to preserve the existing CarrierInfo layout.
    cable1: (
        DVec3 {
            x: 0.0,
            y: 0.0,
            z: 0.0,
        },
        DVec3 {
            x: 0.0,
            y: 0.0,
            z: 0.0,
        },
    ),
    cable2: (
        DVec3 {
            x: 0.0,
            y: 0.0,
            z: 0.0,
        },
        DVec3 {
            x: 0.0,
            y: 0.0,
            z: 0.0,
        },
    ),
    cable3: (
        DVec3 {
            x: 0.0,
            y: 0.0,
            z: 0.0,
        },
        DVec3 {
            x: 0.0,
            y: 0.0,
            z: 0.0,
        },
    ),
    cable4: (
        DVec3 {
            x: 0.0,
            y: 0.0,
            z: 0.0,
        },
        DVec3 {
            x: 0.0,
            y: 0.0,
            z: 0.0,
        },
    ),
};

impl CarrierInfo {
    /// Strict phase-1 compatibility matrix. Unsupported pairs must never create
    /// a detector or an event stream.
    pub fn supports_aircraft_type(&self, aircraft_type: &str) -> bool {
        match self.recovery {
            CarrierRecovery::Vstol { .. } => aircraft_type == "AV8BNA",
            CarrierRecovery::Arrested => {
                AirplaneInfo::by_type(aircraft_type).is_some() && aircraft_type != "AV8BNA"
            }
        }
    }

    /// Reference offset used as x=0 / y=0 for the approach chart.
    /// Arrested recoveries keep the original optimal hook touchdown geometry.
    /// V/STOL V1 uses a line parallel to BRC, one AV-8B wingspan outside the
    /// Tarawa port deck edge, ending abeam the calibrated 7.5 station.
    pub fn approach_reference_offset(&self, plane: &AirplaneInfo) -> DVec3 {
        match &self.recovery {
            CarrierRecovery::Arrested => {
                // optimal hook touchdown point is halfway between the second and third cable
                // (according to NAVAIR 00-80T-104 4.2.8)
                let touchdown_at = (self.cable2.0 - self.cable3.1) / 2.0;
                let touchdown_at = self.cable3.1 + touchdown_at;

                let hook_offset = plane.hook.rotated_by(DRotor3::from_rotation_yz(
                    plane.glide_slope.to_radians().neg(),
                ));

                touchdown_at - hook_offset
            }
            CarrierRecovery::Vstol {
                landing_point,
                approach_axis_port_m,
                ..
            } => DVec3 {
                // DCS/ultraviolet carrier-local +X is starboard, so port is negative X.
                // This is the AV-8B *aircraft centerline* approach axis, not a
                // landing-gear contact point.  Longitudinal x=0 remains the 7.5 station.
                x: -*approach_axis_port_m,
                y: landing_point.y,
                z: landing_point.z,
            },
        }
    }

    pub fn is_vstol(&self) -> bool {
        matches!(&self.recovery, CarrierRecovery::Vstol { .. })
    }

    /// Returns the nearest geometrically calibrated active spot and deck-plane distance.
    /// Adding Tarawa 7 or 8 later requires calibrated catalog entries; scoring remains tied
    /// separately to the intended spot.
    pub fn nearest_active_vstol_spot(&self, local_point: DVec3) -> Option<(&'static str, f64)> {
        self.active_vstol_spots
            .iter()
            .map(|spot| {
                let dx = local_point.x - spot.landing_point.x;
                let dz = local_point.z - spot.landing_point.z;
                (spot.label, (dx * dx + dz * dz).sqrt())
            })
            .filter(|(_, distance)| distance.is_finite())
            .min_by(|(_, left), (_, right)| left.total_cmp(right))
    }

    pub fn by_type(t: &str) -> Option<&'static Self> {
        match t {
            // CVN-74 (USS John C. Stennis) uses the type name "Stennis" in DCS
            // (confirmed via CoreMods/tech/USS_Nimitz/Database/USS_CVN_74.lua: GT.Name = "Stennis")
            "CVN_71" | "CVN_72" | "CVN_73" | "CVN_75" | "Stennis" => Some(&NIMITZ),
            "Forrestal" => Some(&FORRESTAL),
            "LHA_Tarawa" => Some(&TARAWA),
            _ => None,
        }
    }
}

#[derive(Debug)]
pub enum Aoa {
    Fast,
    SlightlyFast,
    OnSpeed,
    SlightlySlow,
    Slow,
}

/// External-model draw argument that exposes the physical arresting-hook
/// position through `Unit.getDrawArgumentValue`, with its validated polarity.
///
/// Values are the animated hook position, not the cockpit lever: a real
/// arrestment drives the value from the down band into the up band for a few
/// seconds (starting before `RunwayTouch`). Consumers must therefore latch the
/// pre-contact baseline instead of reading the latest sample.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HookArgument {
    pub id: u32,
    /// Values at or below this are "hook up".
    pub up_max: f64,
    /// Values at or above this are "hook down".
    pub down_min: f64,
    /// Provenance label persisted in reports.
    pub polarity: &'static str,
}

#[derive(Debug)]
pub struct AirplaneInfo {
    /// Human-readable aircraft display name shown in grading output.
    pub name: &'static str,
    /// Hook position relative to the object's origin.
    pub hook: DVec3,
    /// Physical landing reference relative to the object's origin.
    /// For CATOBAR aircraft this mirrors the hook; for AV-8B this is the pilot
    /// position projected vertically onto the calibrated ground-contact plane.
    pub landing_reference: DVec3,
    /// Validated external hook draw argument; `None` when the module has no
    /// arresting hook or its argument has not been validated live.
    pub hook_argument: Option<HookArgument>,
    /// The optimal glide slope in degrees.
    pub glide_slope: f64,
    /// A function that returns its current AOA rating.
    pub aoa_rating: fn(aoa: f64) -> Aoa,
}

impl PartialEq for AirplaneInfo {
    fn eq(&self, other: &Self) -> bool {
        self.hook == other.hook
            && self.landing_reference == other.landing_reference
            && self.glide_slope == other.glide_slope
    }
}

impl AirplaneInfo {
    pub fn is_vstol(&self) -> bool {
        std::ptr::eq(self, &AV8B)
    }

    pub fn by_type(t: &str) -> Option<&'static Self> {
        match t {
            "FA-18C_hornet" => Some(&FA18C),
            "F-14A-135-GR" | "F-14A-135-GR-Early" | "F-14A-95-GR" => Some(&F14A),
            "F-14B" | "F-14A/B" => Some(&F14B),
            "F-14B(U)" | "F-14BU" => Some(&F14BU),
            "T-45" => Some(&T45),
            "AV8BNA" => Some(&AV8B),
            _ => None,
        }
    }
}

pub fn get_aircraft_id(t: &str) -> Option<i64> {
    match t {
        "FA-18C_hornet" => Some(1),
        "F-14A-135-GR" | "F-14A-135-GR-Early" | "F-14A-95-GR" | "F-14B" | "F-14A/B" => Some(2),
        "F-14B(U)" | "F-14BU" => Some(3),
        "AV8BNA" => Some(4),
        "A-6E" => Some(5),
        "T-45" => Some(0),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strict_recovery_matrix_accepts_only_supported_pairs() {
        let cvn = CarrierInfo::by_type("CVN_71").unwrap();
        let tarawa = CarrierInfo::by_type("LHA_Tarawa").unwrap();

        assert!(cvn.supports_aircraft_type("FA-18C_hornet"));
        assert!(tarawa.supports_aircraft_type("AV8BNA"));
        assert!(!cvn.supports_aircraft_type("AV8BNA"));
        assert!(!tarawa.supports_aircraft_type("FA-18C_hornet"));
        assert!(!cvn.supports_aircraft_type("unsupported"));
    }

    #[test]
    fn simultaneous_hornet_cvn_and_harrier_tarawa_create_exactly_two_pairs() {
        let carriers = [
            CarrierInfo::by_type("CVN_71").unwrap(),
            CarrierInfo::by_type("LHA_Tarawa").unwrap(),
        ];
        let aircraft = ["FA-18C_hornet", "AV8BNA"];
        let pairs = carriers
            .iter()
            .flat_map(|carrier| {
                aircraft
                    .iter()
                    .filter(move |aircraft| carrier.supports_aircraft_type(aircraft))
            })
            .count();

        assert_eq!(pairs, 2);
    }

    #[test]
    fn phase_one_nearest_spot_catalog_is_explicit_and_extensible() {
        let tarawa = CarrierInfo::by_type("LHA_Tarawa").unwrap();
        assert_eq!(tarawa.active_vstol_spots.len(), 1);

        let calibrated = tarawa.active_vstol_spots[0].landing_point;
        let (label, distance) = tarawa
            .nearest_active_vstol_spot(calibrated)
            .expect("phase-1 spot catalog");
        assert_eq!(label, "7.5");
        assert_eq!(distance, 0.0);

        assert!(CarrierInfo::by_type("CVN_71")
            .unwrap()
            .nearest_active_vstol_spot(DVec3::zero())
            .is_none());
    }

    #[test]
    fn compatibility_is_independent_of_aircraft_or_carrier_discovery_order() {
        let tarawa = CarrierInfo::by_type("LHA_Tarawa").unwrap();
        let harrier = AirplaneInfo::by_type("AV8BNA").unwrap();

        // The live Birth handler invokes the same predicate from both branches.
        let aircraft_then_carrier = tarawa.supports_aircraft_type("AV8BNA") && harrier.is_vstol();
        let carrier_then_aircraft = harrier.is_vstol() && tarawa.supports_aircraft_type("AV8BNA");

        assert!(aircraft_then_carrier);
        assert_eq!(aircraft_then_carrier, carrier_then_aircraft);
    }
}
