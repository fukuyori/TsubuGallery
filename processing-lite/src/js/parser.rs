//! p5.js subset の再帰下降パーサ。
//!
//! セミコロンは省略できる (ASI)。改行があれば文が切れたものとして扱う。
//! つぶやき作品は改行だけで区切って書かれることが多いので、ここが要る。

use crate::lexer::CompileError;

use super::ast::*;
use super::lexer::{Kw, TemplatePart, Tok, Token, tokenize};

pub fn parse(source: &str) -> Result<Script, CompileError> {
    Parser { tokens: tokenize(source)?, pos: 0 }.script()
}

/// 式ひとつだけを読む。テンプレートリテラルの `${...}` に使う。
fn parse_expression(source: &str) -> Result<Expr, CompileError> {
    let mut parser = Parser { tokens: tokenize(source)?, pos: 0 };
    let expr = parser.expression()?;
    if !parser.check(&Tok::Eof) {
        return Err(parser.error("${ } の中に余分なものがあります"));
    }
    Ok(expr)
}

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

impl Parser {
    // ---- トークン操作 ---------------------------------------------------

    fn peek(&self) -> &Tok {
        &self.tokens[self.pos.min(self.tokens.len() - 1)].tok
    }

    fn peek_at(&self, offset: usize) -> &Tok {
        &self.tokens[(self.pos + offset).min(self.tokens.len() - 1)].tok
    }

    fn newline_before(&self) -> bool {
        self.tokens[self.pos.min(self.tokens.len() - 1)].newline_before
    }

    fn position(&self) -> (u32, u32) {
        let t = &self.tokens[self.pos.min(self.tokens.len() - 1)];
        (t.line, t.column)
    }

    fn advance(&mut self) -> Tok {
        let tok = self.tokens[self.pos.min(self.tokens.len() - 1)].tok.clone();
        if self.pos < self.tokens.len() - 1 {
            self.pos += 1;
        }
        tok
    }

    fn check(&self, tok: &Tok) -> bool {
        self.peek() == tok
    }

    fn eat(&mut self, tok: &Tok) -> bool {
        if self.check(tok) {
            self.advance();
            true
        } else {
            false
        }
    }

    fn expect(&mut self, tok: &Tok, what: &str) -> Result<(), CompileError> {
        if self.eat(tok) {
            Ok(())
        } else {
            Err(self.error(format!("{what} がありません")))
        }
    }

    fn error(&self, message: impl Into<String>) -> CompileError {
        let (line, column) = self.position();
        CompileError::new(line, column, message)
    }

    fn ident(&mut self) -> Result<String, CompileError> {
        match self.peek().clone() {
            Tok::Ident(name) => {
                self.advance();
                Ok(name)
            }
            _ => Err(self.error("名前がありません")),
        }
    }

    /// 文の終わり。`;` があれば食べる。無くても、改行か `}` か終端なら通す。
    fn end_of_statement(&mut self) -> Result<(), CompileError> {
        if self.eat(&Tok::Semicolon) {
            return Ok(());
        }
        if self.newline_before() || self.check(&Tok::RBrace) || self.check(&Tok::Eof) {
            return Ok(());
        }
        Err(self.error("文の区切りがありません"))
    }

    // ---- 文 -------------------------------------------------------------

    fn script(mut self) -> Result<Script, CompileError> {
        let mut statements = Vec::new();
        while !self.check(&Tok::Eof) {
            statements.push(self.statement()?);
        }
        Ok(Script { statements })
    }

    fn statement(&mut self) -> Result<Stmt, CompileError> {
        match self.peek().clone() {
            Tok::LBrace => self.block(),
            Tok::Semicolon => {
                self.advance();
                Ok(Stmt::Block(Vec::new()))
            }
            Tok::Keyword(Kw::Let | Kw::Const | Kw::Var) => {
                self.advance();
                let declaration = self.declaration()?;
                self.end_of_statement()?;
                Ok(declaration)
            }
            Tok::Keyword(Kw::Function) => self.function(),
            Tok::Keyword(Kw::If) => self.if_statement(),
            Tok::Keyword(Kw::For) => self.for_statement(),
            Tok::Keyword(Kw::While) => self.while_statement(),
            Tok::Keyword(Kw::Break) => {
                let (line, column) = self.position();
                self.advance();
                self.end_of_statement()?;
                Ok(Stmt::Break { line, column })
            }

            Tok::Keyword(Kw::Continue) => {
                let (line, column) = self.position();
                self.advance();
                self.end_of_statement()?;
                Ok(Stmt::Continue { line, column })
            }

            Tok::Keyword(Kw::Return) => {
                self.advance();
                let value = if self.check(&Tok::Semicolon)
                    || self.check(&Tok::RBrace)
                    || self.check(&Tok::Eof)
                    || self.newline_before()
                {
                    None
                } else {
                    Some(self.expression()?)
                };
                self.end_of_statement()?;
                Ok(Stmt::Return(value))
            }
            _ => {
                let expr = self.expression()?;
                self.end_of_statement()?;
                Ok(Stmt::Expr(expr))
            }
        }
    }

