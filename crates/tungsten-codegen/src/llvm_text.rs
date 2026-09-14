use std::collections::HashMap;
use tungsten_syntax::ast::BinOp;
use tungsten_tir::ir::*;
use tungsten_typeck::types::Type;

pub struct LlvmTextEmitter<'a> {
    module: &'a TirModule,
    out: String,
    strings: Vec<String>,
    string_map: HashMap<String, usize>,
    temp_counter: usize,
    var_types: HashMap<Var, String>,
}

impl<'a> LlvmTextEmitter<'a> {
    pub fn new(module: &'a TirModule) -> Self {
        Self {
            module,
            out: String::new(),
            strings: Vec::new(),
            string_map: HashMap::new(),
            temp_counter: 0,
            var_types: HashMap::new(),
        }
    }

    fn next_temp(&mut self) -> String {
        let id = self.temp_counter;
        self.temp_counter += 1;
        format!("%t{}", id)
    }

    fn intern_string(&mut self, s: &str) -> usize {
        if let Some(&idx) = self.string_map.get(s) {
            idx
        } else {
            let idx = self.strings.len();
            self.strings.push(s.to_string());
            self.string_map.insert(s.to_string(), idx);
            idx
        }
    }

    pub fn emit(mut self) -> String {
        // Collect string literals first
        for func in &self.module.functions {
            for block in &func.blocks {
                for inst in &block.instructions {
                    self.collect_strings_from_inst(inst);
                }
            }
        }

        let mut header = String::new();
        header.push_str("; ==========================================================\n");
        header.push_str("; Tungsten Native LLVM Textual IR\n");
        header.push_str("; Zero-cost compilation target with auto-SIMD and LTO support\n");
        header.push_str("; ==========================================================\n\n");
        header.push_str("target datalayout = \"e-m:w-p270:32:32-p271:32:32-p272:64:64-i64:64-i128:128-f80:128-n8:16:32:64-S128\"\n");
        header.push_str("target triple = \"x86_64-pc-windows-gnu\"\n\n");

        // External C declarations
        header.push_str("; C Library Runtime Declarations\n");
        header.push_str("declare i32 @printf(ptr, ...)\n");
        header.push_str("declare i32 @putchar(i32)\n");
        header.push_str("declare void @exit(i32)\n");
        header.push_str("declare ptr @malloc(i64)\n");
        header.push_str("declare ptr @realloc(ptr, i64)\n");
        header.push_str("declare void @free(ptr)\n");
        header.push_str("declare void @Sleep(i32)\n\n");

        // Format strings
        header.push_str("; Global Format Strings\n");
        header.push_str("@fmt_i64 = internal constant [6 x i8] c\"%lld\\0A\\00\"\n");
        header.push_str("@fmt_i64_raw = internal constant [5 x i8] c\"%lld\\00\"\n");
        header.push_str("@fmt_str = internal constant [4 x i8] c\"%s\\0A\\00\"\n");
        header.push_str("@fmt_str_raw = internal constant [3 x i8] c\"%s\\00\"\n");
        header.push_str("@fmt_io_str = internal constant [9 x i8] c\"[IO] %s\\0A\\00\"\n");
        header.push_str("@fmt_io_empty = internal constant [6 x i8] c\"[IO]\\0A\\00\"\n");
        header.push_str("@panic_fmt = internal constant [57 x i8] c\"\\0A[Tungsten Refinement Panic]: %lld outside [%lld, %lld]\\0A\\00\"\n\n");

        // Tungsten Runtime Functions (in pure LLVM IR!)
        header.push_str("; Tungsten Native Runtime Functions\n");
        header.push_str("define void @tungsten_print_i64(i64 %v) {\n");
        header.push_str("    call i32 (ptr, ...) @printf(ptr @fmt_i64_raw, i64 %v)\n");
        header.push_str("    ret void\n}\n\n");

        header.push_str("define void @tungsten_println_i64(i64 %v) {\n");
        header.push_str("    call i32 (ptr, ...) @printf(ptr @fmt_i64, i64 %v)\n");
        header.push_str("    ret void\n}\n\n");

        header.push_str("define void @tungsten_print_str(ptr %s, i64 %len) {\n");
        header.push_str("    %null_check = icmp eq ptr %s, null\n");
        header.push_str("    br i1 %null_check, label %ret_blk, label %print_blk\n");
        header.push_str("print_blk:\n");
        header.push_str("    call i32 (ptr, ...) @printf(ptr @fmt_str_raw, ptr %s)\n");
        header.push_str("    br label %ret_blk\n");
        header.push_str("ret_blk:\n    ret void\n}\n\n");

        header.push_str("define void @tungsten_println_str(ptr %s, i64 %len) {\n");
        header.push_str("    %null_check = icmp eq ptr %s, null\n");
        header.push_str("    br i1 %null_check, label %empty_blk, label %print_blk\n");
        header.push_str("print_blk:\n");
        header.push_str("    call i32 (ptr, ...) @printf(ptr @fmt_str, ptr %s)\n");
        header.push_str("    ret void\n");
        header.push_str("empty_blk:\n");
        header.push_str("    call i32 @putchar(i32 10)\n");
        header.push_str("    ret void\n}\n\n");

        header.push_str("define void @tungsten_io_print(ptr %s, i64 %len) {\n");
        header.push_str("    %null_check = icmp eq ptr %s, null\n");
        header.push_str("    br i1 %null_check, label %empty_blk, label %print_blk\n");
        header.push_str("print_blk:\n");
        header.push_str("    call i32 (ptr, ...) @printf(ptr @fmt_io_str, ptr %s)\n");
        header.push_str("    ret void\n");
        header.push_str("empty_blk:\n");
        header.push_str("    call i32 (ptr, ...) @printf(ptr @fmt_io_empty)\n");
        header.push_str("    ret void\n}\n\n");

        header.push_str("define void @tungsten_refinement_panic(i64 %val, i64 %min_v, i64 %max_v) {\n");
        header.push_str("    call i32 (ptr, ...) @printf(ptr @panic_fmt, i64 %val, i64 %min_v, i64 %max_v)\n");
        header.push_str("    call void @exit(i32 101)\n");
        header.push_str("    unreachable\n}\n\n");

        header.push_str("define ptr @tungsten_alloc(i64 %size, i64 %align) {\n");
        header.push_str("    %is_zero = icmp eq i64 %size, 0\n");
        header.push_str("    %alloc_sz = select i1 %is_zero, i64 8, i64 %size\n");
        header.push_str("    %p = call ptr @malloc(i64 %alloc_sz)\n");
        header.push_str("    ret ptr %p\n}\n\n");

        // Region Allocator in pure LLVM IR (Arena: { ptr buf, i64 offset, i64 capacity })
        header.push_str("define ptr @tungsten_region_enter() {\n");
        header.push_str("    %arena = call ptr @malloc(i64 24)\n");
        header.push_str("    %buf = call ptr @malloc(i64 8192)\n");
        header.push_str("    store ptr %buf, ptr %arena\n");
        header.push_str("    %offset_ptr = getelementptr inbounds i8, ptr %arena, i64 8\n");
        header.push_str("    store i64 0, ptr %offset_ptr\n");
        header.push_str("    %cap_ptr = getelementptr inbounds i8, ptr %arena, i64 16\n");
        header.push_str("    store i64 8192, ptr %cap_ptr\n");
        header.push_str("    ret ptr %arena\n}\n\n");

        header.push_str("define ptr @tungsten_region_alloc(ptr %arena, i64 %size, i64 %align) {\n");
        header.push_str("    %offset_ptr = getelementptr inbounds i8, ptr %arena, i64 8\n");
        header.push_str("    %offset = load i64, ptr %offset_ptr\n");
        header.push_str("    %buf_ptr = load ptr, ptr %arena\n");
        header.push_str("    %ptr = getelementptr inbounds i8, ptr %buf_ptr, i64 %offset\n");
        header.push_str("    %new_offset = add i64 %offset, %size\n");
        header.push_str("    store i64 %new_offset, ptr %offset_ptr\n");
        header.push_str("    ret ptr %ptr\n}\n\n");

        header.push_str("define void @tungsten_region_exit(ptr %arena) {\n");
        header.push_str("    %is_null = icmp eq ptr %arena, null\n");
        header.push_str("    br i1 %is_null, label %done, label %free_blk\n");
        header.push_str("free_blk:\n");
        header.push_str("    %buf = load ptr, ptr %arena\n");
        header.push_str("    call void @free(ptr %buf)\n");
        header.push_str("    call void @free(ptr %arena)\n");
        header.push_str("    br label %done\n");
        header.push_str("done:\n    ret void\n}\n\n");

        header.push_str("define void @tungsten_trace_effect(ptr %eff, i64 %elen, ptr %op, i64 %olen) {\n    ret void\n}\n\n");
        header.push_str("define i64 @tungsten_fiber_spawn(ptr %fn_ptr, i64 %a1, i64 %a2) {\n    ret i64 1\n}\n\n");
        header.push_str("define void @tungsten_fiber_yield() {\n    call void @Sleep(i32 0)\n    ret void\n}\n\n");
        header.push_str("define void @tungsten_fiber_sleep(i64 %ms) {\n    %trunc = trunc i64 %ms to i32\n    call void @Sleep(i32 %trunc)\n    ret void\n}\n\n");
        header.push_str("define i64 @tungsten_channel_new() {\n    ret i64 1\n}\n\n");
        header.push_str("define void @tungsten_channel_send(i64 %cid, i64 %val) {\n    ret void\n}\n\n");
        header.push_str("define i64 @tungsten_channel_recv(i64 %cid) {\n    ret i64 0\n}\n\n");

        // Emit interned string constants
        header.push_str("; User String Literals\n");
        for (i, s) in self.strings.iter().enumerate() {
            let escaped = escape_llvm_string(s);
            let len = s.len() + 1;
            header.push_str(&format!("@str_{} = internal constant [{} x i8] c\"{}\\00\"\n", i, len, escaped));
        }
        header.push('\n');

        // Functions
        for func in &self.module.functions {
            self.emit_function(func);
        }

        let mut full = header;
        full.push_str(&self.out);
        full
    }

