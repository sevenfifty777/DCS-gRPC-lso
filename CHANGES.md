# LSO Changelog

This file records user-visible changes. The crate version is `0.4.0` (`Cargo.toml`); the
Unreleased section lists the changes made after the `0.2.0` tag.

## Unreleased

### Removed

- The loopback HTTP greenie board (`--web-port`, `/`, `/api/passes`) and `--web-expose-ucid`. The
  board is now the LSO page of the DCS Web Dashboard, which reads `<out-dir>/lso.db` directly,
  serves the trap-sheet PNGs, groups passes by pilot and never exposes UCIDs. `lso run` refuses
  the removed flags with a message pointing there, so an old service definition fails loudly
  instead of silently running without a board. `axum` is no longer a direct dependency (it
  remains in the tree only through `tonic`).

### Added

- Discord embed: arrested recoveries get a `Wire` field showing the DCS wire and the independent
  estimate side by side with an agreement marker (`✓` or `⚠ mismatch`) and the proof of the
  arrest (`DCS wire`, `hook transient`, `deck kinematics` with the hold time, `unconfirmed`), so a
  human-LSO trap without a DCS `WIRE#` and an estimator disagreement are both readable in the
  channel. The `Outcome` field keeps the primary wire as before.
- `lso.db` is opened with `PRAGMA journal_mode=WAL` and a 2 s `busy_timeout`, so an external
  read-only consumer (the DCS Web Dashboard LSO page, which reads the file directly) can query the
  board while a pass is being inserted without blocking the writer. SQLite keeps `lso.db-wal` and
  `lso.db-shm` next to the database while LSO runs.
- Commanded hook state for every validated module (F/A-18C and T-45 argument `25`, all F-14
  variants argument `1305`), latched from the stable pre-contact baseline so the arrestment
  excursion of the animated hook cannot flip it. Hook-up deck contacts on the F-14 and T-45 are now
  `T&G (CQ)` instead of `Bolter` (live corpus 2026-09-02/03: 7/7 correct).
- Kinematic arrest confirmation from carrier-relative deck displacement (slow for two seconds within
  eight seconds of contact). A real trap without a DCS `WIRE#` (human LSO, ignored DCS waveoff) is
  graded with `Arrested (wire unknown)` instead of `NC`; `arrest_evidence` names the proof
  (`dcs_wire`, `hook_transient`, `kinematic`, `unconfirmed`, `none`).
- DCS LSO comment parsing (`GRADE:` token, tolerant `WIRE#`, `WO`/`WO(AFU)`/`WOFD` calls): a
  complied-with DCS waveoff is the official `WO` grade (1.0 point); deck contact after a DCS waveoff
  is a cut pass `C`.
- One detection supervisor per generation (`carriers + idle planes` transform RPCs every 2 s, at
  most one recording per plane, nearest compatible carrier) and one shared mission event stream
  fanned out to recorders, replacing per-pair detector tasks and per-recovery event streams.
- Server-side snapshot diagnostics (`queue_wait_ms`, `lua_exec_ms`, `queue_depth`) per datum and in
  the metrics snapshot when DCS-gRPC 0.9.2 provides them.
- Raw hook draw argument written to LSO ACMI files as the `LSOHook` property, and offline replay
  (`lso file`, test fixtures) that reproduces the hook classifier and wire estimator. Fourteen live
  T-45/F-14B(U) recordings are regression fixtures under `tests/recordings/live_2026-09/`.
- `--ownship-hook-diagnostics` (opt-in `GetOwnshipHookState` sampling, off by default because it is
  always unavailable on a dedicated server).
- JSON schema 8 (`hook_state`, `arrest_evidence`, `arrest_kinematics`, `dcs_lso`,
  `hook_observation.baseline_*`, `raw_carrier_velocity`) and SQLite migration 6 (`arrest_evidence`,
  `hook_state`).

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
- Additional F-14 type aliases and distinct F-14A, F-14B, and F-14B(U) display names.
- Carrier-position EMA smoothing for final-approach geometry.
- Independent, timestamped hook sampling with configurable 2-4 Hz cadence, 250-300 ms timeout and
  a legacy-inline A/B switch; per-RPC and loop/tick latency percentiles; live telemetry health.
- JSON report schema raised to 7. Schema 3 added hook freshness, component versions, grading
  availability and continuous wire-plane crossings; schemas 4-7 added acquisition mode, snapshot
  sequence and RTT provenance, ownship hook diagnostics, hook `evidence_source`/`draw_argument`,
  and wire deflection/recovery/lag/crossings evidence. Additive SQLite migration version 5.

