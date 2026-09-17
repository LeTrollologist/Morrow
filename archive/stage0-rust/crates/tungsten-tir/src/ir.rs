use tungsten_syntax::ast::BinOp;
use tungsten_syntax::token::Span;
use tungsten_typeck::interval::Interval;
use tungsten_typeck::types::Type;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BlockId(pub usize);

impl std::fmt::Display for BlockId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "bb{}", self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Var {
    Temp(usize),
    Named(String),
}

impl std::fmt::Display for Var {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Var::Temp(id) => write!(f, "%{}", id),
            Var::Named(name) => write!(f, "_{}", name),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum TirConstant {
    Int(i64),
    Str(String),
    Bool(bool),
    Unit,
}

impl std::fmt::Display for TirConstant {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TirConstant::Int(n) => write!(f, "{}", n),
            TirConstant::Str(s) => write!(f, "\"{}\"", s),
            TirConstant::Bool(b) => write!(f, "{}", b),
            TirConstant::Unit => write!(f, "()"),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Operand {
    Constant(TirConstant),
    Var(Var, Type),
}

impl Operand {
    pub fn get_type(&self) -> Type {
        match self {
            Operand::Constant(TirConstant::Int(_)) => Type::I64,
            Operand::Constant(TirConstant::Str(_)) => Type::String,
            Operand::Constant(TirConstant::Bool(_)) => Type::Bool,
            Operand::Constant(TirConstant::Unit) => Type::Unit,
            Operand::Var(_, ty) => ty.clone(),
        }
    }
}

impl std::fmt::Display for Operand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Operand::Constant(c) => write!(f, "{}", c),
            Operand::Var(v, _) => write!(f, "{}", v),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum RValue {
    Use(Operand),
    BinaryOp(BinOp, Operand, Operand),
    FieldAccess {
        target: Operand,
        field: String,
    },
    MethodCall {
        target: Operand,
        method: String,
        args: Vec<Operand>,
    },
    StructInit {
        name: String,
        fields: Vec<(String, Operand)>,
        arena: Option<Operand>,
    },
    Ref {
        is_mut: bool,
        operand: Operand,
    },
    Cast {
        operand: Operand,
        target_ty: Type,
    },
    EnumInit {
        enum_name: String,
        variant: String,
        tag: usize,
        payload: Vec<Operand>,
        arena: Option<Operand>,
    },
    EnumTag(Operand),
    EnumPayload {
        target: Operand,
        index: usize,
    },
    ArrayInit {
        elements: Vec<Operand>,
        elem_stride: usize,
        arena: Option<Operand>,
    },
    ArrayIndex {
        target: Operand,
        index: Operand,
        stride: usize,
    },
    Deref(Operand),
    AddrOf(Operand),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Instruction {
    Assign {
        dest: Var,
        rvalue: RValue,
        ty: Type,
        span: Span,
    },
    AssertRefinement {
        operand: Operand,
        interval: Interval,
        error_msg: String,
        span: Span,
    },
    PerformEffect {
        effect: String,
        op: String,
        args: Vec<Operand>,
        dest: Option<Var>,
        ty: Type,
        span: Span,
    },
    Call {
        dest: Option<Var>,
        func: Operand,
        args: Vec<Operand>,
        ty: Type,
        span: Span,
    },
    ExternCall {
        dest: Option<Var>,
        func: String,
        args: Vec<Operand>,
        ty: Type,
        span: Span,
    },
    Store {
        ptr: Operand,
        value: Operand,
        span: Span,
    },
    StoreIndex {
        target: Operand,
        index: Operand,
        stride: usize,
        value: Operand,
        span: Span,
    },
    SetField {
        base: Var,
        field: String,
        val: Operand,
        span: Span,
    },
    RegionEnter {
        dest: Var,
        region_id: usize,
        span: Span,
    },
    RegionExit {
        arena: Operand,
        span: Span,
    },
    NurseryEnter {
        dest: Var,
        nursery_id: usize,
        span: Span,
    },
    NurseryExit {
        nursery: Operand,
        span: Span,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct TirHandler {
    pub effect: String,
    pub op: String,
    pub param: Option<String>,
    pub handler_entry: BlockId,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Terminator {
    Return(Option<Operand>),
    Branch(BlockId),
    BranchCond {
        cond: Operand,
        then_block: BlockId,
        else_block: BlockId,
    },
    HandleEffect {
        body_entry: BlockId,
        handlers: Vec<TirHandler>,
        exit_block: BlockId,
    },
    Resume {
        arg: Option<Operand>,
        continuation_block: BlockId,
    },
    Unreachable,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BasicBlock {
    pub id: BlockId,
    pub label: Option<String>,
    pub instructions: Vec<Instruction>,
    pub terminator: Option<Terminator>,
}

impl BasicBlock {
    pub fn new(id: BlockId, label: Option<String>) -> Self {
        Self {
            id,
            label,
            instructions: Vec::new(),
            terminator: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct TirParam {
    pub name: String,
    pub is_mut: bool,
    pub ty: Type,
    pub interval: Option<Interval>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TirFunction {
    pub name: String,
    pub type_params: Vec<String>,
    pub effect_params: Vec<String>,
    pub params: Vec<TirParam>,
    pub return_type: Type,
    pub yields_effects: Vec<String>,
    pub blocks: Vec<BasicBlock>,
    pub entry_block: BlockId,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TirModule {
    pub functions: Vec<TirFunction>,
    pub structs: Vec<tungsten_syntax::ast::StructDecl>,
    pub enums: Vec<tungsten_syntax::ast::EnumDecl>,
    pub effects: Vec<tungsten_syntax::ast::EffectDecl>,
    pub extern_blocks: Vec<tungsten_syntax::ast::ExternBlock>,
    pub source_file: Option<String>,
    pub source_dir: Option<String>,
}