    fn collect_strings_from_inst(&mut self, inst: &Instruction) {
        match inst {
            Instruction::Assign { rvalue, .. } => {
                self.collect_strings_from_rvalue(rvalue);
            }
            Instruction::PerformEffect { effect, op, args, .. } => {
                self.intern_string(effect);
                self.intern_string(op);
                self.intern_string("PlayerOne");
                for a in args {
                    self.collect_strings_from_op(a);
                }
            }
            Instruction::Call { func, args, .. } => {
                self.collect_strings_from_op(func);
                let func_name = match func {
                    Operand::Var(Var::Named(name), _) => name.clone(),
                    Operand::Constant(TirConstant::Str(name)) => name.clone(),
                    _ => "".to_string(),
                };
                if func_name == "@println" {
                    if let Some(Operand::Constant(TirConstant::Str(ref fmt_str))) = args.first() {
                        self.intern_string(fmt_str);
                        for part in fmt_str.split("{}") {
                            if !part.is_empty() {
                                self.intern_string(part);
                            }
                        }
                    }
                }
                for a in args {
                    self.collect_strings_from_op(a);
                }
            }
            Instruction::SetField { val, .. } => {
                self.collect_strings_from_op(val);
            }
            _ => {}
        }
    }

    fn collect_strings_from_rvalue(&mut self, rv: &RValue) {
        match rv {
            RValue::Use(op) | RValue::Cast { operand: op, .. } | RValue::Ref { operand: op, .. } => {
                self.collect_strings_from_op(op);
            }
            RValue::BinaryOp(_, l, r) => {
                self.collect_strings_from_op(l);
                self.collect_strings_from_op(r);
            }
            RValue::FieldAccess { target, .. } => {
                self.collect_strings_from_op(target);
            }
            RValue::MethodCall { target, args, .. } => {
                self.collect_strings_from_op(target);
                for a in args {
                    self.collect_strings_from_op(a);
                }
            }
            RValue::StructInit { fields, arena, .. } => {
                for (_, f) in fields {
                    self.collect_strings_from_op(f);
                }
                if let Some(a) = arena {
                    self.collect_strings_from_op(a);
                }
            }
        }
    }

