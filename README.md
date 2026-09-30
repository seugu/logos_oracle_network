# lon-oracle-lag

Measuring how stale a push oracle's price gets between updates, using real
Binance 1-second data.

A push oracle republishes on a fixed cadence. Between two updates the on-chain
price is frozen while the market keeps moving, so a gap opens: anyone who can
see both numbers can trade against the stale one. This repo measures that gap
on real data instead of estimating it from a volatility model.

Written to check an assumption in the
[Logos Oracle Network](https://github.com/logos-blockchain) design, where the
publish cadence is roughly two minutes. Nothing here is specific to LON —
point it at any symbol and any window length.

## Quick start

```bash
./fetch-data.sh                # downloads two sample days of BTCUSDT
./run.sh                       # 120s windows, 0.5% threshold
```

Fetch whatever span you want:

```bash
./fetch-data.sh --days 10                    # last 10 available days
./fetch-data.sh --days 10 --end 2026-08-25   # 10 days ending on a date
./fetch-data.sh --symbol ETHUSDT --days 5
./fetch-data.sh 2026-08-20 2026-09-08        # explicit dates
./fetch-data.sh --days 30 --dry-run          # list without downloading
```

Binance publishes a day's file the next day, so `--end` defaults to yesterday
(UTC) and the most recent day or two may 404 until the archive catches up. Days
already in `./data` are skipped, so re-running to extend a range is cheap. Each
day is about 2.5 MB zipped.

`run.sh` unpacks each zip in `./data` one at a time and runs the analysis over
the extracted CSVs. Everything else is a flag:

```bash
./run.sh --window 60 --threshold 0.25
./run.sh --phase 10                       # shift the update schedule by 10s
./run.sh --phase-sweep --sweep-step 10    # try every alignment
./run.sh --csv windows.csv --json out.json
./run.sh --data-dir /some/other/folder
```

`analyze.py` works standalone too, on `.zip` or `.csv`:

```bash
./analyze.py data/BTCUSDT-1s-2026-08-20.zip --window 120
```

Python 3.9+, standard library only. No API key — `data.binance.vision` is public.

## What it measures

Two numbers per window, both relative to the price the oracle last published:

**endpoint** — `(close[t+W] - close[t]) / close[t]`
How far off the published price is by the moment of the next update.

**intra-window** — `max over the window of |high or low - close[t]| / close[t]`
The worst gap that existed at *any instant* while the price was stale. This is
the number that matters for arbitrage: a gap is exploitable the moment it
opens, not only at the window boundary. It is always the larger of the two.

For each the script reports median / p90 / p99 / max, plus how many windows
breached the threshold, as a count and as a percentage.

## Why `--phase` exists

A fixed 120-second schedule can start at any second of the minute, and where
the boundaries land changes which price moves get split across two windows and
which land inside one. That is an arbitrary implementation detail, not a
property of the market — so if the answer moves when you shift it, the answer
was never robust.

It moves a lot. On 2026-08-20, sweeping the phase across the whole window:

| phase | endpoint breaches | intra breaches | worst endpoint | worst intra |
|------:|------------------:|---------------:|---------------:|------------:|
| 0     | 7                 | 8              | 0.660%         | 0.705%      |
| 20    | 5                 | 10             | 0.720%         | 0.785%      |
| 40    | 1                 | 10             | 0.820%         | 0.820%      |
| 60    | 1                 | 6              | 0.891%         | **1.021%**  |
| 80    | 1                 | 4              | 0.691%         | 0.742%      |
| 100   | 4                 | 5              | 0.761%         | 0.868%      |

Endpoint breaches range from 1 to 7 on the same day with the same data. The
worst intra-window gap ranges from 0.71% to 1.02%. Quoting a single breach
count without saying which alignment produced it is close to meaningless — run
`--phase-sweep` and report the range.

Note the two columns move in opposite directions. An alignment that splits a
sharp move across two windows lowers the endpoint count while *raising* the
intra-window one: the gap was still there, the endpoint measurement just missed
it. Another reason to treat intra-window as the honest number.

## Results on the two sample days

120-second windows, 0.5% threshold, phase 0. 719 windows per day.

| | 2026-09-08 (calm) | 2026-08-20 (volatile) |
|---|---|---|
| day high/low range | +2.40% | +6.53% |
| day open→close | −0.83% | +5.32% |
| endpoint median | 0.033% | 0.056% |
| endpoint p99 | 0.194% | 0.419% |
| endpoint max | 0.313% | 0.660% |
| **endpoint breaches >0.5%** | **0 of 719** | **7 of 719** (0.97%) |
| intra-window max | 0.336% | 0.705% |
| **intra breaches >0.5%** | **0 of 719** | **8 of 719** (1.11%) |

On a calm day a two-minute oracle never drifts past 0.5% — not once in 719
windows, with the largest gap all day at 0.34%. On a volatile day it happens
7–8 times, clustered into a few minutes of real time (six of the eight fall
between 08:06 and 08:16 UTC), peaking at 0.70% and reaching 1.02% under a less
lucky alignment.

Worth keeping in proportion: lending and stablecoin collateral margins are
typically 5–20%, so a sub-1% transient gap is well inside them. A perpetuals
venue is a different story, and LON's RFC puts perps out of scope.

## Data format

Binance daily 1-second klines, as published at
`https://data.binance.vision/data/spot/daily/klines/<SYMBOL>/1s/`.

Headerless CSV, one row per second, 86400 rows per day:

```
open_time, open, high, low, close, volume, close_time,
quote_volume, trades, taker_buy_base, taker_buy_quote, ignore
```

`open_time` is auto-detected as seconds, milliseconds or microseconds, so
files from different eras of the archive both work.

## Caveats

- **One venue.** Binance spot only. A real oracle takes a median across
  several sources, which dampens single-venue noise — so these figures are
  closer to an upper bound on the gap than a prediction of it.
- **Two days.** The figures above are one calm and one volatile day, chosen to
  bracket the range rather than to be a distribution. For a real distribution
  run `./fetch-data.sh --days 30 && ./run.sh --phase-sweep`.
- **OHLC granularity.** The intra-window figure uses each second's high/low, so
  it catches sub-second spikes only to the extent a one-second bar records
  them. Tick data would give a slightly larger number.
- **No execution modelling.** A measured gap is an opportunity, not a profit:
  fees, slippage, gas and the size the pool can absorb all cut into it.

## License

MIT
