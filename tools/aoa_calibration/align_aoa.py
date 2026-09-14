#!/usr/bin/env python3
"""Align a client-side AoA log (LsoAoaExport.lua CSV) with LSO JSON reports and calibrate.

For every report in the input folder whose recording overlaps the CSV, this script:

1. selects the CSV rows of the same pilot (`unit_name` == the report's `pilot_name`; the aircraft
   type alone is not enough once two pilots fly the same type at the same time), then aligns the
   two series on DCS model time (with a small cross-correlation search for a constant clock
   offset between the pilot's client and the server), restricted to the groove (from
   `groove_entry.timestamp_dcs` to `touchdown_time_dcs`, the only stretch where the LSO's
   wind-corrected AoA is defined);
2. reports the difference between the LSO's computed AoA (`datums[].aoa`) and the flight
   model's true AoA (`aoa_true_deg`): median, mean, spread, and its dependence on bank. A
   constant difference is an offset in our computation; a difference that grows with bank or
   wind points at the correction;
3. reports what the pilot actually flew in the groove: the true AoA and the share of time the
   cockpit indexer showed each state;
4. per aircraft type, measures the true AoA at which each indexer lamp switches (every
   transition over the whole flight, both directions, so the lamp hysteresis is visible) and
   derives the on-speed band in the units `src/data.rs` uses;
5. prints the band currently in the code next to the measured one, and how many groove samples
   each band rates on speed, slightly off, or fast/slow, per pass.

Usage:
    python tools/aoa_calibration/align_aoa.py <csv or folder of csv> <folder of LSO json reports>
        [--max-offset-s 2.0] [--markdown out.md]

Only the Python standard library is used.
"""

from __future__ import annotations

import argparse
import bisect
import csv
import json
import math
import statistics
import sys
from pathlib import Path

# Bands currently in src/data.rs (degrees): (fast_max, slightly_fast_max, on_speed_max_exclusive,
# slightly_slow_max_exclusive). On speed is [slightly_fast_max, on_speed_max). The T-45 and F-14
# values are the ones measured on 14 September 2026 (docs/AOA_CALIBRATION_REVIEW_2026-09-14.md);
# before that flight they were (6.0, 6.5, 7.5, 8.0) and (9.7, 10.2, 11.1, 11.6).
CODE_BANDS = {
    "T-45": (8.0, 8.25, 8.75, 9.0),
    "F-14BU": (9.45, 9.95, 10.8, 11.25),
    "F-14B": (9.45, 9.95, 10.8, 11.25),
    "F-14A": (9.45, 9.95, 10.8, 11.25),
    "FA-18C_hornet": (6.9, 7.4, 8.8, 9.3),
}

# Map the CSV aircraft name (LoGetSelfData().Name) to the report's aircraft_type.
CSV_TO_REPORT_TYPE = {
    "F-14B(U)": "F-14BU",
    "F-14BU": "F-14BU",
    "F-14B": "F-14B",
    "F-14A-135-GR": "F-14A",
    "F-14A-135-GR-Early": "F-14A",
    "F-14A-95-GR": "F-14A",
}

STATES = ("fast", "slightly_fast", "on_speed", "slightly_slow", "slow")
NUMERIC = ("aoa_true_deg", "indexer_slow", "indexer_opt", "indexer_fast", "gauge_units",
           "hud_aoa_units", "bank_deg", "pitch_deg", "lat", "lon", "alt_msl_m", "ias_mps",
           "hook_draw_arg")


def load_csv_rows(path: Path) -> list[dict]:
    rows = []
    with path.open(newline="", encoding="utf-8") as handle:
        for row in csv.DictReader(handle):
            try:
                row["model_time_s"] = float(row["model_time_s"])
            except (KeyError, ValueError):
                continue
            for key in NUMERIC:
                value = row.get(key, "")
                row[key] = float(value) if value not in ("", None) else math.nan
            row["report_type"] = CSV_TO_REPORT_TYPE.get(row.get("aircraft", ""), row.get("aircraft", ""))
            rows.append(row)
    rows.sort(key=lambda r: r["model_time_s"])
    return rows


