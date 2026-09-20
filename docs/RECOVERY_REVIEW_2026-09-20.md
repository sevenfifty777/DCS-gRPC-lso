# Recovery of 20 September 2026: nine passes, four pilots, and the wire estimate checked against the 18th

Nine recordings on CVN-72 on 20 September (09:04 to 20:46 UTC), four pilots (6-Rose and Justice in the F-14B(U), ERGO in the F/A-18C, Ghost-72 in the F-14B(U)), recorded by LSO 0.5.0 at `3c4c6b4` with the uncommitted working tree that became `c818d66` on the 19th (the reports say `lso_dirty true`). Six traps, three hook-up touch-and-goes. The mission wind was calm again (1.0 to 1.4 m/s at deck level, 6.2 to 7.1 m/s aloft, from 084), so the windy session asked for on the 15th and the 18th has still not been flown; nothing in this document is about the wind term.

The question asked of this session: did the wire estimate regress against the 18th? Short answer: the estimator did not change and the stop-position method is right on every trap where DCS named the wire, but on two of the six traps a different, weaker method ran first and blanked the result. The rest of this document is the evidence.

Source folder: `tools/aoa_calibration/recovery_19-20/` (the 4 reports of the 18th plus 9 of the 20th, `dcs.log`, `lso.log`). Re-grading tool: `target/release/lso.exe grade-ab` built on the 19th at 15:00 from the same tree. Previous document: `docs/RECOVERY_REVIEW_2026-09-18.md`. Times are UTC; file names are local time, two hours later.

Part 1 is written for everyone. Part 2 has the numbers.

---

## Part 1. In plain language

### What was flown

| UTC | Pilot | Aircraft | Outcome on the board | DCS LSO | Our grade |
|---|---|---|---|---|---|
| 09:04 | 6-Rose | F-14B(U) | Arrested, wire evidence unavailable | `NC` no comms (comment only) | `--` 2.0 |
| 09:25 | 6-Rose | F-14B(U) | Wire #1 (Rust estimate) | nothing at all | `(OK)`, no points |
| 18:52 | Justice | F-14B(U) | T&G, hook up | none | `--` 2.0 |
| 18:55 | Justice | F-14B(U) | T&G, hook up | none | `(OK)` 3.0 |
| 18:57 | Justice | F-14B(U) | DCS wire 3, Rust estimate unavailable | `---` wire 3 | `(OK)` 3.0 |
| 19:10 | ERGO | F/A-18C | DCS wire 3, Rust estimate 2 | `---` wire 3 | `(OK)` 3.0 |
| 20:40 | Ghost-72 | F-14B(U) | wire 1 (DCS and Rust agree) | `---` wire 1 | `--` 2.0 |
| 20:43 | Justice | F-14B(U) | T&G, hook up | none | `(OK)` 3.0 |
| 20:46 | Justice | F-14B(U) | wire 1 (DCS and Rust agree) | `C` wire 1 | `--` 2.0 |

All nine recorded grades are reproduced by `grade-ab` under the `CONVENTION` column, so the grading side of the build behaved as on the 18th and is not discussed further here.

### What we found about the wire

**1. The estimator code is the same as on the 18th.** The only source changes between the build that flew on the 18th and the one that flew on the 20th are the pattern wave-off and DCS wave-off initiator rules of the 19th; none of them touch the wire estimate, the hook sampler or the deck kinematics. Whatever differs between the two sessions comes from the passes, not the program.

**2. The stop-position method is right every time it is allowed to answer.** The method added on the evening of the 15th says: the arresting gear lets a Tomcat run 87 m past the wire it caught, so wherever the aircraft stops, add 87 m and you are standing on the wire. On the 20th it was consulted on two traps and named the 1-wire on both, which is what DCS said. Counting every trap in the four sessions where DCS named the wire and the aircraft type has the 87 m constant, it is now right on twelve out of twelve, never more than 3.2 m off the wire, on 1-, 2-, 3- and 4-wires, Tomcat and Hornet.

