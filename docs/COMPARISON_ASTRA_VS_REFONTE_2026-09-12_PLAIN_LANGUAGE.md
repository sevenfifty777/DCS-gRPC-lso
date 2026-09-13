# The comparison explained in plain language

This is a companion to `COMPARISON_ASTRA_VS_REFONTE_2026-09-12.md`. That document is written for someone who reads Rust code every day. This one is written for someone who does not. It follows the same section order, so you can read them side by side. Every technical word is explained the first time it appears, and every point ends with a short "what this means for you" or "what to do".

If you only read one section, read section 1 (the verdict) and section 6 (the to-do list).

---

## 0. Words you need before you start

**Branch.** A branch is a separate copy of the project where someone works without disturbing the others. Think of it as a draft of a document that lives next to the original. There are two branches in play here:

- `astra-review`, called **the baseline** in this document. This is the branch that was reviewed on 12 September in the earlier review document.
- `feature/refonte-v3-lua-buffer`, called **the refonte** (French for "rework"). This is your colleague Lenny's branch.

**Merge base.** The last point in history where both branches were identical. Everything after that point is what each person changed. Here that point is 31 August.

**Commit.** A saved snapshot of the code, identified by a short code like `fc67446`. When the document says "at `fc67446`", it means "the version saved under that code".

**DCS.** The flight simulator. **DCS-gRPC** is a plug-in that lets outside programs ask DCS questions ("where is this plane right now?") and receive answers. The LSO program is one of those outside programs.

**gRPC / RPC.** A "remote procedure call" is simply a question sent over the network to another program, which sends back an answer. When the document says "the RPC `GetRecoverySnapshot`", it means "the question named GetRecoverySnapshot that we send to DCS".

**Lua.** The scripting language that runs inside DCS. The DCS side of the plug-in is written in Lua. The LSO program itself is written in Rust.

**Stubs.** A small package of pre-generated code that lets the Rust program speak the exact "language" of the DCS plug-in. The stubs must match the version of the plug-in that is installed on the server, otherwise they may not understand each other.

**Telemetry.** The stream of numbers describing the aircraft: position, speed, altitude, heading, hook position, and so on. Collected many times per second.

**Unary versus buffered.** Two ways of collecting telemetry.

- *Unary*: the LSO program asks "where is the plane?" ten times per second and gets one answer each time. Simple. If a question is lost, that moment is lost forever.
- *Buffered*: DCS records the position twenty times a second into a memory buffer on its side, and the LSO program comes by regularly to pick up everything that accumulated. Nothing is lost if the LSO program is briefly late.

**Recorder / recovery.** A "recovery" is one aircraft attempting to land on the carrier. A "recorder" is the piece of the program that follows one aircraft during one landing attempt and produces a report at the end.

**Finalization.** The moment when the recorder stops following the aircraft, decides what happened (trapped, boltered, waved off), grades the pass, and writes the report file. If finalization never happens, no report is produced.

**`WIRE#`.** A message DCS sends when an aircraft catches an arresting wire, for example `WIRE3`. DCS only sends this message when the built-in AI LSO is active. When a human plays the LSO role, DCS often sends nothing.

**Kinematic.** A fancy word for "based on movement". A "kinematic arrest confirmation" means: the program looked at how the aircraft slowed down and concluded "that was a trap", without any message from DCS saying so.

**Hook transient.** The tail hook moves in a recognisable way when it catches a wire. The program can watch the hook's animation value and estimate which wire was caught. This is an estimate, not a fact.

**Groove.** The final straight part of the approach, roughly the last 15 to 20 seconds before touchdown. The grade is based on what happens in the groove.

**AOA.** Angle of attack. The angle between the wing and the airflow. Pilots fly the approach at a precise AOA. "On speed", "slow" and "fast" are all statements about AOA.

**Case I.** Daytime, good weather carrier operations, where aircraft fly a visual circular pattern around the ship.

**V/STOL, AV-8B, Tarawa.** The Harrier (AV-8B) does not catch wires. It lands vertically on a numbered spot on a smaller ship (Tarawa). The program has separate logic for that.

