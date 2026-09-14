# AoA calibration flight, 14 September 2026: results and the prototype to test

Two pilots, three aircraft types, 22 passes on CVN-72, recorded by LSO 0.5.0 (`664fe5b`, clean) against DCS-gRPC 0.10.0 in buffered mode at 20 Hz. For the first time the pilots' own DCS clients recorded, twenty times a second, what the flight model and the cockpit AoA indexer were saying (`tools/aoa_calibration/LsoAoaExport.lua`), so the LSO's computed angle of attack could be checked against the truth, sample by sample.

Source folder: `tools/aoa_calibration/recovery_aoa_calibration/` (22 JSON reports, charts, ACMI, `lso.db`, `lso.log`, `dcs.log`, `gRPC.log`) plus the two client logs `tools/aoa_calibration/lso_aoa_20260914-202812.csv` (Ghost-72 | TT, T-45C then F-14B(U)) and `lso_aoa_20260914-210902.csv` (Justice, F-14B(U)). Raw tool output: `docs/AOA_CALIBRATION_2026-09-14.md`. Times in this document are UTC (the file names in the folder are local time, two hours later).

Part 1 is written for everyone. Part 2 has the numbers and the method.

---

## Part 1. In plain language

### What we did

The LSO program grades three things on every pass: glideslope, lineup and angle of attack (AoA, "on speed"). Glideslope and lineup are geometry, easy to check. AoA is not: the program has to compute it from the aircraft's speed, its attitude and the wind, and until now nobody had checked whether the number it computes is the number the pilot sees on the indexer.

On 14 September, Ghost-72 and Justice flew with a small recorder installed on their own PC. It wrote down, twenty times a second, the true AoA of the flight model and which of the three indexer lamps were lit (fast chevron, donut, slow chevron). The brief asked them to fly normal passes but also to deliberately hold the fast chevron on some passes and the slow chevron on others, so that every lamp switch would be recorded.

### What we found

**1. The program measures AoA correctly.** Over 20 passes and 7,600 samples in the groove, the LSO's computed AoA differs from the flight model's true AoA by 0.02 degrees on the T-45 and 0.07 degrees on the F-14 (medians). Eight samples in ten are within half a degree at any instant. The difference does not grow with bank, and it did not change on the one pass where the program had to use a substituted wind reading. This part of the program works.

**2. The T-45 "on speed" band in the program was wrong by almost two degrees.** The program believed the donut meant 6.5 to 7.5 degrees. The cockpit says 8.25 to 8.75. So a T-45 pilot flying a perfectly centred donut (8.5 degrees) was being called "slow" by the program on every pass, and a pilot showing the fast chevron (7 degrees) was being called "on speed". This explains why every T-45 pass of 13 September was "slow" in the report. The mistake came from a comment in the aircraft module's own code ("degrees = units minus 10") that turned out to be false: the cockpit gauge shows 17 units at 8.5 degrees, not at 7.

**3. The F-14 band was close, but 0.3 degrees too high.** The program believed the donut meant 10.2 to 11.1 degrees; the cockpit says 9.9 to 10.8. Small, but a pilot centred in the donut (10.4) was sitting near the program's "slightly fast" edge.

**4. The conclusion of 13 September about the F-14 was wrong, and is withdrawn.** That review said "the F-14 reads 3 to 4 degrees fast on every pass, the reference must be wrong". The reference was only 0.3 degrees off. The pilots really were fast: on 14 September the cockpit indexer showed the fast chevron for 100% of the groove on five F-14 passes and 70 to 85% on four more. Some of that was the calibration brief (hold the chevron), but the 13 September F-14 passes, flown with no such brief, sit at the same 6 degrees.

**5. DCS's own LSO is not a reference for AoA.** On the 19:41 pass DCS wrote "SLO X" (slow all the way) while the pilot's cockpit showed the donut for 62% of the groove and the slow chevron for 10%. Its comments should not be used to judge the program's AoA calls.

### What changes

Both bands in the program are replaced by the measured ones, and the T-45 and F-14 are now flagged "calibrated", which means the AoA axis is allowed to affect their grade under the prototype policy. Nothing changes in production until the branch is merged.

| Type | Old band (donut alone) | Measured band (donut alone) | Full measured band, fast chevron to slow chevron |
|---|---|---|---|
| T-45C | 6.5 to 7.5 | **8.25 to 8.75** | 8.0 / 8.25 / 8.75 / 9.0 |
| F-14 (all variants) | 10.2 to 11.1 | **9.95 to 10.8** | 9.45 / 9.95 / 10.8 / 11.25 |
| F/A-18C | 7.4 to 8.8 | not flown, unchanged | 6.9 / 7.4 / 8.8 / 9.3 (documented, unverified) |

A detail pilots should know: the T-45 donut is narrow. Donut alone is only half a degree wide, and a chevron lights next to the donut a quarter of a degree either side. The F-14's donut is almost twice as wide (0.85 degrees). The program now sees both the way the cockpit does.

### What it means for grades

Re-grading the 22 passes with the measured bands changes very little on this particular set, because most of these passes were deliberately flown fast or slow. The one visible move is the 18:52 T-45 trap: recorded `(OK)`, it becomes `--` because the pilot flew the fast chevron for 86% of the groove, which the old band had called "on speed". The 19:38 F-14 trap (Ghost-72, wire 1) becomes `(OK)` under the prototype policy with either band.

