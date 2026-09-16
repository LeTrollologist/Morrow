pub mod abi;
pub mod compiler;
pub mod jit;
pub mod llvm_driver;
pub mod llvm_text;
pub mod runtime;

use std::path::Path;
use tungsten_tir::ir::TirModule;

pub use llvm_driver::{
    compile_llvm_aot, compile_llvm_aot_with_options, run_llvm_aot, AotOptions, LlvmToolchain,
};
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

pub fn compile_to_native_binary_with_options(
    module: &TirModule,
    out_path: &Path,
    options: &AotOptions,
) -> Result<(), String> {
    llvm_driver::compile_llvm_aot_with_options(module, out_path, options)
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

    #[test]
    fn test_llvm_aot_nursery_structured_concurrency() {
        let code = r#"
        fn worker(val: i64, multiplier: i64) {
            println!("Task computed: {}", val * multiplier);
        }

        fn main() {
            nursery n {
                n.spawn(worker, 10, 3);
                n.spawn(worker, 20, 4);
                n.spawn(worker, 30, 5);
            }
            println!("Nursery join complete!");
        }
        "#;
        let ast = parse(code).unwrap();
        let module = compile(&ast).unwrap();
        let (code, stdout) = compile_and_run_llvm(&module).expect("LLVM AOT execution failed");
        assert_eq!(code, 0);
        assert!(stdout.contains("30"), "Output was: {}", stdout);
        assert!(stdout.contains("80"), "Output was: {}", stdout);
        assert!(stdout.contains("150"), "Output was: {}", stdout);
        assert!(stdout.contains("Nursery join complete!"), "Output was: {}", stdout);
    }

    #[test]
    fn test_llvm_aot_nursery_1000_tasks() {
        let code = r#"
        fn worker(id: i64, dummy: i64) {
            let mut acc = 0;
            let mut j = 0;
            while j < 100 {
                acc = acc + j;
                j = j + 1;
            }
        }

        fn main() {
            nursery n {
                let mut i = 0;
                while i < 1000 {
                    n.spawn(worker, i, 0);
                    i = i + 1;
                }
            }
            println!("All 1000 tasks completed successfully!");
        }
        "#;
        let ast = parse(code).unwrap();
        let module = compile(&ast).unwrap();
        let (code, stdout) = compile_and_run_llvm(&module).expect("LLVM AOT execution failed");
        assert_eq!(code, 0);
        assert!(stdout.contains("All 1000 tasks completed successfully!"), "Output was: {}", stdout);
    }

    #[test]
    fn test_llvm_text_debug_info() {
        let code = r#"
        fn compute(a: i64) -> i64 {
            let b = a * 2;
            return b + 1;
        }

        fn main() {
            let res = compute(21);
            println!("Result: {}", res);
        }
        "#;
        let ast = parse(code).unwrap();
        let module = compile(&ast).unwrap();
        let ir = emit_llvm_ir(&module);

        assert!(ir.contains("!llvm.module.flags = !{!0, !1}"));
        assert!(ir.contains("!llvm.dbg.cu = !{!2}"));
        assert!(ir.contains("!DICompileUnit("));
        assert!(ir.contains("!DIFile("));
        assert!(ir.contains("!DISubprogram(name: \"compute\""));
        assert!(ir.contains("!DISubprogram(name: \"main\""));
        assert!(ir.contains("!DILocation("));
        assert!(ir.contains(", !dbg !"));
    }

    #[test]
    fn test_in_place_region_growth() {
        use crate::runtime::PhysicalArena;

        let mut arena = PhysicalArena::new();
        // 1. Allocate initial chunk
        let ptr1 = arena.alloc(64, 8);
        assert!(!ptr1.is_null());

        // 2. In-place grow should return EXACT same pointer because it's the last allocation in the arena!
        let grown_ptr = arena.try_grow(ptr1, 64, 256, 8);
        assert_eq!(ptr1, grown_ptr, "In-place growth must reuse identical pointer without memcpy");

        // 3. Further in-place growth
        let grown_ptr2 = arena.try_grow(grown_ptr, 256, 1024, 8);
        assert_eq!(ptr1, grown_ptr2, "Second in-place growth must also preserve pointer");

        // 4. Interleaved allocation: allocating something else at top
        let interleaved = arena.alloc(32, 8);
        assert!(!interleaved.is_null());

        // 5. Growing ptr1 now MUST allocate a new chunk and fallback because ptr1 is no longer at the arena top!
        let fallback_ptr = arena.try_grow(ptr1, 1024, 2048, 8);
        assert_ne!(ptr1, fallback_ptr, "Interleaved growth must fall back to new allocation");
    }

    #[test]
    fn test_arena_tail_growth_and_forced_relocation() {
        use crate::runtime::PhysicalArena;

        for &align in &[16, 32, 64, 128] {
            let mut arena = PhysicalArena::new();

            // 1. Allocate A
            let a = arena.alloc(64, align);
            assert!(!a.is_null());
            assert_eq!((a as usize) % align, 0);
            unsafe {
                std::ptr::write_bytes(a, 0xAA, 64);
            }

            // 2. grow(A) -> must be the same pointer (tail allocation)
            let a_grown = arena.try_grow(a, 64, 128, align);
            assert_eq!(a, a_grown, "Tail allocation must grow in place with zero copy");
            assert_eq!((a_grown as usize) % align, 0);
            unsafe {
                // Verify initial 64 bytes preserved
                for i in 0..64 {
                    assert_eq!(*a_grown.add(i), 0xAA);
                }
                // Write pattern to newly expanded space
                std::ptr::write_bytes(a_grown.add(64), 0xAB, 64);
            }

            // 3. Allocate B (now B is the tail, A is no longer tail)
            let b = arena.alloc(64, align);
            assert!(!b.is_null());
            assert_eq!((b as usize) % align, 0);
            unsafe {
                std::ptr::write_bytes(b, 0xBB, 64);
            }

            // 4. grow(A) -> MUST move/copy because B is between A and current_top
            let a_relocated = arena.try_grow(a_grown, 128, 256, align);
            assert_ne!(a_grown, a_relocated, "Non-tail allocation must relocate to preserve subsequent allocations");
            assert_eq!((a_relocated as usize) % align, 0);
            unsafe {
                // Verify 128 bytes were preserved during relocation
                for i in 0..64 {
                    assert_eq!(*a_relocated.add(i), 0xAA);
                }
                for i in 64..128 {
                    assert_eq!(*a_relocated.add(i), 0xAB);
                }
                // Verify B was not corrupted by A's relocation
                for i in 0..64 {
                    assert_eq!(*b.add(i), 0xBB);
                }
            }

            // 5. grow(B) -> since A was relocated to the top, A_relocated is now the tail, so B must relocate too!
            let b_relocated = arena.try_grow(b, 64, 128, align);
            assert_ne!(b, b_relocated, "B must relocate because A was allocated after B");
            assert_eq!((b_relocated as usize) % align, 0);
            unsafe {
                for i in 0..64 {
                    assert_eq!(*b_relocated.add(i), 0xBB);
                }
            }

            // 6. Now B_relocated is the tail! Growing B_relocated should be in-place!
            let b_grown = arena.try_grow(b_relocated, 128, 256, align);
            assert_eq!(b_relocated, b_grown, "B_relocated is now the tail and must grow in-place");
            assert_eq!((b_grown as usize) % align, 0);

            // 7. Allocate C (now C is the tail)
            let c = arena.alloc(64, align);
            assert!(!c.is_null());
            assert_eq!((c as usize) % align, 0);

            // 8. grow(B_grown) -> MUST relocate because C is at top!
            let b_relocated2 = arena.try_grow(b_grown, 256, 512, align);
            assert_ne!(b_grown, b_relocated2, "B must relocate because C occupies the tail");

            arena.destroy();
        }
    }

    #[test]
    fn test_llvm_aot_debug_symbols_and_pdb() {
        use std::env;
        use std::fs;

        let code = r#"
        fn add(a: i64, b: i64) -> i64 {
            let res = a + b;
            return res;
        }

        fn main() {
            let val = add(40, 2);
            println!("Answer: {}", val);
        }
        "#;
        let ast = parse(code).unwrap();
        let mut module = tungsten_tir::compile_with_source(
            &ast,
            Some("test_debug.tg".to_string()),
            Some("C:/Tungsten/tests".to_string()),
        ).unwrap();
        tungsten_tir::optimize(&mut module);

        let temp_dir = env::temp_dir();
        let exe_path = temp_dir.join("tungsten_test_debug_symbols.exe");
        let pdb_path = temp_dir.join("tungsten_test_debug_symbols.pdb");

        // Clean up any stale files
        let _ = fs::remove_file(&exe_path);
        let _ = fs::remove_file(&pdb_path);

        let res = compile_to_native_binary(&module, &exe_path, "O0");
        assert!(res.is_ok(), "Native compilation failed: {:?}", res);

        assert!(exe_path.exists(), "Target .exe must exist");
        assert!(pdb_path.exists(), "Target .pdb must exist for native debugging");

        // Run the binary and verify output
        let out = std::process::Command::new(&exe_path).output().expect("Failed to run binary");
        assert!(out.status.success());
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(stdout.contains("Answer: 42"));

        // Cleanup
        let _ = fs::remove_file(&exe_path);
        let _ = fs::remove_file(&pdb_path);
    }

    #[test]
    fn test_llvm_aot_region_vec_and_string_in_place_growth() {
        use std::env;
        use std::fs;
        use std::path::Path;

        let root_dir = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().parent().unwrap();
        let collections_src = fs::read_to_string(root_dir.join("std").join("collections.tg"))
            .expect("std/collections.tg must exist");
        let region_vec_src = fs::read_to_string(root_dir.join("examples").join("region_vec.tg"))
            .expect("examples/region_vec.tg must exist");

        let combined = format!("{}\n\n{}", collections_src, region_vec_src);
        let ast = parse(&combined).expect("Parsing combined source failed");
        let mut module = tungsten_tir::compile(&ast).expect("TIR compile failed");
        tungsten_tir::optimize(&mut module);

        let temp_dir = env::temp_dir();
        let exe_path = temp_dir.join("tungsten_test_region_vec.exe");
        let pdb_path = temp_dir.join("tungsten_test_region_vec.pdb");

        let _ = fs::remove_file(&exe_path);
        let _ = fs::remove_file(&pdb_path);

        let res = compile_to_native_binary(&module, &exe_path, "O0");
        assert!(res.is_ok(), "Native compilation of region_vec failed: {:?}", res);

        let out = std::process::Command::new(&exe_path).output().expect("Failed to execute native binary");
        assert!(out.status.success(), "Native execution failed with code: {:?}", out.status.code());

        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(stdout.contains("Pushed 32 items. Vec len: 32, cap: 32"), "Output missing vector growth: {}", stdout);
        assert!(stdout.contains("Mutated element at index 5: 999"), "Output missing mutation: {}", stdout);
        assert!(stdout.contains("Popped last element: 310"), "Output missing pop: {}", stdout);
        assert!(stdout.contains("Vec len after pop: 31"), "Output missing len after pop: {}", stdout);
        assert!(stdout.contains("String length: 3, cap: 16"), "Output missing string length/cap: {}", stdout);
        assert!(stdout.contains("String UTF-8 valid: 1"), "Output missing UTF-8 validity: {}", stdout);
        assert!(stdout.contains("Sum of elements: 4960"), "Output missing sum: {}", stdout);
        assert!(stdout.contains("Genesis Milestone 1 verified successfully!"), "Output missing completion: {}", stdout);

        let _ = fs::remove_file(&exe_path);
        let _ = fs::remove_file(&pdb_path);
    }
}


