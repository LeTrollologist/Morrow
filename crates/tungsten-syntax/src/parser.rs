use std::collections::HashMap;
use crate::ast::*;
use crate::token::{Span, Token, TokenKind};

pub struct Parser {
    tokens: Vec<Token>,
    cursor: usize,
}

impl Parser {
    pub fn new(tokens: Vec<Token>) -> Self {
        Self { tokens, cursor: 0 }
    }

    fn peek(&self) -> &Token {
        &self.tokens[self.cursor]
    }

    #[allow(dead_code)]
    fn peek_next(&self) -> &Token {
        if self.cursor + 1 < self.tokens.len() {
            &self.tokens[self.cursor + 1]
        } else {
            &self.tokens[self.tokens.len() - 1]
        }
    }

    fn advance(&mut self) -> Token {
        let tok = self.peek().clone();
        if self.cursor < self.tokens.len() - 1 {
            self.cursor += 1;
        }
        tok
    }

    fn check(&self, kind: &TokenKind) -> bool {
        &self.peek().kind == kind
    }

    fn match_token(&mut self, kind: &TokenKind) -> bool {
        if self.check(kind) {
            self.advance();
            true
        } else {
            false
        }
    }

    fn expect(&mut self, kind: TokenKind) -> Result<Token, String> {
        let tok = self.peek();
        if std::mem::discriminant(&tok.kind) == std::mem::discriminant(&kind) {
            Ok(self.advance())
        } else {
            Err(format!(
                "Expected {:?}, found {:?} at line {}, column {}",
                kind, tok.kind, tok.span.line, tok.span.column
            ))
        }
    }

    fn expect_ident(&mut self) -> Result<(String, Span), String> {
        let tok = self.peek().clone();
        if let TokenKind::Ident(name) = tok.kind {
            self.advance();
            Ok((name, tok.span))
        } else {
            Err(format!(
                "Expected identifier, found {:?} at line {}, column {}",
                tok.kind, tok.span.line, tok.span.column
            ))
        }
    }

    pub fn parse_program(&mut self) -> Result<Program, String> {
        let mut items = Vec::new();
        while !self.check(&TokenKind::Eof) {
            items.push(self.parse_item()?);
        }
        Ok(Program { items })
    }

    fn parse_item(&mut self) -> Result<Item, String> {
        let is_pub = self.match_token(&TokenKind::Pub);
        match self.peek().kind {
            TokenKind::Import => {
                if is_pub {
                    return Err("`pub import` is not currently supported".into());
                }
                self.parse_import_decl().map(Item::Import)
            }
            TokenKind::Type => self.parse_type_alias(is_pub).map(Item::TypeAlias),
            TokenKind::Struct => self.parse_struct_decl(is_pub).map(Item::Struct),
            TokenKind::Fn => self.parse_fn_decl(is_pub).map(Item::Fn),
            TokenKind::Effect => self.parse_effect_decl(is_pub).map(Item::Effect),
            _ => Err(format!(
                "Unexpected token {:?} when expecting top-level item at line {}, col {}",
                self.peek().kind,
                self.peek().span.line,
                self.peek().span.column
            )),
        }
    }

    fn parse_import_decl(&mut self) -> Result<ImportDecl, String> {
        let start_tok = self.expect(TokenKind::Import)?;
        let mut path = Vec::new();
        let (first_seg, _) = self.expect_ident()?;
        path.push(first_seg);

        while self.match_token(&TokenKind::ColonColon) {
            let (next_seg, _) = self.expect_ident()?;
            path.push(next_seg);
        }

        let mut alias = None;
        if self.match_token(&TokenKind::As) {
            let (alias_name, _) = self.expect_ident()?;
            alias = Some(alias_name);
        }

        let end_tok = self.expect(TokenKind::Semicolon)?;
        let span = Span::new(start_tok.span.start, end_tok.span.end, start_tok.span.line, start_tok.span.column);
        Ok(ImportDecl {
            path,
            alias,
            span,
        })
    }

    fn parse_type_alias(&mut self, is_pub: bool) -> Result<TypeAlias, String> {
        let start_tok = self.expect(TokenKind::Type)?;
        let (name, _) = self.expect_ident()?;
        self.expect(TokenKind::Eq)?;
        let target = self.parse_type_expr()?;
        let end_tok = self.expect(TokenKind::Semicolon)?;
        Ok(TypeAlias {
            name,
            is_pub,
            target,
            span: Span::new(start_tok.span.start, end_tok.span.end, start_tok.span.line, start_tok.span.column),
        })
    }

