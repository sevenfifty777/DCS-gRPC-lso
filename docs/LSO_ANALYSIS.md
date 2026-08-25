# LSO Technical Analysis

**Implementation baseline:** 2026-08-25, crate version `0.2.0`, repository commit `b9ac263` before
this documentation refresh.

This is the canonical code-level overview. Operational instructions live in
[ADMIN_GUIDE.md](ADMIN_GUIDE.md), while exact grade thresholds live in
[GRADING_REFERENCE.md](GRADING_REFERENCE.md).

## Architecture

LSO is a single Rust binary with two modes:

| Mode | Command | Behavior |
|---|---|---|
| Live | `lso run` | Connects to DCS-gRPC, discovers units, records and persists passes, and optionally serves/publishes results |
| Replay | `lso file <INPUT>` | Parses an LSO-created compressed ACMI and regenerates final-approach PNGs in the current directory |

The live pipeline is:

```text
DCS World + DCS-gRPC 0.9.0
  -> initial group/unit discovery and Birth-event tracking
  -> one detector per supported carrier/aircraft pair (2 s polling)
  -> recovery recorder (100 ms polling plus mission events)
  -> carrier-relative Track and grade computation
  -> approach PNG + pattern PNG + JSON + optional ACMI
  -> SQLite row + terminal summary + optional HTTP/Discord output
```

The process uses Tokio's multithreaded runtime. A connection or mission-stream failure is treated as
transient and retried with exponential backoff, capped at a 30-second interval. Ctrl-C triggers the
shared shutdown signal and prints the in-memory session board.

## Module map

| Path | Responsibility |
|---|---|
| `src/main.rs` | CLI parsing, logging filter, Ctrl-C shutdown, command dispatch |
| `src/commands/run.rs` | gRPC connection lifecycle, unit discovery, Birth events, task deduplication, terminal board |
| `src/commands/file.rs` | Compressed ACMI parsing and offline approach-chart regeneration |
| `src/tasks/detect_recovery_attempt.rs` | Wide-pattern detector for each carrier/plane pair |
| `src/tasks/record_recovery.rs` | Live sampling, event correlation, output generation, DB write, Discord post |
| `src/track.rs` | Pattern/final coordinates, gate capture, outcome state, groove time, wire estimation |
| `src/grading.rs` | Grade labels, points, thresholds, and pass-grade selection |
| `src/data.rs` | Supported aircraft, AoA bands, hook offsets, carrier geometry, cable locations |
| `src/draw.rs` | Final-approach and overhead-pattern PNG rendering |
| `src/db.rs` | SQLite schema, migrations, inserts, and newest-first queries |
| `src/lso_notation.rs` | Plain-English translation of DCS LSO notation |
| `src/client/` | Thin wrappers for DCS-gRPC unit, mission, hook, world, net, and atmosphere services |
| `src/transform.rs` | DCS transform conversion, rounding, rotation, forward vector, and calculated AoA |
| `src/utils/` | Unit conversions, shutdown, intervals, and float precision |

## Unit discovery and task lifecycle

At connection time, LSO requests all coalition groups, keeps airplane and ship groups, and queries
their active units. A supported airplane must have a known DCS type and either a player name or the
`--ki` option. A supported ship must expose the `AircraftCarrier With Arresting Gear` descriptor
attribute and match known carrier geometry.

LSO creates a detector for every supported carrier/aircraft pair. `MissionService.StreamEvents`
adds pairs for later Birth events. The active-task map is keyed by `(plane_id, carrier_id)`; a
respawn aborts and replaces the stale task for that pair.

An absent optional `Unit.type` is logged at debug level and ignored. This is required by the
DCS-gRPC 0.9.0 protobuf representation.

## Detection and recording

Each detector polls both transforms every two seconds. It starts recording when the plane is:

- no more than 1,100 ft MSL;
- no farther than 3.5 nm from the carrier; and
- at least 200 m away, which filters aircraft sitting on or taking off from the deck.

There is no heading, nose-pointing, or quadrant condition. This is deliberate so the full overhead
pattern can be captured. During recording, carrier and aircraft transforms are sampled every 100 ms
and the mission event stream is watched for `LandingQualityMark`, `RunwayTouch`, crash, death,
player-leave, and unit-loss events.

Recording stops when the outcome is resolved and the aircraft moves more than 150 m away from its
minimum distance, when a unit disappears, when the plane leaves the 3.5 nm / 1,100 ft envelope, or
after the post-touchdown window. Attempts that never go below 100 m MSL are discarded, as are tracks
that finish with `Grading::Unknown`.

## Coordinate systems and smoothing

DCS world vectors are converted to the internal east/up/north layout. Each transform contains world
position, geographic position, attitude, heading, forward vector, velocity-derived AoA, and mission
time. Selected values are rounded so live and ACMI replay processing remain stable.

Two carrier-relative streams are accumulated:

- `PatternDatum` uses the raw carrier position and ship BRC. It stores astern/port distance, MSL
  altitude, AoA, and sample time for the overhead chart.
- `Datum` uses the angled-deck axis and the aircraft-specific optimal touchdown point. It stores
  along-deck distance, lateral offset, hook altitude above deck, AoA, and sample time.

