use std::collections::HashMap;
use tungsten_syntax::ast::*;
use tungsten_typeck::checker::TypeChecker;
use tungsten_typeck::interval::Interval;
use tungsten_typeck::types::Type;

use crate::ir::*;

pub fn lower_program(program: &Program) -> Result<TirModule, String> {
    lower_program_with_source(program, None, None)
}

pub fn lower_program_with_source(
    program: &Program,
    source_file: Option<String>,
    source_dir: Option<String>,
) -> Result<TirModule, String> {
    let mut lowerer = TirLowerer::new();
    lowerer.source_file = source_file;
    lowerer.source_dir = source_dir;
    lowerer.lower(program)
}

struct TirLowerer {
    type_checker: TypeChecker,
    blocks: Vec<BasicBlock>,
    current_block_idx: usize,
    next_temp_id: usize,
    var_types: HashMap<String, Type>,
    var_intervals: HashMap<String, Interval>,
    arena_stack: Vec<Operand>,
    next_region_id: usize,
    nursery_stack: Vec<Operand>,
    next_nursery_id: usize,
    scopes: Vec<HashMap<String, (Var, Type)>>,
    next_var_version: usize,
    continuation_stack: Vec<BlockId>,
    source_file: Option<String>,
    source_dir: Option<String>,
}

impl TirLowerer {
    fn new() -> Self {
        Self {
            type_checker: TypeChecker::new(),
            blocks: Vec::new(),
            current_block_idx: 0,
            next_temp_id: 0,
            var_types: HashMap::new(),
            var_intervals: HashMap::new(),
            arena_stack: Vec::new(),
            next_region_id: 1,
            nursery_stack: Vec::new(),
            next_nursery_id: 1,
            scopes: Vec::new(),
            next_var_version: 0,
            continuation_stack: Vec::new(),
            source_file: None,
            source_dir: None,
        }
    }

    fn push_scope(&mut self) {
        self.scopes.push(HashMap::new());
    }

    fn pop_scope(&mut self) {
        self.scopes.pop();
    }

