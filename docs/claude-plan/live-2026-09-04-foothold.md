# Live session 2026-09-04: Foothold (loaded mission, one human pilot)

Data: `trap_records/footholdmission_records/` (5 schema-8 reports + ACMI, `lso.db`, `lso.log`,
`gRPC.log`, `dcs.log`, full Tacview client track). LSO 0.4.0, DCS-gRPC 0.9.2, session
`s1788551836`, generation 8. Read `followup.md` first; this file records what the data showed.

## Pilot ground truth vs. reports

| # | Aircraft | Pilot hook | Report | Outcome | Hook | Arrest evidence | Notes |
|---|---|---|---|---|---|---|---|
| 1 | T-45 | up | 220531 | T&G (CQ) | up | none | ok |
| 2 | T-45 | down | 220833 | Bolter | down | none | ok, min relative speed 49.9 m/s |
| 3 | T-45 | down | 221024 | Bolter | down | none | ok |
| 4 | T-45 | down | **none** | — | — | — | real trap, DCS `GRADE:C ... WIRE# 2`; see below |
| 5 | F-14B(U) | up | 222403 | T&G (CQ) | up | none | ok |
| 6 | F-14B(U) | down | 222718 | Wire #1 | down | dcs_wire | estimate = DCS = 1, kinematics confirmed, held 2.0 s |

Offline replay (`lso file`) of the five ACMIs reproduces every live outcome, grade, hook state,
arrest evidence and wire.

## Pass 4: why it never became a report

`lso.log` (UTC):

```
20:12:20.5  selected recovery telemetry mode aircraft_type="T-45"
20:14:19.3  land event
20:14:20.3  landing quality mark event comment=LSO: GRADE:C : _LULIM_ (DLIM) _LULX_ _TMRDAR_ 3PTSIW WIRE# 2 _EGIW_ [BC]
20:14:27.8  stop (either carrier or plane despawned)        <- no "recording loop ended"
```

`dcs.log` 20:14:27.145: `Player 'Ghost-72 | TT' left unit QualifB-hot-4`. The Tacview track shows
the T-45 decelerating from 62 m/s to deck speed within 1.5 s of touchdown and sitting on the deck
until it is removed 8 s later. The pilot left the slot (to take the F-14) inside the recorder's
10 s post-touchdown window. The `Crash | Dead | PlayerLeaveUnit | UnitLost` arm of the recording
loop returned without grading, regardless of the accepted touchdown and the DCS wire already in
hand. Fixed on `feature/despawn-after-touchdown-20260904`: after an accepted deck contact the
despawn breaks out of the loop and the pass is graded from the recorded evidence
(`events[]` gets `despawn_after_touchdown`). Before any deck contact the recording is still
discarded.

The pass cannot be reconstructed after the fact: no LSO ACMI exists and the Tacview client track
has no hook argument, so it is not a usable fixture either.

## Telemetry under load (decision tree, plan 3.4)

Per pass, from `datums[]`:

| Pass | n | gap p50 / p95 / max ms | RTT p50 / p95 / p99 ms | queue_wait p50 / p95 ms | lua_exec p95 ms | queue_depth max | missing sequences |
|---|---|---|---|---|---|---|---|
| 1 | 1768 | 94 / 126 / 184 | 16 / 34 / 50 | 15 / 33 | 0.18 | 24 | 0 |
| 2 | 1042 | 94 / 124 / 174 | 17 / 33 / 51 | 16 / 32 | 0.19 | 23 | 0 |
| 3 | 1112 | 93 / 124 / 183 | 17 / 32 / 51 | 16 / 31 | 0.15 | 23 | 0 |
| 5 | 1886 | 93 / 124 / 374 | 17 / 34 / 50 | 16 / 34 | 0.07 | 25 | 1 |
| 6 | 1080 | 93 / 125 / 212 | 7 / 36 / 52 | 6 / 35 | 0.16 | 25 | 0 |

Sustained 10 Hz under the Foothold load, RTT p95 well under the 150 ms gate, `lua_exec_ms` is
negligible and `queue_wait_ms` tracks the RTT: the queue is the only cost and it is small. One
374 ms gap in pass 5 (a single snapshot timeout, `rpc_failures=1`, outside the scoring window
for the grade). The server-side `dcs_grpc::stats` lines agree (average queue wait 14 ms,
IPC failures 0 to 7 per minute). Conclusion: the push-mode `StreamRecoverySnapshots` RPC
(plan 3.4) is not justified by this session; keep the polling design.

## Gates (docs/BENCHMARK_PROTOCOL.md)

- hook-up CQ passes → `T&G (CQ)`: 2/2.
- hook-down traps → `hook_state: down`: 1/1 reported (pass 4 lost before grading, see above).
- DCS-labelled trap: estimate = DCS wire and arrest confirmed: 1/1.
- no bolter/T&G confirmed kinematically: 0 false positives in 4.
- groove gap ≤ 300 ms: 4/5; pass 5 had one 338 ms scoring gap (single timeout).

## Also done on the branch

- Discord `Wire` field (backlog item 5): DCS and estimated wire with agreement marker, arrest proof.
- `docs/DCS_gRPC_analyse/` deleted; its documents live in `docs/claude-plan/codex-sessions/`.

## Still open

- Plan 1.8 carrier filter (offline study first), V/STOL fixtures (needs a Tarawa session),
  `graphify-out/` rebuild after merge.
- Fork tag `v0.9.2` and the CI `RUST_SERVER_REF` switch (user action).
- (Fixed on the same branch) clippy `-D warnings` failed on `webpage-rm` since commit `1ced127`:
  `StoredPass` in `src/db.rs` kept fields only the removed web page read. The test-only struct now
  carries `#[expect(dead_code)]` so it keeps mirroring every `passes` column.
