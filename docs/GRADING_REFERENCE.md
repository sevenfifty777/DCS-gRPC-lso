# LSO Grading Reference

This document specifies the behavior implemented by `src/track.rs`, `src/grading.rs`, and
`src/data.rs`. LSO's grading is a simplified geometric model inspired by NAVAIR/MOOSE conventions;
it is not a complete reproduction of a human LSO grade.

## Detection and recording

Every supported carrier/aircraft pair is checked every two seconds. Live recording starts when all
of these conditions hold:

| Condition | Implemented value |
|---|---|
| Plane altitude | At or below 1,100 ft MSL |
| Distance to carrier | 200 m through 3.5 nm, inclusive |
| Heading or quadrant | None; the full overhead pattern is eligible |

Recording switches to 100 ms transform polling and tracks mission events. It stops when the plane
leaves the 3.5 nm / 1,100 ft envelope, a relevant unit disappears, or an outcome completes and the
plane moves away. A recording that never goes below 100 m MSL is discarded. A finished live track
with `Grading::Unknown` is also discarded.

## Coordinate streams

### Final approach

`Datum` is calculated in the angled-deck frame from the aircraft-specific optimal touchdown point:

| Field | Meaning |
|---|---|
| `time` | DCS scenario seconds |
| `x` | Along-deck distance to optimal touchdown in meters; positive on the approach side |
| `y` | Lateral displacement in meters; positive is right of centerline |
| `alt` | Hook altitude above deck in meters, clamped to zero |
| `aoa` | Calculated angle of attack in degrees |

The carrier position used for this geometry is smoothed with an EMA (`alpha = 0.15`).

### Pattern

`PatternDatum` uses raw carrier position and the ship's BRC frame:

| Field | Meaning |
|---|---|
| `time` | DCS scenario seconds |
| `astern_m` | Positive behind the carrier along BRC |
| `port_m` | Positive to port of BRC centerline |
| `alt_ft` | Aircraft altitude MSL in feet |
| `aoa` | Calculated angle of attack in degrees |

The pattern PNG displays +/-2.5 nm port/starboard and +/-3 nm ahead/astern.

## Gate sampling

On the first inbound frame at or inside each gate, LSO freezes a `GateDatum` if `x > 0` and the
hook is no higher than 500 ft above the deck:

| Gate | Along-deck distance |
|---|---:|
| 3/4 nm | 1,389 m |
| 1/2 nm | 926 m |
| 1/4 nm | 463 m |

When an aircraft flies outbound beyond a gate, the corresponding captured gate is cleared so a
later real final can resample it.

Each sample contains:

```text
ideal_gs_alt     = x * tan(aircraft_glide_slope)
gs_deviation_deg = atan2(hook_alt - ideal_gs_alt, x)
lineup_deg       = atan2(y, x)
```

Positive GS is high; negative GS is low. Positive lineup is right of centerline. Equivalent feet
are stored for presentation only and do not determine the grade.

Groove entry is recorded when `x <= 1,389 m`, hook altitude is at or below 300 ft above the deck,
and absolute lineup is at most 10 degrees. Groove time is touchdown time minus this entry time.

## Outcome model

| Internal outcome | Detection | Saved display outcome |
|---|---|---|
| `Recovered` | Matching `RunwayTouch` event | `Wire #N` or `Landed` |
| `IntentionalBolter` | Aircraft moves away after the hook was observed up | `Qualif Bolter` |
| `Bolter` | Aircraft reaches/passes the deck area and moves more than 150 m away without arresting | `Bolter` |
| `WaveoffPilot` | Aircraft entered the groove and moves away without reaching the deck | `Waveoff` |
| `Unknown` | No recognized outcome | Discarded by live mode |

The hook-up test uses DCS draw argument 25. On a recovered pass, a wire parsed from DCS
`LandingQualityMark` text takes precedence over the connector-based geometric estimate.

## Grade labels and points

| Enum | Label | Points | Implemented rule |
|---|---|---:|---|
| `Unicorn` | `_OK_` | 5.0 | Base `OK`, wire 3, and groove time from 15.0 through 18.99 seconds |
| `Ok` | `OK` | 4.0 | All available gate deviations remain below the slight thresholds |
| `OkParentheses` | `(OK)` | 3.0 | At least one slight deviation, with no significant deviation |
| `NoGrade` | `--` | 2.0 | At least one significant deviation, or internal `Unknown` |
| `Cut` | `C` | 0.0 | Quarter-nm GS is strictly below -2.5 degrees |
| `Bolter` | `B` | 2.5 | Bolter outcome |
| `WaveoffPilot` | `WO` | 1.0 | Pilot waveoff outcome |

### Thresholds

The worst available sample across the three gates is used:

| Axis | `OK` range | `(OK)` threshold | `--` threshold |
|---|---|---|---|
| Glideslope high | `< +0.5 deg` | `>= +0.5 deg` | `>= +1.0 deg` |
| Glideslope low magnitude | `< 0.5 deg` | `>= 0.5 deg` | `>= 1.0 deg` |
| Absolute lineup | `< 1.0 deg` | `>= 1.0 deg` | `>= 2.0 deg` |

Cut is checked first and only from the quarter-nm sample. At exact threshold values, the `>=`
branch applies; the Cut test is strict `< -2.5 deg`.

Special outcomes override normal gate grading:

```text
WaveoffPilot -> WO
Bolter        -> B
Unknown       -> -- (but live mode discards Unknown before saving)
```

`IntentionalBolter` is handled with recovered gate logic when a cable can be estimated. Without an
estimated cable, it is graded `B` while retaining the `Qualif Bolter` outcome.

Missing gate samples are skipped rather than treated as automatic deviations. Consequently, a pass
with incomplete samples can still receive a favorable base grade; this is an implementation
limitation.

## AoA visualization

AoA selects track colors but does not change the grade.

| Aircraft | DCS type names | On-speed interval |
|---|---|---|
| F/A-18C | `FA-18C_hornet` | `> 7.4 deg` and `< 8.8 deg` |
| F-14A | `F-14A-135-GR`, `F-14A-135-GR-Early`, `F-14A-95-GR` | `> 10.2 deg` and `< 11.1 deg` |
| F-14B | `F-14B`, `F-14A/B` | `> 10.2 deg` and `< 11.1 deg` |
| F-14B(U) | `F-14B(U)`, `F-14BU` | `> 10.2 deg` and `< 11.1 deg` |
| VNAO T-45C | `T-45` | `> 6.5 deg` and `< 7.5 deg` |

All supported aircraft use a nominal 3.5-degree glide slope.

## Carrier geometry

| DCS type names | Geometry | Deck angle | Deck altitude |
|---|---|---:|---:|
| `CVN_71`, `CVN_72`, `CVN_73`, `CVN_75`, `Stennis` | Nimitz | 9.1359 deg | 20.1494 m |
| `Forrestal` | Forrestal | 9.42 deg | 18.46 m |

The optimal hook touchdown point is midway between the midpoint of wire 2 and the midpoint of wire
3, corrected for the aircraft's rotated hook offset.

## What the grade does not evaluate

- AoA, airspeed, sink rate, throttle/power, wind-over-deck, fuel/weight, or aircraft configuration.
- Deviation duration, corrections between gates, or accumulated LSO calls.
- Human LSO waveoff commands or landing after an LSO-commanded waveoff.
- Case-specific groove-time windows beyond the single `_OK_` interval.

Treat the result as a training aid and code-defined score, not an authoritative real-world grade.
