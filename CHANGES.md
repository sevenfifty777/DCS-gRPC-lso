# LSO Changelog

This file records user-visible changes. The crate version remains `0.2.0`; changes after the
`0.2.0` tag are therefore listed under Unreleased.

## Unreleased

### Added

- `trajectory_deviations`: a continuous GS/lineup series computed from groove entry to touchdown
  (additive JSON field alongside `gate_deviations`), and used as a second, continuous source of
  amplitude for `PassGrade` next to the three point-in-time gates (`PROJECT-DERIVED`; see
  `docs/GRADING_REFERENCE.md`).
- Source-buffered `RecoveryTelemetry` acquisition with idempotent start/read/stop lifecycle,
  exclusive sequence cursors, full-batch processing, epoch/identity validation, explicit
  retention/capacity loss and invalid-unit diagnostics, plus unary rollback through
  `--position-source unary`.
- Optional global DCS-gRPC `X-API-Key` injection from `DCS_GRPC_API_KEY` (or the variable selected by
  `--api-key-env`), with sensitive metadata marking and no token logging.
- Independent `EventCorrelator` and `ReportPipeline` components, including additive event-stream
  status/outcome-confirmation evidence in schema-v3 JSON.
- A priority `PositionCollector`, `--positions-only` baseline mode, optional suspension of background
  detector transforms, `Skip` missed-tick scheduling and per-recovery acquisition percentiles.
- Build Git commit/dirty provenance, explicit DCS-gRPC client/server API-line compatibility, sliding
  telemetry health and additive primary/secondary causes (SQLite migration 6).
- Hook gRPC status codes and recent-evidence ring retention.

- Session/generation-aware supervision, two-second RPC/watchdog deadlines, monotonic freshness and
  explicit skew/gap diagnostics with conservative short extrapolation.
- Strict AV-8B/Tarawa and hook-aircraft/arrested-carrier pairing, slot/UCID human identity and
  session-scoped AI identity.
- Bracketed gate interpolation with `Valid`, `Late`, `Missing` and `Invalid` evidence, plus ordered
  raw `Land`/`RunwayTouch`/LQM evidence and raw hook observations.
- Separate estimated/DCS wire provenance, divergence, confidence, completeness, cause and grading
  version in structured reports and additive SQLite migrations.
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
- `--no-acmi` now also skips TacView serialization and ACMI-only metadata/unit RPCs instead of only
  suppressing the final file.

### Fixed

- Event-stream errors and clean closure no longer become positional `telemetry_gap`; existing gates
  remain intact while outcome availability is assessed separately.
- Plane/carrier respawns with a changed ID abort every stale same-name task within the current
  session/generation, preventing old-ID collectors from polling a current name.
- DCS/LQM wire parsing accepts only cables 1-4 and rejects zero, overflow and malformed suffixes.
- JSON, ACMI and rendered files use atomic create-if-absent publication on Windows and Unix; the JSON
  winner alone may continue to SQLite/render/Discord, and temporary files/directories are cleaned.
- Positions-only ignores missing or invalid Discord user configuration and does not start event,
  hook, ACMI, SQLite, dashboard, render, board or Discord components.
- Errors now retain useful IO paths, JSON line/column data and underlying JSON, SQLite, rendering,
  ACMI and Discord causes.
- Git dirty provenance now intentionally covers tracked files only, with tracked-path/index/HEAD
  rebuild triggers and deterministic parser tests; untracked files and `target/` are excluded.
- CI uses locked build/tests, all-target Clippy with warnings denied, rustfmt, and a pinned locked
  `cargo-audit` installation that consumes the existing `.cargo/audit.toml` ignore list unchanged.
- Detector suspension is scoped to the aircraft already being collected, so a second aircraft can
  still start a simultaneous recovery; positions-only no longer opens or migrates SQLite.
- Unconfirmed arrest no longer overwrites telemetry/gate causes; all independent unavailability
  causes are retained and SQLite completeness values now use the JSON snake-case vocabulary.
- Hook gRPC codes use documented snake-case names, baseline manifests are strict, Git dirty-state
  rebuild tracking covers every tracked file, and acquisition percentiles use bounded online
  histograms instead of unbounded vectors and end-of-pass sorting.
- Positions-only skips TacView update construction, and the direct Axum dependency is aligned with
  Tonic's 0.8 dependency line.
- Repaired the malformed Discord block left by the previous merge.
- Hook/event diagnostic truncation no longer changes positional completeness or masks
  `insufficient_gates`; hook history retains the newest 512 observations.
