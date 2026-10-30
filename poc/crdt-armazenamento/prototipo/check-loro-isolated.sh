#!/bin/sh
# Only bivar-poc-loja-loro may depend on Loro (ADR-0019, decision 7;
# ADR-0021), and it must use exactly the approved version.
set -e
cd "$(dirname "$0")"
for p in bivar-poc-modelo bivar-poc-cadeia bivar-poc-arquivo; do
  if cargo tree --locked -p "$p" -e normal,build --prefix none | grep -E '^loro'; then
    echo "::error::$p depends on Loro; only bivar-poc-loja-loro may"
    exit 1
  fi
done
if ! cargo tree --locked -p bivar-poc-loja-loro -e normal --prefix none | grep -qx 'loro v1.17.1'; then
  echo "::error::bivar-poc-loja-loro must use exactly loro 1.17.1"
  exit 1
fi
echo "Loro is isolated in bivar-poc-loja-loro (1.17.1)"
