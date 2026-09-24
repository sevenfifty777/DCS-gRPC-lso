# Live session review, 13 September 2026

Eight passes flown by one human pilot (Ghost-72 | TT) on CVN-72, five in the T-45C and three in the F-14B(U), recorded by LSO 0.5.0 at commit `664fe5b` (clean tree) against DCS-gRPC 0.10.0 in buffered mode at 20 Hz. Source folder: `trap_records/recovery_13-09/` (JSON reports, charts, ACMI, `lso.db`, `lso.log`, `dcs.log`).

This document lists what was checked, what worked, and what needs fixing, ranked by how much it changes a pilot's result. Each finding says where the evidence is and which step of the plan in `steps_completion.md` it belongs to.

---

## 1. Summary

**What worked.** The whole acquisition chain was clean for the first time: every pass is telemetry-green at a true 20 Hz, poll latency p95 of 35 to 42 ms, no invalid samples, no reconnects, and the shared read budget never had to wait. Provenance is right (real commit, exact server and stubs version, populated baseline manifest). Groove entry fired correctly on all eight passes. The three hook-up touch-and-go passes were classified as touch-and-go. And the human-LSO path did its job on the one pass where DCS sent no `WIRE#` message: pass 8 was confirmed as a trap by deck kinematics and graded.

**What did not.** All eight passes came out `--` (no grade, 2.0 points). Two rules decide almost everything: the AOA axis and the "poor correction at the ramp" upgrade. Without them, the same data yields two `OK`, five `(OK)` and one `--`. Neither the grader nor DCS's own LSO (which gave `C` on three traps) can be called right without a human LSO reference, but two of the inputs feeding those rules look wrong on the evidence, not just harsh: the F-14 AOA reads 3 to 4 degrees below the on-speed band on every F-14 pass ever recorded, and the near-touchdown glideslope reading is really "touchdown position", not glideslope.

**Wire estimate.** Against the four traps DCS labelled, the hook-transient estimate agreed once, disagreed twice (both labelled "high" confidence) and was unavailable once. Two of those failures come from one concrete bug: the hook sampler's routine 300 ms gaps sit exactly on a 300 ms tolerance, and floating-point rounding decides the outcome. On the human-LSO pass, a fallback method that is structurally biased to "wire 1" named wire 1 and that number went into the database `wire` column.

---

## 2. The eight passes

Times are UTC from the DCS event log. "Our grade" is `pass_grade`; all eight scored 2.0 points.

| # | Time | Type | Outcome | DCS LSO comment | Our grade | Wire DCS / Rust (confidence) | Groove s | Episode that set the grade |
|---|---|---|---|---|---|---|---|---|
| 1 | 18:01:51 | T-45 | Trap | `C`: LUL X, LUL IM, LUL IC, LOAR, wire 1 | `--` | 1 / **2** (high) | 17.6 | AOA slow at ramp, 6.0 |
| 2 | 18:11:57 | T-45 | T&G, hook up | none | `--` | – | 16.5 | AOA slow at ramp, 6.0 |
| 3 | 18:14:16 | T-45 | Trap | `C`: SLO X, wire 1 | `--` | 1 / 1 (high) | 21.7 | AOA slow in close, 4.5 |
| 4 | 18:24:11 | T-45 | T&G, hook up | none | `--` | – | 17.6 | AOA slow at ramp, 6.0 |
| 5 | 18:29:37 | T-45 | Trap | `---`: TMRD IC, LO IC, LOAR, wire 2 | `--` | 2 / **none** | 17.5 | AOA slow in close, 4.5 |
| 6 | 18:40:27 | F-14 | T&G, hook up | none | `--` | – | 14.0 | GS high at ramp, 6.0 |
| 7 | 18:45:20 | F-14 | Trap | `C`: LUL X, SLO X, LOIC, LOAR, wire 2 | `--` | 2 / **1** (high) | 12.8 | AOA fast in middle, 3.6 |
| 8 | 18:56:18 | F-14 | Trap, **no DCS comment** | none | `--` | none / 1 (medium, fallback) | 15.7 | AOA fast in close, 4.5 |

