# Grading policy prototype: ramp correction and AoA, before and after

Branch `feature/ramp-aoa-grading-prototype`, cut from `main` at `664fe5b` on 13 September 2026. Nothing is committed and nothing in production behaviour changes on this branch: the three candidate rules sit behind a `CatobarGradingPolicy` switch whose default is the current grader, and a new read-only command re-grades recorded passes under each rule so the effect can be read pass by pass. This is the step 8 exercise from the plan ("only now tune the grading"), done on data that already exists.

The three rules come from findings F1 to F3 of `docs/LIVE_SESSION_REVIEW_2026-09-13.md`.

---

## 1. The three candidate rules

They are applied cumulatively in the tables below, so each column shows what the rule adds on top of the previous one.

| Column | Rule | What it changes |
|---|---|---|
| P0 | Baseline | The grader exactly as on `main` at `664fe5b`. |
| P1 | Touchdown ends the correction assessment | An episode still open at the last trajectory sample ended because the aircraft touched down, not because the pilot stopped correcting. The two verdicts that mean "no correction could be observed" (`ramp_correction_not_stabilized_before_trajectory_end`, `no_real_post_peak_improvement`) keep the measured severity instead of adding a level. Reversals and re-aggravation are observable and still count as poor. |
| P2 | AoA persistence of 1 s | An AoA excursion shorter than one second is written to the report as a diagnostic but does not grade. Glideslope and lineup keep the two-sample guard. |
| P3 | AoA grades calibrated types only | The AoA axis affects the grade only for a type whose computed AoA has been checked against its cockpit indexer. Today that is the F/A-18C only. The F-14 and T-45 keep their AoA episodes as diagnostics until the calibration flight is flown. |

P3 is the complete candidate (`CatobarGradingPolicy::PROTOTYPE`).

---

## 2. The eight passes of 13 September

Produced by `lso grade-ab trap_records/recovery_13-09`, which re-grades the recorded gates, trajectory and datums of each report. The P0 column reproduces the recorded grade on all eight, which validates the re-grading path. Each cell shows the grade and, in brackets, the axis, zone and effective severity of the episode that decided it.

| report | type | outcome | DCS LSO | recorded | P0 baseline | P1 +touchdown ends correction | P2 +AoA 1 s persistence | P3 +AoA calibrated types only |
|---|---|---|---|---|---|---|---|---|
| 17:58 | T-45 | Trap, DCS wire 1 | C | `--` | `--` (AoA ramp 6.0) | `--` (AoA ramp 4.0) | `--` (AoA ramp 4.0) | `(OK)` (LU middle 2.4) |
| 18:08 | T-45 | T&G, hook up | none | `--` | `--` (AoA ramp 6.0) | `--` (AoA ramp 4.0) | `--` (AoA ramp 4.0) | `(OK)` (GS ramp 2.0) |
| 18:12 | T-45 | Trap, DCS wire 1 | C | `--` | `--` (AoA in close 4.5) | `--` (AoA in close 4.5) | `--` (AoA in close 4.5) | `(OK)` (GS ramp 2.0) |
| 18:20 | T-45 | T&G, hook up | none | `--` | `--` (AoA ramp 6.0) | `--` (AoA ramp 4.0) | `--` (AoA middle 3.6) | `(OK)` (GS ramp 2.0) |
| 18:26 | T-45 | Trap, DCS wire 2 | --- | `--` | `--` (AoA in close 4.5) | `--` (AoA in close 4.5) | `--` (AoA in close 4.5) | `(OK)` (GS ramp 2.0) |
| 18:37 | F-14 | T&G, hook up | none | `--` | `--` (GS ramp 6.0) | `--` (GS ramp 4.0) | `--` (GS ramp 4.0) | `--` (GS ramp 4.0) |
| 18:42 | F-14 | Trap, DCS wire 2 | C | `--` | `--` (AoA middle 3.6) | `--` (AoA middle 3.6) | `--` (AoA middle 3.6) | `OK` |
| 18:53 | F-14 | Trap, no DCS comment | none | `--` | `--` (AoA in close 4.5) | `--` (AoA in close 4.5) | `--` (AoA in close 4.5) | `OK` (GS in close 0.0) |