The more important comparison is against the candidate that was on the table before this flight, "P3: AoA does not grade the T-45 and F-14 until calibrated". That candidate would have given `OK` or `(OK)` to nine `--` passes of 14 September and seven of 13 September. On six of the nine, the cockpit showed a chevron alone, fast or slow, for 72 to 100% of the groove. With the bands now measured, that shortcut is no longer needed, and on those six it would have been wrong.

What remains open is not measurement but policy: how much should a sustained "fast" or "slow" cost, and should a medium AoA excursion in the last two seconds before the wires always be a `--` (today it is, because a medium at the ramp is scored 2 times 2.0 = 4.0 whatever the pilot did before). Those two questions need a human LSO's grades on the same passes, and now they can be asked with confidence that the AoA numbers behind them are real.

### The prototype to test, and what would make it production

**Test now:** the prototype policy as it exists on this branch, with the measured bands. In the tables of Part 2 that is the column "P2, measured bands": touchdown ends the correction assessment, AoA needs one second of persistence to grade, and the AoA axis grades the T-45, the F-14 and the F/A-18C. The one-second rule is exactly what the measurement noise requires: the longest fast or slow reading caused by noise alone while the cockpit showed the donut was 0.85 seconds.

**Before production**, three things:

1. One normal session where the pilots fly the donut as they usually would (no chevron holding), to see the grade distribution the new bands produce on ordinary flying. The 13 September set, re-graded below, is a first look: seven of eight passes remain `--`, and all seven are decided by sustained fast or slow readings that the calibrated bands now vouch for.
2. A human LSO grades those passes blind, and the two policy questions above are settled against that reference.
3. The same calibration flight for the F/A-18C. Its band comes from an external document and has never been checked. The two Hornet passes flown by Ducks 1-1 on 14 September sit inside it (median 8.4 degrees, 14 to 19% of the groove "slightly slow"), which is reassuring but not a calibration.

One operational finding outside the AoA question: as soon as two pilots were in the pattern at the same time (19:21 to 19:38), the shared read budget of the server was saturated. Every pass in that window came out with orange telemetry health, one red. Details and numbers in Part 2, section 9. The AoA calibration itself was not affected, but the grading confidence flag was.

---

## Part 2. Technical

### 1. Data set

| Item | Value |
|---|---|
| Reports | 22 (`LSO-*.json`): 6 T-45C (Ghost-72), 14 F-14B(U) (6 Ghost-72, 8 Justice), 2 F/A-18C (Ducks 1-1) |
| Outcomes | 7 traps (all with DCS `WIRE#`), 12 touch-and-go with hook up, 2 bolters, 1 approach with unknown outcome (Cut, 2.5 deg low in close), 1 wave-off |
| Client CSV, Ghost-72 | 154,498 rows, 1291 to 5871 s model time, 33 Hz (another export hook shortens the period), one 102 s gap between slots |
| Client CSV, Justice | 57,255 rows, 3671 to 6534 s, 20 Hz, no gaps |
| Groove samples matched (LSO datum with a client sample within 0.3 s) | 2,358 T-45, 5,265 F-14 |
| Columns that came back empty | `wind_*` (`LoGetWindVelocity` returns nothing in the export sandbox), `hud_aoa_units` (the T-45 HUD `list_indication(10)` block is not named `AOA`) |
| Columns that worked | `aoa_true_deg`, the three indexer lamps (clean 0/1), the AoA gauge, `hook_draw_arg` (T-45 1.0 to 0.0 and back at each trap, F-14 1.0 to 0.16 and back to 0.86) |
| Mission wind | 1.0 to 1.4 m/s from 075 at deck level, 6.0 to 6.5 m/s at 70 to 100 m (the two probes behind every wind reference); one pass (19:21) used the high probe at deck level after the deck probe returned the 180/0.0 sentinel |
| Acquisition | 20.0 Hz effective on all 22; poll latency p95 40 to 49 ms when one pilot was in the pattern, 114 to 279 ms with two (section 9) |

### 2. Method

`tools/aoa_calibration/align_aoa.py`, rewritten during this analysis:

- **Pilot matching.** The first version matched client rows to a report by aircraft type only. From 19:21 both pilots were in the F-14B(U) at the same time and their samples were interleaved, which produced nonsense differences (+25 and +152 degrees on two passes). Rows are now taken from the CSV whose `unit_name` equals the report's `pilot_name`, with a type-only fallback and a warning. All 20 passes matched their pilot.
- **Clock alignment.** For each pass, the constant offset between server datum time and client model time is searched in 50 ms steps over plus or minus 2 s, minimising the *spread* of the difference, not its size, so a real computation offset cannot steer the alignment. The offset came out at -0.55 to -0.65 s on 17 of 20 passes: the server's datum for a given physical state carries a timestamp about 0.6 s later than the client's. The three exceptions (-0.05 to -0.25 s) are passes where the AoA was nearly flat, which gives the search little to lock on.
- **Difference.** LSO `datums[].aoa` minus the client's `aoa_true_deg` interpolated at the aligned time, over the groove only (`groove_entry.timestamp_dcs` to `touchdown_time_dcs`).
- **Lamp thresholds.** Over the whole flight, not just the groove, every change of indexer state between two consecutive client samples (gap 0.3 s or less, AoA change under 1 degree) is recorded with the mid-point true AoA. Both directions are kept, so hysteresis is visible.
- **Agreement.** For every matched groove sample, the cockpit state at the same instant against the LSO rating under a band, as a confusion table; also with the LSO value smoothed over a centred 0.5 s window.
- **Grading impact.** `lso grade-ab` (the read-only re-grader of the prototype branch) on the 22 reports and on the 8 reports of 13 September, once with the old bands and once with the measured ones.