Gate readings at 1/2 and 1/4 NM were all inside 1.0° on glideslope. Lineup was the only large geometric error: pass 1 was 3.0° left at 1/2 NM and 2.2° left at 1/4 NM, which matches DCS's "LUL" calls.

### Sensitivity of the grade to the two contested rules

Recomputed from the `grading_episodes` block of each report, using the grader's own thresholds (effective severity below 1.5 is `OK`, below 3.0 is `(OK)`, otherwise `--`).

| # | As graded | Without the AOA axis | Without AOA and with ramp episodes counted at their measured severity |
|---|---|---|---|
| 1 | `--` | `--` (GS ramp upgraded to 4.0) | `(OK)` (lineup 2.4) |
| 2 | `--` | `--` (GS ramp upgraded to 4.0) | `(OK)` |
| 3 | `--` | `--` (GS ramp upgraded to 4.0) | `(OK)` |
| 4 | `--` | `(OK)` | `(OK)` |
| 5 | `--` | `--` (lineup ramp upgraded to 4.0) | `(OK)` |
| 6 | `--` | `--` | `--` |
| 7 | `--` | `OK` | `OK` |
| 8 | `--` | `OK` | `OK` |

The point is not that the third column is right. The point is that the answer swings from "everything is `--`" to "almost everything is fine" on two rules that have never been checked against a human LSO. That is the same conclusion as section 4c of the 12 September comparison, now with a concrete mechanism behind it.

---

## 3. Findings

Severity: **High** changes a pilot's grade or the outcome; **Medium** changes wire, confidence or what the pilot is told; **Low** is hygiene.

### F1. High. The F-14 AOA reference is almost certainly wrong

Every F-14 pass today read "fast" for the whole groove: computed AOA mostly between 5 and 9°, with the peak normalised error at -3.96 to -3.99 on all three passes, against an on-speed band of 10.2 to 11.1° in `f14_aoa_rating` (`src/data.rs:174`). On pass 7, DCS's LSO called the same approach **SLO X** (slow all the way). Both cannot be right.

It is not this pilot. The three F-14 recordings from 2 and 3 September in `tests/recordings/live_2026-09/` carry the AOA the program computed at the time, and their median on the last 15 s of final is 6.4, 8.1 and 9.8°, again below the band. Two pilots, two weeks, every F-14 pass "fast" by 3 to 4°.

Consequence: passes 7 and 8 are geometrically clean (no glideslope or lineup episode above `OK`) and are `--` purely on AOA. With the current band, an F-14 cannot get better than `--` unless the pilot flies what the program thinks is 10.5°.

The T-45 does not show this: its band was derived from the module's own display code, its values sit around the band, and DCS's "SLO X" on pass 3 matches our "slow" episodes on that pass.

**What to do.** Calibrate before AOA is allowed to grade the F-14: one flight holding the on-speed indexer (donut) on final while reading `datums[].aoa`; the difference is the offset to apply or the band to move. Until then set `aoa_reliable` to false for the F-14 so AOA episodes stay diagnostic. Step 8 of the plan.

### F2. High. Near-touchdown glideslope is touchdown position, and the ramp rule turns it into `--`

Inside 75 m the glideslope deviation is computed as height error over a fixed 75 m (`NEAR_TOUCHDOWN_ANGLE_REFERENCE_M`, `src/track.rs:134`). When the aircraft touches down 19 m short of the reference point, the ideal path is still 1.16 m above the deck there, so the reading is -0.87°. That is what passes 1, 2, 3 and 5 show in their last 20 m: a "small" low episode that appears only because the aircraft is on the deck.

Because the trajectory ends at touchdown, the episode can never "stabilise", so `build_axis_episodes` (`src/grading.rs:977`) classes the correction as poor, bumps severity one level (small becomes medium) and multiplies by the ramp weight 2.0. Result 4.0, which is `--`. In effect: any touchdown more than about 11 m short or long of the reference point is `--`, on every aircraft, regardless of the approach. Pass 2 is a hook-up touch-and-go, where "which wire" has no meaning, and it got the same treatment.

