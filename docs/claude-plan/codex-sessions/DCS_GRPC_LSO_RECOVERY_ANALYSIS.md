# DCS-gRPC LSO recovery analysis

Date: 2026-09-01  
Scope: `DCS-gRPC-lso-new`, `DCS-gRPC-lso-origin`, `rust-server-new`, and `rust-server-origin`

## Executive conclusion

The main defect is not that the recorder has no 100 ms timer. It does. The defect is that one recovery sample still requires two independent unary DCS mission RPCs, and the loop waits for both before it can advance. A 100 ms Tokio interval does not guarantee 10 Hz data when the DCS mission queue or Lua scheduler takes longer to answer. The saved records confirm that this is happening during the groove.

The original LSO application also polled live transforms in this way. It did **not** interpolate, extrapolate, predict, or smooth the trajectory. It requested carrier and aircraft transforms concurrently every nominal 100 ms and recorded only the replies that arrived. Consequently, the original was live polling, but it was never a gap-free or synchronized recorder. Slow RPCs produced missing time, not reconstructed points.

The new LSO adds useful quality evidence, gate interpolation, independent hook sampling, persistence, and much better tests. However, it also adds four grading-critical risks:

1. It extrapolates an older entity when carrier/aircraft timestamps differ by 100-300 ms, but that mechanism does nothing for the much more common whole-loop cadence stalls seen in the saved records.
2. A fixed-alpha carrier-position EMA is used in the actual recovery geometry, not only in chart presentation. Its lag changes with sample cadence, so the live-data gaps directly move the calculated groove and gate geometry.
3. The application measures a groove time but does not use it in the pass grade. The grade is based on only three distance-gate snapshots; it does not yet derive realistic time-ordered LSO corrections or trends from the groove.
4. All three available schema-v3 reports overflow the 512-entry hook-evidence timeline. That diagnostic overflow changes the entire recovery to `buffer_limit`, makes grading technically unavailable, and discards the newest hook samples.

The new server snapshot is version **0.9.1**, not 0.9.0. Its `GetTransform`/mission-queue hot path and its `StreamUnits` implementation are behaviorally unchanged from the 0.8.1 origin snapshot. Therefore, this comparison does not support blaming the gaps on a new 0.9.x transform implementation. It also does not support calling 0.9.1 optimized for this recovery workload: there is no atomic recovery snapshot, latency/load benchmark, or test of the DCS Lua queue under contention.

Possible targeted server fix:

> Reviewed implementation plan: [`GET_RECOVERY_SNAPSHOT_IMPLEMENTATION_PLAN.md`](GET_RECOVERY_SNAPSHOT_IMPLEMENTATION_PLAN.md). The plan preserves this direction while refining the IPC instrumentation, cancellation, hook-field semantics, deadline, fairness, compatibility, and validation requirements.

1. Add a dedicated `dcs.recovery.v0.RecoveryService/GetRecoverySnapshot` unary RPC instead of changing the generic `GetTransform` or `StreamUnits` APIs. Its request should contain the carrier name, aircraft name, an optional aircraft draw-argument number, and a client observation sequence. The server must not hard-code CATOBAR argument `25` or aircraft-specific calibration.
2. Execute that request once in the DCS mission Lua environment. Resolve both units, capture one `timer.getTime()` value, read both transforms, and—when requested—read the supplied aircraft draw argument in the same callback. Return the common DCS timestamp, both raw transforms, the echoed sequence, and an explicit draw-argument status. This is atomic at the mission-callback boundary; the two DCS object reads remain sequential rather than a simulator-wide transaction.
3. Preserve the existing generic endpoints for other clients. The LSO recorder alone should switch from two transform RPCs plus a separate hook RPC to one snapshot RPC per sample. At 10 Hz this changes the current default CATOBAR load from approximately 24 mission requests per second to 10; the live benchmark must record the actual rate.
4. First extend the mission IPC boundary so each request exposes a correlation ID, enqueue instant, dequeue/queue wait, cancellation state, Lua execution duration, completion status, queue depth, and method name. The current aggregate `Stats` and opaque `PendingRequest` queue cannot provide this attribution. Expired requests must be discarded before Lua execution rather than remaining as obsolete FIFO work.
5. Measure contention before introducing priority. If `StreamUnits` is the burst source, first bound its internal all-unit transform fan-out; a generic `getUnitTransform` limit cannot distinguish stream work from ordinary clients. Introduce weighted recovery priority only if bounded fan-out remains insufficient and starvation tests protect normal clients.
6. Poll the atomic method with `MissedTickBehavior::Skip`, at most one client request in flight, and a benchmark-derived deadline below the 300 ms stale-sample boundary—not necessarily below the 100 ms sampling period. A late sample should be recorded as missing, and cancellation-aware IPC must prevent timed-out work from executing later.
7. Validate with a repeatable one-carrier/one-aircraft DCS mission, first alone and then with normal dashboard/stream consumers. Separate correctness gates from live performance targets. Proposed performance targets remain no groove gap above 300 ms under the reference load, p95 at or below 150 ms, p99 at or below 200 ms, and no starvation of ordinary RPCs; DCS frame stalls must be reported rather than hidden by invented samples.

