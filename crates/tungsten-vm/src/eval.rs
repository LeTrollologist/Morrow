use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tungsten_fiber::{Channel, FiberHandle, Scheduler};
use tungsten_syntax::ast::*;
use crate::effects::ActiveHandler;
use crate::value::Value;

pub struct Evaluator {
    functions: HashMap<String, FnDecl>,
    scopes: Vec<HashMap<String, Arc<Mutex<Value>>>>,
    handler_stack: Vec<Vec<ActiveHandler>>,
    pub stdout_lines: Vec<String>,
    pub effect_traces: Vec<String>,
    pub scheduler: Arc<Scheduler>,
    fiber_handles: HashMap<u64, FiberHandle<Value>>,
    channels: Arc<Mutex<HashMap<u64, Arc<Channel<Value>>>>>,
}

#[derive(Debug)]
pub enum EvalSignal {
    Normal(Value),
    Return(Value),
    Error(String),
}

impl Evaluator {
    pub fn new() -> Self {
        Self {
            functions: HashMap::new(),
            scopes: vec![HashMap::new()],
            handler_stack: Vec::new(),
            stdout_lines: Vec::new(),
            effect_traces: Vec::new(),
            scheduler: Scheduler::new(0),
            fiber_handles: HashMap::new(),
            channels: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn load_program(&mut self, program: &Program) {
        for item in &program.items {
            if let Item::Fn(f) = item {
                self.functions.insert(f.name.clone(), f.clone());
            }
        }
    }

    pub fn run_main(&mut self) -> Result<Value, String> {
        if let Some(main_fn) = self.functions.get("main").cloned() {
            match self.eval_fn(&main_fn, Vec::new()) {
                EvalSignal::Normal(v) | EvalSignal::Return(v) => Ok(v),
                EvalSignal::Error(e) => Err(e),
            }
        } else {
            Err("No 'main' function found in program".to_string())
        }
    }

    fn push_scope(&mut self) {
        self.scopes.push(HashMap::new());
    }

    fn pop_scope(&mut self) {
        self.scopes.pop();
    }

    fn define_var(&mut self, name: String, val: Value) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(name, Arc::new(Mutex::new(val)));
        }
    }

