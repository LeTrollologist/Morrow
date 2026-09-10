use std::collections::{HashMap, HashSet};
use tungsten_syntax::ast::*;
use tungsten_syntax::token::Span;

use crate::interval::Interval;
use crate::types::Type;

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
    pub functions: HashMap<String, FnSig>,
    pub known_effects: HashSet<String>,
    pub errors: Vec<TypeError>,
}

#[derive(Debug, Clone)]
pub struct FnSig {
    pub params: Vec<(String, Type, bool)>, // (name, ty, is_mut)
    pub return_type: Type,
    pub yields_effects: HashSet<String>,
}

#[derive(Clone)]
struct Scope {
    variables: HashMap<String, (Type, bool)>, // (type, is_mut)
}

pub struct FnChecker<'a> {
    parent: &'a TypeChecker,
    scopes: Vec<Scope>,
    allowed_yields: HashSet<String>,
    handled_effects: Vec<HashSet<String>>,
    errors: Vec<TypeError>,
}

impl TypeChecker {
    pub fn new() -> Self {
        let mut tc = Self {
            types: HashMap::new(),
            structs: HashMap::new(),
            functions: HashMap::new(),
            known_effects: HashSet::new(),
            errors: Vec::new(),
        };

        // Primitive types
        tc.types.insert("u8".into(), Type::U8);
        tc.types.insert("u16".into(), Type::U16);
        tc.types.insert("u32".into(), Type::U32);
        tc.types.insert("u64".into(), Type::U64);
        tc.types.insert("i64".into(), Type::I64);
        tc.types.insert("bool".into(), Type::Bool);
        tc.types.insert("String".into(), Type::String);

        // Standard algebraic effects
        tc.known_effects.insert("Db".into());
        tc.known_effects.insert("IOError".into());
        tc.known_effects.insert("FileSystem".into());
        tc.known_effects.insert("Network".into());
        tc.known_effects.insert("IO".into());
        tc.known_effects.insert("Random".into());
        tc.known_effects.insert("State".into());
        tc.known_effects.insert("Time".into());
        tc.known_effects.insert("Yield".into());

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
        match ty_expr {
            TypeExpr::Named(name, span) => {
                if let Some(ty) = self.types.get(name) {
                    Ok(ty.clone())
                } else if self.structs.contains_key(name) {
                    Ok(Type::Struct(name.clone()))
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
            TypeExpr::Ref { is_mut, inner, .. } => {
                let inner_ty = self.resolve_type_expr(inner)?;
                Ok(Type::Ref {
                    is_mut: *is_mut,
                    inner: Box::new(inner_ty),
                })
            }
            TypeExpr::Unit(_) => Ok(Type::Unit),
        }
    }

    pub fn check_program(&mut self, program: &Program) -> Result<(), Vec<TypeError>> {
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

        // Pass 3: Register Functions
        for item in &program.items {
            if let Item::Fn(f) = item {
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
                for eff in &f.yields_effects {
                    self.known_effects.insert(eff.clone());
                }
                self.functions.insert(
                    f.name.clone(),
                    FnSig {
                        params,
                        return_type: ret_ty,
                        yields_effects: yields_set,
                    },
                );
            }
        }

        if !self.errors.is_empty() {
            return Err(self.errors.clone());
        }

        // Pass 4: Check Function Bodies
        for item in &program.items {
            if let Item::Fn(f) = item {
                let sig = self.functions.get(&f.name).unwrap().clone();
                let mut fn_checker = FnChecker::new(self, sig.yields_effects.clone());
                fn_checker.push_scope();
                for (p_name, p_ty, p_mut) in &sig.params {
                    fn_checker.define_var(p_name.clone(), p_ty.clone(), *p_mut);
                }
                fn_checker.check_block(&f.body, &sig.return_type);
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
    fn new(parent: &'a TypeChecker, allowed_yields: HashSet<String>) -> Self {
        Self {
            parent,
            scopes: Vec::new(),
            allowed_yields,
            handled_effects: Vec::new(),
            errors: Vec::new(),
        }
    }

    fn push_scope(&mut self) {
        self.scopes.push(Scope {
            variables: HashMap::new(),
        });
    }

    fn pop_scope(&mut self) {
        self.scopes.pop();
    }

    fn define_var(&mut self, name: String, ty: Type, is_mut: bool) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.variables.insert(name, (ty, is_mut));
        }
    }

    fn lookup_var(&self, name: &str) -> Option<(Type, bool)> {
        for scope in self.scopes.iter().rev() {
            if let Some(entry) = scope.variables.get(name) {
                return Some(entry.clone());
            }
        }
        None
    }

    fn check_effect_permission(&mut self, effect: &str, span: Span) {
        // If the effect is handled by an enclosing `handle` block, it is permitted
        for handled in self.handled_effects.iter().rev() {
            if handled.contains(effect) {
                return;
            }
        }
        // Otherwise it must be in the function's `yields` clause
        if !self.allowed_yields.contains(effect) {
            self.errors.push(TypeError::new(
                format!(
                    "Unhandled algebraic effect '{}'. Function must declare 'yields [{}]' or handle it with 'handle {{ ... }} with {} {{ ... }}'",
                    effect, effect, effect
                ),
                span,
            ));
        }
    }

    fn check_block(&mut self, block: &Block, expected_ret: &Type) {
        self.push_scope();
        for stmt in &block.stmts {
            self.check_stmt(stmt);
        }
        if let Some(ref trailing) = block.trailing_expr {
            let (ty, _) = self.check_expr(trailing);
            if !ty.is_compatible_with(expected_ret) && expected_ret != &Type::Unit {
                self.errors.push(TypeError::new(
                    format!("Mismatched return type: expected '{}', found '{}'", expected_ret, ty),
                    trailing.span,
                ));
            }
        }
        self.pop_scope();
    }

    fn check_stmt(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::Let { name, is_mut, ty, init, span } => {
                let (init_ty, _) = self.check_expr(init);
                if let Some(declared_ty_expr) = ty {
                    match self.parent.resolve_type_expr(declared_ty_expr) {
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
            Stmt::Return { value, span: _ } => {
                if let Some(v) = value {
                    self.check_expr(v);
                } else {
                    // unit return
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
                let inner_ty = match &target_ty {
                    Type::Ref { is_mut: m, inner } => {
                        if !*m {
                            self.errors.push(TypeError::new("Cannot mutate field through immutable reference", expr.span));
                        }
                        inner.as_ref()
                    }
                    _ => &target_ty,
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
                } else {
                    self.errors.push(TypeError::new(format!("Cannot access field '{}' on non-struct '{}'", field, target_ty), expr.span));
                    (Type::Unit, false)
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
                (Type::Ref { is_mut: *is_mut, inner: Box::new(inner_ty) }, None)
            }
            ExprKind::FieldAccess { target, field } => {
                let (target_ty, _) = self.check_expr(target);
                let base_ty = match &target_ty {
                    Type::Ref { inner, .. } => inner.as_ref(),
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
                }
                (Type::Unit, None)
            }
            ExprKind::MethodCall { target, method, args } => {
                let (target_ty, target_int) = self.check_expr(target);
                let arg_types: Vec<_> = args.iter().map(|a| self.check_expr(a)).collect();

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
                    if self.parent.known_effects.contains(namespace) {
                        self.check_effect_permission(namespace, expr.span);
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
                    } else {
                        // External driver or library call (e.g. PostgresPool::execute)
                        return (Type::Unit, None);
                    }
                }
                (Type::Unit, None)
            }
            ExprKind::Call { callee, args } => {
                if let ExprKind::Ident(ref fname) = callee.kind {
                    if let Some(sig) = self.parent.functions.get(fname).cloned() {
                        // Check effects yielded by callee
                        for eff in &sig.yields_effects {
                            self.check_effect_permission(eff, expr.span);
                        }

                        for (arg_expr, (param_name, param_ty, _)) in args.iter().zip(sig.params.iter()) {
                            let (arg_ty, _) = self.check_expr(arg_expr);
                            if !arg_ty.is_compatible_with(param_ty) {
                                self.errors.push(TypeError::new(
                                    format!("Type mismatch for parameter '{}': expected '{}', found '{}'", param_name, param_ty, arg_ty),
                                    arg_expr.span,
                                ));
                            }
                        }
                        return (sig.return_type, None);
                    }
                }
                self.check_expr(callee);
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
                if let Some(expected_fields) = self.parent.structs.get(name).cloned() {
                    for (fname, fval) in fields {
                        let (val_ty, val_int) = self.check_expr(fval);
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
                    (Type::Struct(name.clone()), None)
                } else {
                    self.errors.push(TypeError::new(format!("Unknown struct '{}'", name), expr.span));
                    (Type::Unit, None)
                }
            }
            ExprKind::Cast { expr: inner, target_ty } => {
                let (_, inner_int) = self.check_expr(inner);
                let resolved = match self.parent.resolve_type_expr(target_ty) {
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

                self.handled_effects.push(handled_set);
                self.check_block(body, &Type::Unit);
                self.handled_effects.pop();

                // Check handler arms
                for h in handlers {
                    for arm in &h.arms {
                        self.push_scope();
                        for p in &arm.params {
                            self.define_var(p.clone(), Type::String, false);
                        }
                        self.check_expr(&arm.body);
                        self.pop_scope();
                    }
                }

                (Type::Unit, None)
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
        }
    }
}