### 3. The computation is right

LSO computed AoA minus flight-model true AoA, groove only, degrees:

| Pass (UTC) | Pilot | Type | Wind ref | Offset | n | Median | p10 | p90 | Slope per degree of bank |
|---|---|---|---|---|---|---|---|---|---|
| 18:33 | Ghost-72 | T-45 | yes | -0.65 s | 392 | -0.01 | -0.27 | +0.39 | +0.010 |
| 18:37 | Ghost-72 | T-45 | yes | -0.60 s | 574 | -0.03 | -0.35 | +0.37 | -0.050 |
| 18:39 | Ghost-72 | T-45 | yes | -0.55 s | 335 | +0.11 | -0.40 | +0.53 | -0.015 |
| 18:41 | Ghost-72 | T-45 | yes | -0.65 s | 465 | +0.09 | -0.29 | +0.47 | -0.032 |
| 18:49 | Ghost-72 | T-45 | yes | -0.60 s | 275 | +0.07 | -0.37 | +0.46 | -0.118 |
| 18:52 | Ghost-72 | T-45 | yes | -0.60 s | 317 | -0.04 | -0.32 | +0.20 | +0.001 |
| 19:00 | Ghost-72 | F-14 | yes | -0.65 s | 281 | -0.05 | -0.63 | +0.83 | +0.045 |
| 19:04 | Ghost-72 | F-14 | yes | -0.25 s | 300 | -0.08 | -0.26 | +0.31 | -0.043 |
| 19:21 | Justice | F-14 | yes (fallback) | -0.50 s | 366 | +0.21 | -0.29 | +0.60 | +0.031 |
| 19:24 | Ghost-72 | F-14 | yes | -0.65 s | 239 | -0.55 | -0.95 | +0.79 | -0.016 |
| 19:25 | Justice | F-14 | yes | -0.65 s | 337 | +0.13 | -0.55 | +0.72 | -0.034 |
| 19:26 | Ghost-72 | F-14 | yes | -0.10 s | 500 | -0.03 | -0.20 | +0.20 | +0.008 |
| 19:27 | Justice | F-14 | yes | -0.65 s | 320 | +0.16 | -0.34 | +0.56 | +0.032 |
| 19:30 | Justice | F-14 | yes | -0.50 s | 463 | +0.16 | -0.32 | +0.62 | +0.004 |
| 19:33 | Justice | F-14 | yes | -0.60 s | 581 | +0.08 | -0.43 | +0.59 | -0.024 |
| 19:34 | Ghost-72 | F-14 | yes | -0.60 s | 446 | +0.13 | -0.38 | +0.50 | +0.014 |
| 19:35 | Justice | F-14 | yes | -0.65 s | 458 | +0.11 | -0.37 | +0.53 | -0.041 |
| 19:38 | Ghost-72 | F-14 | yes | -0.60 s | 249 | -0.02 | -0.52 | +0.53 | -0.056 |
| 19:38 | Justice | F-14 | yes | -0.65 s | 361 | +0.19 | -0.37 | +0.57 | -0.047 |
| 19:41 | Justice | F-14 | yes | -0.65 s | 364 | +0.15 | -0.25 | +0.60 | -0.044 |

Per type over all matched samples: T-45 median +0.02, mean +0.04, p10 -0.32, p90 +0.44 (n = 2,358); F-14 median +0.07, mean +0.08, p10 -0.43, p90 +0.56 (n = 5,265).

Reading it:

- **No offset.** Every pass median is within 0.21 degrees of zero except 19:24 (-0.55, a short pass at 12 s of groove with the widest AoA swing of the day, 0 to 10.7 degrees, where interpolation across the swing costs more).
- **No bank dependence.** The slope of the difference against absolute bank is between -0.06 and +0.05 degrees per degree of bank on 19 passes (-0.12 on one). At the 5 to 10 degrees of bank seen in the groove that is under 0.3 degrees, and it changes sign from pass to pass, so it is noise, not a term missing from the correction.
- **The substituted wind did not matter.** The 19:21 pass used the 70 m reading (6.5 m/s) at deck level because the deck probe returned the known 180/0.0 sentinel. Its median is +0.21 against +0.08 to +0.19 on Justice's other passes. At 65 m/s approach speed a 5.5 m/s wind error moves the pitch-plane AoA by at most a quarter of a degree, and less when the wind is not aligned with the approach. Finding F7 of the live review (mark AoA unreliable when the low probe is substituted) is still correct in principle but, at this wind speed, has no visible effect.
- **The wind correction was exercised a little.** The reference interpolates between 1 m/s at the deck and 6 to 6.5 m/s at 70 to 100 m, so the wind actually subtracted ran from about 6 m/s at groove entry down to 1 m/s at the ramp. The residual is flat across the groove. A session with 15 to 20 knots of deck wind is still the real test.
- **Noise.** The p10 to p90 width is 0.7 to 0.9 degrees on most passes. Smoothing the LSO series over 0.5 s changes the agreement tables by at most 3 points (section 5), so most of it is genuine short-term difference between the two computations (the server sees the state 0.6 s late through a 20 Hz sampler, the client reads it directly), not white noise that a filter would remove.
- **Two pilots in the pattern** (19:21 to 19:38, section 9) widened p10 to p90 slightly (-0.4 / +0.6 against -0.3 / +0.4) but did not move the median.

