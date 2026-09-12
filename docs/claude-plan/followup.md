# Follow-up handover (written 2026-09-04)

For the agent that picks this project up in a new conversation. Read this file first. The previous
handover, `followup-2026-09-03.md`, is kept for history: its sections 1 (goal), 4 (how to read live
data) and 8 (pitfalls) still apply and are not repeated in full here. The analysis behind the design
is `as-rust-dcs-scripting-gentle-cat.md`; what was built is `plan-implentation.md`; the last live
session is `live-2026-09-04-foothold.md`. Do not re-derive any of it.

## 1. Goal and non-negotiables (unchanged)

Grade every carrier recovery on a dedicated DCS server with human LSOs possible, so no DCS `WIRE#`
may exist. Three layers: DCS wire when present (authoritative) → hook-transient wire estimate
(fallback) → kinematic arrest confirmation (proves the trap, never names the wire). Hook-up passes
read `T&G (CQ)`, never `Bolter`. 10 Hz telemetry under load. Every grade reproducible offline.

Decided by the user, do not reopen:
- DCS `WIRE#` stays authoritative; the estimate is only a fallback.
- Three valid gates are mandatory for approach points; `WO` from the DCS LSO is the only exemption.
- A kinematically confirmed trap without a wire is a normal graded pass with `wire: unknown`.
- `GetOwnshipHookState` is diagnostics only, never a grading input.
- Work on a feature branch created from the base branch; never commit or push unless asked.

## 2. Where things stand

| Repo | Branch | State |
|---|---|---|
| `DCS-gRPC-lso` | `feature/despawn-after-touchdown-20260904` (from `webpage-rm`) | 148 tests, clippy `-D warnings` and fmt clean |
| `rust-server` fork | `feature/post-analysis-plan-20260903` (from `hook-mechanization-api`) | 0.9.2 unreleased, untagged |

On the LSO branch:
- Committed (`b9645fd`): a pass whose aircraft or carrier despawns after an accepted deck contact
  is graded from the recorded evidence instead of dropped (`events[]` gets
  `despawn_after_touchdown`); `lso file` prints one summary line per replayed pass; test-only
  `StoredPass` carries `#[expect(dead_code)]`; the 2026-09-04 session note.
- Committed (`1e7e360`): Discord `Wire` field (DCS / estimated wire with `✓` or `⚠ mismatch`, plus
  the arrest proof) with two tests; changelog entry; `docs/claude-plan/codex-sessions/` (six
  documents rescued from the deleted 15 GB analysis folder).
- Uncommitted when this file was written: the previous handover renamed to
  `followup-2026-09-03.md` and this file.

Since 0.4.0 the greenie board is the LSO page of the DCS Web Dashboard, which reads `<out-dir>/lso.db`
directly (WAL mode). The LSO has no web page any more; `--web-port` is refused.

## 3. What the 2026-09-04 Foothold session established

- Under a loaded mission with one human pilot the recorder held 10 Hz: sample gap p95 about
  125 ms, snapshot RTT p95 about 34 ms, `queue_wait_ms` tracks the RTT, `lua_exec_ms` under
  0.2 ms, no missing observation sequences. **Plan 3.4 (push-mode `StreamRecoverySnapshots`) is
  closed: not needed.** Reopen only if a future session shows `queue_wait_ms` p95 above about
  100 ms with `lua_exec_ms` still small.
- All five reported passes matched the pilot's ground truth and their offline replay.
- The one missing pass was the despawn bug above, now fixed but **not yet confirmed live**.

## 4. Next steps, in order

1. **Commit and merge the branch** (user action), then one loaded-mission session that includes:
   a trap followed by leaving the slot within 10 s (expect a JSON, a DB row and a Discord post with
   `despawn_after_touchdown` in `events[]`), one trap with the DCS LSO active (expect
   `Estimated: n ✓` in the embed) and, if a human LSO can run it, one trap without a DCS comment
   (expect `Arrest: hook transient` or `deck kinematics`). Read the data exactly as
   `followup-2026-09-03.md` section 4 describes.
2. **Plan 1.8, carrier position filter.** `CARRIER_POS_SMOOTH_ALPHA = 0.15` in `src/track.rs`
   smooths the stepped carrier position (DCS steps it about every 1.4 s) and lags it by about
   0.6 s, which moves the gates and puts a sawtooth on the trap-sheet curves. Every datum now
   carries `raw_carrier_position`, `raw_carrier_velocity` and `filtered_carrier_position`, so:
   - write an offline script (scratchpad or `docs/claude-plan/`, never `src/`) over
     `trap_records/*/*.json` that rebuilds the carrier track with step detection plus velocity
     dead-reckoning (`pos = last_step_pos + velocity × (t − t_step)`) and compares, per pass, the
     gate x positions and the residual sawtooth against the EMA;
   - adopt only if both gate-distance error and sawtooth improve on every recording, keep the EMA
     behind a constant for A/B, bump `GRADING_VERSION` in `src/tasks/record_recovery.rs` to
     `project-derived-v2` because grades change;
   - re-run the 14 live fixtures (`cargo test live_2026_09`) and `lso file` on the 2026-09-04
     recordings before and after.
3. **Fork tag `v0.9.2`** (user pushes and tags), then switch `RUST_SERVER_REF` in
   `.github/workflows/ci.yml` from `hook-mechanization-api` to the tag and optionally pin
   `Cargo.toml`. Deferred by the user; do not do it unprompted.
4. **V/STOL**: still zero live fixtures for the AV-8B on Tarawa. Deferred until the user schedules a
   Tarawa session; do not touch `VSTOL.md` rules before that.
5. **`graphify-out/` rebuild** after the merge (stale since 2026-08-28).

Low impact, still deferred: AoA `NaN` when stationary, streaming ACMI to disk.

## 5. Acceptance gates (unchanged, from `docs/BENCHMARK_PROTOCOL.md`)

- hook-up CQ passes → `T&G (CQ)` 100 %; hook-down traps → `hook_state: down` 100 %;
- DCS-labelled traps: `wire_estimated == wire_dcs` and `arrest_kinematics.confirmed == true`;
- WIRE#-less real traps → `Arrested (wire unknown)` or an estimated wire, never `NC`;
- no groove gap > 300 ms, snapshot RTT p95 ≤ 150 ms / p99 ≤ 200 ms;
- no bolter, T&G or waveoff ever confirmed kinematically (a false positive means stop and fix first).

Threshold tuning rules are in `followup-2026-09-03.md` section 5 and still apply.

## 6. Verification commands

```powershell
# LSO
cargo test                                      # 148 as of 2026-09-04
cargo clippy --all-targets -- -D warnings
cargo fmt --all -- --check
cargo test live_fixture_summary -- --nocapture  # windowed deck speed series for tuning
.\target\debug\lso.exe file <recording.zip.acmi> # offline regrade; prints one line per pass

# rust-server
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --check
```

## 7. Pitfalls (additions to the 2026-09-03 list)

- `lso file` writes its PNGs into the current working directory. Run it from the scratchpad, not
  from the repo root; the PNGs are git-ignored, so `git status` will not warn you.
- A full Tacview client recording replays as one track per aircraft lifetime, not per pass, and
  carries no hook argument. It is evidence for a post-mortem, never a fixture.
- `PlayerLeaveUnit` arrives about 7 s after a trap when the pilot changes slot. Anything that must
  happen after touchdown has to survive the recording ending early.
- `rustfmt` rewrites a `\`-continued string literal into one line and keeps the indentation as
  spaces inside the string. Keep `reason = "..."` attributes on one line.
- Large heredocs fail in this Git Bash environment; write long files with the Write tool.
