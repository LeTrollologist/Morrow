pub mod allocator_fuzz;
pub mod generator;
pub mod negative_fuzz;
pub mod oracle;
pub mod rng;

#[cfg(test)]
mod tests {
    use super::*;
    use generator::ProgramGenerator;
    use oracle::execute_dual_oracle;
    use rng::Rng;

    #[test]
    fn test_fuzz_physical_allocator() {
        // Attack PhysicalArena with 2,500 random allocations across arbitrary alignments
        allocator_fuzz::fuzz_physical_arena(2500, 0x1337BEEF);
    }

    #[test]
    fn test_fuzz_interleaved_arenas() {
        // Interleaved parent/child arena lifecycles
        allocator_fuzz::fuzz_interleaved_arenas(100, 0xCAFEBABE);
    }

    #[test]
    fn test_fuzz_extreme_allocations() {
        // Verify OOM and arithmetic overflow safety on massive/extreme sizes
        allocator_fuzz::fuzz_extreme_allocations();
    }

    #[test]
    fn test_fuzz_negative_escapes() {
        // Verify 300 synthesized illegal escape programs are rejected by typechecker
        negative_fuzz::fuzz_negative_escapes(300, 0xDEADBEEF);
    }

    #[test]
    fn test_fuzz_differential_regions() {
        let mut rng = Rng::seed(0x42424242);
        let num_cases = 100;

        for case_idx in 0..num_cases {
            let case_seed = rng.next_u64();
            let case_rng = Rng::seed(case_seed);
            let mut gen = ProgramGenerator::new(case_rng);
            let program = gen.generate_valid_program();

            match execute_dual_oracle(&program) {
                Ok(_) => {}
                Err(err) => {
                    panic!(
                        "Differential fuzzing failure at case #{} (seed: 0x{:X}):\n{:?}\nSource:\n{}",
                        case_idx, case_seed, err, program
                    );
                }
            }
        }
    }
}
