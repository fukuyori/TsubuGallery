//! 再帰下降パーサ (設計書 §13 の Parser)。
//!
//! 対応するのは設計書 §14.1 の範囲。Java 固有の高度な機能 (クラス、配列、
//! 文字列、ジェネリクス) は対象外で、見つけたらその場でエラーにする。

use crate::ast::*;
use crate::lexer::{CompileError, Keyword, Token, TokenKind, tokenize};

pub fn parse(source: &str) -> Result<Ast, CompileError> {
    Parser::new(tokenize(source)?).program()
}

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
    /// 見つけたクラス名。型として書けるようにするため先に集めておく。
    classes: std::collections::HashSet<String>,
}

impl Parser {
    fn new(tokens: Vec<Token>) -> Self {
        // クラス名を先に集める。`P p = ...` の `P` を型と分かるようにするには、
        // 定義より前に使われていても知っている必要がある。
        let mut classes = std::collections::HashSet::new();
        for pair in tokens.windows(2) {
            if pair[0].kind == TokenKind::Keyword(Keyword::Class)
                && let TokenKind::Ident(name) = &pair[1].kind
            {
                classes.insert(name.clone());
            }
        }
        Self { tokens, pos: 0, classes }
    }

    // ---- トークン操作 ---------------------------------------------------

    fn peek(&self) -> &TokenKind {
        &self.tokens[self.pos.min(self.tokens.len() - 1)].kind
    }

    fn peek_at(&self, offset: usize) -> &TokenKind {
        &self.tokens[(self.pos + offset).min(self.tokens.len() - 1)].kind
    }

    fn position(&self) -> (u32, u32) {
        let t = &self.tokens[self.pos.min(self.tokens.len() - 1)];
        (t.line, t.column)
    }

    fn advance(&mut self) -> TokenKind {
        let kind = self.tokens[self.pos.min(self.tokens.len() - 1)].kind.clone();
        if self.pos < self.tokens.len() - 1 {
            self.pos += 1;
        }
        kind
    }

    fn check(&self, kind: &TokenKind) -> bool {
        self.peek() == kind
    }

    fn eat(&mut self, kind: &TokenKind) -> bool {
        if self.check(kind) {
            self.advance();
            true
        } else {
            false
        }
    }

    fn expect(&mut self, kind: &TokenKind, what: &str) -> Result<(), CompileError> {
        if self.eat(kind) {
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
            TokenKind::Ident(name) => {
                self.advance();
                Ok(name)
            }
            _ => Err(self.error("名前がありません")),
        }
    }

    /// 型名を読む。`float[]` のように `[]` が続けば配列型。
    fn type_keyword(&mut self) -> Option<Type> {
        let ty = self.base_type()?;
        self.advance();

        // `[]` が続くだけ配列にする。`int[][]` は 2 次元。
        let mut ty = ty;
        while self.check(&TokenKind::LBracket) && self.peek_at(1) == &TokenKind::RBracket {
            self.advance();
            self.advance();
            match ty.to_array() {
                Some(next) => ty = next,
                // これ以上重ねられない型。`[]` は読み飛ばして先へ進む。
                None => break,
            }
        }
        Some(ty)
    }

    /// 現在位置が型名か。読み進めない。
    fn base_type(&self) -> Option<Type> {
        Some(match self.peek() {
            TokenKind::Keyword(Keyword::Void) => Type::Void,
            TokenKind::Keyword(Keyword::Int) => Type::Int,
            TokenKind::Keyword(Keyword::Float) => Type::Float,
            TokenKind::Keyword(Keyword::Boolean) => Type::Boolean,
            // `PVector` はキーワードではなく名前として来る。
            TokenKind::Ident(name) if name == "PVector" => Type::Vector,
            TokenKind::Ident(name) if name == "String" => Type::Str,
            // クラス名も型として書ける。どのクラスかは実行時に決まる。
            TokenKind::Ident(name) if self.classes.contains(name) => Type::Instance,
            _ => return None,
        })
    }

    // ---- トップレベル ---------------------------------------------------

