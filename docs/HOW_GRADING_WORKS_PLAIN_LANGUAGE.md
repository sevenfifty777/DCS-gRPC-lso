# How the LSO program grades a pass, in plain language

This document explains what the program actually does, step by step, from the moment it notices an aircraft near the carrier to the moment a grade appears on the chart, in Discord and in the database. It is written for someone who does not read code.

It was written on 13 September 2026 from two sources only: the code as it stands at commit `664fe5b` (version 0.5.0), and the eight recorded passes in `trap_records/recovery_13-09/`. Every number in it comes from one of those two places. Older documents in `docs/` describe earlier versions of the program and may say something different; where they disagree, this document reflects the code. An appendix at the end says which file each rule lives in, so a developer can check any sentence.

Two words used everywhere:

- **Sample.** One snapshot of the aircraft and the ship: position, altitude, heading, bank, velocity, and the time on the DCS clock. The program receives twenty of them per second.
- **Reference point.** The spot on the deck where a perfect pass would put the hook. For CVN carriers it is between wire 2 and wire 3. Every distance in the grading is measured from the aircraft's hook to that point, along the angled deck.

---

## 1. The big picture

A pass goes through nine stages. Each has its own section below.

1. **Noticing the aircraft.** A recorder is started when an aircraft is near the ship. It stops when the aircraft leaves, lands, or a timer runs out.
2. **Collecting data.** Twenty position samples per second, about four hook-animation samples per second, DCS's own landing events, and two wind readings.
3. **Deciding when the groove starts.** The program watches for the final turn and the roll-out. Only what happens after that moment is graded.
4. **Measuring the approach.** At every sample: how far above or below the ideal glide path, how far left or right of the centreline, the angle of attack, the sink rate and the bank.
5. **Deciding what happened at the deck.** Trap, bolter, touch-and-go, wave-off, or "approach only".
6. **Proving the trap.** A touchdown event is not proof of a trap. Three kinds of evidence can confirm it.
7. **Naming the wire.** DCS's number when it sends one, otherwise an estimate from the hook animation.
8. **Grading the approach.** Safety cuts first, then a study of every deviation "episode", then a grade.
9. **Reporting.** The JSON file, the two charts, the Discord message and the database row.

---

## 2. Noticing the aircraft

The program watches every aircraft on the server. When one comes within about 3.5 nautical miles of a carrier and below 1,100 feet, it starts a recorder for that aircraft and that ship. The recorder follows the aircraft until one of these happens:

- the aircraft goes back outside 3.5 nautical miles or above 1,100 feet;
- the aircraft touches the deck and then either stops relative to the ship (a trap) or moves away from it (bolter, touch-and-go or wave-off);
- ten seconds have passed since an accepted touchdown;
- nothing at all has been received for 29 seconds.

Not every recorder produces a report. A recorder whose aircraft never went below 100 metres of altitude is thrown away silently, and so is one that never reached the groove and never touched the deck. On 13 September the pilot's seven catapult launches and climb-outs each started a recorder that was discarded a minute later. That is normal and costs nothing except a line in the log.

---

## 3. Collecting data

Four streams feed the grading.

**Position samples, twenty per second.** DCS records the aircraft and the carrier into a buffer on its side; the program reads that buffer about sixteen times a second and gets every sample that accumulated. Each sample carries both the aircraft and the carrier at the same instant, so their relative position is exact. Every sample is checked at the door: a non-finite number, a clock going backwards, or a gap of more than 300 milliseconds since the previous one marks the sample as invalid. Invalid samples still count in the telemetry-quality statistics but never move the geometry, the outcome or the grade.

**Hook animation, about four per second.** Separately, the program asks DCS for the value of the tail-hook animation. It is a number between 0 (hook up) and 1 (hook down). On the T-45 it reads exactly 1.0 with the hook down and drops to exactly 0.0 when the hook catches a wire, then returns to 1.0 about two seconds later when the cable pull-back ends. On the F-14 the same sequence reads 1.0, then about 0.16 for four to five seconds, then about 0.86. This stream is what tells the program whether the pilot had the hook down (section 6) and, when DCS says nothing, which wire was caught (section 8).