**CI.** Continuous integration. An automatic robot that builds and tests the code every time someone pushes a change, and complains if something breaks.

**Tests.** Small automatic checks written by developers. "292 tests pass" means 292 of these checks were run and all gave the expected result. More tests generally means more confidence, but only in the things the tests actually cover.

**Clippy and rustfmt.** Two tools that check Rust code for sloppy patterns and inconsistent formatting. "Clean under clippy" means the tool found nothing to complain about.

**`--locked`.** A build option that forces the exact same versions of every library each time. Without it, two people building the same code on different days might get slightly different programs.

**Provenance.** Information stamped into each report saying exactly which version of the program produced it. Without provenance you cannot know which version of the code a given report came from.

**Fixtures.** Recorded real flights saved as files, used to re-run the program on known data and check that the result did not change. They are the program's "practice exams".

**Watchdog.** A timer that gives up after a fixed delay (here 29 seconds) if the program has been waiting for something that never arrives.

**Ctrl-C.** The keyboard command used to stop a program running in a terminal window.

---

## 1. The verdict, in plain words

The two branches are good at different things, and neither one can simply replace the other.

**Lenny's refonte is the better foundation.** It is organised the way the review recommended. It uses the buffered telemetry that the review said was being wasted. It adds authentication, safer file writing, version stamps in reports, a stricter build robot, and twice as many automatic tests. All of those tests pass, and the code-quality tools are happy.

**The baseline gives better results today for the one thing you actually asked for.** You said you need to grade passes when a human is playing the LSO and DCS sends no `WIRE#` message. The refonte, by design, refuses to give a grade for a trap unless DCS sent that message. In the real sessions Lenny recorded, this meant 21 of 66 traps got no grade. The baseline still tries to work it out from the aircraft's movement and the hook, so it gives a grade in those cases, even though one known bug (F04) is still open in that logic.

**Of the eighteen problems the review found**, the refonte fixes about five, changes two into new policy questions, half-fixes four, and leaves five untouched. It also creates a couple of new problems, two of which are serious:

- Stopping the program with Ctrl-C no longer produces the final reports.
- When three aircraft are being tracked at the same time, the program asks DCS for data faster than DCS allows, and things start timing out.

**Recommendation in one sentence:** keep Lenny's code as the base, bring the useful pieces of the baseline into it, fix the short list in section 6, and only after that start arguing about grading thresholds.

---

## 2. What each branch is, row by row

This explains the table in the original document.

**Amount of change.** The baseline changed about 55 files. The refonte changed about 66 files but with far more new lines (roughly 17,000 added versus 4,000 for the baseline). The refonte is a much bigger rewrite.

**Version number and JSON schema.** Each report is a JSON file, a structured text file. The "schema" number says how the file is laid out. The baseline is at version 0.4.0 with schema 8. The refonte is at 0.2.0 with schema 3. These numbers come from two separate histories and are not comparable. The point is: the report files from the two branches do not have the same layout, and any tool reading them (the dashboard, for instance) must know which one it is reading.

**Embedded web board.** The baseline removed the little built-in web page and now expects a separate dashboard to read the database file `lso.db`. The refonte still has the built-in web page. When merging, one of these has to go, and the document recommends going with the baseline's approach.

**Stubs dependency.** Good news: both branches now point at the same released version of the DCS-gRPC plug-in code, `v0.9.2`. They speak the same language.

**Acquisition.** How telemetry is collected. Baseline: unary, 10 times per second. Refonte: buffered, 20 times per second, with unary kept as a fallback you can switch to.

**Hook evidence.** How the program watches the tail hook. Baseline: the hook value comes bundled inside the same answer as the position, so it is perfectly time-aligned. Refonte: a separate process asks for the hook value on its own, and the time stamp it gets is delayed by however long the batch was waiting. This matters in section 4.

**Arrest without `WIRE#`.** Baseline: makes a decision based on movement and hook. Refonte: notes it as a diagnostic but leaves the pass marked `Incomplete`.

