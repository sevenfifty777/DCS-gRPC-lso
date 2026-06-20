# LSO App — Grading Analysis & NAVAIR Alignment

## 1. Purpose

This document compares the current LSO app grading system against:
- **NAVAIR 00-80T-104** — the authoritative US Navy carrier landing grade standard.
- **MOOSE Airboss.lua** — the widely-used DCS Lua simulation of the same standard.

It identifies every discrepancy, explains the root cause, and specifies the exact code changes needed to align the app with the real-world standard.

---

## 2. How Each System Collects Data

### 2.1 LSO App (`track.rs`)

Data is collected at **10 Hz** (every 100 ms) via DCS-gRPC `UnitService.GetTransform()`.

At each frame, the app computes a **carrier-relative datum**:

| Field | Description |
|---|---|
| `x` | Forward distance along the **angled-deck** centerline, from optimal touchdown (metres) |
| `y` | Lateral offset from the angled-deck centerline (metres, +right / −left) |
| `aoa` | Angle of Attack in degrees |
| `alt` | Hook altitude above the carrier deck, corrected for the aircraft-specific hook arm position rotated by the plane's attitude (metres) |

**Gate sampling**: on the first frame at which the aircraft crosses each gate distance, a `GateDatum` is frozen:

```
gs_deviation_ft = m_to_ft(hook_alt − ideal_gs_alt)
                  where ideal_gs_alt = x × tan(glide_slope°)

lineup_ft       = m_to_ft(y)
```

Three fixed gates (measured from optimal touchdown along the angled deck):

| Gate label | Distance |
|---|---|
| 3/4 nm | 1 389 m |
| 1/2 nm | 926 m |
| 1/4 nm | 463 m |

> **Limitation:** the app stores deviations in **absolute feet**. The same foot value represents very different angular deviations at each gate. At 1/4 nm, 40 ft ≈ 1.5° — a `HIGH` call in Airboss — but the app currently grades it as only a "slight" deviation (`(OK)`).

### 2.2 MOOSE Airboss (`Airboss.lua`)

Data is sampled at the **seven standard LSO groove positions** (XX, IM, IC, AR, IW) via a 1 Hz polling loop calling DCS `Unit:GetAoA()`, `_Glideslope()`, `_Lineup()`.

Deviations are stored and compared in **degrees**, making the scale distance-agnostic. The GSE and LUE values are:

```
GSE = actual_glideslope_deg − nominal_glideslope_deg
LUE = lateral_angle_from_runway_centerline_deg
```

This is a fundamentally more accurate representation because the LSO observes the aircraft visually at a fixed angular scale, not a fixed spatial scale.

---

## 3. Current LSO App Grading vs. NAVAIR 00-80T-104

### 3.1 NAVAIR Grade Scale

| Grade | Label | Points | Condition |
|---|---|---|---|
| Unicorn | `_OK_` | 5.0 | Perfect pass — no deviations, exact groove time, wire 3 |
| Okay Pass | `OK` | 4.0 | No deviations (minor corrections only) |
| Fair Pass | `(OK)` | 3.0 | Only slight deviations |
| No Grade | `--` | 2.0 | Significant (but not extreme) deviations |
| Cut Pass | `C` | 0.0 | Dangerously low at the ramp, OR landed after being waved off |
| Waveoff | `WO` | 1.0 | LSO waveoff (or own waveoff with clean approach = OWO 2.0 pts) |
| Bolter | `B` | 2.5 | Missed all wires |

> There is **no 1-point arrested-landing grade** in NAVAIR. The spectrum for arrested passes is 0 / 2 / 3 / 4 / 5. A "1 pt" grade only applies to waveoffs.

### 3.2 LSO App Grade Scale (current)

| Enum | Label | Points* | Condition |
|---|---|---|---|
| `Ok` | `OK` | — | worst gate < 40 ft GS and 25 ft LU |
| `OkParentheses` | `(OK)` | — | worst gate < 100 ft GS and 60 ft LU |
| `Fair` | `Fair` | — | worst gate < 200 ft GS and 120 ft LU |
| `NoGrade` | `NG` | — | worst gate ≥ 200 ft GS or 120 ft LU |
| `Cut` | `Cut` | — | GS < −150 ft at 1/4 nm gate |
| `Bolter` | `B` | — | bolter outcome |
| `WaveoffPilot` | `WO` | — | waveoff outcome |