**3. Two Tomcat traps show "Rust estimate unavailable" because the hook animation spoke first and said nothing useful.** The program has three ways to name the wire, tried in order: the animated hook's jerk when a wire grabs it, then the stop position, then the crossing sequence. On 18:57 (Justice, DCS says 3-wire) and 09:04 (6-Rose, no DCS reference), the hook did jerk, but not within the 200 ms of a wire crossing the first method demands (214 ms on 18:57; 390 ms before the first wire on 09:04, because the hook hit the deck 22 m short of the wires). The first method then returned "hook jerk not correlated with a wire" as its answer, and the program treated that as an answer instead of a shrug: the stop position was never consulted, although it had the wire (3-wire on 18:57, 0.3 m off; 2-wire on 09:04, 1.9 m off). On the 18th this could not happen because the hook sampler happened to see no clean jerk on that trap, so the second method ran. This is the one code change to make from this session: a non-answer from the hook must fall through to the stop position (Part 2, section 3).

**4. The Hornet's wire was named wrong (2 instead of DCS's 3) because the Hornet has no run-out constant.** The type table says the Hornet's run-out was not constant (89 m on the one Hornet trap of the 14th, 60 m on an older fixture), so the stop position is switched off for it and the crossing sequence answers instead, and the crossing sequence is the method the 15th review showed always picks a wire too early. The 20th adds a second live Hornet trap at 88 m past the 3-wire. Two live traps at 88 and 89 m against one fixture at 60 m from a different carrier and DCS build is enough to switch the Hornet on at 87 m and watch it (section 4).

**5. The 09:25 trap was a straight-in and the program never saw a groove.** 6-Rose flew a 6 km straight-in, wings level the whole way, lineup inside 0.4 degrees at every gate. The groove-entry detector is built for the Case I pattern and arms only after a left turn below 600 ft; there was none, so there was no groove entry, no wind reference, no hook reading, no deck-kinematics window and no stop position. The trap was graded `(OK)` from the three gates with no points, and the wire came from the crossing sequence, which said 1-wire. The hook jerked exactly at the 1-wire crossing and the deceleration began 56 m past it, so 1-wire is very probably right, but the program got there by the method that is least trusted. DCS's own LSO wrote nothing for this trap, not even the "no comms" line. A straight-in is a legitimate approach and needs its own groove entry (section 5).

**6. The 09:04 trap is genuinely ambiguous, and "wire evidence unavailable" is the honest output, reached by accident.** Two independent signatures disagree: the aircraft stopped at a position that says 2-wire (with the 87 m constant, 1.9 m off), while the deceleration started 53 m past the 1-wire, which is exactly the distance seen on every 1-wire trap so far. The hook animation shows the hook striking the deck 22 m short of the 1-wire and staying pushed up while the aircraft rolled over the first two wires, which is what a hook skip looks like: consistent with skipping the 1 and catching the 2, and a run-out of 96 m for a 1-wire would be 7 m beyond anything recorded. DCS graded it "no comms". Left as unresolved; it is the first hook-skip in the corpus and the reason to keep both signatures in the report.

**7. The three hook-up touch-and-goes carry no "would have caught wire N" text, unlike the 18th.** Same code. On the 18th DCS sent no touchdown event for the hook-up pass, so the estimate anchored on the deck crossing and found a wire; on the 20th DCS sent a `runway_touch` about a second after the hook point passed the 4-wire, the estimate anchored on that event, found it more than 300 ms from any crossing and declined. Whether the hypothetical wire appears therefore depends on whether DCS chooses to fire an event, which is not a property of the pass. Minor; noted in section 6.

### What changes

