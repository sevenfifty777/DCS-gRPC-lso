#!/usr/bin/env python3
"""Align a client-side AoA log (LsoAoaExport.lua CSV) with LSO JSON reports and calibrate.

For every report in the input folder whose recording overlaps the CSV, this script:

1. aligns the two series on DCS model time (with a small cross-correlation search for a
   constant clock offset between the pilot's client and the server), restricted to the groove
   (from `groove_entry.timestamp_dcs` to `touchdown_time_dcs`, the only stretch where the
   LSO's wind-corrected AoA is defined);
2. reports the difference between the LSO's computed AoA (`datums[].aoa`) and the flight
   model's true AoA (`aoa_true_deg`): median, mean, spread. A constant difference is an offset
   in our computation; a difference that grows with bank or wind points at the correction;
3. reports the true AoA observed while the cockpit indexer showed each state (on-speed donut
   only, donut plus slow chevron, slow only, donut plus fast chevron, fast only), so the
   on-speed band can be written in the units `src/data.rs` uses, per aircraft type;
4. prints a proposed band per type next to the band currently in the code.

Usage:
    python tools/aoa_calibration/align_aoa.py <csv or folder of csv> <folder of LSO json reports>
        [--max-offset-s 2.0] [--markdown out.md]

Only the Python standard library is used.
"""

from __future__ import annotations

import argparse
import csv
import json
import math
import statistics
import sys
from pathlib import Path