def collect_csv(path: Path) -> list[dict]:
    files = [path] if path.is_file() else sorted(path.glob("lso_aoa_*.csv"))
    rows: list[dict] = []
    for file in files:
        rows.extend(load_csv_rows(file))
    rows.sort(key=lambda r: r["model_time_s"])
    return rows


def indexer_state(row: dict) -> str:
    """Cockpit indexer state from the three lamp arguments (value > 0.5 = lit)."""
    slow = row["indexer_slow"] > 0.5 if not math.isnan(row["indexer_slow"]) else False
    opt = row["indexer_opt"] > 0.5 if not math.isnan(row["indexer_opt"]) else False
    fast = row["indexer_fast"] > 0.5 if not math.isnan(row["indexer_fast"]) else False
    if opt and not slow and not fast:
        return "on_speed"
    if opt and slow:
        return "slightly_slow"
    if opt and fast:
        return "slightly_fast"
    if slow:
        return "slow"
    if fast:
        return "fast"
    return "off"


def rate(band: tuple[float, float, float, float], aoa: float) -> str:
    """The `src/data.rs` rating of a computed AoA under a (fast_max, sf_max, os_max, ss_max) band."""
    fast_max, slightly_fast_max, on_speed_max, slightly_slow_max = band
    if aoa <= fast_max:
        return "fast"
    if aoa <= slightly_fast_max:
        return "slightly_fast"
    if aoa < on_speed_max:
        return "on_speed"
    if aoa < slightly_slow_max:
        return "slightly_slow"
    return "slow"


def interpolate(series: list[tuple[float, float]], times: list[float], t: float) -> float | None:
    """Linear interpolation in a sorted (time, value) list; None outside or across a gap > 0.3 s."""
    index = bisect.bisect_left(times, t)
    if index == 0 or index >= len(series):
        return None
    t0, v0 = series[index - 1]
    t1, v1 = series[index]
    if t1 - t0 > 0.3 or math.isnan(v0) or math.isnan(v1):
        return None
    ratio = (t - t0) / (t1 - t0) if t1 > t0 else 0.0
    return v0 + (v1 - v0) * ratio


def best_offset(lso: list[tuple[float, float]], true_series: list[tuple[float, float]],
                max_offset_s: float) -> tuple[float, int]:
    """Clock offset (added to LSO time) minimising the *spread* of the AoA difference.

    The spread, not the size: a constant offset between the two AoA computations is exactly
    what we are trying to measure, so it must not steer the alignment.
    """
    times = [s[0] for s in true_series]
    best = (0.0, math.inf, 0)
    step = 0.05
    n_steps = int(round(max_offset_s / step))
    for k in range(-n_steps, n_steps + 1):
        offset = k * step
        diffs = []
        for t, ours in lso:
            theirs = interpolate(true_series, times, t + offset)
            if theirs is not None and not math.isnan(ours):
                diffs.append(ours - theirs)
        if len(diffs) >= 20:
            score = statistics.pstdev(diffs)
            if score < best[1]:
                best = (offset, score, len(diffs))
    return best[0], best[2]


def quantile(values: list[float], fraction: float) -> float:
    ordered = sorted(values)
    if not ordered:
        return math.nan
    position = (len(ordered) - 1) * fraction
    low = int(math.floor(position))
    high = min(low + 1, len(ordered) - 1)
    return ordered[low] + (ordered[high] - ordered[low]) * (position - low)


def summarize(values: list[float]) -> str:
    if not values:
        return "n=0"
    return (f"n={len(values)} median={statistics.median(values):.2f} "
            f"mean={statistics.mean(values):.2f} p10={quantile(values, 0.1):.2f} "
            f"p90={quantile(values, 0.9):.2f}")


def slope(xs: list[float], ys: list[float]) -> float:
    """Least-squares slope of ys against xs (NaN when xs has no spread)."""
    if len(xs) < 3:
        return math.nan
    mx, my = statistics.mean(xs), statistics.mean(ys)
    sxx = sum((x - mx) ** 2 for x in xs)
    if sxx <= 0.0:
        return math.nan
    return sum((x - mx) * (y - my) for x, y in zip(xs, ys)) / sxx


