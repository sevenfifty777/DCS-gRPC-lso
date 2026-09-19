# Recovery of 18 September 2026: one pilot, four passes, first flight of the final CONVENTION build

One pilot (Justice, F-14B(U)), four recordings on CVN-72 between 20:30 and 20:43 UTC, recorded by LSO 0.5.0 at `3c4c6b4` (clean: the AoA-axis fix, the three rules of the 15 September evening and the stop-position wire estimate are all in the build that flew). DCS-gRPC 0.10.0, buffered mode at 20 Hz, read budget 40 per second. No client recorder this time, so there is no flight-model AoA and no cockpit indexer to check the LSO's numbers against; everything below comes from the server's own reports.

Source folder: `tools/aoa_calibration/recovery_COVENTION-18-09/` (4 JSON reports, charts, ACMI, `lso.db`, `lso.log`). Re-grading tool: `lso grade-ab --episodes` on the folder, built from the same commit. Previous documents: `docs/RECOVERY_REVIEW_2026-09-15.md` (the fix and the three rules), `docs/GRADING_CONVENTION_PROTOTYPE_2026-09-15.md` (the policy table). Times in this document are UTC; the file names are local time, two hours later.

Part 1 is written for everyone. Part 2 has the numbers.

---

## Part 1. In plain language

### What was flown

A short solo session: an overhead break that turned into an abandoned approach, a go-around flown with the hook up, a hook-up touch-and-go, and one trap. The pilot flew ordinary passes rather than holding the chevrons, which is what the 15 September review asked for. The mission had almost no wind again (2 to 2.5 knots at deck level), the hook was up on two of the three real passes, and the client recorder was not running: none of the three things recommendation 5 of the last review asked for.

### What we found

**1. The build did what the last review said it would.** All four recorded grades are reproduced by the re-grader, and the two passes that a grader could argue about both come out `(OK)` for the reasons the 15 September rules were written:

- **20:38, touch-and-go, hook up.** Fast at the start (fast chevron alone, about 2.7 degrees under the band, roughly 10 to 15 knots over approach speed) for the first three seconds, then corrected and held on speed for ten seconds; slightly high in close, coming down to the glideslope at the ramp; slow chevron for the last second as the nose came up over the round-down. Under the policy of 15 September morning this was a `--`, because the very last AoA sample (12.87 degrees) sits 0.07 degrees past the gross line and one gross sample used to make a whole episode gross. The one-second rule of the evening demotes that single sample to moderate, and a moderate slow at the ramp with an average correction is `(OK)` by the convention table. The gross fast at the start stays gross (2.5 seconds, so the rule does not touch it), but it was corrected inside the start zone, which the table also gives `(OK)`.
- **20:41, trap, 3-wire.** On speed at roll-out, a two-second fast excursion in close corrected well, a little high at the ramp (about 2 metres at 100 metres out) and back on the glideslope by the wires, slow for the last second (about 1.7 degrees over the band), sink rate 5.3 m/s, wheels between the 2 and the 3 wire. `(OK)` under every policy since P2 of 13 September; the two oldest steps gave it `--` on the 0.85-second slow at the ramp, which is shorter than the one-second persistence rule.

Every prototype agrees on the other two recordings (`WO?` for the go-around and `--` for the 20:30 recording), so the interesting part of this session is not the policy but the two recordings that were not passes at all.

**2. The 20:30 recording was graded `--` and given 2 points for something that was never an approach.** The pilot ran up the wake at 590 knots and 120 metres, broke overhead the ship at 79 degrees of bank, flew a wide downwind, turned in slow (the computed AoA reaches 14 to 17 degrees in the turn, which is the slow chevron and then some) and low (55 to 90 metres), overshot the centreline from 18 degrees left to 17 degrees right while still in 11 to 29 degrees of bank, and flew past the ship 270 metres to the right at 106 metres before climbing away. The aircraft never rolled out, never lined up, and the hook state is unknown. The grader has no groove for such a recording, so it fell back to the three gate readings (the values it happened to capture as the aircraft crossed the 3/4, 1/2 and 1/4 nautical mile rings in the middle of a turn: 0.9 low, then 2.0 high, then 8.2 high and 17 degrees right) and wrote `--: glideslope 8.2° high`, with 2 points into the pilot's score. A human LSO would write a pattern wave-off or nothing. This is the one finding of the session that needs a code change: an "approach only, outcome unknown" recording with no groove entry should not be graded from the gates and should not carry points (Part 2, section 4).

**3. The go-around at 20:35 was a fly-through, not a wave-off decision.** Rolled out 160 metres right of the centreline (10 degrees of lineup), hook up, closing at 140 to 150 knots against the deck (about 155 to 165 knots over the ground), AoA between 1 and 6.5 degrees, level at 45 metres from 300 metres out and a right turn over the deck edge. Grade `WO?` with no points, under every policy. Nothing to change; the episodes the grader lists for it (lineup 36 degrees, glideslope 31 degrees at the ramp) are the geometry blowing up as the aircraft passes 100 metres abeam the landing point at 45 metres, and they never reach the grade.

