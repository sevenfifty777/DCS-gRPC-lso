# Follow-up handover: what to do when the next live test data arrives

Written 2026-09-03 for the agent that picks this project up in a new conversation. Read this file
first, then `as-rust-dcs-scripting-gentle-cat.md` (the full analysis and plan) and
`plan-implentation.md` (what was implemented and what was left). Do not re-derive the analysis.

## 1. The goal, in one paragraph

The LSO must grade every carrier recovery on a **dedicated DCS server** (no ownship cockpit) with
**human LSOs possible**, which means no DCS `WIRE#` may exist. Grading therefore stands on three
layers: DCS wire when present (authoritative) → independent hook-transient wire estimate (fallback)
→ kinematic arrest confirmation (proves the trap, never names the wire). Hook-up qualification
passes must read `T&G (CQ)`, never `Bolter`. Telemetry must sustain 10 Hz under mission load, and
every grade must be reproducible offline from the recording. Anything that weakens these is a
regression.

Non-negotiables already decided by the user (do not reopen):
- DCS `WIRE#` stays authoritative; the estimate is only a fallback (decision J reversed 2026-09-02).
- Three valid gates are mandatory for approach points (decision E); `WO` from the DCS LSO is the
  only outcome exempt from that rule.
- A kinematically confirmed trap without a wire is a normal graded pass with `wire: unknown`.
- `GetOwnshipHookState` is diagnostics only (`--ownship-hook-diagnostics`), never a grading input.
- Work on a feature branch created from the base branch; never commit or push unless asked.

## 2. Where things stand

| Repo | Branch | Head at handover | State |
|---|---|---|---|
| `DCS-gRPC-lso` | `feature/post-analysis-plan-20260903` (from `snapshot`) | `dffdff5` | 144 tests, clippy `-D warnings` and fmt clean |
| `rust-server` (DCS-gRPC fork) | `feature/post-analysis-plan-20260903` (from `hook-mechanization-api`) | `9c19ec9` | 0.9.2 unreleased, untagged; 29 tests, clippy/fmt clean |

Implemented (details in `plan-implentation.md` and `CHANGES.md` Unreleased):
- Phase 1 grading: per-aircraft hook argument table (`src/data.rs`), commanded hook state latched
  from the pre-contact baseline, kinematic arrest confirmation, DCS LSO comment parser
  (`GRADE:`, tolerant `WIRE#`, `WO` calls → `WO` grade, or `C` after an ignored waveoff), outbound
  reset of wire evidence, event/drain ordering, honest gap accounting, JSON schema 8, SQLite
  migration 6.
- Phase 2 load/robustness: one detection supervisor per generation
  (`src/tasks/detect_recovery_attempt.rs`), one broadcast mission event stream, opt-in ownship
  sampler, poisoned-mutex recovery, backoff reset after 60 s healthy, web restart, UCID stripping.
- Phase 3 fork: `GetRecoverySnapshot` returns `queue_wait_ms`, `lua_exec_ms`, `queue_depth`,
  `dequeued_model_time`; `DrawArgumentObservation.detail`; `GRPC.errorInternal`; per-environment
  method loading; `StreamUnits.poll_rate_ms` plus fan-out cap; `ipc/` workspace member; 0.9.2.
- Phase 4 harness: raw hook value in ACMI as the `LSOHook` property; `lso file` and
  `extract_recoveries_with_hook` replay it; 14 live recordings under
  `tests/recordings/live_2026-09/` assert hook state, DCS wire = estimated wire, arrest
  confirmation and T&G classification.

Not implemented (deliberately):
- Phase 5 live validation (needs server time). This is what the incoming data is for.
- Plan 1.8 carrier EMA replacement (datums now carry `raw_carrier_velocity` for the offline study).
- Plan 3.4 push-mode `StreamRecoverySnapshots` RPC, gated on the diagnostics below.
- AoA `NaN` when stationary, streaming ACMI to disk: low impact, deferred.
- Fork tag `v0.9.2` and the LSO CI `RUST_SERVER_REF` switch: needs the user to push and tag.

