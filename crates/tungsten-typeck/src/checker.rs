use std::collections::{HashMap, HashSet};
use tungsten_syntax::ast::*;
use tungsten_syntax::token::Span;

use crate::interval::Interval;
use crate::types::{RegionId, Type};

#[derive(Debug, Clone)]
pub struct TypeError {
    pub message: String,
    pub span: Span,
}

impl TypeError {
    pub fn new(message: impl Into<String>, span: Span) -> Self {
        Self {
            message: message.into(),
            span,
        }
    }
}

pub struct TypeChecker {
    pub types: HashMap<String, Type>,
    pub structs: HashMap<String, HashMap<String, Type>>,
    pub generic_structs: HashMap<String, StructDecl>,
    pub enums: HashMap<String, EnumDecl>,
    pub effect_decls: HashMap<String, EffectDecl>,
    pub functions: HashMap<String, FnSig>,
    pub generic_functions: HashMap<String, FnDecl>,
    pub extern_fns: HashMap<String, FnSig>,
    pub known_effects: HashSet<String>,
    pub errors: Vec<TypeError>,
}

#[derive(Debug, Clone)]
pub struct FnSig {
    pub params: Vec<(String, Type, bool)>, // (name, ty, is_mut)
    pub param_type_exprs: Vec<TypeExpr>,
    pub return_type: Type,
    pub yields_effects: HashSet<String>,
    pub is_extern: bool,
}

#[derive(Clone)]
struct Scope {
    variables: HashMap<String, (Type, bool)>, // (type, is_mut)
    region: RegionId,
}

#[derive(Clone)]
pub struct HandlerArmCtx {
    pub effect_name: String,
    pub op_name: String,
    pub expected_resume_ty: Type,
    pub resume_count: usize,
}

pub struct FnChecker<'a> {
    parent: &'a TypeChecker,
    current_type_params: Vec<String>,
    scopes: Vec<Scope>,
    allowed_yields: HashSet<String>,
    handled_effects: Vec<(HashSet<String>, RegionId)>,
    current_handler_arm: Option<HandlerArmCtx>,
    pub relational_ctx: crate::relational::RelationalContext,
    errors: Vec<TypeError>,
    region_stack: Vec<RegionId>,
    next_region_id: usize,
    in_unsafe: bool,
}


impl TypeChecker {
    pub fn new() -> Self {
        let mut tc = Self {
            types: HashMap::new(),
            structs: HashMap::new(),
            generic_structs: HashMap::new(),
            enums: HashMap::new(),
            effect_decls: HashMap::new(),
            functions: HashMap::new(),
            generic_functions: HashMap::new(),
            extern_fns: HashMap::new(),
            known_effects: HashSet::new(),
            errors: Vec::new(),
        };

        // Primitive types
        tc.types.insert("u8".into(), Type::U8);
        tc.types.insert("i8".into(), Type::U8);
        tc.types.insert("u16".into(), Type::U16);
        tc.types.insert("i16".into(), Type::U16);
        tc.types.insert("u32".into(), Type::U32);
        tc.types.insert("i32".into(), Type::U32);
        tc.types.insert("u64".into(), Type::U64);
        tc.types.insert("i64".into(), Type::I64);
        tc.types.insert("usize".into(), Type::Usize);
        tc.types.insert("isize".into(), Type::I64);
        tc.types.insert("bool".into(), Type::Bool);
        tc.types.insert("String".into(), Type::String);

        // Standard algebraic effects
        tc.known_effects.insert("Db".into());
        tc.known_effects.insert("IOError".into());
        tc.known_effects.insert("FileSystem".into());
        tc.known_effects.insert("Network".into());
        tc.known_effects.insert("Net".into());
        tc.known_effects.insert("IO".into());
        tc.known_effects.insert("Random".into());
        tc.known_effects.insert("State".into());
        tc.known_effects.insert("Time".into());
        tc.known_effects.insert("Yield".into());
        tc.known_effects.insert("Async".into());
        tc.known_effects.insert("Channel".into());
        tc.known_effects.insert("Nursery".into());
        tc.known_effects.insert("Foreign".into());
        tc.known_effects.insert("ForeignCall".into());

        // Concurrency types
        tc.types.insert("FiberHandle".into(), Type::Struct("FiberHandle".into()));
        tc.types.insert("Nursery".into(), Type::Struct("Nursery".into()));

        // Standard prelude refinement types
        tc.types.insert(
            "Percentage".into(),
            Type::Refined {
                base: Box::new(Type::U8),
                interval: Interval::new(0, 100),
                name: Some("Percentage".into()),
            },
        );
        tc.types.insert(
            "Port".into(),
            Type::Refined {
                base: Box::new(Type::U16),
                interval: Interval::new(1, 65535),
                name: Some("Port".into()),
            },
        );
        tc.types.insert(
            "NonZeroU32".into(),
            Type::Refined {
                base: Box::new(Type::U32),
                interval: Interval::new(1, u32::MAX as i64),
                name: Some("NonZeroU32".into()),
            },
        );
        tc.types.insert(
            "Byte".into(),
            Type::Refined {
                base: Box::new(Type::U8),
                interval: Interval::new(0, 255),
                name: Some("Byte".into()),
            },
        );

        // Standard Record struct returned by DB / I/O
        let mut record_fields = HashMap::new();
        record_fields.insert("name".into(), Type::String);
        record_fields.insert("hp".into(), Type::I64);
        record_fields.insert("id".into(), Type::U64);
        tc.structs.insert("Record".into(), record_fields);
        tc.types.insert("Record".into(), Type::Struct("Record".into()));

        tc
    }

    pub fn resolve_type_expr(&self, ty_expr: &TypeExpr) -> Result<Type, TypeError> {
        self.resolve_type_expr_with_generics(ty_expr, &[])
    }

    pub fn resolve_type_expr_with_generics(&self, ty_expr: &TypeExpr, type_params: &[String]) -> Result<Type, TypeError> {
        match ty_expr {
            TypeExpr::Named(name, span) => {
                if type_params.iter().any(|tp| tp == name) {
                    Ok(Type::GenericParam(name.clone()))
                } else if let Some(ty) = self.types.get(name) {
                    Ok(ty.clone())
                } else if self.structs.contains_key(name) || self.generic_structs.contains_key(name) {
                    Ok(Type::Struct(name.clone()))
                } else if self.enums.contains_key(name) {
                    Ok(Type::Enum(name.clone()))
                } else {
                    Err(TypeError::new(format!("Unknown type '{}'", name), *span))
                }
            }
            TypeExpr::Refined { base, min, max, inclusive, span } => {
                let base_ty = self.types.get(base).cloned().unwrap_or(Type::U8);
                let actual_max = if *inclusive { *max } else { *max - 1 };
                if min > &actual_max {
                    return Err(TypeError::new(
                        format!("Invalid refinement range: min {} is greater than max {}", min, actual_max),
                        *span,
                    ));
                }
                let interval = Interval::new(*min, actual_max);
                Ok(Type::Refined {
                    base: Box::new(base_ty),
                    interval,
                    name: None,
                })
            }
            TypeExpr::Generic { name, args, span } => {
                let mut resolved_args = Vec::new();
                for a in args {
                    resolved_args.push(self.resolve_type_expr_with_generics(a, type_params)?);
                }
                if self.structs.contains_key(name) || self.generic_structs.contains_key(name) || self.enums.contains_key(name) {
                    Ok(Type::Instantiated {
                        name: name.clone(),
                        args: resolved_args,
                    })
                } else if self.types.contains_key(name) {
                    Ok(Type::Instantiated {
                        name: name.clone(),
                        args: resolved_args,
                    })
                } else {
                    Err(TypeError::new(format!("Unknown generic type '{}'", name), *span))
                }
            }
            TypeExpr::Relational { base, predicate, span: _ } => {
                let base_ty = self.types.get(base).cloned().unwrap_or(Type::Usize);
                let pred_desc = tungsten_syntax::fmt::format_expr(predicate);
                Ok(Type::Relational {
                    base: Box::new(base_ty),
                    predicate_desc: pred_desc,
                })
            }
            TypeExpr::Fn { params, return_type, yields_effects, .. } => {
                let mut p_types = Vec::new();
                for p in params {
                    p_types.push(self.resolve_type_expr_with_generics(p, type_params)?);
                }
                let ret_type = self.resolve_type_expr_with_generics(return_type, type_params)?;
                Ok(Type::Fn {
                    params: p_types,
                    return_type: Box::new(ret_type),
                    yields_effects: yields_effects.clone(),
                })
            }
            TypeExpr::Ref { is_mut, inner, .. } => {
                let inner_ty = self.resolve_type_expr_with_generics(inner, type_params)?;
                Ok(Type::Ref {
                    is_mut: *is_mut,
                    inner: Box::new(inner_ty),
                    region: None,
                })
            }
            TypeExpr::Ptr { mutable, inner, .. } => {
                let inner_ty = self.resolve_type_expr_with_generics(inner, type_params)?;
                Ok(Type::Ptr {
                    is_mut: *mutable,
                    inner: Box::new(inner_ty),
                })
            }
            TypeExpr::Array { elem, len, .. } => {
                let elem_ty = self.resolve_type_expr_with_generics(elem, type_params)?;
                Ok(Type::Array {
                    elem: Box::new(elem_ty),
                    len: *len,
                })
            }
            TypeExpr::Unit(_) => Ok(Type::Unit),
        }
    }