### 4. Indexer thresholds

True AoA at the lamp switch, degrees, over the whole flight of each pilot:

| Threshold | T-45C | switches | towards slow / towards fast | F-14B(U) | switches | towards slow / towards fast |
|---|---|---|---|---|---|---|
| fast chevron alone ends, donut appears | **8.00** | 80 | 8.02 / 7.98 | **9.47** | 74 | 9.55 / 9.38 |
| fast chevron goes out, donut alone | **8.25** | 88 | 8.27 / 8.24 | **9.93** | 83 | 10.01 / 9.85 |
| slow chevron appears next to the donut | **8.75** | 72 | 8.77 / 8.73 | **10.82** | 62 | 10.89 / 10.70 |
| donut goes out, slow chevron alone | **9.00** | 76 | 9.02 / 8.98 | **11.27** | 30 | 11.41 / 11.19 |

- The T-45C thresholds are exact quarter degrees with a hysteresis under 0.05 degrees. The F-14's are symmetric about 10.37 degrees (donut plus or minus 0.45, chevron alone beyond plus or minus 0.9) with a hysteresis of about 0.15 degrees.
- **Cockpit gauge against true AoA** (linear fit on lit-indexer samples): T-45C `units = 6.58 + 1.228 * degrees` (17 units at 8.5 degrees, residual 0.26 units, the needle lags); F-14 gauge argument 2003 scaled 0 to 30: `units = 3.55 + 1.03 * degrees` on both pilots (residual 0.16 to 0.27). The T-45 relation disproves the module's "degrees = units - 10" comment that the old band was built on. The F-14 relation does not match the manual's 15 units on speed under the forum formula `degrees = units / 1.0989 - 3.01` (that gives 10.64), but the argument's scale to units is an assumption, and the lamp thresholds in degrees are what the program needs.
- The same values seen from the groove only: true AoA while the T-45 showed donut alone was 8.28 to 8.72 (p10 to p90, n = 240), slightly fast 8.01 to 8.24, slightly slow 8.79 to 8.96; F-14 donut alone 9.98 to 10.69 (n = 705), slightly fast 9.54 to 9.94, slightly slow 10.77 to 11.26. Consistent with the transition analysis.

Bands now in `src/data.rs` (fast up to, slightly fast up to, on speed below, slightly slow below, slow from):

| Type | Before | Measured, now in the code | Change |
|---|---|---|---|
| T-45C | 6.0 / 6.5 / 7.5 / 8.0 | 8.0 / 8.25 / 8.75 / 9.0 | +1.75 to +2.0 deg |
| F-14A, F-14B, F-14B(U) | 9.7 / 10.2 / 11.1 / 11.6 | 9.45 / 9.95 / 10.8 / 11.25 | -0.25 to -0.35 deg |
| F/A-18C | 6.9 / 7.4 / 8.8 / 9.3 | unchanged | not flown |

Only the F-14B(U) was flown; the A and B share the Heatblur cockpit and AoA vane, so they carry the same band and flag. `aoa_grading_calibrated` is now `true` for the T-45C and the three F-14s.

### 5. Agreement between cockpit and program

Groove samples, cockpit state at the aligned instant against the LSO rating of its own computed AoA (rows sum to 100%):

**T-45C, old band** (exact agreement 53%, within one step 63%):

| cockpit \ LSO | fast | slightly fast | on speed | slightly slow | slow |
|---|---|---|---|---|---|
| fast (1,209) | 45% | 10% | 16% | 25% | 3% |
| slightly fast (135) | 1% | 0% | 1% | 35% | 64% |
| on speed (169) | 0% | 0% | 0% | 4% | **96%** |
| slightly slow (104) | 0% | 0% | 0% | 1% | 99% |
| slow (707) | 1% | 0% | 1% | 1% | 97% |

**T-45C, measured band** (exact 85%, within one step 97%):

| cockpit \ LSO | fast | slightly fast | on speed | slightly slow | slow |
|---|---|---|---|---|---|
| fast (1,209) | 97% | 2% | 1% | 0% | 0% |
| slightly fast (135) | 36% | 45% | 18% | 1% | 0% |
| on speed (169) | 4% | 22% | 30% | 31% | 13% |
| slightly slow (104) | 1% | 1% | 28% | 33% | 38% |
| slow (707) | 3% | 0% | 0% | 3% | 93% |

**F-14B(U), old band** (exact 84%, within one step 98%) and **measured band** (exact 84%, within one step 99%):

| cockpit \ LSO (measured band) | fast | slightly fast | on speed | slightly slow | slow |
|---|---|---|---|---|---|
| fast (1,313) | 97% | 2% | 1% | 0% | 0% |
| slightly fast (237) | 24% | 32% | 44% | 0% | 0% |
| on speed (559) | 2% | 14% | 65% | 17% | 1% |
| slightly slow (82) | 0% | 5% | 26% | 20% | 50% |
| slow (788) | 0% | 0% | 1% | 1% | 98% |

