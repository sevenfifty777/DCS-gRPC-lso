# `astra-review` versus `feature/refonte-v3-lua-buffer`

- Date: 12 September 2026
- Baseline: `astra-review` at `ed51ff0` (the branch reviewed in `REVIEW_RUST_DCS_2026-09-12.md`, plus the merge of `main`)
- Candidate: `feature/refonte-v3-lua-buffer` at `fc67446` (colleague branch, fork remote `lenny`)
- Merge base: `f962498` (PR #18, 31 August 2026)
- Live evidence: 158 JSON reports in `trap_records/refonte-v3-lua-buffer/` produced by nine successive refonte builds between 5 and 12 September, plus the 5 baseline reports of 4 September in `trap_records/footholdmission_records/`

## 1. Verdict

Neither branch should replace the other wholesale.

The refonte branch is the better engineering base. It already implements most of the module split the review proposed (event hub, position collector, report pipeline, pure grading module), it consumes the buffered Lua telemetry the review flagged as unused (F08), it adds the API-key interceptor (F07), no-clobber publication (F11), build provenance (F15), a stricter CI (F13), and it doubles the test count. It builds with `--locked`, passes 292 tests, and is clean under clippy and rustfmt.

The baseline branch gives better results today for the one requirement the product owner has stated: grading a trap when a human LSO runs the pattern and DCS emits no `WIRE#`. The refonte deliberately refuses to grade an arrest without a DCS wire message. In the live corpus this cost a grade on 21 of 66 arrested passes. The baseline keeps kinematic arrest confirmation and the hook-transient wire estimate as fallbacks, with the F04 defect still open.

Of the eighteen review findings, the refonte fixes or mostly fixes five (F07 partially, F08, F11, F13, F15 partially), restructures two into new policy questions (F04, F17), partially addresses four (F01, F02, F05, F06, F14), and leaves the rest untouched (F03, F09, F10, F12, F16). It also introduces new defects, two of which are serious: Ctrl-C never finalizes a recorder, and the buffered read quota is not obeyed once three recoveries run concurrently.

Recommended path: converge on the refonte as the code base, port the baseline's evidence features and live fixtures into it, and fix the short list in section 6 before any further grading calibration.

## 2. What each branch is

| | `astra-review` | `feature/refonte-v3-lua-buffer` |
|---|---|---|
| Change since merge base | 55 source/test files, +3.7k/-0.9k lines | 66 source/doc files, +16.9k/-4.0k lines |
| Crate version / JSON schema | 0.4.0 / schema 8 | 0.2.0 / schema 3 (separate lineage) |
| Embedded web board | removed in 0.4.0, dashboard reads `lso.db` | still present (`src/web.rs`, axum 0.8) |
| Stubs dependency | git tag `v0.9.2` | git tag `v0.9.2` (same lock rev `16291fb`) |
| Acquisition | unary atomic `GetRecoverySnapshot` at 10 Hz | buffered `Start/Read/StopRecoveryTelemetry` at 20 Hz by default, unary as explicit rollback |
| Hook evidence | inline draw argument in the atomic snapshot, plus optional ownship diagnostic | independent `GetDrawArgumentValue` sampler, stamped with batch-delayed capture time |
| Arrest without `WIRE#` | kinematic confirmation plus hook-transient wire estimate | diagnostic only, pass stays `Incomplete` |
| Grading | `project-derived-v1`, worst gate | `project-derived-v7`, zone-weighted episodes with correction credit, AOA axis, Cut on sink rate/bank |
| Groove entry | 3/4 NM gate | Case I roll-out state machine, conditional 3/4 NM gate |
| Tests | 148 | 292 |
| Live regression fixtures | 5 ACMI + 14 live ACMI/hook sidecars from 2 and 3 September | 5 ACMI only; the 14 live fixtures and the sidecar harness are absent |
| CI | no `--locked`, audit non-blocking | `--locked` everywhere, pinned `cargo-audit`, blocking audit |
| Provenance | `lso_commit: unknown` | `build.rs` emits commit and dirty flag |
| Docs | English, `ADMIN_GUIDE`/`DATA_CONTRACTS`, review doc | French `AGENTS.md` (1,122 lines), `primer.md`, `tasking-roadmap.md`; README drifts from code |
| Committed artefacts | none | `releases/lso.exe` (15.6 MB), `trap sample/` with real callsigns and a SQLite file, `graphify-out/` |

Section 2.2 of the review is now stale on one point: the baseline HEAD no longer uses the `../rust-server/stubs` path dependency. Commit `396a3d8` pinned the `v0.9.2` tag and the lockfile records the exact rev.

## 3. Finding-by-finding

| ID | Review finding | Refonte status | Evidence |
|---|---|---|---|
| F01 | Generation does not own and drain child tasks | Partial, Ctrl-C worse | Stream setup failure no longer detaches tasks; generation end aborts recorders via a registry (`run.rs:776`) but never joins them. The recorder loop is a `select` over interval and event hub; the hub stream never ends because each recorder holds the hub `Arc`, so Ctrl-C parks the loop and finalization at `record_recovery.rs:1221` is never reached. |
| F02 | `NOT_FOUND` discards an accepted touchdown | Fixed for unary, partial for buffered | RPC `NotFound` now breaks into the shared finalization (`record_recovery.rs:731`). In buffered mode a vanished unit only arrives as invalid snapshots, and the 10 s post-touchdown cutoff sits inside the per-sample loop (`:977`), so it is never evaluated on empty batches; the pass waits for the 29 s watchdog and can be downgraded to `TelemetryGap`. |
| F03 | Duplicate V/STOL touchdown mutates spot evidence | Unchanged | `Track::landed` (`track.rs:2503-2634`) still mutates spot distance before the duplicate check. The review probe would still report 12 m. A new 200 m rejection now also drops a legitimate V/STOL landing far from spot 7.5. |
| F04 | Eventless kinematic arrest never finishes | Restructured | The 2 s versus 8 s contradiction is gone because kinematic confirmation is now diagnostic only. Cost: a trap with no `Land`/`RunwayTouch` and no `WIRE#` becomes `Bolter` if the aircraft taxis, or `ApproachOnly` if it stays parked. |
| F05 | Invalid geometry accepted, invalid samples mutate state | Partial | Buffered boundary validates status and finiteness (`position_collector.rs:583-653`). Unary path and `TelemetryAligner` unchanged; a `NaN` position is still valid. Invalid samples still update the carrier EMA, pattern datums, distance minima and the outcome branch (`track.rs:1981-2177`). |
| F06 | Startup and subscriber gaps | Partial | Inventory still precedes `StreamEvents`. New `SessionEventHub` journals 512 events and replays those after attempt start, plus a 2 s grace after geometric close. Enrichment RPCs still run inline in the intake loop (`run.rs:637-650`). |
| F07 | No authentication, permanent errors retry | Partial | `ApiKeyInterceptor` on every channel including the stream and the recovery service; key read from `DCS_GRPC_API_KEY`. `Unauthenticated` still retries forever. |
| F08 | Buffered telemetry unused | Mostly fixed | Cursor advances only after a validated batch; epoch, handle, lifecycle and loss reasons are checked; stop on finalization. Missing: a client-side limiter for the shared `readsPerSecond` quota (server default 20/s per client, each recorder reads 10/s). |
| F09 | Carrier EMA affects grading geometry | Unchanged | `CARRIER_POS_SMOOTH_ALPHA = 0.15` still feeds `landing_pos`, lineup and glideslope (`track.rs:2017-2044`), applied to invalid samples as well. Raw and filtered positions are now stored per datum, which helps a later comparison. |
| F10 | Replay does not preserve live semantics | Unchanged | `from_replay` still clamps backward time; `Land` still writes no ACMI event; no hook property in ACMI. The `file` command writes PNGs only. |
| F11 | Output helper can clobber | Fixed | `create_new` temp, fsync, `hard_link` to target, typed `Created`/`AlreadyExists`, process-wide claim by `recovery_id`, concurrency test. Caveat: `hard_link` fails on exFAT/FAT/some SMB shares. |
| F12 | Finalization holds the detector slot, no Discord outbox | Unchanged | Recorder still awaited inline by the detector through wind query, DB insert, render and webhook. |
| F13 | Reproducible builds | Fixed on both | Both branches pin the tag. Refonte CI is stricter (`--locked`, pinned audit tool, blocking audit). Both still use the archived `actions-rs/toolchain`. |
| F14 | Name-only identity across incarnations | Partial | Buffered start sends unit IDs and the server reports `IdMismatch`, but a mismatch does not end the attempt with a typed reason. Unary path still name-only. |
| F15 | Provenance and timestamps | Partial | `lso_commit` and `lso_dirty` populated. `dcs_grpc_client_stubs` is hardcoded to `"0.10.0"` (`record_recovery.rs:1577`) while the lockfile resolves 0.9.2, so every refonte report misreports its stub identity. `grading_version` is still an opaque constant. |
| F16 | Resource bounds | Unchanged | `SessionLog` vector, in-memory ACMI, unbounded `try_join_all` inventory, no admission limit. New unbounded: task registry and claim set. |
| F17 | AOA is a ground-velocity angle | Computation fixed, display not, scope widened | `corrected_aoa_deg` is signed, pitch-plane and wind-corrected (`track.rs:699-706`). `NaN` still renders as **Slow** in `draw.rs:1167-1175`. AOA now affects the CATOBAR grade whenever a wind reference exists, including when the low probe was the sentinel and the high-altitude wind was substituted at deck level. |
| F18 | Docs, audit policy, metrics | Mixed | Audit policy fixed. README says 10 Hz and "AOA does not change the grade", both false on this branch. `AGENTS.md` header cites a stale HEAD and a path on another machine. `docs/GRADING_REFERENCE.md` is referenced from code but does not exist. |

## 4. Live evidence from the refonte corpus

All 158 reports are human passes, 141 of them F-14B(U), captured in buffered mode at 20 Hz.

**Delivery latency regressed after the first session and never recovered.**

| Date | Reports | Build | Read latency p95 (median over reports) | Health |
|---|---|---|---|---|
| 5 Sep | 8 | `96fd52eb` | 26 ms | green |
| 6 Sep | 12 | `b6308bca`, `b7d2623c` | 717 ms | red |
| 7 to 12 Sep | 138 | five later builds | 690 to 840 ms | red |

The position collector did not change between the 5 and 6 September builds. What changed is the environment (several human pilots at once) and the hook sampler being enabled for the F-14 (new draw-argument channel). The per-report distribution is bimodal: p50 around 28 ms, p95 around 700 ms. That shape points to periodic stalls of the DCS-side request queue rather than a slow RPC. Source capture stayed continuous at 20 Hz with zero reader loss in every report, so gates and grades are unaffected. Hook evidence is affected, because drained hook samples are stamped with the batch-delayed capture time. The colleague's roadmap lists the cause as unknown. The baseline's unary atomic mode held 10 Hz with a 34 ms p95 round trip on the 4 September Foothold session.

**Arrests without a DCS wire message are not graded.**

| Arrested passes | 66 |
|---|---|
| With `WIRE#` from DCS | 36 |
| Rust estimate also available | 9 (7 agree at high confidence, 2 diverge) |
| Without `WIRE#`, `unconfirmed_arrest`, no points | 21 |
| Rust estimate shown but no points | 12 of those 21 |

**The grade distribution is dominated by `--`.** Across the corpus, 92 of 158 passes carry the 2.0-point `NoGrade` label, 13 are `(OK)`, one is `OK`, one is `Cut`. The v7 grader (16 reports of 12 September) gives 8 `--`, 4 `(OK)`, 1 `OK`, 2 bolters, 1 incomplete. There is no human LSO truth for any of these, so neither branch's grading can be called validated.

**Other corpus observations.**

- Six AV-8B passes on 9 September: four `Incomplete` for insufficient gates, groove times of 77 to 122 s. V/STOL groove entry is not working on that build.
- `baseline_manifest` is all null in every report. The wiring reads correctly in source; the likely cause is the launch script not passing `--baseline-manifest`.
- The deployed server reported itself as DCS-gRPC 0.10.0 in every report, while the branch now pins the `v0.9.2` tag. The buffered feature also requires `recoveryTelemetry.enabled = true` in the server config. The deployed server line is therefore not the pinned one.
- Wind reference was established on 152 of 158 passes; median mission wind was 1 m/s, so the wind correction has not yet been exercised in a meaningful wind.

## 5. Honest assessment by area

**Acquisition.** The refonte's buffered collector is the right design and the review asked for it. Its boundary validation is stricter than anything in the baseline. Two things stop it from being promotable: the unexplained 700 ms delivery latency, and the missing quota limiter (three concurrent recoveries exceed the server's 20 reads/s and every `ResourceExhausted` is retried as a transient error until the 29 s watchdog fires). The baseline's atomic unary path is simpler and demonstrably held cadence under load, but it can never recover an observation it missed.

