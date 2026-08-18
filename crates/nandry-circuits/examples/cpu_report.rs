use std::time::Instant;

use nandry_circuits::{build_cpu, programs, CpuMachine};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cpu = build_cpu()?;
    println!("NANDRY Core 8-bit CPU");
    println!("  NAND gates : {}", cpu.nand_count);
    println!("  LATCH gates: {}", cpu.latch_count);
    println!("  total gates: {}", cpu.gate_count);
    println!("  bytecode   : {} bytes", cpu.bytecode.len());

    let started = Instant::now();
    let mut fibonacci = CpuMachine::new(&cpu.bytecode)?;
    let fib = fibonacci.run_until_outputs(&programs::fibonacci(), 12, 100)?;
    println!("  Fibonacci  : {:?} ({} ticks)", fib, fibonacci.ticks());

    let mut multiply = CpuMachine::new(&cpu.bytecode)?;
    let product = multiply.run_until_halt(&programs::multiplication(13, 17), 200)?;
    println!("  13 * 17   : {:?} ({} ticks)", product, multiply.ticks());

    let mut gcd = CpuMachine::new(&cpu.bytecode)?;
    let divisor = gcd.run_until_halt(&programs::gcd(84, 30), 200)?;
    println!("  gcd(84,30): {:?} ({} ticks)", divisor, gcd.ticks());
    println!("  elapsed    : {:?}", started.elapsed());
    Ok(())
}
