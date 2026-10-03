# Wasmi dispatch cost on SP1

One SP1 guest interprets one Fibonacci function with Wasmi 2.0.0. Four builds change only the dispatch flags. The cheap build is the one with both flags on.

## The guest

Wasmi is [wasmi-labs/wasmi](https://github.com/wasmi-labs/wasmi). The module is `wasm/fib.wat`. It exports `fib`. The guest calls that export with input 10000 and commits the wrapping `i64`. The host checks the committed value against the same recurrence. Every row returned `15574651946073070043`.

The guest target is `riscv64im-succinct-zkvm-elf` on SP1 6.8.1. The release profile uses opt-level 3 and one codegen unit. `auto-dispatch` stays enabled, so a riscv64 build at that opt-level can use tail calls. `portable-dispatch` turns that path off and forces the loop backend. `indirect-dispatch` selects the indirect form of the tail-call backend.

The four rows are the four pairs of those two flags. The host does not copy the guest. `COMBOS` in `host/src/main.rs` is the list.

## Execute cycles and gas

Cycles are `ExecutionReport::total_instruction_count`. Gas is `ExecutionReport::gas` from the same execute. SP1 fills both while it replays the trace for gas. The gas figure is the raw total multiplied by 10 and divided by 191. This path does not build a proof.

| indirect_dispatch | portable_dispatch | cycles | gas |
| --- | --- | --- | --- |
| false | false | 3076284 | 4180092 |
| true | false | 1954787 | 2915908 |
| false | true | 3276309 | 3128610 |
| true | true | 1794795 | 1617526 |

Both flags on is the fewest cycles, 1794795, and the least gas, 1617526. Against the build with neither flag, that is 1281489 fewer cycles and 2562566 less gas.

The cycle order and the gas order are not the same. `portable-dispatch` alone uses the most cycles, 3276309. Neither flag spends the most gas, 4180092, and that build uses 3076284 cycles. Turning on only `portable-dispatch` adds 200025 cycles and removes 1051482 gas, relative to neither flag.

`indirect-dispatch` alone sits between those rows. It uses 1954787 cycles and 2915908 gas. Adding `portable-dispatch` on top of it removes another 159992 cycles and 1298382 gas.

## Local core proof time

The clock is only `prove().core().run()` on the local CPU prover. Each proof verified. The same four ELFs and the same Fibonacci input were used.

A default core shard keeps this program in one record. On a 15 GiB machine with no swap, that prove was killed at about 15 GiB resident set size. The times below use a smaller shard. `SP1CoreOpts.sharding_threshold.height_threshold` is 262144. `element_threshold` is 25165824, which is 96 times the row cap, the 6.8.1 default ratio. `shard_size` is the same integer, and it only reserves event-vector capacity. One core worker ran at a time.

| indirect_dispatch | portable_dispatch | prove_seconds | verified | cycles |
| --- | --- | --- | --- | --- |
| false | false | 490.273 | yes | 3076284 |
| true | false | 387.127 | yes | 1954787 |
| false | true | 307.351 | yes | 3276309 |
| true | true | 200.467 | yes | 1794795 |

Both flags on is again the cheapest row, at 200.467 seconds. Neither flag is the slowest proof, at 490.273 seconds. `portable-dispatch` alone has the most cycles and a proof of 307.351 seconds, which is faster than `indirect-dispatch` alone at 387.127 seconds.

Proof time on this machine does not follow cycle count. It does follow the pair that already won on cycles and on gas.

## The cheap pair

For this guest, enable `indirect-dispatch` and `portable-dispatch` together. That pair is the fewest cycles, the least gas, and the shortest local core proof in the table.

These proof times use shard row cap 262144. The default shard did not finish on this machine.
