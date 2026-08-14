# LSO Grading Reference

## Data Collection

### Detection

Every **carrier × plane** pair is polled every **2 seconds**. A recovery attempt is triggered when **all** conditions are met:

| Condition | Value |
|---|---|
| Plane altitude | < 1 100 ft MSL |
| Distance to carrier | 200 m – 3.5 nm |
| Plane is behind the carrier | dot(carrier\_forward, ray\_to\_plane) > 0 |

> The nose-pointing check was removed — during the break the aircraft is abeam the carrier with its nose perpendicular to the BRC. The wider trigger (3.5 nm / 1 100 ft) captures the full Case I circuit from the break. False positives (planes that never enter the groove) are discarded at termination because `Grading::Unknown` recordings are dropped.

### Recording

Once detected, the app switches to **10 Hz** (100 ms) polling via DCS-gRPC `UnitService.GetTransform()`.

Each frame computes a **carrier-relative datum** in the angled-deck frame:

| Field | Description |
|---|---|
| `x` | Forward distance along the angled-deck centerline from optimal touchdown (m) |
| `y` | Lateral offset from centerline (m, +right / −left) |
| `aoa` | Angle of Attack (degrees) |
| `alt` | Hook altitude above carrier deck (m) — corrected for the aircraft-specific hook arm position, rotated by the plane's attitude |

The **angled deck** axis (not the ship heading) is used for all geometry.  
Optimal touchdown is the midpoint between wire 2 and wire 3.

### Pattern Tracking

In addition to the groove datum (angled-deck frame), every frame also records a **`PatternDatum`** in the **carrier BRC frame** (ship heading, no deck angle offset). This covers the full circuit from the break through the entire landing.

Recording starts on the **first detection frame** — same trigger as the overall recording (3.5 nm / 1 100 ft / behind carrier). There is no separate start condition for the pattern.

| Field | Description |
|---|---|
| `astern_m` | Distance behind the carrier along BRC (m). Positive = plane is astern of the carrier |
| `port_m` | Lateral distance from BRC centerline (m). Positive = plane is on the port (left) side |
| `alt_ft` | Altitude MSL in feet |
| `aoa` | Angle of Attack (degrees) |

The pattern data is used to render a **separate overhead circuit PNG** (`<filename>-pattern.png`) showing the full Case I oval: break → abeam → ninety → final → touchdown. The track is coloured by AoA using the same scheme as the approach chart. The chart clips data to ±2.5 nm port/starboard and −4 nm to +1.5 nm ahead/astern.

### Gate Sampling

On the **first frame** the aircraft crosses each gate, a `GateDatum` is frozen:

| Gate | Distance from touchdown |
|---|---|
| **3/4 nm** | 1 389 m |
| **1/2 nm** | 926 m |
| **1/4 nm** | 463 m |

Each `GateDatum` stores:

```
gs_deviation_deg = atan2(hook_alt − ideal_gs_alt, x)  [degrees, + = high]
lineup_deg       = atan2(y, x)                         [degrees, + = right / LUL]

ideal_gs_alt = x × tan(glide_slope°)   (glide slope = 3.5° for CVN aircraft)
```

Foot values (`gs_deviation_ft`, `lineup_ft`) are also stored for the PNG chart label but are **not used** in grading.

### Landing Events (from DCS)

| DCS Event | Action |
|---|---|
| `LandingQualityMark` | Stores the DCS grading string; wire number extracted from it takes precedence over the estimated wire |
| `RunwayTouch` | Calls `Track::landed()` — cable estimation runs, 10-second post-landing window begins |
| `Crash / Dead / PlayerLeaveUnit` | Recording stops immediately |

### Termination

- Aircraft never went below 100 m MSL → recording discarded (not a genuine recovery attempt).
- Distance to landing position increases by > 150 m after the minimum reached → **bolter** declared.
- Aircraft entered the groove (inside 3/4 nm, below 300 ft) then climbed away without touching → **waveoff** declared.

---

## Pass Grade Scale (NAVAIR 00-80T-104)

| Label | Enum | Points | Meaning | Implemented |
|---|---|---|---|---|
| `_OK_` | `PassGrade::Unicorn` | **5.0** | Unicorn — zero deviations, groove time 15–18.99 s, wire 3 | ✅ |
| `OK` | `PassGrade::Ok` | **4.0** | Okay pass — all deviations within the OK margin | ✅ |
| `(OK)` | `PassGrade::OkParentheses` | **3.0** | Fair pass — slight deviations only | ✅ |
| `--` | `PassGrade::NoGrade` | **2.0** | No grade — significant deviations | ✅ |
| `C` | `PassGrade::Cut` | **0.0** | Cut pass — dangerously low at the ramp (GS < −2.5° at 1/4 nm) | ✅ |
| `B` | `PassGrade::Bolter` | **2.5** | Bolter — missed all wires | ✅ |
| `WO` | `PassGrade::WaveoffPilot` | **1.0** | Waveoff — broke off inside the groove | ✅ |

