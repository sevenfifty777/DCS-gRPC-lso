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
| `WO` | DCS LSO ordered a waveoff and the aircraft never touched the deck (`OFFICIAL` symbol, `PROJECT-DERIVED` points) | 1.0 |
| `C` | deck contact after a DCS-ordered waveoff (NAVAIR 00-80T-104: landing after a waveoff), whatever the gates say | 0.0 |
| `WO?` | neutral waveoff/go-around, initiator unknown | none |
| `NC` | insufficient/invalid telemetry or unconfirmed trap | none |

A hook-up deck contact (`T&G (CQ)`) keeps the measured approach grade and never receives wire or
trap upgrades. A `WO` outcome does not require three valid gates; every other grade does.

`OFFICIAL`: `_OK_` is a documented grade symbol in NAVAIR 00-80T-104 section 11.4.1.
`PROJECT-DERIVED`: the code reserves a five-point value for an explicit/manual `_OK_`, but no
automatic rule emits it. The former local "wire 3 plus 15-18.99 seconds" Unicorn rule is disabled.
Groove time and estimated wire cannot produce `_OK_`. A touch-and-go cannot receive `_OK_` or points.

The three-gate table above is the historical model and still decides a pass that has no
continuous trajectory. Since the episode grader (next section) a CATOBAR pass with a recorded
groove is graded from its episodes, and AoA does grade the T-45C, the F-14 and the F/A-18C: the
T-45C and F-14 bands were measured on the cockpit indexer on 14 September 2026
(`docs/AOA_CALIBRATION_REVIEW_2026-09-14.md`), the F/A-18C band is documented but not yet verified.
Power, wind, weight and LSO calls are still not scored.

## Episode grading (CATOBAR, continuous trajectory)

`PROJECT-DERIVED`. From groove entry to touchdown the glideslope, lineup and AoA series are cut
into episodes: a run of samples outside the target band, at least two samples long, ended by two
consecutive samples back inside it. Each episode carries a size, a zone and a correction verdict.

| Size | Glideslope | Lineup | AoA | LSO shorthand |
|---|---|---|---|---|
| small | 0.5 to 1.0 deg | 1.0 to 2.0 deg | donut plus a chevron ("slightly") | `(X)`, a little |
| medium | 1.0 to 1.5 deg | 2.0 to 3.0 deg | chevron alone ("fast", "slow") | `X`, moderate |
| large | 1.5 deg and above | 3.0 deg and above | more than 2.0 deg outside the on-speed band (`aoa_large_error_deg`) | `_X_`, gross |

Zones by distance to the landing point: START from groove entry to 926 m, MIDDLE to 463 m, IN CLOSE
to 150 m, RAMP the last 150 m. The correction verdict looks at what happened after the peak: good
(durable improvement within the zone's deadline, 3.0 s at the start down to 0.75 s at the ramp, and
back inside the band), average (real but late or incomplete improvement, or a series that ended at
touchdown before anything could be seen), poor (no improvement, worsening after an improvement, or
two or more reversals larger than the swing threshold: 0.3 deg for glideslope and lineup, 1.0 deg
for AoA under `CONVENTION`, because the computed AoA carries about 0.8 deg of spread). An AoA
episode shorter than one second is written to the report but does not grade.

Two scoring models exist behind `CatobarGradingPolicy` (`src/grading.rs`):

- **Weighted ladder** (`BASELINE`, `PROTOTYPE`): effective severity = corrected level (size, minus
  one for good, plus one for poor) times the zone weight (1.0 / 1.2 / 1.5 / 2.0). The pass grade
  is `OK` below 1.5, `(OK)` below 3.0, `--` otherwise, from the worst episode.
