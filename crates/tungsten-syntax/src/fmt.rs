use crate::ast::*;

pub struct Formatter {
    indent_level: usize,
    output: String,
}

impl Formatter {
    pub fn new() -> Self {
        Self {
            indent_level: 0,
            output: String::new(),
        }
    }

    fn indent(&mut self) {
        for _ in 0..self.indent_level {
            self.output.push_str("    ");
        }
    }

    fn write(&mut self, s: &str) {
        self.output.push_str(s);
    }

    #[allow(dead_code)]
    fn writeln(&mut self, s: &str) {
        self.indent();
        self.output.push_str(s);
        self.output.push('\n');
    }

    pub fn format_program(mut self, program: &Program) -> String {
        for (i, item) in program.items.iter().enumerate() {
            if i > 0 {
                self.output.push('\n');
            }
            self.format_item(item);
        }
        self.output
    }

    fn format_item(&mut self, item: &Item) {
        match item {
            Item::TypeAlias(alias) => {
                self.indent();
                self.write(&format!("type {} = {};\n", alias.name, self.format_type(&alias.target)));
            }
            Item::Struct(st) => {
                self.indent();
                let generics = if st.type_params.is_empty() {
                    String::new()
                } else {
                    format!("<{}>", st.type_params.join(", "))
                };
                self.write(&format!("struct {}{} {{\n", st.name, generics));
                self.indent_level += 1;
                for field in &st.fields {
                    self.indent();
                    self.write(&format!("{}: {},\n", field.name, self.format_type(&field.ty)));
                }
                self.indent_level -= 1;
                self.indent();
                self.write("}\n");
            }
            Item::Fn(f) => {
                self.indent();
                let generics = if f.type_params.is_empty() {
                    String::new()
                } else {
                    format!("<{}>", f.type_params.join(", "))
                };
                self.write(&format!("fn {}{}(", f.name, generics));
                for (i, p) in f.params.iter().enumerate() {
                    if i > 0 {
                        self.write(", ");
                    }
                    if p.is_mut {
                        self.write("mut ");
                    }
                    self.write(&format!("{}: {}", p.name, self.format_type(&p.ty)));
                }
                self.write(")");
                if let Some(ref rt) = f.return_type {
                    self.write(&format!(" -> {}", self.format_type(rt)));
                }
                if !f.yields_effects.is_empty() {
                    self.write(&format!(" yields [{}]", f.yields_effects.join(", ")));
                }
                self.write(" ");
                self.format_block(&f.body);
                self.output.push('\n');
            }
            Item::Effect(eff) => {
                self.indent();
                self.write(&format!("effect {} {{\n", eff.name));
                self.indent_level += 1;
                for op in &eff.operations {
                    self.indent();
                    self.write(&format!("fn {}(", op.name));
                    for (i, (pname, pty)) in op.params.iter().enumerate() {
                        if i > 0 {
                            self.write(", ");
                        }
                        self.write(&format!("{}: {}", pname, self.format_type(pty)));
                    }
                    self.write(&format!(") -> {};\n", self.format_type(&op.return_type)));
                }
                self.indent_level -= 1;
                self.indent();
                self.write("}\n");
            }
        }
    }

    fn format_type(&self, ty: &TypeExpr) -> String {
        match ty {
            TypeExpr::Named(name, _) => name.clone(),
            TypeExpr::Generic { name, args, .. } => {
                let arg_strs: Vec<String> = args.iter().map(|a| self.format_type(a)).collect();
                format!("{}<{}>", name, arg_strs.join(", "))
            }
            TypeExpr::Refined { base, min, max, inclusive, .. } => {
                let op = if *inclusive { "..=" } else { ".." };
                format!("{}({}{}{})", base, min, op, max)
            }
            TypeExpr::Relational { base, predicate, .. } => {
                format!("{}({})", base, self.format_expr(predicate))
            }
            TypeExpr::Ref { is_mut, inner, .. } => {
                if *is_mut {
                    format!("&mut {}", self.format_type(inner))
                } else {
                    format!("&{}", self.format_type(inner))
                }
            }
            TypeExpr::Fn { params, return_type, yields_effects, .. } => {
                let p_strs: Vec<String> = params.iter().map(|p| self.format_type(p)).collect();
                let mut s = format!("fn({})", p_strs.join(", "));
                if !yields_effects.is_empty() {
                    s.push_str(&format!(" yields [{}]", yields_effects.join(", ")));
                }
                s.push_str(&format!(" -> {}", self.format_type(return_type)));
                s
            }
            TypeExpr::Unit(_) => "()".to_string(),
        }
    }

