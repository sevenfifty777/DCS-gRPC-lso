# Data contracts and migrations

## JSON recovery report

New live reports use `schema_version: 3`. The legacy top-level fields (`pilot_name`, `grading`,
`pass_grade`, `dcs_grading`, `gate_deviations`, `datums`, `mission_datetime`) remain present with
compatible meanings. Additive fields include:

- `recovery_id`, aircraft/carrier IDs and types, recovery mode, session and generation;
- `pilot_kind` (`human` or `ai`), never the UCID;
- approach/final grade, optional points, outcome, cause, confidence, completeness and grading source;
- backward-compatible multiple causes: legacy `cause` remains the primary cause and `causes`
  contains `{ primary, secondary[] }`. Position buffer loss, telemetry gaps, invalid telemetry,
  insufficient gates and unconfirmed arrest are retained independently; hook/event truncation is
  diagnostic only;
- intended/nearest spot, spot score and informational spot-zone observation;
- estimated/DCS wire, divergence and primary display provenance;
- gate quality plus raw/corrected telemetry diagnostics;
- `trajectory_deviations`: continuous GS/lineup series from groove entry to touchdown, additive to
  `gate_deviations`. Its worst amplitude feeds `PassGrade` alongside the three gates; empty for a
  pass that never entered the groove;
- optional `wind_heading_deg`/`wind_speed_mps`: contextual wind at the carrier's position, queried
  once per recovery. Absent when the query fails or in `--positions-only`. Never affects
  `pass_grade`/`grade_points`;
- ordered event evidence, raw hook observation and first-contact horizontal speed.
- `event_correlation`, with stream status (`available`, `unavailable` or intentionally `disabled`),
  detailed end/failure information, whether outcome evidence preceded the outage, and an independent
  outcome-confirmation decision. `event_stream_unavailable` is a secondary cause and never changes
  positional completeness;
- recording/completion times, Git commit/dirty state and DCS-gRPC client/server compatibility;
- explicit grading availability and live telemetry health;
- timestamped hook samples with success/timeout/error/stale state and pre-touch provenance;
- continuous hook-plane wire crossings, estimate confidence and reason;
- per-recovery frequency, gap/source-age and position-read latency percentiles;
- `acquisition_source`, with `source_buffered_batch_v1` as the default and
  `paired_unary_polling_v1` as the explicit rollback;
- optional `recovery_telemetry` source epoch, sequence/tick range, batch/sample counts, invalid and
  lost snapshots, overflow/retention counters, missed capture intervals, capacity and configured
  source period. Absence means a unary or legacy producer, never successful buffering.

When detector suspension is enabled, `detector_suspension_scope` is `same_aircraft`: detectors for
other aircraft continue polling so simultaneous recoveries remain discoverable.

`baseline_manifest` is a typed, optional operator-supplied manifest. It carries DCS build,
mission/module versions and SHA-256 values for the mission and deployed DCS-gRPC DLL/Lua files.
Unknown values remain absent rather than being inferred. When a manifest is supplied, unknown keys,
an empty object and malformed SHA-256 strings are rejected.

Individual JSON, PNG, ACMI, Discord payloads and public logs must never contain an UCID. The
deterministic recovery ID contains only session/generation/unit IDs and DCS time.

## SQLite

`lso.db` is private dynamic state. It is neither opened nor created in `--positions-only` mode, and
the dashboard is disabled in that mode. `/api/passes` may expose `pilot_ucid` because the dashboard
is a loopback-only private endpoint in phase 1.

Migrations are additive and recorded in `schema_migrations`:

| Version | Content |
|---:|---|
| 1 | historical `passes` table |
| 2 | recovery/session/carrier/completeness/wire provenance fields and unique recovery index |
| 3 | `points_awarded`, separating a real zero from no points |
| 4 | separate intended spot, actual nearest active spot and distance to intended spot |
| 5 | scored-segment gap, telemetry health, wire-estimate confidence and grading availability |
| 6 | JSON-encoded secondary diagnostic causes; the legacy `cause` column remains primary |

Startup inspects `PRAGMA table_info(passes)` before each `ALTER TABLE`. Unexpected migration errors
are returned; they are never swallowed as duplicate-column errors. Existing rows are preserved.
`points_awarded` defaults true for legacy rows because historical rows always stored a numeric
grade. New incomplete rows store zero in the legacy non-null `grade_points` column for SQL
compatibility and `points_awarded = false`; API serialization returns the latter so clients do not
display a fabricated zero-point score.

`recovery_id` has a unique partial index. Inserts are `INSERT OR IGNORE` and report whether a new row
was created. This controls session-log and Discord idempotence. File publication is atomic
create-if-absent: JSON ownership identifies the winning producer, and only that producer continues
with ACMI, SQLite, rendering and Discord. Existing artifacts are never replaced.

`lso_dirty` covers modified, staged and deleted tracked files. Untracked files deliberately do not
participate; this keeps the definition aligned with Cargo rebuild triggers and avoids watching
`target/` or creating rebuild loops.

## Compatibility policy

- do not rename or remove existing JSON or SQLite fields in this phase;
- add new nullable/defaulted columns only;
- old databases are migrated in place and covered by a legacy-schema test;
- dashboard consumers may ignore unknown fields;
- absence of a new field means "legacy/unknown", not a favourable default;
- protobuf and DCS-gRPC must match the `0.10.0` source-buffered contract. During local validation the
  stubs resolve from the sibling server checkout; publication requires replacing that path with the
  reviewed immutable server commit/tag without changing generated wire types.

Before any future schema or destructive cleanup, retain fixtures for the oldest database and JSON
actually found in production and test both forward migration and dashboard display.