**Grading.** Baseline: version 1, "worst gate", meaning the grade is driven by the single worst moment in the groove. Refonte: version 7, splits the groove into zones, weights them, gives credit for corrections, adds an AOA axis, and gives a Cut for excessive sink rate or bank angle. Version 7 is more sophisticated. Neither has been checked against a real human LSO's opinion.

**Groove entry.** How the program decides "the groove has started". Baseline: the aircraft crossed the 3/4 nautical mile point. Refonte: a small "state machine" that tracks the Case I roll-out and uses the 3/4 mile point only under some conditions.

**Tests.** 148 versus 292. Twice as many in the refonte.

**Live regression fixtures.** The baseline has 5 recorded flights plus 14 more real ones from 2 and 3 September that include hook data. The refonte only has the 5. The 14 real ones were lost in the rework. They need to be brought back.

**CI.** The refonte's build robot is stricter: it refuses to build if library versions drift, and it blocks on security warnings. The baseline's robot only warns.

**Provenance.** The baseline writes `lso_commit: unknown` in every report, which is useless. The refonte writes the real commit code and whether the code had uncommitted changes. That is a real improvement.

**Docs.** The baseline's docs are in English and consistent. The refonte's docs are largely in French, very candid and detailed, but the README says things that are no longer true (see F18).

**Committed artefacts.** The refonte has some files in the repository that should not be there: a 15 MB compiled program (`releases/lso.exe`), a folder of real trap samples with real pilot callsigns and a database file, and a generated `graphify-out/` folder. Compiled programs and personal data do not belong in source control. They should be removed from tracking.

**Note about a stale point in the earlier review.** The review had complained that the baseline pointed at a folder on someone's local disk for the stubs. That is no longer true; the baseline now points at the `v0.9.2` release tag.

---

## 3. The eighteen findings, one by one

Each finding is labelled F01 to F18 in the original review. For each one: what the problem was, what the refonte did about it, and what still needs doing.

### F01. The program does not properly shut down the pieces it started

**The problem.** When the program starts tracking an aircraft, it launches a small background worker (the recorder). The review said: when the program stops or a session ends, it should wait for all those workers to finish cleanly. It did not.

**What the refonte did.** Half fixed, half worse. If setup fails, the workers are no longer left dangling, which is good. When a session ends, the workers are told to abort, but the program does not wait for them to confirm. And a new problem appeared: pressing Ctrl-C no longer lets the workers finish. Technically, the worker sits in a loop waiting for either a timer or a new event, and the event source never closes because the worker itself is holding it open. So the loop waits forever and the "write the report" step is never reached.

**In plain terms.** If you stop the program with Ctrl-C while someone is in the groove, that pass is lost. No report.

**What to do.** Give the worker an explicit "stop now" signal that it checks alongside the timer and the events, so Ctrl-C leads to a written report. This is item 4 in the to-do list.

### F02. A plane that disappears right after landing loses its report

**The problem.** In DCS a plane can vanish (the pilot leaves the server, the slot resets). If that happened right after touchdown, the program used to throw away the pass even though it had all the data it needed.

**What the refonte did.** Fixed in unary mode: when DCS says "unit not found", the program now goes ahead and finalizes. Only partly fixed in buffered mode: there, a vanished unit does not produce a "not found" error, it just produces empty or invalid data. The check that says "it has been 10 seconds since touchdown, wrap up" lives inside the loop that processes individual samples, and if there are no samples, that check is never run. So the program waits for the 29-second watchdog instead and may then downgrade the pass to "telemetry gap".

**In plain terms.** In buffered mode, a pilot who leaves right after trapping might still get a spoiled report.

**What to do.** Run the 10-second check even when a batch arrives empty. Item 4 in the to-do list.

### F03. A Harrier landing counted twice corrupts the spot measurement

**The problem.** DCS sometimes sends the "landed" event twice for a Harrier. The program measures the distance to the landing spot before it checks whether this is a duplicate, so the second, wrong event overwrites the distance.