**4. DCS issued no landing mark for the trap, for the second Tomcat trap in a row.** Like 19:17 on 15 September, the 20:41 trap has a `runway_touch` event, a `land` event two seconds later that the program rejects as a duplicate, and no LSO comment from DCS. The stop-position estimate of the 15 September evening did its job: the aircraft stopped 96.7 metres past the landing point, which is 87 metres past the 3-wire plane, and the deceleration began 54 metres past the same wire; both put it on the **3-wire** to within a metre, so the outcome line reads "Wire #3 (Rust estimate)". Twelve traps with a DCS mark and three without have now been recorded, and nothing in the event stream the program receives distinguishes the two groups. The DCS server log does: 1.9 seconds after the 19:17 touchdown of 15 September it carries `LSO: GRADE: NC : No proper communications`, written as a plain comment event with no aircraft attached instead of a landing quality mark, which is why the program never saw it. The Supercarrier LSO only grades an aircraft the ship's ATC is tracking, which needs the radio check-in from the comms menu; the pilot confirms he flew the four passes of 18 September without it. So the missing marks are not a fault anywhere: DCS graded those traps "no comms", and the stop-position estimate is what names the wire on them (Part 2, section 6).

**5. Two small measurement notes.** The DCS touchdown event arrived 1.4 seconds after the wheels were on the deck on the trap (0.6 to 0.9 seconds on 15 September); the physical-touchdown rule handles it, and the last 1.4 seconds of rollout, where the computed AoA collapses from 12.9 to 2.4 degrees, are outside the graded series. And on the 20:38 pass the deck-level wind probe returned the 180/0.0 sentinel again and the 6.3 m/s reading from 82 metres was used instead of the real 1.2 m/s; by the AoA formula that understates the computed AoA by at most 0.25 degrees, which moves no grade on this pass (the gross fast at the start is 2.7 degrees under the band, the slow at the ramp is one sample either way).

### What changes

| Item | Status |
|---|---|
| Grades of the four recordings | 4 of 4 reproduced by `grade-ab` under `CONVENTION` (P9) |
| AoA bands, thresholds, the three rules of 15 September | unchanged; nothing in this session argues against them |
| "Approach only" recordings with no groove entry | **finding, fixed on 19 September**: was graded `--` with 2 points from gate readings taken in a turn; now `WO(P)`, pattern wave-off, no points (section 4, section 11) |
| Deck contact after a DCS wave-off call | **rule made explicit on 19 September**: graded on the approach flown, the call only noted; a human LSO may overrule DCS (section 11) |
| DCS landing mark absent on a trap | **closed**: DCS writes `GRADE: NC : No proper communications` as a comment event when the pilot has not checked in with the ship's ATC; the program cannot receive that event (section 6) |
| DCS `WO` / `OWO` marks | **implemented on 19 September**: a mark naming the initiator turns `WO?` into `WO` (1.0 point, as the reference already listed) or `OWO` (no points); `GRADE:OWO` now latches the wave-off outcome like `GRADE:WO` (section 6, section 11) |
| Recommendation 5 of 15 September (recorder, hook down, wind) | still open: this session had none of the three |

### Recommendation

Fix the "approach only" grading path before the next session, then fly the ordinary session with the client recorder on, hook down, and 15 to 20 knots of deck wind. The policy itself needs no change from this session: the two real passes came out where the convention puts them, and both were decided by the rules added on the 15th in exactly the situations those rules were written for.

---

## Part 2. Technical

### 1. Data set

| Item | Value |
|---|---|
| Reports | 4 (`LSO-*.json`), all Justice, F-14B(U), CVN-72 |
| Outcomes | 1 "Approach only, outcome unknown" (no groove entry), 1 wave-off with the hook up, 1 touch-and-go with the hook up, 1 arrestment (kinematic, no DCS wire, 3-wire from the stop position) |
| DCS LSO comments | none on any of the four |
| Client CSV | none (recorder not running) |
| Mission wind | from 120 at 1.0 to 1.3 m/s at deck level, 5.9 to 6.3 m/s at 63 to 82 m; calm again |
| Wind probes | the deck probe returned the 180/0.0 sentinel on 20:38 and was overridden by the high probe (6.29 m/s); 20:30 has no wind reference at all (no groove entry), so its AoA is unreliable and never grades |
| Ship | 8.2 m/s (16 knots) on 146; the wind is 26 degrees off the bow, 90% headwind |
| Acquisition | 20.0 Hz on all four, 0 dropped samples, 0 invalid snapshots; read-budget waits 1.5 to 2.8 ms per pass; max delivery age 140 to 180 ms; health green on all four |
| Build | `lso_commit 3c4c6b4`, `lso_dirty false`, grading version `project-derived-v7`; every recorded grade equals the `grade-ab` P9 (`CONVENTION`) column |

Closure speeds in this document are measured in the deck frame (`x`, `y` of the datums move with the ship). The ship adds 8.2 m/s along the approach path, so ground speed is the closure plus about 16 knots, and true airspeed adds the 1.1 to 1.3 m/s of natural headwind on top. The report's `touchdown_horizontal_speed_mps` is a closure speed too; the 10 m/s gap between it and the client's IAS noted on 15 September (69 against 78.8 m/s on the 19:17 pass) is the ship's speed plus the wind.

### 2. The four recordings under every policy step

`lso grade-ab --episodes tools/aoa_calibration/recovery_COVENTION-18-09`. P0 is the baseline grader of `main`, P3 the `PROTOTYPE` of 14 September, P6 the `CONVENTION` as switched on the morning of 15 September, P9 the `CONVENTION` as it stands (P7 gross AoA needs one second of gross readings, P8 series end at the physical touchdown, P9 height instead of angle inside 100 m). The cell shows the grade and the axis, zone and effective severity of the deciding episode.

