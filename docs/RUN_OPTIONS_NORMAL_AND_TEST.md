# How to run LSO: normal use and the latency test

Written for the integration branch on 13 September 2026. Every command below was checked against
the real `lso.exe run --help` of that branch. Read [ADMIN_GUIDE.md](ADMIN_GUIDE.md) first if the
server is not yet on DCS-gRPC v0.10.0.

## 0. Before anything: two things that will bite you

**The old flags are gone.** One of your servers starts LSO with:

```
--recovery-telemetry-mode atomic --recovery-snapshot-timeout-ms 250
```

Those two options belong to the old `astra-review` branch. The integration branch does not have
them. If you start the new binary with them, it stops immediately with an "unexpected argument"
error. Remove them. The equivalent on the new branch is simply the default (`--position-source
buffered`), and the rollback is `--position-source unary`.

**The binary on the server is Lenny's.** His script runs
`...\Saved Games\DCS.openbeta_server\Scripts\DCS-gRPC-lso\lso.exe`. That file was built from his
branch. You must replace it with a build of the integration branch:

```powershell
# on your development machine, in the DCS-gRPC-lso folder, branch "integration"
cargo build --release --locked
# then copy target\release\lso.exe over the one in the Saved Games folder
```

After starting, look at the first lines of the log. You want this one:

```
DCS-gRPC client/server versions match   client=0.10.0 server=0.10.0
```

If it says "compatibility is not established", the server is not running the v0.10.0 release.
Stop and fix that first.

## 1. Normal use

This is the command for real sessions with human pilots, the dashboard and Discord.

```powershell
$env:DCS_GRPC_API_KEY = "<the token written in dcs-grpc.lua>"
$env:LSO_DISCORD_WEBHOOK = "<your Discord webhook URL>"

.\lso.exe run `
  --out-dir "C:\Users\admin\Saved Games\DCS.openbeta_server\Scripts\DCS-gRPC-lso\Records" `
  --position-source buffered `
  --baseline-manifest .\live-baseline.json `
  --discord-webhook $env:LSO_DISCORD_WEBHOOK
```

What each line does:

| Option | Plain meaning |
|---|---|
| `DCS_GRPC_API_KEY` (environment variable) | The password LSO uses to talk to DCS-gRPC. Same value as the `token` in the server's `dcs-grpc.lua`. LSO reads it from the environment, never from the command line, so it does not end up in logs. |
| `LSO_DISCORD_WEBHOOK` (environment variable) | Where LSO posts the Discord message. Keep the URL in the environment, not inside a script that could be shared or committed. |
| `--out-dir` | The folder where every pass is written: JSON report, PNG charts, ACMI recording, and the `lso.db` database the dashboard reads. Point the dashboard's `LSO_DIR` at this same folder. |
| `--position-source buffered` | How positions are collected. `buffered` is the new 20 Hz method and the default. Writing it explicitly makes the log say what you intended. |
| `--baseline-manifest` | A small JSON file describing the DCS build and mission. Its content is copied into every report so you can later tell which server setup produced a pass. Lenny's script builds the path but forgets to pass it, which is why all 158 of his reports have this field empty. |
| `--discord-webhook` | Enables the Discord message per pass. Leave it out to disable Discord. |

Things you might be tempted to add, and my advice:

- **`--no-acmi`**: do not add it yet. The ACMI recording is the file we replay to build regression
  fixtures and to validate the human-LSO policy. Lenny's script disables ACMI by default; for the
  next weeks, keep it on. Each recording is about 50 to 90 KB.
- **`--ki`**: leave it out for normal use. It also records AI aircraft, which clutters the board.
  Useful only when you test alone on an empty server with AI traffic.
- **`--buffered-read-budget-per-second`**: leave the default (16). Only change it together with the
  server's `recoveryTelemetry.readsPerSecond`.
- **`-v`**: add it before `run` (global flag) when you want debug logging: `.\lso.exe -v run ...`.

**If buffered mode misbehaves** (repeated watchdog messages, no reports), restart with the same
command and `--position-source unary`. That path does not need `recoveryTelemetry` on the server
at all. It is the safety net, not the normal mode.

**Stopping**: press Ctrl-C once and wait. LSO finishes the passes that are still being tracked
(up to 30 seconds) and then exits. Wait for the line `active recovery tasks finalised`.

## 2. The latency test (step 7 of the comparison)

### Why