    fn define_scoped_var(&mut self, name: &str, ty: Type) -> Var {
        let var = Var::Named(format!("{}_{}", name, self.next_var_version));
        self.next_var_version += 1;
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(name.to_string(), (var.clone(), ty));
        }
        var
    }

    fn lookup_scoped_var(&self, name: &str) -> Option<(Var, Type)> {
        for scope in self.scopes.iter().rev() {
            if let Some(entry) = scope.get(name) {
                return Some(entry.clone());
            }
        }
        None
    }

    fn new_block(&mut self, label: Option<&str>) -> BlockId {
        let id = BlockId(self.blocks.len());
        self.blocks.push(BasicBlock::new(id, label.map(|s| s.to_string())));
        id
    }

    fn set_current_block(&mut self, id: BlockId) {
        self.current_block_idx = id.0;
    }

    fn cur_block(&mut self) -> &mut BasicBlock {
        &mut self.blocks[self.current_block_idx]
    }

    fn is_current_terminated(&self) -> bool {
        self.blocks[self.current_block_idx].terminator.is_some()
    }

    fn emit(&mut self, inst: Instruction) {
        if !self.is_current_terminated() {
            self.cur_block().instructions.push(inst);
        }
    }

    fn terminate(&mut self, term: Terminator) {
        if !self.is_current_terminated() {
            self.cur_block().terminator = Some(term);
        }
    }

    fn alloc_temp(&mut self, ty: Type) -> (Var, Operand) {
        let id = self.next_temp_id;
        self.next_temp_id += 1;
        let var = Var::Temp(id);
        let operand = Operand::Var(var.clone(), ty);
        (var, operand)
    }

    fn lower(&mut self, program: &Program) -> Result<TirModule, String> {
        let mut structs = Vec::new();
        let mut enums = Vec::new();
        let mut effects = Vec::new();
        let mut extern_blocks = Vec::new();
        let mut functions = Vec::new();

        // 1. Collect type aliases and structs into type_checker
        let _ = self.type_checker.check_program(program);

        for item in &program.items {
            match item {
                Item::Struct(s) => structs.push(s.clone()),
                Item::Enum(en) => enums.push(en.clone()),
                Item::Effect(e) => effects.push(e.clone()),
                Item::ExternBlock(eb) => extern_blocks.push(eb.clone()),
                _ => {}
            }
        }

        // 2. Lower each function
        for item in &program.items {
            if let Item::Fn(fdecl) = item {
                let tir_fn = self.lower_function(fdecl);
                functions.push(tir_fn);
            }
        }

        Ok(TirModule {
            functions,
            structs,
            enums,
            effects,
            extern_blocks,
            source_file: self.source_file.clone(),
            source_dir: self.source_dir.clone(),
        })
    }

    fn lower_function(&mut self, fdecl: &FnDecl) -> TirFunction {
        self.blocks.clear();
        self.current_block_idx = 0;
        self.next_temp_id = 0;
        self.var_types.clear();
        self.var_intervals.clear();
        self.scopes.clear();
        self.push_scope();

        let entry_id = self.new_block(Some("entry"));
        self.set_current_block(entry_id);

        let mut tir_params = Vec::new();
        for p in &fdecl.params {
            let ty = self
                .type_checker
                .resolve_type_expr_with_generics(&p.ty, &fdecl.type_params)
                .unwrap_or(Type::Unit);

            let interval = match &ty {
                Type::Refined { interval, .. } => Some(*interval),
                _ => ty.default_interval(),
            };

            self.var_types.insert(p.name.clone(), ty.clone());
            if let Some(int) = interval {
                self.var_intervals.insert(p.name.clone(), int);
            }
            self.scopes
                .last_mut()
                .unwrap()
                .insert(p.name.clone(), (Var::Named(p.name.clone()), ty.clone()));

            tir_params.push(TirParam {
                name: p.name.clone(),
                is_mut: p.is_mut,
                ty,
                interval,
            });
        }

        let ret_ty = match &fdecl.return_type {
            Some(rt) => self
                .type_checker
                .resolve_type_expr_with_generics(rt, &fdecl.type_params)
                .unwrap_or(Type::Unit),
            None => Type::Unit,
        };

        // Lower the body
        let trailing = self.lower_block(&fdecl.body);

        // If the last block wasn't explicitly terminated:
        if !self.is_current_terminated() {
            if let Some(ret_op) = trailing {
                self.terminate(Terminator::Return(Some(ret_op)));
            } else if ret_ty == Type::Unit {
                self.terminate(Terminator::Return(None));
            } else {
                self.terminate(Terminator::Return(None));
            }
        }

        TirFunction {
            name: fdecl.name.clone(),
            type_params: fdecl.type_params.clone(),
            effect_params: fdecl.effect_params.clone(),
            params: tir_params,
            return_type: ret_ty,
            yields_effects: fdecl.yields_effects.clone(),
            blocks: self.blocks.clone(),
            entry_block: entry_id,
            span: fdecl.span,
        }
    }

    fn lower_block(&mut self, block: &Block) -> Option<Operand> {
        self.push_scope();
        for stmt in &block.stmts {
            self.lower_stmt(stmt);
            if self.is_current_terminated() {
                break;
            }
        }

        if self.is_current_terminated() {
            self.pop_scope();
            return None;
        }

        let res = if let Some(expr) = &block.trailing_expr {
            Some(self.lower_expr(expr))
        } else {
            None
        };
        self.pop_scope();
        res
    }

    fn lower_stmt(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::Let {
                name,
                ty,
                init,
                span,
                ..
            } => {
                let init_op = self.lower_expr(init);
                let resolved_ty = if let Some(t_expr) = ty {
                    self.type_checker.resolve_type_expr(t_expr).unwrap_or(init_op.get_type())
                } else {
                    init_op.get_type()
                };

                if let Type::Array { elem: ref arr_elem, .. } = resolved_ty {
                    let expected_stride = arr_elem.stride();
                    if let Some(inst) = self.cur_block().instructions.last_mut() {
                        if let Instruction::Assign { rvalue: RValue::ArrayInit { ref mut elem_stride, .. }, ref mut ty, .. } = inst {
                            *elem_stride = if expected_stride == 0 { 8 } else { expected_stride };
                            *ty = resolved_ty.clone();
                        }
                    }
                }

                // Check if target type has a refinement interval
                if let Type::Refined { interval, name: ref ref_name, .. } = &resolved_ty {
                    let type_name = ref_name.clone().unwrap_or_else(|| "Refined".into());
                    self.emit(Instruction::AssertRefinement {
                        operand: init_op.clone(),
                        interval: *interval,
                        error_msg: format!("Value assigned to '{}' violates refinement '{}' [{}, {}]", name, type_name, interval.min, interval.max),
                        span: *span,
                    });
                    self.var_intervals.insert(name.clone(), *interval);
                }

                self.var_types.insert(name.clone(), resolved_ty.clone());
                let dest = self.define_scoped_var(name, resolved_ty.clone());
                self.emit(Instruction::Assign {
                    dest,
                    rvalue: RValue::Use(init_op),
                    ty: resolved_ty,
                    span: *span,
                });
            }
            Stmt::Assign {
                target,
                value,
                span,
            } => {
                let val_op = self.lower_expr(value);
                match &target.kind {
                    ExprKind::Ident(name) => {
                        let (dest, ty) = self
                            .lookup_scoped_var(name)
                            .unwrap_or_else(|| (Var::Named(name.clone()), val_op.get_type()));
                        if let Some(interval) = self.var_intervals.get(name) {
                            self.emit(Instruction::AssertRefinement {
                                operand: val_op.clone(),
                                interval: *interval,
                                error_msg: format!("Value assigned to '{}' violates refinement bounds [{}, {}]", name, interval.min, interval.max),
                                span: *span,
                            });
                        }
                        self.emit(Instruction::Assign {
                            dest,
                            rvalue: RValue::Use(val_op),
                            ty,
                            span: *span,
                        });
                    }
                    ExprKind::FieldAccess { target: base, field } => {
                        if let ExprKind::Ident(base_name) = &base.kind {
                            let (base_var, _) = self
                                .lookup_scoped_var(base_name)
                                .unwrap_or_else(|| (Var::Named(base_name.clone()), Type::Unit));
                            self.emit(Instruction::SetField {
                                base: base_var,
                                field: field.clone(),
                                val: val_op,
                                span: *span,
                            });
                        }
                    }
                    ExprKind::Deref(ptr_expr) => {
                        let ptr_op = self.lower_expr(ptr_expr);
                        self.emit(Instruction::Store {
                            ptr: ptr_op,
                            value: val_op,
                            span: *span,
                        });
                    }
                    ExprKind::Index { target: base, index: idx } => {
                        let base_op = self.lower_expr(base);
                        let idx_op = self.lower_expr(idx);
                        let stride = match base_op.get_type().strip_region() {
                            Type::Array { elem, .. } => elem.stride(),
                            Type::Ref { inner, .. } => match inner.strip_region() {
                                Type::Array { elem, .. } => elem.stride(),
                                _ => 8,
                            },
                            Type::Ptr { inner, .. } => inner.stride(),
                            _ => 8,
                        };
                        let stride = if stride == 0 { 8 } else { stride };
                        self.emit(Instruction::StoreIndex {
                            target: base_op,
                            index: idx_op,
                            stride,
                            value: val_op,
                            span: *span,
                        });
                    }
                    _ => {}
                }
            }
            Stmt::Expr { expr, .. } => {
                self.lower_expr(expr);
            }
            Stmt::Return { value, span } => {
                let ret_op = value.as_ref().map(|v| self.lower_expr(v));
                let arenas: Vec<Operand> = self.arena_stack.iter().cloned().rev().collect();
                for arena in arenas {
                    self.emit(Instruction::RegionExit {
                        arena,
                        span: *span,
                    });
                }
                self.terminate(Terminator::Return(ret_op));
            }
        }
    }

    fn lower_expr(&mut self, expr: &Expr) -> Operand {
        match &expr.kind {
            ExprKind::Int(val) => Operand::Constant(TirConstant::Int(*val)),
            ExprKind::Str(val) => Operand::Constant(TirConstant::Str(val.clone())),
            ExprKind::Bool(val) => Operand::Constant(TirConstant::Bool(*val)),
            ExprKind::Ident(name) => {
                let (var, ty) = self
                    .lookup_scoped_var(name)
                    .unwrap_or_else(|| {
                        let ty = self.var_types.get(name).cloned().unwrap_or(Type::Unit);
                        (Var::Named(name.clone()), ty)
                    });
                Operand::Var(var, ty)
            }
            ExprKind::Binary { op, left, right } => {
                let l_op = self.lower_expr(left);
                let r_op = self.lower_expr(right);
                let ty = match op {
                    BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div => l_op.get_type(),
                    _ => Type::Bool,
                };
                let (dest, res_op) = self.alloc_temp(ty.clone());
                self.emit(Instruction::Assign {
                    dest,
                    rvalue: RValue::BinaryOp(*op, l_op, r_op),
                    ty,
                    span: expr.span,
                });
                res_op
            }
            ExprKind::FieldAccess { target, field } => {
                let t_op = self.lower_expr(target);
                let field_ty = match t_op.get_type() {
                    Type::Struct(ref s_name) => {
                        self.type_checker
                            .structs
                            .get(s_name)
                            .and_then(|fields| fields.get(field))
                            .cloned()
                            .unwrap_or(Type::Unit)
                    }
                    _ => Type::Unit,
                };
                let (dest, res_op) = self.alloc_temp(field_ty.clone());
                self.emit(Instruction::Assign {
                    dest,
                    rvalue: RValue::FieldAccess {
                        target: t_op,
                        field: field.clone(),
                    },
                    ty: field_ty,
                    span: expr.span,
                });
                res_op
            }
            ExprKind::MethodCall { target, method, args } => {
                let t_op = self.lower_expr(target);
                let arg_ops = args.iter().map(|a| self.lower_expr(a)).collect();
                let ty = if method == "to_string" {
                    Type::String
                } else {
                    t_op.get_type()
                };
                let (dest, res_op) = self.alloc_temp(ty.clone());
                self.emit(Instruction::Assign {
                    dest,
                    rvalue: RValue::MethodCall {
                        target: t_op,
                        method: method.clone(),
                        args: arg_ops,
                    },
                    ty,
                    span: expr.span,
                });
                res_op
            }
            ExprKind::PathCall { path, args } => {
                let arg_ops: Vec<Operand> = args.iter().map(|a| self.lower_expr(a)).collect();
                if path.len() == 2 {
                    if let Some(en) = self.type_checker.enums.get(&path[0]).cloned() {
                        if let Some((tag, _)) = en.variants.iter().enumerate().find(|(_, v)| v.name == path[1]) {
                            let enum_ty = if !en.type_params.is_empty() {
                                let arg_tys: Vec<Type> = arg_ops.iter().map(|a: &Operand| a.get_type()).collect();
                                Type::Instantiated { name: path[0].clone(), args: arg_tys }
                            } else {
                                Type::Enum(path[0].clone())
                            };
                            let (dest, res_op) = self.alloc_temp(enum_ty.clone());
                            let arena = self.arena_stack.last().cloned();
                            self.emit(Instruction::Assign {
                                dest,
                                rvalue: RValue::EnumInit {
                                    enum_name: path[0].clone(),
                                    variant: path[1].clone(),
                                    tag,
                                    payload: arg_ops,
                                    arena,
                                },
                                ty: enum_ty,
                                span: expr.span,
                            });
                            return res_op;
                        }
                    }

                    let effect = path[0].clone();
                    let op = path[1].clone();
                    let ty = if effect == "IO" && op == "print" {
                        Type::Unit
                    } else if effect == "Random" {
                        Type::I64
                    } else if effect == "Foreign" || effect == "ForeignCall" {
                        if let Some(first_arg) = args.first() {
                            if let ExprKind::Ident(ref fname) = first_arg.kind {
                                self.type_checker.extern_fns.get(fname).map(|s| s.return_type.clone()).unwrap_or(Type::I64)
                            } else {
                                Type::I64
                            }
                        } else {
                            Type::I64
                        }
                    } else {
                        Type::Unit
                    };
                    let (dest, res_op) = self.alloc_temp(ty.clone());
                    self.emit(Instruction::PerformEffect {
                        effect,
                        op,
                        args: arg_ops,
                        dest: Some(dest),
                        ty,
                        span: expr.span,
                    });
                    res_op
                } else {
                    Operand::Constant(TirConstant::Unit)
                }
            }
            ExprKind::Call { callee, args } => {
                let c_op = self.lower_expr(callee);
                let arg_ops = args.iter().map(|a| self.lower_expr(a)).collect();
                let callee_name = match &callee.kind {
                    ExprKind::Ident(name) => Some(name.as_str()),
                    _ => None,
                };
                if let Some(name) = callee_name {
                    if self.type_checker.extern_fns.contains_key(name) {
                        let ret_ty = self.type_checker.extern_fns.get(name).map(|s| s.return_type.clone()).unwrap_or(Type::Unit);
                        let (dest, res_op) = self.alloc_temp(ret_ty.clone());
                        self.emit(Instruction::ExternCall {
                            dest: Some(dest),
                            func: name.to_string(),
                            args: arg_ops,
                            ty: ret_ty,
                            span: expr.span,
                        });
                        return res_op;
                    }
                }
                let ret_ty = if let Some(name) = callee_name {
                    self.type_checker
                        .functions
                        .get(name)
                        .map(|sig| sig.return_type.clone())
                        .or_else(|| {
                            self.type_checker
                                .generic_functions
                                .get(name)
                                .and_then(|f| {
                                    f.return_type.as_ref().and_then(|rt| {
                                        self.type_checker
                                            .resolve_type_expr_with_generics(rt, &f.type_params)
                                            .ok()
                                    })
                                })
                        })
                        .unwrap_or(Type::Unit)
                } else {
                    Type::Unit
                };
                let (dest, res_op) = self.alloc_temp(ret_ty.clone());
                self.emit(Instruction::Call {
                    dest: Some(dest),
                    func: c_op,
                    args: arg_ops,
                    ty: ret_ty,
                    span: expr.span,
                });
                res_op
            }
            ExprKind::MacroCall { name, args } => {
                let arg_ops = args.iter().map(|a| self.lower_expr(a)).collect();
                self.emit(Instruction::Call {
                    dest: None,
                    func: Operand::Var(Var::Named(format!("@{}", name)), Type::Unit),
                    args: arg_ops,
                    ty: Type::Unit,
                    span: expr.span,
                });
                Operand::Constant(TirConstant::Unit)
            }
            ExprKind::StructInit { name, fields } => {
                let f_ops: Vec<(String, Operand)> = fields
                    .iter()
                    .map(|(k, v)| (k.clone(), self.lower_expr(v)))
                    .collect();
                let ty = Type::Struct(name.clone());
                let (dest, res_op) = self.alloc_temp(ty.clone());
                let arena = self.arena_stack.last().cloned();
                self.emit(Instruction::Assign {
                    dest,
                    rvalue: RValue::StructInit {
                        name: name.clone(),
                        fields: f_ops,
                        arena,
                    },
                    ty,
                    span: expr.span,
                });
                res_op
            }
            ExprKind::Cast { expr: inner, target_ty } => {
                let inner_op = self.lower_expr(inner);
                let resolved_ty = self.type_checker.resolve_type_expr(target_ty).unwrap_or(inner_op.get_type());
                if let Type::Refined { interval, ref name, .. } = &resolved_ty {
                    let type_name = name.clone().unwrap_or_else(|| "Refined".into());
                    self.emit(Instruction::AssertRefinement {
                        operand: inner_op.clone(),
                        interval: *interval,
                        error_msg: format!("Cast to refinement '{}' outside interval [{}, {}]", type_name, interval.min, interval.max),
                        span: expr.span,
                    });
                }
                let (dest, res_op) = self.alloc_temp(resolved_ty.clone());
                self.emit(Instruction::Assign {
                    dest,
                    rvalue: RValue::Cast {
                        operand: inner_op,
                        target_ty: resolved_ty.clone(),
                    },
                    ty: resolved_ty,
                    span: expr.span,
                });
                res_op
            }
            ExprKind::Ref { is_mut, expr: inner } => {
                let inner_op = self.lower_expr(inner);
                let ty = Type::Ref {
                    is_mut: *is_mut,
                    inner: Box::new(inner_op.get_type()),
                    region: None,
                };
                let (dest, res_op) = self.alloc_temp(ty.clone());
                self.emit(Instruction::Assign {
                    dest,
                    rvalue: RValue::Ref {
                        is_mut: *is_mut,
                        operand: inner_op,
                    },
                    ty,
                    span: expr.span,
                });
                res_op
            }
            ExprKind::If { cond, then_branch, else_branch } => {
                let cond_op = self.lower_expr(cond);
                let then_bb = self.new_block(Some("then"));
                let else_bb = if else_branch.is_some() {
                    self.new_block(Some("else"))
                } else {
                    BlockId(0)
                };
                let merge_bb = self.new_block(Some("if_merge"));

                let actual_else_bb = if else_branch.is_some() { else_bb } else { merge_bb };

                self.terminate(Terminator::BranchCond {
                    cond: cond_op,
                    then_block: then_bb,
                    else_block: actual_else_bb,
                });

                // Allocate a temp var for the result of the if expression
                let (res_var, res_op) = self.alloc_temp(Type::I64);

                // Then branch
                self.set_current_block(then_bb);
                let then_res = self.lower_block(then_branch);
                if !self.is_current_terminated() {
                    if let Some(tr) = then_res {
                        self.emit(Instruction::Assign {
                            dest: res_var.clone(),
                            rvalue: RValue::Use(tr),
                            ty: Type::I64,
                            span: expr.span,
                        });
                    }
                    self.terminate(Terminator::Branch(merge_bb));
                }

                // Else branch
                if let Some(e_branch) = else_branch {
                    self.set_current_block(else_bb);
                    let else_res = self.lower_block(e_branch);
                    if !self.is_current_terminated() {
                        if let Some(er) = else_res {
                            self.emit(Instruction::Assign {
                                dest: res_var.clone(),
                                rvalue: RValue::Use(er),
                                ty: Type::I64,
                                span: expr.span,
                            });
                        }
                        self.terminate(Terminator::Branch(merge_bb));
                    }
                }

                self.set_current_block(merge_bb);
                res_op
            }
            ExprKind::Handle { body, handlers } => {
                let body_bb = self.new_block(Some("handle_body"));
                let exit_bb = self.new_block(Some("handle_exit"));

                let mut tir_handlers = Vec::new();
                for clause in handlers {
                    for arm in &clause.arms {
                        let handler_bb = self.new_block(Some(&format!("{}_{}", clause.effect_name, arm.op_name)));
                        tir_handlers.push((
                            TirHandler {
                                effect: clause.effect_name.clone(),
                                op: arm.op_name.clone(),
                                param: arm.params.get(0).cloned(),
                                handler_entry: handler_bb,
                            },
                            arm,
                        ));
                    }
                }

                let handler_defs: Vec<TirHandler> = tir_handlers.iter().map(|(h, _)| h.clone()).collect();
                self.terminate(Terminator::HandleEffect {
                    body_entry: body_bb,
                    handlers: handler_defs,
                    exit_block: exit_bb,
                });

                // Lower handler body
                self.set_current_block(body_bb);
                self.lower_block(body);
                if !self.is_current_terminated() {
                    self.terminate(Terminator::Branch(exit_bb));
                }

                // Lower handler arms
                for (th, arm) in tir_handlers {
                    self.set_current_block(th.handler_entry);
                    if let Some(param_name) = &th.param {
                        self.var_types.insert(param_name.clone(), Type::String);
                    }
                    self.continuation_stack.push(exit_bb);
                    let arm_res = self.lower_expr(&arm.body);
                    self.continuation_stack.pop();
                    if !self.is_current_terminated() {
                        self.terminate(Terminator::Resume {
                            arg: Some(arm_res),
                            continuation_block: exit_bb,
                        });
                    }
                }

                self.set_current_block(exit_bb);
                Operand::Constant(TirConstant::Unit)
            }
            ExprKind::Resume(inner) => {
                let res = self.lower_expr(inner);
                let cont_bb = self.continuation_stack.last().copied().unwrap_or(BlockId(0));
                self.terminate(Terminator::Resume {
                    arg: Some(res.clone()),
                    continuation_block: cont_bb,
                });
                res
            }
            ExprKind::Block(inner_block) => {
                self.lower_block(inner_block).unwrap_or(Operand::Constant(TirConstant::Unit))
            }
            ExprKind::Loop(body) => {
                let loop_header = self.new_block(Some("loop_header"));
                let loop_exit = self.new_block(Some("loop_exit"));
                self.terminate(Terminator::Branch(loop_header));

                self.set_current_block(loop_header);
                self.lower_block(body);
                if !self.is_current_terminated() {
                    self.terminate(Terminator::Branch(loop_header));
                }

                self.set_current_block(loop_exit);
                Operand::Constant(TirConstant::Unit)
            }
            ExprKind::Region { name, body } => {
                let region_id = self.next_region_id;
                self.next_region_id += 1;
                let ptr_u8_ty = Type::Ptr { is_mut: true, inner: Box::new(Type::U8) };
                let (arena_dest, arena_op) = self.alloc_temp(ptr_u8_ty.clone());
                self.emit(Instruction::RegionEnter {
                    dest: arena_dest,
                    region_id,
                    span: expr.span,
                });
                self.push_scope();
                if let Some(r_name) = name {
                    let var = self.define_scoped_var(r_name, ptr_u8_ty.clone());
                    self.emit(Instruction::Assign {
                        dest: var,
                        rvalue: RValue::Use(arena_op.clone()),
                        ty: ptr_u8_ty,
                        span: expr.span,
                    });
                }
                self.arena_stack.push(arena_op.clone());
                let res = self.lower_block(body).unwrap_or(Operand::Constant(TirConstant::Unit));
                self.arena_stack.pop();
                self.pop_scope();
                self.emit(Instruction::RegionExit {
                    arena: arena_op,
                    span: expr.span,
                });
                res
            }
            ExprKind::Nursery { name, body } => {
                let nursery_id = self.next_nursery_id;
                self.next_nursery_id += 1;
                let (nursery_dest, nursery_op) = self.alloc_temp(Type::Struct("Nursery".into()));
                self.emit(Instruction::NurseryEnter {
                    dest: nursery_dest,
                    nursery_id,
                    span: expr.span,
                });
                self.push_scope();
                if let Some(n) = name {
                    let var = self.define_scoped_var(n, Type::Struct("Nursery".into()));
                    self.emit(Instruction::Assign {
                        dest: var,
                        rvalue: RValue::Use(nursery_op.clone()),
                        ty: Type::Struct("Nursery".into()),
                        span: expr.span,
                    });
                }
                self.nursery_stack.push(nursery_op.clone());
                let res = self.lower_block(body).unwrap_or(Operand::Constant(TirConstant::Unit));
                self.nursery_stack.pop();
                self.pop_scope();
                self.emit(Instruction::NurseryExit {
                    nursery: nursery_op,
                    span: expr.span,
                });
                res
            }
            ExprKind::Path(path) => {
                if path.len() == 2 {
                    if let Some(en) = self.type_checker.enums.get(&path[0]).cloned() {
                        if let Some((tag, _)) = en.variants.iter().enumerate().find(|(_, v)| v.name == path[1]) {
                            let enum_ty = Type::Enum(path[0].clone());
                            let (dest, res_op) = self.alloc_temp(enum_ty.clone());
                            let arena = self.arena_stack.last().cloned();
                            self.emit(Instruction::Assign {
                                dest,
                                rvalue: RValue::EnumInit {
                                    enum_name: path[0].clone(),
                                    variant: path[1].clone(),
                                    tag,
                                    payload: Vec::new(),
                                    arena,
                                },
                                ty: enum_ty,
                                span: expr.span,
                            });
                            return res_op;
                        }
                    }
                }
                Operand::Constant(TirConstant::Unit)
            }
            ExprKind::Array(elements) => {
                let elem_ops: Vec<Operand> = elements.iter().map(|e| self.lower_expr(e)).collect();
                let (elem_ty, elem_stride) = if let Some(first) = elem_ops.first() {
                    let ty = first.get_type();
                    let s = ty.stride();
                    (ty, if s == 0 { 8 } else { s })
                } else {
                    (Type::Unit, 8)
                };
                let arr_ty = Type::Array {
                    elem: Box::new(elem_ty),
                    len: elem_ops.len(),
                };
                let (dest, res_op) = self.alloc_temp(arr_ty.clone());
                let arena = self.arena_stack.last().cloned();
                self.emit(Instruction::Assign {
                    dest,
                    rvalue: RValue::ArrayInit {
                        elements: elem_ops,
                        elem_stride,
                        arena,
                    },
                    ty: arr_ty,
                    span: expr.span,
                });
                res_op
            }
            ExprKind::Index { target, index } => {
                let t_op = self.lower_expr(target);
                let i_op = self.lower_expr(index);
                let (elem_ty, stride) = match t_op.get_type().strip_region() {
                    Type::Array { elem, .. } => {
                        let s = elem.stride();
                        (*elem.clone(), if s == 0 { 8 } else { s })
                    }
                    Type::Ref { inner, .. } => match inner.strip_region() {
                        Type::Array { elem, .. } => {
                            let s = elem.stride();
                            (*elem.clone(), if s == 0 { 8 } else { s })
                        }
                        _ => (Type::I64, 8),
                    },
                    Type::Ptr { inner, .. } => {
                        let s = inner.stride();
                        (*inner.clone(), if s == 0 { 8 } else { s })
                    }
                    _ => (Type::I64, 8),
                };
                let (dest, res_op) = self.alloc_temp(elem_ty.clone());
                self.emit(Instruction::Assign {
                    dest,
                    rvalue: RValue::ArrayIndex {
                        target: t_op,
                        index: i_op,
                        stride,
                    },
                    ty: elem_ty,
                    span: expr.span,
                });
                res_op
            }
            ExprKind::Match { expr: scrutinee, arms } => {
                let scrut_op = self.lower_expr(scrutinee);
                let (res_var, res_op) = self.alloc_temp(Type::I64);
                let merge_bb = self.new_block(Some("match_merge"));

                for (arm_idx, arm) in arms.iter().enumerate() {
                    match &arm.pattern {
                        Pattern::Wildcard(_) => {
                            let arm_bb = self.new_block(Some(&format!("match_arm_{}", arm_idx)));
                            self.terminate(Terminator::Branch(arm_bb));
                            self.set_current_block(arm_bb);
                            let arm_val = self.lower_expr(&arm.body);
                            self.emit(Instruction::Assign {
                                dest: res_var.clone(),
                                rvalue: RValue::Use(arm_val),
                                ty: Type::I64,
                                span: arm.span,
                            });
                            self.terminate(Terminator::Branch(merge_bb));
                            break;
                        }
                        Pattern::Variable(name, _) => {
                            let arm_bb = self.new_block(Some(&format!("match_arm_{}", arm_idx)));
                            self.terminate(Terminator::Branch(arm_bb));
                            self.set_current_block(arm_bb);
                            self.push_scope();
                            let v_dest = self.define_scoped_var(name, scrut_op.get_type());
                            self.emit(Instruction::Assign {
                                dest: v_dest,
                                rvalue: RValue::Use(scrut_op.clone()),
                                ty: scrut_op.get_type(),
                                span: arm.span,
                            });
                            let arm_val = self.lower_expr(&arm.body);
                            self.pop_scope();
                            self.emit(Instruction::Assign {
                                dest: res_var.clone(),
                                rvalue: RValue::Use(arm_val),
                                ty: Type::I64,
                                span: arm.span,
                            });
                            self.terminate(Terminator::Branch(merge_bb));
                            break;
                        }
                        Pattern::Variant { enum_name, variant_name, subpatterns, span: _ } => {
                            let tag = if let Some(ref en_name) = enum_name {
                                self.type_checker.enums.get(en_name)
                                    .and_then(|en| en.variants.iter().position(|v| &v.name == variant_name))
                                    .unwrap_or(0)
                            } else {
                                match scrut_op.get_type() {
                                    Type::Enum(ref en_name) | Type::Instantiated { name: ref en_name, .. } => {
                                        self.type_checker.enums.get(en_name)
                                            .and_then(|en| en.variants.iter().position(|v| &v.name == variant_name))
                                            .unwrap_or(0)
                                    }
                                    _ => {
                                        self.type_checker.enums.values()
                                            .find_map(|en| en.variants.iter().position(|v| &v.name == variant_name))
                                            .unwrap_or(0)
                                    }
                                }
                            };

                            let (tag_dest, tag_op) = self.alloc_temp(Type::I64);
                            self.emit(Instruction::Assign {
                                dest: tag_dest,
                                rvalue: RValue::EnumTag(scrut_op.clone()),
                                ty: Type::I64,
                                span: arm.span,
                            });
                            let (cmp_dest, cmp_op) = self.alloc_temp(Type::Bool);
                            self.emit(Instruction::Assign {
                                dest: cmp_dest,
                                rvalue: RValue::BinaryOp(
                                    BinOp::Eq,
                                    tag_op,
                                    Operand::Constant(TirConstant::Int(tag as i64)),
                                ),
                                ty: Type::Bool,
                                span: arm.span,
                            });

                            let arm_bb = self.new_block(Some(&format!("arm_{}", variant_name)));
                            let next_bb = self.new_block(Some(&format!("arm_next_{}", arm_idx)));
                            self.terminate(Terminator::BranchCond {
                                cond: cmp_op,
                                then_block: arm_bb,
                                else_block: next_bb,
                            });

                            self.set_current_block(arm_bb);
                            self.push_scope();
                            for (sp_idx, sp) in subpatterns.iter().enumerate() {
                                if let Pattern::Variable(ref var_name, _) = sp {
                                    let (payload_dest, payload_op) = self.alloc_temp(Type::I64);
                                    self.emit(Instruction::Assign {
                                        dest: payload_dest,
                                        rvalue: RValue::EnumPayload {
                                            target: scrut_op.clone(),
                                            index: sp_idx,
                                        },
                                        ty: Type::I64,
                                        span: arm.span,
                                    });
                                    let var_dest = self.define_scoped_var(var_name, Type::I64);
                                    self.emit(Instruction::Assign {
                                        dest: var_dest,
                                        rvalue: RValue::Use(payload_op),
                                        ty: Type::I64,
                                        span: arm.span,
                                    });
                                }
                            }
                            let arm_val = self.lower_expr(&arm.body);
                            self.pop_scope();
                            self.emit(Instruction::Assign {
                                dest: res_var.clone(),
                                rvalue: RValue::Use(arm_val),
                                ty: Type::I64,
                                span: arm.span,
                            });
                            self.terminate(Terminator::Branch(merge_bb));

                            self.set_current_block(next_bb);
                        }
                        _ => {}
                    }
                }

                if !self.is_current_terminated() {
                    self.terminate(Terminator::Branch(merge_bb));
                }

                self.set_current_block(merge_bb);
                res_op
            }
            ExprKind::Try(inner) | ExprKind::EffectCall(inner) | ExprKind::Await(inner) => {
                self.lower_expr(inner)
            }
            ExprKind::Unsafe { body } => {
                self.lower_block(body).unwrap_or(Operand::Constant(TirConstant::Unit))
            }
            ExprKind::Deref(inner) => {
                let inner_op = self.lower_expr(inner);
                let ty = match inner_op.get_type() {
                    Type::Ptr { inner, .. } => *inner,
                    Type::Ref { inner, .. } => *inner,
                    _ => Type::I64,
                };
                let (dest, res_op) = self.alloc_temp(ty.clone());
                self.emit(Instruction::Assign {
                    dest,
                    rvalue: RValue::Deref(inner_op),
                    ty,
                    span: expr.span,
                });
                res_op
            }
            ExprKind::AddrOf { mutable, expr: inner } => {
                let inner_op = self.lower_expr(inner);
                let ty = Type::Ptr {
                    is_mut: *mutable,
                    inner: Box::new(inner_op.get_type()),
                };
                let (dest, res_op) = self.alloc_temp(ty.clone());
                self.emit(Instruction::Assign {
                    dest,
                    rvalue: RValue::AddrOf(inner_op),
                    ty,
                    span: expr.span,
                });
                res_op
            }
        }
    }
}
