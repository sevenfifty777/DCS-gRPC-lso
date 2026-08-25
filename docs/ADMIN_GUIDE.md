# LSO Installation and Administration Guide

**Applies to:** crate version `0.2.0` plus Unreleased changes through 2026-08-25

**Required protocol:** sevenfifty777 DCS-gRPC `0.9.0`, commit
`11aea3484099c2dd21d41a53db2e510f6e5e84c5`

## 1. Prerequisites

### Runtime host

The simplest deployment runs `lso.exe` on the DCS server host. It needs:

- DCS World or DCS Dedicated Server;
- the pinned DCS-gRPC fork installed and running; and
- write permission to the chosen LSO output directory.

Rust is not required to run a prebuilt `lso.exe`. SQLite is bundled into the binary.

### Build host

Building from source requires Git, a stable Rust toolchain, and the native linker/toolchain expected
by the Rust target. On Windows MSVC targets, install the C++ workload from Visual Studio Build Tools.
The project does not declare a `rust-version`; use a current stable toolchain rather than relying on
an undocumented minimum.

## 2. Build LSO

From the repository root:

```powershell
rustc --version
cargo build --release
cargo test
cargo fmt -- --check
cargo clippy -- -D warnings
```

The release binary is `target\release\lso.exe`. Cargo obtains `dcs-grpc-stubs` from the exact Git
commit in `Cargo.toml`; do not replace it with an unpinned branch.

If dependencies changed and `cargo-audit` is already installed, also run:

```powershell
cargo audit
```

Copy the binary to a dedicated runtime directory, for example `C:\LSO`. The image assets are
embedded at compile time, so the runtime does not need the repository's `img` directory.

## 3. Install and start DCS-gRPC

Use the release package built from the pinned fork commit. Extract the package into the DCS Saved
Games server directory. The resulting layout includes:

```text
<Saved Games DCS>\
|-- Mods\tech\DCS-gRPC\
|-- Scripts\DCS-gRPC\
`-- Scripts\Hooks\DCS-gRPC.lua
```

The bundled reference copy at `docs/DCS-gRPC-0.9.0/Docs/DCS-gRPC/README.md` contains the full server
instructions. In particular, DCS's installation `Scripts\MissionScripting.lua` must load:

```lua
dofile(lfs.writedir()..[[Scripts\DCS-gRPC\grpc-mission.lua]])
```

To start DCS-gRPC regardless of mission scripting, create
`<Saved Games DCS>\Config\dcs-grpc.lua` with:

```lua
autostart = true
```

The relevant defaults are:

```lua
host = "127.0.0.1"
port = 50051
```

After DCS loads, check `<Saved Games DCS>\Logs\dcs.log` for `[GRPC]` entries or the DCS-gRPC log,
then verify the TCP listener:

```powershell
Test-NetConnection -ComputerName 127.0.0.1 -Port 50051
```

This confirms TCP reachability, not API compatibility. A live LSO connection is the next check.

### Running LSO on another host

Change DCS-gRPC's `host` only when remote access is required, then restrict port 50051 with a host
firewall or private network/VPN. Avoid exposing an unauthenticated plaintext gRPC endpoint to the
public internet.

Point LSO to it with:

```powershell
.\lso.exe run --uri http://192.168.1.50:50051 -o C:\LSO\recordings
```

## 4. First run

Create the output directory before starting LSO; the application creates `lso.db` but not a missing
parent directory.

```powershell
New-Item -ItemType Directory -Force C:\LSO\recordings
.\lso.exe run -o C:\LSO\recordings
```

Live mode:

1. Opens or migrates `C:\LSO\recordings\lso.db`.
2. Connects to DCS-gRPC at `http://127.0.0.1:50051` and retries transient failures with exponential
   backoff, never waiting more than 30 seconds between retries.
3. Discovers supported active carriers and aircraft and listens for later Birth events.
4. Monitors each carrier/aircraft pair and records recognized passes.
5. Prints the current-session greenie board when Ctrl-C triggers graceful shutdown.

The INFO log confirms connection. Per-pair task details are primarily visible at DEBUG/TRACE level.

## 5. CLI reference

Generated help is authoritative:

```powershell
.\lso.exe --help
.\lso.exe run --help
.\lso.exe file --help
```

Global options must precede the subcommand:

| Option | Meaning |
|---|---|
| `-v`, `--verbose...` | Increase logging: once for DEBUG, twice for TRACE |
| `--color` | Enable ANSI-colored logging |

### `run`