    fn parse_struct_decl(&mut self, is_pub: bool) -> Result<StructDecl, String> {
        let start_tok = self.expect(TokenKind::Struct)?;
        let (name, _) = self.expect_ident()?;

        let mut type_params = Vec::new();
        if self.match_token(&TokenKind::Lt) {
            while !self.check(&TokenKind::Gt) && !self.check(&TokenKind::Eof) {
                let (p, _) = self.expect_ident()?;
                type_params.push(p);
                if !self.match_token(&TokenKind::Comma) {
                    break;
                }
            }
            self.expect(TokenKind::Gt)?;
        }

        self.expect(TokenKind::LBrace)?;

        let mut fields = Vec::new();
        while !self.check(&TokenKind::RBrace) && !self.check(&TokenKind::Eof) {
            let (f_name, f_span) = self.expect_ident()?;
            self.expect(TokenKind::Colon)?;
            let ty = self.parse_type_expr()?;
            self.match_token(&TokenKind::Comma);
            fields.push(FieldDef {
                name: f_name,
                ty,
                span: f_span,
            });
        }
        let end_tok = self.expect(TokenKind::RBrace)?;
        Ok(StructDecl {
            name,
            is_pub,
            type_params,
            fields,
            span: Span::new(start_tok.span.start, end_tok.span.end, start_tok.span.line, start_tok.span.column),
        })
    }

    fn parse_effect_decl(&mut self, is_pub: bool) -> Result<EffectDecl, String> {
        let start_tok = self.expect(TokenKind::Effect)?;
        let (name, _) = self.expect_ident()?;
        self.expect(TokenKind::LBrace)?;
        let mut operations = Vec::new();
        while !self.check(&TokenKind::RBrace) && !self.check(&TokenKind::Eof) {
            self.expect(TokenKind::Fn)?;
            let (op_name, op_span) = self.expect_ident()?;
            self.expect(TokenKind::LParen)?;
            let mut params = Vec::new();
            while !self.check(&TokenKind::RParen) && !self.check(&TokenKind::Eof) {
                let (p_name, _) = self.expect_ident()?;
                self.expect(TokenKind::Colon)?;
                let p_ty = self.parse_type_expr()?;
                params.push((p_name, p_ty));
                if !self.match_token(&TokenKind::Comma) {
                    break;
                }
            }
            self.expect(TokenKind::RParen)?;
            self.expect(TokenKind::Arrow)?;
            let return_type = self.parse_type_expr()?;
            self.expect(TokenKind::Semicolon)?;
            operations.push(EffectOpDef {
                name: op_name,
                params,
                return_type,
                span: op_span,
            });
        }
        let end_tok = self.expect(TokenKind::RBrace)?;
        Ok(EffectDecl {
            name,
            is_pub,
            operations,
            span: Span::new(start_tok.span.start, end_tok.span.end, start_tok.span.line, start_tok.span.column),
        })
    }

    fn parse_fn_decl(&mut self, is_pub: bool) -> Result<FnDecl, String> {
        let start_tok = self.expect(TokenKind::Fn)?;
        let (name, _) = self.expect_ident()?;

        let mut type_params = Vec::new();
        let effect_params = Vec::new();
        if self.match_token(&TokenKind::Lt) {
            while !self.check(&TokenKind::Gt) && !self.check(&TokenKind::Eof) {
                let (p, _) = self.expect_ident()?;
                type_params.push(p);
                if !self.match_token(&TokenKind::Comma) {
                    break;
                }
            }
            self.expect(TokenKind::Gt)?;
        }

        self.expect(TokenKind::LParen)?;
        let mut params = Vec::new();
        while !self.check(&TokenKind::RParen) && !self.check(&TokenKind::Eof) {
            let mut is_mut = false;
            if self.match_token(&TokenKind::Mut) {
                is_mut = true;
            }
            let (p_name, p_span) = self.expect_ident()?;
            self.expect(TokenKind::Colon)?;
            let ty = self.parse_type_expr()?;
            params.push(Param {
                name: p_name,
                is_mut,
                ty,
                span: p_span,
            });
            if !self.match_token(&TokenKind::Comma) {
                break;
            }
        }
        self.expect(TokenKind::RParen)?;

        let mut return_type = None;
        if self.match_token(&TokenKind::Arrow) {
            return_type = Some(self.parse_type_expr()?);
        }

        let mut yields_effects = Vec::new();
        if self.match_token(&TokenKind::Yields) {
            if self.match_token(&TokenKind::LBracket) {
                while !self.check(&TokenKind::RBracket) && !self.check(&TokenKind::Eof) {
                    let is_spread = self.match_token(&TokenKind::DotDot);
                    let (eff, _) = self.expect_ident()?;
                    if is_spread {
                        yields_effects.push(format!("..{}", eff));
                    } else {
                        yields_effects.push(eff);
                    }
                    if !self.match_token(&TokenKind::Comma) {
                        break;
                    }
                }
                self.expect(TokenKind::RBracket)?;
            } else {
                let is_spread = self.match_token(&TokenKind::DotDot);
                let (eff, _) = self.expect_ident()?;
                if is_spread {
                    yields_effects.push(format!("..{}", eff));
                } else {
                    yields_effects.push(eff);
                }
            }
        }

        let body = self.parse_block()?;
        let span = Span::new(start_tok.span.start, body.span.end, start_tok.span.line, start_tok.span.column);

        Ok(FnDecl {
            name,
            is_pub,
            type_params,
            effect_params,
            params,
            return_type,
            yields_effects,
            body,
            span,
        })
    }