**What the refonte did.** Nothing. Same code, same bug. On top of that, a new rule now rejects any landing more than 200 metres from spot 7.5, which will also throw away legitimate Harrier landings on other spots.

**What to do.** Check for duplicates before measuring anything, and reconsider the 200-metre rule. Item 5 and item 9 in the to-do list.

### F04. A trap detected only from movement never finishes

**The problem.** In the baseline, when the program detected a trap from the deceleration alone (no `WIRE#` message), two internal timers disagreed (one said wait 2 seconds, one said wait 8), and the pass could hang.

**What the refonte did.** Made the contradiction disappear by removing the feature: movement-based trap detection is now only a note in the diagnostics and never decides the outcome. The cost is that a real trap with no DCS message is labelled a Bolter if the plane later taxis, or "approach only" if it sits still.

**In plain terms.** The bug is gone because the feature is gone. But the feature was the thing you needed for human LSO sessions.

**What to do.** Bring the feature back from the baseline, fix the timer contradiction, and write down the policy: a movement-confirmed trap with a hook transient counts as a real trap at "medium confidence", and no wire number is invented. Item 3 in the to-do list.

### F05. Bad numbers are accepted and pollute the calculations

**The problem.** Sometimes DCS sends nonsense: a position of "not a number", or a unit in a bad state. The review asked that such samples be rejected at the door, before they influence anything.

**What the refonte did.** Partly. The buffered path now checks samples at the door. The unary path and the piece that aligns timestamps do not. And inside the tracker, an invalid sample still updates the carrier position average, the pattern reference points, the closest-distance records, and the outcome decision.

**What to do.** Add the same door check to the unary path, and add one single "is this sample valid?" gate inside the tracker so nothing downstream ever sees bad data. Item 5 in the to-do list.

### F06. Events can be missed at startup or between subscribers

**The problem.** The program takes an inventory of units, then starts listening for events. Anything that happens in between is missed. Also, some slow lookups were done in the middle of the event-handling loop, holding everything up.

**What the refonte did.** Partly. There is now an "event hub" that keeps the last 512 events in a journal and replays the relevant ones when a new attempt starts, plus a 2-second grace period. That helps. The inventory still comes before the listening starts, and the slow lookups are still done inline.

**What to do.** Nothing urgent. Worth revisiting once the bigger items are done.

### F07. No authentication, and permanent errors retried forever

**The problem.** The program connected to DCS-gRPC without an API key, and when the server said "you are not allowed", the program kept retrying anyway.

**What the refonte did.** Added the API key to every connection. It reads it from the environment variable `DCS_GRPC_API_KEY`. The retry-forever behaviour on "unauthenticated" is still there.

**What to do.** Make "unauthenticated" a permanent error that stops the program with a clear message instead of retrying. Small job.

### F08. The buffered telemetry existed but nobody used it

**The problem.** The DCS side already had the buffered engine, but the LSO program still asked one question at a time.

**What the refonte did.** Mostly fixed. The program now uses the buffer properly: it only advances its bookmark after a validated batch, it checks the session identifiers, and it tells the server to stop when it is done. One thing is missing: the server allows each client 20 reads per second in total, and each recorder reads 10 times per second, so with three recorders running at once the program is over the limit. The server then refuses some reads, the program treats each refusal as a temporary glitch and retries, and eventually the 29-second watchdog fires.

**In plain terms.** Buffered mode works well for one or two aircraft. With three or more in the pattern at once, it breaks down.

**What to do.** Add one shared rate limiter across all recorders that stays under the server's 20 reads per second. Item 4 in the to-do list.

### F09. Smoothed carrier position leaks into the grading

**The problem.** The program smooths the carrier's position over time to reduce jitter. That smoothed position is then used to compute where the landing area is, and the lineup and glideslope numbers. Smoothing introduces a small lag, so the grading geometry is slightly behind reality.

**What the refonte did.** Nothing on the behaviour. It does now store both the raw and the smoothed position for every sample, which will make it possible to compare them later.

