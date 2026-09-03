# Reliability architecture

This document describes the executable reliability contract introduced after analysis revision
`95f1d27ff273c93b547c963514d26c8d77b31d7f`. It is a local implementation contract, not evidence
that the deferred DCS live behaviours have been validated.

## Recovery identity and supervision

A supervisor generation is created for every DCS-gRPC connection. The mission session is read from
`MissionService.GetSessionId`; a negative generation-derived value is used only when that RPC is
unavailable. A recovery is correlated by:

```text
session + generation + aircraft unit id + pilot internal identity
        + carrier unit id + recovery mode
```

Human internal identity is the UCID resolved from the occupied network slot. Display names are never
identity keys. An unresolved human receives a session/unit-local key; an AI receives
`ai:<session>:<unit-id>`. These private keys are not logged or placed in public artifacts.

The phase-1 pairing matrix is strict:

| Aircraft | Carrier | Mode | Detector created |
|---|---|---|---|
| AV-8B NA | LHA Tarawa | V/STOL | yes |
| supported hook aircraft | Nimitz/Forrestal geometry | arrested | yes |
| AV-8B NA | arrested carrier | incompatible | no |
| hook aircraft | Tarawa | incompatible | no |

Initial discovery and later Birth events use the same matrix, irrespective of whether the aircraft
or ship appears first. A respawn replaces the old pair task. Session/generation rotation aborts all
old pair and event tasks. A failed pass is logged locally and does not stop its detector or another
pair.

## Time model

Three clocks are retained:

- DCS source timestamps for aircraft and ship;
- Unix reception timestamps for diagnostics and cross-log correlation;
- monotonic `Instant` values for local gaps, watchdogs, deadlines and durations.

Unary RPC and stream-opening requests have a two-second deadline. Connection establishment also has
a two-second timeout. Active tracking targets 10 Hz.

Recovery telemetry has two acquisition modes. In **atomic** mode (`RecoveryService.GetRecoverySnapshot`,
selected by default when the server implements it) the carrier transform, aircraft transform and the
hook draw argument are read by one server-side Lua callback with one DCS timestamp and an echoed
sequence number, so the hook value shares the 10 Hz sample and needs no extra RPC. In **legacy** mode
(server without the RPC, or `--recovery-telemetry-mode legacy`) the two transforms are fetched
separately and the hook draw argument is kept outside the critical path: an independent 4 Hz sampler
uses a 300 ms deadline (configurable within 2-4 Hz and 250-300 ms; `--legacy-inline-hook-sampling`
restores the former blocking read for A/B comparison). Those hook-sampling flags have no effect in
atomic mode.

Telemetry alignment is `PROJECT-DERIVED`, version `telemetry-contract-v1`:

| Condition | Action |
|---|---|
| skew <= 100 ms | use both transforms directly |
| 100 ms < skew <= 300 ms | extrapolate the older position only when the preceding sample was valid and fresh |
| skew > 300 ms | invalidate the observation |
| sample/source gap > 300 ms | warn; a gate bracket whose actual endpoints exceed it is invalid |
| gap > 1,000 ms in a gate/groove | mark scoring telemetry incomplete; award no points |
| gap > 1,000 ms outside the scored segment | retain a pattern diagnostic without invalidating the grade by itself |
| no real DCS timestamp advancement for 2 s | watchdog ends the pass as incomplete |
| supervisor channel silent for 2 s | rotate the supervisor generation |

Extrapolation is position-only (`position += velocity * dt`). It is reset after an RPC failure,
time reversal, reconnect or session rotation. There is no interpolation or joining across a cut.
Each stored approach datum includes raw and corrected carrier position, both DCS times, both
reception times, skew, gap, alignment method and validity.

## Gate state machine

Each of the 3/4, 1/2 and 1/4 NM gates starts as `Missing` and can become:

- `Late` when tracking starts already inside the threshold;
- `Invalid` when the bracket is stale, skewed, reordered, outside the approach phase or invalid;
- `Valid` only after two valid inbound samples bracket the threshold.