Reading it:

- With the old T-45 band the program called a cockpit donut "slow" 96% of the time. With the measured band the two ends (fast, slow) agree at 93 to 98% on both types.
- The middle three states spread over their neighbours, more on the T-45 whose donut is 0.5 degrees wide against a measurement spread of about 0.8 degrees (p10 to p90). This is the resolution limit of a 20 Hz server-side computation, not a band error: the confusion is symmetric.
- What matters for grading is whether that spread can produce a *medium* episode (fast or slow, the ratings that carry severity) while the pilot is actually in the donut. Under the measured bands: T-45, 20 such runs in 408 donut samples, median 0.33 s, longest 0.70 s; F-14, 21 runs in 878 samples, median 0.25 s, longest 0.85 s. None reached one second. **The one-second persistence rule of the prototype (P2) is therefore the right size**, and smoothing adds nothing (0.5 s smoothing moves exact agreement by 0 to 3 points).

### 6. What was flown

| Pass (UTC) | Pilot | Type | Outcome | DCS LSO | True AoA median (p10 to p90) | Cockpit indexer, share of groove |
|---|---|---|---|---|---|---|
| 18:33 | Ghost-72 | T-45 | T&G | none | 9.33 (8.49 to 9.90) | slow 85%, slightly slow 9%, donut 4% |
| 18:37 | Ghost-72 | T-45 | Bolter | none | 9.44 (7.97 to 10.82) | slow 69%, slightly slow 12%, donut 7%, fast 8% |
| 18:39 | Ghost-72 | T-45 | T&G | none | 5.54 (3.90 to 8.37) | fast 86%, donut 9% |
| 18:41 | Ghost-72 | T-45 | Trap, wire 2 | C: LULIM, LOIC, DRIC, LOAR, LULX, FX, 3PTSIW | 5.43 (4.61 to 6.15) | fast 100% |
| 18:49 | Ghost-72 | T-45 | Bolter | none | 7.84 (-0.69 to 8.48) | fast 57%, slightly fast 20%, donut 16% |
| 18:52 | Ghost-72 | T-45 | Trap, wire 2 | C: LULIM, LULIC, TMRDAR | 7.59 (2.07 to 8.16) | fast 86%, slightly fast 9%, donut 5% |
| 19:00 | Ghost-72 | F-14 | T&G | none | 7.82 (0.90 to 9.85) | fast 80%, slightly fast 8%, donut 12% |
| 19:04 | Ghost-72 | F-14 | Trap, wire 4 | C: LULIM, TMRDIC, LOIC, LULX, LULIC, LNFIW | 3.69 (2.21 to 5.07) | fast 100% |
| 19:21 | Justice | F-14 | T&G | none | 5.91 (4.38 to 7.13) | fast 100% |
| 19:24 | Ghost-72 | F-14 | T&G | none | 7.06 (0.09 to 10.72) | fast 72%, donut 7%, slow 10% |
| 19:25 | Justice | F-14 | T&G | none | 7.53 (3.34 to 8.62) | fast 100% |
| 19:26 | Ghost-72 | F-14 | Approach only | none | 8.71 (6.02 to 9.88) | fast 78%, slightly fast 11%, donut 7% |
| 19:27 | Justice | F-14 | T&G | none | 6.21 (2.42 to 7.18) | fast 100% |
| 19:30 | Justice | F-14 | T&G | WO: TMRDAR, SLOX, LOIC, WO(AFU)IC | 13.16 (12.46 to 14.59) | slow 100% |
| 19:33 | Justice | F-14 | T&G | none | 13.07 (11.70 to 14.29) | slow 100% |
| 19:34 | Ghost-72 | F-14 | T&G | none | 10.14 (9.02 to 10.77) | donut 53%, slightly fast 26%, fast 12%, slightly slow 9% |
| 19:35 | Justice | F-14 | T&G | none | 12.56 (11.95 to 14.07) | slow 100% |
| 19:38 | Ghost-72 | F-14 | Trap, wire 1 | C: LULIM, LOIC, LOAR | 9.41 (1.42 to 10.33) | fast 41%, slightly fast 23%, donut 36% |
| 19:38 | Justice | F-14 | T&G | none | 10.41 (4.60 to 11.19) | donut 70%, slightly slow 18%, slightly fast 6%, slow 6% |
| 19:41 | Justice | F-14 | Trap, wire 2 | C: SLOX, TMRDAR, DRX, DRIM | 10.28 (7.79 to 11.28) | donut 62%, slightly fast 14%, fast 7%, slightly slow 7%, slow 10% |
| 20:01 | Ducks 1-1 | F/A-18C | Wave-off | none | LSO computed 8.40 (7.87 to 8.85), no client log | n/a |
| 20:04 | Ducks 1-1 | F/A-18C | Trap, wire 3 | --- | LSO computed 8.47 (7.95 to 8.95), no client log | n/a |