def analyze_report(report_path: Path, csv_rows: list[dict], max_offset_s: float,
                   warnings: list[str]) -> dict | None:
    with report_path.open(encoding="utf-8") as handle:
        report = json.load(handle)
    aircraft = report.get("aircraft_type", "?")
    pilot = report.get("pilot_name", "")
    entry = (report.get("groove_entry") or {}).get("timestamp_dcs")
    touchdown = report.get("touchdown_time_dcs")
    if entry is None:
        return None
    end = touchdown if touchdown is not None else entry + 25.0
    lso = [(d["time"], d["aoa"]) for d in report.get("datums", [])
           if entry <= d["time"] <= end and d.get("aoa") is not None
           and isinstance(d["aoa"], (int, float)) and not math.isnan(d["aoa"])]
    if len(lso) < 20:
        return None
    # Candidate CSV rows: this pilot (fall back to the type alone when the CSV carries another
    # unit name, e.g. a renamed slot), within the groove window plus slack.
    window = [r for r in csv_rows
              if r["report_type"] == aircraft
              and entry - max_offset_s - 1 <= r["model_time_s"] <= end + max_offset_s + 1]
    csv_type_rows = [r for r in window if r.get("unit_name") == pilot]
    if len(csv_type_rows) < 20:
        others = sorted({r.get("unit_name", "?") for r in window})
        if len(window) >= 20:
            warnings.append(f"{report_path.name}: no CSV rows for pilot '{pilot}', "
                            f"using type-only match (CSV units in window: {others})")
            csv_type_rows = window
        else:
            return None
    true_series = [(r["model_time_s"], r["aoa_true_deg"]) for r in csv_type_rows]
    true_times = [s[0] for s in true_series]
    offset, matched = best_offset(lso, true_series, max_offset_s)
    if matched < 20:
        return None
    bank_series = [(r["model_time_s"], abs(r["bank_deg"])) for r in csv_type_rows]
    state_series = [(r["model_time_s"], indexer_state(r)) for r in csv_type_rows]
    lso_times = [t for t, _ in lso]
    diffs, banks, by_state, aligned_true, paired = [], [], {}, [], []
    for index, (t, ours) in enumerate(lso):
        theirs = interpolate(true_series, true_times, t + offset)
        if theirs is None:
            continue
        diffs.append(ours - theirs)
        aligned_true.append(theirs)
        bank = interpolate(bank_series, true_times, t + offset)
        banks.append(bank if bank is not None else math.nan)
        # Cockpit state at the same instant (nearest CSV sample), and the LSO value smoothed over
        # a centred 0.5 s window, to see whether a little smoothing raises the agreement.
        nearest = bisect.bisect_left(true_times, t + offset)
        nearest = min(max(nearest, 0), len(state_series) - 1)
        if nearest > 0 and abs(state_series[nearest - 1][0] - (t + offset)) < abs(state_series[nearest][0] - (t + offset)):
            nearest -= 1
        lo = bisect.bisect_left(lso_times, t - 0.25)
        hi = bisect.bisect_right(lso_times, t + 0.25)
        smoothed = statistics.mean(v for _, v in lso[lo:hi])
        paired.append((state_series[nearest][1], ours, smoothed))
    # True AoA by indexer state, over the groove window of the CSV (independent of the LSO).
    groove_rows = [r for r in csv_type_rows
                   if entry - offset <= r["model_time_s"] <= end - offset
                   and not math.isnan(r["aoa_true_deg"])]
    for r in groove_rows:
        by_state.setdefault(indexer_state(r), []).append(r["aoa_true_deg"])
    pairs = [(b, d) for b, d in zip(banks, diffs) if not math.isnan(b)]
    return {
        "report": report_path.name,
        "pilot": pilot,
        "aircraft": aircraft,
        "outcome": report.get("outcome", ""),
        "dcs_grading": (report.get("dcs_grading") or "").replace("\n", " "),
        "wind_reference_established": report.get("wind_reference_established"),
        "wind_fallback": report.get("wind_reading_is_groove_entry_fallback"),
        "wind_speed_mps": report.get("wind_speed_mps"),
        "offset_s": offset,
        "matched": len(diffs),
        "diff": diffs,
        "bank_slope": slope([p[0] for p in pairs], [p[1] for p in pairs]),
        "lso_groove": [ours for _, ours in lso],
        "paired": paired,
        "true_groove": [r["aoa_true_deg"] for r in groove_rows],
        "by_state": by_state,
        "groove_rows": len(groove_rows),
    }