    fn declaration(&mut self) -> Result<Stmt, CompileError> {
        let mut names = Vec::new();
        loop {
            let name = self.ident()?;
            let init = if self.eat(&Tok::Assign) { Some(self.assignment()?) } else { None };
            names.push((name, init));
            if !self.eat(&Tok::Comma) {
                break;
            }
        }
        Ok(Stmt::Declare(names))
    }

    fn block(&mut self) -> Result<Stmt, CompileError> {
        self.expect(&Tok::LBrace, "`{`")?;
        let mut statements = Vec::new();
        while !self.check(&Tok::RBrace) {
            if self.check(&Tok::Eof) {
                return Err(self.error("`}` がありません"));
            }
            statements.push(self.statement()?);
        }
        self.expect(&Tok::RBrace, "`}`")?;
        Ok(Stmt::Block(statements))
    }

    fn function(&mut self) -> Result<Stmt, CompileError> {
        let (line, column) = self.position();
        self.advance();
        let name = self.ident()?;
        let params = self.params()?;
        let Stmt::Block(body) = self.block()? else { unreachable!("block() は Block を返す") };
        Ok(Stmt::Function { name, params, body, line, column })
    }

    fn params(&mut self) -> Result<Vec<Param>, CompileError> {
        self.expect(&Tok::LParen, "`(`")?;
        let mut params = Vec::new();
        if !self.check(&Tok::RParen) {
            loop {
                let name = self.ident()?;
                // `(a, b = expr)` の既定値。渡されなかったときに評価する。
                let default =
                    if self.eat(&Tok::Assign) { Some(self.assignment()?) } else { None };
                params.push(Param { name, default });
                if !self.eat(&Tok::Comma) {
                    break;
                }
            }
        }
        self.expect(&Tok::RParen, "`)`")?;
        Ok(params)
    }

    fn if_statement(&mut self) -> Result<Stmt, CompileError> {
        self.advance();
        self.expect(&Tok::LParen, "`(`")?;
        let cond = self.expression()?;
        self.expect(&Tok::RParen, "`)`")?;
        let then = Box::new(self.statement()?);
        let otherwise = if self.eat(&Tok::Keyword(Kw::Else)) {
            Some(Box::new(self.statement()?))
        } else {
            None
        };
        Ok(Stmt::If { cond, then, otherwise })
    }

    fn while_statement(&mut self) -> Result<Stmt, CompileError> {
        self.advance();
        self.expect(&Tok::LParen, "`(`")?;
        let cond = self.expression()?;
        self.expect(&Tok::RParen, "`)`")?;
        let body = Box::new(self.statement()?);
        Ok(Stmt::While { cond, body })
    }

    fn for_statement(&mut self) -> Result<Stmt, CompileError> {
        self.advance();
        self.expect(&Tok::LParen, "`(`")?;

        // `for (v of xs)` / `for (const v of xs)`。`of` は予約語ではないので
        // 識別子として現れる。並びを見て判断する。
        let declared = matches!(self.peek(), Tok::Keyword(Kw::Let | Kw::Const | Kw::Var));
        let name_at = if declared { 1 } else { 0 };
        if matches!(self.peek_at(name_at), Tok::Ident(_))
            && matches!(self.peek_at(name_at + 1), Tok::Ident(w) if w == "of")
        {
            if declared {
                self.advance();
            }
            let name = self.ident()?;
            self.advance(); // of
            let iterable = self.expression()?;
            self.expect(&Tok::RParen, "`)`")?;
            let body = Box::new(self.statement()?);
            return Ok(Stmt::ForOf { name, declared, iterable, body });
        }

        let init = if self.eat(&Tok::Semicolon) {
            None
        } else {
            let stmt = match self.peek() {
                Tok::Keyword(Kw::Let | Kw::Const | Kw::Var) => {
                    self.advance();
                    self.declaration()?
                }
                _ => Stmt::Expr(self.expression()?),
            };
            self.expect(&Tok::Semicolon, "`;`")?;
            Some(Box::new(stmt))
        };

        let cond = if self.check(&Tok::Semicolon) { None } else { Some(self.expression()?) };
        self.expect(&Tok::Semicolon, "`;`")?;

        let update = if self.check(&Tok::RParen) { None } else { Some(self.expression()?) };
        self.expect(&Tok::RParen, "`)`")?;

        let body = Box::new(self.statement()?);
        Ok(Stmt::For { init, cond, update, body })
    }

