use std::fs;
use std::path::PathBuf;

fn main() {
    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let lock = manifest_dir.join("Cargo.lock");
    let wasm = manifest_dir.join("fib.wasm");
    println!("cargo:rerun-if-changed={}", lock.display());
    println!("cargo:rerun-if-changed={}", wasm.display());

    let text = fs::read_to_string(&lock).unwrap_or_else(|err| {
        panic!("read {}: {err}", lock.display());
    });
    let version = wasmi_version(&text).unwrap_or_else(|| {
        panic!("package wasmi not found in {}", lock.display());
    });
    println!("cargo:rustc-env=WASMI_VERSION={version}");
}

/// Version of the `wasmi` package itself, not `wasmi_core` / `wasmi_ir`.
fn wasmi_version(lock: &str) -> Option<String> {
    let mut saw_wasmi = false;
    for line in lock.lines() {
        let line = line.trim();
        if line == "name = \"wasmi\"" {
            saw_wasmi = true;
            continue;
        }
        if !saw_wasmi {
            continue;
        }
        if let Some(rest) = line.strip_prefix("version = \"") {
            if let Some(version) = rest.strip_suffix('"') {
                return Some(version.to_string());
            }
        }
        if line.starts_with("name = ") || line.starts_with("[[") {
            saw_wasmi = false;
        }
    }
    None
}