| Option | Default | Meaning |
|---|---|---|
| `-o, --out-dir <PATH>` | `.` | Output directory for charts, JSON, optional ACMI, and `lso.db` |
| `--uri <URI>` | `http://127.0.0.1:50051` | DCS-gRPC endpoint |
| `--discord-webhook <URL>` | disabled | Discord webhook for completed-pass posts |
| `--discord-users <FILE>` | disabled | JSON map of DCS pilot names to Discord numeric user IDs |
| `--ki` | false | Include AI-controlled aircraft |
| `--no-acmi` | false | Do not save or attach ACMI; charts, JSON, and database remain enabled |
| `--web-port <PORT>` | disabled | Start the HTTP greenie board on the selected port |

Examples:

```powershell
# Normal live recording
.\lso.exe run -o C:\LSO\recordings

# Debug logging and no ACMI
.\lso.exe -v run -o C:\LSO\recordings --no-acmi

# HTTP board on port 8080
.\lso.exe run -o C:\LSO\recordings --web-port 8080

# Remote DCS-gRPC
.\lso.exe run -o C:\LSO\recordings --uri http://192.168.1.50:50051
```

### `file`

```powershell
.\lso.exe file C:\LSO\recordings\LSO-20260825-031018-Pilot.zip.acmi
```

Only compressed ACMI files created by LSO contain the metadata expected by replay mode. Replay
writes the final-approach PNG to the current working directory. It does not write the pattern PNG,
JSON, database row, or Discord post, and it does not preserve the input file's directory as the
output directory.

## 6. Detection and pass lifecycle

The detector samples every two seconds and begins recording when a supported aircraft is between
200 m and 3.5 nm from the carrier and no higher than 1,100 ft MSL. It applies no heading/quadrant
check so the overhead pattern can be captured.

After detection, LSO samples at 100 ms and records both the BRC pattern and angled-deck final. Gate
samples are captured inbound at 3/4, 1/2, and 1/4 nm below 500 ft above deck. Groove entry also
requires no more than 300 ft above deck and lineup within 10 degrees.

Recognized saved outcomes are recovered (`Wire #N` or `Landed`), bolter, pilot waveoff, and hook-up
qualification bolter (`Qualif Bolter`). Tracks that never descend below 100 m MSL or finish with an
Unknown outcome are discarded.

## 7. Output artifacts

Each saved live pass uses this base name:

```text
LSO-YYYYMMDD-HHMMSS-<PilotAsciiAlphanumeric>
```

| Artifact | Behavior |
|---|---|
| `<base>.png` | Final-approach side/top trap sheet with grade, outcome, and gate labels |
| `<base>-pattern.png` | Overhead pattern chart in the carrier BRC frame |
| `<base>.json` | Structured final-approach report |
| `<base>.zip.acmi` | Compressed carrier/aircraft recording; omitted with `--no-acmi` |
| `lso.db` | Shared persistent database; one row added per saved pass |

### JSON shape

```json
{
  "pilot_name": "Viper",
  "grading": {
    "Recovered": {
      "cable": 3,
      "cable_estimated": 3
    }
  },
  "pass_grade": "Ok",
  "dcs_grading": "OK 3 WIRE# 3",
  "gate_deviations": {
    "at_three_quarter_nm": {
      "gs_deviation_deg": 0.16,
      "lineup_deg": -0.05,
      "gs_deviation_ft": 12.4,
      "lineup_ft": -3.1
    },
    "at_half_nm": null,
    "at_quarter_nm": null
  },
  "datums": [
    { "time": 123.4, "x": 1389.2, "y": -3.1, "aoa": 8.1, "alt": 84.7 }
  ],
  "mission_datetime": "2024-06-15T18:25:04Z"
}
```

`dcs_grading` is omitted when absent. `mission_datetime` is omitted when its live gRPC query fails.
`pass_grade` uses Rust enum names (`Unicorn`, `Ok`, `OkParentheses`, `NoGrade`, `Cut`, `Bolter`,
`WaveoffPilot`), not display labels. `grading` may also be `"Bolter"`, `"WaveoffPilot"`, or an
`IntentionalBolter` object. Saved live JSON does not include pattern datums, map, UCID, grade points,
display outcome, wind, or groove time.

## 8. Grading

The current display labels and points are:

| Label | Points | Summary |
|---|---:|---|
| `_OK_` | 5.0 | Base `OK`, wire 3, groove time 15.0-18.99 s |
| `OK` | 4.0 | GS magnitude below 0.5 degrees and lineup below 1 degree |
| `(OK)` | 3.0 | GS reaches 0.5 degrees or lineup reaches 1 degree, below significant thresholds |
| `--` | 2.0 | GS reaches 1 degree or lineup reaches 2 degrees |
| `C` | 0.0 | Quarter-nm GS below -2.5 degrees |
| `B` | 2.5 | Bolter |
| `WO` | 1.0 | Pilot waveoff |

