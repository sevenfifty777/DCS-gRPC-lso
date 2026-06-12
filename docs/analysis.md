# LSO Application — Complete Analysis

## 🏗️ Architecture Overview

The LSO is a **Rust async CLI tool** (Tokio runtime) that connects to a running DCS World server via **DCS-gRPC** (gRPC/tonic) and automatically detects, records, and grades carrier recoveries. It has two operating modes:

| Mode | Command | Description |
|------|---------|-------------|
| **Live** | `lso.exe run` | Connects to DCS-gRPC in real-time, monitors all carrier/plane pairs |
| **Replay** | `lso.exe file <acmi>` | Re-processes a previously saved `.zip.acmi` recording |

---

## 📡 How Data Is Collected

### Step 1 — Initial Unit Discovery (`commands/run.rs`)
On startup, the app:
1. Calls `CoalitionService.GetGroups(All)` to enumerate all groups in the mission
2. Filters to **Airplane** and **Ship** categories
3. For each unit, calls `UnitService.GetDescriptor()` to check attributes
4. Classifies units as either:
   - **Carrier** → if attribute `"AircraftCarrier With Arresting Gear"` is present AND type matches a known carrier
   - **Plane** → if type matches a known aircraft AND has a player name (or `--ki` flag is set)
5. Spawns a `detect_recovery_attempt` task for **every carrier × plane combination**

### Step 2 — Birth Event Listening (`commands/run.rs`)
A background task streams `MissionService.StreamEvents()` and watches for `BirthEvent` to catch units that spawn **after** startup, spawning new detection tasks dynamically.

### Step 3 — Recovery Detection (`tasks/detect_recovery_attempt.rs`)
Each carrier/plane pair is polled **every 2 seconds** via `UnitService.GetTransform()`. A recovery attempt is detected when ALL of these conditions are true:
- Plane altitude < **500 ft**
- Distance to carrier between **200 m and 1.5 nm** (200m minimum filters out takeoffs)
- Plane is **behind** the carrier (dot product of carrier forward vs. ray-to-plane > 0)
- Plane nose roughly points toward carrier (dot product > **0.65**)

### Step 4 — High-Frequency Recording (`tasks/record_recovery.rs`)
Once a recovery attempt is detected, the app switches to **100ms polling** (10 Hz) and:
- Calls `UnitService.GetTransform()` for both carrier and plane simultaneously
- Simultaneously streams `MissionService.StreamEvents()` for landing events
- Records everything to an in-memory **TacView ACMI** buffer (compressed zip)
- Feeds each frame into the `Track` data structure

### Step 5 — Transform Data (`transform.rs`)
Each `GetTransform` response is converted into a `Transform` struct containing:

| Field | Source | Precision |
|-------|--------|-----------|
| `position` (x, y, z) | `Position.u`, `alt`, `Position.v` | ±0.01 m |
| `lat`, `lon` | `Position.lat`, `Position.lon` | 7 decimal places |
| `alt` | `Position.alt` | ±0.01 m |
| `heading` | `Orientation.heading` | ±0.1° |
| `yaw`, `pitch`, `roll` | `Orientation` | ±0.1° |
| `rotation` | Euler angles → DRotor3 quaternion | — |
| `forward` | Computed from yaw/pitch | — |
| `aoa` | Computed: `acos(forward · velocity_normalized)` | ±0.01° |
| `time` | Scenario time in seconds | ±0.01 s |

> **Note:** AOA is computed from the forward vector and velocity vector (not taken directly from DCS), which ensures consistency between live and replay modes.

---

## 📊 What Data Is Tracked (Per Frame)

The `Track` struct accumulates `Datum` records. Each `Datum` contains:

| Field | Meaning |
|-------|---------|
| `x` | Distance along the angled deck centerline (meters, from optimal touchdown) |
| `y` | Lateral offset from the glide slope centerline (meters, + = right, - = left) |
| `aoa` | Angle of Attack in degrees |
| `alt` | Hook altitude above deck (meters, clamped to 0) |