The core endpoint remains a small vertical addition: one additive protobuf service, one Rust service method, one Lua handler, generated stubs, one LSO client method, and focused tests. Exact timing attribution and safe short deadlines require the separate IPC observability/cancellation prerequisite described in the plan. `StreamUnits` should not be repurposed for recovery grading because its whole-second polling and all-unit fan-out have the wrong timing and load characteristics.

No production fix was made in this analysis. The recommended path is deliberately incremental.

## Scope and method

I compared the Rust, Lua, protobuf, manifests/lockfiles, relevant documentation, and tests in all four snapshots. Generated build output, Graphify output, ACMI/PNG artifacts, and old trap data were treated as evidence or generated material rather than source code.

The folders do not contain usable `.git` histories. This is therefore a directory-snapshot comparison, not a verified "Sunday commit versus yesterday commit" comparison. Exact commit ancestry and commit dates cannot be established from these copies.

The inspected source inventory, excluding generated/output directories, was:

| Snapshot | In-scope files | Rust | Lua | Proto | Markdown |
| --- | ---: | ---: | ---: | ---: | ---: |
| LSO origin | 24 | 21 | 0 | 0 | 2 |
| LSO new | 155 | 31 | 24 | 40 | 26 |
| Server origin | 101 | 58 | 18 | 16 | 4 |
| Server new | 120 | 66 | 22 | 20 | 6 |

The LSO-new counts include its copied DCS-gRPC reference/API bundle. The deepest review followed the grading data path rather than treating generated reference copies as application logic.

```text
Tokio 100 ms tick
    |
    +-- GetTransform(carrier) -- gRPC -- Rust MissionRpc -- DCS-MSE queue -- Lua/DCS
    |
    +-- GetTransform(plane)   -- gRPC -- Rust MissionRpc -- DCS-MSE queue -- Lua/DCS
             |
             +-- wait for both replies
                     |
                     +-- timestamp alignment / possible extrapolation
                     +-- carrier EMA
                     +-- carrier-relative x/y/alt/AOA
                     +-- groove entry + three grading gates
                     +-- touchdown/outcome correlation
                     +-- JSON/SQLite/chart/ACMI/Discord
```

This structure is important: parallel client futures do not make the two DCS mission requests atomic, and neither the client timer nor the server throughput setting is a real-time guarantee.

## Was the original recovery record really live?

Yes, in the limited and precise sense that it repeatedly requested current DCS state. No, if "live" means synchronized, lossless 10 Hz sampling.

The origin implementation:

- creates two `UnitClient`s and a nominal 100 ms interval;
- launches carrier and plane `GetTransform` calls concurrently and waits for both;
- preserves the DCS timestamps returned with each transform;
- writes separate ACMI frames when those timestamps differ by 10 ms or more;
- passes the two raw transforms directly to `Track::next`;
- computes carrier-relative geometry directly and appends a datum;
- uses transforms included in landing/runway-touch events as event evidence.

There is no position interpolation, extrapolation, filtering, smoothing, or motion prediction in the origin recording path. `remove_unchanged` only compresses repeated Tacview fields; it does not create or alter recovery points.

The interval in both origin and new uses `MissedTickBehavior::Delay`. When an RPC pair takes 800 ms, the recorder cannot collect the seven intervening 100 ms states. It resumes after the reply; it does not backfill them. Thus the correct description is:

> Original: nominal 10 Hz live polling, raw received state, possible carrier/aircraft timestamp skew, and real gaps whenever the request path is late.

A further DCS limitation remains: `getUnitTransform` attaches `timer.getTime()` when Lua handles the request, but an object's returned position can remain unchanged between DCS simulation updates. The original application had no way to identify or correct that source quantization.