The final-approach origin uses an exponential moving average of carrier position with alpha `0.15`.
This mitigates DCS carrier-position steps. Pattern data and cable estimation retain raw transforms.

## Outcomes, gates, and grading

The internal outcome enum distinguishes:

| Outcome | Meaning |
|---|---|
| `Recovered` | A matching `RunwayTouch` was observed; DCS wire text overrides the geometric estimate when parseable |
| `IntentionalBolter` | The aircraft flew away with the hook observed up; displayed as `Qualif Bolter` |
| `Bolter` | The aircraft passed the deck or touched down and then moved away without arresting |
| `WaveoffPilot` | The aircraft entered the groove and moved away without reaching the deck |
| `Unknown` | No recognizable outcome; the live recorder discards it |

Hook-up state is sampled from DCS draw argument 25. A geometric wire estimate uses the rotated hook
offset and carrier cable midpoints; the DCS `LandingQualityMark` wire takes precedence in a recovered
result.

Code-review note: the cable estimator passes the degree-valued `deck_angle` directly to
`DRotor3::from_rotation_xz`, whose implementation applies trigonometric functions to the supplied
angle. Other rotation sites convert degrees to radians. The current ACMI fixture tests still pass,
so this unit mismatch should receive a focused test before any behavior change.

Gate samples are captured on the first inbound crossing at 1,389 m, 926 m, and 463 m while the hook
is below 500 ft above the deck. Groove entry requires the aircraft to be inside 3/4 nm, below 300 ft
above the deck, and within 10 degrees of lineup. Gate degrees drive the pass grade; stored foot
values are presentation data. AoA is not part of grade computation.

See [GRADING_REFERENCE.md](GRADING_REFERENCE.md) for the precise thresholds and special-case rules.

## Outputs and persistence

Live mode creates two PNGs, one JSON sidecar, an optional compressed ACMI, and one SQLite row per
saved pass. JSON contains the final-approach datums but not `pattern_datums`, map, UCID, numeric
points, outcome display text, wind, or groove time. Those values are available only in other output
surfaces where implemented.

The database is created at `<out-dir>/lso.db`. Startup applies additive migrations for columns added
after the original schema. Queries return all rows newest first; there is currently no pagination or
retention policy.

The HTTP board serves `/` and `/api/passes`, refreshes every ten seconds, and binds to
`0.0.0.0:<port>`. It has no authentication, authorization, or TLS. Database query/task failures in
the API handler currently degrade to an empty list.

Discord posts contain two PNG attachments and the ACMI when enabled. The embed includes aircraft,
map, wall-clock UTC time, optional mission time, pilot, grade and points, outcome, gate values, DCS
notation and translation, optional wind, and optional groove time. JSON is not attached.

Replay mode is intentionally narrower: it parses only LSO-authored ACMI metadata and writes the
final-approach PNG to the process's current working directory. It does not recreate JSON, pattern
PNG, database rows, or external notifications.

## Supported geometry

Supported airplanes are F/A-18C, F-14A, F-14B, F-14B(U), and VNAO T-45C under the exact type aliases
listed in [the README](../README.md#supported-units). Supported carrier geometry is Nimitz-class
(`CVN_71`, `CVN_72`, `CVN_73`, `CVN_75`, `Stennis`) and Forrestal (`Forrestal`).

`get_aircraft_id` also contains numeric mappings for AV-8BNA and A-6E, but those types are not
accepted by `AirplaneInfo::by_type` and therefore are not monitored. The numeric mapping alone does
not constitute aircraft support.

## Test coverage and validation boundaries

The suite includes five ACMI wire-estimation fixtures, grading branch/threshold tests, LSO-notation
translation tests, carrier touchdown geometry, and a visual-artifact generator. The generated chart
test writes images for manual inspection; it does not assert pixels.

Automated tests do not replace a live integration pass against DCS World and the pinned DCS-gRPC
server. Discord delivery, wind queries, UCID lookup, mission changes, firewall behavior, and web
exposure need environment-level validation.

## Current limitations and review findings

- Grade computation uses only three GS/LU snapshots. It does not score AoA, trend, power, sink rate,
  wind-over-deck, aircraft weight, or an LSO-called waveoff.
- Detection and pairing are polling-based and every supported plane is paired with every supported
  carrier; there is no intended-carrier disambiguation.
- Thresholds, supported types, polling intervals, and network bind host are compile-time behavior;
  there is no configuration file.
- The web board is a full-history table with no auth, pagination, filtering, statistics, or live
  approach view.
- The CLI dispatch path unwraps command errors, so fatal failures terminate with Rust panic output
  instead of a polished user-facing error.
- The web API converts database/task failures to `[]`, which can look like a genuinely empty board.
- Database migration statements ignore all `ALTER TABLE` errors, which can hide failures other than
  the expected duplicate-column condition.
- The `GS_SLIGHT_LOW` executable constant is 0.5 degrees, while one comment and test name refer to
  0.8 degrees; the existing `-0.9` test input does not disambiguate the boundary.
- Discord webhook URLs are accepted only as a CLI argument. Operators must protect process/service
  configuration and avoid committing or sharing the URL.
- Offline replay does not reproduce every live artifact.
- DCS-gRPC and live DCS behavior remain coupled to the exact pinned fork revision.