    fn format_block(&mut self, block: &Block) {
        self.write("{\n");
        self.indent_level += 1;
        for stmt in &block.stmts {
            self.format_stmt(stmt);
        }
        if let Some(ref trailing) = block.trailing_expr {
            self.indent();
            let expr_str = self.format_expr(trailing);
            self.write(&expr_str);
            self.write("\n");
        }
        self.indent_level -= 1;
        self.indent();
        self.write("}");
    }

    fn format_stmt(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::Let { name, is_mut, ty, init, .. } => {
                self.indent();
                self.write("let ");
                if *is_mut {
                    self.write("mut ");
                }
                self.write(name);
                if let Some(t) = ty {
                    self.write(&format!(": {}", self.format_type(t)));
                }
                let init_str = self.format_expr(init);
                self.write(&format!(" = {};\n", init_str));
            }
            Stmt::Assign { target, value, .. } => {
                self.indent();
                let target_str = self.format_expr(target);
                let value_str = self.format_expr(value);
                self.write(&format!("{} = {};\n", target_str, value_str));
            }
            Stmt::Expr { expr, has_semicolon, .. } => {
                self.indent();
                let expr_str = self.format_expr(expr);
                self.write(&expr_str);
                if *has_semicolon {
                    self.write(";");
                }
                self.write("\n");
            }
            Stmt::Return { value, .. } => {
                self.indent();
                self.write("return");
                if let Some(v) = value {
                    let val_str = self.format_expr(v);
                    self.write(&format!(" {}", val_str));
                }
                self.write(";\n");
            }
        }
    }

    fn format_expr(&self, expr: &Expr) -> String {
        match &expr.kind {
            ExprKind::Int(n) => n.to_string(),
            ExprKind::Str(s) => format!("\"{}\"", s.replace('"', "\\\"")),
            ExprKind::Bool(b) => b.to_string(),
            ExprKind::Ident(name) => name.clone(),
            ExprKind::Binary { op, left, right } => {
                let op_str = match op {
                    BinOp::Add => "+",
                    BinOp::Sub => "-",
                    BinOp::Mul => "*",
                    BinOp::Div => "/",
                    BinOp::Eq => "==",
                    BinOp::NotEq => "!=",
                    BinOp::Lt => "<",
                    BinOp::LtEq => "<=",
                    BinOp::Gt => ">",
                    BinOp::GtEq => ">=",
                    BinOp::And => "&&",
                    BinOp::Or => "||",
                };
                if let ExprKind::Ident(ref name) = left.kind {
                    if name == "_val" {
                        return format!("{} {}", op_str, self.format_expr(right));
                    }
                }
                format!("{} {} {}", self.format_expr(left), op_str, self.format_expr(right))
            }
            ExprKind::FieldAccess { target, field } => {
                format!("{}.{}", self.format_expr(target), field)
            }
            ExprKind::MethodCall { target, method, args } => {
                let arg_strs: Vec<String> = args.iter().map(|a| self.format_expr(a)).collect();
                format!("{}.{}({})", self.format_expr(target), method, arg_strs.join(", "))
            }
            ExprKind::PathCall { path, args } => {
                let arg_strs: Vec<String> = args.iter().map(|a| self.format_expr(a)).collect();
                format!("{}({})", path.join("::"), arg_strs.join(", "))
            }
            ExprKind::Call { callee, args } => {
                let arg_strs: Vec<String> = args.iter().map(|a| self.format_expr(a)).collect();
                format!("{}({})", self.format_expr(callee), arg_strs.join(", "))
            }
            ExprKind::MacroCall { name, args } => {
                let arg_strs: Vec<String> = args.iter().map(|a| self.format_expr(a)).collect();
                format!("{}!({})", name, arg_strs.join(", "))
            }
            ExprKind::StructInit { name, fields } => {
                if fields.is_empty() {
                    format!("{} {{}}", name)
                } else {
                    let mut s = format!("{} {{\n", name);
                    for (fname, fval) in fields {
                        for _ in 0..(self.indent_level + 1) {
                            s.push_str("    ");
                        }
                        s.push_str(&format!("{}: {},\n", fname, self.format_expr(fval)));
                    }
                    for _ in 0..self.indent_level {
                        s.push_str("    ");
                    }
                    s.push('}');
                    s
                }
            }
            ExprKind::Cast { expr, target_ty } => {
                format!("{} as {}", self.format_expr(expr), self.format_type(target_ty))
            }
            ExprKind::Try(inner) => {
                format!("{}?", self.format_expr(inner))
            }
            ExprKind::EffectCall(inner) => {
                format!("{}!", self.format_expr(inner))
            }
            ExprKind::Await(inner) => {
                format!("{}.await", self.format_expr(inner))
            }
            ExprKind::Ref { is_mut, expr: inner } => {
                if *is_mut {
                    format!("&mut {}", self.format_expr(inner))
                } else {
                    format!("&{}", self.format_expr(inner))
                }
            }
            ExprKind::Handle { body, handlers } => {
                let mut s = "handle ".to_string();
                let mut sub_fmt = Formatter {
                    indent_level: self.indent_level,
                    output: String::new(),
                };
                sub_fmt.format_block(body);
                s.push_str(&sub_fmt.output);

                for h in handlers {
                    s.push_str(&format!(" with {} {{\n", h.effect_name));
                    for arm in &h.arms {
                        for _ in 0..(self.indent_level + 1) {
                            s.push_str("    ");
                        }
                        if arm.op_name == "_catch" {
                            s.push_str(&format!("{} => {},\n", arm.params.first().cloned().unwrap_or_default(), self.format_expr(&arm.body)));
                        } else {
                            s.push_str(&format!("{}({}) => {},\n", arm.op_name, arm.params.join(", "), self.format_expr(&arm.body)));
                        }
                    }
                    for _ in 0..self.indent_level {
                        s.push_str("    ");
                    }
                    s.push('}');
                }
                s
            }
            ExprKind::Block(b) => {
                let mut sub_fmt = Formatter {
                    indent_level: self.indent_level,
                    output: String::new(),
                };
                sub_fmt.format_block(b);
                sub_fmt.output
            }
            ExprKind::If { cond, then_branch, else_branch } => {
                let mut s = format!("if {} ", self.format_expr(cond));
                let mut sub_fmt = Formatter {
                    indent_level: self.indent_level,
                    output: String::new(),
                };
                sub_fmt.format_block(then_branch);
                s.push_str(&sub_fmt.output);

                if let Some(eb) = else_branch {
                    s.push_str(" else ");
                    let mut eb_fmt = Formatter {
                        indent_level: self.indent_level,
                        output: String::new(),
                    };
                    eb_fmt.format_block(eb);
                    s.push_str(&eb_fmt.output);
                }
                s
            }
        }
    }
}

pub fn format(program: &Program) -> String {
    let formatter = Formatter::new();
    formatter.format_program(program)
}

pub fn format_expr(expr: &Expr) -> String {
    let formatter = Formatter::new();
    formatter.format_expr(expr)
}

pub fn format_source(source: &str) -> Result<String, String> {
    let program = crate::parse(source)?;
    Ok(format(&program))
}