*\* `PassGrade` carries no numeric points in the current code — points are absent from the model entirely.*

### 3.3 Discrepancy Table

| # | Area | NAVAIR standard | Current LSO app | Severity |
|---|---|---|---|---|
| D1 | **Label: `Fair`** | No "Fair" label; 2-pt grade is `--` (no grade) | Uses non-standard label `Fair` | Medium — confuses users familiar with real LSO sheets |
| D2 | **Label: `NG`** | No `NG` label; label for extreme deviations is still `--` or `C` | Uses `NG` at 1 pt — grade that does not exist in NAVAIR | High — phantom grade |
| D3 | **Cut definition** | Cut = landed after being waved off (LSO call), or dangerously low | App triggers Cut on geometric criterion only (< −150 ft GS at 1/4 nm) | Medium — partial; the geometric trigger is a valid proxy but misses the "landed after WO" case |
| D4 | **Bolter points** | Bolter = 2.5 pts | Bolter carries no points in the model | High — breaks any point-based averaging |
| D5 | **Waveoff points** | Waveoff = 1.0 pt | Waveoff carries no points | High |
| D6 | **Unicorn grade** | `_OK_` 5.0 pts exists (Airboss: N=0, Tgroove 16.49–16.59 s, wire 3) | Not implemented | Low — rare, but visually impactful |
| D7 | **AoA penalty** | Fast/slow AoA counts as a deviation deducting grade | AoA is tracked and charted but **never penalises** the grade | High — a dangerous slow pass can still score `OK` |
| D8 | **Angular vs. foot thresholds** | Airboss compares degree deviations (distance-invariant) | App uses fixed feet at all gates → too lenient at close range (1/4 nm) | High — grade inflation at IC/AR |
| D9 | **Cumulative vs. worst-of-three** | Airboss accumulates deviation counts at all positions | App takes the **worst single gate** value | Medium — one early mistake dominates even a perfect final segment |
| D10 | **Grade for AoA only** | A slow/fast pass with good GS/LU is still `(OK)` or `--` | Impossible to penalise — AoA grade is not computed | High (same as D7) |

---

## 4. Threshold Comparison

### 4.1 Glideslope

**NAVAIR / Airboss (degrees, CVN):**

| Zone | Airboss field | Threshold |
|---|---|---|
| OK (no deviation) | `gle._min` / `gle._max` | −0.3° to +0.4° |
| Slight `(H)`/`(L)` | `gle.High` / `gle.Low` | ±0.8° |
| Significant `H`/`L` | `gle.HIGH` / `gle.LOW` | ±1.5° |

**LSO App (absolute feet):**

| Grade tier | `GS_SLIGHT` | `GS_SIGNIFICANT` | `GS_EXTREME` |
|---|---|---|---|
| Threshold | 40 ft | 100 ft | 200 ft |

**Angular equivalent at each gate:**

| Gate | Distance | 40 ft → degrees | 100 ft → degrees | 200 ft → degrees |
|---|---|---|---|---|
| 3/4 nm | 1389 m | 0.48° | 1.19° | 2.37° |
| 1/2 nm | 926 m | 0.71° | 1.78° | — |
| **1/4 nm** | **463 m** | **1.43°** | **3.57°** | — |

At the 1/4 nm gate, the app treats 40 ft (1.43°) as only a "slight" deviation — Airboss calls this `HIGH` (significant, deducts a full grade). At close range the app is **3–4× too lenient**.

### 4.2 Lineup

**NAVAIR / Airboss (degrees, CVN):**

| Zone | Airboss field | Threshold |
|---|---|---|
| OK | `lue._min` / `lue._max` | −0.5° to +0.5° |
| Slight `(LUL)`/`(LUR)` | `lue.Left` / `lue.Right` | ±1.0° |
| Large `LUL`/`LUR` | `lue.LEFT` / `lue.RIGHT` | ±3.0° |

**LSO App (absolute feet):**

| Tier | `LU_SLIGHT` | `LU_SIGNIFICANT` | `LU_EXTREME` |
|---|---|---|---|
| Threshold | 25 ft | 60 ft | 120 ft |