**What to do.** Later. Not on the critical path.

### F10. Replaying a recorded flight does not behave like the live run

**The problem.** When you re-run the program on a recorded file, some things are different from live: time going backwards is silently clamped, the "landed" event is not written to the recording, and the hook value is not in the recording at all. So a replay cannot fully reproduce a live pass.

**What the refonte did.** Nothing. The `file` command also only writes picture files, not the JSON report, so you cannot compare a replay to a live report number for number.

**What to do.** Later, but this matters for building the practice-exam corpus in item 3.

### F11. Writing a report could overwrite an existing one

**The problem.** If two passes ended up with the same file name, the second silently overwrote the first.

**What the refonte did.** Fixed properly. It writes to a temporary file, forces it to disk, then links it into place in a way that fails if the target already exists. There is a test for it. One caveat: the linking technique does not work on some disk formats (exFAT, FAT, some network shares). If the output folder is on such a disk, writing will fail.

**What to do.** Make sure the output folder is on an NTFS disk, or add a fallback for other formats.

### F12. The end of one pass blocks the detection of the next

**The problem.** When a pass finishes, the program queries the wind, writes to the database, draws the picture, and posts to Discord, all while the detector is waiting. If Discord is slow, the next pass may be missed.

**What the refonte did.** Nothing.

**What to do.** Later. Move the slow parts (especially Discord) to a separate queue.

### F13. Builds were not reproducible

**The problem.** Building the program on two different days could give two different results because library versions were not pinned.

**What the refonte did.** Fixed on both branches. The refonte's build robot is stricter. Both still use an outdated helper action in the build robot, which is cosmetic.

**What to do.** Nothing required.

### F14. Aircraft are identified by name only

**The problem.** If a pilot leaves and another pilot takes the same slot, the aircraft has the same name but is a different object. The program could mix them up.

**What the refonte did.** Partly. In buffered mode the program sends the unit's numeric ID and the server reports a mismatch if it changes. But the program does not act on that report by ending the attempt with a clear reason. Unary mode is still name-only.

**What to do.** When the server reports an ID mismatch, end the attempt and say why. Item 4 in the to-do list.

### F15. Reports do not say which version made them

**The problem.** Every report said `lso_commit: unknown`.

**What the refonte did.** Now writes the real commit code and a "dirty" flag. But the version of the stubs is written as the literal text `"0.10.0"` typed into the code, while the actual stubs used are `0.9.2`. So every refonte report lies about its stub version. The grading version is also just a label with no meaning attached.

**What to do.** Read the stubs version from the stubs package itself instead of typing it by hand. Item 6 in the to-do list.

### F16. No limits on memory use

**The problem.** Several lists inside the program grow without bound: the session log, the in-memory recording, the inventory. A long session can eat memory.

**What the refonte did.** Nothing, and added two more unbounded lists (the task registry and the claim set).

**What to do.** Later.

### F17. AOA was computed from the wrong thing

**The problem.** The program computed angle of attack from ground speed direction, which is wrong when there is wind.

**What the refonte did.** The calculation is fixed: it is now signed, uses the pitch plane, and corrects for wind. Two issues remain. First, when the AOA is "not a number", the picture still colours it as **Slow** instead of showing "unknown". Second, AOA now influences the grade whenever a wind reference exists, and the wind reference can be quietly substituted from a high-altitude reading when the deck-level reading was missing. So a wrong wind can change a grade without anyone knowing.

**What to do.** Fix the colour for unknown AOA (item 5). Decide separately whether AOA should affect the grade at all (item 8).

### F18. Documentation says things that are no longer true

**The problem.** Various docs were out of date or contradictory.

**What the refonte did.** Mixed. The security audit policy is fixed. But the README says 10 Hz (it is 20) and says "AOA does not change the grade" (it does). `AGENTS.md` refers to a commit and a file path from another machine. The code refers to a grading reference document that does not exist.