**DCS events.** DCS announces `runway_touch` and `land` when wheels touch the deck. Only an event whose aircraft and ship match the recorder, and whose hook is within 200 metres of the reference point, is accepted; the second `land` event DCS sends one second later is rejected as a duplicate. When DCS's built-in AI LSO is active it also sends a "landing quality mark", a text line such as `LSO: GRADE:C : _LULX_ LOAR WIRE# 1`. The program keeps the whole line for display and reads two things out of it: whether it starts with `GRADE:WO` (a wave-off) and the number after `WIRE#` (only 1 to 4 are accepted).

**Wind.** At the instant the groove starts, the program asks DCS for the wind twice: at the aircraft's altitude and at deck level. DCS sometimes answers "180 degrees, 0 m/s" for the deck-level reading, which the program treats as a known bad answer; it retries once and, if the bad answer persists, uses the high-altitude reading for both. The wind is used for one thing only: correcting the angle of attack (section 5). Whether the wind reading was substituted is recorded in the report (`wind_reading_is_groove_entry_fallback`).

**Carrier smoothing.** DCS moves the carrier in steps of about ten metres every 1.4 seconds rather than smoothly. The program smooths the carrier's position over time to remove the resulting sawtooth from the charts and gate distances. One measurement deliberately uses the raw, unsmoothed position: the "has the aircraft stopped relative to the ship" test in section 7.

---

## 4. Deciding when the groove starts

The grade is only about the groove, the final straight leg. The program does not use a fixed distance for its start. It looks for the physical roll-out after the last turn of a Case I pattern, using this sequence:

1. **Pattern observed.** The aircraft is seen to the left of the extended centreline by more than 0.75 degrees. That is where a port-side pattern puts it on the downwind and in the turn.
2. **Last turn armed.** While inbound, below 600 feet, the aircraft banks more than 10 degrees. This is taken as the final turn.
3. **Roll-out confirming.** The bank comes back under 10 degrees while the aircraft keeps making progress toward the ship, still below 600 feet, with no gap in the data longer than 300 milliseconds.
4. **Groove confirmed.** Step 3 has held continuously for 0.75 seconds. The groove starts at that sample.

If during step 3 the bank goes back above 10 degrees, the count restarts. If the aircraft moves away from the ship by more than 150 metres, the whole sequence resets so a new circuit starts clean.

Lineup, ground track and how fast lineup is changing are measured at that moment and written to the report as diagnostics, but none of them delays or blocks the groove. A pilot who rolls out 2.7 degrees left of centreline (pass 1 on 13 September) is in the groove, with a lineup error the grading will see.

On 13 September all eight passes triggered this way, between 723 and 1,076 metres from the reference point.

**Groove time** is the time from that sample to the DCS touchdown event. It is only used for the "perfect pass" rule (section 9) and is printed on the report. The eight passes ran from 12.8 to 21.7 seconds.

When the groove starts, everything measured on an earlier attempt (a bolter that came round again, a low pass over the ship) is cleared, so only the attempt being graded is scored.

---

## 5. Measuring the approach

### The frame of reference

At every sample the program computes three numbers for the hook:

- **x**, the distance to the reference point measured along the angled deck. Positive means short of it, negative means past it.
- **y**, the sideways offset from the extended centreline. Positive is right, negative is left.
- **alt**, the height of the hook above the deck.

The hook position is the aircraft's position plus a fixed offset for its type (for the T-45 the hook sits 4.8 metres behind and 1.8 metres below the aircraft's reference point, rotated with the aircraft's attitude). For the F-14 an extra +1.0 metre vertical correction is applied on top of the measured model offset, because a live test in early September showed the modelled hook sitting about a metre below the deck while on it.

The ideal glide path is 3.5 degrees for every carrier-hook aircraft in the program (F/A-18C, F-14, T-45). At distance x the ideal hook height is x multiplied by the tangent of 3.5 degrees. At 926 metres that is 56.6 metres; at 100 metres it is 6.1 metres.

