#!/usr/bin/env bash
# Runs one fuzzing target in libFuzzer's fork mode (ADR-0024, section 5).
# From fuzz/:
#   run-fork.sh <target> <seconds>
#
# - Out-of-memory is the verifier's refusal family (the program refuses such
#   a file and survives): it is ignored, counted in the log, and one input
#   per family (grouped by the stack that asked) is kept in kept/<target>/.
# - A panic, any other crash, or a timeout stops the run and fails the job.
#   Their inputs are kept the same way.
#
# Time: in fork mode each child process runs for up to 300 s, and at the end
# libFuzzer waits for the children still running. <seconds> is therefore the
# budget minus 300 s (fuzz.yml).
set -u
target=$1
seconds=$2
out="$PWD/fork-out"
kept="$PWD/kept/$target"
rm -rf "$out" "$kept" "artifacts/$target"
mkdir -p "$out" "$kept" "artifacts/$target" "found/$target"

# A request the sanitizer cannot serve returns null, so the hook of
# fuzz_targets/oom_hook.rs can report it as out-of-memory.
export ASAN_OPTIONS="allocator_may_return_null=1${ASAN_OPTIONS:+:$ASAN_OPTIONS}"

# -ignore_timeouts and -ignore_ooms are on by default in fork mode, and
# -ignore_crashes is off: all three are written out so none is implicit.
start=$(date +%s)
cargo fuzz run "$target" "found/$target" "corpus/$target" -- \
  -fork=2 -ignore_ooms=1 -ignore_timeouts=0 -ignore_crashes=0 \
  -max_total_time="$seconds" -rss_limit_mb=4096 -timeout=60 -max_len=1048576 \
  2>&1 | while IFS= read -r line; do printf '%(%s)T %s\n' -1 "$line"; done > "$out/fuzz.log"
cargo_rc=${PIPESTATUS[0]}
end=$(date +%s)

# Every line of the log starts with the second it was written. The whole log
# is long when many children stop; its end and the lines that matter go to
# the job log.
tail -n 60 "$out/fuzz.log" | cut -d' ' -f2-
echo "---- lines that matter ----"
grep -E 'ERROR|panicked at|deadly signal|runtime error|wrapping up soon|INFO: exiting|Test unit written|INFO: -fork' \
  "$out/fuzz.log" | tail -n 40 | cut -d' ' -f2- | cut -c1-400

fuzzer_rc=$(grep -o 'INFO: exiting: [0-9]*' "$out/fuzz.log" | tail -1 | grep -o '[0-9]*$')
fuzzer_time=$(grep -o 'INFO: exiting: [0-9]* time: [0-9]*' "$out/fuzz.log" | tail -1 | grep -o '[0-9]*$')
counters=$(grep -o 'oom/timeout/crash: [0-9]*/[0-9]*/[0-9]*' "$out/fuzz.log" | tail -1 | grep -o '[0-9/]*$')
last_status=$(grep -E '^[0-9]+ #[0-9]+: cov: ' "$out/fuzz.log" | tail -1 | cut -d' ' -f2- | cut -c1-200)
jobs=$(grep -cE '^[0-9]+ #[0-9]+: cov: ' "$out/fuzz.log")
wrapped=$(grep -c 'wrapping up soon' "$out/fuzz.log")
t_first=$(head -n 1 "$out/fuzz.log" | cut -d' ' -f1)
t_wrap=$(grep 'wrapping up soon' "$out/fuzz.log" | tail -1 | cut -d' ' -f1)
t_exit=$(grep 'INFO: exiting' "$out/fuzz.log" | tail -1 | cut -d' ' -f1)
ooms_ignored=${counters%%/*}

# What the run left behind, by kind. Fork mode does not look at the children
# still running when the time ends, so a crash in one of them would leave an
# input here and nothing else: the files are counted too.
oom_files=$(find "artifacts/$target" -type f -name 'oom-*' | wc -l)
slow_files=$(find "artifacts/$target" -type f -name 'slow-unit-*' | wc -l)
failure_files=$(find "artifacts/$target" -type f ! -name 'oom-*' ! -name 'slow-unit-*' | wc -l)

# One input per family goes to kept/<target>/; the rest are discarded.
python3 "$(dirname "$0")/fork-families.py" "$target" "artifacts/$target" "$kept" "$out"
families_rc=$?
families=$(find "$kept" -mindepth 1 -maxdepth 1 -type d | wc -l)
kept_inputs=$(find "$kept" -type f ! -name 'replay.txt' ! -name 'families.tsv' | wc -l)
rm -rf "artifacts/$target"

# Green only if the time ran out (or libFuzzer ended by itself) with no
# crash and no timeout. Fork mode exits with the code of its last child: 71
# is an ignored out-of-memory, accepted only when the time ran out.
verdict=fail
why="libFuzzer ended with code ${fuzzer_rc:-none} (cargo: $cargo_rc)"
if [ "${fuzzer_rc:-x}" = 0 ] && [ "$cargo_rc" -eq 0 ]; then
  verdict=pass
  why="ended with code 0"
elif [ "${fuzzer_rc:-x}" = 71 ] && [ "$wrapped" -gt 0 ]; then
  verdict=pass
  why="time ran out; the last child was an ignored out-of-memory"
fi
if [ "$failure_files" -gt 0 ]; then
  verdict=fail
  why="$failure_files crash or timeout input(s) were written"
fi
if [ "$families_rc" -ne 0 ]; then
  verdict=fail
  why="the inputs could not be grouped by family (fork-families.py: $families_rc)"
fi

to_wrap=none
wrap_to_exit=none
if [ -n "$t_wrap" ] && [ -n "$t_first" ]; then
  to_wrap=$((t_wrap - t_first))
  [ -n "$t_exit" ] && wrap_to_exit=$((t_exit - t_wrap))
fi
# The families, one line each in the table: kind, inputs, where.
family_list=$(tail -n +2 "$kept/families.tsv" 2>/dev/null | awk -F'\t' '{printf "F%s %s x%s: %s || ", $1, $2, $3, $7}' | cut -c1-2500)

summary="FORK SUMMARY target=$target verdict=$verdict ($why) seconds_asked=$seconds elapsed_s=$((end - start)) fuzzer_time_s=${fuzzer_time:-none} start_to_time_up_s=$to_wrap time_up_to_exit_s=$wrap_to_exit children=$jobs ooms_ignored=${ooms_ignored:-none} oom/timeout/crash=${counters:-none} oom_files=$oom_files slow_unit_files=$slow_files crash_or_timeout_files=$failure_files families=$families inputs_kept=$kept_inputs | last status: ${last_status:-none} | families: ${family_list:-none}"
echo "$summary" | tee "$out/summary.txt"
cp "$out/summary.txt" "$kept/summary.txt"
# Readable on the run's page without opening the log.
if [ "$verdict" = pass ]; then
  echo "::notice title=fuzz $target::$summary"
  exit 0
fi
echo "::error title=fuzz $target::$summary"
exit 1