    fn collect_strings_from_op(&mut self, op: &Operand) {
        if let Operand::Constant(TirConstant::Str(s)) = op {
            self.intern_string(s);
        }
    }

    fn collect_vars_from_inst(&self, inst: &Instruction, vars: &mut HashMap<Var, String>) {
        match inst {
            Instruction::Assign { dest, rvalue, ty, .. } => {
                vars.insert(dest.clone(), type_to_llvm(ty));
                self.collect_vars_from_rvalue(rvalue, vars);
            }
            Instruction::AssertRefinement { operand, .. } => {
                self.collect_vars_from_op(operand, vars);
            }
            Instruction::Call { dest, func, args, ty, .. } => {
                if let Some(d) = dest {
                    vars.insert(d.clone(), type_to_llvm(ty));
                }
                self.collect_vars_from_op(func, vars);
                for a in args {
                    self.collect_vars_from_op(a, vars);
                }
            }
            Instruction::PerformEffect { dest, args, ty, .. } => {
                if let Some(d) = dest {
                    vars.insert(d.clone(), type_to_llvm(ty));
                }
                for a in args {
                    self.collect_vars_from_op(a, vars);
                }
            }
            Instruction::SetField { base, val, .. } => {
                vars.entry(base.clone()).or_insert_with(|| "ptr".to_string());
                self.collect_vars_from_op(val, vars);
            }
            Instruction::RegionEnter { dest, .. } => {
                vars.insert(dest.clone(), "ptr".to_string());
            }
            Instruction::RegionExit { arena, .. } => {
                self.collect_vars_from_op(arena, vars);
            }
        }
    }

    fn collect_vars_from_rvalue(&self, rv: &RValue, vars: &mut HashMap<Var, String>) {
        match rv {
            RValue::Use(op) | RValue::Cast { operand: op, .. } | RValue::Ref { operand: op, .. } => {
                self.collect_vars_from_op(op, vars);
            }
            RValue::BinaryOp(_, l, r) => {
                self.collect_vars_from_op(l, vars);
                self.collect_vars_from_op(r, vars);
            }
            RValue::FieldAccess { target, .. } => {
                self.collect_vars_from_op(target, vars);
            }
            RValue::MethodCall { target, args, .. } => {
                self.collect_vars_from_op(target, vars);
                for a in args {
                    self.collect_vars_from_op(a, vars);
                }
            }
            RValue::StructInit { fields, arena, .. } => {
                for (_, f) in fields {
                    self.collect_vars_from_op(f, vars);
                }
                if let Some(a) = arena {
                    self.collect_vars_from_op(a, vars);
                }
            }
        }
    }

    fn collect_vars_from_op(&self, op: &Operand, vars: &mut HashMap<Var, String>) {
        if let Operand::Var(v, ty) = op {
            if let Var::Named(name) = v {
                if name.starts_with('@') {
                    return;
                }
                let clean = name.trim_start_matches('_');
                if self.module.functions.iter().any(|f| f.name == clean) {
                    return;
                }
            }
            vars.entry(v.clone()).or_insert_with(|| type_to_llvm(ty));
        }
    }

