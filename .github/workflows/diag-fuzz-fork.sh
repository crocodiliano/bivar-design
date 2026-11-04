#!/usr/bin/env bash
# TEMPORARY (branch diag/fuzz-fork, never merged). Runs one fuzzing target
# in libFuzzer's fork mode, from fuzz/:
#   diag-fuzz-fork.sh <target> <seconds> <corpus dir>...
# Out-of-memory (the verifier's refusal family) is ignored, counted and kept;
# panics, other crashes and timeouts stop the run and fail the job.
set -u
target=$1
seconds=$2
shift 2
out="$PWD/diag-out"
mkdir -p "$out" "artifacts/$target"
export ASAN_OPTIONS="allocator_may_return_null=1${ASAN_OPTIONS:+:$ASAN_OPTIONS}"
export BIVAR_FUZZ_OOM_LOG="$out/oom-hook.txt"
: > "$BIVAR_FUZZ_OOM_LOG"
echo "ASAN_OPTIONS=$ASAN_OPTIONS"
start=$(date +%s)
cargo fuzz run "$target" "$@" -- \
  -fork=2 -ignore_ooms=1 -ignore_timeouts=0 -ignore_crashes=0 \
  -max_total_time="$seconds" -rss_limit_mb=4096 -timeout=60 -max_len=1048576 \
  > "$out/fuzz.log" 2>&1
cargo_rc=$?
end=$(date +%s)
tail -n 120 "$out/fuzz.log"

# fork mode exits with the code of its last job; it is 71 (an ignored
# out-of-memory) only when that job ran out of memory just as time ran out.
fuzzer_rc=$(grep -o 'INFO: exiting: [0-9]*' "$out/fuzz.log" | tail -1 | grep -o '[0-9]*$')
counters=$(grep -o 'oom/timeout/crash: [0-9]*/[0-9]*/[0-9]*' "$out/fuzz.log" | tail -1)
oom_files=$(find "artifacts/$target" -name 'oom-*' | wc -l)
crash_files=$(find "artifacts/$target" -name 'crash-*' | wc -l)
timeout_files=$(find "artifacts/$target" -name 'timeout-*' | wc -l)
hook_lines=$(wc -l < "$BIVAR_FUZZ_OOM_LOG")
wrapped=$(grep -c 'wrapping up soon' "$out/fuzz.log")
rc=$cargo_rc
if [ "$cargo_rc" -ne 0 ] && [ "${fuzzer_rc:-x}" = 71 ] && [ "$wrapped" -gt 0 ]; then
  rc=0
fi
summary="SUMMARY target=$target seconds_asked=$seconds elapsed_s=$((end - start)) cargo_rc=$cargo_rc fuzzer_rc=${fuzzer_rc:-none} time_ran_out=$wrapped counters=[${counters:-none}] oom_files=$oom_files crash_files=$crash_files timeout_files=$timeout_files hook_lines=$hook_lines final_rc=$rc"
echo "$summary" | tee "$out/summary.txt"
# Readable without logging in (annotations); the full log is the artifact.
echo "::notice title=fork $target ${DIAG_PROBE:-}::$summary"
tail_lines=$(grep -E 'oom/timeout/crash|INFO: exiting|wrapping up|ERROR|deadly|panicked|SUMMARY|BIVAR-FUZZ|Test unit written' "$out/fuzz.log" | tail -n 12 | tr '\n' '|' | cut -c1-3800)
echo "::notice title=fork $target ${DIAG_PROBE:-} log::$tail_lines"
exit "$rc"
