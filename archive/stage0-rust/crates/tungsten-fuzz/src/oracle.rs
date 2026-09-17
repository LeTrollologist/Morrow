use tungsten_codegen::compile_and_run_with_traces;
use tungsten_syntax::parse;
use tungsten_tir::{compile, optimize};
use tungsten_typeck::check;
use tungsten_vm::{execute_and_capture_full, value::Value};

#[derive(Debug)]
pub enum FuzzError {
    Parse(String),
    Typeck(String),
    VmRuntime(String),
    JitRuntime(String),
    Divergence {
        vm_val: i64,
        jit_val: i64,
        source: String,
        tir: String,
    },
    TraceDivergence {
        vm_trace: Vec<String>,
        jit_trace: Vec<String>,
        source: String,
    },
}

pub fn execute_dual_oracle(code: &str) -> Result<i64, FuzzError> {
    // 1. Parse AST
    let ast = parse(code).map_err(FuzzError::Parse)?;

    // 2. Type Check
    check(&ast).map_err(|errs| {
        let msg = errs
            .iter()
            .map(|e| format!("{}:{}: {}", e.span.line, e.span.column, e.message))
            .collect::<Vec<_>>()
            .join("\n");
        FuzzError::Typeck(msg)
    })?;

    // 3. Evaluate via Tree-Walking VM (capturing stdout and effect traces)
    let (vm_val, _, vm_trace) = execute_and_capture_full(&ast).map_err(FuzzError::VmRuntime)?;

    // 4. Compile and Run via Cranelift JIT (capturing effect traces)
    let mut tir = compile(&ast).map_err(FuzzError::JitRuntime)?;
    optimize(&mut tir);
    let tir_dump = tungsten_tir::print(&tir);

    let (jit_ret, jit_trace) = compile_and_run_with_traces(&tir).map_err(FuzzError::JitRuntime)?;

    // 5. Effect Execution Trace Parity Check
    if vm_trace != jit_trace {
        return Err(FuzzError::TraceDivergence {
            vm_trace,
            jit_trace,
            source: code.to_string(),
        });
    }

    // 6. Return Value Differential Parity Check
    match vm_val {
        Value::Int(vm_int) => {
            if vm_int != jit_ret {
                return Err(FuzzError::Divergence {
                    vm_val: vm_int,
                    jit_val: jit_ret,
                    source: code.to_string(),
                    tir: tir_dump,
                });
            }
            Ok(jit_ret)
        }
        Value::Unit => Ok(jit_ret),
        other => Err(FuzzError::VmRuntime(format!("Unsupported return value: {:?}", other))),
    }
}