**What to do.** Update the README and `AGENTS.md`, and either write `docs/GRADING_REFERENCE.md` or remove the reference to it.

---

## 4. What the real sessions showed

Lenny recorded 158 real passes between 5 and 12 September, all by human pilots, mostly in the F-14B, using buffered mode at 20 Hz. The original document dug through those files. Four things came out.

### 4a. Data delivery got slow after the first day and stayed slow

On 5 September, the program received each batch of data about 26 milliseconds after asking, at worst. From 6 September onward, that worst-case number jumped to around 700 to 800 milliseconds and never came back down.

The code that collects data did not change between those two days. Two things did change: several human pilots were flying at once, and the hook sampler was switched on for the F-14. The pattern of the delays (mostly fast, occasionally very slow) looks like DCS's internal request queue stalling now and then, rather than a slow network.

**Does it matter?** Not for the grades. The data was still captured continuously at 20 Hz on the DCS side and nothing was lost, so every gate and every grade is unaffected. It does matter for the hook evidence, because hook samples are stamped with the delayed time, so the hook timing is off by up to 700 ms. That is enough to matter when estimating which wire was caught.

For comparison, the baseline's unary mode measured 34 ms worst-case on a busy 4 September session.

**What to do.** Item 7: run a controlled test with the hook sampler off, then with positions only, and compare. The new `v0.9.2` server has diagnostic fields that will show exactly where the time goes.

### 4b. Traps without a DCS wire message get no grade

Out of 66 arrested passes:

| | Count |
|---|---|
| DCS sent a `WIRE#` message | 36 |
| Of those, the program also computed its own estimate | 9 (7 agreed with DCS, 2 did not) |
| No `WIRE#` message, marked "unconfirmed arrest", zero points | 21 |
| Of those 21, the program's own estimate was shown but ignored | 12 |

So the refonte threw away one trap in three because of its policy. That is the human-LSO problem from the verdict.

### 4c. Almost every pass is graded "--"

92 of 158 passes carry the "--" label (no grade, 2.0 points). Only 13 are "(OK)", one is "OK", one is "Cut". Even with the newest grader (version 7), half of the passes are "--".

**What this means.** Either the pilots are flying badly, or the grader is too harsh, or the grader is measuring the wrong thing. There is no way to know, because no human LSO wrote down their own grade for any of these passes. Without that reference, neither branch's grading can be called correct.

### 4d. Smaller observations

- **Harrier passes on 9 September.** Six passes, four marked incomplete because the program could not find enough gates, with groove times of 77 to 122 seconds (a normal groove is 15 to 20 seconds). The Harrier groove-entry logic is broken in that build.
- **`baseline_manifest` is empty in every report.** The code that fills it looks right. The most likely cause is that the script that launches the program does not pass the `--baseline-manifest` option. Check the launch script.
- **The server reports version 0.10.0.** But the branch is pinned to `0.9.2`. This means Lenny was running a server he built himself from his own fork, not the released one. See section 7.
- **Wind was almost nil.** Median mission wind was 1 m/s, so the new wind correction has not actually been tested in real wind.

---

## 5. Honest assessment by area

**Collecting data.** The refonte's buffered approach is the right one and the review asked for it. Two things stop it from being the default today: the unexplained 700 ms delay, and the missing rate limiter that breaks with three aircraft at once. The baseline's approach is simpler and held up under load, but it can never recover a moment it missed.

**Starting and stopping cleanly.** Both branches fail. The refonte improved some cases and broke Ctrl-C.

**Deciding what happened.** The baseline is ahead on what you asked for: it detects traps from movement, estimates the wire from the hook, handles pilots who leave right after landing, and has 14 real recorded flights to test against. The refonte has cleaner internal plumbing and a better wire correlation method, but it refuses eventless traps and lost the recorded flights.

**Grading.** The refonte's grader is better engineered and better tested. It is also more generous than the baseline's: it gives full points to passes with no proven outcome, uses a shared threshold for AOA reversals, counts stabilisation in samples rather than seconds, and lets AOA change the grade based on a wind reference that may have been substituted. None of these is provably wrong. All of them are opinions without a human LSO to check against. The rule from the review still applies: do not change how data is collected and how grades are computed in the same step, or you will never know which change caused what.