### The three gates

Three fixed distances are called gates: 3/4 nautical mile (1,389 metres), 1/2 (926 metres) and 1/4 (463 metres). When two consecutive samples straddle a gate distance, the program interpolates the hook's height and sideways offset at exactly that distance and records:

- glideslope deviation in degrees, the angle between the actual and ideal height seen from the reference point (height error divided by the gate distance);
- lineup in degrees, the angle between the hook and the centreline seen from the reference point.

A gate reading is only valid if both straddling samples are valid, inbound, less than 300 milliseconds apart, below 500 feet, and (for the Harrier only) within 10 degrees of lineup.

The 3/4 gate has one special rule. On a Case I pattern the aircraft usually crosses 1,389 metres while still in the final turn, so the reading there is a turn artifact. On 13 September every 3/4 reading was captured before the roll-out, with lineups of 6 to 11 degrees left. When the 3/4 gate was captured before the groove started it is ignored by the grading and is not required. The 1/2 and 1/4 gates are always required, in that order, for the pass to be gradable. The report and chart still print the 3/4 reading.

### The continuous trajectory

From the groove start to touchdown, every valid inbound sample is also turned into the same two deviations, at the aircraft's own distance instead of a fixed gate. This series is what the grading actually studies. Two adjustments apply close to the ship:

- Below 75 metres from the reference point, the glideslope angle is computed as if the aircraft were still 75 metres out. Otherwise a normal few-decimetre flare offset would turn into tens of degrees as the distance shrinks to nothing.
- Below 150 metres, the lineup angle is computed as if the aircraft were still 150 metres out, for the same reason.
- Samples closer than 3 metres are not recorded at all.

A practical consequence, visible on 13 September: when the hook reaches the deck 19 metres short of the reference point, the ideal path at that spot is still 1.16 metres above the deck, so the last samples read about 0.87 degrees "low". Close to the deck the glideslope number is really a measure of where the aircraft touched down.

Each trajectory sample also records the sink rate (change of hook height since the previous sample) and the bank angle. Both feed the safety cuts in section 9.

### Angle of attack

DCS is not asked for the aircraft's own AOA gauge. The program computes it:

- **Without a wind reference**: the angle between the aircraft's nose direction and the direction it is moving over the ground.
- **With a wind reference** (the normal case, once the groove has started): the wind vector at the aircraft's altitude is subtracted from the ground velocity to get the air velocity, that vector is expressed in the aircraft's own axes, and only the vertical component is kept. Sideways crab is discarded, which is what a real AOA vane does.

The number is then classified against a band for the aircraft type, in degrees:

| Type | Fast | Slightly fast | On speed | Slightly slow | Slow |
|---|---|---|---|---|---|
| F/A-18C | up to 6.9 | 6.9 to 7.4 | 7.4 to 8.8 | 8.8 to 9.3 | 9.3 and above |
| F-14 (all) | up to 9.7 | 9.7 to 10.2 | 10.2 to 11.1 | 11.1 to 11.6 | 11.6 and above |
| T-45C | up to 6.0 | 6.0 to 6.5 | 6.5 to 7.5 | 7.5 to 8.0 | 8.0 and above |
| AV-8B | below 10 | – | 10 to 12 | – | above 12 |

The T-45 band was derived from the module's own cockpit display code. The F-14 band comes from a forum conversion formula and, on every F-14 pass recorded so far, reads three to four degrees above where the pilots are actually flying, so it is under review (see the 13 September session review). The AV-8B band colours the chart only and never affects a grade.

The AOA classification is used in two places: the colour of the trace on the chart, and, for carrier-hook aircraft only, as a third grading axis alongside glideslope and lineup (section 9). AOA affects the grade whenever a wind reference exists, including when the deck-level wind reading was substituted.

---

## 6. Deciding what happened at the deck

The program keeps an "outcome" for the attempt. It starts empty and is set by the first of these that applies.