A small rolling window selects a valid inbound pair that truly brackets the threshold. Its gap is
computed exclusively between those two endpoints; an old gap carried by the first endpoint is not
reused. No pair over 300 ms and no pair spanning a cut is joined. The gate is linearly interpolated,
and its DCS time, effective distance, bracket gap/skew, method, state and reason are persisted. The
three valid times must be strictly ordered. Zero, one or two valid gates
always produce `NC`, no points and no favourable grade.

## Event and outcome evidence

The recorder consumes `Land`, `RunwayTouch` and `LandingQualityMark` with exact aircraft/carrier unit
IDs. Every matching event is retained in arrival order with its DCS timestamp, source, acceptance,
confidence and rejection reason. The first geometrically plausible contact is preserved; later
duplicates cannot overwrite its time, position or speed.

`Land`/`RunwayTouch` prove a correlated contact, not by themselves an arrested trap. A DCS wire from
LQM is the currently implemented positive trap confirmation and remains authoritative. A geometric
wire remains an estimate: it requires a complete hook-deflection transient (>=0.8 -> <=0.7 -> >=0.8)
correlated <=200 ms after the last finite cable-plane crossing and is used only when DCS supplies
no wire.
A CATOBAR contact without confirmed wire is incomplete and receives no favourable grade. A deck
crossing followed by departure without arrest is a bolter. A departure before deck crossing is a
waveoff with unknown initiator (`WO?`) and no points.

The external hook draw argument is per aircraft: `1305` for all supported F-14 variants and `25` for
the F/A-18C and T-45. It is observed through the groove and an explicit final quarter-NM window,
inside the atomic snapshot or through the legacy independent sampler. Raw values, local age, DCS
association, success/timeout/error/stale status and the requested `evidence_source`/`draw_argument`
are stored. Only the F/A-18C corpus currently has a calibrated polarity: a stable pre-touch run at 0
means hook up and a stable run at 1 means hook down. The classifier uses a three-second window,
minimum sample count and stability duration. Other modules, including F-14 and T-45, remain
`unknown` pending live proof. Unknown or stale data cannot create a certain outcome or favourable
result.

`HookService.GetOwnshipHookState` (`LoGetMechInfo().hook`) is diagnostic-only. `LoGetMechInfo` is
ownship-only, so on a dedicated server the RPC returned `unavailable` in 100% of 6164 live samples;
its values are persisted as evidence and never feed grading.

For V/STOL, first-contact horizontal speed is retained so future evidence can distinguish VL and
RVL without inventing a threshold. A contact followed by departure is normalized to a neutral
go-around/touch-and-go outcome, never a bolter. Duplicate contacts are retained as robustness
evidence. These simulations do not prove the ordering or reliability of real Tarawa events.

## Bounded resources and outputs

- supervisor error queue: 16 entries;
- approach datums: 72,000;
- pattern datums: 72,000;
- event evidence: 256.

Overflow is explicit, counted and changes completeness to `BufferLimit`. Output filenames include
session, generation, aircraft, carrier and DCS time. JSON and ACMI writes use same-directory temporary
files and atomic rename; an existing deterministic target is treated idempotently. SQLite has a
unique partial index on `recovery_id`.

SQLite is attempted independently of PNG, ACMI and Discord. PNG rendering runs in `spawn_blocking`.
Failures of secondary outputs are logged after persistence and do not remove a pass. Discord is
sent only for a newly inserted database row, which prevents retry duplicates. The dashboard binds
only to `127.0.0.1` and reports database failures as HTTP 500 instead of an empty result.

## Runtime state overview

```text
Disconnected
  -> connect/deadline -> Generation(session, generation)
  -> strict discovery -> Pair detector(s)
  -> pattern envelope -> Active recovery at 10 Hz
  -> gates + events + evidence
  -> outcome + completeness + score
  -> durable row / atomic artifacts
  -> isolated render/publication

session change, 2 s silence or stream failure
  -> abort old generation -> backoff -> new generation
```

See [DATA_CONTRACTS.md](DATA_CONTRACTS.md), [GRADING_REFERENCE.md](GRADING_REFERENCE.md), and
[BENCHMARK_PROTOCOL.md](BENCHMARK_PROTOCOL.md).