| Item | Status |
|---|---|
| Grades of the nine recordings | 9 of 9 reproduced by `grade-ab` under `CONVENTION` |
| Wire estimate code | unchanged since the 18th; no regression in the method |
| Stop-position method | 12 of 12 against DCS across all sessions, max residual 3.2 m; 2 of 2 on the 20th |
| Hook-transient "not correlated" blocking the stop position | **fixed on 20 September** (section 10): the non-answer falls through to the stop position, then the crossings; decides 18:57 (unavailable to 3, agreeing with DCS) and 09:04 (unavailable to 2) |
| Hornet run-out constant | **tried and backed out on 20 September** (section 10): with 87 m the old `wire_4_01_FA18C` fixture (60 m run-out) is named 1-wire; stays off, the two live traps recorded in the type table |
| Straight-in approaches | **deferred**: no groove entry, no points, no reliable wire (section 5); to be taken up with the Case II and III work |
| Hook-up touch-and-go hypothetical wire | inconsistent between sessions, event-dependent (section 6); low priority |
| Wind probe at the waterline | the 180/0.0 sentinel hit 3 of 9 passes again; recommendation 4 of the 18th stands |
| Windy session, client recorder, hook down | still not flown |

### Recommendation

Make the hook-transient non-answer fall through to the stop position (done the same day, section 10), give the Hornet the 87 m constant (tried, backed out: section 10), and fly the windy session. The straight-in entry waits for the Case II and III work.

---

## Part 2. Technical

### 1. Data set

| Item | Value |
|---|---|
| Reports | 9 new (`LSO-20260920-*.json`) plus the 4 of the 18th in the same folder |
| Outcomes | 6 arrestments (4 with a DCS landing quality mark), 3 hook-up touch-and-goes |
| DCS LSO lines in `dcs.log` | 4 landing quality marks (`--- WIRE# 3`, `--- WIRE# 3`, `--- WIRE# 1`, `C WIRE# 1`), one `GRADE: NC : No proper communications` comment for 09:04, nothing for 09:25 |
| Mission wind | 084 at 1.0 to 1.4 m/s deck level, 6.2 to 7.1 m/s at 76 to 141 m |
| Wind probes | 180/0.0 sentinel on 18:52, 18:57, 20:46 (low probe altitude -0.0 or 0.0, overridden by the high probe), as diagnosed on the 18th |
| Acquisition | 20.0 Hz, 0 dropped, 0 invalid, health green on all nine |
| Build | `lso_commit 3c4c6b4`, `lso_dirty true` (the working tree of the 19th); `git diff 3c4c6b4 c818d66 -- src/track.rs` touches only the DCS wave-off handling in `finish()`, `dcs_grade_is_waveoff` and tests |
| Debug log | `lso.log` was at INFO on the 20th, so the `wire estimate` debug lines used on the 7th to 12th are absent; everything below is from the reports |

### 2. Every trap in the corpus, three signatures against the DCS wire

Aircraft `x` is the landing-area frame of the datums (positive astern). The wire planes sit at about +16 (1), +4 (2), -9 (3), -21 (4) m. "Stop" is `deck_kinematics.x_at_slow_m`; "stop wire" is the crossing nearest `stop + 87`; "onset" is `arrest_deceleration_onset_time` and the distance is from the DCS wire's crossing position to the aircraft position at the onset; "hook" is the completed hook-animation transient and its lag after the last crossing before it.