On Lenny's corpus, data delivery was fast on 5 September (26 ms) and slow from 6 September onward
(around 700 ms at the 95th percentile). Nothing changed in the collection code between those two
days. What changed was the environment: several human pilots at once, and the hook sampler being
switched on for the F-14. We want to know which of these causes the delay. Grades are not
affected, but hook timing is.

### The measurement

Every JSON report contains the field `telemetry_quality.position_poll_p95_latency_ms`. That is
the number to compare. Below 50 ms is what we saw on the good day. Around 700 ms is the problem.

### The three runs

Do all three on the **same evening, same mission, same players**, one after the other. Each run
should last long enough to collect a handful of passes (five or more). Between runs, stop LSO
with Ctrl-C, change only the flag, start again. Nothing changes on the server.

| Run | Command | What it tells you |
|---|---|---|
| **A: reference** | the normal command from section 1 | today's behaviour: hook sampler on, events on, all outputs on |
| **B: no separate hook stream** | normal command plus `--legacy-inline-hook-sampling` | the hook is read once per position tick instead of through its own stream of requests |
| **C: positions only** | normal command plus `--positions-only` | pure position collection; no hook, no events, no database, no PNG, no Discord |

Run C produces JSON reports only, no grades, no Discord. That is expected.

### How to read the result

| A | B | C | Conclusion |
|---|---|---|---|
| slow | fast | fast | The separate hook request stream is the cause. Fix: route hook samples through the buffered engine instead of a separate stream. |
| slow | slow | fast | The event stream or the output work (database, PNG, Discord) is the cause. |
| slow | slow | slow | The server itself is slow with this many players. Look at the server side (`readsPerSecond`, DCS load). |
| fast | fast | fast | The slowdown was specific to Lenny's server build. Nothing to fix on the client. |

Optional fourth run: the normal command plus `--position-source unary`. It gives the old method's
number on the same evening, for comparison with the 34 ms measured on 4 September.

### Pulling the numbers out of the reports

Run this in PowerShell in the `Records` folder after each session. It prints one line per report
with the number we care about and the read-budget waits, so you can paste the lines into a table.

```powershell
Get-ChildItem *.json | Sort-Object Name | ForEach-Object {
    $r = Get-Content $_ -Raw | ConvertFrom-Json
    "{0}  p95={1} ms  budget_waits={2}  outcome={3}" -f $_.Name,
        $r.telemetry_quality.position_poll_p95_latency_ms,
        $r.recovery_telemetry.read_budget_waits,
        $r.outcome
}
```

Write down, for each run: the date, the number of players in the air, the flag used, and the
p95 values. Three to five passes per run is enough to see a 26 ms versus 700 ms difference.

### A cleaner test if you want it

There is no flag that turns off only the hook sampler while keeping everything else. Run B is the
closest we have. If you want a strict "hook sampler off, everything else on" run, ask for a
`--no-hook-sampling` flag; it is a few lines of code.

## 3. Fixing Lenny's launch script

`run-live-buffered.ps1` is now ignored by git (it will never be committed), which is right
because it contains the Discord webhook in clear text. Three changes make it correct:

1. **Pass the manifest.** The script defines `$baselineManifest` but never adds
   `"--baseline-manifest", $baselineManifest` to the argument list. Add it.
2. **Read the webhook from the environment.** Replace the hard-coded URL with
   `$env:LSO_DISCORD_WEBHOOK` and set that variable once on the server, outside the script.
3. **Turn ACMI on by default.** Invert the `-EnableAcmi` switch into `-DisableAcmi`, or simply
   drop the `--no-acmi` line for now.

A corrected argument block, keeping Lenny's structure:

```powershell
$lsoArguments = @(
    "run",
    "--uri", "http://127.0.0.1:50051",
    "--position-source", $PositionSource,
    "--out-dir", $outputDirectory,
    "--baseline-manifest", $baselineManifest,
    "--discord-webhook", $env:LSO_DISCORD_WEBHOOK
)
if ($PositionsOnly) { $lsoArguments += "--positions-only" }
if ($LegacyInlineHook) { $lsoArguments += "--legacy-inline-hook-sampling" }
if ($DisableAcmi) { $lsoArguments += "--no-acmi" }
if ($IncludeAi) { $lsoArguments += "--ki" }
```

with the matching `param(...)` switches at the top: `[switch]$PositionsOnly`,
`[switch]$LegacyInlineHook`, `[switch]$DisableAcmi`, `[switch]$IncludeAi`. Then the three test
runs become:

```powershell
.\run-live-buffered.ps1                       # run A
.\run-live-buffered.ps1 -LegacyInlineHook     # run B
.\run-live-buffered.ps1 -PositionsOnly        # run C
.\run-live-buffered.ps1 -PositionSource unary # optional run D
```

