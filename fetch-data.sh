#!/usr/bin/env bash
#
# Download Binance 1-second klines so anyone can reproduce the numbers.
#
#   ./fetch-data.sh                              # BTCUSDT, the two days in the writeup
#   ./fetch-data.sh --days 10                    # last 10 available days
#   ./fetch-data.sh --days 10 --end 2026-08-25   # 10 days ending 2026-08-25
#   ./fetch-data.sh --symbol ETHUSDT --days 5
#   ./fetch-data.sh 2026-08-20 2026-09-08        # explicit dates
#   ./fetch-data.sh --days 30 --dry-run          # just list what it would fetch
#
# Files land in ./data/ and are left zipped; run.sh unpacks them.
# Source: https://data.binance.vision/  (public, no API key)
#
# Note: Binance publishes a day's file the following day, so --end defaults to
# yesterday (UTC). Very recent dates may 404 until the archive catches up.

set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DATA_DIR="$HERE/data"
BASE="https://data.binance.vision/data/spot/daily/klines"

SYMBOL="BTCUSDT"
DAYS=""
END=""
DRY=0
EXPLICIT=()

# ---- portable date arithmetic (GNU coreutils and BSD/macOS) ---------------
if date -u -d "2020-01-01 -1 day" +%F >/dev/null 2>&1; then
  DATE_FLAVOUR="gnu"
elif date -u -v-1d -j -f "%Y-%m-%d" "2020-01-01" +%F >/dev/null 2>&1; then
  DATE_FLAVOUR="bsd"
else
  echo "error: cannot work out how to do date arithmetic with your 'date'" >&2
  exit 1
fi

date_minus() {     # date_minus YYYY-MM-DD N  ->  YYYY-MM-DD
  local d="$1" n="$2"
  if [[ "$DATE_FLAVOUR" == "gnu" ]]; then
    date -u -d "$d -$n day" +%F
  else
    date -u -v-"${n}"d -j -f "%Y-%m-%d" "$d" +%F
  fi
}

today_utc() { date -u +%F; }

is_valid_date() {
  if [[ "$DATE_FLAVOUR" == "gnu" ]]; then
    date -u -d "$1" +%F >/dev/null 2>&1
  else
    date -u -j -f "%Y-%m-%d" "$1" +%F >/dev/null 2>&1
  fi
}

# ---- args ----------------------------------------------------------------
while [[ $# -gt 0 ]]; do
  case "$1" in
    --days)    DAYS="$2"; shift 2 ;;
    --end)     END="$2"; shift 2 ;;
    --symbol)  SYMBOL="$2"; shift 2 ;;
    --data-dir) DATA_DIR="$2"; shift 2 ;;
    --dry-run) DRY=1; shift ;;
    -h|--help) sed -n '2,19p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'; exit 0 ;;
    -*) echo "error: unknown flag $1" >&2; exit 1 ;;
    *)
      # a bare token is either a symbol (first, if it isn't a date) or a date
      if [[ "$1" =~ ^[0-9]{4}-[0-9]{2}-[0-9]{2}$ ]]; then
        EXPLICIT+=("$1")
      elif [[ ${#EXPLICIT[@]} -eq 0 && -z "$DAYS" ]]; then
        SYMBOL="$1"
      else
        echo "error: unexpected argument '$1'" >&2; exit 1
      fi
      shift ;;
  esac
done

# ---- build the day list --------------------------------------------------
DATES=()

if [[ -n "$DAYS" ]]; then
  if ! [[ "$DAYS" =~ ^[0-9]+$ ]] || [[ "$DAYS" -lt 1 ]]; then
    echo "error: --days must be a positive integer" >&2; exit 1
  fi
  if [[ -z "$END" ]]; then
    END="$(date_minus "$(today_utc)" 1)"      # yesterday UTC
  fi
  if ! is_valid_date "$END"; then
    echo "error: --end is not a valid date: $END" >&2; exit 1
  fi
  for ((i = DAYS - 1; i >= 0; i--)); do
    DATES+=("$(date_minus "$END" "$i")")
  done
elif [[ ${#EXPLICIT[@]} -gt 0 ]]; then
  DATES=("${EXPLICIT[@]}")
else
  DATES=(2026-08-20 2026-09-08)               # the two days in the README
fi

echo "symbol   $SYMBOL"
echo "days     ${#DATES[@]}  (${DATES[0]} .. ${DATES[${#DATES[@]}-1]})"
echo "dest     $DATA_DIR"
echo

if [[ $DRY -eq 1 ]]; then
  for d in "${DATES[@]}"; do
    echo "  would fetch  ${SYMBOL}-1s-${d}.zip"
  done
  exit 0
fi

mkdir -p "$DATA_DIR"

ok=0; skipped=0; failed=0
FAILED_DATES=()

for day in "${DATES[@]}"; do
  name="${SYMBOL}-1s-${day}.zip"
  out="$DATA_DIR/$name"
  if [[ -s "$out" ]]; then
    echo "have     $name"
    skipped=$((skipped + 1))
    continue
  fi
  url="$BASE/$SYMBOL/1s/$name"
  printf 'fetch    %s ... ' "$name"
  if curl -fsSL -o "$out" "$url"; then
    echo "ok ($(du -h "$out" | cut -f1))"
    ok=$((ok + 1))
  else
    rm -f "$out"
    echo "FAILED"
    failed=$((failed + 1))
    FAILED_DATES+=("$day")
  fi
done

echo
echo "downloaded $ok, already had $skipped, failed $failed"

if [[ $failed -gt 0 ]]; then
  echo
  echo "failed: ${FAILED_DATES[*]}"
  echo "  1s klines exist only for dates Binance has published, and the most"
  echo "  recent day or two often lag. Check the symbol spelling too."
fi

echo
ls -la "$DATA_DIR" | grep -v '^total' | grep -v '\.gitkeep'