    // ---- 式 -------------------------------------------------------------

    /// もっとも優先度の低い式。カンマ演算子を含む。
    ///
    /// 引数や配列の要素はカンマで区切られるので、そちらは
    /// [`Parser::assignment`] から読むこと。
    fn expression(&mut self) -> Result<Expr, CompileError> {
        let mut expr = self.assignment()?;
        while self.eat(&Tok::Comma) {
            let next = self.assignment()?;
            expr = Expr::Sequence(Box::new(expr), Box::new(next));
        }
        Ok(expr)
    }

    fn assignment(&mut self) -> Result<Expr, CompileError> {
        let left = self.ternary()?;

        let op = match self.peek() {
            Tok::Assign => AssignOp::Set,
            Tok::PlusAssign => AssignOp::Add,
            Tok::MinusAssign => AssignOp::Sub,
            Tok::StarAssign => AssignOp::Mul,
            Tok::StarStarAssign => AssignOp::Pow,
            Tok::SlashAssign => AssignOp::Div,
            Tok::PercentAssign => AssignOp::Rem,
            Tok::AmpAssign => AssignOp::BitAnd,
            Tok::PipeAssign => AssignOp::BitOr,
            Tok::CaretAssign => AssignOp::BitXor,
            Tok::ShlAssign => AssignOp::Shl,
            Tok::ShrAssign => AssignOp::Shr,
            _ => return Ok(left),
        };

        let Some(target) = as_target(&left) else {
            return Err(self.error("ここへは代入できません"));
        };
        if op != AssignOp::Set && matches!(target, Target::Destructure(_)) {
            return Err(self.error("分割代入で使えるのは = だけです"));
        }
        self.advance();
        let value = Box::new(self.assignment()?);
        Ok(Expr::Assign { target, op, value })
    }

    fn ternary(&mut self) -> Result<Expr, CompileError> {
        let cond = self.logical_or()?;
        if !self.eat(&Tok::Question) {
            return Ok(cond);
        }
        let then = Box::new(self.assignment()?);
        self.expect(&Tok::Colon, "`:`")?;
        let other = Box::new(self.assignment()?);
        Ok(Expr::Ternary { cond: Box::new(cond), then, other })
    }

    fn logical_or(&mut self) -> Result<Expr, CompileError> {
        let mut lhs = self.logical_and()?;
        while self.eat(&Tok::OrOr) {
            let rhs = self.logical_and()?;
            lhs = Expr::Logical { op: LogicalOp::Or, lhs: Box::new(lhs), rhs: Box::new(rhs) };
        }
        Ok(lhs)
    }

    fn logical_and(&mut self) -> Result<Expr, CompileError> {
        let mut lhs = self.bit_or()?;
        while self.eat(&Tok::AndAnd) {
            let rhs = self.bit_or()?;
            lhs = Expr::Logical { op: LogicalOp::And, lhs: Box::new(lhs), rhs: Box::new(rhs) };
        }
        Ok(lhs)
    }

    // ビット演算の強さは JavaScript に合わせる。`|` < `^` < `&` < 等値 の順。

    fn bit_or(&mut self) -> Result<Expr, CompileError> {
        let mut lhs = self.bit_xor()?;
        while self.eat(&Tok::Pipe) {
            let rhs = self.bit_xor()?;
            lhs = Expr::Binary { op: BinaryOp::BitOr, lhs: Box::new(lhs), rhs: Box::new(rhs) };
        }
        Ok(lhs)
    }

    fn bit_xor(&mut self) -> Result<Expr, CompileError> {
        let mut lhs = self.bit_and()?;
        while self.eat(&Tok::Caret) {
            let rhs = self.bit_and()?;
            lhs = Expr::Binary { op: BinaryOp::BitXor, lhs: Box::new(lhs), rhs: Box::new(rhs) };
        }
        Ok(lhs)
    }