## What the new recorder changed

The acquisition topology is still two unary transforms per tick. The principal additions are:

- receive timestamps and RPC metrics;
- a 2-second active telemetry watchdog;
- telemetry warning at more than 300 ms and invalidation at more than 1,000 ms;
- alignment of transforms with different DCS timestamps;
- position extrapolation for an older carrier or aircraft when skew is 100-300 ms and recent history is considered healthy;
- an independent hook sampler, normally separated from transform polling;
- a carrier-position EMA;
- interpolated 3/4, 1/2, and 1/4 nm gates;
- independent grading, quality/completeness reasons, JSON/SQLite persistence, charts, and operational metrics.

The extrapolation is limited to `position` and `alt` using velocity times delta. It does not advance rotation, heading, AoA, latitude/longitude, or other state. A sample can therefore be spatially moved to a new time while retaining old attitude-dependent evidence. This is especially relevant because hook offset uses aircraft rotation and lineup uses carrier heading.

The replay path is not equivalent to the live path. `TelemetrySample::from_replay` accepts up to 300 ms skew as `direct` and does not perform live alignment/extrapolation. A replay containing the same raw transforms can therefore produce different geometry from a live run. Existing parity tests use synchronized inputs and do not cover this case.

## Saved telemetry evidence

The refreshed snapshot contains 3 schema-v3 reports, 23 schema-v2 reports, and 237 older reports without a `schema_version`. Schema v3 is therefore available as current-generation live evidence. The three v3 reports identify DCS-gRPC 0.9.1, but their `lso_commit` is `unknown`; they validate the v3 runtime behavior while not proving an exact Git commit.

### Schema-v3 evidence

| Scope | Samples | Median gap | P90 gap | Gaps >300 ms | Gaps >1,000 ms | Invalid samples |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Entire v3 records | 2,586 | 101.5 ms | 542.7 ms | 491 (19.0%) | 20 (0.77%) | 21 |
| V3 groove-distance window (`-200 <= x <= 1389 m`) | 769 | 99.1 ms | 567.7 ms | 152 (19.8%) | 0 | 0 |

For v3 gaps over 300 ms, the median wall-clock receipt gap is 736 ms and the median DCS gap is 743.8 ms; both have a p90 of approximately 964 ms. This rules out a chart-only artifact and confirms real request/response cadence stalls in the current-generation records.

V3 alignment results are 2,565 `direct`, 21 `invalid`, and zero extrapolated samples. Only 1 of 9 v3 gate-quality entries is valid. Each report has a maximum scoring-segment gap between 949.2 and 965.5 ms, and every final grade is `Incomplete` with `grading_availability: unavailable_technical`.

This demonstrates the mismatch between the implemented correction and the observed failure mode: extrapolation handles carrier-versus-plane timestamp skew, while the v3 problem is predominantly that **the whole pair arrives late**. Groove samples below the 1,000 ms invalid threshold remain nominally valid even when their 300-966 ms gaps are too stale for gate interpolation.

The individual v3 reports are:

| Recovery | Max gap | Max scoring gap | Warnings | Invalid | Valid gates | Hook evidence dropped | Result |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | --- |
| Nazgul91, 22:31 | 971.1 ms | 965.5 ms | 189 | 0 | 0/3 | 250 | Incomplete, unavailable |
| ERGON, 22:57 | 1,043.6 ms | 949.2 ms | 175 | 18 | 1/3 | 259 | Incomplete, unavailable |
| ERGON, 23:01 | 1,029.2 ms | 954.1 ms | 148 | 3 | 0/3 | 111 | Incomplete, unavailable |

The final telemetry `health` value is only the health of the latest processed sample. One report ends `green/nominal` despite severe earlier gaps and `buffer_limit`; it must not be interpreted as whole-pass health.

### Schema-v2 historical comparison

The 23 v2 reports contain 15,651 samples. Their median gap is 105.1 ms, p90 is 706.2 ms, and 3,360 samples (21.5%) exceed 300 ms. In the v2 groove window, 1,071 of 4,990 samples (21.5%) exceed 300 ms and p90 is 696.6 ms. V2 alignment is 15,575 `direct`, 76 `invalid`, and zero extrapolated. Only 14 of 69 gate entries are valid, with 54 invalid and one late.

