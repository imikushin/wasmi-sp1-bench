# Wasmi v2 dispatch cycle benchmark on SP1

Measures which Wasmi 2.0.0 feature pair uses the fewest SP1 instruction cycles to interpret one Fibonacci WebAssembly module. The host executes the guest and reads `ExecutionReport::total_instruction_count`. It does not generate a proof.

## Run the benchmark

You need `protoc`, `g++`, and the SP1 6.8.1 toolchain. `host/rust-toolchain.toml` and `guest/rust-toolchain.toml` pin channel `1.96`. The workspace `rust-toolchain.toml` has the same pin, because `./bench.sh` starts `cargo` at the repo root and rustup does not look inside `host/`. SP1 6.8.1's succinct compiler is rustc 1.96.0-dev. `cargo prove` compiles the guest with that succinct compiler, because only it provides `riscv64im-succinct-zkvm-elf`. `cargo prove build` defaults to that target. The 6.8.1 toolchain still contains a riscv32 target. This bench uses the default riscv64 execute path. `bench.sh` points `CXX` at `g++` because `sp1-core-machine` compiles a C++ file.

```bash
curl -L https://sp1up.succinct.xyz | bash
sp1up
./bench.sh
```

`./bench.sh` is the command to rerun. It rebuilds all four guests and prints a tab-separated table. The same text is written to `artifacts/cycles.txt`.

The columns are `indirect_dispatch`, `portable_dispatch`, `status`, `cycles`, `gas`, `fib_result`, and `wasmi_version`. `gas` is `ExecutionReport::gas` from that same execute. SP1 normalizes it as raw gas times 10 divided by 191. Pass `--execute-only` to skip the guest rebuild and execute the ELFs already in `artifacts/`.

`status` is `ok`, or the build or execute error. `cycles` is `ExecutionReport::total_instruction_count` from `ProverClient::execute`. SP1 6.8.1 fills those opcode counts while it replays the trace for gas, so the bench leaves that replay on. The cycle command does not generate a proof. A failed build or execute is one row. The other rows still run. The process exits 1 if any row is not `ok`.

## Local core prove

`./prove-local.sh <shard_size>` times a local CPU core proof of the four ELFs already written under `artifacts/`. It does not rebuild guests and it does not call a network prover. `shard_size` is `SP1CoreOpts::sharding_threshold.height_threshold`. The same integer is `SP1CoreOpts::shard_size`. `element_threshold` is `shard_size * 96`, the 6.8.1 default ratio. One core worker runs at a time. The clock covers `prove().core().run()` only.

## Feature matrix

A feature set is the pair `indirect-dispatch` and `portable-dispatch`. Those are the published names in the wasmi 2.0.0 crate. They match the requested `indirect_dispatch` and `portable_dispatch` flags. All four pairs are built from the single `guest/` crate. The list is `COMBOS` in `host/src/main.rs`.

`auto-dispatch` stays enabled on every build. That is Wasmi's default. On riscv64 at `opt-level` 3 it selects tail-call dispatch. Enabling `portable-dispatch` overrides it and forces the loop backend. The guest release profile sets `opt-level = 3` and `codegen-units = 1` so the tail-call path is emitted.

Each build uses its own `CARGO_TARGET_DIR` under `target/guest/ind-<0|1>-port-<0|1>`. `cargo prove` appends `elf-compilation` under that directory.

## Program

`wasm/fib.wat` is an iterative Fibonacci function. The bench compiles it once with `wat` 1.228.0 and embeds `guest/fib.wasm`. The guest calls `fib(10000)` and commits the wrapping `i64` result plus the Wasmi version from the guest lockfile. The host prints that result and checks it against the same wrapping recurrence.
