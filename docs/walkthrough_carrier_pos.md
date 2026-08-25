# Walkthrough: Carrier Position EMA Smoothing

## Problem

DCS updates the carrier's world position in discrete steps (~every 1.4 s) rather than every simulation frame. When the LSO tool polls at 100 ms, this creates a **sawtooth pattern** in the approach datum `x` coordinate (distance to landing point along the angled deck), manifesting as periodic **stairstep drops of 10–20 ft** on the side-view trap sheet chart.

All 8 trap sheets in `trap sample/` exhibited this artifact to varying degrees.

## Change Made

#### [MODIFY] [track.rs](file:///c:/Users/thierry/Documents/GitHub/sevenfifty777/DCS-gRPC-lso/src/track.rs)

Added **exponential moving average (EMA) smoothing** of the carrier position used for approach geometry computation. The change consists of:

1. **New constant** `CARRIER_POS_SMOOTH_ALPHA = 0.15` (line 66) — documented with the rationale for the value and its impact on lag vs. smoothness.

2. **New field** `smoothed_carrier_pos: Option<DVec3>` on the `Track` struct (line 111) — stores the EMA-smoothed carrier position across frames.

3. **EMA logic** inserted in `Track::next()` after the pattern datum block (lines 216–231):
   - On the first frame, initializes to the raw carrier position
   - On subsequent frames, blends: `smoothed = prev + (raw - prev) × 0.15`

4. **Two substitutions** — `carrier.position` → `smoothed_pos` for:
   - `landing_pos` computation (line 237) — the origin of the approach x/y datum
   - `carrier_distance` computation (line 243) — the pattern exit range check

5. **Unchanged** — pattern datums (overhead circuit chart) still use raw `carrier.position` since the wide-scale view is unaffected.

## Validation

| Check | Result |
|-------|--------|
| `cargo build` | ✅ Compiles cleanly |
| `cargo test` | ⚠️ Pre-existing failure: `tests/recordings/` directory missing from local checkout (unrelated to this change) |
| Code review | ✅ Smoothing only applied to approach geometry; pattern datums, heading, and rotation untouched |

## Next Steps

To fully verify visually, deploy the fix and fly a few traps in DCS. The periodic stairstep drops visible in the current trap sheet charts should be eliminated, replaced by smooth descent curves.
