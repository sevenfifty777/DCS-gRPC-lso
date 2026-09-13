# GetRecoverySnapshot targeted implementation plan

Date: 2026-09-01  
Status: reviewed plan; no production implementation in this document  
Applies to: `rust-server` 0.9.1 lineage and `DCS-gRPC-lso-new`

## Goal

Replace the LSO recorder's two independent transform RPCs and optional separate hook RPC with one additive recovery-specific RPC that observes the carrier, aircraft, DCS mission time, and optional aircraft draw argument in one DCS mission callback. Preserve all existing public APIs and grading rules while making acquisition timing measurable, bounded, and safe to compare with the legacy path.

This is an acquisition change, not a grading redesign. Carrier smoothing, grading gates, groove rules, and touch-and-go policy remain out of scope until the new acquisition path has passed controlled live validation.

## Review of the proposed seven server steps

The direction in `DCS_GRPC_LSO_RECOVERY_ANALYSIS.md` is sound, but the implementation should use the following refinements.

| Proposed step | Review result | Required refinement |
| --- | --- | --- |
| Add a narrow unary RPC | Keep | Add a dedicated `dcs.recovery.v0.RecoveryService`; do not overload `GetTransform` or `StreamUnits`. |
| Read both transforms, hook, and one DCS time in one callback | Keep with qualification | This is atomic at the mission-callback boundary, not a literally simultaneous simulator-state transaction. The two unit reads are sequential inside one callback but share one authoritative DCS time. |
| Preserve generic endpoints and reduce request load | Keep | The stated reduction from about 24 to 10 mission requests/second applies to the current default CATOBAR mode: 20 transform requests plus four independent hook polls. Record the actual observed rate in validation. |
| Add enqueue/dequeue/execution/completion measurements | Split into a prerequisite | Current `Stats` only reports aggregate queue size and block time. Exact queue wait and per-request execution timing require a small `dcs-module-ipc` API change or equivalent local integration before the metrics can be claimed. |
| Add per-method limits/fairness | Narrow | `StreamUnits` creates the proven burst by queueing one transform future per eligible unit. Bound that internal fan-out if measurements confirm contention. A generic `getUnitTransform` limit cannot distinguish stream work from ordinary clients. Add priority only after bounded fan-out is insufficient and starvation tests exist. |
| Use skipped ticks and a deadline below the next sample | Correct | At 10 Hz, a deadline below 100 ms conflicts with the provisional p95 target of 150 ms. Use `Skip`, allow only one client request in flight, derive the deadline from the 300 ms stale-sample contract, and make the server queue cancellation-aware so timed-out work is discarded. |
| Validate against latency and fairness thresholds | Keep, split gates | Separate deterministic correctness gates from live performance targets. A DCS frame stall can still create a gap even when the snapshot code is correct. |

## Decisions fixed by this plan

1. The new API is additive and lives in a new `RecoveryService` rather than changing existing `UnitService` or `MissionService` behavior.
2. The request carries an optional aircraft draw-argument number, not a CATOBAR boolean. The server must not hard-code LSO hook argument `25` or aircraft-specific calibration.
3. The response distinguishes `not requested`, `observed`, and `unavailable`; absence must not be interpreted as hook-up or hook-down.
4. One DCS timestamp belongs to the whole response. Nested transforms do not carry independent timestamps.
5. A client-generated observation sequence is echoed by the response and persisted by the LSO for request/report correlation.
6. The LSO keeps a legacy compatibility mode during rollout. It chooses one acquisition mode at recovery start and never mixes legacy and atomic samples inside one recovery.
7. Expired/cancelled queued work must be removable before a short deadline is enabled. A client timeout alone does not remove the existing `PendingRequest` from the server's FIFO queue.
8. No grading threshold, carrier EMA, gate, groove-time, or outcome-policy change is included in the atomic-snapshot pull requests.

## Target contract

Create `protos/dcs/recovery/v0/recovery.proto` with a contract equivalent to:

```proto
service RecoveryService {
  rpc GetRecoverySnapshot(GetRecoverySnapshotRequest)
      returns (GetRecoverySnapshotResponse) {}
}

message GetRecoverySnapshotRequest {
  string carrier_name = 1;
  string aircraft_name = 2;
  optional uint32 aircraft_draw_argument = 3;
  uint64 sequence = 4;
}

message RecoveryTransform {
  dcs.common.v0.Position position = 1;
  dcs.common.v0.Orientation orientation = 2;
  dcs.common.v0.Velocity velocity = 3;
}

enum DrawArgumentStatus {
  DRAW_ARGUMENT_STATUS_UNSPECIFIED = 0;
  DRAW_ARGUMENT_STATUS_NOT_REQUESTED = 1;
  DRAW_ARGUMENT_STATUS_OBSERVED = 2;
  DRAW_ARGUMENT_STATUS_UNAVAILABLE = 3;
}

message DrawArgumentObservation {
  DrawArgumentStatus status = 1;
  optional double value = 2;
}

message GetRecoverySnapshotResponse {
  double time = 1;
  RecoveryTransform carrier = 2;
  RecoveryTransform aircraft = 3;
  DrawArgumentObservation aircraft_draw_argument = 4;
  uint64 sequence = 5;
}
```

The final field names may follow repository lint conventions, but the presence and error semantics must remain explicit. Both names must be non-empty. A missing carrier or aircraft returns `NOT_FOUND`. Invalid input returns `INVALID_ARGUMENT`. An unavailable optional draw argument returns a successful transform snapshot with `UNAVAILABLE`; it must not discard otherwise valid transform evidence.

The Lua-to-protobuf conversion must follow the existing `GetTransformResponseIntermediate` pattern. Lua returns two raw DCS transforms, and the `stubs` crate converts each through the existing `RawTransform -> Transform` logic. Do not duplicate the orientation, projection, or velocity formulas in Lua or in the LSO client.

## Target data path

```text
LSO 100 ms scheduler
  -> one GetRecoverySnapshot request, sequence N
  -> tonic RecoveryService handler
  -> one cancellation-aware mission IPC queue entry
  -> one getRecoverySnapshot Lua callback
       - resolve carrier and aircraft
       - capture one timer.getTime()
       - export carrier raw transform
       - export aircraft raw transform
       - optionally read requested draw argument
  -> stubs convert both raw transforms
  -> response sequence N + one DCS time
  -> LSO records raw observation, timing, hook status, and acquisition mode
  -> existing Track processing, without grading-rule changes
```

## Dependency and delivery order

### Work package 0 — Freeze baseline and the reference load

Purpose: make the comparison repeatable before changing the server.

Tasks:

- Preserve representative legacy fixtures containing the observed 500-1,000 ms stalls.
- Define the live mission, carrier, aircraft, route, DCS settings, server throughput limit, machine, dashboard consumers, and `StreamUnits` options.
- Record the exact server and LSO commits, DCS version, mission hash, run start/end, and acquisition mode with every result.
- Capture legacy request rate, client tick lag, client RPC round-trip time, DCS timestamp gaps, queue-size summaries, ordinary-RPC latency, and timeout/error counts.
- Define the ordinary-client probe used in every loaded run, for example a fixed-rate `GetTransform` or `GetVersion` request from a separate client.

Acceptance criteria:

- Another operator can run the same isolated and loaded scenarios from written instructions.
- Baseline output identifies each run and contains enough raw values to recompute percentiles and gap counts.
- No source comparison is described as commit ancestry unless it was taken from an actual Git checkout.

### Work package 1 — Make mission IPC observable and cancellation-aware

Purpose: prevent short client deadlines from leaving obsolete work in the FIFO queue and obtain the measurements promised by the analysis.

Current constraint: `dcs-module-ipc` stores opaque `PendingRequest` objects in an unbounded `VecDeque`. The server can see the method at dequeue time, but it cannot currently obtain a request ID, enqueue instant, exact queue wait, or cancellation state through the `Request` trait.

Tasks:

- Extend the IPC request metadata with a monotonic request ID and enqueue `Instant`.
- Expose request ID, method, queue wait, and receiver-cancelled state to the dequeue path.
- Make `try_next` discard requests whose response receiver has closed before Lua execution.
- Add a bounded queue policy with an explicit overload result; do not silently drop live requests.
- Emit structured, method-keyed measurements for queue wait, Lua callback execution, server-side request total, queue depth at enqueue/dequeue, completion status, and cancellation/expiry.
- Keep log cardinality bounded: method and outcome may be labels; unit names, player names, and sequence IDs belong in sampled trace fields, not metric labels.
- Pin the resulting IPC dependency exactly in `Cargo.lock`; do not rely on an unreviewed floating revision.