| Session | UTC | Pilot | Type | DCS | Recorded estimate (method) | Stop, m | Stop wire (residual) | Onset past DCS wire | Hook transient |
|---|---|---|---|---|---|---|---|---|---|
| 14 Sept | | Ghost-72 | T-45 | 2 | 2 (hook) | -57.7 | (no constant) | 39.9 | 111 ms after 2 |
| 14 Sept | | Ghost-72 | T-45 | 2 | none (crossing not correlated) | -52.1 | | 39.6 | none |
| 14 Sept | | Ghost-72 | F-14 | 4 | none (hook not correlated) | -111.5 | **4** (3.2) | 57.0 | 245 ms after 4 |
| 14 Sept | | Ghost-72 | F-14 | 1 | none (crossing not correlated) | -71.5 | **1** (1.4) | 59.0 | none |
| 14 Sept | | Justice | F-14 | 2 | none (hook not correlated) | -83.5 | **2** (1.8) | 57.9 | 223 ms after 1 |
| 14 Sept | | Ducks | F/A-18C | 3 | none (crossing not correlated) | -98.0 | **3** (2.1) | 53.4 | none |
| 15 Sept | | Ghost-72 | T-45 | 1 | 1 (hook) | -40.6 | | 38.0 | 199 ms after 1 |
| 15 Sept | | Ghost-72 | T-45 | 3 | none | -62.4 | | 30.9 | none |
| 15 Sept | | Justice | F-14 | 2 | none | -84.8 | **2** (3.1) | 57.7 | none |
| 15 Sept | 19:17 | Ghost-72 | F-14 | none | 1 (crossing) | -85.2 | 2 (2.0) | | none |
| 15 Sept | | Justice | F-14 | 1 | 1 (crossing) | -71.1 | **1** (1.1) | 57.2 | none |
| 15 Sept | | Ghost-72 | F-14 | 4 | 4 (hook) | -111.5 | **4** (1.9) | 55.0 | 12 ms after 4 |
| 15 Sept | | Justice | F-14 | 2 | none | -84.9 | **2** (2.6) | 56.2 | none |
| 18 Sept | 20:41 | Justice | F-14 | none | 3 (stop) | -96.7 | 3 (0.2) | | none |
| 20 Sept | 09:04 | 6-Rose | F-14 | none (`NC`) | none (hook not correlated) | -79.9 | 2 (1.9) | | 387 ms before 1 |
| 20 Sept | 09:25 | 6-Rose | F-14 | none | 1 (crossing) | no window | | | none (no groove) |
| 20 Sept | 18:57 | Justice | F-14 | 3 | none (hook not correlated) | -97.5 | **3** (0.3) | 53.8 | 214 ms after 3 |
| 20 Sept | 19:10 | ERGO | F/A-18C | 3 | 2 (crossing) | -97.0 | **3** (1.3) | 53.8 | none |
| 20 Sept | 20:40 | Ghost-72 | F-14 | 1 | 1 (stop) | -73.1 | **1** (1.9) | 55.8 | none |
| 20 Sept | 20:46 | Justice | F-14 | 1 | 1 (hook, high) | -72.7 | **1** (0.4) | 52.9 | 111 ms after 1 |

Three things the table says:

- **Stop position, Tomcat and Hornet, against DCS: 12 of 12**, residuals 0.3 to 3.2 m, all four wires represented. The T-45 is excluded (its run-out spreads 53 to 62 m, one pendant spacing, as the type table already says).
- **Deceleration onset is a second, independent signature.** On the Tomcat the onset sits 52.9 to 59.0 m past the engaged wire on all ten DCS-confirmed traps; on the Hornet 53.4 and 53.8 m; on the T-45 31 to 40 m. Nobody planned this constant; it falls out of the arresting engine's pull-up curve and the onset detector's threshold. It can serve as a tie-breaker and is what makes 09:04 ambiguous (section 3).
- **The hook transient is right when it correlates inside 200 ms (5 of 5) and would have been wrong once if the window were widened to 250 ms** (14 September, Justice, 223 ms after the 1-wire, DCS 2-wire). So the 200 ms window is not the thing to loosen; the priority order is.

### 3. The hook-transient non-answer blocks the stop position

`Track::wire_estimate_with` (`src/track.rs`, from line 4005):

```rust
if let Some(estimate) = self.wire_estimate_from_hook_transient(event_time) {
    return estimate;
}
if !hook_up {
    if let Some(estimate) = stop_x.and_then(|x| self.wire_estimate_from_stop_position(x)) {
        return estimate;
    }
}
```

