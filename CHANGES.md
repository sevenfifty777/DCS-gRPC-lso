# LSO Changelog

This file records user-visible changes. The crate version is `0.5.0` on the integration lineage
(the first number after both the `astra-review` 0.4.0 line and the refonte 0.2.0 line); changes
since the `0.2.0` tag are listed under Unreleased.

## Unreleased

### Fixed

- JSON files saved with a UTF-8 byte-order mark (Windows PowerShell 5.1's default) now load: the
  baseline manifest downloaded from the live server started with one and would have been
  rejected as invalid JSON at line 1, column 1 (`src/commands/run.rs`).
- A duplicate `Land`/`RunwayTouch` (DCS sends two for one V/STOL landing) is rejected before any
  mutation, so the spot distance, nearest spot and terminal datum measured at the first accepted
  contact are never overwritten by the second event a few metres further on. The V/STOL
  admissibility check now measures to the nearest active spot instead of the approach axis, so a
  legitimate landing on another spot of the same ship is not rejected as foreign geometry
  (review finding F03; `src/track.rs`).
- Non-finite positions, altitudes, orientations or velocities are rejected on every acquisition
  path with the new `non_finite_value` invalid reason (unary aligner, replay, buffered pair), and
  the tracker has one invalid-sample boundary: an invalid sample still feeds the telemetry-quality
  accounting but no longer moves the carrier smoothing, the pattern trace, the distance floor,
  hook evidence, deck contact or the outcome decision (review finding F05; `src/telemetry.rs`,
  `src/transform.rs`, `src/track.rs`).
- A datum whose AoA is unknown (`NaN`) is painted neutral grey on the charts instead of falling
  through every module's threshold chain to the Slow colour (review finding F17; `src/draw.rs`).
- Ctrl-C now reaches finalisation: the recorder's merged tick/event stream ends on shutdown
  (the event half never ended on its own, parking the loop), the recording is finalised with the
  evidence in hand and a `shutdown` event, and `lso run` waits up to 30 s for the recoveries still
  in flight before returning instead of dropping the runtime under them (review finding F01,
  Ctrl-C half; `src/tasks/record_recovery.rs`, `src/commands/run.rs`).
- Buffered `ReadRecoveryTelemetry` calls are paced by a process-wide token bucket shared by every
  concurrent recovery, `--buffered-read-budget-per-second` (default 16, i.e. 80 % of the server's
  `recoveryTelemetry.readsPerSecond` default of 20). Three or more simultaneous recoveries no
  longer exceed the server quota and storm `RESOURCE_EXHAUSTED` retries until the watchdog;
  a remaining quota refusal is logged with the two knobs to adjust. The JSON `recovery_telemetry`
  block adds `read_budget_per_second`, `read_budget_waits` and `read_budget_wait_total_ms`
  (review finding F08; `src/tasks/position_collector.rs`).
- The 10 s post-touchdown cutoff is evaluated once per tick, outside the per-sample loop, so a
  unit that vanished right after touchdown in buffered mode (empty or invalid batches only) is
  finalised after 10 s instead of waiting for the 29 s watchdog and being downgraded to
  `telemetry_gap` (review finding F02, buffered half; `src/tasks/record_recovery.rs`).
- A buffered `id_mismatch` observation (the aircraft or carrier name now resolves to another unit
  incarnation) ends the attempt with a typed `unit_identity_mismatch` event and finalises the
  evidence recorded so far, instead of polling the new incarnation until the watchdog (review
  finding F14; `src/tasks/record_recovery.rs`, `src/tasks/position_collector.rs`).

### Removed

- The embedded loopback web greenie board (`src/web.rs`, `--web-port`, `/api/passes`) and the
  direct `axum` dependency. The greenie board is the LSO page of the DCS Web Dashboard, which reads
  `<out-dir>/lso.db` directly. `--web-port` and `--web-expose-ucid` remain parseable for one
  release and stop LSO with an explanatory error (`src/commands/run.rs`).

### Changed

- `dcs-grpc-stubs` is pinned to the fork release `v0.10.0` (commit `a0dea7f`), the release the
  deployed server must run. `build.rs` now reads the resolved stubs version from `Cargo.lock` and
  exposes it as `crate::client::DCS_GRPC_STUBS_VERSION`; the server compatibility check and the
  `dcs_grpc_client_stubs` report field use it instead of a typed `"0.10.0"` literal, which had
  misreported every report while the lockfile resolved `0.9.2` (review finding F15;
  `build.rs`, `build_support.rs`, `src/client/mod.rs`, `src/commands/run.rs`,
  `src/tasks/record_recovery.rs`).
- `releases/lso.exe`, `trap sample/` (recorded passes with real callsigns and a SQLite file) and
  `graphify-out/` are no longer tracked and are ignored; launch scripts (`run-live*.ps1`) stay
  untracked so the Discord webhook never enters source control. `docs/ADMIN_GUIDE.md` documents
  the server release, `recoveryTelemetry.enabled`, the read quota, run, stop and rollback.
- JSON reports are `schema_version: 9`, the first number of the merged lineage after the
  `astra-review` schema 8 and the refonte schema 3. The field layout is the refonte's; the number
  only guarantees that a consumer can tell the three lineages apart (`src/tasks/record_recovery.rs`).
- `lso.db` is opened in SQLite WAL mode with a 2 s busy timeout so an external reader (the DCS Web
  Dashboard) can query the board while a pass is being inserted. Database mutex poisoning is
  recovered instead of propagated (`src/db.rs`, `src/utils/mod.rs`).

- CASE I CATOBAR grading is versioned `project-derived-v7`: correction quality is measured from
  the deterministic episode peak, uses START/MIDDLE/IN_CLOSE/RAMP deadlines of 3.0/2.5/1.5/0.75 s,
  requires two stabilizing samples, and weights corrected severity by the peak zone. AoA trend and
  reversal analysis uses signed distance to the existing aircraft OnSpeed band. Episode JSON adds
  peak, delay, stabilization, aggravation, inversion and qualification evidence (`src/grading.rs`,
  `src/tasks/record_recovery.rs`). All coefficients and correction rules remain PROJECT-DERIVED
  pending comparison with human-LSO assessments.

### Added

- Arrest confirmation without a DCS `WIRE#` (human-LSO policy, ported from `astra-review`): a
  completed hook-deflection transient correlated with a finite pendant crossing, or the
  displacement-based deck kinematics (aircraft stopped relative to the raw carrier position
  within 8 s of contact and held 2 s), now confirm an arrested-carrier contact as `Recovered`
  at medium confidence instead of leaving it `unconfirmed_arrest`. No wire number is invented:
  the hook transient names the crossed wire, the kinematic path leaves `cable_estimated` to the
  independent estimate or `None`. The velocity-based signature (`arrest_confirmation.kinematic`)
  stays diagnostic. JSON adds `hook_state`, `arrest_evidence` (`dcs_wire`, `hook_transient`,
  `kinematic`, `unconfirmed`, `none`), `arrest_confirmation.deck_kinematics` and
  `wire_estimation.hook_deflection_time_dcs`/`hook_recovered_time_dcs`/`correlation_lag_ms`;
  SQLite migration 7 adds `arrest_evidence` and `hook_state`; `cause` gains
  `hook_transient_arrest_without_dcs_wire` and `kinematic_arrest_without_wire` (`src/track.rs`,
  `src/tasks/record_recovery.rs`, `src/db.rs`).
- Wire-plane crossings require the hook to be between the two pendant end points and within 3 m
  of the wire (`finite_hook_plane_crossing`), and the hook-transient estimate takes precedence
  over the deceleration-onset selection when a complete transient exists. The commanded hook
  state is latched from the in-groove baseline ending 1.5 s before the earliest contact evidence,
  falling back to the pre-contact freeze; a hook-up deck contact that nothing confirms as an arrest
  finalises as a touch-and-go (`src/track.rs`).
- An arrest with no `Land`/`RunwayTouch` at all is established from deck kinematics once the
  aircraft crossed the touchdown point, and the track ends after a bounded 10 s evidence window
  from the moment it went slow (review finding F04).
- Fourteen live regression fixtures (T-45 and F-14B(U), 2 and 3 September 2026) with hook
  draw-argument sidecars in `tests/recordings/live_2026-09/`, each replayed as recorded, without
  the DCS LSO message, and without any hook data. Offline replay (`lso file`, `extract_recoveries_with_hook`)
  reads an `LSOHook` ACMI property when present, accepts sidecar samples with their own
  timestamps, keeps feeding 10 s after touchdown, and restarts a fresh track after an attempt that
  ended without touchdown instead of stopping at the first one (`src/commands/file.rs`,
  `src/tests.rs`).
- Additive schema-v3 `grading_episodes` audit trail records CASE I CATOBAR axis, timing, duration,
  most severe zone and weight, raw/corrected/effective severity, peak, evolution, return to a lower
  band, oscillations, correction quality and whether the episode affected the grade. Unreliable
  AoA remains visible with zero grade effect; V/STOL emits an empty list (`src/grading.rs`,
  `src/track.rs`, `src/tasks/record_recovery.rs`).
- Session-level `StreamEvents` hub with in-place reconnect, a 512-event DCS-time journal, exact
  aircraft/carrier correlation and a two-second finalization grace period for late LQM/contact
  events. JSON event correlation reports interruption and reconnection counts
  (`src/tasks/event_hub.rs`, `src/commands/run.rs`, `src/tasks/record_recovery.rs`).
- `ApproachOnly` distinguishes a recognisable final with no proven outcome from both a waveoff and
  a false start. A complete positional assessment keeps its project grade and points even when the
  event outcome is unavailable; attempts with no groove, significant gate or outcome remain
  unpublished (`src/track.rs`, `src/grading.rs`, `src/tasks/record_recovery.rs`).

- Additive `pattern_rendering` JSON diagnostic records the number of continuous pattern branches,
  the zero-based primary branch selected for normal display, why it was selected and how many
  older branches were attenuated (`src/draw.rs`, `src/tasks/record_recovery.rs`).
- Additive graduated-assessment contract across JSON, SQLite, Discord and the board:
  `assessment_scope`, `observed_from_distance_m`, `missing_coverage`, `points_eligible` and
  `fallback_source`. A measured partial approach remains visible without points, independently of
  a certain bolter/T&G/waveoff/trap outcome or DCS wire (`src/track.rs`,
  `src/tasks/record_recovery.rs`, `src/db.rs`, `src/web.rs`).
- `trajectory_deviations[].track_angle_deg` preserves the fitted post-roll-out ground route as an
  additive diagnostic without changing grading (`src/track.rs`).

### Changed

- CASE I CATOBAR grading is versioned `project-derived-v6` and classifies persistent glideslope,
  lineup and aircraft-specific AoA episodes by START/MIDDLE/IN_CLOSE/RAMP zone and correction
  quality. Good/average/poor correction adjusts raw severity by -1/0/+1 before the zone weight;
  only the worst effective episode decides OK/(OK)/--, so axes are never summed. Existing Cut,
  `_OK_`, T&G cap, outcome, completeness, wire and V/STOL paths remain separate. All new zones,
  weights, thresholds and correction rules are PROJECT-DERIVED and await human-LSO validation
  (`src/grading.rs`, `src/track.rs`, `src/tasks/record_recovery.rs`).
- Grading is now versioned `project-derived-v5`. Continuous lineup inside the final 150 m uses a
  dedicated 150 m angular reference instead of the vertical 75 m flare reference, so the 1.5°
  late-lineup threshold consistently represents about 3.93 m throughout that window. The raw
  signed lateral displacement is serialized additively as
  `trajectory_deviations[].lineup_deviation_m`; the 75 m glideslope guard is unchanged
  (`src/track.rs`, `src/tasks/record_recovery.rs`).
- Case I CATOBAR groove entry now follows an explicit port-pattern/final-turn/roll-out state
  machine: the last turn arms only inbound below 600 ft, and `|bank| <= 10°` must then persist for
  0.75 source seconds. The 3/4 NM distance, lineup, ground route and lineup trend no longer delay a
  physical roll-out; the `-0.75°` port corridor remains an optional interception diagnostic.
  Capture gaps above 300 ms reset confirmation, while source-time reversal and a new outbound
  branch reset branch state. V/STOL and implicit Case II/III activation remain unchanged/disabled
  (`src/track.rs`, `lso.toml`).
- Read-only replay of the 44 human JSON reports from 7-8 September 2026 finds 44 entries versus 41
  with the preceding stable-axis detector. Of the 41 comparable entries, 23 move earlier and 18
  remain identical; 38 comparable groove durations grow by 0-31.50 s (3.99 s mean). One geometric
  grade changes from `(OK)` to `--` because newly included corrections are no longer hidden. This
  is offline corpus evidence only, not live DCS validation (`src/commands/groove_ab.rs`).

### Fixed

- `GetWind` queries (groove-entry low probe and the end-of-attempt report query) now retry once,
  bounded, when the raw response looks like DCS's `180deg/0.0 m/s` zero-wind sentinel, confirmed by
  inspecting `../DCS-gRPC` to come from the DCS engine itself rather than this crate or the fork. If
  the groove-entry low probe still looks suspect after its retry, the AoA correction reuses the
  high probe's wind vector for both interpolation points instead; the end-of-attempt query falls
  back to that same high probe if it too is still suspect after its own retry. Raw probe readings
  are still serialized as observed, never overwritten; the two new additive JSON fields
  `wind_reference_probes.low_reading_overridden_by_high` and
  `wind_reading_is_groove_entry_fallback` record when a fallback fired (`src/track.rs`,
  `src/tasks/record_recovery.rs`).
- Missing point-in-time gate objects can be replaced as coverage evidence by adjacent valid,
  inbound continuous-trajectory samples that chronologically bracket the required distance within
  300 ms. Serialized gate quality records this provenance, PNG/Discord identify it, V/STOL keeps
  its three-distance rule, and incomplete wording no longer incorrectly requires “three” gates
  when CATOBAR 3/4 NM is legitimately excluded (`src/track.rs`, `src/grading.rs`, `src/draw.rs`,
  `src/tasks/record_recovery.rs`).

- Live CATOBAR validation on 8 September 2026 (28 clean-build F-14B(U) reports, two human pilots)
  confirms the pattern renderer separates and attenuates earlier circuits without spurious joins:
  8 reports contain two branches, 18 contain three and 2 contain five. The same corpus confirms
  clean attempt separation for `GRADE:WO -> three T&G -> WIRE# 2`, with one accepted LQM in the
  waveoff and trap tracks and no cross-track contamination (`src/draw.rs`, `src/track.rs`,
  `src/tasks/record_recovery.rs`).
- Live buffered-source validation on that corpus confirms late delivery alone no longer suppresses
  grading: all 28 reports retain continuous 20 Hz capture (50 ms maximum gap), no invalid source
  snapshots and no reader sequence loss; 23 are `available/full` despite delivery p95 650-880 ms
  and a 1,030 ms maximum. The five partial reports are explained solely by unconfirmed arrest, not
  telemetry delivery (`src/telemetry.rs`, `src/track.rs`).
- Live concurrency validation on that corpus confirms two human F-14B(U) recoveries can be
  collected and published concurrently without output collision or cross-track contamination.
  Multiple overlapping pairs completed, including simultaneous T&G reports and an overlapping
  bolter/T&G pair (`src/commands/run.rs`, `src/tasks/report_pipeline.rs`).

- `InvalidTelemetry` is now proportional to proven loss of scored-segment coverage. One isolated
  invalid buffered-source sequence is diagnostic when immediately bounded by real valid
  sequence/tick/time anchors within 300 ms and outside every valid gate bracket; consecutive,
  longer, gate-touching or unbounded errors remain blocking. Missing source time may be bounded
  without inventing a timestamp, and receipt time is never substituted. JSON retains every
  observation with its attribution basis, real bounds, coverage gap and explicit verdict effect
  (`src/telemetry.rs`, `src/tasks/position_collector.rs`, `src/track.rs`).
- Buffered delivery latency no longer invalidates otherwise continuous source capture; it remains
  visible in health metrics and warnings. Buffered RPC/empty-batch recovery now uses the source
  ring's advertised retention (one-second safety margin, 30-second cap), while unary keeps its
  two-second watchdog (`src/telemetry.rs`, `src/tasks/position_collector.rs`,
  `src/tasks/record_recovery.rs`).
- Exhaustion of the old pattern-chart history now compacts only that non-scoring history and emits
  `pattern_history_truncated`; it no longer creates a scoring `BufferLimit` (`src/track.rs`).

- `groove_entry` in schema-v3 JSON records the exact DCS confirmation and roll-out-start times,
  receipt-clock evidence, distance, relative altitude, lineup, bank, fitted track angle, lineup
  trend, inbound progress, approach side, optional port-corridor crossing, arm reason,
  confirmation duration/sample count, trigger and all effective thresholds that latched CATOBAR
  groove entry. Existing fields remain present; `stability_duration_s` and
  `stability_sample_count` now mean roll-out confirmation, not stabilized lineup/route/trend.
  Because the current RPC
  has no exact DCS-to-UTC anchor, the report says so explicitly instead of presenting receipt time
  as capture UTC (`src/track.rs`, `src/tasks/record_recovery.rs`).
- `lso.exe groove-ab <json-or-directory>` replays the production gate/trajectory/Case-I-roll-out
  geometry from persisted schema-v3 `datums` and prints a read-only TSV comparison of old/new
  entry distance/time, duration delta, geometric grade, confirmation evidence and newly retained
  lineup/bank/route amplitudes. It cannot reconstruct events,
  RPC timing, UTC mapping or velocities that the JSON did not persist (`src/commands/groove_ab.rs`).
- Per-observation invalid-source evidence now retains source sequence/tick/time, aircraft versus
  carrier entity, exact status code/name, source read time, client receipt time, scored-segment
  attribution and verdict effect. Missing source time is explicit and never replaced by receipt
  time (`src/telemetry.rs`, `src/tasks/position_collector.rs`, `src/track.rs`).
- `arrest_confirmation` adds a structured Phase-A kinematic arrest signature: correlated contact,
  deceleration onset, aircraft/carrier relative-speed stop and hold, capture continuity,
  on-deck/bounce/departure checks, thresholds, provenance and rejection reason. Source snapshots
  older than the contact are excluded even when their batch is delivered later. It is diagnostic
  only: without a DCS `WIRE#` it is capped at `medium` and never lifts `unconfirmed_arrest`, invents
  a wire or awards points (`src/track.rs`).
- `arrest_deceleration_onset_time` (diagnostic-only, `wire_estimation`): `wire_estimate_at`
  (`src/track.rs`) now prefers the earliest wire-plane crossing at or after a detected sustained
  horizontal-speed deceleration (`WIRE_ARREST_DECELERATION_MPS2 = 5.0 m/s^2` over
  `WIRE_ARREST_DECELERATION_MIN_CONSECUTIVE_SAMPLES = 2` samples, `PROJECT-DERIVED`) over the
  previous plain last-crossing-before-event selection — this is the deceleration proxy the P1
  wire-bias fix left open on 6 September 2026 (see `tasking-roadmap.md`): cable stretch can carry
  the hook geometrically past the wire actually caught into the next wire's threshold while the
  aircraft is already decelerating on deck, and the old logic would report that later, higher
  crossing. Falls back to the previous behaviour whenever no onset was observed (every bolter/
  touch-and-go/waveoff, and any arrest whose deceleration signature was lost to a gap). Relies on
  `plane.velocity`, populated only by the live DCS-gRPC path; `lso.exe file` (ACMI/Tacview replay)
  never sets it, so every offline-replayed fixture still resolves through the fallback path
  unchanged.
- Wire-estimate event correlation (`wire_estimate_at`, `src/track.rs`) now anchors on
  `arrest_deceleration_onset_time` when one was observed, instead of always comparing the DCS
  touchdown event to the last raw wire-plane crossing. Confirmed live 6 September 2026 (evening,
  human F-14B(U) session, 6 recoveries) that the last raw crossing sits 505-1953 ms before the DCS
  event on every single pass — always outside `SAMPLE_GAP_WARNING_MS` (300 ms) — blocking every
  wire estimate that evening, including both DCS-confirmed `WIRE# 1` arrests; the onset itself sat
  only 180-280 ms before that same event on those two arrests. Falls back to the previous
  last-crossing anchor whenever no onset was observed, unchanged. The same live data showed the
  aircraft coasting at near-constant speed for 0.895-0.990 s after the hook geometrically crossed
  wire 1 — well after it had already swept past all four wire thresholds (in ~0.62-0.66 s) — before
  a measurable deceleration began, so `WIRE_ARREST_ONSET_TOLERANCE_S` is widened from 0.5 to 1.2 s
  to still select that earliest (actually caught) crossing rather than a later one the hook merely
  slid past. Both changes are `PROJECT-DERIVED`, calibrated on only 2 live samples (same pilot/
  aircraft type), and still need revalidation on a broader live corpus and on a fresh live
  recording (this fix was derived from an already-captured log, not exercised by a new live test).
- `groove_time_secs`: the recovery report now serializes the groove duration (previously computed
  but visible only in the Discord embed), so `_OK_` eligibility can be checked without Discord
  configured.
- `wind_reference_probes`: diagnostic-only field surfacing the two raw `GetWind` responses (each
  altitude/heading/speed) behind `wind_reference_established`, plus a DEBUG-level log of each probe
  individually (and of the separate report-time `GetWind` query, previously only logged on failure).
  Added to investigate a confirmed live anomaly (two reports out of eight reading `180deg/0.0 m/s`
  against a consistent `95deg/0.99-1.42 m/s` on the other six, same ship/mission/timeframe) — no
  change to the AoA correction logic itself; see `tasking-roadmap.md` for what this is meant to
  determine on the next live test.
- Automatic `_OK_` grade (`is_amplitude_perfect`/`grade_from_gates`, `project-derived-v4`): a pass
  already `Ok` by every existing rule is upgraded to `_OK_` (5.0 points) when every gate and every
  continuous sample stays inside a tighter GS/lineup band (`PROJECT-DERIVED`, borrowed from the
  MOOSE `Ops.Airboss` mod) and `groove_time_secs` falls within the NATOPS-documented 15-18 s groove
  window (NAVAIR 00-80T-105 §6.2.4.3). Independent of the wire number caught; never available to a
  touch-and-go.
- Hook-position calibration (touch-and-go vs bolter) extended from the F/A-18C to the VNAO T-45
  (draw argument index 25, shared with the F/A-18C) and the F-14A/B/B(U) (index 1305), via new
  `AirplaneInfo::hook_draw_argument`. The `<=0.2`=up/`>=0.8`=down polarity is empirically
  confirmed for the F/A-18C and was subsequently confirmed live in both directions for the T-45
  and F-14B(U); F-14A/F-14B still reuse it as an assumption pending type-specific live validation.
- A dedicated sink-rate/bank-angle Cut (`dangerous_sink_rate_or_bank`, `src/grading.rs`): a
  sustained (>=3 consecutive samples) sink rate >=8.0 m/s or bank >=30 degrees inside the
  quarter-NM grades the pass `C`. `PROJECT-DERIVED`; NATOPS documents sink rate and bank as
  waveoff-judgment factors but codifies no numeric threshold for either.
- CATOBAR groove-entry stable-axis detector (`groove_stability_measurement`, `src/track.rs`): in
  addition to the distance/altitude box, entry requires `|lineup| <= 2°`, `|bank| <= 10°`, fitted
  ground-track error `<= 10°`, fitted one-second lineup trend `<= 0.5°/s`, all continuously true
  for 0.75 source seconds with any capture gap above 300 ms resetting persistence. Calibrated over
  all 12 F-14B(U) reports from 7 September 2026; it delays the two confirmed premature entries but
  does not force a 15–18-second duration. CATOBAR only; V/STOL keeps the box alone.
- A persistence guard on continuous-trajectory amplitude (`PERSISTENCE_MIN_CONSECUTIVE_SAMPLES =
  2`): an isolated frame above the slight/significant threshold no longer counts alone. Never
  applied to the Cut threshold or the late-approach weighting, which stay sensitive to a single
  sample. An overcorrection/oscillation check (`OSCILLATION_MIN_REVERSALS`/
  `OSCILLATION_MIN_SWING_DEG`) also caps a pass at `(OK)` when GS/lineup shows repeated direction
  reversals over the same 4-second window used by the correction-trend check, even when the net
  deviation is near zero.
- `sink_rate_mps`/`bank_deg` in `trajectory_deviations`, and `roll_deg` in `datums`: contextual
  telemetry, scored only through the dedicated Cut above.
- `trajectory_deviations`: a continuous GS/lineup series computed from groove entry to touchdown
  (additive JSON field alongside `gate_deviations`), and used as a second, continuous source of
  amplitude for `PassGrade` next to the three point-in-time gates (`PROJECT-DERIVED`; see
  `AGENTS.md`).
- A correction-trend check on that same trajectory: a pass whose GS/lineup deviation is still
  measurably worsening in the final 4 seconds before touchdown is capped at `(OK)` instead of
  `Ok`, matching NATOPS' own OK ("reasonable deviations with good corrections") vs (OK) ("fair —
  reasonable deviations") distinction. Never used to raise a grade amplitude placed lower, and
  never checked once a pass is already below `Ok` (`PROJECT-DERIVED`; see
  `AGENTS.md`, "Gates, outcomes et câble").
- A late-approach severity check: a moderate GS/lineup deviation (above `LATE_WINDOW_GS_DEG`/
  `LATE_WINDOW_LU_DEG`, between the general slight/significant thresholds) found inside the last
  150 m before the ramp caps an otherwise-`Ok`/`(OK)` pass at `--` instead, since there is no
  distance left to correct it there. The identical deviation earlier in the approach is graded
  normally. Never raises a grade, never affects an already-`NoGrade`/`Cut` result, never touches
  Cut itself (`PROJECT-DERIVED`; see `AGENTS.md`, "Gates, outcomes et câble").
- `wind_heading_deg`/`wind_speed_mps`: contextual wind at the carrier's position, in the JSON
  report for every recovery (previously queried only for the Discord embed, and not persisted).
  Never affects `pass_grade`/`grade_points`; absent in `--positions-only`.
- `aoa` in `datums`/`pattern_datums` is now wind-corrected once a wind reference is established
  (two `AtmosphereService.GetWind` calls at groove entry, interpolated by altitude for the rest of
  the recovery — DCS wind is deterministic and altitude-dependent, not time-varying) instead of
  the raw nose-vs-ground-velocity angle, which was systematically biased by wind (always present
  during carrier ops) and mixed sideslip/crab into a single always-positive value. New additive
  `wind_reference_established` field records whether the correction applied; falls back to the raw
  approximation, never a fabricated value, when it did not (`PROJECT-DERIVED`; see
  `AGENTS.md`, "Gates, outcomes et câble"). Still chart/report context only, never scored.
- `lso.exe cadence-ab`: an offline diagnostic that replays already-recorded JSON reports'
  `datums` with an artificially reduced pre-groove sampling cadence and compares the resulting
  gates/trajectory/grade to the full cadence actually recorded, over a file or a directory
  searched recursively. Purely a measurement tool for the still-open adaptive-cadence question
  (B.2 of the notation/cadence work); it never changes live recording, the fork, or the input
  files.
- Source-buffered `RecoveryTelemetry` acquisition with idempotent start/read/stop lifecycle,
  exclusive sequence cursors, full-batch processing, epoch/identity validation, explicit
  retention/capacity loss and invalid-unit diagnostics, plus unary rollback through
  `--position-source unary`.
- Optional global DCS-gRPC `X-API-Key` injection from `DCS_GRPC_API_KEY` (or the variable selected by
  `--api-key-env`), with sensitive metadata marking and no token logging.
- Independent `EventCorrelator` and `ReportPipeline` components, including additive event-stream
  status/outcome-confirmation evidence in schema-v3 JSON.
- A priority `PositionCollector`, `--positions-only` baseline mode, optional suspension of background
  detector transforms, `Skip` missed-tick scheduling and per-recovery acquisition percentiles.
- Build Git commit/dirty provenance, explicit DCS-gRPC client/server API-line compatibility, sliding
  telemetry health and additive primary/secondary causes (SQLite migration 6).
- Hook gRPC status codes and recent-evidence ring retention.

- Session/generation-aware supervision, two-second RPC/watchdog deadlines, monotonic freshness and
  explicit skew/gap diagnostics with conservative short extrapolation.
- Strict AV-8B/Tarawa and hook-aircraft/arrested-carrier pairing, slot/UCID human identity and
  session-scoped AI identity.
- Bracketed gate interpolation with `Valid`, `Late`, `Missing` and `Invalid` evidence, plus ordered
  raw `Land`/`RunwayTouch`/LQM evidence and raw hook observations.
- Separate estimated/DCS wire provenance, divergence, confidence, completeness, cause and grading
  version in structured reports and additive SQLite migrations.
- Bounded telemetry/event buffers and runtime RPC, stream, queue, IO and render metrics.
- Simplified NAVAIR-style grading from 3/4, 1/2, and 1/4 nm glideslope and lineup samples, with
  `_OK_`, `OK`, `(OK)`, `--`, `C`, `B`, and `WO` labels and numeric points.
- Neutral unknown-initiator waveoff/go-around evidence, conservative touch-and-go handling, and
  explicit pass outcome storage.
- A second PNG showing the overhead carrier pattern in the BRC frame.
- Pretty-printed JSON recovery reports with gate samples, final-approach datums, and mission time.
- Persistent `<out-dir>/lso.db` storage, automatic migrations for older databases, and pilot UCID,
  aircraft, map, UTC grade time, mission time, points, and outcome fields.
- Optional HTTP greenie board (`--web-port`) and `/api/passes` JSON endpoint.
- Session greenie board printed on shutdown.
- Richer Discord embeds with map, UTC and mission time, grade/points, outcome, gate deviations, DCS
  notation translated to plain English, wind, and groove time. Both approach and pattern PNGs are
  attached; ACMI is attached unless disabled.
- `--no-acmi` to keep charts and JSON without saving Tacview recordings.
- `--no-acmi` now also skips TacView serialization and ACMI-only metadata/unit RPCs instead of only
  suppressing the final file.
- Discord "Why This Grade" field: a short, plain-language explanation of the specific rule that
  produced the pass grade (e.g. "(OK): drifted 0.6° high on glideslope — OK needs better than
  0.5°."), or the existing telemetry-unavailability message when grading itself was unavailable.
  Built by new `grade_from_gates_with_reason`/`compute_pass_grade_with_reason` (`src/grading.rs`),
  which `grade_from_gates`/`compute_pass_grade` now wrap, so the displayed reason can never diverge
  from the actual grade (same single-source-of-truth rationale as the `wire_estimated`/
  `wire_estimation` fix above). Stored on `TrackResult::grade_reason`. Replaces the separate
  "Technical status" field, which is now folded into this one.
- Discord "LSO Notes" fallback (`describe_measured_deviations`, `src/grading.rs`) for a touch-and-go,
  which DCS never sends a `LandingQualityMark` comment for (confirmed live 6 September 2026: 3 of 3
  T&G passes in one session had no `dcs_grading` at all). Explicitly labelled as measured by LSO,
  never phrased to resemble a DCS/NATOPS comment.
- Diagnostic logging for the next live investigation round: `wire_estimate_at` confidence/reason,
  3/4 NM gate eligibility, groove/touchdown timing summary, and the roll-out bank/track-angle check
  are now logged (DEBUG/TRACE); a geometric `Bolter` refused by `GRADE:WO` is now logged (INFO); a
  new `ActivePriorityPlanes::active_count` surfaces genuine concurrent-recovery overlap (INFO) when
  `--suspend-detectors-during-recovery` is set; the two false-start abandon paths in
  `record_recovery` are promoted from DEBUG to INFO with elapsed time and lowest altitude reached.

### Fixed

- A matching DCS `GRADE:WO` LQM now establishes `WaveoffUnknown` on the current track immediately,
  allowing the existing departure guard to finalize that attempt before the next circuit. This
  prevents successive waveoffs and a later arrested landing from being collapsed into one report,
  where the first WO consumed the later authoritative `WIRE#` as a duplicate. Event diagnostics
  now report such an explicit DCS waveoff as outcome-confirmed while retaining unknown initiator;
  regression coverage uses the observed `WO -> WO -> WIRE# 2` sequence
  (`src/track.rs`, `src/tasks/event_correlator.rs`).
- Invalid buffered unit observations were reduced to an aggregate count and attributed using the
  track's state when a late batch arrived; post-touch errors could therefore revoke an otherwise
  usable pass. Attribution now runs at finish from each observation's own source capture time:
  before-groove and post-touch are diagnostic only, in-groove is blocking, and genuinely missing
  source time is explicitly indeterminate with conservative unavailability. Source statuses are
  preserved rather than relabelled `TimeWentBackwards` (`src/tasks/position_collector.rs`,
  `src/track.rs`).
- `wire_estimated` could diverge from the diagnostic `wire_estimation` when the `Land` event
  correlation raced ahead of the positional wire-crossing update that produced the same value;
  `cable_estimated` is now always reconciled once, in `Track::finish()`, against the full
  wire-crossing history.
- `trajectory_deviations`' `atan2`-based GS/lineup angles could explode as the remaining distance
  to the deck approached zero: samples below `TRAJECTORY_MIN_DISTANCE_M = 3 m` are no longer pushed
  at all, and below `NEAR_TOUCHDOWN_ANGLE_REFERENCE_M = 75 m` the angle is computed against that
  fixed reference distance instead of the shrinking true distance — an ordinary few-decimetre
  flare/reference offset near the ramp no longer manufactures a many-degree deviation, while a real,
  larger offset still degrades the grade or trips the Cut as before.
- A go-around that overflew the deck well above deck level was classified `Bolter` instead of
  `WaveoffUnknown`; `crossed_deck_threshold` now also requires the aircraft to be near deck level
  (`DECK_CROSSING_ALT_CAP_FT = 50 ft`) at the moment of crossing.
- Confirmed live 5 September 2026 (evening, human test): the 50 ft guard above still let a
  purely geometric deck crossing with no confirmed contact and no DCS event at all (survol
  measured at 8.7 m) be classified `Bolter`. The geometry-only `Bolter` path (no event ever
  correlated) now additionally requires hook altitude at the crossing to be at or below
  `DECK_CONTACT_CONFIRMATION_ALT_M = 1.0 m` (`deck_crossing_confirmed_contact`); otherwise the
  pass is `WaveoffUnknown`. Separately, `Track::finish()` now refuses a `Bolter` outright when the
  DCS LQM itself opens with `GRADE:WO` (`dcs_grade_is_waveoff`) — not inventing a waveoff author,
  only declining a bolter DCS's own grading directly contradicts, as observed on a pass graded
  `GRADE:WO ... WO(AFU)IC` by the LSO with no `runway_touch`/`land` event.
- F-14 (all variants) hook geometry (`F14_HOOK`, `src/data.rs`) modelled the hook ~0.8-1.1 m below
  actual deck level while the aircraft was physically on deck/rolling through a wire (T-45 reads
  ~0.0 m at touchdown by comparison). Corrected with an empirical `+1.0 m` vertical offset on top
  of the ModelViewer2-extracted position, pending a fresh ModelViewer2 remeasurement and human
  confirmation on F-14A/F-14B specifically (see `tasking-roadmap.md`). This bias was feeding a
  spurious "hook below deck ⇒ contact" reading on the item above, inflating near-deck GS
  deviations, and is the likely common cause of the wire-estimate bias tracked separately.
- Hook-position calibration (`calibrated_hook_state`) could read a raw sample taken while the
  crosse was physically pressed against the deck (reads `0.0`, i.e. "up") as evidence, because the
  cut-off used to select which samples to interpret was the event-correlated `landing_time`, which
  lags true physical contact by ~0.2-1 s. On a confirmed live T-45 trap this window alone would
  have produced an invented `TouchAndGo` on a real arrest without the DCS LQM as a safety net.
  Interpretation is now frozen at the first *geometric* hook contact
  (`first_hook_ground_contact_time`, `alt <= 0.0`), whichever of that or `landing_time` fires
  first.
- `wire_estimate_at` (`Track::finish()`) could report `confidence: "high"` from tight timing
  brackets alone, even though a tight bracket only proves the crossing was measured precisely, not
  that the aircraft actually stopped there — confirmed live 5 September 2026 (evening) on
  survols/bolters with no arrest at all reading a semantically false "high" wire estimate. `"high"`
  now additionally requires a DCS-confirmed arrest (a parsed LQM `WIRE#`); without one, confidence
  is capped at `"medium"`. Wire selection separately uses the sustained-deceleration onset and its
  1.2 s crossing window described under Added; both remain pending fresh live revalidation.
- Event-stream errors and clean closure no longer become positional `telemetry_gap`; existing gates
  remain intact while outcome availability is assessed separately.
- Plane/carrier respawns with a changed ID abort every stale same-name task within the current
  session/generation, preventing old-ID collectors from polling a current name.
- DCS/LQM wire parsing accepts only cables 1-4 and rejects zero, overflow and malformed suffixes.
- JSON, ACMI and rendered files use atomic create-if-absent publication on Windows and Unix; the JSON
  winner alone may continue to SQLite/render/Discord, and temporary files/directories are cleaned.
- Positions-only ignores missing or invalid Discord user configuration and does not start event,
  hook, ACMI, SQLite, dashboard, render, board or Discord components.
- Errors now retain useful IO paths, JSON line/column data and underlying JSON, SQLite, rendering,
  ACMI and Discord causes.
- Git dirty provenance now intentionally covers tracked files only, with tracked-path/index/HEAD
  rebuild triggers and deterministic parser tests; untracked files and `target/` are excluded.
- CI uses locked build/tests, all-target Clippy with warnings denied, rustfmt, and a pinned locked
  `cargo-audit` installation that consumes the existing `.cargo/audit.toml` ignore list unchanged.
- Detector suspension is scoped to the aircraft already being collected, so a second aircraft can
  still start a simultaneous recovery; positions-only no longer opens or migrates SQLite.
- Unconfirmed arrest no longer overwrites telemetry/gate causes; all independent unavailability
  causes are retained and SQLite completeness values now use the JSON snake-case vocabulary.
- Hook gRPC codes use documented snake-case names, baseline manifests are strict, Git dirty-state
  rebuild tracking covers every tracked file, and acquisition percentiles use bounded online
  histograms instead of unbounded vectors and end-of-pass sorting.
- Positions-only skips TacView update construction, and the direct Axum dependency is aligned with
  Tonic's 0.8 dependency line.
- Repaired the malformed Discord block left by the previous merge.
- Hook/event diagnostic truncation no longer changes positional completeness or masks
  `insufficient_gates`; hook history retains the newest 512 observations.
- Wire crossings are segmented at final entry, DCS/LQM wire evidence remains visible when the Rust
  estimate is unavailable, and each invalid gate displays its own bracket gap.
- Additional F-14 type aliases and distinct F-14A, F-14B, and F-14B(U) display names.
- Carrier-position EMA smoothing for final-approach geometry.
- Independent, timestamped hook sampling with configurable 2-4 Hz cadence, 250-300 ms timeout and
  a legacy-inline A/B switch; per-RPC and loop/tick latency percentiles; live telemetry health.
- Schema-v3 report evidence for hook freshness, component versions, grading availability and
  continuous wire-plane crossings; additive SQLite migration version 5.
- Recovery-monitor tasks for respawned units replace stale tasks instead of accumulating duplicate
  recordings after mission changes.
- Recording ends when a plane exits the 3.5 nm / 1,100 ft pattern envelope, preventing indefinite
  ACMI capture after a missed approach or mission change.
- Carrier-position smoothing reduces periodic sawtooth artifacts in final-approach charts and gate
  measurements.
- CATOBAR charts select the latest continuous inbound branch, preventing earlier overhead-pattern
  points from joining the real final as a false vertical drop.
- F-14B(U) identification and trap-sheet naming.
- Gate brackets use only their actual endpoint interval and can recover from an isolated degraded
  sample without crossing a real cut; frozen DCS timestamps now age and trip the watchdog.
- Pattern-only gaps no longer invalidate the scored groove, while gate/groove gaps remain blocking.
- Fragmented CATOBAR grooves render as separate labelled fragments instead of disappearing or being
  connected artificially. A late RunwayTouch transform can no longer manufacture a wire-4 crossing;
  an estimate now requires a continuous crossing correlated within 300 ms of the event.

### Changed

- Multi-circuit pattern PNGs no longer draw every circuit as one equally prominent polyline. The
  renderer splits confirmed approach/departure reversals and telemetry discontinuities into
  independent branches, keeps AoA colours on the branch containing groove entry (touchdown/latest
  fallback), and draws older branches as thin grey context without artificial joins. Track
  lifecycle, telemetry and grading are unchanged (`src/draw.rs`).
- Telemetry health now reports source capture spacing and delivery age as separate maxima, scored
  maxima, warning ratios and p50/p95/p99 distributions while retaining legacy worst-of gap fields.
  Reader-observed sequence loss/continuity is separate from source ring capacity/retention churn;
  no 300 ms/1,000 ms rule was relaxed and delayed capture is not interpolated
  (`src/track.rs`, `src/tasks/position_collector.rs`).
- Hook history is a documented recent-evidence ring of 2,048 entries (~8.5 minutes at 4 Hz), with
  capacity, policy, retained DCS interval, truncation reason and dropped count in JSON. It preserves
  final-window/contact evidence and remains independent from position collection; polarity and
  stability logic are unchanged (`src/track.rs`).
- The CATOBAR ground-track proxy fits least-squares position over all valid inbound samples in its
  two-second buffer rather than relying on two endpoints. The stricter stable-axis contract is
  documented under Added. The geometric AoA remains excluded because it is not a reliable on-speed
  signal (`src/track.rs`).
- CATOBAR `_OK_`/`OK`/`(OK)` no longer unconditionally requires the 3/4 NM gate
  (`GateDeviations::all_valid`/`three_quarter_counts`, `src/track.rs`; `grade_from_gates`,
  `src/grading.rs`): when that gate was captured *before* roll-out-confirmed groove entry
  (`Track::groove_entry_time`) -- present or not, valid or not -- it counts toward neither
  completeness nor GS/lineup amplitude, and only the 1/2 NM and 1/4 NM gates (plus the continuous
  trajectory) are required/scored. On a real Case I pattern the 3/4 NM gate routinely falls in the
  base-to-final turn rather than the groove (confirmed live 5 September 2026, evening: 7 of 8 human
  passes, lineup up to -10.5° at that gate), imposing `--` regardless of the actual groove that
  followed. A 3/4 NM gate captured *after* groove entry is unaffected. V/STOL keeps the historical
  unconditional rule (it has no roll-out-confirmed groove entry to anchor this on); `cadence-ab`
  also keeps it, since it never re-derives groove entry from a replay. `PROJECT-DERIVED`; not
  revalidated on live mission data yet (no NATOPS text makes any fixed gate crossing a
  qualification requirement in the first place -- see `AGENTS.md`, "Gates, outcomes et câble").
- CATOBAR grading now takes the worst GS/lineup amplitude across the continuous trajectory as well
  as the three gates, not the three gates alone; a significant excursion strictly between two gates
  (previously invisible to grading) can now downgrade the pass, and a dip below the Cut threshold
  anywhere at or inside the quarter-NM distance is caught, not only exactly at the gate crossing.
  This can only make the reported amplitude equal or worse than before, never better.
- Pilot-facing surfaces (Discord embed, PNG chart, SQLite/greenie-board log) now always show the
  DCS/LQM wire alone when it is available, instead of ever displaying it next to a diverging Rust
  geometric estimate (`Grading::pilot_facing_outcome`); the full JSON `outcome` field still records
  both wire values side by side for diagnostics.
- Full-pattern JSON `datums` are now subsampled to one in four outside the scoring-relevant window
  (before groove entry and beyond ¾ nm / 500 ft); the scoring zone itself, gate evidence and
  grading are unaffected, only the pattern/break portion of the report shrinks.
- DCS-gRPC client stubs are aligned with the sibling `0.10.0` server checkout while its commit is
  unpublished; release packaging must replace the local path with a reviewed immutable remote pin.
- The original MOOSE-style automatic `_OK_` rule (wire-3 plus a 15-18.99 s groove-time window) was
  disabled; `_OK_` was reserved for an explicit official/manual grade for a time. A different,
  wire-independent automatic `_OK_` rule was later added — see "Added" above.
- PNG rendering and SQLite work run outside latency-sensitive sampling tasks. Atomic artifact names
  include session/generation/unit identity and database inserts are idempotent.
- The HTTP dashboard now binds to `127.0.0.1` and returns HTTP 500 on database failure.
- The detection envelope now captures the full pattern: 200 m to 3.5 nm from the carrier and at or
  below 1,100 ft MSL, without nose-pointing or rear-hemisphere checks.
- Gate sampling is restricted to inbound crossings below 500 ft above the deck, and groove entry
  also requires lineup within 10 degrees.
- The earlier DCS-gRPC migration moved client stubs to the official sevenfifty777 fork release tag `v0.9.0`,
  resolved in `Cargo.lock` to commit `5bd6d6e42491c8697a5c5a95e80a2e689923bd3b`; `tonic` was
  updated to 0.13.
- Unit discovery safely ignores DCS units whose optional type field is absent.
- T-45 AoA brackets now use values derived from the VNAO T-45 display-electronics data instead of
  the former F/A-18C copy.
- F/A-18C CQ touch-and-go recognition now requires stable, timestamped pre-touch hook evidence;
  uncalibrated modules remain unknown. Technical unavailability is separate from pilot performance.
- Discord "Gates (GS / LU)" field now shows degrees instead of feet, matching every other angle
  shown in the embed.

### Security and dependencies

- The lockfile was refreshed for DCS-gRPC 0.9.0 and its gRPC stack.
- Known vulnerable transitive versions identified during the migration were updated; the audit was
  clean at that time. Current `cargo audit` allowances are tracked directly in `.cargo/audit.toml`,
  not duplicated here since they change independently of this migration.

## 0.2.0 - 2024-11-10

- Initial tagged `0.2.0` release. It provided live DCS-gRPC monitoring, carrier-relative recovery
  charts, wire estimation, compressed ACMI recording, Discord webhook delivery, and ACMI replay.

Earlier tags: `0.1.1`, `0.1.0`.