**Angular equivalent at each gate:**

| Gate | Distance | 25 ft → degrees | 60 ft → degrees | 120 ft → degrees |
|---|---|---|---|---|
| 3/4 nm | 1389 m | 0.30° | 0.71° | 1.43° |
| 1/2 nm | 926 m | 0.44° | 1.06° | 2.12° |
| **1/4 nm** | **463 m** | **0.88°** | **2.13°** | — |

Lineup is somewhat closer but still 1.7–2× too lenient at 1/4 nm.

---

## 5. Code Changes Required

### 5.1 `src/track.rs` — Store Angular Deviations

**Change `GateDatum`** to store angular deviation in degrees (the natural LSO unit), keeping the foot values for the chart display label.

```rust
pub struct GateDatum {
    /// Glide slope deviation from the ideal glide path in degrees
    /// (positive = high, negative = low).
    pub gs_deviation_deg: f64,
    /// Lateral lineup deviation from the angled-deck centerline in degrees
    /// (positive = right / lined up left, negative = left / lined up right).
    pub lineup_deg: f64,
    // Kept for the PNG chart label (human-readable display only)
    pub gs_deviation_ft: f64,
    pub lineup_ft: f64,
}
```

**Compute at each gate** (where `x` = gate distance in metres, the denominator for angle):

```rust
let gs_deviation_deg = (alt - ideal_gs_alt).atan2(x).to_degrees();
let lineup_deg       = y.atan2(x).to_degrees();
```

### 5.2 `src/grading.rs` — NAVAIR-Aligned Grade Scale

**Replace all constants** with degree-based thresholds matching Airboss CVN values:

```rust
// Glideslope deviation thresholds (degrees from ideal 3.5° path)
const GS_OK_MARGIN:    f64 = 0.4;   // within ±0.4° → no deviation
const GS_SLIGHT:       f64 = 0.8;   // slight (H)/(L)
const GS_SIGNIFICANT:  f64 = 1.5;   // significant H/L  → Fair pass
const GS_CUT_LOW:      f64 = -2.5;  // dangerously low at 1/4 nm → Cut

// Lineup deviation thresholds (degrees from deck centerline)
const LU_OK_MARGIN:    f64 = 0.5;   // within ±0.5° → no deviation
const LU_SLIGHT:       f64 = 1.0;   // slight (LUL)/(LUR)
const LU_SIGNIFICANT:  f64 = 3.0;   // large LUL/LUR → No Grade
```

**Rename and fix `PassGrade` enum** to match NAVAIR labels:

```rust
pub enum PassGrade {
    Ok,             // OK   — 4.0 pts — no deviations
    OkParentheses,  // (OK) — 3.0 pts — slight deviations only
    NoGrade,        // --   — 2.0 pts — significant deviations (replaces "Fair")
    Cut,            // C    — 0.0 pts — dangerously low or landed after WO
    Bolter,         // B    — 2.5 pts
    WaveoffPilot,   // WO   — 1.0 pt
}
```

**Add a `points()` method** so the greenie board and Discord embed can display numeric scores:

```rust
pub fn points(self) -> Option<f64> {
    match self {
        Self::Ok            => Some(4.0),
        Self::OkParentheses => Some(3.0),
        Self::NoGrade       => Some(2.0),
        Self::Cut           => Some(0.0),
        Self::Bolter        => Some(2.5),
        Self::WaveoffPilot  => Some(1.0),
    }
}
```

**Update `grade_from_gates`** to use degrees and the new thresholds. Also include AoA-based penalty in a follow-up pass (see §5.3).

**Updated grade mapping from degrees:**

| Condition | Grade |
|---|---|
| `gs_deg < GS_CUT_LOW` at 1/4 nm | `Cut` |
| worst `gs_deg.abs() >= GS_SIGNIFICANT` or `lu_deg.abs() >= LU_SIGNIFICANT` | `NoGrade` (`--`) |
| worst `gs_deg.abs() >= GS_SLIGHT` or `lu_deg.abs() >= LU_SLIGHT` | `OkParentheses` (`(OK)`) |
| all within OK margin | `Ok` |

### 5.3 AoA Penalty (New Logic)

