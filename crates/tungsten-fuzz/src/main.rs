use std::env;
use std::time::Instant;

use tungsten_fuzz::allocator_fuzz;
use tungsten_fuzz::generator::ProgramGenerator;
use tungsten_fuzz::negative_fuzz;
use tungsten_fuzz::oracle::execute_dual_oracle;
use tungsten_fuzz::rng::Rng;

fn main() {
    let args: Vec<String> = env::args().collect();
    let mut cases = 500;
    let mut seed = 0x9876543210ABCDEF;

    let mut idx = 1;
    while idx < args.len() {
        if args[idx] == "--cases" && idx + 1 < args.len() {
            cases = args[idx + 1].parse().unwrap_or(cases);
            idx += 2;
        } else if args[idx] == "--seed" && idx + 1 < args.len() {
            seed = args[idx + 1].parse().unwrap_or(seed);
            idx += 2;
        } else {
            idx += 1;
        }
    }

    println!("============================================================");
    println!("     Tungsten Property-Based Differential Fuzzer");
    println!("============================================================");
    println!("Seed:  0x{:X}", seed);
    println!("Cases: {}", cases);
    println!("------------------------------------------------------------");

    let start = Instant::now();

    // 1. Allocator Invariant Fuzzing
    println!("\n[1/3] Fuzzing PhysicalArena allocator invariants...");
    let alloc_start = Instant::now();
    allocator_fuzz::fuzz_physical_arena(cases * 10, seed);
    allocator_fuzz::fuzz_interleaved_arenas(cases / 2, seed);
    println!("      Passed in {:?}", alloc_start.elapsed());

    // 2. Negative Escape Fuzzing
    println!("\n[2/3] Fuzzing negative escape rejections...");
    let neg_start = Instant::now();
    negative_fuzz::fuzz_negative_escapes(cases, seed);
    println!("      Passed in {:?}", neg_start.elapsed());

    // 3. Dual-Oracle Differential Fuzzing
    println!("\n[3/3] Fuzzing VM vs. Cranelift JIT differential parity...");
    let diff_start = Instant::now();
    let mut master_rng = Rng::seed(seed);

    for case_num in 1..=cases {
        let case_seed = master_rng.next_u64();
        let case_rng = Rng::seed(case_seed);
        let mut gen = ProgramGenerator::new(case_rng);
        let program = gen.generate_valid_program();

        if let Err(err) = execute_dual_oracle(&program) {
            eprintln!("\nCRITICAL DIVERGENCE / FAILURE DETECTED!");
            eprintln!("Case #{}/{} (seed: 0x{:X})", case_num, cases, case_seed);
            eprintln!("Error: {:?}", err);
            eprintln!("\nReproducing Source Code:\n{}", program);
            std::process::exit(1);
        }

        if case_num % 100 == 0 || case_num == cases {
            println!("      Progress: {}/{} valid region programs verified identical", case_num, cases);
        }
    }
    println!("      Passed in {:?}", diff_start.elapsed());

    println!("\n============================================================");
    println!(" Fuzzing completed successfully in {:?}", start.elapsed());
    println!(" Zero divergences, zero memory corruptions, zero leaks.");
    println!("============================================================");
}
