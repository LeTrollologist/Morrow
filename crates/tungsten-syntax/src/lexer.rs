use crate::token::{Span, Token, TokenKind};

pub struct Lexer<'a> {
    source: &'a str,
    chars: Vec<(usize, char)>,
    cursor: usize,
    line: usize,
    column: usize,
}

impl<'a> Lexer<'a> {
    pub fn new(source: &'a str) -> Self {
        let chars = source.char_indices().collect();
        Self {
            source,
            chars,
            cursor: 0,
            line: 1,
            column: 1,
        }
    }

    fn peek_char(&self) -> Option<char> {
        self.chars.get(self.cursor).map(|&(_, ch)| ch)
    }

    fn peek_next_char(&self) -> Option<char> {
        self.chars.get(self.cursor + 1).map(|&(_, ch)| ch)
    }

    fn advance(&mut self) -> Option<char> {
        if let Some(&(_, ch)) = self.chars.get(self.cursor) {
            self.cursor += 1;
            if ch == '\n' {
                self.line += 1;
                self.column = 1;
            } else {
                self.column += 1;
            }
            Some(ch)
        } else {
            None
        }
    }

    fn current_pos(&self) -> (usize, usize, usize) {
        let byte_pos = self.chars.get(self.cursor).map(|&(pos, _)| pos).unwrap_or(self.source.len());
        (byte_pos, self.line, self.column)
    }

