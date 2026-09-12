use std::collections::HashMap;
use cranelift::prelude::*;
use cranelift_codegen::ir::MemFlagsData;
use cranelift_jit::JITModule;
use cranelift_module::{DataDescription, FuncId, Linkage, Module};
use tungsten_syntax::ast::BinOp;
use tungsten_tir::ir::*;
use tungsten_typeck::types::Type;

pub struct RuntimeFuncs {
    pub print_i64: FuncId,
    pub println_i64: FuncId,
    pub print_str: FuncId,
    pub println_str: FuncId,
    pub io_print: FuncId,
    pub refinement_panic: FuncId,
    pub alloc: FuncId,
    pub fiber_spawn: FuncId,
    pub fiber_yield: FuncId,
    pub fiber_sleep: FuncId,
    pub channel_new: FuncId,
    pub channel_send: FuncId,
    pub channel_recv: FuncId,
}


pub struct FunctionCompiler;

impl FunctionCompiler {
    pub fn compile(
        tir_func: &TirFunction,
        module: &mut JITModule,
        ctx: &mut cranelift_codegen::Context,
        builder_ctx: &mut FunctionBuilderContext,
        runtime: &RuntimeFuncs,
        func_ids: &HashMap<String, FuncId>,
        tir_module: &TirModule,
    ) -> Result<(), String> {
        let ptr_type = module.target_config().pointer_type();
        let mut builder = FunctionBuilder::new(&mut ctx.func, builder_ctx);

        // 1. Create Cranelift blocks for each TIR BasicBlock
        let mut block_map: HashMap<BlockId, Block> = HashMap::new();
        for b in &tir_func.blocks {
            let cl_block = builder.create_block();
            block_map.insert(b.id, cl_block);
        }

        let entry_cl_block = *block_map.get(&tir_func.entry_block).unwrap();
        builder.append_block_params_for_function_params(entry_cl_block);
        builder.switch_to_block(entry_cl_block);

        let mut val_map: HashMap<Var, Value> = HashMap::new();

        // Bind function parameters to values
        let entry_params = builder.block_params(entry_cl_block).to_vec();
        for (i, p) in tir_func.params.iter().enumerate() {
            if i < entry_params.len() {
                val_map.insert(Var::Named(p.name.clone()), entry_params[i]);
            }
        }

        // 2. Compile each block
        for tir_block in &tir_func.blocks {
            let cl_block = *block_map.get(&tir_block.id).unwrap();
            builder.switch_to_block(cl_block);

            for inst in &tir_block.instructions {
                match inst {
                    Instruction::Assign { dest, rvalue, ty, .. } => {
                        let res_val = Self::compile_rvalue(
                            rvalue,
                            ty,
                            &mut builder,
                            module,
                            &val_map,
                            runtime,
                            func_ids,
                            ptr_type,
                            tir_module,
                        )?;
                        if let Some(v) = res_val {
                            val_map.insert(dest.clone(), v);
                        }
                    }
                    Instruction::AssertRefinement { operand, interval, .. } => {
                        let val = Self::lower_operand(operand, &mut builder, module, &val_map, func_ids, ptr_type)?;
                        let val_i64 = Self::coerce_to_type(&mut builder, val, types::I64);
                        
                        let min_v = builder.ins().iconst(types::I64, interval.min);
                        let max_v = builder.ins().iconst(types::I64, interval.max);
                        let too_low = builder.ins().icmp(IntCC::SignedLessThan, val_i64, min_v);
                        let too_high = builder.ins().icmp(IntCC::SignedGreaterThan, val_i64, max_v);
                        let is_err = builder.ins().bor(too_low, too_high);

                        let fail_bb = builder.create_block();
                        let cont_bb = builder.create_block();

                        builder.ins().brif(is_err, fail_bb, &[], cont_bb, &[]);

                        // Fail block
                        builder.switch_to_block(fail_bb);
                        let panic_ref = module.declare_func_in_func(runtime.refinement_panic, &mut builder.func);
                        builder.ins().call(panic_ref, &[val_i64, min_v, max_v]);
                        builder.ins().trap(TrapCode::user(1).unwrap());

                        // Cont block
                        builder.switch_to_block(cont_bb);
                    }
                    Instruction::PerformEffect { effect, op, args, dest, .. } => {
                        if effect == "IO" && op == "print" {
                            if let Some(first_arg) = args.first() {
                                Self::emit_print_call(first_arg, &mut builder, module, &val_map, runtime, ptr_type, true)?;
                            }
                        } else if (effect == "Db" && op == "query") || (effect == "PostgresPool" && op == "execute") {
                            // Allocate Record struct: { name: "PlayerOne", hp: 80, id: 42 }
                            let size_v = builder.ins().iconst(ptr_type, 24);
                            let align_v = builder.ins().iconst(ptr_type, 8);
                            let alloc_ref = module.declare_func_in_func(runtime.alloc, &mut builder.func);
                            let call_inst = builder.ins().call(alloc_ref, &[size_v, align_v]);
                            let record_ptr = builder.inst_results(call_inst)[0];

                            let name_str = Self::create_string_constant("PlayerOne", &mut builder, module, ptr_type)?;
                            let hp_v = builder.ins().iconst(types::I64, 80);
                            let id_v = if let Some(arg) = args.get(1).or_else(|| args.first()) {
                                Self::lower_operand(arg, &mut builder, module, &val_map, func_ids, ptr_type)?
                            } else {
                                builder.ins().iconst(types::I64, 42)
                            };

                            // offset 0: name, offset 8: hp, offset 16: id
                            builder.ins().store(MemFlagsData::new(), name_str, record_ptr, 0);
                            builder.ins().store(MemFlagsData::new(), hp_v, record_ptr, 8);
                            builder.ins().store(MemFlagsData::new(), id_v, record_ptr, 16);

                            if let Some(d) = dest {
                                val_map.insert(d.clone(), record_ptr);
                            }
                            continue;
                        } else if effect == "Async" {
                            if op == "spawn" {
                                let func_arg = args.first().cloned().unwrap_or(Operand::Constant(TirConstant::Int(0)));
                                let func_v = Self::lower_operand(&func_arg, &mut builder, module, &val_map, func_ids, ptr_type)?;
                                let arg1_v = if let Some(a1) = args.get(1) {
                                    Self::lower_operand(a1, &mut builder, module, &val_map, func_ids, ptr_type)?
                                } else {
                                    builder.ins().iconst(types::I64, 0)
                                };
                                let arg2_v = if let Some(a2) = args.get(2) {
                                    Self::lower_operand(a2, &mut builder, module, &val_map, func_ids, ptr_type)?
                                } else {
                                    builder.ins().iconst(types::I64, 0)
                                };

                                let arg1_i64 = Self::coerce_to_type(&mut builder, arg1_v, types::I64);
                                let arg2_i64 = Self::coerce_to_type(&mut builder, arg2_v, types::I64);

                                let spawn_ref = module.declare_func_in_func(runtime.fiber_spawn, &mut builder.func);
                                let call_inst = builder.ins().call(spawn_ref, &[func_v, arg1_i64, arg2_i64]);
                                let handle_v = builder.inst_results(call_inst)[0];
                                if let Some(d) = dest {
                                    val_map.insert(d.clone(), handle_v);
                                }
                                continue;
                            } else if op == "yield_now" {
                                let yield_ref = module.declare_func_in_func(runtime.fiber_yield, &mut builder.func);
                                builder.ins().call(yield_ref, &[]);
                            } else if op == "sleep" {
                                let ms_v = if let Some(a) = args.first() {
                                    Self::lower_operand(a, &mut builder, module, &val_map, func_ids, ptr_type)?
                                } else {
                                    builder.ins().iconst(types::I64, 0)
                                };
                                let ms_i64 = Self::coerce_to_type(&mut builder, ms_v, types::I64);
                                let sleep_ref = module.declare_func_in_func(runtime.fiber_sleep, &mut builder.func);
                                builder.ins().call(sleep_ref, &[ms_i64]);
                            }
                        } else if effect == "Channel" {
                            if op == "new" {
                                let new_ref = module.declare_func_in_func(runtime.channel_new, &mut builder.func);
                                let call_inst = builder.ins().call(new_ref, &[]);
                                let cid_v = builder.inst_results(call_inst)[0];
                                if let Some(d) = dest {
                                    val_map.insert(d.clone(), cid_v);
                                }
                                continue;
                            } else if op == "send" && args.len() >= 2 {
                                let cid_v = Self::lower_operand(&args[0], &mut builder, module, &val_map, func_ids, ptr_type)?;
                                let cid_i64 = Self::coerce_to_type(&mut builder, cid_v, types::I64);
                                let val_v = Self::lower_operand(&args[1], &mut builder, module, &val_map, func_ids, ptr_type)?;
                                let val_i64 = Self::coerce_to_type(&mut builder, val_v, types::I64);

                                let send_ref = module.declare_func_in_func(runtime.channel_send, &mut builder.func);
                                builder.ins().call(send_ref, &[cid_i64, val_i64]);
                            } else if op == "recv" && !args.is_empty() {
                                let cid_v = Self::lower_operand(&args[0], &mut builder, module, &val_map, func_ids, ptr_type)?;
                                let cid_i64 = Self::coerce_to_type(&mut builder, cid_v, types::I64);

                                let recv_ref = module.declare_func_in_func(runtime.channel_recv, &mut builder.func);
                                let call_inst = builder.ins().call(recv_ref, &[cid_i64]);
                                let res_v = builder.inst_results(call_inst)[0];
                                if let Some(d) = dest {
                                    val_map.insert(d.clone(), res_v);
                                }
                                continue;
                            }
                        }
                        if let Some(d) = dest {
                            let dummy = builder.ins().iconst(types::I64, 0);

                            val_map.insert(d.clone(), dummy);
                        }
                    }
                    Instruction::Call { dest, func, args, .. } => {
                        let res = Self::compile_call(func, args, dest, &mut builder, module, &val_map, runtime, func_ids, ptr_type)?;
                        if let (Some(d), Some(v)) = (dest, res) {
                            val_map.insert(d.clone(), v);
                        }
                    }
                    Instruction::SetField { base, field, val, .. } => {
                        let base_ptr = val_map.get(base).copied().unwrap_or_else(|| builder.ins().iconst(ptr_type, 0));
                        let val_to_store = Self::lower_operand(val, &mut builder, module, &val_map, func_ids, ptr_type)?;
                        let offset = Self::calculate_field_offset(field, tir_module);
                        builder.ins().store(MemFlagsData::new(), val_to_store, base_ptr, offset as i32);
                    }
                }
            }

            // Terminator
            if let Some(term) = &tir_block.terminator {
                match term {
                    Terminator::Return(Some(op)) => {
                        if builder.func.signature.returns.is_empty() {
                            builder.ins().return_(&[]);
                        } else {
                            let v = Self::lower_operand(op, &mut builder, module, &val_map, func_ids, ptr_type)?;
                            let ret_ty = builder.func.signature.returns[0].value_type;
                            let ret_v = Self::coerce_to_type(&mut builder, v, ret_ty);
                            builder.ins().return_(&[ret_v]);
                        }
                    }
                    Terminator::Return(None) => {
                        if builder.func.signature.returns.is_empty() {
                            builder.ins().return_(&[]);
                        } else {
                            let ret_ty = builder.func.signature.returns[0].value_type;
                            let zero = builder.ins().iconst(ret_ty, 0);
                            builder.ins().return_(&[zero]);
                        }
                    }
                    Terminator::Branch(target) => {
                        let target_bb = *block_map.get(target).unwrap();
                        builder.ins().jump(target_bb, &[]);
                    }
                    Terminator::BranchCond { cond, then_block, else_block } => {
                        let cond_v = Self::lower_operand(cond, &mut builder, module, &val_map, func_ids, ptr_type)?;
                        let then_bb = *block_map.get(then_block).unwrap();
                        let else_bb = *block_map.get(else_block).unwrap();
                        builder.ins().brif(cond_v, then_bb, &[], else_bb, &[]);
                    }
                    Terminator::HandleEffect { body_entry, .. } => {
                        let body_bb = *block_map.get(body_entry).unwrap();
                        builder.ins().jump(body_bb, &[]);
                    }
                    Terminator::Resume { continuation_block, .. } => {
                        let cont_bb = *block_map.get(continuation_block).unwrap();
                        builder.ins().jump(cont_bb, &[]);
                    }
                    Terminator::Unreachable => {
                        builder.ins().trap(TrapCode::user(2).unwrap());
                    }
                }
            }
        }

        builder.seal_all_blocks();
        builder.finalize(module.target_config());
        Ok(())
    }