    fn bit_and(&mut self) -> Result<Expr, CompileError> {
        let mut lhs = self.equality()?;
        while self.eat(&Tok::Amp) {
            let rhs = self.equality()?;
            lhs = Expr::Binary { op: BinaryOp::BitAnd, lhs: Box::new(lhs), rhs: Box::new(rhs) };
        }
        Ok(lhs)
    }

    fn equality(&mut self) -> Result<Expr, CompileError> {
        let mut lhs = self.comparison()?;
        loop {
            let op = match self.peek() {
                Tok::Eq => BinaryOp::Eq,
                Tok::Ne => BinaryOp::Ne,
                _ => return Ok(lhs),
            };
            self.advance();
            let rhs = self.comparison()?;
            lhs = Expr::Binary { op, lhs: Box::new(lhs), rhs: Box::new(rhs) };
        }
    }

    fn comparison(&mut self) -> Result<Expr, CompileError> {
        let mut lhs = self.shift()?;
        loop {
            let op = match self.peek() {
                Tok::Lt => BinaryOp::Lt,
                Tok::Le => BinaryOp::Le,
                Tok::Gt => BinaryOp::Gt,
                Tok::Ge => BinaryOp::Ge,
                _ => return Ok(lhs),
            };
            self.advance();
            let rhs = self.shift()?;
            lhs = Expr::Binary { op, lhs: Box::new(lhs), rhs: Box::new(rhs) };
        }
    }

    fn shift(&mut self) -> Result<Expr, CompileError> {
        let mut lhs = self.additive()?;
        loop {
            let op = match self.peek() {
                Tok::Shl => BinaryOp::Shl,
                Tok::Shr => BinaryOp::Shr,
                Tok::UShr => BinaryOp::UShr,
                _ => return Ok(lhs),
            };
            self.advance();
            let rhs = self.additive()?;
            lhs = Expr::Binary { op, lhs: Box::new(lhs), rhs: Box::new(rhs) };
        }
    }

    fn additive(&mut self) -> Result<Expr, CompileError> {
        let mut lhs = self.multiplicative()?;
        loop {
            let op = match self.peek() {
                Tok::Plus => BinaryOp::Add,
                Tok::Minus => BinaryOp::Sub,
                _ => return Ok(lhs),
            };
            self.advance();
            let rhs = self.multiplicative()?;
            lhs = Expr::Binary { op, lhs: Box::new(lhs), rhs: Box::new(rhs) };
        }
    }

    fn multiplicative(&mut self) -> Result<Expr, CompileError> {
        let mut lhs = self.exponent()?;
        loop {
            let op = match self.peek() {
                Tok::Star => BinaryOp::Mul,
                Tok::Slash => BinaryOp::Div,
                Tok::Percent => BinaryOp::Rem,
                _ => return Ok(lhs),
            };
            self.advance();
            let rhs = self.exponent()?;
            lhs = Expr::Binary { op, lhs: Box::new(lhs), rhs: Box::new(rhs) };
        }
    }

    /// `a ** b`。右結合なので、右辺はもう一度この段から読む。
    ///
    /// 右辺だけは単項演算を許す (`2 ** -1`)。左辺の `-2 ** 2` は JavaScript では
    /// 構文エラーだが、ここでは書かれたとおり `(-2) ** 2` として読む。
    fn exponent(&mut self) -> Result<Expr, CompileError> {
        let lhs = self.unary()?;
        if !self.eat(&Tok::StarStar) {
            return Ok(lhs);
        }
        let rhs = self.exponent()?;
        Ok(Expr::Binary { op: BinaryOp::Pow, lhs: Box::new(lhs), rhs: Box::new(rhs) })
    }

    fn unary(&mut self) -> Result<Expr, CompileError> {
        match self.peek() {
            Tok::Minus => {
                self.advance();
                Ok(Expr::Unary { op: UnaryOp::Neg, operand: Box::new(self.unary()?) })
            }
            Tok::Bang => {
                self.advance();
                Ok(Expr::Unary { op: UnaryOp::Not, operand: Box::new(self.unary()?) })
            }
            Tok::Tilde => {
                self.advance();
                Ok(Expr::Unary { op: UnaryOp::BitNot, operand: Box::new(self.unary()?) })
            }
            Tok::Plus => {
                self.advance();
                self.unary()
            }
            Tok::Increment | Tok::Decrement => {
                let delta = if self.check(&Tok::Increment) { 1.0 } else { -1.0 };
                self.advance();
                let operand = self.unary()?;
                let Some(target) = as_target(&operand) else {
                    return Err(self.error("ここは増減できません"));
                };
                Ok(Expr::Update { target, delta, prefix: true })
            }
            _ => self.postfix(),
        }
    }

