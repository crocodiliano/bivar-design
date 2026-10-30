#!/bin/sh
# usage: inventory.sh <inv-dir>  -> TSV: crate unsafe_lines c_files build_rs c_build_deps
D="$1"
while read -r cv; do
  name=$(echo "$cv" | sed -E "s/-[0-9]+.[0-9]+.[0-9]+.*$//"); ver="${cv#"$name"-}"
  if [ -d "$D/vendor/$name-$ver" ]; then dir="$D/vendor/$name-$ver"; else dir="$D/vendor/$name"; fi
  [ -d "$dir" ] || { echo "$cv	MISSING"; continue; }
  u=$(find "$dir" -name '*.rs' -not -path '*/tests/*' -not -path '*/benches/*' -not -path '*/examples/*' -print0 | xargs -0 grep -hwE 'unsafe' 2>/dev/null | grep -vE '^\s*//' | wc -l)
  c=$(find "$dir" -type f \( -name '*.c' -o -name '*.cc' -o -name '*.cpp' -o -name '*.h' -o -name '*.hpp' -o -name '*.S' -o -name '*.s' -o -name '*.asm' \) | wc -l)
  if [ -f "$dir/build.rs" ] || grep -qE '^build *= *"' "$dir/Cargo.toml"; then b=sim; else b=nao; fi
  cb=$(awk '/^\[build-dependencies/{f=1;next} /^\[/{f=0} f' "$dir/Cargo.toml" | grep -oE '^(cc|cmake|bindgen|pkg-config|vcpkg|cxx-build)' | tr '\n' ',' )
  printf '%s\t%s\t%s\t%s\t%s\n' "$cv" "$u" "$c" "$b" "${cb:--}"
done < "$D/used.txt"