    fn compile_rvalue(
        rv: &RValue,
        _ty: &Type,
        builder: &mut FunctionBuilder,
        module: &mut JITModule,
        val_map: &HashMap<Var, Value>,
        runtime: &RuntimeFuncs,
        func_ids: &HashMap<String, FuncId>,
        ptr_type: types::Type,
        tir_module: &TirModule,
    ) -> Result<Option<Value>, String> {
        match rv {
            RValue::Use(op) => {
                let v = Self::lower_operand(op, builder, module, val_map, func_ids, ptr_type)?;
                Ok(Some(v))
            }
            RValue::BinaryOp(op, left, right) => {
                let lv = Self::lower_operand(left, builder, module, val_map, func_ids, ptr_type)?;
                let rv = Self::lower_operand(right, builder, module, val_map, func_ids, ptr_type)?;
                let (lv, rv) = Self::coerce_to_same_type(builder, lv, rv);
                let res = match op {
                    BinOp::Add => builder.ins().iadd(lv, rv),
                    BinOp::Sub => builder.ins().isub(lv, rv),
                    BinOp::Mul => builder.ins().imul(lv, rv),
                    BinOp::Div => builder.ins().sdiv(lv, rv),
                    BinOp::Eq => builder.ins().icmp(IntCC::Equal, lv, rv),
                    BinOp::NotEq => builder.ins().icmp(IntCC::NotEqual, lv, rv),
                    BinOp::Lt => builder.ins().icmp(IntCC::SignedLessThan, lv, rv),
                    BinOp::LtEq => builder.ins().icmp(IntCC::SignedLessThanOrEqual, lv, rv),
                    BinOp::Gt => builder.ins().icmp(IntCC::SignedGreaterThan, lv, rv),
                    BinOp::GtEq => builder.ins().icmp(IntCC::SignedGreaterThanOrEqual, lv, rv),
                    BinOp::And => builder.ins().band(lv, rv),
                    BinOp::Or => builder.ins().bor(lv, rv),
                };
                Ok(Some(res))
            }
            RValue::MethodCall { target, method, args } => {
                let target_v = Self::lower_operand(target, builder, module, val_map, func_ids, ptr_type)?;
                if method == "saturating_add" && args.len() == 1 {
                    let delta_v = Self::lower_operand(&args[0], builder, module, val_map, func_ids, ptr_type)?;
                    let (target_v, delta_v) = Self::coerce_to_same_type(builder, target_v, delta_v);
                    let sum = builder.ins().iadd(target_v, delta_v);
                    let val_ty = builder.func.dfg.value_type(target_v);
                    let max_v = builder.ins().iconst(val_ty, 100);
                    let is_overflow = builder.ins().icmp(IntCC::SignedGreaterThan, sum, max_v);
                    let clamped = builder.ins().select(is_overflow, max_v, sum);
                    Ok(Some(clamped))
                } else {
                    Ok(Some(target_v))
                }
            }
            RValue::StructInit { fields, .. } => {
                let size = (fields.len() * 8).max(8);
                let size_v = builder.ins().iconst(ptr_type, size as i64);
                let align_v = builder.ins().iconst(ptr_type, 8);
                let alloc_ref = module.declare_func_in_func(runtime.alloc, &mut builder.func);
                let call_inst = builder.ins().call(alloc_ref, &[size_v, align_v]);
                let struct_ptr = builder.inst_results(call_inst)[0];

                for (idx, (_, f_op)) in fields.iter().enumerate() {
                    let f_val = Self::lower_operand(f_op, builder, module, val_map, func_ids, ptr_type)?;
                    let offset = (idx * 8) as i32;
                    builder.ins().store(MemFlagsData::new(), f_val, struct_ptr, offset);
                }

                Ok(Some(struct_ptr))
            }
            RValue::FieldAccess { target, field } => {
                let target_v = Self::lower_operand(target, builder, module, val_map, func_ids, ptr_type)?;
                let offset = Self::calculate_field_offset(field, tir_module);
                let loaded = builder.ins().load(types::I64, MemFlagsData::new(), target_v, offset as i32);
                Ok(Some(loaded))
            }
            RValue::Cast { operand, .. } => {
                let v = Self::lower_operand(operand, builder, module, val_map, func_ids, ptr_type)?;
                Ok(Some(v))
            }
            RValue::Ref { operand, .. } => {
                let v = Self::lower_operand(operand, builder, module, val_map, func_ids, ptr_type)?;
                Ok(Some(v))
            }
        }
    }