**Versions, build robot, file writing.** The refonte wins clearly. The typed-in stub version and the committed binary undermine it slightly.

**Documentation.** The refonte's French docs are refreshingly honest about what has and has not been tested live. But the README is wrong on two points, the docs mix French and English, and the change log has duplicated sections.

---

## 6. The to-do list, explained

The original document gives nine steps in a deliberate order. Steps 1 to 3 must be done before merging anything. Steps 4 to 6 are fixes the review already asked for. Steps 7 to 9 are about tuning and checking.

### Step 1. Use the refonte as the starting point, but do not deploy it yet

Make a new branch starting from Lenny's refonte. Call it an integration branch. Do not install it on the server as the default until steps 2 to 5 are done. Keep the option `--position-source unary` documented as the way to fall back to the old collection method if buffered mode misbehaves.

**Why.** The refonte is the better skeleton. It is just not ready to run in production as-is.

### Step 2. Bring the baseline's database contract into it

Three concrete things:

- Delete the built-in web page (`src/web.rs`) and the web library it uses (axum).
- Take the baseline's database settings (WAL mode and a busy timeout, which let the dashboard read while the program writes).
- Pick one JSON schema number for the merged code and stick to it.

**Why.** The separate dashboard reads the database file. The database layout is the promise you must not break.

### Step 3. Bring the baseline's evidence features and recorded flights into it

Concrete list:

- The 14 real recorded flights from 2 and 3 September, with their hook data files and the test harness that replays them.
- The fix for pilots who leave right after touchdown.
- The movement-based trap detection.
- The hook-based wire estimate.

Then write down the policy in the primer and roadmap, in one sentence: *a trap confirmed by movement and a hook transient counts as a real trap at medium confidence, and the report never invents a wire number.*

**Why.** This is the human-LSO requirement. It is the reason the baseline currently gives better results.

### Step 4. Fix the new data-collection bugs before making buffered mode the default

Four small fixes:

- One shared rate limiter across all recorders, kept below the server's 20 reads per second.
- Run the "10 seconds since touchdown" check even when a batch of data is empty.
- When the server says the unit ID changed, end the attempt with a clear reason.
- Give the recorder loop an explicit "shut down now" branch so Ctrl-C leads to a report.

### Step 5. Fix the review's leftover high-priority items

- F03: in the Harrier landing code, check for duplicates before changing any numbers.
- F05: validate numbers in the unary path and the aligner, and add one single validity gate inside the tracker.
- F17: unknown AOA must not be coloured as Slow.

### Step 6. Fix the version strings and clean the repository

- Read the stubs version from the stubs package instead of typing `"0.10.0"` in two places.
- Remove `releases/lso.exe`, `trap sample/` and `graphify-out/` from source control.
- Keep the launch script that contains the Discord webhook out of source control.
- Install the released `v0.9.2` server build, and document in the admin guide that `recoveryTelemetry.enabled = true` must be set in the server config for buffered mode to work.

### Step 7. Find out why data delivery is slow

Run one human session with the hook sampler turned off, then one with `--positions-only`, on the same mission with the same number of players. Compare the `position_poll_p95_latency_ms` number in the reports. If the hook sampler is the cause, change the design so hook samples come through the buffered engine instead of a separate stream. That would also fix the delayed hook time stamps.

### Step 8. Only now, tune the grading, and only against a real LSO

Have a human LSO write down their grade for a set of passes. Run both graders (baseline version 1 and refonte version 7) on the same passes and compare to the human. Decide two policy questions separately: does an "approach only" pass earn points, and may AOA change the grade. Give every set of thresholds a version code so reports can be regrouped later when thresholds change.

### Step 9. Re-check the Harrier on the merged code

The 9 September Harrier passes show groove entry is broken. Use the baseline's Tarawa spot logic and the F03 test case as the pass/fail test.

