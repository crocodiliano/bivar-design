#!/bin/sh
# Only bivar-poc-loja-loro may depend on Loro directly (ADR-0019, decision 7;
# ADR-0021), and it must use exactly the approved version. Every package of
# the workspace is checked, not a fixed list (audit 2026-10-31, M7): a new
# package, or the verifier, cannot add Loro without this failing. Packages
# may reach Loro through bivar-poc-loja-loro, as the program will through
# bivar-doc; only a direct dependency is refused.
set -e
cd "$(dirname "$0")"
members=$(cargo metadata --locked --no-deps --format-version 1 \
  | tr ',' '\n' | sed -n 's/^.*"name":"\(bivar-poc-[a-z-]*\)".*$/\1/p' | sort -u)
[ -n "$members" ] || { echo "::error::no workspace packages found"; exit 1; }
for p in $members; do
  direct=$(cargo tree --locked -p "$p" -e normal,build --depth 1 --prefix none \
    | sed 1d | grep -E '^loro(-[a-z-]+)? ' || true)
  if [ -n "$direct" ] && [ "$p" != bivar-poc-loja-loro ]; then
    echo "::error::$p depends on Loro directly; only bivar-poc-loja-loro may:"
    echo "$direct"
    exit 1
  fi
done
if ! cargo tree --locked -p bivar-poc-loja-loro -e normal --depth 1 --prefix none | grep -qx 'loro v1.17.1'; then
  echo "::error::bivar-poc-loja-loro must use exactly loro 1.17.1"
  exit 1
fi
echo "Loro is a direct dependency of bivar-poc-loja-loro only (1.17.1); checked:" $members
