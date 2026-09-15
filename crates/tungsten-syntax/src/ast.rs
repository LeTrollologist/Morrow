use crate::token::Span;

#[derive(Debug, Clone, PartialEq)]
pub struct Program {
    pub items: Vec<Item>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Item {
    Import(ImportDecl),
    TypeAlias(TypeAlias),
    Struct(StructDecl),
    Enum(EnumDecl),
    Fn(FnDecl),
    Effect(EffectDecl),
    ExternBlock(ExternBlock),
}

#[derive(Debug, Clone, PartialEq)]
pub struct ImportDecl {
    pub path: Vec<String>,
    pub alias: Option<String>,
    pub span: Span,
}

/// An `extern "C" { ... }` block containing C ABI function declarations.
#[derive(Debug, Clone, PartialEq)]
pub struct ExternBlock {
    pub abi: String,             // e.g. "C"
    pub fns: Vec<ExternFnDecl>,
    pub span: Span,
}

/// A single `fn name(params) -> ret` declaration inside an extern block.
#[derive(Debug, Clone, PartialEq)]
pub struct ExternFnDecl {
    pub name: String,
    pub params: Vec<(String, TypeExpr)>,
    pub ret: TypeExpr,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TypeAlias {
    pub name: String,
    pub is_pub: bool,
    pub target: TypeExpr,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StructDecl {
    pub name: String,
    pub is_pub: bool,
    pub repr_c: bool,            // true when decorated with #[repr(C)]
    pub type_params: Vec<String>,
    pub fields: Vec<FieldDef>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FieldDef {
    pub name: String,
    pub ty: TypeExpr,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EnumDecl {
    pub name: String,
    pub is_pub: bool,
    pub type_params: Vec<String>,
    pub variants: Vec<VariantDef>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct VariantDef {
    pub name: String,
    pub payload: Vec<TypeExpr>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EffectDecl {
    pub name: String,
    pub is_pub: bool,
    pub operations: Vec<EffectOpDef>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EffectOpDef {
    pub name: String,
    pub params: Vec<(String, TypeExpr)>,
    pub return_type: TypeExpr,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FnDecl {
    pub name: String,
    pub is_pub: bool,
    pub type_params: Vec<String>,
    pub effect_params: Vec<String>,
    pub params: Vec<Param>,
    pub return_type: Option<TypeExpr>,
    pub yields_effects: Vec<String>,
    pub body: Block,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Param {
    pub name: String,
    pub is_mut: bool,
    pub ty: TypeExpr,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TypeExpr {
    Named(String, Span),
    Generic {
        name: String,
        args: Vec<TypeExpr>,
        span: Span,
    },
    Refined {
        base: String,
        min: i64,
        max: i64,
        inclusive: bool,
        span: Span,
    },
    Relational {
        base: String,
        predicate: Box<Expr>,
        span: Span,
    },
    Ref {
        is_mut: bool,
        inner: Box<TypeExpr>,
        span: Span,
    },
    /// Raw C-compatible pointer: `*T` (immutable) or `*mut T` (mutable).
    /// Only dereferenceable inside `unsafe { ... }` blocks.
    Ptr {
        mutable: bool,
        inner: Box<TypeExpr>,
        span: Span,
    },
    Fn {
        params: Vec<TypeExpr>,
        return_type: Box<TypeExpr>,
        yields_effects: Vec<String>,
        span: Span,
    },
    Array {
        elem: Box<TypeExpr>,
        len: usize,
        span: Span,
    },
    Unit(Span),
}

impl TypeExpr {
    pub fn span(&self) -> Span {
        match self {
            TypeExpr::Named(_, s) => *s,
            TypeExpr::Generic { span, .. } => *span,
            TypeExpr::Refined { span, .. } => *span,
            TypeExpr::Relational { span, .. } => *span,
            TypeExpr::Ref { span, .. } => *span,
            TypeExpr::Ptr { span, .. } => *span,
            TypeExpr::Fn { span, .. } => *span,
            TypeExpr::Array { span, .. } => *span,
            TypeExpr::Unit(s) => *s,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Block {
    pub stmts: Vec<Stmt>,
    pub trailing_expr: Option<Box<Expr>>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Stmt {
    Let {
        name: String,
        is_mut: bool,
        ty: Option<TypeExpr>,
        init: Expr,
        span: Span,
    },
    Assign {
        target: Expr,
        value: Expr,
        span: Span,
    },
    Expr {
        expr: Expr,
        has_semicolon: bool,
        span: Span,
    },
    Return {
        value: Option<Expr>,
        span: Span,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Eq,
    NotEq,
    Lt,
    LtEq,
    Gt,
    GtEq,
    And,
    Or,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ExprKind {
    Int(i64),
    Str(String),
    Bool(bool),
    Ident(String),
    Binary {
        op: BinOp,
        left: Box<Expr>,
        right: Box<Expr>,
    },
    FieldAccess {
        target: Box<Expr>,
        field: String,
    },
    MethodCall {
        target: Box<Expr>,
        method: String,
        args: Vec<Expr>,
    },
    PathCall {
        path: Vec<String>,
        args: Vec<Expr>,
    },
    Call {
        callee: Box<Expr>,
        args: Vec<Expr>,
    },
    MacroCall {
        name: String,
        args: Vec<Expr>,
    },
    StructInit {
        name: String,
        fields: Vec<(String, Expr)>,
    },
    Cast {
        expr: Box<Expr>,
        target_ty: TypeExpr,
    },
    Try(Box<Expr>),
    EffectCall(Box<Expr>),
    Await(Box<Expr>),
    Ref {
        is_mut: bool,
        expr: Box<Expr>,
    },
    Handle {
        body: Block,
        handlers: Vec<HandlerClause>,
    },
    Block(Block),
    If {
        cond: Box<Expr>,
        then_branch: Block,
        else_branch: Option<Block>,
    },
    Region {
        name: Option<String>,
        body: Block,
    },
    Nursery {
        name: Option<String>,
        body: Block,
    },
    Loop(Block),
    Resume(Box<Expr>),
    Match {
        expr: Box<Expr>,
        arms: Vec<MatchArm>,
    },
    Array(Vec<Expr>),
    Index {
        target: Box<Expr>,
        index: Box<Expr>,
    },
    Path(Vec<String>),
    /// An `unsafe { ... }` block. Required for raw pointer dereferences and extern C calls.
    Unsafe {
        body: Block,
    },
    /// Unary dereference of a raw pointer: `*ptr`. Only valid inside `unsafe { ... }`.
    Deref(Box<Expr>),
    /// Take the raw address of a local variable: `&x` (immutable) or `&mut x` (mutable).
    AddrOf {
        mutable: bool,
        expr: Box<Expr>,
    },
}



#[derive(Debug, Clone, PartialEq)]
pub struct Expr {
    pub kind: ExprKind,
    pub span: Span,
}

impl Expr {
    pub fn new(kind: ExprKind, span: Span) -> Self {
        Self { kind, span }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct HandlerClause {
    pub effect_name: String,
    pub arms: Vec<HandlerArm>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct HandlerArm {
    pub op_name: String,
    pub params: Vec<String>,
    pub body: Expr,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Pattern {
    Wildcard(Span),
    Literal(Box<Expr>),
    Variable(String, Span),
    Variant {
        enum_name: Option<String>,
        variant_name: String,
        subpatterns: Vec<Pattern>,
        span: Span,
    },
}

impl Pattern {
    pub fn span(&self) -> Span {
        match self {
            Pattern::Wildcard(s) => *s,
            Pattern::Literal(e) => e.span,
            Pattern::Variable(_, s) => *s,
            Pattern::Variant { span, .. } => *span,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct MatchArm {
    pub pattern: Pattern,
    pub body: Expr,
    pub span: Span,
}
