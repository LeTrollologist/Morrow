pub mod abi;
pub mod compiler;
pub mod jit;
pub mod runtime;

use tungsten_tir::ir::TirModule;

pub use runtime::set_silent_mode;

pub fn compile_and_run(module: &TirModule) -> Result<i64, String> {
    let mut engine = jit::JitEngine::new()?;
    engine.compile_and_run(module)
}

pub fn compile_and_run_with_traces(module: &TirModule) -> Result<(i64, Vec<String>), String> {
    runtime::clear_effect_traces();
    let mut engine = jit::JitEngine::new()?;
    let ret = engine.compile_and_run(module)?;
    let traces = runtime::get_recorded_effect_traces();
    Ok((ret, traces))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tungsten_syntax::parse;
    use tungsten_tir::compile;

    #[test]
    fn test_jit_simple_arithmetic() {
        let code = r#"
        fn main() {
            let x = 15;
            let y = 25;
            println!("Sum is {}", x + y);
        }
        "#;
        let ast = parse(code).unwrap();
        let module = compile(&ast).unwrap();
        let res = compile_and_run(&module);
        assert!(res.is_ok());
    }

    #[test]
    fn test_jit_function_calls() {
        let code = r#"
        fn square(n: i64) -> i64 {
            n * n
        }

        fn main() {
            let res = square(9);
            println!("Square is {}", res);
        }
        "#;
        let ast = parse(code).unwrap();
        let module = compile(&ast).unwrap();
        let res = compile_and_run(&module);
        assert!(res.is_ok());
    }

    #[test]
    fn test_jit_refinement_in_bounds() {
        let code = r#"
        type Percentage = u8(0..=100);

        fn main() {
            let p: Percentage = 80 as Percentage;
            println!("Percentage is {}", p);
        }
        "#;
        let ast = parse(code).unwrap();
        let module = compile(&ast).unwrap();
        let res = compile_and_run(&module);
        assert!(res.is_ok());
    }

    #[test]
    fn test_jit_region_bump_allocation() {
        let code = r#"
        struct Point {
            x: i64,
            y: i64,
        }

        fn main() {
            let total = region frame {
                let p = Point { x: 10, y: 20 };
                p.x + p.y
            };
            println!("Total from region is {}", total);
        }
        "#;
        let ast = parse(code).unwrap();
        let module = compile(&ast).unwrap();
        let res = compile_and_run(&module);
        assert!(res.is_ok());
    }

    #[test]
    fn test_arena_zero_size_and_alignments() {
        use crate::runtime::PhysicalArena;

        let mut arena = PhysicalArena::new();

        // 1. Zero-sized allocations must not panic and must return non-null aligned pointers
        let z1 = arena.alloc(0, 1);
        assert_eq!(z1 as usize % 1, 0);
        let z8 = arena.alloc(0, 8);
        assert_eq!(z8 as usize % 8, 0);
        let z16 = arena.alloc(0, 16);
        assert_eq!(z16 as usize % 16, 0);
        let z64 = arena.alloc(0, 64);
        assert_eq!(z64 as usize % 64, 0);

        // 2. Non-standard and standard alignments with varied sizes
        let alignments = [1, 2, 4, 8, 16, 32, 64, 128];
        let sizes = [1, 3, 7, 15, 24, 31, 64, 100];

        for &align in &alignments {
            for &size in &sizes {
                let ptr = arena.alloc(size, align);
                assert!(!ptr.is_null());
                assert_eq!(
                    (ptr as usize) % align,
                    0,
                    "Allocation of size {} not aligned to {}",
                    size,
                    align
                );

                // Verify writable memory
                unsafe {
                    std::ptr::write_bytes(ptr, 0xAA, size);
                    assert_eq!(*ptr, 0xAA);
                    assert_eq!(*ptr.add(size - 1), 0xAA);
                }
            }
        }

        arena.destroy();
    }

    #[test]
    fn test_arena_chunk_boundary_and_growth() {
        use crate::runtime::PhysicalArena;

        let mut arena = PhysicalArena::new();

        // Initial chunk capacity is 4096. Allocate exactly 4096 bytes.
        let p1 = arena.alloc(4096, 16);
        assert!(!p1.is_null());
        assert_eq!((p1 as usize) % 16, 0);
        unsafe {
            std::ptr::write_bytes(p1, 0x55, 4096);
        }

        // Next allocation immediately crosses chunk boundary and forces chunk 2
        let p2 = arena.alloc(64, 32);
        assert!(!p2.is_null());
        assert_eq!((p2 as usize) % 32, 0);
        unsafe {
            std::ptr::write_bytes(p2, 0x66, 64);
        }

        // Allocate a large block larger than initial chunk size, forcing chunk 3
        let p3 = arena.alloc(16384, 64);
        assert!(!p3.is_null());
        assert_eq!((p3 as usize) % 64, 0);
        unsafe {
            std::ptr::write_bytes(p3, 0x77, 16384);
        }

        // Verify all 3 distinct chunks preserved their data without corruption
        unsafe {
            assert_eq!(*p1, 0x55);
            assert_eq!(*p1.add(4095), 0x55);
            assert_eq!(*p2, 0x66);
            assert_eq!(*p2.add(63), 0x66);
            assert_eq!(*p3, 0x77);
            assert_eq!(*p3.add(16383), 0x77);
        }

        arena.destroy();
    }

    #[test]
    fn test_arena_destruction_and_reallocation() {
        use crate::runtime::{tungsten_region_alloc, tungsten_region_enter, tungsten_region_exit};

        // Repeated lifecycle sequence: enter, allocate, destroy, repeat
        for cycle in 0..10 {
            let arena = tungsten_region_enter();
            assert!(!arena.is_null());

            for i in 0..100 {
                let ptr = tungsten_region_alloc(arena, 32, 8);
                assert!(!ptr.is_null());
                assert_eq!((ptr as usize) % 8, 0);
                unsafe {
                    *(ptr as *mut i64) = (cycle * 100 + i) as i64;
                    assert_eq!(*(ptr as *mut i64), (cycle * 100 + i) as i64);
                }
            }

            tungsten_region_exit(arena);
        }
    }
}
