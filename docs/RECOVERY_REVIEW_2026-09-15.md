# Recovery of 15 September 2026: the first session under CONVENTION, the calibration re-checked, and a bug in the AoA axis

Two pilots, two aircraft types, 23 passes on CVN-72, recorded by LSO 0.5.0 at `4a2af25` (clean, `CONVENTION` as the production default) against DCS-gRPC 0.10.0 in buffered mode at 20 Hz, with the read budget raised to 40 per second after the 14 September finding. Both pilots flew with the client recorder (`tools/aoa_calibration/LsoAoaExport.lua`) again, so every pass has the flight model's true AoA and the cockpit indexer next to the LSO's own numbers.

Source folder: `tools/aoa_calibration/recovery_aoa_calibration2/` (23 JSON reports, charts, ACMI, `lso.db`, `lso.log`, `dcs.log`, `gRPC.log`) plus the two client logs `lso_aoa_20260915-203504.csv` (Ghost-72 | TT, T-45C then F-14B(U)) and `lso_aoa_20260915-205125.csv` (Justice, F-14B(U)). Raw tool output: `docs/AOA_CALIBRATION_2026-09-15.md`. Previous documents: `docs/AOA_CALIBRATION_REVIEW_2026-09-14.md` (the calibration), `docs/GRADING_CONVENTION_PROTOTYPE_2026-09-15.md` (the policy now in production). Times in this document are UTC; the file names in the folder are local time, two hours later.

Part 1 is written for everyone. Part 2 has the numbers and the method.

---

## Part 1. In plain language

### What we did

Same set-up as 14 September, one day later: normal Case I pattern, the LSO recording on the server, the small recorder on each pilot's PC writing the true AoA and the three indexer lamps twenty times a second. Three things were new. The grader ran the `CONVENTION` policy live for the first time. The server's read quota had been raised so that two pilots in the pattern would no longer saturate it. And the pilots were asked to fly ordinary passes rather than hold the chevrons, which is what the 14 September review had asked for before trusting the grade distribution.

### What we found

**1. The calibration holds.** The cockpit lamp thresholds measured today are the same as yesterday's to within 0.05 degrees on both types, and the LSO's computed AoA sits on the flight model's value with a median difference of 0.00 degrees on the T-45 and 0.00 on the F-14 over 8,240 groove samples. In the whole session there was not a single stretch of half a second or more where the program called "fast" or "slow" while the cockpit showed the donut alone. Nothing changes in the bands.

**2. The grades came out as the convention predicts, and the re-grader reproduces all 23.** Ten passes are `(OK)`, four are `--`, five wave-offs, four bolters. No pass reached `OK`. Compared with the previous policy the convention raised seven passes from `--` to `(OK)` and lowered none, the same direction as the before-and-after of yesterday.

**3. But four of those ten `(OK)`s were wrong, because of a bug in the AoA axis.** The grader measures each AoA reading as a distance to the edge of the on-speed band. The routine that finds that edge walked toward the band in doubling steps (0.25, 0.5, 1, 2, 4 degrees) and, when a step jumped clean over the band, gave up. The caller then dropped that reading from the series as if it had never been sampled. The readings it dropped are not random: on the F-14 every value between 6.8 and 7.95 degrees and between 12.8 and 13.95, on the T-45 every value between 4.75 and 6.25 and between 6.75 and 7.25. Those are exactly the readings 2 to 3 degrees outside the band, the ones the "gross AoA" rule of the convention was written for. In practice the gross tier only fired when a pilot was more than about 3 degrees off, and a Tomcat crossing the ramp at 7.1 degrees (fast chevron, about 12 knots over approach speed) was graded as a moderate 7.98.

The fix is one function (`normalized_aoa_error` in `src/grading.rs`): fixed 0.1 degree steps, which cannot jump over the narrowest band. A test now sweeps every reading from 0 to 25 degrees on all four carrier types. Re-grading everything recorded so far moves eight passes from `(OK)` to `--`: four of today's, three of 14 September, one of 13 September. Three of the eight are real, sustained gross excursions the old code could not see (today's 19:17, yesterday's 19:00, the 18:26 of the 13th). The other five cross the 2 degree line by 0.1 to 0.4 degrees for one to four samples, mostly in the last second before the wheels touch. That leads to the next point.

**4. The gross tier needs its own persistence rule.** The one-second rule of the prototype applies to a whole AoA episode. A three-second "slow at the ramp" that touches the gross line for a single sample at the moment of touchdown counts as gross for the whole episode, and gross at the ramp is a `--` whatever the pilot did. Today's 19:03 (Ghost-72, T-45, 3-wire) is the clearest case: the AoA collapses from 9.5 to 2.8 degrees as the nose drops onto the deck, the DCS touchdown event arrives 0.7 seconds after the wheels, and those 0.2 seconds of rollout are what the fixed grader calls "gross fast at the ramp". The proposed rule: gross readings must themselves last one second before an episode counts as gross, otherwise it is moderate. On the recorded passes that rule keeps the five real gross cases and returns the five borderline ones to the grade the convention gives a moderate deviation. Not implemented here; it is a policy choice and the numbers are in Part 2, section 5.

**5. Three passes deserve a look by a pilot, whatever the grader says.**
- **19:17, Ghost-72, F-14, "wire 1".** Fast chevron for 74% of the groove, 2.2 degrees high at the ramp, then a push-over: 7.2 m/s of sink (1,400 ft/min) one and a half seconds before the deck, at 171 knots ground speed. The aircraft stopped on the deck with a 2.3 g deceleration, so it did catch a wire, but DCS issued no landing mark, and the "wire 1" the report shows is the first hook-plane crossing, not a measurement. The deck kinematics say **2-wire**: on every Tomcat trap where DCS did name the wire, the aircraft stopped 85 to 89 m past it, and this one stopped 85 m past the 2-wire, 26 m short of where the two known 4-wire traps stopped (section 6). The pilot's recollection of a 4-wire is understandable: the wheels touched the deck beyond the 4-wire, after the hook had already caught the 2-wire in the air. The recorded grade was `(OK)`; with the fix it is `--`. A real LSO would write it `--` or `C`.
- **19:23, Justice, F-14, 1-wire.** High the whole groove (1.0 to 1.2 degrees), then settled through the glideslope to 1.0 low at the ramp, slow chevron the last two seconds, sink rate 6 m/s. DCS wrote `C`. Recorded `(OK)`; `--` with the fix, on a gross reading that lasts 0.3 seconds at the moment of touchdown. The convention says a moderate deviation with an average correction at the ramp is `(OK)`, and by the letter that is what this pass is; a 1-wire from high is the kind of pass the written convention leaves to the LSO's judgement.
- **19:03, Ghost-72, T-45, 3-wire.** DCS wrote `C` with "low in close, low at the ramp, three-point landing in the wires". Our grader sees 1.6 degrees low at 70 m (moderate) and slow the last two seconds: `(OK)`. With the fix it becomes `--`, but for the wrong reason (point 4). Whether 1.6 degrees low at the ramp is "moderate" or "gross" is the glideslope threshold question in section 8.

**6. DCS's own LSO is still not a reference for AoA.** It wrote "slow all the way" on 19:23 (cockpit: fast 35%, donut 29%, slow 8%) and on 19:45 (donut 40%, fast 25%). But its ramp calls on 19:03 and 19:23 point at exactly the two passes where our grader was most lenient, and its "three-point landing" on 19:03 and 19:34 is confirmed by the true AoA collapsing before the touchdown event.

