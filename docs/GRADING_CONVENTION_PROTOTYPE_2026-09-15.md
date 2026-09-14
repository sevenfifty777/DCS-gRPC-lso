# Grading against the written LSO convention: prototype and before-and-after

Branch `feature/ramp-aoa-grading-prototype`, 15 September 2026. Follow-up to `docs/GRADING_POLICY_PROTOTYPE_2026-09-13.md` (the ramp and AoA rules, now in production as `PROTOTYPE`) and `docs/AOA_CALIBRATION_REVIEW_2026-09-14.md` (the calibrated AoA bands). Those two left two policy questions open "for the human LSO": how much a sustained moderate deviation should cost, and whether a moderate excursion at the ramp is always a `--`. No human LSO is available, so this document settles them against the only reference that exists in writing, and shows the effect on every recorded pass.

**Decision, 15 September 2026:** `CatobarGradingPolicy::CONVENTION` is the production default on this branch (`impl Default` in `src/grading.rs`). The tables below were produced before the switch and still label it "the candidate"; `PROTOTYPE` is the previous default and stays available as the P3 column of `grade-ab`.

---

## 1. In plain language

**The reference.** Real LSO grades follow a written convention. A deviation is written three ways: in parentheses for "a little" (`(LOAR)`), plain for a moderate one (`LOAR`), underlined for a gross one (`_LOAR_`). The grade then follows from the worst deviation and how it was handled: `OK` is reasonable deviations with good corrections, `(OK)` is reasonable deviations with average corrections, `--` is a gross deviation or a moderate one left uncorrected, and `C` is unsafe. Our grader already sorts every episode into the same three sizes (small, medium, large) and three corrections (good, average, poor), so the convention can be written as a table and applied directly.

**What the current grader does differently.** Today an episode's cost is its size times a zone weight (the ramp counts double), with a level added for a poor correction and removed for a good one. That arithmetic agrees with the convention almost everywhere, but not in three cells:

- a moderate deviation with an average correction in close or at the ramp is a `--` today; the convention says `(OK)`. This single cell is behind most of the `--` grades of the last two sessions ("fast in close for 11 seconds, average correction", "slow at the ramp for 2.8 seconds");
- a small deviation with a poor correction in close or at the ramp is a `--` today; the convention never sends "a little" below `(OK)`;
- a gross deviation with a good correction early in the groove is `(OK)` today; the convention keeps gross at `--` unless the pilot actually got back inside the band.

**Two AoA details had to be fixed at the same time.** First, every "fast" or "slow" reading was a moderate deviation whatever its size, so 6 degrees on a Tomcat (about 15 knots fast) weighed the same as a quarter of a degree past the chevron; the candidate adds a gross tier at 2 degrees outside the on-speed band. Second, the detector that flags "overcontrol" counted any swing of 0.3 degrees, which is below the 0.8 degrees of noise the computed AoA carries, so nearly every long AoA episode was called poorly corrected; the candidate requires a 1 degree swing on AoA.

**What it changes on the 44 recorded passes.** Compared with the production grader, the candidate raises 11 passes by one grade and lowers none: four of the 22 passes of 14 September, three of the eight of 13 September, four of the 14 replay fixtures. Every pass that stays `--` does so on a gross deviation confirmed by the cockpit or on a moderate one that was left uncorrected. The two passes flown 100% on the fast chevron by the indexer and the three flown 100% slow stay `--`.

**Recommendation.** Adopt `CONVENTION` as the production default for this week's session. It is the current policy plus three rules with a written source, and it never grades a pass lower than today.

---

## 2. The candidate, step by step

`lso grade-ab` compares cumulative steps; P0 to P3 are unchanged from the 13 September document.