- **Convention table** (`CONVENTION`, the production default since 15 September 2026): the grade
  band of each episode is read from the written LSO convention (NAVAIR 00-80T-104 grade
  definitions, LSO NATOPS shorthand), on the same scale:

  | Deviation | Correction | START, MIDDLE | IN CLOSE, RAMP |
  |---|---|---|---|
  | a little | good | `OK` | `OK` |
  | a little | average | `OK` | `(OK)` |
  | a little | poor | `(OK)` | `(OK)` |
  | moderate | good | `OK` | `(OK)` |
  | moderate | average | `(OK)` | `(OK)` |
  | moderate | poor | `--` | `--` |
  | gross | good | `(OK)` | `--` |
  | gross | average or poor | `--` | `--` |

  "Good" in this table also requires that the axis came back inside its band; a gross excursion
  reduced to a moderate one and held there is an average correction. The three cells where the
  weighted ladder disagreed with the convention were: moderate with average correction in close or
  at the ramp (ladder `--`, convention `(OK)`), a little with poor correction in close or at the
  ramp (ladder `--`, convention `(OK)`), and gross with good correction at the start or middle
  (ladder `(OK)`, convention `--` unless back in the band).

`lso grade-ab <reports>` prints both models and every intermediate step per recorded pass. The
comparison on 44 recorded passes is in `docs/GRADING_CONVENTION_PROTOTYPE_2026-09-15.md`.

## Commanded hook state

`PROJECT-DERIVED`, validated on the 2026-09-02/03 live corpus. The external draw argument
(`25` on F/A-18C and T-45, `1305` on all F-14 variants; `0` = up, `1` = down) is the *animated*
hook position: on a real trap it drops into the up band 0.5-1.4 s before `RunwayTouch` and returns
1.7-6.5 s later. The commanded state is therefore latched from the latest stable run (at most 3 s,
at least 5 samples spanning 0.5 s, all in one band) of in-groove samples that ends 1.5 s before the
earliest contact evidence (touchdown event or deck-threshold crossing). Modules without a validated
argument, unstable baselines and mixed bands stay `unknown`, which keeps today's conservative
behaviour. The persisted `hook_observation.baseline_*` fields show the window that was used.

## Arrest confirmation

An arrested outcome is confirmed by, in order of preference: a DCS `WIRE#`; a complete hook
transient correlated with a cable crossing (below); or `PROJECT-DERIVED` deck kinematics, validated
on the same corpus: the carrier-relative horizontal displacement over a one-second window falls to
6 m/s or less and stays below 8 m/s for two consecutive seconds, starting within eight seconds of
contact, inside the run-out band (-160 m to +60 m along the deck) with no telemetry gap above
300 ms. The arresting cable pulls the aircraft back at 10-15 m/s for about 1.5 s before it settles,
which is why the rule looks for any qualifying window rather than the first slow sample. Bolters and
hook-up touch-and-go passes leave the deck at 45-60 m/s and never qualify. Kinematics confirm the
arrest only; they never name a wire, so the outcome reads `Arrested (wire unknown)` and
`arrest_evidence: kinematic`. Deck contact without any of the three proofs stays
`unconfirmed_arrest` / `NC`.

## Wire evidence

`PROJECT-DERIVED` estimation continuously records when the transformed hook crosses each finite
pendant while the aircraft is in the groove. A crossing is rejected if the hook is outside the cable
endpoints, more than 3 m vertically from the cable, or the telemetry bracket exceeds 300 ms. This
prevents an overhead or high-altitude crossing of an infinite cable plane from suppressing the real
deck crossing.

The selected wire is the last valid crossing no more than 200 ms before a complete external hook
transient: stable down (`raw >= 0.8`) for at least 0.2 s, deflected (`raw <= 0.7`) within 2 s of the
touchdown event, then recovered to down within 8 s. A stable hook-up value, a transition that does
not recover, stale samples, and geometry without a matching hook transient produce no estimate.

`wire_dcs` is parsed independently from LQM text and remains authoritative whenever present. The
estimate is the labelled fallback when DCS supplies no wire, including a recovery supervised by a
human LSO. `wire_divergent` remains true when both sources exist and differ. A completed correlated
estimate can confirm an arrested trap for scoring; a touchdown event or cable-plane crossing alone
cannot.

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