    pub fn tokenize(&mut self) -> Result<Vec<Token>, String> {
        let mut tokens = Vec::new();

        while let Some(ch) = self.peek_char() {
            if ch.is_whitespace() {
                self.advance();
                continue;
            }

            // Line comments
            if ch == '/' && self.peek_next_char() == Some('/') {
                while let Some(c) = self.advance() {
                    if c == '\n' {
                        break;
                    }
                }
                continue;
            }

            let (start_byte, start_line, start_col) = self.current_pos();

            let kind = match ch {
                '(' => { self.advance(); TokenKind::LParen }
                ')' => { self.advance(); TokenKind::RParen }
                '{' => { self.advance(); TokenKind::LBrace }
                '}' => { self.advance(); TokenKind::RBrace }
                '[' => { self.advance(); TokenKind::LBracket }
                ']' => { self.advance(); TokenKind::RBracket }
                ';' => { self.advance(); TokenKind::Semicolon }
                ',' => { self.advance(); TokenKind::Comma }
                ':' => {
                    self.advance();
                    if self.peek_char() == Some(':') {
                        self.advance();
                        TokenKind::ColonColon
                    } else {
                        TokenKind::Colon
                    }
                }
                '.' => {
                    self.advance();
                    if self.peek_char() == Some('.') {
                        self.advance();
                        if self.peek_char() == Some('=') {
                            self.advance();
                            TokenKind::DotDotEq
                        } else {
                            TokenKind::DotDot
                        }
                    } else {
                        TokenKind::Dot
                    }
                }
                '-' => {
                    self.advance();
                    if self.peek_char() == Some('>') {
                        self.advance();
                        TokenKind::Arrow
                    } else {
                        TokenKind::Minus
                    }
                }
                '+' => { self.advance(); TokenKind::Plus }
                '*' => { self.advance(); TokenKind::Star }
                '/' => { self.advance(); TokenKind::Slash }
                '=' => {
                    self.advance();
                    if self.peek_char() == Some('>') {
                        self.advance();
                        TokenKind::FatArrow
                    } else if self.peek_char() == Some('=') {
                        self.advance();
                        TokenKind::EqEq
                    } else {
                        TokenKind::Eq
                    }
                }
                '!' => {
                    self.advance();
                    if self.peek_char() == Some('=') {
                        self.advance();
                        TokenKind::NotEq
                    } else {
                        TokenKind::Exclamation
                    }
                }
                '?' => { self.advance(); TokenKind::Question }
                '&' => {
                    self.advance();
                    if self.peek_char() == Some('&') {
                        self.advance();
                        TokenKind::AmpAmp
                    } else {
                        TokenKind::Ampersand
                    }
                }
                '|' => {
                    self.advance();
                    if self.peek_char() == Some('|') {
                        self.advance();
                        TokenKind::PipePipe
                    } else {
                        return Err(format!("Unexpected character '|' at line {}, column {}", start_line, start_col));
                    }
                }
                '<' => {
                    self.advance();
                    if self.peek_char() == Some('=') {
                        self.advance();
                        TokenKind::LtEq
                    } else {
                        TokenKind::Lt
                    }
                }
                '>' => {
                    self.advance();
                    if self.peek_char() == Some('=') {
                        self.advance();
                        TokenKind::GtEq
                    } else {
                        TokenKind::Gt
                    }
                }
                '"' => {
                    self.advance(); // consume opening quote
                    let mut s = String::new();
                    let mut closed = false;
                    while let Some(c) = self.advance() {
                        if c == '"' {
                            closed = true;
                            break;
                        } else if c == '\\' {
                            if let Some(escaped) = self.advance() {
                                match escaped {
                                    'n' => s.push('\n'),
                                    't' => s.push('\t'),
                                    'r' => s.push('\r'),
                                    '\\' => s.push('\\'),
                                    '"' => s.push('"'),
                                    other => s.push(other),
                                }
                            }
                        } else {
                            s.push(c);
                        }
                    }
                    if !closed {
                        return Err(format!("Unterminated string literal at line {}, column {}", start_line, start_col));
                    }
                    TokenKind::Str(s)
                }
                c if c.is_ascii_digit() => {
                    let mut num_str = String::new();
                    while let Some(digit) = self.peek_char() {
                        if digit.is_ascii_digit() || digit == '_' {
                            if digit != '_' {
                                num_str.push(digit);
                            }
                            self.advance();
                        } else {
                            break;
                        }
                    }
                    let val = num_str.parse::<i64>().map_err(|e| format!("Invalid integer literal '{}': {}", num_str, e))?;
                    TokenKind::Int(val)
                }
                c if c.is_ascii_alphabetic() || c == '_' => {
                    let mut ident = String::new();
                    while let Some(ic) = self.peek_char() {
                        if ic.is_ascii_alphanumeric() || ic == '_' {
                            ident.push(ic);
                            self.advance();
                        } else {
                            break;
                        }
                    }
                    match ident.as_str() {
                        "fn" => TokenKind::Fn,
                        "struct" => TokenKind::Struct,
                        "type" => TokenKind::Type,
                        "let" => TokenKind::Let,
                        "mut" => TokenKind::Mut,
                        "if" => TokenKind::If,
                        "else" => TokenKind::Else,
                        "handle" => TokenKind::Handle,
                        "with" => TokenKind::With,
                        "yields" => TokenKind::Yields,
                        "context" => TokenKind::Context,
                        "return" => TokenKind::Return,
                        "true" => TokenKind::True,
                        "false" => TokenKind::False,
                        "as" => TokenKind::As,
                        "region" => TokenKind::Region,
                        "effect" => TokenKind::Effect,
                        "resume" => TokenKind::Resume,
                        "import" => TokenKind::Import,
                        "pub" => TokenKind::Pub,
                        _ => TokenKind::Ident(ident),
                    }

                }
                unknown => {
                    return Err(format!("Unexpected character '{}' at line {}, column {}", unknown, start_line, start_col));
                }
            };

            let (end_byte, _, _) = self.current_pos();
            tokens.push(Token::new(
                kind,
                Span::new(start_byte, end_byte, start_line, start_col),
            ));
        }

        let (final_byte, final_line, final_col) = self.current_pos();
        tokens.push(Token::new(
            TokenKind::Eof,
            Span::new(final_byte, final_byte, final_line, final_col),
        ));

        Ok(tokens)
    }
}