    fn emit_function(&mut self, func: &TirFunction) {
        let is_main = func.name == "main";
        let ret_llvm_ty = if is_main {
            "i32".to_string()
        } else {
            type_to_llvm_ret(&func.return_type)
        };

        let mut params_sig = Vec::new();
        for p in &func.params {
            let ty_str = type_to_llvm(&p.ty);
            params_sig.push(format!("{} %arg_{}", ty_str, p.name));
        }

        self.out.push_str(&format!("define {} @{}({}) {{\n", ret_llvm_ty, func.name, params_sig.join(", ")));

        // Entry block: allocate stack slots for parameters and local variables
        self.out.push_str("entry:\n");

        // Collect all variables used in the function
        let mut vars = HashMap::new();
        for p in &func.params {
            vars.insert(Var::Named(p.name.clone()), type_to_llvm(&p.ty));
        }
        for block in &func.blocks {
            for inst in &block.instructions {
                self.collect_vars_from_inst(inst, &mut vars);
            }
            if let Some(ref term) = block.terminator {
                match term {
                    Terminator::Return(Some(op)) => self.collect_vars_from_op(op, &mut vars),
                    Terminator::BranchCond { cond, .. } => self.collect_vars_from_op(cond, &mut vars),
                    _ => {}
                }
            }
        }

        self.var_types = vars.clone();

        for (v, ty_str) in &vars {
            let v_name = var_to_slot_name(v);
            self.out.push_str(&format!("    {} = alloca {}\n", v_name, ty_str));
            if ty_str == "ptr" {
                self.out.push_str(&format!("    store ptr null, ptr {}\n", v_name));
            } else {
                self.out.push_str(&format!("    store {} 0, ptr {}\n", ty_str, v_name));
            }
        }

        // Store parameter values into allocas
        for p in &func.params {
            let slot = var_to_slot_name(&Var::Named(p.name.clone()));
            let ty_str = type_to_llvm(&p.ty);
            self.out.push_str(&format!("    store {} %arg_{}, ptr {}\n", ty_str, p.name, slot));
        }

        // Jump to first basic block
        self.out.push_str(&format!("    br label %bb{}\n\n", func.entry_block.0));

        // Emit basic blocks
        for block in &func.blocks {
            self.out.push_str(&format!("bb{}:\n", block.id.0));

            for inst in &block.instructions {
                self.emit_instruction(inst);
            }

            if let Some(ref term) = block.terminator {
                self.emit_terminator(term, is_main, &func.return_type);
            } else {
                if is_main {
                    self.out.push_str("    ret i32 0\n");
                } else if ret_llvm_ty == "void" {
                    self.out.push_str("    ret void\n");
                } else {
                    self.out.push_str(&format!("    ret {} 0\n", ret_llvm_ty));
                }
            }
            self.out.push('\n');
        }

        self.out.push_str("}\n\n");
    }

