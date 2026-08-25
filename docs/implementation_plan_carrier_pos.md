# Carrier Position Smoothing - Implementation Record

**Status:** Implemented in commit `8a4d87f` on 2026-07-30

**Primary source:** [`src/track.rs`](../src/track.rs)

## Problem

DCS can expose a moving carrier position in visible steps while LSO samples at 100 ms. Using the raw
position for every final-approach datum caused sawtooth artifacts and unstable gate measurements.

## Implemented design

- `CARRIER_POS_SMOOTH_ALPHA` is fixed at `0.15`.
- `Track::smoothed_carrier_pos` retains the exponential moving average between samples.
- `Track::next` initializes the average from the first raw carrier position and then applies
  `previous + (raw - previous) * alpha`.
- The smoothed position is used for the optimal landing origin and final-approach distance.
- The overhead `PatternDatum` continues to use raw carrier position.
- Cable estimation continues to use the raw touchdown transforms and connector geometry.

This keeps the visualization fix out of the physical cable calculation.

## Validation

The repository includes:

- five ACMI fixtures asserting expected and estimated wires;
- `cargo test generate_chart_images -- --nocapture`, which writes approach and pattern images under
  `target/test-charts/` for manual comparison; and
- real trap samples under `trap sample/` for visual review.

The alpha value remains compile-time behavior; there is no runtime tuning option.
