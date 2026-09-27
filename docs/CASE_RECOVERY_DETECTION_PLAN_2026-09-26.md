# Recovery case detection (Case I / II / III): analysis and plan

26 September 2026, decisions D1–D6 taken on 27 September 2026. Status on 27 September 2026:
phases 1, 2, 3, 5 and 6 implemented on branch `feature/case-recovery-weather`; phase 1 passed its
live smoke test on the dedicated server; phase 4 (calibration) is still to do, and phase 6 needs
its live Case III session. Nothing beyond the phase 1 smoke test has been validated live.

Implementation notes that differ from the text below:

- **Preset density.** DCS's `coverage` is a rendering parameter (the "Overcast" presets stay
  between 0.61 and 0.90), so "density = 10 × coverage" was dropped. A preset's density comes from
  ED's own METAR description in its name (FEW 2, SCT 4, BKN 7, OVC 9, the mean for a pair such as
  `BKN/OVC`), which matches CRT's hand-written table. Presets without a METAR description give an
  unknown density.
- **NATOPS ceiling of a preset.** Taken from the same METAR codes: the lowest layer's base if it
  is BKN/OVC, no ceiling if no layer is, unknown if a denser layer sits above a thin lowest one.
- **Migration 9** also holds `night` and `flown_approach`.
- **`CaseIIIGrooveDetector`** uses the same straight-in rule as `flown_approach` and enters the
  groove at the first sample inside 3/4 NM. Lineup within 2°, the glideslope box and the 0.75 s
  hold from section 7 are not entry conditions: as with the Case I roll-out, they would leave a
  real straight-in ungraded for the very deviation it should be graded on; the segment itself
  already proves a sustained wings-level final. The first detector to confirm owns the groove.
  The Case III rules travel with the gates as `gate_deviations.case_iii_straight_in`.
- **`flown_approach`** checks the continuity of the segment (no gap over 1 s, no outbound motion)
  and requires lineup within 5°. On the local corpora it classifies the 20 September straight-in
  as `straight_in` and the Case I passes as `overhead_pattern`; three other `straight_in` passes
  (6, 8 and 12 September) need a human look before phase 6.

## 1. What we are trying to know

DCS's built-in carrier ATC (Marshal) decides the recovery case for the pilot. LSO grades today as if
every CATOBAR pass were a Case I pass: the groove only starts after a port final turn and roll-out
(`CaseIGrooveDetector`, `src/track.rs`), and the 3/4 NM gate is ignored when it falls before that
roll-out. That is right for Case I and Case II. It is wrong for Case III, where the pilot flies a
long straight-in final and the ball call is at approach minimums, about 3/4 NM (NAVAIR 00-80T-104
§6.6.3.1 items 1 and 4). The straight-in of 20 September at 09:25 (`docs/RECOVERY_REVIEW_2026-09-20.md`
section 5) got no groove, no points and a fallback wire for exactly this reason.

So LSO needs two facts per pass:

1. **Ordered case**: which case DCS's ATC would have ordered, from the weather and the time of day.
2. **Flown approach**: what the pilot actually flew (break and port turn, or straight-in), from the
   geometry LSO already records.

The ordered case chooses the grading rules. The flown approach is checked against it.

## 2. The rule DCS uses (ED developer statement)

`tools/case_recovery-detection/conditions.txt`, as written by the ED developer:

> CASE III when it is dark, or cloud density > 8 (of 10 max) and (cloud ceiling less than 1000 feet
> or there is precipitation), or fog transparency is less than 5% on 5 NM distance.
> Else CASE I when cloud density < 6 (of 10 max) or cloud ceiling more than 1000 feet and fog
> transparency is better than 5% on 5 NM distance.
> If none of the conditions is met, it will call a CASE II.

Written as code:

```text
if dark
   or (density > 8 and (ceiling < 1000 ft or precipitation))
   or fog_visibility < 5 NM                      -> Case III
else if density < 6 or ceiling > 1000 ft         -> Case I
else                                             -> Case II
```

Three points about this reading:

- **The Case I sentence is ambiguous, but the ambiguity does not matter.** Whether it reads
  `(density < 6) or (ceiling > 1000 and fog ok)` or `(density < 6 or ceiling > 1000) and fog ok`, the
  fog clause is always true by the time we reach it, because bad fog has already produced Case III.
  Both readings reduce to `density < 6 or ceiling > 1000 ft`.
- **"Fog transparency below 5% at 5 NM" is the same as "fog visibility below 5 NM".** Meteorological
  visibility (WMO meteorological optical range) is defined as the distance at which light is
  attenuated to 5%. So the rule is a plain 5 NM visibility limit. This is our interpretation; it
  must be confirmed by the calibration in section 6.
- **This is not the NATOPS rule.** NATOPS uses a 3,000 ft ceiling for Case I and 1,000 ft for Case II.
  With DCS's rule, a 1,500 ft overcast without rain is Case I, and Case II only happens with
  6–8/10 cloud and a ceiling at or below 1,000 ft. We follow DCS, because DCS's ATC is what tells
  the pilot what to fly (D2). The NATOPS reading is stored beside it as a diagnostic only (D6,
  section 5.1).

**What the DCS install shows (checked 27 September 2026).** The rule is not in any Lua file:

- The Marshal radio calls are built in `Scripts/Speech/common.lua` (the check-in replies around lines
  1680–1810). They receive the case already decided, `message.parameters.case` (0 = Case I, 1 =
  Case II, 2 = Case III), together with the weather DCS used: `visibility` (metres),
  `clouds_density` (0–10) and `clouds_ceiling`. The Lua only turns these into speech (density above
  2 is read as "scattered clouds", 9 and above as "solid layer"). Those thresholds choose the words,
  not the case, although "solid layer from 9" matches the "density > 8" of ED's rule.
- The decision is made in native code: `woATC::getRecoveryCASE()`, exported by `bin/Flight.dll`
  and called by `Mods/tech/Supercarrier/bin/edSupercarrier.dll`. It takes no arguments, so it reads
  the weather and time itself.
- "Dark" most likely comes from `getDayLightCondition(float) -> bool`, exported by
  `bin/WorldGeneral.dll` next to `getSunLightFactor()`. The float argument suggests a threshold on
  a light factor rather than a sun angle or a time window. This is an inference from function names
  only; calibration decides.

Reading the machine code of these DLLs would mean reverse engineering ED's binaries, which their
licence does not allow, so the thresholds stay a calibration matter (section 6). The speech Lua
does give a better ground truth for that calibration, though (section 6, step 2).

What the rule means for the pilot, per case:

| Ordered case | What the pilot hears and flies | Where the groove starts |
|---|---|---|
| I | "Case I", overhead at 800 ft, break, downwind, port turn to final | at the roll-out on final, about 15–18 s from touchdown |
| II | Marshal descent under control until the ship is in sight, then the same break and pattern as Case I | same as Case I |
| III | Marshal stack, controlled descent, long straight-in final on ICLS/ACLS; no break, no turn | at the ball call, at approach minimums, about 3/4 NM |

Case I and Case II end the same way. For grading there are only two families: **visual pattern
(I and II)** and **instrument straight-in (III)**.

## 3. How the CRT script detects the case, and what we keep

`DCS-CRT-Carrier-Recovery-Tool/CRT.lua` (v2.1). Everything from `carrier_on()` downward
(MOOSE AIRBOSS setup, recovery windows, TACAN/ICLS/Link4, coastline detour) is AIRBOSS management and
is out of scope. The detection part is `IsNight()`, `CloudInfo()`, `fog_visibility()` and
`weather_case_factor()`.

| CRT part | How it works | Keep? |
|---|---|---|
| `IsNight()` | MOOSE sunset/sunrise at map origin `(0,0)`; night = from 30 min **before** sunset to 30 min **after** sunrise | Idea yes, implementation no: map origin instead of the carrier, and a ±30 min window ED never stated |
| `CloudInfo()` | Hand-written density per preset (Scattered = 4, Broken = 7, Overcast = 9, Rainy = 9 or 5); for manual clouds, `clouds.density` and `clouds.iprecptns` | Idea yes, table no (see below) |
| `fog_visibility()` | Legacy fog: `weather.fog.visibility`; new fog (`fog2`): `world.weather.getFogVisibilityDistance()`; adds 10 NM when fog is off | No: it trusts the mission's fog switches, which live data contradicts (see below) |
| `weather_case_factor()` | Its own thresholds: 3,000 ft base for Case I, density > 4 for Case II, any rain means II or III | No: these are NATOPS-like thresholds, not the DCS rule in section 2 |
| Dynamic weather | Always Case III | No: we mark it `indeterminate` (D5) |

