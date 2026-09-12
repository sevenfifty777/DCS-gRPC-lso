# Recovery telemetry source buffer

`dcs.recovery.v0.RecoveryService` exposes three additive RPCs in addition to
the existing `GetRecoverySnapshot` call:

- `StartRecoveryTelemetry` registers an opaque recovery handle and an expected
  aircraft/carrier name-and-ID pair;
- `ReadRecoveryTelemetry` reads, without deleting, snapshots whose per-recovery
  sequence is strictly greater than `after_sequence`;
- `StopRecoveryTelemetry` idempotently releases the capture and keeps a short
  transform-free tombstone so retries can distinguish stopped, expired and
  unknown handles.

The feature is disabled by default. Enable it in
`Saved Games/DCS/Config/dcs-grpc.lua`:

```lua
autostart = true
auth.enabled = true
auth.tokens = {
  { client = "lso", token = "replace-with-a-secret" },
}
recoveryTelemetry.enabled = true
```

If `host` is not a loopback address, startup rejects enabled recovery telemetry
unless authentication is enabled. Handles are not secrets. The authenticated
`client` label owns the handle; the API token itself is never sent to Lua.

## Capture semantics

The first active registration starts one mission scheduler callback. The
callback stops rescheduling itself after the last registration is stopped or
expires. At every callback it:

1. reads one mission-relative `capture_time`;
2. increments one epoch-global `capture_tick`;
3. reads each active carrier once and reuses that exact observation for all
   recoveries on that carrier;
4. reads each aircraft and appends one snapshot to its recovery ring.

Aircraft and carrier observations therefore share a callback and source
timestamp, but they are sequential DCS API reads rather than a simulator-wide
transaction. A sequence is consumed on every executed callback even when a
unit is absent, has respawned under the same name with another ID, throws while
being read, or returns invalid/non-finite transform data. No snapshots are
synthesized for scheduler callbacks that never ran.

The default bounds are 600 snapshots and 30 seconds of DCS mission time per
recovery. Capacity overwrite and age expiration have separate counters and
loss reasons. A batch is limited to 100 snapshots. Repeating the same read is
safe and returns the same retained data; clients deduplicate with
`(source_epoch, recovery_handle, sequence)`.

A `Read` also purges ring entries whose sequence is `<= after_sequence`: that
cursor is an implicit acknowledgment of everything the client already has,
never of the batch the call is about to return, so retrying the same request
is still non-destructive. This keeps steady-state ring occupancy close to what
the client has actually consumed instead of the full configured capacity, and
counts these acknowledged evictions separately from genuine capacity overwrite
so `capacityOverflowCount` only reflects the ring actually running out of room
for unread data.

An empty ring is reported as the explicit range `0/0`. If a requested missing
range spans both age expiration and capacity overwrite, `loss_reason` is
`MIXED`; otherwise it identifies the single cause affecting that cursor.

`source_epoch` is a native random UUID created when the collector module loads.
Mission/Lua/server reloads create a new epoch; a network reconnect alone does
not. Clients must not join sequences across epochs.

## Lifecycle and diagnostics

A valid `Start` or `Read` renews the default 60-second DCS-time lease. Expired
and explicitly stopped handles retain a bounded 30-second tombstone without
transforms. `ReadRecoveryTelemetryResponse.lifecycle_status` distinguishes
active, unknown, expired, stopped and epoch-mismatch states.
While its tombstone is live, a handle cannot be reused; clients should create a
new opaque handle. This keeps delayed `Read` and `Stop` retries unambiguous.

The per-owner read token bucket runs in Rust before a request enters the bounded
mission IPC queue. It uses a process-monotonic clock, so pausing DCS mission time
does not prevent quota refill.

The batch reports retained sequence bounds, the effective cursor, loss reason,
capacity-overflow count, configured period/capacity/retention, last observed
source gap, estimated missed capture intervals, produced/invalid/served counts,
age expiration, high-water mark, active recovery/carrier counts and native
capture duration, read time, source age and batch counts/sizes when available.
These are technical facts only: Lua performs
no gate detection, interpolation, grading or hook/catch correlation.

The `diagnostics` block is refreshed at most once per
`recoveryTelemetry.diagnosticsIntervalSeconds` (default 1 second) per recovery
instead of on every batch; it is otherwise omitted from the response. Clients
that only consume `snapshots` are unaffected, and the last batch before a
client stops reading is at most one interval stale. Deduplicated observation
errors (`getUnitByName`/`getUnitId`/`exportRawTransform` failures) are kept in
a small bounded structure (128 most recent distinct errors) instead of growing
for the life of the mission.

## Validation scope

The deterministic harness covers ring behavior, pagination, retry, delayed
reads, real producer gaps, carrier sharing, same-name ID mismatch, stop and TTL
expiry, empty ranges, mixed loss provenance, idle restarts and scheduling
failure rollback. Scheduler callback exceptions are contained and retried with
rate-limited logging. These tests do not establish DCS FPS cost, scheduler
behavior or live gap percentiles. Those require a versioned DCS mission run
with matching DLL, Lua, mission and client artifacts.