**7. Two operational details.** The Tomcat's indexer flashes when the hook is up in carrier mode, so on every hook-up F-14 pass half the recorder's samples show no lamp at all; the statistics use the lit samples and are unaffected, but a calibration brief should say "hook down". And the raised read quota works: with both pilots in the pattern for fifty minutes, 22 of 23 passes have green telemetry health and the recorders waited 2 to 3 milliseconds per pass for a read token, against 20 to 113 seconds yesterday.

### What changes

| Item | Status |
|---|---|
| AoA on-speed bands in `src/data.rs` | unchanged, confirmed a second time |
| `normalized_aoa_error` | fixed on this branch, two tests added |
| `docs/GRADING_REFERENCE.md` | glideslope "gross" threshold corrected to what the code does (2.5 degrees, not 1.5) |
| gross AoA persistence | proposed in section 5, **implemented the same evening** (section 13) |
| series end at the physical touchdown, last 100 m in height | proposed in section 7, **implemented the same evening** (section 13) |
| wire from the stop position, "would have caught" on hook-up passes | proposed in section 6, **implemented the same evening** for the F-14 (section 13) |
| the eight re-graded passes | listed in section 4; five of them return to `(OK)` with the rules of section 13; the convention document's tables carry a note |

### Recommendation

Merge the branch before the next session, then fly the ordinary session once more: the four grades that changed today were all decided in the last two seconds of the pass, and that is where the grader still needs a pilot's eye. The final grades of every recorded pass under the policy as it now stands are in section 13.

---

## Part 2. Technical

### 1. Data set

| Item | Value |
|---|---|
| Reports | 23 (`LSO-*.json`): 7 T-45C (Ghost-72), 16 F-14B(U) (7 Ghost-72, 9 Justice) |
| Outcomes | 7 arrestments (5 with DCS `WIRE#`, 1 kinematic without landing mark, 1 kinematic plus mark), 8 touch-and-go with hook up, 4 bolters, 5 wave-offs |
| Client CSV, Ghost-72 | 130,483 rows; Justice 60,933 rows; 191,416 in total |
| Groove samples matched | 1,801 T-45, 6,439 F-14 (LSO datum with a client sample within 0.3 s) |
| Clock offset client to server | -0.05 to -0.80 s, -0.60 ± 0.1 on 15 of 23 (same as 14 September) |
| Mission wind | 0.7 to 0.9 m/s from 091 at deck level, 4.2 to 5.5 m/s at 51 to 175 m; calm again |
| Wind probes | the deck probe returned the 180/0.0 sentinel on 5 passes (18:41, 18:56, 19:02, 19:29, 19:43) and was overridden by the high probe; the groove-entry fallback was used on 5 others (18:47, 18:56, 19:00, 19:21, 19:26) |
| Acquisition | 20.0 Hz on all 23; read budget 40/s; read-budget wait 2 to 3 ms per pass; health green on 22, orange on 18:49 (late delivery, max delivery age 310 ms) |
| Build | `lso_commit 4a2af25`, `lso_dirty false`, grading version `project-derived-v7`; every recorded grade equals the `grade-ab` P6 (`CONVENTION`) column |

### 2. Calibration re-check

`tools/aoa_calibration/align_aoa.py`, unchanged since 14 September.

**LSO computed minus flight-model true AoA, groove only, degrees.** Per pass the median runs from -0.17 to +0.18 (23 passes, n = 207 to 726 each), p10 to p90 widths 0.3 to 1.5. Per type:

| Type | n | median | mean | p10 | p90 | 14 September |
|---|---|---|---|---|---|---|
| T-45C | 1,801 | 0.00 | +0.01 | -0.32 | +0.36 | +0.02 / -0.32 / +0.44 |
| F-14B(U) | 6,439 | 0.00 | +0.03 | -0.41 | +0.50 | +0.07 / -0.43 / +0.56 |

The slope against bank stays within -0.14 and +0.04 degrees per degree of bank on 22 passes; the one outlier (19:26, -0.30) is a 10-second bolter flown between 1 and 7.7 degrees true AoA, where interpolation across the swing dominates. The five passes with a substituted or fallback wind reading have medians of -0.02 to +0.18, indistinguishable from the rest: at 0.9 m/s of deck wind a 4.5 m/s error moves the pitch-plane AoA by a quarter of a degree at most, as computed yesterday. The wind correction is still waiting for a windy session.

**Indexer thresholds**, true AoA at the lamp switch over the whole flight, both directions:

| Threshold | T-45C today | 14 Sept | switches | F-14B(U) today | 14 Sept | switches | in `src/data.rs` |
|---|---|---|---|---|---|---|---|
| fast chevron alone ends | 8.00 | 8.00 | 82 | 9.46 | 9.47 | 153 | 8.0 / 9.45 |
| fast chevron out, donut alone | 8.25 | 8.25 | 113 | 9.93 | 9.93 | 159 | 8.25 / 9.95 |
| slow chevron joins the donut | 8.75 | 8.75 | 116 | 10.84 | 10.82 | 111 | 8.75 / 10.8 |
| donut out, slow chevron alone | 9.00 | 9.00 | 92 | 11.30 | 11.27 | 74 | 9.0 / 11.25 |

Hysteresis 0.04 degrees on the T-45, 0.10 to 0.19 on the F-14, same as yesterday. Every code threshold is within 0.05 degrees of the measurement. True AoA while the cockpit showed each state, groove samples: T-45 donut alone 8.28 to 8.70 (p10 to p90, n = 288), F-14 donut alone 10.03 to 10.74 (n = 850).

**Agreement between cockpit and program** (exact, within one step): T-45 76% / 96% (85% / 97% yesterday, on a set with fewer transitions), F-14 81% / 97% (84% / 99%). The two ends agree at 91 to 96% (fast) and 71 to 79% (slow); the slow rows are small (55 and 343 samples) and their disagreements are the touchdown collapse, where the LSO's series drops about half a second before the lamp changes.

**Persistence check.** For every groove sample where the cockpit showed the donut alone, runs of the LSO rating "fast" or "slow" (the ratings that carry a medium severity): no run of 0.5 s or longer on any of the 23 passes (yesterday: longest 0.70 s T-45, 0.85 s F-14). The one-second rule (P2) has a margin of a factor two on this set. The reverse (LSO on speed while the cockpit shows a chevron alone) happened twice, 0.8 and 1.6 s, both on the steep AoA changes of 19:04 where the server's series lags by the clock offset.

**Hook-up F-14 passes.** The Tomcat's indexer flashes with the hook up and the hook-bypass switch in CARRIER, so the recorder sees all three lamps off on alternate samples:

| Hook | Passes | Lamp-off share of groove samples |
|---|---|---|
| F-14 up | 18:56, 18:59, 19:02, 19:10, 19:15, 19:18, 19:39, 19:43 | 50 to 54% |
| F-14 down | the other 8 | 2 to 8% (the samples after touchdown) |
| T-45 (any) | all 7 | 0 to 4% |

Per-state shares and thresholds are computed on lit samples only, so nothing above is affected; the F-14 threshold counts above come from the hook-down passes and from Ghost-72's flight. The recorder could carry the hook-bypass argument next time.