    pub fn parse_type_expr(&mut self) -> Result<TypeExpr, String> {
        let tok = self.peek().clone();
        match tok.kind {
            TokenKind::Fn => {
                let start_tok = self.advance();
                self.expect(TokenKind::LParen)?;
                let mut params = Vec::new();
                while !self.check(&TokenKind::RParen) && !self.check(&TokenKind::Eof) {
                    params.push(self.parse_type_expr()?);
                    if !self.match_token(&TokenKind::Comma) {
                        break;
                    }
                }
                self.expect(TokenKind::RParen)?;
                let mut yields_effects = Vec::new();
                if self.match_token(&TokenKind::Yields) {
                    if self.match_token(&TokenKind::LBracket) {
                        while !self.check(&TokenKind::RBracket) && !self.check(&TokenKind::Eof) {
                            let is_spread = self.match_token(&TokenKind::DotDot);
                            let (eff, _) = self.expect_ident()?;
                            if is_spread {
                                yields_effects.push(format!("..{}", eff));
                            } else {
                                yields_effects.push(eff);
                            }
                            if !self.match_token(&TokenKind::Comma) {
                                break;
                            }
                        }
                        self.expect(TokenKind::RBracket)?;
                    } else {
                        let is_spread = self.match_token(&TokenKind::DotDot);
                        let (eff, _) = self.expect_ident()?;
                        if is_spread {
                            yields_effects.push(format!("..{}", eff));
                        } else {
                            yields_effects.push(eff);
                        }
                    }
                }
                let mut ret_ty = TypeExpr::Unit(start_tok.span);
                if self.match_token(&TokenKind::Arrow) {
                    ret_ty = self.parse_type_expr()?;
                }
                let end_span = ret_ty.span().end;
                Ok(TypeExpr::Fn {
                    params,
                    return_type: Box::new(ret_ty),
                    yields_effects,
                    span: Span::new(start_tok.span.start, end_span, start_tok.span.line, start_tok.span.column),
                })
            }
            TokenKind::Ampersand => {
                let start_tok = self.advance();
                let is_mut = self.match_token(&TokenKind::Mut);
                let inner = self.parse_type_expr()?;
                let span = Span::new(start_tok.span.start, inner.span().end, start_tok.span.line, start_tok.span.column);
                Ok(TypeExpr::Ref {
                    is_mut,
                    inner: Box::new(inner),
                    span,
                })
            }
            TokenKind::Ident(base_name) => {
                self.advance();
                // Check if it is a generic type instantiation: `Box<T>`
                if self.match_token(&TokenKind::Lt) {
                    let mut args = Vec::new();
                    while !self.check(&TokenKind::Gt) && !self.check(&TokenKind::Eof) {
                        args.push(self.parse_type_expr()?);
                        if !self.match_token(&TokenKind::Comma) {
                            break;
                        }
                    }
                    let end_gt = self.expect(TokenKind::Gt)?;
                    return Ok(TypeExpr::Generic {
                        name: base_name,
                        args,
                        span: Span::new(tok.span.start, end_gt.span.end, tok.span.line, tok.span.column),
                    });
                }

                // Check if it is a refinement or relational predicate: `u8(0..=100)` or `usize(>= start && <= arr_len)`
                if self.match_token(&TokenKind::LParen) {
                    // Check if it's a numeric range: `Int` followed by `..=` or `..`
                    if let TokenKind::Int(min_val) = self.peek().kind {
                        if matches!(self.peek_next().kind, TokenKind::DotDot | TokenKind::DotDotEq) {
                            self.advance(); // consume min_val
                            let inclusive = if self.match_token(&TokenKind::DotDotEq) {
                                true
                            } else {
                                self.advance(); // consume DotDot
                                false
                            };
                            let max_val = self.expect_int()?;
                            let rparen = self.expect(TokenKind::RParen)?;
                            return Ok(TypeExpr::Refined {
                                base: base_name,
                                min: min_val,
                                max: max_val,
                                inclusive,
                                span: Span::new(tok.span.start, rparen.span.end, tok.span.line, tok.span.column),
                            });
                        }
                    }

                    // Otherwise, parse as a relational predicate expression
                    let predicate = self.parse_expr()?;
                    let rparen = self.expect(TokenKind::RParen)?;
                    return Ok(TypeExpr::Relational {
                        base: base_name,
                        predicate: Box::new(predicate),
                        span: Span::new(tok.span.start, rparen.span.end, tok.span.line, tok.span.column),
                    });
                }

                Ok(TypeExpr::Named(base_name, tok.span))
            }
            TokenKind::LParen => {
                let start_tok = self.advance();
                let end_tok = self.expect(TokenKind::RParen)?;
                Ok(TypeExpr::Unit(Span::new(start_tok.span.start, end_tok.span.end, start_tok.span.line, start_tok.span.column)))
            }
            _ => Err(format!(
                "Unexpected token {:?} when parsing type at line {}, col {}",
                tok.kind, tok.span.line, tok.span.column
            )),
        }
    }