    fn postfix(&mut self) -> Result<Expr, CompileError> {
        let mut expr = self.primary()?;

        loop {
            let (line, column) = self.position();
            match self.peek() {
                Tok::Dot => {
                    self.advance();
                    let name = self.ident()?;
                    expr = Expr::Member { object: Box::new(expr), name };
                }
                Tok::LBracket => {
                    self.advance();
                    let index = self.expression()?;
                    self.expect(&Tok::RBracket, "`]`")?;
                    expr = Expr::Index { object: Box::new(expr), index: Box::new(index) };
                }
                Tok::LParen => {
                    self.advance();
                    let mut args = Vec::new();
                    if !self.check(&Tok::RParen) {
                        loop {
                            // `f(...xs)`。個数は実行時にしか分からない。
                            if self.eat(&Tok::Spread) {
                                args.push(Expr::Spread(Box::new(self.assignment()?)));
                            } else {
                                args.push(self.assignment()?);
                            }
                            if !self.eat(&Tok::Comma) {
                                break;
                            }
                        }
                    }
                    self.expect(&Tok::RParen, "`)`")?;
                    expr = Expr::Call { callee: Box::new(expr), args, line, column };
                }
                Tok::Increment | Tok::Decrement => {
                    // 改行をまたぐ `++` は次の文の前置。ASI の決まり。
                    if self.newline_before() {
                        return Ok(expr);
                    }
                    let delta = if self.check(&Tok::Increment) { 1.0 } else { -1.0 };
                    self.advance();
                    let Some(target) = as_target(&expr) else {
                        return Err(self.error("ここは増減できません"));
                    };
                    expr = Expr::Update { target, delta, prefix: false };
                }
                _ => return Ok(expr),
            }
        }
    }

    fn primary(&mut self) -> Result<Expr, CompileError> {
        // アロー関数は括弧の中身を読む前に見分ける必要がある。
        if let Some(expr) = self.try_arrow()? {
            return Ok(expr);
        }

        match self.peek().clone() {
            Tok::Number(v) => {
                self.advance();
                Ok(Expr::Number(v))
            }
            Tok::Str(text) => {
                self.advance();
                Ok(Expr::Str(text))
            }

            // `` `a${b}c` `` は連結へ均す。`+` は片方が文字列なら連結になるので、
            // 先頭を必ず文字列にしておけば数値が混ざっても文字列のままになる。
            Tok::Template(parts) => {
                self.advance();
                let mut expr: Option<Expr> = None;
                for part in &parts {
                    let piece = match part {
                        TemplatePart::Text(text) => Expr::Str(text.clone()),
                        TemplatePart::Expr(code) => parse_expression(code)?,
                    };
                    expr = Some(match expr {
                        None => match part {
                            // 先頭が式なら、空文字列と足して文字列に寄せる。
                            TemplatePart::Expr(_) => Expr::Binary {
                                op: BinaryOp::Add,
                                lhs: Box::new(Expr::Str(String::new())),
                                rhs: Box::new(piece),
                            },
                            TemplatePart::Text(_) => piece,
                        },
                        Some(left) => Expr::Binary {
                            op: BinaryOp::Add,
                            lhs: Box::new(left),
                            rhs: Box::new(piece),
                        },
                    });
                }
                Ok(expr.unwrap_or_else(|| Expr::Str(String::new())))
            }
            Tok::Keyword(Kw::True) => {
                self.advance();
                Ok(Expr::Bool(true))
            }
            Tok::Keyword(Kw::False) => {
                self.advance();
                Ok(Expr::Bool(false))
            }
            Tok::Keyword(Kw::Undefined | Kw::Null) => {
                self.advance();
                Ok(Expr::Undefined)
            }
            Tok::Ident(name) => {
                self.advance();
                Ok(Expr::Ident(name))
            }
            Tok::LParen => {
                self.advance();
                let inner = self.expression()?;
                self.expect(&Tok::RParen, "`)`")?;
                Ok(inner)
            }
            Tok::LBracket => {
                self.advance();
                let mut items = Vec::new();
                if !self.check(&Tok::RBracket) {
                    loop {
                        if self.eat(&Tok::Spread) {
                            items.push(ArrayElem::Spread(self.assignment()?));
                        } else {
                            items.push(ArrayElem::Item(self.assignment()?));
                        }
                        if !self.eat(&Tok::Comma) {
                            break;
                        }
                        // `[1, 2, ]` のような末尾のカンマを許す。
                        if self.check(&Tok::RBracket) {
                            break;
                        }
                    }
                }
                self.expect(&Tok::RBracket, "`]`")?;
                Ok(Expr::Array(items))
            }
            Tok::LBrace => self.object_literal(),
            _ => Err(self.error("式がありません")),
        }
    }

