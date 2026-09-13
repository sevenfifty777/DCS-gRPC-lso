# Steps Completion Log

## Step 1 — Starting point

Use the refonte as the starting point, but do not deploy it yet.

## Step 2 — Web board removal and dashboard handover

Step 2 is done and verified. Nothing is committed, following your rule that I only commit when asked.

### What changed on the integration branch

- **The built-in web board is gone.** `web.rs` is deleted, the axum library is removed from `Cargo.toml`, and the lockfile lost the axum entries. The greenie board is now the DCS Web Dashboard reading `lso.db`.
- **Old launch scripts fail loudly.** Passing `--web-port` or `--web-expose-ucid` stops LSO with a message that says the board moved to the dashboard, instead of a confusing "unexpected argument". See `run.rs:171`. There is a test for it.
- **The database is dashboard-friendly.** `db.rs` now opens `lso.db` in WAL mode with a 2 second busy timeout, so the dashboard can read while a pass is being written. A test checks the mode and the sidecar files exist.
- **One schema number for the merged lineage.** Reports now say `schema_version: 9`, above both the baseline's 8 and the refonte's 3. The field layout is still the refonte's. See `record_recovery.rs:1530`.
- **Crate version is 0.5.0.** This is my judgment call. Reports carry the version, and 0.2.0 would be indistinguishable from Lenny's builds. Say so if you want it left at 0.2.0.
- **Docs updated.** README, CHANGES and the French `AGENTS.md` no longer describe the web port, and both mention WAL and schema 9.

### Verification

| Check | Result |
| --- | --- |
| `cargo test --locked` | 291 + 2 passed |
| `cargo clippy --locked --all-targets -D warnings` | clean |
| `cargo fmt --check` | clean |
| `git diff --check` | clean |

### One thing to know

The refonte already uses migration number 6 for its own columns, and the baseline also uses 6 for the `arrest_evidence` and `hook_state` columns. When those two columns come over in step 3, I will register them as migration 7 so the two lineages don't collide.

### Next

Step 3, the biggest one: bring over the 14 live recorded flights with their hook sidecar harness, the despawn-after-touchdown fix, the kinematic arrest confirmation, and the hook-transient wire estimate, then write the human-LSO policy sentence into the primer and roadmap. Tell me to go and I'll start with the fixtures and harness, since they give us a way to check the other three as they land.

## Step 3 — Human-LSO trap confirmation

Step 3 is complete and verified. Nothing is committed, same as step 2.

### What the integration branch now does for a human-LSO trap

When DCS sends no `WIRE#`, an arrested-carrier contact is confirmed by one of two independent proofs, in this order:

1. **Hook transient.** The animated hook sits stably down, deflects sharply at contact, and returns down within eight seconds. The wire is the last pendant crossing within 200 ms of that deflection. Confidence medium, wire named.
2. **Deck kinematics.** The aircraft, measured against the raw carrier position, drops below 6 m/s within eight seconds of contact and holds for two seconds. Confidence medium, no wire invented.

Without either, the pass stays `unconfirmed_arrest` with no points, as before. The velocity-based signature the refonte already had stays diagnostic. The policy sentence is written into `primer.md` and `tasking-roadmap.md`.

### Validation on real flights

| Replay mode | 7 DCS-labelled traps | 6 hook-up touch-and-go | 1 hook-down bolter |
| --- | --- | --- | --- |
| As recorded | correct wire, `dcs_wire` | touch-and-go | bolter |
| DCS message removed | correct wire, `hook_transient`, graded | touch-and-go | bolter |
| DCS message and hook data removed | kinematic, graded, no wire invented | never promoted to a trap | bolter |

That is 42 fixture tests. The full suite is 333 unit tests plus 2 integration tests, all passing, clippy and rustfmt clean.

### Other changes worth knowing

- Wire-plane crossings now require the hook to be physically between the pendant ends, which fixes a false early crossing on the F-14 wire-4 recording.
- A hook-up contact that nothing confirms as an arrest finalizes as a touch-and-go, because the live recordings stop about two seconds after touchdown, before the aircraft has moved far enough for the existing decision.
- An eventless arrest, with no `Land` message at all, is now established from deck kinematics and ends after a bounded ten-second window. That closes review finding F04.
- Replay restarts a fresh track after an attempt that ends without touchdown. One recording contains a 360-knot low pass over the ship before the real approach.
- SQLite migration 7 adds `arrest_evidence` and `hook_state`. JSON adds the same plus the deck-kinematics and hook-deflection evidence. See `db.rs` and `track.rs`.