    fn expect_int(&mut self) -> Result<i64, String> {
        let tok = self.peek().clone();
        if let TokenKind::Int(val) = tok.kind {
            self.advance();
            Ok(val)
        } else {
            Err(format!(
                "Expected integer, found {:?} at line {}, column {}",
                tok.kind, tok.span.line, tok.span.column
            ))
        }
    }

    pub fn parse_block(&mut self) -> Result<Block, String> {
        let start_tok = self.expect(TokenKind::LBrace)?;
        let mut stmts = Vec::new();
        let mut trailing_expr = None;

        while !self.check(&TokenKind::RBrace) && !self.check(&TokenKind::Eof) {
            if self.check(&TokenKind::Let) {
                stmts.push(self.parse_let_stmt()?);
            } else if self.check(&TokenKind::Return) {
                stmts.push(self.parse_return_stmt()?);
            } else {
                let expr = self.parse_expr()?;
                if self.match_token(&TokenKind::Semicolon) {
                    let span = expr.span;
                    stmts.push(Stmt::Expr {
                        expr,
                        has_semicolon: true,
                        span,
                    });
                } else if self.match_token(&TokenKind::Eq) {
                    // Assignment
                    let rhs = self.parse_expr()?;
                    let semi = self.expect(TokenKind::Semicolon)?;
                    let span = Span::new(expr.span.start, semi.span.end, expr.span.line, expr.span.column);
                    stmts.push(Stmt::Assign {
                        target: expr,
                        value: rhs,
                        span,
                    });
                } else if self.check(&TokenKind::RBrace) {
                    trailing_expr = Some(Box::new(expr));
                    break;
                } else {
                    let span = expr.span;
                    stmts.push(Stmt::Expr {
                        expr,
                        has_semicolon: false,
                        span,
                    });
                }
            }
        }

        let end_tok = self.expect(TokenKind::RBrace)?;
        Ok(Block {
            stmts,
            trailing_expr,
            span: Span::new(start_tok.span.start, end_tok.span.end, start_tok.span.line, start_tok.span.column),
        })
    }

    fn parse_let_stmt(&mut self) -> Result<Stmt, String> {
        let start_tok = self.expect(TokenKind::Let)?;
        let is_mut = self.match_token(&TokenKind::Mut);
        let (name, _) = self.expect_ident()?;

        let mut ty = None;
        if self.match_token(&TokenKind::Colon) {
            ty = Some(self.parse_type_expr()?);
        }

        self.expect(TokenKind::Eq)?;
        let init = self.parse_expr()?;
        let end_tok = self.expect(TokenKind::Semicolon)?;

        Ok(Stmt::Let {
            name,
            is_mut,
            ty,
            init,
            span: Span::new(start_tok.span.start, end_tok.span.end, start_tok.span.line, start_tok.span.column),
        })
    }

    fn parse_return_stmt(&mut self) -> Result<Stmt, String> {
        let start_tok = self.expect(TokenKind::Return)?;
        let mut value = None;
        if !self.check(&TokenKind::Semicolon) {
            value = Some(self.parse_expr()?);
        }
        let end_tok = self.expect(TokenKind::Semicolon)?;
        Ok(Stmt::Return {
            value,
            span: Span::new(start_tok.span.start, end_tok.span.end, start_tok.span.line, start_tok.span.column),
        })
    }

    pub fn parse_expr(&mut self) -> Result<Expr, String> {
        self.parse_handle_or_comparison()
    }

