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

### What `NC` actually means

`NC` is a single project display symbol, but it is never a single internal reason. The JSON
`cause`/`causes` fields (`docs/DATA_CONTRACTS.md`) already separate "telemetry too degraded to
measure" from every other kind of unavailability, so a diagnostic consumer never has to guess which
applies:

| `cause` value | Meaning | Category |
|---|---|---|
| `telemetry_gap` | A gap in the scored segment exceeded the sample-gap/extrapolation limits | Telemetry too degraded to measure |
| `invalid_telemetry` | A scored sample failed validation (skew, non-monotonic time, etc.) | Telemetry too degraded to measure |
| `position_buffer_limit` | The position buffer overflowed or lost samples in the scored segment | Telemetry too degraded to measure |
| `insufficient_gates` | Telemetry was fine, but fewer than three valid, ordered gates were captured | Structural — nothing to grade |
| `unconfirmed_arrest` | Contact was observed but no DCS/LQM wire confirms an arrest | Proof, not measurement, is missing |
| `unknown` (grading only, `Grading::Unknown`) | The aircraft was tracked but never became a scored approach | Not a telemetry problem at all |

A pass can report more than one of these at once: `cause` is the highest-priority one (see
`Completeness::priority` in `src/track.rs`) and `causes.secondary` lists the rest. Because this
differentiation already exists in the persisted JSON, no additional internal status or field was
introduced: `NC` itself never needs to distinguish these cases, since anything that needs to is
already reading `cause`/`causes`, not the display grade.

## Gates

`PROJECT-DERIVED`, `project-derived-v3`:

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
historical module/MOOSE-inspired model pending validation. `abs(GS)`/`abs(LU)` are the worst values
found across the three gates **and** the continuous trajectory (see below):

| Result | Rule | Points |
|---|---|---:|
| `OK` | all three gates valid; `abs(GS) < 0.5 deg`, `abs(LU) < 1.0 deg` | 4.0 |
| `(OK)` | no significant deviation; `abs(GS) >= 0.5 deg` or `abs(LU) >= 1.0 deg` | 3.0 |
| `--` | `abs(GS) >= 1.0 deg` or `abs(LU) >= 2.0 deg` | 2.0 |
| `C` | GS strictly below `-2.5 deg` at the quarter-NM gate, or anywhere in the continuous trajectory at or inside 463 m | 0.0 |
| `B` | confirmed bolter and all three gates valid | 2.5 |
| `WO?` | neutral waveoff/go-around, initiator unknown | none |
| `NC` | insufficient/invalid telemetry or unconfirmed trap | none |

`OFFICIAL`: `_OK_` is a documented grade symbol in NAVAIR 00-80T-104 section 11.4.1.
`PROJECT-DERIVED`: the code reserves a five-point value for an explicit/manual `_OK_`, but no
automatic rule emits it. The former local "wire 3 plus 15-18.99 seconds" Unicorn rule is disabled.
Groove time and estimated wire cannot produce `_OK_`. A touch-and-go cannot receive `_OK_` or points.

### Continuous trajectory (amplitude only)

`PROJECT-DERIVED`. Historically only the three point-in-time gates fed the grade, so a deviation
spike strictly between two gates (e.g. between 1/2 NM and 1/4 NM) could go completely unscored even
though the full trajectory was already being recorded. `trajectory_deviations` (additive JSON field)
now samples the same GS/lineup geometry as a gate crossing, but continuously, at the aircraft's own
distance, from groove entry to touchdown. Its worst GS-high, GS-low and lineup values are combined
with the three gates' (the maximum of both), and any of its samples at or inside the quarter-NM
distance is checked against the Cut threshold exactly like the quarter-NM gate itself. This can only
ever make the amplitude reading equal or worse than the three-gate-only computation, never better,
and availability is still governed exclusively by `gates.all_valid()` — an incomplete pass is never
made gradable by trajectory data alone.

AoA is chart information only and no AoA table changes the grade. Power, sink rate, wind, weight
and LSO calls are still not scored because no validated per-aircraft rule has been adopted.

### Correction trend (Ok vs (OK) only)

`PROJECT-DERIVED`. NATOPS 00-80T-104 section 11.4.1 distinguishes `OK` ("reasonable deviations
**with good corrections**") from `(OK)` ("fair — reasonable deviations") on whether corrections
were good, not on amplitude alone — a distinction the amplitude-only rules above cannot express:
two passes with identical worst deviations, one that arrived high and corrected to clean, one that
arrived clean and drifted to the same worst value, would otherwise be graded identically.

Once amplitude alone would already grade a pass `Ok` (every trajectory sample and gate under the
`GS_SLIGHT_HIGH`/`GS_SLIGHT_LOW`/`LU_SLIGHT` thresholds), a simple two-point slope of `abs(GS)` and
`abs(lineup)` is computed over the final 4 seconds of the recorded trajectory (`TREND_WINDOW_S`,
chosen because a correction takes roughly 1-2 s to fly, so 4 s gives room to see a real trend
without reaching into an unrelated earlier part of the approach — deliberately no more
sophisticated filtering than that, per the design brief for this item). If either slope reaches
`TREND_WORSENING_DEG_PER_S` (0.075 deg/s — chosen so the check is reachable at all within the Ok
amplitude ceiling over that window, while staying clearly above ordinary aim-point noise; not a
NAVAIR value), the pass is capped at `(OK)` instead of `Ok`.

Trend is asymmetric by design: it can only ever **hold back** an `Ok` that amplitude alone would
have granted, never **raise** a grade amplitude alone placed below `Ok`, and it is never checked at
all once a pass is already below `Ok` — matching the same "never invent leniency" rule already
applied to the continuous trajectory itself. Whether a deviation was corrected within the window,
rather than only whether it grew, is not evaluated; nor is duration of a given deviation. Both would
require more than the two-point derivative this item's design brief called for.

### Wind

`PROJECT-DERIVED`, contextual only. `wind_heading_deg`/`wind_speed_mps` (additive JSON fields) are
queried once per recovery from `AtmosphereService.GetWind` at the carrier's last known position, so
a report can show that a deviation happened in a stiff crosswind rather than calm air. Both fields
are absent when the query fails or in `--positions-only` (which never queries output-only DCS
metadata). **Wind never changes `pass_grade` or `grade_points`**: the project has no validated
doctrine for how much correction credit a given wind condition should earn, so inventing one here
would be exactly the kind of unverified rule this module otherwise avoids.

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
