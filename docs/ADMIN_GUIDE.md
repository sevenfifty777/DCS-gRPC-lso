# Installation and administration

## Compatibility

The client is pinned to DCS-gRPC `v0.9.0`, Cargo revision
`5bd6d6e42491c8697a5c5a95e80a2e689923bd3b`. Do not change protobuf or this pin before authenticating
the deployed DLL/Lua tree as described in [LIVE_VALIDATION.md](LIVE_VALIDATION.md).

## Build and validate

```powershell
cargo build --release --locked
cargo fmt --all -- --check
cargo test --locked --no-fail-fast
cargo clippy --locked --all-targets -- -D warnings
git diff --check
```

Create the output directory before start:

```powershell
New-Item -ItemType Directory -Force C:\LSO\recordings
.\target\release\lso.exe run -o C:\LSO\recordings
```

Useful options:

| Option | Meaning |
|---|---|
| `--uri` | DCS-gRPC URI; default `http://127.0.0.1:50051` |
| `--ki` | include supported AI aircraft; AI remains explicitly labelled |
| `--no-acmi` | omit ACMI while retaining JSON/PNG/SQLite |
| `--web-port` | private dashboard on `127.0.0.1:<port>` |
| `--discord-webhook` | optional secondary publication |

The dashboard has no OAuth2/TLS in phase 1 and cannot be opened remotely because it binds loopback.
Do not add a public bind as an operational workaround.

## Supported matrix

- AV-8B NA (`AV8BNA`) only with Tarawa (`LHA_Tarawa`), experimental V/STOL;
- F/A-18C, supported F-14 aliases and T-45 only with known Nimitz/Forrestal arrested geometry;
- incompatible pairs create no detector or event stream.

`--ki` is opt-in. Humans use private slot-resolved UCID identity; AI uses a session/unit identity.
Never publish UCIDs from the private database/API.

## Outputs

The base filename includes wall time, sanitized display name, session, generation, aircraft/carrier
IDs and DCS time. This prevents simultaneous-pass collisions. Outputs are:

- atomic schema-v3 JSON report;
- optional atomic compressed ACMI;
- approach and pattern PNG rendered outside sampling tasks;
- additive/idempotent `lso.db` row;
- optional Discord message for a newly inserted row only.

A secondary output failure is logged and does not remove an already persisted pass. Dashboard query
failure returns HTTP 500. The `points_awarded` API field distinguishes no points from a real zero.

## Operations

INFO logs include the DCS-gRPC-reported version, session/generation and ten-second runtime metrics.
Repeated watchdog rotations indicate a silent/unhealthy gRPC channel, not a reason to increase the
two-second deadline without measurement.

Use [BENCHMARK_PROTOCOL.md](BENCHMARK_PROTOCOL.md) before performance changes and
[DEPLOYMENT_ROLLBACK.md](DEPLOYMENT_ROLLBACK.md) for a release switch. The rollback procedure is not
validated until timed on staging.

Offline replay:

```powershell
.\target\release\lso.exe file tests\recordings\wire_3_01_T45.zip.acmi
```

Replay consumes only LSO-authored ACMI. Common live/replay geometry is covered by an invariance test,
but replay cannot reproduce network timing, UCID, DCS event delivery or server performance.