def lamp_transitions(rows: list[dict]) -> dict[tuple[str, str], list[float]]:
    """True AoA (midpoint of the two samples) at every change of indexer state, over the whole
    flight of one pilot in one type. Keyed by (state before, state after)."""
    transitions: dict[tuple[str, str], list[float]] = {}
    previous = None
    for row in rows:
        state = indexer_state(row)
        if previous is not None:
            prev_row, prev_state = previous
            gap = row["model_time_s"] - prev_row["model_time_s"]
            if (state != prev_state and gap <= 0.3 and "off" not in (state, prev_state)
                    and not math.isnan(row["aoa_true_deg"]) and not math.isnan(prev_row["aoa_true_deg"])
                    and abs(row["aoa_true_deg"] - prev_row["aoa_true_deg"]) < 1.0):
                transitions.setdefault((prev_state, state), []).append(
                    (row["aoa_true_deg"] + prev_row["aoa_true_deg"]) / 2.0)
        previous = (row, state)
    return transitions


# The four thresholds of a `src/data.rs` band, each as the pair of lamp transitions that mark it
# (either direction of crossing).
THRESHOLD_TRANSITIONS = [
    ("fast_max", (("fast", "slightly_fast"), ("slightly_fast", "fast"))),
    ("slightly_fast_max", (("slightly_fast", "on_speed"), ("on_speed", "slightly_fast"))),
    ("on_speed_max", (("on_speed", "slightly_slow"), ("slightly_slow", "on_speed"))),
    ("slightly_slow_max", (("slightly_slow", "slow"), ("slow", "slightly_slow"))),
]


def measured_band(transitions: dict[tuple[str, str], list[float]]) -> tuple[list[float | None], list[str]]:
    thresholds: list[float | None] = []
    notes = []
    for name, (up, down) in THRESHOLD_TRANSITIONS:
        values = transitions.get(up, []) + transitions.get(down, [])
        if len(values) >= 3:
            thresholds.append(statistics.median(values))
            up_txt = f"{statistics.median(transitions[up]):.2f} (n={len(transitions[up])})" if transitions.get(up) else "none"
            down_txt = f"{statistics.median(transitions[down]):.2f} (n={len(transitions[down])})" if transitions.get(down) else "none"
            notes.append(f"{name}: {statistics.median(values):.2f} deg from {len(values)} switches "
                         f"(towards slow {up_txt}, towards fast {down_txt})")
        else:
            thresholds.append(None)
            notes.append(f"{name}: not enough lamp switches (n={len(values)})")
    return thresholds, notes


def state_shares(values_by_state: dict[str, list[float]]) -> str:
    total = sum(len(v) for k, v in values_by_state.items() if k != "off")
    if total == 0:
        return "no lit samples"
    parts = []
    for state in STATES:
        n = len(values_by_state.get(state, []))
        if n:
            parts.append(f"{state} {100.0 * n / total:.0f}%")
    return ", ".join(parts)


def rating_shares(values: list[float], band: tuple[float, float, float, float]) -> str:
    if not values:
        return "n=0"
    counts = {state: 0 for state in STATES}
    for value in values:
        counts[rate(band, value)] += 1
    total = len(values)
    return ", ".join(f"{state} {100.0 * counts[state] / total:.0f}%" for state in STATES if counts[state])


