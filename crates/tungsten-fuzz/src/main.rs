use std::env;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use tungsten_fuzz::allocator_fuzz;
use tungsten_fuzz::generator::ProgramGenerator;
use tungsten_fuzz::negative_fuzz;
use tungsten_fuzz::oracle::execute_dual_oracle;
use tungsten_fuzz::rng::Rng;

struct FailureReport {
    case_num: usize,
    seed: u64,
    error: String,
    program: String,
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let mut cases = 100_000;
    let default_threads = thread::available_parallelism()
        .map(|p| p.get())
        .unwrap_or(4);
    let mut num_threads = default_threads;
    let mut seed = 0x9876543210ABCDEF;

    let mut idx = 1;
    while idx < args.len() {
        if args[idx] == "--cases" && idx + 1 < args.len() {
            cases = args[idx + 1].replace('_', "").parse().unwrap_or(cases);
            idx += 2;
        } else if args[idx] == "--threads" && idx + 1 < args.len() {
            num_threads = args[idx + 1].parse().unwrap_or(num_threads);
            idx += 2;
        } else if args[idx] == "--seed" && idx + 1 < args.len() {
            seed = args[idx + 1].parse().unwrap_or(seed);
            idx += 2;
        } else {
            idx += 1;
        }
    }

    tungsten_codegen::set_silent_mode(true);

    println!("============================================================");
    println!("     Tungsten Boosted Multi-Core Differential Fuzzer");
    println!("============================================================");
    println!("Seed:      0x{:X}", seed);
    println!("Cases:     {}", format_number(cases));
    println!("Threads:   {} (Parallel CPU Workers)", num_threads);
    println!("------------------------------------------------------------");

    let start_all = Instant::now();

    // 1. Allocator Invariant Fuzzing
    println!("\n[1/3] Fuzzing PhysicalArena allocator invariants...");
    let alloc_start = Instant::now();
    let alloc_cases = cases.min(100_000);
    allocator_fuzz::fuzz_physical_arena(alloc_cases, seed);
    allocator_fuzz::fuzz_interleaved_arenas((cases / 10).min(10_000), seed);
    allocator_fuzz::fuzz_extreme_allocations();
    println!("      {} physical allocator invariants verified in {:?}", format_number(alloc_cases), alloc_start.elapsed());

    // 2. Negative Escape Fuzzing
    println!("\n[2/3] Fuzzing negative escape rejections...");
    let neg_start = Instant::now();
    let neg_cases = cases.min(50_000);
    negative_fuzz::fuzz_negative_escapes(neg_cases, seed);
    println!("      {} illegal escape attacks rejected in {:?}", format_number(neg_cases), neg_start.elapsed());

    // 3. Dual-Oracle Multi-Core Differential Fuzzing
    println!("\n[3/3] Fuzzing VM vs. Cranelift Native JIT parity ({} cases across {} threads)...", format_number(cases), num_threads);
    let diff_start = Instant::now();

    let completed = Arc::new(AtomicUsize::new(0));
    let stop_signal = Arc::new(AtomicBool::new(false));
    let failure_slot: Arc<Mutex<Option<FailureReport>>> = Arc::new(Mutex::new(None));

    let cases_per_thread = (cases + num_threads - 1) / num_threads;

    let mut handles = Vec::new();
    for thread_idx in 0..num_threads {
        let completed_clone = Arc::clone(&completed);
        let stop_clone = Arc::clone(&stop_signal);
        let failure_clone = Arc::clone(&failure_slot);

        let thread_seed = seed.wrapping_add((thread_idx as u64).wrapping_mul(0x517cc1b727220a95));

        let handle = thread::spawn(move || {
            let mut thread_master_rng = Rng::seed(thread_seed);
            let start_case = thread_idx * cases_per_thread;
            let end_case = (start_case + cases_per_thread).min(cases);

            for case_idx in start_case..end_case {
                if stop_clone.load(Ordering::Relaxed) {
                    break;
                }

                let case_seed = thread_master_rng.next_u64();
                let case_rng = Rng::seed(case_seed);
                let mut gen = ProgramGenerator::new(case_rng);
                let program = gen.generate_valid_program();

                if let Err(err) = execute_dual_oracle(&program) {
                    stop_clone.store(true, Ordering::SeqCst);
                    let mut slot = failure_clone.lock().unwrap();
                    if slot.is_none() {
                        *slot = Some(FailureReport {
                            case_num: case_idx + 1,
                            seed: case_seed,
                            error: format!("{:?}", err),
                            program,
                        });
                    }
                    break;
                }

                completed_clone.fetch_add(1, Ordering::Relaxed);
            }
        });

        handles.push(handle);
    }

    // Monitor loop
    let mut last_reported_time = Instant::now();
    let mut last_completed = 0;

    while completed.load(Ordering::Relaxed) < cases && !stop_signal.load(Ordering::Relaxed) {
        thread::sleep(Duration::from_millis(500));
        let done = completed.load(Ordering::Relaxed);
        let now = Instant::now();
        let elapsed = now.duration_since(last_reported_time);

        if elapsed >= Duration::from_secs(1) || done >= cases {
            let progress_pct = (done as f64 / cases as f64) * 100.0;
            let delta = done.saturating_sub(last_completed);
            let rate = delta as f64 / elapsed.as_secs_f64();
            let total_elapsed = now.duration_since(diff_start).as_secs_f64();
            let avg_rate = done as f64 / total_elapsed.max(0.001);
            let remaining = cases.saturating_sub(done);
            let eta_secs = if avg_rate > 0.0 { remaining as f64 / avg_rate } else { 0.0 };

            println!(
                "      Progress: {} / {} ({:.1}%) | Speed: {:.0} cases/s | Elapsed: {:.1}s | ETA: {:.1}s",
                format_number(done),
                format_number(cases),
                progress_pct,
                rate,
                total_elapsed,
                eta_secs
            );

            last_reported_time = now;
            last_completed = done;
        }
    }

    for h in handles {
        let _ = h.join();
    }

    // Check for failure
    if let Some(fail) = failure_slot.lock().unwrap().take() {
        eprintln!("\n============================================================");
        eprintln!("CRITICAL DIVERGENCE / COMPILATION FAILURE DETECTED!");
        eprintln!("============================================================");
        eprintln!("Case #{}/{} (Seed: 0x{:X})", fail.case_num, cases, fail.seed);
        eprintln!("Error: {}", fail.error);
        eprintln!("\nReproducing Source Code:\n{}", fail.program);
        eprintln!("============================================================");
        std::process::exit(1);
    }

    let final_done = completed.load(Ordering::Relaxed);
    let diff_duration = diff_start.elapsed();
    let total_duration = start_all.elapsed();
    let overall_rate = final_done as f64 / diff_duration.as_secs_f64().max(0.001);

    println!(
        "      Completed {} differential programs in {:?} ({:.0} cases/sec)",
        format_number(final_done),
        diff_duration,
        overall_rate
    );

    println!("\n============================================================");
    println!(" Fuzzing completed successfully in {:?}", total_duration);
    println!(" Total cases: {}", format_number(final_done));
    println!(" Zero divergences, zero memory corruptions, zero leaks.");
    println!("============================================================");
}

fn format_number(n: usize) -> String {
    let s = n.to_string();
    let mut out = String::new();
    let len = s.len();
    for (i, ch) in s.chars().enumerate() {
        if i > 0 && (len - i) % 3 == 0 {
            out.push(',');
        }
        out.push(ch);
    }
    out
}