Reading the columns:

- **P1 alone changes no grade** on this set, but it does what it was meant to: every ramp episode drops one level (6.0 to 4.0). The reason it does not reach `(OK)` is that a "slow" or "fast" AoA reading is a medium severity, and medium at the ramp is 2 × 2.0 = 4.0 whatever the correction. The AoA axis, not the ramp rule, is what holds these passes at `--`.
- **P2 changes one deciding episode** (18:20: the 0.85 s ramp blip is dropped and the pass falls back to a 3.95 s "slow" episode in the middle zone, still `--`). The other AoA episodes on this set are 3.7 to 13.9 s long, well past one second, so a persistence rule cannot touch them.
- **P3 is where the set moves**: seven of eight become `(OK)` or `OK`. What decides them afterwards is the geometry that was there all along: lineup 2.7° left at the start of pass 1, and a 1-wire touchdown (0.8 to 0.9° "low" in the last 20 m) on the other T-45 passes, both now scored at their measured severity. The F-14 touch-and-go at 18:37 stays `--` on a 1.3° high, climbing ramp, which is a real deviation and is unaffected by any of the three rules.

So the honest reading is: the ramp rule (P1) was the visible mechanism, but the AoA axis is what actually decides these eight passes, and it decides them on a band that reads slow for the T-45 and fast for the F-14 on every pass ever recorded. Until the calibration flight settles whether that is the pilots or the band, P3 is the only column that grades these passes on what was measured with confidence.

---

## 3. The fourteen live fixtures of 2 and 3 September

Produced by `cargo test live_2026_09::grading_policy_ab_table -- --ignored --nocapture`. These recordings are replayed from ACMI, so no wind reference exists and the AoA axis never grades them; only P1 can move a row, and P2 and P3 are shown to confirm they change nothing here.

| fixture | type | outcome | DCS | recorded | P0 baseline | P1 +touchdown ends correction | P2 | P3 |
|---|---|---|---|---|---|---|---|---|
| t45_hookdown_wire1 | T-45 | Trap | wire 1 | `--` | `--` (GS ramp 4.0) | `(OK)` (GS start 2.0) | `(OK)` | `(OK)` |
| t45_hookdown_wire3 | T-45 | Trap | wire 3 | `--` | `--` (LU ramp 4.0) | `(OK)` (LU ramp 2.0) | `(OK)` | `(OK)` |
| t45_hookdown_wire4 | T-45 | Trap | wire 4 | `(OK)` | `(OK)` (LU ramp 2.0) | `(OK)` (LU ramp 2.0) | `(OK)` | `(OK)` |
| t45_hookdown_bolter | T-45 | Bolter | none | `B` | `B` | `B` | `B` | `B` |
| t45_hookup_1 | T-45 | T&G | none | `(OK)` | `(OK)` (LU in close 1.5) | `(OK)` (LU in close 1.5) | `(OK)` | `(OK)` |
| t45_hookup_2 | T-45 | T&G | none | `--` | `--` (LU ramp 4.0) | `(OK)` (LU ramp 2.0) | `(OK)` | `(OK)` |
| t45_hookup_3 | T-45 | T&G | none | `(OK)` | `(OK)` (GS ramp 2.0) | `(OK)` (GS ramp 2.0) | `(OK)` | `(OK)` |
| f14bu_hookdown_wire1 | F-14 | Trap | wire 1 | `--` | `--` (LU ramp 4.0) | `--` (LU ramp 4.0) | `--` | `--` |
| f14bu_hookdown_wire2 | F-14 | Trap | wire 2 | `(OK)` | `(OK)` (GS ramp 2.0) | `(OK)` (GS ramp 2.0) | `(OK)` | `(OK)` |
| f14bu_hookdown_wire4 | F-14 | Trap | wire 4 | `--` | `--` (LU ramp 4.0) | `--` (LU ramp 4.0) | `--` | `--` |
| f14bu_hookup_1 | F-14 | T&G | none | `--` | `--` (LU ramp 6.0) | `--` (LU ramp 6.0) | `--` | `--` |
| f14bu_hookup_2_dcs_waveoff | F-14 | T&G | none | `--` | `--` (GS in close 3.0) | `--` (GS in close 3.0) | `--` | `--` |
| f14bu_hookup_3 | F-14 | T&G | none | `(OK)` | `(OK)` (GS start 2.0) | `(OK)` (GS start 2.0) | `(OK)` | `(OK)` |
| f14bu_hookup_4 | F-14 | T&G | none | `--` | `--` (LU ramp 4.0) | `--` (LU ramp 4.0) | `--` | `--` |

