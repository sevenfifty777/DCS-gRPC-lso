# DCS-gRPC LSO — Full-state analysis and improvement/fix plan

Date: 2026-09-03. LSO HEAD `c5370d8` (branch `snapshot`, crate 0.3.0, 119/119 tests pass locally).
rust-server HEAD `bfb928b` (branch `hook-mechanization-api`, 0.9.1 + unreleased `GetOwnshipHookState`).

## Context

The LSO app grades DCS carrier recoveries (CATOBAR: F/A-18C, F-14A/B/B(U), T-45C on Nimitz-class/Forrestal;
V/STOL: AV-8B on Tarawa) from DCS-gRPC telemetry, writes JSON/PNG/ACMI/SQLite, posts to Discord, serves a
web greenie board. Over August–September the team ran four analysis rounds (GPT-5.6 analysis + todo
arbitration, recovery-snapshot analysis, snapshot test analysis, wire/hook post-test analysis) and four live
test campaigns, and forked DCS-gRPC to add `RecoveryService.GetRecoverySnapshot` and
`HookService.GetOwnshipHookState`. The last Codex session ended on a usage limit before it could write the
"what next" document. This plan is that document: verified current state, all open issues found by this
review, and a phased plan.

Assumptions (stated, not confirmed by the user): production runs on a **dedicated server** (`DCS_server.exe`,
confirmed by `dcs.log` in campaigns C/D); the 10 Hz active cadence is a fixed requirement; DCS `WIRE#` stays
authoritative and the estimator is the human-LSO fallback; correctness fixes rank above throughput work.

---

## 1. Verified current state

### 1.1 What the app does today (working, verified in code + campaign D JSON)
- Discovery: `GetGroups` → `GetUnits` per group → `GetDescriptor` per ship; strict pairing matrix
  (`data.rs:410-417`); Birth events add later units; UCID/slot identity (`run.rs:614-657`).
- Detection: one task per (plane, carrier) pair, 2 `GetTransform` every 2 s, envelope 3.5 nm / 1100 ft / >200 m.
- Recording: 10 Hz loop; **atomic** `GetRecoverySnapshot` (carrier + aircraft + hook draw arg, one Lua
  callback, one timestamp, echoed sequence) with legacy 2×`GetTransform` fallback on `UNIMPLEMENTED`.
  Telemetry aligner (100/300/1000 ms skew/gap contract), 2 s watchdog, per-recovery event stream.
- Grading: bracketed/interpolated gates at ¾/½/¼ nm (Valid/Late/Missing/Invalid), NAVAIR-style
  `_OK_/OK/(OK)/--/C/B/WO?/NC`, points; V/STOL averaged gates + spot 7.5 bonus.
- Wire: DCS `WIRE#` authoritative; estimator = completed hook-deflection transient (≥0.8 → ≤0.7 → ≥0.8)
  correlated ≤200 ms after the last finite cable-plane crossing (`track.rs:1257-1445`).
- Outputs: schema-7 JSON, side/top trapsheet + pattern PNG, compressed ACMI, SQLite (migrations 1–5,
  idempotent insert), Discord embed, `/api/passes` on 127.0.0.1, session greenie board, 10 s metrics log.
- Offline: `lso file <acmi>` regenerates the approach PNG only (no hook data in ACMI → no estimator).

