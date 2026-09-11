pub mod abi;
pub mod compiler;
pub mod jit;
pub mod runtime;

use tungsten_tir::ir::TirModule;

pub fn compile_and_run(module: &TirModule) -> Result<i64, String> {
    let mut engine = jit::JitEngine::new()?;
    engine.compile_and_run(module)
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
}