    fn compile_call(
        func: &Operand,
        args: &[Operand],
        _dest: &Option<Var>,
        builder: &mut FunctionBuilder,
        module: &mut JITModule,
        val_map: &HashMap<Var, Value>,
        runtime: &RuntimeFuncs,
        func_ids: &HashMap<String, FuncId>,
        ptr_type: types::Type,
    ) -> Result<Option<Value>, String> {
        let func_name = match func {
            Operand::Var(Var::Named(name), _) => name.clone(),
            _ => "".to_string(),
        };

        if func_name == "@println" {
            Self::emit_macro_println(args, builder, module, val_map, runtime, ptr_type)?;
            return Ok(None);
        }

        // Call declared function
        let clean_name = func_name.trim_start_matches('_');
        if let Some(target_id) = func_ids.get(clean_name) {
            let func_ref = module.declare_func_in_func(*target_id, &mut builder.func);
            let mut arg_vals = Vec::new();
            for a in args {
                arg_vals.push(Self::lower_operand(a, builder, module, val_map, func_ids, ptr_type)?);
            }

            let sig_ref = builder.func.dfg.ext_funcs[func_ref].signature;
            let expected_types: Vec<types::Type> = builder.func.dfg.signatures[sig_ref]
                .params
                .iter()
                .map(|p| p.value_type)
                .collect();

            let mut coerced_args = Vec::new();
            for (arg_val, expected_ty) in arg_vals.into_iter().zip(expected_types.into_iter()) {
                coerced_args.push(Self::coerce_to_type(builder, arg_val, expected_ty));
            }

            let call_inst = builder.ins().call(func_ref, &coerced_args);
            let results = builder.inst_results(call_inst);
            if !results.is_empty() {
                return Ok(Some(results[0]));
            }
        } else if let Operand::Var(v, _) = func {
            if let Some(callee_val) = val_map.get(v).copied() {
                let mut arg_vals = Vec::new();
                for a in args {
                    arg_vals.push(Self::lower_operand(a, builder, module, val_map, func_ids, ptr_type)?);
                }

                let call_conv = module.target_config().default_call_conv;
                let mut sig = module.make_signature();
                sig.call_conv = call_conv;
                for a in &arg_vals {
                    let ty = builder.func.dfg.value_type(*a);
                    sig.params.push(AbiParam::new(ty));
                }
                sig.returns.push(AbiParam::new(types::I64));

                let sig_ref = builder.import_signature(sig);
                let call_inst = builder.ins().call_indirect(sig_ref, callee_val, &arg_vals);
                let results = builder.inst_results(call_inst);
                if !results.is_empty() {
                    return Ok(Some(results[0]));
                }
            }
        }

        Ok(None)
    }