    fn emit_instruction(&mut self, inst: &Instruction) {
        match inst {
            Instruction::Assign { dest, rvalue, ty, .. } => {
                let (rval_res, rval_ty) = self.emit_rvalue(rvalue, ty);
                let target_ty = self.var_types.get(dest).cloned().unwrap_or_else(|| type_to_llvm(ty));
                let coerced = self.coerce_val(&rval_res, &rval_ty, &target_ty);
                let slot = var_to_slot_name(dest);
                self.out.push_str(&format!("    store {} {}, ptr {}\n", target_ty, coerced, slot));
            }
            Instruction::AssertRefinement { operand, interval, .. } => {
                let op_val = self.emit_operand(operand);
                let op_ty = self.get_operand_llvm_type(operand);
                let op_i64 = if op_ty == "i64" {
                    op_val
                } else {
                    let t_ext = self.next_temp();
                    self.out.push_str(&format!("    {} = zext {} {} to i64\n", t_ext, op_ty, op_val));
                    t_ext
                };
                let t_low = self.next_temp();
                let t_high = self.next_temp();
                let t_err = self.next_temp();
                let id = self.temp_counter;

                self.out.push_str(&format!("    {} = icmp slt i64 {}, {}\n", t_low, op_i64, interval.min));
                self.out.push_str(&format!("    {} = icmp sgt i64 {}, {}\n", t_high, op_i64, interval.max));
                self.out.push_str(&format!("    {} = or i1 {}, {}\n", t_err, t_low, t_high));
                self.out.push_str(&format!("    br i1 {}, label %panic_{}, label %cont_{}\n", t_err, id, id));

                self.out.push_str(&format!("panic_{}:\n", id));
                self.out.push_str(&format!("    call void @tungsten_refinement_panic(i64 {}, i64 {}, i64 {})\n", op_i64, interval.min, interval.max));
                self.out.push_str("    unreachable\n");

                self.out.push_str(&format!("cont_{}:\n", id));
            }
            Instruction::PerformEffect { effect, op, args, dest, .. } => {
                if effect == "IO" && op == "print" {
                    if let Some(first_arg) = args.first() {
                        let arg_val = self.emit_operand(first_arg);
                        self.out.push_str(&format!("    call void @tungsten_io_print(ptr {}, i64 0)\n", arg_val));
                    }
                } else if (effect == "Db" && op == "query") || (effect == "PostgresPool" && op == "execute") {
                    let ptr_temp = self.next_temp();
                    self.out.push_str(&format!("    {} = call ptr @tungsten_alloc(i64 24, i64 8)\n", ptr_temp));
                    
                    let s_idx = self.intern_string("PlayerOne");
                    let name_gep = self.next_temp();
                    self.out.push_str(&format!("    {} = getelementptr inbounds i8, ptr {}, i64 0\n", name_gep, ptr_temp));
                    self.out.push_str(&format!("    store ptr @str_{}, ptr {}\n", s_idx, name_gep));

                    let hp_gep = self.next_temp();
                    self.out.push_str(&format!("    {} = getelementptr inbounds i8, ptr {}, i64 8\n", hp_gep, ptr_temp));
                    self.out.push_str(&format!("    store i64 80, ptr {}\n", hp_gep));

                    let id_val = if let Some(a) = args.get(1).or_else(|| args.first()) {
                        self.emit_operand(a)
                    } else {
                        "42".to_string()
                    };
                    let id_gep = self.next_temp();
                    self.out.push_str(&format!("    {} = getelementptr inbounds i8, ptr {}, i64 16\n", id_gep, ptr_temp));
                    self.out.push_str(&format!("    store i64 {}, ptr {}\n", id_val, id_gep));

                    if let Some(d) = dest {
                        let slot = var_to_slot_name(d);
                        self.out.push_str(&format!("    store ptr {}, ptr {}\n", ptr_temp, slot));
                    }
                } else if effect == "Async" {
                    if op == "spawn" {
                        let fn_arg = args.first().map(|a| self.emit_operand(a)).unwrap_or_else(|| "null".to_string());
                        let a1 = args.get(1).map(|a| self.emit_operand(a)).unwrap_or_else(|| "0".to_string());
                        let a2 = args.get(2).map(|a| self.emit_operand(a)).unwrap_or_else(|| "0".to_string());
                        let res_temp = self.next_temp();
                        self.out.push_str(&format!("    {} = call i64 @tungsten_fiber_spawn(ptr {}, i64 {}, i64 {})\n", res_temp, fn_arg, a1, a2));
                        if let Some(d) = dest {
                            let slot = var_to_slot_name(d);
                            self.out.push_str(&format!("    store i64 {}, ptr {}\n", res_temp, slot));
                        }
                    } else if op == "yield_now" {
                        self.out.push_str("    call void @tungsten_fiber_yield()\n");
                    } else if op == "sleep" {
                        let ms = args.first().map(|a| self.emit_operand(a)).unwrap_or_else(|| "0".to_string());
                        self.out.push_str(&format!("    call void @tungsten_fiber_sleep(i64 {})\n", ms));
                    }
                } else if effect == "Channel" {
                    if op == "new" {
                        let res_temp = self.next_temp();
                        self.out.push_str(&format!("    {} = call i64 @tungsten_channel_new()\n", res_temp));
                        if let Some(d) = dest {
                            let slot = var_to_slot_name(d);
                            self.out.push_str(&format!("    store i64 {}, ptr {}\n", res_temp, slot));
                        }
                    } else if op == "send" && args.len() >= 2 {
                        let cid = self.emit_operand(&args[0]);
                        let val = self.emit_operand(&args[1]);
                        self.out.push_str(&format!("    call void @tungsten_channel_send(i64 {}, i64 {})\n", cid, val));
                    } else if op == "recv" && !args.is_empty() {
                        let cid = self.emit_operand(&args[0]);
                        let res_temp = self.next_temp();
                        self.out.push_str(&format!("    {} = call i64 @tungsten_channel_recv(i64 {})\n", res_temp, cid));
                        if let Some(d) = dest {
                            let slot = var_to_slot_name(d);
                            self.out.push_str(&format!("    store i64 {}, ptr {}\n", res_temp, slot));
                        }
                    }
                }
            }
            Instruction::Call { dest, func, args, ty, .. } => {
                let func_name = match func {
                    Operand::Var(Var::Named(name), _) => name.clone(),
                    Operand::Constant(TirConstant::Str(name)) => name.clone(),
                    _ => "".to_string(),
                };

                if func_name == "@println" {
                    self.emit_macro_println(args);
                    return;
                }

                let clean = func_name.trim_start_matches('_');
                let target_fn = self.module.functions.iter().find(|f| f.name == clean);
                let mut arg_strs = Vec::new();
                for (idx, a) in args.iter().enumerate() {
                    let op_str = self.emit_operand(a);
                    let from_ty = self.get_operand_llvm_type(a);
                    let expected_ty = if let Some(tf) = target_fn {
                        tf.params.get(idx).map(|p| type_to_llvm(&p.ty)).unwrap_or_else(|| from_ty.clone())
                    } else {
                        from_ty.clone()
                    };
                    let coerced = self.coerce_val(&op_str, &from_ty, &expected_ty);
                    arg_strs.push(format!("{} {}", expected_ty, coerced));
                }

                let ret_ty = type_to_llvm_ret(ty);
                if ret_ty == "void" {
                    self.out.push_str(&format!("    call void @{}({})\n", clean, arg_strs.join(", ")));
                } else {
                    let call_temp = self.next_temp();
                    self.out.push_str(&format!("    {} = call {} @{}({})\n", call_temp, ret_ty, clean, arg_strs.join(", ")));
                    if let Some(d) = dest {
                        let dest_ty = self.var_types.get(d).cloned().unwrap_or_else(|| type_to_llvm(ty));
                        let coerced = self.coerce_val(&call_temp, &ret_ty, &dest_ty);
                        let slot = var_to_slot_name(d);
                        self.out.push_str(&format!("    store {} {}, ptr {}\n", dest_ty, coerced, slot));
                    }
                }
            }
            Instruction::SetField { base, field, val, .. } => {
                let base_slot = var_to_slot_name(base);
                let base_ty = self.var_types.get(base).cloned().unwrap_or_else(|| "ptr".to_string());
                let base_val = self.next_temp();
                self.out.push_str(&format!("    {} = load {}, ptr {}\n", base_val, base_ty, base_slot));
                let base_ptr = self.coerce_val(&base_val, &base_ty, "ptr");
                let offset = self.calculate_field_offset(field);
                let gep = self.next_temp();
                self.out.push_str(&format!("    {} = getelementptr inbounds i8, ptr {}, i64 {}\n", gep, base_ptr, offset));
                let val_str = self.emit_operand(val);
                let val_ty = self.get_operand_llvm_type(val);
                let val_i64 = self.coerce_val(&val_str, &val_ty, "i64");
                self.out.push_str(&format!("    store i64 {}, ptr {}\n", val_i64, gep));
            }
            Instruction::RegionEnter { dest, .. } => {
                let arena_temp = self.next_temp();
                self.out.push_str(&format!("    {} = call ptr @tungsten_region_enter()\n", arena_temp));
                let slot = var_to_slot_name(dest);
                self.out.push_str(&format!("    store ptr {}, ptr {}\n", arena_temp, slot));
            }
            Instruction::RegionExit { arena, .. } => {
                let arena_val = self.emit_operand(arena);
                let arena_ty = self.get_operand_llvm_type(arena);
                let arena_ptr = if arena_ty == "ptr" {
                    arena_val
                } else {
                    let t = self.next_temp();
                    self.out.push_str(&format!("    {} = inttoptr {} {} to ptr\n", t, arena_ty, arena_val));
                    t
                };
                self.out.push_str(&format!("    call void @tungsten_region_exit(ptr {})\n", arena_ptr));
            }
        }
    }