    fn parse_handle_or_comparison(&mut self) -> Result<Expr, String> {
        if self.check(&TokenKind::Handle) {
            return self.parse_handle_expr();
        }
        self.parse_logical_or()
    }

    fn parse_handle_expr(&mut self) -> Result<Expr, String> {
        let start_tok = self.expect(TokenKind::Handle)?;
        let body = self.parse_block()?;
        let mut handlers = Vec::new();

        while self.match_token(&TokenKind::With) {
            if self.match_token(&TokenKind::LBrace) {
                // Unified capability matching block: with { Db::query(...) => ..., IOError::raise(...) => ... }
                let start_brace_span = self.peek().span;
                let mut map: HashMap<String, Vec<HandlerArm>> = HashMap::new();
                let mut order: Vec<String> = Vec::new();

                while !self.check(&TokenKind::RBrace) && !self.check(&TokenKind::Eof) {
                    let (eff_or_op, ident_span) = self.expect_ident()?;
                    let (eff_name, op_name) = if self.match_token(&TokenKind::ColonColon) {
                        let (op, _) = self.expect_ident()?;
                        (eff_or_op, op)
                    } else {
                        ("IO".to_string(), eff_or_op)
                    };

                    let mut params = Vec::new();
                    if self.match_token(&TokenKind::LParen) {
                        while !self.check(&TokenKind::RParen) && !self.check(&TokenKind::Eof) {
                            let (p, _) = self.expect_ident()?;
                            params.push(p);
                            if !self.match_token(&TokenKind::Comma) {
                                break;
                            }
                        }
                        self.expect(TokenKind::RParen)?;
                    }
                    self.expect(TokenKind::FatArrow)?;
                    let body_expr = self.parse_expr()?;
                    self.match_token(&TokenKind::Comma);
                    self.match_token(&TokenKind::Semicolon);

                    if !map.contains_key(&eff_name) {
                        order.push(eff_name.clone());
                    }
                    map.entry(eff_name).or_default().push(HandlerArm {
                        op_name,
                        params,
                        body: body_expr,
                        span: ident_span,
                    });
                }
                let end_brace = self.expect(TokenKind::RBrace)?;
                for eff_name in order {
                    let arms = map.remove(&eff_name).unwrap();
                    handlers.push(HandlerClause {
                        effect_name: eff_name,
                        arms,
                        span: Span::new(start_brace_span.start, end_brace.span.end, start_brace_span.line, start_brace_span.column),
                    });
                }
            } else {
                let (eff_name, eff_span) = self.expect_ident()?;
                self.expect(TokenKind::LBrace)?;
                let mut arms = Vec::new();

                while !self.check(&TokenKind::RBrace) && !self.check(&TokenKind::Eof) {
                    let (ident, ident_span) = self.expect_ident()?;
                    let mut params = Vec::new();
                    let op_name;
                    if self.match_token(&TokenKind::LParen) {
                        op_name = ident;
                        while !self.check(&TokenKind::RParen) && !self.check(&TokenKind::Eof) {
                            let (p, _) = self.expect_ident()?;
                            params.push(p);
                            if !self.match_token(&TokenKind::Comma) {
                                break;
                            }
                        }
                        self.expect(TokenKind::RParen)?;
                    } else {
                        op_name = "_catch".to_string();
                        params.push(ident);
                    }
                    self.expect(TokenKind::FatArrow)?;
                    let body_expr = self.parse_expr()?;
                    self.match_token(&TokenKind::Comma);
                    self.match_token(&TokenKind::Semicolon);

                    arms.push(HandlerArm {
                        op_name,
                        params,
                        body: body_expr,
                        span: ident_span,
                    });
                }
                let end_brace = self.expect(TokenKind::RBrace)?;
                handlers.push(HandlerClause {
                    effect_name: eff_name,
                    arms,
                    span: Span::new(eff_span.start, end_brace.span.end, eff_span.line, eff_span.column),
                });
            }
        }

        let end_span = handlers.last().map(|h| h.span.end).unwrap_or(body.span.end);
        Ok(Expr::new(
            ExprKind::Handle { body, handlers },
            Span::new(start_tok.span.start, end_span, start_tok.span.line, start_tok.span.column),
        ))
    }

    fn parse_logical_or(&mut self) -> Result<Expr, String> {
        let mut left = self.parse_logical_and()?;
        while self.match_token(&TokenKind::PipePipe) {
            let right = self.parse_logical_and()?;
            let span = Span::new(left.span.start, right.span.end, left.span.line, left.span.column);
            left = Expr::new(
                ExprKind::Binary {
                    op: BinOp::Or,
                    left: Box::new(left),
                    right: Box::new(right),
                },
                span,
            );
        }
        Ok(left)
    }

