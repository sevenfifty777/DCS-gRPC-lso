# DCS-gRPC Fork Migration

**Date:** 2026-08-25; updated for source-buffered telemetry on 2026-09-02
**Status:** 0.9 migration historically validated; 0.10 buffered migration implemented locally and awaiting live validation

**LSO version:** 0.2.0

**Current server/stubs version:** local 0.10.0

**Current server commit:** `c6fb3f7737f48c82601866f696d7df66ac727414` (not yet published)

**Historical official baseline:** [`v0.9.0`](https://github.com/sevenfifty777/rust-server/releases/tag/v0.9.0), commit `5bd6d6e42491c8697a5c5a95e80a2e689923bd3b`

> Sections describing the 0.9 migration are retained as history. The active local validation target
> is the 0.10.0 source-buffered contract. LSO currently uses `path = "../DCS-gRPC/stubs"` so the two
> local checkouts cannot drift; this must become a reviewed immutable remote `rev` or tag before a
> portable release is built.

## 1. Purpose

This migration changes LSO from the upstream DCS-gRPC 0.8.1 Rust stubs to the customized
[`sevenfifty777/rust-server`](https://github.com/sevenfifty777/rust-server) fork. The fork is based
on DCS-gRPC 0.9.0 and contains additional APIs and server fixes required for future development.

The migration covers more than changing a Git URL. DCS-gRPC 0.9.0 uses a newer gRPC toolchain and
contains a protobuf compatibility change that makes `dcs.common.v0.Unit.type` optional. LSO was
updated to compile against that interface and to behave safely when DCS does not supply a unit type.

## 2. Dependency Change

### Before

```toml
tonic = "0.11"

[dependencies.stubs]
package = "dcs-grpc-stubs"
git = "https://github.com/DCS-gRPC/rust-server.git"
rev = "0.8.1"
features = ["client"]
```

The `0.8.1` revision resolved to upstream commit
`803b06035887cd558a8b9bade68d240c1b705df1`.

### After

```toml
tonic = "0.13"

[dependencies.stubs]
package = "dcs-grpc-stubs"
git = "https://github.com/sevenfifty777/rust-server.git"
tag = "v0.9.0"
features = ["client"]
```

The manifest selects the official release tag instead of `main`. `Cargo.lock` records the resolved
commit, so locked builds continue to use the reviewed source rather than following later branch
changes. At the original migration on 2026-08-25, the fork had no `v0.9.0` tag or GitHub release, so
commit `11aea3484099c2dd21d41a53db2e510f6e5e84c5` was used provisionally. The official annotated tag
published on 2026-08-28 resolves to `5bd6d6e42491c8697a5c5a95e80a2e689923bd3b`.

## 3. Why `tonic` Was Upgraded

The fork's `dcs-grpc-stubs` crate declares `tonic = "0.13"`, `prost = "0.13"`, and
`prost-types = "0.13"`. Generated service clients are parameterized by the transport types from
their version of `tonic`. LSO therefore must use the same major/minor `tonic` API when it passes a
`tonic::transport::Channel` to clients such as `MissionServiceClient`, `UnitServiceClient`, and
`WorldServiceClient`.

The direct LSO dependency was upgraded from `tonic 0.11` to `tonic 0.13`. Cargo resolved the exact
version to `tonic 0.13.1`. This also updated the associated `prost`, `hyper`, `h2`, `tower`, and Axum
transport dependencies in `Cargo.lock`.

## 4. DCS API Compatibility Change

### Changed protobuf field

In the 0.9.0 generated API, the DCS unit type is optional:

```text
dcs.common.v0.Unit.type: optional string
```

The generated Rust representation consequently changed from:

```rust
pub r#type: String
```

to:

```rust
pub r#type: Option<String>
```

The field is optional because DCS does not guarantee that every exported object supplies a type.
The first compatibility build identified five locations that still expected a mandatory `String`.

### Candidate discovery behavior

`src/commands/run.rs` now checks `Unit.type` before attempting to match an aircraft or carrier:

- A unit without a DCS type is ignored as a recovery candidate.
- The condition is logged at `debug` level with the unit ID and unit name.
- No `unwrap()` or panic is used.
- A valid aircraft type is stored in the `Candidate::Plane` value so later recovery-task creation
  receives a guaranteed `String`.
- Both initial mission discovery and later `Birth` events use the same validated value.

Ignoring an untyped unit is necessary because LSO selects aircraft and carrier geometry through
`AirplaneInfo::by_type()` and `CarrierInfo::by_type()`. Without a type, LSO cannot safely choose the
correct hook offset, cable positions, glideslope, or carrier dimensions.

### ACMI object-name fallback

`src/tasks/record_recovery.rs` previously used `Unit.type` directly as the Tacview object name.
It now uses the DCS unit name when the optional type is absent:

```rust
Property::Name(unit.r#type.unwrap_or_else(|| unit.name.clone()))
```

This preserves useful ACMI output for an object that can be queried but has no exported DCS type.

## 5. Lockfile and Security Updates

`Cargo.lock` was regenerated from the forked stubs and the `tonic 0.13` dependency graph. The
resulting stubs entry is:

```text
dcs-grpc-stubs 0.9.0
git+https://github.com/sevenfifty777/rust-server.git
tag v0.9.0
commit 5bd6d6e42491c8697a5c5a95e80a2e689923bd3b
```

The initial RustSec scan found two vulnerable transitive packages and one unsound package warning.
They were updated to their verified patched versions:

| Package | Previous | Updated | Reason |
|---|---:|---:|---|
| `crossbeam-epoch` | 0.9.18 | 0.9.20 | Fixes [RUSTSEC-2026-0204](https://rustsec.org/advisories/RUSTSEC-2026-0204.html), an invalid pointer dereference in pointer formatting |
| `quinn-proto` | 0.11.14 | 0.11.15 | Fixes high-severity [RUSTSEC-2026-0185](https://rustsec.org/advisories/RUSTSEC-2026-0185.html), remote memory exhaustion during out-of-order stream reassembly |
| `anyhow` | 1.0.102 | 1.0.103 | Fixes [RUSTSEC-2026-0190](https://rustsec.org/advisories/RUSTSEC-2026-0190.html), unsoundness in `Error::downcast_mut()` |

The final audit reported zero known vulnerabilities. It retained one allowed maintenance warning:

- `ttf-parser 0.20.0` is marked unmaintained by
  [RUSTSEC-2026-0192](https://rustsec.org/advisories/RUSTSEC-2026-0192.html).
- It is pulled in through `plotters 0.3.7` and is not a direct LSO dependency.
- The warning does not describe a known exploitable vulnerability, but it should be reviewed when
  Plotters or the chart-rendering stack is next upgraded.

### Official release tag review

The official release was published on 2026-08-28, so it was still inside the normal observation
window for a new dependency release when this update was requested. The source diff from the
provisional commit to the tag target was therefore reviewed before the lockfile was changed:

- `stubs/Cargo.toml` and `stubs/build.rs` did not change, so the stubs introduced no new direct,
  build, or transitive dependency requirements.
- The protobuf edits are formatting and lint comments; they do not change field numbers, field
  types, service names, or RPC signatures.
- The stubs library change is module ordering plus a Clippy allowance for generated tonic methods.
- The material server behavior change is the stale-unit/SRS error-handling fix included in the
  official release. It does not alter the LSO client API.
- All intervening commits were authored through the same `sevenfifty777` GitHub account. The
  annotated tag is not cryptographically signed, so the lockfile's exact commit remains important.

## 6. Files Changed

| File | Change |
|---|---|
| `Cargo.toml` | Points `dcs-grpc-stubs` to the official fork tag and upgrades `tonic` to 0.13 |
| `Cargo.lock` | Records stubs 0.9.0, the resolved release commit, the new gRPC stack, and patched transitive packages |
| `src/commands/run.rs` | Handles optional `Unit.type` during initial discovery and `Birth` events |
| `src/tasks/record_recovery.rs` | Adds a safe ACMI name fallback when `Unit.type` is absent |
| `README.md` | States that the forked DCS-gRPC 0.9.0 server is required |
| `docs/ADMIN_GUIDE.md` | Updates installation requirements, repository instructions, and the official release baseline |
| `docs/LSO_ANALYSIS.md` | Updates the architecture and dependency descriptions |
| `docs/analysis2.md` | Updates the recorded server, stubs, and `tonic` versions |

The supplied 0.9.0 server documentation under `docs/DCS-gRPC-0.9.0/` remains a reference copy and
was not rewritten as part of this migration.

## 7. Validation Performed

### Dependency resolution

```powershell
cargo update -p dcs-grpc-stubs
```

Result: the fork was fetched successfully and `Cargo.lock` resolved `dcs-grpc-stubs 0.9.0` from tag
`v0.9.0` at the exact release commit.

### Full test suite

```powershell
cargo test --locked --no-fail-fast
```

Result:

```text
55 passed; 0 failed; 0 ignored
```

This validates compilation of the generated 0.9.0 clients and all existing unit tests, including
carrier cable geometry and grading tests. It does not replace a live integration test against a
running DCS World server.

### Security audit

```powershell
cargo audit
```

Final result:

```text
0 vulnerabilities
1 allowed warning: ttf-parser 0.20.0 is unmaintained
```

### Reference and whitespace checks

The active manifest and project documentation were scanned for the former upstream URL, the 0.8.1
requirement, and the provisional `11aea348...` pin. No active reference remains, except historical
text that explicitly records the previous dependency state. `git diff --check` also completed
successfully.

The repository-wide `cargo fmt -- --check` command still reports formatting drift in pre-existing
source files. Broad automatic formatting was deliberately not applied because it would create
unrelated runtime-code changes. On 2026-08-28, `cargo clippy --locked -- -D warnings` continued to
report the same seven existing code-quality findings; see `docs/analysis_results.md` for the current
validation summary.

## 8. Runtime Deployment Requirement

The Rust client stubs and the DCS server installation should describe the same protocol version.
Deploy the official release together with the LSO binary built from this lockfile:

```text
https://github.com/sevenfifty777/rust-server/releases/tag/v0.9.0
tag v0.9.0
locked commit 5bd6d6e42491c8697a5c5a95e80a2e689923bd3b
```

Using upstream DCS-gRPC 0.8.1 with this LSO build is no longer the supported configuration. A
server/client mismatch may result in unimplemented RPCs, incompatible message fields, or different
runtime behavior even when the TCP connection itself succeeds.

## 9. Future Fork Upgrade Procedure

When a newer fork release is ready:

1. Review the fork history, release tag, and exact target commit SHA.
2. Review `Cargo.toml`, `stubs/Cargo.toml`, protobuf changes, build dependencies, and the changelog
   in the server repository.
3. Update only the `tag` value in the LSO `[dependencies.stubs]` section unless the fork also changes
   its required `tonic` version.
4. Regenerate the lockfile:

   ```powershell
   cargo update -p dcs-grpc-stubs
   ```

5. Compile and run all tests:

   ```powershell
   cargo test
   ```

6. Run the dependency audit:

   ```powershell
   cargo audit
   ```

7. Confirm that `Cargo.lock` records the intended full commit SHA.
8. Update the version and commit references in `README.md`, `docs/ADMIN_GUIDE.md`, and the analysis
   documentation.
9. Perform a live smoke test against DCS World before deploying the new LSO binary to the server.

## 10. Rollback Reference

If a temporary rollback to the former upstream stubs is required, restore these manifest values:

```toml
tonic = "0.11"

[dependencies.stubs]
package = "dcs-grpc-stubs"
git = "https://github.com/DCS-gRPC/rust-server.git"
rev = "0.8.1"
features = ["client"]
```

Then regenerate `Cargo.lock`, revert the optional-`Unit.type` compatibility code only if the old
generated structs require it, and run `cargo test` plus `cargo audit`. This rollback is documented
for recovery purposes; the supported target after this migration is the pinned 0.9.0 fork.