    fn program(mut self) -> Result<Ast, CompileError> {
        let mut ast = Ast::default();

        while !self.check(&TokenKind::Eof) {
            let (line, column) = self.position();

            if self.check(&TokenKind::Keyword(Keyword::Class)) {
                ast.classes.push(self.class_declaration()?);
                continue;
            }

            let Some(ty) = self.type_keyword() else {
                return Err(self.error("トップレベルには型か関数定義だけを書けます"));
            };
            let name = self.ident()?;

            if self.check(&TokenKind::LParen) {
                ast.functions.push(self.function(ty, name, line, column)?);
            } else if ty == Type::Void {
                return Err(CompileError::new(line, column, "void の変数は宣言できません"));
            } else {
                let init = if self.eat(&TokenKind::Assign) {
                    // `int[] a = {1,2,3}` は式ではないのでここで受ける。
                    if ty.is_array() && self.check(&TokenKind::LBrace) {
                        Some(self.array_literal(ty)?)
                    } else {
                        Some(self.expression()?)
                    }
                } else {
                    None
                };
                self.expect(&TokenKind::Semicolon, "`;`")?;
                ast.globals.push(Stmt::VarDecl { ty, name, init, line, column });
            }
        }

        Ok(ast)
    }

    /// `class P { float x; P(float a) { ... } void step() { ... } }`。
    fn class_declaration(&mut self) -> Result<Class, CompileError> {
        let (line, column) = self.position();
        self.advance();
        let name = self.ident()?;
        self.expect(&TokenKind::LBrace, "`{`")?;

        let mut fields = Vec::new();
        let mut methods = Vec::new();
        let mut constructor = None;

        while !self.check(&TokenKind::RBrace) {
            if self.check(&TokenKind::Eof) {
                return Err(self.error("`}` がありません"));
            }
            let (mline, mcolumn) = self.position();

            // クラス名と同じ名前で始まればコンストラクタ。
            if matches!(self.peek(), TokenKind::Ident(n) if *n == name)
                && self.peek_at(1) == &TokenKind::LParen
            {
                self.advance();
                let f = self.function(Type::Void, name.clone(), mline, mcolumn)?;
                if constructor.replace(f).is_some() {
                    return Err(self.error("コンストラクタは 1 つだけです"));
                }
                continue;
            }

            let Some(ty) = self.type_keyword() else {
                return Err(self.error("フィールドかメソッドの型がありません"));
            };
            let member = self.ident()?;

            if self.check(&TokenKind::LParen) {
                methods.push(self.function(ty, member, mline, mcolumn)?);
                continue;
            }

            // フィールド。`float x, y;` のように並べて書ける。
            let mut names = vec![member];
            while self.eat(&TokenKind::Comma) {
                names.push(self.ident()?);
            }
            self.expect(&TokenKind::Semicolon, "`;`")?;
            for n in names {
                fields.push((ty, n));
            }
        }
        self.expect(&TokenKind::RBrace, "`}`")?;

        Ok(Class { name, fields, constructor, methods, line, column })
    }

    fn function(
        &mut self,
        return_type: Type,
        name: String,
        line: u32,
        column: u32,
    ) -> Result<Function, CompileError> {
        self.expect(&TokenKind::LParen, "`(`")?;

        let mut params = Vec::new();
        if !self.check(&TokenKind::RParen) {
            loop {
                let Some(ty) = self.type_keyword() else {
                    return Err(self.error("引数の型がありません"));
                };
                if ty == Type::Void {
                    return Err(self.error("void の引数は書けません"));
                }
                params.push((ty, self.ident()?));
                if !self.eat(&TokenKind::Comma) {
                    break;
                }
            }
        }
        self.expect(&TokenKind::RParen, "`)`")?;

        let Stmt::Block(body) = self.block()? else {
            unreachable!("block() は必ず Block を返す");
        };

        Ok(Function { name, return_type, params, body, line, column })
    }

    // ---- 文 -------------------------------------------------------------

    fn block(&mut self) -> Result<Stmt, CompileError> {
        self.expect(&TokenKind::LBrace, "`{`")?;
        let mut stmts = Vec::new();
        while !self.check(&TokenKind::RBrace) {
            if self.check(&TokenKind::Eof) {
                return Err(self.error("`}` がありません"));
            }
            stmts.push(self.statement()?);
        }
        self.expect(&TokenKind::RBrace, "`}`")?;
        Ok(Stmt::Block(stmts))
    }