**DCS touchdown event.** An accepted `runway_touch` or `land` sets the outcome to "recovered" provisionally, records the touchdown time, and records the aircraft's horizontal speed at that instant. Provisional means: the aircraft touched the deck, nothing yet says it stopped.

**DCS says wave-off.** A landing quality mark that starts with `GRADE:WO` sets the outcome to "wave-off" when no touchdown has been recorded, and overrides a bolter the program had decided from geometry alone.

**Crossing the reference point without an event.** If the hook passes the reference point (x goes from positive to negative) below 50 feet and no DCS event arrives, the program notes a deck crossing and whether the hook was within 1 metre of the deck at that instant.

**Moving away.** Once the aircraft is 150 metres farther from the reference point than its closest approach, the outcome is settled:

- Provisional "recovered", and the deck-kinematics test (section 7) says the aircraft stopped: it is a trap; the aircraft is simply taxiing. Recording stops.
- Provisional "recovered", hook was up: **touch-and-go**.
- Provisional "recovered", hook down, did not stop: **bolter**.
- No event, deck crossed, hook up: **touch-and-go**.
- No event, deck crossed, hook within 1 metre of the deck: **bolter**.
- No event, deck crossed higher than that: **wave-off, initiator unknown**.
- No event, in the groove, deck not crossed: **wave-off, initiator unknown**.

**Stopped without any event.** If DCS sent no touchdown event at all but the hook crossed the reference point and the aircraft then stopped relative to the ship (section 7), the outcome becomes "recovered" from kinematics alone, and the recorder stops ten seconds after the aircraft first went slow.

**Nothing happened.** If the groove was entered (or a 1/2 or 1/4 gate was captured) and none of the above fired before the recorder ended, the outcome is **approach only**.

**Hook state.** "Hook up" and "hook down" come from the hook-animation stream: a value of 0.2 or below is up, 0.8 or above is down. The program reads the value from groove samples that end 1.5 seconds before the earliest contact evidence, so the arrestment deflection itself never flips a real trap to "hook up". It needs at least two samples over 0.2 seconds to call "down" and three over 0.4 seconds to call "up"; otherwise the state is "unknown" and the hook is not used in the decision.

On 13 September, three passes were flown with the hook up. Each produced a `runway_touch` event, the aircraft stayed at 50 to 67 m/s, and the outcome was touch-and-go.

---

## 7. Proving the trap

A touchdown event proves contact, not a trap. Before a "recovered" pass can earn points, one of three proofs must exist. They are checked in this order and the first one found is recorded as `arrest_evidence` in the report.

1. **DCS wire number.** The landing quality mark contains `WIRE# n` with n from 1 to 4. Confidence "high". This is authoritative: the wire it names is the wire.
2. **Hook transient.** The hook animation shows the complete arrestment signature described in section 3, near the touchdown, and it lines up with the hook passing a wire (section 8). Confidence "medium".
3. **Deck kinematics.** Measured against the raw carrier position, the aircraft's speed relative to the ship drops to 6 m/s or less within 8 seconds of the contact reference (the touchdown event, or the deck crossing if there was no event), stays under 8 m/s for 2 seconds without a data gap, and does so within the deck run-out band (from 60 metres short of the reference point to 160 metres past it). Confidence "medium". This proof never names a wire on its own.

If none of the three exists, the pass is recorded as `unconfirmed_arrest`, the grade is still computed and shown, but no points are awarded.

There is a fourth signal in the report, called the kinematic "velocity signature". It requires the touchdown event, a measured deceleration onset within 1.2 seconds of it, a two-second hold below 5 m/s, no bounce and no forward departure. It is written to the report for diagnosis but never changes the outcome. On every trap of 13 September it read "rejected: forward departure detected", because the pilot taxied forward after the wire retracted. That is expected and harmless; the deck-kinematics proof above had already confirmed the trap.

A hook-up contact that none of the three proofs confirms is reclassified as a touch-and-go even if the recorder ended before the aircraft moved away.

