# DCS-gRPC LSO

LSO is a Rust command-line tool that monitors DCS carrier recoveries through the
[sevenfifty777 DCS-gRPC fork](https://github.com/sevenfifty777/rust-server). It records the Case I
pattern and final approach, estimates the arresting wire, applies a simplified gate-based pass
grade, produces trap-sheet images and structured output, and can publish a greenie board through
the DCS Web Dashboard or Discord.

![LSO example report](docs/example.png)

## Current capabilities

- Strict compatible-pair monitoring, including units spawned after LSO starts: AV-8B/Tarawa V/STOL
  and supported hook aircraft on arrested carriers.
- Full-pattern detection inside 3.5 nm and below 1,100 ft MSL, followed by 10 Hz recording.
- Carrier-relative final-approach and overhead-pattern PNG charts with AoA-coloured tracks.
- Bracketed/interpolated gates at 3/4, 1/2, and 1/4 nm with freshness/skew evidence. Incomplete
  observations receive `NC` and no points.
- Aircraft/carrier transforms are captured together in DCS mission Lua, retained in a bounded
  source ring, and read incrementally by sequence. CATOBAR hook state remains an independent 4 Hz
  sampler with a 300 ms timeout. Stale or unknown hook data is never reused as certainty.
- Separate outcome, grade, points, cause, confidence, completeness, rule version and wire provenance.
- Arrest confirmation without a DCS `WIRE#` (human LSO): a completed hook-deflection transient
  correlated with a pendant crossing, or the aircraft stopping relative to the deck, grades the
  trap at medium confidence without inventing a wire number (`arrest_evidence` in JSON/SQLite).
  Validated in replay on 14 live T-45/F-14B(U) passes with hook sidecars.
- Event correlation and positional completeness are independent: an event-stream outage is reported
  as `event_stream_unavailable` and cannot manufacture a positional gap or a favourable outcome.
- Per-recovery source-capture gap, delivery-age, reader-sequence-loss and source-ring-churn
  diagnostics, plus sliding-window telemetry health that identifies sustained gate-capture risk.
- JSON reports, optional compressed Tacview ACMI recordings, and persistent SQLite history.
- Optional Discord reports and terminal session summary. The greenie board is the LSO page of the
  DCS Web Dashboard, which reads `lso.db` directly.
- Offline regeneration of the approach chart from ACMI files created by LSO.

The pass grade is a `PROJECT-DERIVED` training score, never an official USN/USMC certification. It
uses glideslope and lineup deviations at three gates; AoA colours the charts but does not change the grade. See
[AGENTS.md](AGENTS.md), "Gates, outcomes et câble", for the exact behavior.

## Requirements

- Windows or another platform supported by the Rust dependency stack.
- DCS World running the matching DCS-gRPC fork build. This checkout resolves `dcs-grpc-stubs` from
  the fork release tag `v0.10.0` (`sevenfifty777/rust-server`) so the live client and deployed server
  use exactly the same protobuf contract; the compiled stubs version is read back from `Cargo.lock`
  at build time and reported in every JSON as `dcs_grpc_client_stubs`. The server needs
  `recoveryTelemetry.enabled = true` for the default buffered position source; see
  [docs/ADMIN_GUIDE.md](docs/ADMIN_GUIDE.md).
- A Rust stable toolchain only when building from source.

The DCS-gRPC server and this client must use compatible protobuf APIs. Upstream DCS-gRPC 0.8.1 is
not supported by the current build.

## Quick start

Build LSO from the repository root:

```powershell
cargo build --release
New-Item -ItemType Directory -Force C:\LSO\recordings
$env:DCS_GRPC_API_KEY = "<token configured in Config\\dcs-grpc.lua>"
.\target\release\lso.exe run -o C:\LSO\recordings
```

LSO connects to `http://127.0.0.1:50051` by default and retries transient gRPC failures with
exponential backoff. It reads the optional API key from `DCS_GRPC_API_KEY`; change the variable name
with `--api-key-env` or pass an empty name only for an intentionally unauthenticated server. Use
`--uri` when DCS-gRPC is on another host or port.

Common examples:

```powershell
# Save charts and JSON, but not ACMI
.\lso.exe run -o C:\LSO\recordings --no-acmi

# A/B diagnostic: compare the independent hook sampler with the former blocking path
.\lso.exe run -o C:\LSO\recordings --hook-sampling-hz 4 --hook-timeout-ms 300
.\lso.exe run -o C:\LSO\recordings --legacy-inline-hook-sampling

# Minimal acquisition baseline: positions + JSON quality report only
.\lso.exe run -o C:\LSO\recordings --positions-only --baseline-manifest .\baseline.json

# Explicit rollback/control run using the former paired unary polling source
.\lso.exe run -o C:\LSO\recordings --position-source unary

# Share a smaller buffered read budget (server default quota is 20 reads/s per client label)
.\lso.exe run -o C:\LSO\recordings --buffered-read-budget-per-second 12

# Keep normal outputs but suspend redundant same-aircraft detector transforms during collection
.\lso.exe run -o C:\LSO\recordings --suspend-detectors-during-recovery

# Enable debug or trace logging; global flags go before the subcommand
.\lso.exe -v run -o C:\LSO\recordings
.\lso.exe -vv run -o C:\LSO\recordings

# Regenerate an approach PNG from an LSO-created ACMI file
.\lso.exe file C:\LSO\recordings\LSO-20260825-031018-Pilot.zip.acmi

# Offline diagnostic: test an artificially reduced pre-groove sampling cadence against
# already-recorded JSON reports (a file or a directory searched recursively)
.\lso.exe cadence-ab C:\LSO\recordings --stride 2 --stride 4

# Offline read-only comparison of recorded vs current CATOBAR groove entry/duration/geometry
.\lso.exe groove-ab C:\LSO\recordings
```

Use `lso.exe --help` and `lso.exe run --help` for the complete generated CLI reference.
With `--no-acmi`, the TacView writer and ACMI-only metadata/unit RPCs are not started; live grading,
JSON, PNG, SQLite and health diagnostics use the same telemetry path as normal.
`--positions-only` additionally disables event/hook sampling, ACMI, SQLite, PNG, Discord and the
session/dashboard outputs. It does not read `--discord-users`, open or create `lso.db`, or query
output-only DCS metadata; it retains the JSON position report so cadence and latency percentiles can
be compared between live runs. Detector suspension is scoped by aircraft: another aircraft can still
be discovered and recorded concurrently.
The default `--position-source buffered` lifecycle is idempotent `StartRecoveryTelemetry`, ordered
`ReadRecoveryTelemetry` batches with an exclusive sequence cursor, then best-effort
`StopRecoveryTelemetry`. Epoch changes, sequence-contract violations, invalid unit observations and
source retention/capacity loss remain explicit technical evidence in the schema-v9 report. All
concurrent recoveries share one client-side read budget (`--buffered-read-budget-per-second`,
default 16) kept below the server's `recoveryTelemetry.readsPerSecond`; an `id_mismatch`
observation ends the attempt with a typed event. Ctrl-C finalises the passes in flight (up to
30 s) before `lso run` returns. Invalid
source observations retain their source timestamp/entity/status and are attributed at finalization;
receipt time is never substituted for missing source time. `groove-ab` reuses persisted geometry
exactly, but cannot reconstruct unpersisted RPC timing, events, UTC anchors or velocities.
Copy [`docs/BASELINE_MANIFEST.example.json`](docs/BASELINE_MANIFEST.example.json) and fill in the DCS
build, mission/module versions and deployed DLL/Lua hashes to make those comparisons attributable.
Supplied manifests reject unknown keys, empty content and malformed SHA-256 values.

## Output

A completed live pass writes or updates the following items in `--out-dir`:

| Artifact | Purpose |
|---|---|
| `LSO-<date>-<pilot>-<recovery-id>.png` | Final-approach trap sheet |
| `LSO-<date>-<pilot>-<recovery-id>-pattern.png` | Overhead pattern chart |
| `LSO-<date>-<pilot>-<recovery-id>.json` | Schema-v9 result, gates, event/time/hook/wire evidence and telemetry quality |
| `LSO-<date>-<pilot>-<recovery-id>.zip.acmi` | Compressed Tacview recording; omitted with `--no-acmi` |
| `lso.db` | Shared SQLite history in WAL mode; one row is inserted per saved pass. The DCS Web Dashboard reads it directly |

Pilot names in filenames are reduced to ASCII alphanumeric characters. Offline `file` mode writes
only a regenerated approach PNG to the current working directory; it does not update JSON,
SQLite, Discord, or the pattern chart.

Artifact publication is create-if-absent on Windows and Unix. A completed temporary file is linked
atomically into place, so a concurrent producer cannot replace the winner; JSON ownership gates the
matching ACMI, SQLite, render and Discord work for a `recovery_id`.

Build provenance defines `lso_dirty` from tracked Git files only. Modified, staged or deleted tracked
files participate; untracked files (including `target/`) deliberately do not.

## Supported units

| Aircraft | DCS type names |
|---|---|
| F/A-18C Hornet | `FA-18C_hornet` |
| F-14A Tomcat | `F-14A-135-GR`, `F-14A-135-GR-Early`, `F-14A-95-GR` |
| F-14B Tomcat | `F-14B`, `F-14A/B` |
| F-14B(U) Tomcat | `F-14B(U)`, `F-14BU` |
| VNAO T-45C Goshawk | `T-45` |
| AV-8B NA (Tarawa only) | `AV8BNA` |

| Carrier geometry | DCS type names |
|---|---|
| Nimitz-class | `CVN_71`, `CVN_72`, `CVN_73`, `CVN_75`, `Stennis` |
| Forrestal | `Forrestal` |
| Tarawa (AV-8B only) | `LHA_Tarawa` |

Unsupported types are ignored. `Stennis` is DCS's type name for CVN-74.

## Greenie board and Discord

The embedded loopback web board (`--web-port`) was removed in 0.5.0. The greenie board is now the
LSO page of the DCS Web Dashboard, which opens `<out-dir>/lso.db` read-only; point its `LSO_DIR` at
the same `--out-dir`. The database is opened in SQLite WAL mode with a 2 s busy timeout so the
dashboard can query while a pass is being inserted. Passing `--web-port` or `--web-expose-ucid`
now stops LSO with a message that says so, instead of silently running without a board.

Discord delivery is enabled with `--discord-webhook`. Keep webhook URLs out of source control,
screenshots, logs, and shared command transcripts. `--discord-users` accepts a JSON map from DCS
pilot names to Discord numeric user IDs; it is optional.

## Documentation

- [Installation and administration](docs/ADMIN_GUIDE.md) — server release, config keys, run, stop, roll back
- [Full technical reference](AGENTS.md) — architecture, contracts, grading, build, deployment, benchmark
- [Project overview and grading logic for non-developers](primer.md) (French)
- [Contributing](CONTRIBUTING.md)
- [Changelog](CHANGES.md)

## License

LSO is licensed under the [GNU Affero General Public License v3.0](LICENSE).