    fn statement(&mut self) -> Result<Stmt, CompileError> {
        match self.peek() {
            TokenKind::LBrace => self.block(),
            TokenKind::Keyword(Keyword::If) => self.if_statement(),
            TokenKind::Keyword(Keyword::While) => self.while_statement(),
            TokenKind::Keyword(Keyword::For) => self.for_statement(),
            TokenKind::Keyword(Keyword::Switch) => self.switch_statement(),
            TokenKind::Keyword(Keyword::Break) => {
                let (line, column) = self.position();
                self.advance();
                self.expect(&TokenKind::Semicolon, "`;`")?;
                Ok(Stmt::Break { line, column })
            }

            TokenKind::Keyword(Keyword::Continue) => {
                let (line, column) = self.position();
                self.advance();
                self.expect(&TokenKind::Semicolon, "`;`")?;
                Ok(Stmt::Continue { line, column })
            }

            TokenKind::Keyword(Keyword::Return) => {
                let (line, column) = self.position();
                self.advance();
                let value = if self.check(&TokenKind::Semicolon) {
                    None
                } else {
                    Some(self.expression()?)
                };
                self.expect(&TokenKind::Semicolon, "`;`")?;
                Ok(Stmt::Return { value, line, column })
            }
            _ => {
                let stmt = self.simple_statement()?;
                self.expect(&TokenKind::Semicolon, "`;`")?;
                Ok(stmt)
            }
        }
    }

    /// `;` を伴わない単文。`for` の初期化・更新部でも使う。
    fn simple_statement(&mut self) -> Result<Stmt, CompileError> {
        let (line, column) = self.position();

        if let Some(ty) = self.type_keyword() {
            if ty == Type::Void {
                return Err(CompileError::new(line, column, "void の変数は宣言できません"));
            }
            let name = self.ident()?;
            let init = if self.eat(&TokenKind::Assign) {
                // `float[] a = {1, 2, 3}` の形。`{` は式ではないのでここで受ける。
                if ty.is_array() && self.check(&TokenKind::LBrace) {
                    Some(self.array_literal(ty)?)
                } else {
                    Some(self.expression()?)
                }
            } else {
                None
            };
            return Ok(Stmt::VarDecl { ty, name, init, line, column });
        }

        // 添字やプロパティを伴う文。`a[i] = v` / `v.x += 1` / `a[i].add(u)`。
        //
        // 代入で終わるとは限らない (メソッド呼び出しの式文もある) ので、
        // まず左辺を読んでから、続くトークンで何の文かを決める。
        if matches!(self.peek(), TokenKind::Ident(_) | TokenKind::Keyword(Keyword::This))
            && matches!(self.peek_at(1), TokenKind::LBracket | TokenKind::Dot)
        {
            let expr = self.postfix()?;

            if let Some(op) = self.assign_op() {
                self.advance();
                let value = self.expression()?;
                return match expr {
                    Expr::Index { target, index, .. } => Ok(Stmt::AssignIndex {
                        target: *target,
                        index: *index,
                        op,
                        value,
                        line,
                        column,
                    }),
                    Expr::Field { target, name } => Ok(Stmt::AssignField {
                        target: *target,
                        name,
                        op,
                        value,
                        line,
                        column,
                    }),
                    _ => Err(self.error("ここへは代入できません")),
                };
            }

            let delta = match self.peek() {
                TokenKind::Increment => Some(1),
                TokenKind::Decrement => Some(-1),
                _ => None,
            };
            if let Some(delta) = delta {
                self.advance();
                return match expr {
                    Expr::Index { target, index, .. } => Ok(Stmt::IncDecIndex {
                        target: *target,
                        index: *index,
                        delta,
                        line,
                        column,
                    }),
                    _ => Err(self.error("ここは増減できません")),
                };
            }

            // 代入でも増減でもなければ式文。`v.add(u)` がこれ。
            return Ok(Stmt::Expr(expr));
        }

        // `名前 =` / `名前 +=` / `名前++` は代入文。それ以外は式文。
        if let TokenKind::Ident(name) = self.peek().clone() {
            let op = assign_op_of(self.peek_at(1));
            if let Some(op) = op {
                self.advance();
                self.advance();
                let value = self.expression()?;
                return Ok(Stmt::Assign { name, op, value, line, column });
            }

            let delta = match self.peek_at(1) {
                TokenKind::Increment => Some(1),
                TokenKind::Decrement => Some(-1),
                _ => None,
            };
            if let Some(delta) = delta {
                self.advance();
                self.advance();
                return Ok(Stmt::IncDec { name, delta, line, column });
            }
        }

        Ok(Stmt::Expr(self.expression()?))
    }

    fn if_statement(&mut self) -> Result<Stmt, CompileError> {
        self.advance();
        self.expect(&TokenKind::LParen, "`(`")?;
        let cond = self.expression()?;
        self.expect(&TokenKind::RParen, "`)`")?;
        let then = Box::new(self.statement()?);
        let otherwise = if self.eat(&TokenKind::Keyword(Keyword::Else)) {
            Some(Box::new(self.statement()?))
        } else {
            None
        };
        Ok(Stmt::If { cond, then, otherwise })
    }

