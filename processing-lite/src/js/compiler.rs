//! p5.js subset の Bytecode コンパイラ。
//!
//! # 変数のあつかい
//!
//! **関数の引数だけがローカルで、それ以外はすべてグローバル**にする。
//! つぶやき作品はほぼ全部の変数をグローバルに置くので、これで足りる。
//! 代わりに、アロー関数はローカル変数を閉じ込められない (クロージャは無い)。
//!
//! # `draw` の呼び方
//!
//! p5 では `draw` はグローバル変数に入った関数なので、毎フレーム名前で引く。
//! そのために「グローバルを読んで呼ぶ」だけの小さな関数を合成し、それを
//! [`Program::draw`] にしている。VM 側は Java Mode と同じ入口で動く。

use std::collections::HashMap;

use crate::bytecode::{CompiledFunction, Op, Program};
use crate::lexer::CompileError;
use crate::natives::{self, BuiltinVar};

use super::ast::*;

pub fn compile(script: &Script) -> Result<Program, CompileError> {
    Compiler::new().run(script)
}

/// コンパイル中のループ 1 つ分。飛び先はループを組み終えてから埋める。
#[derive(Default)]
struct LoopCtx {
    breaks: Vec<usize>,
    continues: Vec<usize>,
}

struct Compiler {
    globals: HashMap<String, u16>,
    keys: HashMap<String, u16>,
    functions: Vec<CompiledFunction>,

    /// 現在の関数の引数。ここに無い名前はグローバル。
    params: HashMap<String, u16>,
    code: Vec<Op>,
    /// 文字列リテラルの表。
    strings: Vec<String>,
    /// 隠し変数の通し番号。`for...of` の走査用などに使う。
    hidden: u32,
    /// 入れ子になったループ。`break` と `continue` の飛び先を集める。
    loops: Vec<LoopCtx>,
}

impl Compiler {
    fn new() -> Self {
        Self {
            globals: HashMap::new(),
            keys: HashMap::new(),
            functions: Vec::new(),
            params: HashMap::new(),
            code: Vec::new(),
            strings: Vec::new(),
            hidden: 0,
            loops: Vec::new(),
        }
    }

    fn run(mut self, script: &Script) -> Result<Program, CompileError> {
        // トップレベルは 1 つの関数にまとめ、グローバルの初期化として実行する。
        self.params.clear();
        self.code = Vec::new();
        for stmt in &script.statements {
            self.statement(stmt)?;
        }
        self.code.push(Op::Return);
        let init_code = std::mem::take(&mut self.code);

        let globals_init = self.push_function("<script>", 0, init_code);

        let setup = self.entry_point("setup");
        let draw = self.entry_point("draw");
        if setup.is_none() && draw.is_none() {
            return Err(CompileError::new(1, 1, "setup() か draw() のどちらかが必要です"));
        }

        let mut keys = vec![String::new(); self.keys.len()];
        for (name, index) in &self.keys {
            keys[*index as usize] = name.clone();
        }

        Ok(Program {
            strings: self.strings.clone(),
            functions: self.functions,
            keys,
            global_names: self.globals.clone(),
            globals_init,
            global_count: self.globals.len() as u16,
            setup,
            draw,
        })
    }

    /// `draw` のようなグローバルを読んで呼ぶだけの関数を合成する。
    fn entry_point(&mut self, name: &str) -> Option<u16> {
        let slot = *self.globals.get(name)?;
        let code = vec![Op::LoadGlobal(slot), Op::CallValue(0), Op::Pop, Op::Return];
        Some(self.push_function(name, 0, code))
    }

    fn push_function(&mut self, name: &str, arity: u8, code: Vec<Op>) -> u16 {
        let index = self.functions.len() as u16;
        self.functions.push(CompiledFunction {
            name: name.to_string(),
            arity,
            // 引数だけがローカル。
            local_count: arity as u16,
            return_type: crate::ast::Type::Float,
            code,
        });
        index
    }

    /// 文字列を定数表へ入れ、その番号を返す。
    fn intern(&mut self, text: &str) -> u16 {
        if let Some(index) = self.strings.iter().position(|s| s == text) {
            return index as u16;
        }
        self.strings.push(text.to_string());
        (self.strings.len() - 1) as u16
    }

    /// ユーザーが書けない名前の隠し変数を 1 つ作る。
    ///
    /// `$` で始まる名前は識別子として読めないので、ぶつかることがない。
    fn hidden_global(&mut self, what: &str) -> u16 {
        self.hidden += 1;
        let name = format!("${what}.{}", self.hidden);
        self.global(&name)
    }

