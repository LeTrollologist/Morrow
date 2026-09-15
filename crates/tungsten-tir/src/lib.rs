pub mod ir;
pub mod lower;
pub mod opt;
pub mod print;
pub mod verify;

use tungsten_syntax::ast::Program;

pub fn compile(program: &Program) -> Result<ir::TirModule, String> {
    let module = lower::lower_program(program)?;
    if let Err(errs) = verify::verify_module(&module) {
        let err_strs: Vec<String> = errs.into_iter().map(|e| e.to_string()).collect();
        return Err(err_strs.join("\n"));
    }
    Ok(module)
}

pub fn optimize(module: &mut ir::TirModule) -> opt::OptStats {
    let pm = opt::PassManager::new();
    pm.run(module)
}

pub fn print(module: &ir::TirModule) -> String {
    print::print_module(module)
}

pub fn verify(module: &ir::TirModule) -> Result<(), Vec<verify::TirVerifyError>> {
    verify::verify_module(module)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lower_simple_arithmetic() {
        let code = r#"
        fn add_five(x: i64) -> i64 {
            let result = x + 5;
            return result;
        }
        "#;
        let ast = tungsten_syntax::parse(code).unwrap();
        let module = compile(&ast).expect("TIR compile failed");
        assert_eq!(module.functions.len(), 1);
        let f = &module.functions[0];
        assert_eq!(f.name, "add_five");
        assert_eq!(f.params.len(), 1);
        assert!(!f.blocks.is_empty());

        let printed = print(&module);
        assert!(printed.contains("fn add_five"));
        assert!(printed.contains("bb0:"));
        assert!(printed.contains("return"));
    }

    #[test]
    fn test_const_folding() {
        let code = r#"
        fn compute() -> i64 {
            let a = 10 + 20;
            let b = 5 * 2;
            return a + b;
        }
        "#;
        let ast = tungsten_syntax::parse(code).unwrap();
        let mut module = compile(&ast).unwrap();
        let stats = optimize(&mut module);
        assert!(stats.const_folds >= 2);

        let printed = print(&module);
        // a + b should fold into 40
        assert!(printed.contains("40"));
    }

    #[test]
    fn test_dead_code_elimination() {
        let code = r#"
        fn dead_stuff() -> i64 {
            let unused = 999 * 888;
            return 42;
        }
        "#;
        let ast = tungsten_syntax::parse(code).unwrap();
        let mut module = compile(&ast).unwrap();
        let stats = optimize(&mut module);
        assert!(stats.dead_code_pruned >= 1);

        let printed = print(&module);
        // 'unused' temp calculation should be eliminated
        assert!(!printed.contains("888"));
        assert!(printed.contains("42"));
    }

    #[test]
    fn test_bounds_check_elimination() {
        let code = r#"
        type Health = u8(0..=100);

        fn safe_cast() -> Health {
            let h: Health = 50 as Health;
            return h;
        }
        "#;
        let ast = tungsten_syntax::parse(code).unwrap();
        let mut module = compile(&ast).unwrap();
        
        let initial_printed = print(&module);
        assert!(initial_printed.contains("assert_refinement"));

        let stats = optimize(&mut module);
        assert!(stats.bounds_checks_eliminated >= 1);

        let optimized_printed = print(&module);
        // The assertion should have been eliminated since 50 is known in bounds [0, 100]
        assert!(!optimized_printed.contains("assert_refinement"));
    }

    #[test]
    fn test_effect_handlers_lowering() {
        let code = r#"
        fn run_pipeline() -> () {
            handle {
                IO::print("test message");
            } with IO {
                print(msg) => 0,
            }
        }
        "#;
        let ast = tungsten_syntax::parse(code).unwrap();
        let module = compile(&ast).unwrap();
        let printed = print(&module);
        assert!(printed.contains("handle_effect"));
        assert!(printed.contains("perform IO::print"));
        assert!(printed.contains("resume"));
    }

    #[test]
    fn test_enums_and_arrays_lowering() {
        let code = r#"
        pub enum Option<T> {
            Some(T),
            None,
        }

        fn check_opt(val: i64) -> i64 {
            let opt = Option::Some(val);
            let res = match opt {
                Option::Some(x) => x + 1,
                Option::None => 0,
            };
            res
        }

        fn check_arr() -> u8 {
            let arr: [u8; 4] = [10, 20, 30, 40];
            arr[2]
        }
        "#;
        let ast = tungsten_syntax::parse(code).unwrap();
        let module = compile(&ast).unwrap();
        assert_eq!(module.enums.len(), 1);
        let printed = print(&module);
        assert!(printed.contains("enum_tag"));
        assert!(printed.contains("enum_payload"));
        assert!(printed.contains("Option::Some#0"));
        assert!(printed.contains("stride 1"));
    }

    #[test]
    fn test_ffi_lowering() {
        let code = r#"
        extern "C" {
            fn puts(s: *u8) -> i32;
        }

        fn main() {
            unsafe {
                let msg: *u8 = 0 as *u8;
                let c: u8 = *msg;
                *msg = 65 as u8;
                puts(msg);
            }
        }
        "#;
        let ast = tungsten_syntax::parse(code).unwrap();
        let module = compile(&ast).unwrap();
        assert_eq!(module.extern_blocks.len(), 1);
        let printed = print(&module);
        assert!(printed.contains("extern_call puts"));
        assert!(printed.contains("store"));
    }
}