**What to do.** Stop the glideslope assessment where the hook reaches the deck (or at a fixed 25 to 30 m from the reference) and report touchdown position separately as a plain note ("touched down 19 m short, about the 1-wire"). Do not apply the "poor correction" upgrade to an episode that ends because the trajectory ended. Whether a 1-wire deserves `--` is then an explicit policy decision, not a side effect. Step 8.

### F3. High. A 0.07-unit AOA blip for under a second at the ramp is a `--`

Pass 4: AOA peaked at 8.07 against a "slow" threshold of 8.0, for 0.85 s, 1.3 s before touchdown. Classified medium, correction poor ("no real post-peak improvement" in a two-sample window), upgraded to large, ramp weight 2.0, effective severity 6.0, the highest value seen all day. Geometry on that pass was `(OK)`. Passes 1 and 2 have the same shape (peak 9.49 and 9.49 in the last 1 to 2 s).

The persistence guard is two samples (100 ms). That is fine for position, which is smooth, but AOA on a T-45 flares through 4 units in the last second (see `datums`: 9.4 then 5.6 at touchdown on pass 1). A real LSO does not call AOA in the last second.

**What to do.** Require about 1 s of persistence for AOA episodes, and do not apply the "poor correction" upgrade in the ramp zone at all, since by definition nothing can be corrected in the last two seconds. Step 8.

### F4. Medium. The hook-transient wire method fails on a floating-point edge

The hook sampler runs at a median 250 ms with a p90 of 300 ms (measured on the `hook_observation.timeline` of all eight reports). `completed_hook_deflection_near` (`src/track.rs:3556`) rejects any sample pair whose gap exceeds `SAMPLE_GAP_WARNING_MS` = 300.0. A nominal 300 ms gap comes out as either 299.99999999995 ms or 300.00000000018 ms depending on how DCS rounded the two timestamps.

| Pass | Gap at the deflection pair | Result |
|---|---|---|
| 1 | 299.999999999955 ms | accepted, transient found |
| 5 | 300.000000000182 ms | **rejected**, no transient, then the fallback also failed (event lag 310 ms), so "Rust estimate unavailable" |
| 8 | 200 ms at the deflection, but the gap just before the pre-deflection sample was 300.000000000182 ms, so the stable-before walk stopped at once and the stable duration became 0 s | **rejected**, fallback used (see F5) |

The hook animation itself is unambiguous in all five traps: the T-45 goes 1.0 to 0.0 and back to 1.0 about 2 s later; the F-14 goes 1.0 to about 0.16 and back to 0.86 about 4.5 s later. The method has the evidence; the tolerance throws it away.

**What to do.** Compare against the tolerance with a small epsilon, or better, set the hook-sample tolerance from the sampler's own period (350 to 400 ms). Longer term, step 7 already proposes routing hook samples through the buffered engine at 20 Hz, which fixes this and F6 together.

### F5. Medium. The fallback wire method is biased to wire 1, and it named a wire on the human-LSO pass

When no hook transient is found, `wire_estimate_at` (`src/track.rs:3941`) picks the earliest hook-plane crossing at or after "deceleration onset minus 1.2 s". Today the four crossings spanned only 0.6 to 0.8 s and the first crossing preceded the onset by 0.8 to 1.15 s on every trap, so a 1.2 s window always contains wire 1. Applied to all five traps it would have said 1, 1, 1, 1, 1 against DCS's 1, 1, 2, 2 and unknown.

The raw datums give the lag the window should be built from. On the four DCS-labelled traps, the first visible loss of closing speed comes 0.55 to 0.65 s after the hook crossed the plane of the wire DCS named, and the code's threshold-based onset comes 0.75 to 0.95 s after it. Those are the numbers to anchor on, not 1.2 s.

