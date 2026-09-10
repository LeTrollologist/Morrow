#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
    pub line: usize,
    pub column: usize,
}

impl Span {
    pub fn new(start: usize, end: usize, line: usize, column: usize) -> Self {
        Self { start, end, line, column }
    }

    pub fn dummy() -> Self {
        Self { start: 0, end: 0, line: 1, column: 1 }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    // Keywords
    Fn,
    Struct,
    Type,
    Let,
    Mut,
    If,
    Else,
    Handle,
    With,
    Yields,
    Context,
    Return,
    True,
    False,
    As,

    // Identifiers & Literals
    Ident(String),
    Int(i64),
    Str(String),

    // Symbols & Delimiters
    LParen,      // (
    RParen,      // )
    LBrace,      // {
    RBrace,      // }
    LBracket,    // [
    RBracket,    // ]
    Semicolon,   // ;
    Comma,       // ,
    Colon,       // :
    ColonColon,  // ::
    Dot,         // .
    Arrow,       // ->
    FatArrow,    // =>

    // Operators
    Plus,        // +
    Minus,       // -
    Star,        // *
    Slash,       // /
    Eq,          // =
    EqEq,        // ==
    NotEq,       // !=
    Lt,          // <
    LtEq,        // <=
    Gt,          // >
    GtEq,        // >=
    Exclamation, // !
    Question,    // ?
    Ampersand,   // &
    AmpAmp,      // &&
    PipePipe,    // ||
    DotDotEq,    // ..=
    DotDot,      // ..

    Eof,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

impl Token {
    pub fn new(kind: TokenKind, span: Span) -> Self {
        Self { kind, span }
    }
}
