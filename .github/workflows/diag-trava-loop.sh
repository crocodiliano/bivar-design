#!/usr/bin/env bash
# Temporary (branch diag/trava-linux). Runs the compiled copias suite ($BIN)
# in a loop for 30 minutes.
# Usage: diag-trava-loop.sh <config> <stop after N failures, 0 = never> [test args]
set -u
cfg=$1
stop=$2
shift 2
dir="diag/$cfg"
mkdir -p "$dir"
end=$((SECONDS + 1800))
runs=0
fails=0
lockruns=0
while [ "$SECONDS" -lt "$end" ]; do
  runs=$((runs + 1))
  log="$PWD/$dir/diag-$runs.log"
  out="$dir/out-$runs.txt"
  if BIVAR_DIAG_LOCK_LOG="$log" "$BIN" "$@" >"$out" 2>&1; then ok=1; else ok=0; fi
  locks=$(grep -c LOCKFAIL "$log" 2>/dev/null || true)
  locks=${locks:-0}
  [ "$ok" = 0 ] && fails=$((fails + 1))
  [ "$locks" -gt 0 ] && lockruns=$((lockruns + 1))
  if [ "$ok" = 1 ] && [ "$locks" = 0 ]; then rm -f "$log" "$out"; fi
  if [ "$stop" -gt 0 ] && [ "$fails" -ge "$stop" ]; then break; fi
done
line="$cfg runs=$runs failures=$fails runs_with_refused_lock=$lockruns seconds=$SECONDS args=$*"
echo "$line" | tee -a diag/summary.txt
exit 0