Pass 8 on 13 September is the case the whole design exists for: DCS sent a `land` event but no landing quality mark, so there was no wire number. The deck kinematics confirmed the stop 3 seconds after contact, the pass was recorded as `arrest_evidence: kinematic` with medium confidence, and it was graded with points.

---

## 8. Naming the wire

**When DCS gives a number**, that number is the wire. The chart says "Arrested — wire 2". The program still computes its own estimate for comparison, and the report records both (`wire_dcs`, `wire_estimated`) plus whether they disagree (`wire_divergent`). The chart never shows the estimate when DCS spoke.

**When DCS gives nothing**, the estimate is used and the chart says "Wire #1 (Rust estimate)". The database `wire` column holds the DCS number when there is one, otherwise the estimate.

**When the hook was up** (an intentional bolter or a touch-and-go), nothing was caught, but the same crossings say which wire the hook would have caught, and that is worth telling the pilot. The outcome then reads "T&G (CQ) — would have caught wire 2", the report's `wire_estimation.reason` is `hypothetical_hook_up_plane_crossing` and `wire_primary` is `rust_hypothetical`, so the number can never be mistaken for an arrestment.

The estimate is built from three pieces of evidence, tried in this order.

**Wire crossings.** Each wire is modelled as a line between its two deck anchor points, taken from the carrier's 3D model. At every sample the program checks whether the hook point moved from behind a wire's line to in front of it, while being between the two anchors and within 3 metres of the wire's height. The moment of crossing is interpolated between the two samples. On 13 September the four crossings of each trap spanned 0.6 to 0.8 seconds, about one wire every 200 to 250 milliseconds.

**Method A, the hook transient.** The program looks for: the hook reading 0.8 or more for at least 0.2 seconds, then 0.7 or less on the very next sample, within 2 seconds of the touchdown reference, then back to 0.8 or more within 8 seconds. The wire is the last crossing that happened between 0 and 200 milliseconds before that deflection sample. Any sample gap in this search longer than 300 milliseconds disqualifies the pair.

**Method B, the stop position**, used when method A finds nothing and the deck kinematics have confirmed a stop. The arresting gear's run-out is a constant of the aircraft type in DCS: a Tomcat comes to rest 85 to 89 metres past the wire it caught, whatever its entry speed (ten of ten traps across the 2, 3, 14 and 15 September 2026 recordings). So the stop position plus 87 metres is the wire's position, matched against the recorded crossings; if no crossing sits within 6 metres of it, method B names nothing. The T-45C (49 to 61 metres between recordings) and the F/A-18C (60 and 89 metres) do not have a usable constant yet and skip this method. Added on 15 September 2026 after a Tomcat trap with no landing mark was named "wire 1" by method C when it was a 2-wire caught in the air (`docs/RECOVERY_REVIEW_2026-09-15.md`, section 6).

**Method C, the deceleration fallback**, used when neither A nor B names a wire, and on every hook-up pass. The program watches the aircraft's horizontal speed and marks the "deceleration onset" as the first of two consecutive samples slowing at 5 m/s² or more. The touchdown event must fall between 0 and 300 milliseconds after that onset; otherwise no wire is named. The wire is then the earliest crossing that happened no more than 1.2 seconds before the onset. On a trap this tends to answer "1-wire" whatever was caught, because the onset is detected about 55 metres past the wire, later than the hook takes to sweep all four planes; that is why method B sits before it.

**Confidence labels.** "High" requires a DCS-confirmed trap plus tight sample brackets; otherwise "medium"; "insufficient" when no wire could be named, with a reason string in the report.

What this means in practice: the hook is sampled only four times a second, and wires pass under it four times a second, so the estimate cannot be better than about one wire either way. On 13 September it agreed with DCS on one trap, was one wire off on two, and could not answer on one. Details and the causes are in the session review.

---

## 9. Grading the approach

### Who gets graded

A grade is computed for a trap, a bolter, a touch-and-go and an "approach only" pass, as long as the required gates (section 5) are valid and in order. A wave-off gets the label `WO?` and no grade. A pass whose gates are missing gets `NC` (not complete).

