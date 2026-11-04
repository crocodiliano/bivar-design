#!/usr/bin/env bash
# Temporary (branch diag/trava-linux). Runs the compiled copias suite ($BIN)
# alternating A (as is) and C (BIVAR_DIAG_SERIALIZE=1: no verifier launch
# while a file lock is asked for), until each has 500 runs or 30 minutes
# have passed. Never stops early on a failure.
set -u
mkdir -p diag/A diag/C
echo "config;run;exit;refusals;seconds;tests" > diag/runs.csv
end=$((SECONDS + 1800))
declare -A n=([A]=0 [C]=0)
while [ "$SECONDS" -lt "$end" ] && { [ "${n[A]}" -lt 500 ] || [ "${n[C]}" -lt 500 ]; }; do
  for cfg in A C; do
    [ "${n[$cfg]}" -ge 500 ] && continue
    n[$cfg]=$((n[$cfg] + 1))
    i=${n[$cfg]}
    log="$PWD/diag/$cfg/diag-$i.log"
    out="diag/$cfg/out-$i.txt"
    start=$(date +%s.%N)
    if [ "$cfg" = C ]; then
      BIVAR_DIAG_SERIALIZE=1 BIVAR_DIAG_LOCK_LOG="$log" "$BIN" >"$out" 2>&1
    else
      BIVAR_DIAG_LOCK_LOG="$log" "$BIN" >"$out" 2>&1
    fi
    code=$?
    secs=$(awk -v a="$start" -v b="$(date +%s.%N)" 'BEGIN {printf "%.3f", b - a}')
    refusals=$(grep -c "LOCKFAIL path=" "$log" 2>/dev/null || true)
    refusals=${refusals:-0}
    tests=$(grep -oE '^test [a-z_]+ \.\.\.' "$out" | awk '{print $2}' | sort | tr '\n' ',')
    echo "$cfg;$i;$code;$refusals;$secs;$tests" >> diag/runs.csv
    if [ "$code" = 0 ] && [ "$refusals" = 0 ]; then rm -f "$log" "$out"; fi
  done
done
for cfg in A C; do
  awk -F';' -v c="$cfg" '$1==c {runs++; if ($3!=0) fails++; ref+=$4; if ($4>0) rr++; t+=$5}
    END {printf "%s runs=%d failures=%d refusals=%d runs_with_refusal=%d seconds_in_runs=%.0f\n", c, runs, fails, ref, rr, t}' diag/runs.csv | tee -a diag/summary.txt
  echo "$cfg test lists: $(awk -F';' -v c="$cfg" '$1==c {print $6}' diag/runs.csv | sort | uniq -c | tr '\n' ' ')" | tee -a diag/summary.txt
done
echo "loop seconds=$SECONDS" | tee -a diag/summary.txt
exit 0