### 1.2 Campaign scorecard (from `trap_records/` JSON, verified this session)
| Campaign | Setup | Telemetry | Outcome |
|---|---|---|---|
| A (09-01) loaded mission, T-45, atomic 250 ms | 2 recoveries | ~5.05 Hz effective, 27.9% missing sequences in runs of 3, gaps 900–1100 ms, RTT p95 143 ms | 1/6 gates valid → both `insufficient_gates` |
| B (09-02) simple mission, T-45 | 4 recoveries | 10.00 Hz, 5629/5629 ok, RTT p95 33 ms, max gap 152 ms | 12/12 gates valid; pass 4 real trap → `NC unconfirmed_arrest` (DCS LSO waveoff withheld WIRE#) |
| C (09-02) hook detection, T-45 ×5, F-14BU ×4 | 9 | clean (gaps <147 ms) | `GetOwnshipHookState` 0 observed / 6164 unavailable; 4 hook-up passes graded **Bolter** instead of T&G |
| D (09-03) wire estimator, F-14BU ×3, T-45 ×2 | 5 | clean (max gap 146 ms, skew 0) | estimator **2/2 wires correct (68.5 / 94.6 ms lag), 3/3 no-estimate on hook-up, 0 false**; hook-up still Bolter; `interpreted_state: unknown` ×5 |

Conclusion: atomic acquisition and the new wire estimator work. Two things are still wrong in grading
(hook-up classification, arrest confirmation without WIRE#), one server RPC is dead on a dedicated server,
and telemetry under mission load is unexplained and unmeasured at the queue level.

---

## 2. Findings (this review, file:line verified)

Severity: **S1** wrong grade/lost pass, **S2** load/robustness, **S3** hygiene.

### A. Grading / hook / wire correctness
- **A1 (S1)** Hook polarity interpreted only for the Hornet: `calibrated_hook_state` returns `Unknown` unless
  `plane_info.name == "F/A-18C Hornet"` (`track.rs:1511-1514`, string compare). F-14 (arg 1305) and T-45
  (arg 25) hook-up passes → `Bolter` (campaign C: 4/4, D: 3/3). Test `uncalibrated_f14_hook_values_remain_unknown`
  (`track.rs:2541`) encodes the defect. Note the physical excursion on a real trap (arg drops into the "up"
  band 0.5–1.4 s **before** `RunwayTouch` and returns 1.7–6.5 s after) means a naive "latest sample" rule is
  unsafe; the rule must latch the pre-contact baseline.
- **A2 (S1)** A physical trap without `WIRE#` (DCS LSO waveoff ignored, human LSO, no LQM) ends as
  `NC / unconfirmed_arrest` (`track.rs:1201-1214`). No kinematic arrest confirmation exists although the data
  is unambiguous (post-touch carrier-relative speed → <5 m/s in 2 s vs ~48 m/s on bolters, campaign B).
- **A3 (S1)** `wire_crossings` is never reset while gates/groove are reset when x > ¾ nm
  (`track.rs:848-854` vs `1304-1307`): bolter-then-trap in one recording keeps first-pass crossings, second
  pass cannot re-record a wire → estimate `insufficient`. Also `previous_wire_plane` *is* reset (`1263-1266`).
- **A4 (S1)** Hook samples drained **before** `landed()` in Land/RunwayTouch arms (`record_recovery.rs:739→746`,
  `832→839`) so post-touch samples carry `before_touchdown: true`; `in_final_window` uses a stale `previous_x`
  (`track.rs:1459`, only updated at `983`, frozen after `x ≤ 0`).
- **A5 (S1)** Aligner reset on RPC error hides outages: `telemetry_aligner.reset()` (`record_recovery.rs:479`)
  clears `previous_sample_at`, so the next sample reports `sample_gap_ms = 0`. Campaign A JSON says
  `health: green`, max scoring gap 170–237 ms while gate brackets were 900–1080 ms. Renderer prints the
  under-estimated gap (`draw.rs` ~1120).
- **A6 (S2)** DCS LQM `GRADE:WO` / `OWO` tokens are not parsed: DCS-ordered waveoff becomes `Bolter` (campaign
  D #2). Only `WIRE#` is parsed (`track.rs:1750`).
- **A7 (S2)** Deflection window is symmetric ±2 s (`track.rs:1410`), lag ceiling 200 ms vs legacy 4 Hz sampler
  (250 ms period) and batch-attributed hook timestamps (`record_recovery.rs:610-615`) — only atomic mode gives
  the estimator usable timing. Legacy mode should be explicitly "no estimate".
- **A8 (S2)** Bolter detection = distance from min point > 150 m (`track.rs:700`), not kinematics; marginal
  after a trap (carrier moves ~77 m in the 10 s post-landing window).
- **A9 (S2)** Carrier EMA α=0.15 (`track.rs:73,637-647`) feeds scoring geometry (gates, groove, landing pos),
  ~0.6 s lag; flagged P0 on 09-01, still open. JSON keeps `raw/corrected/filtered_carrier_position`, so an
  alternative (step-detect + velocity dead-reckoning) can be evaluated offline.
- **A10 (S2)** `approach_grade` serialised even when `pass_grade` forced to `Incomplete` (`track.rs:1215-1218`,
  `record_recovery.rs:1041`); `_groove_time_secs` unused (`grading.rs:175`); `parse_dcs_wire` fragile to
  `WIRE #4` spacing; AoA `NaN` when stationary (`transform.rs:39-48`) flows to JSON/PNG.
- **A11 (S2)** Unknown CATOBAR module without a draw-argument mapping (`record_recovery.rs:178-185`) can never
  be graded (no hook → `UnconfirmedArrest`) even with a DCS wire absent; missing hook ≠ hook up.

### B. Telemetry and performance under load
- **B1 (S2)** Detection is O(planes × carriers): one tokio task and 2 unary RPCs / 2 s per pair
  (`detect_recovery_attempt.rs:19-29`); 40 planes × 2 carriers = 80 RPC/s idle, all through the single
  mission IPC queue (1024 slots, 18 calls per 30 ms tick). Carrier transform fetched once per pair.
- **B2 (S2)** One `MissionService.StreamEvents` per active recovery (`record_recovery.rs:382`) plus the global
  one; every event serialised N+1 times.
- **B3 (S2)** `OwnshipHookSampler` runs 2–4 Hz per CATOBAR recovery (`record_recovery.rs:399-401`) and is 100%
  `unavailable` on a dedicated server (`LoGetMechInfo` is ownship-only): pure cost, diagnostic-only role.
- **B4 (S2)** Campaign A loss pattern (runs of exactly 3 missing sequences at 250 ms timeout ≈ one ~1 s stall
  each, ~200 stalls in ~280 s) points at mission-scheduler stalls or queue congestion, but the snapshot
  response carries no queue-wait / Lua-exec timing, so the cause cannot be attributed. Timeout ceiling 299 ms
  conflates RPC deadline with gate freshness (`run.rs:76`).
- **B5 (S3)** Hot-path allocations: 4 `Transform` clones per tick in `align` (`telemetry.rs:118-119,154-155`),
  `calibrated_hook_state()` collects a Vec on **every** hook sample (`track.rs:1504,1515-1524`),
  `completed_hook_deflection_near` O(n²) over 512 (`1385-1443`), `wire_estimate_at` clones crossings on
  every return (`1332,1354,1366,1380`), `Vec::remove(0)` ring buffers (`1491`, `ownship_hook.rs:80`),
  `datums`/`pattern_datums` without capacity, ACMI held fully in memory. Benchmark showed replay CPU is
  already tiny; these matter only for the live loop's tick jitter and are low priority.
- **B6 (S3)** `tracing` filter drops all `tonic/h2/hyper` output (`main.rs:58-59`) — transport failures
  invisible at `-vv`.

### C. Robustness
- **C1 (S2)** `.expect("… mutex poisoned")` ×8 in the Birth handler (`run.rs:270-496`) and DB (`db.rs:207,266`):
  one panic silently ends unit discovery for the generation / bricks the DB and web board.
- **C2 (S2)** Every error is `transient` (`run.rs:147`); `atomic` mode with `UNIMPLEMENTED` → detector logs
  every 2 s forever; backoff never resets after a long healthy run (`run.rs:101-107`).
- **C3 (S2)** Birth for a known unit aborts the pair task mid-recording (`run.rs:379-388`) → pass lost with
  no output; no per-aircraft recovery ownership (two co-located carriers can both record one plane).
- **C4 (S3)** `stream_events` sends a 2 s `grpc-timeout` header on a server-streaming call (`mission_client.rs:51`);
  harmless with tonic's server today, wrong by contract. Web server task never restarted on error
  (`run.rs:122-129`). `now_local()` silently falls back to UTC for filenames only (`record_recovery.rs:280`).
  `/api/passes` returns UCIDs (`db.rs:74`, `web.rs`).

### D. rust-server fork
- **D1 (S1)** `GetOwnshipHookState` cannot work on a dedicated server (0/6164 observed). Keep the RPC, but the
  LSO must stop depending on it; proto/README should say "client-side DCS only".
- **D2 (S2)** `GRPC.errorInternal` is undefined in `grpc.lua` but called by `spot.lua:26,56,86` and
  `unit.lua:413` → Lua error instead of a structured gRPC error.
- **D3 (S2)** All method files load in both Lua envs (`grpc.lua:152-169`); `recovery.lua` uses `Unit`/`timer`
  (nil in hook env), `hook.lua` captures `local DCS = DCS` at load (nil in mission env) → a routing mistake
  returns a *successful* `UNAVAILABLE`. Off-by-one `i >= callsPerTick` vs `i > callsPerTick` (`grpc.lua:217/261`).
- **D4 (S2)** `stream.rs:32-34` poll rate in whole seconds, `poll_rate = 0` accepted → `interval(ZERO)`; one
  IPC request per unit per tick via `try_join_all` (unbounded fan-out) — can starve the recovery queue.
- **D5 (S2)** `recovery.lua:33-42` `pcall` discards the failure reason; no queue-wait/exec timing in the response
  although the ipc fork records them.
- **D6 (S3)** `ipc/` directory is tracked but not a workspace member; build uses git rev `55f0bf5` of the
  `sevenfifty777/dcs-module-ipc` fork; its tests never run. `lua_files.rs` 0-byte file. `#![allow(dead_code)]`
  crate-wide. `log4rs::init_config(...).unwrap()` and `SERVER.read().unwrap()` in `src/lib.rs` (panic → poisoned
  RwLock → module dead until DCS restart).
- **D7 (S2)** Release/versioning: two local packages both claim 0.9.1 (`Releases/DCS-gRPC-0.9.1` without
  `getOwnshipHookState`, `…-hook-mechanization-api` with it); `src/lib.rs:88-97` version guard cannot tell
  them apart. README/STATUS don't mention `RecoveryService` or `GetOwnshipHookState`. 12 `cargo audit`
  vulnerabilities deferred and CI has no audit step.

### E. Repo, docs, CI
- **E1 (S2)** `Cargo.toml:38-41` `stubs = { path = "../rust-server/stubs" }` → **GitHub CI cannot build**;
  README/CHANGES/LIVE_VALIDATION still say tag `v0.9.0` / commit `5bd6d6e`.
- **E2 (S3)** Version/schema drift: CHANGES.md says crate "remains 0.2.0" (is 0.3.0), "schema-v3"; README says
  "schema-v4" (is 7); README/CONTRIBUTING link `docs/DCS-gRPC-0.9.0/` (renamed to 0.9.1);
  `DCS_GRPC_FORK_MIGRATION.md` says "0 vulnerabilities" vs 12; CHANGES says wire lag 300 ms (code 200 ms);
  `RELIABILITY_ARCHITECTURE.md` describes pre-atomic hook sampling and "argument 25" for all aircraft;
  `todoanalyse` decision J ("estimate primary") reversed but never annotated; `GRADING_PR_REVIEW.md`,
  dead links to `analysis2.md`/`analysis_results.md`.
- **E3 (S2)** Test corpus cannot exercise the estimator: ACMI fixtures carry no hook data
  (`commands/file.rs:358` passes `None`), so all five `tests.rs` expectations have `cable_estimated: None`.
  The 14 real recoveries of campaigns C/D (ACMI + JSON hook timelines + DCS labels) are not used by any test.
  `docs/wire-estimation-hook-post-test-analyze.md` is untracked. `docs/DCS_gRPC_analyse/` (15 GB, ignored)
  holds full repo copies incl. `target/`.
- **E4 (S3)** `graphify-out/` graph is dated 08-28, predates all telemetry/hook/wire work.

---

## 3. Plan

Phases are ordered by value/risk; each ends with a green test suite and an updated CHANGES.md.

### Phase 0 — Make the repo truthful and buildable (½ day)
1. Commit `docs/wire-estimation-hook-post-test-analyze.md`; add this document as `docs/STATE_AND_PLAN_2026-09-03.md`.
2. **Stubs dependency**: tag the rust-server branch (`v0.9.2-rc1`, includes RecoveryService + GetOwnshipHookState),
   switch `Cargo.toml` to `git = …, tag = …`, keep a commented path line for local dev. CI builds again.
3. Docs sweep (E2): CHANGES.md header/version/schema/lag, README output table + doc paths, LIVE_VALIDATION
   manifest, DCS_GRPC_FORK_MIGRATION audit line, RELIABILITY_ARCHITECTURE atomic-mode + per-aircraft argument,
   annotate `todoanalyse` J as reversed with the evidence pointer, delete `GRADING_PR_REVIEW.md` or mark obsolete.
4. Add `cargo audit` (allow-list file) to CI as non-blocking, listing the 12 known advisories.

### Phase 1 — Grading correctness (S1 items; 2–3 days)
Files: `src/track.rs`, `src/tasks/record_recovery.rs`, `src/telemetry.rs`, `src/grading.rs`, `src/data.rs`.

1. **Hook-state classifier by temporal signature (A1)** — replace `calibrated_hook_state` with
   `commanded_hook_state()`:
   - per-aircraft polarity table in `data.rs` (`AirplaneInfo.hook_argument: Option<HookArgument { id, down_min: 0.8, up_max: 0.2 }>`),
     covering F/A-18C (25), T-45 (25), F-14 all variants (1305); remove the `plane_info.name` string compare
     and the duplicate mapping in `record_recovery.rs:178-185`.
   - baseline window = successful samples with `in_groove` and `associated_time_dcs ≤ min(touchdown, first deflection) − 1.5 s`
     (before the contact excursion; campaign C/D show the excursion starts ≤1.4 s before RunwayTouch);
     require ≥5 samples spanning ≥0.5 s with all in one band → `Up`/`Down`, else `Unknown`.
   - keep `completed_hook_deflection_near` as the arrest transient (unchanged, validated in D).
   - outcomes: `Down` + no arrest → `Bolter`; `Up` + deck crossing → `TouchAndGo` (`T&G (CQ)`); `Unknown` → today's
     behaviour. Persist `hook_observation.baseline_value/baseline_window/polarity_source`.
2. **Kinematic arrest confirmation (A2, A8)** — new `ArrestKinematics` in `track.rs` evaluated post-touchdown
   from `datums` (carrier-relative speed from x/y deltas; the JSON already stores `touchdown_horizontal_speed_mps`):
   arrested if relative speed ≤ 6 m/s within 5 s of touchdown, held ≥ 2 s, no gap > 300 ms in that window,
   and x within the deck run-out band. Use it as: (a) arrest confirmation → `Grading::Recovered { cable: None,
   cable_estimated: estimate }` with `completeness: Complete`, new `arrest_evidence: "dcs_wire" | "hook_transient" | "kinematic"`,
   outcome text `Arrested (wire unknown)` when no wire; (b) bolter = departure (relative speed stays > 25 m/s and
   x < 0 past deck end) instead of the 150 m rule for arrested carriers (keep 150 m for V/STOL).
   Thresholds are PROJECT-DERIVED from campaign B; store them as named constants and validate on the 14
   real recoveries (Phase 4 harness).
3. **Multi-approach state (A3)** — clear `wire_crossings` and `previous_wire_plane` in the same outbound reset
   block (`track.rs:848-854`); allow re-crossing after reset (drop the `any(wire == wire)` guard in favour of
   "one crossing per wire per groove entry").
4. **Event ordering (A4)** — in Land/RunwayTouch arms call `datums.landed()` first, then drain hook samples with
   `before_touchdown` derived from `associated_time_dcs < landing_time` (not from drain order); compute
   `in_final_window` from the sample's own x (pass current x into `observe_hook_sample`, or store `last_x`
   updated before the early `grading.is_some()` return).
5. **Honest gap accounting (A5)** — on RPC failure do not clear `previous_sample_at`; add
   `TelemetryAligner::invalidate_history()` that resets only extrapolation history. `max_sample_gap_ms` and
   `health` then reflect outages; renderer shows the gate bracket gap when a gate is invalid.
6. **LQM tokens (A6)** — `parse_dcs_grading()` returning `{ wire: Option<u8>, waveoff: bool, ... }`; tolerant of
   `WIRE #n`/`WIRE#n`; DCS `WO`/`OWO` → `Grading::WaveoffDcs` (new variant, label `WO`, points per existing table)
   unless a trap is confirmed (then `Recovered` + note `dcs_lso_waveoff_ignored`).
7. **Estimator hygiene (A7, A10, A11)** — deflection window `[-0.5 s, +2 s]` around touchdown; in legacy
   acquisition mark `wire_estimation.reason = "legacy_sampling_not_correlatable"`; clear `approach_grade` in JSON
   when `pass_grade == Incomplete` (or rename to `approach_grade_raw` and document); `aoa: Option<f64>` (None
   when speed < 1 m/s); when an arrested aircraft has no hook argument mapping, allow the kinematic path to
   confirm the arrest.
8. **Carrier smoothing (A9)** — offline experiment first (script over `trap_records/*/*.json` comparing
   `raw`/`filtered` carrier positions and gate x values): implement step-detection + velocity dead-reckoning
   (`pos = last_step_pos + velocity × (t − t_step)`), keep EMA behind a constant for A/B, adopt if gate-distance
   error and sawtooth both improve. Do this last in Phase 1; it changes grades.
9. Tests: rewrite `uncalibrated_f14_hook_values_remain_unknown` into per-aircraft polarity tests; add
   contact-excursion test (baseline Down, transient to 0.0 at −1.0 s … +2.4 s → still Down); bolter-then-trap
   crossing reset; gap accounting after a simulated RPC failure; LQM parser cases; kinematic arrest on
   synthetic run-out. Update `docs/GRADING_REFERENCE.md`, `DATA_CONTRACTS.md` (schema 8: `arrest_evidence`,
   `hook_observation.baseline_*`, `dcs_lso_waveoff`), SQLite migration 6 (`arrest_evidence`, `hook_state`).

### Phase 2 — Load reduction and robustness on the LSO side (2 days)
Files: `src/commands/run.rs`, `src/tasks/detect_recovery_attempt.rs`, `src/tasks/record_recovery.rs`,
`src/ownship_hook.rs`, `src/telemetry.rs`, `src/track.rs`, `src/db.rs`.

1. **Single detection supervisor (B1)** — replace per-pair tasks with one `detect_supervisor` task: every 2 s
   fetch each carrier transform once and each candidate plane once (`N + M` RPCs, batched with a small
   concurrency cap of 4), evaluate all compatible pairs in memory, and spawn `record_recovery` for a plane at
   most once at a time (per-aircraft ownership: nearest compatible carrier wins; C3). Keep `is_recovery_attempt`
   unchanged. Planes/carriers maps become `RwLock<HashMap>`; Birth handler only inserts.
   Optional later step: use `StreamUnits(poll_rate=1)` as the prefilter once D4 is fixed server-side.
2. **Shared event stream (B2)** — one `StreamEvents` in `run()` fanned out via `tokio::sync::broadcast`
   (capacity 256, lag counted in metrics); recorders subscribe and filter by unit ids. Use the same stream
   for the Birth handler. Remove the per-request 2 s timeout on streaming calls (C4).
3. **Ownship hook sampler opt-in (B3, D1)** — gate `OwnshipHookSampler` behind `--ownship-hook-diagnostics`
   (default off); when on and the first 8 polls are all `Unavailable`, stop the sampler and log once.
4. **Failure handling (C1, C2)** — replace `.expect("… poisoned")` with `lock().unwrap_or_else(PoisonError::into_inner)`
   (or `parking_lot`); classify errors: `InvalidArgument/Unimplemented` under forced modes and config-file
   errors are fatal (exit 2 with message), transport errors transient; recreate `ExponentialBackoff` per
   connection attempt after ≥60 s healthy uptime; a Birth for a unit currently being recorded must **not**
   abort the recorder (mark the pair "respawn pending", restart detection after the recording ends).
5. **Hot-path trims (B5)** — `align()` takes `&ObservedTransform` and clones once into the sample;
   `VecDeque` for hook/ownship/event timelines; `Vec::with_capacity(4096)` for datums; cache
   `commanded_hook_state` result and recompute only when a new final-window sample arrives; make
   `wire_estimate_at` return crossings by reference (`Cow`/`&[…]`) and guard the debug log with
   `tracing::enabled!`. Stream ACMI records to a `tokio::fs::File` behind a `BufWriter` instead of a full
   in-memory `Cursor` (write-atomic still via temp+rename).
6. **Observability (B6)** — allow `tonic`/`h2` at `warn` in the filter; add per-recovery summary log line
   (rate, missing sequences, max gap, timeouts) at recording end; web task restart with backoff; strip
   `pilot_ucid` from `/api/passes` unless `--web-expose-ucid`.

### Phase 3 — rust-server fork (2 days + release)
Files: `lua/DCS-gRPC/grpc.lua`, `lua/DCS-gRPC/methods/{recovery,hook,unit,spot}.lua`, `src/rpc/recovery.rs`,
`src/stream.rs`, `src/lib.rs`, `protos/dcs/recovery/v0/recovery.proto`, `stubs/`, `Cargo.toml`, README/STATUS/CHANGELOG.

1. **Snapshot diagnostics (B4, D5)** — extend `GetRecoverySnapshotResponse` with
   `queue_wait_ms`, `lua_exec_ms`, `dequeued_model_time`, `queue_depth` (the ipc fork already measures queue wait
   and correlation ids; expose them through `MissionRpc::request` → response metadata or fields). Keep the
   `pcall` failure text in a `draw_argument.detail` string. LSO persists them per datum and the 10 s metrics
   snapshot reports p95 of each. This is what will tell campaign-A stalls apart from queue congestion.
2. **Lua fixes (D2, D3)** — define `GRPC.errorInternal`; register methods per environment (`if isMissionEnv then
   dofile(recovery.lua) … else dofile(hook-only) end`) or guard each method with an env check that returns
   `errorUnimplemented`; fix the `>=`/`>` off-by-one; luacheck already runs in CI.
3. **StreamUnits hardening (D4)** — `poll_rate_ms` (proto-compatible: keep `poll_rate` seconds, add optional
   `poll_rate_ms`), reject 0, cap in-flight `GetTransform` per stream (semaphore 8), reuse one state for
   concurrent streams (existing TODO). Document that it is a discovery/prefilter API.
4. **Optional: push-mode telemetry (design gate, measure first)** — `RecoveryService.StreamRecoverySnapshots`
   where Lua samples both units in its own `timer.scheduleFunction` at 100 ms model time and pushes frames;
   removes per-request queue wait and client tick jitter, samples on the DCS clock. Implement only if Phase 3.1
   diagnostics show queue wait (not scheduler stalls) dominates campaign-A gaps.
5. **Hygiene (D6, D7)** — make `ipc/` a workspace member with `dcs-module-ipc = { path = "ipc" }` (or delete the
   directory and keep the git rev); remove `lua_files.rs`; replace `.unwrap()` on `log4rs`/`SERVER` locks with
   error returns; add `cargo audit` job; bump `Cargo.toml`/`version.lua` to `0.9.2`, regenerate `Releases/`
   from `build_release.ps1` only, delete the hand-named `…-hook-mechanization-api` folder, mention both custom
   services in README/STATUS/CHANGELOG with the "client-side only" caveat for `GetOwnshipHookState`.
6. Tag `v0.9.2` and pin the LSO to it (closes Phase 0.2 loop).

### Phase 4 — Test harness on real data (1–2 days)
Files: `src/commands/file.rs`, `src/tests.rs`, `tests/recordings/`, `src/tasks/record_recovery.rs` (ACMI writer).

1. **Hook data in ACMI** — write the raw hook value as a Tacview custom property (`Property::Unknown("LSOHook", value)`)
   on each plane frame, and `LandingQualityMark`/events already go as ACMI events; `extract_recoveries` reads it
   back and feeds `observe_hook_sample`, so `lso file` and the fixture tests exercise the estimator and the
   classifier. Keep legacy ACMI (no property) working (`hook_state = None`).
2. **Fixtures** — add the 14 campaign C/D recordings (ACMI + JSON sidecar for DCS label, pilot ground truth,
   expected wire/outcome) under `tests/recordings/live_2026-09/`; one test per file asserting
   `(grading, wire_dcs, wire_estimated, outcome)`; the three hook-up passes must assert `T&G (CQ)` after Phase 1.
   For campaign-D ACMIs (recorded before 4.1) build the hook timeline from the JSON sidecar.
3. **Loop-level tests** — simulated RPC failure → gap accounting; Birth during recording → no abort;
   supervisor produces `N + M` RPCs per tick (count via `RUNTIME_METRICS`).
4. **CI** — builds on the git tag; `cargo test --workspace`; clippy `-D warnings`; fmt; `cargo audit` allow-list.

### Phase 5 — Live validation and acceptance (server time)
1. Re-run campaign A's loaded mission with Phase 2 + 3.1 in place, `-v`, keep `lso.log`/`gRPC.log`/`dcs.log`;
   report snapshot `queue_wait_ms` vs `lua_exec_ms` distributions. Decide Phase 3.4 from that data.
2. Matrix from `BENCHMARK_PROTOCOL.md` (Hornet/CVN, AV-8B/Tarawa, simultaneous, 40 players/2 ships,
   3 carriers, induced gRPC delay/reconnect/mission rotation), ≥10 min each, ≥10 recoveries per cell.
3. Acceptance gates (existing, unchanged): no groove gap > 300 ms, snapshot RTT p95 ≤ 150 ms / p99 ≤ 200 ms,
   CPU/RAM ≤ 50%, zero duplicate outputs; plus new: hook-up CQ passes → `T&G (CQ)` 100%, WIRE#-less real
   traps → `Arrested (wire unknown)` or estimated wire, never `NC`.

---

## 4. Verification
- Unit: `cargo test --locked` (expect 119 + new tests), `cargo clippy --locked --all-targets -- -D warnings`,
  `cargo fmt --check`; rust-server: `cargo test --workspace`, `luacheck ./lua`, `protolint`.
- Offline replay: `lso file` on the 14 live ACMIs → wires/outcomes match DCS labels and pilot ground truth
  (table in §1.2); PNGs regenerated to `target/test-charts/` for visual check.
- Live: Phase 5 runs; JSON `telemetry_quality`, `arrest_evidence`, `hook_observation.baseline_*`,
  snapshot timing fields; metrics log `rpc_per_second` must drop from `2·N·M/2 + …` to `(N+M)/2 + 10/recovery`.

## 5. Decisions left to the user (defaults applied if unanswered)
1. Deployment is dedicated-server only → `GetOwnshipHookState` becomes opt-in diagnostics (default: yes).
2. A kinematically confirmed trap without WIRE# earns a normal grade with `wire: unknown` (default: yes,
   consistent with decision E "three valid gates" and the human-LSO requirement).
3. Carrier EMA replacement (Phase 1.8) changes historical grade comparability → do it, bump `grading_version`
   to `project-derived-v2` (default: yes).
4. Push-mode streaming RPC (Phase 3.4) only after Phase 3.1 measurements (default: measure first).
