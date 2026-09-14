use std::collections::HashMap;
use cranelift::prelude::*;
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::{FuncId, Linkage, Module};
use tungsten_tir::ir::TirModule;

use crate::abi::to_cranelift_type;
use crate::compiler::{FunctionCompiler, RuntimeFuncs};
use crate::runtime;

pub struct JitEngine {
    module: JITModule,
    runtime: RuntimeFuncs,
}

impl JitEngine {
    pub fn new() -> Result<Self, String> {
        let mut flag_builder = settings::builder();
        flag_builder.set("use_colocated_libcalls", "false").map_err(|e| e.to_string())?;
        flag_builder.set("is_pic", "false").map_err(|e| e.to_string())?;

        let isa_builder = cranelift_native::builder().map_err(|e| e.to_string())?;
        let isa = isa_builder.finish(settings::Flags::new(flag_builder)).map_err(|e| e.to_string())?;

        let mut builder = JITBuilder::with_isa(isa, cranelift_module::default_libcall_names());

        // Register host runtime symbols
        builder.symbol("tungsten_print_i64", runtime::tungsten_print_i64 as *const u8);
        builder.symbol("tungsten_println_i64", runtime::tungsten_println_i64 as *const u8);
        builder.symbol("tungsten_print_str", runtime::tungsten_print_str as *const u8);
        builder.symbol("tungsten_println_str", runtime::tungsten_println_str as *const u8);
        builder.symbol("tungsten_io_print", runtime::tungsten_io_print as *const u8);
        builder.symbol("tungsten_refinement_panic", runtime::tungsten_refinement_panic as *const u8);
        builder.symbol("tungsten_alloc", runtime::tungsten_alloc as *const u8);
        builder.symbol("tungsten_fiber_spawn", runtime::tungsten_fiber_spawn as *const u8);
        builder.symbol("tungsten_fiber_yield", runtime::tungsten_fiber_yield as *const u8);
        builder.symbol("tungsten_fiber_sleep", runtime::tungsten_fiber_sleep as *const u8);
        builder.symbol("tungsten_channel_new", runtime::tungsten_channel_new as *const u8);
        builder.symbol("tungsten_channel_send", runtime::tungsten_channel_send as *const u8);
        builder.symbol("tungsten_channel_recv", runtime::tungsten_channel_recv as *const u8);
        builder.symbol("tungsten_region_enter", runtime::tungsten_region_enter as *const u8);
        builder.symbol("tungsten_region_alloc", runtime::tungsten_region_alloc as *const u8);
        builder.symbol("tungsten_region_exit", runtime::tungsten_region_exit as *const u8);
        builder.symbol("tungsten_trace_effect", runtime::tungsten_trace_effect as *const u8);

        let mut module = JITModule::new(builder);
        let ptr_type = module.target_config().pointer_type();

        // Declare runtime functions
        let runtime = Self::declare_runtime_funcs(&mut module, ptr_type)?;

        Ok(Self { module, runtime })
    }