### Step one: safety cuts

Before anything else, three checks can end the grading with a **Cut** (`C`, 0 points):

- glideslope more than 2.5 degrees low at the 1/4 nautical mile gate, or at any trajectory sample inside 463 metres;
- sink rate of 8 m/s or more on three consecutive samples inside 463 metres;
- bank of 30 degrees or more on three consecutive samples inside 463 metres.

None of the eight passes on 13 September came near these (worst sink rate 6.75 m/s, worst bank 7 degrees).

### Step two: episodes

The trajectory is cut into four zones by distance to the reference point. Each zone has a weight that says how much a deviation there matters.

| Zone | Distance | Weight | Time allowed for a "good" correction |
|---|---|---|---|
| Start | more than 926 m | 1.0 | 3.0 s |
| Middle | 463 to 926 m | 1.2 | 2.5 s |
| In close | 150 to 463 m | 1.5 | 1.5 s |
| Ramp | under 150 m | 2.0 | 0.75 s |

Every sample is given a severity on each of three axes:

| Axis | None | Small | Medium | Large |
|---|---|---|---|---|
| Glideslope (degrees) | under 0.5 | 0.5 to 1.0 | 1.0 to 2.5 | 2.5 and above |
| Lineup (degrees) | under 1.0 | 1.0 to 2.0 | 2.0 to 3.0 | 3.0 and above |
| Angle of attack | on speed | slightly fast or slow | fast or slow | never |

An **episode** is a run of consecutive samples where the severity is not "none", ending when two consecutive samples are back to "none". A run of a single sample is ignored as noise; two samples (a tenth of a second) is enough to count.

For each episode the program records:

- the **peak**: the worst sample, its zone and its value;
- the **maximum severity**: the worst level reached;
- the **correction quality**, judged on what happened after the peak:
  - **Good**: the severity dropped one level within the zone's time allowance and stayed there for at least two samples, and either returned all the way to "none" or ended a level below the peak.
  - **Average**: it improved, but late or without stabilising.
  - **Poor**: two or more reversals of direction after the peak (swings of at least 0.3 degrees), or it got worse again after improving, or it never improved, or the peak was in the ramp zone and the trajectory ended (touchdown) before it stabilised.

The correction quality moves the severity level: good takes one level off, average leaves it, poor adds one (capped at large). The result multiplied by the zone weight is the episode's **effective severity**.

Example from pass 1 on 13 September: lineup peaked at 2.7 degrees left at the very start of the groove (medium, in the middle zone), improved slowly and only reached "none" 13.75 seconds later, so the correction was "average"; medium stays level 2, times weight 1.2, effective severity 2.4.

### Step three: the grade

The episode with the highest effective severity decides:

| Worst effective severity | Grade | Points |
|---|---|---|
| under 1.5 | `OK` | 4.0 |
| 1.5 to under 3.0 | `(OK)` | 3.0 |
| 3.0 and above | `--` | 2.0 |

With the weights above, this means: a small deviation corrected well is free anywhere; a small deviation not corrected is `(OK)` in the middle and in close and `--` at the ramp; a medium deviation is `--` everywhere except the start zone, where it needs a good correction to stay `(OK)`.

**Perfect pass** (`_OK_`, 5 points) is reserved for a trap where there were no episodes at all, every gate and the whole trajectory stayed within 0.4 degrees high, 0.3 degrees low and 0.5 degrees of lineup, nothing was getting worse in the last four seconds, and the groove time was between 15 and 18 seconds. A touch-and-go that meets this is capped at `OK`.

**AOA and the grade.** The AOA axis takes part in the episodes exactly like glideslope and lineup, with severity "medium" for fast or slow. The "distance from the on-speed band" is used to judge improvement and reversals. If no wind reference was established, AOA episodes are still written to the report but marked as not affecting the grade.

**Approach only.** A pass with no outcome at all is graded on its groove like any other and does earn points; the reason text is prefixed with "Approach only (outcome unknown)".

**Bolter** is `B`, 2.5 points, regardless of how the approach looked, as long as the gates are valid.

