# Mission Date and Time - Implementation Record

**Status:** Implemented across commits `3e50c67` and `4d74c8a`

**Protocol baseline:** official DCS-gRPC tag `v0.9.0` at the revision recorded in `Cargo.lock`

## Implemented flow

At the end of a recognized pass, LSO calls `MissionService.GetScenarioCurrentTime` through
[`src/client/mission_client.rs`](../src/client/mission_client.rs). Failure is non-fatal: LSO logs a
warning and uses an empty string.

The value is then written to:

- `mission_datetime` in the recovery JSON when non-empty;
- the SQLite `passes.mission_datetime` column;
- the web board's Mission Time column and `/api/passes` payload; and
- the Discord `Mission Date/Time` field when non-empty.

[`src/db.rs`](../src/db.rs) creates the column for new databases and applies an additive migration
with an empty-string default for older databases.

## Time fields are distinct

| Field | Source | Format/use |
|---|---|---|
| Filename timestamp | Local wall clock at recording start | `YYYYMMDD-HHMMSS` |
| Discord `Date / Time (UTC)` | UTC wall clock at recording start | RFC 3339 |
| Database `grade_date` | UTC wall clock | `YYYY-MM-DD HH:MM:SS` |
| `mission_datetime` | DCS scenario clock queried at completion | DCS-gRPC formatted string |

Mission time should not be treated as UTC and may intentionally represent another historical date
or accelerated scenario time.

## Validation boundary

Compilation and unit tests validate the data path and schema types. Populating a meaningful mission
time still requires a live DCS/DCS-gRPC integration test because the value originates in the running
scenario.
