use tungsten_syntax::parse;
use tungsten_typeck::check;
use tungsten_tir::{compile, optimize};
use tungsten_codegen::compile_and_run;
use tungsten_vm::{execute_and_capture, value::Value};

fn assert_differential(code: &str) {
    let ast = parse(code).expect("Syntax parse failed");
    check(&ast).expect("Type check failed");

    // 1. Evaluate via Tree-Walking VM
    let (vm_val, _) = execute_and_capture(&ast).expect("VM execution failed");

    // 2. Compile and run via Native Cranelift JIT
    let mut tir = compile(&ast).expect("TIR compile failed");
    optimize(&mut tir);
    let jit_res = compile_and_run(&tir).expect("JIT execution failed");

    // 3. Verify semantic parity of return values
    match vm_val {
        Value::Int(vm_int) => {
            assert_eq!(
                vm_int, jit_res,
                "Semantic divergence: VM returned {}, but JIT returned {}",
                vm_int, jit_res
            );
        }
        Value::Unit => {
            // Both executed to completion without panicking
        }
        other => {
            panic!("Unsupported return value comparison: {:?}", other);
        }
    }
}

#[test]
fn test_diff_deeply_nested_regions() {
    let code = r#"
    struct Layer1 { a: i64 }
    struct Layer2 { b: i64 }
    struct Layer3 { c: i64 }
    struct Layer4 { d: i64 }

    fn main() -> i64 {
        let res = region r1 {
            let l1 = Layer1 { a: 10 };
            let s2 = region r2 {
                let l2 = Layer2 { b: 20 };
                let s3 = region r3 {
                    let l3 = Layer3 { c: 30 };
                    let s4 = region r4 {
                        let l4 = Layer4 { d: 40 };
                        l4.d * 2
                    };
                    l3.c + s4
                };
                l2.b + s3
            };
            l1.a + s2
        };
        res
    }
    "#;
    assert_differential(code);
}

#[test]
fn test_diff_early_return_from_nested_regions() {
    let code = r#"
    struct Box { val: i64 }

    fn evaluate(flag: i64) -> i64 {
        region r1 {
            let b1 = Box { val: 100 };
            region r2 {
                let b2 = Box { val: 200 };
                if flag > 5 {
                    return b1.val + b2.val;
                }
            };
        };
        0
    }

    fn main() -> i64 {
        let early = evaluate(10);
        let normal = evaluate(2);
        early - normal
    }
    "#;
    assert_differential(code);
}

#[test]
fn test_diff_references_through_function_calls() {
    let code = r#"
    struct Point {
        x: i64,
        y: i64,
    }

    fn distance_sq(p: &Point) -> i64 {
        p.x * p.x + p.y * p.y
    }

    fn main() -> i64 {
        let d = region frame {
            let p1 = Point { x: 5, y: 12 };
            let p2 = Point { x: 3, y: 4 };
            distance_sq(&p1) + distance_sq(&p2)
        };
        d
    }
    "#;
    assert_differential(code);
}

#[test]
fn test_diff_multiple_allocations_different_shapes() {
    let code = r#"
    struct Single { x: i64 }
    struct Pair { x: i64, y: i64 }
    struct Triple { x: i64, y: i64, z: i64 }

    fn main() -> i64 {
        let total = region frame {
            let s = Single { x: 7 };
            let p = Pair { x: 10, y: 20 };
            let t = Triple { x: 100, y: 200, z: 300 };
            s.x + p.x + p.y + t.x + t.y + t.z
        };
        total
    }
    "#;
    assert_differential(code);
}

#[test]
fn test_diff_sequential_regions_reuse() {
    let code = r#"
    struct Accumulator { val: i64 }

    fn main() -> i64 {
        let first = region r1 {
            let a1 = Accumulator { val: 42 };
            a1.val * 2
        };

        let second = region r2 {
            let a2 = Accumulator { val: 100 };
            a2.val + 50
        };

        first + second
    }
    "#;
    assert_differential(code);
}

#[test]
fn test_diff_nested_structs_in_region() {
    let code = r#"
    struct Inner {
        val: i64,
    }

    struct Outer {
        inner: Inner,
        extra: i64,
    }

    fn main() -> i64 {
        let total = region frame {
            let o = Outer {
                inner: Inner { val: 42 },
                extra: 58,
            };
            o.inner.val + o.extra
        };
        total
    }
    "#;
    assert_differential(code);
}

#[test]
fn test_diff_mutable_field_updates_in_region() {
    let code = r#"
    struct Counter {
        count: i64,
    }

    fn main() -> i64 {
        let final_count = region frame {
            let mut c = Counter { count: 0 };
            c.count = c.count + 10;
            c.count = c.count * 3;
            c.count
        };
        final_count
    }
    "#;
    assert_differential(code);
}

#[test]
fn test_diff_region_with_multiple_branches() {
    let code = r#"
    struct Config {
        mode: i64,
        threshold: i64,
    }

    fn run_scenario(mode: i64) -> i64 {
        region frame {
            let cfg = Config { mode: mode, threshold: 50 };
            if cfg.mode == 1 {
                return cfg.threshold * 2;
            } else {
                return cfg.threshold / 2;
            }
        };
        0
    }

    fn main() -> i64 {
        let m1 = run_scenario(1);
        let m2 = run_scenario(2);
        m1 + m2
    }
    "#;
    assert_differential(code);
}