    pub fn check_program(&mut self, program: &Program) -> Result<(), Vec<TypeError>> {
        // Pass 0: Register Effects
        for item in &program.items {
            if let Item::Effect(eff) = item {
                self.known_effects.insert(eff.name.clone());
                self.effect_decls.insert(eff.name.clone(), eff.clone());
            }
        }

        // Pass 1: Register Type Aliases
        for item in &program.items {
            if let Item::TypeAlias(alias) = item {
                match self.resolve_type_expr(&alias.target) {
                    Ok(mut ty) => {
                        if let Type::Refined { ref mut name, .. } = ty {
                            *name = Some(alias.name.clone());
                        }
                        self.types.insert(alias.name.clone(), ty);
                    }
                    Err(e) => self.errors.push(e),
                }
            }
        }

        // Pass 2: Register Structs
        for item in &program.items {
            if let Item::Struct(st) = item {
                if !st.type_params.is_empty() {
                    self.generic_structs.insert(st.name.clone(), st.clone());
                    self.types.insert(st.name.clone(), Type::Struct(st.name.clone()));
                } else {
                    let mut fields = HashMap::new();
                    for f in &st.fields {
                        match self.resolve_type_expr(&f.ty) {
                            Ok(ty) => {
                                fields.insert(f.name.clone(), ty);
                            }
                            Err(e) => self.errors.push(e),
                        }
                    }
                    self.structs.insert(st.name.clone(), fields);
                    self.types.insert(st.name.clone(), Type::Struct(st.name.clone()));
                }
            }
        }

        // Pass 2.5: Register Enums
        for item in &program.items {
            if let Item::Enum(en) = item {
                self.enums.insert(en.name.clone(), en.clone());
                self.types.insert(en.name.clone(), Type::Enum(en.name.clone()));
            }
        }

        // Pass 3: Register Functions
        for item in &program.items {
            if let Item::Fn(f) = item {
                for eff in &f.yields_effects {
                    self.known_effects.insert(eff.clone());
                }

                if !f.type_params.is_empty() || !f.effect_params.is_empty() {
                    self.generic_functions.insert(f.name.clone(), f.clone());
                } else {
                    let mut params = Vec::new();
                    for p in &f.params {
                        match self.resolve_type_expr(&p.ty) {
                            Ok(ty) => params.push((p.name.clone(), ty, p.is_mut)),
                            Err(e) => self.errors.push(e),
                        }
                    }
                    let ret_ty = match &f.return_type {
                        Some(rt) => self.resolve_type_expr(rt).unwrap_or(Type::Unit),
                        None => Type::Unit,
                    };
                    let yields_set: HashSet<String> = f.yields_effects.iter().cloned().collect();
                    let param_type_exprs = f.params.iter().map(|p| p.ty.clone()).collect();
                    self.functions.insert(
                        f.name.clone(),
                        FnSig {
                            params,
                            param_type_exprs,
                            return_type: ret_ty,
                            yields_effects: yields_set,
                            is_extern: false,
                        },
                    );
                }
            }
        }

        // Pass 3.5: Register Extern Blocks
        for item in &program.items {
            if let Item::ExternBlock(eb) = item {
                for f in &eb.fns {
                    let mut params = Vec::new();
                    let mut param_type_exprs = Vec::new();
                    for (pname, pty) in &f.params {
                        match self.resolve_type_expr(pty) {
                            Ok(ty) => params.push((pname.clone(), ty, false)),
                            Err(e) => self.errors.push(e),
                        }
                        param_type_exprs.push(pty.clone());
                    }
                    let ret_ty = self.resolve_type_expr(&f.ret).unwrap_or(Type::Unit);
                    let sig = FnSig {
                        params,
                        param_type_exprs,
                        return_type: ret_ty,
                        yields_effects: HashSet::new(),
                        is_extern: true,
                    };
                    self.extern_fns.insert(f.name.clone(), sig.clone());
                    self.functions.insert(f.name.clone(), sig);
                }
            }
        }

        if !self.errors.is_empty() {
            return Err(self.errors.clone());
        }

        // Pass 4: Check Function Bodies
        for item in &program.items {
            if let Item::Fn(f) = item {
                let yields_set: HashSet<String> = f.yields_effects.iter().cloned().collect();
                let type_params = &f.type_params;
                let mut fn_checker = FnChecker::new(self, yields_set, type_params.clone());
                fn_checker.push_scope();

                for p in &f.params {
                    let ty = self.resolve_type_expr_with_generics(&p.ty, type_params).unwrap_or(Type::Unit);
                    fn_checker.define_var(p.name.clone(), ty, p.is_mut);
                }
                let ret_ty = match &f.return_type {
                    Some(rt) => self.resolve_type_expr_with_generics(rt, type_params).unwrap_or(Type::Unit),
                    None => Type::Unit,
                };

                fn_checker.check_block(&f.body, &ret_ty);
                fn_checker.pop_scope();

                self.errors.extend(fn_checker.errors);
            }
        }

        if self.errors.is_empty() {
            Ok(())
        } else {
            Err(self.errors.clone())
        }
    }
}


impl<'a> FnChecker<'a> {
    fn new(parent: &'a TypeChecker, allowed_yields: HashSet<String>, current_type_params: Vec<String>) -> Self {
        Self {
            parent,
            current_type_params,
            scopes: Vec::new(),
            allowed_yields,
            handled_effects: Vec::new(),
            current_handler_arm: None,
            relational_ctx: crate::relational::RelationalContext::new(),
            errors: Vec::new(),
            region_stack: vec![RegionId(1)],
            next_region_id: 2,
            in_unsafe: false,
        }
    }

    fn current_region(&self) -> RegionId {
        *self.region_stack.last().unwrap_or(&RegionId(0))
    }

    fn lookup_var_region(&self, name: &str) -> Option<RegionId> {
        for scope in self.scopes.iter().rev() {
            if scope.variables.contains_key(name) {
                return Some(scope.region);
            }
        }
        None
    }

    fn push_scope(&mut self) {
        let reg = self.current_region();
        self.scopes.push(Scope {
            variables: HashMap::new(),
            region: reg,
        });
    }

    fn pop_scope(&mut self) {
        self.scopes.pop();
    }

    fn define_var(&mut self, name: String, mut ty: Type, is_mut: bool) {
        if let Type::Ref { ref mut region, .. } = ty {
            if region.is_none() {
                *region = Some(self.current_region());
            }
        }
        if let Some(scope) = self.scopes.last_mut() {
            scope.variables.insert(name, (ty, is_mut));
        }
    }

    fn lookup_var(&self, name: &str) -> Option<(Type, bool)> {
        for scope in self.scopes.iter().rev() {
            if let Some(val) = scope.variables.get(name) {
                return Some(val.clone());
            }
        }
        None
    }


    fn check_effect_permission(&mut self, effect: &str, span: Span) {
        // If the effect is handled by an enclosing `handle` block, it is permitted
        for (handled, _) in self.handled_effects.iter().rev() {
            if handled.contains(effect) {
                return;
            }
        }
        // Otherwise it must be in the function's `yields` clause
        let clean = effect.trim_start_matches("..");
        if !self.allowed_yields.contains(effect) && !self.allowed_yields.contains(clean) && !self.allowed_yields.contains(&format!("..{}", effect)) {
            self.errors.push(TypeError::new(
                format!(
                    "Unhandled algebraic effect '{}'. Function must declare 'yields [{}]' or handle it with 'handle {{ ... }} with {{ {}::... => ... }}'",
                    effect, effect, effect
                ),
                span,
            ));
        }
    }

