//! Time one local CPU core proof of an already-built guest ELF.
//!
//! The process proves a single ELF so a crash leaves the other rows for the driver.

use std::env;
use std::fs;
use std::process::ExitCode;
use std::time::Instant;

use sp1_core_executor::SP1CoreOpts;
use sp1_sdk::blocking::{Elf, ProveRequest, Prover, ProverClient, SP1Stdin};
use sp1_sdk::ProvingKey;

fn main() -> ExitCode {
    let mut args = env::args().skip(1);
    let elf_path = args.next().expect("elf path");
    let indirect = args.next().expect("indirect_dispatch");
    let portable = args.next().expect("portable_dispatch");
    let cycles = args.next().expect("cycles");
    let shard_size: usize = args
        .next()
        .expect("shard_size")
        .parse()
        .expect("shard_size integer");

    // `shard_size` only reserves event-vec capacity (`shard_size >> 3`). A core
    // shard ends when its tallest chip reaches `height_threshold` rows or its
    // trace area reaches `element_threshold`. SP1 6.8.1 defaults those to
    // 1 << 22 and 96 times that. This program is under the default row cap, so
    // the whole trace was one shard. The same integer is the row cap, and the
    // area cap keeps the published 96× ratio.
    let mut opts = SP1CoreOpts::default();
    let height_threshold = shard_size as u64;
    let element_threshold = height_threshold.saturating_mul(96);
    opts.shard_size = shard_size;
    opts.sharding_threshold.height_threshold = height_threshold;
    opts.sharding_threshold.element_threshold = element_threshold;
    eprintln!(
        "initializing local cpu prover shard_size={shard_size} height_threshold={height_threshold} element_threshold={element_threshold}"
    );
    sp1_sdk::utils::setup_logger();
    let client = ProverClient::builder().cpu().core_opts(opts).build();

    let bytes = match fs::read(&elf_path) {
        Ok(bytes) => bytes,
        Err(err) => {
            println!("{indirect}\t{portable}\t-\t-\t{cycles}\tread elf: {err}");
            return ExitCode::from(2);
        }
    };

    eprintln!("setup {elf_path}");
    let pk = match client.setup(Elf::from(bytes)) {
        Ok(pk) => pk,
        Err(err) => {
            println!("{indirect}\t{portable}\t-\t-\t{cycles}\tsetup failed: {err}");
            return ExitCode::from(2);
        }
    };

    eprintln!("prove {elf_path}");
    let started = Instant::now();
    let proof = match client.prove(&pk, SP1Stdin::new()).core().run() {
        Ok(proof) => proof,
        Err(err) => {
            let secs = started.elapsed().as_secs_f64();
            println!("{indirect}\t{portable}\t{secs:.3}\t-\t{cycles}\tprove failed: {err}");
            return ExitCode::from(2);
        }
    };
    let secs = started.elapsed().as_secs_f64();

    match client.verify(&proof, pk.verifying_key(), None) {
        Ok(()) => {
            println!("{indirect}\t{portable}\t{secs:.3}\tyes\t{cycles}\tok");
            ExitCode::SUCCESS
        }
        Err(err) => {
            println!("{indirect}\t{portable}\t{secs:.3}\tno\t{cycles}\tverify failed: {err}");
            ExitCode::from(2)
        }
    }
}
