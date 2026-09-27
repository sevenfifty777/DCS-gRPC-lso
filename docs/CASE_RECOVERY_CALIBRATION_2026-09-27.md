# Recovery case calibration (phase 4): test list and examples

27 September 2026. Companion to `docs/CASE_RECOVERY_DETECTION_PLAN_2026-09-26.md` (section 6 and
phase 4). Nothing below has been run yet.

LSO now predicts the recovery case DCS's Marshal should order (Case I, II or III) and grades a
straight-in final with the Case III rules. The prediction follows the rule an ED developer stated,
but several of its inputs are LSO's best reading of DCS data, not something ED documented. Each
such reading is listed in the `assumptions` field of every prediction. This document lists every
test needed to confirm or replace them, with the mission settings to use, what LSO should predict,
and what to do when DCS disagrees.

## 1. The rule being checked

ED's rule, as LSO applies it (`src/recovery_case.rs`, `dcs-ed-statement-v1`):

```text
Case III  if dark
          or (cloud density > 8 and (ceiling < 1000 ft or precipitation))
          or fog visibility < 5 NM
Case I    else if cloud density < 6 or ceiling > 1000 ft
Case II   otherwise
```

Beside it, LSO stores a NATOPS reading for comparison only (`natops-minima-v1`, NAVAIR 00-80T-105
§4.2): Case III at night (30 minutes after sunset to 30 minutes before sunrise), or with a ceiling
below 1,000 ft, or visibility below 5 NM; Case I with a ceiling of 3,000 ft or more; Case II in
between. The NATOPS reading is doctrine and needs no calibration. It is shown in the examples only
so the two can be told apart.

What the pilot hears: the case is the first thing Marshal says in the check-in reply, for example
"CASE I recovery, expected BRC 270" or "CASE III recovery, CV-1 approach, expected final bearing
270".

## 2. What must be calibrated

| # | Item | LSO's current reading | Decides |
|---|---|---|---|
| C1 | ED's "dark" | sun more than 6° below the horizon at the carrier | Case III at dusk and dawn |
| C2 | Theatre time zones | fixed UTC offset per map, from MOOSE | the time used for the sun, on every map |
| C3 | Manual cloud density | the Mission Editor density (0 to 10), used as is | Case III / II / I with manual clouds |
| C4 | Ceiling | base of the lowest cloud layer, sea level reference, rounded to the foot | the 1,000 ft boundary |
| C5 | Precipitation | any `iprecptns` above 0 (rain, thunderstorm, snow, snowstorm) | Case III under dense cloud |
| C6 | Cloud presets | density from ED's METAR text in the preset name (FEW 2, SCT 4, BKN 7, OVC 9); ceiling = mission base + offset of the lowest covered layer | every mission using a preset |
| C7 | Fog | live value from `world.weather`; zero visibility or zero thickness means no fog | Case III in fog |
| C8 | Dynamic weather | not classified (`indeterminate`) | nothing: kept as decided (D5), recorded for information |
| C9 | Case III grading | straight-in groove at 3/4 NM, 3/4 NM gate required, `_OK_` without groove time | grades of straight-in passes |

C1 to C8 check the prediction and change no grade. C9 checks the only part that changes grades.

## 3. Setup

### 3.1 Where to run the tests

The Marshal voice is generated on the pilot's own PC, not on the server. The most useful ground
truth, a log line written by DCS's own speech code (3.3), therefore has to be on the machine of the
pilot who checks in. Two options:

- **Preferred: single player on a calibration PC** with DCS-gRPC and LSO running locally. The log
  line can be added freely there.
- **Multiplayer on a test server** that does not enforce the integrity check. Never on the
  dedicated server's normal sessions, and never on a client that joins a server with "pure scripts"
  enforced: the modified file would fail the check.

If neither is possible, listen to the call and write it down (3.4). That is enough for C1 to C5,
C7 and C8. C6 is much easier with the log line, because it also gives ED's own density and ceiling.

