# AoA calibration: client export plus alignment

Purpose: check the LSO program's computed angle of attack against the DCS flight model's own value, and find the true-AoA range the cockpit indexer calls "on speed" for the T-45C, the F-14 and the F/A-18C. Background: `docs/LIVE_SESSION_REVIEW_2026-09-13.md`, finding F1, and `docs/GRADING_POLICY_PROTOTYPE_2026-09-13.md`.

Two pieces:

- `LsoAoaExport.lua` runs on the **pilot's PC** inside DCS's export sandbox and writes a CSV at 20 Hz.
- `align_aoa.py` runs anywhere, takes that CSV and the LSO JSON reports of the same session, and prints the calibration result.

## 1. Install on the pilot's PC (once)

1. Copy `LsoAoaExport.lua` to `%USERPROFILE%\Saved Games\DCS\Scripts\LsoAoaExport.lua`.
2. Open `%USERPROFILE%\Saved Games\DCS\Scripts\Export.lua` and append this line at the end:

   ```lua
   pcall(function() local lsolfs=require('lfs'); dofile(lsolfs.writedir()..'Scripts/LsoAoaExport.lua') end)
   ```

   The script chains to the hooks already there (Tacview, SRS, SimShaker, DCS-BIOS all keep working).
3. On the server, `Saved Games\DCS.openbeta_server\Config\serverSettings.lua` must have `allow_ownship_export = true` (it already does if SimShaker or Tacview work on that server).

Nothing else to configure. A new file `Saved Games\DCS\Logs\lso_aoa_<date>-<time>.csv` appears each time a mission starts; a line `LSO-AOA: writing ...` in `Logs\dcs.log` confirms it.

## 2. What to fly

- Normal Case I pattern, the LSO program recording on the server as usual.
- At least four passes per type (T-45C and F-14). Hold the donut through the groove as you normally would; the script records what the indexer shows, so there is no need to note anything by hand.
- On two passes per type, hold the slow chevron (donut plus "V") for a few seconds in the middle of the groove, and on two others the fast chevron. This marks where the indexer switches.
- Calm wind is fine for the first run. A second run with 15 to 20 knots of deck wind in the mission is the natural follow-up, since the LSO's correction has never been exercised in real wind.

Afterwards send the CSV file(s) together with the usual server folder (JSON reports).

## 3. Run the alignment

```
python tools/aoa_calibration/align_aoa.py "<folder with lso_aoa_*.csv>" "<folder with LSO-*.json>" --markdown docs/AOA_CALIBRATION_<date>.md
```

For each pass it aligns the two clocks (a small constant offset between the client and the server is searched automatically), then prints:

- **LSO minus true AoA** over the groove: median, mean and spread. A constant value is an offset in our computation. A spread that follows bank or wind is a problem in the correction itself.
- **True AoA by indexer state**: what the flight model read while the cockpit showed donut only, donut plus chevron, or chevron only. That is the on-speed band in the units `src/data.rs` uses, per type.
- The band currently in the code next to the proposed one.

## 4. Columns of the CSV

| Column | Source |
|---|---|
| `model_time_s` | `LoGetModelTime()`, mission clock, same base as `datums[].time` in the reports |
| `aircraft`, `unit_name` | `LoGetSelfData()` |
| `lat`, `lon`, `alt_msl_m`, `heading_deg`, `pitch_deg`, `bank_deg` | `LoGetSelfData()` |
| `aoa_true_deg` | `LoGetAngleOfAttack()`, the flight model's own AoA in degrees |
| `tas_mps`, `ias_mps`, `vertical_speed_mps` | `LoGetTrueAirSpeed()`, `LoGetIndicatedAirSpeed()`, `LoGetVerticalVelocity()` |
| `wind_x_mps`, `wind_y_mps`, `wind_z_mps` | `LoGetWindVelocity()` at the aircraft, for comparison with the LSO's wind reference |
| `indexer_slow`, `indexer_opt`, `indexer_fast` | cockpit lamp arguments: T-45C 320/321/322, F-14 3760/3761/3762, F/A-18C 4/5/6 (1 = lit) |
| `gauge_raw`, `gauge_units` | cockpit AoA gauge: T-45C argument 840 (0 to 1 is 0 to 30 units), F-14 argument 2003 (assumed 0 to 30 units, confirm on the trace); empty on the F/A-18C, which has no gauge |
| `hud_aoa_units` | T-45C HUD "AOA" readout via `list_indication(10)`; empty on other types (the F/A-18C HUD alpha element is not wired yet) |
| `hook_draw_arg` | tail-hook animation value of your own aircraft (25 on T-45C/F/A-18C, 1305 on F-14), a second look at the hook sampler |

Argument numbers come from the VNAO T-45C v1.0.3 `Cockpit/Scripts/mainpanel_init.lua` and from the DCS-BIOS F-14 and F/A-18C module definitions. The F/A-18C is wired so that its data is collected the same way when Hornet passes start being flown; its band in `src/data.rs` is the only one taken from an external documented source, and this test will confirm or correct it like the others.