Measurement boundary:

- The LSO can measure client end-to-end round-trip time.
- The server can measure queue wait, callback execution, and server handler total.
- The difference is combined tonic/serialization/transport overhead; it is not a trustworthy one-way network latency measurement without synchronized clocks and a defined clock model.

Acceptance criteria:

- Unit tests prove FIFO order for live requests, cancellation removal, bounded-queue overload behavior, and correct timing-field ordering.
- A timed-out request that has not started never executes in Lua later.
- Existing RPC behavior remains unchanged when the queue is below its limit.
- Metrics can associate a delayed recovery observation with server queue wait and callback execution rather than only an aggregate duration.

Commit boundary: keep the IPC observability/cancellation change separate from the new recovery API.

### Work package 2 — Add the protobuf and generated-stub contract

Server files expected to change:

- `protos/dcs/recovery/v0/recovery.proto` — new API and documented semantics.
- `protos/dcs/dcs.proto` — import the recovery service definition.
- `stubs/src/recovery.rs` — generated module wrapper and raw-transform intermediate conversion.
- `stubs/src/lib.rs` — export the new module and add deserialization tests.
- `stubs/build.rs` — register the response intermediate conversion.

Tasks:

- Add the contract above without changing existing field numbers or services.
- Reuse `dcs.common.v0` position/orientation/velocity messages.
- Convert two raw Lua transforms with the existing common conversion code.
- Test `NOT_REQUESTED`, `OBSERVED`, and `UNAVAILABLE` JSON shapes, including a real zero hook value.
- Test missing raw-transform fields defensively; decide whether they are rejected or converted to defaults, and document that choice. Prefer rejection for this grading-critical method so malformed data cannot look valid.
- Ensure both client and server stub features compile.

Acceptance criteria:

- Existing client code generated from the old API continues to compile and call unchanged endpoints.
- New stubs deserialize a representative Lua response and preserve sequence, common time, both transforms, hook status, and a zero hook value.
- Proto documentation states that the snapshot is callback-atomic, not a simulator-wide transaction.

Commit boundary: the contract and stub tests may be one reviewable commit before the runtime handler.

### Work package 3 — Implement the server and mission-Lua vertical slice

Server files expected to change:

- `src/rpc/recovery.rs` — tonic method delegating to one mission IPC request.
- `src/rpc.rs` — register the recovery RPC module.
- `src/server.rs` — register `RecoveryServiceServer`.
- `lua/DCS-gRPC/methods/recovery.lua` — resolve units and produce one snapshot.
- `lua/DCS-gRPC/grpc.lua` — load the new mission method file.
- `CHANGELOG.md` and API documentation/release inputs — document the additive endpoint.

Lua handler order:

1. Validate request fields.
2. Resolve carrier; return structured `NOT_FOUND` if absent.
3. Resolve aircraft; return structured `NOT_FOUND` if absent.
4. Capture one `timer.getTime()` value.
5. Export the carrier raw transform.
6. Export the aircraft raw transform.
7. If requested, read the supplied aircraft draw argument and set an explicit status.
8. Echo the sequence and return success.

Tasks:

- Keep the callback short and free of persistence, logging loops, grading logic, smoothing, retries, or extra unit discovery.
- Use existing structured Lua error helpers. Do not introduce or rely on the currently undefined `GRPC.errorInternal` path.
- Attach the request/IPC trace identifier to structured diagnostics without logging unit/player names at normal information level.
- Ensure a hook-read problem does not erase valid transforms; report `UNAVAILABLE` and retain diagnostic context.
- Confirm the new Lua file is included by the integrity manifest and release package.

Acceptance criteria:

- One public gRPC call creates exactly one mission IPC queue entry and one Lua callback.
- One response contains both transforms and only one DCS timestamp.
- Existing `GetTransform`, `GetDrawArgumentValue`, and `StreamUnits` behavior is unchanged.
- Missing-unit, malformed-request, omitted-hook, observed-zero-hook, and hook-unavailable cases are covered.
- Static validation passes; live DCS validation is still required before calling the callback behavior proven.

### Work package 4 — Add an opt-in LSO acquisition path

LSO files expected to change:

- `Cargo.toml` and `Cargo.lock` — update `dcs-grpc-stubs` only after a server commit/tag containing the new API exists.
- `src/client/recovery_client.rs` and `src/client/mod.rs` — add one typed client method and timing instrumentation.
- `src/tasks/record_recovery.rs` — choose the acquisition path once and feed the existing tracker.
- `src/utils/interval.rs` — add a recovery-specific `Skip` interval without changing unrelated schedulers.
- `src/commands/run.rs` — expose `auto`, `legacy`, and `atomic` acquisition modes and a bounded timeout.
- Recovery report schema/tests — persist acquisition mode, response sequence, snapshot timing, and hook status; bump `schema_version` because persisted provenance changes.

Tasks:

- Keep `TelemetryAligner` available for legacy and replay data. The atomic live path should produce synchronized direct samples and must not silently extrapolate them.
- In `auto`, probe capability once before or at recovery start. Fall back only on `UNIMPLEMENTED`; other errors remain visible. Lock the chosen mode for the whole recovery.
- In atomic mode, disable both independent and inline hook RPCs for CATOBAR and use the hook observation in the snapshot.
- Keep V/STOL behavior unchanged: omit the draw-argument request and preserve its existing recovery path semantics.
- Maintain at most one client snapshot request in flight. Use `MissedTickBehavior::Skip`.
- Start with a configurable timeout below the 300 ms stale threshold, not below the 100 ms sampling period. Tune it from the baseline and record timed-out samples as gaps.
- Persist a missing/late observation rather than inventing a position or queuing catch-up calls.

Acceptance criteria:

- `legacy` reproduces the current acquisition topology.
- `atomic` issues 10 snapshot RPCs/second nominally and no separate hook RPCs.
- `auto` falls back on an older server before recording begins and never produces a mixed-mode report.
- CATOBAR and V/STOL tests show no unintended behavior change outside acquisition.
- The same accepted atomic observation feeds live and replay processing with identical authoritative values.

Commit boundary: LSO opt-in integration is separate from the server commits and from every grading change.

### Work package 5 — Controlled A/B validation

Run four cells using the frozen reference setup:

| Acquisition | Consumers | Minimum evidence |
| --- | --- | --- |
| Legacy | LSO only | 10 completed recoveries |
| Atomic | LSO only | 10 completed recoveries |
| Legacy | Dashboard/normal streams enabled | 10 completed recoveries |
| Atomic | Dashboard/normal streams enabled | 10 completed recoveries |

Use the same mission, carrier, aircraft, route, sampling configuration, server throughput limit, and ordinary-client probe. Randomize or alternate run order when practical so mission duration and host warm-up do not systematically favor one mode.

Correctness release gates:

- Every accepted atomic observation has one response time, one matching sequence, two non-default transforms, and an explicit hook status.
- Carrier/aircraft timestamp skew is zero by contract; no synthetic alignment or extrapolation is applied to atomic live samples.
- One atomic tick produces one mission request; no timed-out request executes later.
- Generic RPC compatibility tests pass and V/STOL behavior is unchanged.
- The hook-buffer fix is present before judging whether a completed pass remains gradable after more than 512 hook observations.

Provisional performance targets from the analysis:

- No groove sample gap above 300 ms under the defined reference load.
- Snapshot round-trip p95 at or below 150 ms and p99 at or below 200 ms.
- No sustained queue growth and no recovery request backlog.
- Ordinary-client timeout/error rate does not increase; its p99 stays within the predeclared baseline budget.
- Normal CATOBAR mission-request rate falls from the measured legacy value (expected about 24/s) to about 10/s.

These are live performance targets, not unit-test claims. If the atomic path is correct but DCS frame or mission-script stalls still violate them, preserve the gaps, report the responsible timing component, and lower the declared supported sample rate or revise the reference-load budget. Do not interpolate missing states to make the target pass.

### Work package 6 — Add load protection only from measured evidence

First response if `StreamUnits` is shown to create queue bursts:

- Replace its all-at-once `try_join_all` transform fan-out with a named, bounded concurrency value.
- Preserve per-unit backoff, event handling, stale-unit removal, and stream response behavior.
- Test large unit sets, stationary-unit backoff, cancellation, one missing unit, and stream shutdown.
- Re-run the four A/B cells and ordinary-client probe.