    fn check_block(&mut self, block: &Block, expected_ret: &Type) -> (Type, Option<Interval>) {
        self.push_scope();
        for stmt in &block.stmts {
            self.check_stmt(stmt);
        }
        let trailing_res = if let Some(ref trailing) = block.trailing_expr {
            let (ty, intv) = self.check_expr(trailing);
            if !ty.is_compatible_with(expected_ret) && expected_ret != &Type::Unit {
                self.errors.push(TypeError::new(
                    format!("Mismatched return type: expected '{}', found '{}'", expected_ret, ty),
                    trailing.span,
                ));
            }
            // Region escape check: if returning from function scope (or expected return is specified)
            if let Some(r) = ty.region() {
                if expected_ret != &Type::Unit && r.0 > 0 {
                    self.errors.push(TypeError::new(
                        format!(
                            "Region escape violation: region-backed value with local region '{}' cannot escape function return",
                            r
                        ),
                        trailing.span,
                    ));
                }
            }
            (ty, intv)
        } else {
            (Type::Unit, None)
        };
        self.pop_scope();
        trailing_res
    }

    fn check_stmt(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::Let { name, is_mut, ty, init, span } => {
                let (init_ty, _) = self.check_expr(init);
                if let Some(declared_ty_expr) = ty {
                    match self.parent.resolve_type_expr_with_generics(declared_ty_expr, &self.current_type_params) {
                        Ok(declared_ty) => {
                            if !init_ty.is_compatible_with(&declared_ty) {
                                self.errors.push(TypeError::new(
                                    format!("Cannot assign type '{}' to variable '{}' of type '{}'", init_ty, name, declared_ty),
                                    *span,
                                ));
                            }
                            self.define_var(name.clone(), declared_ty, *is_mut);
                        }
                        Err(e) => self.errors.push(e),
                    }
                } else {
                    self.define_var(name.clone(), init_ty, *is_mut);
                }
            }
            Stmt::Assign { target, value, span } => {
                let (val_ty, val_interval) = self.check_expr(value);
                let (target_ty, is_mut) = self.check_assign_target(target);

                if !is_mut {
                    self.errors.push(TypeError::new(
                        "Cannot assign to immutable variable or field",
                        *span,
                    ));
                }

                // Region escape check for assignment: inner region value assigned to outer variable or field
                if let Some(val_reg) = val_ty.region() {
                    let target_reg = match &target.kind {
                        ExprKind::Ident(ref target_name) => self.lookup_var_region(target_name),
                        ExprKind::FieldAccess { target: base, .. } => {
                            if let ExprKind::Ident(ref base_name) = base.kind {
                                self.lookup_var_region(base_name)
                            } else {
                                Some(self.current_region())
                            }
                        }
                        _ => Some(self.current_region()),
                    };

                    if let Some(t_reg) = target_reg {
                        if val_reg.0 > t_reg.0 {
                            self.errors.push(TypeError::new(
                                format!(
                                    "Region escape violation: cannot assign region-backed value from inner region '{}' to outer target in region '{}'",
                                    val_reg, t_reg
                                ),
                                *span,
                            ));
                        }
                    }
                }

                // Refinement check for assignment
                if let Type::Refined { ref interval, ref name, .. } = target_ty {
                    if let Some(val_int) = val_interval {
                        if !val_int.is_subset_of(interval) {
                            let type_display = name.as_deref().unwrap_or("refined type");
                            self.errors.push(TypeError::new(
                                format!(
                                    "Refinement violation: value bounds [{}..={}] exceed allowable bounds [{}..={}] for '{}'",
                                    val_int.min, val_int.max, interval.min, interval.max, type_display
                                ),
                                *span,
                            ));
                        }
                    }
                }

                if !val_ty.is_compatible_with(&target_ty) {
                    self.errors.push(TypeError::new(
                        format!("Cannot assign '{}' to '{}'", val_ty, target_ty),
                        *span,
                    ));
                }
            }
            Stmt::Expr { expr, .. } => {
                self.check_expr(expr);
            }
            Stmt::Return { value, span } => {
                if let Some(v) = value {
                    let (ret_ty, _) = self.check_expr(v);
                    if let Some(r) = ret_ty.region() {
                        if r.0 > 0 {
                            self.errors.push(TypeError::new(
                                format!(
                                    "Region escape violation: region-backed value of type '{}' with local region '{}' cannot escape function return",
                                    ret_ty, r
                                ),
                                *span,
                            ));
                        }
                    }
                }
            }
        }
    }


    fn check_assign_target(&mut self, expr: &Expr) -> (Type, bool) {
        match &expr.kind {
            ExprKind::Ident(name) => {
                if let Some((ty, is_mut)) = self.lookup_var(name) {
                    let is_effective_mut = is_mut || matches!(ty, Type::Ref { is_mut: true, .. });
                    (ty, is_effective_mut)
                } else {
                    self.errors.push(TypeError::new(format!("Undefined variable '{}'", name), expr.span));
                    (Type::Unit, false)
                }
            }
            ExprKind::FieldAccess { target, field } => {
                let (target_ty, is_mut) = self.check_assign_target(target);
                let stripped = target_ty.strip_region();
                let inner_ty = match stripped {
                    Type::Ref { is_mut: m, inner, .. } => {
                        if !*m {
                            self.errors.push(TypeError::new("Cannot mutate field through immutable reference", expr.span));
                        }
                        inner.strip_region()
                    }
                    _ => stripped,
                };
                if let Type::Struct(sname) = inner_ty {
                    if let Some(fields) = self.parent.structs.get(sname) {
                        if let Some(fty) = fields.get(field) {
                            (fty.clone(), is_mut)
                        } else {
                            self.errors.push(TypeError::new(format!("Struct '{}' has no field '{}'", sname, field), expr.span));
                            (Type::Unit, false)
                        }
                    } else {
                        (Type::Unit, false)
                    }
                } else if let Type::Instantiated { name: sname, args } = inner_ty {
                    if let Some(st_decl) = self.parent.generic_structs.get(sname) {
                        if let Some(fdef) = st_decl.fields.iter().find(|f| f.name == *field) {
                            let mut subst = crate::unify::Subst::new();
                            for (tp, arg_ty) in st_decl.type_params.iter().zip(args.iter()) {
                                subst.bind(tp.clone(), arg_ty.clone());
                            }
                            let raw_ty = self.parent.resolve_type_expr_with_generics(&fdef.ty, &st_decl.type_params).unwrap_or(Type::Unit);
                            let resolved = crate::unify::substitute(&raw_ty, &subst);
                            (resolved, is_mut)
                        } else {
                            self.errors.push(TypeError::new(format!("Generic struct '{}' has no field '{}'", sname, field), expr.span));
                            (Type::Unit, false)
                        }
                    } else {
                        (Type::Unit, false)
                    }
                } else {
                    self.errors.push(TypeError::new(format!("Cannot access field '{}' on non-struct '{}'", field, target_ty), expr.span));
                    (Type::Unit, false)
                }
            }
            ExprKind::Deref(inner) => {
                if !self.in_unsafe {
                    self.errors.push(TypeError::new(
                        "Raw pointer or reference dereference requires an explicit 'unsafe { ... }' block",
                        expr.span,
                    ));
                }
                let (inner_ty, _) = self.check_expr(inner);
                match inner_ty {
                    Type::Ptr { is_mut, inner } => {
                        if !is_mut {
                            self.errors.push(TypeError::new("Cannot assign through immutable raw pointer (*const T)", expr.span));
                        }
                        (*inner, is_mut)
                    }
                    Type::Ref { is_mut, inner, .. } => {
                        if !is_mut {
                            self.errors.push(TypeError::new("Cannot assign through immutable reference (&T)", expr.span));
                        }
                        (*inner, is_mut)
                    }
                    other => {
                        self.errors.push(TypeError::new(format!("Cannot dereference non-pointer type '{}'", other), expr.span));
                        (Type::Unit, false)
                    }
                }
            }
            ExprKind::Index { target: base, index } => {
                let (base_ty, is_mut) = self.check_assign_target(base);
                let (idx_ty, _) = self.check_expr(index);
                if !idx_ty.is_compatible_with(&Type::Usize) && !idx_ty.is_compatible_with(&Type::I64) {
                    self.errors.push(TypeError::new(format!("Index must be integer, found '{}'", idx_ty), index.span));
                }
                match base_ty {
                    Type::Array { elem, .. } => (*elem, is_mut),
                    Type::Ref { is_mut: m, inner, .. } => {
                        if let Type::Array { elem, .. } = *inner {
                            (*elem, m)
                        } else {
                            self.errors.push(TypeError::new(format!("Cannot index non-array type '&{}'", inner), expr.span));
                            (Type::Unit, false)
                        }
                    }
                    Type::Ptr { is_mut: m, inner } => {
                        if !self.in_unsafe {
                            self.errors.push(TypeError::new("Indexing a raw pointer requires an explicit 'unsafe { ... }' block", expr.span));
                        }
                        if !m {
                            self.errors.push(TypeError::new("Cannot assign through immutable pointer (*const T)", expr.span));
                        }
                        (*inner, m)
                    }
                    other => {
                        self.errors.push(TypeError::new(format!("Cannot index non-array/non-pointer type '{}'", other), expr.span));
                        (Type::Unit, false)
                    }
                }
            }
            _ => {
                self.errors.push(TypeError::new("Invalid assignment target", expr.span));
                (Type::Unit, false)
            }
        }
    }

    fn check_expr(&mut self, expr: &Expr) -> (Type, Option<Interval>) {
        match &expr.kind {
            ExprKind::Int(val) => {
                (Type::I64, Some(Interval::point(*val)))
            }
            ExprKind::Str(_) => (Type::String, None),
            ExprKind::Bool(_) => (Type::Bool, None),
            ExprKind::Ident(name) => {
                if let Some((ty, _)) = self.lookup_var(name) {
                    let interval = ty.default_interval();
                    (ty, interval)
                } else if let Some(sig) = self.parent.functions.get(name) {
                    let p_types = sig.params.iter().map(|(_, ty, _)| ty.clone()).collect();
                    let fn_ty = Type::Fn {
                        params: p_types,
                        return_type: Box::new(sig.return_type.clone()),
                        yields_effects: sig.yields_effects.iter().cloned().collect(),
                    };
                    (fn_ty, None)
                } else if let Some(fdecl) = self.parent.generic_functions.get(name) {
                    let p_types = fdecl.params.iter().map(|p| self.parent.resolve_type_expr_with_generics(&p.ty, &fdecl.type_params).unwrap_or(Type::Unit)).collect();
                    let ret_ty = match &fdecl.return_type {
                        Some(rt) => self.parent.resolve_type_expr_with_generics(rt, &fdecl.type_params).unwrap_or(Type::Unit),
                        None => Type::Unit,
                    };
                    let fn_ty = Type::Fn {
                        params: p_types,
                        return_type: Box::new(ret_ty),
                        yields_effects: fdecl.yields_effects.clone(),
                    };
                    (fn_ty, None)
                } else {
                    self.errors.push(TypeError::new(format!("Undefined variable '{}'", name), expr.span));
                    (Type::Unit, None)
                }
            }
            ExprKind::Binary { op, left, right } => {
                let (lty, l_int) = self.check_expr(left);
                let (_rty, r_int) = self.check_expr(right);

                match op {
                    BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div => {
                        let computed_interval = match (op, l_int, r_int) {
                            (BinOp::Add, Some(li), Some(ri)) => Some(li.add(&ri)),
                            (BinOp::Sub, Some(li), Some(ri)) => Some(li.sub(&ri)),
                            _ => None,
                        };
                        (lty, computed_interval)
                    }
                    _ => (Type::Bool, None),
                }
            }
            ExprKind::Ref { is_mut, expr: inner } => {
                let (inner_ty, _) = self.check_expr(inner);
                let inferred_reg = if let Some(r) = inner_ty.region() {
                    r
                } else if let ExprKind::Ident(ref var_name) = inner.kind {
                    self.lookup_var_region(var_name).unwrap_or(self.current_region())
                } else {
                    self.current_region()
                };
                (
                    Type::Ref {
                        is_mut: *is_mut,
                        inner: Box::new(inner_ty),
                        region: Some(inferred_reg),
                    },
                    None,
                )
            }

            ExprKind::FieldAccess { target, field } => {
                let (target_ty, _) = self.check_expr(target);
                let stripped = target_ty.strip_region();
                let base_ty = match stripped {
                    Type::Ref { inner, .. } => inner.strip_region(),
                    other => other,
                };
                if let Type::Struct(sname) = base_ty {
                    if let Some(fields) = self.parent.structs.get(sname) {
                        if let Some(fty) = fields.get(field) {
                            return (fty.clone(), fty.default_interval());
                        } else {
                            self.errors.push(TypeError::new(format!("Field '{}' not found on struct '{}'", field, sname), expr.span));
                        }
                    }
                } else if let Type::Instantiated { name: sname, args } = base_ty {
                    if let Some(st_decl) = self.parent.generic_structs.get(sname) {
                        if let Some(fdef) = st_decl.fields.iter().find(|f| f.name == *field) {
                            let mut subst = crate::unify::Subst::new();
                            for (tp, arg_ty) in st_decl.type_params.iter().zip(args.iter()) {
                                subst.bind(tp.clone(), arg_ty.clone());
                            }
                            let raw_ty = self.parent.resolve_type_expr_with_generics(&fdef.ty, &st_decl.type_params).unwrap_or(Type::Unit);
                            let resolved = crate::unify::substitute(&raw_ty, &subst);
                            return (resolved.clone(), resolved.default_interval());
                        } else {
                            self.errors.push(TypeError::new(format!("Field '{}' not found on generic struct '{}'", field, sname), expr.span));
                        }
                    }
                }
                (Type::Unit, None)
            }
            ExprKind::MethodCall { target, method, args } => {
                let (target_ty, target_int) = self.check_expr(target);
                let arg_types: Vec<_> = args.iter().map(|a| self.check_expr(a)).collect();

                if method == "to_string" && args.is_empty() {
                    return (Type::String, None);
                }

                // Built-in refinement-safe methods like `saturating_add`
                if method == "saturating_add" && args.len() == 1 {
                    if let Type::Refined { ref base, ref interval, ref name } = target_ty {
                        let arg_int = arg_types[0].1.unwrap_or(Interval::new(0, 255));
                        let new_int = target_int
                            .unwrap_or(*interval)
                            .saturating_add(&arg_int, interval);
                        return (
                            Type::Refined {
                                base: base.clone(),
                                interval: *interval,
                                name: name.clone(),
                            },
                            Some(new_int),
                        );
                    } else {
                        return (target_ty, None);
                    }
                }

                if method == "spawn" {
                    return (Type::Struct("FiberHandle".into()), None);
                }

                (Type::Unit, None)
            }
            ExprKind::PathCall { path, args } => {
                for a in args {
                    self.check_expr(a);
                }
                // Check if path is an effect operation, e.g. Db::query
                if path.len() == 2 {
                    let namespace = &path[0];
                    let op = &path[1];
                    if let Some(en) = self.parent.enums.get(namespace) {
                        if let Some(v) = en.variants.iter().find(|v| &v.name == op) {
                            for (a, pty_expr) in args.iter().zip(v.payload.iter()) {
                                let (aty, _) = self.check_expr(a);
                                if let Ok(expected_ty) = self.parent.resolve_type_expr_with_generics(pty_expr, &en.type_params) {
                                    if !aty.is_compatible_with(&expected_ty) {
                                        self.errors.push(TypeError::new(
                                            format!("Mismatched argument type for enum variant '{}::{}': expected '{}', got '{}'", namespace, op, expected_ty, aty),
                                            a.span,
                                        ));
                                    }
                                }
                            }
                            if !en.type_params.is_empty() {
                                let args_tys: Vec<Type> = args.iter().map(|a| self.check_expr(a).0).collect();
                                return (Type::Instantiated { name: namespace.clone(), args: args_tys }, None);
                            } else {
                                return (Type::Enum(namespace.clone()), None);
                            }
                        }
                    }
                    if self.parent.known_effects.contains(namespace) {
                        self.check_effect_permission(namespace, expr.span);

                        let handler_region = self.handled_effects.iter().rev()
                            .find(|(h, _)| h.contains(namespace))
                            .map(|(_, reg)| *reg);

                        if let Some(h_reg) = handler_region {
                            for a in args {
                                let (arg_ty, _) = self.check_expr(a);
                                if let Type::Ref { region: Some(r), .. } = &arg_ty {
                                    if r.0 > h_reg.0 {
                                        self.errors.push(TypeError::new(
                                            format!(
                                                "Region escape violation: reference with region '{}' cannot escape into outer effect handler at region '{}'",
                                                r, h_reg
                                            ),
                                            a.span,
                                        ));
                                    }
                                }
                            }
                        } else {
                            for a in args {
                                let (arg_ty, _) = self.check_expr(a);
                                if let Type::Ref { region: Some(r), .. } = &arg_ty {
                                    if r.0 > 1 {
                                        self.errors.push(TypeError::new(
                                            format!(
                                                "Region escape violation: reference with local region '{}' cannot escape across function effect yield",
                                                r
                                            ),
                                            a.span,
                                        ));
                                    }
                                }
                            }
                        }

                        if let Some(eff_decl) = self.parent.effect_decls.get(namespace) {
                            if let Some(op_def) = eff_decl.operations.iter().find(|o| o.name == *op) {
                                let ret_ty = self.parent.resolve_type_expr(&op_def.return_type).unwrap_or(Type::Unit);
                                return (ret_ty, None);
                            }
                        }

                        if namespace == "Db" && op == "query" {
                            return (Type::Struct("Record".into()), None);
                        }
                        if namespace == "Random" {
                            return (Type::I64, Some(Interval::new(0, i64::MAX)));
                        }
                        if namespace == "Time" {
                            return (Type::U64, Some(Interval::new(0, i64::MAX)));
                        }
                        if namespace == "IO" {
                            return (Type::Unit, None);
                        }
                        if namespace == "Async" {
                            if op == "spawn" {
                                return (Type::Struct("FiberHandle".into()), None);
                            }
                            if op == "yield_now" {
                                return (Type::Unit, None);
                            }
                            if op == "await_fiber" {
                                return (Type::I64, None);
                            }
                            if op == "sleep" {
                                return (Type::Unit, None);
                            }
                        }
                        if namespace == "Channel" {
                            if op == "new" || op == "bounded" {
                                return (Type::I64, None);
                            }
                            if op == "send" || op == "close" {
                                return (Type::Unit, None);
                            }
                            if op == "recv" {
                                return (Type::I64, None);
                            }
                        }
                        if namespace == "Nursery" {
                            if op == "spawn" {
                                return (Type::Struct("FiberHandle".into()), None);
                            }
                            if op == "wait_all" {
                                return (Type::Unit, None);
                            }
                        }
                        if namespace == "Net" {
                            if op == "listen" || op == "accept" || op == "connect" || op == "write" {
                                return (Type::I64, None);
                            }
                            if op == "read" {
                                return (Type::String, None);
                            }
                            if op == "close" {
                                return (Type::Unit, None);
                            }
                        }
                        if (namespace == "Foreign" || namespace == "ForeignCall") && (op == "call" || op == "blocking") {
                            if let Some(first_arg) = args.first() {
                                if let ExprKind::Ident(ref fname) = first_arg.kind {
                                    if let Some(sig) = self.parent.functions.get(fname) {
                                        return (sig.return_type.clone(), None);
                                    }
                                }
                            }
                            return (Type::I64, None);
                        }
                    } else {
                        let full_path = path.join("::");
                        let func_sig = self.parent.functions.get(&full_path)
                            .or_else(|| self.parent.functions.get(op))
                            .or_else(|| {
                                self.parent.functions.iter().find_map(|(k, v)| {
                                    if k.ends_with(&format!("_{}", op)) {
                                        Some(v)
                                    } else {
                                        None
                                    }
                                })
                            });

                        if let Some(sig) = func_sig {
                            for eff in &sig.yields_effects {
                                self.check_effect_permission(eff, expr.span);
                            }
                            return (sig.return_type.clone(), None);
                        }

                        // External driver or library call (e.g. PostgresPool::execute)
                        return (Type::Unit, None);
                    }
                }
                (Type::Unit, None)
            }
            ExprKind::Call { callee, args } => {
                if let ExprKind::Ident(ref fname) = callee.kind {
                    // 1. Check if it's a generic function
                    if let Some(fdecl) = self.parent.generic_functions.get(fname).cloned() {
                        let mut subst = crate::unify::Subst::new();
                        let mut arg_types = Vec::new();
                        for a in args {
                            arg_types.push(self.check_expr(a));
                        }

                        // Container store region check: if passing &mut Container and an item, item cannot be from inner region
                        if args.len() >= 2 {
                            let first_arg_reg = arg_types[0].0.region().or_else(|| {
                                if let ExprKind::Ref { expr: inner, .. } = &args[0].kind {
                                    if let ExprKind::Ident(ref name) = inner.kind {
                                        self.lookup_var_region(name)
                                    } else {
                                        None
                                    }
                                } else {
                                    None
                                }
                            });
                            if let Some(target_reg) = first_arg_reg {
                                for (_, (arg_ty, _)) in args[1..].iter().zip(arg_types[1..].iter()) {
                                    if let Some(item_reg) = arg_ty.region() {
                                        if item_reg.0 > target_reg.0 {
                                            self.errors.push(TypeError::new(
                                                format!(
                                                    "Region escape violation: cannot store value from inner region '{}' into outer container in region '{}'",
                                                    item_reg, target_reg
                                                ),
                                                expr.span,
                                            ));
                                        }
                                    }
                                }
                            }
                        }

                        // Unify argument types with parameter types
                        for ((arg_expr, (arg_ty, _)), param) in args.iter().zip(arg_types.iter()).zip(fdecl.params.iter()) {
                            let expected_param_ty = self.parent.resolve_type_expr_with_generics(&param.ty, &fdecl.type_params).unwrap_or(Type::Unit);
                            if let Err(err) = crate::unify::unify(&expected_param_ty, arg_ty, &mut subst) {
                                self.errors.push(TypeError::new(
                                    format!("Type unification error for argument in call to '{}': {}", fname, err),
                                    arg_expr.span,
                                ));
                            }

                            // If param is relational, verify predicate
                            if let TypeExpr::Relational { ref predicate, .. } = param.ty {
                                if let Err(err) = self.relational_ctx.verify_predicate(arg_expr, predicate) {
                                    self.errors.push(TypeError::new(
                                        format!("Relational refinement check failed for parameter '{}': {}", param.name, err),
                                        arg_expr.span,
                                    ));
                                }
                            }
                        }

                        // Check effect permissions for generic function yields
                        for eff in &fdecl.yields_effects {
                            if let Some(bound_effs) = subst.effect_bindings.get(eff) {
                                for b_eff in bound_effs {
                                    self.check_effect_permission(b_eff, expr.span);
                                }
                            } else if !fdecl.effect_params.contains(eff) {
                                self.check_effect_permission(eff, expr.span);
                            }
                        }

                        let mut ret_ty = match &fdecl.return_type {
                            Some(rt) => {
                                let raw_ret = self.parent.resolve_type_expr_with_generics(rt, &fdecl.type_params).unwrap_or(Type::Unit);
                                crate::unify::substitute(&raw_ret, &subst)
                            }
                            None => Type::Unit,
                        };

                        // Propagate region bounds from arguments (e.g. vec_new_in(r))
                        let max_arg_region = arg_types.iter().filter_map(|(ty, _)| ty.region()).max();
                        if let Some(r) = max_arg_region {
                            if matches!(ret_ty.strip_region(), Type::Instantiated { .. } | Type::Struct(_) | Type::Ptr { .. } | Type::Ref { .. }) {
                                ret_ty = ret_ty.bounded_with(r);
                            }
                        }

                        return (ret_ty, None);
                    }

                    // 2. Check regular monomorphic function
                    if let Some(sig) = self.parent.functions.get(fname).cloned() {
                        if sig.is_extern && !self.in_unsafe {
                            self.errors.push(TypeError::new(
                                format!("Calling extern function '{}' is unsafe and must be enclosed in an unsafe {{ ... }} block", fname),
                                expr.span,
                            ));
                        }

                        // Check effects yielded by callee
                        for eff in &sig.yields_effects {
                            self.check_effect_permission(eff, expr.span);
                        }

                        // Create temporary local relational context for call arguments
                        let mut call_rel_ctx = self.relational_ctx.clone();
                        for (arg_expr, (param_name, _, _)) in args.iter().zip(sig.params.iter()) {
                            if let ExprKind::Int(v) = arg_expr.kind {
                                call_rel_ctx.add_interval(param_name.clone(), Interval::point(v));
                            }
                        }

                        let mut arg_tys = Vec::new();
                        for ((arg_expr, (param_name, param_ty, _)), p_ty_expr) in args.iter().zip(sig.params.iter()).zip(sig.param_type_exprs.iter()) {
                            let (arg_ty, _) = self.check_expr(arg_expr);
                            arg_tys.push(arg_ty.clone());
                            if !arg_ty.is_compatible_with(param_ty) {
                                self.errors.push(TypeError::new(
                                    format!("Type mismatch for parameter '{}': expected '{}', found '{}'", param_name, param_ty, arg_ty),
                                    arg_expr.span,
                                ));
                            }

                            // Relational predicate verification
                            if let TypeExpr::Relational { ref predicate, .. } = p_ty_expr {
                                if let Err(err) = call_rel_ctx.verify_predicate(arg_expr, predicate) {
                                    self.errors.push(TypeError::new(
                                        format!("Relational refinement check failed for parameter '{}': {}", param_name, err),
                                        arg_expr.span,
                                    ));
                                }
                            }
                        }

                        let mut ret_ty = sig.return_type;
                        let max_arg_region = arg_tys.iter().filter_map(|ty| ty.region()).max();
                        if let Some(r) = max_arg_region {
                            if matches!(ret_ty.strip_region(), Type::Instantiated { .. } | Type::Struct(_) | Type::Ptr { .. } | Type::Ref { .. }) {
                                ret_ty = ret_ty.bounded_with(r);
                            }
                        }

                        return (ret_ty, None);
                    }
                }

                // 3. Higher-order function / function variable call
                let (callee_ty, _) = self.check_expr(callee);
                if let Type::Fn { params, return_type, yields_effects } = callee_ty {
                    for eff in &yields_effects {
                        self.check_effect_permission(eff, expr.span);
                    }
                    for (arg_expr, param_ty) in args.iter().zip(params.iter()) {
                        let (arg_ty, _) = self.check_expr(arg_expr);
                        if !arg_ty.is_compatible_with(param_ty) {
                            self.errors.push(TypeError::new(
                                format!("Type mismatch in function call: expected '{}', found '{}'", param_ty, arg_ty),
                                arg_expr.span,
                            ));
                        }
                    }
                    return (*return_type, None);
                }

                for a in args {
                    self.check_expr(a);
                }
                (Type::Unit, None)
            }
            ExprKind::MacroCall { name: _, args } => {
                for a in args {
                    self.check_expr(a);
                }
                (Type::Unit, None)
            }
            ExprKind::StructInit { name, fields } => {
                let mut max_field_reg = None;
                if let Some(expected_fields) = self.parent.structs.get(name).cloned() {
                    for (fname, fval) in fields {
                        let (val_ty, val_int) = self.check_expr(fval);
                        if let Some(r) = val_ty.region() {
                            max_field_reg = Some(max_field_reg.map_or(r, |existing: RegionId| RegionId(existing.0.max(r.0))));
                        }
                        if let Some(expected_ty) = expected_fields.get(fname) {
                            // Refinement verification
                            if let Type::Refined { interval, .. } = expected_ty {
                                if let Some(vi) = val_int {
                                    if !vi.is_subset_of(interval) {
                                        self.errors.push(TypeError::new(
                                            format!(
                                                "Refinement error: value bounds [{}..={}] exceed refined field '{}' bounds [{}..={}]",
                                                vi.min, vi.max, fname, interval.min, interval.max
                                            ),
                                            fval.span,
                                        ));
                                    }
                                }
                            }
                            if !val_ty.is_compatible_with(expected_ty) {
                                self.errors.push(TypeError::new(
                                    format!("Field '{}' expected type '{}', found '{}'", fname, expected_ty, val_ty),
                                    fval.span,
                                ));
                            }
                        } else {
                            self.errors.push(TypeError::new(format!("Unknown field '{}' for struct '{}'", fname, name), fval.span));
                        }
                    }
                    let mut res_ty = Type::Struct(name.clone());
                    if let Some(r) = max_field_reg {
                        res_ty = res_ty.bounded_with(r);
                    }
                    (res_ty, None)
                } else if let Some(st_decl) = self.parent.generic_structs.get(name).cloned() {
                    let mut subst = crate::unify::Subst::new();
                    for (fname, fval) in fields {
                        let (val_ty, _) = self.check_expr(fval);
                        if let Some(r) = val_ty.region() {
                            max_field_reg = Some(max_field_reg.map_or(r, |existing: RegionId| RegionId(existing.0.max(r.0))));
                        }
                        if let Some(fdef) = st_decl.fields.iter().find(|f| f.name == *fname) {
                            let expected_field_ty = self.parent.resolve_type_expr_with_generics(&fdef.ty, &st_decl.type_params).unwrap_or(Type::Unit);
                            if let Err(err) = crate::unify::unify(&expected_field_ty, &val_ty, &mut subst) {
                                self.errors.push(TypeError::new(
                                    format!("Type mismatch for generic struct field '{}': {}", fname, err),
                                    fval.span,
                                ));
                            }
                        } else {
                            self.errors.push(TypeError::new(format!("Unknown field '{}' for struct '{}'", fname, name), fval.span));
                        }
                    }
                    let args: Vec<Type> = st_decl.type_params.iter().map(|tp| subst.get(tp).cloned().unwrap_or(Type::Unit)).collect();
                    let mut res_ty = Type::Instantiated { name: name.clone(), args };
                    if let Some(r) = max_field_reg {
                        res_ty = res_ty.bounded_with(r);
                    }
                    (res_ty, None)
                } else {
                    self.errors.push(TypeError::new(format!("Unknown struct '{}'", name), expr.span));
                    (Type::Unit, None)
                }
            }
            ExprKind::Cast { expr: inner, target_ty } => {
                let (_, inner_int) = self.check_expr(inner);
                let resolved = match self.parent.resolve_type_expr_with_generics(target_ty, &self.current_type_params) {
                    Ok(ty) => ty,
                    Err(e) => {
                        self.errors.push(e);
                        Type::Unit
                    }
                };

                // Compile-time refinement check during cast: `x as Health`
                if let Type::Refined { ref interval, .. } = resolved {
                    if let Some(int) = inner_int {
                        if int.min == int.max && !interval.contains(int.min) {
                            self.errors.push(TypeError::new(
                                format!("Cast to refinement type failed: constant value {} is outside allowable range [{}..={}]", int.min, interval.min, interval.max),
                                expr.span,
                            ));
                        }
                    }
                }

                let out_int = resolved.default_interval();
                (resolved, out_int)
            }
            ExprKind::Try(inner) => {
                // Try checks IOError effect
                self.check_effect_permission("IOError", expr.span);
                self.check_expr(inner)
            }
            ExprKind::EffectCall(inner) => {
                // `fetch_player(42)!` invokes effectful function
                self.check_expr(inner)
            }
            ExprKind::Await(inner) => {
                self.check_expr(inner)
            }
            ExprKind::Handle { body, handlers } => {
                let mut handled_set = HashSet::new();
                for h in handlers {
                    handled_set.insert(h.effect_name.clone());
                }

                let cur_reg = self.current_region();
                self.handled_effects.push((handled_set, cur_reg));
                let (body_ty, body_intv) = self.check_block(body, &Type::Unit);
                self.handled_effects.pop();

                // Check handler arms
                for h in handlers {
                    for arm in &h.arms {
                        let mut expected_resume_ty = Type::Unit;
                        let mut param_tys: Vec<(String, Type)> = Vec::new();

                        if let Some(eff_decl) = self.parent.effect_decls.get(&h.effect_name) {
                            if let Some(op_def) = eff_decl.operations.iter().find(|o| o.name == arm.op_name) {
                                expected_resume_ty = self.parent.resolve_type_expr(&op_def.return_type).unwrap_or(Type::Unit);
                                for (p_idx, (p_name, p_ty_expr)) in op_def.params.iter().enumerate() {
                                    let bound_name = arm.params.get(p_idx).unwrap_or(p_name);
                                    let resolved_ty = self.parent.resolve_type_expr(p_ty_expr).unwrap_or(Type::String);
                                    param_tys.push((bound_name.clone(), resolved_ty));
                                }
                            }
                        }

                        if param_tys.is_empty() {
                            for p in &arm.params {
                                param_tys.push((p.clone(), Type::String));
                            }
                            if h.effect_name == "Db" && arm.op_name == "query" {
                                expected_resume_ty = Type::Struct("Record".into());
                            } else if h.effect_name == "Random" {
                                expected_resume_ty = Type::I64;
                            } else if h.effect_name == "Time" {
                                expected_resume_ty = Type::U64;
                            } else if h.effect_name == "Net" && arm.op_name == "read" {
                                expected_resume_ty = Type::String;
                            }
                        }

                        // Linearity check on arm body:
                        // Continuations are single-shot: verify resume is called at most once on any path
                        let mut resume_errors = Vec::new();
                        check_resume_linearity(&arm.body, false, &mut resume_errors);
                        self.errors.extend(resume_errors);

                        self.current_handler_arm = Some(HandlerArmCtx {
                            effect_name: h.effect_name.clone(),
                            op_name: arm.op_name.clone(),
                            expected_resume_ty,
                            resume_count: 0,
                        });

                        self.push_scope();
                        for (pname, pty) in param_tys {
                            self.define_var(pname, pty, false);
                        }
                        self.check_expr(&arm.body);
                        self.pop_scope();

                        self.current_handler_arm = None;
                    }
                }

                (body_ty, body_intv)
            }
            ExprKind::Resume(inner) => {
                let (expected_ty, resume_count) = if let Some(ref mut ctx) = self.current_handler_arm {
                    ctx.resume_count += 1;
                    (ctx.expected_resume_ty.clone(), ctx.resume_count)
                } else {
                    self.errors.push(TypeError::new(
                        "Cannot call 'resume' outside of an effect handler arm",
                        expr.span,
                    ));
                    (Type::Unit, 0)
                };

                if resume_count > 1 {
                    self.errors.push(TypeError::new(
                        "Linearity violation: 'resume' cannot be called multiple times in a single handler arm (continuations are single-shot)",
                        expr.span,
                    ));
                }

                let (val_ty, val_intv) = self.check_expr(inner);
                if !val_ty.is_compatible_with(&expected_ty) && expected_ty != Type::Unit {
                    self.errors.push(TypeError::new(
                        format!("Type mismatch in resume: expected '{}', got '{}'", expected_ty, val_ty),
                        inner.span,
                    ));
                }

                // Region safety: reference cannot escape through resume
                if let Type::Ref { region: Some(r), .. } = &val_ty {
                    if r.0 > 1 {
                        self.errors.push(TypeError::new(
                            format!("Region escape violation: reference with region '{}' cannot escape through resume", r),
                            inner.span,
                        ));
                    }
                }

                (expected_ty, val_intv)
            }
            ExprKind::Block(b) => {
                self.check_block(b, &Type::Unit);
                (Type::Unit, None)
            }
            ExprKind::If { cond, then_branch, else_branch } => {
                self.check_expr(cond);
                self.check_block(then_branch, &Type::Unit);
                if let Some(eb) = else_branch {
                    self.check_block(eb, &Type::Unit);
                }
                (Type::Unit, None)
            }
            ExprKind::Loop(b) => {
                self.check_block(b, &Type::Unit);
                (Type::Unit, None)
            }
            ExprKind::Region { name, body } => {
                let reg_id = RegionId(self.next_region_id);
                self.next_region_id += 1;
                self.region_stack.push(reg_id);

                self.push_scope();
                if let Some(r_name) = name {
                    let r_ty = Type::Ptr { is_mut: true, inner: Box::new(Type::U8) }.bounded_with(reg_id);
                    self.define_var(r_name.clone(), r_ty, false);
                }
                let (body_ty, body_intv) = self.check_block(body, &Type::Unit);
                self.pop_scope();

                self.region_stack.pop();

                // Linear escape analysis: ensure no reference, raw pointer, or region-bounded collection escapes
                if let Some(r) = body_ty.region() {
                    if r.0 >= reg_id.0 {
                        self.errors.push(TypeError::new(
                            format!(
                                "Region escape violation: region-backed value of type '{}' with region '{}' cannot escape enclosing region block",
                                body_ty, r
                            ),
                            body.span,
                        ));
                    }
                }
                if matches!(body_ty.strip_region(), Type::Ptr { .. }) {
                    self.errors.push(TypeError::new(
                        "Pointer escape violation: raw pointer cannot escape enclosing region block".to_string(),
                        body.span,
                    ));
                }

                (body_ty, body_intv)
            }
            ExprKind::Nursery { name, body } => {
                self.push_scope();
                if let Some(n) = name {
                    self.define_var(n.clone(), Type::Struct("Nursery".into()), false);
                }
                for stmt in &body.stmts {
                    self.check_stmt(stmt);
                }
                let trailing_res = if let Some(ref trailing) = body.trailing_expr {
                    self.check_expr(trailing)
                } else {
                    (Type::Unit, None)
                };
                self.pop_scope();
                if matches!(&trailing_res.0, Type::Ptr { .. }) {
                    self.errors.push(TypeError::new(
                        "Pointer escape violation: raw pointer cannot escape enclosing nursery block".to_string(),
                        body.span,
                    ));
                }
                trailing_res
            }
            ExprKind::Path(path) => {
                if path.len() == 2 {
                    if let Some(en) = self.parent.enums.get(&path[0]) {
                        if let Some(_) = en.variants.iter().find(|v| &v.name == &path[1]) {
                            return (Type::Enum(path[0].clone()), None);
                        }
                    }
                }
                (Type::Unit, None)
            }
            ExprKind::Array(elements) => {
                if elements.is_empty() {
                    (Type::Array { elem: Box::new(Type::Unit), len: 0 }, None)
                } else {
                    let (first_ty, _) = self.check_expr(&elements[0]);
                    for (idx, elem) in elements.iter().enumerate().skip(1) {
                        let (elem_ty, _) = self.check_expr(elem);
                        if !elem_ty.is_compatible_with(&first_ty) {
                            self.errors.push(TypeError::new(
                                format!("Mismatched element type in array literal at index {}: expected '{}', found '{}'", idx, first_ty, elem_ty),
                                elem.span,
                            ));
                        }
                    }
                    (Type::Array { elem: Box::new(first_ty), len: elements.len() }, None)
                }
            }
            ExprKind::Index { target, index } => {
                let (target_ty, _) = self.check_expr(target);
                let (idx_ty, idx_intv) = self.check_expr(index);
                if !matches!(idx_ty, Type::I64 | Type::Usize | Type::U32 | Type::U16 | Type::U8) {
                    self.errors.push(TypeError::new(
                        format!("Array index must be integer, found '{}'", idx_ty),
                        index.span,
                    ));
                }
                let inner_target = match &target_ty {
                    Type::Ref { inner, .. } => inner.as_ref(),
                    _ => &target_ty,
                };
                if let Type::Array { elem, len } = inner_target {
                    if let Some(intv) = idx_intv {
                        if intv.min == intv.max && (intv.min < 0 || intv.min >= *len as i64) {
                            self.errors.push(TypeError::new(
                                format!("Array index out of bounds: index {} is out of bounds for array of length {}", intv.min, len),
                                index.span,
                            ));
                        }
                    }
                    (*elem.clone(), None)
                } else if let Type::Ptr { inner, .. } = inner_target {
                    if !self.in_unsafe {
                        self.errors.push(TypeError::new(
                            "Indexing a raw pointer is unsafe and requires an 'unsafe { ... }' block",
                            expr.span,
                        ));
                    }
                    (*inner.clone(), None)
                } else {
                    self.errors.push(TypeError::new(
                        format!("Cannot index into non-array type '{}'", target_ty),
                        target.span,
                    ));
                    (Type::Unit, None)
                }
            }
            ExprKind::Match { expr: scrutinee, arms } => {
                let (scrut_ty, _) = self.check_expr(scrutinee);
                let enum_decl = match &scrut_ty {
                    Type::Enum(name) => self.parent.enums.get(name).cloned(),
                    Type::Instantiated { name, .. } => self.parent.enums.get(name).cloned(),
                    _ => None,
                };

                let mut matched_variants = HashSet::new();
                let mut has_catch_all = false;
                let mut arm_return_types = Vec::new();

                for arm in arms {
                    self.push_scope();
                    self.check_pattern(&arm.pattern, &scrut_ty, enum_decl.as_ref(), &mut matched_variants, &mut has_catch_all, true);
                    let (arm_ty, arm_intv) = self.check_expr(&arm.body);
                    self.pop_scope();
                    arm_return_types.push((arm_ty, arm_intv, arm.span));
                }

                // Exhaustiveness check
                if let Some(en) = &enum_decl {
                    if !has_catch_all {
                        for v in &en.variants {
                            if !matched_variants.contains(&v.name) {
                                self.errors.push(TypeError::new(
                                    format!("Non-exhaustive match on enum '{}': variant '{}' is not covered", en.name, v.name),
                                    expr.span,
                                ));
                            }
                        }
                    }
                }

                if let Some((first_ty, first_intv, _)) = arm_return_types.first() {
                    for (aty, _, aspan) in arm_return_types.iter().skip(1) {
                        if !aty.is_compatible_with(first_ty) {
                            self.errors.push(TypeError::new(
                                format!("Match arm type mismatch: expected '{}', found '{}'", first_ty, aty),
                                *aspan,
                            ));
                        }
                    }
                    (first_ty.clone(), *first_intv)
                } else {
                    (Type::Unit, None)
                }
            }
            ExprKind::Unsafe { body } => {
                let prev_unsafe = self.in_unsafe;
                self.in_unsafe = true;
                let res = self.check_block(body, &Type::Unit);
                self.in_unsafe = prev_unsafe;
                res
            }
            ExprKind::Deref(inner) => {
                if !self.in_unsafe {
                    self.errors.push(TypeError::new(
                        "Dereferencing a raw pointer is unsafe and must be enclosed in an unsafe { ... } block",
                        expr.span,
                    ));
                }
                let (inner_ty, _) = self.check_expr(inner);
                match inner_ty {
                    Type::Ptr { inner, .. } => (*inner, None),
                    Type::Ref { inner, .. } => (*inner, None),
                    other => {
                        self.errors.push(TypeError::new(
                            format!("Cannot dereference non-pointer type '{}'", other),
                            expr.span,
                        ));
                        (Type::Unit, None)
                    }
                }
            }
            ExprKind::AddrOf { mutable, expr: inner } => {
                let (inner_ty, _) = self.check_expr(inner);
                (Type::Ptr { is_mut: *mutable, inner: Box::new(inner_ty) }, None)
            }
        }
    }

    fn check_pattern(
        &mut self,
        pat: &Pattern,
        expected_ty: &Type,
        enum_decl: Option<&EnumDecl>,
        matched_variants: &mut HashSet<String>,
        has_catch_all: &mut bool,
        is_top_level: bool,
    ) {
        match pat {
            Pattern::Wildcard(_) => {
                if is_top_level {
                    *has_catch_all = true;
                }
            }
            Pattern::Variable(name, _) => {
                if is_top_level {
                    *has_catch_all = true;
                }
                self.define_var(name.clone(), expected_ty.clone(), false);
            }
            Pattern::Literal(lit) => {
                let (lit_ty, _) = self.check_expr(lit);
                if !lit_ty.is_compatible_with(expected_ty) {
                    self.errors.push(TypeError::new(
                        format!("Mismatched literal in pattern: expected '{}', got '{}'", expected_ty, lit_ty),
                        lit.span,
                    ));
                }
            }
            Pattern::Variant { enum_name: _, variant_name, subpatterns, span } => {
                if let Some(en) = enum_decl {
                    if let Some(v) = en.variants.iter().find(|v| &v.name == variant_name) {
                        if is_top_level {
                            matched_variants.insert(variant_name.clone());
                        }
                        if subpatterns.len() != v.payload.len() {
                            self.errors.push(TypeError::new(
                                format!("Variant '{}' expects {} payload values, but pattern has {}", variant_name, v.payload.len(), subpatterns.len()),
                                *span,
                            ));
                        }
                        for (sp, pty_expr) in subpatterns.iter().zip(v.payload.iter()) {
                            let pty = self.parent.resolve_type_expr_with_generics(pty_expr, &en.type_params).unwrap_or(Type::Unit);
                            self.check_pattern(sp, &pty, None, matched_variants, has_catch_all, false);
                        }
                    } else {
                        self.errors.push(TypeError::new(
                            format!("Enum '{}' has no variant '{}'", en.name, variant_name),
                            *span,
                        ));
                    }
                } else {
                    if is_top_level {
                        matched_variants.insert(variant_name.clone());
                    }
                }
            }
        }
    }
}