### 3.2 LSO build and the weather reading

Use a `lso.exe` built from the branch `feature/case-recovery-weather`. Before each check-in, take a
reading at the carrier (the unit name as written in the mission):

```powershell
# On the dedicated server, with the launch script
.\run-live-buffered.ps1 -Weather -Carrier "CVN-71" -WeatherLabel C3-d9-b200

# Or directly
.\lso.exe weather --raw --carrier "CVN-71" --output .\calib\C3-d9-b200.json
```

The fields to read in the output:

| Field | Meaning |
|---|---|
| `recovery_case.ordered` | LSO's prediction with ED's rule: `I`, `II`, `III` or `indeterminate` |
| `recovery_case.reasons` | why, e.g. `dense_cloud_low_ceiling`, `fog_below5_nm`, `dark` |
| `recovery_case.unknown_inputs` | why it is `indeterminate`, e.g. `missing_cloud_data` |
| `recovery_case.natops.case` | the NATOPS reading, for comparison |
| `recovery_case.conditions.sun_elevation_deg` | sun elevation LSO computed (C1, C2) |
| `recovery_case.conditions.cloud_density_0_10` | the density LSO used (C3, C6) |
| `recovery_case.conditions.ceiling` | the ceiling LSO used, in feet (C4, C6) |
| `recovery_case.conditions.fog` | `absent`, or `present` with visibility in NM (C7) |
| `query.runtime_fog_visibility_m`, `query.runtime_fog_thickness_m`, `query.fog2_raw` | raw fog values from DCS (C7) |

### 3.3 The speech log line (calibration PC only)

Back up `C:\Program Files\Eagle Dynamics\DCS World\Scripts\Speech\common.lua` first. A DCS update
or repair restores the original anyway.

In that file, find the two Marshal check-in replies,
`[base.Message.wMsgATCMarshallCopyInbound]` (Case I reply) and
`[base.Message.wMsgATCMarshallCopyInbound2and3]` (Case II and III reply). In each, add the lines
below right after `make = function(self, message, language)`:

```lua
            base.pcall(function()
                base.log.write('LSO-CALIB', base.log.INFO, base.string.format(
                    'case=%s clouds_density=%s clouds_ceiling=%s visibility=%s',
                    base.tostring(message.parameters.case),
                    base.tostring(message.parameters.clouds_density),
                    base.tostring(message.parameters.clouds_ceiling),
                    base.tostring(message.parameters.visibility)))
            end)
```

`pcall` makes sure a problem in the log line can never break the radio call. After each check-in,
`Saved Games\DCS\Logs\dcs.log` holds a line such as:

```text
... INFO    LSO-CALIB: case=2 clouds_density=9 clouds_ceiling=250 visibility=80000
```

- `case`: 0 = Case I, 1 = Case II, 2 = Case III. This is the value DCS decided, before any speech.
- `clouds_density`: ED's density out of 10, the value its rule compares with 6 and 8.
- `clouds_ceiling`: ED's ceiling, in metres (the speech code converts it to thousands of feet).
- `visibility`: metres; values above 19,000 are read as "ten plus miles".

Remove the lines (or restore the backup) when the campaign is over.

### 3.4 Record sheet

One row per check-in. Take the LSO reading first, then check in. Write the prediction down before
hearing the call.

| Test | Time (UTC) | DCS build | Theatre | Setting | LSO ED | LSO NATOPS | LSO density / ceiling | DCS `case` | DCS density / ceiling | Heard | Match | Notes |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| C3-e | 2026-10-01 18:05 | 2.9.29.27468 | Caucasus | manual 9/10, base 200 m, no rain | III | III | 9 / 656 ft | 2 | 9 / 200 | CASE III | yes | |

Keep the JSON file of each reading (`-WeatherLabel` names it) and the matching `dcs.log` excerpt.

### 3.5 Common mission settings

Unless a test says otherwise:

- Supercarrier (CVN-71 to 75) on a known theatre, sailing, with a flyable CATOBAR aircraft spawned
  in the air about 30 NM out, so Marshal can be called straight away.
