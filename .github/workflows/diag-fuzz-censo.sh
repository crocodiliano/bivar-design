#!/usr/bin/env bash
# TEMPORARY (branch diag/fuzz-censo, never merged). From fuzz/:
#   diag-fuzz-censo.sh <seconds>
# bivar_records in fork mode; crashes, out-of-memory and timeouts are all
# ignored and counted (census only). -keep_seed=1: without it fork mode's
# first merge drops seeds that crash, and the 1.6 TiB input would never run.
set -u
seconds=$1
out="$PWD/census-out"
mkdir -p "$out" artifacts/bivar_records found/bivar_records seed-real
cp diag/crash-c6869029ad0329fc11298f77b7f2ea0fe497bf61 seed-real/
export ASAN_OPTIONS="allocator_may_return_null=1"
export BIVAR_FUZZ_OOM_LOG="$out/oom-hook.txt"
: > "$BIVAR_FUZZ_OOM_LOG"
echo "seeds: found=$(ls found/bivar_records | wc -l) corpus=$(ls corpus/bivar_records | wc -l) real=1"
start=$(date +%s)
cargo fuzz run bivar_records found/bivar_records corpus/bivar_records seed-real -- \
  -fork=2 -keep_seed=1 -ignore_crashes=1 -ignore_ooms=1 -ignore_timeouts=1 \
  -max_total_time="$seconds" -rss_limit_mb=4096 -timeout=60 -max_len=1048576 \
  > "$out/fuzz.log" 2>&1
cargo_rc=$?
end=$(date +%s)
echo "seconds_asked=$seconds elapsed_s=$((end - start)) cargo_rc=$cargo_rc" | tee "$out/time.txt"
tail -n 40 "$out/fuzz.log"
# The census never fails the job by itself: the replay step reports.
exit 0
