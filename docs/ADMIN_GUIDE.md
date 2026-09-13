# Installation and administration

This guide covers a dedicated-server deployment of LSO against the `sevenfifty777/rust-server`
fork of DCS-gRPC. It replaces the local launch script as the reference: the script stays out of
source control because it carries the Discord webhook and machine-specific paths.

## 1. Server side: DCS-gRPC v0.10.0

LSO is pinned to the fork release **v0.10.0** (`Cargo.toml`, `[dependencies.stubs] tag =
"v0.10.0"`). Deploy exactly that release on the server, never a locally built DLL from another
commit, and never half of the package:

1. Download `DCS-gRPC-0.10.0.zip` from
   <https://github.com/sevenfifty777/rust-server/releases/tag/v0.10.0>.
2. Stop the DCS server. Replace the whole `Mods/tech/DCS-gRPC/` tree (DLL **and** Lua files) and
   `Scripts/Hooks/DCS-gRPC.lua` with the contents of the zip.
3. Edit `Saved Games/DCS.server/Config/dcs-grpc.lua`. The buffered position source LSO uses by
   default is opt-in on the server:

   ```lua
   -- LSO reads positions through StartRecoveryTelemetry / ReadRecoveryTelemetry.
   -- Without this line the server answers UNIMPLEMENTED and LSO cannot start a recording
   -- in its default --position-source buffered mode.
   recoveryTelemetry.enabled = true
   recoveryTelemetry.periodSeconds = 0.05     -- 20 Hz capture (default)
   recoveryTelemetry.retentionSeconds = 30    -- ring retention, drives LSO's 29 s watchdog
   recoveryTelemetry.readsPerSecond = 20      -- quota per authenticated client label (default)

   -- Strongly recommended, mandatory when host is not a loopback address.
   auth.enabled = true
   auth.tokens = {
     { client = "lso", token = "<a long random token>" },
   }
   ```

   `readsPerSecond` is enforced per client label. Every LSO recovery in one process shares that
   label, which is why LSO paces its own reads below the quota (see `--buffered-read-budget-per-second`
   below). Raise the server quota and the client budget together if you ever need more than about
   eight simultaneous recoveries.
4. Start the DCS server and check `dcs.log` for the DCS-gRPC banner reporting `0.10.0`.

What v0.10.0 contains that earlier builds lack: the `RecoveryService` buffered telemetry
(`v0.9.2`), the queue and Lua latency diagnostics on `GetRecoverySnapshot` (`v0.9.2`), and the
restored `RESOURCE_EXHAUSTED` Lua helper without which hitting `maxActiveRecoveries` crashed the
`StartRecoveryTelemetry` handler (`v0.9.3`). `v0.10.0` is `v0.9.3` renumbered after the merge of
the colleague fork; it carries no protobuf change against the `0.9.x` line.

## 2. Client side: build and validate LSO

```powershell
cargo build --release --locked
cargo fmt --all -- --check
cargo test --locked --no-fail-fast
cargo clippy --locked --all-targets -- -D warnings
git diff --check
```

`--locked` matters: `build.rs` reads the resolved `dcs-grpc-stubs` version out of `Cargo.lock`
and compiles it into the binary. That value is what the runtime compatibility check compares with
the server's reported version, and what every JSON report records as `dcs_grpc_client_stubs`. It
is never typed by hand. The binary also records the Git commit it was built from (`lso_commit`)
and whether tracked files were modified (`lso_dirty`).

## 3. Run

```powershell
New-Item -ItemType Directory -Force C:\LSO\recordings
$env:DCS_GRPC_API_KEY = "<the token configured in dcs-grpc.lua>"
$env:LSO_DISCORD_WEBHOOK = "<optional webhook URL>"   # keep it out of scripts and logs
.\target\release\lso.exe run -o C:\LSO\recordings --discord-webhook $env:LSO_DISCORD_WEBHOOK
```

