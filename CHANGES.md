# LSO Changelog

This file records user-visible changes. The crate version remains `0.2.0`; changes after the
`0.2.0` tag are therefore listed under Unreleased.

## Unreleased

### Added

- Session/generation-aware supervision, two-second RPC/watchdog deadlines, monotonic freshness and
  explicit skew/gap diagnostics with conservative short extrapolation.
- Strict AV-8B/Tarawa and hook-aircraft/arrested-carrier pairing, slot/UCID human identity and
  session-scoped AI identity.
- Bracketed gate interpolation with `Valid`, `Late`, `Missing` and `Invalid` evidence, plus ordered
  raw `Land`/`RunwayTouch`/LQM evidence and raw hook observations.
- Separate estimated/DCS wire provenance, divergence, confidence, completeness, cause and grading
  version in schema-v2 reports and additive SQLite migrations.
- Bounded telemetry/event buffers and runtime RPC, stream, queue, IO and render metrics.
- Simplified NAVAIR-style grading from 3/4, 1/2, and 1/4 nm glideslope and lineup samples, with
  `_OK_`, `OK`, `(OK)`, `--`, `C`, `B`, and `WO` labels and numeric points.
- Neutral unknown-initiator waveoff/go-around evidence, conservative touch-and-go handling, and
  explicit pass outcome storage.
- A second PNG showing the overhead carrier pattern in the BRC frame.
- Pretty-printed JSON recovery reports with gate samples, final-approach datums, and mission time.
- Persistent `<out-dir>/lso.db` storage, automatic migrations for older databases, and pilot UCID,
  aircraft, map, UTC grade time, mission time, points, and outcome fields.
- Optional HTTP greenie board (`--web-port`) and `/api/passes` JSON endpoint.
- Session greenie board printed on shutdown.
- Richer Discord embeds with map, UTC and mission time, grade/points, outcome, gate deviations, DCS
  notation translated to plain English, wind, and groove time. Both approach and pattern PNGs are
  attached; ACMI is attached unless disabled.
- `--no-acmi` to keep charts and JSON without saving Tacview recordings.
- Additional F-14 type aliases and distinct F-14A, F-14B, and F-14B(U) display names.
- Carrier-position EMA smoothing for final-approach geometry.

### Changed

- The former automatic wire-3/groove-time `_OK_` rule is disabled. `_OK_` is reserved for an
  explicit official/manual grade; incomplete passes and touch-and-go outcomes receive no points.
- PNG rendering and SQLite work run outside latency-sensitive sampling tasks. Atomic artifact names
  include session/generation/unit identity and database inserts are idempotent.
- The HTTP dashboard now binds to `127.0.0.1` and returns HTTP 500 on database failure.
- The detection envelope now captures the full pattern: 200 m to 3.5 nm from the carrier and at or
  below 1,100 ft MSL, without nose-pointing or rear-hemisphere checks.
- Gate sampling is restricted to inbound crossings below 500 ft above the deck, and groove entry
  also requires lineup within 10 degrees.
- The DCS-gRPC client stubs now come from the official sevenfifty777 fork release tag `v0.9.0`,
  resolved in `Cargo.lock` to commit `5bd6d6e42491c8697a5c5a95e80a2e689923bd3b`; `tonic` was
  updated to 0.13.
- Unit discovery safely ignores DCS units whose optional type field is absent.
- T-45 AoA brackets now use values derived from the VNAO T-45 display-electronics data instead of
  the former F/A-18C copy.

### Fixed

- Recovery-monitor tasks for respawned units replace stale tasks instead of accumulating duplicate
  recordings after mission changes.
- Recording ends when a plane exits the 3.5 nm / 1,100 ft pattern envelope, preventing indefinite
  ACMI capture after a missed approach or mission change.
- Carrier-position smoothing reduces periodic sawtooth artifacts in final-approach charts and gate
  measurements.
- CATOBAR charts select the latest continuous inbound branch, preventing earlier overhead-pattern
  points from joining the real final as a false vertical drop.
- F-14B(U) identification and trap-sheet naming.

### Security and dependencies

- The lockfile was refreshed for DCS-gRPC 0.9.0 and its gRPC stack.
- Known vulnerable transitive versions identified during the migration were updated. The recorded
  audit result and remaining allowed maintenance warning are documented in
  [DCS_GRPC_FORK_MIGRATION.md](docs/DCS_GRPC_FORK_MIGRATION.md).

## 0.2.0 - 2024-11-10

- Initial tagged `0.2.0` release. It provided live DCS-gRPC monitoring, carrier-relative recovery
  charts, wire estimation, compressed ACMI recording, Discord webhook delivery, and ACMI replay.

Earlier tags: `0.1.1`, `0.1.0`.