| Step | Rule | Source |
|---|---|---|
| P0 | baseline grader | `main` at `664fe5b` |
| P1 | touchdown ends the correction assessment | 13 September, F2 |
| P2 | AoA needs 1 s to grade | 13 September, F3 |
| P3 | AoA grades calibrated types only, **production default (`PROTOTYPE`)** | 14 September calibration |
| P4 | episode cost read from the convention table instead of size times zone weight | NAVAIR 00-80T-104 grade definitions, LSO NATOPS shorthand |
| P5 | gross AoA tier at 2 degrees outside the on-speed band | PROJECT-DERIVED: about two indexer units, 10 to 15 knots |
| P6 | AoA oscillation swing 1 degree instead of 0.3, **candidate (`CONVENTION`)** | measured AoA noise, 14 September section 3 |

The table behind P4, with the same three sizes and three correction verdicts the grader already produces:

| Deviation | Correction | START, MIDDLE | IN CLOSE, RAMP |
|---|---|---|---|
| a little (small) | good | `OK` | `OK` |
| a little | average | `OK` | `(OK)` |
| a little | poor | `(OK)` | `(OK)` |
| moderate (medium) | good | `OK` | `(OK)` |
| moderate | average | `(OK)` | `(OK)` |
| moderate | poor | `--` | `--` |
| gross (large) | good | `(OK)` | `--` |
| gross | average or poor | `--` | `--` |

"Good" requires that the axis came back inside its band. Without that clause the 19:25 pass of 14 September, flown on the fast chevron for 100% of the groove, would have been `(OK)`: its gross excursion was quickly reduced to a moderate one and then held there for 15 seconds, which the correction logic calls "good". The zone still matters twice: through the column, and through the correction deadlines (3.0 s at the start, 0.75 s at the ramp).

### The two AoA parameters, in the cockpit

**P5, gross AoA tier at 2 degrees** (`aoa_large_error_deg`). The indexer gives the grader three sizes: donut (on speed), donut plus chevron (a little fast or slow, "small"), chevron alone ("fast" or "slow", "medium"). Chevron alone starts a quarter of a degree past the donut on the T-45 and covers everything beyond, so a pilot just past the chevron edge and a pilot 5 degrees off both read "fast". The rule: more than 2 degrees outside the on-speed band is a gross deviation.

| Aircraft | On-speed band | Gross from | What the pilot sees |
|---|---|---|---|
| T-45C | 8.25 to 8.75 deg | below 6.25 or above 10.75 deg | gauge near 14 or 20 units instead of 17; about 12 to 15 knots off approach speed |
| F-14 | 9.95 to 10.8 deg | below 7.95 or above 12.8 deg | gauge about 2 units off the 15-unit mark; 10 to 15 knots off |

Fast side that is a Tomcat at the ramp near 145 knots instead of 130, flat, hook high, the pass DCS writes `FX` and a real LSO underlines. Slow side it is nose high, slow chevron solid, sink rate building, close to a settle at the ramp. A pilot half a chevron off (9.3 on the Tomcat) is still only moderate. On 14 September the tier decides 19:04, 19:21, 19:24, 19:25, 19:27, 19:30, 19:33 and 19:35 (true AoA 2.4 to 6 or 13 to 15 degrees, cockpit on a chevron for 100% of the groove); on 13 September the three F-14 passes at 6 degrees.

**P6, AoA oscillation swing of 1 degree** (`aoa_oscillation_min_swing_deg`). This one is about the measurement, not the pilot. The grader's overcontrol detector calls a correction poor when the deviation reverses direction twice or more with swings of at least 0.3 degrees: "hunting", "chasing the ball". That threshold suits glideslope and lineup, which the server measures smoothly. The computed AoA jitters by about plus or minus 0.4 degrees around its true value from one sample to the next (measured against the flight model on 14 September, section 3 of the calibration review), so a pilot holding a perfectly steady fast chevron for 14 seconds produced dozens of 0.3 degree "reversals" that were noise, and was graded as overcontrolling. At 1 degree a reversal only counts when the AoA really moves a full degree back and forth: the donut going out to a chevron and back, or the needle swinging a unit either way, which is a genuine pumping of stick or throttle. It decides 17:58, 18:08 and 18:20 of 13 September (`--` to `(OK)`); 18:12 of the same day, where the pilot went slow, corrected, and went slow again in close, keeps its `--`.