**The CRT preset table is wrong for most modern presets.** The DCS install on this machine
(`Config/Effects/clouds.lua`) has 88 presets: ED's `Preset1`–`Preset27`, `RainyPreset1`–`6`,
`NEWRAINPRESET4`, and the ATMOS-X presets `Preset35`–`Preset88`. CRT only knows the first group. Every
ATMOS-X preset falls through to `clouds.density`, which is 0 when a preset is used, and to
`clouds.iprecptns`, which is also 0. So under "Nimbostratus with heavy rain" (`Preset65`,
precipitation 0.8, coverage 0.685) or "Scattered Thunderstorms" (`Preset73`), CRT says density 0,
no rain, Case I.

`clouds.lua` gives us what we need directly for every preset: each layer's `altitudeMin`,
`altitudeMax` and `coverage` (0 to 1), plus `precipitationPower` (above 0 means rain or snow, -1 or 0
means none). We generate our table from that file instead of copying CRT's.

## 4. Where the data comes from

| Input | Source | Available today? |
|---|---|---|
| Carrier latitude/longitude | carrier transform, already read by `record_recovery.rs` | yes |
| Theatre | `WorldService.GetTheatre`, already called in `record_recovery.rs` | yes |
| Mission date and time | `timer.getAbsTime()` and `env.mission.date` (or `TimerService.GetAbsoluteTime`) | through the same `Eval` call as the weather (see below); LSO has no `TimerService` client |
| Clouds (preset, base, thickness, density, precipitation), fog (legacy and `fog2`), visibility, dust, dynamic-weather flag | `env.mission.weather` and `world.weather.getFogVisibilityDistance()` / `getFogThickness()` | no dedicated RPC; readable through `CustomService.Eval` |

The fork has no weather RPC. `AtmosphereService` only offers `GetWind` and
`GetWindWithTurbulence`, which return a wind vector. The weather fields exist only inside the
mission scripting environment. The options:

- **A. `CustomService.Eval` (chosen).** Eval is always enabled on our dedicated server
  (`evalEnabled = true`). LSO sends one fixed, read-only Lua snippet once per query. The snippet
  returns the raw fields of `env.mission.weather`, the two `world.weather` fog values,
  `timer.getAbsTime()` and `env.mission.date` as JSON. **No case logic in Lua**: the snippet only
  copies raw values, and all classification is done in Rust, as AGENTS.md requires. There is no fork
  change, no new tag and no repin. Constraints:
  - it must be `CustomService.Eval` (mission environment), not `HookService.Eval`, which runs in
    the GUI environment where `env.mission` and `world.weather` do not exist;
  - the response is untyped JSON: Rust parses it strictly, and any missing, non-finite or unexpected
    field gives `Indeterminate` for the input it feeds, never a guessed value;
  - on a server where Eval is disabled the call fails with `PERMISSION_DENIED`. This gives
    `Indeterminate` with a diagnostic, never an error, and grading stays exactly as today;
  - enabling Eval lets anyone with the API key run arbitrary Lua. LSO's use adds no risk beyond the
    server setting that already exists, but the gRPC port must stay on loopback or behind the API
    key.
- **B. New read-only RPC in the fork (deferred).** For example `AtmosphereService.GetMissionWeather`,
  returning the same raw fields with a typed protobuf contract. It is only needed if LSO is deployed
  on servers where Eval is off. It would need a fork change, a new tag and a repin (AGENTS.md
  procedure), and the fork may only be changed on your explicit request. The Rust side is kept
  behind one weather-source function so that switching from A to B changes nothing else.
- **C. Read the `.miz` file.** LSO does not reliably have access to it, and animated fog changes at
  run time. Rejected.