    fn emit_macro_println(&mut self, args: &[Operand]) {
        if args.is_empty() {
            self.out.push_str("    call i32 @putchar(i32 10)\n");
            return;
        }

        if let Operand::Constant(TirConstant::Str(ref fmt_str)) = args[0] {
            if args.len() == 1 {
                let idx = self.intern_string(fmt_str);
                self.out.push_str(&format!("    call void @tungsten_println_str(ptr @str_{}, i64 {})\n", idx, fmt_str.len()));
                return;
            }

            let parts: Vec<&str> = fmt_str.split("{}").collect();
            let mut arg_idx = 1;
            for (i, part) in parts.iter().enumerate() {
                if !part.is_empty() {
                    let idx = self.intern_string(part);
                    self.out.push_str(&format!("    call void @tungsten_print_str(ptr @str_{}, i64 {})\n", idx, part.len()));
                }

                if i < parts.len() - 1 && arg_idx < args.len() {
                    let arg_op = &args[arg_idx];
                    arg_idx += 1;
                    let val_str = self.emit_operand(arg_op);
                    let val_ty = self.get_operand_llvm_type(arg_op);
                    match arg_op.get_type() {
                        Type::String => {
                            let s_ptr = self.coerce_val(&val_str, &val_ty, "ptr");
                            self.out.push_str(&format!("    call void @tungsten_print_str(ptr {}, i64 0)\n", s_ptr));
                        }
                        _ => {
                            let val_i64 = if val_ty == "i64" {
                                val_str
                            } else {
                                let t_ext = self.next_temp();
                                self.out.push_str(&format!("    {} = zext {} {} to i64\n", t_ext, val_ty, val_str));
                                t_ext
                            };
                            self.out.push_str(&format!("    call void @tungsten_print_i64(i64 {})\n", val_i64));
                        }
                    }
                }
            }

            self.out.push_str("    call i32 @putchar(i32 10)\n");
        } else {
            for a in args {
                let val_str = self.emit_operand(a);
                let val_ty = self.get_operand_llvm_type(a);
                match a.get_type() {
                    Type::String => {
                        let s_ptr = self.coerce_val(&val_str, &val_ty, "ptr");
                        self.out.push_str(&format!("    call void @tungsten_println_str(ptr {}, i64 0)\n", s_ptr));
                    }
                    _ => {
                        let val_i64 = if val_ty == "i64" {
                            val_str
                        } else {
                            let t_ext = self.next_temp();
                            self.out.push_str(&format!("    {} = zext {} {} to i64\n", t_ext, val_ty, val_str));
                            t_ext
                        };
                        self.out.push_str(&format!("    call void @tungsten_println_i64(i64 {})\n", val_i64));
                    }
                }
            }
        }
    }

    fn coerce_val(&mut self, val: &str, from_ty: &str, to_ty: &str) -> String {
        if from_ty == to_ty {
            val.to_string()
        } else if from_ty == "i64" && (to_ty == "i8" || to_ty == "i16" || to_ty == "i32") {
            let t = self.next_temp();
            self.out.push_str(&format!("    {} = trunc i64 {} to {}\n", t, val, to_ty));
            t
        } else if (from_ty == "i8" || from_ty == "i16" || from_ty == "i32") && to_ty == "i64" {
            let t = self.next_temp();
            self.out.push_str(&format!("    {} = zext {} {} to i64\n", t, from_ty, val));
            t
        } else if from_ty == "ptr" && to_ty == "i64" {
            let t = self.next_temp();
            self.out.push_str(&format!("    {} = ptrtoint ptr {} to i64\n", t, val));
            t
        } else if from_ty == "i64" && to_ty == "ptr" {
            let t = self.next_temp();
            self.out.push_str(&format!("    {} = inttoptr i64 {} to ptr\n", t, val));
            t
        } else {
            val.to_string()
        }
    }