    fn while_statement(&mut self) -> Result<Stmt, CompileError> {
        self.advance();
        self.expect(&TokenKind::LParen, "`(`")?;
        let cond = self.expression()?;
        self.expect(&TokenKind::RParen, "`)`")?;
        let body = Box::new(self.statement()?);
        Ok(Stmt::While { cond, body })
    }

    fn for_statement(&mut self) -> Result<Stmt, CompileError> {
        let (line, column) = self.position();
        self.advance();
        self.expect(&TokenKind::LParen, "`(`")?;

        // 拡張 for。`for (int v : a)`。
        if self.looks_like_foreach() {
            let ty = self.type_keyword().expect("looks_like_foreach で確認済み");
            let name = self.ident()?;
            self.expect(&TokenKind::Colon, "`:`")?;
            let iterable = self.expression()?;
            self.expect(&TokenKind::RParen, "`)`")?;
            let body = Box::new(self.statement()?);
            return Ok(Stmt::ForEach { ty, name, iterable, body, line, column });
        }

        let init = if self.eat(&TokenKind::Semicolon) {
            None
        } else {
            let s = self.simple_statement()?;
            self.expect(&TokenKind::Semicolon, "`;`")?;
            Some(Box::new(s))
        };

        let cond =
            if self.check(&TokenKind::Semicolon) { None } else { Some(self.expression()?) };
        self.expect(&TokenKind::Semicolon, "`;`")?;

        let update =
            if self.check(&TokenKind::RParen) { None } else { Some(Box::new(self.simple_statement()?)) };
        self.expect(&TokenKind::RParen, "`)`")?;

        let body = Box::new(self.statement()?);
        Ok(Stmt::For { init, cond, update, body })
    }

    /// `{1, 2, 3}` の形の配列初期化子。
    fn array_literal(&mut self, ty: Type) -> Result<Expr, CompileError> {
        self.expect(&TokenKind::LBrace, "`{`")?;
        let mut items = Vec::new();
        while !self.check(&TokenKind::RBrace) {
            items.push(self.expression()?);
            if !self.eat(&TokenKind::Comma) {
                break;
            }
        }
        self.expect(&TokenKind::RBrace, "`}`")?;
        Ok(Expr::ArrayLit { ty, items })
    }

    /// 現在位置が代入演算子ならその種類。読み進めない。
    fn assign_op(&self) -> Option<AssignOp> {
        assign_op_of(self.peek())
    }

    /// 拡張 for (`for (int v : a)`) の形か。
    fn looks_like_foreach(&self) -> bool {
        if self.base_type().is_none() {
            return false;
        }
        // 型名のあとに `[]` が入ることもある。
        let after_type =
            if self.peek_at(1) == &TokenKind::LBracket { 3 } else { 1 };
        matches!(self.peek_at(after_type), TokenKind::Ident(_))
            && self.peek_at(after_type + 1) == &TokenKind::Colon
    }

    /// `switch (v) { case 1: ...; break; default: ... }`。
    ///
    /// Java と同じく `break` が無ければ次の case へ落ちる。落ちる書き方は
    /// つぶやきでは短縮に使われるので、そのまま再現する。
    fn switch_statement(&mut self) -> Result<Stmt, CompileError> {
        let (line, column) = self.position();
        self.advance();
        self.expect(&TokenKind::LParen, "`(`")?;
        let value = self.expression()?;
        self.expect(&TokenKind::RParen, "`)`")?;
        self.expect(&TokenKind::LBrace, "`{`")?;

        let mut cases: Vec<SwitchCase> = Vec::new();
        while !self.check(&TokenKind::RBrace) {
            if self.check(&TokenKind::Eof) {
                return Err(self.error("`}` がありません"));
            }

            let label = if self.eat(&TokenKind::Keyword(Keyword::Case)) {
                let label = self.expression()?;
                self.expect(&TokenKind::Colon, "`:`")?;
                Some(label)
            } else if self.eat(&TokenKind::Keyword(Keyword::Default)) {
                self.expect(&TokenKind::Colon, "`:`")?;
                if cases.iter().any(|c| c.label.is_none()) {
                    return Err(self.error("default は 1 つだけです"));
                }
                None
            } else {
                return Err(self.error("case か default がありません"));
            };

            // 次のラベルか `}` までが、このラベルの中身。
            let mut body = Vec::new();
            while !self.check(&TokenKind::RBrace)
                && !self.check(&TokenKind::Keyword(Keyword::Case))
                && !self.check(&TokenKind::Keyword(Keyword::Default))
            {
                if self.check(&TokenKind::Eof) {
                    return Err(self.error("`}` がありません"));
                }
                body.push(self.statement()?);
            }
            cases.push(SwitchCase { label, body });
        }
        self.expect(&TokenKind::RBrace, "`}`")?;

        Ok(Stmt::Switch { value, cases, line, column })
    }