### Two things I did not change, on purpose

- **The despawn-after-touchdown path needed no port.** The refonte already routes both the removal event and the unit-not-found error into finalization. The remaining buffered-mode gap, the ten-second cutoff never being evaluated on empty batches, is step 4.
- **The refonte's groove detector latched a "groove"** on that 360-knot low pass at 18 degrees of lineup. That is a detection calibration question, so I recorded it as a confirmed bug in the roadmap rather than touching thresholds now.

### Next

Step 4, the four acquisition fixes: a shared read-rate limiter under the server quota, the post-touchdown cutoff on empty batches, ending an attempt on an ID mismatch, and giving the recorder loop a shutdown branch so Ctrl-C reaches finalization.

## Step 4 — Acquisition fixes

Step 4 is complete and verified. You committed steps 2 and 3 yourself, so the working tree now holds only the step 4 changes, uncommitted.

### The four acquisition fixes

1. **Ctrl-C now finalizes passes in flight.** The recorder's merged tick-and-event stream ends on shutdown, the recording is finalized with the evidence so far plus a shutdown event, and `lso run` waits up to 30 seconds for active recorders before returning. Without that wait the runtime was torn down under them. See `record_recovery.rs` and `run.rs`.
2. **Shared read budget under the server quota.** All buffered recoveries share one process-wide token bucket. The default is 16 reads per second, 80 percent of the server's default quota of 20. A new flag `--buffered-read-budget-per-second` adjusts it, and a quota refusal from the server now logs which knob to turn. The JSON telemetry block records how often reads waited and for how long. See `position_collector.rs`.
3. **Post-touchdown cutoff on empty batches.** The ten-second cutoff is now evaluated once per tick outside the per-sample loop, so a unit that vanished right after touchdown in buffered mode finalizes after ten seconds instead of waiting for the 29-second watchdog.
4. **Identity mismatch ends the attempt.** A buffered `id_mismatch` observation records a typed `unit_identity_mismatch` event and finalizes the evidence recorded so far, instead of polling the new incarnation.

### Verification

| Check | Result |
| --- | --- |
| `cargo test --locked` | 337 + 2 passed |
| `cargo clippy --locked --all-targets -D warnings` | clean |
| `cargo fmt --check`, `git diff --check` | clean |

New tests cover the budget pacing with three concurrent readers on paused virtual time, the degenerate-rate guard, the identity-mismatch helper, and the cutoff helper. The Ctrl-C path itself is not unit-testable and should be tried once live.

**One dev-only change to `Cargo.toml`.** The paced-time test needs tokio's `test-util` feature. It is a feature flag on an existing dependency, no new crate, and the lockfile is unchanged.

**Left open, recorded in the roadmap.** A generation ending on a fatal error still aborts recorders without joining them. That is the second half of review finding F01 and was not in the step 4 list.

### Next

Step 5, the review's untouched high-priority items: duplicate-check before mutation in the Harrier landing code, finite-value validation on the unary path and aligner plus one validity gate in the tracker, and the unknown-AOA colour falling through to Slow.

## Step 5 — Leftover correctness items

Step 5 is the review's leftover correctness items: the Harrier duplicate-touchdown mutation, finite-value validation on the unary path and one invalid-sample gate in the tracker, and the unknown-AOA colour.

Step 5 is complete and verified. Six files changed, nothing committed.

### The three review items