Read [GRADING_REFERENCE.md](GRADING_REFERENCE.md) for exact inclusive boundaries, missing-gate
behavior, qualification bolters, AoA bands, and limitations.

## 9. Supported units

### Aircraft

| Display family | Accepted DCS type names | On-speed AoA interval |
|---|---|---|
| F/A-18C | `FA-18C_hornet` | `> 7.4` and `< 8.8` degrees |
| F-14A | `F-14A-135-GR`, `F-14A-135-GR-Early`, `F-14A-95-GR` | `> 10.2` and `< 11.1` degrees |
| F-14B | `F-14B`, `F-14A/B` | `> 10.2` and `< 11.1` degrees |
| F-14B(U) | `F-14B(U)`, `F-14BU` | `> 10.2` and `< 11.1` degrees |
| VNAO T-45C | `T-45` | `> 6.5` and `< 7.5` degrees |

All use a nominal 3.5-degree glide slope.

### Carriers

| Geometry | Accepted DCS type names |
|---|---|
| Nimitz | `CVN_71`, `CVN_72`, `CVN_73`, `CVN_75`, `Stennis` |
| Forrestal | `Forrestal` |

`Stennis` is DCS's type name for CVN-74. Unsupported aircraft or ships are ignored. The presence of
an internal numeric aircraft ID is not sufficient for support; the type also needs an
`AirplaneInfo` entry.

## 10. Web greenie board

Start it with:

```powershell
.\lso.exe run -o C:\LSO\recordings --web-port 8080
```

Routes:

| Route | Response |
|---|---|
| `GET /` | Self-contained HTML table, refreshed every 10 seconds |
| `GET /api/passes` | All database rows newest first as JSON |

The listener is `0.0.0.0:<port>`, not loopback-only. It has no authentication, authorization, TLS,
pagination, or retention limit. Restrict it with Windows Firewall and do not expose it directly to
the internet.

Example API object:

```json
{
  "id": 42,
  "timestamp": "LSO-20260825-031018-Viper",
  "pilot_name": "Viper",
  "pilot_ucid": "0123456789abcdef",
  "aircraft_id": 1,
  "pass_grade": "OK",
  "wire": 3,
  "dcs_grading": "OK 3 WIRE# 3",
  "aircraft_type": "F/A-18C Hornet",
  "map_name": "Caucasus",
  "lso_notes": "Wire 3",
  "grade_date": "2026-08-25 01:10:18",
  "grade_points": 4.0,
  "mission_datetime": "2024-06-15T18:25:04Z",
  "outcome": "Wire #3"
}
```

Nullable fields can be `null`; migrated historical rows may have empty strings or zero defaults.
The API currently turns database/task failures into `[]`, so an unexpectedly empty result should be
checked against application logs and the database itself.

### Reverse proxy topology

The proxy and LSO cannot own the same host port. A normal same-host topology is:

```text
HTTPS proxy :443 -> LSO HTTP 127.0.0.1:8080
```

LSO currently binds `0.0.0.0`, so use the firewall to limit direct access. If a proxy must listen on
port 8080, give LSO another port such as 8081 and proxy to that. Changing only a bind address cannot
resolve two processes competing for one port.

## 11. Discord

Start Discord delivery with a webhook URL:

```powershell
.\lso.exe run -o C:\LSO\recordings `
  --discord-webhook "https://discord.com/api/webhooks/YOUR_ID/YOUR_TOKEN"
```

Treat the full URL as a secret: do not commit it, paste it into issue reports, or expose service
configuration/logs containing it. Rotate it in Discord if disclosed.

The embed includes aircraft, map, UTC time, optional mission time, pilot, grade/points, outcome,
gate deviations, DCS LSO notation and its English translation, optional wind, and optional groove
time. It attaches both PNGs and, unless `--no-acmi` is used, the ACMI. It does not attach JSON.

For mentions, create a local JSON file:

```json
{
  "Viper": 123456789012345678,
  "Ghost": 234567890123456789
}
```

Pass it with `--discord-users C:\LSO\users.json`. Names not present in the map remain plain text.

## 12. SQLite administration

The database path is `<out-dir>\lso.db`. Current schema:

```sql
CREATE TABLE passes (
    id                 INTEGER PRIMARY KEY AUTOINCREMENT,
    timestamp          TEXT    NOT NULL,
    pilot_name         TEXT    NOT NULL,
    pilot_ucid         TEXT,
    aircraft_id        INTEGER,
    pass_grade         TEXT    NOT NULL,
    wire               INTEGER,
    dcs_grading        TEXT,
    aircraft_type      TEXT,
    map_name           TEXT,
    grade_date         TEXT    NOT NULL DEFAULT '',
    grade_points       REAL    NOT NULL DEFAULT 0.0,
    mission_datetime   TEXT    NOT NULL DEFAULT '',
    outcome            TEXT    NOT NULL DEFAULT ''
);
```

`pass_grade` stores display labels such as `OK` and `(OK)`. `aircraft_type` stores a display name
such as `F/A-18C Hornet`, `F-14A/B`, or `F-14B(U)`, not necessarily the raw DCS type name.

Inspect recent rows with an installed SQLite CLI:

```powershell
sqlite3 C:\LSO\recordings\lso.db "SELECT * FROM passes ORDER BY id DESC LIMIT 20;"
```

Export in PowerShell without Bash line-continuation syntax:

```powershell
sqlite3 -csv -header C:\LSO\recordings\lso.db "SELECT * FROM passes ORDER BY id;" |
  Set-Content -Encoding utf8 .\passes.csv