    // ---- 式 -------------------------------------------------------------

    fn expression(&mut self) -> Result<Expr, CompileError> {
        self.ternary()
    }

    fn ternary(&mut self) -> Result<Expr, CompileError> {
        let cond = self.logical_or()?;
        if !self.eat(&TokenKind::Question) {
            return Ok(cond);
        }
        let then = self.expression()?;
        self.expect(&TokenKind::Colon, "`:`")?;
        let other = self.expression()?;
        Ok(Expr::Ternary { cond: Box::new(cond), then: Box::new(then), other: Box::new(other) })
    }

    fn logical_or(&mut self) -> Result<Expr, CompileError> {
        let mut lhs = self.logical_and()?;
        while self.eat(&TokenKind::OrOr) {
            let rhs = self.logical_and()?;
            lhs = Expr::Logical { op: LogicalOp::Or, lhs: Box::new(lhs), rhs: Box::new(rhs) };
        }
        Ok(lhs)
    }

    fn logical_and(&mut self) -> Result<Expr, CompileError> {
        let mut lhs = self.bit_or()?;
        while self.eat(&TokenKind::AndAnd) {
            let rhs = self.bit_or()?;
            lhs = Expr::Logical { op: LogicalOp::And, lhs: Box::new(lhs), rhs: Box::new(rhs) };
        }
        Ok(lhs)
    }

    // ビット演算の強さは Java に合わせる。`|` < `^` < `&` < 等値 の順。
    // `a & 1 == 0` が `a & (1 == 0)` になる Java の落とし穴もそのまま再現する。

    fn bit_or(&mut self) -> Result<Expr, CompileError> {
        let mut lhs = self.bit_xor()?;
        while self.eat(&TokenKind::Pipe) {
            let rhs = self.bit_xor()?;
            lhs = Expr::Binary { op: BinaryOp::BitOr, lhs: Box::new(lhs), rhs: Box::new(rhs) };
        }
        Ok(lhs)
    }

    fn bit_xor(&mut self) -> Result<Expr, CompileError> {
        let mut lhs = self.bit_and()?;
        while self.eat(&TokenKind::Caret) {
            let rhs = self.bit_and()?;
            lhs = Expr::Binary { op: BinaryOp::BitXor, lhs: Box::new(lhs), rhs: Box::new(rhs) };
        }
        Ok(lhs)
    }

    fn bit_and(&mut self) -> Result<Expr, CompileError> {
        let mut lhs = self.equality()?;
        while self.eat(&TokenKind::Amp) {
            let rhs = self.equality()?;
            lhs = Expr::Binary { op: BinaryOp::BitAnd, lhs: Box::new(lhs), rhs: Box::new(rhs) };
        }
        Ok(lhs)
    }

    fn equality(&mut self) -> Result<Expr, CompileError> {
        let mut lhs = self.comparison()?;
        loop {
            let op = match self.peek() {
                TokenKind::Eq => BinaryOp::Eq,
                TokenKind::Ne => BinaryOp::Ne,
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
                TokenKind::Lt => BinaryOp::Lt,
                TokenKind::Le => BinaryOp::Le,
                TokenKind::Gt => BinaryOp::Gt,
                TokenKind::Ge => BinaryOp::Ge,
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
                TokenKind::Shl => BinaryOp::Shl,
                TokenKind::Shr => BinaryOp::Shr,
                TokenKind::UShr => BinaryOp::UShr,
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
                TokenKind::Plus => BinaryOp::Add,
                TokenKind::Minus => BinaryOp::Sub,
                _ => return Ok(lhs),
            };
            self.advance();
            let rhs = self.multiplicative()?;
            lhs = Expr::Binary { op, lhs: Box::new(lhs), rhs: Box::new(rhs) };
        }
    }

    fn multiplicative(&mut self) -> Result<Expr, CompileError> {
        let mut lhs = self.unary()?;
        loop {
            let op = match self.peek() {
                TokenKind::Star => BinaryOp::Mul,
                TokenKind::Slash => BinaryOp::Div,
                TokenKind::Percent => BinaryOp::Rem,
                _ => return Ok(lhs),
            };
            self.advance();
            let rhs = self.unary()?;
            lhs = Expr::Binary { op, lhs: Box::new(lhs), rhs: Box::new(rhs) };
        }
    }

