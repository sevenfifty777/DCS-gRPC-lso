# Technical architecture

LSO is a Rust/Tokio binary with live (`lso run`) and deterministic ACMI replay (`lso file`) paths.
The reliability implementation is organized as:

```text
DCS-gRPC v0.9.0
  -> session/generation supervisor and strict discovery
  -> one isolated detector per compatible aircraft/carrier pair
  -> 10 Hz recovery recorder + correlated mission events
  -> time alignment and bounded Track state
  -> gates, evidence, outcome, completeness and project score
  -> idempotent SQLite + atomic local artifacts
  -> blocking-worker PNG + optional Discord + loopback dashboard
```

| Module | Responsibility |
|---|---|
| `commands/run.rs` | generation/session lifecycle, discovery, strict pairing, identity, watchdog |
| `tasks/detect_recovery_attempt.rs` | two-second wide-envelope discovery, locally supervised passes |
| `tasks/record_recovery.rs` | 10 Hz sampling, event correlation, outputs and publication isolation |
| `telemetry.rs` | monotonic freshness, skew policy, extrapolation and cut reset |
| `track.rs` | bounded state, geometry, gates, evidence, outcomes and wire sources |
| `grading.rs` | project score and experimental V/STOL model |
| `db.rs` | additive migrations and idempotent private persistence |
| `metrics.rs` | RPC/stream/queue/IO/render instrumentation |
| `web.rs` | loopback-only private dashboard |

Detailed current contracts:

- [Reliability architecture and state machines](RELIABILITY_ARCHITECTURE.md)
- [Data contracts and migrations](DATA_CONTRACTS.md)
- [Grading provenance and formulas](GRADING_REFERENCE.md)
- [Benchmark protocol](BENCHMARK_PROTOCOL.md)
- [Live corpus and version manifest](LIVE_VALIDATION.md)
- [Deployment and rollback runbook](DEPLOYMENT_ROLLBACK.md)

The active tracking cadence remains 10 Hz. Invalid Cartesian pairs are not created. Transform cache
or stream-based discovery optimization is intentionally deferred until the live benchmark shows a
benefit without increasing gap, skew, missing data or correlation errors.

This architecture is locally tested, not production-certified. The deployed DCS/DCS-gRPC manifest,
Tarawa event semantics, hook polarity, load/FPS benchmark and timed rollback remain external gates.
