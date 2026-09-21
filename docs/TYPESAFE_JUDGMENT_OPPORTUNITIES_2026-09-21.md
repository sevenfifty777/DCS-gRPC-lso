# Where a typed AI judgment can stand in for parsing and hand-written rules: a TypeSafe assessment

Branch `jev-plan`, 21 September 2026, against HEAD `1565b9d` (the hook-transient fall-through of the 20th). This document is an assessment only: no code, configuration or threshold was changed to produce it. Every proposal below is opt-in, offline first, and leaves the deterministic grader in charge of every grade, wire and outcome the program reports.

**What TypeSafe is, in one paragraph.** TypeSafe's System One model (Jev) does not write text. It takes a piece of *state* (a string or a JSON object) and a set of typed *questions*, and answers each one with a typed value plus calibrated probabilities: a `Choice` picks one option from a closed list and returns the probability of every option; a `Score` places the input on an ordered ladder of described levels; a `Noul` returns the probability that a yes/no statement is true. Several questions over the same state are answered in one request, independently and in parallel. The two documented limits that shaped this list: Jev is not a calculator (it does not count, compare numbers or dates, or interpolate reliably), and it only answers the literal question asked over the state it is given. So the model belongs where the program needs *reading* and *judgment*, never where it needs arithmetic or a reproducible number.

Part 1 is written for everyone. Part 2 has the code references, the question designs and the evidence.

---

## Part 1. In plain language

### What was looked at

Both repositories: the Rust client in this repository (`src/`, about 25 000 lines, the grader, the wire estimate, the outcome logic, the Discord embed) and the `sevenfifty777/rust-server` fork (the DCS-gRPC server, its recovery telemetry Lua and the protos). The fork turned out to be a pure relay: it parses nothing and decides nothing, it moves raw observations and raw DCS event strings to the client. Everything below is about the client.

Two questions were asked of every file: where does the program read free text with hand-written rules, and where does it encode a *judgment* (a verdict a person would give) as a ladder of thresholds because no person was available to give it.

### The short answer

**There is almost no free text in this program.** Telemetry, hook animation, wind and events are all typed. The one free-text input is the comment DCS's own LSO writes on a landing quality mark, such as `LSO: GRADE:C : _SLOX_ _TMRDAR_ (LURIM) WIRE# 2 _EGIW_ [BC]`. That string is read by four separate hand-written parsers, and the one that produces the "LSO Notes" line in Discord gets most real comments wrong (next section). This is the clearest case for replacing a parser, and it is also the only one.

**The larger opportunity is the missing human LSO.** The grader's own comments say it: every threshold, weight and correction rule "requires comparison with human-LSO assessments", and the convention table that decides `OK` / `(OK)` / `--` was adopted on 15 September "against the only reference that exists in writing" because no LSO was available to judge. A calibrated judgment model can be that second reader: shown the same facts a reviewer sees, in words, it returns a verdict *and how sure it is*. Where it agrees with the grader, nothing changes. Where it disagrees or hesitates, that pass goes on the review list. This runs offline, on the recorded reports, and never touches the live grade.

### What the recorded DCS comments revealed

One hundred and sixty distinct DCS comments were collected from the 165 recorded reports that carry one (under `tools/aoa_calibration/` and `trap_records/`) and from the `dcs.log` files, and the current translator (`src/lso_notation.rs`, compiled on its own and run over them, nothing in the repository changed) was compared with the LSO shorthand glossary in the NATOPS manual kept in `docs/NATOPS/` (NAVAIR 00-80T-104, sections 11.4.1 to 11.4.3). The translator was written from the examples in its own header comment, which do not resemble what DCS actually sends. Four problems, each visible today in the Discord "LSO Notes" field:

| What DCS writes | What Discord shows today | What the shorthand means |
|---|---|---|
| `_TMRDAR_` | "slightly turning, slightly drift" | too much rate of descent at the ramp, underlined |
| `_LOAR_` | "slightly low" (position lost) | low at the ramp, underlined |
| `WO(AFU)IC` | "wings not level, all fouled up, lineup" | waveoff, all fouled up, in close |
| `WOFDIC` | "wings not level, fast, drift, lineup" | waveoff, fouled deck, in close |
| `3PTSIW` | "power, turning, slow, lineup, wings not level" | landed three points, in the wires |
| `(LLIW)` | "a little low, a little low, a little lineup, a little wings not level" | landed left, in the wires, a little |
| `_EGIW_` / `(EGIW)` | "energy (AoA), glide slope, lineup, wings not level" | eased gun, in the wires |
| `(NX)` | nothing | nose (attitude) at the start, a little |
| `LSO: GRADE:WO  WONSUX [BC]` | "Low, slow, glide slope, drift, energy (AoA), wings not level, wings not level, not set up" | waveoff; not set up at the start |

