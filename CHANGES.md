# LSO Changelog

All notable changes to the LSO carrier recovery analysis tool are documented here.

---

## [Unreleased] — Tier 3 additions — 2026-06-10

### Added

#### F11 — Web Greenie Board Dashboard (`src/web.rs` — new module)

A lightweight HTTP dashboard is now available when the `--web-port` option is provided.

**New CLI option:**
```
lso run --web-port 8080
```

**Routes:**

| Method | Path         | Description                               |
|--------|--------------|-------------------------------------------|
| GET    | `/`          | Self-contained HTML greenie board page    |
| GET    | `/api/passes`| JSON array of all recorded passes (newest first) |

The HTML page is fully self-contained (no external CDN), auto-refreshes every 10 seconds, and applies colour-coded grade cells:

| Grade  | Colour         |
|--------|----------------|
| OK     | Green bold     |
| (OK)   | Light green    |
| Fair   | Yellow         |
| NG     | Orange bold    |
| Cut    | Red bold       |
| B / WO | Gray           |

All user-supplied values (pilot names, DCS grade strings) are HTML-escaped in the browser to prevent XSS.

The web server is started as a background `tokio::spawn` task and shares the same `SharedDb` as the recording tasks. It runs until the process exits.

**New dependencies:**
- `axum = "0.7"` — async HTTP framework (hyper 1.x, compatible with tonic 0.11)
- tokio `"net"` feature added for `TcpListener`

> After adding axum, run `cargo audit` to verify no known CVEs.

#### F13 — Persistent SQLite Database (`src/db.rs` — new module)

Every completed recovery pass is now persisted to `<out_dir>/lso.db` (a SQLite database created automatically on first run).

**Schema:**
```sql
CREATE TABLE IF NOT EXISTS passes (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    timestamp   TEXT    NOT NULL,   -- LSO-YYYYMMDD-HHMMSS-Pilot prefix
    pilot_name  TEXT    NOT NULL,
    pass_grade  TEXT    NOT NULL,   -- PassGrade label: "OK", "(OK)", etc.
    wire        INTEGER,            -- NULL for bolter / waveoff
    dcs_grading TEXT                -- raw DCS LandingQualityMark comment
);
```

**API:**
```rust
pub struct RecoveryDb { /* Mutex<Connection> */ }
impl RecoveryDb {
    pub fn open(path: &Path) -> rusqlite::Result<Self>;
    pub fn insert(&self, pass: &DbPass) -> rusqlite::Result<()>;
    pub fn all_passes(&self) -> rusqlite::Result<Vec<StoredPass>>;
}
pub type SharedDb = Arc<RecoveryDb>;
```

- `RecoveryDb` is wrapped in `Arc` (`SharedDb`) and cloned across all tasks and the web server.
- DB writes in `record_recovery.rs` use `tokio::task::spawn_blocking` so the async runtime is not blocked. Write failures are logged as errors and **do not abort the recovery**.
- `SharedDb` was added to `TaskParams` and passed down from `execute()` → `run()` → each spawned task.

**New dependency:**
- `rusqlite = { version = "0.31", features = ["bundled"] }` — SQLite client with bundled C source (no system SQLite required)

> After adding rusqlite, run `cargo audit` to verify no known CVEs.

---

## [Unreleased] — Tier 2 additions — 2026-06-10

### Added

#### F6 — NAVAIR-style Pass Grading (`src/grading.rs`, `src/track.rs`, `src/draw.rs`, `src/tasks/record_recovery.rs`)

New module `src/grading.rs` implements a simplified NAVAIR 00-80T-104 pass grading algorithm.

**`PassGrade` enum:**

| Grade          | Label | Points | Description                         |
|----------------|-------|--------|-------------------------------------|
| `Ok`           | OK    | 4      | All gates within ±40 ft GS, ±25 ft LU |
| `OkParentheses`| (OK)  | 3      | Slight deviation (< 100 ft GS / 60 ft LU) |
| `Fair`         | Fair  | 2      | Significant deviation (< 200 ft GS / 120 ft LU) |
| `NoGrade`      | NG    | 1      | Extreme deviation at any gate       |
| `Cut`          | Cut   | 0      | GS < −150 ft at 1/4 nm (dangerously low at ramp) |
| `Bolter`       | B     | —      | Bolter                              |
| `WaveoffPilot` | WO    | —      | Pilot waveoff                       |

