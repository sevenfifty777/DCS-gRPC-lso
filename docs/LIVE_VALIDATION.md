# Live corpus collection and version manifest

## Current manifest

| Component | Known value | Status |
|---|---|---|
| LSO base revision | `b8533927494aad5868f2197ce5a29131b64f30eb` | known; buffered-client changes are the current working tree |
| working branch | `feature/refonte-v3-lua-buffer` | known |
| DCS-gRPC server revision | `c6fb3f7737f48c82601866f696d7df66ac727414` | local committed `0.10.0` build |
| Rust DCS-gRPC stubs | sibling `../DCS-gRPC/stubs`, version `0.10.0` | exact local contract; immutable remote pin still required for publication |
| protobuf | DCS-gRPC recovery telemetry `0.10.0` | start/read/stop batch contract |
| deployed DCS build | not authenticated | required live evidence |
| deployed `dcs_grpc.dll` SHA-256 | not authenticated | required live evidence |
| deployed Lua tree SHA-256 | not authenticated | required live evidence |
| mission `.miz` SHA-256 | per-capture | required |
| aircraft module versions | not authenticated | required live evidence |

Do not promote the local sibling dependency as a distributable release. Publish/review the server
commit first, replace the path with its immutable remote revision, regenerate the lockfile, and
capture the deployed DLL/Lua manifest.

## Capture package

For each test, create one directory named with UTC time and scenario ID. It must contain:

- DCS build/module versions and mission hash;
- SHA-256 of deployed DLL and every DCS-gRPC Lua file;
- DCS log, DCS-gRPC log, LSO trace log, JSON, optional ACMI and event observer log;
- synchronized UTC start/end plus DCS session ID/generation;
- aircraft/carrier unit IDs and anonymized pilot token (never UCID);
- expected scenario actions written before reviewing results.

Keep an unshared private mapping only when investigators need to relate the anonymized token to a
test participant. Never place UCIDs in fixtures, documentation, PNGs, ACMI or issue trackers.

## Required scenarios

Run at least:

1. nominal AV-8B vertical landing on Tarawa;
2. rolling vertical landing;
3. bounce/double contact;
4. touch-and-go and go-around;
5. absent, duplicated, late and reordered `Land`/`RunwayTouch`/LQM;
6. hook argument through groove/final for every supported CATOBAR module;
7. wires 1-4 with DCS LQM and independent observer truth;
8. carrier straight, turning and accelerating around 100/300 ms skew;
9. reconnect, mission rotation, player leave/respawn/slot change and homonyms;
10. simultaneous Hornet/CVN and AV-8B/Tarawa;
11. 40 players/two ships and three-carrier stress.
12. the same Hornet/CVN recovery in independent and `--legacy-inline-hook-sampling` modes;
13. both modes with `--no-acmi`, a delayed hook RPC and frozen transform source timestamps.
14. `--position-source unary` control versus the default buffered source on the identical mission;
15. buffered delivery delays of 300 ms and 1 s, verifying that captured intermediate sequences are
    returned and processed;
16. producer freeze, capacity overflow, retention expiry, epoch change and retry of an identical
    `after_sequence`, verifying that no position is fabricated or silently lost.

For each event, correlate its raw payload and arrival order across DCS, DCS-gRPC, LSO and ACMI.
Do not normalize the source log before preserving it.

## Promotion criteria

Live evidence is sufficient only when repeated runs agree and the captured versions/hashes are
identical. Promote the result into an anonymized deterministic fixture, record what it proves, and
record what it does not prove.

The following remain unvalidated until that promotion occurs:

- Tarawa `RunwayTouch`/`Land`/LQM reliability and order;
- real VL/RVL, bounce and multiple-contact behaviour;
- hook argument polarity per module;
- estimated-wire precision;
- skew policy while the ship turns/accelerates;
- exact 7.5 occupation geometry;
- 40-player/two-ship and three-carrier load;
- DCS FPS/tick impact;
- actual deployed DLL/Lua versions.

Software robustness tests for absence, duplicates, delay and reordering are conservative simulations,
not proof of DCS behaviour.

The Ergo corpus supports only the F/A-18C calibration currently implemented (stable raw 0 for the
four T&G passes and a stable transition to 1 for the arrested pass). Repeat it live and confirm that
the pre-touch timeline, not a final post-touch sample, drives the outcome. F-14 polarity must remain
unknown until an equivalent versioned corpus exists.

During the A/B run, archive the ten-second metric snapshots and calculate observed source captures,
delivered snapshots and processed position samples per second separately. The buffered capture target
is 20 Hz while gate acceptance remains based on the existing 300 ms evidence threshold; neither can
be certified by unit tests or TacView replay.
TacView/ACMI may be kept as optional diagnosis only; absence under `--no-acmi` must not change live
gates, grading availability or health state.