    fn global(&mut self, name: &str) -> u16 {
        if let Some(slot) = self.globals.get(name) {
            return *slot;
        }
        let slot = self.globals.len() as u16;
        self.globals.insert(name.to_string(), slot);
        slot
    }

    fn key(&mut self, name: &str) -> u16 {
        if let Some(index) = self.keys.get(name) {
            return *index;
        }
        let index = self.keys.len() as u16;
        self.keys.insert(name.to_string(), index);
        index
    }

    // ---- 文 -------------------------------------------------------------

    fn statement(&mut self, stmt: &Stmt) -> Result<(), CompileError> {
        match stmt {
            Stmt::Expr(expr) => {
                self.expression(expr)?;
                self.code.push(Op::Pop);
            }

            Stmt::Declare(names) => {
                for (name, init) in names {
                    match init {
                        Some(expr) => self.expression(expr)?,
                        None => self.code.push(Op::ConstUndefined),
                    }
                    self.store(name);
                    self.code.push(Op::Pop);
                }
            }

            Stmt::Block(statements) => {
                for s in statements {
                    self.statement(s)?;
                }
            }

            Stmt::If { cond, then, otherwise } => {
                self.expression(cond)?;
                let jump_else = self.emit_jump(Op::JumpIfFalse(0));
                self.statement(then)?;
                match otherwise {
                    Some(other) => {
                        let jump_end = self.emit_jump(Op::Jump(0));
                        self.patch(jump_else);
                        self.statement(other)?;
                        self.patch(jump_end);
                    }
                    None => self.patch(jump_else),
                }
            }

            Stmt::While { cond, body } => {
                let start = self.code.len() as u32;
                self.expression(cond)?;
                let jump_end = self.emit_jump(Op::JumpIfFalse(0));

                self.loops.push(LoopCtx::default());
                self.statement(body)?;
                let ctx = self.loops.pop().expect("直前に積んだ");

                // continue は条件の判定へ戻る。
                for at in ctx.continues {
                    self.patch_to(at, start);
                }
                self.code.push(Op::Jump(start));
                self.patch(jump_end);
                for at in ctx.breaks {
                    self.patch(at);
                }
            }

            Stmt::For { init, cond, update, body } => {
                if let Some(init) = init {
                    self.statement(init)?;
                }
                let start = self.code.len() as u32;
                let jump_end = match cond {
                    Some(cond) => {
                        self.expression(cond)?;
                        Some(self.emit_jump(Op::JumpIfFalse(0)))
                    }
                    None => None,
                };
                self.loops.push(LoopCtx::default());
                self.statement(body)?;
                let ctx = self.loops.pop().expect("直前に積んだ");

                // continue は更新式へ飛ぶ。飛ばすと `i++` が実行されず無限ループになる。
                let update_at = self.code.len() as u32;
                for at in ctx.continues {
                    self.patch_to(at, update_at);
                }
                if let Some(update) = update {
                    self.expression(update)?;
                    self.code.push(Op::Pop);
                }
                self.code.push(Op::Jump(start));
                if let Some(jump_end) = jump_end {
                    self.patch(jump_end);
                }
                for at in ctx.breaks {
                    self.patch(at);
                }
            }

            Stmt::Break { line, column } => {
                if self.loops.is_empty() {
                    return Err(CompileError::new(
                        *line,
                        *column,
                        "break はループの中でしか使えません".to_string(),
                    ));
                }
                let at = self.emit_jump(Op::Jump(0));
                self.loops.last_mut().expect("空でないと確認済み").breaks.push(at);
            }

            Stmt::Continue { line, column } => {
                if self.loops.is_empty() {
                    return Err(CompileError::new(
                        *line,
                        *column,
                        "continue はループの中でしか使えません".to_string(),
                    ));
                }
                let at = self.emit_jump(Op::Jump(0));
                self.loops.last_mut().expect("空でないと確認済み").continues.push(at);
            }

            // `for (v of xs)`。配列と添字を隠し変数に置いて回す。
            Stmt::ForOf { name, iterable, body } => {
                let array = self.hidden_global("forof.array");
                let index = self.hidden_global("forof.index");
                let value = self.global(name);

                self.expression(iterable)?;
                self.code.push(Op::StoreGlobal(array));
                self.code.push(Op::ConstInt(0));
                self.code.push(Op::StoreGlobal(index));

                let start = self.code.len() as u32;
                self.code.push(Op::LoadGlobal(index));
                self.code.push(Op::LoadGlobal(array));
                self.code.push(Op::ArrayLen);
                self.code.push(Op::Lt);
                let jump_end = self.emit_jump(Op::JumpIfFalse(0));

                self.code.push(Op::LoadGlobal(array));
                self.code.push(Op::LoadGlobal(index));
                self.code.push(Op::GetIndex);
                self.code.push(Op::StoreGlobal(value));

                self.loops.push(LoopCtx::default());
                self.statement(body)?;
                let ctx = self.loops.pop().expect("直前に積んだ");

                let update_at = self.code.len() as u32;
                for at in ctx.continues {
                    self.patch_to(at, update_at);
                }
                self.code.push(Op::LoadGlobal(index));
                self.code.push(Op::ConstInt(1));
                self.code.push(Op::Add);
                self.code.push(Op::StoreGlobal(index));
                self.code.push(Op::Jump(start));

                self.patch(jump_end);
                for at in ctx.breaks {
                    self.patch(at);
                }
            }

            Stmt::Return(value) => match value {
                Some(expr) => {
                    self.expression(expr)?;
                    self.code.push(Op::ReturnValue);
                }
                None => {
                    self.code.push(Op::ConstUndefined);
                    self.code.push(Op::ReturnValue);
                }
            },

            Stmt::Function { name, params, body, .. } => {
                let index = self.compile_function(name, params, body)?;
                self.code.push(Op::ConstFunction(index));
                self.store(name);
                self.code.push(Op::Pop);
            }
        }
        Ok(())
    }

