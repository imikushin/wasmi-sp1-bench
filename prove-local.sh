#!/usr/bin/env bash
# Time a local CPU core proof for each already-built guest ELF.
# Stops after the first hard failure. Does not rebuild guests or call a network prover.
set -u

cd "$(dirname "$0")"

unset RUSTUP_TOOLCHAIN
export SP1_PROVER=cpu
export SP1_WORKER_NUM_CORE_WORKERS=1
export RUST_LOG="${RUST_LOG:-info}"
export CARGO_TERM_COLOR=never
export PATH="${HOME}/.sp1/bin:${PATH}"

shard_size="${1:?shard_size}"
bin="target/release/prove-one"
if [[ ! -x "$bin" ]]; then
  echo "missing $bin" >&2
  exit 1
fi

# Smallest measured cycle count first. A failure there applies to the larger programs too.
combos=(
  "ind-1-port-1 true true 1794795"
  "ind-1-port-0 true false 1954787"
  "ind-0-port-0 false false 3076284"
  "ind-0-port-1 false true 3276309"
)

tmpdir=$(mktemp -d)
trap 'rm -rf "$tmpdir"' EXIT
: >"$tmpdir/rows"

stop_reason=""
for spec in "${combos[@]}"; do
  read -r name indirect portable cycles <<<"$spec"
  elf="artifacts/${name}/wasmi-guest"
  if [[ ! -f "$elf" ]]; then
    printf '%s\t%s\t-\t-\t%s\tmissing elf %s\n' "$indirect" "$portable" "$cycles" "$elf" >>"$tmpdir/rows"
    stop_reason="missing $elf"
    break
  fi

  echo "=== prove $name cycles=$cycles shard_size=$shard_size ===" >&2
  set +e
  "$bin" "$elf" "$indirect" "$portable" "$cycles" "$shard_size" >>"$tmpdir/rows" 2> >(tee "$tmpdir/err" >/tmp/prove-shard-err.log)
  code=$?
  set -e
  cat "$tmpdir/err" >&2

  if [[ "$code" -eq 0 ]]; then
    continue
  fi

  if [[ "$code" -ge 128 ]]; then
    sig=$((code - 128))
    printf '%s\t%s\t-\t-\t%s\tprove process killed by signal %s\n' \
      "$indirect" "$portable" "$cycles" "$sig" >>"$tmpdir/rows"
    stop_reason="signal $sig while proving $name"
  elif ! grep -q $'\t'"$cycles"$'\t' "$tmpdir/rows"; then
    printf '%s\t%s\t-\t-\t%s\tprove process exited %s\n' \
      "$indirect" "$portable" "$cycles" "$code" >>"$tmpdir/rows"
    stop_reason="exit $code while proving $name"
  else
    stop_reason="prove failed for $name"
  fi
  break
done

echo
echo "prover=local-cpu mode=core sp1=6.8.1 shard_size=$shard_size height_threshold=$shard_size element_threshold=$((shard_size * 96)) core_workers=1"
echo "prove_seconds=wall clock of CpuProver::prove().core().run() only"
echo -e "indirect_dispatch\tportable_dispatch\tprove_seconds\tverified\tcycles\tstatus"
# Reprint in the original feature-matrix order.
order=("false false" "true false" "false true" "true true")
for key in "${order[@]}"; do
  line=$(awk -F '\t' -v k="$key" 'BEGIN{split(k,a," ")} $1==a[1] && $2==a[2] {print; found=1} END{if(!found) exit 1}' "$tmpdir/rows" || true)
  if [[ -n "$line" ]]; then
    printf '%s\n' "$line"
  else
    read -r indirect portable <<<"$key"
    case "$key" in
      "false false") cycles=3076284 ;;
      "true false") cycles=1954787 ;;
      "false true") cycles=3276309 ;;
      "true true") cycles=1794795 ;;
    esac
    printf '%s\t%s\t-\t-\t%s\tnot attempted: %s\n' \
      "$indirect" "$portable" "$cycles" "${stop_reason:-stopped}"
  fi
done

if [[ -n "$stop_reason" ]]; then
  echo "blocker: $stop_reason"
  exit 1
fi
