#!/usr/bin/env bash
# Runs one fuzzing target in libFuzzer's fork mode (ADR-0024, section 5).
# From fuzz/:
#   run-fork.sh <target> <seconds>
#
# 1. Seed check: every input of the starting collection (found/ and corpus/)
#    runs once, alone, in a process of its own, with the limits of the fork
#    mode. Fork mode itself skips, silently, a starting input that fails
#    (FuzzerFork.cpp, 342-351: crash-resistant merge, no artifact), so an
#    input of the cache that starts failing after a change would vanish
#    (owner, 2026-11-11). A failing input goes to artifacts/<target>/.
# 2. Fork mode, ignoring out-of-memory, crashes (panics included) and never
#    timeouts: every failure input is kept in artifacts/<target>/.
# 3. Grouping by family (fork-families.py) decides the verdict (owner,
#    2026-11-11; audit 2026-11-10, I1): known families give a warning with
#    their count and a green job; a new family, one through the code before
#    the verifier, or an input that cannot be classified turns it red.
#    Green also needs the grouping's last line to be "verdict: green".
#
# Time: in fork mode each child process runs for up to 300 s, and at the end
# libFuzzer waits for the children still running. <seconds> is therefore the
# budget minus 300 s (fuzz.yml).
set -u
target=$1
seconds=$2
out="$PWD/fork-out"
kept="$PWD/kept/$target"
bin="target/x86_64-unknown-linux-gnu/release/$target"
rm -rf "$out" "$kept" "artifacts/$target"
mkdir -p "$out" "$kept" "artifacts/$target" "found/$target"

# A request the sanitizer cannot serve returns null, so the hook of
# fuzz_targets/oom_hook.rs can report it as out-of-memory.
export ASAN_OPTIONS="allocator_may_return_null=1${ASAN_OPTIONS:+:$ASAN_OPTIONS}"

# 1. Seed check: one input at a time, at most SEED_SECONDS in all; the
# inputs left over are counted and give a warning (owner, 2026-11-11).
SEED_SECONDS=120
seed_total=0
seed_checked=0
seed_failed=0
seed_error=""
seed_start=$(date +%s)
if [ ! -x "$bin" ]; then
  seed_error="the seed check could not run: $bin is missing"