The v2 and v3 sets therefore tell the same operational story. Median cadence is close to the intended 100 ms, but repeated 0.5-1.0 second stalls damage gate availability. The small three-record v3 set has a better p90 than v2, but it is far from sufficient to claim the problem solved.

## Groove findings, in priority order

### P0 — The 10 Hz timer is not a 10 Hz acquisition guarantee

Each tick blocks until two separate mission RPCs complete. The interval has `Delay` behavior and there is no bounded per-tick cancellation below the general 2-second RPC deadline. The server processes mission requests from DCS through a scheduled Lua callback. Any DCS frame stall, mission-script load, queue competition, or other client fan-out directly creates a hole.

The new telemetry layer reports these holes but does not reconstruct them. Samples between 300 and 1,000 ms are warnings, not invalid samples. They can still affect groove entry, the smoothed carrier position, minimum distance, deck-crossing state, bolter/touch-and-go decisions, and persisted geometry even though gate interpolation separately rejects stale brackets over 300 ms.

Possible focused fix:

1. First add a controlled live benchmark that records client queue/RPC latency and server queue wait/execution latency for only carrier and aircraft transforms.
2. Then add one narrow `GetRecoverySnapshot` unary RPC returning both transforms, one authoritative DCS time, and—when requested—the aircraft hook draw argument from the same Lua callback.
3. Poll that single RPC at 10 Hz with `Skip` behavior and a deadline comfortably below the next intended sample; record a missed sample rather than building delay.
4. Keep warning/invalid raw evidence, but do not let samples outside the scoring-quality contract mutate grading or outcome state.

This reduces normal CATOBAR recovery load from roughly 20 transform RPCs plus 4 hook RPCs per second to 10 atomic snapshot RPCs per second and removes cross-request timestamp ambiguity. It is a small, recovery-specific vertical change across proto, Rust server, Lua, stubs, and LSO—not a broad rewrite.

### P0 — Hook-evidence overflow invalidates every schema-v3 recovery

Every v3 report has `completeness: buffer_limit`. This is not caused by the 72,000-entry transform/pattern buffers or the 256-entry event buffer. The hook timeline has a separate 512-entry limit. The observed hook-status totals are 762, 771, and 623; subtracting the retained 512 entries gives exactly the persisted dropped counts of 250, 259, and 111.

When the timeline is full, `observe_hook_sample` increments `telemetry_quality.dropped_samples` and sets the whole pass to `Completeness::BufferLimit`. Finalization then forces `grading_availability` to `unavailable_technical` and the pass grade to `Incomplete`, even if flight telemetry and gates would otherwise be usable.

The buffer also retains the first 512 hook observations and rejects every later observation. With a long 3.5-nm pattern recording, this preferentially discards the near-deck/final evidence that hook classification needs most.

Possible focused fix:

- make hook evidence a bounded rolling buffer that retains the newest samples, or retain a low-rate pattern summary plus all groove/final-window samples;
- count overwritten diagnostic history separately from scoring-data loss;
- do not set whole-pass `BufferLimit` merely because nonessential historical hook samples were compacted;
- mark the recovery incomplete only if required groove/touchdown hook evidence is actually unavailable;
- add a test exceeding 512 hook polls and prove that recent pre-touch/final samples remain persisted and the pass stays gradable.

### P0 — Carrier smoothing changes the grade

The new tracker applies a fixed-alpha EMA to carrier world position, then uses the filtered position for landing reference, distance, x/y, gate crossings, groove entry, stop decisions, and grading. It is not presentation-only smoothing.

At a stable 100 ms cadence, alpha 0.15 has an approximate 0.62-second time constant and a steady-motion lag of about 0.57 seconds. At irregular cadence, one sample after an 800-1,000 ms hole still advances the filter by only 15%, so its physical lag becomes much larger. The filter also cannot genuinely interpolate a carrier position that DCS has held stale; it waits for a jump and then eases toward the already-old/new endpoint.

Possible focused fix:

- maintain separate authoritative scoring geometry and display geometry;
- use synchronized raw snapshot values for grading;
- if DCS carrier quantization must be corrected, use an explicit time-based kinematic model with recorded provenance and bounded prediction, then validate it against raw state;
- keep any EMA only for chart rendering and never feed it back into gates, event correlation, or outcome classification.

### P0 — "Groove" is not yet being graded as a groove

