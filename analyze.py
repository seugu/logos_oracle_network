#!/usr/bin/env python3
"""
Oracle staleness / arbitrage-window analysis on Binance 1-second klines.

Models a push oracle that republishes a price every W seconds and measures how
far the live market drifts from the last published value in between.

Two deviations are reported per window:

  endpoint  - (close[t+W] - close[t]) / close[t]
              How stale the published price is by the moment of the next update.

  intra     - max over the window of |high/low - close[t]| / close[t]
              The worst gap that existed at ANY instant inside the window.
              This is the real arbitrage exposure: the gap is exploitable the
              moment it opens, not only at the window boundary.

Input: Binance `1s` kline CSV (or the .zip containing it), as published at
https://data.binance.vision/data/spot/daily/klines/<SYMBOL>/1s/

Columns (no header):
  open_time, open, high, low, close, volume, close_time, quote_volume,
  trades, taker_buy_base, taker_buy_quote, ignore

open_time is auto-detected as seconds / milliseconds / microseconds.
"""

import argparse
import csv
import io
import json
import os
import sys
import zipfile
from datetime import datetime, timezone


# --------------------------------------------------------------------------
# loading
# --------------------------------------------------------------------------

def _detect_time_unit(ts: int) -> int:
    """Return divisor that converts the raw timestamp to seconds."""
    if ts > 1_000_000_000_000_000:      # microseconds
        return 1_000_000
    if ts > 1_000_000_000_000:          # milliseconds
        return 1_000
    return 1                            # seconds


