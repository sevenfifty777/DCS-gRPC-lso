# LSO — Installation & Administration Guide

> **Applies to:** LSO v0.2.0 (post Tier 1/2/3 improvements)  
> **Required:** DCS-gRPC server rev 0.8.1

---

## Table of Contents

1. [Prerequisites](#1-prerequisites)
2. [Building from Source](#2-building-from-source)
3. [DCS-gRPC Server Setup](#3-dcs-grpc-server-setup)
4. [First Run](#4-first-run)
5. [All CLI Options](#5-all-cli-options)
6. [Output Files](#6-output-files)
7. [Web Greenie Board](#7-web-greenie-board)
8. [Discord Integration](#8-discord-integration)
9. [Session Greenie Board (Terminal)](#9-session-greenie-board-terminal)
10. [Offline ACMI Replay](#10-offline-acmi-replay)
11. [Supported Carriers & Aircraft](#11-supported-carriers--aircraft)
12. [Pass Grading Reference](#12-pass-grading-reference)
13. [Database Administration](#13-database-administration)
14. [Logging & Diagnostics](#14-logging--diagnostics)
15. [Running as a Windows Service](#15-running-as-a-windows-service)
16. [Troubleshooting](#16-troubleshooting)

---

## 1. Prerequisites

### Build machine (your PC)

You only need the Rust toolchain **to compile** LSO. Once you have `lso.exe` the Rust toolchain is no longer needed anywhere.

| Requirement | Version | Notes |
|---|---|---|
| Rust toolchain | stable ≥ 1.75 | Install via [rustup.rs](https://rustup.rs) — build machine only |
| Visual C++ Build Tools | 2019 or 2022 | Required by `rusqlite` bundled feature on Windows — build machine only |
| Git | any | To clone the repo — build machine only |

### DCS server machine

This is the machine running DCS World. Both the DCS-gRPC Lua module and `lso.exe` run here.
**No Rust installation is required.** The compiled binary is fully self-contained — SQLite is statically linked inside it.

| Requirement | Version | Notes |
|---|---|---|
| DCS World | 2.9+ | Server or client install |
| DCS-gRPC Lua module | rev 0.8.1 | Installed inside DCS Saved Games — see [Section 3](#3-dcs-grpc-server-setup) |
| `lso.exe` | built from source | Copy from build machine — the only file needed |

---

## 2. Building from Source

Do this **on your build/dev PC**, not on the DCS server.

LSO lives as a sub-crate inside the DCS-gRPC workspace repository. If you already have the repo cloned locally (e.g. at `C:\Users\<you>\Documents\GitHub\DCS-gRPC`), just build from there — no re-cloning needed.

```powershell
# Navigate to the lso sub-crate inside your existing local repo
cd C:\Users\<you>\Documents\GitHub\DCS-gRPC\lso

# Build the lso binary (release mode — smaller, faster)
cargo build --release

# The compiled binary is at:
#   target\release\lso.exe
```

> **If you don't have the repo yet**, clone it first:
> ```powershell
> git clone https://github.com/DCS-gRPC/rust-server.git
> cd rust-server\lso
> cargo build --release
> ```
> Note: this is the upstream public repo. If you plan to keep your changes, push to your own fork or remote instead.

The build downloads and compiles all Rust dependencies automatically. Expect 3–5 minutes on first build.

### Transfer to the DCS server

Copy only `lso.exe` to the DCS server — no other files from the build tree are needed:

```powershell
# Example: copy to a dedicated folder on the DCS server
Copy-Item target\release\lso.exe \\DCS-SERVER\C$\LSO\lso.exe
```

Or use any file transfer method (USB, SCP, shared folder, etc.).

**Security audit (run after every build if dependencies changed):**
```powershell
cargo install cargo-audit
cargo audit
```

---

## 3. DCS-gRPC Server Setup

Do this **on the DCS server machine**.

LSO connects to DCS World via the DCS-gRPC Lua module, which runs inside the DCS process and exposes a gRPC endpoint on `localhost:50051`. LSO and DCS-gRPC must be on the **same machine** — LSO connects to `127.0.0.1:50051` by default.

### 3.1 Install the DCS-gRPC Lua module

1. Download the DCS-gRPC release matching `rev = 0.8.1` from:  
   <https://github.com/DCS-gRPC/rust-server/releases>

2. Copy the files:
   ```
   DCS-gRPC/           → <DCS Saved Games>\Scripts\Hooks\
   DCS-gRPC.lua        → <DCS Saved Games>\Scripts\Hooks\
   ```
   Where `<DCS Saved Games>` is typically:
   ```
   C:\Users\<you>\Saved Games\DCS\       (stable)
   C:\Users\<you>\Saved Games\DCS.openbeta\  (open beta)
   ```

3. Verify the `Config\options.lua` has `net` enabled (required for gRPC to bind its port).

### 3.2 Verify gRPC is listening

After loading a mission that contains a carrier:
```powershell
# Should return HTTP/2 response headers (not "connection refused")
Test-NetConnection -ComputerName 127.0.0.1 -Port 50051
```

DCS-gRPC logs its status in `<DCS Saved Games>\Logs\dcs.log`:
```
gRPC server started on 127.0.0.1:50051
```

### 3.3 Remote server access

If LSO runs on a different machine than DCS, change the bind address in the DCS-gRPC config and pass `--uri` to LSO:
```powershell
lso.exe run --uri http://192.168.1.50:50051 -o C:\LSO\recordings
```

---

## 4. First Run

Create an output directory and start LSO:

```powershell
mkdir C:\LSO\recordings
.\lso.exe run -o C:\LSO\recordings
```

LSO will:
1. Connect to DCS-gRPC at `http://127.0.0.1:50051` (retrying with exponential back-off until DCS is ready).
2. Enumerate all active carriers and carrier-capable aircraft in the mission.
3. Spawn a monitoring task for every (carrier, aircraft) pair.
4. Record each qualifying recovery attempt as PNG chart + ACMI + JSON + SQLite row.
5. On Ctrl-C: print the session greenie board to the terminal and exit cleanly.

Expected startup output:
```
INFO lso: Connecting to gRPC server uri=http://127.0.0.1:50051
INFO lso: Connected
INFO lso: Monitoring CVN_71 ↔ Hornet-1 (pilot: Viper)
INFO lso: Monitoring CVN_71 ↔ Hornet-2 (pilot: Ghost)
```

---

## 5. All CLI Options

```
USAGE:
    lso.exe [OPTIONS] <SUBCOMMAND>

GLOBAL OPTIONS:
    -v, --verbose     Increase log verbosity (repeat for DEBUG / TRACE)
        --color       Enable coloured log output

SUBCOMMANDS:
    run     Connect to DCS-gRPC to track carrier recoveries
    file    Replay recoveries from an existing LSO ACMI file
    help    Print help for a subcommand
```

### `run` subcommand

| Flag | Default | Description |
|---|---|---|
| `-o, --out-dir <PATH>` | `.` (current dir) | Directory where PNG, ACMI, JSON and `lso.db` are written |
| `--uri <URI>` | `http://127.0.0.1:50051` | DCS-gRPC endpoint URI |
| `--discord-webhook <URL>` | _(disabled)_ | Discord webhook URL for per-recovery posts |
| `--discord-users <FILE>` | _(disabled)_ | JSON file mapping pilot names → Discord user IDs |
| `--web-port <PORT>` | _(disabled)_ | Port to serve the web greenie board on (e.g. `8080`) |
| `--ki` | false | Also record KI (AI-controlled) carrier landings |
| `--no-acmi` | false | Skip saving TacView ACMI files (PNG chart and JSON report are still saved) |

### `file` subcommand

| Argument | Description |
|---|---|
| `<INPUT>` | Path to an `LSO-*.zip.acmi` file previously recorded by LSO |

> Only ACMI files **created by LSO** are supported. Raw TacView recordings do not contain the LSO metadata required for re-grading.

### Examples

```powershell
# Minimal live mode
lso.exe run -o C:\LSO\recordings

# Live mode with web dashboard on port 8080
lso.exe run -o C:\LSO\recordings --web-port 8080

# Live mode with Discord and web dashboard
lso.exe run `
  -o C:\LSO\recordings `
  --discord-webhook "https://discord.com/api/webhooks/…" `
  --discord-users C:\LSO\users.json `
  --web-port 8080

# Live mode without ACMI files (PNG chart and JSON only)
lso.exe run -o C:\LSO\recordings --no-acmi

# Remote DCS server
lso.exe run -o C:\LSO\recordings --uri http://192.168.1.50:50051

# Replay an old recording
lso.exe file C:\LSO\recordings\LSO-20260610-183042-Viper.zip.acmi

# Debug verbosity
lso.exe -vv run -o C:\LSO\recordings
```

---

## 6. Output Files

Every saved recovery produces four files in `--out-dir`:

| File | Description |
|---|---|
| `LSO-<YYYYMMDD-HHMMSS>-<Pilot>.png` | PNG approach chart (side view + top-down view, AoA-coloured track, grade overlay) |
| `LSO-<YYYYMMDD-HHMMSS>-<Pilot>.zip.acmi` | Compressed TacView ACMI recording (carrier + aircraft) — omitted when `--no-acmi` |
| `LSO-<YYYYMMDD-HHMMSS>-<Pilot>.json` | Machine-readable recovery report (see schema below) |
| `lso.db` | SQLite database accumulating all passes across all sessions |

> The pilot name in the filename is sanitised to ASCII alphanumeric characters only.

### JSON report schema

```json
{
  "pilot_name": "Viper",
  "grading": { "Recovered": { "cable": 3, "cable_estimated": 3 } },
  "pass_grade": "Ok",
  "dcs_grading": "OK 3 WIRE# 3",
  "gate_deviations": {
    "at_three_quarter_nm": { "gs_deviation_ft": 12.4, "lineup_ft": -3.1 },
    "at_half_nm":          { "gs_deviation_ft":  8.0, "lineup_ft": -1.8 },
    "at_quarter_nm":       { "gs_deviation_ft":  2.2, "lineup_ft":  0.5 }
  },
  "datums": [
    { "x": 1389.2, "y": -3.1, "aoa": 8.1, "alt": 42.3 },
    ...
  ]
}
```

**`grading` variants:**
- `"Unknown"` — outcome not determined (should not appear in saved files)
- `"Bolter"` — aircraft did not catch a wire
- `"WaveoffPilot"` — pilot climbed away from inside the groove
- `{ "Recovered": { "cable": N, "cable_estimated": N } }` — caught wire N

**`pass_grade` values:** `"Ok"`, `"OkParentheses"`, `"Fair"`, `"NoGrade"`, `"Cut"`, `"Bolter"`, `"WaveoffPilot"`

---

## 7. Web Greenie Board

Start with `--web-port`:

```powershell
lso.exe run -o C:\LSO\recordings --web-port 8080
```

Then open a browser: **http://localhost:8080**

The page:
- Shows all passes from the SQLite database, newest first
- Auto-refreshes every 10 seconds
- Colour-codes grades (green = OK, yellow = Fair, orange = NG, red = Cut)

### Exposing to the network

By default the server binds to `0.0.0.0:<port>`, so it is reachable from other machines on the same network. To restrict access, configure your firewall:

```powershell
# Allow inbound on port 8080 from LAN only
New-NetFirewallRule -DisplayName "LSO Web Board" `
  -Direction Inbound -Protocol TCP -LocalPort 8080 `
  -RemoteAddress 192.168.1.0/24 -Action Allow
```

> LSO does not implement authentication. Do not expose the web port to the public internet.

### API endpoint

`GET /api/passes` returns a JSON array:

```json
[
  {
    "id": 42,
    "timestamp": "LSO-20260610-183042-Viper",
    "pilot_name": "Viper",
    "pass_grade": "Ok",
    "wire": 3,
    "dcs_grading": "OK 3 WIRE# 3"
  },
  ...
]
```

---

## 8. Discord Integration

### 8.1 Create a webhook

1. In Discord, go to the target channel → **Edit Channel** → **Integrations** → **Webhooks**.
2. Click **New Webhook**, copy the URL.

### 8.2 Start LSO with the webhook

```powershell
lso.exe run -o C:\LSO\recordings `
  --discord-webhook "https://discord.com/api/webhooks/123456789/XXXXXXXXXX"
```

Each completed pass posts an embed containing:
- Pilot name (optionally mentioned by Discord user ID)
- NAVAIR pass grade (OK / Fair / NG etc.)
- Outcome (Wire #N / Bolter / Waveoff)
- Gate deviation table (GS and lineup at 3/4 nm, 1/2 nm, 1/4 nm)
- Attached PNG chart
- Attached ACMI file

### 8.3 User ID mapping (optional)

Create a JSON file that maps DCS pilot names to Discord snowflake user IDs so Discord mentions the correct user:

```json
{
  "Viper": 123456789012345678,
  "Ghost": 234567890123456789
}
```

Pass it with `--discord-users C:\LSO\users.json`. Pilots not in the file are shown by their DCS name instead.

---

## 9. Session Greenie Board (Terminal)

When LSO exits (Ctrl-C or clean shutdown) it prints a text greenie board to stdout:

```
╔══════════════════════════════════════════════════════════╗
║              SESSION GREENIE BOARD                       ║
╠═══════════════════════╦══════╦══════╦════════════════════╣
║ Pilot                 ║ Wire ║ Grd  ║ DCS Grade          ║
╠═══════════════════════╬══════╬══════╬════════════════════╣
║ Viper                 ║  3   ║ OK   ║ OK 3 WIRE# 3       ║
║ Ghost                 ║  -   ║ WO   ║ -                  ║
║ Slider                ║  1   ║ NG   ║ NG 1 WIRE# 1       ║
╚═══════════════════════╩══════╩══════╩════════════════════╝
```

> This board covers only the current session. Historical data is in the SQLite database.

---

## 10. Offline ACMI Replay

Re-generate PNG charts from previously recorded ACMI files:

```powershell
lso.exe file C:\LSO\recordings\LSO-20260610-183042-Viper.zip.acmi
```

This reads the ACMI, re-runs the tracking and grading logic, and writes a fresh PNG into the same directory. Useful if chart rendering was updated.

---

## 11. Supported Carriers & Aircraft

### Carriers

| DCS Type Name(s) | Ship | Notes |
|---|---|---|
| `CVN_71`, `CVN_72`, `CVN_73`, `CVN_75` | USS Nimitz-class (CVN-71 to CVN-75) | Uses Nimitz cable geometry |
| `Stennis` | USS John C. Stennis (CVN-74) | Same as Nimitz (DCS type name is `"Stennis"`) |
| `Forrestal` | USS Forrestal | Different cable positions |

### Aircraft

| DCS Type Name | Aircraft | AoA On-Speed | Glide Slope |
|---|---|---|---|
| `FA-18C_hornet` | F/A-18C Hornet | 7.4° – 8.8° | 3.5° |
| `F-14A-135-GR`, `F-14B` | F-14A/B Tomcat | ~10.2° – 11.1° | 3.5° |
| `T-45` | VNAO T-45C Goshawk | 7.0° – 7.5° | 3.5° |

> Aircraft or carriers **not** in the tables above are silently ignored by the monitoring tasks. To add support, add entries to `src/data.rs`.

---

## 12. Pass Grading Reference

LSO uses a simplified NAVAIR 00-80T-104 grading algorithm based on gate deviations sampled at 3/4 nm, 1/2 nm, and 1/4 nm from the carrier.

### Grade thresholds

| Grade | Label | Pts | Condition |
|---|---|---|---|
| OK | `OK` | 4 | All gates: GS deviation < ±40 ft AND lineup < ±25 ft |
| OK (parentheses) | `(OK)` | 3 | Worst gate: GS < ±100 ft AND lineup < ±60 ft |
| Fair | `Fair` | 2 | Worst gate: GS < ±200 ft AND lineup < ±120 ft |
| No Grade | `NG` | 1 | Any gate: GS ≥ ±200 ft OR lineup ≥ ±120 ft |
| Cut | `Cut` | 0 | GS < −150 ft at the 1/4-nm gate (dangerously low at the ramp) |
| Bolter | `B` | — | Aircraft did not catch a wire |
| Waveoff (Pilot) | `WO` | — | Pilot climbed away after entering the groove (inside 3/4 nm, ≤ 300 ft AGL) |

### Waveoff detection

A waveoff is recorded when:
1. The aircraft enters the groove (x ≤ 1,389 m from the carrier AND altitude ≤ 300 ft AGL), **and**
2. No `RunwayTouch` event is received before tracking ends.

---

## 13. Database Administration

The SQLite database lives at `<out-dir>/lso.db`.

### View all passes

```powershell
# Using the sqlite3 CLI (download from https://www.sqlite.org/download.html)
sqlite3 C:\LSO\recordings\lso.db "SELECT * FROM passes ORDER BY id DESC LIMIT 20;"
```

### Schema

```sql
CREATE TABLE passes (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    timestamp   TEXT    NOT NULL,   -- "LSO-YYYYMMDD-HHMMSS-Pilot"
    pilot_name  TEXT    NOT NULL,
    pass_grade  TEXT    NOT NULL,   -- "OK", "(OK)", "Fair", "NG", "Cut", "B", "WO"
    wire        INTEGER,            -- NULL for bolter / waveoff
    dcs_grading TEXT                -- raw DCS LandingQualityMark string, or NULL
);
```

### Export to CSV

```powershell
sqlite3 -csv -header C:\LSO\recordings\lso.db \
  "SELECT * FROM passes ORDER BY id;" > passes.csv
```

### Per-pilot statistics

```sql
SELECT
    pilot_name,
    COUNT(*)                                              AS total,
    SUM(CASE WHEN pass_grade = 'OK'   THEN 1 ELSE 0 END) AS ok,
    SUM(CASE WHEN pass_grade = '(OK)' THEN 1 ELSE 0 END) AS ok_par,
    SUM(CASE WHEN pass_grade = 'Fair' THEN 1 ELSE 0 END) AS fair,
    SUM(CASE WHEN pass_grade = 'NG'   THEN 1 ELSE 0 END) AS ng,
    SUM(CASE WHEN pass_grade = 'Cut'  THEN 1 ELSE 0 END) AS cut,
    SUM(CASE WHEN pass_grade = 'B'    THEN 1 ELSE 0 END) AS bolter,
    SUM(CASE WHEN pass_grade = 'WO'   THEN 1 ELSE 0 END) AS waveoff
FROM passes
GROUP BY pilot_name
ORDER BY total DESC;
```

### Backup

```powershell
Copy-Item C:\LSO\recordings\lso.db C:\LSO\backup\lso-$(Get-Date -Format yyyyMMdd).db
```

---

## 14. Logging & Diagnostics

Log verbosity is controlled by the `-v` global flag:

| Flag | Level | Shows |
|---|---|---|
| _(none)_ | INFO | Connections, recoveries saved, errors |
| `-v` | DEBUG | Task start/stop, bolter detection, grading decisions |
| `-vv` | TRACE | Every 100 ms poll, cable candidate scores |

Redirect logs to a file:

```powershell
lso.exe run -o C:\LSO\recordings 2> C:\LSO\lso.log
```

Or combine stdout and stderr:

```powershell
lso.exe run -o C:\LSO\recordings *> C:\LSO\lso.log
```

Enable colour with `--color` when running in a terminal that supports ANSI:

```powershell
lso.exe --color run -o C:\LSO\recordings
```

---

## 15. Running as a Windows Service

Use [NSSM (Non-Sucking Service Manager)](https://nssm.cc) to run LSO as a background Windows service that starts automatically with the server.

```powershell
# Install NSSM (or use winget)
winget install NSSM.NSSM

# Create the service
nssm install LSO "C:\LSO\lso.exe"
nssm set LSO AppParameters "run -o C:\LSO\recordings --web-port 8080"
nssm set LSO AppDirectory "C:\LSO"

# Optional: add Discord webhook (pilots appear by their DCS name if --discord-users is omitted)
nssm set LSO AppParameters "run -o C:\LSO\recordings --web-port 8080 --discord-webhook ""https://discord.com/api/webhooks/YOUR_ID/YOUR_TOKEN"""

# Optional: also enable Discord @mentions by passing the user ID map
nssm set LSO AppParameters "run -o C:\LSO\recordings --web-port 8080 --discord-webhook ""https://discord.com/api/webhooks/YOUR_ID/YOUR_TOKEN"" --discord-users C:\LSO\users.json"
nssm set LSO AppStdout "C:\LSO\lso.log"
nssm set LSO AppStderr "C:\LSO\lso.log"
nssm set LSO AppRotateFiles 1
nssm set LSO AppRotateBytes 10485760   # 10 MB per log file

# Start the service
nssm start LSO

# Check status
nssm status LSO

# Stop and remove the service
nssm stop LSO
nssm remove LSO confirm
```

> LSO connects to DCS-gRPC with exponential back-off, so it is safe to start the service before DCS loads the mission. It will keep retrying until gRPC becomes available.

---

## 16. Troubleshooting

### LSO exits immediately with "connection refused"

DCS is not running or DCS-gRPC is not installed. LSO retries automatically with back-off — check the logs for the retry messages. If it exits immediately, check `--uri`.

### "discard as plane was never below 100 m MSL"

The aircraft was in the pattern area but never descended to a landing altitude. This is normal for overhead patterns or missed approaches that don't enter the groove.

### "discard: no recovery outcome (Unknown grading)"

The aircraft briefly entered the tracking zone but never entered the groove (inside 3/4 nm AND below 300 ft). Not a valid pass.

### No recordings being created

1. Verify the carrier type is supported (see [Section 11](#11-supported-carriers--aircraft)).
2. Verify the aircraft type is supported.
3. Check `--ki` is set if monitoring AI aircraft.
4. Use `-vv` to see all tracking decisions in the log.

### PNG chart is blank or missing the track

The recording loop captured fewer than 2 data points (approach was very brief). The ACMI and JSON are still saved.

### Web dashboard shows "Loading…" indefinitely

The `lso.db` file may not have been created yet (no passes saved). Open `GET /api/passes` directly — it returns `[]` for an empty database, which the page correctly displays as "No passes recorded yet."

### Discord posts are missing the chart image

Discord webhooks have a 25 MB attachment limit. Very long approaches (>15 min) may produce oversized ACMI files. The PNG is always ≤ 1 MB and should always attach successfully.

### Port 8080 already in use

Choose a different `--web-port` value, or find and stop the conflicting process:
```powershell
Get-Process -Id (Get-NetTCPConnection -LocalPort 8080).OwningProcess
```
