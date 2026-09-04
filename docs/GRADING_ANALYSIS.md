# Grading design record

**Status:** implemented reliability model.

**Current specification:** [GRADING_REFERENCE.md](GRADING_REFERENCE.md)

This document records the grading design boundary. The result is a project score, never an
official USN/USMC certification.

## Structural contract

Outcome, grade, optional points, comment, cause, confidence, completeness and grading version are
separate values. A pass with fewer than three valid, ordered gate observations is `NC`, has no
favourable grade and receives no points. Event evidence and telemetry quality remain available for
audit independently of the displayed grade.

`_OK_` is an official display symbol (`OFFICIAL`: NAVAIR 00-80T-104, 2001, section 11.4.1), but no
automatic rule currently emits it. The former project rule based on wire 3 and a 15--18.99 second
groove time is disabled. A touch-and-go cannot receive `_OK_` or any other favourable automatic
grade.

## Project-derived rules

The three interpolated gate deviations, the continuous groove-to-touchdown trajectory amplitude, a
final-approach correction-trend check (Ok vs (OK) only), a late-approach severity check
(Ok/(OK) vs NoGrade only), numerical thresholds, points mapping, `NC` representation, wire geometry
and experimental AV-8B/Tarawa spot model are `PROJECT-DERIVED`, version `project-derived-v4`. Their
formulas, assumptions and limitations are specified in
[GRADING_REFERENCE.md](GRADING_REFERENCE.md). AoA, deviation duration and spot-zone occupancy are
recorded only; they do not affect the score.

For V/STOL phase 1, the intended spot is 7.5. The nearest actual spot and distance to 7.5 are
reported separately. Spots 7 and 8 are not activated for scoring. Incomplete V/STOL data never
produces a favourable grade.

## Verification surface

Focused tests cover missing and unordered gates, interpolation, legacy wire/time non-upgrade,
touch-and-go, waveoff with unknown initiator, V/STOL incompleteness, wire provenance and
zero/360-degree geometry. Captured ACMI fixtures verify deterministic replay behavior but do not
prove live DCS event semantics.