"Dark" is computed in Rust: the sun's elevation at the carrier from its latitude/longitude and the
mission date and time, with the standard NOAA solar position formulas. DCS mission time is local
theatre time, so we need each theatre's UTC offset (MOOSE keeps such a table; we check each value
against DCS before using it). ED did not say what "dark" means. We start with the hypothesis
**sun elevation below -6° (end of civil twilight)** and calibrate it (section 6).

## 5. Proposed design

### 5.1 Classifier: pure Rust, no I/O

New module `src/recovery_case.rs`:

```rust
pub struct RecoveryConditions {
    pub sun_elevation_deg: Option<f64>,
    pub dynamic_weather: bool,
    pub cloud_density_0_10: Option<f64>,  // manual clouds, or derived from the preset table
    pub ceiling_ft: Option<f64>,          // above sea level = above the deck's waterline
    pub precipitation: Option<bool>,
    pub fog_visibility_nm: Option<f64>,   // from world.weather.getFogVisibilityDistance(), see below
    pub visibility_nm: Option<f64>,       // mission visibility (env.mission.weather.visibility), NATOPS diagnostic only
    pub preset: Option<String>,
}

pub enum OrderedCase { I, II, III, Indeterminate }

pub struct CaseAssessment {
    pub case: OrderedCase,
    pub rule_version: &'static str,       // "dcs-ed-statement-v1"
    pub reasons: Vec<CaseReason>,         // e.g. Dark, LowCeilingOvercast, Precipitation, Fog
    pub natops_case: OrderedCase,         // diagnostic only (D6), never used for grading
    pub natops_rule_version: &'static str, // "natops-minima-v1"
    pub natops_reasons: Vec<CaseReason>,
    pub inputs: RecoveryConditions,       // snapshot for the JSON
}
```

**Fog comes from the runtime functions, not the mission's fog switches.** First live reading, 27
September 2026, dedicated server (Caucasus, `Logs\Weather\weather-20260927-121250.json`): the
mission had `enable_fog = false` and no `fog2` table, yet `world.weather.getFogVisibilityDistance()`
returned 2,000 m and `getFogThickness()` 1,000 m, and fog was visible in game. The `.miz` holds the
same values (`enable_fog = false`, `fog = { visibility = 2000, thickness = 1000 }`, no `fog2`). The
mission uses our DCS-Dynamic-Weather tool, but it sets nothing at run time: `SetWeather.lua` only
builds restart menus, and its `.miz` editor leaves the old fog values in place when it switches fog
off. So it is DCS itself that applies the legacy fog block regardless of `enable_fog` when there is
no `fog2` table. The mission flags therefore cannot say whether fog is present; the runtime values
are the fog input. LSO does not depend on DCS-Dynamic-Weather: it reads only what DCS reports. Still open: what
the runtime functions return in a mission with no fog at all (0, or a stored value?). A no-fog
sample must be recorded before the classifier treats a runtime value as fog; until then a runtime
value that cannot be told apart from "no fog" gives `Indeterminate` for the fog clause. Expected
case for that first mission (clouds 5/10 at 3,000 m, fog 2,000 m): ED Case III (fog under 5 NM),
NATOPS Case III; the Marshal call has not been checked yet.

Rules for missing data, following the project's truth rules (never invent a value):

- dynamic weather, an unknown preset, or a failed weather RPC gives `Indeterminate`;
- a single sufficient Case III reason (for example, dark) is enough even if other inputs are
  missing;
- `Indeterminate` never changes the grading: the pass is graded exactly as today.

**NATOPS-minima diagnostic (D6).** Beside ED's case, the same inputs are classified with the
doctrinal minima of CV NATOPS, NAVAIR 00-80T-105 §4.2 "Control criteria" (checked against
`docs/NATOPS/CV-NATOPS-JUL09.pdf`):

> Case I: [...] daytime departures, recoveries, and the ceiling and visibility in the carrier control
> zone are no lower than 3,000 feet and 5 nm respectively.
> Case II: [...] daytime departure or recovery, and the ceiling and visibility in the carrier control
> zone are no lower than 1,000 feet and 5 nm respectively.
> Case III: [...] the ceiling or visibility in the carrier control zone are lower than 1,000 feet and
> 5 nm respectively; or a nighttime departure or recovery (one-half hour after sunset and one-half
> hour before sunrise).

