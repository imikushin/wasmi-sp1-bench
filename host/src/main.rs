//! Build one Wasmi guest four ways and record SP1 execute cycle counts.
//!
//! The feature matrix below is the only list of combinations. Each row is a
//! separate `cargo prove build` of `guest/` with its own target directory.

use std::fs;
use std::fmt::Write as _;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;

use sp1_sdk::blocking::{Elf, Prover, ProverClient, SP1Stdin};

/// Iterations baked into the guest. Must match `FIB_N` in the guest.
const FIB_N: u32 = 10_000;

struct Combo {
    indirect: bool,
    portable: bool,
    /// Extra Cargo features on top of the guest default (`auto-dispatch`).
    features: &'static [&'static str],
}

/// The four legal pairs of `(indirect-dispatch, portable-dispatch)`.
const COMBOS: &[Combo] = &[
    Combo {
        indirect: false,
        portable: false,
        features: &[],
    },
    Combo {
        indirect: true,
        portable: false,
        features: &["indirect-dispatch"],
    },
    Combo {
        indirect: false,
        portable: true,
        features: &["portable-dispatch"],
    },
    Combo {
        indirect: true,
        portable: true,
        features: &["indirect-dispatch", "portable-dispatch"],
    },
];

struct Row {
    indirect: bool,
    portable: bool,
    status: String,
    cycles: Option<u64>,
    /// `ExecutionReport::gas`, the normalized SP1 gas. `None` when the execute
    /// did not fill it.
    gas: Option<u64>,
    fib_result: Option<u64>,
    wasmi_version: String,
}

fn main() {
    let root = repo_root();
    // Existing ELFs already embed the module. Skip the guest rebuild.
    let execute_only = std::env::args().any(|arg| arg == "--execute-only");
    if !execute_only {
        compile_wasm_once(&root);
        generate_guest_lockfile(&root);
    }
    let reference = wrapping_fib(FIB_N);
    // CPU executor. In SP1 6.8.1 the opcode counts on ExecutionReport are filled
    // only while the gas replay walks the trace, so leave calculate_gas on.
    // That same replay is what fills ExecutionReport::gas.
    let client = ProverClient::builder().cpu().build();

    let mut rows = Vec::with_capacity(COMBOS.len());
    for combo in COMBOS {
        rows.push(run_combo(&root, &client, combo, reference, execute_only));
    }

    let table = render_table(&rows, reference);
    println!("\n{table}");
    let out = root.join("artifacts/cycles.txt");
    if let Some(parent) = out.parent() {
        fs::create_dir_all(parent).ok();
    }
    fs::write(&out, &table).unwrap_or_else(|err| {
        eprintln!("failed to write {}: {err}", out.display());
    });

    if rows.iter().any(|row| row.status != "ok") {
        std::process::exit(1);
    }
}

fn repo_root() -> PathBuf {
    let cwd = std::env::current_dir().expect("current dir");
    if cwd.join("guest/Cargo.toml").is_file() && cwd.join("wasm/fib.wat").is_file() {
        return cwd;
    }
    panic!("run from the repository root so guest/ and wasm/fib.wat resolve");
}

fn compile_wasm_once(root: &Path) -> PathBuf {
    let wat_path = root.join("wasm/fib.wat");
    let wasm_path = root.join("guest/fib.wasm");
    let bytes = wat::parse_file(&wat_path).unwrap_or_else(|err| {
        panic!("compile {}: {err}", wat_path.display());
    });
    if bytes.get(..4) != Some(b"\0asm") {
        panic!("wat compiler did not emit a Wasm module");
    }
    fs::write(&wasm_path, &bytes).unwrap_or_else(|err| {
        panic!("write {}: {err}", wasm_path.display());
    });
    println!(
        "compiled {} once ({} bytes) -> {}",
        wat_path.display(),
        bytes.len(),
        wasm_path.display()
    );
    wasm_path
}

fn generate_guest_lockfile(root: &Path) {
    let status = Command::new("cargo")
        .arg("generate-lockfile")
        .arg("--manifest-path")
        .arg(root.join("guest/Cargo.toml"))
        .status()
        .unwrap_or_else(|err| panic!("spawn cargo generate-lockfile: {err}"));
    if !status.success() {
        panic!("cargo generate-lockfile failed for the guest");
    }
}