    fn unary(&mut self) -> Result<Expr, CompileError> {
        // `(int)x` のキャスト。`(` の次が型名で、その次が `)` のときだけ。
        // `(a)*b` のような括弧と取り違えないよう、3 トークン見てから決める。
        if self.check(&TokenKind::LParen) {
            let cast = match (self.peek_at(1), self.peek_at(2)) {
                (TokenKind::Keyword(Keyword::Int), TokenKind::RParen) => Some(Type::Int),
                (TokenKind::Keyword(Keyword::Float), TokenKind::RParen) => Some(Type::Float),
                (TokenKind::Keyword(Keyword::Boolean), TokenKind::RParen) => Some(Type::Boolean),
                _ => None,
            };
            if let Some(ty) = cast {
                self.advance();
                self.advance();
                self.advance();
                return Ok(Expr::Cast { ty, operand: Box::new(self.unary()?) });
            }
        }

        // `new PVector(x, y)` と `new float[n]`。
        if self.check(&TokenKind::Keyword(Keyword::New)) {
            let (line, column) = self.position();
            self.advance();
            let Some(element) = self.base_type() else {
                return Err(self.error("new のあとに型がありません"));
            };
            self.advance();

            // `new PVector(...)` は 1 本、`new PVector[n]` は配列。
            // 型のあとが `(` か `[` かで見分ける。
            if element == Type::Instance && self.check(&TokenKind::LParen) {
                // 直前に読んだ識別子がクラス名。
                let class = match &self.tokens[self.pos - 1].kind {
                    TokenKind::Ident(n) => n.clone(),
                    _ => return Err(self.error("クラス名がありません")),
                };
                self.advance();
                let mut args = Vec::new();
                while !self.check(&TokenKind::RParen) {
                    args.push(self.expression()?);
                    if !self.eat(&TokenKind::Comma) {
                        break;
                    }
                }
                self.expect(&TokenKind::RParen, "`)`")?;
                return self.suffixes(Expr::New { class, args, line, column });
            }

            if element == Type::Vector && self.check(&TokenKind::LParen) {
                self.advance();
                let mut args = Vec::new();
                while !self.check(&TokenKind::RParen) {
                    args.push(self.expression()?);
                    if !self.eat(&TokenKind::Comma) {
                        break;
                    }
                }
                self.expect(&TokenKind::RParen, "`)`")?;
                return self.suffixes(Expr::NewVector { args });
            }
            // `new float[3][4]` のように次元の数だけ `[...]` が並ぶ。
            let mut sizes = Vec::new();
            let mut ty = element;
            while self.check(&TokenKind::LBracket) {
                self.advance();
                sizes.push(self.expression()?);
                self.expect(&TokenKind::RBracket, "`]`")?;
                ty = ty
                    .to_array()
                    .ok_or_else(|| CompileError::new(line, column, "この型の配列は作れません"))?;
            }
            if sizes.is_empty() {
                return Err(self.error("配列の大きさがありません"));
            }
            if sizes.len() > 2 {
                return Err(CompileError::new(line, column, "配列は 2 次元までです"));
            }
            return self.suffixes(Expr::NewArray { ty, sizes });
        }

        let op = match self.peek() {
            TokenKind::Minus => Some(UnaryOp::Neg),
            TokenKind::Bang => Some(UnaryOp::Not),
            TokenKind::Tilde => Some(UnaryOp::BitNot),
            // 単項 `+` は何もしないので読み捨てる。
            TokenKind::Plus => {
                self.advance();
                return self.unary();
            }
            _ => None,
        };
        match op {
            Some(op) => {
                self.advance();
                Ok(Expr::Unary { op, operand: Box::new(self.unary()?) })
            }
            None => self.postfix(),
        }
    }

    /// `a[i]` と `a.length`。primary のあとに続けられる。
    fn postfix(&mut self) -> Result<Expr, CompileError> {
        let expr = self.primary()?;
        self.suffixes(expr)
    }

    /// 添字・プロパティ・メソッド呼び出しを続けて読む。
    ///
    /// `new P(1).show()` のように、primary 以外のあとにも続けられるよう
    /// 切り出してある。
    fn suffixes(&mut self, expr: Expr) -> Result<Expr, CompileError> {
        let mut expr = expr;
        loop {
            if self.check(&TokenKind::LBracket) {
                let (line, column) = self.position();
                self.advance();
                let index = Box::new(self.expression()?);
                self.expect(&TokenKind::RBracket, "`]`")?;
                expr = Expr::Index { target: Box::new(expr), index, line, column };
                continue;
            }
            if self.check(&TokenKind::Dot) {
                let (line, column) = self.position();
                self.advance();
                let name = self.ident()?;

                // `v.add(u)` のメソッド呼び出し。
                if self.check(&TokenKind::LParen) {
                    self.advance();
                    let mut args = Vec::new();
                    while !self.check(&TokenKind::RParen) {
                        args.push(self.expression()?);
                        if !self.eat(&TokenKind::Comma) {
                            break;
                        }
                    }
                    self.expect(&TokenKind::RParen, "`)`")?;
                    expr = Expr::MethodCall { target: Box::new(expr), name, args, line, column };
                    continue;
                }

                expr = if name == "length" {
                    Expr::ArrayLen { target: Box::new(expr) }
                } else {
                    Expr::Field { target: Box::new(expr), name }
                };
                continue;
            }
            return Ok(expr);
        }
    }