§6.4 repeats it: Case III "whenever existing weather at the ship is below Case II minimums and
during all flight operations conducted between one-half hour after sunset and one-half hour before
sunrise".

```text
if natops_night or ceiling < 1000 ft or visibility < 5 NM   -> Case III
else if ceiling >= 3000 ft                                  -> Case I
else                                                        -> Case II
```

- **Night** is the NATOPS time window: from 30 minutes after sunset to 30 minutes before sunrise, at
  the carrier's position (`OFFICIAL`). It is computed from the same sun-position code as ED's
  "dark" but is a separate value, because ED's threshold is still unknown (section 6). The two
  cases can therefore disagree at dusk as well as on weather.
- **Visibility** is the lower of the mission visibility and the fog visibility.
- **Ceiling** is not defined in 00-80T-105. We use the usual aviation meaning, the base of the
  lowest layer covering at least 5/10 (broken or overcast; a scattered layer is not a ceiling),
  marked `PROJECT-DERIVED`. No layer at 5/10 or more means no ceiling, which passes the ceiling
  test.
- The "anticipated instrument conditions" wording of the manual cannot be measured and is left
  out; only the numbers are applied.

`natops_case` is recorded in the JSON and SQLite so the board can show how often DCS and doctrine
disagree. It never chooses a detector, never changes a grade and never raises the mismatch
diagnostic. It follows the same missing-data rules and can be `Indeterminate` on its own.

The preset table is a checked-in file (for example `data/dcs_cloud_presets.json`) generated from
`clouds.lua` by a small `lso.exe` subcommand, with the DCS version it came from. It stores, per
preset, the layers (altitudes, coverage) and the precipitation power. How ED turns a preset into
"density out of 10" and "ceiling" is not known. The first hypothesis is **density = 10 × coverage of
the lowest layer with coverage above 0, ceiling = that layer's base shifted by the mission's
`clouds.base` offset**. Calibration decides (section 6).

### 5.2 When LSO asks

Once per attempt, when the detector opens a `Track`, next to the existing wind probes: one `Eval`
call (weather and time together), then the pure classifier. Fog can change during a mission
(animated fog) and dusk can pass during a long pattern, so we also recompute at groove entry and
keep both in the JSON. Nothing is on the 10–20 Hz position path. `PERMISSION_DENIED` (Eval
disabled), `UNIMPLEMENTED` (a future server without the call), a timeout or malformed JSON all give
`Indeterminate` with a diagnostic, never an error.

Marshal gives the case at check-in, well before LSO opens a `Track` (at most 3.5 NM and 1,100 ft).
The JSON therefore records LSO's reading of the conditions at those two moments, not the call
Marshal actually made; the two can only differ if the weather changed in between.

On a Case III glideslope the aircraft is at about 1,300 ft at 3.5 NM, so it only enters the
detector's 1,100 ft envelope at about 3 NM. That is still well before 3/4 NM, where Case III grading
starts, so the envelope does not need to change for this work.

### 5.3 Flown approach from geometry

We already have most of it: `CaseIGrooveDetector` knows whether it observed the port pattern and
armed the last turn. We add a classification `flown_approach`:

- `overhead_pattern`: a port final turn below 600 ft was observed (today's arming condition);
- `straight_in`: wings level (bank at most 10°) and inbound on the extended centerline for a long
  final, starting beyond 2 NM, with no port turn;
- `unknown`: neither.

If `flown_approach` does not match the ordered case (break in Case III weather, straight-in in
Case I weather), LSO adds a diagnostic `approach_does_not_match_ordered_case`. It is not a grade
penalty: NATOPS does not grade the arrival.

### 5.4 Outputs

Additive only, as usual:

- JSON: `recovery_case { ordered, rule_version, reasons[], natops { case, rule_version,
  reasons[] }, inputs{...}, at_attempt_start, at_groove_entry, flown_approach, mismatch }`;
- SQLite: migration 9, columns `ordered_case`, `natops_case` and `flown_approach`;
- Discord, PNG and board: "Case I/II/III" beside the grade, and "night" when the sun is below the
  threshold.

## 6. Calibrating against DCS

The ED statement gives the structure, not the exact meaning of "dark", "density" for a preset, or
"ceiling". We measure it before trusting it. The full test list, with mission settings and the
expected prediction for each, is in `docs/CASE_RECOVERY_CALIBRATION_2026-09-27.md`.

1. **Test missions.** A Supercarrier on each theatre we use, one mission per condition, varying one
   factor at a time:
   - time around sunset and sunrise, in 5-minute steps;
   - manual clouds at density 5, 6, 8 and 9, with the base at 900, exactly 1,000 and 1,100 ft, with
     and without precipitation (a ceiling of exactly 1,000 ft fails both `< 1000` and `> 1000`, so
     with density 6–8 the rule as written gives Case II);
   - legacy fog and `fog2` at 4 and 6 NM visibility;
   - a sample of ED and ATMOS-X presets, each at its minimum and maximum base.

   The two `.miz` files shipped with CRT are a starting point.
2. **Ground truth.** A pilot checks in with Marshal. Before the check-in, LSO logs what it
   predicts. The case called is recorded, plus the DCS build. Two ways to capture the call:
   - by ear, as a fallback;
   - preferred: on the calibration PC only (single player, never the dedicated server or a client
     that joins it, because it changes a file covered by the integrity check), add one temporary
     `log.write` line to the Marshal check-in replies in `Scripts/Speech/common.lua`. It logs
     `message.parameters.case`, `clouds_density`, `clouds_ceiling` and `visibility` to `dcs.log`.
     This gives the exact case ED's code chose, and also ED's own density and ceiling for every
     preset, which answers the two hypotheses of section 5.1 directly (density out of 10 for a
     preset, and which layer is the ceiling) without guessing. Back up the file first; a DCS
     update or repair restores it anyway.
3. **Acceptance.** Every tested condition matches, including the boundaries. A mismatch changes the
   hypothesis (dark threshold, preset density, ceiling definition) and the test is run again. The
   resulting table becomes deterministic fixtures in `src/tests.rs`.
4. **Ask ED.** One precise question to the developer who wrote `conditions.txt`: what "dark" is (sun
   elevation, or sunset plus or minus some minutes), how density out of 10 is derived for a preset,
   and whether the ceiling is the preset's lowest layer. One answer can save many test missions.

## 7. Grading per case

This is the step after detection. LSO NATOPS uses the same grade symbols for every case; what
changes is where the graded groove starts and which gates count.

| Rule | Case I and II (visual pattern) | Case III (straight-in) |
|---|---|---|
| Groove entry | today's detector: port turn roll-out, wings level for 0.75 s | new `CaseIIIGrooveDetector`: established wings level (bank at most 10°), lineup within 2°, inside the glideslope box, sustained 0.75 s, no turn required; starts no later than 3/4 NM (the ball call at approach minimums) |
| 3/4 NM gate | counts only if captured after the roll-out (today's relaxation) | always required, as for the historical three-gate rule |
| Zones and correction windows of `project-derived-v7` | unchanged | unchanged from 3/4 NM inward; before 3/4 NM the pilot is under CCA control and is not graded (kept as context in the JSON) |
| `_OK_` groove time 15–18 s | applies (NAVAIR 00-80T-105 §6.2.4.3, Case I) | does not apply: that number is written for Case I. `_OK_` on the amplitude conditions only (D3), marked `PROJECT-DERIVED` |
| Pattern PNG | break and downwind branches as today | long final; no pattern branches |
| Night | recorded, no grade change | recorded, no grade change |

For night, NAVAIR 00-80T-105 §4.2 and §6.4 define night operations as from half an hour after
sunset to half an hour before sunrise, and NAVAIR 00-80T-104 (currency notes) does not count a
landing within half an hour of sunset or sunrise as a night landing. We use that window for the
"night" statistic on the board and for the NATOPS diagnostic, not for the ordered case, which
follows ED's "dark".

Which rules apply when ordered and flown disagree:

- **Ordered III, flown straight-in**: Case III rules.
- **Ordered I or II, flown overhead pattern**: Case I rules (today's behaviour).
- **Ordered III, flown overhead pattern**: the pilot flew a visual pattern at night or in weather.
  Case I detector (it is the only one that finds this groove), plus the mismatch diagnostic.
- **Ordered I or II, flown straight-in**: Case III detector plus the mismatch diagnostic (D4), so a
  clean straight-in keeps its points. This covers the 20 September straight-in that was deferred to
  this work.
- **Ordered `Indeterminate`** (including all dynamic weather, D5): choose the detector from the
  flown approach alone.

In short, the flown approach always chooses the detector, and the ordered case only decides whether
the mismatch diagnostic is raised.

AV-8B/Tarawa: the case is recorded, but V/STOL grading does not change in this work (NAVAIR
00-80T-111 Case I/II/III is a separate task).

## 8. Implementation phases

Before starting: create a feature branch from `main`.

| Phase | Content | Depends on | Tests |
|---|---|---|---|
| 1 | LSO weather source: `CustomService.Eval` client with the fixed read-only snippet (raw weather, fog, `timer.getAbsTime()`, mission date), strict JSON parsing, behind one weather-source function; dump the raw values on the dedicated server for the calibration missions | Eval enabled (already the case) | parser unit tests on recorded JSON (complete, missing fields, non-finite values, `fog2` and legacy fog), `PERMISSION_DENIED` and malformed JSON give `Indeterminate`, live smoke test |
| 2 | LSO: `recovery_case.rs` (pure classifier), sun position, theatre UTC table, preset table generated from `clouds.lua` | phase 1 fields | table-driven unit tests, boundaries (density 6 and 8, ceiling exactly 1,000 ft, 5 NM, dark threshold), ATMOS-X presets, dynamic weather gives `Indeterminate`; NATOPS diagnostic boundaries (ceiling 1,000 and 3,000 ft, visibility 5 NM, scattered layer not a ceiling, sunset + 30 min and sunrise − 30 min) and a case where ED and NATOPS disagree (1,500 ft overcast without rain: ED I, NATOPS II) |
| 3 | Queries in `record_recovery.rs`, JSON `recovery_case`, SQLite migration 9, display on Discord, PNG and board | phase 2 | migration test on the oldest DB fixture, `PERMISSION_DENIED`/`UNIMPLEMENTED` give `Indeterminate` |
| 4 | Calibration campaign (section 6), fixtures | phases 2–3 deployed | one fixture per tested condition |
| 5 | `flown_approach` and the mismatch diagnostic | phase 3 | the 20 September 09:25 straight-in classified `straight_in`; existing Case I corpus classified `overhead_pattern` |
| 6 | `CaseIIIGrooveDetector` and the Case III grading rules | phase 5 | synthetic CCA finals, `groove-ab` on the existing corpus unchanged for Case I, a live Case III session at night |

Phases 1 to 4 change no grade. Grading changes only in phase 6, after calibration. A typed fork
RPC (section 4, option B) is not a phase: it is added only if LSO has to run on a server where Eval
is off.

## 9. Decisions

All decided on 27 September 2026.

| # | Question | Decision |
|---|---|---|
| D1 | Weather source: `CustomService.Eval` or a new read-only fork RPC? | `Eval` in production, since it is always enabled on our server. The fork RPC is deferred until LSO runs on a server with Eval off |
| D2 | Follow ED's rule rather than NATOPS minima (3,000 ft / 1,000 ft) for the ordered case? | ED's rule: it is what Marshal tells the pilot |
| D3 | `_OK_` in Case III: amplitude only, or never? | Amplitude only, marked `PROJECT-DERIVED` |
| D4 | Straight-in flown in Case I or II weather: grade it with the Case III detector, or leave it ungraded as today? | Grade it with the Case III detector plus the mismatch diagnostic; a clean straight-in keeps its points |
| D5 | Dynamic weather: `Indeterminate` (graded as flown) or forced Case III like CRT? | `Indeterminate` |
| D6 | Also store a NATOPS-minima case beside ED's, as a diagnostic? | Yes: `natops_case` in JSON and SQLite, diagnostic only (section 5.1); minima and night window checked against NAVAIR 00-80T-105 §4.2 and §6.4 |