`wire_estimate_from_hook_transient` (from line 3558) returns `Some` in three cases: a wire (correlated within `MAX_HOOK_DEFLECTION_WIRE_LAG_MS` = 200 ms of the last crossing before the deflection), or `wire: None` with reason `hook_deflection_not_correlated_with_wire_crossing` when no crossing precedes the deflection or the lag is outside 0 to 200 ms. The caller returns all three unconditionally. The two `None` forms therefore pre-empt both the stop position and the crossing fallback.

What the hook did on the two traps, from `hook_observation.timeline` (raw 1.0 is down, near 0 is pushed up):

| UTC | Samples before | Deflection | Where the aircraft was | Nearest crossing | Result |
|---|---|---|---|---|---|
| 18:57 | 1.0 until 3141.90 (0.88) | 3142.20 raw -0.72 | x = -22.2 m, on the 4-wire plane, wheels at 0.09 m | 3-wire at 3141.99, 214 ms earlier | lag 214 > 200: `None` |
| 09:04 | 1.0 until 25329.72 (0.955) | 25329.92 raw 0.22 | x = +38.5 m, 22 m short of the 1-wire, wheels at 0.1 m | 1-wire at 25330.31, 387 ms later | no crossing before the deflection: `None` |

On 18:57 every other signature says 3-wire: DCS `WIRE# 3`, stop -97.5 m (engaged at -10.5, the 3-wire crossing at -10.75), onset 53.8 m past the 3-wire. The hook itself deflected 11 m past the 3-wire plane, which is the same geometry as 20:46 (deflection 6 m past the 1-wire it caught, 111 ms); the pendant is dragged that far before the animation registers. The 200 ms window is simply tight for a Tomcat at 62 m/s: 214, 223 and 245 ms have now been seen on three DCS-confirmed traps.

On 09:04 the deflection is the hook hitting the deck before the wires (the aircraft touched down short: wheels at 0.10 m at the 1-wire crossing, 0.29 m at the 2-wire, rising to 0.46 m at the 4-wire, a small bounce). The hook then reads 0.13 to 0.42 across all four crossings, with one sample at -0.006 between the 1- and 2-wire planes: pushed up, not dragging. Stop -79.9 m puts the engaged wire at +7.1, 1.9 m from the 2-wire crossing (+5.25) and 9.2 m from the 1-wire (+16.35); the onset, corrected for the 0.1 s between the last datum and the onset time, sits about 53 m past the 1-wire and 42 m past the 2-wire. The 1-wire stops in the corpus are -71.1 to -73.1 m, the 2-wire stops -83.5 to -85.2 m; -79.9 sits between them. Unresolved. A hook that strikes the deck, bounces over the 1-wire and catches the 2 is the picture most consistent with the animation and the stop; the onset then reads 11 m early for a 2-wire, which no other trap has shown. The report should carry both numbers rather than pick.

**The fix.** In `wire_estimate_with`, return the hook-transient result only when it names a wire; otherwise remember its reason and go on to the stop position, then the crossing selection. When the later methods also fail, the returned evidence should keep the hook's `hook_deflection_time_dcs` and reason so the report still shows what the hook did. Two lines in the caller, one field carried through. Test: the Tomcat of 18:57 (deflection 214 ms after the 3-wire, stop -97.5) must return 3 with reason `stop_position_run_out`; the 09:04 shape (deflection before the first crossing, stop -79.9) must return 2. In the cockpit and on the board nothing else changes: a hook-up pass keeps the hypothetical path, a pass with no stop keeps the crossing fallback.

On the recorded corpus this decides 18:57 (unavailable to 3), 09:04 (unavailable to 2), and the two 14 September traps recorded before the stop-position method existed (4 and 2, both matching DCS). No estimate that currently names a wire moves.

### 4. The Hornet run-out

