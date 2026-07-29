# Fix: Smooth Carrier Position to Eliminate Approach Sawtooth

## Problem

DCS updates the carrier's position in discrete steps (~every 1.4 seconds), not smoothly every frame. Since the LSO tool polls at 100ms, 13 out of every 14 frames see the carrier at the same position, then one frame jumps ahead by ~10m. This makes the datum `x` coordinate (distance to landing point along the angled deck) sawtooth, creating visible stairstep drops of 10–20ft on the side-view chart and potentially skewing gate deviation measurements.

## Proposed Changes

The fix applies **exponential moving average (EMA) smoothing** to the carrier position used for approach geometry computations. This is a single-file change in `track.rs`.

> [!IMPORTANT]
> The smoothing is applied only to the carrier position used for **approach datum** and **gate deviation** computations. The **pattern datum** (overhead circuit chart) continues to use the raw carrier position, since the wide-scale pattern chart is not visually affected and needs no correction.

---

### Track Module

#### [MODIFY] [track.rs](file:///c:/Users/thierry/Documents/GitHub/sevenfifty777/DCS-gRPC-lso/src/track.rs)

**1. Add smoothing constant and field to `Track` struct (lines 78–94)**

Add a constant for the EMA smoothing factor and a new field to store the smoothed carrier position:

```rust
/// EMA smoothing factor for carrier position.
/// α = 0.15 spreads a ~14-frame DCS position step over ~18 frames,
/// introducing ~0.6s positional lag (~4.6m at 15 kts). The resulting
/// gate-distance error is < 0.5%, well within acceptable tolerance.
const CARRIER_POS_SMOOTH_ALPHA: f64 = 0.15;
```

Add to `Track` struct:
```rust
/// Exponentially smoothed carrier position used for approach geometry.
/// Eliminates the sawtooth caused by DCS updating the carrier's world
/// position in discrete steps (~every 1.4 s) rather than every frame.
smoothed_carrier_pos: Option<DVec3>,
```

**2. Initialise the new field in `Track::new()` (line 151–164)**

```rust
smoothed_carrier_pos: None,
```

**3. Apply smoothing at the top of `Track::next()` (after pattern datum block, before line 197)**

Insert a smoothing step that computes `smoothed_pos` from the raw `carrier.position`:

```rust
// Smooth carrier position to eliminate DCS quantisation sawtooth.
// The carrier's world position updates in discrete jumps (~every 1.4 s);
// between updates, the same stale position is returned. EMA blends the
// raw position toward the smoothed estimate each frame, producing a
// steady progression instead of a stairstep.
let smoothed_pos = match self.smoothed_carrier_pos {
    Some(prev) => {
        let s = prev + (carrier.position - prev) * CARRIER_POS_SMOOTH_ALPHA;
        self.smoothed_carrier_pos = Some(s);
        s
    }
    None => {
        self.smoothed_carrier_pos = Some(carrier.position);
        carrier.position
    }
};
```

**4. Replace `carrier.position` with `smoothed_pos` in approach computations (lines 197–211)**

Three specific substitutions:

| Line | Current | Replacement |
|------|---------|-------------|
| 201 | `carrier.position + landing_pos_offset` | `smoothed_pos + landing_pos_offset` |
| 211 | `(carrier.position - plane.position).mag()` | `(smoothed_pos - plane.position).mag()` |

The following usages remain **unchanged** (raw `carrier.position`):
- Pattern datum BRC frame computation (line 181) — wide-scale chart unaffected
- `carrier.heading` / `carrier.rotation` — heading changes slowly, not quantised

---

## Open Questions

> [!NOTE]
> **Alpha value tuning**: The proposed α = 0.15 provides a good balance between smoothness and lag. A higher α (e.g. 0.25) would reduce lag but leave more sawtooth visible; a lower α (e.g. 0.10) would be smoother but add more lag. The lag at α = 0.15 is ~4.6m, causing < 0.5% error in gate distance — negligible for grading. Would you like a different trade-off?

## Verification Plan

### Automated Tests

Existing tests replay TacView recordings through the same `Track::next()` code path:

```bash
cargo test
```

The 5 wire-detection tests (`wire_1_01` through `wire_4_02`) must continue to detect the correct wire number. The smoothing should not change wire detection since it only affects approach geometry, not the landing/cable estimation.

### Visual Verification

Regenerate the test chart images and compare before/after:

```bash
cargo test generate_chart_images -- --nocapture
```

The output PNGs in `target/test-charts/` should show smoother approach curves without the stairstep drops. The gate deviation labels should also appear more realistic (no more jumps of 10–20ft between consecutive points).

### Manual Verification

After deploying the fix, have pilots fly a few traps and compare the new trap sheet charts against the samples in `trap sample/`. The periodic stairstep drops should be eliminated.