| UTC | Outcome | Recorded | P0 | P1 | P2 | P3 (PROTOTYPE) | P4 | P5 | P6 (CONVENTION 15/09 am) | P7 | P8 | P9 (CONVENTION) |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| 20:30 | Approach only, outcome unknown | `--` | `--` | `--` | `--` | `--` | `--` | `--` | `--` | `--` | `--` | `--` |
| 20:35 | Wave-off, hook up | `WO?` | `WO?` | `WO?` | `WO?` | `WO?` | `WO?` | `WO?` | `WO?` | `WO?` | `WO?` | `WO?` |
| 20:38 | T&G, hook up, would have caught wire 2 | `(OK)` | `--` AoA ramp 6.0 | `--` AoA ramp 4.0 | `--` AoA ramp 4.0 | `--` AoA ramp 4.0 | `--` AoA start 3.0 | `--` AoA ramp 3.3 | `--` AoA ramp 3.3 | **`(OK)`** AoA ramp 2.3 | `(OK)` AoA ramp 2.3 | `(OK)` AoA ramp 2.3 |
| 20:41 | Trap, wire 3 (stop position) | `(OK)` | `--` AoA ramp 6.0 | `--` AoA ramp 4.0 | **`(OK)`** GS ramp 2.0 | `(OK)` GS ramp 2.0 | `(OK)` GS ramp 2.3 | `(OK)` GS ramp 2.3 | `(OK)` GS ramp 2.3 | `(OK)` GS ramp 2.3 | `(OK)` GS ramp 2.3 | `(OK)` GS ramp 2.3 |

The 20:30 row grades from the gates under every step (no groove entry, no trajectory, no episodes: section 4). The 20:35 row is decided by the outcome. The two real passes:

**20:38.** The steps move it three times.

| Step | Grade | What decides it |
|---|---|---|
| P0 | `--` | slow (12.87) 3.1 s at the ramp, "poor": no improvement after the peak, because the series ends there |
| P1 to P3 | `--` | same episode, "average" once the end of the trajectory stops counting as a failed correction; medium at the ramp is 2 x 2.0 = 4.0 under the arithmetic model |
| P4 | `--` | the table makes the ramp episode 2.3 (`(OK)`); the deciding episode becomes the start: fast (7.24) 12.65 s, "poor" on 0.3-degree reversals, and moderate-poor is `--` in the table |
| P5 | `--` | the gross tier makes the start episode large (7.24 is 2.7 under the band) and the ramp episode large (12.87 is 0.07 over the gross line); large-average at the ramp is `--` |
| P6 | `--` | the 1-degree swing makes the start correction "good" (gross, corrected within the start zone: `(OK)`), but the ramp is still gross-average: `--` |
| **P7** | **`(OK)`** | the gross readings at the ramp span one sample (0.05 s), so the episode is demoted to moderate: moderate-average at the ramp is `(OK)` |
| P8, P9 | `(OK)` | no touchdown event, nothing to cut; the glideslope episode at the ramp goes from 1.72 degrees "average" to 1.49 degrees "good" in height, without changing the grade |

In the cockpit: fast chevron alone for the first three seconds after roll-out (about 2.7 degrees under the band, gauge near 12 units, roughly 10 to 15 knots over approach speed; closure 118 knots, about 134 over the ground), donut from 10 seconds in, a flicker of slightly-fast in close, then the slow chevron rising from 11.0 to 12.9 degrees over the last 1.1 seconds as the nose came up over the round-down at 4.4 m/s of sink. The wheels touched at 24.6 m past the landing point (beyond the 4-wire plane), hook up. The hypothetical wire is the 2-wire: the hook point, had it been down, crossed the 2-wire plane at 9222.08 with the aircraft 1.3 m above the deck.

**20:41.** Only the two oldest steps disagree.

| Step | Grade | What decides it |
|---|---|---|
| P0 | `--` | slow (12.46) 0.85 s at the ramp, "poor" (series ends): medium x 2.0 with a level added = 6.0 |
| P1 | `--` | same, "average": 4.0 |
| P2 onward | `(OK)` | the 0.85 s excursion is shorter than the one-second AoA persistence window and stops grading; what decides is 1.46 degrees high at 70 m (1.21 degrees at 100 m in height under P9, about 2 m), corrected to 0.2 degrees by the wires: moderate-good at the ramp, `(OK)` |

In the cockpit: on speed at roll-out (10.4 degrees; closure 112 knots, about 128 over the ground), fast chevron for 2.5 seconds in close (8.9 degrees, 1.1 under the band), back to the donut, a little high through the ramp, slow chevron for the last 1.5 seconds (11.2 rising to 12.5, closure 108 to 110 knots), sink 5.2 to 5.4 m/s (about 1,000 ft/min; on speed on a 3.5-degree glideslope at that speed is about 3.9 m/s, the cut threshold is 8.0). Wheels at x = -1 to -7 m, between the 2-wire (+3) and the 3-wire (-10) planes.

### 3. Groove statistics

From the raw datums between groove entry and the end of the graded series (touchdown event on the trap, last trajectory sample on the touch-and-go). F-14B(U) on-speed band 9.95 to 10.8 degrees, gross beyond 2.0 degrees outside it (below 7.95, above 12.8).