- Static weather (`atmosphere_type` 0), no dust.
- Mission time around 12:00 local, so it is clearly daylight.
- Clear sky: manual clouds, density 0.
- No fog: `enable_fog` off **and** fog visibility and thickness set to 0. On DCS 2.9.29 the fog block
  is applied even with the switch off (live finding of 27 September 2026), so leftover values would
  add fog.
- Visibility 80 km.
- Change one factor at a time.

The Mission Editor shows cloud base in feet or metres depending on the unit setting. The tables
below give both.

## 4. Tests

### C1 — What ED calls "dark"

**LSO's reading.** Dark when the sun is more than 6° below the horizon at the carrier (end of civil
twilight). Hypothesis only: ED did not say. The function names in DCS's binaries
(`getDayLightCondition`, `getSunLightFactor`) suggest a light-level threshold, which may not match
an angle exactly.

**What it means in the cockpit.** At −6° the horizon is still visible and the sky is lit; the
brightest stars are just appearing. Somewhere between sunset and full dark, Marshal switches from
Case I to Case III.

**How to test.** Clear sky, no fog, so darkness is the only possible Case III reason. Take readings
at several mission times around sunset, using `sun_elevation_deg` from the LSO output to choose them
(adjust the mission start time until the value is close to the target):

| Test | Target sun elevation | LSO prediction | What a mismatch would mean |
|---|---|---|---|
| C1-a | +2° (sun just above horizon) | I | ED's dark starts before sunset: unlikely, report it |
| C1-b | 0° (sunset) | I | ED's dark starts at sunset |
| C1-c | −3° | I | threshold between 0° and −3° |
| C1-d | −6° | I or III (boundary) | reference point |
| C1-e | −9° | III | threshold beyond −9° |
| C1-f | −12° | III | threshold beyond −12° (nautical twilight) |

Repeat C1-b to C1-e once at dawn (sun rising), since a light-level test may not be symmetric.

**Result.** The elevation at which Marshal switches to Case III becomes `DARK_SUN_ELEVATION_DEG`
in `src/recovery_case.rs`. If the switch does not follow the elevation (for example it depends on
the moon), write down the moon phase and report it: the model would have to change.

### C2 — Theatre time zones

**LSO's reading.** DCS mission time is local theatre time with a fixed offset: Caucasus +4, Persian
Gulf +4, Nevada −8, Normandy 0, The Channel +2, Syria +3, Marianas +10, South Atlantic −3, Sinai
+2, Kola +3, Afghanistan +4.5, Iraq +3, Germany (Cold War) +1. Copied from MOOSE, not checked.

**What it means.** A wrong offset shifts LSO's sun by whole hours: it could call dark in daylight
or the reverse.

**How to test.** On each theatre you fly, set the mission time so LSO reports `sun_elevation_deg`
close to 0 (sunset), then look at the sun in game from the carrier deck or the cockpit.

| Test | Theatre | LSO sun elevation | Expected in game |
|---|---|---|---|
| C2-CA | Caucasus | about 0° | sun on the horizon |
| C2-PG | Persian Gulf | about 0° | sun on the horizon |
| C2-SY | Syria | about 0° | sun on the horizon |
| C2-MA | Marianas | about 0° | sun on the horizon |

A sun clearly above the horizon, or already gone, means the offset is wrong by roughly the time
difference; one hour moves the sun by about 10 to 15°. Correct `theatre_utc_offset_hours`.

### C3 — Manual cloud density (the 6 and 8 boundaries)

**LSO's reading.** With manual clouds, the Mission Editor density is ED's density, used as is.

**What it means.** Density is how much of the sky is covered, out of 10. ED's rule needs **more than
8** for Case III (with a low base or precipitation) and **less than 6** for an automatic Case I.

**How to test.** Manual clouds, no precipitation, 1,000 m thick.

