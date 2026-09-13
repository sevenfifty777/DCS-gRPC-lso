# GetRecoverySnapshot implementation record

Date: 2026-09-01  
Status: code-complete for static validation; live DCS A/B validation pending

Dependency note: the copied LSO manifest now references the GitHub rust-server `dev` branch rather
than the adjacent local `stubs` directory. The static validation recorded below was completed with
the local implementation before this switch. Push the server changes to `dev`, then run
`cargo update -p dcs-grpc-stubs` and repeat the LSO validation. Replace the branch with the exact
release tag when it is published.

## Implemented

- Vendored the locked `dcs-module-ipc` 0.9.1 source as a workspace crate and added bounded queues, monotonic request IDs, enqueue/dequeue depth, queue wait, cancelled-request removal, and explicit overload/cancel errors.
- Added server diagnostics for per-request queue wait and Lua execution time, handler total time, outcomes, cancellation counts, and minute aggregates.
- Added the additive `dcs.recovery.v0.RecoveryService/GetRecoverySnapshot` protobuf, generated-stub conversion, tonic service registration, and one mission-Lua callback.
- Kept `GetTransform`, `GetDrawArgumentValue`, `StreamUnits`, grading thresholds, carrier smoothing, geometry, and recovery outcome rules unchanged.
- Added the copied LSO client's `auto`, `legacy`, and `atomic` modes. Mode is selected once at recovery start; `auto` falls back only on `UNIMPLEMENTED`.
- Atomic mode uses one 100 ms skip-scheduled snapshot request with a configurable 100-299 ms deadline, echoes/persists its sequence, and disables separate CATOBAR hook polling.
- V/STOL atomic requests omit the draw argument. Existing V/STOL grading semantics are unchanged.
- Bumped recovery-report `schema_version` from 3 to 4 and added `acquisition_mode` plus per-datum `observation_sequence` and atomic request round-trip time.
- Changed hook evidence storage to retain the newest 512 samples and count compacted samples without marking the recovery telemetry buffer-limited.

## Validation completed

```text
cargo fmt --all -- --check
cargo check --locked --workspace --all-targets
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo test --locked -p dcs-grpc-stubs recovery
cargo test --locked --workspace --no-run

# In DCS-gRPC-lso-new
cargo fmt -- --check
cargo check --locked --all-targets
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked no_acmi_and_hook_ab_configuration_are_accepted
cargo test --locked auto_falls_back_only_when_snapshot_rpc_is_unimplemented
cargo test --locked hook_timeline_compaction_preserves_recent_final_evidence_without_buffer_limit
```

The recovery-stub tests cover observed zero, not-requested, unavailable, and malformed/missing-transform responses. The LSO focused tests cover CLI defaults and final-hook evidence retention.

The IPC test executable compiles, but cannot start in this Windows development checkout because `lua.dll` is not available on the test process search path. The tests remain present for an environment with the Lua 5.1 runtime.

The complete LSO suite currently reports 102 passed and one pre-existing failure:
`grading::tests::test_touch_and_go_keeps_the_measured_approach_grade` expects `Incomplete` but the
implementation returns `OkParentheses`. The atomic-acquisition work does not change that grading
test or its production rule.

`cargo audit` found 12 vulnerabilities and 10 warnings in the existing 338-package lockfile,
including high-severity advisories in `quinn-proto` and legacy `rustls`, plus current advisories in
`bytes`, both locked `h2` lines, `ring`, `rustls-webpki`, and `time`. This implementation did not
change those package versions; remediation is intentionally a separate dependency-upgrade task.

## Still required before production rollout

- Run the IPC test executable in CI or a development environment containing the Lua 5.1 runtime.
- Run the new Lua callback inside DCS and verify missing-unit, omitted-hook, observed-zero, and hook-unavailable behavior.
- Freeze the exact baseline load and run the four controlled legacy/atomic A/B cells from the implementation plan.
- Archive raw client/server timing, queue, request-rate, timeout, sequence, ordinary-client probe, DCS version, mission hash, and consumer-load evidence.
- Do not enable `StreamUnits` fan-out limits or queue priority unless those measurements show the remaining contention and the separate compatibility tests are ready.

## Rollback

- Client-only rollback: start the LSO with `--recovery-telemetry-mode legacy`.
- Server rollback: deploy the preceding server build; existing public services were not changed.