1. **The magnitude is read backwards.** DCS renders the NATOPS underline as underscores: `_LOAR_`. The manual says the underline is "for emphasis", and the project's own grading convention (`docs/GRADING_CONVENTION_PROTOTYPE_2026-09-15.md`, section 1) reads it as the *gross* deviation, with parentheses for "a little" and plain text for moderate. The translator renders every underlined symbol as "slightly". A pilot reading "slightly low" in Discord was in fact grossly low.
2. **The position is lost on every underlined symbol.** The translator expects `_LO_AR`; DCS writes `_LOAR_`, with the underline around the position too. The suffix is then treated as unknown letters and dropped.
3. **The vocabulary is a fraction of the glossary, and the single letters are wrong.** The table knows 25 symbols; the manual lists about seventy. `TMRD`, `DR`, `DL`, `NEP`, `NERD`, `WU`, `LNF`, `3PTS`, `FD`, `EG` are missing, so their letters are consumed one at a time by the single-letter entries: `S` is rendered "slow" (NATOPS: settle; slow is `SLO`), `T` "turning" (not a symbol), `D` "drift" (NATOPS: `DR`/`DL`), and `E`, `G`, `I`, `W` as four separate T-45 axes (in every recorded comment they are `EG` + `IW`, eased gun in the wires). Unknown letters are skipped silently, which is why nothing warns.
4. **Waveoff comments are not stripped of their prefix.** DCS separates `GRADE:C` and `GRADE:---` from the symbols with ` : `, but writes `GRADE:WO` followed by two spaces. The translator looks for ` : `, finds none, and tokenises `LSO:` and `GRADE:WO` as deviations. Every waveoff in the corpus therefore starts its notes with "Low, slow, glide slope, drift, energy (AoA), wings not level".

The unit tests pass because they test the header's invented examples, including one that asserts the wrong reading of `(EGIW)`. None of this is a TypeSafe finding as such; it is what surfaced when the real strings were put next to the parser, and it is the reason the first recommendation has a deterministic step zero.

### The opportunities, ranked

| # | Opportunity | Where it lives | What it replaces or adds | Runs | Value | Risk |
|---|---|---|---|---|---|---|
| 1 | DCS LSO comment read as typed structure (symbol, magnitude, position, grade, wire) | `src/lso_notation.rs`, `src/track.rs` `parse_dcs_wire`, `src/grading.rs` `dcs_waveoff_initiator`, `src/commands/grade_ab.rs`, `tools/aoa_calibration/align_aoa.py` | Replaces the four parsers; yields DCS's own (deviation, size, zone) list in the same shape as the grader's episodes | Live, **no AI needed**: a glossary parser reads all 160 recorded comments with no unknown or ambiguous token (`docs/LSO_NOTATION_PARSER_PROTOTYPE_2026-09-21.md`) | High as a bug fix and as the DCS-vs-project comparison; nil as an AI use | None |
| 2 | Second-opinion LSO on correction quality and the convention table | `src/grading.rs` `build_axis_episodes` (the eight-branch correction ladder), `convention_effective_severity` | Adds a "JEV" column to `grade-ab` with a verdict and confidence per episode and per pass; disagreements and hesitations feed the next review | Offline, batch | High: the comparison the grader has been waiting for since 13 September | Low: opinion only, grade unchanged |
| 3 | Pass triage for the review documents | The hand-written `docs/RECOVERY_REVIEW_*.md` tables | Adds a ranked "look at these first" table from the JSON reports | Offline, batch | Medium: saves the first hour of every review | Low |
| 4 | Aircraft and carrier type name to profile | `src/data.rs` (two tables), `src/commands/file.rs`, `tools/aoa_calibration/LsoAoaExport.lua` | Replaces silent drops of unknown DCS names with a proposed mapping, reviewed by a person before use | Once per new name, cached | Medium: new module variants stop needing a release | Medium if unreviewed (wrong AoA band); none when cached and approved |
| 5 | Wire adjudication when two methods disagree | `src/track.rs` `wire_estimate_with` fall-through chain | Adds a labelled opinion (wire, hook-skip likelihood, confidence) when the stop position, the deceleration onset and the hook transient disagree | Offline first | Low to medium: one such trap so far (09:04 on the 20th) | Low: diagnostic only |
| 6 | Terminal outcome when the evidence is thin | `src/track.rs` bolter / waveoff / touch-and-go branch, `ApproachOnly`, `PatternWaveoff` | Adds a calibrated "how sure" on `WO?`, `Bolter`, `T&G` for the review | Offline | Low: the code's evidence ladder is already explicit | Low |

### Where not to use it

Not on any number the grader computes or compares: the glideslope, lineup and AoA thresholds, zone weights and correction deadlines, the episode segmentation, the groove-entry state machine, the wire crossings and the deceleration onset, gate validation, the Cut rules, the near-deck geometry, the detection envelope. Three reasons. Jev's documented weaknesses are exactly counting, comparing and interpolating numbers. These paths must stay bit-for-bit reproducible so that `grade-ab`, `cadence-ab` and `groove-ab` can replay a corpus. And the project already has the right tool for tunables on its roadmap: the `lso.toml` externalisation (`tasking-roadmap.md`, "Externaliser les seuils PROJECT-DERIVED"). Where a threshold is wrong, the fix is a measured threshold, not a model.