    fn declare_runtime_funcs(module: &mut JITModule, ptr_type: types::Type) -> Result<RuntimeFuncs, String> {
        // print_i64: (I64) -> ()
        let mut sig = module.make_signature();
        sig.params.push(AbiParam::new(types::I64));
        let print_i64 = module
            .declare_function("tungsten_print_i64", Linkage::Import, &sig)
            .map_err(|e| e.to_string())?;

        // println_i64: (I64) -> ()
        let mut sig = module.make_signature();
        sig.params.push(AbiParam::new(types::I64));
        let println_i64 = module
            .declare_function("tungsten_println_i64", Linkage::Import, &sig)
            .map_err(|e| e.to_string())?;

        // print_str: (ptr, len) -> ()
        let mut sig = module.make_signature();
        sig.params.push(AbiParam::new(ptr_type));
        sig.params.push(AbiParam::new(ptr_type));
        let print_str = module
            .declare_function("tungsten_print_str", Linkage::Import, &sig)
            .map_err(|e| e.to_string())?;

        // println_str: (ptr, len) -> ()
        let mut sig = module.make_signature();
        sig.params.push(AbiParam::new(ptr_type));
        sig.params.push(AbiParam::new(ptr_type));
        let println_str = module
            .declare_function("tungsten_println_str", Linkage::Import, &sig)
            .map_err(|e| e.to_string())?;

        // io_print: (ptr, len) -> ()
        let mut sig = module.make_signature();
        sig.params.push(AbiParam::new(ptr_type));
        sig.params.push(AbiParam::new(ptr_type));
        let io_print = module
            .declare_function("tungsten_io_print", Linkage::Import, &sig)
            .map_err(|e| e.to_string())?;

        // refinement_panic: (val: I64, min: I64, max: I64) -> ()
        let mut sig = module.make_signature();
        sig.params.push(AbiParam::new(types::I64));
        sig.params.push(AbiParam::new(types::I64));
        sig.params.push(AbiParam::new(types::I64));
        let refinement_panic = module
            .declare_function("tungsten_refinement_panic", Linkage::Import, &sig)
            .map_err(|e| e.to_string())?;

        // alloc: (size: ptr, align: ptr) -> ptr
        let mut sig = module.make_signature();
        sig.params.push(AbiParam::new(ptr_type));
        sig.params.push(AbiParam::new(ptr_type));
        sig.returns.push(AbiParam::new(ptr_type));
        let alloc = module
            .declare_function("tungsten_alloc", Linkage::Import, &sig)
            .map_err(|e| e.to_string())?;

        // fiber_spawn: (func_ptr: ptr, arg1: I64, arg2: I64) -> I64
        let mut sig = module.make_signature();
        sig.params.push(AbiParam::new(ptr_type));
        sig.params.push(AbiParam::new(types::I64));
        sig.params.push(AbiParam::new(types::I64));
        sig.returns.push(AbiParam::new(types::I64));
        let fiber_spawn = module
            .declare_function("tungsten_fiber_spawn", Linkage::Import, &sig)
            .map_err(|e| e.to_string())?;

        // fiber_yield: () -> ()
        let sig = module.make_signature();
        let fiber_yield = module
            .declare_function("tungsten_fiber_yield", Linkage::Import, &sig)
            .map_err(|e| e.to_string())?;

        // fiber_sleep: (ms: I64) -> ()
        let mut sig = module.make_signature();
        sig.params.push(AbiParam::new(types::I64));
        let fiber_sleep = module
            .declare_function("tungsten_fiber_sleep", Linkage::Import, &sig)
            .map_err(|e| e.to_string())?;

        // channel_new: () -> I64
        let mut sig = module.make_signature();
        sig.returns.push(AbiParam::new(types::I64));
        let channel_new = module
            .declare_function("tungsten_channel_new", Linkage::Import, &sig)
            .map_err(|e| e.to_string())?;

        // channel_send: (cid: I64, val: I64) -> ()
        let mut sig = module.make_signature();
        sig.params.push(AbiParam::new(types::I64));
        sig.params.push(AbiParam::new(types::I64));
        let channel_send = module
            .declare_function("tungsten_channel_send", Linkage::Import, &sig)
            .map_err(|e| e.to_string())?;

        // channel_recv: (cid: I64) -> I64
        let mut sig = module.make_signature();
        sig.params.push(AbiParam::new(types::I64));
        sig.returns.push(AbiParam::new(types::I64));
        let channel_recv = module
            .declare_function("tungsten_channel_recv", Linkage::Import, &sig)
            .map_err(|e| e.to_string())?;

        // region_enter: () -> ptr
        let mut sig = module.make_signature();
        sig.returns.push(AbiParam::new(ptr_type));
        let region_enter = module
            .declare_function("tungsten_region_enter", Linkage::Import, &sig)
            .map_err(|e| e.to_string())?;

        // region_alloc: (arena: ptr, size: ptr, align: ptr) -> ptr
        let mut sig = module.make_signature();
        sig.params.push(AbiParam::new(ptr_type));
        sig.params.push(AbiParam::new(ptr_type));
        sig.params.push(AbiParam::new(ptr_type));
        sig.returns.push(AbiParam::new(ptr_type));
        let region_alloc = module
            .declare_function("tungsten_region_alloc", Linkage::Import, &sig)
            .map_err(|e| e.to_string())?;

        // region_exit: (arena: ptr) -> ()
        let mut sig = module.make_signature();
        sig.params.push(AbiParam::new(ptr_type));
        let region_exit = module
            .declare_function("tungsten_region_exit", Linkage::Import, &sig)
            .map_err(|e| e.to_string())?;

        // trace_effect: (eff_ptr: ptr, eff_len: ptr, op_ptr: ptr, op_len: ptr) -> ()
        let mut sig = module.make_signature();
        sig.params.push(AbiParam::new(ptr_type));
        sig.params.push(AbiParam::new(ptr_type));
        sig.params.push(AbiParam::new(ptr_type));
        sig.params.push(AbiParam::new(ptr_type));
        let trace_effect = module
            .declare_function("tungsten_trace_effect", Linkage::Import, &sig)
            .map_err(|e| e.to_string())?;

        Ok(RuntimeFuncs {
            print_i64,
            println_i64,
            print_str,
            println_str,
            io_print,
            refinement_panic,
            alloc,
            fiber_spawn,
            fiber_yield,
            fiber_sleep,
            channel_new,
            channel_send,
            channel_recv,
            region_enter,
            region_alloc,
            region_exit,
            trace_effect,
        })

    }