> **`_OK_` vs `OK` — same deviation requirements, different procedure:**
> Both grades require all gate deviations within the OK zone (GS `< +0.5°` high / `< −0.5°` low, LU `< ±1.0°`).
> `_OK_` additionally requires **wire 3** and **groove time 15.0–18.99 s**. If either condition fails the grade falls back to `OK` (4.0 pts).
> In practice `_OK_` is extremely rare — it demands a zero-deviation approach **and** the correct wire **and** correct timing simultaneously.

---

## Grading Thresholds

All thresholds are in **degrees** (distance-invariant, matching MOOSE Airboss CVN defaults).

### Glideslope (GS)

> Legend: 🟢 same as our code · 🔴 tighter (penalises earlier) · 🔵 more lenient (penalises later)

| Zone | Our code | MOOSE CVN | NAVAIR 00-80T-104 | Grade impact |
|---|---|---|---|---|
| Slight High `(H)` | `> +0.5°` | 🔵 `> +0.8°` | 🟢 `> +0.5°` | Caps at `(OK)` |
| Significant High `H` | `> +1.0°` | 🔵 `> +1.5°` | 🟢 `> +1.0°` | Caps at `--` |
| Slight Low `(L)` | `> −0.5°` | 🔵 `> −0.8°` | 🟢 `> −0.5°` | Caps at `(OK)` |
| Significant Low `L` | `> −1.0°` | 🔵 `> −1.5°` | 🟢 `> −1.0°` | Caps at `--` |
| **Cut threshold** | `< −2.5°` at ¼ nm | 🟢 `< −2.5°` | 🟢 `< −2.5°` | Grade = `C` |

> Both high and low sides match NAVAIR. MOOSE uses wider thresholds (🔵 more lenient) on all GS tiers — our code penalises at smaller deviations.

### Lineup (LU)

> Legend: 🟢 same as our code · 🔴 tighter (penalises earlier) · 🔵 more lenient (penalises later)

| Zone | Our code | MOOSE CVN | NAVAIR 00-80T-104 | Grade impact |
|---|---|---|---|---|
| Slight `(LUL)`/`(LUR)` | `> ±1.0°` | 🟢 `> ±1.0°` | 🟢 `> ±1.0°` | Caps at `(OK)` |
| Medium `LUL`/`LUR` | `> ±2.0°` | 🟢 `> ±2.0°` | 🟢 `> ±2.0°` | Caps at `--` |

> MOOSE has an additional Large tier at `> ±3.0°`, but our code caps at `--` from `> ±2.0°` (same as NAVAIR). The `LU_SIGNIFICANT = 3.0` constant is commented out in the code.

### Decision Logic

The grade is determined by the **worst single deviation** across all three gates:

```
if GS < −2.5° at 1/4 nm                          → C   (Cut)
else if worst_gs_high ≥ 1.0° OR worst_gs_low ≥ 1.0° OR worst_lu ≥ 2.0°  → --  (No Grade)
else if worst_gs_high ≥ 0.5° OR worst_gs_low ≥ 0.5° OR worst_lu ≥ 1.0°  → (OK) (Fair Pass)
else                                               → OK  (Okay Pass)
```

Special outcomes override the gate logic:

```
Grading::WaveoffPilot  → WO  (1.0 pts)
Grading::Bolter        → B   (2.5 pts)
Grading::Unknown       → --  (2.0 pts)
```

---

## Supported Aircraft & Glide Slopes

| Aircraft | DCS Type | Glide Slope | AoA On-Speed |
|---|---|---|---|
| F/A-18C Hornet | `FA-18C_hornet` | 3.5° | 7.4–8.8° |
| F-14A Tomcat | `F-14A-135-GR` / `F-14A-135-GR-Early` / `F-14A-95-GR` | 3.5° | 10.2–11.1° |
| F-14B Tomcat | `F-14B` / `F-14A/B` | 3.5° | 10.2–11.1° |
| F-14B(U) Tomcat | `F-14B(U)` / `F-14BU` | 3.5° | 10.2–11.1° |
| T-45C Goshawk | `T-45` | 3.5° | 6.5–7.5° |

## Supported Carriers

| DCS Type | Class | Deck Angle | Deck Alt |
|---|---|---|---|
| `CVN_71` / `72` / `73` / `75` / `Stennis` | Nimitz | 9.14° | 20.15 m |
| `Forrestal` | Forrestal | 9.42° | 18.46 m |