Groove entry is the first qualifying sample inside 3/4 nm, below 300 ft and within 10 degrees of lineup. Touchdown time minus this sample time is persisted as `groove_time_secs`. However, `compute_pass_grade` names the parameter `_groove_time_secs` and never reads it. Tests explicitly establish that missing groove time does not affect a clean grade.

The pass grade comes from three point samples at 3/4, 1/2, and 1/4 nm. AoA is visual only. The application does not derive time-ordered corrections, trends, start/in-close behavior, or an LSO-style error sequence from telemetry. `lso_notation.rs` parses an existing DCS LQM comment; it is not a notation generator from the measured groove.

Possible focused fix, after telemetry is stable:

- decide and document whether groove time is diagnostic, a validity condition, or a scoring input;
- preserve a continuous, quality-gated groove segment rather than reducing it immediately to three points;
- derive deviations and trends over defined groove regions, including duration/persistence and recovery from an error;
- keep thresholds explicitly project-derived until calibrated against trusted observer/video examples within DCS limitations;
- add golden replay cases where independent human/LSO assessment is known.

### P1 — Live and replay can disagree

Live alignment can extrapolate 100-300 ms skew. Replay marks the same skew direct and leaves positions untouched. This undermines deterministic regression testing, because reproducing a saved pass may not reproduce live scoring geometry.

Possible fix: persist both raw observations and the exact correction inputs/results, and run one shared alignment implementation in live and replay modes. A golden replay must reproduce gate values, groove entry, outcome, completeness, and grade bit-for-bit within declared float tolerances.

### P1 — Quality rejection is incomplete

Invalid or badly delayed data is often excluded from gate capture, but geometry and several state machines are updated before or outside those gates. That allows evidence declared unsuitable for scoring to influence eventual classification.

Possible fix: split processing into:

1. raw audit storage;
2. health evaluation;
3. accepted scoring state transition;
4. display-only processing.

Only step 3 may modify groove/gate/outcome state, and it should require an explicitly accepted sample.

### P1 — Independent hook samples lack an authoritative DCS timestamp

The independent sampler stores wall receipt time, but when its queue is drained every pending hook value is assigned the current transform's `plane.time`. Several delayed hook polls can therefore collapse onto one DCS time, and pre/post-touch ordering depends on scheduling.

Possible fix: include the hook value in the atomic recovery snapshot. Until then, treat wall-time-only hook evidence as an interval with uncertainty rather than an exact DCS instant.

### P1 — The current grading rule and its test contradict each other

`compute_pass_grade` deliberately preserves the measured approach grade for a CATOBAR `TouchAndGo` with valid gates. The test named `test_touch_and_go_keeps_the_measured_approach_grade` expects `Incomplete`, while the next test expects `Ok` for the same outcome without an estimated wire. This is the one current test failure and represents an unresolved product rule, not a flaky test.

Possible fix: decide the qualification-touch-and-go policy first, change either the implementation or the contradictory assertion in one isolated commit, and document the rule.

### P1 — A valid touchdown can still become `UnconfirmedArrest`

For CATOBAR, `RunwayTouch`/`Land` establishes contact and an estimated wire can exist, but the final result is forced to incomplete if DCS LQM did not provide an authoritative cable. This is conservative and avoids awarding a false trap, but it can turn genuine DCS recoveries into `NC` when LQM is absent or unreliable.

Possible fix: keep `contact confirmed`, `arrest kinematically confirmed`, `wire estimated`, and `wire reported by DCS` as separate facts. Define a bounded deceleration/retention test for arrest confirmation before changing grade policy.

### P2 — The larger detector envelope increases load and duplicate-risk

Origin started a recorder only inside 1.5 nm/below 500 ft and required the aircraft to be behind and pointing generally toward the carrier. New starts within 3.5 nm/below 1,100 ft in any quadrant to capture the full overhead pattern. That is a valid feature choice, but the existing detector model still polls every compatible plane/carrier pair. More pairs remain active longer, and co-located compatible carriers can start competing recorders for one aircraft.

Possible fix: retain the pattern envelope but introduce per-aircraft recovery ownership and choose the best carrier candidate before starting a high-rate recorder. Keep low-rate detection separate from one exclusive high-rate recording session.

## gRPC server assessment

### Version and compatibility

- `rust-server-origin` declares 0.8.1.
- `rust-server-new` declares and reports 0.9.1; its changelog has 0.9.0 on 2026-08-27 and 0.9.1 on 2026-08-28.
- LSO-new pins the fork's `dcs-grpc-stubs` tag `v0.9.0`, lock commit `5bd6d6e42491c8697a5c5a95e80a2e689923bd3b`.