else
  for f in "found/$target"/* "corpus/$target"/*; do
    [ -f "$f" ] || continue
    seed_total=$((seed_total + 1))
    [ $(($(date +%s) - seed_start)) -ge "$SEED_SECONDS" ] && continue
    seed_checked=$((seed_checked + 1))
    timeout 90 "$bin" -rss_limit_mb=4096 -timeout=60 -artifact_prefix="artifacts/$target/" "$f" \
      > "$out/seed.txt" 2>&1
    rc=$?
    if [ "$rc" -ne 0 ]; then
      seed_failed=$((seed_failed + 1))
      { echo "== $f (code $rc)"; tail -n 80 "$out/seed.txt"; } >> "$out/seed-check.log"
      sum=$(sha1sum "$f" | cut -d' ' -f1)
      # libFuzzer names the input by its kind; if it did not write it
      # (killed by the wall clock), it is kept under a name of ours.
      if ! compgen -G "artifacts/$target/*-$sum" > /dev/null; then
        kind=crash
        [ "$rc" -eq 124 ] && kind=timeout
        cp "$f" "artifacts/$target/$kind-$sum"
      fi
    fi
  done
fi
seed_s=$(($(date +%s) - seed_start))
seed_left=$((seed_total - seed_checked))

# 2. Fork mode. All three -ignore flags are written out so none is implicit.
start=$(date +%s)
cargo fuzz run "$target" "found/$target" "corpus/$target" -- \
  -fork=2 -ignore_ooms=1 -ignore_timeouts=0 -ignore_crashes=1 \
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

oom_files=$(find "artifacts/$target" -type f -name 'oom-*' | wc -l)
slow_files=$(find "artifacts/$target" -type f -name 'slow-unit-*' | wc -l)
failure_files=$(find "artifacts/$target" -type f ! -name 'oom-*' ! -name 'slow-unit-*' | wc -l)

# 3. Grouping. At most 300 s (replays stop after 150 s, plus one replay of
# up to 90 s): a grouping that hangs is stopped (124) and reads as "could
# not be grouped".
timeout 300 python3 "$(dirname "$0")/fork-families.py" "$target" "artifacts/$target" "$kept" "$out" "$(cd "$(dirname "$0")/.." && pwd)" | tee "$out/families.txt"
families_rc=${PIPESTATUS[0]}
families_last=$(tail -n 1 "$out/families.txt")
families_verdict=$(grep '^verdict: red: ' "$out/families.txt" | tail -1 | cut -d' ' -f3-)
classes=$(grep '^classes: ' "$out/families.txt" | tail -1 | cut -d' ' -f2-)
reserve=$(grep '^reserve: ' "$out/families.txt" | tail -1 | cut -d' ' -f2-)
known_list=$(tail -n +2 "$kept/families.tsv" 2>/dev/null | awk -F'\t' '$8 == "known" {printf "F%s %s x%s: %s || ", $1, $2, $3, $7}' | cut -c1-1500)
families=$(find "$kept" -mindepth 1 -maxdepth 1 -type d ! -name reserve | wc -l)

reasons=()
grouped=no
if [ "$families_rc" -eq 0 ] && [ "$families_last" = "verdict: green" ]; then
  grouped=yes
elif [ "$families_rc" -eq 3 ] && [ -n "$families_verdict" ]; then
  grouped=yes
  reasons+=("$families_verdict")
else
  # Each red reason of its own (owner, 2026-11-11): "could not be grouped"
  # is only for a grouping that failed, hung (124), did not run, or did not
  # end with its verdict line.
  reasons+=("the inputs could not be grouped by family: fork-families.py failed, hung or did not run (code $families_rc)")
fi
# The grouping kept what must be kept (kept/<target>/reserve); if it did
# not finish, the raw inputs stay for the artifact (M6).
[ "$grouped" = yes ] && rm -rf "artifacts/$target"

# Fork mode exits with the code of its last child, so any code is accepted
# when the time ran out; before that, only 0.
if ! { [ "${fuzzer_rc:-x}" = 0 ] && [ "$cargo_rc" -eq 0 ]; } && [ "$wrapped" -eq 0 ]; then
  reasons+=("libFuzzer stopped before the time ran out, with code ${fuzzer_rc:-none} (cargo: $cargo_rc)")
fi
[ -n "$seed_error" ] && reasons+=("$seed_error")

verdict=pass
why="ended normally"
if [ "${#reasons[@]}" -gt 0 ]; then
  verdict=fail
  why=$(printf '%s; ' "${reasons[@]}")
  why=${why%; }
fi

to_wrap=none
wrap_to_exit=none
if [ -n "$t_wrap" ] && [ -n "$t_first" ]; then
  to_wrap=$((t_wrap - t_first))
  [ -n "$t_exit" ] && wrap_to_exit=$((t_exit - t_wrap))
fi
family_list=$(tail -n +2 "$kept/families.tsv" 2>/dev/null | awk -F'\t' '{printf "F%s %s x%s [%s]: %s || ", $1, $2, $3, $8, $7}' | cut -c1-2500)

summary="FORK SUMMARY target=$target verdict=$verdict ($why) seeds=$seed_total seeds_checked=$seed_checked seeds_failed=$seed_failed seeds_left_unchecked=$seed_left seed_check_s=$seed_s seconds_asked=$seconds elapsed_s=$((end - start)) fuzzer_time_s=${fuzzer_time:-none} start_to_time_up_s=$to_wrap time_up_to_exit_s=$wrap_to_exit children=$jobs oom/timeout/crash=${counters:-none} oom_files=$oom_files slow_unit_files=$slow_files crash_or_timeout_files=$failure_files families=$families classes: ${classes:-none} | reserve: ${reserve:-none} | last status: ${last_status:-none} | families: ${family_list:-none}"
echo "$summary" | tee "$out/summary.txt"
cp "$out/summary.txt" "$kept/summary.txt"
# Readable on the run's page without opening the log.
if [ "$verdict" = fail ]; then
  echo "::error title=fuzz $target::$summary"
  exit 1
fi
warn=""
[ -n "$known_list" ] && warn="known families (approved, fuzz/known-families.txt): ${known_list% || }. "
[ "$seed_left" -gt 0 ] && warn="$warn$seed_left of $seed_total starting inputs were not checked alone (limit of $SEED_SECONDS s). "
if [ -n "$warn" ]; then
  echo "::warning title=fuzz $target::$warn$summary"
else
  echo "::notice title=fuzz $target::$summary"
fi
exit 0