def agreement(paired: list[tuple[str, float, float]], band: tuple[float, float, float, float],
              smoothed: bool) -> tuple[str, float, float]:
    """Confusion of the cockpit indexer state against the LSO rating under `band`, as Markdown
    rows, plus the share of samples that agree exactly and within one step."""
    order = {state: i for i, state in enumerate(STATES)}
    matrix: dict[str, dict[str, int]] = {s: {t: 0 for t in STATES} for s in STATES}
    exact = within_one = total = 0
    for cockpit, raw, smooth in paired:
        if cockpit not in order:
            continue
        ours = rate(band, smooth if smoothed else raw)
        matrix[cockpit][ours] += 1
        total += 1
        exact += cockpit == ours
        within_one += abs(order[cockpit] - order[ours]) <= 1
    rows = ["| cockpit shows \\ LSO rates | " + " | ".join(STATES) + " |", "|---|" + "---|" * len(STATES)]
    for cockpit in STATES:
        n = sum(matrix[cockpit].values())
        if n:
            rows.append(f"| {cockpit} (n={n}) | " + " | ".join(
                f"{100.0 * matrix[cockpit][ours] / n:.0f}%" for ours in STATES) + " |")
    if total == 0:
        return "no paired samples", math.nan, math.nan
    return "\n".join(rows), 100.0 * exact / total, 100.0 * within_one / total


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("csv", type=Path, help="lso_aoa_*.csv file, or a folder containing them")
    parser.add_argument("reports", type=Path, help="folder of LSO JSON reports (searched recursively)")
    parser.add_argument("--max-offset-s", type=float, default=2.0,
                        help="largest client/server clock offset to search (default 2.0 s)")
    parser.add_argument("--markdown", type=Path, help="also write the result as a Markdown file")
    args = parser.parse_args()

    csv_rows = collect_csv(args.csv)
    if not csv_rows:
        print("no CSV rows found", file=sys.stderr)
        return 1
    reports = sorted(p for p in args.reports.rglob("*.json") if p.name.startswith("LSO-"))
    warnings: list[str] = []
    results = [r for r in (analyze_report(p, csv_rows, args.max_offset_s, warnings) for p in reports) if r]
    if not results:
        print("no report overlapped the CSV (check aircraft type and that both cover the same mission)",
              file=sys.stderr)
        return 1

    pilots = sorted({r.get("unit_name", "?") for r in csv_rows})
    lines = ["# AoA calibration result", "",
             f"CSV rows: {len(csv_rows)} (pilots: {', '.join(pilots)}), reports matched: {len(results)}", ""]
    for warning in warnings:
        lines.append(f"- warning: {warning}")
    if warnings:
        lines.append("")
    lines += ["## Per pass: LSO computed AoA minus flight-model true AoA (degrees, groove only)", "",
              "| report | pilot | type | wind ref | clock offset | matched | median | mean | p10 | p90 | per deg of bank |",
              "|---|---|---|---|---|---|---|---|---|---|---|"]
    per_type: dict[str, dict] = {}
    for r in results:
        diffs = r["diff"]
        wind = "yes" + (" (fallback)" if r["wind_fallback"] else "") if r["wind_reference_established"] else "no"
        lines.append(f"| {r['report'][4:19]} | {r['pilot']} | {r['aircraft']} | {wind} | {r['offset_s']:+.2f} s | "
                     f"{r['matched']} | {statistics.median(diffs):+.2f} | {statistics.mean(diffs):+.2f} | "
                     f"{quantile(diffs, 0.1):+.2f} | {quantile(diffs, 0.9):+.2f} | {r['bank_slope']:+.3f} |")
        bucket = per_type.setdefault(r["aircraft"], {"diff": [], "by_state": {}, "passes": []})
        bucket["diff"].extend(diffs)
        bucket["passes"].append(r)
        for state, values in r["by_state"].items():
            bucket["by_state"].setdefault(state, []).extend(values)

    lines += ["", "## Per pass: what was flown in the groove (flight model and cockpit indexer)", "",
              "| report | pilot | type | outcome | DCS LSO | true AoA median | p10 | p90 | indexer share of groove |",
              "|---|---|---|---|---|---|---|---|---|"]
    for r in results:
        true = r["true_groove"]
        dcs = r["dcs_grading"]
        dcs = dcs.split("GRADE:")[1].split("[")[0].strip() if "GRADE:" in dcs else "none"
        lines.append(f"| {r['report'][4:19]} | {r['pilot']} | {r['aircraft']} | {r['outcome']} | {dcs} | "
                     f"{statistics.median(true):.2f} | {quantile(true, 0.1):.2f} | {quantile(true, 0.9):.2f} | "
                     f"{state_shares(r['by_state'])} |")

    lines += ["", "## Per type: indexer lamp thresholds and the on-speed band", ""]
    proposed_bands: dict[str, tuple[float, float, float, float]] = {}
    for aircraft, bucket in per_type.items():
        lines.append(f"### {aircraft}")
        lines.append("")
        lines.append(f"- LSO minus true AoA over all matched groove samples: {summarize(bucket['diff'])}")
        lines.append("- True AoA while the indexer showed each state (groove samples of all passes):")
        for state in STATES + ("off",):
            values = bucket["by_state"].get(state, [])
            if values:
                lines.append(f"  - {state}: {summarize(values)}")
        type_rows = [r for r in csv_rows if r["report_type"] == aircraft]
        transitions: dict[tuple[str, str], list[float]] = {}
        for pilot in sorted({r.get("unit_name", "?") for r in type_rows}):
            for key, values in lamp_transitions([r for r in type_rows if r.get("unit_name") == pilot]).items():
                transitions.setdefault(key, []).extend(values)
        thresholds, notes = measured_band(transitions)
        lines.append("- Lamp switch thresholds measured over the whole flight (true AoA at the switch):")
        for note in notes:
            lines.append(f"  - {note}")
        other = {k: v for k, v in transitions.items()
                 if not any(k in pair for _, pair in THRESHOLD_TRANSITIONS)}
        if other:
            lines.append("  - other state changes seen: " + ", ".join(
                f"{a}->{b} n={len(v)} median {statistics.median(v):.2f}" for (a, b), v in sorted(other.items())))
        code = CODE_BANDS.get(aircraft)
        if code:
            lines.append(f"- Band in src/data.rs: fast <= {code[0]}, slightly fast <= {code[1]}, "
                         f"on speed < {code[2]}, slightly slow < {code[3]}, slow >= {code[3]}")
        if all(t is not None for t in thresholds):
            band = tuple(round(t, 1) for t in thresholds)  # type: ignore[arg-type]
            proposed_bands[aircraft] = band  # type: ignore[assignment]
            lines.append(f"- Measured band (rounded to 0.1 deg): fast <= {band[0]}, slightly fast <= {band[1]}, "
                         f"on speed < {band[2]}, slightly slow < {band[3]}, slow >= {band[3]}")
        else:
            lines.append("- Measured band: incomplete (see thresholds above)")
        lines.append("")

    lines += ["## Per pass: how each band rates the LSO's own groove samples", "",
              "| report | pilot | type | indexer (cockpit) | code band | measured band |",
              "|---|---|---|---|---|---|"]
    for r in results:
        code = CODE_BANDS.get(r["aircraft"])
        measured = proposed_bands.get(r["aircraft"])
        lines.append(f"| {r['report'][4:19]} | {r['pilot']} | {r['aircraft']} | {state_shares(r['by_state'])} | "
                     f"{rating_shares(r['lso_groove'], code) if code else 'n/a'} | "
                     f"{rating_shares(r['lso_groove'], measured) if measured else 'n/a'} |")

    lines += ["", "## Per type: agreement between the cockpit indexer and the LSO rating (groove samples)", ""]
    for aircraft, bucket in per_type.items():
        paired = [p for r in bucket["passes"] for p in r["paired"]]
        lines.append(f"### {aircraft}")
        lines.append("")
        for label, band in (("code band", CODE_BANDS.get(aircraft)), ("measured band", proposed_bands.get(aircraft))):
            if band is None:
                continue
            for smoothed in (False, True):
                table, exact, within = agreement(paired, band, smoothed)
                lines.append(f"**{label}, LSO AoA {'smoothed over 0.5 s' if smoothed else 'raw'}**: "
                             f"exact agreement {exact:.0f}%, within one step {within:.0f}%")
                lines.append("")
                lines.append(table)
                lines.append("")

    text = "\n".join(lines)
    print(text)
    if args.markdown:
        args.markdown.write_text(text + "\n", encoding="utf-8")
    return 0


if __name__ == "__main__":
    sys.exit(main())