The methods used by this LSO compile against the deployed server snapshot, so no immediate wire incompatibility was found. Nevertheless, labeling the server folder "0.9.0" is inaccurate and complicates deployment/reproduction. Server binary, Lua installation, API docs, and client stubs should have an explicit compatibility matrix.

### Recovery hot path

`GetTransform` is unchanged in the relevant behavior:

1. Rust forwards `getUnitTransform` through `MissionRpc`.
2. Lua calls `Unit.getByName`.
3. The exporter calls `getPosition()` and `getVelocity()`.
4. Lua returns `timer.getTime()` plus the transform.

The DCS Lua request executor is also unchanged apart from loading new method files and structured-error handling. With the default throughput limit of 600, it schedules at 30 ms and processes up to 18 queued calls per callback. This is a theoretical queue cap, not a real-time service guarantee: `timer.scheduleFunction` runs on DCS simulation scheduling and can be delayed by frame/mission load.

The 0.9.x additions do not inherently slow `GetTransform` when unused. They can add load when other clients call them, but the diff provides no evidence of a transform-path regression.

### `StreamUnits` is not the solution for the groove

`StreamUnits` accepts integer seconds and defaults to five seconds. Internally it enumerates mission units and fans out one `GetTransform` request per unit on an update cycle, still through the same throughput-limited mission queue. It is intended for low-rate global/unit-state streaming, not paired 10 Hz grading.

Using it for LSO would reduce timing precision and could increase queue pressure. A separate dashboard or client using it at a short poll rate can also compete with recovery RPCs. The server does not reject `poll_rate = 0`; that can reach `tokio::time::interval(Duration::ZERO)` and should be validated independently.

### Concrete server defect outside the recovery hot path

New Lua methods call `GRPC.errorInternal` in `unit.lua` and `spot.lua`, but `grpc.lua` defines no such helper. The surrounding request handler will turn the resulting Lua exception into a generic error, so this is not the cause of transform gaps, but it breaks the intended structured error path and should be fixed in a small separate change.

### Optimization verdict

The new server compiles, formats, lints, and passes its current Rust tests. That establishes static health, not recovery-load optimization. The main server crate has no tests for DCS Lua transform latency, queue fairness, `StreamUnits` contention, or the new Lua endpoints. No live DCS benchmark in the snapshot proves sustained paired 10 Hz delivery.

Verdict: **no new transform regression was found in the source diff, but the server is not designed or validated to guarantee gap-free recovery telemetry.**

## Other findings to retain for later steps

- The new independent hook sampler is directionally correct because it avoids adding a third blocking RPC to every transform tick. Legacy inline mode does add that delay and should remain diagnostic-only.
- The quality/completeness fields, recovery IDs, session generation, persistence isolation, and telemetry metrics are valuable improvements and should be preserved.
- All three CATOBAR gates are required. This prevents invented grades from incomplete evidence, but makes availability highly sensitive to recorder gaps.
- Grading thresholds are explicitly marked project-derived. They need empirical calibration before being presented as real-world-equivalent notation.
- The V/STOL branch has deliberately separate final grading/spot rules. CATOBAR fixes should not alter those rules without dedicated V/STOL replay tests.
- Three schema-v3 live reports now exist and confirm cadence/gate failures. Because `lso_commit` is `unknown` and the sample is small, a complete version-identifiable live validation matrix is still required.

## Recommended incremental repair sequence

Do not combine these into one large update.

### Step 1 — Freeze the evidence contract

- Decide the contradictory CATOBAR touch-and-go rule.
- Add failing regression tests for a 750-1,000 ms groove gap, 100-300 ms cross-entity skew, an invalid sample near a gate, and independent-hook ordering.
- Add a regression exceeding 512 hook observations; it must retain the newest final-window evidence without invalidating the pass solely because older diagnostics were compacted.
- Make live and replay use the same accepted-sample/alignment contract.

Acceptance criterion: the same raw input yields the same quality, gates, groove time, outcome, and grade in live simulation and replay.

### Step 2 — Prove where latency occurs

- Run a controlled mission with one carrier/one aircraft and no other high-rate client.
- Correlate LSO tick lag and transform RPC histograms with new server queue-wait and Lua-execution timings.
- Repeat with dashboard/`StreamUnits`/other consumers enabled.