| UTC | Groove | Entry distance, lineup | AoA p10 / p50 / p90 | On speed | Fast (gross) | Slow (gross) | Gross runs |
|---|---|---|---|---|---|---|---|
| 20:35 | 12.5 s, no touchdown | 921 m, 9.95 right (162 m) | 1.8 / 4.8 / 6.1 | 0% | 100% (100%) | 0% | one run, the whole groove |
| 20:38 | 18.2 s, no touchdown | 1,090 m, 3.69 right (70 m) | 7.6 / 9.2 / 11.1 | 11% | 72% (20%) | 17% (0%) | 2.50 s and 1.25 s at the start (7.24, 7.49); 0.05 s at the last sample (12.87) |
| 20:41 | 13.4 s to the event | 690 m, 1.70 right (21 m) | 9.1 / 10.4 / 11.1 | 45% | 30% (4%) | 25% (0%) | 0.05 s at the physical touchdown (12.89); 0.50 s of rollout (2.36), both outside the P8 series |

Glideslope and lineup by zone, degrees, from `trajectory_deviations`:

| UTC | Start (> 1,000 m) | Middle (600 to 1,000) | In close (300 to 600) | Ramp (100 to 300) | Last 100 m |
|---|---|---|---|---|---|
| 20:38 GS | -0.37 to -0.06 | -0.05 to +0.66 | +0.61 to +0.77 | +0.78 to +1.47 | +0.74 to +1.72 (ends +0.74 at 4.7 m) |
| 20:38 LU | +3.4 to +3.7 | +1.9 to +3.4 | +1.4 to +1.9 | +1.2 to +1.5 | +0.6 to +1.2 |
| 20:41 GS | (entered at 690 m) | -0.11 to -0.10 | -0.14 to +0.40 | +0.40 to +1.19 | +0.20 to +1.46 (ends +0.20 at 4.4 m) |
| 20:41 LU | | +1.5 to +1.7 | +0.4 to +1.5 | -0.3 to +0.4 | -0.6 to -0.3 |

Both passes were flown a little high from in close to the ramp and a little right early on; both corrections completed before the wires. The 20:38 lineup at roll-out (3.7 degrees, 70 m right) is a gross lineup by the 2.5-degree threshold, corrected progressively over the whole groove, "good" because it ended inside the band: `(OK)` in the table's start column, and not the deciding episode.

### 4. The 20:30 recording: a `--` with 2 points for a pattern that never became an approach

The track, in the deck frame (x positive astern along the approach path, y positive to the right), from the datums:

| DCS time | Where | Altitude | Bank | Computed AoA | What it is |
|---|---|---|---|---|---|
| 8599 to 8617 | 6.1 km astern to 600 m, on the centreline, 300 m/s (590 knots) | 120 to 220 m | 0 to 10 | 0.4 to 0.7 | the run-in up the wake |
| 8617 to 8622 | over the ship, 230 m off | 250 to 300 m | 79 to 87 left | 4 to 7 | the break |
| 8625 to 8660 | downwind 3.2 km abeam, turning in | 290 down to 106 m | 66 to 32 left | 3 to 17 | the approach turn, slow (14 to 17 degrees is the slow chevron and beyond) and low |
| 8661 to 8674 | 1.8 to 2.5 km out, still turning | 55 to 73 m | 14 to 34 left | 6.6 to 15.8 | 55 m (180 ft) in the approach turn, against a 600 ft pattern |
| 8675.85 | 3/4 nm gate, 1,389 m | 60 m | 26 left | 17.2 | GS -0.93, LU -18.5 |
| 8682.71 | 1/2 nm gate, 926 m | 88 m | 17 left | 7.5 | GS +1.99, LU -5.6 |
| 8688.65 | 1/4 nm gate, 463 m | 95 m | 18 left | 6.9 | GS +8.20, LU +17.1: the glideslope at 463 m is 28 m above the deck |
| 8694 | abeam the landing point, 270 m to the right | 106 m | 18 left | 5.3 | flying past |
| 8696 to 8749 | outbound to 5.7 km, climbing | 106 to 313 m | 5 to 67 | 3 to 7 | departure |

The aircraft crossed the centreline from 18 degrees left to 17 degrees right in 13 seconds without ever reducing the bank below 11 degrees, so the Case I roll-out detector never fired: no groove entry, no trajectory series, no episodes, no wind reference (the reference is taken at groove entry, so the AoA column above is the raw geometric value and is marked unreliable in the report). The hook sampler has 0 samples in the groove and the state is "unknown". The recording ended as "Approach only, outcome unknown" because the aircraft did get below 100 m (53 m at 8664, 2.4 km out), which is the threshold that keeps a recording instead of discarding it as "never below 100 m MSL" (the log shows three such discards between 20:28 and 20:34, the other circuits of the same break).

What graded it: `compute_pass_grade_with_reason_and_policy` in `src/grading.rs`, arm `Grading::ApproachOnly`. It asks `gates.all_valid(groove_entry_time)`; with no groove entry that is the unconditional three-gates rule, and the three gates are present, valid and in chronological order (they were captured as the aircraft crossed the three distance rings, interpolated at 3/4, measured at 1/2 and 1/4). So it hands the pass to `grade_from_gates_with_reason_and_policy`, which finds 8.2 degrees high at 1/4 nm and writes `--`. The episode grader is not eligible (empty trajectory) and keeps that base grade. `grade_points` is 2.0 and `points_eligible` is true, so the pilot's average carries a `--` for a pattern that a human LSO would log as a pattern wave-off or not at all. The `grade-ab` tool shows the row as "(replayed)" because it re-derived the geometry from the datums and found no groove either.