- Wire crossings are segmented at final entry, DCS/LQM wire evidence remains visible when the Rust
  estimate is unavailable, and each invalid gate displays its own bracket gap.
- Additional F-14 type aliases and distinct F-14A, F-14B, and F-14B(U) display names.
- Carrier-position EMA smoothing for final-approach geometry.
- Independent, timestamped hook sampling with configurable 2-4 Hz cadence, 250-300 ms timeout and
  a legacy-inline A/B switch; per-RPC and loop/tick latency percentiles; live telemetry health.
- Schema-v3 report evidence for hook freshness, component versions, grading availability and
  continuous wire-plane crossings; additive SQLite migration version 5.

### Changed

- CATOBAR grading now takes the worst GS/lineup amplitude across the continuous trajectory as well
  as the three gates, not the three gates alone; a significant excursion strictly between two gates
  (previously invisible to grading) can now downgrade the pass, and a dip below the Cut threshold
  anywhere at or inside the quarter-NM distance is caught, not only exactly at the gate crossing.
  This can only make the reported amplitude equal or worse than before, never better.
- Pilot-facing surfaces (Discord embed, PNG chart, SQLite/greenie-board log) now always show the
  DCS/LQM wire alone when it is available, instead of ever displaying it next to a diverging Rust
  geometric estimate (`Grading::pilot_facing_outcome`); the full JSON `outcome` field still records
  both wire values side by side for diagnostics.
- Full-pattern JSON `datums` are now subsampled to one in four outside the scoring-relevant window
  (before groove entry and beyond ¾ nm / 500 ft); the scoring zone itself, gate evidence and
  grading are unaffected, only the pattern/break portion of the report shrinks.
- DCS-gRPC client stubs are aligned with the sibling `0.10.0` server checkout while its commit is
  unpublished; release packaging must replace the local path with a reviewed immutable remote pin.
- The former automatic wire-3/groove-time `_OK_` rule is disabled. `_OK_` is reserved for an
  explicit official/manual grade; incomplete passes and touch-and-go outcomes receive no points.
- PNG rendering and SQLite work run outside latency-sensitive sampling tasks. Atomic artifact names
  include session/generation/unit identity and database inserts are idempotent.
- The HTTP dashboard now binds to `127.0.0.1` and returns HTTP 500 on database failure.
- The detection envelope now captures the full pattern: 200 m to 3.5 nm from the carrier and at or
  below 1,100 ft MSL, without nose-pointing or rear-hemisphere checks.
- Gate sampling is restricted to inbound crossings below 500 ft above the deck, and groove entry
  also requires lineup within 10 degrees.
- The earlier DCS-gRPC migration moved client stubs to the official sevenfifty777 fork release tag `v0.9.0`,
  resolved in `Cargo.lock` to commit `5bd6d6e42491c8697a5c5a95e80a2e689923bd3b`; `tonic` was
  updated to 0.13.
- Unit discovery safely ignores DCS units whose optional type field is absent.
- T-45 AoA brackets now use values derived from the VNAO T-45 display-electronics data instead of
  the former F/A-18C copy.
- F/A-18C CQ touch-and-go recognition now requires stable, timestamped pre-touch hook evidence;
  uncalibrated modules remain unknown. Technical unavailability is separate from pilot performance.

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
- Gate brackets use only their actual endpoint interval and can recover from an isolated degraded
  sample without crossing a real cut; frozen DCS timestamps now age and trip the watchdog.
- Pattern-only gaps no longer invalidate the scored groove, while gate/groove gaps remain blocking.
- Fragmented CATOBAR grooves render as separate labelled fragments instead of disappearing or being
  connected artificially. A late RunwayTouch transform can no longer manufacture a wire-4 crossing;
  an estimate now requires a continuous crossing correlated within 300 ms of the event.

### Security and dependencies

- The lockfile was refreshed for DCS-gRPC 0.9.0 and its gRPC stack.
- Known vulnerable transitive versions identified during the migration were updated. The recorded
  audit result and remaining allowed maintenance warning are documented in
  [DCS_GRPC_FORK_MIGRATION.md](docs/DCS_GRPC_FORK_MIGRATION.md).

## 0.2.0 - 2024-11-10

- Initial tagged `0.2.0` release. It provided live DCS-gRPC monitoring, carrier-relative recovery
  charts, wire estimation, compressed ACMI recording, Discord webhook delivery, and ACMI replay.

Earlier tags: `0.1.1`, `0.1.0`.