- Nine of the fourteen F-14 passes and four of the six T-45 passes were flown with a chevron lit for 70% or more of the groove. The brief asked for chevron holding on two passes per type; the pilots did more. This set is a calibration set, not a sample of normal flying.
- DCS's "SLO X" on 19:41 contradicts the cockpit (donut 62%, slow 10%, and the 10% is the last 2.75 s before the wires, where the AoA rose to 12.8). On 19:30 "SLO X" agrees (slow 100%). DCS's LSO also called 18:41 "FX" (fast all the way), which agrees with fast 100%.
- The 13 September F-14 passes, flown before any calibration brief, sit at the same place: with the measured band the re-grader finds 11.7 to 13.4 s episodes at 5.95 to 5.99 degrees on all three (section 7). The "3 to 4 degrees fast" of finding F1 was the pilot.

### 7. Grading impact

`lso grade-ab` on the 22 reports. "P0" is the current production grader, "P2" is P0 plus the two prototype rules that are independent of calibration (touchdown ends the correction assessment; AoA needs 1 s to grade), "P3 old" is the candidate proposed on 13 September (P2 plus: AoA does not grade the T-45 and F-14). The last column is the prototype proposed now. Each cell gives the grade and the episode that decided it (axis, zone, effective severity).

| Pass (UTC) | Type | Outcome | Recorded | P0, old bands | P2, old bands | P3 old (AoA off for T-45/F-14) | **P2, measured bands** |
|---|---|---|---|---|---|---|---|
| 18:33 | T-45 | T&G | `--` | `--` AoA middle 3.6 | `--` AoA middle 3.6 | `(OK)` LU middle 2.4 | `--` AoA middle 3.6 |
| 18:37 | T-45 | Bolter | `B` | `B` | `B` | `B` | `B` |
| 18:39 | T-45 | T&G | `C` | `C` sink rate | `C` | `C` | `C` |
| 18:41 | T-45 | Trap 2 | `--` | `--` AoA ramp 4.0 | `--` AoA middle 3.6 | `--` LU start 3.0 | `--` LU start 3.0 |
| 18:49 | T-45 | Bolter | `B` | `B` | `B` | `B` | `B` |
| 18:52 | T-45 | Trap 2 | `(OK)` | `(OK)` AoA middle 2.4 | `(OK)` AoA middle 2.4 | `OK` LU middle 1.2 | `--` AoA in close 3.0 |
| 19:00 | F-14 | T&G | `--` | `--` AoA ramp 6.0 | `--` AoA ramp 4.0 | `(OK)` LU middle 2.4 | `--` AoA ramp 4.0 |
| 19:04 | F-14 | Trap 4 | `--` | `--` GS ramp 6.0 | `--` GS ramp 4.0 | `--` GS ramp 4.0 | `--` GS ramp 4.0 |
| 19:21 | F-14 | T&G | `--` | `--` GS ramp 6.0 | `--` GS ramp 4.0 | `--` GS ramp 4.0 | `--` GS ramp 4.0 |
| 19:24 | F-14 | T&G | `--` | `--` AoA ramp 6.0 | `--` AoA ramp 4.0 | `OK` LU middle 1.2 | `--` AoA ramp 4.0 |
| 19:25 | F-14 | T&G | `--` | `--` AoA middle 3.6 | `--` AoA middle 3.6 | `(OK)` LU start 2.0 | `--` AoA middle 3.6 |
| 19:26 | F-14 | Approach only | `C` | `C` GS low | `C` | `C` | `C` |
| 19:27 | F-14 | T&G | `--` | `--` AoA middle 3.6 | `--` AoA middle 3.6 | `OK` LU start 1.0 | `--` AoA in close 3.0 |
| 19:30 | F-14 | T&G | `--` | `--` AoA ramp 6.0 | `--` GS ramp 4.0 | `--` GS ramp 4.0 | `--` GS ramp 4.0 |
| 19:33 | F-14 | T&G | `--` | `--` GS ramp 6.0 | `--` GS ramp 4.0 | `--` GS ramp 4.0 | `--` GS ramp 4.0 |
| 19:34 | F-14 | T&G | `--` | `--` LU start 3.0 | `--` LU start 3.0 | `--` LU start 3.0 | `--` LU start 3.0 |
| 19:35 | F-14 | T&G | `--` | `--` AoA ramp 6.0 | `--` AoA ramp 4.0 | `(OK)` GS ramp 2.0 | `--` AoA ramp 4.0 |
| 19:38 | F-14 | Trap 1 | `--` | `--` AoA ramp 6.0 | `(OK)` GS ramp 2.0 | `(OK)` GS ramp 2.0 | `(OK)` GS ramp 2.0 |
| 19:38 | F-14 | T&G | `--` | `--` AoA ramp 6.0 | `--` AoA ramp 4.0 | `OK` GS ramp 0.0 | `--` AoA ramp 4.0 |
| 19:41 | F-14 | Trap 2 | `--` | `--` AoA ramp 6.0 | `--` AoA ramp 4.0 | `OK` LU start 1.0 | `--` AoA ramp 4.0 |
| 20:01 | F/A-18C | Wave-off | `WO?` | `WO?` | `WO?` | `WO?` | `WO?` |
| 20:04 | F/A-18C | Trap 3 | `(OK)` | `(OK)` GS start 2.0 | `(OK)` | `(OK)` | `(OK)` |

The 8 passes of 13 September (`trap_records/recovery_13-09/`), same columns:

| Pass (UTC) | Type | Outcome | Recorded | P2, old bands | P3 old | **P2, measured bands** | Deciding AoA episode under the measured band |
|---|---|---|---|---|---|---|---|
| 17:58 | T-45 | Trap 1 | `--` | `--` AoA ramp 4.0 | `(OK)` | `--` AoA ramp 4.0 | slow to 9.74 for the last 2.35 s; fast 9.2 s in the middle (3.6) |
| 18:08 | T-45 | T&G | `--` | `--` AoA ramp 4.0 | `(OK)` | `--` AoA middle 3.6 | fast (7.26) for 14.0 s in the middle |
| 18:12 | T-45 | Trap 1 | `--` | `--` AoA in close 4.5 | `(OK)` | `--` AoA in close 4.5 | fast 8.4 s at the start, then slow to 9.70 for 7.1 s in close (DCS: SLO X) |
| 18:20 | T-45 | T&G | `--` | `--` AoA middle 3.6 | `(OK)` | `--` AoA in close 4.5 | fast (7.26) for 9.4 s in close |
| 18:26 | T-45 | Trap 2 | `--` | `--` AoA in close 4.5 | `(OK)` | `(OK)` AoA middle 2.4 | fast (6.25) for 11.75 s in the middle, corrected late |
| 18:37 | F-14 | T&G | `--` | `--` GS ramp 4.0 | `--` | `--` GS ramp 4.0 | fast (5.97) for 12 s in the middle (3.6), outranked by glideslope |
| 18:42 | F-14 | Trap 2 | `--` | `--` AoA middle 3.6 | `OK` | `--` AoA middle 3.6 | fast (5.99) for 11.7 s in the middle |
| 18:53 | F-14 | Trap, no DCS | `--` | `--` AoA in close 4.5 | `OK` | `--` AoA in close 3.0 | fast (5.95) for 13.4 s in close |

Reading the two tables:

- **Measured bands versus old bands change one grade on 14 September** (18:52, `(OK)` to `--`) and one on 13 September (18:26, `--` to `(OK)`). On the other passes the deciding episode is either not AoA, or an AoA excursion so large that both bands agree (6 degrees on a Tomcat is fast under any band).
- **The old P3 candidate would have been wrong.** It turns nine `--` into `OK` or `(OK)` on 14 September and seven on 13 September. On six of the nine (18:33, 19:00, 19:24, 19:25, 19:27, 19:35) the cockpit showed a chevron alone for 72 to 100% of the groove; the other three (19:38 Ghost-72, 19:38 Justice, 19:41) were largely on speed and are `--` only on a ramp excursion, which is the ramp-rule question below. It was a reasonable guard while the bands were in doubt; the doubt is gone.
- **What now decides the `--`s is policy, not measurement.** Under the proposed column, 14 of the 20 non-Hornet passes on 14 September are `--`: 8 on AoA, 4 on a medium glideslope at the ramp, 2 on lineup at the start. Every AoA episode behind the 8 is 2.75 s to 22 s long and peaks 1.8 to 4 degrees outside the donut, confirmed by the cockpit. Whether "fast for the whole middle" is a `--` (today: medium 2.0 times the zone weight, poor or average correction) or a `(OK)` with a comment is the human-LSO question. So is the ramp rule: a medium excursion in the last 2 to 3 s before the wires scores 4.0 even when nothing could be corrected (19:38 Justice, 19:41 Justice: AoA rising to 12.7 in the last 2.8 s of otherwise on-speed passes, DCS "SLO X" on one of them). P1 removed the extra level; the ramp weight still makes it a `--` on its own.
- **With the same bands, P2 never lowers a grade below P0** on either set, and the one-second rule removed only sub-second blips (18:20 on the 13th, 19:38 and 19:41 on the 14th), as designed. The two grades that do move between the last two columns (18:52 down, 18:26 up) are the band change, not the policy.

### 8. The F/A-18C

Not flown with the client recorder. The band in the code (6.9 / 7.4 / 8.8 / 9.3, from an external document) is unchanged and the type keeps `aoa_grading_calibrated: true` as before. Ducks 1-1's two passes give a first look: LSO computed AoA median 8.40 and 8.47 in the groove, 78 to 86% rated on speed and 14 to 19% slightly slow, with three short small "slow" episodes at 8.98 to 9.28 that do not grade. A pilot flying the Hornet's donut lands in the band's upper half, which is plausible for a documented centre of 8.1 but says nothing about the edges. The client recorder already carries the Hornet lamp arguments (4/5/6); one session by a Hornet pilot closes this.

### 9. Acquisition: two pilots saturate the read budget

| Pass (UTC) | Pilot | Pilots in pattern | Health | Poll latency p50 / p95 / max (ms) | Delivery age p95 (ms) | Read-budget wait, whole pass | Late samples |
|---|---|---|---|---|---|---|---|
| 18:33 to 19:04 (8 passes) | Ghost-72 | 1 | green | 16 to 18 / 40 to 44 / 73 to 138 | 90 to 120 | 1.6 to 2.6 ms | 0% |
| 19:21 | Justice | 2 | orange | 22 / 206 / 360 | 210 | 52.0 s | 1% |
| 19:24 | Ghost-72 | 2 | orange | 62 / 279 / 437 | 280 | 92.5 s | 4% |
| 19:25 | Justice | 2 | orange | 62 / 267 / 425 | 280 | 92.7 s | 3% |
| 19:26 | Ghost-72 | 2 | **red** (9 invalid samples, 2 invalid snapshots in one batch) | 64 / 261 / 386 | 250 | 87.5 s | 2% |
| 19:27 to 19:38 (7 passes) | both | 2 | orange | 19 to 64 / 114 to 271 / 318 to 464 | 150 to 260 | 20.0 to 112.7 s | 0 to 3% |
| 19:41 | Justice | 1 | green | 18 / 49 / 101 | 110 | 2.3 ms | 0% |
| 20:01, 20:04 | Ducks 1-1 | 1 | green | 17 / 41 to 43 / 80 to 105 | 100 | 1.9 to 2.0 ms | 0% |