Only if bounded fan-out plus the atomic RPC still misses the reference-load budget:

- Design queue classes in the IPC layer, such as recovery and normal.
- Use weighted fairness or a bounded burst allowance; never strict recovery-first priority.
- Prove normal requests make progress under continuous recovery load and recovery requests make progress under stream load.

Acceptance criteria:

- Queue depth remains bounded under the reference unit count.
- `StreamUnits` output remains compatible.
- Neither request class can starve the other.
- Priority is not merged without before/after evidence showing that bounded fan-out was insufficient.

Commit boundary: stream fan-out and queue-priority work are independent server changes, not part of the atomic RPC commit.

### Work package 7 — Rollout, compatibility, and rollback

Delivery order:

1. Land the independent LSO hook-buffer repair so live results can remain gradable.
2. Land and pin IPC cancellation/observability support.
3. Land the additive server API and keep existing endpoints unchanged.
4. Publish a server/stubs commit or tag containing the contract.
5. Update the LSO stubs pin and ship the opt-in acquisition modes with `legacy` still available.
6. Run and archive A/B evidence.
7. Change the LSO default to `auto` or `atomic` only after the gates pass.
8. Defer removal of the legacy path until at least one compatibility window has passed and rollback is no longer required.

Rollback:

- Switch the LSO to `legacy`; do not downgrade the server merely because the additive endpoint is unused.
- If the server callback itself is unsafe, deploy the previous server build and retain the captured trace/report artifacts.
- Never roll back by mixing legacy and atomic samples in one recovery report.

## Validation commands

Run the smallest checks after each package and the full relevant set before merge.

Server and stubs:

```powershell
cargo fmt --all -- --check
cargo check --locked --workspace --all-targets
cargo test --locked --workspace --no-fail-fast
cargo clippy --locked --workspace --all-targets -- -D warnings
luacheck ./lua
protolint lint protos/.
```

LSO:

```powershell
cargo fmt -- --check
cargo check --locked --all-targets
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked --no-fail-fast
```

Record unavailable tools rather than replacing their checks with weaker claims. The copied LSO checkout previously had one failing touch-and-go policy test; preserve its exact test name and do not report the suite green until that result is intentionally resolved.

Live validation must additionally prove:

- the Lua handler loads in the DCS mission sandbox;
- two real unit transforms and the requested draw argument are returned;
- common time and sequence remain coherent;
- request counts and timing fields match the expected pipeline;
- dashboard/stream consumers remain responsive;
- server shutdown and mission reload do not leave queued snapshot work behind.

## Risks and controls

| Risk | Control |
| --- | --- |
| "Atomic" is interpreted as simultaneous physics state | Document callback-level atomicity and retain Lua execution duration. |
| Hook value `0` is confused with absence | Use explicit status plus an optional numeric value. |
| Short deadlines create server-side zombie work | Complete IPC cancellation handling before enabling the deadline. |
| New stubs are used against an old server | One-time `UNIMPLEMENTED` capability fallback in `auto`; persist selected mode. |
| Stream fairness work changes dashboard behavior | Separate commit, bounded fan-out first, compatibility and load tests. |
| Metrics create high-cardinality or sensitive logs | Keep names/sequences out of metric labels and sample trace-level detail. |
| Improved acquisition changes grades indirectly | Keep grading logic unchanged, persist acquisition mode, compare the same raw evidence. |
| V/STOL regresses from CATOBAR hook assumptions | Omit hook request for V/STOL and retain explicit V/STOL regression tests. |
| Generated transforms drift from existing `GetTransform` math | Reuse the stubs crate's single `RawTransform -> Transform` conversion. |

## Definition of done

The targeted server fix is complete only when:

- the additive RPC and stubs are documented, compiled, linted, and tested;
- one LSO atomic observation produces one cancellation-aware mission request and one callback;
- both transforms, one DCS time, sequence, and explicit hook status are persisted;
- old APIs and ordinary clients remain compatible;
- the LSO can select and report one acquisition mode per recovery and can roll back to legacy;
- controlled isolated and loaded A/B evidence is archived with raw metrics;
- correctness gates pass and performance results are reported without hiding DCS stalls;
- no carrier smoothing, grading threshold, groove model, or outcome-policy change is bundled with the acquisition work.