The same path graded 19:26 of 14 September `C` (2.5 degrees low inside 1/4 nm), but that recording had a groove entry and a trajectory; the `C` came from the trajectory, not from the gates alone. Every other "approach only" in the three sessions had a groove.

**Proposed rule.** An `ApproachOnly` recording with no groove entry is not graded from the gates: `PassGrade::Incomplete` (or a new pattern wave-off outcome if the chart should say so), no points, reason "no roll-out on final: pattern wave-off, not graded". In the cockpit that is the pass where the pilot never got wings level on the centreline: the LSO never picked him up, so there is no grade to give. On the recorded sets it decides exactly this one recording (14 and 15 September have no `ApproachOnly` without a groove). Two lines in the `ApproachOnly` arm plus a test with valid gates and `groove_entry_time = None`. Not implemented here.

A weaker variant would keep the gate grade but withhold the points; it still shows the pilot a `--` for a pattern he abandoned on purpose, so the first form is the one to take.

### 5. The 20:35 go-around

Groove entry at 921 m with a lineup of 9.95 degrees (162 m right of the centreline): the entry criteria carry `max_lineup_deg 2.0` but `lineup_blocks_entry false`, so the roll-out alone (bank 2.1 degrees, track angle -13.7, sustained 0.75 s) triggers the entry. The aircraft then drove in 8 to 12 degrees right of the centreline, on the glideslope in the middle (-0.03 to +0.34), climbing from 300 m out (+0.4 to +4.3 in close, +4 to +21 at the ramp) to level at 45 m, and turned right through the deck edge at 100 m abeam. Hook up throughout. Computed AoA 0.9 to 6.5 degrees (closure 140 to 150 knots, about 156 to 166 over the ground). No touchdown, no wave-off call from anyone: `Grading::WaveoffUnknown`, `WO?`, no points, under every step. The three episodes in the report (lineup gross right 30 to 36 degrees at the ramp, glideslope gross high 22 to 31 degrees, AoA gross fast the whole groove with four reversals) are correct as measurements and never reach the grade.

Whether a roll-out 160 m right of the centreline should count as a groove entry is a separate question that this pass raises and does not decide: on a landed pass the same entry would produce a gross lineup episode at the start, and if corrected inside the band, the convention table gives `(OK)`. Left as is.

### 6. The trap: wire from the stop position, and the missing DCS mark

| Item | Value |
|---|---|
| Hook plane crossings | 1-wire 9360.07, 2-wire 9360.28, 3-wire 9360.50, 4-wire 9360.73 (hook down) |
| Physical touchdown | 9360.37 (altitude 0.1 m at x = -1.3 m, sink rate from 5.4 to 3.4 m/s over the last sample) |
| Deceleration onset | 9361.47 at x = -63.8 m: 54 m past the 3-wire plane (-10 m); the 14 and 15 September calibration gave 53 to 59 m |
| Stop | `x_at_slow_m` -96.7 m: 86.7 m past the 3-wire plane; the calibration gave 85 to 89 m, the F-14 run-out constant is 87 m |
| Estimate | `stop_position_run_out`, wire 3, medium confidence, residual 0.3 m |
| DCS `runway_touch` | 9361.80, 1.43 s after the wheels, 0.33 s after the deceleration onset; correlated and accepted |
| DCS `land` | 9363.80, rejected as a duplicate touchdown |
| DCS landing quality mark | none; `missing_dcs_lqm_reason landing_quality_mark_absent` |
| Kinematic confirmation | `deck_kinematics.confirmed true` (relative speed 0.0 held 2 s from 9364.77); the older detector says `post_contact_forward_departure_detected`, diagnostic only, as on 19:17 |

The `runway_touch` event is later than on 15 September (0.6 to 0.9 s then, 1.4 s here), late enough that it arrives after the arresting engine has started to pull. P8 ends the series at the sink-rate reversal, so the rollout (computed AoA collapsing from 12.9 to 2.4 as the nose comes down, 0.5 s of "gross fast") is outside the graded window; under the fix-only policy of the 15 September morning that rollout would have been another `--` on a touchdown collapse.

**The missing landing mark, closed.** Fifteen traps have now been recorded across the 12, 14, 15 and 18 September sessions. Twelve have a DCS mark, delivered 1.0 to 1.6 s after the `runway_touch`, before or after the `land` event. Three have none: 12 September 19:54 (Hornet, Ducks), 15 September 19:17 (F-14, Ghost-72), 18 September 20:41 (F-14, Justice). On all three the `land` event follows `runway_touch` by exactly 2.0 s, but so does it on five of the twelve marked traps, so the event stream the program receives does not separate the two groups.

The DCS server log of 15 September (`recovery_aoa_calibration2/dcs.log`, line 73839) does:

```
2026-09-15 19:20:22.831 INFO Scripting (Main): event:type=comment,comment=LSO: GRADE: NC : No proper communications,t=3154.668,linked_event_id=0,event_id=4594,
```