The shared read budget (`--buffered-read-budget-per-second`, default 16, that is 80% of the server's default `readsPerSecond` of 20) is enough for one recorder at 20 Hz and not for two: with two pilots in the pattern the recorders spent 20 to 113 s per pass waiting for a read token, poll latency p95 went from 41 to 260 ms, every pass was flagged orange (`late_delivery_warning`) and one red. Health returned to green the moment one pilot was alone again. This is the "multi-pilot case" left open by section 4 of the live review, and the answer is that it does not hold at the default quota. The sample rate itself stayed at 20 Hz and no snapshot was lost, so the grades are not affected, but the confidence flag is. Options: raise `readsPerSecond` on the server and the budget with it, or read larger batches less often per recorder. The `lso.log` of the day carries no error, 136 warnings: the routine duplicate-touchdown rejections, the telemetry-degraded warnings of that window, one "invalid unit observations" batch, and the reconnections around the mission restarts of 01:45, 10:05 and 18:06 UTC.

### 10. What changed on the branch

| File | Change |
|---|---|
| `src/data.rs` | T-45C band 8.0 / 8.25 / 8.75 / 9.0, F-14 band 9.45 / 9.95 / 10.8 / 11.25, both flagged `aoa_grading_calibrated: true`, comments rewritten with the measurement and its source |
| `src/grading.rs` | **production default switched from `BASELINE` to `PROTOTYPE`**: `impl Default for CatobarGradingPolicy` returns `PROTOTYPE` and the three production wrappers (`compute_pass_grade_with_reason`, `compute_catobar_assessment`, `grade_from_gates_with_reason`) call `default()`, so that `impl` is the single place to flip it back. One AoA test fixture updated for the new bands; eight tests that encoded the baseline's "series ended inside the excursion, add a level" upgrade now assert the prototype grade through the production wrappers and keep the baseline value through the explicit-policy variant |
| `src/track.rs` | one end-to-end test expectation updated for the same reason (`--` to `(OK)` on a medium excursion in the middle zone) |
| `tools/aoa_calibration/align_aoa.py` | pilot matching, per-pass flown-AoA and indexer table, lamp transition thresholds with hysteresis, bank slope, agreement tables, band rating comparison; `CODE_BANDS` now holds the measured values |
| `tools/aoa_calibration/README.md` | describes the new outputs and the pilot-name requirement |
| `docs/AOA_CALIBRATION_2026-09-14.md` | raw tool output for this flight |
| `docs/GRADING_POLICY_PROTOTYPE_2026-09-13.md` | update note pointing here, finding F1 withdrawn |

| Check | Result |
|---|---|
| `cargo test --locked` | 343 + 3 passed, 1 ignored (the fixture table test, unchanged: ACMI replays carry no wind reference, so AoA never grades them) |
| `cargo clippy --locked --all-targets -D warnings` | clean |
| `cargo fmt --check`, `git diff --check` | clean |
| `grade-ab` P0 column against the 22 recorded grades, old bands | 22 of 22 identical |

Nothing is committed. The recovery folder and the two CSV files are untracked and large (the log alone is 489 MB); they belong next to `trap_records/`, which is ignored.

### 11. Recommendation

1. **Test the prototype as it now stands**: `CatobarGradingPolicy::PROTOTYPE` with the measured bands. That is P1 (touchdown ends the correction assessment), P2 (one second of AoA persistence, now shown to match the measurement noise exactly) and P3 (AoA grades calibrated types only, which after this flight means all three carrier types). **This branch now builds with `PROTOTYPE` as the production default** (section 10); a build from it runs the candidate live. `lso grade-ab` still shows the baseline as its P0 column for comparison.
   All four are in grading.rs: from BASELINE to default
    grading.rs:560 in compute_pass_grade_with_reason
    grading.rs:680 in compute_catobar_assessment (the line you selected is just under it)
    grading.rs:1314 in grade_from_gates_with_reason
    grading.rs:287 in impl Default for CatobarGradingPolicy
2. **Fly one ordinary session** (donut held as usual, no chevron holding) and run `grade-ab` on it. That gives the grade distribution real flying produces under the calibrated bands.
3. **Have a human LSO grade** that session, the 14 September traps and the 13 September set blind. The two open policy questions are then decided against a reference: the cost of a sustained fast or slow, and the fixed 4.0 for a medium excursion at the ramp.
4. **Calibrate the F/A-18C** the same way, one session.
5. **Raise the read quota** before the next multi-pilot session, or the confidence flags of every pass flown with company will read orange.(done: `--buffered-read-budget-per-second` 40, `readsPerSecond` 50 on the server).
6. **Keep the client recorder installed** on the two pilots' PCs. It costs nothing, and the next windy session will give the wind correction the test it has still not had.