    fn object_literal(&mut self) -> Result<Expr, CompileError> {
        self.expect(&Tok::LBrace, "`{`")?;
        let mut fields = Vec::new();

        if !self.check(&Tok::RBrace) {
            loop {
                let key = match self.peek().clone() {
                    Tok::Ident(name) => {
                        self.advance();
                        name
                    }
                    Tok::Number(v) => {
                        self.advance();
                        v.to_string()
                    }
                    _ => return Err(self.error("プロパティ名がありません")),
                };
                // `{x}` は `{x: x}` の略記。
                let value = if self.eat(&Tok::Colon) {
                    self.assignment()?
                } else {
                    Expr::Ident(key.clone())
                };
                fields.push((key, value));

                if !self.eat(&Tok::Comma) {
                    break;
                }
                if self.check(&Tok::RBrace) {
                    break;
                }
            }
        }

        self.expect(&Tok::RBrace, "`}`")?;
        Ok(Expr::Object(fields))
    }

    /// アロー関数なら読む。違えば何も消費しない。
    fn try_arrow(&mut self) -> Result<Option<Expr>, CompileError> {
        // `x => ...`
        if matches!(self.peek(), Tok::Ident(_)) && *self.peek_at(1) == Tok::Arrow {
            let name = self.ident()?;
            self.advance();
            let body = self.arrow_body()?;
            return Ok(Some(Expr::Arrow {
                params: vec![Param { name, default: None }],
                body: Box::new(body),
            }));
        }

        // `(a, b) => ...`。括弧の対応を先読みして確かめる。
        if *self.peek() == Tok::LParen
            && let Some(after) = self.matching_paren()
            && self.tokens.get(after + 1).map(|t| &t.tok) == Some(&Tok::Arrow)
        {
            let params = self.params()?;
            self.expect(&Tok::Arrow, "`=>`")?;
            let body = self.arrow_body()?;
            return Ok(Some(Expr::Arrow { params, body: Box::new(body) }));
        }

        Ok(None)
    }

    fn arrow_body(&mut self) -> Result<ArrowBody, CompileError> {
        if self.check(&Tok::LBrace) {
            let Stmt::Block(body) = self.block()? else { unreachable!("block() は Block を返す") };
            Ok(ArrowBody::Block(body))
        } else {
            Ok(ArrowBody::Expr(self.assignment()?))
        }
    }

    /// いまの `(` に対応する `)` の位置。
    fn matching_paren(&self) -> Option<usize> {
        let mut depth = 0usize;
        for index in self.pos..self.tokens.len() {
            match self.tokens[index].tok {
                Tok::LParen => depth += 1,
                Tok::RParen => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(index);
                    }
                }
                Tok::Eof => return None,
                _ => {}
            }
        }
        None
    }
}

/// 式が代入先になれるなら、その形を返す。
fn as_target(expr: &Expr) -> Option<Target> {
    match expr {
        Expr::Ident(name) => Some(Target::Var(name.clone())),
        Expr::Member { object, name } => Some(Target::Member(object.clone(), name.clone())),
        Expr::Index { object, index } => Some(Target::Index(object.clone(), index.clone())),
        // `[a, b] = [1, 2]`。左辺は配列リテラルとして読まれてくる。
        Expr::Array(items) => {
            let mut targets = Vec::with_capacity(items.len());
            for item in items {
                match item {
                    // 展開を受け取る `[a, ...rest]` は対応しない。
                    ArrayElem::Spread(_) => return None,
                    ArrayElem::Item(expr) => targets.push(Some(as_target(expr)?)),
                }
            }
            Some(Target::Destructure(targets))
        }
        _ => None,
    }
}