### 3. The 23 passes

Cockpit shares are of lit groove samples, from the client recorder. "Recorded" is the grade the server wrote (`CONVENTION` as switched on that morning); "fixed" is the same policy with the `normalized_aoa_error` fix of section 4 and nothing else. The deciding episode is that grader's. The grades under the policy as it stands at the end of the day, with the three rules of section 13, differ on five rows (19:03 and 19:23 back to `(OK)`, 19:43 to `OK`, 19:15 `--` on a different episode, 19:02 `--` on a different episode); section 13 lists them.

| UTC | Pilot | Type | Outcome | DCS LSO | Cockpit indexer | True AoA p10 to p90 | Recorded | Fixed | Deciding episode |
|---|---|---|---|---|---|---|---|---|---|
| 18:41 | Ghost-72 | T-45 | T&G, hook up | none | fast 72%, sl. fast 13%, donut 10% | 7.2 to 8.5 | `(OK)` | `(OK)` | fast (7.25) 10.4 s in the middle, average; fast 2.85 s at the ramp, average; lineup 3.2 left at entry, good |
| 18:45 | Ghost-72 | T-45 | Bolter | none | fast 73%, sl. fast 21% | 7.4 to 8.2 | `B` | `B` | fast (7.26) 11.3 s in close |
| 18:47 | Ghost-72 | T-45 | Wave-off | `WO _LULIM_ _LULIC_ _TMRDIC_ _LULX_ WO(AFU)IC` | fast 89% | 6.5 to 8.0 | `WO?` | `WO?` | go-around |
| 18:49 | Ghost-72 | T-45 | Trap 1 | `--- _TMRDAR_ (EGTL)` | fast 99% | 5.9 to 7.4 | `--` | `--` | gross fast (4.9) 2.85 s from in close to the ramp, average (was: moderate, poor) |
| 18:56 | Justice | F-14 | T&G, hook up | none | fast 35%, donut 26%, sl. fast 23%, sl. slow 10%, slow 7% | 8.8 to 11.2 | `(OK)` | `(OK)` | slow (11.4) 3.1 s at the ramp, good; lineup 3.45 left at the start, good |
| 18:59 | Justice | F-14 | Wave-off | `OWO _LULIM_ LOIM LOIC _DRIC_ (LURIC) WO(AFU)IC` | fast 76% | 7.2 to 10.9 | `WO?` | `WO?` | go-around |
| 19:00 | Ghost-72 | T-45 | Bolter | none | fast 82%, sl. fast 16% | 6.9 to 8.1 | `B` | `B` | fast at the ramp |
| 19:02 | Justice | F-14 | T&G, hook up | none | donut 52%, sl. slow 41%, slow 7% | 10.3 to 11.2 | `--` | `--` | gross high (2.66) at the ramp, good; lineup 3.0 right in close, good |
| 19:03 | Ghost-72 | T-45 | Trap 3 | `C _LULIM_ _TMRDIC_ _LOIC_ _PIC_ _PPPIC_ _LOAR_ 3PTSIW` | donut 42%, sl. slow 21%, slow 19%, sl. fast 13% | 8.2 to 9.2 | `(OK)` | `--` | 1.59 low at 70 m, medium, average; slow (9.5) last 2 s; "gross fast" = 0.2 s of touchdown collapse (section 5) |
| 19:04 | Justice | F-14 | Trap 2 | `--- _SLOX_ _TMRDIC_ LOAR` | slow 45%, donut 38% | 10.2 to 13.0 | `--` | `--` | gross slow (14.0) 2.55 s at the ramp, average |
| 19:10 | Justice | F-14 | Wave-off, hook up | none | fast 100% | 5.0 to 8.0 | `WO?` | `WO?` | go-around; gross fast 20 s |
| 19:15 | Justice | F-14 | T&G, hook up, no touchdown event | none | slow 40%, sl. slow 22%, donut 21%, sl. fast 11%, fast 7% | 9.5 to 12.1 | `(OK)` | `--` | slow 6.3 s at the ramp rising to 13.2, gross for 0.9 s at the end; slow, fast, slow through the groove |
| 19:17 | Ghost-72 | F-14 | "Wire 1 (estimate)", no DCS mark | none | fast 74%, sl. fast 18% | 7.5 to 9.7 | `(OK)` | `--` | gross fast (7.1) 1.9 s at the ramp; 2.2 high at the ramp; sink 7.2 m/s (section 6) |
| 19:18 | Justice | F-14 | T&G, hook up | none | donut 52%, sl. slow 18%, slow 18% | 9.8 to 11.4 | `(OK)` | `(OK)` | slow (11.7) 3.2 s at the ramp, average; lineup 2.5 right for 15 s, corrected in the last 3 s; 1.54 high at 86 m |
| 19:21 | Justice | F-14 | Wave-off | none | fast 95% | 6.0 to 9.2 | `WO?` | `WO?` | go-around; gross fast 11 s |
| 19:23 | Justice | F-14 | Trap 1 | `C WX _DRX_ _LURX_ _SLOX_ (LURIM) _DRIM_ _EGIW_` | fast 35%, donut 29%, sl. fast 25%, slow 8% | 8.8 to 11.0 | `(OK)` | `--` | slow (12.6 to 13.0) 3.35 s at the ramp, gross for 0.3 s at touchdown; high 1.2 for 15 s then 1.0 low at the ramp; sink 6.2 m/s (section 6) |
| 19:26 | Ghost-72 | F-14 | Bolter | none | fast 100% | 4.2 to 7.7 | `B` | `B` | gross fast 8 s |
| 19:29 | Ghost-72 | F-14 | Bolter | none | fast 100% | 4.7 to 7.1 | `B` | `B` | gross fast 34 s; lineup 9.4 left at the start |
| 19:31 | Ghost-72 | F-14 | Wave-off | `WO LULX _FX_ _LOIC_ _PIC_ _PPPIC_ _LULIM_ _TMRDIC_ WO(AFU)IC` | fast 73%, slow 20% | 6.6 to 12.9 | `WO?` | `WO?` | go-around |
| 19:34 | Ghost-72 | F-14 | Trap 4 | `C 3PTSIW (EGIW)` | fast 91% | 4.7 to 9.2 | `--` | `--` | gross fast (4.0) 8.75 s in close and at the ramp; 2.4 high at the ramp; 171 kt at the wires |
| 19:39 | Justice | F-14 | T&G, hook up | none | donut 72%, sl. slow 18% | 10.0 to 11.1 | `(OK)` | `(OK)` | slow (11.35) 1.15 s at the ramp, good |
| 19:43 | Justice | F-14 | T&G, hook up | none | donut 59%, sl. fast 26% | 9.5 to 10.8 | `(OK)` | `(OK)` | 0.86 high at the ramp, small, "trajectory ended" (section 7) |
| 19:45 | Justice | F-14 | Trap 2 | `--- _SLOX_ _TMRDAR_ _EGIW_` | donut 40%, fast 25%, sl. fast 18% | 9.2 to 11.2 | `(OK)` | `(OK)` | slow (12.2) 2.45 s at the ramp, average; fast (8.6) 7 s in the middle, good |

Reading it:

- The convention did what the before-and-after predicted: over `PROTOTYPE` (P3) it raised 18:41, 19:03, 19:15, 19:17, 19:18, 19:23 and 19:45 by one grade and lowered none. P4 alone (the table without the gross tier) would also have raised 19:04 and 19:34, both flown on a chevron for 45 to 91% of the groove; the gross tier is what keeps them `--`, as on 14 September.
- Justice's six hook-up passes are the ordinary flying the 14 September review asked for: donut 52 to 72% on four of them, moderate slow at the ramp on five. Under the convention all six are `(OK)`; none reaches `OK` (section 7).
- Ghost-72's F-14 passes were flown fast (74 to 100% chevron, true AoA 4 to 8 degrees, 165 to 171 knots at the wires on the two traps) and both traps are `--` on the gross tier, one of them only once the tier can see 7.1 degrees.
- The DCS `SLOX` on 19:23 and 19:45 is contradicted by the cockpit (fast 25 to 35%, donut 29 to 40%); its `3PTSIW` on 19:03 and 19:34 agrees with the true AoA dropping to -2.5 and +1 degrees before the touchdown event.

### 4. The bug: readings 2 to 3 degrees outside the band were dropped

`normalized_aoa_error(plane, aoa)` in `src/grading.rs` returns the signed distance from a reading to the nearest edge of the on-speed band, using only the type's `aoa_rating` closure. It searched for a point inside the band by stepping toward it with `step = 0.25` doubled each time (0.25, 0.5, 1, 2, 4, 8 ...), and returned `None` when none of those 32 candidates rated on speed. `classify_catobar_episodes` used `filter_map` on that result, so a `None` reading was dropped from the AoA series: no sample, no severity, no peak.

The candidates from a reading `a` on the fast side are `a + 0.25·2^k`; the reading is kept only if one of them falls inside the band. With the calibrated bands:

| Type | On-speed band | Readings kept (fast side) | **Dropped** | Readings kept (slow side) | **Dropped** |
|---|---|---|---|---|---|
| F-14 | 9.95 to 10.8 | 9.7 to 9.95, 8.95 to 9.8, 7.95 to 8.8, 5.95 to 6.8, 1.95 to 2.8 | **8.8 to 8.95, 6.8 to 7.95, 2.8 to 5.95** | 10.8 to 11.8, 11.95 to 12.8, 13.95 to 14.8 | **11.8 to 11.95, 12.8 to 13.95, 14.8 to 15.95** |
| T-45C | 8.25 to 8.75 | 8.0 to 8.25, 7.25 to 8.0, 6.25 to 6.75, 4.25 to 4.75 | **6.75 to 7.25, 4.75 to 6.25, 2.75 to 4.25** | 8.75 to 9.25, 9.75 to 10.25, 10.75 to 11.25 | **9.25 to 9.75, 10.25 to 10.75** |

In the cockpit: on the F-14 the dropped fast window is the gauge between about 10.5 and 11.7 units (fast chevron, 10 to 15 knots over), the dropped slow window 16.7 to 17.9 units (slow chevron, 8 to 12 knots under). On the T-45 the dropped windows are 12.4 to 14.3 units and 14.9 to 15.5 units (fast chevron, 5 to 15 knots over). The gross tier (`aoa_large_error_deg` = 2.0) begins at 7.95 / 12.8 on the F-14 and 6.25 / 10.75 on the T-45, so on the F-14 it could only see gross readings below 6.8, above 13.95, or in the 1.95 to 2.8 window; the 14 September passes "at 6.0" fell in the 5.95 to 6.8 window by luck.

Within an episode a dropped sample does not end the episode (episodes end on two consecutive on-speed samples, and dropped samples are simply absent), so durations were right and peaks were wrong. The 14 September table said "fast (8.0) 4.3 s at the ramp" for a pass whose true AoA sat at 6.8 to 7.2 for six seconds, cockpit fast chevron 80%.

**Fix.** Fixed steps of 0.1 degrees (600 at most), then the existing bisection. The narrowest band, the T-45C's 0.5 degrees, cannot be jumped over. Test `normalized_aoa_error_reaches_the_band_from_any_reading` sweeps 0 to 25 degrees in 0.05 steps on the T-45, F-14B, F-14B(U) and F/A-18C and checks sign and edge; `gross_aoa_tier_applies_only_beyond_the_policy_distance` gains 7.2 and 13.2 on the F-14 as `Large`.

**Effect on every recorded pass, `CONVENTION` column of `grade-ab`:**

| Set | Pass | Before | After | Gross readings in the grader's window (raw datums, seconds, peak) | Verdict |
|---|---|---|---|---|---|
| 15 Sept | 19:03 T-45 trap 3 | `(OK)` | `--` AoA ramp | 0.05 s and 0.15 s at 1.2 and 0.9 s before the event, 5.3 | touchdown collapse (section 5) |
| 15 Sept | 19:15 F-14 T&G | `(OK)` | `--` AoA ramp | 0.90 s at the end of the series, 13.2 | borderline: slow chevron 3.2 s, gross by 0.4 |
| 15 Sept | 19:17 F-14 "wire 1" | `(OK)` | `--` AoA ramp | 1.90 s at 3.5 s before touchdown, 7.1 | real |
| 15 Sept | 19:23 F-14 trap 1 | `(OK)` | `--` AoA ramp | 0.10 s and 0.20 s at 1.6 and 1.4 s before the event, 13.0 | touchdown |
| 14 Sept | 19:00 F-14 T&G | `(OK)` | `--` AoA ramp | 4.40 s at 11.9 s, 1.90 s at 3.2 s before touchdown, 6.8 | real (cockpit fast 80%) |
| 14 Sept | 19:38 F-14 T&G (Justice) | `(OK)` | `--` AoA ramp | 0.10 s at 1.5 s before the event, 13.0 | touchdown |
| 14 Sept | 19:41 F-14 trap 2 | `(OK)` | `--` AoA ramp | 0.20 s at 1.5 s before the event, 12.9 | touchdown |
| 13 Sept | 18:26 T-45 trap 2 | `(OK)` | `--` AoA in close | 0.90 s and 1.45 s at 15.7 and 10.0 s before touchdown, 5.5 | real (2.75 under the band, 11.75 s episode) |

Two passes already `--` change their deciding episode from moderate to gross (15 Sept 18:49: 2.85 s at 4.9 degrees; 14 Sept 19:04: 13.55 s at 2.4). No other grade moves in the 53 recorded passes, and the P0 to P3 columns move with the P6 one wherever the gross tier is off, since the dropped readings also shortened moderate episodes there.

### 5. Gross AoA needs one second of gross readings

Under P2 an AoA episode needs one second to grade. The severity of an episode is the maximum over its samples, so a single gross sample inside a three-second moderate episode makes the episode gross, and gross at the ramp is `--` in every cell of the convention table. Five of the eight passes of section 4 are decided that way, four of them by readings taken after the wheels were on the deck: the DCS touchdown event (`runway_touch`) arrives 0.6 to 0.9 s after the sink rate reverses in the trajectory, the AoA series runs to the last trajectory sample (0.85 to 1.5 s before the event), and the flight model's AoA collapses as the nose comes down. On 19:03 the true AoA goes from 9.5 to -2.5 degrees between 1.1 and 0.6 s before the event, the cockpit lamp switches from slow to fast, and the LSO's series reads 6.9 then 2.8: "gross fast at the ramp".