# Bands currently in src/data.rs (degrees): (fast_max, slightly_fast_max, on_speed_max_exclusive,
# slightly_slow_max_exclusive). On speed is [slightly_fast_max, on_speed_max).
CODE_BANDS = {
    "T-45": (6.0, 6.5, 7.5, 8.0),
    "F-14BU": (9.7, 10.2, 11.1, 11.6),
    "F-14B": (9.7, 10.2, 11.1, 11.6),
    "F-14A": (9.7, 10.2, 11.1, 11.6),
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


def load_csv_rows(path: Path) -> list[dict]:
    rows = []
    with path.open(newline="", encoding="utf-8") as handle:
        for row in csv.DictReader(handle):
            try:
                row["model_time_s"] = float(row["model_time_s"])
            except (KeyError, ValueError):
                continue
            for key in ("aoa_true_deg", "indexer_slow", "indexer_opt", "indexer_fast",
                        "gauge_units", "hud_aoa_units", "bank_deg", "lat", "lon", "alt_msl_m"):
                value = row.get(key, "")
                row[key] = float(value) if value not in ("", None) else math.nan
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


def interpolate(series: list[tuple[float, float]], t: float) -> float | None:
    """Linear interpolation in a sorted (time, value) list; None outside or across a gap > 0.3 s."""
    import bisect

    times = [s[0] for s in series]
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
    best = (0.0, math.inf, 0)
    step = 0.05
    n_steps = int(round(max_offset_s / step))
    for k in range(-n_steps, n_steps + 1):
        offset = k * step
        diffs = []
        for t, ours in lso:
            theirs = interpolate(true_series, t + offset)
            if theirs is not None and not math.isnan(ours):
                diffs.append(ours - theirs)
        if len(diffs) >= 20:
            score = statistics.pstdev(diffs)
            if score < best[1]:
                best = (offset, score, len(diffs))
    return best[0], best[2]


def summarize(values: list[float]) -> str:
    if not values:
        return "n=0"
    values = sorted(values)
    q = statistics.quantiles(values, n=10) if len(values) >= 10 else [values[0]] * 9
    return (f"n={len(values)} median={statistics.median(values):.2f} "
            f"mean={statistics.mean(values):.2f} p10={q[0]:.2f} p90={q[8]:.2f}")


def analyze_report(report_path: Path, csv_rows: list[dict], max_offset_s: float) -> dict | None:
    with report_path.open(encoding="utf-8") as handle:
        report = json.load(handle)
    aircraft = report.get("aircraft_type", "?")
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
    # Candidate CSV rows: same aircraft type, times within the groove window plus slack.
    csv_type_rows = [r for r in csv_rows
                     if CSV_TO_REPORT_TYPE.get(r["aircraft"], r["aircraft"]) == aircraft
                     and entry - max_offset_s - 1 <= r["model_time_s"] <= end + max_offset_s + 1]
    if len(csv_type_rows) < 20:
        return None
    true_series = [(r["model_time_s"], r["aoa_true_deg"]) for r in csv_type_rows]
    offset, matched = best_offset(lso, true_series, max_offset_s)
    if matched < 20:
        return None
    diffs, by_state, banks = [], {}, []
    state_series = [(r["model_time_s"], indexer_state(r)) for r in csv_type_rows]
    for t, ours in lso:
        theirs = interpolate(true_series, t + offset)
        if theirs is None:
            continue
        diffs.append(ours - theirs)
    # True AoA by indexer state, over the whole groove window of the CSV (independent of the LSO).
    for r in csv_type_rows:
        if entry - offset <= r["model_time_s"] <= end - offset and not math.isnan(r["aoa_true_deg"]):
            by_state.setdefault(indexer_state(r), []).append(r["aoa_true_deg"])
    return {
        "report": report_path.name,
        "aircraft": aircraft,
        "wind_reference_established": report.get("wind_reference_established"),
        "wind_fallback": report.get("wind_reading_is_groove_entry_fallback"),
        "offset_s": offset,
        "matched": len(diffs),
        "diff": diffs,
        "by_state": by_state,
    }


def propose_band(on_speed: list[float], slightly_slow: list[float], slightly_fast: list[float]) -> str:
    if len(on_speed) < 20:
        return "not enough on-speed samples"
    values = sorted(on_speed)
    q = statistics.quantiles(values, n=20)
    low, high = q[0], q[18]  # p5 .. p95 of what the donut alone covered
    centre = statistics.median(values)
    return (f"on speed {low:.1f} to {high:.1f} deg (centre {centre:.1f}); "
            f"slightly slow edge ~{statistics.median(slightly_slow):.1f}" if len(slightly_slow) >= 10 else
            f"on speed {low:.1f} to {high:.1f} deg (centre {centre:.1f}); slightly slow: no samples") + (
            f"; slightly fast edge ~{statistics.median(slightly_fast):.1f}" if len(slightly_fast) >= 10 else
            "; slightly fast: no samples")


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
    results = [r for r in (analyze_report(p, csv_rows, args.max_offset_s) for p in reports) if r]
    if not results:
        print("no report overlapped the CSV (check aircraft type and that both cover the same mission)",
              file=sys.stderr)
        return 1

    lines = ["# AoA calibration result", "",
             f"CSV rows: {len(csv_rows)}, reports matched: {len(results)}", "",
             "## Per pass: LSO computed AoA minus flight-model true AoA (degrees, groove only)", "",
             "| report | type | wind ref | clock offset | matched samples | median | mean | p10 | p90 |",
             "|---|---|---|---|---|---|---|---|---|"]
    per_type: dict[str, dict] = {}
    for r in results:
        diffs = sorted(r["diff"])
        q = statistics.quantiles(diffs, n=10) if len(diffs) >= 10 else [diffs[0]] * 9
        wind = "yes" + (" (fallback)" if r["wind_fallback"] else "") if r["wind_reference_established"] else "no"
        lines.append(f"| {r['report'][:24]} | {r['aircraft']} | {wind} | {r['offset_s']:+.2f} s | {r['matched']} | "
                     f"{statistics.median(diffs):+.2f} | {statistics.mean(diffs):+.2f} | {q[0]:+.2f} | {q[8]:+.2f} |")
        bucket = per_type.setdefault(r["aircraft"], {"diff": [], "by_state": {}})
        bucket["diff"].extend(r["diff"])
        for state, values in r["by_state"].items():
            bucket["by_state"].setdefault(state, []).extend(values)

    lines += ["", "## Per type: true AoA while the cockpit indexer showed each state (degrees)", ""]
    for aircraft, bucket in per_type.items():
        lines.append(f"### {aircraft}")
        lines.append("")
        lines.append(f"- LSO minus true AoA over all matched samples: {summarize(bucket['diff'])}")
        for state in ("fast", "slightly_fast", "on_speed", "slightly_slow", "slow", "off"):
            values = bucket["by_state"].get(state, [])
            if values:
                lines.append(f"- {state}: {summarize(values)}")
        code = CODE_BANDS.get(aircraft)
        if code:
            lines.append(f"- band in src/data.rs: fast <= {code[0]}, slightly fast <= {code[1]}, "
                         f"on speed < {code[2]}, slightly slow < {code[3]}, slow >= {code[3]}")
        lines.append("- proposed from this flight: " + propose_band(
            bucket["by_state"].get("on_speed", []),
            bucket["by_state"].get("slightly_slow", []),
            bucket["by_state"].get("slightly_fast", [])))
        lines.append("")

    text = "\n".join(lines)
    print(text)
    if args.markdown:
        args.markdown.write_text(text + "\n", encoding="utf-8")
    return 0


if __name__ == "__main__":
    sys.exit(main())
