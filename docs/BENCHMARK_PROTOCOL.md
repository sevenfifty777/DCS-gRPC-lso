# Benchmark and optimization protocol

Optimization is accepted only after a reproducible baseline and only when telemetry quality does
not regress. Offline replay can validate CPU/RAM/render/IO mechanics; RPC, streams, skew and DCS
FPS/tick require the live server corpus.

## Built-in metrics

Live mode logs a cumulative snapshot every ten seconds:

- unary RPC calls and RPC/s;
- `GetTransform` calls, errors and mean latency;
- active event streams and active recoveries;
- supervisor queue high-water mark (capacity 16);
- bytes written by atomic ACMI/JSON output;
- PNG render count and mean render time.

Track reports add maximum sample gap/skew, invalid/warning/dropped counts and bounded-buffer
completeness. PNG rendering and SQLite calls run in blocking workers, outside 10 Hz sampling tasks.

## Offline baseline

Build once, then run each fixture at least 20 times from a clean output directory. Record median,
p95 and maximum. On Windows PowerShell:

```powershell
cargo build --release --locked
$samples = 1..20 | ForEach-Object {
  $process = Start-Process -FilePath .\target\release\lso.exe `
    -ArgumentList @('file','tests\recordings\wire_3_01_T45.zip.acmi') `
    -PassThru -NoNewWindow
  $process.WaitForExit()
  $process.Refresh()
  [pscustomobject]@{
    ExitCode = $process.ExitCode
    CPU_s = $process.TotalProcessorTime.TotalSeconds
    PeakWorkingSet_MB = $process.PeakWorkingSet64 / 1MB
  }
}
$samples | Measure-Object CPU_s,PeakWorkingSet_MB -Average -Minimum -Maximum
```

Also record input/output bytes and wall time. Keep the exact Git diff, Rust version, build profile,
CPU model, RAM and OS with results.

## Live matrix

Run identical missions for baseline and candidate builds:

1. one Hornet/CVN recovery;
2. one AV-8B/Tarawa recovery;
3. simultaneous Hornet/CVN and AV-8B/Tarawa;
4. 40 players with two ships;
5. stress case with three carriers;
6. deliberate gRPC delay, 300/1,000 ms gaps, reconnect and mission rotation.

Capture at one-second resolution where possible:

- process CPU percent and working set/private bytes;
- runtime metrics above, including RPC/s and stream count;
- disk bytes/s and queue/buffer high-water marks;
- p50/p95/p99 transform latency, skew and sample gap;
- PNG render duration;
- DCS server FPS/simulation tick from the agreed server tool.

Each run needs at least ten minutes steady state plus all recoveries. Preserve anonymized raw logs and
mission hashes.

## Acceptance gates

An optimization is rejected if any scenario increases:

- missing/invalid/stale samples;
- maximum or p95 skew/gap;
- correlation errors or duplicate outputs;
- cross-recovery interaction;
- queue overflow;
- recovery task restarts.

The 10 Hz active cadence is fixed. `StreamUnits` may only replace discovery/prefilter work after a
measured experiment. A shared transform cache must key by unit and enforce freshness; it must not
reuse data across session/generation or an RPC cut.

## Optimizations already safe by invariant

The strict compatibility matrix removes known-invalid Cartesian pairs; this is required correctness,
not a benchmark-derived cadence reduction. Bounded buffers prevent unbounded memory growth. Atomic
writes, idempotent DB insertion and `spawn_blocking` rendering protect correctness under concurrent
passes. Further transform caching, RPC deduplication or allocation work remains contingent on live
measurements.

No claim is made yet for 40-player/three-carrier load or DCS FPS/tick impact.

## Local before/after result (reference measured 2026-08-30, candidate remeasured 2026-08-31)

This microbenchmark used the 11,629-byte `wire_3_01_T45.zip.acmi` fixture, a locked release build,
20 fresh processes per revision and the same host. Environment: Rust 1.98.0 x86-64 MSVC, Windows
10.0.19045, Intel64 family 6 model 151, 20 logical processors. Working set was sampled every 10 ms;
Windows CPU-time resolution is visible in 15.625 ms steps.

| Revision | Failures | Wall median / p95 | CPU median / p95 | Peak working set median / p95 |
|---|---:|---:|---:|---:|
| reference `95f1d27...` | 0/20 | 170.454 / 201.571 ms | 140.625 / 140.625 ms | 15.936 / 16.551 MiB |
| reliability implementation | 0/20 | 175.732 / 188.499 ms | 140.625 / 171.875 ms | 15.756 / 16.336 MiB |

Median wall time changed by +5.278 ms (+3.10%), median CPU was unchanged at the measurement
resolution, and median peak working set changed by -0.180 MiB (-1.13%). This single-fixture result
does not justify additional transform/RPC optimization and says nothing about live DCS FPS, RPC
latency, streams or multi-recovery load. The reference source was built from an isolated `git
archive`; the working tree and user-modified analysis files were not reset or copied over.