On pass 8 (no DCS comment) this fallback produced "Wire #1 (Rust estimate)" at medium confidence. That string is the chart title, `wire_estimated` is 1, and `record_recovery.rs:1742` stores `wire = wire_dcs.or(wire_estimated)`, so the dashboard's `wire` column reads 1 with nothing marking it as an estimate. The policy sentence written in step 3 says a kinematically confirmed trap never invents a wire number; this pass had a kinematic confirmation and a named wire.

**What to do.** Either select the crossing nearest to "onset minus the measured lag" instead of the earliest crossing in a wide window, or, simpler and consistent with the policy, do not name a wire from the fallback when the arrest is not DCS-confirmed. Keep `wire` null in the database when `wire_dcs` is null; `wire_estimated` and `wire_estimation_confidence` already exist for the estimate. Step 3 follow-up.

### F6. Medium. "High" confidence on two wrong wire calls

Pass 1 said wire 2 (DCS: 1); pass 7 said wire 1 (DCS: 2). Both carried `wire_estimation.confidence: "high"`. "High" only requires a DCS-confirmed arrest and tight brackets (`src/track.rs:3953`); it says nothing about whether the deflection could be tied to one wire rather than its neighbour.

The underlying limit is resolution. Wires are 12.5 m apart, so at 50 to 65 m/s the hook crosses them 200 to 250 ms apart, and the hook sampler only reports every 250 ms. The deflection is known to within one sample, which is about one wire. On pass 1 the crossings sat 290 ms (wire 1) and 50 ms (wire 2) before the deflection sample and the 200 ms rule chose 2; on pass 7 wire 2's crossing was 50 ms after the deflection sample and the rule chose 1.

**What to do.** Reserve "high" for a deflection whose preceding sample gap is under about 120 ms and where exactly one crossing falls in the window. With the sampler at 250 ms that will rarely happen, which is honest. The real fix is the 20 Hz hook path from step 7.

### F7. Medium. Substituted wind reference still lets AOA grade the pass

Pass 7: the deck-level wind probe returned 0.0 m/s at heading 180 and was overridden by the 67 m reading of 6.03 m/s at 96° (`wind_reference_probes.low_reading_overridden_by_high: true`, `wind_reading_is_groove_entry_fallback: true`). All other passes used about 1 m/s. `aoa_reliable` is simply `wind_reference.is_some()` (`src/track.rs:3095`), so the AOA episodes on that pass kept `affects_grade: true` and set the grade. This is the leftover from review finding F17 and item 8 of the plan, seen live.

**What to do.** When the low probe was substituted, mark AOA unreliable for that pass (episodes stay in the report as diagnostics).

### F8. Medium. The 3/4 NM gate shows turn values to the pilot

On every pass today the 3/4 NM gate was captured before roll-out (timestamps 986.5 vs groove entry 997.3 on pass 1, and likewise on the others), so its lineup reads -11.4, -9.2, -2.5, -7.0, -6.6, -6.0, -8.9 and -7.7°. The grader correctly ignores it, but the chart header prints "3/4nm: GS +1.1° LU -11.4°" as if it were an approach error, and for touch-and-go passes the Discord field "LSO Notes (measured by LSO, not a DCS comment)" is built by `describe_measured_deviations` (`src/grading.rs:443`), which does not know about groove entry and will say "left of centerline at 3/4 NM (-9.2°)".

**What to do.** Pass `groove_entry_time` into `describe_measured_deviations` and skip the gate when it precedes roll-out; label it "(in turn)" or omit it on the chart.

### F9. Low. F-14 hook height correction now over-corrects

At engagement the modelled F-14 hook sits 0.3 to 0.7 m above the deck (pass 7 at x = 1.6 to 7.8 m: 0.29 to 0.43 m; pass 8: 0.45 to 0.70 m) and 1.9 m above deck at the touch-and-go contact on pass 6. The +1.0 m `F14_HOOK_VERTICAL_CORRECTION_M` (`src/data.rs:206`) was set when the hook read 0.8 to 1.1 m below deck; it now looks about 0.4 to 0.5 m too large. Effect: roughly +0.3° "high" bias on F-14 ramp glideslope, which is part of pass 6's 1.3° high reading. The T-45 reads 0.00 m on the deck, as documented.