The same rule in the other direction: the model is never given raw series or numbers to reason over. Code computes the facts ("came back inside the band 1.2 s after the peak, the deadline in close is 1.5 s"), turns them into words ("came back inside the band before the zone's deadline"), and asks the judgment on the words.

### Recommendation

1. **Step zero, deterministic, this week:** fix the translator against the NATOPS glossary (full symbol table, underline as emphasis, suffixes `X`, `IM`, `IC`, `AR`, `TL`, `IW`, `AW`, `BC`, prefix stripping for `GRADE:WO` and `GRADE:OWO`), and add the 160 recorded comments to its tests. This needs no AI and removes the visible Discord errors. Prototyped and run over the corpus in `docs/LSO_NOTATION_PARSER_PROTOTYPE_2026-09-21.md`; section 2 below stands as the design of the parser, with the Jev questions in 2.3 reduced to an optional guard that the corpus never triggered.
2. **Then #1 and #2 as Python tools** under `tools/`, reading the JSON reports already on disk, writing their answers next to them. Compare with the corrected tokenizer (for #1) and with the `CONVENTION` column of `grade-ab` (for #2) over every recorded pass that has a report. Every disagreement is read by a person.
3. **Only then wire #1 into the binary**, behind a flag, with a short timeout, the tokenizer as fallback, and the answer and its probabilities written into the report so nothing downstream ever re-asks.
4. #3 follows from #2 for free (same tool, one more table). #4, #5 and #6 wait for a need.

---

## Part 2. Technical

### 1. The TypeSafe contract used below

- Endpoint `POST https://api.typesafe.ai/v1/systemone`, bearer key, body `{state, model: "jev-latest", questions}`. Python SDK `typesafe-sdk` (`TypeSafeClient().system_one(state=..., questions=...)`), key from `TYPESAFE_API_KEY`. No Rust SDK; from Rust it is one HTTPS call with `reqwest`, which the crate does not yet depend on (`Cargo.toml` has `tonic` and `serenity` only).
- `Choice`: up to 255 options, returns `choice`, `probabilities` per option, `confidence`. For three options the docs give confidence as `(3 × p_max − 1) / 2`, so confidence 0.5 means the winning option has about two thirds of the probability and 0.9 means about 93 %.
- `Score`: 2 to 10 described levels, returns a probability-weighted position, the per-level probabilities and a confidence.
- `Noul`: returns P(yes). A value near 0.5 means the model is split, not "medium".
- Questions in one request are answered independently; one answer never becomes context for another. Speculative questions are cheap to add and code reads only the ones that apply.
- Jev 1.13 known limits (docs, "jaggedness"): no reliable counting or arithmetic, dates and numbers read as text, worse on double negatives and multi-hop questions, large irrelevant state degrades accuracy, sensitive to injected instructions. Every design below pre-computes in code and sends words.

### 2. Opportunity 1: the DCS LSO comment as typed structure

#### 2.1 Current code

| Parser | Location | What it does | Fragility |
|---|---|---|---|
| `to_english` | `src/lso_notation.rs:69-98`, tokens `:104-159`, `greedy_decode` `:163-198` | Strips the `GRADE:x :` prefix, drops everything after `WIRE#`, splits on whitespace, greedy longest-first over a 25-entry table, skips unrecognised characters one at a time (`:176-181`) | Wrong magnitude, lost positions, wrong single letters, waveoff prefix not stripped (Part 1). Also drops `_EGIW_ [BC]` because it follows `WIRE#`. `("LULR", "lined up left")` and `("LURC", "lined up right")` at `:28-29` look transposed. |
| `parse_dcs_wire` | `src/track.rs:5199-5219` | `split_once("WIRE#")`, digit run, guard on the next character, range 1 to 4 | Correct on the corpus; duplicated logic |
| `dcs_waveoff_initiator` | `src/grading.rs:571-581` | `split_once("GRADE:")`, `starts_with("OWO")` before `starts_with("WO")` | Order-sensitive; `WOP` (waveoff pattern, NATOPS) would read as `WO` |
| grade label for A/B tables | `src/commands/grade_ab.rs:495-500` | first whitespace token after `GRADE:` | Third copy |
| grade label in the AoA tool | `tools/aoa_calibration/align_aoa.py:424` | `split("GRADE:")[1].split("[")[0]` | Fourth copy, different rule |

Consumers: the Discord embed (`src/tasks/record_recovery.rs:2039-2044`) and the greenie board (`src/db.rs:344`, translated at read time).

#### 2.2 What the corpus looks like

Structure actually observed (160 distinct comments): `LSO: GRADE:<label>` then either ` : ` (for `C`, `---`, `OK`, `(OK)`, `_OK_`, `NC`) or two spaces (for `WO` and `OWO`; none of the 38 waveoff comments uses ` : `); then symbols separated by two spaces; then `WIRE# n` followed by symbols in the wires (`_EGIW_`, `(EGTL)`, `3PTSIW`) and `[BC]`; or, for a waveoff, the waveoff symbol with its reason and position (`WO(AFU)IC`, `WOFDIC`, `WONSUX`, `WO(AFU)TL`) and `[BC]`.