| Test | Density | Base | LSO ED | LSO NATOPS | Why |
|---|---|---|---|---|---|
| C3-a | 5 | 3,000 m (9,843 ft) | I | I | density below 6 |
| C3-b | 5.9 | 200 m (656 ft) | I | III | density below 6, even with a low base |
| C3-c | 6 | 200 m (656 ft) | II | III | not below 6, not above 8, base not above 1,000 ft |
| C3-d | 8 | 200 m (656 ft) | II | III | 8 is not above 8 |
| C3-e | 9 | 200 m (656 ft) | III | III | above 8 with a base below 1,000 ft |
| C3-f | 9 | 457 m (1,499 ft) | I | II | above 8 but base above 1,000 ft and no rain |

C3-f is the case where ED and NATOPS disagree most clearly (a 1,500 ft overcast without rain).

**Result.** Compare the DCS `case` and `clouds_density` with LSO's. If DCS reports another density
than the one set in the editor, note both: LSO would have to read it the way DCS does.

### C4 — Ceiling and the 1,000 ft boundary

**LSO's reading.** The ceiling is the base of the cloud layer above sea level (the deck is close to
sea level), rounded to the foot. ED says "less than 1000 feet" for Case III and "more than 1000
feet" for Case I, so exactly 1,000 ft is neither.

**What it means.** 1,000 ft is the altitude of the Case I break and a normal pattern; a ceiling below
it means the pattern cannot be flown visually.

**How to test.** Density 7 (between 6 and 8, so only the ceiling decides between I and II), no
precipitation.

| Test | Base | LSO ceiling | LSO ED | LSO NATOPS |
|---|---|---|---|---|
| C4-a | 280 m | 919 ft | II | III |
| C4-b | 304.8 m | 1,000 ft | II | II |
| C4-c | 335 m | 1,099 ft | I | II |
| C4-d | 1,000 m | 3,281 ft | I | I |

**Result.** If C4-b comes out Case I, ED's comparison is "at least 1,000 ft", not "more than". If
C4-a comes out Case I, ED's ceiling is not the base (for example base plus some margin). Compare
`clouds_ceiling` in the log with the base you set.

### C5 — Precipitation

**LSO's reading.** Any precipitation code above 0 counts as "precipitation" in ED's rule: 1 rain,
2 thunderstorm, 3 snow, 4 snowstorm.

**What it means.** Under dense cloud, rain or snow alone makes it Case III, even with a high base.

**How to test.** Density 9, base 900 m (2,953 ft), so the base alone would give Case I.

| Test | Precipitation | LSO ED | LSO NATOPS |
|---|---|---|---|
| C5-a | none | I | II |
| C5-b | rain | III | II |
| C5-c | thunderstorm | III | II |
| C5-d | snow (needs a cold season temperature) | III | II |
| C5-e | snowstorm | III | II |
| C5-f | rain, density 7 | I | II |

C5-f checks that precipitation alone is not enough without dense cloud. NATOPS has no
precipitation rule, so it stays Case II (ceiling between 1,000 and 3,000 ft) throughout.

**Result.** Any code DCS does not treat as precipitation (for example snow) must be excluded in
`derive_conditions`.

### C6 — Cloud presets

**LSO's reading.** Two hypotheses:

1. **Density.** DCS's `coverage` parameter is a rendering value (even "Overcast" presets stay below
   0.9), so LSO reads ED's own description in the preset name instead: the METAR code of the lowest
   layer, FEW = 2, SCT = 4, BKN = 7, OVC = 9, and the mean for a pair (BKN/OVC = 8). Presets with no
   METAR text have an unknown density.
2. **Ceiling.** The mission's cloud base is the base of the preset's lowest layer; the lowest
   *covered* layer keeps its offset above it.

**What it means.** Most missions use presets, so this decides the prediction for most sessions.

**Important limit.** No ED preset can put the cloud base below 1,000 ft: their lowest base is 420 m
(1,378 ft). Only ATMOS-X presets go lower. Several examples below are therefore Case I with ED's
rule even under a full overcast, because the base stays above 1,000 ft and there is no rain.

