#!/usr/bin/env bash
#
# Unpack every Binance 1s-kline zip in a folder and run the staleness analysis
# over the extracted CSVs.
#
#   ./run.sh                          # ./data, 120s windows, 0.5% threshold
#   ./run.sh --phase 10               # shift the update schedule by 10s
#   ./run.sh --window 60 --threshold 0.25
#   ./run.sh --phase-sweep            # try every alignment
#   ./run.sh --data-dir /some/where --csv windows.csv
#
# Any flag not listed here is passed straight through to analyze.py.

set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DATA_DIR="$HERE/data"
KEEP=0
PASSTHRU=()

while [[ $# -gt 0 ]]; do
  case "$1" in
    --data-dir) DATA_DIR="$2"; shift 2 ;;
    --keep)     KEEP=1; shift ;;          # leave the extracted CSVs on disk
    -h|--help)
      sed -n '2,15p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'
      echo
      python3 "$HERE/analyze.py" --help
      exit 0 ;;
    *) PASSTHRU+=("$1"); shift ;;
  esac
done

if [[ ! -d "$DATA_DIR" ]]; then
  echo "error: data directory not found: $DATA_DIR" >&2
  echo "hint:  ./fetch-data.sh BTCUSDT 2026-08-20 2026-09-08" >&2
  exit 1
fi

shopt -s nullglob
ZIPS=("$DATA_DIR"/*.zip)
LOOSE_CSVS=("$DATA_DIR"/*.csv)
shopt -u nullglob

if [[ ${#ZIPS[@]} -eq 0 && ${#LOOSE_CSVS[@]} -eq 0 ]]; then
  echo "error: no .zip or .csv files in $DATA_DIR" >&2
  exit 1
fi

WORK="$(mktemp -d)"
if [[ $KEEP -eq 0 ]]; then
  trap 'rm -rf "$WORK"' EXIT
else
  echo "keeping extracted files in $WORK"
fi

CSVS=()

# unzip one at a time so a single corrupt archive doesn't kill the run
for z in "${ZIPS[@]}"; do
  base="$(basename "$z" .zip)"
  dest="$WORK/$base"
  mkdir -p "$dest"
  if ! unzip -q -o "$z" -d "$dest" 2>/dev/null; then
    echo "warn: could not unzip $(basename "$z"), skipping" >&2
    continue
  fi
  found=0
  while IFS= read -r -d '' csv; do
    CSVS+=("$csv")
    found=1
  done < <(find "$dest" -name '*.csv' -print0)
  [[ $found -eq 0 ]] && echo "warn: no CSV inside $(basename "$z")" >&2
done

for c in "${LOOSE_CSVS[@]}"; do
  CSVS+=("$c")
done

if [[ ${#CSVS[@]} -eq 0 ]]; then
  echo "error: nothing to analyse" >&2
  exit 1
fi

# deterministic order so reruns are comparable
IFS=$'\n' CSVS=($(sort <<<"${CSVS[*]}")); unset IFS

echo "analysing ${#CSVS[@]} file(s) from $DATA_DIR"

python3 "$HERE/analyze.py" "${CSVS[@]}" ${PASSTHRU[@]+"${PASSTHRU[@]}"}
