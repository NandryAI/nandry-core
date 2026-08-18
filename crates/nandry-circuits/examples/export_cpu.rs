use std::{env, fs, path::PathBuf};

use nandry_circuits::build_cpu;

fn main() {
    let output = env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("target/cpu-v0.bin"));
    let cpu = build_cpu().expect("CPU v0 circuit must build");
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent).expect("output directory must be creatable");
    }
    fs::write(&output, &cpu.bytecode).expect("CPU artifact must be writable");
    println!(
        "wrote {} bytes, {} gates ({} NAND, {} latch) to {}",
        cpu.bytecode.len(),
        cpu.gate_count,
        cpu.nand_count,
        cpu.latch_count,
        output.display()
    );
}