## 3. What the next test data must contain

Ask for (or locate under `trap_records/<campaign>/`) per session:
- the schema-8 `LSO-*.json` reports and their `LSO-*.zip.acmi` recordings (ACMI now embeds `LSOHook`);
- `lso.log` (or stdout) with the 10 s `runtime metrics snapshot` lines;
- `gRPC.log` and `dcs.log` from the server (confirms `DCS_server.exe`, mission load, Lua errors);
- the pilot's ground truth per pass: hook up/down, intended outcome, whether the DCS LSO called a
  waveoff, which wire the pilot believes was caught;
- the LSO and DCS-gRPC versions actually deployed (`lso_version`, `dcs_grpc_version` in the JSON).

Both a loaded mission (campaign A conditions: about 5 Hz, 28 % missing sequences) and a simple
mission run are needed to separate server load from code behaviour.

## 4. How to read the data (do this before touching code)

1. Per-pass table from the JSON (fields: `aircraft_type`, `outcome`, `pass_grade`, `grade_points`,
   `hook_state`, `arrest_evidence`, `wire_dcs`, `wire_estimated`, `wire_divergent`,
   `wire_estimation.reason`, `arrest_kinematics.{confirmed,reason,held_s,min_relative_speed_mps}`,
   `hook_observation.{baseline_state,baseline_reason,baseline_samples}`, `dcs_lso.waveoff_ordered`,
   `telemetry_quality.{completeness,health,max_sample_gap_ms,max_scoring_sample_gap_ms}`,
   `acquisition_mode`). Compare each row with the pilot's ground truth.
2. Telemetry under load from `datums[]`: distribution of `sample_gap_ms`, `request_round_trip_ms`,
   `queue_wait_ms`, `lua_exec_ms`, `queue_depth`; count missing `observation_sequence` values
   (runs of exactly 3 missing = one stall of about 1 s at the 250 ms timeout). Cross-check with the
   `snapshot_queue_wait_p95_ms` / `snapshot_lua_exec_p95_ms` metrics lines in `lso.log`.
3. Replay every new ACMI offline (`lso file <acmi>` or a fixture test) and confirm the offline grade
   equals the live JSON. A divergence is a bug in either the ACMI writer or the replay path.
4. Only then decide what to change. Add any new labelled pass as a fixture
   (`tests/recordings/live_2026-09/<name>.zip.acmi` plus `<name>.hook.json`; see the existing
   sidecars and the `live_2026_09` module in `src/tests.rs`). The `hook_samples` list is optional
   when the ACMI carries `LSOHook`; keep `pilot_hook` and `dcs_wire` as the label.

Keep any analysis script under `docs/claude-plan/` or the scratchpad, not in `src/`.

## 5. Acceptance gates and the decision tree

Gates (from `docs/BENCHMARK_PROTOCOL.md` plus this work):
- hook-up CQ passes → `T&G (CQ)` 100 %; hook-down traps → `hook_state: down` 100 %;
- DCS-labelled traps: `wire_estimated == wire_dcs` and `arrest_kinematics.confirmed == true`;
- WIRE#-less real traps → `Arrested (wire unknown)` or an estimated wire, never `NC`;
- no groove gap > 300 ms, snapshot RTT p95 ≤ 150 ms / p99 ≤ 200 ms, CPU/RAM ≤ 50 %;
- no bolter, T&G or waveoff ever confirmed kinematically (a false positive means stop and fix first).

Decision tree for the load question (plan 3.4):
- `queue_wait_ms` p95 high, `lua_exec_ms` small → queue congestion. Reduce other clients' load
  (StreamUnits users, `throughputLimit`), then consider the push-mode streaming RPC.