    fn primary(&mut self) -> Result<Expr, CompileError> {
        let (line, column) = self.position();

        match self.peek().clone() {
            TokenKind::Int(v) => {
                self.advance();
                Ok(Expr::Int(v))
            }
            TokenKind::Str(text) => {
                self.advance();
                Ok(Expr::Str(text))
            }
            TokenKind::Keyword(Keyword::This) => {
                self.advance();
                Ok(Expr::This)
            }

            // `int(x)` / `float(x)` の変換。型名と同じ綴りなので、`(` が続く
            // ときだけ関数として扱う。キャストの `(int)x` とは別物。
            TokenKind::Keyword(kw @ (Keyword::Int | Keyword::Float | Keyword::Boolean))
                if self.peek_at(1) == &TokenKind::LParen =>
            {
                let name = match kw {
                    Keyword::Int => "int",
                    Keyword::Float => "float",
                    _ => "boolean",
                };
                self.advance();
                self.advance();
                let mut args = Vec::new();
                while !self.check(&TokenKind::RParen) {
                    args.push(self.expression()?);
                    if !self.eat(&TokenKind::Comma) {
                        break;
                    }
                }
                self.expect(&TokenKind::RParen, "`)`")?;
                Ok(Expr::Call { name: name.to_string(), args, line, column })
            }
            TokenKind::Float(v) => {
                self.advance();
                Ok(Expr::Float(v))
            }
            TokenKind::Keyword(Keyword::True) => {
                self.advance();
                Ok(Expr::Bool(true))
            }
            TokenKind::Keyword(Keyword::False) => {
                self.advance();
                Ok(Expr::Bool(false))
            }
            TokenKind::LParen => {
                self.advance();
                let inner = self.expression()?;
                self.expect(&TokenKind::RParen, "`)`")?;
                Ok(inner)
            }
            TokenKind::Ident(name) => {
                self.advance();
                if !self.eat(&TokenKind::LParen) {
                    return Ok(Expr::Var(name));
                }
                let mut args = Vec::new();
                if !self.check(&TokenKind::RParen) {
                    loop {
                        args.push(self.expression()?);
                        if !self.eat(&TokenKind::Comma) {
                            break;
                        }
                    }
                }
                self.expect(&TokenKind::RParen, "`)`")?;
                Ok(Expr::Call { name, args, line, column })
            }
            _ => Err(self.error("式がありません")),
        }
    }
}

