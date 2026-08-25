# DCS-gRPC LSO

LSO is a Rust command-line tool that monitors DCS carrier recoveries through the
[sevenfifty777 DCS-gRPC fork](https://github.com/sevenfifty777/rust-server). It records the Case I
pattern and final approach, estimates the arresting wire, applies a simplified gate-based pass
grade, produces trap-sheet images and structured output, and can publish a greenie board locally
or through Discord.

![LSO example report](docs/example.png)

## Current capabilities

- Live monitoring of every supported carrier and player aircraft pair, including units spawned
  after LSO starts.
- Full-pattern detection inside 3.5 nm and below 1,100 ft MSL, followed by 10 Hz recording.
- Carrier-relative final-approach and overhead-pattern PNG charts with AoA-coloured tracks.
- Gate samples at 3/4, 1/2, and 1/4 nm and simplified grades `_OK_`, `OK`, `(OK)`, `--`, `C`, `B`,
  and `WO`.
- Recovered, bolter, pilot-waveoff, and hook-up qualification/touch-and-go outcomes.
- JSON reports, optional compressed Tacview ACMI recordings, and persistent SQLite history.
- Optional Discord reports, terminal session summary, and HTTP greenie board.
- Offline regeneration of the approach chart from ACMI files created by LSO.

The pass grade is geometric and intentionally simplified. It uses glideslope and lineup deviations
at three gates; AoA colours the charts but does not currently change the grade. See
[the grading reference](docs/GRADING_REFERENCE.md) for the exact behavior.

## Requirements

- Windows or another platform supported by the Rust dependency stack.
- DCS World with the forked DCS-gRPC server version `0.9.0`, pinned at commit
  `11aea3484099c2dd21d41a53db2e510f6e5e84c5`.
- A Rust stable toolchain only when building from source.

The DCS-gRPC server and this client must use compatible protobuf APIs. Upstream DCS-gRPC 0.8.1 is
not supported by the current build.

## Quick start

Build LSO from the repository root:

```powershell
cargo build --release
New-Item -ItemType Directory -Force C:\LSO\recordings
.\target\release\lso.exe run -o C:\LSO\recordings
```

LSO connects to `http://127.0.0.1:50051` by default and retries transient gRPC failures with
exponential backoff. Use `--uri` when DCS-gRPC is on another host or port.

Common examples:

```powershell
# Include the persistent web board at http://localhost:8080
.\lso.exe run -o C:\LSO\recordings --web-port 8080

# Save charts and JSON, but not ACMI
.\lso.exe run -o C:\LSO\recordings --no-acmi

# Enable debug or trace logging; global flags go before the subcommand
.\lso.exe -v run -o C:\LSO\recordings
.\lso.exe -vv run -o C:\LSO\recordings

# Regenerate an approach PNG from an LSO-created ACMI file
.\lso.exe file C:\LSO\recordings\LSO-20260825-031018-Pilot.zip.acmi
```

Use `lso.exe --help` and `lso.exe run --help` for the complete generated CLI reference.

## Output

A completed live pass writes or updates the following items in `--out-dir`:

| Artifact | Purpose |
|---|---|
| `LSO-<date>-<time>-<pilot>.png` | Final-approach trap sheet |
| `LSO-<date>-<time>-<pilot>-pattern.png` | Overhead pattern chart |
| `LSO-<date>-<time>-<pilot>.json` | Pilot, outcome, grade enum, gate deviations, final-approach datums, and mission time |
| `LSO-<date>-<time>-<pilot>.zip.acmi` | Compressed Tacview recording; omitted with `--no-acmi` |
| `lso.db` | Shared SQLite history; one row is inserted per saved pass |

Pilot names in filenames are reduced to ASCII alphanumeric characters. Offline `file` mode writes
only a regenerated approach PNG to the current working directory; it does not update JSON,
SQLite, Discord, or the pattern chart.

## Supported units

| Aircraft | DCS type names |
|---|---|
| F/A-18C Hornet | `FA-18C_hornet` |
| F-14A Tomcat | `F-14A-135-GR`, `F-14A-135-GR-Early`, `F-14A-95-GR` |
| F-14B Tomcat | `F-14B`, `F-14A/B` |
| F-14B(U) Tomcat | `F-14B(U)`, `F-14BU` |
| VNAO T-45C Goshawk | `T-45` |

| Carrier geometry | DCS type names |
|---|---|
| Nimitz-class | `CVN_71`, `CVN_72`, `CVN_73`, `CVN_75`, `Stennis` |
| Forrestal | `Forrestal` |

Unsupported types are ignored. `Stennis` is DCS's type name for CVN-74.

## Web and Discord

`--web-port <PORT>` serves `/` and `/api/passes` and refreshes the browser board every 10 seconds.
The server binds to `0.0.0.0`, has no authentication or TLS, and should not be exposed directly to
the public internet. Restrict it with a firewall or place it behind an authenticated reverse proxy
on a different listening port.

Discord delivery is enabled with `--discord-webhook`. Keep webhook URLs out of source control,
screenshots, logs, and shared command transcripts. `--discord-users` accepts a JSON map from DCS
pilot names to Discord numeric user IDs; it is optional.

## Documentation

- [Installation and administration](docs/ADMIN_GUIDE.md)
- [Technical architecture](docs/LSO_ANALYSIS.md)
- [Grading behavior](docs/GRADING_REFERENCE.md)
- [DCS-gRPC fork migration](docs/DCS_GRPC_FORK_MIGRATION.md)
- [Contributing](CONTRIBUTING.md)
- [Changelog](CHANGES.md)

`docs/DCS-gRPC-0.9.0/` is a bundled upstream/fork reference snapshot and is not maintained as LSO
documentation.

## License

LSO is licensed under the [GNU Affero General Public License v3.0](LICENSE).
