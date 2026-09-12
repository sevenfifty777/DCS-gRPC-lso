# Contributing to DCS-gRPC LSO

Please report bugs and incorrect wire detections in the
[sevenfifty777/DCS-gRPC-lso issue tracker](https://github.com/sevenfifty777/DCS-gRPC-lso/issues).
For a wire-detection report, attach an LSO-generated `.zip.acmi` recording when it is safe to share
and describe the expected and observed wire.

## Development setup

Install a stable Rust toolchain, clone the repository, and run from the repository root. `Cargo.toml`
currently pins the DCS-gRPC stubs by local path (`../rust-server/stubs`), so clone the
[sevenfifty777/rust-server](https://github.com/sevenfifty777/rust-server) fork (branch
`hook-mechanization-api`, 0.9.1 plus `RecoveryService`/`HookService`) next to this repository before
building. A tagged `v0.9.2` release will replace the path pin.

```powershell
cargo build
cargo run -- run
```

Global logging flags must appear before the subcommand:

```powershell
cargo run -- -v run
cargo run -- -vv run
```

Replay mode accepts only ACMI recordings created by LSO:

```powershell
cargo run -- file .\tests\recordings\wire_3_01_T45.zip.acmi
```

The regenerated approach image is written to the current working directory.

## Validation

Run the smallest relevant test first, then the repository checks before opening a pull request:

```powershell
cargo test <test_name>
cargo test
cargo fmt -- --check
cargo clippy -- -D warnings
```

To regenerate visual test artifacts for manual inspection:

```powershell
cargo test generate_chart_images -- --nocapture
```

The images are written under `target/test-charts/` and are not source files.

When dependencies change, also run `cargo audit` if `cargo-audit` is installed (CI runs it as a
non-blocking job). Do not change the DCS-gRPC stubs pin (path today, release tag once `v0.9.2` is
published) without reviewing protobuf compatibility and updating the migration and administrator
documentation.

## Change guidelines

- Keep live and ACMI replay geometry deterministic where the same input is available.
- Add focused tests for grading, geometry, parsing, or supported-unit changes.
- Update the README, administrator guide, grading reference, or technical analysis when behavior,
  CLI flags, output fields, network binding, or supported units change.
- Do not commit Discord webhook URLs, credentials, local databases, generated charts, logs, or
  private recordings.
- Preserve the bundled `docs/DCS-gRPC-0.9.1/` reference snapshot unless deliberately replacing the
  pinned server version.

This repository is licensed under the [GNU AGPL v3](LICENSE). Contributions are submitted under
the same license.
