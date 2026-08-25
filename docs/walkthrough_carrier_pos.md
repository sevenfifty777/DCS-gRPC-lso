# Walkthrough: Carrier Position EMA Smoothing

**Status:** Implemented

**Related record:** [implementation_plan_carrier_pos.md](implementation_plan_carrier_pos.md)

DCS carrier positions can advance in discrete steps while LSO samples every 100 ms. The final
approach geometry now applies an exponential moving average with alpha `0.15`:

```text
smoothed = previous + (raw - previous) * 0.15
```

The implementation is in [`src/track.rs`](../src/track.rs):

1. The first sample initializes `smoothed_carrier_pos` from the raw position.
2. Later samples move the stored position 15 percent toward the newest raw value.
3. The smoothed position defines the final-approach landing origin and distance.
4. Pattern-chart coordinates and cable estimation remain based on raw transforms.

This separation reduces sawtooth artifacts without changing the connector-based wire calculation.

For visual verification, run:

```powershell
cargo test generate_chart_images -- --nocapture
```

Inspect the generated files under `target/test-charts/` and compare them with representative live
trap sheets. The test generates artifacts for human inspection; it is not a pixel-regression test.
