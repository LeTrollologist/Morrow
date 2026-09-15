pub mod abi;
pub mod compiler;
pub mod jit;
pub mod llvm_driver;
pub mod llvm_text;
pub mod runtime;

use std::path::Path;
use tungsten_tir::ir::TirModule;

pub use llvm_driver::{compile_llvm_aot, run_llvm_aot, LlvmToolchain};
pub use llvm_text::emit_llvm_ir;
pub use runtime::set_silent_mode;

pub fn compile_and_run(module: &TirModule) -> Result<i64, String> {
    let mut engine = jit::JitEngine::new()?;
    engine.compile_and_run(module)
}

pub fn compile_and_run_llvm(module: &TirModule) -> Result<(i32, String), String> {
    llvm_driver::run_llvm_aot(module)
}

pub fn compile_to_native_binary(module: &TirModule, out_path: &Path, opt_level: &str) -> Result<(), String> {
    llvm_driver::compile_llvm_aot(module, out_path, opt_level)
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

    #[test]
    fn test_llvm_text_emission() {
        let code = r#"
        fn add(a: i64, b: i64) -> i64 {
            a + b
        }
        fn main() {
            let res = add(10, 20);
            println!("Result: {}", res);
        }
        "#;
        let ast = parse(code).unwrap();
        let module = compile(&ast).unwrap();
        let ir = emit_llvm_ir(&module);
        assert!(ir.contains("define i64 @add("));
        assert!(ir.contains("define i32 @main("));
        assert!(ir.contains("target triple = \"x86_64-pc-windows-gnu\""));
    }

    #[test]
    fn test_llvm_aot_simple_arithmetic() {
        let code = r#"
        fn main() {
            let x = 15;
            let y = 25;
            println!("Sum is {}", x + y);
        }
        "#;
        let ast = parse(code).unwrap();
        let module = compile(&ast).unwrap();
        let (code, stdout) = compile_and_run_llvm(&module).expect("LLVM AOT execution failed");
        assert_eq!(code, 0);
        assert!(stdout.contains("Sum is 40"), "Output was: {}", stdout);
    }

    #[test]
    fn test_llvm_aot_function_calls() {
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
        let (code, stdout) = compile_and_run_llvm(&module).expect("LLVM AOT execution failed");
        assert_eq!(code, 0);
        assert!(stdout.contains("Square is 81"), "Output was: {}", stdout);
    }

    #[test]
    fn test_llvm_aot_region_bump_allocation() {
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
        let (code, stdout) = compile_and_run_llvm(&module).expect("LLVM AOT execution failed");
        assert_eq!(code, 0);
        assert!(stdout.contains("Total from region is 30"), "Output was: {}", stdout);
    }

    #[test]
    fn test_llvm_aot_refinement_in_bounds() {
        let code = r#"
        type Percentage = u8(0..=100);

        fn main() {
            let p: Percentage = 80 as Percentage;
            println!("Percentage is {}", p);
        }
        "#;
        let ast = parse(code).unwrap();
        let module = compile(&ast).unwrap();
        let (code, stdout) = compile_and_run_llvm(&module).expect("LLVM AOT execution failed");
        assert_eq!(code, 0);
        assert!(stdout.contains("Percentage is 80"), "Output was: {}", stdout);
    }

    #[test]
    fn test_llvm_aot_refinement_panic() {
        let code = r#"
        type Percentage = u8(0..=100);

        fn main() {
            let p: Percentage = 150 as Percentage;
            println!("Should not print");
        }
        "#;
        let ast = parse(code).unwrap();
        let module = compile(&ast).unwrap();
        let (code, stdout) = compile_and_run_llvm(&module).expect("LLVM AOT execution failed");
        assert_eq!(code, 101);
        assert!(stdout.contains("[Tungsten Refinement Panic]"), "Output was: {}", stdout);
    }

    #[test]
    fn test_jit_enums_and_arrays() {
        let code = r#"
        enum Shape {
            Circle(i64),
            Rectangle(i64, i64),
            Point,
        }

        fn area(s: Shape) -> i64 {
            match s {
                Shape::Circle(r) => r * r * 3,
                Shape::Rectangle(w, h) => w * h,
                Shape::Point => 0,
            }
        }

        fn main() {
            let s1 = Shape::Circle(10);
            let s2 = Shape::Rectangle(4, 5);
            let s3 = Shape::Point;

            let a1 = area(s1);
            let a2 = area(s2);
            let a3 = area(s3);

            let arr: [i64; 3] = [a1, a2, a3];
            let sum = arr[0] + arr[1] + arr[2];

            println!("Areas: {}, {}, {}. Sum: {}", arr[0], arr[1], arr[2], sum);
        }
        "#;
        let ast = parse(code).unwrap();
        let module = compile(&ast).unwrap();
        let res = compile_and_run(&module);
        assert!(res.is_ok());
    }

    #[test]
    fn test_llvm_aot_enums_and_arrays() {
        let code = r#"
        enum Shape {
            Circle(i64),
            Rectangle(i64, i64),
            Point,
        }

        fn area(s: Shape) -> i64 {
            match s {
                Shape::Circle(r) => r * r * 3,
                Shape::Rectangle(w, h) => w * h,
                Shape::Point => 0,
            }
        }

        fn main() {
            let s1 = Shape::Circle(10);
            let s2 = Shape::Rectangle(4, 5);
            let s3 = Shape::Point;

            let a1 = area(s1);
            let a2 = area(s2);
            let a3 = area(s3);

            let arr: [i64; 3] = [a1, a2, a3];
            let sum = arr[0] + arr[1] + arr[2];

            println!("Areas: {}, {}, {}. Sum: {}", arr[0], arr[1], arr[2], sum);
        }
        "#;
        let ast = parse(code).unwrap();
        let module = compile(&ast).unwrap();
        let (code, stdout) = compile_and_run_llvm(&module).expect("LLVM AOT execution failed");
        assert_eq!(code, 0);
        assert!(stdout.contains("Areas: 300, 20, 0. Sum: 320"), "Output was: {}", stdout);
    }

    #[test]
    fn test_llvm_aot_byte_array_stride() {
        let code = r#"
        fn main() {
            let bytes: [u8; 4] = [65, 66, 67, 68];
            let b0 = bytes[0];
            let b1 = bytes[1];
            let b2 = bytes[2];
            let b3 = bytes[3];
            println!("Bytes: {}, {}, {}, {}", b0, b1, b2, b3);
        }
        "#;
        let ast = parse(code).unwrap();
        let module = compile(&ast).unwrap();
        let (code, stdout) = compile_and_run_llvm(&module).expect("LLVM AOT execution failed");
        assert_eq!(code, 0);
        assert!(stdout.contains("Bytes: 65, 66, 67, 68"), "Output was: {}", stdout);
    }

    #[test]
    fn test_llvm_aot_ffi_extern_and_pointers() {
        let code = r#"
        extern "C" {
            fn strlen(s: *const u8) -> usize;
            fn puts(s: *const u8) -> i32;
        }

        fn main() {
            let msg = "Hello from FFI!";
            let len = unsafe { strlen(msg) };
            unsafe { puts(msg); }
            let mut val: i64 = 42;
            let ptr: *mut i64 = &val;
            unsafe {
                *ptr = 100;
            }
            let read_back = unsafe { *ptr };
            println!("Len: {}, ReadBack: {}", len, read_back);
        }
        "#;
        let ast = parse(code).unwrap();
        let module = compile(&ast).unwrap();
        let (code, stdout) = compile_and_run_llvm(&module).expect("LLVM AOT execution failed");
        assert_eq!(code, 0);
        assert!(stdout.contains("Hello from FFI!"), "Output was: {}", stdout);
        assert!(stdout.contains("Len: 15, ReadBack: 100"), "Output was: {}", stdout);
    }
}


