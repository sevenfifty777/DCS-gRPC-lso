# Grading branch review

## Review status

- Branch: `grading`
- Base: `origin/main` at `88224a9`
- Implementation commit: `257c4a7 Restore CATOBAR grading alongside V/STOL`
- Pull request: not created; the branch is available on `origin/grading` for review.
- Comparison copy: `modfi/grading.rs` remains untracked and is not part of the commit.

The implementation commit changes eight source files: 271 insertions and 72 deletions relative to
`origin/main`.

## Why more than grading.rs changed

The V/STOL merge removed `IntentionalBolter` from `grading.rs`, but it also removed the enum variant,
hook-state collection, detection state, output labels, and wire reporting from other files. Restoring
only `grading.rs` would not compile and would not restore qualification touch-and-go detection.

The supporting edits below are the minimum required to restore that CATOBAR behavior while keeping
V/STOL grading separate.

## Behavioral overview

| Recovery path | Hook handling | Approach grade | Final bonus | Display outcome |
| --- | --- | --- | --- | --- |
| CATOBAR recovered | Live hook state is sampled | Existing gate, wire, and groove-time logic | None | Wire or landed |
| CATOBAR qualification touch-and-go | Raised hook is detected and the closest deck point is retained | Gate grade when a cable can be estimated; otherwise `B` | None | `Qualif Bolter` |
| CATOBAR normal bolter | Hook is not raised | `B` | None | `Bolter` |
| V/STOL recovered | Hook sampling is bypassed | Average of available gate points | Spot 7.5 bonus | `V/STOL recovery` / `Spot 7.5` |
| V/STOL bolter or waveoff | Hook sampling is bypassed | Existing outcome points | None | Bolter or waveoff |

The `!carrier_info.is_vstol()` guards are important. Without them, an AV-8B hook/draw-argument value
could incorrectly turn a valid V/STOL recovery into a qualification bolter.

## File-by-file changes

### src/grading.rs

CATOBAR recovery restoration:

- Restores `Grading::IntentionalBolter` handling in `compute_pass_grade`.
- A qualification pass with an estimated cable uses the normal CATOBAR gate grade.
- A qualification pass without an estimated cable returns `PassGrade::Bolter` while retaining its
  `IntentionalBolter` outcome.
- Treats `IntentionalBolter` as a bolter in the V/STOL approach function. This makes the match
  exhaustive but does not route V/STOL recoveries through CATOBAR wire logic.

V/STOL grading correction:

- Keeps the upstream spot grades and bonuses:

  | Distance to spot 7.5 | Spot grade | Bonus |
  | --- | --- | --- |
  | `< 1 m` | A | 1.00 |
  | `1 m` to `< 3 m` | B | 0.75 |
  | `3 m` to `< 5 m` | C | 0.50 |
  | `>= 5 m` | D | 0.00 |

- Keeps the arithmetic average of available V/STOL gate scores.
- Corrects the average-to-label mapping to use grade midpoints:

  | Averaged points | Approach label |
  | --- | --- |
  | `>= 3.5` | `OK` |
  | `>= 2.5` and `< 3.5` | `(OK)` |
  | `>= 1.0` and `< 2.5` | `--` |
  | `< 1.0` | `C` |

  This resolves an upstream contradiction: the V/STOL test expected `(3 + 4 + 4) / 3 = 3.67` to
  remain `OK`, while the old mapping required a full 4.0 and therefore returned `(OK)`.

Test changes:

- Replaces misleading CATOBAR threshold names/comments with exact inclusive boundary tests at
  0.5 degrees slight glideslope and 1.0 degree significant glideslope.
- Adds qualification-bolter tests with and without an estimated cable.
- Keeps V/STOL tests separate and expands them to cover spot boundaries, labels, bonus values,
  averaged gate behavior, fractional final points, midpoint boundaries, and non-recovery outcomes.

### src/track.rs

- Restores `Grading::IntentionalBolter { cable_estimated }`.
- Restores closest-point transforms and raised-hook tracking for arrested recoveries.
- Extends `Track::next` with a `hook_state` argument.
- Detects a raised-hook qualification touch-and-go after touchdown or after crossing the deck without
  a DCS touchdown event.
- Preserves normal bolter detection when the hook was not raised.
- Limits hook tracking and closest-point qualification detection to non-V/STOL carriers.
- Keeps all upstream V/STOL gate, spot-distance, and final-bonus calculations.

### src/client/unit_client.rs

- Restores `get_draw_argument_value`, used to read DCS draw argument 25 for hook state.

### src/tasks/record_recovery.rs

- Samples draw argument 25 during live CATOBAR tracking and at the runway-touch event.
- Bypasses the hook RPC for V/STOL and supplies the neutral hook-down value instead.
- Passes hook state into `Track::next`.
- Reports the estimated cable for qualification passes.
- Restores `Qualif Bolter` in both CATOBAR and V/STOL-safe Discord outcome matches.
- Handles the current gRPC stub's optional unit type when generating the initial Tacview update.

### src/commands/file.rs

- Updates offline ACMI processing for the new `Track::next` signature.
- Uses hook-down (`1.0`) because recorded Tacview files do not include the live hook draw argument.
  This prevents offline extraction from inventing qualification bolters.

### src/draw.rs

- Restores the `Qualif Bolter` label on recovery charts.

### src/transform.rs

- Restores `Clone` on `Transform`, required to retain carrier and aircraft transforms at the closest
  deck point for cable estimation.

### src/commands/run.rs

These four small compatibility changes are not grading behavior. Current `origin/main` did not
compile against its pinned gRPC stubs because `Unit.type` is now `Option<String>`:

- Converts optional aircraft type to a string when storing or spawning a tracked aircraft.
- Uses `as_deref().and_then(...)` when identifying aircraft and carrier types.
- A missing type now produces no candidate instead of a type mismatch.

Without these compatibility changes, `cargo test` fails before any grading test can run.

## Validation performed

```text
cargo test grading::tests --no-fail-fast
34 passed; 0 failed

cargo test --no-fail-fast
49 passed; 0 failed

git diff --check
passed
```

The complete suite includes the existing ACMI recovery tests for F/A-18, F-14, and T-45 wire
detection.

## Known limitations and review points

- Current `origin/main` no longer has a dedicated database `outcome` column. The restored
  qualification outcome is present in the serialized `Grading` value, chart, Discord output, and
  estimated wire, but is not stored as a separate SQLite outcome field in this branch.
- Live hook RPC failures default to `1.0` (hook down). This avoids false qualification-bolter
  classifications but can miss one if DCS-gRPC cannot supply the hook argument.
- Offline ACMI extraction cannot distinguish hook-up qualification passes because the required draw
  argument is not recorded.
- The V/STOL midpoint mapping is a behavior correction, not only a test rename. Reviewers should
  confirm that nearest-grade midpoint mapping is the desired policy.

## Suggested review commands

```powershell
git fetch origin
git switch grading
git diff origin/main...grading -- src/grading.rs
git diff origin/main...grading -- src/track.rs src/tasks/record_recovery.rs
git diff origin/main...grading --stat
cargo test --no-fail-fast
```

The untracked comparison file can still be compared directly:

```powershell
git diff --no-index -- src/grading.rs modfi/grading.rs
```