/// トークンが代入演算子ならその種類。
fn assign_op_of(tok: &TokenKind) -> Option<AssignOp> {
    Some(match tok {
        TokenKind::Assign => AssignOp::Set,
        TokenKind::PlusAssign => AssignOp::Add,
        TokenKind::MinusAssign => AssignOp::Sub,
        TokenKind::StarAssign => AssignOp::Mul,
        TokenKind::SlashAssign => AssignOp::Div,
        TokenKind::PercentAssign => AssignOp::Rem,
        TokenKind::AmpAssign => AssignOp::BitAnd,
        TokenKind::PipeAssign => AssignOp::BitOr,
        TokenKind::CaretAssign => AssignOp::BitXor,
        TokenKind::ShlAssign => AssignOp::Shl,
        TokenKind::ShrAssign => AssignOp::Shr,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn expr(source: &str) -> Expr {
        let ast = parse(&format!("void draw() {{ x = {source}; }}")).expect("パースに成功する");
        match &ast.functions[0].body[0] {
            Stmt::Assign { value, .. } => value.clone(),
            other => panic!("代入文ではない: {other:?}"),
        }
    }

    #[test]
    fn multiplication_binds_tighter_than_addition() {
        assert_eq!(
            expr("1 + 2 * 3"),
            Expr::Binary {
                op: BinaryOp::Add,
                lhs: Box::new(Expr::Int(1)),
                rhs: Box::new(Expr::Binary {
                    op: BinaryOp::Mul,
                    lhs: Box::new(Expr::Int(2)),
                    rhs: Box::new(Expr::Int(3)),
                }),
            }
        );
    }

    #[test]
    fn subtraction_is_left_associative() {
        // 1 - 2 - 3 は (1 - 2) - 3。
        let Expr::Binary { op, lhs, .. } = expr("1 - 2 - 3") else { panic!("二項演算ではない") };
        assert_eq!(op, BinaryOp::Sub);
        assert!(matches!(*lhs, Expr::Binary { op: BinaryOp::Sub, .. }));
    }

    #[test]
    fn comparison_binds_looser_than_arithmetic() {
        let Expr::Binary { op, lhs, .. } = expr("a + 1 < b") else { panic!("二項演算ではない") };
        assert_eq!(op, BinaryOp::Lt);
        assert!(matches!(*lhs, Expr::Binary { op: BinaryOp::Add, .. }));
    }

    #[test]
    fn logical_and_binds_tighter_than_or() {
        let Expr::Logical { op, rhs, .. } = expr("a || b && c") else { panic!("論理演算ではない") };
        assert_eq!(op, LogicalOp::Or);
        assert!(matches!(*rhs, Expr::Logical { op: LogicalOp::And, .. }));
    }

    #[test]
    fn ternary_is_right_associative() {
        let Expr::Ternary { other, .. } = expr("a ? 1 : b ? 2 : 3") else { panic!("三項演算ではない") };
        assert!(matches!(*other, Expr::Ternary { .. }));
    }

    #[test]
    fn unary_minus_applies_before_multiplication() {
        let Expr::Binary { op, lhs, .. } = expr("-a * b") else { panic!("二項演算ではない") };
        assert_eq!(op, BinaryOp::Mul);
        assert!(matches!(*lhs, Expr::Unary { op: UnaryOp::Neg, .. }));
    }

    #[test]
    fn parses_a_whole_sketch() {
        let ast = parse(
            r#"
            int count = 0;

            void setup() {
              size(400, 400);
            }

            void draw() {
              background(0);
              for (int i = 0; i < 10; i++) {
                float x = wobble(i);
                if (x > 0.5) {
                  ellipse(x, i * 10, 4, 4);
                } else {
                  point(x, i);
                }
              }
              count++;
            }

            float wobble(int i) {
              return sin(i * 0.1) * 0.5 + 0.5;
            }
            "#,
        )
        .expect("パースに成功する");

        assert_eq!(ast.globals.len(), 1);
        assert_eq!(ast.functions.len(), 3);
        assert!(ast.function("setup").is_some());
        assert!(ast.function("draw").is_some());
        assert_eq!(ast.function("wobble").expect("ある").params.len(), 1);
    }

    #[test]
    fn for_loop_parts_may_be_empty() {
        let ast = parse("void draw() { for (;;) { x = 1; } }").expect("パースに成功する");
        let Stmt::For { init, cond, update, .. } = &ast.functions[0].body[0] else {
            panic!("for 文ではない");
        };
        assert!(init.is_none() && cond.is_none() && update.is_none());
    }

    #[test]
    fn dangling_else_binds_to_the_nearest_if() {
        let ast = parse("void draw() { if (a) if (b) x = 1; else x = 2; }").expect("パースに成功する");
        let Stmt::If { then, otherwise, .. } = &ast.functions[0].body[0] else {
            panic!("if 文ではない");
        };
        assert!(otherwise.is_none(), "外側の if に else が付いてはいけない");
        assert!(matches!(**then, Stmt::If { otherwise: Some(_), .. }));
    }

    #[test]
    fn missing_semicolon_reports_a_position() {
        let e = parse("void draw() {\n  background(0)\n}").unwrap_err();
        assert_eq!(e.line, 3);
        assert!(e.message.contains("`;`"), "{e}");
    }

    #[test]
    fn unclosed_brace_is_an_error() {
        let e = parse("void draw() { background(0);").unwrap_err();
        assert!(e.message.contains("`}`"), "{e}");
    }

    #[test]
    fn top_level_junk_is_rejected() {
        let e = parse("42;").unwrap_err();
        assert!(e.message.contains("トップレベル"), "{e}");
    }

    #[test]
    fn a_class_is_read_at_the_top_level() {
        let ast = parse("class Foo { float x; Foo(float a) { x = a; } void go() {} }").expect("読める");
        let class = &ast.classes[0];
        assert_eq!(class.name, "Foo");
        assert_eq!(class.fields.len(), 1);
        assert!(class.constructor.is_some());
        assert_eq!(class.methods.len(), 1);
    }
}