`src/data.rs` line 156 keeps `arresting_run_out_m: None` for the Hornet because the 14 September 3-wire trap gave 89 m and the `wire_4_01_FA18C` fixture gave 60 m. The 20th adds ERGO's 3-wire trap: stop -97.0, 3-wire crossing at -8.73, run-out 88.3 m; onset 53.8 m past the wire, the same as the Tomcat's. Two live Hornet traps on CVN-72 under DCS 2.9.29 at 88 and 89 m, against one fixture from another carrier and build at 60 m. The fixture is the outlier, and the reason the table gives (carrier, DCS build, weight) is the reason to trust the live pair for this server.

**Proposal.** `arresting_run_out_m: Some(87.0)` for the Hornet with a comment naming the two traps; the residual guard (`WIRE_STOP_POSITION_MAX_RESIDUAL_M` = 6 m) already declines to name a wire if a future Hornet stops 60 m past it. Decides 19:10 (2 to 3, agreeing with DCS) and the 14 September Ducks trap (none to 3). The `wire_4_01_FA18C` fixture test must be checked: with 87 m and a 60 m real run-out the guard should return `None` and the fixture keeps its current path.

### 5. The straight-in of 09:25

The track, from the datums: first sample 6,372 m out at 170 m, descending steadily to 33 m at 465 m out, bank never beyond 8.6 degrees and under 3 degrees inside 4 km, lineup +0.02, +0.39, -0.25 degrees at the three gates, glideslope +0.42, +0.18, +0.61. A textbook straight-in. Groove entry (`groove_entry.criteria`) requires `last_turn_armed`, which needs a port pattern turn below 600 ft while inbound; there was none, so:

- no groove entry, no groove series, no wind reference (`wind_reference_probes` absent), no episodes;
- `hook_observation.samples_in_groove` 0, `interpreted_state` unknown, so `hook_up` is false and the hook transient is never looked for (`in_final_window` is false on every sample);
- `deck_kinematics.samples` 0: the deck-kinematics buffer is only filled once `entered_groove` is set (`src/track.rs` line 2263), so `evaluate_deck_arrest_kinematics` returns `no_deck_samples_after_reference` and there is no stop position, although the `kinematic` block did see 202 post-contact samples and a sustained stop (`accepted true`);
- the wire came from the crossing fallback: onset at 26520.57, the first crossing after `onset - 1.2 s` is the 1-wire, so "Wire #1 (Rust estimate)", medium confidence, reason `continuous_hook_plane_crossing`. The hook animation dropped from 0.70 to 0.30 at 26519.52, on the 1-wire crossing (26519.54), and the onset is 56.4 m past the 1-wire, so 1-wire is almost certainly right.
- grade `(OK)` from the gates, `points_eligible` false, `assessment_scope partial`, cause `unconfirmed_arrest`.

DCS's LSO wrote nothing for this trap in `dcs.log` (the 09:04 trap of the same pilot got the `NC` comment), so DCS did not consider it a graded approach either; possibly the pilot had not been in the pattern.

The 18th review left open whether a roll-out 160 m off the centreline should count as a groove entry; this trap is the opposite case, a perfect lineup with no turn to arm the detector. A straight-in entry rule (wings level, inside 2 degrees of lineup, inside the 300 ft box, sustained 0.75 s, no last-turn requirement when the aircraft has been inbound and wings-level for more than, say, 10 s) would give this pass a groove, a wind reference, a hook state, a deck window and points. Not implemented here; the roll-out detector's `last_turn_arm_reason` field is where the new reason would go.

### 6. Hook-up touch-and-goes: the hypothetical wire depends on a DCS event

`touchdown_reference` in `finish()` is `landing_time`, else `deck_crossing_time`, else the last datum. With no arrest onset, the crossing fallback requires the reference to be within `SAMPLE_GAP_WARNING_MS` (300 ms) of the last crossing before it.

