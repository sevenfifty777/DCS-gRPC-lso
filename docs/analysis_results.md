# Code & Docs Review — Grading Timing & Recording Ranges

## 1. When Does Grading Take Data Into Account?

Grading is **not continuous** — it samples data at **three specific moments** during the approach, then evaluates the **worst single deviation** across those three samples.

### The Three Grading Gates (data sampling moments)

Each gate fires **exactly once**, on the **first 100ms frame** where the aircraft crosses that distance threshold while below 500 ft AGL (hook altitude above deck):

| Gate | Trigger distance | When it typically fires |
|---|---|---|
| **¾ nm** | x ≤ 1,389 m from touchdown | ~22 seconds before landing (at ~120 kts) — "start of the groove" |
| **½ nm** | x ≤ 926 m from touchdown | ~15 seconds before landing — "in the middle" |
| **¼ nm** | x ≤ 463 m from touchdown | ~7 seconds before landing — "at the ramp / in close" |

> [!IMPORTANT]
> Grading uses **only** the deviation snapshot frozen at the exact frame each gate is crossed. All the 10 Hz data between gates is recorded for the chart but **does not influence the grade**.

### Gate Sampling Guards

Two guards prevent false gate captures:

1. **x > 0 guard** ([track.rs:L316](file:///c:/Users/thierry/Documents/GitHub/sevenfifty777/DCS-gRPC-lso/src/track.rs#L316)): Gate sampling only runs when the aircraft is on the approach side of the touchdown point. This prevents bogus ~177° atan2 readings when the aircraft is ahead of the carrier (e.g., in the break).

2. **500 ft altitude guard** ([track.rs:L328](file:///c:/Users/thierry/Documents/GitHub/sevenfifty777/DCS-gRPC-lso/src/track.rs#L328)): `m_to_ft(alt) <= 500.0`. This rejects the overhead-pattern crossing of x=0 at 600–1000 ft.

### What Deviation Values Are Sampled

At each gate, four values are frozen ([track.rs:L318-L324](file:///c:/Users/thierry/Documents/GitHub/sevenfifty777/DCS-gRPC-lso/src/track.rs#L318-L324)):

```
gs_deviation_deg = atan2(hook_alt − ideal_gs_alt, x)   [degrees, + = high, − = low]
lineup_deg       = atan2(lateral_offset, x)             [degrees, + = right of CL]
gs_deviation_ft  = (hook_alt − ideal_gs_alt) in feet    [for chart display only]
lineup_ft        = lateral_offset in feet                [for chart display only]
```

> [!NOTE]
> **Only the degree values** are used for grading. The foot values are kept solely for the PNG chart labels and Discord embed display.

### Grade Decision Flow

After the track finishes, the grade is computed in [grading.rs:L96-L120](file:///c:/Users/thierry/Documents/GitHub/sevenfifty777/DCS-gRPC-lso/src/grading.rs#L96-L120):

```
1. Special outcomes override everything:
   - WaveoffPilot → WO (1.0 pts)
   - Bolter → B (2.5 pts)
   - Unknown → -- (2.0 pts)

2. For Recovered landings, the worst deviation across all 3 gates determines the base grade:
   - GS < −2.5° at ¼ nm only             → C  (Cut, 0.0 pts)
   - worst_gs_high ≥ 1.0° OR
     worst_gs_low ≥ 1.0° OR
     worst_lu ≥ 2.0°                      → -- (No Grade, 2.0 pts)
   - worst_gs_high ≥ 0.5° OR
     worst_gs_low ≥ 0.5° OR
     worst_lu ≥ 1.0°                      → (OK) (Fair, 3.0 pts)
   - everything within margins             → OK (4.0 pts)

3. Unicorn upgrade (only if base == OK):
   - Wire 3 AND groove time 15.0–18.99 s  → _OK_ (5.0 pts)
```

### Groove Entry & Groove Time

**Groove entry** is marked when the aircraft is inside ¾ nm AND below **300 ft AGL** ([track.rs:L354-L360](file:///c:/Users/thierry/Documents/GitHub/sevenfifty777/DCS-gRPC-lso/src/track.rs#L354-L360)). The DCS simulation time is recorded at that moment.

**Groove time** = `landing_time − groove_entry_time`. This is used only for the Unicorn (_OK_) check (must be 15.0–18.99 s).

---

## 2. Recording Ranges — Distance & Altitude

### Phase 1: Detection (2-second polling)

The detection task polls every 2 seconds to see if a recovery attempt should start:

| Parameter | Range | Source |
|---|---|---|
| **Distance to carrier** | 200 m – 3.5 nm (6,482 m) | [detect_recovery_attempt.rs:L65](file:///c:/Users/thierry/Documents/GitHub/sevenfifty777/DCS-gRPC-lso/src/tasks/detect_recovery_attempt.rs#L65) and [L74](file:///c:/Users/thierry/Documents/GitHub/sevenfifty777/DCS-gRPC-lso/src/tasks/detect_recovery_attempt.rs#L74) |
| **Altitude** | ≤ 1,100 ft MSL | [detect_recovery_attempt.rs:L56](file:///c:/Users/thierry/Documents/GitHub/sevenfifty777/DCS-gRPC-lso/src/tasks/detect_recovery_attempt.rs#L56) |
| **Position** | Any quadrant (no rear-hemisphere check) | [detect_recovery_attempt.rs:L79-L81](file:///c:/Users/thierry/Documents/GitHub/sevenfifty777/DCS-gRPC-lso/src/tasks/detect_recovery_attempt.rs#L79-L81) |

### Phase 2: Active Recording (100ms polling)

Once detection triggers, the 10 Hz recording loop runs. It records **two parallel data streams**:

#### A. Approach Datums (angled-deck frame)

Recorded for every frame where the aircraft is on the approach side (x > 0) or near touchdown. Used for the side-view and top-down charts.

| Field | Unit | Range/Notes |
|---|---|---|
| `x` | meters | ~1,400 m (¾ nm) down to 0 (touchdown) and below |
| `y` | meters | lateral offset from angled-deck CL |
| `alt` | meters | hook height above deck (clamped to ≥ 0) |
| `aoa` | degrees | raw angle of attack |

#### B. Pattern Datums (BRC frame)

Recorded **every frame** regardless of x > 0 guard. Used for the overhead circuit chart.

| Field | Unit | Range/Notes |
|---|---|---|
| `astern_m` | meters | distance behind carrier along BRC |
| `port_m` | meters | lateral distance from BRC centerline (+ = port) |
| `alt_ft` | feet MSL | raw altitude, not deck-relative |
| `aoa` | degrees | raw angle of attack |

**Pattern chart clipping** (in [draw.rs](file:///c:/Users/thierry/Documents/GitHub/sevenfifty777/DCS-gRPC-lso/src/draw.rs)): ±2.5 nm port/starboard, −4 nm to +1.5 nm ahead/astern.

### Recording Termination Conditions

The recording stops when ANY of these occurs:

| Condition | Distance/Altitude | Effect | Source |
|---|---|---|---|
| **Exited pattern zone** | > 3.5 nm from carrier OR > 1,100 ft MSL | Recording stops | [track.rs:L248](file:///c:/Users/thierry/Documents/GitHub/sevenfifty777/DCS-gRPC-lso/src/track.rs#L248) |
| **Moving away > 150 m** after entering groove | distance − min_distance > 150 m | **Waveoff** declared | [track.rs:L270-L274](file:///c:/Users/thierry/Documents/GitHub/sevenfifty777/DCS-gRPC-lso/src/track.rs#L270-L274) |
| **Moving away > 150 m** after touchdown | distance − min_distance > 150 m | **Bolter** declared | [track.rs:L259-L263](file:///c:/Users/thierry/Documents/GitHub/sevenfifty777/DCS-gRPC-lso/src/track.rs#L259-L263) |
| **DCS RunwayTouch event** | — | Cable estimated, 10s post-landing window starts | [record_recovery.rs:L280-L347](file:///c:/Users/thierry/Documents/GitHub/sevenfifty777/DCS-gRPC-lso/src/tasks/record_recovery.rs#L280-L347) |
| **Crash / Dead / PlayerLeave** | — | Recording stops immediately | [record_recovery.rs:L349-L379](file:///c:/Users/thierry/Documents/GitHub/sevenfifty777/DCS-gRPC-lso/src/tasks/record_recovery.rs#L349-L379) |
| **Post-discard: never below 100m MSL** | > 100 m MSL throughout | Recording **discarded** | [record_recovery.rs:L388](file:///c:/Users/thierry/Documents/GitHub/sevenfifty777/DCS-gRPC-lso/src/tasks/record_recovery.rs#L388) |

### Gate Distance & Altitude Summary

| Gate | Distance from touchdown | Typical on-glidepath altitude |
|---|---|---|
| **¾ nm** | 1,389 m | ~278 ft AGL |
| **½ nm** | 926 m | ~185 ft AGL |
| **¼ nm** | 463 m | ~93 ft AGL |

> [!NOTE]
> The "typical altitude" above assumes a 3.5° glide slope. Actual altitude at the gate depends on the aircraft's deviation from the ideal glidepath.

### Altitude Guards at Different Phases

| Phase | Altitude Guard | Purpose |
|---|---|---|
| Detection trigger | ≤ 1,100 ft MSL | Captures overhead break (~800 ft) |
| Gate sampling | ≤ 500 ft AGL (deck-relative) | Rejects overhead crossing at altitude |
| Groove entry | ≤ 300 ft AGL (deck-relative) | Marks start of the "groove" for waveoff detection and groove time |
| Discard filter | never below 100 m (328 ft) MSL | Drops non-genuine attempts |

---

## 3. Documentation Accuracy Issues Found

### GRADING_REFERENCE.md

| Line | Issue | Code Truth | Severity |
|---|---|---|---|
| L98 | States GS_SLIGHT_LOW threshold as `< −0.8°` | Code uses `GS_SLIGHT_LOW = 0.5` (symmetric, [grading.rs:L11](file:///c:/Users/thierry/Documents/GitHub/sevenfifty777/DCS-gRPC-lso/src/grading.rs#L11)) | 🔴 **Wrong** |
| L114 | GS Significant High shown as `> +1.5°` | Code uses `GS_SIGNIFICANT = 1.0` ([grading.rs:L12](file:///c:/Users/thierry/Documents/GitHub/sevenfifty777/DCS-gRPC-lso/src/grading.rs#L12)) | 🔴 **Wrong** |
| L117 | GS Significant Low shown as `> −1.0°` with 🟢 "same as code" | Code uses `GS_SIGNIFICANT = 1.0` but doc says 1.0 matches — yet the MOOSE column says `> −0.9°` 🔴 and doc says "same as code" 🟢 for NAVAIR. This is internally consistent for the code column. | ✅ OK |
| L115 | GS Significant High — MOOSE column says 🟢 `> +1.5°` | But the code uses 1.0, and the doc's "Our code" column also says `> +1.0°`. The MOOSE 🟢 should actually be 🔴 (tighter from MOOSE's perspective, more lenient from our code). | 🟡 **Misleading legend** |
| L128 | LU OK zone listed as `±0.5°` | There is no OK zone in the code — the code only checks `>= LU_SLIGHT (1.0°)` and `>= LU_MEDIUM (2.0°)`. Values 0–1.0° are all OK. | 🟡 **Misleading** |
| L131-L133 | Shows separate "Large" LU tier at `> ±3.0°` | Code has no separate large tier — `LU_MEDIUM = 2.0` is the `NoGrade` threshold. The `LU_SIGNIFICANT = 3.0` constant is commented out in [grading.rs:L20](file:///c:/Users/thierry/Documents/GitHub/sevenfifty777/DCS-gRPC-lso/src/grading.rs#L20). | 🟡 **Stale** |
| L141 | Decision logic shows `worst_gs_high ≥ 1.5°` | Code uses `GS_SIGNIFICANT = 1.0` | 🔴 **Wrong** |
| L142 | Decision logic shows `worst_gs_low ≥ 0.8°` | Code uses `GS_SLIGHT_LOW = 0.5` | 🔴 **Wrong** |
| L160-163 | F/A-18C AoA listed as `7.4–8.1°`, Super Hornet listed | Code says OnSpeed = `7.4 < aoa < 8.8` ([data.rs:L149](file:///c:/Users/thierry/Documents/GitHub/sevenfifty777/DCS-gRPC-lso/src/data.rs#L149)). No Super Hornet in code. | 🔴 **Wrong AoA range & phantom aircraft** |
| L162 | F-14 DCS types listed as `F-14A / F-14B / F-14A/B / F-14B(U)` | Code uses `F-14A-135-GR`, `F-14A-135-GR-Early`, `F-14A-95-GR`, `F-14B`, `F-14A/B`, `F-14B(U)`, `F-14BU` | 🟡 **Incomplete** |

### GRADING_ANALYSIS.md

| Issue | Details | Severity |
|---|---|---|
| Section 3.2 describes old foot-based thresholds | Code now uses degree-based thresholds (this doc was written pre-migration) | 🔴 **Entirely stale** |
| Section 5 "Code Changes Required" | All these changes have already been implemented | 🔴 **Stale — reads as TODO but is done** |
| States `PassGrade` has no `points()` method | Code has `points()` returning `f64` | 🔴 **Stale** |
| States no Unicorn grade | Code has `PassGrade::Unicorn` with 5.0 pts | 🔴 **Stale** |

### ADMIN_GUIDE.md

| Line | Issue | Code Truth | Severity |
|---|---|---|---|
| L278 | `pass_grade` values listed as `"Ok"`, `"OkParentheses"`, `"Fair"`, `"NoGrade"` | Code uses NAVAIR labels: `"_OK_"`, `"OK"`, `"(OK)"`, `"--"`, `"C"`, `"B"`, `"WO"` ([grading.rs:L56-L65](file:///c:/Users/thierry/Documents/GitHub/sevenfifty777/DCS-gRPC-lso/src/grading.rs#L56-L65)). The JSON stores the `PassGrade` enum variant name, but the DB stores `label()`. | 🟡 **Partially stale** — the JSON indeed serializes enum names like `"Ok"`, `"Unicorn"`, etc. but the list is incomplete (missing `"Unicorn"`) and `"Fair"` doesn't exist |
| L295 | Web board color legend says "yellow = Fair, orange = NG" | Code CSS uses `g-NG` for `"--"` grade. No `"Fair"` exists. | 🟡 **Stale labels** |
| L346 | Discord embed described as "NAVAIR pass grade (OK / Fair / NG etc.)" | No "Fair" or "NG" labels exist anymore | 🟡 **Stale** |
| L379 | Session greenie board example shows `NG` | Should be `--` | 🟡 **Stale** |
| L415 | T-45 AoA listed as `7.0° – 7.5°` | Code says OnSpeed = `6.5 < aoa < 7.5` ([data.rs:L222-L226](file:///c:/Users/thierry/Documents/GitHub/sevenfifty777/DCS-gRPC-lso/src/data.rs#L222-L226)) — lower bound is 6.5, not 7.0 | 🔴 **Wrong** |
| L413 | F/A-18C AoA listed as `7.4° – 8.8°` | Correct ✅ (matches code [data.rs:L148-L150](file:///c:/Users/thierry/Documents/GitHub/sevenfifty777/DCS-gRPC-lso/src/data.rs#L148-L150)) | ✅ OK |
| L429-L435 | Grade thresholds table still shows foot-based values | Code uses degree-based thresholds | 🔴 **Entirely stale** |
| L472 | DB schema shows `pass_grade` examples including `"Fair"` and `"NG"` | Labels are now `"_OK_"`, `"OK"`, `"(OK)"`, `"--"`, `"C"`, `"B"`, `"WO"` | 🔴 **Stale** |
| L478 | Schema shown is missing `pilot_ucid`, `aircraft_id`, `mission_datetime` columns | Code has all three ([db.rs:L69-L78](file:///c:/Users/thierry/Documents/GitHub/sevenfifty777/DCS-gRPC-lso/src/db.rs#L69-L78)) | 🔴 **Incomplete** |
| L498 | SQL example counts `'Fair'` and `'NG'` grades | Should count `'--'` instead | 🔴 **Stale** |

---

## 4. Summary

### Grading Data Timing
- Gate deviations are sampled at **3 discrete moments** (¾, ½, ¼ nm from touchdown)
- The **first frame** crossing each distance threshold captures a snapshot — that single snapshot determines the grade for that gate
- The **worst deviation** across all 3 gates drives the final grade
- Groove time (for Unicorn) is measured from the first frame inside ¾ nm AND below 300 ft, to touchdown

### Recording Range Summary

| Parameter | Start Recording | Stop Recording |
|---|---|---|
| **Distance** | 200 m – 3.5 nm | > 3.5 nm or +150 m divergence |
| **Altitude** | ≤ 1,100 ft MSL | > 1,100 ft MSL |
| **Gate sampling** | ¾ nm (1,389 m) to ¼ nm (463 m) | only below 500 ft AGL |
| **Groove detection** | ≤ ¾ nm AND ≤ 300 ft AGL | — |

### Doc Accuracy

| Document | Status |
|---|---|
| [GRADING_REFERENCE.md](file:///c:/Users/thierry/Documents/GitHub/sevenfifty777/DCS-gRPC-lso/docs/GRADING_REFERENCE.md) | 🟡 Has 5 inaccuracies — threshold values and aircraft table need updating |
| [GRADING_ANALYSIS.md](file:///c:/Users/thierry/Documents/GitHub/sevenfifty777/DCS-gRPC-lso/docs/GRADING_ANALYSIS.md) | 🔴 Entirely stale — describes pre-migration state as if it's current, §5 reads as TODO but all changes are done |
| [ADMIN_GUIDE.md](file:///c:/Users/thierry/Documents/GitHub/sevenfifty777/DCS-gRPC-lso/docs/ADMIN_GUIDE.md) | 🟡 Section 12 grade thresholds are stale (foot-based), schema and labels outdated |
