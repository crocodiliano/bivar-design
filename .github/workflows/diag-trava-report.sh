#!/usr/bin/env bash
# Temporary (branch diag/trava-linux). Puts the counts and the first kept
# runs into annotations, which are readable without signing in.
set -u
enc() { sed -e 's/%/%25/g' -e 's/\r/%0D/g' | awk 'BEGIN{ORS="%0A"} {print}'; }
while read -r line; do
  echo "::notice title=Counts::$line"
done < diag/summary.txt
shown=0
for cfg in A B; do
  for out in $(ls "diag/$cfg"/out-*.txt 2>/dev/null | sort -t- -k2 -n | head -4); do
    [ "$shown" -ge 7 ] && break
    n=${out##*/out-}
    n=${n%.txt}
    log="diag/$cfg/diag-$n.log"
    msg=$( { echo "run $cfg-$n"; grep -E "panicked|saving|FAILED|^test .* \.\.\. FAILED" "$out"; echo "--- diag log"; cat "$log" 2>/dev/null; } | head -c 60000 | enc)
    echo "::notice title=Run $cfg-$n::$msg"
    shown=$((shown + 1))
  done
done
exit 0