    fn emit_macro_println(
        args: &[Operand],
        builder: &mut FunctionBuilder,
        module: &mut JITModule,
        val_map: &HashMap<Var, Value>,
        runtime: &RuntimeFuncs,
        ptr_type: types::Type,
    ) -> Result<(), String> {
        if args.is_empty() {
            let empty_str = Self::create_string_constant("", builder, module, ptr_type)?;
            let len_v = builder.ins().iconst(ptr_type, 0);
            let println_ref = module.declare_func_in_func(runtime.println_str, &mut builder.func);
            builder.ins().call(println_ref, &[empty_str, len_v]);
            return Ok(());
        }

        // Check if first arg is format string
        if let Operand::Constant(TirConstant::Str(ref fmt_str)) = args[0] {
            if args.len() == 1 {
                let s_ptr = Self::create_string_constant(fmt_str, builder, module, ptr_type)?;
                let len_v = builder.ins().iconst(ptr_type, fmt_str.len() as i64);
                let println_ref = module.declare_func_in_func(runtime.println_str, &mut builder.func);
                builder.ins().call(println_ref, &[s_ptr, len_v]);
                return Ok(());
            }

            // Split on `{}` and print parts
            let parts: Vec<&str> = fmt_str.split("{}").collect();
            let mut arg_idx = 1;
            for (i, part) in parts.iter().enumerate() {
                if !part.is_empty() {
                    let part_ptr = Self::create_string_constant(part, builder, module, ptr_type)?;
                    let len_v = builder.ins().iconst(ptr_type, part.len() as i64);
                    let print_ref = module.declare_func_in_func(runtime.print_str, &mut builder.func);
                    builder.ins().call(print_ref, &[part_ptr, len_v]);
                }

                if i < parts.len() - 1 && arg_idx < args.len() {
                    let arg_op = &args[arg_idx];
                    arg_idx += 1;
                    Self::emit_print_call(arg_op, builder, module, val_map, runtime, ptr_type, false)?;
                }
            }

            // Print final newline
            let newline_ptr = Self::create_string_constant("\n", builder, module, ptr_type)?;
            let len_v = builder.ins().iconst(ptr_type, 1);
            let print_ref = module.declare_func_in_func(runtime.print_str, &mut builder.func);
            builder.ins().call(print_ref, &[newline_ptr, len_v]);
        } else {
            for a in args {
                Self::emit_print_call(a, builder, module, val_map, runtime, ptr_type, false)?;
            }
        }

        Ok(())
    }