    fn parse_logical_and(&mut self) -> Result<Expr, String> {
        let mut left = self.parse_comparison()?;
        while self.match_token(&TokenKind::AmpAmp) {
            let right = self.parse_comparison()?;
            let span = Span::new(left.span.start, right.span.end, left.span.line, left.span.column);
            left = Expr::new(
                ExprKind::Binary {
                    op: BinOp::And,
                    left: Box::new(left),
                    right: Box::new(right),
                },
                span,
            );
        }
        Ok(left)
    }

    fn parse_comparison(&mut self) -> Result<Expr, String> {
        // Support unary relational comparisons like `>= start`
        if let Some(op) = self.match_comparison_op() {
            let right = self.parse_additive()?;
            let span = right.span;
            let implicit_left = Expr::new(ExprKind::Ident("_val".to_string()), span);
            return Ok(Expr::new(
                ExprKind::Binary {
                    op,
                    left: Box::new(implicit_left),
                    right: Box::new(right),
                },
                span,
            ));
        }

        let mut left = self.parse_additive()?;

        while let Some(op) = self.match_comparison_op() {
            let right = self.parse_additive()?;
            let span = Span::new(left.span.start, right.span.end, left.span.line, left.span.column);
            left = Expr::new(
                ExprKind::Binary {
                    op,
                    left: Box::new(left),
                    right: Box::new(right),
                },
                span,
            );
        }

        Ok(left)
    }

    fn match_comparison_op(&mut self) -> Option<BinOp> {
        if self.match_token(&TokenKind::EqEq) {
            Some(BinOp::Eq)
        } else if self.match_token(&TokenKind::NotEq) {
            Some(BinOp::NotEq)
        } else if self.match_token(&TokenKind::Lt) {
            Some(BinOp::Lt)
        } else if self.match_token(&TokenKind::LtEq) {
            Some(BinOp::LtEq)
        } else if self.match_token(&TokenKind::Gt) {
            Some(BinOp::Gt)
        } else if self.match_token(&TokenKind::GtEq) {
            Some(BinOp::GtEq)
        } else {
            None
        }
    }

    fn parse_additive(&mut self) -> Result<Expr, String> {
        let mut left = self.parse_multiplicative()?;

        while self.check(&TokenKind::Plus) || self.check(&TokenKind::Minus) {
            let tok = self.advance();
            let op = match tok.kind {
                TokenKind::Plus => BinOp::Add,
                TokenKind::Minus => BinOp::Sub,
                _ => unreachable!(),
            };
            let right = self.parse_multiplicative()?;
            let span = Span::new(left.span.start, right.span.end, left.span.line, left.span.column);
            left = Expr::new(
                ExprKind::Binary {
                    op,
                    left: Box::new(left),
                    right: Box::new(right),
                },
                span,
            );
        }

        Ok(left)
    }

    fn parse_multiplicative(&mut self) -> Result<Expr, String> {
        let mut left = self.parse_cast()?;

        while self.check(&TokenKind::Star) || self.check(&TokenKind::Slash) {
            let tok = self.advance();
            let op = match tok.kind {
                TokenKind::Star => BinOp::Mul,
                TokenKind::Slash => BinOp::Div,
                _ => unreachable!(),
            };
            let right = self.parse_cast()?;
            let span = Span::new(left.span.start, right.span.end, left.span.line, left.span.column);
            left = Expr::new(
                ExprKind::Binary {
                    op,
                    left: Box::new(left),
                    right: Box::new(right),
                },
                span,
            );
        }

        Ok(left)
    }

    fn parse_cast(&mut self) -> Result<Expr, String> {
        let mut expr = self.parse_unary()?;

        while self.match_token(&TokenKind::As) {
            let target_ty = self.parse_type_expr()?;
            let span = Span::new(expr.span.start, target_ty.span().end, expr.span.line, expr.span.column);
            expr = Expr::new(
                ExprKind::Cast {
                    expr: Box::new(expr),
                    target_ty,
                },
                span,
            );
        }

        Ok(expr)
    }

    fn parse_unary(&mut self) -> Result<Expr, String> {
        if self.check(&TokenKind::Ampersand) {
            let tok = self.advance();
            let is_mut = self.match_token(&TokenKind::Mut);
            let inner = self.parse_unary()?;
            let span = Span::new(tok.span.start, inner.span.end, tok.span.line, tok.span.column);
            return Ok(Expr::new(
                ExprKind::Ref {
                    is_mut,
                    expr: Box::new(inner),
                },
                span,
            ));
        }

        self.parse_postfix()
    }

