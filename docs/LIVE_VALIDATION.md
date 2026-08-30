# Live corpus collection and version manifest

## Current manifest

| Component | Known value | Status |
|---|---|---|
| analysed repository revision | `95f1d27ff273c93b547c963514d26c8d77b31d7f` | known |
| analysed branch | `feature/developpement-post-analyse-3082026` | known |
| Rust DCS-gRPC stubs | tag `v0.9.0`, commit `5bd6d6e42491c8697a5c5a95e80a2e689923bd3b` | pinned in Cargo.lock |
| protobuf | DCS-gRPC v0.9.0 | unchanged |
| deployed DCS build | not authenticated | required live evidence |
| deployed `dcs_grpc.dll` SHA-256 | not authenticated | required live evidence |
| deployed Lua tree SHA-256 | not authenticated | required live evidence |
| mission `.miz` SHA-256 | per-capture | required |
| aircraft module versions | not authenticated | required live evidence |

Do not change protobuf or the client pin until the server DLL/Lua manifest is captured and the
production pin decision is recorded.

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