| Label | `C` | `---` | `WO` | `_OK_` | `(OK)` | `OK` | `OWO` | `NC` |
|---|---|---|---|---|---|---|---|---|
| Comments | 63 | 48 | 36 | 4 | 4 | 2 | 2 | 1 |

Symbols seen: `SLO`, `LO`, `H`, `F`, `LUL`, `LUR`, `DR`, `DL`, `TMRD`, `P`, `PP`, `PPP`, `N`, `W`, `EG`, `LL`, `LR`, `3PTS`, `LNF`, `NSU`, `AFU`, `FD`. Suffixes seen: `X`, `IM`, `IC`, `AR`, `TL`, `IW`, `BC`. Modifiers: `(...)` a little, plain, `_..._` emphasis; the underline wraps symbol and suffix together. The most frequent tokens are `_EGIW_` (97 comments), `_SLOX_` (78), `_LULX_` (52), `_LOAR_` (51), `_LOIC_` (48): every one of them is mistranslated today.

Glossary reference: NAVAIR 00-80T-104 section 11.4.1 (general symbols and modifiers: `( )` "a little", underline "for emphasis", `_OK_` perfect pass, `WO`, `WOP`, `OWO`, `B`, `NC`, `--`, `C`), 11.4.2 (descriptive symbols, about seventy), 11.4.3 (suffixes `CCA`, `OT`, `BC`, `X`, `IM`, `IC`, `AR`, `TL`, `IW`, `AW`). Extracted with `pdftotext` from `docs/NATOPS/LSO-NATOPS-MAY09.pdf`.

#### 2.3 Design: pre-parsed candidates, judged selection

This is the docs' "pre-parsed value extraction" pattern: code finds every candidate reading, the model picks one, code copies it verbatim. The model cannot invent a symbol that the glossary does not contain.