```

Back up the database while LSO is stopped, or use SQLite's backup command for a live database:

```powershell
New-Item -ItemType Directory -Force C:\LSO\backup
sqlite3 C:\LSO\recordings\lso.db ".backup 'C:/LSO/backup/lso-backup.db'"
```

Existing databases are migrated additively at startup. Review historical rows because new columns
use empty/zero defaults rather than reconstructed values.

## 13. Logging and diagnostics

```powershell
# DEBUG
.\lso.exe -v run -o C:\LSO\recordings

# TRACE
.\lso.exe -vv run -o C:\LSO\recordings

# PowerShell combined stream capture
.\lso.exe run -o C:\LSO\recordings *> C:\LSO\lso.log
```

Do not publish logs without checking them for pilot names, UCIDs, paths, network addresses, DCS
grading text, and command-line configuration. The code does not intentionally log the webhook URL,
but process/service configuration may expose it separately.

## 14. Windows service example

NSSM is one possible service wrapper. Install it separately, then configure the executable,
parameters, working directory, and logs:

```powershell
nssm install LSO "C:\LSO\lso.exe"
nssm set LSO AppParameters "run -o C:\LSO\recordings --web-port 8080"
nssm set LSO AppDirectory "C:\LSO"
nssm set LSO AppStdout "C:\LSO\lso.log"
nssm set LSO AppStderr "C:\LSO\lso.log"
nssm set LSO AppRotateFiles 1
nssm set LSO AppRotateBytes 10485760
nssm start LSO
nssm status LSO
```

If you add a Discord webhook to service parameters, restrict access to the service configuration
and rotate the webhook after any disclosure. Starting LSO before the mission is acceptable because
gRPC failures are retried.

## 15. Troubleshooting

### DCS-gRPC connection failures

- Confirm DCS and the mission environment are running.
- Confirm DCS-gRPC autostart or mission startup is configured.
- Check DCS `[GRPC]` logs and `Test-NetConnection` to the same host/port used in `--uri`.
- Confirm the deployed server matches the pinned 0.9.0 fork commit.
- Use `-v` to see retry reasons; LSO normally keeps retrying rather than exiting.

### No saved passes

- Confirm both aircraft and carrier type strings are supported.
- Add `--ki` for AI aircraft.
- Use TRACE logging to see candidate and detector decisions.
- A pass is intentionally discarded if it never goes below 100 m MSL or never resolves beyond
  `Unknown`.
- The output directory must already exist and be writable.

### Unexpected waveoff or bolter

The outcome uses groove entry, minimum distance, hook state, touchdown events, and a 150 m moving-away
threshold. Inspect the LSO ACMI and trace log; replay can validate geometry but does not reproduce all
live gRPC events or outputs.

### Web page stays on Loading

Open `http://localhost:<port>/api/passes`. If it returns `[]`, inspect `lso.db` and logs; the handler
also returns an empty array on database/task failure. If the endpoint is unreachable, check the
listener and firewall.

### Port bind error on Windows

The log line saying the dashboard is listening is emitted immediately before the actual bind and is
not proof that binding succeeded. Find the owner of the selected port:

```powershell
netstat -ano | findstr :8080
Get-Process -Id <PID>
```

If no process appears, inspect excluded TCP port ranges. Give LSO and any reverse proxy separate
ports; switching between `0.0.0.0` and `127.0.0.1` alone does not solve a same-port collision.

### Discord post fails

- Verify the webhook has not been deleted or rotated.
- Check attachment sizes; live posts may contain two PNGs and one ACMI.
- Retry with `--no-acmi` to isolate an oversized ACMI attachment.
- Check wind/UCID/mission-time warnings separately; those lookups are non-fatal, while webhook
  execution failure is fatal to the recording task and then retried by the outer run loop.

### Replay output is not beside the ACMI

This is expected. `lso file` writes the approach PNG to the current working directory. Change to the
desired directory before invoking it.