    /// 関数の本体を別のコード列としてコンパイルする。
    fn compile_function(
        &mut self,
        name: &str,
        params: &[Param],
        body: &[Stmt],
    ) -> Result<u16, CompileError> {
        let outer_params = std::mem::take(&mut self.params);
        let outer_code = std::mem::take(&mut self.code);

        for (slot, param) in params.iter().enumerate() {
            self.params.insert(param.name.clone(), slot as u16);
        }

        // 既定値。渡されなかった引数だけを埋める。
        for (slot, param) in params.iter().enumerate() {
            let Some(default) = &param.default else { continue };
            self.code.push(Op::LoadLocal(slot as u16));
            self.code.push(Op::IsUndefined);
            let skip = self.emit_jump(Op::JumpIfFalse(0));
            self.expression(default)?;
            self.code.push(Op::StoreLocal(slot as u16));
            self.patch(skip);
        }

        for stmt in body {
            self.statement(stmt)?;
        }
        // 最後まで来たら undefined を返す。
        self.code.push(Op::ConstUndefined);
        self.code.push(Op::ReturnValue);

        let code = std::mem::replace(&mut self.code, outer_code);
        self.params = outer_params;
        Ok(self.push_function(name, params.len() as u8, code))
    }

    // ---- 式 -------------------------------------------------------------