fn check_resume_linearity(expr: &Expr, in_loop: bool, errors: &mut Vec<TypeError>) -> usize {
    match &expr.kind {
        ExprKind::Resume(inner) => {
            check_resume_linearity(inner, in_loop, errors);
            if in_loop {
                errors.push(TypeError::new(
                    "Linearity violation: continuations are single-shot and cannot be resumed inside loops",
                    expr.span,
                ));
            }
            1
        }
        ExprKind::Block(b) => {
            let mut seq_resumes = 0;
            for stmt in &b.stmts {
                match stmt {
                    Stmt::Let { init, .. } => {
                        seq_resumes += check_resume_linearity(init, in_loop, errors);
                    }
                    Stmt::Assign { value, .. } => {
                        seq_resumes += check_resume_linearity(value, in_loop, errors);
                    }
                    Stmt::Expr { expr, .. } => {
                        seq_resumes += check_resume_linearity(expr, in_loop, errors);
                    }
                    Stmt::Return { value: Some(v), .. } => {
                        seq_resumes += check_resume_linearity(v, in_loop, errors);
                    }
                    _ => {}
                }
            }
            if let Some(t) = &b.trailing_expr {
                seq_resumes += check_resume_linearity(t, in_loop, errors);
            }
            if seq_resumes > 1 {
                errors.push(TypeError::new(
                    "Linearity violation: multiple 'resume' calls along the same execution path (continuations are single-shot)",
                    b.span,
                ));
            }
            seq_resumes
        }
        ExprKind::If { cond, then_branch, else_branch } => {
            let cond_res = check_resume_linearity(cond, in_loop, errors);
            let then_block_expr = Expr::new(ExprKind::Block(then_branch.clone()), then_branch.span);
            let then_res = check_resume_linearity(&then_block_expr, in_loop, errors);
            let else_res = if let Some(eb) = else_branch {
                let else_block_expr = Expr::new(ExprKind::Block(eb.clone()), eb.span);
                check_resume_linearity(&else_block_expr, in_loop, errors)
            } else {
                0
            };
            cond_res + std::cmp::max(then_res, else_res)
        }
        ExprKind::Match { expr, arms } => {
            let expr_res = check_resume_linearity(expr, in_loop, errors);
            let mut max_arm = 0;
            for arm in arms {
                let arm_res = check_resume_linearity(&arm.body, in_loop, errors);
                max_arm = std::cmp::max(max_arm, arm_res);
            }
            expr_res + max_arm
        }
        ExprKind::Loop(b) => {
            let loop_block_expr = Expr::new(ExprKind::Block(b.clone()), b.span);
            let count = check_resume_linearity(&loop_block_expr, true, errors);
            if count > 0 {
                errors.push(TypeError::new(
                    "Linearity violation: 'resume' cannot be invoked inside a loop (continuations are single-shot)",
                    b.span,
                ));
            }
            0
        }
        ExprKind::Binary { left, right, .. } => {
            check_resume_linearity(left, in_loop, errors) + check_resume_linearity(right, in_loop, errors)
        }
        ExprKind::Call { callee, args } => {
            let mut total = check_resume_linearity(callee, in_loop, errors);
            for a in args {
                total += check_resume_linearity(a, in_loop, errors);
            }
            total
        }
        ExprKind::MethodCall { target, args, .. } => {
            let mut total = check_resume_linearity(target, in_loop, errors);
            for a in args {
                total += check_resume_linearity(a, in_loop, errors);
            }
            total
        }
        _ => 0,
    }
}