**Lifecycle.** Both branches fail F01. The refonte improved stream-setup failure and explicit abort, then regressed Ctrl-C. The baseline at least lets recorders exit the loop when the broadcast closes.

**Evidence and outcome.** The baseline is ahead on what the product owner asked for: kinematic arrest confirmation, hook-transient wire estimation, the despawn-after-touchdown fix, and 14 labelled live fixtures with hook sidecars. The refonte has a cleaner event hub and correlator, a better wire correlation by deceleration onset, and explicit `ApproachOnly` and `GRADE:WO` segmentation, but it discards eventless traps by policy and dropped the live fixture corpus.

**Grading.** The refonte grader is pure, typed, and covered by 99 unit tests against 34 in the baseline. It is also materially more lenient than the baseline tiers, awards full points to `ApproachOnly` passes with no proven outcome, applies a shared 0.3° reversal threshold to AOA, counts post-peak stabilisation in samples rather than seconds, and lets AOA affect the grade on a wind reference that can be silently substituted. None of these is wrong by construction, but each is a policy choice without human LSO truth behind it. The review's rule still holds: do not retune thresholds and change acquisition in the same change.

**Provenance, CI and publication.** The refonte wins clearly: `--locked` CI, blocking audit, build provenance, no-clobber outputs, authenticated channels. The hardcoded stub version string and the committed binary and sample folder undercut the provenance claim.