- `queue_wait_ms` and `lua_exec_ms` both small but RTT/gaps still large → transport or client
  scheduling; look at `tick_lag_*` metrics and the tonic/h2 warnings that are now logged.
- Gaps come in stalls of about 1 s with both fields absent → the server is not 0.9.2; deploy the
  fork first, nothing can be concluded without the diagnostics.
- `lua_exec_ms` normal but `dequeued_model_time` jumps → DCS frame or mission-script stalls; the LSO
  cannot fix it. Document it and keep the 10 Hz skip behaviour.

Threshold tuning rules if a gate fails:
- Hook baseline (`HOOK_BASELINE_*` in `src/track.rs`): widen `HOOK_BASELINE_GUARD_S` only if a real
  trap shows the excursion starting earlier than 1.4 s before `RunwayTouch`; check
  `hook_observation.baseline_end_dcs` against `touchdown_time_dcs` in the failing JSON first.
- Arrest kinematics (`ARREST_*`, `KINEMATIC_SPEED_WINDOW_S`): print the windowed series with
  `cargo test live_fixture_summary -- --nocapture` before changing anything. The cable pull-back
  after run-out reaches 10 to 15 m/s and must stay under the 8 m/s hold limit only once settled.
  Never erode the margin to bolters, which leave the deck at 45 to 60 m/s.
- Wire estimator (`MAX_HOOK_DEFLECTION_*`, `MAX_WIRE_VERTICAL_SEPARATION_M`): a `null` estimate on a
  DCS-labelled trap is acceptable (DCS wins); a wrong estimate is not. Widen the lag window only
  with a labelled counter-example.

## 6. Backlog after the data is in (priority order)

1. Apply the decision tree above; implement plan 3.4 only if the diagnostics justify it.
2. Tag `v0.9.2` on the fork after the user pushes; switch `RUST_SERVER_REF` in
   `.github/workflows/ci.yml`; optionally pin `Cargo.toml` to the tag.
3. Plan 1.8: evaluate step-detect plus velocity dead-reckoning against `raw_carrier_position`,
   `raw_carrier_velocity` and `filtered_carrier_position` in the new datums; adopt only if gate
   distance error and the sawtooth both improve; bump `grading_version` to `project-derived-v2`.
4. V/STOL (AV-8B on Tarawa) has zero live fixtures; request a Tarawa session before touching it.
5. Discord embed: show `DCS: n / Estimated: n` with an agreement marker and `arrest_evidence`
   (proposed in the wire-estimation analysis, not implemented).
6. Housekeeping: `graphify-out/` is stale (2026-08-28); `docs/DCS_gRPC_analyse/` (15 GB, git
   ignored) can be deleted locally.

## 7. Verification commands

```powershell
# LSO
cargo test                                      # 144 as of 2026-09-03
cargo clippy --all-targets -- -D warnings
cargo fmt --all -- --check
cargo test live_fixture_summary -- --nocapture  # windowed deck speed series for tuning
.\target\debug\lso.exe file <recording.zip.acmi> # offline regrade, writes the approach PNG

# rust-server
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --check
```

## 8. Pitfalls learned the hard way

- ACMI has no velocity. Replay kinematics use carrier-relative displacement over 1 s, never
  frame-to-frame velocity, because DCS steps ship positions every 1.4 s or so.
- The external hook argument is the animated hook, not the lever: a real trap reads "up" for
  0.5 to 1.4 s before `RunwayTouch` and 1.7 to 6.5 s after. Never classify from the latest sample.
- The aligner must not be fully reset on an RPC error, or outages vanish from the gap statistics.
- `GRADE:WO` comments have no ` : ` separator; `WO(AFU)IC`, `WOFDIC`, `WONSUX` are waveoff calls;
  `_WX_` is wings, not a waveoff.
- `docs/DCS_gRPC_analyse/` contains full repo copies with `target/` dirs; never grep it by default.
- Large heredocs fail in this Git Bash environment; write long files with the Write tool.