`t=3154.668` is 1.9 s after the 19:17 `runway_touch` (3152.77), the same delay as the marks on the graded traps. It is a DCS `comment` event with no initiator, not a `landing quality mark` event, so the DCS-gRPC bridge does not forward it: `lua/DCS-gRPC/methods/mission.lua` maps `S_EVENT_LANDING_QUALITY_MARK` and `S_EVENT_PLAYER_COMMENT` (a different event, with an initiator) and nothing else that carries a comment string, and the LSO's event handler in `src/tasks/record_recovery.rs` has arms for the landing mark, land, runway touch, crash, dead, leave-unit and unit-lost events only. `NC` is the Supercarrier LSO's grade for a pilot who did not check in with the ship's ATC on the radio menu; the pilot confirms the four passes of 18 September were flown without the check-in, and the 18 September `lso.log` shows no landing mark event at all that day. The DCS log of the 18th (`dcs-20260919-020030.log`, added to the folder on the 19th) shows the same for the 20:41 trap, `GRADE: NC : No proper communications` at `t=9363.894`, 2.1 s after the `runway_touch`, and no LSO line at all for the other three recordings.

Consequences: none for the grade (the program never used the DCS mark as a grading input). The stop-position estimate names the wire on all three (residuals 0.2 to 3.9 m). If the DCS comment is wanted in the report on such passes, the bridge would have to forward the plain `comment` event and the program match it by time (no initiator to match on), which is not worth doing for a string that says "no comms". A line in the pilot brief ("check in with the ship if you want the DCS grade next to ours") covers it.

**DCS wave-off grades, for reference.** The DCS logs of 14 and 15 September carry four wave-off grades, all received by the program as landing quality marks and all recorded as `WO?` (initiator unknown) with no points:

| UTC | Pilot | DCS grade | Program outcome |
|---|---|---|---|
| 14 Sept 19:30 | Justice | `WO _TMRDAR_ (NX) _SLOX_ _LOIC_ _PIC_ _PPPIC_ WO(AFU)IC` | T&G (a `runway_touch` 0.5 s after the mark: the aircraft touched the deck on the go-around); recorded `--` |
| 15 Sept 18:47 | Ghost-72 | `WO _LULIM_ _LULIC_ _TMRDIC_ _LULX_ WO(AFU)IC` | wave-off, `WO?` |
| 15 Sept 18:59 | Justice | `OWO : _LULIM_ LOIM LOIC _DRIC_ (LURIC) WO(AFU)IC` | wave-off, `WO?` |
| 15 Sept 19:31 | Ghost-72 | `WO LULX _FX_ _LOIC_ _PIC_ _PPPIC_ _LULIM_ _TMRDIC_ WO(AFU)IC` | wave-off, `WO?` |

`WO` is a wave-off called by the LSO, `OWO` an own wave-off by the pilot; the program's `WO?` deliberately claims neither. Two details: `dcs_grade_is_waveoff` in `src/track.rs` tests for the substring `GRADE:WO`, which `GRADE:OWO` does not contain, so an own wave-off mark does not latch the wave-off outcome the way an LSO wave-off does (on 18:59 the geometry had already decided the outcome, so nothing was lost); and when a DCS mark is present the report could say `WO` or `OWO` instead of `WO?`, since DCS has named the initiator. Neither affects points. On 18 September there were no DCS wave-off grades because there was no check-in.

### 7. The wind substitution on 20:38

How the wind enters the AoA. At groove entry the program queries DCS's wind twice, at the aircraft's own altitude (the "high" probe, 40 to 80 m on these passes) and at the carrier's deck altitude (the "low" probe), and from then on corrects every sample with the wind interpolated linearly between the two by the aircraft's altitude (`WindReference::at_altitude`, `src/track.rs`). `corrected_aoa_deg` subtracts that wind vector from the ground velocity and keeps the vertical component in the body frame, so an overstated headwind lengthens the air-relative horizontal speed, flattens the air-relative flight path angle and lowers the computed AoA (test `headwind_correction_lowers_the_computed_aoa_for_the_same_sink_rate`).

On this pass the deck probe returned the 180/0.0 sentinel after its retry and the high probe's vector was reused for the low point: 6.29 m/s from 120 at 82 m at both ends of the interpolation, against 1.14 to 1.42 m/s actually measured at deck level on the passes before and after. The error is therefore zero at 82 m and grows toward the deck: about 2.5 m/s of extra headwind mid-groove, 4.5 m/s at the ramp (90% of the difference, the wind being 26 degrees off the bow). At a closure of 59 to 61 m/s plus 8.2 of ship speed and a sink of 3.7 to 4.4 m/s, that flattens the air-relative flight path angle by 0.12 degrees in the middle and 0.24 at the ramp. The AoA series of the pass reads that much lower than it should: the gross fast at the start is nearer 7.4 than 7.24 (still gross, still corrected), the last sample nearer 13.1 than 12.87 (still one sample, still demoted by P7), and the on-speed share is 11% instead of a few points more. No grade moves. The sentinel itself has now hit 12 of the 50 recorded passes (8 of 35 Tomcat, 4 of 12 T-45; the substituted reading was 4.5 to 7.0 m/s each time against 0.7 to 1.4 at deck level).

**What the sentinel is, measured live on 19 September.** The mission was queried from the server's scripting environment (`atmosphere.getWind` at the carrier's position, at 26 heights) while CVN-72 steamed at 30 knots on 315 with the mission's natural wind from 119.5 degrees:

| Height above the sea | Wind | | Height | Wind |
|---|---|---|---|---|
| 0.00 m (ship at +0.025 m) | 2.3 kts | | 30 m | 9.8 kts |
| 0.03 m | 2.7 kts | | 40 m | 10.4 kts |
| 0.06 m | 3.0 kts | | 50 m | 11.0 kts |
| 1 m | 4.9 kts | | 60 m | 11.4 kts |
| 5 m | 6.7 kts | | 80 m | 12.2 kts |
| 10 m | 7.8 kts | | 100 m | 12.8 kts |
| 20 m | 9.0 kts | | 500 m | 16.6 kts |

Direction 119.5 degrees at every height to 500 m. Two things follow:

1. **The sentinel is the sea surface.** A first query made while the ship's altitude read -0.003 m returned 180/0.0 at height 0; the same query at +0.025 m returned 2.3 knots. DCS returns a zero wind vector at or below the water surface, and the carrier's reported altitude wobbles around zero. Every one of the 12 passes that hit the sentinel has a low-probe altitude of 0.0 or -0.0; every pass that did not has 0.03 to 0.06 m. It is not intermittent and a retry cannot fix it: the low probe is being taken at the waterline.
2. **The probes are right, the interpolation is not.** The LSO's low probe on the 18th read 2.4 to 2.8 knots at 0.03 to 0.06 m and the high probe 11.5 to 12.2 knots at 63 to 82 m: exactly this profile. But the correction interpolates linearly between those two points, and the profile is logarithmic. At 20 m above the sea, which is the height of the flight deck and therefore of the aircraft at the ramp, the interpolation gives about 5 knots where DCS has 9; at 40 m, 7.4 against 10.4. On this mission that understates the headwind at the ramp by about 2 m/s and reads the AoA there about 0.1 degrees high; with the 15 to 20 knots aloft of the planned windy session the same shape gives about 3.5 m/s and 0.2 degrees at the ramp. In the cockpit that is a fifth of an indexer unit, on the side that makes the grader call "slow" a little early.

Both points have the same fix: take the low probe at flight-deck height (carrier altitude plus about 20 m, where the ramp and the wires are) instead of at the waterline. That is above the sentinel, it is the lowest height the aircraft reaches before the wheels touch, and between deck height and groove-entry height the profile is close enough to linear (9.0 to 12.2 knots over 60 m) for the existing two-point interpolation. Adding a third probe mid-groove would refine it further but is not needed.

### 8. Distribution under the final policy, all sessions

`grade-ab` from the same binary on the three folders, P9 column:

| Set | Reports | `OK` | `(OK)` | `--` | `B` | `C` | `WO?` | P9 against the recorded grade |
|---|---|---|---|---|---|---|---|---|
| 14 September | 22 | 0 | 5 | 12 | 2 | 2 | 1 | 19 equal; 19:38 x2 and 19:41 higher (the three of section 13 of the 15 September review) |
| 15 September | 24 | 2 | 7 | 6 | 4 | 0 | 5 | 21 equal; 19:15 and 19:17 lower, 19:43 higher (as documented) |
| 18 September | 4 | 0 | 2 | 1 | 0 | 0 | 1 | 4 equal (the first session recorded by the final build) |
| Total | 50 | 2 | 14 | 19 | 6 | 2 | 7 | |

Of the 19 `--`, one is the 20:30 pattern of section 4. The rest are the gross AoA and lineup cases of the two previous reviews.

### 9. What was checked, what was not

| Check | Result |
|---|---|
| `cargo build --release` at `3c4c6b4` | clean, 32 s |
| `grade-ab` on the 4 reports | 4 of 4 equal to the recorded grade under P9 |
| `grade-ab` on the 14 and 15 September folders | reproduces section 13 of the 15 September review |
| `lso.log` between 20:28 and 20:45 | three "never below 100 m" discards (20:28 x2, 20:33), the 20:41 land event and its duplicate; no warnings, no read-budget waits above 3 ms |
| Client CSV, `align_aoa.py` | not run: no recorder file in the folder |
| Code | nothing changed; the working tree carries only the user's `.gitignore` line (`recovery_*/`) |

### 10. Recommendations

**1. Do not grade an "approach only" recording that has no groove entry.** What it is: section 4; the grader graded a break-and-abandon pattern `--` from three gate readings taken at 11 to 29 degrees of bank and put 2 points into the pilot's score. In the cockpit: the pilot never rolled wings level on the centreline, the LSO never picked him up. Done: **implemented on 19 September** (section 11): the `ApproachOnly` arm of `compute_pass_grade_with_reason_and_policy` returns the new grade `WO(P)` (pattern wave-off, the LSO shorthand) with no points when `groove_entry_time` is `None`; a test pins the 20:30 gates. `NC` stays what it was, "not counted" for telemetry problems, and is unrelated to DCS's own `GRADE: NC` for missing comms. Decides: 20:30 of this session (`--` 2.0 to `WO(P)`), and every future overhead pattern abandoned before the groove. No other recorded pass moves.