**Documentation and process.** The refonte's `AGENTS.md`, `primer.md` and `tasking-roadmap.md` are unusually candid about what is and is not validated live, which is valuable. They are in French while README and CHANGES are in English, the README contradicts the code on cadence and AOA, and the CHANGES file has duplicated sections. The baseline's docs are thinner but consistent with the 0.4.0 dashboard split.

## 6. Recommended actions

Order matters. Steps 1 to 3 are prerequisites for any merge; 4 to 6 are the correctness fixes the review already asked for; 7 onward is calibration.

1. **Choose the refonte as the code base, not as the release.** Create an integration branch from `feature/refonte-v3-lua-buffer`. Do not deploy it as the default until steps 2 to 5 are done. Keep `--position-source unary` as the documented rollback.
2. **Port the baseline's 0.4.0 contract into it.** Remove `src/web.rs` and axum, take the SQLite WAL and busy-timeout change, and settle one JSON schema number for the merged lineage. The DCS Web Dashboard reads `lso.db`, so the DB schema is the contract to protect.
3. **Port the baseline's evidence features and fixtures.** Bring over `tests/recordings/live_2026-09/` with the hook sidecar harness, the despawn-after-touchdown finalization, the kinematic arrest confirmation, and the hook-transient wire estimate. Then decide the human-LSO policy explicitly: a confirmed kinematic arrest with a hook transient yields a gradable `Recovered` at medium confidence with no invented wire number. Write that policy into the primer and roadmap.
4. **Fix the new acquisition defects before promoting buffered mode.** Add a process-wide read limiter below the server quota, evaluate the post-touchdown cutoff on empty batches, end an attempt with a typed reason on `IdMismatch`, and make Ctrl-C reach finalization by giving the recorder loop an explicit shutdown branch.
5. **Fix the review's untouched P1 items in the merged tree.** F03 admissibility before mutation in `Track::landed`, F05 finite-value validation in the unary client and aligner plus a single invalid-sample boundary in `Track`, and the `NaN` AOA colour falling through to Slow.
6. **Correct provenance strings and repository hygiene.** Derive `dcs_grpc_client_stubs` from the stubs crate version instead of a literal, remove `releases/lso.exe`, `trap sample/` and `graphify-out/` from tracking, and keep the launch script with the webhook untracked. Pin the deployed server to the `v0.9.2` tag build with `recoveryTelemetry.enabled` documented in the admin guide.
7. **Explain the delivery latency with a controlled test.** Run one human session with the hook sampler disabled, then one with `--positions-only`, on the same mission and player count, and compare `position_poll_p95_latency_ms`. If the sampler is the cause, drain hook samples through the buffered engine instead of a separate RPC stream, which also fixes the delayed hook timestamps.
8. **Calibrate grading only after the above, against human LSO truth.** Record LSO comments alongside the same passes. Compare v1 worst-gate and v7 episode grades on the identical corpus. Decide separately whether `ApproachOnly` earns points and whether AOA may affect the grade, and version the threshold set with a config hash so reports can be regrouped later.
9. **Re-validate V/STOL on the merged tree.** The 9 September AV-8B passes show groove entry broken on that build; the baseline's Tarawa spot logic and the review's F03 fixture should be the acceptance gate.