---

## 3. The 22 passes of 14 September

Recorded grade, production grader (P3), candidate (P6) and the episode that decides the candidate's grade. Cockpit indexer shares and DCS comments are in the calibration review, section 6.

| Pass (UTC) | Type | Outcome | Recorded | P0 | P3 (production) | P6 (candidate) | Candidate's deciding episode |
|---|---|---|---|---|---|---|---|
| 18:33 | T-45 | T&G | `--` | `--` | `--` AoA middle | `--` AoA middle | slow (10.6) 15.6 s, poor: worsened after improving; cockpit slow 85% |
| 18:37 | T-45 | Bolter | `B` | `B` | `B` | `B` | bolter |
| 18:39 | T-45 | T&G | `C` | `C` | `C` | `C` | sink rate 8.4 m/s inside 1/4 NM |
| 18:41 | T-45 | Trap 2 | `--` | `--` | `--` LU start | `--` AoA middle | gross fast (4.6) 16 s, average; cockpit fast 100% |
| 18:49 | T-45 | Bolter | `B` | `B` | `B` | `B` | bolter |
| 18:52 | T-45 | Trap 2 | `(OK)` | `--` | `--` AoA in close | **`(OK)`** AoA in close | fast (6.7) 14.8 s, average; cockpit fast 86% |
| 19:00 | F-14 | T&G | `--` | `--` | `--` AoA ramp | **`(OK)`** GS ramp | fast (8.0) 4.3 s at the ramp, average |
| 19:04 | F-14 | Trap 4 | `--` | `--` | `--` GS ramp | `--` AoA in close | gross fast (2.4) 13.5 s; cockpit fast 100% |
| 19:21 | F-14 | T&G | `--` | `--` | `--` GS ramp | `--` AoA start | gross fast (6.0) 13 s, poor; cockpit fast 100% |
| 19:24 | F-14 | T&G | `--` | `--` | `--` AoA ramp | `--` AoA ramp | gross fast (6.0) 9 s at the ramp |
| 19:25 | F-14 | T&G | `--` | `--` | `--` AoA middle | `--` AoA middle | gross fast (6.0) 15 s, never back in the band; cockpit fast 100% |
| 19:26 | F-14 | Approach only | `C` | `C` | `C` | `C` | 2.5 deg low inside 1/4 NM |
| 19:27 | F-14 | T&G | `--` | `--` | `--` AoA in close | `--` AoA in close | gross fast (6.0) 11.6 s; cockpit fast 100% |
| 19:30 | F-14 | T&G | `--` | `--` | `--` GS ramp | `--` AoA ramp | gross slow (14.8) 22 s; cockpit slow 100%, DCS SLO X |
| 19:33 | F-14 | T&G | `--` | `--` | `--` GS ramp | `--` AoA in close | gross slow (14.8) 24 s; cockpit slow 100% |
| 19:34 | F-14 | T&G | `--` | `--` | `--` LU start | `--` LU start | gross lineup (4.6 deg) 18 s at the start, average |
| 19:35 | F-14 | T&G | `--` | `--` | `--` AoA ramp | `--` AoA ramp | gross slow (14.8) 22 s; cockpit slow 100% |
| 19:38 | F-14 | Trap 1 | `--` | `--` | `(OK)` GS ramp | `(OK)` GS ramp | 1.0 deg low at the ramp, good |
| 19:38 | F-14 | T&G | `--` | `--` | `--` AoA ramp | **`(OK)`** AoA ramp | slow (12.7) 2.9 s at the ramp, average; cockpit donut 70% |
| 19:41 | F-14 | Trap 2 | `--` | `--` | `--` AoA ramp | **`(OK)`** AoA ramp | slow (12.8) 2.75 s at the ramp, average; cockpit donut 62% |
| 20:01 | F/A-18C | Wave-off | `WO?` | `WO?` | `WO?` | `WO?` | go-around |
| 20:04 | F/A-18C | Trap 3 | `(OK)` | `(OK)` | `(OK)` GS start | `(OK)` GS start | 1.5 deg low at the start, average |

