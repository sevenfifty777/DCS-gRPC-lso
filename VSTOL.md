# AV-8B / LHA Tarawa V/STOL support

This branch keeps the native CATOBAR path intact and adds a separate recovery path for:

- Aircraft: `AV8BNA`
- Carrier: `LHA_Tarawa`

## Geometry

- AV-8B reference: pilot station projected vertically onto the calibrated ground-contact plane: `(3.43, -1.89645, 0.0)` m aircraft-local.
- Tarawa spot 7.5: `(-3.10, 19.95, -64.81)` m carrier-local.
- Ideal V/STOL approach axis: parallel to Tarawa BRC, `27.24 m` port of ship centerline (18 m half deck width + 9.24 m AV-8B wingspan).
- Ideal glide slope: `3.0°`.
- Hover reference at the 7.5 longitudinal station: `120 ft` above water.
- AV-8B target AoA for trace colouring: `10–12°`. AoA is not part of the numeric V/STOL grade.

## Recovery grading

The V/STOL approach uses the same GS/LU severity thresholds as the CATOBAR gate logic, evaluated independently at:

- 3/4 NM
- 1/2 NM
- 1/4 NM

For V/STOL only, the numeric approach score is the arithmetic mean of the three gate scores. CATOBAR continues to use its native grading path unchanged.

Gate points:

- `OK` = 4.0
- `(OK)` = 3.0
- `--` = 2.0
- `C` = 0.0

Spot 7.5 bonus:

- A: `< 1 m` = `+1.00`
- B: `1–<3 m` = `+0.75`
- C: `3–<5 m` = `+0.50`
- D: `>=5 m` = `+0.00`

Final V/STOL points = averaged approach points + spot bonus, capped at 5.0.

## Rendering

V/STOL uses its own two-panel renderer and Tarawa assets:

- `img/tarawa-vstol-recovery-side.png`
- `img/tarawa-vstol-recovery-top.png`
- `img/tarawa-vstol-pattern-top.png`

The side and top recovery sprites use the same displayed longitudinal scale and are independently anchored to the calibrated 7.5 reference. The CATOBAR renderer and carrier assets remain on their original branch.

## Production cleanup

Calibration/debug tooling is intentionally not included in this production source tree. The normal CLI remains the original `run` / `file` workflow.