## 7. Are the two rust-server forks at the same level?

No. `LennyKruger/rust-server` is strictly behind `sevenfifty777/rust-server` on both branches, and everything Lenny pushed is already contained in the `v0.9.2` tag.

| Ref | Commit | Date | Version string | Relation |
|---|---|---|---|---|
| `lenny/main` | `c2c6ab5` | 2 Sep | 0.9.1 | 16 commits behind `origin/main`, 0 ahead; no buffered telemetry at all |
| `lenny/feature/refonte-lso-v3-lua-buffer` | `68db2dc` | 4 Sep | 0.10.0 | 6 behind `origin/feature/refonte-lso-v3-lua-buffer`, 0 ahead; merged into `origin/main` by PR #4 |
| `origin/main` = `v0.9.2` | `16291fb` | 6 Sep | 0.9.2 | 13 commits after Lenny's last push; GitHub release "DCS-gRPC v0.9.2" |
| Lenny fork tags and releases | none | | | |

What `v0.9.2` has that Lenny's fork does not:

- `HookService.GetOwnshipHookState` (hook.proto, hook.lua, rpc/hook.rs).
- Latency diagnostics on `GetRecoverySnapshot`: `queue_wait_ms`, `lua_exec_ms`, `queue_depth`, `dequeued_model_time`, and a `detail` string on unavailable draw arguments. These are exactly the measurements needed to explain the 700 ms delivery latency in section 4.
- `grpc.lua` changes: `GRPC.monotonicMs`, per-request IPC metadata passed to Lua handlers, mission-env versus hook-env loading of `recovery.lua` and `hook.lua`, and the `callsPerTick` off-by-one fix (`>` became `>=`).
- A floating-point fix in the buffered engine's diagnostics interval, CI running the Lua engine tests, release scripts, and removal of Lenny's experimental `ipc/` crate (Lua 5.1 headers and an IPC prototype that was never a workspace member).
- Version renamed from `0.10.0` down to `0.9.2` at release.

