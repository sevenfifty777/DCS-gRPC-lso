# LSO shorthand glossary

The symbols the DCS LSO writes in a landing-quality-mark comment, and the ones the project writes for its own graded episodes. Source: NAVAIR 00-80T-104 (the LSO NATOPS manual, `docs/NATOPS/LSO-NATOPS-MAY09.pdf`), chapter 11.4: general symbols (11.4.1), descriptive symbols (11.4.2), suffixes (11.4.3). This file is the human-readable copy of the tables in `src/lso_notation.rs`; the two must be kept in step.

Counts below are over the 160 distinct DCS comments recorded up to 20 September 2026 (`tests/fixtures/dcs_lso_comments.txt`); "comments" is the number of comments containing the symbol at least once.

## 1. How DCS writes a comment

```
LSO: GRADE:C : _SLOX_  _TMRDAR_  (LURIM)  _DRIM_  WIRE# 2 _EGIW_ [BC]
LSO: GRADE:WO  _DRX_  _LURX_  LOIM  _LOIC_  WOFDIC [BC]
LSO: GRADE:OWO : _LULIM_  LOIM  LOIC  _DRIC_  (LURIC)  WO(AFU)IC [BC]
LSO: GRADE:_OK_ : WIRE# 3
LSO: GRADE: NC : No proper communications
```

- `GRADE:` then the label. `C`, `---`, `OK`, `(OK)`, `_OK_`, `OWO` and `NC` are followed by ` : `; `WO` by spaces only.
- Symbols are separated by two spaces. One token is one or more descriptive symbols followed by a position suffix, wrapped in a modifier.
- `WIRE# n` names the wire caught. The symbols after it (`_EGIW_`, `(EGTL)`, `3PTSIW`) describe the touchdown and the roll-out.
- `[BC]` closes 122 of the 160 comments: the ball call was made. DCS can only record it when the pilot's communications with the ship are working (section 7).
- After `NC` the rest is free text.

## 2. Grade labels (NATOPS 11.4.1)