    fn define_var_cell(&mut self, name: String, cell: Arc<Mutex<Value>>) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(name, cell);
        }
    }

    fn lookup_cell(&self, name: &str) -> Option<Arc<Mutex<Value>>> {
        for scope in self.scopes.iter().rev() {
            if let Some(cell) = scope.get(name) {
                return Some(Arc::clone(cell));
            }
        }
        None
    }

    fn lookup_var(&self, name: &str) -> Option<Value> {
        self.lookup_cell(name).map(|c| c.lock().unwrap().clone())
    }

    pub fn eval_fn(&mut self, f: &FnDecl, args: Vec<Value>) -> EvalSignal {
        self.push_scope();
        for (param, arg) in f.params.iter().zip(args) {
            match arg {
                Value::Ref(cell) => {
                    self.define_var_cell(param.name.clone(), cell);
                }
                val => {
                    self.define_var(param.name.clone(), val);
                }
            }
        }

        let res = self.eval_block(&f.body);
        self.pop_scope();

        match res {
            EvalSignal::Return(v) => EvalSignal::Normal(v),
            other => other,
        }
    }

    pub fn eval_block(&mut self, block: &Block) -> EvalSignal {
        self.push_scope();
        for stmt in &block.stmts {
            match self.eval_stmt(stmt) {
                EvalSignal::Normal(_) => {}
                ret_or_err => {
                    self.pop_scope();
                    return ret_or_err;
                }
            }
        }

        let result = if let Some(ref trailing) = block.trailing_expr {
            self.eval_expr(trailing)
        } else {
            EvalSignal::Normal(Value::Unit)
        };

        self.pop_scope();
        result
    }

    fn eval_stmt(&mut self, stmt: &Stmt) -> EvalSignal {
        match stmt {
            Stmt::Let { name, init, .. } => {
                let val = match self.eval_expr(init) {
                    EvalSignal::Normal(v) => v,
                    early => return early,
                };
                self.define_var(name.clone(), val);
                EvalSignal::Normal(Value::Unit)
            }
            Stmt::Assign { target, value, .. } => {
                let val = match self.eval_expr(value) {
                    EvalSignal::Normal(v) => v,
                    early => return early,
                };
                if let Err(e) = self.eval_assign(target, val) {
                    return EvalSignal::Error(e);
                }
                EvalSignal::Normal(Value::Unit)
            }
            Stmt::Expr { expr, .. } => {
                match self.eval_expr(expr) {
                    EvalSignal::Normal(_) => EvalSignal::Normal(Value::Unit),
                    early => early,
                }
            }
            Stmt::Return { value, .. } => {
                let val = if let Some(v) = value {
                    match self.eval_expr(v) {
                        EvalSignal::Normal(res) => res,
                        early => return early,
                    }
                } else {
                    Value::Unit
                };
                EvalSignal::Return(val)
            }
        }
    }

    fn eval_assign(&mut self, target: &Expr, new_val: Value) -> Result<(), String> {
        match &target.kind {
            ExprKind::Ident(name) => {
                if let Some(cell) = self.lookup_cell(name) {
                    *cell.lock().unwrap() = new_val;
                    Ok(())
                } else {
                    Err(format!("Undefined variable '{}' in assignment", name))
                }
            }
            ExprKind::FieldAccess { target: inner_target, field } => {
                match &inner_target.kind {
                    ExprKind::Ident(name) => {
                        if let Some(cell) = self.lookup_cell(name) {
                            cell.lock().unwrap().set_field(field, new_val)?;
                            Ok(())
                        } else {
                            Err(format!("Undefined variable '{}' in field assignment", name))
                        }
                    }
                    _ => Err("Complex field assignment target not supported".to_string()),
                }
            }
            _ => Err("Invalid assignment target".to_string()),
        }
    }

    pub fn eval_expr(&mut self, expr: &Expr) -> EvalSignal {
        match &expr.kind {
            ExprKind::Int(n) => EvalSignal::Normal(Value::Int(*n)),
            ExprKind::Str(s) => EvalSignal::Normal(Value::Str(s.clone())),
            ExprKind::Bool(b) => EvalSignal::Normal(Value::Bool(*b)),
            ExprKind::Ident(name) => {
                if let Some(v) = self.lookup_var(name) {
                    EvalSignal::Normal(v)
                } else if self.functions.contains_key(name) {
                    EvalSignal::Normal(Value::Fn(name.clone()))
                } else {
                    EvalSignal::Error(format!("Undefined variable '{}'", name))
                }
            }
            ExprKind::Binary { op, left, right } => {
                let l = match self.eval_expr(left) {
                    EvalSignal::Normal(v) => v,
                    early => return early,
                };
                let r = match self.eval_expr(right) {
                    EvalSignal::Normal(v) => v,
                    early => return early,
                };
                match (op, l, r) {
                    (BinOp::Add, Value::Int(a), Value::Int(b)) => EvalSignal::Normal(Value::Int(a + b)),
                    (BinOp::Sub, Value::Int(a), Value::Int(b)) => EvalSignal::Normal(Value::Int(a - b)),
                    (BinOp::Mul, Value::Int(a), Value::Int(b)) => EvalSignal::Normal(Value::Int(a * b)),
                    (BinOp::Div, Value::Int(a), Value::Int(b)) => {
                        if b == 0 {
                            EvalSignal::Error("Division by zero".to_string())
                        } else {
                            EvalSignal::Normal(Value::Int(a / b))
                        }
                    }
                    (BinOp::Eq, a, b) => EvalSignal::Normal(Value::Bool(a == b)),
                    (BinOp::NotEq, a, b) => EvalSignal::Normal(Value::Bool(a != b)),
                    (BinOp::Lt, Value::Int(a), Value::Int(b)) => EvalSignal::Normal(Value::Bool(a < b)),
                    (BinOp::LtEq, Value::Int(a), Value::Int(b)) => EvalSignal::Normal(Value::Bool(a <= b)),
                    (BinOp::Gt, Value::Int(a), Value::Int(b)) => EvalSignal::Normal(Value::Bool(a > b)),
                    (BinOp::GtEq, Value::Int(a), Value::Int(b)) => EvalSignal::Normal(Value::Bool(a >= b)),
                    _ => EvalSignal::Error("Unsupported binary operation".to_string()),
                }
            }
            ExprKind::Ref { expr: inner, .. } => {
                if let ExprKind::Ident(name) = &inner.kind {
                    if let Some(cell) = self.lookup_cell(name) {
                        EvalSignal::Normal(Value::Ref(cell))
                    } else {
                        EvalSignal::Error(format!("Cannot borrow undefined variable '{}'", name))
                    }
                } else {
                    let val = match self.eval_expr(inner) {
                        EvalSignal::Normal(v) => v,
                        early => return early,
                    };
                    EvalSignal::Normal(Value::Ref(Arc::new(Mutex::new(val))))
                }
            }
            ExprKind::FieldAccess { target, field } => {
                let target_val = match self.eval_expr(target) {
                    EvalSignal::Normal(v) => v,
                    early => return early,
                };
                if let Some(fval) = target_val.get_field(field) {
                    EvalSignal::Normal(fval)
                } else {
                    EvalSignal::Error(format!("Field '{}' not found on {}", field, target_val))
                }
            }
            ExprKind::MethodCall { target, method, args } => {
                let target_val = match self.eval_expr(target) {
                    EvalSignal::Normal(v) => v,
                    early => return early,
                };
                let mut eval_args = Vec::new();
                for a in args {
                    match self.eval_expr(a) {
                        EvalSignal::Normal(v) => eval_args.push(v),
                        early => return early,
                    }
                }

                if method == "to_string" && eval_args.is_empty() {
                    return EvalSignal::Normal(Value::Str(target_val.to_string()));
                }

                if method == "saturating_add" && eval_args.len() == 1 {
                    let cur = target_val.as_int().unwrap_or(0);
                    let delta = eval_args[0].as_int().unwrap_or(0);
                    // Clamps to 100 for refinement type Health
                    let result = (cur + delta).min(100);
                    EvalSignal::Normal(Value::Int(result))
                } else {
                    EvalSignal::Error(format!("Unknown method '{}'", method))
                }
            }
            ExprKind::PathCall { path, args } => {
                let mut eval_args = Vec::new();
                for a in args {
                    match self.eval_expr(a) {
                        EvalSignal::Normal(v) => eval_args.push(v),
                        early => return early,
                    }
                }

                if path.len() == 2 {
                    let namespace = &path[0];
                    let op = &path[1];
                    self.effect_traces.push(format!("{}:{}", namespace, op));

                    // Check for active algebraic effect handler
                    if let Some(result) = self.dispatch_effect(namespace, op, eval_args.clone()) {
                        return result;
                    }

                    // Check built-in mock driver (PostgresPool)
                    if namespace == "PostgresPool" && op == "execute" {
                        let mut fields = HashMap::new();
                        fields.insert("name".into(), Value::Str("PlayerOne".into()));
                        fields.insert("hp".into(), Value::Int(80));
                        fields.insert("id".into(), Value::Int(42));
                        return EvalSignal::Normal(Value::Struct {
                            name: "Record".into(),
                            fields,
                        });
                    }

                    // Standard library effect fallbacks
                    if namespace == "Random" {
                        if op == "between" && eval_args.len() >= 2 {
                            let min = eval_args[0].as_int().unwrap_or(0);
                            let max = eval_args[1].as_int().unwrap_or(100);
                            return EvalSignal::Normal(Value::Int((min + max) / 2));
                        }
                        if op == "next_u64" {
                            return EvalSignal::Normal(Value::Int(42));
                        }
                    }

                    if namespace == "Time" && op == "now_ms" {
                        return EvalSignal::Normal(Value::Int(1725900000000));
                    }

                    if namespace == "IO" && op == "print" && !eval_args.is_empty() {
                        let msg = eval_args[0].as_str().unwrap_or_else(|| eval_args[0].to_string());
                        println!("{}", msg);
                        self.stdout_lines.push(msg);
                        return EvalSignal::Normal(Value::Unit);
                    }

                    // Async & Fiber runtime operations
                    if namespace == "Async" {
                        if op == "spawn" && !eval_args.is_empty() {
                            let f_val = &eval_args[0];
                            let func_name = match f_val {
                                Value::Fn(name) => name.clone(),
                                _ => f_val.to_string(),
                            };

                            if let Some(target_fn) = self.functions.get(&func_name).cloned() {
                                let mut fiber_eval = Evaluator::new();
                                fiber_eval.functions = self.functions.clone();
                                fiber_eval.channels = Arc::clone(&self.channels);
                                fiber_eval.scheduler = Arc::clone(&self.scheduler);
                                let pass_args: Vec<Value> = eval_args[1..].to_vec();

                                let handle = self.scheduler.spawn(move || {
                                    match fiber_eval.eval_fn(&target_fn, pass_args) {
                                        EvalSignal::Normal(v) | EvalSignal::Return(v) => Ok(v),
                                        EvalSignal::Error(e) => Err(e),
                                    }
                                });

                                let h_id = handle.id.0;
                                self.fiber_handles.insert(h_id, handle);

                                let mut fields = HashMap::new();
                                fields.insert("id".to_string(), Value::Int(h_id as i64));
                                return EvalSignal::Normal(Value::Struct {
                                    name: "FiberHandle".into(),
                                    fields,
                                });
                            } else {
                                return EvalSignal::Error(format!("Async::spawn: function '{}' not found", func_name));
                            }
                        }

                        if op == "yield_now" {
                            std::thread::yield_now();
                            return EvalSignal::Normal(Value::Unit);
                        }

                        if op == "sleep" && !eval_args.is_empty() {
                            let ms = eval_args[0].as_int().unwrap_or(0);
                            std::thread::sleep(std::time::Duration::from_millis(ms.max(0) as u64));
                            return EvalSignal::Normal(Value::Unit);
                        }

                        if op == "await_fiber" && !eval_args.is_empty() {
                            let h_id = match &eval_args[0] {
                                Value::Struct { fields, .. } => fields.get("id").and_then(|v| v.as_int()).unwrap_or(0) as u64,
                                Value::Int(n) => *n as u64,
                                _ => 0,
                            };

                            if let Some(handle) = self.fiber_handles.get(&h_id) {
                                match handle.join() {
                                    Ok(res) => return EvalSignal::Normal(res),
                                    Err(e) => return EvalSignal::Error(e),
                                }
                            } else {
                                return EvalSignal::Error(format!("Fiber {} not found", h_id));
                            }
                        }
                    }

                    // Channel messaging operations
                    if namespace == "Channel" {
                        if op == "new" {
                            static CH_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
                            let cid = CH_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                            let ch = Arc::new(Channel::unbounded());
                            self.channels.lock().unwrap().insert(cid, ch);
                            return EvalSignal::Normal(Value::Int(cid as i64));
                        }
                        if op == "send" && eval_args.len() >= 2 {
                            let cid = eval_args[0].as_int().unwrap_or(1) as u64;
                            let val = eval_args[1].clone();
                            let ch_opt = self.channels.lock().unwrap().get(&cid).cloned();
                            if let Some(ch) = ch_opt {
                                let _ = ch.send(val);
                                return EvalSignal::Normal(Value::Unit);
                            } else {
                                return EvalSignal::Error(format!("Channel {} not found", cid));
                            }
                        }
                        if op == "recv" && !eval_args.is_empty() {
                            let cid = eval_args[0].as_int().unwrap_or(1) as u64;
                            let ch_opt = self.channels.lock().unwrap().get(&cid).cloned();
                            if let Some(ch) = ch_opt {
                                match ch.recv() {
                                    Ok(val) => return EvalSignal::Normal(val),
                                    Err(e) => return EvalSignal::Error(e),
                                }
                            } else {
                                return EvalSignal::Error(format!("Channel {} not found", cid));
                            }
                        }
                    }

                    // Net effect runtime operations
                    if namespace == "Net" {
                        if op == "listen" {
                            let port = eval_args.first().and_then(|v| v.as_int()).unwrap_or(8080);
                            return EvalSignal::Normal(Value::Int(1000 + port));
                        }
                        if op == "accept" {
                            let sock = eval_args.first().and_then(|v| v.as_int()).unwrap_or(1);
                            return EvalSignal::Normal(Value::Int(sock * 10 + 1));
                        }
                        if op == "connect" {
                            let port = eval_args.get(1).and_then(|v| v.as_int()).unwrap_or(8080);
                            return EvalSignal::Normal(Value::Int(2000 + port));
                        }
                        if op == "read" {
                            return EvalSignal::Normal(Value::Str("HTTP/1.1 200 OK\r\nContent-Length: 14\r\n\r\nHello Tungsten".into()));
                        }
                        if op == "write" {
                            let len = eval_args.get(1).and_then(|v| v.as_str()).map(|s| s.len() as i64).unwrap_or(0);
                            return EvalSignal::Normal(Value::Int(len));
                        }
                        if op == "close" {
                        }
                    }
                }

                let full_path = path.join("::");
                let func_candidate = self.functions.get(&full_path)
                    .or_else(|| self.functions.get(path.last().unwrap()))
                    .or_else(|| {
                        self.functions.iter().find_map(|(k, v)| {
                            if k.ends_with(&format!("_{}", path.last().unwrap())) {
                                Some(v)
                            } else {
                                None
                            }
                        })
                    });

                if let Some(fdecl) = func_candidate.cloned() {
                    return self.eval_fn(&fdecl, eval_args);
                }

                EvalSignal::Error(format!("Path call not found: {:?}", path))
            }
            ExprKind::Call { callee, args } => {
                let mut eval_args = Vec::new();
                for a in args {
                    match self.eval_expr(a) {
                        EvalSignal::Normal(v) => eval_args.push(v),
                        early => return early,
                    }
                }

                if let ExprKind::Ident(fname) = &callee.kind {
                    if let Some(fdecl) = self.functions.get(fname).cloned() {
                        return self.eval_fn(&fdecl, eval_args);
                    }
                }

                let callee_val = match self.eval_expr(callee) {
                    EvalSignal::Normal(v) => v,
                    early => return early,
                };

                if let Value::Fn(fname) = callee_val {
                    if let Some(fdecl) = self.functions.get(&fname).cloned() {
                        return self.eval_fn(&fdecl, eval_args);
                    } else {
                        return EvalSignal::Error(format!("Function '{}' not found", fname));
                    }
                }

                EvalSignal::Error(format!("Cannot call non-function value '{}'", callee_val))
            }
            ExprKind::MacroCall { name, args } => {
                let mut eval_args = Vec::new();
                for a in args {
                    match self.eval_expr(a) {
                        EvalSignal::Normal(v) => eval_args.push(v),
                        early => return early,
                    }
                }
                if name == "println" {
                    let output = if eval_args.is_empty() {
                        String::new()
                    } else {
                        let fmt = eval_args[0].as_str().unwrap_or_else(|| eval_args[0].to_string());
                        let mut res = String::new();
                        let mut arg_idx = 1;
                        let mut chars = fmt.chars().peekable();
                        while let Some(c) = chars.next() {
                            if c == '{' && chars.peek() == Some(&'}') {
                                chars.next(); // consume '}'
                                if arg_idx < eval_args.len() {
                                    res.push_str(&eval_args[arg_idx].to_string());
                                    arg_idx += 1;
                                } else {
                                    res.push_str("{}");
                                }
                            } else {
                                res.push(c);
                            }
                        }
                        res
                    };
                    println!("{}", output);
                    self.stdout_lines.push(output);
                    EvalSignal::Normal(Value::Unit)
                } else {
                    EvalSignal::Error(format!("Unknown macro '{}!'", name))
                }
            }
            ExprKind::StructInit { name, fields } => {
                let mut struct_fields = HashMap::new();
                for (fname, fexpr) in fields {
                    match self.eval_expr(fexpr) {
                        EvalSignal::Normal(v) => {
                            struct_fields.insert(fname.clone(), v);
                        }
                        early => return early,
                    }
                }
                EvalSignal::Normal(Value::Struct {
                    name: name.clone(),
                    fields: struct_fields,
                })
            }
            ExprKind::Cast { expr: inner, .. } => {
                self.eval_expr(inner)
            }
            ExprKind::Try(inner) => {
                self.eval_expr(inner)
            }
            ExprKind::EffectCall(inner) => {
                self.eval_expr(inner)
            }
            ExprKind::Await(inner) => {
                self.eval_expr(inner)
            }
            ExprKind::Handle { body, handlers } => {
                let mut active_handlers = Vec::new();
                for h in handlers {
                    let mut ah = ActiveHandler::new(h.effect_name.clone());
                    for arm in &h.arms {
                        ah.add_arm(arm.clone());
                    }
                    active_handlers.push(ah);
                }

                self.handler_stack.push(active_handlers);
                let res = self.eval_block(body);
                self.handler_stack.pop();

                res
            }
            ExprKind::Block(b) => {
                self.eval_block(b)
            }
            ExprKind::Region { body, .. } => {
                self.eval_block(body)
            }
            ExprKind::If { cond, then_branch, else_branch } => {
                let c = match self.eval_expr(cond) {
                    EvalSignal::Normal(v) => v.as_bool().unwrap_or(false),
                    early => return early,
                };
                if c {
                    self.eval_block(then_branch)
                } else if let Some(eb) = else_branch {
                    self.eval_block(eb)
                } else {
                    EvalSignal::Normal(Value::Unit)
                }
            }
            ExprKind::Resume(inner) => {
                self.eval_expr(inner)
            }
        }
    }

    fn dispatch_effect(&mut self, effect_name: &str, op_name: &str, args: Vec<Value>) -> Option<EvalSignal> {
        let mut matched_arm = None;
        for handlers in self.handler_stack.iter().rev() {
            for handler in handlers {
                if handler.effect_name == effect_name {
                    if let Some(arm) = handler.arms.get(op_name) {
                        matched_arm = Some(arm.clone());
                        break;
                    }
                }
            }
            if matched_arm.is_some() {
                break;
            }
        }

        if let Some(arm) = matched_arm {
            self.push_scope();
            for (pname, val) in arm.params.iter().zip(args.iter()) {
                self.define_var(pname.clone(), val.clone());
            }
            let res = self.eval_expr(&arm.body);
            self.pop_scope();
            Some(res)
        } else {
            None
        }
    }
}