**How to test.** Use each preset at the base given, then compare DCS's `clouds_density` and
`clouds_ceiling` (log line) with LSO's `cloud_density_0_10` and `ceiling`.

| Test | Preset (editor name) | Base (allowed range) | ED METAR | LSO density | LSO ED | LSO NATOPS |
|---|---|---|---|---|---|---|
| C6-a | Preset1, Light Scattered 1 | 840 m (840–4,200) | FEW/SCT | 3 | I | I |
| C6-b | Preset7, Scattered 3 | 1,680 m (1,680–5,040) | BKN | 7 | I | I |
| C6-c | Preset21, Overcast 1 | 1,260 m (1,260–4,200) | BKN/OVC | 8 | I | I |
| C6-d | Preset25, Overcast 5 | 420 m (420–3,360) | OVC | 9 | I | II |
| C6-e | Preset27, Overcast 7 | 420 m (420–2,520) | OVC | 9 | I | II |
| C6-f | RainyPreset1, Overcast And Rain 1 | 420 m (420–2,940) | OVC + rain | 9 | III | II |
| C6-g | Preset65, Nimbostratus with heavy rain | 609 m (609–6,096) | OVC + rain | 9 | III | II |
| C6-h | Preset40, Scattered Showers 6 | 200 m (200–10,000) | none (text only) | unknown | indeterminate | indeterminate |
| C6-i | Preset72, Cirrostratus 4 | 5,000 m (5,000–18,288) | OVC | 9 | I | I |

C6-d and C6-e are the key tests of the density reading: a full overcast at 1,378 ft without rain is
Case I with ED's rule but Case II for NATOPS. C6-h tells what ED does with a preset LSO cannot read.
C6-i checks the ceiling offset: its lowest covered layer sits 3,904 m above the preset's first
layer, so LSO puts the ceiling at about 29,200 ft.

**Result.** The log gives ED's density and ceiling for each preset. If they differ from LSO's, the
mapping in `CloudPreset::metar_cover_densities` (`src/cloud_presets.rs`) or the ceiling offset in
`derive_conditions` changes. With enough presets logged, ED's values could replace the hypothesis
entirely as a checked-in table.

### C7 — Fog

**LSO's reading.** The live values `world.weather.getFogVisibilityDistance()` and
`getFogThickness()` decide; the mission's fog switches do not (confirmed on 27 September 2026: a
mission with the switch off showed 2,000 m fog, and DCS reported it). A value of 0 for either is
read as "no fog". That last point is still a hypothesis.

**What it means.** Fog under 5 NM visibility (9,260 m) gives Case III whatever the clouds: the
carrier cannot be seen from the break.

| Test | Fog setting | LSO fog | LSO ED | LSO NATOPS |
|---|---|---|---|---|
| C7-a | none: switch off, visibility 0, thickness 0 | absent (if the live values read 0) | I | I |
| C7-b | legacy fog on, 7,408 m (4 NM), 300 m thick | present, 4.0 NM | III | III |
| C7-c | legacy fog on, 11,112 m (6 NM), 300 m thick | present, 6.0 NM | I | I |
| C7-d | legacy fog **off** but 2,000 m / 1,000 m left in the fields | present, 1.1 NM | III | III |
| C7-e | new fog (fog2), each mode available in the editor | read from the live values | depends | depends |
| C7-f | legacy fog on, 7,408 m, only 10 m thick | present, 4.0 NM | III | III |

- C7-a answers the open question: what DCS returns with no fog at all. Save its JSON.
- C7-d confirms the switch finding on a second mission.
- C7-e: save `query.fog2_raw` for each mode; LSO still does not know what each `fog2.mode` means.
- C7-f checks whether ED ignores very thin fog, far below the approach path.

**Result.** If C7-a returns non-zero values, the "no fog" test in `derive_conditions` must use
another signal. If C7-f is Case I for DCS, ED also looks at thickness.