    fn expression(&mut self, expr: &Expr) -> Result<(), CompileError> {
        match expr {
            Expr::Number(v) => self.code.push(Op::ConstFloat(*v)),
            Expr::Bool(v) => self.code.push(Op::ConstBool(*v)),
            Expr::Undefined => self.code.push(Op::ConstUndefined),

            Expr::Str(text) => {
                let index = self.intern(text);
                self.code.push(Op::ConstStr(index));
            }

            Expr::Ident(name) => self.load(name),

            Expr::Array(items) => {
                // 展開が無ければ、そのまま並べて 1 命令で作る。
                if items.iter().all(|e| matches!(e, ArrayElem::Item(_))) {
                    for item in items {
                        let ArrayElem::Item(expr) = item else { unreachable!() };
                        self.expression(expr)?;
                    }
                    self.code.push(Op::NewArray(items.len() as u16));
                } else {
                    // 展開が混ざると個数が実行時まで決まらない。空から足していく。
                    self.code.push(Op::NewArray(0));
                    for item in items {
                        match item {
                            ArrayElem::Item(expr) => {
                                self.expression(expr)?;
                                self.code.push(Op::ArrayPush);
                            }
                            ArrayElem::Spread(expr) => {
                                self.expression(expr)?;
                                self.code.push(Op::ArrayExtend);
                            }
                        }
                    }
                }
            }

            Expr::Object(fields) => {
                self.code.push(Op::NewObject);
                for (name, value) in fields {
                    let key = self.key(name);
                    self.expression(value)?;
                    self.code.push(Op::InitProp(key));
                }
            }

            Expr::Arrow { params, body } => {
                let statements = match body.as_ref() {
                    ArrowBody::Block(statements) => statements.clone(),
                    // `p => expr` は `return expr` と同じ。
                    ArrowBody::Expr(expr) => vec![Stmt::Return(Some(expr.clone()))],
                };
                let index = self.compile_function("<arrow>", params, &statements)?;
                self.code.push(Op::ConstFunction(index));
            }

            Expr::Unary { op, operand } => {
                self.expression(operand)?;
                self.code.push(match op {
                    UnaryOp::Neg => Op::Neg,
                    UnaryOp::Not => Op::Not,
                    UnaryOp::BitNot => Op::BitNot,
                });
            }

            Expr::Binary { op, lhs, rhs } => {
                self.expression(lhs)?;
                self.expression(rhs)?;
                self.code.push(binary_op(*op));
            }

            Expr::Logical { op, lhs, rhs } => {
                self.expression(lhs)?;
                self.code.push(Op::Dup);
                let jump = match op {
                    LogicalOp::And => self.emit_jump(Op::JumpIfFalse(0)),
                    LogicalOp::Or => self.emit_jump(Op::JumpIfTrue(0)),
                };
                self.code.push(Op::Pop);
                self.expression(rhs)?;
                self.patch(jump);
            }

            Expr::Ternary { cond, then, other } => {
                self.expression(cond)?;
                let jump_other = self.emit_jump(Op::JumpIfFalse(0));
                self.expression(then)?;
                let jump_end = self.emit_jump(Op::Jump(0));
                self.patch(jump_other);
                self.expression(other)?;
                self.patch(jump_end);
            }

            Expr::Member { object, name } => {
                // `Math.PI` や `Math.sin` は、対応する組み込みへ読み替える。
                if let Some(op) = self.math_member(object, name) {
                    self.code.push(op);
                    return Ok(());
                }
                let key = self.key(name);
                self.expression(object)?;
                self.code.push(Op::GetProp(key));
            }

            Expr::Index { object, index } => {
                self.expression(object)?;
                self.expression(index)?;
                self.code.push(Op::GetIndex);
            }

            Expr::Sequence(first, second) => {
                self.expression(first)?;
                self.code.push(Op::Pop);
                self.expression(second)?;
            }

            Expr::Assign { target, op, value } => self.assign(target, *op, value)?,
            Expr::Update { target, delta, prefix } => self.update(target, *delta, *prefix)?,
            Expr::Call { callee, args, line, column } => self.call(callee, args, *line, *column)?,
        }
        Ok(())
    }

    /// 代入。JavaScript の代入は式なので、書いた値をスタックに残す。
    fn assign(&mut self, target: &Target, op: AssignOp, value: &Expr) -> Result<(), CompileError> {
        match target {
            // `[a, b] = xs`。右辺を隠し変数へ置いてから、順に取り出す。
            // `[a, b] = [b, a]` の入れ替えが期待どおり動くのはこのため。
            Target::Destructure(targets) => {
                let temp = self.hidden_global("destructure");
                self.expression(value)?;
                self.code.push(Op::StoreGlobal(temp));

                for (index, slot) in targets.iter().enumerate() {
                    let Some(slot) = slot else { continue };
                    self.code.push(Op::LoadGlobal(temp));
                    self.code.push(Op::ConstInt(index as i32));
                    self.code.push(Op::GetIndex);
                    self.store_top(slot)?;
                    self.code.push(Op::Pop);
                }
                // 代入は式なので、右辺そのものを残す。
                self.code.push(Op::LoadGlobal(temp));
            }
            Target::Var(name) => {
                if op != AssignOp::Set {
                    self.load(name);
                }
                self.expression(value)?;
                self.apply_compound(op);
                self.store(name);
            }
            Target::Member(object, name) => {
                let key = self.key(name);
                self.expression(object)?;
                if op != AssignOp::Set {
                    self.code.push(Op::Dup);
                    self.code.push(Op::GetProp(key));
                }
                self.expression(value)?;
                self.apply_compound(op);
                self.code.push(Op::SetProp(key));
            }
            Target::Index(object, index) => {
                self.expression(object)?;
                self.expression(index)?;
                if op != AssignOp::Set {
                    // `[target, index]` を複製してから読む。
                    self.code.push(Op::Dup2);
                    self.code.push(Op::GetIndex);
                }
                self.expression(value)?;
                self.apply_compound(op);
                self.code.push(Op::SetIndex);
            }
        }
        Ok(())
    }