Reading the rows:

- **P1 moves three T-45 passes from `--` to `(OK)`** (wire 1, wire 3, hook-up 2). In each, the deciding episode was a small deviation in the last seconds that could not stabilise before touchdown and was upgraded to medium. With the upgrade gone, the passes are graded on what remained: a small glideslope episode at the start, a small lineup episode at the ramp.
- **The five F-14 rows that stay `--` are not ramp-rule cases.** Their deciding episodes are lineup of 2.2 to 3.8° at the ramp with an "average" correction (real improvement, but late), or 1.9° high on glideslope held for 9 s. Those are medium and large severities on their own merits, and none of the three rules is meant to touch them. Whether a 2.5° lineup error at the ramp with the 150 m reference (about 6.5 m off centreline) deserves `--` is a separate threshold question for the human-LSO comparison.
- **Nothing that was `(OK)` or `B` changes**, and no pass moves down.

---

## 4. What was built

- `src/grading.rs`: `CatobarGradingPolicy` with `BASELINE` (the default, used by every production entry point) and `PROTOTYPE`; the three rules are implemented inside the episode classifier and the AoA axis setup; `compute_catobar_assessment_with_policy`, `compute_pass_grade_with_reason_and_policy` and `grade_from_gates_with_reason_and_policy` take an explicit policy, and the existing function names are unchanged wrappers over the baseline.
- `src/data.rs`: a new `aoa_grading_calibrated` flag on each aircraft type (F/A-18C true; F-14, T-45, AV-8B false) with the reason in the comment.
- `src/commands/grade_ab.rs`: `lso grade-ab <report or directory> [--episodes]`. Re-grades the recorded gates, trajectory and datums under the four steps and prints the Markdown table above; `--episodes` lists every episode under every step with its severity, correction verdict, reason and duration. It falls back to a geometry replay from `datums` only when a report carries no recorded trajectory, because the report's `alt` field is clamped at zero and a replay from it softens the last metres before touchdown.
- `src/tests.rs`: the ignored test `live_2026_09::grading_policy_ab_table` prints the same table for the fourteen fixtures; `LSO_GRADE_AB_EPISODES=1` adds the episode listing.

Verification on the branch:

| Check | Result |
|---|---|
| `cargo test --locked` | 343 + 3 passed, 1 ignored (the table test) |
| `cargo clippy --locked --all-targets -D warnings` | clean |
| `cargo fmt --check`, `git diff --check` | clean |
| `grade-ab` P0 column against the eight recorded grades | 8 of 8 identical |

---

## 5. What this does and does not settle

It settles the mechanism: on the 22 passes available, the touchdown rule removes the systematic one-level upgrade at the ramp and never lowers a grade, and the AoA axis is the single largest reason for `--` on the 13 September set. It also settles that the one-second AoA persistence, on its own, is nearly irrelevant: the AoA episodes that decide grades are long, not blips.

It does not settle whether the T-45 and F-14 AoA bands are right, and it does not settle whether a medium lineup error at the ramp should be `--`. Both need the human reference from step 8. The order I would suggest:

1. Fly the AoA calibration leg for the T-45 and the F-14 (donut centred, read the HUD AoA against `datums[].aoa`). If the bands move, re-run `grade-ab` on the same eight reports before deciding P3.
2. Adopt P1 regardless. It corrects a rule that penalises something the program cannot observe.
3. Keep P2 as a guard; it costs nothing and protects against the 18:20 case.
4. Have a human LSO grade the eight passes and the fourteen fixtures blind, then compare against the P3 column with the calibrated bands. That comparison is the first real calibration of the grader.

To switch production to the candidate once decided, change the default from `BASELINE` to `PROTOTYPE` in `CatobarGradingPolicy` and re-run the fixture tests; nothing else needs to move.