fn run_combo(
    root: &Path,
    client: &impl Prover,
    combo: &Combo,
    reference: u64,
    execute_only: bool,
) -> Row {
    let name = combo_name(combo);
    let artifact_dir = root.join("artifacts").join(&name);
    let elf_path = if execute_only {
        println!("\n=== execute existing {name} ===");
        let elf = artifact_dir.join("wasmi-guest");
        if elf.is_file() {
            Ok(elf)
        } else {
            Err(format!("missing elf {}", elf.display()))
        }
    } else {
        println!(
            "\n=== build {name} features=[{}] ===",
            combo.features.join(",")
        );
        let target_dir = root.join("target").join("guest").join(&name);
        fs::create_dir_all(&artifact_dir).expect("artifact dir");
        build_guest(root, combo, &target_dir, &artifact_dir)
    };

    match elf_path {
        Err(status) => Row {
            indirect: combo.indirect,
            portable: combo.portable,
            status,
            cycles: None,
            gas: None,
            fib_result: None,
            wasmi_version: lockfile_wasmi_version(root).unwrap_or_else(|| "-".to_string()),
        },
        Ok(elf_path) => {
            println!("=== execute {name} ===");
            match execute_elf(client, &elf_path) {
                Ok((cycles, gas, fib, version)) => {
                    let status = if cycles == 0 {
                        "execute returned total_instruction_count 0".to_string()
                    } else if gas.is_none() {
                        "execute returned gas None".to_string()
                    } else if fib == reference {
                        "ok".to_string()
                    } else {
                        format!(
                            "result mismatch: guest returned {fib}, wrapping fib({FIB_N}) is {reference}"
                        )
                    };
                    Row {
                        indirect: combo.indirect,
                        portable: combo.portable,
                        status,
                        cycles: Some(cycles),
                        gas,
                        fib_result: Some(fib),
                        wasmi_version: version,
                    }
                }
                Err(status) => Row {
                    indirect: combo.indirect,
                    portable: combo.portable,
                    status,
                    cycles: None,
                    gas: None,
                    fib_result: None,
                    wasmi_version: lockfile_wasmi_version(root).unwrap_or_else(|| "-".to_string()),
                },
            }
        }
    }
}

fn build_guest(
    root: &Path,
    combo: &Combo,
    target_dir: &Path,
    artifact_dir: &Path,
) -> Result<PathBuf, String> {
    let mut cmd = Command::new("cargo");
    cmd.arg("prove")
        .arg("build")
        .arg("--locked")
        .arg("--output-directory")
        .arg(artifact_dir)
        .arg("--elf-name")
        .arg("wasmi-guest")
        .current_dir(root.join("guest"))
        // cargo prove derives its elf-compilation dir from this, so each
        // feature pair gets a private target directory.
        .env("CARGO_TARGET_DIR", target_dir)
        // Drop a stale toolchain pin. `cargo prove` injects the succinct rustc.
        .env_remove("RUSTUP_TOOLCHAIN")
        .env_remove("RUSTFLAGS")
        .env_remove("CARGO_ENCODED_RUSTFLAGS");
    if !combo.features.is_empty() {
        cmd.arg("--features").arg(combo.features.join(","));
    }

    let log_path = artifact_dir.join("build.log");
    let log = run_and_capture(&mut cmd).map_err(|err| format!("build failed: {err}"))?;
    fs::write(&log_path, &log).ok();
    if !log_succeeded(&log) {
        return Err(format!("build failed: {}", summarize(&log)));
    }

    let elf = artifact_dir.join("wasmi-guest");
    if !elf.is_file() {
        return Err(format!(
            "build failed: cargo prove exited without writing {}",
            elf.display()
        ));
    }
    Ok(elf)
}

/// `cargo prove` calls `std::process::exit` on failure, so a non-zero status is
/// the failure signal. Captured logs also include rustc errors.
fn log_succeeded(log: &str) -> bool {
    // `run_and_capture` prefixes a non-zero exit with this marker line.
    !log.lines().any(|line| line.starts_with("COMMAND_EXIT:"))
}

fn execute_elf(
    client: &impl Prover,
    elf_path: &Path,
) -> Result<(u64, Option<u64>, u64, String), String> {
    let bytes = fs::read(elf_path).map_err(|err| format!("execute failed: read elf: {err}"))?;
    let (mut public_values, report) = client
        .execute(Elf::from(bytes), SP1Stdin::new())
        .run()
        .map_err(|err| format!("execute failed: {err}"))?;
    let fib: u64 = public_values.read();
    let version: String = public_values.read();
    let cycles = report.total_instruction_count();
    // Normalized gas: raw * 10 / 191. None only when calculate_gas is off.
    let gas = report.gas();
    println!("fib_result: {fib}");
    println!("wasmi_version: {version}");
    println!("total_instruction_count: {cycles}");
    match gas {
        Some(gas) => println!("gas: {gas}"),
        None => println!("gas: none"),
    }
    Ok((cycles, gas, fib, version))
}