    /// `+=` などの演算。`=` なら何もしない。
    fn apply_compound(&mut self, op: AssignOp) {
        let binary = match op {
            AssignOp::Set => return,
            AssignOp::Add => BinaryOp::Add,
            AssignOp::Sub => BinaryOp::Sub,
            AssignOp::Mul => BinaryOp::Mul,
            AssignOp::Div => BinaryOp::Div,
            AssignOp::Rem => BinaryOp::Rem,
            AssignOp::BitAnd => BinaryOp::BitAnd,
            AssignOp::BitOr => BinaryOp::BitOr,
            AssignOp::BitXor => BinaryOp::BitXor,
            AssignOp::Shl => BinaryOp::Shl,
            AssignOp::Shr => BinaryOp::Shr,
        };
        self.code.push(binary_op(binary));
    }

    /// `Math.PI` などの定数を読む。`Math` 以外や未対応の名前なら `None`。
    fn math_member(&self, object: &Expr, name: &str) -> Option<Op> {
        if !is_math(object) || self.globals.contains_key("Math") {
            return None;
        }
        Some(match name {
            "PI" => Op::ConstFloat(std::f32::consts::PI),
            "E" => Op::ConstFloat(std::f32::consts::E),
            "SQRT2" => Op::ConstFloat(std::f32::consts::SQRT_2),
            "LN2" => Op::ConstFloat(std::f32::consts::LN_2),
            "LN10" => Op::ConstFloat(std::f32::consts::LN_10),
            // 関数として持ち回る書き方 (`S=Math.sin`) にも応える。
            other => Op::ConstNativeFn(natives::resolve_any(math_name(other))?),
        })
    }

    /// スタックトップの値を的へ書き込む。書いた値はスタックに残る。
    fn store_top(&mut self, target: &Target) -> Result<(), CompileError> {
        match target {
            Target::Var(name) => self.store(name),
            Target::Member(object, name) => {
                let key = self.key(name);
                // `[obj, value]` の順に積む必要があるので、値を退避してから
                // オブジェクトを積み直す。
                let temp = self.hidden_global("store");
                self.code.push(Op::StoreGlobal(temp));
                self.expression(object)?;
                self.code.push(Op::LoadGlobal(temp));
                self.code.push(Op::SetProp(key));
            }
            Target::Index(object, index) => {
                let temp = self.hidden_global("store");
                self.code.push(Op::StoreGlobal(temp));
                self.expression(object)?;
                self.expression(index)?;
                self.code.push(Op::LoadGlobal(temp));
                self.code.push(Op::SetIndex);
            }
            Target::Destructure(_) => {
                return Err(CompileError::new(0, 0, "入れ子の分割代入は書けません".to_string()));
            }
        }
        Ok(())
    }

    /// `i++` と `++i`。後置は元の値を返す。
    fn update(&mut self, target: &Target, delta: f32, prefix: bool) -> Result<(), CompileError> {
        // 元の値が要るのは後置だけ。
        if !prefix {
            match target {
                Target::Var(name) => self.load(name),
                Target::Member(object, name) => {
                    let key = self.key(name);
                    self.expression(object)?;
                    self.code.push(Op::GetProp(key));
                }
                Target::Index(object, index) => {
                    self.expression(object)?;
                    self.expression(index)?;
                    self.code.push(Op::GetIndex);
                }
                Target::Destructure(_) => {
                    return Err(CompileError::new(0, 0, "ここは増減できません".to_string()));
                }
            }
        }

        self.assign(target, AssignOp::Add, &Expr::Number(delta))?;
        if !prefix {
            // 新しい値を捨てて、元の値を残す。
            self.code.push(Op::Pop);
        }
        Ok(())
    }