Since the app already stores `Datum.aoa` per-frame, we can compute an **AoA grade** by looking at the AoA samples within the groove (x ≤ 1389 m) and applying the aircraft-specific `aoa_rating` function from `data.rs`.

Rule (matches Airboss behaviour):
- Any `Aoa::Fast` or `Aoa::Slow` sample in the groove → counts as a **significant** deviation → caps grade at `NoGrade`.
- Any `Aoa::SlightlyFast` or `Aoa::SlightlySlow` → counts as a **slight** deviation → caps grade at `OkParentheses` if no GS/LU is already worse.

The `grade_from_gates` signature will need a `datums: &[Datum]` parameter to access per-frame AoA.

### 5.4 `src/draw.rs` — Display Degrees in Chart Header

The gate deviation labels on the PNG currently show `GS +42ft  LU -18ft`.
Change to `GS +0.5°  LU -0.3°` using the new `gs_deviation_deg` and `lineup_deg` fields, since degrees are the natural comparison unit and match what Airboss reports.
Keep the foot values available for a secondary tooltip if desired.

**Update `fmt_gate`:**
```rust
fn fmt_gate(gate: Option<&GateDatum>) -> String {
    match gate {
        Some(g) => format!(
            "GS {:+.1}°  LU {:+.1}°",
            g.gs_deviation_deg, g.lineup_deg
        ),
        None => "-".to_string(),
    }
}
```

### 5.5 `src/web.rs` — CSS Grade Classes

The dashboard HTML hardcodes CSS classes based on grade labels. Update the map and classes:

| Old label | New label | CSS class |
|---|---|---|
| `Fair` | `--` | `g-NG` |
| `NG` | *(removed)* | — |
| `Cut` | `C` | `g-Cut` |

Also add **points column** to the greenie board table:

```javascript
// in gradeClass()
return ({'OK':'OK','(OK)':'OKP','--':'NG','C':'Cut','B':'B','WO':'WO'})[g] || '';
```

Add a `points` field to `StoredPass` (populated from `PassGrade::points()`) and display it in the table.

### 5.6 Test Updates (`src/tests.rs`)

All five integration test recordings currently test wire detection only. Extend each to also assert:
- The correct `PassGrade` variant is returned.
- For wire-3 passes, grade is `Ok` or `OkParentheses` (depending on GS/LU at gates).

---

## 6. Files Touched Summary

| File | Change |
|---|---|
| `src/track.rs` | Add `gs_deviation_deg`, `lineup_deg` to `GateDatum`; compute angles at gate sampling |
| `src/grading.rs` | Replace foot constants with degree constants; rename `Fair`→`NoGrade`; remove 1pt `NoGrade`; fix `Cut` label; add `points()` method; update `grade_from_gates` to use degrees and AoA |
| `src/draw.rs` | Update `fmt_gate` to show degrees; update chart-header to show points |
| `src/web.rs` | Update CSS class map and grade labels in dashboard JS; add points column |
| `src/db.rs` | *(No schema change needed — grade is stored as string label)* |
| `src/tests.rs` | Add `PassGrade` assertions to existing integration tests |

---

## 7. Out of Scope (This Change)

The following improvements identified in `LSO_ANALYSIS.md` are explicitly NOT part of this grading alignment:
- Unicorn grade (`_OK_`, 5 pts) — rare, needs groove time tracking
- OWO (Own Waveoff) variant — needs DCS event distinction between LSO WO and pilot WO
- WOFD (Foul Deck Waveoff) — needs deck fouling detection
- Cumulative deviation count (vs. worst-of-three) — larger architectural change
- Multiple carrier disambiguation
- Wind-over-deck scoring

---

## 8. Reference — NAVAIR 00-80T-104 Grade Points Quick Reference

| Pass outcome | NAVAIR label | Points |
|---|---|---|
| Unicorn | `_OK_` | 5.0 |
| Okay | `OK` | 4.0 |
| Fair / slightly below avg | `(OK)` | 3.0 |
| No grade / ugly but safe | `--` | 2.0 |
| Own waveoff (clean) | `OWO` | 2.0 |
| Bolter | `B` | 2.5 |
| Waveoff | `WO` | 1.0 |
| Cut | `C` | 0.0 |
| Foul deck waveoff | `WOFD` | ungraded |
| Pattern waveoff | `WOP` | 2.0 |
