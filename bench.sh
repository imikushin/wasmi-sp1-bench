#!/usr/bin/env bash
# Rebuild all four Wasmi dispatch combinations and print the SP1 cycle table.
# This is the command a reviewer reruns. It does not generate a proof.
set -euo pipefail

cd "$(dirname "$0")"

unset RUSTUP_TOOLCHAIN
export PATH="${HOME}/.sp1/bin:${PATH}"
export SP1_PROVER=cpu
export CARGO_TERM_COLOR=never
export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-2}"
# sp1-core-machine builds a small C++ file. Clang on this image does not
# see libstdc++ headers; g++ does.
export CC="${CC:-gcc}"
export CXX="${CXX:-g++}"

if ! command -v cargo-prove >/dev/null 2>&1; then
  echo "cargo-prove is not on PATH. Install the current SP1 toolchain:" >&2
  echo "  curl -L https://sp1up.succinct.xyz | bash && sp1up" >&2
  exit 1
fi

exec cargo run --release -p host --bin wasmi-sp1-bench