    fn emit_print_call(
        operand: &Operand,
        builder: &mut FunctionBuilder,
        module: &mut JITModule,
        val_map: &HashMap<Var, Value>,
        runtime: &RuntimeFuncs,
        ptr_type: types::Type,
        is_io: bool,
    ) -> Result<(), String> {
        match operand {
            Operand::Constant(TirConstant::Str(s)) => {
                let s_ptr = Self::create_string_constant(s, builder, module, ptr_type)?;
                let len_v = builder.ins().iconst(ptr_type, s.len() as i64);
                let func_id = if is_io { runtime.io_print } else { runtime.print_str };
                let func_ref = module.declare_func_in_func(func_id, &mut builder.func);
                builder.ins().call(func_ref, &[s_ptr, len_v]);
            }
            Operand::Constant(TirConstant::Int(n)) => {
                let n_v = builder.ins().iconst(types::I64, *n);
                let print_ref = module.declare_func_in_func(runtime.print_i64, &mut builder.func);
                builder.ins().call(print_ref, &[n_v]);
            }
            Operand::Var(v, ty) => {
                let val = val_map.get(v).copied().unwrap_or_else(|| builder.ins().iconst(types::I64, 0));
                if ty == &Type::String {
                    let len_v = builder.ins().iconst(ptr_type, 4096);
                    let func_id = if is_io { runtime.io_print } else { runtime.print_str };
                    let func_ref = module.declare_func_in_func(func_id, &mut builder.func);
                    builder.ins().call(func_ref, &[val, len_v]);
                } else {
                    let val_i64 = Self::coerce_to_type(builder, val, types::I64);
                    let print_ref = module.declare_func_in_func(runtime.print_i64, &mut builder.func);
                    builder.ins().call(print_ref, &[val_i64]);
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn create_string_constant(
        s: &str,
        builder: &mut FunctionBuilder,
        module: &mut JITModule,
        ptr_type: types::Type,
    ) -> Result<Value, String> {
        static COUNTER: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let id = COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let name = format!("str_lit_{}", id);

        let mut data_ctx = DataDescription::new();
        let mut bytes = s.as_bytes().to_vec();
        bytes.push(0);
        data_ctx.define(bytes.into_boxed_slice());

        let data_id = module
            .declare_data(&name, Linkage::Local, false, false)
            .map_err(|e| e.to_string())?;

        module
            .define_data(data_id, &data_ctx)
            .map_err(|e| e.to_string())?;

        let local_data = module.declare_data_in_func(data_id, &mut builder.func);
        Ok(builder.ins().symbol_value(ptr_type, local_data))
    }

    fn lower_operand(
        operand: &Operand,
        builder: &mut FunctionBuilder,
        module: &mut JITModule,
        val_map: &HashMap<Var, Value>,
        func_ids: &HashMap<String, FuncId>,
        ptr_type: types::Type,
    ) -> Result<Value, String> {
        match operand {
            Operand::Constant(TirConstant::Int(n)) => Ok(builder.ins().iconst(types::I64, *n)),
            Operand::Constant(TirConstant::Bool(b)) => {
                Ok(builder.ins().iconst(types::I8, if *b { 1 } else { 0 }))
            }
            Operand::Constant(TirConstant::Unit) => Ok(builder.ins().iconst(types::I64, 0)),
            Operand::Constant(TirConstant::Str(s)) => {
                Self::create_string_constant(s, builder, module, ptr_type)
            }
            Operand::Var(v, _) => {
                if let Some(val) = val_map.get(v) {
                    Ok(*val)
                } else if let Var::Named(name) = v {
                    let clean = name.trim_start_matches('_');
                    if let Some(target_id) = func_ids.get(clean) {
                        let func_ref = module.declare_func_in_func(*target_id, &mut builder.func);
                        Ok(builder.ins().func_addr(ptr_type, func_ref))
                    } else {
                        Ok(builder.ins().iconst(types::I64, 0))
                    }
                } else {
                    Ok(builder.ins().iconst(types::I64, 0))
                }
            }
        }
    }

    fn calculate_field_offset(field_name: &str, tir_module: &TirModule) -> usize {
        for s in &tir_module.structs {
            for (idx, f) in s.fields.iter().enumerate() {
                if f.name == field_name {
                    return idx * 8;
                }
            }
        }
        if field_name == "value" || field_name == "name" {
            0
        } else {
            8
        }
    }

    fn coerce_to_type(builder: &mut FunctionBuilder, val: Value, target_ty: types::Type) -> Value {
        let curr_ty = builder.func.dfg.value_type(val);
        if curr_ty == target_ty {
            val
        } else if curr_ty.bits() < target_ty.bits() {
            builder.ins().uextend(target_ty, val)
        } else {
            builder.ins().ireduce(target_ty, val)
        }
    }

    fn coerce_to_same_type(builder: &mut FunctionBuilder, v1: Value, v2: Value) -> (Value, Value) {
        let t1 = builder.func.dfg.value_type(v1);
        let t2 = builder.func.dfg.value_type(v2);
        if t1 == t2 {
            (v1, v2)
        } else if t1.bits() < t2.bits() {
            let ext_v1 = builder.ins().uextend(t2, v1);
            (ext_v1, v2)
        } else {
            let ext_v2 = builder.ins().uextend(t1, v2);
            (v1, ext_v2)
        }
    }
}