**Code (deterministic, no model):**
1. Split the header: label after `GRADE:` up to ` : ` or double space; everything else is the body.
2. Split the body on runs of two or more spaces (DCS's separator; single spaces occur inside `_PP _PIC_`, which the corpus shows once and which a person reads as one broken token).
3. For each token, strip and record the modifier (`(...)`, `_..._`, plain, or a `WO` prefix with an optional parenthesised reason), then enumerate every segmentation of the remaining letters into glossary symbols plus an optional suffix. `SLOX` has two: `SLO`+`X` and `S`+`LO`+`X`. `LLIW` has `LL`+`IW` and `L`+`L`+`IW` only if `L` is a symbol (it is not: `LO` is). `TMRDAR` has exactly one. Most tokens have one candidate and never reach the model.
4. Find the digit runs after `WIRE#`.

**One request per comment** (state: the full comment, the token list, the glossary as `[{symbol, meaning}]`, and three sentences of notation rules), with these questions:

| Question id | Type | Instructions | Criteria |
|---|---|---|---|
| `grade_label` | Choice | Which grade does the comment's `GRADE:` field carry? | `OK`, `_OK_ perfect`, `(OK)`, `--`, `C`, `WO`, `WOP`, `OWO`, `B`, `NC`, `absent` |
| `wire` | Choice | Which of these digit spans is the wire number named after `WIRE#`? | the spans code found, plus `none` |
| `tokens[i].reading` (only for tokens with more than one segmentation) | Choice | Which reading did the LSO intend for token `tokens[i].text`, given the whole comment? | the candidate segmentations, each spelled out ("slow, at the start" / "settle, low, at the start"), plus `none of these` |
| `tokens[i].magnitude` | Score | How strongly is this deviation marked? | `a little (parentheses)`, `moderate (plain)`, `gross (underlined)` |
| `tokens[i].position` | Choice | Where in the approach does the suffix place it? | `X start`, `IM middle`, `IC in close`, `AR at the ramp`, `TL to land`, `IW in the wires`, `AW all the way`, `BC ball call`, `none` |
| `unknown_vocabulary` | Noul | Does the comment contain a symbol that is not in the glossary? | yes/no |
| `waveoff_reason` (speculative, read only when the label is `WO`) | Choice | What reason does the waveoff symbol give? | `AFU`, `FD fouled deck`, `NSU not set up`, `none stated` |

Magnitude and position are asked even though code already knows them from the modifier and the suffix: the two answers must agree with the deterministic read, and a disagreement flags the token. Cheap insurance, and it catches the day DCS changes its rendering.

**Output** is a list of `(symbol, magnitude, position)` triples plus label and wire. Rendered to English by a table lookup (the meaning column of the glossary), never by the model.

#### 2.4 Why this matters beyond the notes line

The grader already produces, per pass, a list of episodes with `axis`, `maximum_severity` (`Small`/`Medium`/`Large`) and `peak_zone` (`Start`/`Middle`/`InClose`/`Ramp`). The parsed DCS comment is the same shape: `LO` is the glideslope axis, `LUL`/`LUR`/`DR`/`DL` the lineup axis, `SLO`/`F` the AoA axis; parentheses, plain and underline map to the same three sizes; `X`, `IM`, `IC`, `AR` to the same four zones. So `grade-ab` can print, per pass and per axis, whether DCS's LSO and the project saw the same thing in the same third of the groove. That is a machine-readable version of the comparison every review document does by hand, and it is available on every arrested pass DCS comments on.

#### 2.5 Guardrails

- The corrected deterministic tokenizer (step zero) is the fallback and the reference. The model is consulted only for tokens with more than one candidate reading, plus the per-comment checks.
- A `tokens[i].reading` with confidence below 0.5 (the winning reading holding less than about two thirds of the probability) is rendered verbatim, in monospace, rather than translated, and the report records it.
- `unknown_vocabulary` above 0.5, or a magnitude/position disagreement with the deterministic read, produces a `tracing::warn!` and a report flag. Nothing is dropped silently any more.
- The report (schema v3) gains `dcs_grading_parsed` with `source` (`"tokenizer"` or `"jev"`), `model`, a hash of the question set, the triples, and the probabilities. `src/db.rs:344` and `grade-ab` read that field instead of re-parsing.
- Live path: the Discord publish waits at most a short timeout for the request, then falls back. The translation is not on the grading path.
- Secrets: `TYPESAFE_API_KEY` from the environment, same handling as `DCS_GRPC_API_KEY` (`src/commands/run.rs:126-127`, `:356-374`); never in the report or the log.

#### 2.6 Evaluation

Corpus: the 160 distinct comments (`grep dcs_grading` over the JSON reports under `tools/aoa_calibration/` and `trap_records/`, `grep GRADE:` over the `dcs.log` files) plus the nine strings in `src/lso_notation.rs` tests, re-labelled against the glossary. Measure: exact-match rate of the triple list against a hand-labelled answer key; number of tokens sent to the model (expected: a small minority); agreement of the model's magnitude/position with the deterministic read (expected: near total, anything else is a bug in one or the other). Read every disagreement.

### 3. Opportunity 2: a second-opinion LSO on correction quality and the pass grade

#### 3.1 Current code

- `build_axis_episodes`, `src/grading.rs:1240-1490`: segments each axis into episodes, then assigns `CorrectionQuality` through an eight-branch ladder (`:1395-1440`) whose reasons are `repeated_significant_inversions`, `aggravation_after_improvement`, `trajectory_ended_before_correction_could_be_assessed` (two branches), `no_real_post_peak_improvement`, `ramp_correction_not_stabilized_before_trajectory_end`, `post_peak_improvement_within_zone_deadline_and_stabilized`, `real_post_peak_improvement_late_incomplete_or_insufficiently_stabilized`.
- `convention_effective_severity`, `src/grading.rs:378-428`: the eight-row table (size × correction × early/late) adopted on 15 September because "no human LSO is available".
- `CatobarGradingPolicy`, `src/grading.rs:255-354`: nine switches, each justified in its doc comment by named passes ("five passes were decided that way by 0.1 to 0.9 s of gross readings").
- The header at `:99-101` states the gap: every rule "requires comparison with human-LSO assessments before it can be treated as operationally calibrated".

The ladder is not wrong; it is a sequence of reasonable calls written by a developer in the absence of the person whose calls they are. What is missing is a reader who applies the written convention to each episode independently of the ladder, so the two can be compared.

#### 3.2 Design: verbal state, atomic judgments, code owns the grade

A Python tool (`tools/typesafe/second_opinion.py`, proposed name) reads schema-v3 reports. For each graded episode it builds a **verbal** state from fields the report already carries (`axis`, `most_severe_zone`, `maximum_severity`, `duration_s`, `first_durable_improvement_delay_s`, `return_to_none_delay_s`, `oscillation_reversals`, `stabilization_samples`, `post_correction_aggravation`, `correction_reason`, whether the episode ends at the series end). Every number is turned into a sentence in code before it is sent, for example:

```json
{
  "convention": "<the eight-row table and the good/average/poor definitions, quoted from docs/GRADING_REFERENCE.md>",
  "episode": {
    "axis": "glideslope",
    "zone_of_the_peak": "in close (last third of the groove)",
    "size_at_the_peak": "moderate (between one and two and a half degrees high)",
    "how_it_evolved": "started coming back down within the zone's correction deadline, was back inside the band before the ramp, and stayed there",
    "reversals": "no oscillation",
    "ended_because": "the pilot corrected it, not because the aircraft touched down",
    "duration": "about three seconds"
  },
  "what_the_grader_concluded": "not shown"
}
```

Questions per episode, one request:

| Question id | Type | Instructions | Criteria |
|---|---|---|---|
| `correction_quality` | Score | By the written convention, how well was this deviation corrected? | `poor: no real return toward the target, or repeated reversals, or it got worse again`, `average: a real correction that was late, incomplete or not held`, `good: came back inside the band promptly for the zone and stayed there` |
| `observable` | Noul | Could the correction be judged at all, or did the trajectory end (touchdown) before it could be seen? | yes = it could be judged |
| `cell` | Choice | Which grade band does the convention table give this episode on its own? | `OK`, `(OK)`, `--` |

Per pass, a second request with the list of episodes summarised in words and the grader's own worst-episode rule stated as a rule: `pass_grade` Choice over `OK`, `(OK)`, `--`; `deciding_axis` Choice over the axes present plus `no single deciding episode`.

The grader's verdict is deliberately not in the state, so the answer is independent.

#### 3.3 How it is used

A new column in the `grade-ab` table: the model's grade, its confidence, and a marker when it differs from `CONVENTION`. Passes go on the next review's list when the model disagrees, or when its `pass_grade` confidence is below 0.6 (the winning grade holds under about three quarters of the probability), or when `observable` sits between 0.35 and 0.65 (the model cannot tell whether touchdown ended the assessment, which is precisely the `touchdown_ends_correction_assessment` switch's territory).

Nothing about the grade changes. The tool's output is an opinion column, labelled as such, with probabilities, on the same footing as the "LSO Notes (measured by LSO, not a DCS comment)" label the embed already uses.

#### 3.4 Evaluation

Run over the 44 passes of the 15 September comparison (22 of the 14th, 8 of the 13th, 14 replay fixtures), the 23 of the 15th, the four of the 18th and the nine of the 20th, then over the wider report corpus. Count agreement with `CONVENTION` per episode and per pass. Then take the passes the review documents already argued about by hand (the 19:25 pass of 14 September flown on the fast chevron; the five passes of 15 September decided by under a second of gross AoA; the 20:30 pattern abandonment of 18 September) and check whether the model's hesitation lands on them. If it does, the column earns its place. If it agrees with everything, it has added nothing and should be dropped rather than kept for show.

Store each answer with the report's identity, the question-set hash and the model name, so a later run can be diffed.

### 4. Opportunity 3: pass triage for the review documents

The three review documents of the 15th, 18th and 20th each open with a table of every pass (UTC, pilot, type, outcome on the board, DCS LSO, our grade) and then a numbered list of what was unusual: a straight-in with no groove entry, a wire named by the least trusted method, a DCS comment absent, the wind probe on its sentinel, a hook-up touch-and-go whose hypothetical wire depends on a DCS event. Assembling that is the first hour of every review.

Same tool as section 3, one more request per report. Code writes a paragraph of facts from the report (outcome, `assessment_scope`, DCS mark present or absent and its label, wire estimate `reason` and `confidence` string, whether the stop position and the crossings agree, groove entry found or not, gate coverage source, wind sentinel hit, hook state), and asks:

| Question id | Type | Instructions |
|---|---|---|
| `dcs_and_project_disagree` | Noul | Do the DCS LSO's grade and the project's grade tell a different story about this pass? |
| `wire_from_weak_evidence` | Noul | Was the reported wire named by a method the project trusts least, or by methods that disagree? |
| `approach_type_unusual` | Choice | `ordinary Case I pattern`, `straight-in`, `pattern abandoned before the groove`, `cannot tell` |
| `measurement_problem` | Noul | Is there a sign of a measurement problem (sentinel wind, missing gates, telemetry gap) rather than a piloting one? |
| `review_priority` | Score | `routine`, `worth a look`, `must be read` |

Output: the Part 1 table of the next review, pre-filled and sorted by `review_priority`, with the Nouls as columns. The person still writes the findings; the tool only says where to look first.

### 5. Opportunity 4: aircraft and carrier type name to profile

#### 5.1 Current code

- `AirplaneInfo::by_type`, `src/data.rs:636-646`: exact match on `"FA-18C_hornet"`, `"F-14A-135-GR" | "F-14A-135-GR-Early" | "F-14A-95-GR"`, `"F-14B" | "F-14A/B"`, `"F-14B(U)" | "F-14BU"`, `"T-45"`, `"AV8BNA"`.
- `get_aircraft_id`, `src/data.rs:649-659`: a second table over the same names plus `"A-6E" => Some(5)`, which has no profile.
- `CarrierInfo::by_type`, `src/data.rs:560-569`: `"CVN_71" | "CVN_72" | "CVN_73" | "CVN_75" | "Stennis"` (CVN-74 is literally named `Stennis` in DCS), `"Forrestal"`, `"LHA_Tarawa"`.
- ACMI path, `src/commands/file.rs:152-197`: unmatched names are dropped at `trace` level (`:173`, `:195`), so a replay of a recording with a new variant name produces nothing and says nothing.
- A third table in `tools/aoa_calibration/LsoAoaExport.lua:37-45`.

The `F-14A-135-GR-Early` entry is the history of this problem: a new variant string appears, a release is needed.

#### 5.2 Design: judged once, cached, human-approved

When a name is not in the table, one request with state `{dcs_type_name, display_name, category, known_profiles: [{id, description}]}` where each description says what the profile physically is, in words a pilot would use ("F-14B Tomcat: swing-wing, two seats, twin engines; the F-14A and F-14B(U) have their own profiles") and:

- `profile`: Choice over the six profile ids plus `unsupported`.
- `is_naval_fixed_wing`: Noul.

The answer is written to a JSON map next to the binary (`aircraft-aliases.json`, same mechanism as `--discord-users`), **with `approved: false`**. The binary only ever uses approved entries; an unapproved proposal produces a `warn!` at unit discovery naming the proposed profile and the confidence. A person flips `approved`. Confidence below 0.9 (the model not clearly preferring one profile) is logged as "needs a decision" and no proposal is written.

A wrong profile means a wrong AoA band and a wrong hook offset, so this is the one place where the model's answer is never used without a human in the loop, whatever its confidence.

### 6. Opportunity 5: wire adjudication when the signatures disagree

#### 6.1 Current code

`wire_estimate_with`, `src/track.rs:4008-4046`: hook transient with a named wire first; else the stop position on a confirmed arrestment; else the deceleration-onset-anchored crossing; else the last crossing (`wire_estimate_from_stop_or_crossings`, `:4050-4170`). Confidence is a string, `"high"` / `"medium"` / `"insufficient"` (`:4142-4147`). The fall-through of the 20th exists because a non-answer from the first method used to be treated as an answer.

The chain is a priority order, and a priority order cannot express "two methods disagree and here is why one is more believable". The 09:04 trap of the 20th is the case: stop position says 2 (1.9 m residual), deceleration onset says 1 (53 m past the wire, the distance seen on every 1-wire trap), the hook struck the deck 22 m short and stayed deflected over wires 1 and 2. The review called it "genuinely ambiguous" and "reached by accident".

#### 6.2 Design: diagnostic only

Only when the report carries two or more methods naming different wires. Code writes each method's claim as a sentence with its own validity check already applied ("the stop position names wire 2; the residual is inside the 6 m tolerance"; "the deceleration onset was 53 m past wire 1, which is what every recorded 1-wire trap shows"; "the hook animation shows the hook striking the deck short of the wires and staying deflected over wires 1 and 2"; "DCS did not name a wire"). Questions:

- `wire`: Choice over `1`, `2`, `3`, `4`, `cannot be resolved from this evidence`.
- `hook_skip`: Noul, "Is this the signature of a hook skip (hook bouncing over one wire and catching the next)?"
- `methods_reconcilable`: Noul.

Written to the report as `wire_ai_opinion` with probabilities. The reported wire stays whatever the deterministic chain says, including "unavailable". Value is the calibrated hedge on the review's ambiguous cases, and a running count of how often the model's opinion later matched DCS when DCS did name the wire.

### 7. Opportunity 6: terminal outcome when the evidence is thin

`src/track.rs:2336-2379` decides `TouchAndGo`, `Bolter` or `WaveoffUnknown` from confirmed contact, hook state and the deck crossing; `Grading::ApproachOnly` (`:1726`) and `PassGrade::PatternWaveoff` (`src/grading.rs:809-812`) cover the approach that was never established; `dcs_waveoff_initiator` upgrades `WO?` to `WO` or `OWO` when DCS says so. The doc comments record two live misclassifications that motivated the current shape (a 28 ft fly-over and a `GRADE:WO` waveoff both read as bolters).

The evidence ladder is explicit and reasonable. A judgment adds only a calibrated "how sure" for the review: state is the same verbal evidence (contact confirmed or not, hook up or down, minimum height over the deck in words, DCS mark present, groove entry found), questions `outcome` Choice over `arrested`, `bolter`, `touch-and-go`, `LSO waveoff`, `own waveoff`, `pattern waveoff`, `cannot tell`, plus one Noul per outcome for the cases where two apply. Lowest priority; listed to mark the boundary between "the code already knows" and "the code is guessing".

### 8. Not recommended, and why

| Area | Location | Why not |
|---|---|---|
| Deviation thresholds and cut rules | `src/grading.rs:12-102`, `:451-467` | Numbers; must stay reproducible; belong in `lso.toml` |
| Zone weights and correction deadlines | `src/grading.rs:150-177` | Same |
| Episode segmentation, peak, trend, reversals | `src/grading.rs:1250-1361` | Time-series arithmetic, Jev's weakest area |
| Groove-entry detector | `src/track.rs:152-211`, `CaseIGrooveDetector` `:692`, `observe` `:1542-1635` | Geometry and rates; the straight-in problem is a Case II/III design task, deferred by decision |
| Wire crossings, deceleration onset, hook-transient band | `src/track.rs:213-344`, `:3561` | Timing tolerances measured live; a model cannot measure |
| Near-touchdown geometry | `src/track.rs:102-141`, `src/grading.rs:1164-1232` | Arithmetic |
| Gate validation, coverage recovery | `src/track.rs:1117` and the 300 ms bracket rules | Contract; `AGENTS.md` truth rules forbid loosening |
| Detection envelope | `src/tasks/detect_recovery_attempt.rs:72-110` | Three numbers, runs on every sample of every unit |
| Carrier position smoothing, wind correction | `src/track.rs:92`, `:823-851` | Signal processing |

### 9. Integration

- **Phase 1 (offline).** `tools/typesafe/` in Python (`pip install typesafe-sdk`; `TYPESAFE_API_KEY` in the environment). Inputs: the JSON reports on disk. Outputs: a sidecar JSON per report and a Markdown table. Nothing in `src/` changes. Sections 2 (comparison against the corrected tokenizer), 3, 4 live here.
- **Phase 2 (live, section 2 only).** `reqwest` with rustls behind `--typesafe` on `lso run`, a timeout of a few hundred milliseconds, tokenizer fallback, the parsed structure and probabilities serialised into the report (`dcs_grading_parsed`), and the embed reading from that field. The schema bump is the only cross-cutting change; `grade-ab` and `db.rs` gain a reader for the field and keep the tokenizer path for old reports.
- **Provenance.** Every AI-derived field carries `source`, `model`, `questions_sha256` and the raw probabilities. The Discord embed labels any AI-derived line the way it already labels the measured notes.
- **Cost and latency.** Not stated in the docs; measure on the 160-comment, 165-report corpus before deciding anything. Section 2 is one request per arrested pass; sections 3 and 4 are batch. Batching many questions in one request is the documented way to keep cost down.
- **Truth rules.** Nothing here announces a live validation, moves logic into Lua, fabricates a trajectory or loosens a bracket. The model reads words the program wrote from measurements it already made.

### Appendix A. Every parsing site found, and which section covers it

| Site | Section |
|---|---|
| `src/lso_notation.rs` (whole file) | 2 |
| `src/track.rs:5195-5219` `dcs_grade_is_waveoff`, `parse_dcs_wire` | 2 |
| `src/grading.rs:571-594` `dcs_waveoff_initiator`, `apply_dcs_waveoff_initiator` | 2 |
| `src/commands/grade_ab.rs:495-500` grade label | 2 |
| `tools/aoa_calibration/align_aoa.py:424` grade label | 2 |
| `src/data.rs:560-569`, `:636-646`, `:649-659` type tables | 5 |
| `src/commands/file.rs:126-136` (pilot name, default `"KI"`), `:152-197` (ACMI names), `:390-391` (hook property) | 5 (names); the rest is fine as code |
| `tools/aoa_calibration/LsoAoaExport.lua:37-45` | 5 |
| `src/commands/run.rs:854`, `:1198` version strings; `:325` BOM strip | code, no change |
| `run-live-buffered.ps1:28` token regex | code, no change |
| `src/tasks/record_recovery.rs:1836`, `:1847` `format!("{:?}")` as serialisation | code; worth a `Display` impl, not AI |

### Appendix B. Incidental findings (not AI-related, one line each, no fix proposed here)

- `src/lso_notation.rs`: magnitude inverted, positions lost on underlined tokens, waveoff prefix not stripped, single-letter entries contradict the glossary, `(EGIW)` test asserted the wrong reading. Section 2.2 of Part 1. **Fixed on this branch the same day** by the glossary parser (`docs/LSO_NOTATION_PARSER_PROTOTYPE_2026-09-21.md`); the line references in this document to that file describe the previous version.
- `src/grading.rs:1370-1371`: the `EpisodeEvolution::Stagnant` branch is unreachable (`!improved` then `improved` cover every case).
- `src/grading.rs:1836-1881`: `episode_reason` and the "OK: trajectoire stable" string are French in an otherwise English embed and report.
- `src/data.rs:655`: `"A-6E" => Some(5)` has no `AirplaneInfo`.
- `src/lso_notation.rs:28-29`: `LULR` and `LURC` look transposed.
- `src/grading.rs:571-581`: `starts_with("WO")` also matches `WOP` (waveoff pattern, NATOPS 11.4.1), which would be reported as an LSO waveoff rather than a pattern one; not seen in the corpus yet.

### Appendix C. TypeSafe pages read for this document

- Building guide: https://docs.typesafe.ai/concepts/how-to-build-with-system-one.md
- Primitives: https://docs.typesafe.ai/primitives.md (Choice, Score, Noul pages under it)
- Confidence: https://docs.typesafe.ai/confidence.md
- State: https://docs.typesafe.ai/concepts/state.md
- Pre-parsed value extraction cookbook: https://docs.typesafe.ai/cookbooks/pre_parsed_value_extraction_cookbook.md
- Composite scoring pattern: https://docs.typesafe.ai/patterns/composite-scoring.md
- Jev 1.13 known limits: https://docs.typesafe.ai/model-jaggedness/jev-1.13.md
- Python SDK: https://docs.typesafe.ai/sdk/python.md
- HTTP API: https://docs.typesafe.ai/api.md
- The full index is mirrored at `docs/jev/llms.txt`.