### What the pilot is told

The report and the Discord message carry a one-sentence reason built at the same time as the grade, for example: "--: AoA lent en RAMP, sans retour stable vers la cible après le pic." It names the axis, the maximum severity, the zone of the peak and the correction quality of the deciding episode. When DCS sent a landing quality mark, its text is shown as "LSO Notation" with an English translation of the shorthand; when it did not (every touch-and-go, and traps with no AI LSO), a plain-language list of the program's own measured deviations is shown instead, labelled as measured by the program and not a DCS comment.

---

## 10. When a grade carries no points

A grade can be computed and displayed but still award no points. The report says so with `points_eligible: false` and a `cause`. This happens when:

- the required gates were not captured in valid order (`insufficient_gates`);
- a data gap of more than one second, or an invalid sample, fell inside the scored part of the groove (`telemetry_gap`, `invalid_telemetry`);
- the recorder's memory limit was hit (`buffer_limit`, never seen live);
- the pass was a deck contact with the hook down that none of the three proofs in section 7 confirmed (`unconfirmed_arrest`).

If there is no approach evidence at all, the label becomes `NC`. A wave-off never carries points.

Every report also carries a **telemetry health** colour, computed over a rolling ten-second window: red when a sample is invalid or over a second late, when the rate falls under 6 per second, or when 15 percent of samples are late or gapped; orange from 8 per second or 5 percent; green otherwise. All eight passes of 13 September were green.

---

## 11. The Harrier is different

For the AV-8B on the Tarawa the recovery is vertical, so most of the above does not apply:

- there is no hook, no wire, no arrest proof; the outcome is set by the DCS landing event, and a contact followed by departure is labelled a wave-off rather than a touch-and-go;
- the groove starts when the aircraft enters a box (inside 3/4 nautical mile, below 300 feet, within 10 degrees of lineup) with no roll-out logic;
- the ideal path is 3.0 degrees toward a point 120 feet above the water abeam spot 7.5, not a deck touchdown point;
- the approach grade is the average of the three gate scores (each gate is scored `OK`, `(OK)`, `--` or Cut on the same thresholds), with no episodes and no AOA;
- a landed pass gets a spot-accuracy bonus from the distance between the pilot's ground reference and the calibrated spot: under 1 metre `A` (+1.0), under 3 `B` (+0.75), under 5 `C` (+0.5), otherwise `D` (+0); the sum is capped at 5 and mapped back to the same labels.

---

## 12. What is written where

| Where | What it shows |
|---|---|
| JSON report | Everything: every sample, every gate, the full trajectory, every episode with its numbers, the wire crossings, the hook timeline, the arrest proofs, the telemetry statistics, the version of the program and server that produced it. |
| Approach chart (`.png`) | Side view and top view of the groove, coloured by AOA (red fast, green on speed, yellow slow), the grade, the outcome line, the three gate readings including the 3/4 one even when the grading ignored it. |
| Pattern chart (`-pattern.png`) | The whole circuit from above, with earlier circuits faded. |
| Discord | Grade, outcome, "Why This Grade", the DCS notation and its translation or the program's own measured notes, wind, groove time. |
| Database `lso.db` | One row per pass for the dashboard: grade, points, wire (DCS if present, else the estimate), the DCS text, arrest evidence, hook state, telemetry health, and the reason codes. |
| Tacview file (`.zip.acmi`) | The recorded positions, written by the program itself, with its own computed AOA. |

---

## 13. Three passes from 13 September, end to end

**Pass 1, T-45, 18:01 UTC.** Groove confirmed at 870 metres with the aircraft 2.7 degrees left. Gates: 1/2 nm 0.35 degrees high and 3.0 left; 1/4 nm 0.13 high and 2.2 left. The hook reached the deck 17 metres short of the reference point. DCS sent `runway_touch`, then a landing quality mark with `WIRE# 1` and its own grade `C`. Arrest proof: DCS wire, high confidence. The program's own wire estimate said 2 (the hook deflection sample came 290 milliseconds after the wire-1 crossing, outside the 200 millisecond window, and 50 milliseconds after wire 2's). Five episodes; the deciding one was AOA "slow" at the ramp, peak 9.49 degrees against a slow threshold of 8.0, never stabilised before touchdown, so poor correction, level 3, times 2.0, effective 6.0. Grade `--`, 2.0 points.