Consequences for the LSO refonte branch:

- Every one of the 158 corpus reports says server `0.10.0`, so Lenny's sessions ran a server built from his own fork head, not the released `v0.9.2`. That server lacks the queue diagnostics and the `callsPerTick` fix.
- The LSO refonte hardcodes `DCS_GRPC_CLIENT_STUB_VERSION = "0.10.0"` (`run.rs:31`) and the report field `dcs_grpc_client_stubs: "0.10.0"` (`record_recovery.rs:1577`). Against a real `v0.9.2` server the compatibility check returns `incompatible_api_line`. It only logs a warning, but every report will carry that label. Both constants should be derived from the stubs crate version.
- The `SYNC_UPSTREAM.md` procedure on the LSO refonte branch describes pulling from Lenny's LSO fork with `--ff-only`. For rust-server the direction is reversed: Lenny should fast-forward from `sevenfifty777/rust-server` `main` or check out the `v0.9.2` tag, then rebuild and redeploy the DLL and Lua bundle.

Will Lenny's LSO branch work against the `v0.9.2` server? Yes, functionally. Every RPC it calls exists in `v0.9.2` with the same request and response shapes; the additions since his head are optional fields and a new hook RPC he does not use. The server config key `recoveryTelemetry.enabled` and its defaults are identical. Three things to know:

