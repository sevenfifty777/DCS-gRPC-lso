# LSO Application — Deep Technical Analysis

## Table of Contents

1. [Overview](#1-overview)
2. [Architecture](#2-architecture)
3. [Technology Stack & Dependencies](#3-technology-stack--dependencies)
4. [Data Collection Pipeline](#4-data-collection-pipeline)
5. [Data Collected — Field-by-Field](#5-data-collected--field-by-field)
6. [Aircraft & Carrier Support](#6-aircraft--carrier-support)
7. [Data Processing & Analysis](#7-data-processing--analysis)
8. [Export Mechanisms](#8-export-mechanisms)
9. [Discord Integration](#9-discord-integration)
10. [CLI Commands & Options](#10-cli-commands--options)
11. [Testing Strategy](#11-testing-strategy)
12. [Identified Gaps & Bugs](#12-identified-gaps--bugs)
13. [Improvement & Feature Roadmap](#13-improvement--feature-roadmap)
14. [Source File Map](#14-source-file-map)

---

## 1. Overview

**LSO** (Landing Signal Officer) is a standalone Rust CLI tool that connects to a running DCS World server via the [DCS-gRPC](https://github.com/DCS-gRPC/rust-server) server and automatically monitors every carrier recovery attempt.

For each approach it:
- Detects when a carrier-capable aircraft is in a valid final approach posture.
- Records position, orientation, velocity, and AoA data at **10 Hz** (every 100 ms).
- Correlates the approach against the carrier's angled-deck geometry to produce an x/y deviation track in the carrier-relative frame.
- Generates a **PNG visual report** (side view + top-down view) with AoA-colored approach path.
- Generates a **compressed ACMI file** (TacView-compatible) of the full recovery.
- Optionally posts the report to **Discord** via webhook.

**Current version:** `0.2.0`  
**Required DCS-gRPC version:** `0.8.1`

---

## 2. Architecture

```
┌──────────────────────────────────────────────────────────────────┐
│                          DCS World Server                        │
│   (runs DCS-gRPC Lua hooks, exposes gRPC on :50051)              │
└─────────────────────┬────────────────────────────────────────────┘
                      │  gRPC (tonic / protobuf)
                      ▼
┌──────────────────────────────────────────────────────────────────┐
│                           LSO CLI                                │
│                                                                  │
│  main.rs                                                         │
│  ├── commands/run.rs         (live monitoring mode)              │
│  │   ├── Initial sync: CoalitionSvc + GroupSvc + UnitSvc         │
│  │   ├── MissionSvc event stream (BirthEvent, ...)               │
│  │   └── Per (carrier, plane) pair → spawn tokio task            │
│  │       ├── tasks/detect_recovery_attempt.rs  (2s poll)         │
│  │       └── tasks/record_recovery.rs          (100ms poll)      │
│  │                                                               │
│  └── commands/file.rs        (offline ACMI replay mode)          │
│      └── Parse .zip.acmi → extract recoveries → draw chart       │
│                                                                  │
│  Core modules:                                                   │
│  ├── transform.rs     (gRPC → internal spatial Transform)        │
│  ├── data.rs          (static AirplaneInfo / CarrierInfo tables)  │
│  ├── track.rs         (Datum accumulation + Grading logic)        │
│  ├── draw.rs          (PNG chart via plotters)                   │
│  └── client/          (thin gRPC service wrappers)               │
└──────────────────────────────────────────────────────────────────┘
                      │
                      ▼
     ┌────────────────┴──────────────────┐
     │  Output files (per recovery)      │
     │  LSO-<datetime>-<pilot>.png        │
     │  LSO-<datetime>-<pilot>.zip.acmi  │
     └───────────────────────────────────┘
                      │  (optional)
                      ▼
            Discord Webhook POST
```

### Module Map

| File | Responsibility |
|---|---|
| `src/main.rs` | CLI entry point, logging setup, graceful shutdown |
| `src/commands/run.rs` | Live mode: unit discovery, task spawning, event loop |
| `src/commands/file.rs` | Offline mode: ACMI parsing and chart re-generation |
| `src/tasks/detect_recovery_attempt.rs` | 2-second polling loop to detect approach window |
| `src/tasks/record_recovery.rs` | 100 ms recording loop + file save + Discord post |
| `src/track.rs` | Carrier-relative datum accumulation, cable estimation, bolter detection |
| `src/transform.rs` | Convert gRPC `Position/Orientation/Velocity` → `Transform` |
| `src/data.rs` | Hardcoded aircraft hook positions, AoA brackets, cable pendant coordinates |
| `src/draw.rs` | Render side-view and top-view PNG charts via `plotters` |
| `src/client/unit_client.rs` | Wraps `UnitService` gRPC calls |
| `src/client/mission_client.rs` | Wraps `MissionService` gRPC calls (events + scenario time) |
| `src/client/hook_client.rs` | Wraps `HookService` gRPC calls (mission name) |
| `src/error.rs` | Unified error enum |
| `src/utils/mod.rs` | Unit conversions (m↔nm↔ft, ft↔nm) |
| `src/utils/interval.rs` | Abortable tokio interval stream (wraps `tokio::time::interval` with shutdown support) |
| `src/utils/precision.rs` | Float precision rounding trait (`max_precision`) used to guarantee live ↔ replay determinism |
| `src/utils/shutdown.rs` | Graceful shutdown signal mechanism (`Shutdown` / `ShutdownHandle` pair) |

---

## 3. Technology Stack & Dependencies

| Crate | Version | Role |
|---|---|---|
| `tonic` | 0.11 | gRPC client (connects to DCS-gRPC) |
| `stubs` (dcs-grpc-stubs) | rev 0.8.1 | Proto-generated DCS service clients |
| `tokio` | 1.2 | Async runtime (multi-thread) |
| `backoff` | 0.4 | Exponential back-off reconnect on gRPC errors |
| `futures-util` | 0.3 | Async stream combinators (`select`, `StreamExt`) |
| `clap` | 4.0 | CLI argument parsing with `derive` |
| `tacview` | 0.2 | ACMI read/write (custom crate) |
| `plotters` + `plotters-bitmap` | 0.3 | 2D chart rendering to PNG |
| `image` | 0.24 | Embed carrier silhouette images into chart |
| `serenity` | 0.12 | Discord webhook API |
| `ultraviolet` | 0.9 (f64) | 3D math: `DVec3`, `DRotor3` for spatial transforms |
| `time` | 0.3 | Date/time formatting for file names and ACMI timestamps |
| `serde` / `serde_json` | 1.0 | Pilot→Discord-ID JSON mapping file |
| `zip` | 2.2 | ACMI `.zip.acmi` compression |
| `thiserror` | 2.0 | Error derive macros |
| `tracing` / `tracing-subscriber` | 0.1/0.3 | Structured logging |
| `once_cell` | 1.8 | Lazy statics for ACMI filename format descriptor |
| `pin-project` | 1.0 | Pin projections for async types |

---

## 4. Data Collection Pipeline

### Phase 0 — Startup & Discovery

1. Connect to DCS-gRPC (`http://127.0.0.1:50051` by default).
2. Call `CoalitionService.GetGroups` (all coalitions, no category filter) to get all groups.
3. For each **Airplane** or **Ship** group, call `GroupService.GetUnits` to enumerate active units.
4. For each unit, call `UnitService.GetDescriptor` (ships) or inspect the unit type (planes) to classify them:
   - **Carrier**: Ship with attribute `"AircraftCarrier With Arresting Gear"` and a known type in `CarrierInfo::by_type`.
   - **Plane**: Airplane type matching `AirplaneInfo::by_type` AND either human-piloted (`player_name` set) or `--ki` flag provided.
5. Spawn one `detect_recovery_attempt` task per **(carrier, plane) pair**.
6. Subscribe to `MissionService.StreamEvents` to detect future `BirthEvent`s and spawn tasks for late-spawned units.

### Phase 1 — Recovery Attempt Detection (`detect_recovery_attempt`)

Polling interval: **every 2 seconds**.

For each carrier/plane pair, call `UnitService.GetTransform` on both units simultaneously and test `is_recovery_attempt()`:

```
is_recovery_attempt(carrier, plane) → bool
  ├── plane.alt ≤ 500 ft            (altitude gate)
  ├── distance ≤ 1.5 nm             (proximity gate)
  ├── distance > 200 m              (exclude takeoffs)
  ├── plane is BEHIND the carrier   (dot(carrier.forward, ray) > 0)
  └── plane is POINTING at carrier  (dot(plane.forward, ray) > 0.65)
```

If all conditions pass, immediately invoke `record_recovery`.

### Phase 2 — Recovery Recording (`record_recovery`)

Polling interval: **every 100 ms** (10 Hz).

Runs a dual-stream loop: a timer stream and the DCS event stream, merged with `futures_util::select`:

**On each timer tick:**
1. `UnitService.GetTransform` for both carrier and plane (parallel gRPC calls).
2. Append a ACMI frame (`Record::Frame(time)`) and update records for both objects.
3. Call `Track::next(carrier, plane)` to compute and store a `Datum`.
4. Track the minimum altitude reached (`lowest_altitude`).
5. Stop if `Track::next` returns `false` (bolter/overshoot detected) and 10 s has elapsed since.

**On each DCS event:**
| Event | Action |
|---|---|
| `LandingQualityMark` (matching plane+carrier) | Store DCS grading string; write to ACMI as a `Message` event |
| `RunwayTouch` (matching plane+carrier) | Call `Track::landed`; write `Landed` event to ACMI; start 10 s post-landing window |
| `Crash` / `Dead` / `PlayerLeaveUnit` / `UnitLost` | Stop recording immediately |

**Termination condition:**
- The plane was never below 100 ft AGL → discard the recording (filtered out as not a genuine recovery).

---

## 5. Data Collected — Field-by-Field

### Per-Frame Transform (carrier and plane, every 100 ms)

| Field | Source | Description |
|---|---|---|
| `time` | `GetTransformResponse.time` | Seconds since mission start |
| `position.lat` | `Position.lat` | Geodetic latitude (°) |
| `position.lon` | `Position.lon` | Geodetic longitude (°) |
| `position.alt` | `Position.alt` | Altitude above MSL (m) |
| `position.u` | `Position.u` | DCS X coordinate (m, north = +x) |
| `position.v` | `Position.v` | DCS Z coordinate (m, east = +z) |
| `heading` | `Orientation.heading` | Magnetic heading (°) |
| `yaw` | `Orientation.yaw` | Yaw (°) |
| `pitch` | `Orientation.pitch` | Pitch (°) |
| `roll` | `Orientation.roll` | Roll (°) |
| `forward` | Derived from yaw/pitch | Unit forward vector in world frame |
| `rotation` | Derived from yaw/pitch/roll | `DRotor3` quaternion for 3D math |
| `aoa` (plane only) | Derived: `acos(forward · velocity_normalized)` | Angle of Attack (°) |

> **Note:** DCS uses an unusual right-hand coordinate system where `+x` points north. The `fix_vector()` function in `transform.rs` converts it to a standard left-hand system (`+z` north, `+x` east) before any math.

### Data NOT Currently Tracked

The following fields are available from DCS-gRPC or derivable but are **not** stored in `Datum` or recorded:

| Data | Availability | Notes |
|---|---|---|
| Airspeed | Available — `velocity.mag()` | Not added to `Datum`; would require a `speed` field |
| Vertical speed / sink rate | Derivable from consecutive `alt` values | Not computed |
| Engine RPM / throttle position | **Not exposed** by DCS-gRPC | Would require a new gRPC endpoint |
| Wind speed and direction at carrier | **Not exposed** by DCS-gRPC | Needed for correct wind-over-deck scoring |
| Aircraft weight / fuel state | **Not exposed** by DCS-gRPC | Relevant for AoA interpretation |

### Per-Datum (carrier-relative approach data, stored in `Track`)

Each `Datum` represents the aircraft's position in the **angled-deck frame**:

| Field | Description |
|---|---|
| `x` | Forward distance along the angled-deck centerline (m) |
| `y` | Lateral offset from the centerline (m, + = right, - = left) |
| `aoa` | Angle of Attack at this point (°) |
| `alt` | Hook altitude above deck (m), clamped to ≥ 0 |

The hook altitude is calculated as:
```
hook_alt = plane.alt - carrier.deck_altitude + hook_offset.y
```
where `hook_offset` is the aircraft-type-specific hook position rotated by the plane's attitude.

### Grading

| Value | Meaning |
|---|---|
| `Grading::Unknown` | Recording ended without a land or bolter event |
| `Grading::Bolter` | Distance to carrier increased > 150 m after decreasing (plane flew over deck) |
| `Grading::Recovered { cable, cable_estimated }` | Caught a wire; `cable` = DCS-reported wire (from `LandingQualityMark`), `cable_estimated` = LSO-computed wire |

The DCS-reported wire takes precedence over the estimated wire in the final `TrackResult`.

---

## 6. Aircraft & Carrier Support

### Supported Aircraft

| DCS Type Name | AoA On-Speed Bracket | Hook Position (relative to origin) | Glide Slope |
|---|---|---|---|
| `FA-18C_hornet` | 7.5° – 8.8° | (0, −2.24, −7.24) m | 3.5° |
| `F-14A-135-GR` / `F-14B` / `F-14A/B` / `F-14B(U)` | 10.2° – 11.1° | (0, −1.98, −6.56) m | 3.5° |
| `T-45` | 7.5° – 8.8° (borrows FA-18C values) | (0, −1.78, −4.78) m | 3.5° |

AoA brackets per aircraft:

| Rating | FA-18C | F-14 |
|---|---|---|
| Fast | ≤ 6.9° | ≤ 9.7° |
| Slightly Fast | ≤ 7.4° | ≤ 10.2° |
| On Speed | 7.4° – 8.8° | 10.2° – 11.1° |
| Slightly Slow | 8.8° – 9.3° | 11.1° – 11.6° |
| Slow | ≥ 9.3° | ≥ 11.6° |

### Supported Carriers

| DCS Type(s) | Model Used | Deck Angle | Deck Altitude | Notes |
|---|---|---|---|---|
| CVN-71, CVN-72, CVN-73, CVN-75, Stennis | `NIMITZ` | 9.14° | 20.15 m | Nimitz-class |
| Forrestal (CV-59) | `FORRESTAL` | 9.42° | 18.46 m | Forrestal-class |
| CVN-74 | **Not supported** | — | — | `CarrierInfo::by_type` returns `None` for this type |

> Cable positions are hardcoded 3D vectors extracted from DCS ModelViewer2 using connector data from `USS_Nimitz_RunwaysAndRoutes.lua`.

---

## 7. Data Processing & Analysis

### Coordinate Transform (carrier-relative frame)

```
1. Compute the optimal landing offset from carrier origin:
   touchdown = midpoint(cable2.left, cable3.right) - hook_offset_at_glide_slope

2. Compute ray from plane to ideal touchdown point (horizontal only, altitude ignored).

3. Project ray onto angled-deck forward axis (fb):
   x = dot(ray, fb)    ← along-track distance (nm in chart)

4. y = sqrt(|ray|² - x²), signed by lateral side:
   a = rotate(unit_x, fb_rot)
   if dot(ray, a) > 0: y is right-of-centerline (positive)
   else: y is left-of-centerline (negative)

5. Hook altitude above deck:
   alt = plane.alt - carrier.deck_altitude + (hook_offset rotated by plane_attitude).y
```

### Cable Estimation

When a `RunwayTouch` event fires, the plane has already arrested (hook past wire). The algorithm back-corrects by moving the hook touchdown 3 m forward (in carrier's deck direction) to compensate for the event delay, then finds the first cable whose midpoint is forward of the compensated touchdown position.

### Bolter Detection

During active tracking, if the distance from plane to optimal landing position starts increasing and grows by more than 150 m from the minimum reached, a bolter is declared.

### Precision Rounding

All spatial values are rounded to fixed precision before math:
- Lat/Lon: 7 decimal places
- U/V/Alt: 2 decimal places
- Yaw/Pitch/Roll/Heading: 1 decimal place
- AoA: 2 decimal places

This ensures that values computed from a live session match values re-computed from an ACMI replay (deterministic).

### Chart Rendering (draw.rs)

Two stacked panels are rendered into a 1000 px wide PNG:

**Side View (altitude vs. distance):**
- Y axis: altitude (ft), range 0 – 350 ft
- X axis: distance from carrier (nm), range 0 – 0.78 nm
- Glide-slope guide lines at ±0.25°, ±0.6°, ±0.9°, ±1.5° from nominal (3.5°)
- Approach path colored by AoA bracket

**Top View (lateral offset vs. distance):**
- Y axis: lateral offset (nm), range ±0.15 nm
- X axis: same as side view
- Centerline guide fans at 0.25°, 0.75°, 3.0°, 6.0° left and right
- Approach path colored by AoA bracket

**AoA Color Scheme:**

| Rating | Color |
|---|---|
| Fast | Red `#EF4444` |
| Slightly Fast | Orange `#EFA544` |
| On Speed | Yellow `#FEF08A` |
| Slightly Slow | Yellow-Green `#AAC522` |
| Slow | Green `#22C55E` |

---

## 8. Export Mechanisms

### 8.1 PNG Chart

- Path: `<out_dir>/LSO-<YYYYMMDD-HHMMSS>-<pilot_alphanumeric>.png`
- Format: Bitmap PNG, 1000 px wide × (dynamic height based on altitude range + top view)
- Library: `plotters` + `plotters-bitmap`
- Contains carrier silhouette images embedded from `img/carrier-top.png` and `img/carrier-side.png`

### 8.2 ACMI TacView Recording

- Path: `<out_dir>/LSO-<YYYYMMDD-HHMMSS>-<pilot_alphanumeric>.zip.acmi`
- Format: ACMI 2.2, gzip-compressed ZIP
- Library: custom `tacview` crate
- Contains:
  - `GlobalProperty::ReferenceTime` — scenario start (UTC)
  - `GlobalProperty::RecordingTime` — wall-clock recording start (UTC)
  - `GlobalProperty::Title` — `"Carrier Recovery during <mission_name>"`
  - `GlobalProperty::Author` — `"dcs-grpc-lso v<version>"`
  - `GlobalProperty::ReferenceLatitude/Longitude` — carrier's position at first frame
  - Object `id=1`: carrier (lat/lon/alt, U/V, orientation, heading, type tags, color)
  - Object `id=2`: plane (lat/lon/alt, U/V, orientation, heading, AoA, pilot name, type tags, color)
  - `Record::Frame(time)` every 100 ms
  - `record::Event { Landed }` when `RunwayTouch` fires
  - `record::Event { Message, text: "<DCS grading>" }` when `LandingQualityMark` fires
- Differential coordinate encoding: only changed properties are written per frame (`remove_unchanged`)

### 8.3 Discord Webhook

- Triggered if `--discord-webhook <url>` is provided
- Posts an **embed** with fields: `Pilot` (mentions user if in `--discord-users` map) and `Grading` (wire number or "Bolter")
- Attaches the PNG chart file
- Attaches the ACMI file
- Library: `serenity` 0.12 (Discord API)

---

## 9. Discord Integration

The Discord integration uses Serenity's webhook API:

1. `Http::new("token")` creates a client (no bot token needed for webhooks).
2. `get_webhook_from_url` parses the webhook URL to extract the webhook ID and token.
3. An embed is built with pilot name and grading.
4. If a `--discord-users` JSON file is provided (`{ "Pilot Name": discord_user_id }`), the pilot name is replaced with a `<@user_id>` mention.
5. Both the PNG and ACMI are attached directly to the webhook message.

---

## 10. CLI Commands & Options

### `lso run` — Live monitoring

```
lso run [OPTIONS]
  -o, --out-dir <PATH>           Output directory [default: .]
  --uri <URI>                    DCS-gRPC URI [default: http://127.0.0.1:50051]
  --discord-webhook <URL>        Discord webhook URL
  --discord-users <JSON_FILE>    Pilot name → Discord user ID mapping
  --ki                           Also record KI (AI) aircraft recoveries
  -v / -vv                       Increase log verbosity (INFO / DEBUG / TRACE)
  --color                        Enable ANSI color in logs
```

### `lso file` — Offline ACMI re-analysis

```
lso file <INPUT_ACMI>
```

Re-reads an LSO-generated `.zip.acmi` file, re-runs cable detection and track analysis, and re-draws the PNG chart. Output is written alongside the input file.

---

## 11. Testing Strategy

Integration tests live in `src/tests.rs` and use real ACMI recordings stored in `tests/recordings/`:

| Recording | Expected DCS Wire | Expected Estimated Wire |
|---|---|---|
| `wire_1_01_FA18C.zip.acmi` | 1 | 1 |
| `wire_2_01_FA18C.zip.acmi` | 2 | 2 |
| `wire_3_01_T45.zip.acmi` | 3 | 3 |
| `wire_4_01_FA18C.zip.acmi` | 4 | 4 |
| `wire_4_02_F14A.zip.acmi` | 4 | 4 |

Each test: parses the ACMI → extracts recoveries → asserts exactly one recovery with the expected grading.

No unit tests exist for the geometry math or chart rendering.

---

## 12. Identified Gaps & Bugs

### Functional Gaps

| # | Area | Gap |
|---|---|---|
| G1 | **Carrier support** | CVN-74 (`CarrierInfo::by_type` returns `None`) — silently ignored, never tracked |
| G2 | **Aircraft support** | AV-8B, E-2D (carrier aircraft) not supported |
| G3 | **T-45 AoA** | T-45 uses FA-18C AoA brackets verbatim — explicitly flagged as "potentially wrong" in a comment |
| G4 | **No LSO grading** | Only DCS's own `LandingQualityMark` grading is surfaced; no independent NAVAIR-style call (WO, OWO, LUL, etc.) |
| G5 | **No GS deviation score** | Altitude deviation from ideal glide slope is visualized but not quantified numerically |
| G6 | **No lineup deviation score** | Lateral offset is visualized but not quantified |
| G7 | **No power/cut assessment** | Throttle data is not collected from DCS |
| G8 | **No bolter count per session** | Bolter events are detected per attempt but not aggregated into a session/greenie board |
| G9 | **No waveoff detection** | The LSO never declares a waveoff; the approach is simply discarded if altitude never drops below 100 ft |
| G10 | **No database / persistence** | Every run is stateless; there is no pilot history or grade aggregation |
| G11 | **Single ACMI reference point** | The reference lat/lon is fixed to the carrier's first-frame position; if the carrier moves significantly, old frames could accumulate floating-point error |
| G12 | **No flag grades** | No detection of the F/A-18C "Flag" light or equivalent bad-approach indicator |
| G13 | **No multi-carrier disambiguation** | Each plane spawns a task for *every* carrier simultaneously with no logic to determine which carrier is actually being approached — can produce duplicate/phantom recordings |
| G14 | **Polling latency in detection** | 2-second polling interval could miss a very fast or very steep approach; event-driven proximity detection would eliminate this window |
| G15 | **No configurable thresholds** | All detection distances, altitude gates, and AoA brackets are hardcoded constants; users cannot tune them without recompiling |

### Code-Level Issues

| # | File | Issue |
|---|---|---|
| C1 | `data.rs` | `CarrierInfo::by_type` has an unused variable binding `t` in the catch-all arm (`t => None`) — Rust warns about this |
| C2 | `commands/run.rs` | Error propagation from spawned `tokio` tasks uses `mpsc::channel(1)` with capacity 1; if two tasks error simultaneously the second error is silently dropped |
| C3 | `tasks/record_recovery.rs` | `Http::new("token")` passes a literal `"token"` string as the bot token — this is intentional for webhooks but looks like a placeholder to readers |
| C4 | `track.rs` | Several `println!` debug lines are commented out (cable/hook position debugging) — dead code |
| C5 | `draw.rs` | A large block of old per-aircraft AoA coloring logic is commented out at the end of `aoa_color()` — dead code |
| C6 | `commands/run.rs` | `TODO: better error report than unwrap?` comment on the `file` command path |
| C7 | `data.rs` | Module-level `#![allow(unused)]` suppresses all dead-code warnings, masking any future unreferenced constants |
| C8 | `commands/file.rs` | `#[allow(unused)]` on `extract_recoveries` — function is only used in integration tests; a `#[cfg(test)]` attribute would be cleaner |

---

## 13. Improvement & Feature Roadmap

The following improvements are ordered from lowest to highest implementation complexity.

### Tier 1 — Quick Wins (1–3 days each)

#### F1: Add CVN-74 (USS John C. Stennis) support
CVN-74 is already in `CarrierInfo::by_type` under the name `"Stennis"` but the DCS type string for CVN-74 is `"CVN_74"`. Add `"CVN_74"` to the match arm alongside `"CVN_71"` etc.

#### F2: Fix T-45 AoA brackets
Research or measure the correct AoA on-speed bracket for the T-45 in DCS and replace the borrowed FA-18C values.

#### F3: Numerical GS and lineup deviation scores
At the time of key gates (3 nm, 1 nm, 0.5 nm, in-close), record the angular deviation from the ideal 3.5° glide slope and from the deck centerline and print them in the chart header / Discord embed.

#### F4: Structured JSON output
Alongside the PNG and ACMI, write a `LSO-<...>.json` file containing all `Datum` points and the final `TrackResult` as structured data. This enables downstream tooling (web dashboards, spreadsheets, greenie boards) to consume LSO data without parsing ACMI.

#### F5: Remove dead code
Remove the commented-out `println!` blocks in `track.rs` and the old `aoa_color` block in `draw.rs`.

#### F5b: Configurable thresholds via config file
Load detection and recording parameters from an `lso.toml` configuration file (using `serde` + `toml`). Configurable items: detection altitude gate, proximity distances, polling intervals, AoA brackets per aircraft. This avoids recompilation for tuning.

---

### Tier 2 — Medium Complexity (1–2 weeks each)

#### F6: NAVAIR-style pass grading
Implement the six standard call grades:
- **OK** — perfect pass
- **(OK)** — slightly below average but safe
- **Fair** — below average
- **No Grade** — dangerous deviation
- **Cut** — unsafe, below glide slope at the ramp
- **Bolter**
- **WO** — waveoff (plane climbed away before touchdown)

This requires defining numeric deviation thresholds for each gate (3 nm, 1 nm, in-close, at the ramp) in both GS and lineup, combined with AoA rating. The grading algorithm is specified in NAVAIR 00-80T-104.

#### F7: Waveoff detection
Declare a waveoff when: the aircraft was inside 0.75 nm AND below 300 ft AND then climbed above 500 ft without a `RunwayTouch` event. Currently these attempts are silently discarded.

#### F8: Support additional carrier aircraft
Add hook positions and AoA brackets for:
- **AV-8B** (ski-jump / VSTOL — different logic needed, no arresting gear)
- **E-2D Hawkeye** (arrested landing, different AoA bracket)
- **S-3B Viking** (legacy, present in some missions)
- **A-4E-C** (community mod, frequently used on carrier servers)

#### F9: Session summary and greenie board (local)
At the end of a session (on CTRL+C), aggregate all completed passes by pilot and print/export a text-based greenie board showing each pilot's pass grades.

#### F10: Multiple carrier/plane ACMI tracks
Currently only the carrier and plane involved in the recovery are written to the ACMI (IDs 1 and 2). Add all active mission objects (other aircraft in the pattern, ship escorts) to give more context in TacView.

#### F10b: Wind-over-deck computation
Track carrier speed vector and sample DCS atmosphere data to compute relative wind at the carrier. Factor wind-over-deck into glide slope corrections and include it in the report and Discord embed. The `AtmosphereService` in DCS-gRPC already exposes wind data.

#### F10c: Ball call detection
Detect when a pilot calls "ball" (via SRS voice keyword detection or a DCS scripting trigger) and use that as the definitive start of the graded segment rather than the 1.5 nm proximity gate. Integrate with F12 (SRS) for the trigger mechanism.

---

### Tier 3 — Large Features (weeks to months)

#### F11: Web dashboard with greenie board
Build a simple HTTP server (using `axum` or `warp`) that LSO serves alongside its normal operation. The dashboard would display:
- Current recoveries in progress (live)
- Historical pass grades per pilot
- Aggregated statistics (average AoA, average lineup deviation, bolter rate)

#### F12: SRS radio calls integration
Using the existing `srs/` crate in the same workspace, synthesize LSO radio calls (via the `tts/` crate — AWS Polly, Azure TTS, Google TTS, or Windows TTS) triggered by approach deviations. For example, "POWER" when AoA is slow, "LINEUP" when laterally offset.

This would require:
1. Subscribing to the in-progress `Datum` stream in real time (currently only collected after the fact).
2. Defining call trigger thresholds.
3. Connecting to SRS using the existing SRS client crate.

#### F13: Persistent database
Replace the stateless in-memory model with a SQLite database (`rusqlite` or `sqlx`) to store:
- All recovery attempts with full datum arrays
- Pilot history across missions
- Session metadata (mission name, server, date)

#### F14: Real-time approach overlay
Stream current position data to a lightweight WebSocket endpoint. A browser client could render a live moving-map approach display, similar to a ship's PLAT camera HUD.

#### F15: Multi-carrier, multi-server support
Allow LSO to monitor multiple DCS-gRPC servers simultaneously (e.g., a training server and an operational server) and consolidate all recoveries into a single greenie board.

#### F16: PLAT camera simulation
Render a simulated **PLAT (Pilot Landing Aid Television)** view from the carrier's stern perspective: a synthetic camera feed showing the final approach path overlaid with ball, datum, and cut lights. Could be served as a video stream (MJPEG) or rendered as a PNG sequence alongside the existing chart.

---

## 14. Source File Map

Approximate source lines of code (SLOC) per file, version 0.2.0:

| File | ~SLOC | Purpose |
|---|---|---|
| `src/main.rs` | 70 | Entry point, CLI parsing, tracing setup, graceful shutdown |
| `src/commands/run.rs` | 337 | Live mode: unit discovery, task spawning, birth event loop |
| `src/commands/file.rs` | 383 | Replay mode: parse ACMI, extract tracks, re-draw charts |
| `src/tasks/detect_recovery_attempt.rs` | 104 | 2 s polling loop, approach detection conditions |
| `src/tasks/record_recovery.rs` | 518 | 100 ms recording loop, ACMI write, Discord post |
| `src/tasks/mod.rs` | 27 | `TaskParams` shared context struct |
| `src/track.rs` | 248 | `Track`, `Datum`, `Grading`, cable estimation |
| `src/transform.rs` | 80 | DCS gRPC → `Transform` conversion |
| `src/data.rs` | 273 | Static carrier and aircraft reference data |
| `src/draw.rs` | 452 | PNG chart generation via `plotters` |
| `src/client/hook_client.rs` | 24 | gRPC `HookServiceClient` wrapper |
| `src/client/mission_client.rs` | 50 | gRPC `MissionServiceClient` wrapper |
| `src/client/unit_client.rs` | 66 | gRPC `UnitServiceClient` wrapper |
| `src/error.rs` | 19 | Unified error enum |
| `src/utils/mod.rs` | 23 | Unit conversion functions |
| `src/utils/interval.rs` | 13 | Abortable interval stream |
| `src/utils/precision.rs` | 10 | Float precision rounding trait |
| `src/utils/shutdown.rs` | 163 | Graceful shutdown signal mechanism |
| `src/tests.rs` | 57 | Integration tests with ACMI recordings |
| **Total** | **~2,650** | 19 source files |

---

## Summary Table

| Area | Current State | Key Gap | Priority Improvement |
|---|---|---|---|
| Aircraft support | 3 types (F/A-18C, F-14A/B, T-45) | T-45 AoA wrong; no AV-8B/E-2D | F2, F8 |
| Carrier support | 5 Nimitz + 1 Forrestal (CVN-74 broken) | CVN-74 ignored | F1 |
| Data collected | Position, orientation, AoA @ 10 Hz | No throttle, no roll rate | F6, F7 |
| Grading | DCS native wire only | No NAVAIR pass grade | F6 |
| Export | PNG + ACMI + Discord | No structured JSON | F4 |
| Persistence | None (stateless) | No pilot history | F13 |
| Feedback loop | None | No real-time radio calls | F12 |
| Waveoff | Silently discarded | Not counted or reported | F7, F9 |
| Speed / sink rate | Not collected | Not in `Datum` struct | F4 (JSON export enables this) |
| Multi-carrier | All pairs tracked simultaneously | No disambiguation logic | G13 |
| Thresholds | Hardcoded constants | Not user-configurable | F5b |