**What to do.** Halve the correction or, as the comment already asks, re-measure in ModelViewer2.

### F10. Low. DCS touch event timing on the T-45 is outside the correlation window

On all three T-45 traps the measurable deceleration began 310 to 370 ms before DCS's `runway_touch` event (`deceleration_contact_delta_ms`), and the hook touched the deck 1.0 to 1.2 s before it. The fallback wire method requires the event to be within 300 ms of the onset, so on the T-45 the fallback is unavailable every time (pass 5). Only matters while F4 stands.

### F11. Low. Operational notes from the log

- Every trap produced a second `land` event 1.0 s after `runway_touch`; all were rejected as duplicates and logged at WARN ("duplicate touchdown ignored"). Routine on 5 of 5 traps, so INFO would be the right level.
- Seven recorder attempts were started and discarded (28 to 63 s each, lowest altitude 10 to 24 m): five catapult launches and two climb-outs re-detected right after a touch-and-go. Harmless, but each one holds a read-budget share while it lasts.
- At 17:19:22 LSO terminated because `live-baseline.json` did not exist yet. Fatal is defensible, but "warn and continue with an empty manifest" would have avoided one restart. Five manual restarts between 17:15 and 17:32 precede the session.
- The mission wind was 2 to 3 kt again. The wind-corrected AOA has still never been exercised in real wind (same caveat as section 4d of the comparison).
- One other pass today, outside this folder: an AV-8B by another pilot at 17:39:50 (before the mission restart) finalised as `NC` with red telemetry health. Not analysed here.

---

## 4. What worked, with numbers

| Check | Result |
|---|---|
| Effective sample rate | 20.0 Hz on all eight passes |
| Position poll latency p95 / max | 35 to 42 ms / 90 to 356 ms |
| Max capture gap on the DCS side | 50 ms on all eight |
| Invalid samples, dropped samples, lost snapshots | 0 |
| Shared read budget (16/s) total wait per pass | 2.0 to 3.0 ms |
| Hook sampler | 575 to 814 successful samples per pass, 0 timeouts, 1 error total |
| Event stream | available throughout, 0 reconnections during the session |
| Groove entry | fired on 8 of 8, all via port final-turn roll-out, 723 to 1076 m out |
| Touch-and-go with hook up | 3 of 3 classified `T&G (CQ)`, hook state `up` |
| Duplicate `land` events | 5 of 5 rejected |
| Human-LSO trap (no `WIRE#`) | pass 8 confirmed by deck kinematics, medium confidence, graded, 2.0 points |
| Provenance | `lso_version` 0.5.0, real commit, `lso_dirty` false, server and stubs both 0.10.0, compatibility `exact`, `baseline_manifest` populated |

This partly answers step 7. With one pilot in the pattern and the hook sampler on, the buffered path shows none of the 700 to 800 ms delivery delay seen in Lenny's sessions. The remaining unknown is the three-pilot case, and that still needs its own session.

---

## 5. Suggested order of work

1. **F4** (epsilon on the hook gap tolerance): a one-line change that recovers the wire estimate on passes like 5 and 8.
2. **F5** (no invented wire from the fallback; keep `wire` null without DCS): restores the step 3 policy and cleans the dashboard.
3. **F1** (F-14 AOA calibration flight, AOA diagnostic-only for the F-14 until then).
4. **F2 and F3** (end the glideslope assessment at deck contact; longer AOA persistence; no poor-correction upgrade in the ramp zone). These are grading-threshold changes and belong to step 8, so record the before and after on these eight reports plus the 14 fixtures rather than tuning by eye.
5. **F7, F8, F6** in any order.
6. **F9, F10, F11** when convenient.

Everything in sections 2 and 3 can be re-derived from the JSON reports in the folder; no live session is needed to verify the numbers, only to verify the fixes.
