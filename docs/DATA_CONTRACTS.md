# Data contracts and migrations

## JSON recovery report

New live reports use `schema_version: 8`. The legacy top-level fields (`pilot_name`, `grading`,
`pass_grade`, `dcs_grading`, `gate_deviations`, `datums`, `mission_datetime`) remain present with
compatible meanings. Additive fields include:

- `recovery_id`, aircraft/carrier IDs and types, recovery mode, session and generation;
- `pilot_kind` (`human` or `ai`), never the UCID;
- approach/final grade, optional points, outcome, cause, confidence, completeness and grading source;
- intended/nearest spot, spot score and informational spot-zone observation;
- estimated/DCS wire, divergence and primary display provenance. A DCS-reported wire is
  authoritative; the independent estimate is used only when DCS does not provide a wire;
- gate quality plus raw/corrected telemetry diagnostics;
- ordered event evidence, raw hook observation and first-contact horizontal speed.
- recording/completion times and LSO/DCS-gRPC version evidence;
- explicit grading availability and live telemetry health;
- timestamped hook samples with success/timeout/error/stale state and pre-touch provenance;
- physical external-model hook samples record `hook_observation.evidence_source` as
  `external_draw_argument` and persist the exact requested `hook_observation.draw_argument`:
  `1305` for supported F-14 variants and `25` for F/A-18C and T-45. A null argument with
  `evidence_source: not_requested` means no external draw argument was requested. Schema 5 and
  older reports do not contain this provenance, so their argument number must not be claimed from
  the saved artifact alone;
- ownship-only `LoGetMechInfo().hook` diagnostics with raw `status_value` and `value`, DCS model
  time, aircraft identity checks and explicit unavailable/error states. These fields are evidence
  collection only until live F-14, F/A-18C and T-45 polarity is validated;
- finite hook-plane wire crossings plus the correlated hook-deflection/recovery timestamps,
  correlation lag, estimate confidence and reason. An estimate requires a stable hook-down value,
  a sharp transient near touchdown, recovery to the down value within eight seconds, and a valid
  cable crossing no more than 200 ms before that transient. A stable hook-up value or an
  incomplete transient produces no estimate;
- schema 8: `hook_state` (commanded hook state `up`/`down`/`unknown`), `arrest_evidence`
  (`dcs_wire`, `hook_transient`, `kinematic`, `unconfirmed`, `none`), `arrest_kinematics`
  (confirmation, reason, reference/slow-since times, held seconds, minimum relative speed),
  `dcs_lso` (parsed `GRADE:` token, wire and waveoff call), `hook_observation.baseline_*`
  (state, mean value, sample count, window and reason), per-datum `raw_carrier_velocity` and,
  when DCS-gRPC 0.9.2 provides them, `queue_wait_ms`, `lua_exec_ms` and `queue_depth`.
  `approach_grade` is omitted when the pass is incomplete.

LSO-written ACMI files carry the raw hook draw argument as the custom Tacview property `LSOHook`
on each aircraft frame; `lso file` and the replay fixtures read it back so the hook classifier and
wire estimator behave as they did live. Older ACMI files without the property replay with no hook
evidence.

Individual JSON, PNG, ACMI, Discord payloads and public logs must never contain an UCID. The
deterministic recovery ID contains only session/generation/unit IDs and DCS time.

## SQLite

`lso.db` is private dynamic state. Its only external reader is the DCS Web Dashboard, which opens
the file read-only (the database uses WAL journaling so reads never block inserts), tolerates
columns it does not know, uses `pilot_ucid` solely to group passes by pilot, and never serialises
it. LSO 0.4.0 no longer serves the database over HTTP.

Migrations are additive and recorded in `schema_migrations`:

| Version | Content |
|---:|---|
| 1 | historical `passes` table |
| 2 | recovery/session/carrier/completeness/wire provenance fields and unique recovery index |
| 3 | `points_awarded`, separating a real zero from no points |
| 4 | separate intended spot, actual nearest active spot and distance to intended spot |
| 5 | scored-segment gap, telemetry health, wire-estimate confidence and grading availability |
| 6 | `arrest_evidence` and commanded `hook_state` |

Startup inspects `PRAGMA table_info(passes)` before each `ALTER TABLE`. Unexpected migration errors
are returned; they are never swallowed as duplicate-column errors. Existing rows are preserved.
`points_awarded` defaults true for legacy rows because historical rows always stored a numeric
grade. New incomplete rows store zero in the legacy non-null `grade_points` column for SQL
compatibility and `points_awarded = false`; API serialization returns the latter so clients do not
display a fabricated zero-point score.

`recovery_id` has a unique partial index. Inserts are `INSERT OR IGNORE` and report whether a new row
was created. This controls session-log and Discord idempotence.

## Compatibility policy

- do not rename or remove existing JSON or SQLite fields in this phase;
- add new nullable/defaulted columns only;
- old databases are migrated in place and covered by a legacy-schema test;
- dashboard consumers may ignore unknown fields;
- absence of a new field means "legacy/unknown", not a favourable default;
- the local hook-mechanization integration uses the sibling `rust-server/stubs` checkout; replace
  it with the exact DCS-gRPC release tag that publishes `GetOwnshipHookState` before packaging.

Before any future schema or destructive cleanup, retain fixtures for the oldest database and JSON
actually found in production and test both forward migration and dashboard display.