**Threshold constants (feet):**

| Threshold       | GS   | LU   |
|-----------------|------|------|
| Slight          |  40  |  25  |
| Significant     | 100  |  60  |
| Extreme         | 200  | 120  |
| Cut (low @ ramp)| −150 | —    |

- `compute_pass_grade(&Grading, &GateDeviations) -> PassGrade` is the public entry point.
- `TrackResult` now carries `pass_grade: PassGrade`, computed in `Track::finish()`.
- `RecoveryReport` (JSON) includes `pass_grade`.
- Discord embed gains a **"Grade"** inline field and the **"Grading"** field was renamed to **"Outcome"** to distinguish the NAVAIR grade from the wire/bolter/waveoff outcome.
- PNG chart now shows `Grade: <label>` on a second line beneath the pilot name, with the wire/waveoff on the third line.
- 7 unit tests in `grading::tests` cover every grade branch.

#### F7 — Waveoff Detection (`src/track.rs`, `src/tasks/record_recovery.rs`)

- New `Grading::WaveoffPilot` variant added to the `Grading` enum.
- `Track` gains an `entered_groove: bool` field, set to `true` when the aircraft is simultaneously:
  - inside 3/4 nm of the carrier (x ≤ 1 389 m), **and**
  - at or below 300 ft AGL on the deck-relative altitude.
- `Track::finish()` now resolves any `grading = None` + `entered_groove = true` combination to
  `Grading::WaveoffPilot`, so waveoffs are no longer silently discarded.
- Discard logic in `record_recovery.rs` was rationalised:
  - The old altitude-only threshold (`lowest_altitude > 100 m MSL`) is kept as a coarse pre-filter.
  - A second, post-finish check now discards only if `track.grading == Grading::Unknown`
    (no groove entry at all), ensuring that waveoffs, bolters, and recoveries are all saved.
- `draw_chart()` overlays `"Waveoff"` on the chart for waveoff passes.

#### F9 — Session Greenie Board (`src/tasks/mod.rs`, `src/commands/run.rs`)

A session-scoped pass log is accumulated across all recovery tasks and printed to stdout when
the process exits (Ctrl-C or clean shutdown).

**New types in `tasks/mod.rs`:**
```rust
pub struct CompletedPass {
    pub timestamp:   String,      // LSO-YYYYMMDD-HHMMSS-Pilot prefix
    pub pilot_name:  String,
    pub pass_grade:  PassGrade,
    pub wire:        Option<u8>,
    pub dcs_grading: Option<String>,
}
pub type SessionLog = Arc<Mutex<Vec<CompletedPass>>>;
```
- `TaskParams` gains `session_log: SessionLog`.
- `record_recovery.rs` appends a `CompletedPass` to the log after every saved recovery.
- `execute()` in `run.rs` creates the `SessionLog`, passes it (cloned Arc) into each spawned
  `detect_recovery_attempt` task, and calls `print_greenie_board()` before returning.

**Greenie board terminal output (example):**
```
╔══════════════════════════════════════════════════════════╗
║              SESSION GREENIE BOARD                       ║
╠═══════════════════════╦══════╦══════╦════════════════════╣
║ Pilot                 ║ Wire ║ Grd  ║ DCS Grade          ║
╠═══════════════════════╬══════╬══════╬════════════════════╣
║ John Doe              ║  3   ║ OK   ║ 3 WIRE# 3          ║
║ Jane Smith            ║  -   ║ WO   ║ -                  ║
╚═══════════════════════╩══════╩══════╩════════════════════╝
```

---

## [Unreleased] — Tier 1 additions — 2026-06-10

### Added

#### F3 — Gate Deviation Scores (`src/track.rs`, `src/draw.rs`)

Two new public types capture glide-slope and lineup deviations at the three standard
LSO grading gates:

```rust
pub struct GateDatum {
    pub gs_deviation_ft: f64,   // positive = high, negative = low
    pub lineup_ft: f64,         // positive = right of centreline, negative = left
}

pub struct GateDeviations {
    pub at_three_quarter_nm: Option<GateDatum>,
    pub at_half_nm:           Option<GateDatum>,
    pub at_quarter_nm:        Option<GateDatum>,
}
```

Gate distances used (first-crossing sample, metres):