def load_klines(path: str):
    """Load a 1s kline file (.csv or .zip) -> list of (ts_seconds, high, low, close)."""
    if path.lower().endswith(".zip"):
        with zipfile.ZipFile(path) as zf:
            names = [n for n in zf.namelist() if n.lower().endswith(".csv")]
            if not names:
                raise ValueError(f"no .csv inside {path}")
            if len(names) > 1:
                print(f"  note: {len(names)} CSVs in archive, using {names[0]}",
                      file=sys.stderr)
            raw = zf.read(names[0]).decode("utf-8")
    else:
        with open(path, encoding="utf-8") as fh:
            raw = fh.read()

    rows = []
    divisor = None
    for rec in csv.reader(io.StringIO(raw)):
        if not rec or not rec[0].strip():
            continue
        try:
            ts = int(rec[0])
        except ValueError:
            continue                    # header line, if the format ever grows one
        if divisor is None:
            divisor = _detect_time_unit(ts)
        rows.append((ts // divisor, float(rec[2]), float(rec[3]), float(rec[4])))

    rows.sort(key=lambda r: r[0])
    return rows


# --------------------------------------------------------------------------
# analysis
# --------------------------------------------------------------------------

def analyse(rows, window: int, threshold_pct: float, phase: int):
    """Walk non-overlapping `window`-second buckets starting at index `phase`."""
    n = len(rows)
    windows = []

    start = phase
    while start + window < n:
        end = start + window
        p0 = rows[start][3]             # last published price
        p1 = rows[end][3]               # price at the next update

        endpoint = (p1 - p0) / p0 * 100.0

        hi = max(r[1] for r in rows[start:end + 1])
        lo = min(r[2] for r in rows[start:end + 1])
        up = (hi - p0) / p0 * 100.0
        down = (lo - p0) / p0 * 100.0
        intra = up if abs(up) >= abs(down) else down

        windows.append({
            "ts": rows[start][0],
            "utc": datetime.fromtimestamp(rows[start][0], tz=timezone.utc)
                           .strftime("%Y-%m-%d %H:%M:%S"),
            "price": p0,
            "endpoint_pct": endpoint,
            "intra_pct": intra,
        })
        start = end

    def pct(values, q):
        if not values:
            return 0.0
        s = sorted(values)
            # nearest-rank percentile
        idx = min(len(s) - 1, int(len(s) * q))
        return s[idx]

    ep = [abs(w["endpoint_pct"]) for w in windows]
    ia = [abs(w["intra_pct"]) for w in windows]

    breaches_ep = [w for w in windows if abs(w["endpoint_pct"]) > threshold_pct]
    breaches_ia = [w for w in windows if abs(w["intra_pct"]) > threshold_pct]

    return {
        "n_windows": len(windows),
        "windows": windows,
        "endpoint": {
            "median": pct(ep, 0.50),
            "p90": pct(ep, 0.90),
            "p99": pct(ep, 0.99),
            "max": max(ep) if ep else 0.0,
            "breaches": breaches_ep,
            "breach_count": len(breaches_ep),
            "breach_pct": 100.0 * len(breaches_ep) / len(windows) if windows else 0.0,
        },
        "intra": {
            "median": pct(ia, 0.50),
            "p90": pct(ia, 0.90),
            "p99": pct(ia, 0.99),
            "max": max(ia) if ia else 0.0,
            "breaches": breaches_ia,
            "breach_count": len(breaches_ia),
            "breach_pct": 100.0 * len(breaches_ia) / len(windows) if windows else 0.0,
        },
    }


def day_stats(rows):
    hi = max(r[1] for r in rows)
    lo = min(r[2] for r in rows)
    return {
        "bars": len(rows),
        "first": datetime.fromtimestamp(rows[0][0], tz=timezone.utc)
                         .strftime("%Y-%m-%d %H:%M:%S"),
        "last": datetime.fromtimestamp(rows[-1][0], tz=timezone.utc)
                        .strftime("%Y-%m-%d %H:%M:%S"),
        "high": hi,
        "low": lo,
        "range_pct": (hi / lo - 1) * 100.0,
        "open": rows[0][3],
        "close": rows[-1][3],
        "change_pct": (rows[-1][3] / rows[0][3] - 1) * 100.0,
    }


# --------------------------------------------------------------------------
# reporting
# --------------------------------------------------------------------------

def report(path, rows, res, args):
    d = day_stats(rows)
    name = os.path.basename(path)

    print()
    print("=" * 74)
    print(f"  {name}")
    print("=" * 74)
    print(f"  bars        {d['bars']}  ({d['first']} -> {d['last']} UTC)")
    print(f"  day range   {d['low']:.2f} - {d['high']:.2f}   ({d['range_pct']:+.2f}% high/low)")
    print(f"  open/close  {d['open']:.2f} -> {d['close']:.2f}   ({d['change_pct']:+.3f}%)")
    print(f"  windows     {res['n_windows']} x {args.window}s, phase offset {args.phase}s, "
          f"threshold {args.threshold}%")

    for key, label, blurb in (
        ("endpoint", "ENDPOINT", "price move by the time of the next oracle update"),
        ("intra", "INTRA-WINDOW", "worst gap at any instant while the price was stale"),
    ):
        s = res[key]
        print()
        print(f"  {label}  ({blurb})")
        print(f"    median {s['median']:.4f}%   p90 {s['p90']:.4f}%   "
              f"p99 {s['p99']:.4f}%   max {s['max']:.4f}%")
        print(f"    breaches > {args.threshold}%:  {s['breach_count']} "
              f"of {res['n_windows']} windows  ({s['breach_pct']:.3f}%)")

        if s["breaches"]:
            shown = sorted(s["breaches"],
                           key=lambda w: -abs(w[f"{key}_pct"]))[:args.top]
            print(f"    largest {len(shown)}:")
            for w in shown:
                print(f"      {w['utc']}  {w[f'{key}_pct']:+8.3f}%   @ {w['price']:.2f}")
            if len(s["breaches"]) > len(shown):
                print(f"      ... and {len(s['breaches']) - len(shown)} more "
                      f"(use --top N or --csv to see all)")


def phase_sweep(rows, args):
    """Run every possible phase offset and report how much the answer moves."""
    results = []
    for ph in range(0, args.window, args.sweep_step):
        r = analyse(rows, args.window, args.threshold, ph)
        results.append((ph, r["endpoint"]["breach_count"], r["intra"]["breach_count"],
                        r["endpoint"]["max"], r["intra"]["max"]))

    print()
    print("  PHASE SWEEP  (every alignment of the update schedule)")
    print(f"    {'phase':>6}  {'ep breach':>10}  {'intra breach':>13}  "
          f"{'ep max %':>9}  {'intra max %':>12}")
    for ph, epc, iac, epm, iam in results:
        print(f"    {ph:>6}  {epc:>10}  {iac:>13}  {epm:>9.3f}  {iam:>12.3f}")

    eps = [r[1] for r in results]
    ias = [r[2] for r in results]
    print(f"    endpoint breaches  min {min(eps)}  max {max(eps)}  "
          f"spread {max(eps) - min(eps)}")
    print(f"    intra breaches     min {min(ias)}  max {max(ias)}  "
          f"spread {max(ias) - min(ias)}")


def write_csv(out_path, per_file):
    with open(out_path, "w", newline="", encoding="utf-8") as fh:
        w = csv.writer(fh)
        w.writerow(["file", "window_start_utc", "unix_ts", "published_price",
                    "endpoint_pct", "intra_window_pct"])
        for name, res in per_file:
            for win in res["windows"]:
                w.writerow([name, win["utc"], win["ts"], f"{win['price']:.2f}",
                            f"{win['endpoint_pct']:.6f}", f"{win['intra_pct']:.6f}"])
    print(f"\n  wrote {out_path}")


# --------------------------------------------------------------------------

def main():
    ap = argparse.ArgumentParser(
        description="Measure oracle staleness / arbitrage windows from Binance 1s klines.",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog="""examples:
  ./analyze.py data/BTCUSDT-1s-2026-08-20.zip
  ./analyze.py data/*.zip --window 120 --threshold 0.5
  ./analyze.py data/*.zip --phase 10
  ./analyze.py data/*.zip --phase-sweep --sweep-step 10
  ./analyze.py data/*.zip --csv windows.csv --json out.json
""")
    ap.add_argument("files", nargs="+", help=".zip or .csv 1s kline files")
    ap.add_argument("--window", type=int, default=120,
                    help="oracle update interval in seconds (default: 120)")
    ap.add_argument("--threshold", type=float, default=0.5,
                    help="deviation threshold in percent (default: 0.5)")
    ap.add_argument("--phase", type=int, default=0,
                    help="skip this many seconds before the first window, to test "
                         "a different alignment of the update schedule (default: 0)")
    ap.add_argument("--phase-sweep", action="store_true",
                    help="run every phase offset and report the spread")
    ap.add_argument("--sweep-step", type=int, default=10,
                    help="phase increment for --phase-sweep (default: 10)")
    ap.add_argument("--top", type=int, default=15,
                    help="how many largest breaches to list (default: 15)")
    ap.add_argument("--csv", metavar="PATH", help="write every window to a CSV")
    ap.add_argument("--json", metavar="PATH", help="write the summary as JSON")
    args = ap.parse_args()

    if args.phase >= args.window:
        ap.error(f"--phase must be smaller than --window ({args.window})")

    per_file = []
    summary = {}

    for path in args.files:
        if not os.path.exists(path):
            print(f"  skip (missing): {path}", file=sys.stderr)
            continue
        rows = load_klines(path)
        if len(rows) < args.window + 1:
            print(f"  skip (only {len(rows)} bars): {path}", file=sys.stderr)
            continue

        res = analyse(rows, args.window, args.threshold, args.phase)
        report(path, rows, res, args)
        if args.phase_sweep:
            phase_sweep(rows, args)

        name = os.path.basename(path)
        per_file.append((name, res))
        summary[name] = {
            "day": day_stats(rows),
            "n_windows": res["n_windows"],
            "endpoint": {k: v for k, v in res["endpoint"].items() if k != "breaches"},
            "intra": {k: v for k, v in res["intra"].items() if k != "breaches"},
        }

    if not per_file:
        print("no usable input files", file=sys.stderr)
        return 1

    # ---- combined -------------------------------------------------------
    tot_w = sum(r["n_windows"] for _, r in per_file)
    tot_ep = sum(r["endpoint"]["breach_count"] for _, r in per_file)
    tot_ia = sum(r["intra"]["breach_count"] for _, r in per_file)

    print()
    print("=" * 74)
    print(f"  TOTAL over {len(per_file)} file(s)")
    print("=" * 74)
    print(f"  windows                      {tot_w}")
    print(f"  endpoint breaches > {args.threshold}%     {tot_ep}  "
          f"({100.0 * tot_ep / tot_w:.3f}%)")
    print(f"  intra-window breaches > {args.threshold}% {tot_ia}  "
          f"({100.0 * tot_ia / tot_w:.3f}%)")
    print(f"  worst endpoint               "
          f"{max(r['endpoint']['max'] for _, r in per_file):.3f}%")
    print(f"  worst intra-window           "
          f"{max(r['intra']['max'] for _, r in per_file):.3f}%")
    print()

    if args.csv:
        write_csv(args.csv, per_file)
    if args.json:
        with open(args.json, "w", encoding="utf-8") as fh:
            json.dump({"params": vars(args), "files": summary}, fh, indent=2)
        print(f"  wrote {args.json}")

    return 0


if __name__ == "__main__":
    sys.exit(main())
