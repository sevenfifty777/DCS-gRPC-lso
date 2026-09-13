Work is complete on both repos, uncommitted on branch feature/post-analysis-plan-20260903 (created from snapshot in the LSO repo and from hook-mechanization-api in rust-server, as you asked). The full analysis and plan are in docs/claude-plan/as-rust-dcs-scripting-gentle-cat.md, which you committed.

Verification

Repo	Result
DCS-gRPC-lso	144 tests pass (was 119), clippy -D warnings clean, fmt clean
rust-server	29 tests pass, clippy clean, fmt clean; luacheck/protolint not installed locally
What was wrong, and what is fixed

Hook-up passes on the F-14 and T-45 were graded Bolter because polarity was only interpreted for the Hornet. The commanded hook state is now latched from the stable pre-contact baseline for all validated modules, so the arrestment excursion cannot flip it. All 7 hook-up recordings now grade as T&G (CQ).
A real trap without a DCS WIRE# ended as NC. Deck kinematics now confirm the arrest (2 s below 6 m/s within 8 s of contact, computed from carrier-relative displacement over a 1 s window because DCS steps ship positions every 1.4 s and the cable pulls the aircraft back at 10 to 15 m/s). Outcome reads "Arrested (wire unknown)" with arrest_evidence: kinematic.
DCS LSO comments are parsed for GRADE:, tolerant WIRE#, and WO calls. A complied waveoff is WO (1.0 pt); deck contact after a DCS waveoff is a Cut.
Telemetry outages were hidden by the aligner reset; wire crossings were never cleared across approaches; hook samples were drained before landed(). All three are fixed.
Load: one detection supervisor per generation (carriers + idle planes RPCs, one recording per plane) and one shared event stream replace the per-pair tasks and per-recovery streams. The ownship hook sampler is opt-in and stops after 8 unavailable answers. Poisoned mutexes recover, backoff resets after a healthy minute, the web board restarts, UCIDs are stripped from the API by default.
rust-server 0.9.2: GetRecoverySnapshot returns queue_wait_ms, lua_exec_ms, queue_depth; GRPC.errorInternal defined; method files load per environment; StreamUnits gets poll_rate_ms, rejects zero, caps fan-out; ipc/ is a real workspace member; panicking unwraps in lib.rs removed. The LSO persists the diagnostics per datum and reports p50/p95 in the metrics log.
Test harness: the raw hook value is written to ACMI as the LSOHook property, replay reads it back, and the 14 campaign C/D recordings are regression fixtures asserting hook state, DCS wire = estimated wire, arrest confirmation, and T&G classification.
Docs aligned with reality (versions, schema 8, migration 6, wire lag, per-aircraft argument, audit status, reversed decision J) and CI clones the fork next to the checkout so it can build again.
Not done, and why

Phase 5 live validation needs server time. Rerun the campaign A loaded mission and read the new snapshot_queue_wait vs snapshot_lua_exec percentiles to decide whether the push-mode streaming RPC is worth building.
The fork is at 0.9.2 but untagged and unpushed. Once you push and tag v0.9.2, switch RUST_SERVER_REF in the LSO CI and consider a git pin in Cargo.toml.
Carrier EMA replacement (plan item 1.8) is deferred; datums now record raw_carrier_velocity so it can be evaluated offline first. AoA NaN on a stationary aircraft and streaming the ACMI to disk are also deferred as low impact.
lua_files.rs in rust-server shows as a staged deletion from the agent's git rm; nothing is committed.