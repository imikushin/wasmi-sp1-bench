//! Interpret one embedded Fibonacci module with Wasmi inside the SP1 guest.
//!
//! The module bytes are produced once by the host from `wasm/fib.wat`. Feature
//! flags select Wasmi's dispatch scheme; this source is not copied per flag.

#![no_main]

use wasmi::{Engine, Linker, Module, Store};

sp1_zkvm::entrypoint!(main);

const FIB_WASM: &[u8] = include_bytes!("../fib.wasm");

/// Fixed input. Ten thousand interpreter iterations, so dispatch dominates
/// module translation.
const FIB_N: i32 = 10_000;

pub fn main() {
    let result = std::hint::black_box(interpret_fib());
    // Commit the value so the interpreter work cannot be deleted, and so the
    // host can print the number Wasmi actually returned.
    sp1_zkvm::io::commit(&result);
    let version = String::from(env!("WASMI_VERSION"));
    sp1_zkvm::io::commit(&version);
}

fn interpret_fib() -> u64 {
    let engine = Engine::default();
    let module = Module::new(&engine, FIB_WASM).unwrap_or_else(|err| {
        panic!("translate wasm: {err:?}");
    });
    let mut store: Store<()> = Store::new(&engine, ());
    let linker: Linker<()> = Linker::new(&engine);
    let instance = linker
        .instantiate_and_start(&mut store, &module)
        .unwrap_or_else(|err| panic!("instantiate: {err:?}"));
    let fib = instance
        .get_typed_func::<i32, i64>(&store, "fib")
        .unwrap_or_else(|err| panic!("export fib: {err:?}"));
    let value = fib
        .call(&mut store, FIB_N)
        .unwrap_or_else(|err| panic!("call fib: {err:?}"));
    value as u64
}