## 4. Two things that confused us

### Does `--no-acmi` enable or disable ACMI?

The flag itself always **disables** the ACMI recording. Lenny's script only adds it when you did
not pass `-EnableAcmi`:

```powershell
if (-not $EnableAcmi) { $lsoArguments += "--no-acmi" }
```

So with the script as it is, ACMI is off unless you launch it as
`.\run-live-buffered.ps1 -EnableAcmi`. That is why the current server produces no ACMI files.
Either add `-EnableAcmi` every time, or use the corrected block from section 3, which turns ACMI on
by default and offers `-DisableAcmi` instead.

### What is the manifest for?

The manifest is a small JSON file **you write once** that describes the server setup: DCS build,
mission, module versions, optionally the hashes of the deployed DCS-gRPC DLL and Lua files. LSO
does not compute anything from it. It copies the content into every report under
`baseline_manifest`, so that weeks later you can tell which environment produced a given pass.
It is a label, nothing more. Without it, two reports from different server setups look identical
and a change in behaviour cannot be explained.

Copy [BASELINE_MANIFEST.example.json](BASELINE_MANIFEST.example.json) next to the launch script
as `live-baseline.json` and fill it in:

```json
{
  "dcs_build": "2.9.x.xxxxx",
  "mission": "your-mission-name.miz",
  "mission_sha256": null,
  "dcs_grpc_dll_sha256": null,
  "dcs_grpc_lua_sha256": null,
  "module_versions": {
    "F-14": "version from the DCS module manager",
    "T-45": "version",
    "Supercarrier": "version"
  }
}
```

Rules LSO checks when it loads the file: no unknown keys, no empty strings, and each `sha256`
field is either `null` or a real SHA-256 hash. Fill in what you know, leave the rest `null`.

**State of Lenny's `live-baseline.json` (downloaded from the server on 13 September).** The file
exists and its content is valid: DCS build `2.9.29.27468`, the Foothold mission, and three real
SHA-256 hashes. Two things to know:

- The file starts with a UTF-8 byte-order mark, which is how Windows PowerShell 5.1 saves UTF-8.
  LSO used to reject such a file with "invalid JSON at line 1, column 1", so even with the flag
  passed it would not have started. Since 13 September the loader ignores the mark; no need to
  re-save the file.
- The two DCS-gRPC hashes describe the DLL and Lua files of **Lenny's own server build**. After
  you deploy the v0.10.0 release they are wrong. Recompute them on the server and paste the
  values back into the file:

  ```powershell
  Get-FileHash "C:\...\Mods\tech\DCS-gRPC\dcs_grpc.dll" -Algorithm SHA256
  Get-FileHash "C:\...\Mods\tech\DCS-gRPC\lua\DCS-gRPC\grpc.lua" -Algorithm SHA256
  ```

  Also replace `"lso": "0.2.0"` under `module_versions` with the aircraft and Supercarrier module
  versions: the LSO version is already recorded in every report by the binary itself.

The script still never passes `--baseline-manifest`, so add the argument (section 3). Until then
the field stays empty in every report, which is harmless but carries no environment label. The
file is site-specific and is now ignored by git.

## 5. Quick reference: every `run` option on this branch

| Option | Default | Use it when |
|---|---|---|
| `-o`, `--out-dir` | `.` | always; the Records folder |
| `--uri` | `http://127.0.0.1:50051` | DCS-gRPC is on another host or port |
| `--api-key-env` | `DCS_GRPC_API_KEY` | the token lives in a variable with another name |
| `--position-source` | `buffered` | `unary` for rollback or the optional run D |
| `--buffered-read-budget-per-second` | `16` | only together with the server quota |
| `--baseline-manifest` | none | always, so reports say which server setup produced them |
| `--discord-webhook` | none | normal use |
| `--discord-users` | none | you have a JSON mapping pilot names to Discord IDs |
| `--no-acmi` | off | not for now (fixtures need ACMI) |
| `--ki` | off | testing alone with AI aircraft |
| `--positions-only` | off | run C of the latency test |
| `--legacy-inline-hook-sampling` | off | run B of the latency test |
| `--hook-sampling-hz` | `4` | rarely; 2 to 4 allowed |
| `--hook-timeout-ms` | `300` | rarely; 250 to 300 allowed |
| `--suspend-detectors-during-recovery` | off | rarely; reduces duplicate detector work while a pass is recorded |
| `-v`, `-vv` (before `run`) | off | debug or trace logging |