**Proposed rule (not implemented):** an episode's maximum severity is `Large` only if its gross samples span at least `aoa_min_episode_duration_s` (1.0 s); otherwise `Medium`. In the cockpit: the fast or slow chevron alone, 2 degrees past the donut edge, held for a full second (F-14: below 7.95 or above 12.8 degrees; T-45: below 6.25 or above 10.75), which is 10 to 15 knots off approach speed for a second, not a needle flick at touchdown.

What it decides on the recorded passes, gross runs measured on the raw datums inside the grader's window:

| Keeps gross (`--`) | Gross run | Returns to moderate | Gross run |
|---|---|---|---|
| 15 Sept 19:17 | 1.90 s | 15 Sept 19:03 | 0.15 s |
| 15 Sept 18:49 | 2.85 s | 15 Sept 19:23 | 0.20 s |
| 15 Sept 19:04 | 2.55 s | 15 Sept 19:15 | 0.90 s |
| 15 Sept 19:34 | 8.75 s | 14 Sept 19:38 Justice | 0.10 s |
| 14 Sept 19:00 | 4.40 s | 14 Sept 19:41 | 0.20 s |
| 13 Sept 18:26 | 1.45 s | | |
| all 14 September chevron passes | 2.2 to 20 s | | |

The five that return to moderate go back to `(OK)` under the convention (moderate at the ramp, average or good correction), which is what the recorded grade was. 19:15 at 0.90 s is the closest call, and it is a pass with no touchdown event at all, so its series ends where the trajectory happens to stop.

A second, complementary fix is to end the AoA series at the physical touchdown (the sink-rate reversal, or the hook and weight-on-wheels samples the report already carries) instead of at the last trajectory sample before the DCS event. That would remove the collapse from all axes; the glideslope angle in the last 30 m has the same problem (section 7).

### 6. Two passes that need a pilot's eye

**19:17, Ghost-72, F-14, "Wire #1 (Rust estimate)".** True AoA 8.0 to 8.5 in close for eight seconds (fast chevron, 1.5 to 2 degrees under), 7.2 to 7.5 the last three seconds. Glideslope +0.4 in the middle rising to +2.06 at 93 m (high at the ramp), then a push-over: sink rate 5.0, 6.2, 7.2, 6.4 m/s over the last 2.5 s (the cut threshold is 8.0 m/s sustained). Ground speed at the touchdown event 69 m/s (134 knots) plus 0.9 m/s of wind, IAS 78.8 m/s (153 knots) in the CSV. The hook (down, arg 1.0) crossed the four wire planes at 3151.43 to 3152.03, the `runway_touch` event is at 3152.77, and the deck kinematics confirm a stop (relative speed 0.01 m/s, held 2 s, deceleration onset 3152.53): the aircraft caught a wire, DCS issued no landing mark, and the "wire 1" is the first plane crossing, 1.3 s before the touch, with the aircraft still 1.15 m above the deck.

**Which wire.** The wire planes sit at x = +14 (1), +3 (2), -10 (3) and -22 m (4) in the report's deck frame. Two server-side positions identify the engaged wire without any clock alignment, calibrated on the seven Tomcat traps of 14 and 15 September where DCS reported the wire:

| Trap | DCS wire | Deceleration onset x | Stop x (`deck_kinematics.x_at_slow_m`) | Onset past the wire | Stop past the wire |
|---|---|---|---|---|---|
| 14 Sept 19:38 Ghost-72 | 1 | -42.0 | -71.5 | 59 m | 88 m |
| 15 Sept 19:23 Justice | 1 | -40.1 | -71.1 | 54 m | 85 m |
| 14 Sept 19:41 Justice | 2 | -52.6 | -83.5 | 55 m | 86 m |
| 15 Sept 19:04 Justice | 2 | -52.3 | -84.8 | 55 m | 87 m |
| 15 Sept 19:45 Justice | 2 | -51.5 | -84.9 | 53 m | 87 m |
| 14 Sept 19:04 Ghost-72 | 4 | -78.2 | -111.5 | 53 m | 86 m |
| 15 Sept 19:34 Ghost-72 | 4 | -77.6 | -111.5 | 55 m | 89 m |
| **15 Sept 19:17 Ghost-72** | **none** | **-52.3** | **-85.2** | **wire at +2.7** | **wire at +2 to +4** |

The arresting engine's run-out is constant in DCS, 85 to 89 m past the wire whatever the entry speed (58 to 76 m/s in this table), and the deceleration-onset detector fires 53 to 59 m past it. Both put 19:17 on the **2-wire**, within 2 m of the three known 2-wire traps and 26 m short of the two known 4-wire traps. The client recorder adds the sequence: the hook animation dips at client time 3151.47 while the aircraft is still descending at 7 m/s (hook touched the deck and skipped), rebounds to 0.97, comes down for good at 3151.87 to 3151.98 and stays at 0.16 to 0.27 (the Tomcat's loaded-hook reading), and the true airspeed falls from 71 to 58 m/s over the next half second. Hook first, wire second, wheels last: an in-flight engagement, which is why the wheels came down beyond the 4-wire and the pilot remembers a 4-wire. The DCS touchdown event (x = -64 m, 25 m past the 4-wire plane) records the wheels, not the hook.