### Changed

- `approach_grade` is omitted from JSON when the pass is technically incomplete.
- Reconnection restarts from a fresh backoff after a generation that stayed healthy for a minute;
  forced atomic telemetry on a server without `GetRecoverySnapshot` ends the generation with one
  clear error instead of retrying every recovery.
- Poisoned mutexes are recovered instead of aborting unit discovery or the database; the web board is
  restarted with a bounded delay; transport warnings from `tonic`/`h2`/`hyper` are logged.
- Hook and ownship evidence rings use `VecDeque`; datum vectors are pre-sized.

- The former automatic wire-3/groove-time `_OK_` rule is disabled. `_OK_` is reserved for an
  explicit official/manual grade; incomplete passes and touch-and-go outcomes receive no points.
- PNG rendering and SQLite work run outside latency-sensitive sampling tasks. Atomic artifact names
  include session/generation/unit identity and database inserts are idempotent.
- The HTTP dashboard now binds to `127.0.0.1` and returns HTTP 500 on database failure.
- The detection envelope now captures the full pattern: 200 m to 3.5 nm from the carrier and at or
  below 1,100 ft MSL, without nose-pointing or rear-hemisphere checks.
- Gate sampling is restricted to inbound crossings below 500 ft above the deck, and groove entry
  also requires lineup within 10 degrees.
- The DCS-gRPC client stubs come from the sevenfifty777 fork. `Cargo.toml` currently pins them by
  local path (`../rust-server/stubs`, the fork's 0.9.1+ `hook-mechanization-api` branch that adds
  `RecoveryService.GetRecoverySnapshot` and `HookService.GetOwnshipHookState`) pending a tagged
  `v0.9.2` release; `tonic` was updated to 0.13.
- Unit discovery safely ignores DCS units whose optional type field is absent.
- T-45 AoA brackets now use values derived from the VNAO T-45 display-electronics data instead of
  the former F/A-18C copy.
- F/A-18C CQ touch-and-go recognition now requires stable, timestamped pre-touch hook evidence;
  uncalibrated modules remain unknown. Technical unavailability is separate from pilot performance.

### Fixed

- A pass whose aircraft (or carrier) disappears inside the post-touchdown window is graded from
  the evidence already recorded instead of being discarded. The recorder used to treat every
  `Crash`/`Dead`/`PlayerLeaveUnit`/`UnitLost` event as "nothing to grade", so a pilot who left
  the slot right after the trap lost the pass entirely (2026-09-04 Foothold session: T-45
  `WIRE# 2` received, player left the unit 7.5 s after `Land`, no JSON, no DB row, no Discord
  post). The despawn is recorded in `events[]` as `despawn_after_touchdown`. Before any accepted
  deck contact the recording is still dropped.
- `lso file` prints one summary line per replayed pass (outcome, grade, points, hook state,
  arrest evidence, DCS and estimated wire, completeness, PNG path) so an offline regrade can be
  compared with the live JSON report.
- Telemetry outages are no longer hidden: an RPC failure keeps the last sample time, so the next
  sample's gap, `max_sample_gap_ms` and health reflect the outage (campaign A reports claimed
  `health: green` with 900-1080 ms gate brackets).
- Wire-crossing and deck evidence is cleared when the aircraft flies back out past 3/4 nm, so a
  bolter followed by a trap can be attributed to its wire.
- Hook samples drained on `Land`/`RunwayTouch` are classified against the landing time instead of
  the drain order; the final-window flag uses the current along-deck distance.
- The mission event stream no longer carries the 2 s unary `grpc-timeout` header.

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
  an estimate now requires a complete hook-deflection transient (>=0.8 -> <=0.7 -> >=0.8) occurring
  <=200 ms after the last finite cable-plane crossing. DCS `WIRE#` remains authoritative; the
  estimate is used only when DCS supplies no wire.

### Security and dependencies

- The lockfile was refreshed for DCS-gRPC 0.9.0 and its gRPC stack.
- Known vulnerable transitive versions identified during the migration were updated. The recorded
  audit result and remaining allowed maintenance warning are documented in
  [DCS_GRPC_FORK_MIGRATION.md](docs/DCS_GRPC_FORK_MIGRATION.md).

## 0.2.0 - 2024-11-10

- Initial tagged `0.2.0` release. It provided live DCS-gRPC monitoring, carrier-relative recovery
  charts, wire estimation, compressed ACMI recording, Discord webhook delivery, and ACMI replay.

Earlier tags: `0.1.1`, `0.1.0`.