---

## 7. Are the two copies of the server plug-in at the same level?

There are two copies of the DCS-gRPC server plug-in code: yours (`sevenfifty777/rust-server`) and Lenny's (`LennyKruger/rust-server`). The short answer is **no, Lenny's is behind yours on every branch**, and everything Lenny ever pushed is already inside your `v0.9.2` release.

| Lenny's copy | Compared to yours |
|---|---|
| His `main` | 16 commits behind, nothing you do not have, no buffered telemetry at all |
| His refonte branch | 6 commits behind, nothing you do not have, already merged into your main |
| His tags and releases | none |

**What your `v0.9.2` has that Lenny's does not:**

- A new question the LSO program can ask: "what is the hook state of the player's own aircraft?"
- Timing diagnostics on the position question: how long the request waited in the queue, how long the Lua code took, how deep the queue was. These are exactly the numbers needed to explain the 700 ms delay in section 4.
- Several Lua-side improvements, including a fix to a counter that was off by one.
- A floating-point fix, tests for the Lua engine in the build robot, release scripts, and removal of an experimental folder Lenny had left in.
- The version number was renamed from 0.10.0 to 0.9.2 at release time.

**Consequences for Lenny's LSO branch:**

- All 158 of his reports say server 0.10.0, so his sessions ran on a server he built from his own fork, not from your release. That server lacks the diagnostics and the counter fix.
- His LSO code has `"0.10.0"` typed into it in two places. Against your real `v0.9.2` server, the compatibility check will say "incompatible API line". This is only a warning and changes nothing in behaviour, but every report will carry that label until the typed-in strings are replaced with the version read from the stubs package.
- His `SYNC_UPSTREAM.md` document describes pulling changes from his LSO fork. For the server plug-in the direction is the opposite: he should pull from your `main` or check out the `v0.9.2` tag, rebuild, and reinstall the DLL and Lua files.

**Will his LSO branch work against your `v0.9.2` server?** Yes. Every question it asks exists in `v0.9.2` with the same shape. The additions are optional. Three things to know:

1. The version warning described above. Cosmetic.
2. **One real bug in `v0.9.2` that is not in Lenny's copy.** A Lua file (`recovery.lua`, line 125) calls a helper function `GRPC.errorResourceExhausted` that no longer exists in `v0.9.2`'s `grpc.lua`. When the engine reaches its limit (16 active recoveries or 8 active carriers), instead of politely saying "resource exhausted", the Lua code crashes with "attempt to call a nil value" and the client receives a generic error. The automatic tests did not catch it because they use a fake version of that helper. **Fix:** put the helper back into `grpc.lua`. The Rust side already knows how to map it.
3. The counter fix and the diagnostics change timing slightly and add fields. Neither affects his LSO program.

**Action.** Add the missing Lua helper, release `v0.9.3`, have Lenny install that release, delete or rebase his stale server branch, and replace the typed-in version strings in the LSO code so the next batch of reports carries the correct version.

---

## 8. What was actually checked to write this

To be clear about what the original comparison is based on:

| Check | Result |
|---|---|
| Ran all automatic tests on the baseline | 148 passed |
| Ran all automatic tests on the refonte | 292 passed |
| Ran the code-quality tools on the refonte | clean |
| Ran the refonte's `lso file` command on four baseline recorded flights | it runs, but writes only pictures, so no numbers to compare |
| Read all 158 refonte report files for grading, timing and version fields | summary in section 4 |
| Live DCS, dashboard, Discord | **not tested** |

The last line matters: nothing in this comparison was verified against a running simulator. The findings come from reading the code, running the automatic tests, and mining the report files Lenny already produced.

Three separate code audits were run on the refonte and their file and line references are in section 3 of the original document. The server fork comparison used standard git commands. No branch was modified, committed or pushed. The temporary build folders were removed afterwards. One thing was left in place: a git "remote" named `lenny` pointing at Lenny's server fork, in your local `rust-server` folder. It is harmless and useful for the next comparison.