| Gate    | Metres |
|---------|--------|
| 3/4 nm  | 1 389  |
| 1/2 nm  |   926  |
| 1/4 nm  |   463  |

- `Track` struct now owns a `gate_deviations: GateDeviations` field, initialised to
  `Default` in `Track::new()`.
- `Track::next()` samples GS deviation (`alt − ideal_gs_alt`) and lateral lineup (`y`)
  at each gate on first crossing; subsequent frames are ignored.
- `TrackResult` carries the populated `gate_deviations` field out of `Track::finish()`.
- `draw_chart()` overlays three gate-reading lines on the PNG chart immediately below
  the grading label (y = 80 / 108 / 136 px, 18 pt font).
- New helper `fmt_gate()` in `draw.rs` formats a `GateDatum` as `GS ±XXft  LU ±XXft`.
- `GateDatum` and `GateDeviations` derive `Debug`, `PartialEq`, `Default` (where
  applicable), and `serde::Serialize`.

#### F4 — JSON Output (`src/tasks/record_recovery.rs`)

After each recovery attempt a `.json` file is now written alongside the existing
`.zip.acmi` and `.png` files.

New `RecoveryReport` serde struct (private, local to `record_recovery.rs`):

```rust
struct RecoveryReport<'a> {
    pilot_name:      &'a str,
    grading:         &'a Grading,
    dcs_grading:     Option<&'a str>,   // omitted when None
    gate_deviations: &'a GateDeviations,
    datums:          &'a [Datum],
}
```

File written with `serde_json::to_vec_pretty` — no new dependencies required
(`serde` / `serde_json` were already present in `Cargo.toml`).

The Discord webhook embed gains a new **"Gates (GS / LU)"** field showing all three
gate deviations in one block, e.g.:

```
3/4nm: +12ft / -3ft
1/2nm:  +8ft / -1ft
1/4nm:  +2ft /  0ft
```

### Fixed

#### F2 — T-45 AoA Brackets (`src/data.rs`)

The T-45 angle-of-attack rating brackets were previously copy-pasted from the FA-18C
with a `// TODO: potentially wrong` comment.  They have been replaced with values
derived from the VNAO T-45 v1.0.2 Display Electronics Unit (`DisplayElectronicsUnit.lua`)
using the indexer thresholds and the conversion formula
`degrees ≈ UNITS_AOA − 10`.

| Rating        | Old (FA-18C copy) | New (VNAO DEU)  |
|---------------|-------------------|-----------------|
| Fast          | ≤ 6.9°            | ≤ 6.0°          |
| SlightlyFast  | ≤ 7.4°            | ≤ 6.5°          |
| OnSpeed       | < 8.8°            | < 7.5°          |
| SlightlySlow  | < 9.3°            | < 8.0°          |
| Slow          | ≥ 9.3°            | ≥ 8.0°          |

### Changed

#### C1 — `data.rs` code hygiene (`src/data.rs`)

- Removed `#![allow(unused)]` crate-level attribute — all items are used.
- Both wildcard `match` arms (`t => None`) were renamed to `_ => None` to suppress
  the unused-variable warning.
- Added comment on the `"Stennis"` match arm to clarify that CVN-74 (`USS_CVN_74.lua`
  sets `GT.Name = "Stennis"`) is already covered by that arm.

#### C7 — `Datum` and `Grading` serde derives (`src/track.rs`)

- Added `serde::Serialize` to `Datum` (required for JSON output).
- Added `serde::Serialize` to `Grading` (required for JSON output).

### Removed

#### F5a — Dead `println!` blocks in `estimate_cable()` (`src/track.rs`)

Eight commented-out `// println!(...)` blocks used for visual debugging of cable
estimation were removed:

- `name;x;y;z` header line
- `plane_position` line
- `hook_touchdown` line
- `cable_N` mid-cable lines (×4, one per wire)
- `p0_N` / `p1_N` pendant endpoint lines (×8)

#### F5b — Legacy per-aircraft AoA block in `aoa_color()` (`src/draw.rs`)

A large `/* ... */` block (~40 lines) containing the original per-type AoA colour
logic (hard-coded F-14 and FA-18C thresholds duplicating `data.rs`) was removed.
The function now delegates entirely to `AirplaneInfo::aoa_rating()`.

---

## [0.2.0] — prior release

Initial public release of the LSO carrier recovery tool.  See the project README for
feature details.