### C8 — Dynamic weather (information only)

**LSO's reading.** Dynamic weather is not classified: `indeterminate` by day, Case III at night
(darkness alone decides). Decision D5, not up for change.

| Test | Setting | LSO ED |
|---|---|---|
| C8-a | dynamic weather, day | indeterminate |
| C8-b | dynamic weather, night (sun below −12°) | III |

Note what Marshal calls, for the record only.

### C9 — Case III grading (the only part that changes grades)

**LSO's reading.** A straight-in final (wings level within 10°, within 5° of the centerline, from
beyond 2 NM to 3/4 NM, no telemetry gap over 1 s) enters the groove at 3/4 NM. From there the pass
is graded as in Case I, except that the 3/4 NM gate is always required and `_OK_` does not need the
15–18 s groove time.

**What it means in the cockpit.** The pilot flies a long, straight final, as on an ICLS/ACLS
approach, and is graded from the ball call at 3/4 NM, like a human LSO would.

These tests need a normal LSO recording (`run-live-buffered.ps1`), not a weather reading. Look at
the JSON report of each pass.

| Test | Conditions | What to fly | Expected in the report |
|---|---|---|---|
| C9-a | night (Case III ordered) | full Case III: Marshal stack, long straight final | `groove_entry.trigger` = `case_iii_straight_in_three_quarter_nm`, entry about 1,389 m; `gate_deviations.case_iii_straight_in` = true; `recovery_case.flown_approach.kind` = `straight_in`; `mismatch` = false; a grade |
| C9-b | clear day (Case I ordered) | straight-in from 4 NM without the break | same groove entry and grade as C9-a; `mismatch` = true, `diagnostics` = `approach_does_not_match_ordered_case`; no penalty |
| C9-c | night or fog (Case III ordered) | normal overhead pattern | Case I groove entry (`case_i_port_final_turn_rollout_sustained`); `flown_approach` = `overhead_pattern`; `mismatch` = true |
| C9-d | clear day | normal overhead pattern | Case I groove entry, `mismatch` = false: today's behaviour unchanged |
| C9-e | night | straight-in with a correction turn (bank over 10°) between 2 NM and 3/4 NM | no Case III entry (the segment breaks); report it: such a pass is not graded as a straight-in |
| C9-f | night | straight-in flown 3° off centerline all the way | Case III entry, lineup deviation graded, not blocked |
| C9-g | night | a very clean straight-in | `_OK_` possible without any groove-time condition |

For C9-a, C9-f and C9-g, ask a human LSO to grade the same passes (from the ACMI or live) and
compare. For the passes already recorded, look at the four straight-ins found on the local corpora
(20 September 09:25, 12 September 17:54, 8 September 19:12 and 6 September 03:20 UTC) with their
pattern PNG, and confirm they were real long finals.

**Result.** If human grades disagree with LSO's on Case III passes, the thresholds to review are in
`src/flown_approach.rs` (straight-in definition) and the `case_iii_straight_in` rules in
`src/track.rs` and `src/grading.rs`.

## 5. Order of the campaign

1. C7-a (no-fog reading): quick, and needed to trust every other result.
2. C3, C4, C5 with manual clouds: one mission, change the weather between check-ins.
3. C6 presets: one mission per preset, or change the preset between check-ins.
4. C7-b to C7-f fog.
5. C1 dark (dusk and dawn) and C2 time zones on each theatre used.
6. C8 dynamic weather.
7. C9 grading, with a night Case III session and a human LSO if possible.

## 6. After the campaign

- Send the record sheet, the JSON readings and the `dcs.log` excerpts.
- Each confirmed hypothesis is removed from `ASSUMPTIONS` in `src/recovery_case.rs`; each
  corrected one changes its constant or function, with the tested conditions turned into unit
  tests (one test per row of the record sheet).
- If a preset or theatre is still untested, it stays a hypothesis and keeps its name in the
  `assumptions` field of every prediction.
- `tasking-roadmap.md` is updated with what remains open.
