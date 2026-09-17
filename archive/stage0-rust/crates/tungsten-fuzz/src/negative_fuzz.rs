use crate::rng::Rng;
use tungsten_syntax::parse;
use tungsten_typeck::check;

pub fn fuzz_negative_escapes(iterations: usize, seed: u64) {
    let mut rng = Rng::seed(seed);

    for i in 0..iterations {
        let mode = rng.gen_range(0, 4);
        let code = match mode {
            0 => generate_return_escape(&mut rng, i),
            1 => generate_assignment_escape(&mut rng, i),
            2 => generate_trailing_block_escape(&mut rng, i),
            _ => generate_effect_continuation_escape(&mut rng, i),
        };

        let ast = parse(&code).expect("Negative fuzz program must be syntactically valid");
        let typeck_res = check(&ast);

        assert!(
            typeck_res.is_err(),
            "SAFETY VIOLATION! Compiler accepted illegal escaping reference!\nGenerated code:\n{}",
            code
        );

        let errs = typeck_res.unwrap_err();
        let has_escape_violation = errs
            .iter()
            .any(|e| e.message.contains("Region escape violation") || e.message.contains("Cannot assign"));

        assert!(
            has_escape_violation,
            "Expected safety diagnostic, but got:\n{:?}\nIn code:\n{}",
            errs, code
        );
    }
}

fn generate_return_escape(rng: &mut Rng, id: usize) -> String {
    let val_x = rng.gen_i64(1, 100);
    let val_y = rng.gen_i64(1, 100);
    format!(
        r#"
        struct StructNeg{} {{
            x: i64,
            y: i64,
        }}

        fn illegal_escape_fn() -> &StructNeg{} {{
            let s = StructNeg{} {{ x: {}, y: {} }};
            &s
        }}

        fn main() {{
            let _ = illegal_escape_fn();
        }}
        "#,
        id, id, id, val_x, val_y
    )
}

fn generate_assignment_escape(rng: &mut Rng, id: usize) -> String {
    let val = rng.gen_i64(1, 100);
    format!(
        r#"
        struct ObjNeg{} {{
            v: i64,
        }}

        fn main() {{
            let global_holder = ObjNeg{} {{ v: 0 }};
            let mut escape_sink = &global_holder;
            region r_inner{} {{
                let secret = ObjNeg{} {{ v: {} }};
                // Illegal escape: assigning inner region reference to outer variable
                escape_sink = &secret;
            }};
        }}
        "#,
        id, id, id, id, val
    )
}

fn generate_trailing_block_escape(rng: &mut Rng, id: usize) -> String {
    let val = rng.gen_i64(1, 100);
    format!(
        r#"
        struct LeakNeg{} {{
            data: i64,
        }}

        fn main() {{
            let _leaked = region r_leak{} {{
                let local_obj = LeakNeg{} {{ data: {} }};
                // Illegal escape: yielding reference to region-local object out of the region
                &local_obj
            }};
        }}
        "#,
        id, id, id, val
    )
}

fn generate_effect_continuation_escape(rng: &mut Rng, id: usize) -> String {
    let val = rng.gen_i64(1, 100);
    format!(
        r#"
        struct Secret{} {{
            key: i64,
        }}

        fn attack_handler() {{
            let mut captured_ref = &Secret{} {{ key: 0 }};
            handle {{
                region r_suspended{} {{
                    let inner_sec = Secret{} {{ key: {} }};
                    // Illegal escape across effect continuation boundary
                    captured_ref = &inner_sec;
                    IO::print("suspending inside region");
                }};
            }} with IO {{
                print(msg) => 0
            }};
        }}

        fn main() {{
            attack_handler();
        }}
        "#,
        id, id, id, id, val
    )
}