    fn parse_postfix(&mut self) -> Result<Expr, String> {
        let mut expr = self.parse_primary()?;

        loop {
            if self.match_token(&TokenKind::Dot) {
                // Could be field access or method call or .await
                let (member, m_span) = self.expect_ident()?;
                if member == "await" {
                    let span = Span::new(expr.span.start, m_span.end, expr.span.line, expr.span.column);
                    expr = Expr::new(ExprKind::Await(Box::new(expr)), span);
                } else if self.match_token(&TokenKind::LParen) {
                    // Method call
                    let args = self.parse_call_args()?;
                    let span = Span::new(expr.span.start, self.peek().span.start, expr.span.line, expr.span.column);
                    expr = Expr::new(
                        ExprKind::MethodCall {
                            target: Box::new(expr),
                            method: member,
                            args,
                        },
                        span,
                    );
                } else {
                    // Field access
                    let span = Span::new(expr.span.start, m_span.end, expr.span.line, expr.span.column);
                    expr = Expr::new(
                        ExprKind::FieldAccess {
                            target: Box::new(expr),
                            field: member,
                        },
                        span,
                    );
                }
            } else if self.match_token(&TokenKind::Exclamation) {
                // Postfix effect invocation `!`
                let span = Span::new(expr.span.start, self.peek().span.start, expr.span.line, expr.span.column);
                expr = Expr::new(ExprKind::EffectCall(Box::new(expr)), span);
            } else if self.match_token(&TokenKind::Question) {
                // Postfix try `?`
                let span = Span::new(expr.span.start, self.peek().span.start, expr.span.line, expr.span.column);
                expr = Expr::new(ExprKind::Try(Box::new(expr)), span);
            } else if self.match_token(&TokenKind::LParen) {
                // Function call `callee(args)`
                let args = self.parse_call_args()?;
                let span = Span::new(expr.span.start, self.peek().span.start, expr.span.line, expr.span.column);
                expr = Expr::new(
                    ExprKind::Call {
                        callee: Box::new(expr),
                        args,
                    },
                    span,
                );
            } else {
                break;
            }
        }

        Ok(expr)
    }

    fn parse_call_args(&mut self) -> Result<Vec<Expr>, String> {
        let mut args = Vec::new();
        while !self.check(&TokenKind::RParen) && !self.check(&TokenKind::Eof) {
            args.push(self.parse_expr()?);
            if !self.match_token(&TokenKind::Comma) {
                break;
            }
        }
        self.expect(TokenKind::RParen)?;
        Ok(args)
    }