At start-up LSO logs the server version and one of three verdicts:

| Log line | Meaning |
|---|---|
| `DCS-gRPC client/server versions match` | server reports the pinned version (`0.10.0`): the validated pairing |
| `same API line accepted pending live validation` | another patch of the same `0.10.x` line; run it, but note it in the session log |
| `DCS-gRPC compatibility is not established` | another API line, or version unavailable: stop and fix the deployment |

Useful options:

| Option | Meaning |
|---|---|
| `--uri` | DCS-gRPC URI; default `http://127.0.0.1:50051` |
| `--api-key-env` | environment variable holding the API key; default `DCS_GRPC_API_KEY`; empty only for an intentionally unauthenticated loopback server |
| `--position-source buffered` | default; `unary` is the documented rollback that needs no `recoveryTelemetry` on the server |
| `--buffered-read-budget-per-second` | process-wide `ReadRecoveryTelemetry` budget shared by all recoveries; default 16, keep it below the server's `readsPerSecond` |
| `--hook-sampling-hz`, `--hook-timeout-ms` | independent hook draw-argument sampler (default 4 Hz, 300 ms) |
| `--ki` | also record supported AI aircraft; AI stays labelled as such |
| `--no-acmi` | omit the Tacview recording, keep JSON, PNG, SQLite, Discord |
| `--positions-only` | acquisition diagnostic: positions and JSON quality report only |
| `--baseline-manifest` | JSON describing the DCS build, mission and deployed DLL/Lua hashes, copied into every report |
| `--discord-webhook`, `--discord-users` | optional Discord publication |
| `-v`, `-vv` | debug or trace logging; global flags go before `run` |

The greenie board is the LSO page of the DCS Web Dashboard: point its `LSO_DIR` at the same
`--out-dir`. `lso.db` is opened in SQLite WAL mode so the dashboard can read while a pass is being
written. LSO opens no listening port of its own; the former `--web-port` flag is refused with an
explanatory error.

## 4. Stop, switch, roll back

**Stop.** Ctrl-C finalises the passes still in flight (source stop, event grace period, wind
query, JSON, SQLite, PNG, Discord) and returns within 30 seconds. Wait for `active recovery tasks
finalised` before killing the process.

**Switch to a new LSO build.** Stop LSO, keep the previous binary next to the new one, start the
new one and check within two minutes: the compatibility line above, `session`/`generation` in the
logs, no migration error on `lso.db`, the expected number of aircraft/carrier pairs, the dashboard
reading new rows, and the ten-second runtime metrics line.

**Roll back acquisition without changing binaries.** Restart with `--position-source unary`.
Keep the buffered and unary logs of the same session so the latency percentiles stay comparable.

**Roll back the binary.** Stop LSO, point the service at the preserved previous binary, restart,
confirm the compatibility line and the dashboard. A secondary output failure (Discord, PNG) never
justifies delaying the restoration of local recording.

## 5. What is deliberately not in source control

| Path | Why |
|---|---|
| `run-live*.ps1` and other launch scripts | carry the Discord webhook and machine paths; read the webhook from the environment instead |
| `releases/` | built binaries are published on GitHub Releases |
| `trap sample/` | recorded passes with real callsigns and a SQLite file; regression fixtures live in `tests/recordings/` |
| `graphify-out/` | locally generated knowledge graph |
| `trap_records/` | live session archives used for analysis |

## 6. Offline replay

```powershell
.\target\release\lso.exe file tests\recordings\live_2026-09\t45_hookdown_wire3.zip.acmi
```

Replay consumes only LSO-authored ACMI. It reads the `LSOHook` property when the recording has it,
keeps feeding samples ten seconds after touchdown, and restarts a fresh track after an attempt that
ended without touchdown. It cannot reproduce network timing, UCID, DCS event delivery or server
performance, and it carries no velocity, so the velocity-based diagnostics stay empty.