Four passes rise, none falls. The intermediate step P4 alone (the table without the gross AoA tier) would also have raised 19:24, 19:27, 19:30, 19:33 and 19:35, all flown 100% on a chevron by the cockpit; the gross tier at P5 is what keeps them at `--`.

## 4. The eight passes of 13 September

| Pass (UTC) | Type | Outcome | Recorded | P0 | P3 (production) | P6 (candidate) | Candidate's deciding episode |
|---|---|---|---|---|---|---|---|
| 17:58 | T-45 | Trap 1 | `--` | `--` | `--` AoA ramp | **`(OK)`** AoA ramp | slow (9.7) last 2.35 s, average; the 9 s fast in the middle is no longer "overcontrol" |
| 18:08 | T-45 | T&G | `--` | `--` | `--` AoA middle | **`(OK)`** AoA ramp | slow (9.5) 1.1 s at the ramp, good; 14 s fast in the middle now average |
| 18:12 | T-45 | Trap 1 | `--` | `--` | `--` AoA in close | `--` AoA in close | slow (9.7) 7.1 s in close, poor: worsened after an improvement (DCS: SLO X) |
| 18:20 | T-45 | T&G | `--` | `--` | `--` AoA in close | **`(OK)`** GS ramp | 9.4 s fast in close now average; 1-wire touchdown reads 0.9 deg low at the ramp |
| 18:26 | T-45 | Trap 2 | `--` | `--` | `(OK)` AoA middle | `(OK)` GS ramp | 11.75 s fast in the middle, average |
| 18:37 | F-14 | T&G | `--` | `--` | `--` GS ramp | `--` AoA middle | gross fast (6.0) 12 s, poor |
| 18:42 | F-14 | Trap 2 | `--` | `--` | `--` AoA middle | `--` AoA middle | gross fast (6.0) 11.7 s, poor |
| 18:53 | F-14 | Trap, no DCS | `--` | `--` | `--` AoA in close | `--` AoA in close | gross fast (6.0) 13.4 s, poor |

Three of the four T-45 passes that the 13 September note said should be `(OK)` on geometry are now `(OK)`. What moved them is P6, the AoA swing threshold: their long "fast" episodes at 7.3 degrees (0.7 under the band) were called "repeated significant inversions" because the computed AoA wobbles by 0.4 degrees around its value, and a poor correction on a moderate deviation is a `--` under any model. With the threshold at the measured noise level those episodes are "average", and a moderate deviation with an average correction is `(OK)`. Pass 18:12 stays `--` on a real re-aggravation: slow, corrected, then slow again in close, which is also what DCS wrote.

## 5. The 14 replay fixtures of 2 and 3 September

No wind reference in these replays, so AoA never grades them; only the table (P4) can move a row.