    fn parse_primary(&mut self) -> Result<Expr, String> {
        let tok = self.peek().clone();
        match tok.kind {
            TokenKind::Int(val) => {
                self.advance();
                Ok(Expr::new(ExprKind::Int(val), tok.span))
            }
            TokenKind::Str(s) => {
                self.advance();
                Ok(Expr::new(ExprKind::Str(s), tok.span))
            }
            TokenKind::True => {
                self.advance();
                Ok(Expr::new(ExprKind::Bool(true), tok.span))
            }
            TokenKind::False => {
                self.advance();
                Ok(Expr::new(ExprKind::Bool(false), tok.span))
            }
            TokenKind::Resume => {
                let start_span = self.advance().span;
                if self.match_token(&TokenKind::LParen) {
                    if self.match_token(&TokenKind::RParen) {
                        let dummy = Expr::new(ExprKind::Int(0), start_span);
                        Ok(Expr::new(ExprKind::Resume(Box::new(dummy)), start_span))
                    } else {
                        let val = self.parse_expr()?;
                        let end_tok = self.expect(TokenKind::RParen)?;
                        let span = Span::new(start_span.start, end_tok.span.end, start_span.line, start_span.column);
                        Ok(Expr::new(ExprKind::Resume(Box::new(val)), span))
                    }
                } else {
                    let dummy = Expr::new(ExprKind::Int(0), start_span);
                    Ok(Expr::new(ExprKind::Resume(Box::new(dummy)), start_span))
                }
            }
            TokenKind::Ident(ref name) => {
                let name = name.clone();
                let ident_span = tok.span;
                self.advance();

                // Check for macro call like println!(...)
                if self.match_token(&TokenKind::Exclamation) && self.match_token(&TokenKind::LParen) {
                    let args = self.parse_call_args()?;
                    let span = Span::new(ident_span.start, self.peek().span.start, ident_span.line, ident_span.column);
                    return Ok(Expr::new(
                        ExprKind::MacroCall { name, args },
                        span,
                    ));
                }

                // Check for PathCall: Ident::method(...)
                if self.match_token(&TokenKind::ColonColon) {
                    let mut path = vec![name];
                    let (next_seg, _) = self.expect_ident()?;
                    path.push(next_seg);
                    while self.match_token(&TokenKind::ColonColon) {
                        let (seg, _) = self.expect_ident()?;
                        path.push(seg);
                    }
                    if self.match_token(&TokenKind::LParen) {
                        let args = self.parse_call_args()?;
                        let span = Span::new(ident_span.start, self.peek().span.start, ident_span.line, ident_span.column);
                        return Ok(Expr::new(
                            ExprKind::PathCall { path, args },
                            span,
                        ));
                    } else {
                        return Err(format!("Expected '(' after path at line {}", ident_span.line));
                    }
                }

                // Check for StructInit: Ident { field: val, ... }
                // We check if the next token is '{' and what follows looks like field:
                if self.check(&TokenKind::LBrace) && self.looks_like_struct_init() {
                    self.advance(); // consume '{'
                    let mut fields = Vec::new();
                    while !self.check(&TokenKind::RBrace) && !self.check(&TokenKind::Eof) {
                        let (f_name, _) = self.expect_ident()?;
                        self.expect(TokenKind::Colon)?;
                        let f_val = self.parse_expr()?;
                        fields.push((f_name, f_val));
                        if !self.match_token(&TokenKind::Comma) {
                            break;
                        }
                    }
                    let end_brace = self.expect(TokenKind::RBrace)?;
                    let span = Span::new(ident_span.start, end_brace.span.end, ident_span.line, ident_span.column);
                    return Ok(Expr::new(
                        ExprKind::StructInit { name, fields },
                        span,
                    ));
                }

                Ok(Expr::new(ExprKind::Ident(name), ident_span))
            }
            TokenKind::LParen => {
                let start_span = self.advance().span;
                if self.match_token(&TokenKind::RParen) {
                    Ok(Expr::new(ExprKind::Int(0), start_span))
                } else {
                    let inner = self.parse_expr()?;
                    self.expect(TokenKind::RParen)?;
                    Ok(inner)
                }
            }
            TokenKind::LBrace => {
                let block = self.parse_block()?;
                let span = block.span;
                Ok(Expr::new(ExprKind::Block(block), span))
            }
            TokenKind::If => {
                let start_tok = self.advance();
                let cond = self.parse_expr()?;
                let then_branch = self.parse_block()?;
                let mut else_branch = None;
                if self.match_token(&TokenKind::Else) {
                    if self.check(&TokenKind::If) {
                        let if_expr = self.parse_primary()?;
                        // Wrap in synthetic block
                        else_branch = Some(Block {
                            stmts: vec![Stmt::Expr {
                                expr: if_expr.clone(),
                                has_semicolon: false,
                                span: if_expr.span,
                            }],
                            trailing_expr: Some(Box::new(if_expr.clone())),
                            span: if_expr.span,
                        });
                    } else {
                        else_branch = Some(self.parse_block()?);
                    }
                }
                let span = Span::new(
                    start_tok.span.start,
                    else_branch.as_ref().map(|b| b.span.end).unwrap_or(then_branch.span.end),
                    start_tok.span.line,
                    start_tok.span.column,
                );
                Ok(Expr::new(
                    ExprKind::If {
                        cond: Box::new(cond),
                        then_branch,
                        else_branch,
                    },
                    span,
                ))
            }
            TokenKind::Region => {
                let start_tok = self.advance();
                let mut name = None;
                if let TokenKind::Ident(ref n) = self.peek().kind {
                    if self.peek_next().kind == TokenKind::LBrace {
                        name = Some(n.clone());
                        self.advance();
                    }
                }
                let body = self.parse_block()?;
                let span = Span::new(
                    start_tok.span.start,
                    body.span.end,
                    start_tok.span.line,
                    start_tok.span.column,
                );
                Ok(Expr::new(
                    ExprKind::Region { name, body },
                    span,
                ))
            }

            _ => Err(format!(
                "Unexpected token {:?} in expression at line {}, col {}",
                tok.kind, tok.span.line, tok.span.column
            )),
        }
    }

    fn looks_like_struct_init(&self) -> bool {
        // We are looking ahead from '{'
        // If the pattern is '{' Ident ':' ... then it's a struct initialization
        if self.cursor + 2 < self.tokens.len() {
            let next1 = &self.tokens[self.cursor + 1];
            let next2 = &self.tokens[self.cursor + 2];
            matches!(next1.kind, TokenKind::Ident(_)) && matches!(next2.kind, TokenKind::Colon)
        } else {
            false
        }
    }
}