Acceptance criterion: identify whether each gap is client scheduling, transport, server queue wait, or DCS Lua execution delay; do not infer it from aggregate RPC time alone.

### Step 3 — Add the atomic recovery snapshot

- Add one narrow proto method and one Lua callback returning carrier transform, plane transform, common DCS time, and optional hook value.
- Keep existing generic APIs unchanged.
- Switch only the recovery recorder to that method.

Acceptance criterion: in the controlled groove, p99 sample gap is below the chosen scoring limit and there is no carrier/plane timestamp skew by construction. If DCS itself stalls, record the gap; do not invent samples.

### Step 4 — Separate measurement, grading, and display

- Remove fixed-alpha EMA output from scoring/outcome state.
- Quarantine rejected samples from all state transitions.
- Keep raw, corrected, and display values separately persisted.

Acceptance criterion: changing chart smoothing cannot change gates, outcome, wire evidence, groove time, or grade.

### Step 5 — Improve groove realism

- Define continuous groove regions and time/persistence/trend rules.
- Decide the role of groove time and AoA.
- Calibrate with trusted DCS tracks plus independent observer assessment.
- Add golden replays for clean pass, high/low, lineup drift, correction, waveoff, bolter, touch-and-go, wires 1-4, and telemetry faults.

Acceptance criterion: grades are deterministic, evidence-explainable, and comparable to the chosen operational reference within documented DCS limits.

### Step 6 — Clean independent server issues

- add `GRPC.errorInternal` or replace those calls with an existing structured helper;
- validate nonzero `StreamUnits.poll_rate`;
- document the 0.9.0-stubs/0.9.1-server compatibility boundary;
- add Lua-path and queue-contention tests/benchmarks.

## Validation performed

| Command/check | Result |
| --- | --- |
| LSO-new `cargo fmt -- --check` | Pass |
| LSO-new `cargo clippy --locked --all-targets -- -D warnings` | Pass |
| LSO-new `cargo test --locked --no-fail-fast` | **Fail: 100 passed, 1 failed** (`grading::tests::test_touch_and_go_keeps_the_measured_approach_grade`) |
| LSO-origin `cargo test --locked --no-fail-fast` | Pass: 5 passed; one compiler warning about function-pointer comparison |
| Server-new `cargo fmt --all -- --check` | Pass |
| Server-new `cargo clippy --locked --workspace --all-targets -- -D warnings` | Pass |
| Server-new `cargo test --locked --workspace --no-fail-fast` | Pass: 14 tests; no core DCS/Lua latency coverage |
| Server-origin `cargo test --locked --workspace --no-fail-fast` | Pass: 14 tests; no core DCS/Lua latency coverage |
| Lua static lint | Not run: `luacheck` is not installed |
| Persisted schema-v3 live evidence | Available: 3 reports using DCS-gRPC 0.9.1; `lso_commit` is `unknown` |

## Final answer to the immediate question

The original app did not extrapolate. It performed nominal 10 Hz live polling and stored the raw replies; it could and did have unfilled gaps whenever DCS-gRPC was late. The schema-v3 evidence confirms that the new app has not solved that acquisition limitation. It added skew extrapolation and grading-aware interpolation, but the saved problem is whole-loop latency, and the new carrier EMA makes those latency variations affect grading geometry. Separately, the 512-entry hook timeline currently makes every available v3 recovery technically incomplete.

The first engineering target should therefore be the acquisition contract—not threshold tuning: measure the queue, obtain carrier/aircraft/hook state atomically in one DCS callback, quarantine unhealthy samples, and separate display smoothing from scoring. Only after that data path is stable should the groove model be made more realistic.

## Annex A — Dependency and implementation order for the atomic snapshot

The atomic `GetRecoverySnapshot` is the foundational telemetry update. It should be implemented before tuning most groove and grading behavior because it changes the timing, synchronization, hook provenance, and server load of the evidence consumed by those later rules.

It is not a universal fix. Its expected effect on each identified problem is:

| Existing issue | Expected effect of atomic snapshot | Work still required afterward |
| --- | --- | --- |
| Carrier/aircraft timestamp skew | Eliminated by acquiring both transforms under one DCS timestamp | Retain a regression proving zero skew by construction |
| Position extrapolation | Normally unnecessary for live recovery after synchronization | Remove or disable it only after live validation; keep raw evidence and replay compatibility |
| Independent hook timing ambiguity | Eliminated if the hook value is read in the same Lua callback | Preserve module-specific calibration and explicit unknown state |
| Recovery request load | Reduced from approximately 24 mission requests per second to 10 | Measure queue wait and confirm other clients are not starved |
| Missing or stale grading gates | Expected to improve through lower load and atomic state | Gate quality limits and missed-sample handling still need validation |
| DCS frame or mission-script stalls | Not eliminated | Record explicit gaps; never invent intermediate samples |
| Carrier EMA affecting grades | Not fixed | Separate authoritative scoring geometry from display smoothing |
| 512-entry hook-evidence overflow | Not fixed | Use a recent/final-preserving bounded buffer and stop invalidating a pass for compacted diagnostics |
| Invalid samples changing state | Not fixed | Quarantine rejected samples from groove, gate, distance, and outcome transitions |
| Live/replay disagreement | Partly simplified | Use one shared processing contract and persist sufficient raw snapshot provenance |
| Groove time ignored by grading | Not fixed | Decide whether time is diagnostic, a validity condition, or a scoring input |
| Three-point-only groove grading | Not fixed | Add continuous region, persistence, trend, and correction logic after telemetry stabilization |
| Touch-and-go policy contradiction | Not fixed | Decide and document the intended CATOBAR rule, then align code and tests |
| Undefined `GRPC.errorInternal` | Not fixed | Correct separately in a small server Lua change |

### Recommended implementation order

#### A1 — Fix the hook evidence buffer

Do this first because it is small and independent. Every available schema-v3 recovery is currently forced to `Incomplete` after its hook timeline reaches 512 entries. A rolling/relevance-aware buffer must retain recent pre-touch and final-window evidence without treating compacted older diagnostics as loss of required grading data.

Acceptance criterion: a recovery with more than 512 hook polls retains the newest final-window evidence and remains gradable when all required evidence is present.

#### A2 — Freeze regression evidence and add latency instrumentation

Before changing acquisition behavior, add fixtures for the observed 500-1,000 ms stalls and record client tick lag, total RPC latency, server queue wait, Lua execution duration, queue depth, and method name.

Acceptance criterion: every delayed sample can be attributed to client scheduling, transport, server queue wait, or DCS/Lua execution rather than inferred from one aggregate duration.

#### A3 — Implement the atomic snapshot as one vertical change

Add the protobuf request/response, generated stubs, Rust service method, one Lua handler, and one LSO client method. Keep `GetTransform`, `StreamUnits`, and all existing clients unchanged. Include carrier transform, aircraft transform, common DCS time, and optional hook draw argument in the response.

Acceptance criterion: one recovery tick creates one mission request, carrier and aircraft have one authoritative timestamp, hook evidence is associated with that same observation, and existing APIs remain compatible.

#### A4 — Perform controlled live A/B validation

Run the same carrier, aircraft, mission, and approach first with the existing two-transform/independent-hook path and then with the atomic path. Repeat with ordinary dashboard and streaming consumers enabled.

Acceptance criterion: under the defined reference load, no groove gap exceeds 300 ms, p95 is at or below 150 ms, p99 is at or below 200 ms, and ordinary RPC clients show no starvation. If DCS cannot meet those limits, lower the declared supported sampling rate instead of manufacturing data.

#### A5 — Separate measurement, grading, and presentation

Once the acquisition behavior is stable, stop using carrier EMA output in grading geometry and prevent unhealthy samples from changing groove, gate, wire, distance, or outcome state. Any presentation smoothing must remain downstream of authoritative grading data.

Acceptance criterion: changing chart filters cannot change gate values, groove time, outcome, wire evidence, completeness, or grade.

#### A6 — Improve groove realism

Only after the data path is stable should groove duration, AoA, continuous deviations, trends, correction quality, region-specific errors, notation, and project-derived thresholds be tuned against trusted DCS tracks and independent observer assessment.

Acceptance criterion: the same raw recovery deterministically produces the same explained result in live processing and replay, and every grade can identify the accepted evidence and rule that produced it.

### Change-management boundary

These stages should remain separate commits or pull requests. In particular, do not combine the atomic RPC with grading-threshold changes. Otherwise, a changed grade cannot be attributed cleanly to improved telemetry, altered geometry, or a new grading rule. The atomic snapshot is the primary architectural correction, but the hook buffer is the prerequisite that makes its live results usable for evaluation.
