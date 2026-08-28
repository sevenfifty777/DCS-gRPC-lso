# Documentation Audit Results

**Audit date:** 2026-08-25

**Dependency baseline refreshed:** 2026-08-28

**Implementation baseline:** crate `0.2.0`, commit `b9ac263` before documentation changes

**Excluded by request:** `docs/analys-codex-justice/`

## Scope

The audit compared first-party Markdown and HTML documentation with the Rust source, manifest,
lockfile, CI workflow, test fixtures, generated CLI help, and the bundled DCS-gRPC 0.9.0 reference.
The `docs/DCS-gRPC-0.9.0/` tree was treated as an immutable dependency snapshot rather than LSO
documentation.

## Main discrepancies found

| Area | Outdated documentation | Verified implementation |
|---|---|---|
| Product status | Early prototype that only reports wire/AoA | Gate grading, outcomes, two charts, JSON, SQLite, Discord, terminal and web boards |
| Detection | 1.5 nm, 500 ft, behind carrier, nose aimed at carrier | 200 m to 3.5 nm, at or below 1,100 ft MSL, any heading/quadrant |
| Saved artifacts | Two files | Two PNGs, JSON, optional ACMI, plus one row in shared `lso.db` |
| Grading | Foot thresholds, `Fair`/`NG`, no points | Degree thresholds and `_OK_`/`OK`/`(OK)`/`--`/`C`/`B`/`WO` points |
| Outcomes | Recovered or bolter only | Recovered, bolter, pilot waveoff, and hook-up qualification bolter |
| Replay | Regenerates reports beside input | Regenerates only the approach PNG in the current working directory |
| Pattern chart | Not present or clipped at old ranges | Separate 900x900 PNG, +/-2.5 nm port/starboard and +/-3 nm ahead/astern |
| Web board | Planned | Implemented on `0.0.0.0:<port>`, unauthenticated, HTTP only |
| Database | Minimal schema or planned | 14 columns including UCID, aircraft ID/display name, map, UTC/mission time, points, and outcome |
| DCS-gRPC | Upstream 0.8.1 | official sevenfifty777 fork tag `v0.9.0`, locked at `5bd6d6e...` |
| Links | Machine-local `file:///c:/...` links | Repository-relative links |
| Development commands | `cargo run -- run -vv` and `cargo run --file ...` | `cargo run -- -vv run` and `cargo run -- file ...` |

## Documents updated

| Document | Result |
|---|---|
| `README.md` | Rewritten as the current product overview and quick start |
| `CONTRIBUTING.md` | Corrected commands, validation, security, and repository links |
| `CHANGES.md` | Replaced stale feature-design detail with current unreleased changes |
| `docs/ADMIN_GUIDE.md` | Reconciled installation, CLI, artifacts, JSON/API/schema, network, service, and troubleshooting guidance |
| `docs/LSO_ANALYSIS.md` | Rewritten as the canonical code-level analysis |
| `docs/GRADING_REFERENCE.md` | Reconciled trigger, gate, outcome, threshold, and support tables |
| `docs/GRADING_REFERENCE.html` | Rebuilt as a lightweight companion pointing to the canonical Markdown specification |
| `docs/GRADING_ANALYSIS.md` | Converted from a stale proposal to an implemented-design record |
| `docs/analysis.md`, `docs/analysis2.md` | Retained as compatibility pages pointing to the canonical analysis |
| Implementation plan/walkthrough files | Converted from proposals or stale validation notes to completed implementation records |
| `docs/DCS_GRPC_FORK_MIGRATION.md` | Kept as the detailed dependency migration record; validation wording refreshed where needed |

## Code review observations

The documentation refresh did not modify runtime code.

| Priority | Finding | Evidence and impact |
|---|---|---|
| High - verify | Cable estimation constructs `DRotor3::from_rotation_xz(-deck_angle)` without converting the degree-valued carrier angle to radians. Other rotation sites call `.to_radians()`, and Ultraviolet feeds this parameter to `sin_cos`. | The existing five ACMI wire fixtures all pass, so this needs a focused direction/angle test before changing behavior. If confirmed, the estimated deck-forward vector can be wrong. |
| High - operations | The HTTP board is unauthenticated HTTP and binds to every interface. | Firewall/reverse-proxy isolation is required; direct internet exposure is unsafe. |
| Medium | The web API converts a failed blocking task or database query to an empty list. | Operators can mistake a database failure for a genuinely empty board. |
| Medium | Additive database migrations discard every `ALTER TABLE` error, not only duplicate-column errors. | A real migration failure can remain hidden until a later query or insert fails. |
| Medium | Command execution errors are unwrapped in `main`. | Fatal errors produce panic-style output instead of a concise diagnostic and exit code path. |
| Medium | Discord delivery occurs after files and the database row are written; webhook failure propagates to the outer retry loop. | A delivery failure can restart live monitoring after local persistence and deserves duplicate/operational testing. |
| Low | `GS_SLIGHT_LOW` is `0.5`, but its nearby comment mentions MOOSE `0.8`, and a test is named `test_slight_gs_low_threshold_is_0_8` while using `-0.9`. | The test does not distinguish a 0.5 threshold from 0.8; documentation follows the executable 0.5 value. |
| Coverage | No focused tests were found for `IntentionalBolter`, database migrations, HTTP error behavior, Discord delivery, or the deck-angle unit passed to cable estimation. | These paths remain dependent on broader or live integration behavior. |

Additional documented limitations:

- Grade computation does not use AoA, wind, trend, power, or sink rate.
- All plane/carrier combinations are monitored; intended-carrier disambiguation is not implemented.
- Offline replay reproduces only the approach chart.
- JSON omits pattern datums and several values available in the database or Discord embed.

## Validation results

| Check | Result |
|---|---|
| `cargo test --locked --no-fail-fast` | Passed: 55 passed, 0 failed |
| Local Markdown/HTML link check | Passed: all repository-relative targets resolve |
| `git diff --check` | Passed; Git reports only Windows LF-to-CRLF notices |
| `cargo audit` | 0 vulnerabilities; one allowed unmaintained warning for `ttf-parser 0.20.0` (`RUSTSEC-2026-0192`) |
| `cargo fmt -- --check` | Failed on pre-existing formatting drift across Rust sources; no source formatting was applied |
| `cargo clippy --locked -- -D warnings` | Failed with 7 pre-existing findings: three large error results, type complexity, drain/collect, manual range containment, and derivable `Default` |

## Canonical documentation map

- Operator overview: [README](../README.md)
- Installation and administration: [ADMIN_GUIDE.md](ADMIN_GUIDE.md)
- Architecture and limitations: [LSO_ANALYSIS.md](LSO_ANALYSIS.md)
- Grade behavior: [GRADING_REFERENCE.md](GRADING_REFERENCE.md)
- DCS-gRPC migration: [DCS_GRPC_FORK_MIGRATION.md](DCS_GRPC_FORK_MIGRATION.md)

Historical design records are explicitly labelled and should not be used as current operating
instructions.