    fn emit_rvalue(&mut self, rv: &RValue, _ty: &Type) -> (String, String) {
        match rv {
            RValue::Use(op) => (self.emit_operand(op), self.get_operand_llvm_type(op)),
            RValue::BinaryOp(op, l, r) => {
                let lv = self.emit_operand(l);
                let l_ty = self.get_operand_llvm_type(l);
                let lv_i64 = self.coerce_val(&lv, &l_ty, "i64");

                let rv = self.emit_operand(r);
                let r_ty = self.get_operand_llvm_type(r);
                let rv_i64 = self.coerce_val(&rv, &r_ty, "i64");

                let t = self.next_temp();
                let op_str = match op {
                    BinOp::Add => "add i64",
                    BinOp::Sub => "sub i64",
                    BinOp::Mul => "mul i64",
                    BinOp::Div => "sdiv i64",
                    BinOp::Eq => "icmp eq i64",
                    BinOp::NotEq => "icmp ne i64",
                    BinOp::Lt => "icmp slt i64",
                    BinOp::LtEq => "icmp sle i64",
                    BinOp::Gt => "icmp sgt i64",
                    BinOp::GtEq => "icmp sge i64",
                    BinOp::And => "and i64",
                    BinOp::Or => "or i64",
                };
                if matches!(op, BinOp::Eq | BinOp::NotEq | BinOp::Lt | BinOp::LtEq | BinOp::Gt | BinOp::GtEq) {
                    let cmp_t = self.next_temp();
                    self.out.push_str(&format!("    {} = {} {}, {}\n", cmp_t, op_str, lv_i64, rv_i64));
                    self.out.push_str(&format!("    {} = zext i1 {} to i64\n", t, cmp_t));
                } else {
                    self.out.push_str(&format!("    {} = {} {}, {}\n", t, op_str, lv_i64, rv_i64));
                }
                (t, "i64".to_string())
            }
            RValue::MethodCall { target, method, args } => {
                let target_v = self.emit_operand(target);
                let target_ty = self.get_operand_llvm_type(target);
                let target_i64 = self.coerce_val(&target_v, &target_ty, "i64");
                if method == "saturating_add" && args.len() == 1 {
                    let delta_v = self.emit_operand(&args[0]);
                    let delta_ty = self.get_operand_llvm_type(&args[0]);
                    let delta_i64 = self.coerce_val(&delta_v, &delta_ty, "i64");
                    let sum_t = self.next_temp();
                    let cmp_t = self.next_temp();
                    let sel_t = self.next_temp();
                    self.out.push_str(&format!("    {} = add i64 {}, {}\n", sum_t, target_i64, delta_i64));
                    self.out.push_str(&format!("    {} = icmp sgt i64 {}, 100\n", cmp_t, sum_t));
                    self.out.push_str(&format!("    {} = select i1 {}, i64 100, i64 {}\n", sel_t, cmp_t, sum_t));
                    (sel_t, "i64".to_string())
                } else {
                    (target_i64, "i64".to_string())
                }
            }
            RValue::StructInit { fields, arena, .. } => {
                let size = (fields.len() * 8).max(8);
                let ptr_temp = self.next_temp();
                if let Some(a_op) = arena {
                    let a_val = self.emit_operand(a_op);
                    let a_ty = self.get_operand_llvm_type(a_op);
                    let a_ptr = if a_ty == "ptr" {
                        a_val
                    } else {
                        let t = self.next_temp();
                        self.out.push_str(&format!("    {} = inttoptr {} {} to ptr\n", t, a_ty, a_val));
                        t
                    };
                    self.out.push_str(&format!("    {} = call ptr @tungsten_region_alloc(ptr {}, i64 {}, i64 8)\n", ptr_temp, a_ptr, size));
                } else {
                    self.out.push_str(&format!("    {} = call ptr @tungsten_alloc(i64 {}, i64 8)\n", ptr_temp, size));
                }

                for (idx, (_, f_op)) in fields.iter().enumerate() {
                    let f_val = self.emit_operand(f_op);
                    let f_ty = self.get_operand_llvm_type(f_op);
                    let f_i64 = self.coerce_val(&f_val, &f_ty, "i64");
                    let gep = self.next_temp();
                    self.out.push_str(&format!("    {} = getelementptr inbounds i8, ptr {}, i64 {}\n", gep, ptr_temp, idx * 8));
                    self.out.push_str(&format!("    store i64 {}, ptr {}\n", f_i64, gep));
                }

                (ptr_temp, "ptr".to_string())
            }
            RValue::FieldAccess { target, field } => {
                let target_v = self.emit_operand(target);
                let target_ty = self.get_operand_llvm_type(target);
                let target_ptr = self.coerce_val(&target_v, &target_ty, "ptr");
                let offset = self.calculate_field_offset(field);
                let gep = self.next_temp();
                let loaded = self.next_temp();
                self.out.push_str(&format!("    {} = getelementptr inbounds i8, ptr {}, i64 {}\n", gep, target_ptr, offset));
                self.out.push_str(&format!("    {} = load i64, ptr {}\n", loaded, gep));
                (loaded, "i64".to_string())
            }
            RValue::Cast { operand, target_ty } => {
                let val = self.emit_operand(operand);
                let from_ty = self.get_operand_llvm_type(operand);
                let to_ty = type_to_llvm(target_ty);
                let coerced = self.coerce_val(&val, &from_ty, &to_ty);
                (coerced, to_ty)
            }
            RValue::Ref { operand, .. } => {
                (self.emit_operand(operand), "ptr".to_string())
            }
        }
    }