**2. Brief the pilots: check in with the ship's ATC if the DCS grade is wanted next to ours; name the wave-off initiator when DCS does.** What it is: section 6; 3 of 15 traps have no landing quality mark because DCS graded them `NC : No proper communications` in a comment event the program cannot receive, and four recorded wave-offs carry a DCS `WO` or `OWO` the program displayed as `WO?`. Done: cause found in the 15 September DCS log and confirmed by the pilot for 18 September; the stop-position wire estimate names the wire on all three; **implemented on 19 September** (section 11): a mark opening on `GRADE:WO` gives `WO` (1.0 point, the value `docs/GRADING_REFERENCE.md` had listed since the table was written), `GRADE:OWO` gives `OWO` (no points) and latches the wave-off outcome; without a mark the grade stays `WO?`. To do: one line in the brief. Decides: 18:47 and 19:31 of 15 September (`WO?` to `WO`, 1.0 point each), 18:59 (`WO?` to `OWO`); the 14 September 19:30 DCS `WO` had a deck contact and keeps its `--`.

**3. Fly recommendation 5 of 15 September as written.** What it is: the client recorder on both PCs, hook down, 15 to 20 knots of deck wind. This session had none of the three, so the AoA numbers above (the 7.24 at the start of 20:38, the 12.5 to 12.9 at both ramps) stand on the server's computation alone, which was calibrated to 0.00 degrees median on the 14th and 15th but has never been checked with real wind. To do: the same pilots, the same procedure, `align_aoa.py` afterwards. Decides: the wind term of the AoA, the `OK` rate on ordinary flying (two `OK` in 50 passes so far).

**4. Take the low wind probe at flight-deck height, not at the waterline.** What it is: section 7; the low probe is queried at the carrier's altitude, which is the sea surface, where DCS's wind profile goes to a few knots and to exactly zero whenever the ship's altitude reads a hair below zero (the "sentinel", 12 of 50 passes). The aircraft never flies there: at the ramp it is at deck height, 20 m above the sea, where the wind is already three quarters of its value aloft. Done: measured live on the 19th (the profile table in section 7). To do: in the groove-entry wind reference of `src/tasks/record_recovery.rs`, query the low probe at `carrier.alt + carrier_info.deck_altitude` (20.15 m on the Nimitz class in `src/data.rs`, the same constant the hook-height computation already uses) and give `WindReference` that altitude; the sentinel retry and the high-probe fallback can stay as a guard. Decides: no grade so far; up to 0.2 degrees of AoA at the ramp in the planned windy session, on the "slow" side.

### 11. Implemented on 19 September: pattern wave-off and DCS wave-off initiator

Branch `feature/approach-only-and-owo`, off `feature/ramp-grader-touchdown-and-wire`. Nothing is committed.

| File | Change |
|---|---|
| `src/grading.rs` | `Grading::ApproachOnly` with `groove_entry_time == None` returns the new `PassGrade::PatternWaveoff` (`WO(P)`, no points), reason "WO(P): pattern waveoff, no roll-out on final"; `PassGrade` also gains `Waveoff` (`WO`, 1.0) and `OwnWaveoff` (`OWO`, none); `DcsWaveoffInitiator`, `dcs_waveoff_initiator` (parses `GRADE:WO` / `GRADE:OWO`), `apply_dcs_waveoff_initiator` (only a `WO?` is ever changed); two tests |
| `src/track.rs` | `dcs_grade_is_waveoff` uses the parser, so `GRADE:OWO` latches the wave-off outcome like `GRADE:WO`; the DCS mark refuses a bolter only when there is no touchdown event at all (a bolter with a correlated `runway_touch` stays a bolter); `finish()` applies the initiator to the pass grade and its points, and appends "DCS called a waveoff on this pass" to the reason of any trap, bolter or touch-and-go carrying such a mark; two tests on the 14 and 15 September comments; the event-stream-failure test gains the groove entry it implied |
| `src/tasks/record_recovery.rs` | outcome "Waveoff — ordered by the LSO (DCS)" / "Own waveoff (DCS)" / "Pattern waveoff — no roll-out on final" in the report, the chart and Discord; `cause` `dcs_lso_waveoff` / `dcs_own_waveoff` / `pattern_waveoff_no_groove_entry` |
| `src/commands/grade_ab.rs`, `src/commands/groove_ab.rs` | the columns apply the same mapping; the recorded-grade column reads `WO` / `OWO` |
| `docs/GRADING_REFERENCE.md`, `docs/HOW_GRADING_WORKS_PLAIN_LANGUAGE.md`, `CHANGES.md` | the two rules |

| Check | Result |
|---|---|
| `cargo test --locked` | 358 + 3 passed, 1 ignored (the fixture table test, as before) |
| `cargo clippy --locked --all-targets -D warnings`, `cargo fmt --check`, `git diff --check` | clean |
| `grade-ab` on the three recorded sets | 20:30 of 18 September `--` to `WO(P)` in every column; 15 September 18:47 and 19:31 `WO?` to `WO`, 18:59 `WO?` to `OWO`; every other row unchanged, including the 14 September 19:30 touch-and-go that DCS had graded `WO` |

What it means on the greenie board: a pattern abandoned before the groove no longer costs the pilot a `--`; a wave-off called by the DCS LSO now counts 1.0 point when the pilot had checked in, which is the value the reference table always listed and the first time it is applied; an own wave-off is shown as such and does not count. A pilot who touches the deck after a DCS wave-off call is graded on what he flew, and the reason says DCS called a wave-off, because a human LSO may overrule DCS; the reference's old "`C` for landing after a waveoff" line, never implemented, is withdrawn. Whether an own wave-off should carry points is a policy choice left open here.