- The version label only. His LSO logs `incompatible_api_line` because it compares the server string `0.9.2` with its hardcoded `0.10.0`; the check is a warning and changes no behaviour.
- One regression in `v0.9.2` that is not in Lenny's fork: `lua/DCS-gRPC/methods/recovery.lua:125` still calls `GRPC.errorResourceExhausted`, but `grpc.lua` in `v0.9.2` no longer defines it (Lenny's `grpc.lua` does). When the engine hits `maxActiveRecoveries` (16) or `maxActiveCarriers` (8), `StartRecoveryTelemetry` raises `attempt to call field 'errorResourceExhausted' (a nil value)` and the client receives a generic error instead of `RESOURCE_EXHAUSTED`. The Lua tests stub the helper, so CI did not catch it. Fix: add the helper back to `grpc.lua` (the Rust side already maps the `RESOURCE_EXHAUSTED` type in `src/rpc.rs:219`).
- The hook-environment `callsPerTick` fix and the queue diagnostics change timing slightly and add fields; neither affects his parser.

Action: fix the missing Lua helper and cut `v0.9.3`, Lenny redeploys that release build, deletes or rebases the stale `feature/refonte-lso-v3-lua-buffer` on his fork, and the LSO stub version constants are replaced by the crate version so the next corpus carries the right provenance.

## 8. Validation performed for this comparison

| Check | Result |
|---|---|
| `cargo test --locked --no-fail-fast` on `astra-review` HEAD `ed51ff0` | 148 passed |
| `cargo test --locked --no-fail-fast` on refonte `fc67446` (sibling worktree, git-tag stubs) | 290 + 2 passed |
| `cargo clippy --locked --all-targets -- -D warnings` on refonte | clean |
| `cargo fmt --all -- --check` on refonte | clean |
| Refonte `lso file` on four baseline live fixtures | runs, writes PNGs only, no JSON to compare |
| 158 refonte JSON reports mined for grading, telemetry and provenance fields | summary in section 4 |
| Live DCS, dashboard, Discord | not exercised |

Three parallel source audits of the refonte checkout were run against the review's findings; their file and line references are reproduced in section 3. The rust-server comparison in section 7 used `git fetch` of both forks into the local `rust-server` checkout, `rev-list --left-right --count`, and file-level diffs; the `lenny` remote was added to that checkout and left in place. No branch was modified, committed or pushed. The temporary worktrees used for building were removed afterwards.