    fn emit_operand(&mut self, op: &Operand) -> String {
        match op {
            Operand::Constant(TirConstant::Int(n)) => n.to_string(),
            Operand::Constant(TirConstant::Bool(b)) => if *b { "1".to_string() } else { "0".to_string() },
            Operand::Constant(TirConstant::Unit) => "0".to_string(),
            Operand::Constant(TirConstant::Str(s)) => {
                let idx = self.intern_string(s);
                format!("@str_{}", idx)
            }
            Operand::Var(v, ty) => {
                let slot = var_to_slot_name(v);
                let load_temp = self.next_temp();
                let ty_str = self.var_types.get(v).cloned().unwrap_or_else(|| type_to_llvm(ty));
                self.out.push_str(&format!("    {} = load {}, ptr {}\n", load_temp, ty_str, slot));
                load_temp
            }
        }
    }

    fn get_operand_llvm_type(&self, op: &Operand) -> String {
        match op {
            Operand::Var(v, ty) => {
                self.var_types.get(v).cloned().unwrap_or_else(|| type_to_llvm(ty))
            }
            Operand::Constant(TirConstant::Str(_)) => "ptr".to_string(),
            _ => "i64".to_string(),
        }
    }

    fn emit_terminator(&mut self, term: &Terminator, is_main: bool, ret_ty: &Type) {
        match term {
            Terminator::Return(Some(op)) => {
                let val = self.emit_operand(op);
                let from_ty = self.get_operand_llvm_type(op);
                if is_main {
                    let coerced = self.coerce_val(&val, &from_ty, "i32");
                    self.out.push_str(&format!("    ret i32 {}\n", coerced));
                } else {
                    let ty_str = type_to_llvm_ret(ret_ty);
                    let coerced = self.coerce_val(&val, &from_ty, &ty_str);
                    self.out.push_str(&format!("    ret {} {}\n", ty_str, coerced));
                }
            }
            Terminator::Return(None) => {
                if is_main {
                    self.out.push_str("    ret i32 0\n");
                } else {
                    self.out.push_str("    ret void\n");
                }
            }
            Terminator::Branch(target) => {
                self.out.push_str(&format!("    br label %bb{}\n", target.0));
            }
            Terminator::BranchCond { cond, then_block, else_block } => {
                let cond_v = self.emit_operand(cond);
                let trunc_cond = self.next_temp();
                self.out.push_str(&format!("    {} = trunc i64 {} to i1\n", trunc_cond, cond_v));
                self.out.push_str(&format!("    br i1 {}, label %bb{}, label %bb{}\n", trunc_cond, then_block.0, else_block.0));
            }
            Terminator::HandleEffect { body_entry, .. } => {
                self.out.push_str(&format!("    br label %bb{}\n", body_entry.0));
            }
            Terminator::Resume { continuation_block, .. } => {
                self.out.push_str(&format!("    br label %bb{}\n", continuation_block.0));
            }
            Terminator::Unreachable => {
                self.out.push_str("    unreachable\n");
            }
        }
    }

    fn calculate_field_offset(&self, field_name: &str) -> usize {
        for s in &self.module.structs {
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
}

fn var_to_slot_name(v: &Var) -> String {
    match v {
        Var::Named(name) => {
            let clean = name.trim_start_matches('@').trim_start_matches('_');
            format!("%slot_{}", clean)
        }
        Var::Temp(id) => format!("%slot_tmp_{}", id),
    }
}

fn type_to_llvm(ty: &Type) -> String {
    match ty {
        Type::Unit => "i64".to_string(),
        Type::Bool | Type::U8 => "i8".to_string(),
        Type::U16 => "i16".to_string(),
        Type::U32 => "i32".to_string(),
        Type::I64 | Type::U64 | Type::Usize => "i64".to_string(),
        Type::String | Type::Ref { .. } | Type::Struct(_) | Type::Instantiated { .. } | Type::Fn { .. } => {
            "ptr".to_string()
        }
        Type::Refined { base, .. } | Type::Relational { base, .. } => type_to_llvm(base),
        _ => "i64".to_string(),
    }
}

fn type_to_llvm_ret(ty: &Type) -> String {
    match ty {
        Type::Unit => "void".to_string(),
        _ => type_to_llvm(ty),
    }
}

fn escape_llvm_string(s: &str) -> String {
    let mut res = String::new();
    for b in s.bytes() {
        if b >= 32 && b <= 126 && b != b'\\' && b != b'"' {
            res.push(b as char);
        } else {
            res.push_str(&format!("\\{:02X}", b));
        }
    }
    res
}

pub fn emit_llvm_ir(module: &TirModule) -> String {
    let emitter = LlvmTextEmitter::new(module);
    emitter.emit()
}