1. **Harrier duplicate touchdown (F03).** The landing function now checks for a duplicate before touching anything, so the second `Land` event DCS sends a few metres later can no longer overwrite the spot distance measured at the first contact. The admissibility check for a Harrier now measures to the nearest active spot rather than to the approach axis, so a landing on another spot of the same ship is not thrown out as foreign geometry. See `track.rs`.
2. **Non-finite values and one invalid-sample boundary (F05).** Positions, altitudes, orientations and velocities are checked for finiteness on every acquisition path, with a new `non_finite_value` reason. The unary aligner rejects before extrapolating, so a NaN velocity is never folded into a plausible position. Inside the tracker there is now one clearly marked boundary: an invalid sample still feeds the quality accounting, but no longer moves the carrier smoothing, the pattern trace, the distance floor, hook evidence, deck contact or the outcome decision. See `telemetry.rs` and `transform.rs`.
3. **Unknown AOA colour (F17).** A datum with a non-finite AOA is painted neutral grey instead of falling through every module's threshold chain to the Slow colour. Grading already skipped non-finite AOA, so only the chart needed the change. See `draw.rs`.

### Verification

| Check | Result |
| --- | --- |
| `cargo test --locked` | 342 + 2 passed |
| `cargo clippy --locked --all-targets -D warnings` | clean |
| `cargo fmt --check`, `git diff --check` | clean |

Five new tests cover each fix. All 42 live-fixture tests from step 3 still pass.

### One thing worth knowing

Two existing simulation tests broke and I corrected their fixtures rather than the code. They fed a carrier transform frozen at time zero, so every sample carried a one-second clock skew and was always marked invalid. They only passed because invalid samples used to drive the outcome, which is precisely the behaviour the review asked to remove. Both now use a carrier clock that follows the aircraft.

### Next

Step 6, provenance and repository hygiene: derive the stub version from the stubs crate instead of the typed `"0.10.0"` literals, remove the committed binary, sample folder and graph output from tracking, keep the launch script with the webhook untracked, and document the v0.9.2 server pin with `recoveryTelemetry.enabled` in the admin guide.


## Step 6 — Provenance and repository hygiene

Step 6 is complete and verified. The removals are staged, everything else is unstaged, nothing is committed.

### What changed

- **Server pin moved to v0.10.0.** Your new release is v0.9.3 renumbered after the merge of Lenny's fork: against v0.9.2 it changes only the restored `RESOURCE_EXHAUSTED` Lua helper and stops tracking the generated version file. No protobuf change, so the stubs line is fully compatible. `Cargo.toml` now pins `tag = "v0.10.0"` and the lockfile resolved to commit `a0dea7f`.
- **Provenance is derived, not typed.** The build script reads the resolved stubs version out of `Cargo.lock` and exposes it as one constant in `client/mod.rs`. Both the compatibility check and the `dcs_grpc_client_stubs` report field use it, so the mismatch the comparison flagged cannot recur. The build output confirms the compiled value is `0.10.0`. The compatibility test now derives its fixtures from that constant instead of a literal. See `build.rs` and `build_support.rs`.
- **Repository hygiene.** The 15 MB binary, the 100-file sample folder with real callsigns and a SQLite file, and the generated graph output are removed from tracking but kept on disk. All three paths are ignored, as is any `run-*.ps1` launch script, so the Discord webhook never enters source control. Git confirms each path matches its ignore rule.
- **Admin guide.** New `docs/ADMIN_GUIDE.md` covers installing the v0.10.0 release, the `recoveryTelemetry.enabled = true` requirement with the read quota and API key, the three compatibility verdicts LSO logs at startup, the run options, Ctrl-C behaviour, switch and rollback, and what is deliberately left out of git. README, the French reference and the changelog point to it.

### Verification

| Check | Result |
| --- | --- |
| `cargo build --locked` against v0.10.0 | clean |
| `cargo test --locked` | 342 + 3 passed |
| `cargo clippy --locked --all-targets -D warnings` | clean |
| `cargo fmt --check`, `git diff --check` | clean |

### One thing to do on the server side

Lenny's sessions ran a server built from his own fork head that reported `0.10.0` but lacked the queue diagnostics and the Lua helper. Now that the real v0.10.0 release exists, that is the build to deploy, with `recoveryTelemetry.enabled = true`. Until then, reports from his server will still carry a misleading version string.

### Next

Step 7, the controlled latency test: one human session with the hook sampler disabled, one with `--positions-only`, same mission and player count, comparing the `position_poll_p95_latency_ms` field. That step needs a live server session rather than code, so tell me when you have a session planned and I can prepare the exact launch commands and the comparison script.