**Pass 4, T-45 touch-and-go, 18:24 UTC.** Hook up throughout the groove (animation at 0.0). Gates: 1/2 nm 0.63 high and centred; 1/4 nm 0.66 high and 0.3 left. Glideslope came down from 0.95 degrees high at 150 metres to on-path at touchdown; that episode was "average" and scored 2.0, which alone would be `(OK)`. But AOA read 8.07 for 0.85 seconds at the ramp, 0.07 above the slow threshold, with no improvement seen in the two samples that followed: poor, level 3, times 2.0, effective 6.0. Grade `--`, 2.0 points. No DCS comment, so the Discord notes were the program's own measured deviations.

**Pass 8, F-14, 18:56 UTC.** DCS sent `land` but no landing quality mark, so no wire and no DCS grade. Hook down. The deck-kinematics test confirmed the stop 3 seconds after contact and held it. Arrest proof: kinematic, medium confidence. Method A found no hook transient (a 300.0000000002 millisecond gap between two hook samples failed the 300 millisecond limit), so method B named wire 1 at medium confidence, and that number went on the chart and into the database. Glideslope and lineup were clean; the AOA axis read "fast" for the whole groove against the F-14 band and decided the grade: `--`, 2.0 points.

---

## Appendix: where each rule lives

| Rule | File and line (commit `664fe5b`) |
|---|---|
| Pattern zone 3.5 nm / 1,100 ft, moving-away rules, outcome decisions | `src/track.rs:2212`, `src/track.rs:2260-2404` |
| Groove roll-out state machine and its thresholds | `src/track.rs:175-208`, `src/track.rs:1525-1617` |
| Gate distances, gate capture and validity | `src/track.rs:60`, `src/track.rs:4676-4760`, `src/track.rs:1048-1088` |
| Trajectory samples, 75 m and 150 m references, 3 m cutoff | `src/track.rs:105-141`, `src/track.rs:2657-2683`, `src/track.rs:4388-4401` |
| Hook offsets, glide slope and AOA bands per aircraft | `src/data.rs:139-321` |
| Wire anchor positions per carrier | `src/data.rs:11-137` |
| Raw AOA | `src/transform.rs:59` |
| Wind-corrected AOA and the wind reference | `src/track.rs:806-841`, `src/tasks/record_recovery.rs:935-1000` |
| Hook state from the animation value | `src/track.rs:4100-4172` |
| Touchdown event acceptance | `src/track.rs:2740-2889` |
| Arrest proofs and their precedence | `src/track.rs:3373-3485`, `src/track.rs:3685-3805`, `src/track.rs:3159-3176` |
| Wire crossings, hook transient, deceleration fallback | `src/track.rs:3285-3348`, `src/track.rs:3556-3614`, `src/track.rs:3865-3979` |
| Safety cuts | `src/grading.rs:1175-1201`, `src/grading.rs:1464-1510` |
| Zones, severities, episodes, correction quality | `src/grading.rs:143-177`, `src/grading.rs:783-807`, `src/grading.rs:846-1041` |
| Grade from episodes, perfect pass, touch-and-go cap | `src/grading.rs:1307-1330`, `src/grading.rs:1217-1235`, `src/grading.rs:603-657` |
| Points per label | `src/grading.rs:328-338` |
| Points withheld, causes, availability | `src/track.rs:3128-3189`, `src/tasks/record_recovery.rs:1440-1492` |
| Harrier gate average and spot bonus | `src/grading.rs:345-411`, `src/grading.rs:575-598` |
| Telemetry health colours | `src/track.rs:2065-2088` |
| Ten-second post-touchdown cutoff | `src/tasks/record_recovery.rs:2194` |