The estimator wrote "wire 1" because, with no hook-deflection correlation, `continuous_hook_plane_crossing` (`src/track.rs`) picks the earliest crossing not more than `WIRE_ARREST_ONSET_TOLERANCE_S` (1.2 s) before the deceleration onset. The onset is detected 53 to 59 m past the wire, 0.8 to 1.0 s at approach speed, and the hook sweeps all four planes in 0.6 s, so every crossing is inside the tolerance and the rule returns the 1-wire on any trap without a landing mark (it was right on 19:23 by coincidence: that one was a 1-wire). The stop position carries the information the crossing sequence does not: `x_at_slow_m + 87 m` names the wire to within 2 m on eight of eight traps, and the onset position (`+ 55 m`) does the same. Whatever the wire, the pass is a high, fast dive at the ramp; the fixed grade `--` stands on the AoA alone and the glideslope agrees (2.20 high at the ramp, "medium" under the code's 2.5 threshold).

**19:23, Justice, F-14, 1-wire, DCS `C`.** Glideslope +0.83 at the start, +1.0 to +1.2 through the middle and in close (14.85 s, "medium good" because it came back), through zero at 2 s before the event and -1.04 at 1.5 s (31 m), sink rate 5.2 to 6.2 m/s from 3 s out. AoA: fast chevron 8.5 to 9.0 in the middle for 5.7 s, donut for 7 s in close, slow chevron the last 2 s rising to 12.6 true (13.0 in the LSO's series for 0.3 s at touchdown). DCS's `_SLOX_` is wrong on the groove and right at the ramp; its `_DRX_ _LURX_` (drift right, lined up right all the way) is not in our lineup series (-0.8 to +0.3). The convention's letter gives `(OK)`: every deviation is moderate, every correction average or good. The fixed grader gives `--` on 0.3 s of gross at touchdown. A human LSO looking at a 1-wire settled from high with the slow chevron and 6 m/s of sink would write `--` on judgement, which is the one thing the table does not encode.

### 7. The last thirty metres

No pass reached `OK`. The closest is 19:43 (Justice, hook up): donut 59%, slightly fast 26%, glideslope within ±0.4 degrees to 95 m, lineup within 0.8. Its deciding episode is "0.86 degrees high at the ramp, small, trajectory ended before the correction could be assessed", which under the convention is `(OK)`. The samples behind it: +0.40 at 95 m, +0.74 at 65 m, +0.83 at 35 m, +0.86 at 5 m. In height that is 0.66, 0.84, 0.51 and 0.07 m above the glideslope, two to three feet, a normal 3-wire. The angular deviation is kept in degrees down to 5 m from the landing point, where a foot is ten degrees, and a "small" episode at the ramp that the trajectory ends before it can be corrected is `(OK)` by the table. The same geometry explains the "medium good" glideslope episodes at the ramp on 19:23 (-1.04 at 31 m, then -0.14 at 3 m: the "correction" is the aircraft reaching the deck).

Two ways to make `OK` reachable without loosening anything: convert the glideslope error to height (or clamp the angle) inside the last 50 to 100 m, and end all three series at the physical touchdown (section 5). Both are measurement, not policy.

### 8. Glideslope thresholds: the doc said 1.5, the code says 2.5

`gs_severity` in `src/grading.rs` sizes a glideslope deviation as small from 0.5, medium from 1.0 and large from 2.5 degrees, and has since the episode grader was introduced (`07741b8`). `docs/GRADING_REFERENCE.md` said 1.5 for large; it was written yesterday from memory and is corrected in this change. Tried on the 53 recorded passes with the AoA fix in place, large at 1.5 instead of 2.5 changes one grade: 15 September 19:18 (Justice, T&G) goes `(OK)` to `--` on +1.54 at 86 m, corrected to +0.48 by the wires ("gross, good, at the ramp" is `--` in the table). 19:03 (-1.59 at 70 m) and 19:17 (+2.20 at 93 m) are `--` under both, on AoA. The 14 and 13 September sets do not move. In feet, 1.5 degrees at 100 m is 8.6 ft; at 50 m, 4.3 ft. The value is a policy choice; the code's 2.5 stays until it is made.

### 9. Outcome and data quirks

| Pass | Quirk |
|---|---|
| 19:15 Justice T&G | no `runway_touch` event, `touchdown_time_dcs` null, `groove_time_secs` null, outcome from `hook_up_near_deck`; the report carries `wire_estimated 2, wire_primary rust_estimated` from hook-plane crossings flown with the hook up. That is a "would have caught the 2-wire", useful on an intentional bolter, but it is presented with the same field and confidence as a caught wire (recommendation 4). The alignment tool, with no touchdown, took the groove to the last datum and reported "fast 31%" where the groove proper is slow 40%. |
| 19:17 Ghost-72 F-14 | kinematic arrest with no DCS landing mark; the report's "wire 1" is the first plane crossing, the stop position says 2-wire (section 6); `arrest_confirmation.kinematic.accepted false, post_contact_forward_departure_detected` against `deck_kinematics.confirmed true`. |
| 19:03, 19:04, 19:45 traps | "Rust estimate unavailable" with a DCS wire, as on 14 September. |
| 18:49 | the only orange pass: `late_delivery_warning`, max delivery age 310 ms, one pilot in the pattern; the 18:47 wave-off just before it had 220 ms. Not the read budget (waits 2 ms). |
| align_aoa.py | the F-14 "off" lamp state is now half the samples on hook-up passes (section 2); the tool's per-state tables exclude it, the CSV row count does not. |

### 10. Acquisition

| Item | 14 September, two pilots | 15 September, two pilots (18:56 to 19:45) |
|---|---|---|
| `read_budget_per_second` | 16 | 40 |
| read-budget wait per pass | 20 to 113 s | 2 to 3 ms |
| telemetry health | orange on 9, red on 1 | green on 22, orange on 1 (18:49, before the second pilot joined) |
| max delivery age | 150 to 280 ms | 170 to 310 ms |
| effective rate | 20 Hz | 20 Hz, 0 dropped samples, 0 invalid snapshots |

Finding 9 of the 14 September review is closed.

### 11. What changed on the branch

Branch `feature/ramp-grader-touchdown-and-wire`, off `feature/ramp-aoa-grading-prototype`. The first row is the analysis; the rest is the implementation of recommendations 1 to 4 (section 13).

| File | Change |
|---|---|
| `src/grading.rs` | `normalized_aoa_error`: fixed 0.1 degree steps instead of doubling steps; comment records the dropped windows. Two tests: the 0 to 25 degree sweep on four types, and 7.2 / 13.2 degrees on the F-14 as gross |
| `src/grading.rs` | `CatobarGradingPolicy` gains `aoa_large_min_duration_s`, `series_ends_at_physical_touchdown`, `near_deck_reference_distance_m`; `demote_short_gross_runs`, `physical_touchdown_time`, `near_deck_equivalent_deg`; `CONVENTION` carries all three; three tests |
| `src/commands/grade_ab.rs` | ten policy steps (P7 gross AoA needs 1 s, P8 series end at physical touchdown, P9 height inside 100 m = `CONVENTION`) |
| `src/data.rs` | `AirplaneInfo::arresting_run_out_m`: 87 m for the F-14A/B/B(U) (ten of ten traps across the 2, 3, 14 and 15 September recordings), unset for the T-45C (49 to 61 m) and the F/A-18C (60 and 89 m) |
| `src/track.rs` | `wire_estimate_from_stop_position` (reason `stop_position_run_out`), `wire_estimate_with` (hook transient, then stop position on a confirmed arrestment, then the crossing selection), reason `hypothetical_hook_up_plane_crossing` on hook-up passes, outcome "T&G (CQ) — would have caught wire N"; three tests |
| `src/tasks/record_recovery.rs` | same outcome text in the JSON `outcome`; `wire_primary` `rust_hypothetical` on a hook-up pass |
| `CHANGES.md` | Unreleased: the fix and the two changes |
| `docs/GRADING_REFERENCE.md` | glideslope gross threshold 2.5 degrees (what the code does), with a note; the three new rules |
| `docs/HOW_GRADING_WORKS_PLAIN_LANGUAGE.md` | the wire estimate paragraph: stop position on the Tomcat, "would have caught" on hook-up passes |
| `docs/GRADING_CONVENTION_PROTOTYPE_2026-09-15.md` | update note: four rows of its tables changed with the fix, pointer here |
| `docs/AOA_CALIBRATION_2026-09-15.md` | raw output of `align_aoa.py` on today's recovery |
| `docs/RECOVERY_REVIEW_2026-09-15.md` | this document |

| Check | Result |
|---|---|
| `cargo test --locked` | 354 + 3 passed, 1 ignored (the fixture table test, as before) |
| `cargo clippy --locked --all-targets -D warnings` | clean |
| `cargo fmt --check`, `git diff --check` | clean |
| `grade-ab` on the 23 reports, before the fix | 23 of 23 equal to the recorded grade |
| `grade-ab` on the 23 reports, after the fix alone | 19 equal, 4 lower (section 4) |
| `grade-ab` on the 23 reports, final `CONVENTION` | 21 equal, 1 higher (19:43 `OK`), 2 lower (19:15, 19:17) (section 13) |
| live fixtures (`tests/recordings/live_2026-09`, `wire_*`) | unchanged; the three F-14 traps without a DCS wire now name their wire from the stop position (residual 0.2 to 3.9 m) |

Nothing is committed. The recovery folder is ignored by `.gitignore` (`recovery_aoa_calibration*`).

### 12. Recommendations

Each one with what it means in the cockpit, what is already done on this branch, what remains, and which passes it decides.

Two words are used throughout and mean different things. **The fix** (recommendation 1) is a bug repair, already in the code: the routine that measures how far a reading sits outside the band failed for readings 2 to 3 degrees out, and those readings were thrown away; the fix makes it work for every reading, adds no rule and changes no threshold. **The rule** (recommendation 2) is a new policy, proposed and not implemented: gross readings must themselves last one second before an episode counts as gross. A pass can therefore be graded three ways:

| Pass | Before (bug) | Fix only | Fix plus rule |
|---|---|---|---|
| 19:17, gross fast 1.9 s at the ramp | `(OK)` | `--` | `--` |
| 19:03, gross for 0.15 s as the nose drops at touchdown | `(OK)` | `--` | `(OK)` |

The fix should go in as it is. The rule is a choice: without it, one sample at touchdown can decide a `--`.

**Status at the end of 15 September 2026** (the user chose fix plus rule, the physical touchdown with height, and the wire changes; all implemented that evening, see section 13):

| # | Recommendation | Status |
|---|---|---|
| 1 | Merge the `normalized_aoa_error` fix | implemented, on the branch, not committed |
| 2 | Gross AoA needs one second of gross readings | implemented (`aoa_large_min_duration_s`, in `CONVENTION`) |
| 3 | End the series at the physical touchdown; height instead of angle in the last 100 m | implemented (`series_ends_at_physical_touchdown`, `near_deck_reference_distance_m`, in `CONVENTION`) |
| 4 | Wire from the stop position on traps; "would have caught" wire on hook-up passes; why 19:17 has no landing mark | implemented for the F-14 (the T-45C and Hornet run-outs are not constant); the landing mark question is open |
| 5 | Fly the ordinary session again, hook down, with deck wind | to do: a build from this branch, a windy mission, the same pilots |

The text below describes each recommendation as it was written before the implementation.

**1. Merge the `normalized_aoa_error` fix.**

- *What it is.* The routine that measures how far an AoA reading sits from the on-speed band gave up when its doubling steps (0.25, 0.5, 1, 2, 4 degrees) jumped over the band, and the caller then dropped the reading as if it had never been sampled. On the F-14 that hid every reading between 6.8 and 7.95 degrees and between 12.8 and 13.95; on the T-45 between 4.75 and 6.25 and between 6.75 and 7.25 (section 4). In the cockpit that is the chevron alone, 10 to 15 knots off approach speed: exactly the zone the gross AoA rule of the convention was written for. The fix changes no threshold; it lets the gross tier see what it was meant to see.
- *Done.* Fixed in `src/grading.rs` with fixed 0.1 degree steps, which cannot jump over the narrowest band (the T-45C's 0.5 degrees). Two tests added, 348 + 3 tests pass, clippy and fmt clean. `docs/GRADING_REFERENCE.md` and the convention document carry the change.
- *To do.* Commit on this branch and merge; nothing is committed. The next server build must carry it or the gross tier stays blind.
- *Decides.* Today's 19:17, yesterday's 19:00 and the 18:26 of 13 September (real gross excursions, `(OK)` to `--`), and the five borderline passes of recommendation 2.

**2. Decide the gross persistence rule.**

- *What it is.* Under P2 an AoA episode needs one second to grade, but the severity of an episode is the maximum over its samples, so one gross sample inside a three-second moderate episode makes the whole episode gross, and gross at the ramp is `--` in every cell of the convention table. The proposal (section 5): the gross readings themselves must span one second before an episode counts as gross, otherwise it is moderate. In the cockpit: the fast or slow chevron alone, 2 degrees past the donut edge (F-14 below 7.95 or above 12.8 degrees, T-45 below 6.25 or above 10.75), held for a full second, which is 10 to 15 knots off for a second and not a flick of the needle as the nose drops onto the deck.
- *Done.* Measured only. On the recorded passes the rule keeps every real gross pass (shortest gross run 1.45 s) and returns five to the grade the convention gives a moderate deviation, `(OK)`.
- *To do.* A policy decision, since it moves grades by rule and not by correctness. Then about ten lines in `build_axis_episodes` (sum the duration of `Large` samples, demote the episode's maximum to `Medium` below `aoa_min_episode_duration_s`) and a test with a 0.5 s gross spike inside a 3 s moderate episode.
- *Decides.* Today's 19:03, 19:15 and 19:23, yesterday's 19:38 Justice and 19:41: with the fix and without the rule they are `--`; with the fix and the rule they return to `(OK)` (their gross readings last 0.1 to 0.9 s). The rule changes nothing on 19:17, 18:49, 19:04 and 19:34 today, 19:00 yesterday and 18:26 of the 13th: their gross readings last 1.45 to 8.75 s, so they are `--` with or without it.

**3. End the series at the physical touchdown, and measure the last thirty metres in height.**

- *What it is.* Two measurement problems, not policy. The DCS `runway_touch` event arrives 0.6 to 0.9 s after the sink rate reverses in the trajectory, so the last samples graded are rollout, where the flight model's AoA collapses from 9.5 to -2.5 degrees as the nose comes down (19:03 reads "gross fast at the ramp" from that, section 5). And the glideslope error is kept in degrees down to 5 m from the landing point, where a foot is ten degrees: 19:43 missed `OK` on 0.86 degrees at 5 m, which is 7 cm of height (section 7). The same geometry makes a low at the ramp "corrected" when the aircraft reaches the deck (19:23).
- *Done.* Diagnosed and quantified in sections 5 and 7. No code.
- *To do.* Cut the glideslope, lineup and AoA series at the physical touchdown: the sink-rate reversal in `trajectory_deviations`, or the weight-on-wheels and hook samples the report already carries. Convert the glideslope error to height above the glideslope, or clamp the angle, inside the last 50 to 100 m. Both live in the trajectory builder of `src/track.rs` and the observation builders of `src/grading.rs`.
- *Decides.* Until it is done no pass can reach `OK` through the ramp, and the touchdown collapse can decide a `--` on its own (19:03). 19:43 is the first candidate for an `OK`.

**4. Estimate the wire from the stop position on traps; keep a "would have caught" wire on hook-up passes; find out why 19:17 has no landing mark.**

- *What it is.* Two different questions hide behind one estimator. On an arrestment the question is *which wire was caught*. With no DCS landing mark and no hook-deflection correlation, `continuous_hook_plane_crossing` picks the earliest hook-plane crossing within 1.2 s before the deceleration onset; the onset is detected 53 to 59 m past the wire, 0.8 to 1.0 s at approach speed, and the hook sweeps all four planes in 0.6 s, so every crossing is inside the tolerance and the rule returns the 1-wire on any trap without a mark. The stop position carries the information: the arresting engine's run-out is 85 to 89 m past the wire on eight of eight Tomcat traps whatever the entry speed, so `x_at_slow_m + 87 m` names the wire to within 2 m (section 6). On an intentional bolter or hook-up touch-and-go the question is *which wire the hook would have caught*, and that is worth answering: it is the feedback a pilot practising the pattern wants, and the geometry is the same as on a trap, the hook point's height when it crosses each wire plane, with no arrestment to confirm it. 19:15 carries `wire_estimated 2` from exactly that geometry; the problem is not the estimate but that the report presents it with the same field and the same confidence as a caught wire.
- *Done.* The calibration table of section 6 (seven traps with a DCS wire, one without). The 19:17 case is settled as a 2-wire caught in the air, wheels down beyond the 4-wire.
- *To do.* In `src/track.rs`: on a confirmed arrestment without hook-deflection correlation, derive the wire from `deck_kinematics.x_at_slow_m` (or the onset position plus 55 m) instead of the crossing sequence, with the T-45's shorter run-out (52 to 60 m on four traps) as its own table entry. On a hook-up pass, keep the crossing-based estimate but label it as hypothetical: a separate field or reason (`would_have_caught_wire`, confidence never above "medium"), the outcome line reading "T&G, hook up, would have caught the 2-wire" rather than "Wire #2", and the hook point's height at each plane crossing shown so that a hook that would have skipped over the deck reads as "no wire". The same label applies to a bolter with the hook down, where the crossings say which wire the hook missed and by how much. For the missing landing mark on 19:17, read `dcs.log` around 19:17:30 UTC; the arrest itself is confirmed by the deck kinematics (relative speed 0.01 m/s, held 2 s).
- *Decides.* The outcome line of 19:17 ("Wire #1" to "Wire #2"), the presentation of the estimate on 19:15 and on every hook-up pass (today: eight of Justice's and Ghost-72's), and every future trap without a landing mark.

**5. Fly the ordinary session again, hook down, with deck wind.**

- *What it is.* The four grades that changed today were all decided in the last two seconds of the pass, so the next session is the test of recommendations 1 to 3 together on ordinary flying. Hook down on the calibration passes because the Tomcat's indexer flashes with the hook up in carrier mode, which darkens half the recorder's samples (section 2). And the wind correction has still never been exercised: both sessions had under 1 m/s at deck level, and the five passes flown on a substituted wind reading show no effect because there was no wind to substitute.
- *Done.* The client recorder and the raised read quota (40 reads per second) are in place and proven: 22 of 23 passes green with both pilots in the pattern, read-budget waits of 2 to 3 ms per pass against 20 to 113 s yesterday.
- *To do.* A server build with the fix of recommendation 1 (and 2 if adopted), a mission with 15 to 20 knots of deck wind, the same pilots flying the donut as they usually would, hook down. Then `grade-ab` and `align_aoa.py` as today.
- *Decides.* The grade distribution under `CONVENTION` on ordinary flying, which no session has produced yet (today: ten `(OK)`, no `OK`), and the first check of the wind term of the AoA computation.

### 13. Implemented the same evening: the policy as it now stands

The user chose recommendations 1 to 4 (fix plus rule, physical touchdown with height, the wire changes, keeping a hypothetical wire on hook-up passes). They are on branch `feature/ramp-grader-touchdown-and-wire` (section 11) and `CatobarGradingPolicy::CONVENTION` now carries the three grading rules; `lso grade-ab` shows them as steps P7 (gross AoA needs 1 s), P8 (series end at the physical touchdown) and P9 (height inside 100 m, equal to `CONVENTION`).

**Every recorded pass whose grade or deciding episode moves between the fix alone (section 4) and the final policy:**

| Set | Pass | Recorded | Fix alone | Final `CONVENTION` | What moved it |
|---|---|---|---|---|---|
| 15 Sept | 19:03 T-45 trap 3 | `(OK)` | `--` AoA ramp | `(OK)` GS ramp 2.3 | the 0.15 s of gross fast was the touchdown collapse (P7 demotes it, P8 removes the samples); 1.5 low at 70 m, moderate, corrected |
| 15 Sept | 19:23 F-14 trap 1 | `(OK)` | `--` AoA ramp | `(OK)` AoA ramp 2.3 | 0.3 s of gross slow at touchdown demoted; slow (12.8) 2.9 s at the ramp, average |
| 15 Sept | 19:43 F-14 T&G | `(OK)` | `(OK)` GS ramp | **`OK`** AoA middle 1.1 | 0.86 deg at 5 m is 7 cm: judged in height it is nothing; the worst episode left is a moderate fast in the middle, corrected well |
| 15 Sept | 19:15 F-14 T&G | `(OK)` | `--` AoA ramp | `--` AoA start 3.0 | the 0.9 s of gross at the end is demoted; what decides is now the start of the groove: slow with 1 degree swings while rolling out of the turn, "moderate, poor" |
| 15 Sept | 19:02 F-14 T&G | `--` | `--` GS ramp | `--` LU in close 3.2 | the 2.66 high at the ramp is 2.3 m at 50 m, moderate in height; the gross lineup in close (3.0 right, corrected) decides instead |
| 15 Sept | 19:17 F-14 trap | `(OK)` | `--` AoA ramp | `--` AoA ramp 3.3 | unchanged: 1.9 s of gross fast at the ramp in the air |
| 14 Sept | 19:38 Justice T&G | `--` | `--` AoA ramp | `(OK)` AoA ramp 2.3 | 0.1 s of gross at touchdown demoted |
| 14 Sept | 19:41 Justice trap 2 | `--` | `--` AoA ramp | `(OK)` AoA ramp 2.3 | 0.2 s of gross at touchdown demoted |
| 14 Sept | 19:38 Ghost-72 trap 1 | `--` | `(OK)` GS ramp | `(OK)` LU ramp 2.3 | deciding episode only |
| 14 Sept | 19:00 F-14 T&G | `--` | `--` AoA ramp | `--` AoA ramp 3.3 | unchanged: 4.4 s and 1.9 s of gross fast |
| 13 Sept | 18:26 T-45 trap 2 | `--` | `--` AoA in close | `--` AoA in close 3.2 | unchanged: 1.45 s of gross fast in close |

Every other pass of the three sets keeps the grade of section 4. Against the grades the server wrote on 15 September: 21 of 23 equal, 19:43 higher, 19:15 and 19:17 lower. Over the 53 recorded passes the final policy gives one `OK`, the first under the convention, on the pass a pilot would pick for it.

**Wire estimate.** With the stop-position method the 19:17 outcome would read "Wire #2 (Rust estimate)" from a live run (the recorded report keeps its "Wire #1"; `grade-ab` does not re-estimate wires). On the three F-14 fixtures of 2 and 3 September that carry no DCS wire the stop position names the right wire with residuals of 0.2 to 3.9 m, so the Tomcat constant holds across two campaigns and two DCS-gRPC versions. The T-45C stopped 49 m past the wire on the fixtures and 52 to 61 m on this week's traps, the Hornet 60 m on the `wire_4_01` fixture and 89 m on Sunday's trap: for those two types the run-out stays unset and the crossing-based fallback is unchanged. Hook-up passes now read "T&G (CQ) — would have caught wire N" with reason `hypothetical_hook_up_plane_crossing`.

**Still open.** Why DCS issued no landing mark for 19:17 (`dcs.log` around 19:17:30 UTC); the T-45C and Hornet run-outs; the 19:15 start-of-groove swing verdict, which is the P6 swing rule applied while the aircraft is still rolling out of the turn and may deserve the same look the ramp got today.
