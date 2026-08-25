# Add In-Mission Date/Time to Recovery Records

Currently, all timestamps in the system use the DCS **server's wall-clock time** (via `OffsetDateTime::now_local()`). The mission editor can set any date/time (e.g. a 1980s Cold War scenario at 0600 local), and that information is lost in the trapsheet.

This change adds the **in-mission date/time** (the simulated clock from the DCS mission) alongside the existing server time, using the `GetScenarioCurrentTime` gRPC API that's already available in the v0.8.1 stubs.

## Proposed Changes

### gRPC Client Layer

#### [MODIFY] [mission_client.rs](file:///c:/Users/thierry/Documents/GitHub/sevenfifty777/DCS-gRPC-lso/src/client/mission_client.rs)

Add a `get_scenario_current_time()` method, mirroring the existing `get_scenario_start_time()`:

```rust
pub async fn get_scenario_current_time(&mut self) -> Result<String, Status> {
    let res = self
        .svc
        .get_scenario_current_time(mission::v0::GetScenarioCurrentTimeRequest {})
        .await?
        .into_inner();
    Ok(res.datetime)
}
```

---

### Database Layer

#### [MODIFY] [db.rs](file:///c:/Users/thierry/Documents/GitHub/sevenfifty777/DCS-gRPC-lso/src/db.rs)

- Add `mission_datetime: String` field to `DbPass` (the insertion struct)
- Add `mission_datetime: String` field to `StoredPass` (the query/JSON struct)
- Add `mission_datetime TEXT NOT NULL DEFAULT ''` to the `CREATE TABLE` schema
- Add an `ALTER TABLE passes ADD COLUMN mission_datetime TEXT NOT NULL DEFAULT '';` migration for existing databases
- Update the `INSERT` and `SELECT` statements to include the new column

---

### Recovery Recording

#### [MODIFY] [record_recovery.rs](file:///c:/Users/thierry/Documents/GitHub/sevenfifty777/DCS-gRPC-lso/src/tasks/record_recovery.rs)

At pass completion (around line 463, after the track is finalised and just before the DB insert), call `mission.get_scenario_current_time()` to capture the in-mission datetime at the moment of recovery. This is a non-fatal query — if it fails, fall back to an empty string (same pattern as the wind query).

```rust
let mission_datetime: String = match mission.get_scenario_current_time().await {
    Ok(dt) => dt,
    Err(err) => {
        tracing::warn!(?err, "failed to query in-mission datetime");
        String::new()
    }
};
```

Pass the value into the `DbPass` struct.

> [!NOTE]
> We query `GetScenarioCurrentTime` at pass-completion rather than computing `scenario_start_time + plane.time`. The gRPC call accounts for any time acceleration the server might be running, and gives us the formatted ISO 8601 string directly.

---

### Web Dashboard

#### [MODIFY] [web.rs](file:///c:/Users/thierry/Documents/GitHub/sevenfifty777/DCS-gRPC-lso/src/web.rs)

- Add a `Mission Time` column to the `<thead>` (after `Grade Date`)
- Render `p.mission_datetime` in the corresponding `<td>` row
- Update the `colspan` on the empty/loading placeholder from `11` to `12`

---

### Discord Embed

#### [MODIFY] [record_recovery.rs](file:///c:/Users/thierry/Documents/GitHub/sevenfifty777/DCS-gRPC-lso/src/tasks/record_recovery.rs)

Add a `Mission Date/Time` field to the Discord embed (after the existing `Date / Time (UTC)` field), but only when the value is non-empty:

```rust
if !mission_datetime.is_empty() {
    embed = embed.field("Mission Date/Time", mission_datetime.as_str(), false);
}
```

---

### JSON Report

#### [MODIFY] [record_recovery.rs](file:///c:/Users/thierry/Documents/GitHub/sevenfifty777/DCS-gRPC-lso/src/tasks/record_recovery.rs)

Add `mission_datetime` to the `RecoveryReport` struct so it's persisted in the `.json` sidecar file alongside the ACMI and PNG chart.

## Summary of Touched Files

| File | Change |
|------|--------|
| [mission_client.rs](file:///c:/Users/thierry/Documents/GitHub/sevenfifty777/DCS-gRPC-lso/src/client/mission_client.rs) | Add `get_scenario_current_time()` |
| [db.rs](file:///c:/Users/thierry/Documents/GitHub/sevenfifty777/DCS-gRPC-lso/src/db.rs) | New column + migration |
| [record_recovery.rs](file:///c:/Users/thierry/Documents/GitHub/sevenfifty777/DCS-gRPC-lso/src/tasks/record_recovery.rs) | Query + pass to DB + Discord + JSON |
| [web.rs](file:///c:/Users/thierry/Documents/GitHub/sevenfifty777/DCS-gRPC-lso/src/web.rs) | New dashboard column |

## Verification Plan

### Automated Tests
```bash
cargo build
cargo test
```

### Manual Verification
- Run against a live DCS server and verify:
  - The `Mission Time` column appears on the web dashboard
  - The Discord embed shows the `Mission Date/Time` field
  - The `.json` report includes `mission_datetime`
  - The SQLite `passes` table has the `mission_datetime` column populated