| Pass | Hook | Last crossing | `runway_touch` | Lag | Result |
|---|---|---|---|---|---|
| 18 Sept 20:38 | up | 4-wire 9222.52 | none | reference was the deck crossing, inside 300 ms of the 2-wire | "would have caught wire 2" |
| 18:52 | up | 4-wire 2851.59 | 2852.63 | 1,050 ms | none |
| 18:55 | up | 4-wire 2996.85 | 2997.86 | 1,010 ms | none |
| 20:43 | up | 4-wire 9529.68 | 9530.26 | 580 ms | none |

Same code, same kind of pass, different text on Discord, decided by whether DCS emitted an event for a hook-up deck contact (it did on all three of the 20th, on none of the 18th). If the hypothetical wire is wanted at all, it should be anchored on the geometry (the crossing at which the hook point was lowest, or the wheels' contact point) and not on the reference time. On the 20th the wheels were on the deck (altitude 0.0) at the 3-wire on 18:52 and 18:55 and at the 2-wire on 20:43. Low priority; the text is cosmetic and carries no points.

### 7. Grades and the other findings of the 18th

`grade-ab` on the folder: all nine recorded grades equal the `CONVENTION` (P9) column. The 20 September rows are decided by lineup at the start (09:04, 18:52: gross left, 13 and 5.8 degrees at roll-out, corrected with an "average" correction), AoA at the ramp (18:55, 19:10, 20:43: moderate, `(OK)`), glideslope at the ramp (18:57), lineup in the middle (20:40, 3.7 degrees left for 11.5 s) and lineup at the ramp (20:46, 3.9 degrees left, 17.9 s). The pilots will recognise these: three of the four `--` are lineup passes, flown well left and drifting right through the groove, and DCS's own `C` on 20:46 (`_LULX_ _LULIM_ _LULIC_`) says the same.

The two DCS marks of `---` on `(OK)` passes (18:57 and 19:10) and the `---` on our `--` (20:40) are not compared further here; the DCS strings are in `dcs.log` lines 64506, 65504, 71123 and 71396 for whoever wants to.

The findings of the 18th check out on the 20th: the `NC : No proper communications` comment appeared again for the one pilot who had not checked in (09:04), the pilots who had checked in received marks 1.2 to 1.4 s after `runway_touch`, the `land` event 1.0 to 2.0 s later was rejected as a duplicate on every trap, and the waterline sentinel hit 3 of 9 passes with the low probe at 0.0 or -0.0 m. The 19 September changes (`WO(P)`, `WO`/`OWO`) were not exercised: no wave-off of any kind was flown.

### 8. What was checked, what was not

| Check | Result |
|---|---|
| `git diff 3c4c6b4 c818d66` | wire estimate, hook sampler, deck kinematics untouched |
| `grade-ab` on the 13 reports | 13 of 13 equal to the recorded grade under P9 (the 18th's 20:30 reads `WO(P)` in every replayed column, as documented on the 19th) |
| Stop position on every trap of the four sessions | section 2: 12 of 12 against DCS for Tomcat and Hornet |
| Deceleration onset on every trap | section 2: 52.9 to 59.0 m on the Tomcat, 53.4 to 53.8 on the Hornet |
| Hook timelines of the six traps | section 3 |
| `dcs.log` LSO lines | 5 (4 marks, 1 `NC`); nothing for 09:25 |
| Client CSV, `align_aoa.py` | not run: no recorder file |
| Code | nothing changed |

### 9. Recommendations

**1. Let a hook-transient non-answer fall through.** What it is: section 3; `wire_estimate_with` returns the hook method's "not correlated" verdict as final and never asks the stop position. On the board: "Rust estimate unavailable" on a trap where the aircraft's stop names the wire to within a metre. Decides 18:57 and 09:04 of this session, two 14 September traps; nothing that currently names a wire moves. Two lines plus a test.

**2. Give the Hornet the 87 m run-out.** What it is: section 4; two live Hornet traps at 88 and 89 m. On the board: 19:10 reads 3 with DCS instead of "Rust estimate 2" flagged divergent. Decides 19:10 and the 14 September Ducks trap. One line and a fixture check.

**3. Add a straight-in groove entry.** What it is: section 5; the Case I roll-out detector needs a turn to arm and a straight-in has none. In the cockpit: a pilot who flies a clean straight-in gets no points, no AoA series and a wire from the least trusted method. Decides 09:25. Design first, then implement.

**4. Fly the windy session.** Unchanged from the 15th and the 18th: client recorder on, hook down, 15 to 20 knots of deck wind. This session had the hook down on six passes, which is progress, and no recorder and no wind, which is not.

### 10. Implemented on 20 September: the fall-through, and the Hornet constant tried and backed out

Branch `feature/wire-estimate-fallthrough-20260920`, off `feature/approach-only-and-owo`. Nothing is committed.

| File | Change |
|---|---|
| `src/track.rs` | `Track::wire_estimate_with` keeps a hook-transient result only when it names a wire; otherwise the new `wire_estimate_from_stop_or_crossings` runs (stop position on a hook-down confirmed stop, then the crossing selection) and the transient's `hook_deflection_time_dcs`, `hook_recovered_time_dcs` and `correlation_lag_ms` are copied onto whatever it returns, so the report still shows what the hook did. `WireEstimateEvidence` field comments updated. Two tests: the 18:57 shape (deflection 214 ms after the 3-wire, 6 ms before the 4-wire, stop -97.5 m) must give 3 by the stop position and, without a confirmed stop, must reach the crossing selection instead of the transient's non-answer; the 09:04 shape (deflection 387 ms before the first crossing, stop -79.9 m) must give 2 |
| `src/data.rs` | the Hornet keeps `arresting_run_out_m: None`; the comment now records the two live traps (88 and 89 m) and why the constant stays off (below) |
| `docs/GRADING_REFERENCE.md`, `docs/HOW_GRADING_WORKS_PLAIN_LANGUAGE.md`, `CHANGES.md` | the three-method order and the fall-through |

| Check | Result |
|---|---|
| `cargo test --locked` | 360 + 3 passed, 1 ignored (the fixture table test, as before); the two new tests included |
| `cargo clippy --locked --all-targets -D warnings`, `cargo fmt --check`, `git diff --check` | clean |
| `lso file` on the LSO-written ACMIs, release build | 18:57: `wire estimate from stop position wire=3 residual 0.25 m`; 09:04: `wire=2 residual 1.87 m`; 18 September 20:41: `wire=3 residual 0.17 m`, unchanged. (The ACMIs carry no hook samples, so the offline replay never sees a transient; the live hook path is covered by the 14 live fixtures with hook sidecars, all passing) |
| `grade-ab` | does not replay the wire estimate; grades unchanged by construction (the wire is not a grading input) |

**The Hornet constant, tried and backed out.** With `Some(87.0)` the `wire_4_01_FA18C` fixture test fails: that recording (an older carrier and DCS build) stopped 82.6 m past the landing point after catching the 4-wire, a 60 m run-out, and `stop + 87` lands 5.3 m from the fixture's 1-wire plane, inside the 6 m residual guard, so the stop position names it "1-wire" over the crossing selection's correct 4. The guard cannot be tightened to exclude it: live residuals reach 3.9 m on the September fixtures. A deceleration-onset cross-check (53 to 59 m past the wire on every live Tomcat and Hornet trap) would not have helped either, because that fixture records no onset at all. So either the fixture is from a DCS build whose Hornet ran 60 m and the live pair (88, 89 m) is what this server does, or the Hornet's run-out really varies; one more live Hornet trap with a DCS mark decides it. Until then 19:10-style passes keep "Rust estimate N" from the crossing selection, flagged divergent when DCS disagrees.

On the board: a Tomcat trap where the hook jerked outside the 200 ms window now reads "Wire #N (Rust estimate)" from where it stopped, instead of "Rust estimate unavailable". Nothing that already named a wire moves, hook-up passes are untouched, and the T-45 and Hornet still skip the stop position.