    fn call(
        &mut self,
        callee: &Expr,
        args: &[Expr],
        line: u32,
        column: u32,
    ) -> Result<(), CompileError> {
        let argc = args.len() as u8;

        // `Math.sin(x)` は組み込みの `sin(x)` と同じものへ読み替える。
        if let Expr::Member { object, name } = callee
            && is_math(object)
            && let Some(native) = natives::resolve(math_name(name), argc)
        {
            for arg in args {
                self.expression(arg)?;
            }
            self.code.push(Op::CallNative(native, argc));
            return Ok(());
        }

        // `$.map(f)` のようなメソッド呼び出し。
        if let Expr::Member { object, name } = callee {
            let key = self.key(name);
            self.expression(object)?;
            for arg in args {
                self.expression(arg)?;
            }
            self.code.push(Op::CallMethod(key, argc));
            return Ok(());
        }

        // 名前が API そのものなら、直接呼ぶ。
        if let Expr::Ident(name) = callee
            && !self.globals.contains_key(name)
            && !self.params.contains_key(name)
        {
            if let Some(native) = natives::resolve(name, argc) {
                for arg in args {
                    self.expression(arg)?;
                }
                self.code.push(Op::CallNative(native, argc));
                return Ok(());
            }
            if natives::is_native(name) && !natives::is_variadic(name) {
                let arities = natives::accepted_arities(name)
                    .iter()
                    .map(u8::to_string)
                    .collect::<Vec<_>>()
                    .join(" か ");
                return Err(CompileError::new(
                    line,
                    column,
                    format!("{name} は引数 {arities} 個で呼びます ({argc} 個渡されています)"),
                ));
            }
        }

        // それ以外は値として呼ぶ。
        self.expression(callee)?;
        for arg in args {
            self.expression(arg)?;
        }
        self.code.push(Op::CallValue(argc));
        Ok(())
    }

    // ---- 変数 -----------------------------------------------------------

    fn load(&mut self, name: &str) {
        if let Some(slot) = self.params.get(name) {
            self.code.push(Op::LoadLocal(*slot));
            return;
        }
        if let Some(builtin) = BuiltinVar::resolve(name) {
            self.code.push(Op::LoadBuiltin(builtin));
            return;
        }
        // 未知の名前でも、API 名なら関数値として読める (`B = blendMode`)。
        if !self.globals.contains_key(name)
            && let Some(native) = natives::resolve_any(name)
        {
            self.code.push(Op::ConstNativeFn(native));
            return;
        }
        let slot = self.global(name);
        self.code.push(Op::LoadGlobal(slot));
    }

    /// 値を書き込み、その値をスタックに残す。
    fn store(&mut self, name: &str) {
        self.code.push(Op::Dup);
        if let Some(slot) = self.params.get(name) {
            self.code.push(Op::StoreLocal(*slot));
        } else {
            let slot = self.global(name);
            self.code.push(Op::StoreGlobal(slot));
        }
    }

    // ---- 補助 -----------------------------------------------------------

    fn emit_jump(&mut self, op: Op) -> usize {
        self.code.push(op);
        self.code.len() - 1
    }

    fn patch(&mut self, index: usize) {
        let target = self.code.len() as u32;
        self.patch_to(index, target);
    }

    /// ジャンプ命令の飛び先を指定の位置に決める。
    fn patch_to(&mut self, index: usize, target: u32) {
        self.code[index] = match self.code[index] {
            Op::Jump(_) => Op::Jump(target),
            Op::JumpIfFalse(_) => Op::JumpIfFalse(target),
            Op::JumpIfTrue(_) => Op::JumpIfTrue(target),
            other => unreachable!("ジャンプ命令ではない: {other:?}"),
        };
    }
}

/// `Math` そのものを指しているか。
fn is_math(expr: &Expr) -> bool {
    matches!(expr, Expr::Ident(name) if name == "Math")
}

/// `Math.foo` の `foo` を、このランタイムの API 名へ直す。
///
/// 名前が違うものだけを並べる。`sin` `cos` のように同じものはそのまま通す。
fn math_name(name: &str) -> &str {
    match name {
        "random" => "random",
        "trunc" => "int",
        "cbrt" => "cbrt",
        other => other,
    }
}

/// 二項演算子から命令へ。
fn binary_op(op: BinaryOp) -> Op {
    match op {
        BinaryOp::Add => Op::Add,
        BinaryOp::Sub => Op::Sub,
        BinaryOp::Mul => Op::Mul,
        BinaryOp::Div => Op::Div,
        BinaryOp::Rem => Op::Rem,
        BinaryOp::Eq => Op::Eq,
        BinaryOp::Ne => Op::Ne,
        BinaryOp::Lt => Op::Lt,
        BinaryOp::Le => Op::Le,
        BinaryOp::Gt => Op::Gt,
        BinaryOp::Ge => Op::Ge,
        BinaryOp::BitAnd => Op::BitAnd,
        BinaryOp::BitOr => Op::BitOr,
        BinaryOp::BitXor => Op::BitXor,
        BinaryOp::Shl => Op::Shl,
        BinaryOp::Shr => Op::Shr,
        BinaryOp::UShr => Op::UShr,
    }
}

