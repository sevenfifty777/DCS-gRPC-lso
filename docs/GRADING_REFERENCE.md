# Grading and outcome reference

LSO produces a project training score. It is never a USN/USMC certification or an official LSO
grade. Every executable rule below is labelled `OFFICIAL` or `PROJECT-DERIVED`.

## Sources and provenance

`OFFICIAL` references used for vocabulary and symbols:

- NAVAIR 00-80T-104, 1 May 2009, section 6.3.2 for qualification touch-and-go terminology,
  section 6.6.4 for foul-deck waveoff context, and section 11.4.1 for grade symbols;
- NAVAIR 00-80T-111, 15 December 2004, chapter 23 and cards A-5/A-9 for V/STOL phases and the
  human assessment of hover, cross, VL, power, attitude, spot and relative heading.

These publications do not prescribe this module's three-gate formulas, geometric thresholds or
V/STOL A/B/C/D bonus. Those rules are `PROJECT-DERIVED`.

## Structural result

The persisted result separates outcome, display grade, optional points, comment/cause, confidence,
completeness, grading version, cable estimate and DCS cable evidence.

An incomplete observation has grade `NC` and `points = null`. `WO?` means a go-around/waveoff was
observed but its initiator was not proven. The module never invents OWO, WOP or a pilot waveoff.

## Gates

`PROJECT-DERIVED`, `project-derived-v1`:

| Gate | Distance | Acceptance |
|---|---:|---|
| 3/4 NM | 1,389 m | valid inbound bracket, ordered/fresh/skew-valid samples |
| 1/2 NM | 926 m | same |
| 1/4 NM | 463 m | same |

At threshold `x`:

```text
ideal_alt = base_alt + x * tan(aircraft_glide_slope)
gs_deg    = atan2(observed_alt - ideal_alt, x)
lineup    = atan2(lateral_offset, x)
```

Both bracket endpoints must be valid, in phase and lined up. A bracket gap above 300 ms, skew above
300 ms, or non-increasing DCS time invalidates the gate. Starting inside a gate records `Late` and
does not manufacture a historical observation. The three valid gate times must be strictly ordered.

## CATOBAR score

All numerical boundaries and point mappings in this table are `PROJECT-DERIVED`, retained from the
historical module/MOOSE-inspired model pending validation:

| Result | Rule | Points |
|---|---|---:|
| `OK` | all three gates valid; `abs(GS) < 0.5 deg`, `abs(LU) < 1.0 deg` | 4.0 |
| `(OK)` | no significant deviation; `abs(GS) >= 0.5 deg` or `abs(LU) >= 1.0 deg` | 3.0 |
| `--` | `abs(GS) >= 1.0 deg` or `abs(LU) >= 2.0 deg` | 2.0 |
| `C` | quarter-NM GS strictly below `-2.5 deg` | 0.0 |
| `B` | confirmed bolter and all three gates valid | 2.5 |
| `WO?` | neutral waveoff/go-around, initiator unknown | none |
| `NC` | insufficient/invalid telemetry or unconfirmed trap | none |

`OFFICIAL`: `_OK_` is a documented grade symbol in NAVAIR 00-80T-104 section 11.4.1.
`PROJECT-DERIVED`: the code reserves a five-point value for an explicit/manual `_OK_`, but no
automatic rule emits it. The former local "wire 3 plus 15-18.99 seconds" Unicorn rule is disabled.
Groove time and estimated wire cannot produce `_OK_`. A touch-and-go cannot receive `_OK_` or points.

AoA is chart information only. No AoA table changes the grade. Trends, duration of deviations,
continuous excursions, power, sink rate, wind, weight and LSO calls are not scored because no
validated per-aircraft rule has been adopted.

## Wire evidence

`PROJECT-DERIVED` geometry estimates the closest cable midpoint to the transformed hook/contact
position, including the historical 3 m event-latency compensation. Angles are converted from degrees
to radians before rotor construction. The estimate is the primary display wire under decision J,
but is always labelled estimated.

`wire_dcs` is parsed independently from LQM text. `wire_divergent` is true when both sources exist and
differ. Only DCS wire evidence currently confirms an arrested trap for scoring; a minimum geometric
distance or estimated cable alone does not.

## V/STOL experimental score

Everything in this section except the cited doctrinal vocabulary is `PROJECT-DERIVED` and
experimental. It must not be represented as NAVAIR scoring.

Phase 1 activates only AV-8B NA on Tarawa, with `intended_spot = 7.5`. The active geometric catalog
contains only calibrated spot 7.5, so `actual_nearest_spot` is independently selected as 7.5 when a
touchdown position exists. Spots 7 and 8 are explicit future catalog candidates; they are neither
active nor scored until live calibration is available.

The approach score is the arithmetic mean of the three CATOBAR-style gate point values. Three valid
gates are mandatory. The touchdown distance on the deck plane maps to:

| Distance from 7.5 | Spot grade | Bonus |
|---:|---|---:|
| `< 1 m` | A | 1.00 |
| `>= 1 m` and `< 3 m` | B | 0.75 |
| `>= 3 m` and `< 5 m` | C | 0.50 |
| `>= 5 m` | D | 0.00 |

The bonus is capped at 5.0, then mapped to `OK >= 4`, `(OK) >= 3`, `-- >= 2`, otherwise `C`.
The 15 m zone around 7.5 records entry/presence/exit for information only. It never creates a
penalty or `foul deck` result.

The exact Tarawa event behaviour, spot geometry, wire accuracy, hook polarity and VL/RVL boundaries
remain live-validation items. Raw touchdown horizontal speed and raw hook values are retained so
those decisions can later be made without rewriting history.