fn run_and_capture(cmd: &mut Command) -> Result<String, String> {
    let mut child = cmd
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|err| err.to_string())?;
    let stdout = child.stdout.take().expect("stdout");
    let stderr = child.stderr.take().expect("stderr");

    let out_handle = thread::spawn(move || {
        let mut buf = String::new();
        let reader = BufReader::new(stdout);
        for line in reader.lines() {
            let line = line.unwrap_or_default();
            println!("{line}");
            buf.push_str(&line);
            buf.push('\n');
        }
        buf
    });
    let err_handle = thread::spawn(move || {
        let mut buf = String::new();
        let reader = BufReader::new(stderr);
        for line in reader.lines() {
            let line = line.unwrap_or_default();
            eprintln!("{line}");
            buf.push_str(&line);
            buf.push('\n');
        }
        buf
    });

    let status = child.wait().map_err(|err| err.to_string())?;
    let mut log = out_handle.join().unwrap_or_default();
    log.push_str(&err_handle.join().unwrap_or_default());
    if !status.success() {
        let code = status.code().unwrap_or(-1);
        log.push_str(&format!("COMMAND_EXIT: {code}\n"));
    }
    Ok(log)
}

fn summarize(log: &str) -> String {
    let lines: Vec<&str> = log
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    let errors: Vec<&str> = lines
        .iter()
        .copied()
        .filter(|line| {
            let lower = line.to_ascii_lowercase();
            lower.contains("error") || lower.starts_with("command_exit")
        })
        .collect();
    let chosen: &[&str] = if errors.is_empty() { &lines } else { &errors };
    let start = chosen.len().saturating_sub(4);
    let text = chosen[start..].join(" | ");
    if text.chars().count() > 500 {
        let tail: String = text.chars().rev().take(500).collect();
        tail.chars().rev().collect()
    } else if text.is_empty() {
        "no compiler output".to_string()
    } else {
        text
    }
}

fn lockfile_wasmi_version(root: &Path) -> Option<String> {
    let text = fs::read_to_string(root.join("guest/Cargo.lock")).ok()?;
    let mut saw = false;
    for line in text.lines() {
        let line = line.trim();
        if line == "name = \"wasmi\"" {
            saw = true;
            continue;
        }
        if !saw {
            continue;
        }
        if let Some(rest) = line.strip_prefix("version = \"") {
            return rest.strip_suffix('"').map(str::to_string);
        }
        if line.starts_with("name = ") || line.starts_with("[[") {
            saw = false;
        }
    }
    None
}

fn wrapping_fib(n: u32) -> u64 {
    let mut a: u64 = 0;
    let mut b: u64 = 1;
    for _ in 0..n {
        let sum = a.wrapping_add(b);
        a = b;
        b = sum;
    }
    a
}

fn combo_name(combo: &Combo) -> String {
    format!(
        "ind-{}-port-{}",
        u8::from(combo.indirect),
        u8::from(combo.portable)
    )
}

fn render_table(rows: &[Row], reference: u64) -> String {
    let mut out = String::new();
    let _ = writeln!(
        out,
        "fib_n={FIB_N} reference_fib={reference} sp1=6.8.1 target=riscv64im-succinct-zkvm-elf"
    );
    let _ = writeln!(
        out,
        "cycles=ExecutionReport::total_instruction_count from ProverClient::execute (no proof)"
    );
    let _ = writeln!(
        out,
        "gas=ExecutionReport::gas from the same execute (normalized: raw * 10 / 191)"
    );
    let _ = writeln!(
        out,
        "indirect_dispatch\tportable_dispatch\tstatus\tcycles\tgas\tfib_result\twasmi_version"
    );
    for row in rows {
        let _ = writeln!(
            out,
            "{}\t{}\t{}\t{}\t{}\t{}\t{}",
            row.indirect,
            row.portable,
            row.status.replace(['\n', '\t'], " "),
            row.cycles
                .map(|n| n.to_string())
                .unwrap_or_else(|| "-".to_string()),
            row.gas
                .map(|n| n.to_string())
                .unwrap_or_else(|| "-".to_string()),
            row.fib_result
                .map(|n| n.to_string())
                .unwrap_or_else(|| "-".to_string()),
            row.wasmi_version
        );
    }
    let best = rows.iter().filter_map(|row| {
        let cycles = row.cycles?;
        Some((cycles, row))
    });
    if let Some((cycles, row)) = best.min_by_key(|(cycles, _)| *cycles) {
        let _ = writeln!(
            out,
            "fewest_cycles\tindirect_dispatch={}\tportable_dispatch={}\tcycles={cycles}",
            row.indirect, row.portable
        );
    }
    out
}