| Label | Meaning | Comments | Project grade it maps to |
|---|---|---|---|
| `_OK_` | perfect pass | 4 | `_OK_` (`PassGrade::Perfect`) |
| `OK` | reasonable deviations with good corrections | 2 | `OK` |
| `(OK)` | fair: reasonable deviations with average corrections | 4 | `(OK)` |
| `---` (NATOPS `--`) | no grade: below average but safe | 48 | `--` |
| `C` | cut: unsafe, gross deviations inside the waveoff window | 63 | `C` |
| `WO` | waveoff ordered by the LSO | 36 | `WO` (`dcs_waveoff_initiator`: LSO) |
| `OWO` | own waveoff, the pilot's decision | 2 | `OWO` (`dcs_waveoff_initiator`: pilot) |
| `WOP` | pattern waveoff | 0 | none yet (the project's own `WO(P)` comes from the groove-entry detector, not from DCS) |
| `B` | bolter | 0 | `B` is detected from telemetry, never read from DCS |
| `NC` | no count | 1 | no grade from DCS; the free text is kept |

## 3. Modifiers (NATOPS 11.4.1)

| Written | NATOPS meaning | Project reading | Rendered as |
|---|---|---|---|
| `(LOAR)` | parentheses: "a little" | `Magnitude::ALittle`, the grader's `Small` | "a little low at the ramp" |
| `LOAR` | plain | `Magnitude::Moderate`, the grader's `Medium` | "low at the ramp" |
| `_LOAR_` | underline (DCS writes underscores): "for emphasis" | `Magnitude::Gross`, the grader's `Large`, per `docs/GRADING_CONVENTION_PROTOTYPE_2026-09-15.md` | "low at the ramp (gross)" |
| `WO(AFU)IC` | parentheses directly after `WO` | the waveoff's reason, not "a little" | "waveoff: all fouled up in close" |
| `_PPPIC_` | a symbol repeated | repeated calls | "power (three calls) in close (gross)" |

NATOPS also defines a dot between symbols ("on", `S·LUIC`), a dash ("to", `HIM-IC`), a square (signal not answered) and a circle (signal answered too slowly) around a symbol, and an `OC` prefix (over-controlled). DCS has written none of them.

## 4. Descriptive symbols (NATOPS 11.4.2)

"Axis" is the grader's axis the symbol is compared against in `grade-ab` (`Deviation::grading_axis`). "Project writes" marks the symbols `from_episodes` produces for the project's own episodes.

| Symbol | Meaning | Axis | Comments | Project writes |
|---|---|---|---|---|
| `AA` | angling approach | | 0 | |
| `ACC` | accelerating | AoA | 0 | |
| `AFU` | all fouled up | | 39 (always as `WO(AFU)`) | |
| `B` | flat glideslope | glideslope | 0 | |
| `C` | climbing | | 0 | |
| `CB` | coming back to lineup | lineup | 0 | |
| `CD` | coming down | glideslope | 0 | |
| `CH` | chased | | 0 | |
| `CO` | come-on | | 0 | |
| `CU` | cocked up | | 0 | |
| `DD` | deck down | | 0 | |
| `DEC` | decelerating | AoA | 0 | |
| `DL` | drifted left | lineup | 31 | |
| `DN` | dropped nose | | 0 | |
| `DR` | drifted right | lineup | 42 | |
| `DU` | deck up | | 0 | |
| `EG` | eased gun | | 110 (as `_EGIW_`, `(EGTL)`) | |
| `F` | fast | AoA | 9 | yes |
| `FD` | fouled deck | | 1 (as `WOFDIC`) | |
| `GLI` | gliding approach | | 0 | |
| `H` | high | glideslope | 1 | yes |
| `HO` | hold off | | 0 | |
| `LIG` | long in the groove | | 0 | |
| `LL` | landed left | | 3 | |
| `LLU` | late lineup | lineup | 0 | |
| `LO` | low | glideslope | 72 | yes |
| `LR` | landed right | | 4 | |
| `LTR` | left to right | | 0 | |
| `LU` | lineup | lineup | 0 | |
| `LUL` | lined up left | lineup | 68 | yes |
| `LUR` | lined up right | lineup | 46 | yes |
| `LWD` | left wing down | | 0 | |
| `N` | nose | | 9 (as `(NX)`) | |
| `ND` | nose down | | 0 | |
| `NEA` | not enough attitude | | 0 | |
| `NEP` | not enough power | | 0 | |
| `NERD` | not enough rate of descent | glideslope | 0 | |
| `NERR` | not enough right rudder | | 0 | |
| `NESA` | not enough straightaway | | 0 | |
| `NH` | no hook | | 0 | |
| `NSU` | not set up | | 3 (as `WONSUX`) | |
| `OR` | overrotated | | 0 | |
| `OS` | overshot | lineup | 0 | |
| `OSCB` | overshot coming back | lineup | 0 | |
| `P` | power | | 16 (`P`, `PP`, `PPP`) | |
| `PD` | pitching deck | | 0 | |
| `PNU` | pulled nose up | | 0 | |
| `ROT` | rotated | | 0 | |
| `RUD` | rudder | | 0 | |
| `RUF` | rough | | 0 | |
| `RWD` | right wing down | | 0 | |
| `RR` | right rudder | | 0 | |
| `RTL` | right to left | | 0 | |
| `S` | settling | glideslope | 0 | |
| `SD` | spotted the deck | | 0 | |
| `SHT` | ship's turn | | 0 | |
| `SKD` | skidded | | 0 | |
| `SLO` | slow | AoA | 78 | yes |
| `SRD` | stopped rate of descent | glideslope | 0 | |
| `ST` | steep turn | | 0 | |
| `TCA` | too close abeam | | 0 | |
| `TMA` | too much attitude | | 0 | |
| `TMP` | too much power | | 0 | |
| `TMRD` | too much rate of descent | glideslope | 46 | |
| `TMRR` | too much right rudder | | 0 | |
| `TTL` | turned too late | | 0 | |
| `TTS` | turned too soon | | 0 | |
| `TWA` | too wide abeam | | 0 | |
| `W` | wings (read as "wings not level", section 7) | | 28 (as `WX`) | |
| `WU` | wrapped up | | 0 | |
| `XCTL` | cross-controlled | lineup | 0 | |
| `LLWD` | landed left wing down | | 0 | |
| `LRWD` | landed right wing down | | 0 | |
| `LNF` | landed nose first | | 3 | |
| `3PTS` | landed three points | | 15 | |

`WO` inside the body (43 comments) is the waveoff call itself, with its reason in parentheses or attached (`WO(AFU)IC`, `WO(AFU)TL`, `WO(AFU)AR`, `WOFDIC`, `WONSUX`).

The project could write more of these from what it already measures: `TMRD` from the sink rate, `W` from bank, `3PTS` and `LNF` from pitch at touchdown, `DR`/`DL` from the lineup rate. None is written today; `from_episodes` writes only what the grader graded.

## 5. Position suffixes (NATOPS 11.4.3)

| Suffix | Meaning | Tokens | Grader zone |
|---|---|---|---|
| `X` | at the start (first third of the glideslope) | 258 | `Start` |
| `IM` | in the middle (middle third) | 134 | `Middle` |
| `IC` | in close (last third) | 197 | `InClose` |
| `AR` | at the ramp | 80 | `Ramp` |
| `TL` | to land | 23 | none |
| `IW` | in the wires | 120 | none |
| `AW` | all the way | 0 | none |
| `BC` | at the ball call (as a suffix) | 0 | none |
| `OT` | out of the turn | 0 | none |
| `CCA` | on the carrier-controlled approach | 0 | none |

## 6. Where this lives in the code

| What | Where |
|---|---|
| The three tables (symbols, suffixes, grade labels) | `SYMBOLS`, `SUFFIXES`, `GRADES` in `src/lso_notation.rs` |
| Reading a DCS comment | `lso_notation::parse` → `Notation` (grade, wire, deviations, ball call, free text, unknown tokens) |
| The English line | `Notation::english`; `lso_notation::to_english(&str)` for the greenie board (`src/db.rs`) |
| The shorthand line | `Notation::shorthand` |
| The project's episodes in shorthand | `lso_notation::from_episodes` (Discord "LSO Notation (measured by LSO, not a DCS comment)") |
| DCS versus the grader, per axis and zone | `lso_notation::compare` → `Comparison::summary`, the last column of `lso grade-ab` |
| In the JSON report | `dcs_grading_parsed` and `lso_notation_measured` |
| The wire and the waveoff initiator | `Notation::wire` (filtered to 1 to 4 in `src/track.rs`), `Notation::waveoff_initiator` (`src/grading.rs`) |
| Test corpus | `tests/fixtures/dcs_lso_comments.txt`, read by the tests of `src/lso_notation.rs` |

## 7. Readings settled on 21 September 2026

Three choices the parser makes that the glossary alone did not settle:

1. **`[BC]`** is rendered "ball call" and means the ball call was made. In NATOPS a square around a symbol means a signal was not answered, but DCS uses the brackets differently: it can only record the call when the pilot's communications with the ship are working, so `[BC]` is present when comms worked and the call was heard, and absent otherwise (27 comments name a wire without it). Its absence therefore says "not confirmed", not "not answered".
2. **`W`** is rendered "wings not level" (glossary entry: "Wings"). That is what the DCS LSO means by `WX`.
3. **Parentheses directly after `WO`** hold the waveoff's reason: `WO(AFU)IC` is "waveoff: all fouled up in close", not "a little all fouled up". Everywhere else parentheses mean "a little".