    pub fn compile_and_run(&mut self, tir_module: &TirModule) -> Result<i64, String> {
        let ptr_type = self.module.target_config().pointer_type();

        // 1. Declare all functions in the module
        let mut func_ids: HashMap<String, FuncId> = HashMap::new();
        for f in &tir_module.functions {
            let mut sig = self.module.make_signature();
            for p in &f.params {
                let cl_ty = to_cranelift_type(&p.ty, ptr_type).unwrap_or(types::I64);
                sig.params.push(AbiParam::new(cl_ty));
            }
            if let Some(ret_cl_ty) = to_cranelift_type(&f.return_type, ptr_type) {
                sig.returns.push(AbiParam::new(ret_cl_ty));
            }

            let func_id = self
                .module
                .declare_function(&f.name, Linkage::Export, &sig)
                .map_err(|e| e.to_string())?;
            func_ids.insert(f.name.clone(), func_id);
        }

        // 2. Compile each function definition
        let mut ctx = self.module.make_context();
        let mut builder_ctx = FunctionBuilderContext::new();

        for f in &tir_module.functions {
            let func_id = *func_ids.get(&f.name).unwrap();
            let mut sig = self.module.make_signature();
            for p in &f.params {
                let cl_ty = to_cranelift_type(&p.ty, ptr_type).unwrap_or(types::I64);
                sig.params.push(AbiParam::new(cl_ty));
            }
            if let Some(ret_cl_ty) = to_cranelift_type(&f.return_type, ptr_type) {
                sig.returns.push(AbiParam::new(ret_cl_ty));
            }
            ctx.func.signature = sig;

            FunctionCompiler::compile(
                f,
                &mut self.module,
                &mut ctx,
                &mut builder_ctx,
                &self.runtime,
                &func_ids,
                tir_module,
            )?;

            if let Err(e) = self.module.define_function(func_id, &mut ctx) {
                return Err(format!("Compilation error in fn '{}': {:?}\n\nCranelift IR:\n{}", f.name, e, ctx.func));
            }
            self.module.clear_context(&mut ctx);
        }

        // 3. Finalize in-memory compilation
        self.module.finalize_definitions().map_err(|e| e.to_string())?;

        // 4. Invoke native `main` function
        if let Some(main_id) = func_ids.get("main") {
            let main_ptr = self.module.get_finalized_function(*main_id);
            let main_func_def = tir_module.functions.iter().find(|f| f.name == "main");
            let has_return = main_func_def
                .map(|f| !matches!(f.return_type, tungsten_typeck::types::Type::Unit))
                .unwrap_or(false);

            if has_return {
                let main_fn: fn() -> i64 = unsafe { std::mem::transmute(main_ptr) };
                let res = main_fn();
                Ok(res)
            } else {
                let main_fn: fn() -> () = unsafe { std::mem::transmute(main_ptr) };
                main_fn();
                Ok(0)
            }
        } else {
            Err("No 'main' function found in module".into())
        }
    }

    pub fn call_func(&self, _func_name: &str, func_id: FuncId) -> *const u8 {
        self.module.get_finalized_function(func_id)
    }
}