| Fixture | Type | Outcome | P0 | P3 (production) | P6 (candidate) | Candidate's deciding episode |
|---|---|---|---|---|---|---|
| t45_hookdown_wire1 | T-45 | Trap 1 | `--` | `(OK)` | `(OK)` | small glideslope at the ramp |
| t45_hookdown_wire3 | T-45 | Trap 3 | `--` | `(OK)` | `(OK)` | small lineup at the ramp |
| t45_hookdown_wire4 | T-45 | Trap 4 | `(OK)` | `(OK)` | `(OK)` | small lineup at the ramp |
| t45_hookdown_bolter | T-45 | Bolter | `B` | `B` | `B` | bolter |
| t45_hookup_1 | T-45 | T&G | `(OK)` | `(OK)` | `(OK)` | small lineup in close |
| t45_hookup_2 | T-45 | T&G | `--` | `(OK)` | `(OK)` | small lineup at the ramp |
| t45_hookup_3 | T-45 | T&G | `(OK)` | `(OK)` | `(OK)` | small glideslope at the ramp |
| f14bu_hookdown_wire1 | F-14 | Trap 1 | `--` | `--` | **`(OK)`** | moderate lineup at the ramp, average |
| f14bu_hookdown_wire2 | F-14 | Trap 2 | `(OK)` | `(OK)` | `(OK)` | small glideslope at the ramp |
| f14bu_hookdown_wire4 | F-14 | Trap 4 | `--` | `--` | **`(OK)`** | moderate lineup at the ramp, average |
| f14bu_hookup_1 | F-14 | T&G | `--` | `--` | `--` | gross lineup at the ramp |
| f14bu_hookup_2_dcs_waveoff | F-14 | T&G | `--` | `--` | **`(OK)`** | moderate glideslope in close, average |
| f14bu_hookup_3 | F-14 | T&G | `(OK)` | `(OK)` | `(OK)` | small lineup in close |
| f14bu_hookup_4 | F-14 | T&G | `--` | `--` | **`(OK)`** | moderate lineup at the ramp, average |

The four F-14 rows that move are the "2.2 to 3.8 degrees of lineup at the ramp with a real but late correction" cases the 13 September document flagged as a threshold question. Under the convention a moderate deviation with an average correction is `(OK)`, whatever the zone; the one gross lineup at the ramp stays `--`.

## 6. Summary of the effect

| Set | Passes | Raised by the candidate | Lowered |
|---|---|---|---|
| 14 September | 22 | 4 (`--` to `(OK)`) | 0 |
| 13 September | 8 | 3 (`--` to `(OK)`) | 0 |
| Fixtures | 14 | 4 (`--` to `(OK)`) | 0 |

Of the 20 passes that remain `--` across the three sets, 13 are decided by a gross AoA deviation confirmed by the cockpit indexer, 3 by a moderate AoA deviation that got worse after an improvement or oscillated by more than a degree, 3 by gross lineup, 1 by gross glideslope. No pass reaches `OK` that did not already, and no `OK` is created by the candidate: the ordinary-flying session is still needed to see the candidate's `OK` rate.

## 7. What was built

| File | Change |
|---|---|
| `src/grading.rs` | `CatobarGradingPolicy` gains `lso_convention_table`, `aoa_large_error_deg`, `aoa_oscillation_min_swing_deg`; new constant `CONVENTION`; `convention_effective_severity` (the table); the AoA observation builder applies the gross tier; the reversal counter takes an explicit swing for AoA; four new tests |
| `src/commands/grade_ab.rs` | seven policy steps (P0 to P6) instead of four |
| `docs/GRADING_REFERENCE.md` | new "Episode grading" section: sizes, zones, correction verdicts, both scoring models and the convention table; the "AoA is chart information only" paragraph replaced |

| Check | Result |
|---|---|
| `cargo test --locked` | 347 + 3 passed, 1 ignored |
| `cargo clippy --locked --all-targets -D warnings` | clean |
| `cargo fmt --check`, `git diff --check` | clean |
| `grade-ab` P0 column against the 30 recorded grades | identical (the fixtures' "recorded" column is now the `PROTOTYPE` grade, since the harness grades with the production default) |

**Switched on 15 September 2026.** `impl Default for CatobarGradingPolicy` in `src/grading.rs` returns `CONVENTION`. Two tests that pinned the previous behaviour through the production wrappers were updated, each keeping its baseline assertion through the explicit-policy variant: a moderate glideslope deviation at the ramp with an unobservable correction is `(OK)` (was `--`), and "a little" lineup with a poor correction is `(OK)` (was `--`). After the switch: 347 + 3 tests pass, clippy, fmt and whitespace clean, and `grade-ab` on the 22 passes of 14 September reproduces the P6 column above. To go back, the same line returns `PROTOTYPE`.