The coordinate system is **carrier-relative**: the x-axis is aligned to the **angled deck** (not the ship's heading), computed using the deck angle offset from BRC (Base Recovery Course).

### Static Reference Data (`data.rs`)

**Carriers supported:**
| Type | Deck Angle | Deck Alt | Notes |
|------|-----------|----------|-------|
| CVN-71/72/73/75, Stennis | 9.1359° | 20.15 m | Nimitz-class |
| CV-59 (Forrestal) | 9.42° | 18.46 m | Forrestal-class |

Each carrier has precise **cable pendant positions** (1–4) extracted from DCS ModelViewer2, stored as 3D vectors relative to the ship's origin.

**Aircraft supported:**
| Type | Hook offset | Glide slope |
|------|------------|-------------|
| FA-18C_hornet | (0, -2.24, -7.24) m | 3.5° |
| F-14A-135-GR, F-14B | (0, -1.98, -6.56) m | 3.5° |
| T-45 | (0, -1.78, -4.78) m | 3.5° |

**AOA Ratings (per aircraft):**

| Rating | FA-18C | F-14 |
|--------|--------|------|
| Fast | ≤ 6.9° | ≤ 9.7° |
| Slightly Fast | ≤ 7.4° | ≤ 10.2° |
| On Speed | < 8.8° | < 11.1° |
| Slightly Slow | < 9.3° | < 11.6° |
| Slow | ≥ 9.3° | ≥ 11.6° |

---

## 🎯 Grading Logic (`track.rs`)

### Cable Estimation
When a `RunwayTouch` event fires:
1. Computes the **hook touchdown position** = plane position + hook offset (rotated by plane attitude)
2. Compensates +3m forward (because the land event fires slightly after the wire catch)
3. Iterates cables 1→4, finds the first cable whose midpoint is **behind** the touchdown position
4. Returns that cable number

### DCS Native Grading
The `LandingQualityMark` event provides DCS's own wire number (e.g. `"WIRE# 3"`). If present, it **overrides** the estimated cable. The estimated cable is still stored for comparison.

### Grading States
```
Grading::Unknown       — tracking started but no landing yet
Grading::Bolter        — plane moved >150m away after approaching
Grading::Recovered {
    cable: Option<u8>,           — DCS-reported wire (or estimated if no DCS grade)
    cable_estimated: Option<u8>  — always the geometric estimate
}
```

### Track Termination
Recording stops when:
- Distance to carrier increases by >150m from minimum (bolter/go-around)
- 10 seconds after a `RunwayTouch` event
- Plane altitude never went below 100 ft → **discarded** (not a real approach)
- Plane crashes, dies, or player leaves unit
- Carrier or plane despawns

---

## 📤 Data Export

### 1. PNG Chart (`draw.rs`)
A bitmap image with two overlaid views:

**Side View (glide slope):**
- X-axis: distance from carrier (0 to ¾ nm), in nautical miles
- Y-axis: altitude in feet (0–350 ft)
- Guide lines at: ±0.25°, ±0.6°/0.7°, ±0.9°/1.5° from optimal glide slope (green/yellow/red)
- Approach path colored by AOA rating (red=fast → green=slow)
- Carrier side silhouette image embedded

**Top View (lineup):**
- X-axis: same distance
- Y-axis: lateral offset in nm (±0.15 nm)
- Guide lines at: 0.25°, 0.75°, 3°, 6° from centerline
- Approach path colored by AOA rating
- Carrier top-down silhouette image embedded

**Text overlay:** Pilot name + grading result (cable number or "Bolter")

### 2. TacView ACMI Recording (`.zip.acmi`)
A compressed TacView-compatible recording containing:
- `ReferenceTime` (scenario start), `RecordingTime`, `Title`, `Author`
- `ReferenceLatitude`/`ReferenceLongitude` (carrier's initial position)
- Per-frame updates at 10 Hz: carrier and plane position, orientation, AOA
- `Landed` event with carrier/plane IDs
- `Message` event with DCS LSO grading text
- Delta-compressed coordinates (only changed values written)

### 3. Discord Webhook (optional)
Posts an embed with:
- **Pilot** field (Discord mention if user mapping exists, otherwise name)
- **Grading** field (cable number or "Bolter")
- Attached PNG chart
- Attached `.zip.acmi` file

**File naming:** `LSO-YYYYMMDD-HHMMSS-<PilotName>.png` / `.zip.acmi`

---

## 🔍 Current Limitations & Gaps

1. **No proper LSO grading system** — only wire number + AOA color. No "OK", "Fair", "No Grade", "Cut Pass" grades with deviation calls (e.g. "H", "LO", "LUL", "LOLO", "F", "SLO", etc.)
2. **No glideslope deviation scoring** — the chart shows it visually but no numeric score is computed
3. **No lineup deviation scoring** — same issue
4. **No power/throttle data** — DCS-gRPC doesn't expose throttle position, but it could be inferred
5. **No greenie board** — no persistent storage of grades across sessions
6. **Only 2 carriers supported** — CVN-74 is listed in README but mapped to Nimitz data; no Super Carrier (CVN-78) support
7. **T-45 AOA brackets are wrong** — explicitly noted in code as "same as FA18C, so potentially wrong"
8. **No speed data** — airspeed not recorded in datums
9. **No wind data** — no relative wind on final
10. **No multi-carrier support** — if two carriers are present, each plane tracks both, but there's no disambiguation
11. **Polling-based detection** (2s interval) — could miss very fast approaches; event-driven would be better
12. **No CVN-74 (John C. Stennis)** — it's mapped to Nimitz data but CVN-74 is actually the John C. Stennis, not a Nimitz-class
13. **No persistence/database** — each run is independent, no historical data

---

## 🚀 Potential Improvements & New Features

### High Priority
1. **Full LSO Grading System** — implement NATOPS-style pass grades (OK, Fair, No Grade, Bolter, Cut Pass, WO) with deviation calls based on glideslope/lineup/AOA deviations at key gates (3/4 nm, 1/2 nm, in-close, at-the-ramp)
2. **Greenie Board** — persistent SQLite/JSON database storing all grades per pilot, with a Discord-based board command
3. **Numeric deviation scores** — compute glideslope deviation in tenths of a degree, lineup in feet, at each gate

### Medium Priority
4. **Add more aircraft** — AV-8B (STOVL, different logic), A-4E-C (community mod), S-3B Viking, E-2C Hawkeye
5. **Add Super Carrier (CVN-78)** — extract cable positions from DCS ModelViewer2
6. **Fix T-45 AOA brackets** — research correct T-45 approach AOA
7. **Speed data** — record airspeed in datums (available from velocity vector magnitude)
8. **Event-driven detection** — use `StreamEvents` for proximity detection instead of 2s polling
9. **Web dashboard** — serve a local HTTP server with live approach visualization

### Lower Priority
10. **JSON/CSV export** — structured data export for external analysis tools
11. **Configurable thresholds** — allow users to tune detection parameters via config file
12. **Multi-carrier disambiguation** — track which carrier a plane is actually approaching
13. **SRS radio calls** — integrate with DCS-SRS to play LSO radio calls in real-time

---

## Summary

The LSO app is a well-structured, async Rust application that correctly implements the core data pipeline: **gRPC polling → coordinate transformation → carrier-relative tracking → cable estimation → visual report generation**. The architecture is clean and extensible. The biggest gap is the absence of a proper NATOPS grading system — the app knows *which wire* was caught and *what the AOA was*, but doesn't synthesize that into a pass grade with deviation calls, which is what real LSOs actually produce.

Would you like me to develop a detailed implementation plan for any of these improvements? For example, I could plan out the full LSO grading system, the greenie board, additional aircraft support, or any other feature you're most interested in.