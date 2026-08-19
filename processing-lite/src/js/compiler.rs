//! p5.js subset の Bytecode コンパイラ。
//!
//! # 変数のあつかい
//!
//! **引数と、その関数の中でしか使われない `let` / `const` / `var` がローカル**で、
//! それ以外はすべてグローバル。つぶやき作品はほぼ全部の変数をグローバルに置くので、
//! これで足りる。クロージャは無い — 入れ子の関数から見えるのは、自分の引数と
//! グローバルだけ。
//!
//! `let` をローカルにするのは、再帰する関数がループ変数を壊さないようにするため。
//! 外から見える名前 (入れ子の関数や別の関数が触る名前) はグローバルのままにして、
//! 「グローバルをクロージャの代わりに使う」書き方が動き続けるようにしている。
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

/// 途中の値の置き場。関数の中はローカル、トップレベルはグローバル。
#[derive(Clone, Copy)]
enum Slot {
    Local(u16),
    Global(u16),
}

impl Slot {
    fn load(self) -> Op {
        match self {
            Slot::Local(slot) => Op::LoadLocal(slot),
            Slot::Global(slot) => Op::LoadGlobal(slot),
        }
    }

    fn store(self) -> Op {
        match self {
            Slot::Local(slot) => Op::StoreLocal(slot),
            Slot::Global(slot) => Op::StoreGlobal(slot),
        }
    }
}

/// `break` / `continue` の飛び先を集める入れ物。飛び先は組み終えてから埋める。
///
/// `switch` も `break` を受けるのでここへ積むが、`continue` は素通りして
/// 外側のループへ行く。それを見分けるのが [`Self::is_loop`]。
#[derive(Default)]
struct LoopCtx {
    /// ループなら真。`switch` なら偽。
    is_loop: bool,
    breaks: Vec<usize>,
    continues: Vec<usize>,
}

impl LoopCtx {
    fn loop_body() -> Self {
        Self { is_loop: true, ..Self::default() }
    }
}

struct Compiler {
    globals: HashMap<String, u16>,
    keys: HashMap<String, u16>,
    functions: Vec<CompiledFunction>,

    /// 現在の関数のローカル。引数と、この関数の外に出ない `let` が入る。
    /// ここに無い名前はグローバル。
    params: HashMap<String, u16>,
    /// 名前がプログラム全体で何回現れるか。ローカルにしてよいかの判定に使う。
    uses: HashMap<String, usize>,
    /// いま関数の本体を組んでいるか。トップレベルなら false。
    in_function: bool,
    /// 現在の関数が使うローカルの数。隠し変数を足すたびに増える。
    local_slots: u16,
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
            uses: HashMap::new(),
            in_function: false,
            local_slots: 0,
            code: Vec::new(),
            strings: Vec::new(),
            hidden: 0,
            loops: Vec::new(),
        }
    }

    fn run(mut self, script: &Script) -> Result<Program, CompileError> {
        // どの名前がどこまで届いているかを先に数える。関数の中で閉じている
        // 名前だけをローカルにするための材料。
        let mut names = Names::new(true);
        names.statements(&script.statements);
        self.uses = names.counts;

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
        self.push_function_with_locals(name, arity, arity as u16, code)
    }

    fn push_function_with_locals(
        &mut self,
        name: &str,
        arity: u8,
        local_count: u16,
        code: Vec<Op>,
    ) -> u16 {
        let index = self.functions.len() as u16;
        self.functions.push(CompiledFunction {
            name: name.to_string(),
            arity,
            local_count,
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

    /// 途中の値を置く場所を 1 つ作る。
    ///
    /// 関数の中ならローカルに取る。グローバルに置くと、退避してから読み直す
    /// までのあいだに同じ関数が再帰で入ってきたとき、上書きされてしまう
    /// (`for...of` の走査位置がその典型)。
    fn hidden_slot(&mut self, what: &str) -> Slot {
        if !self.in_function {
            return Slot::Global(self.hidden_global(what));
        }
        let slot = self.local_slots;
        self.local_slots += 1;
        Slot::Local(slot)
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

                self.loops.push(LoopCtx::loop_body());
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
                self.loops.push(LoopCtx::loop_body());
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

            // 振り分けだけを先に並べ、中身はそのあとへ続けて置く。break が
            // 無ければ次の case へ落ちるのは JavaScript も Java と同じ。
            Stmt::Switch { value, cases, .. } => {
                let slot = self.hidden_slot("switch.value");
                self.expression(value)?;
                self.code.push(slot.store());

                let mut entries = Vec::new();
                for case in cases {
                    let Some(label) = &case.label else { continue };
                    self.code.push(slot.load());
                    self.expression(label)?;
                    self.code.push(Op::Eq);
                    entries.push(self.emit_jump(Op::JumpIfTrue(0)));
                }
                let to_default = self.emit_jump(Op::Jump(0));

                // break の行き先を集めるためにループとして積む。continue は
                // switch では止まらないので、外側のループへ渡す。
                self.loops.push(LoopCtx::default());
                let mut default_at = None;
                let mut entry = 0;
                for case in cases {
                    match case.label {
                        Some(_) => {
                            let at = entries[entry];
                            let here = self.code.len() as u32;
                            self.patch_to(at, here);
                            entry += 1;
                        }
                        None => default_at = Some(self.code.len() as u32),
                    }
                    for stmt in &case.body {
                        self.statement(stmt)?;
                    }
                }
                let ctx = self.loops.pop().expect("直前に積んだ");

                let end = self.code.len() as u32;
                self.patch_to(to_default, default_at.unwrap_or(end));
                for at in ctx.breaks {
                    self.patch_to(at, end);
                }
            }

            Stmt::Break { line, column } => {
                if self.loops.is_empty() {
                    return Err(CompileError::new(
                        *line,
                        *column,
                        "break はループか switch の中でしか使えません".to_string(),
                    ));
                }
                let at = self.emit_jump(Op::Jump(0));
                self.loops.last_mut().expect("空でないと確認済み").breaks.push(at);
            }

            Stmt::Continue { line, column } => {
                // switch は素通りする。JavaScript の continue はループにしか効かない。
                if !self.loops.iter().any(|c| c.is_loop) {
                    return Err(CompileError::new(
                        *line,
                        *column,
                        "continue はループの中でしか使えません".to_string(),
                    ));
                }
                let at = self.emit_jump(Op::Jump(0));
                self.loops
                    .iter_mut()
                    .rev()
                    .find(|c| c.is_loop)
                    .expect("あると確認済み")
                    .continues
                    .push(at);
            }

            // `for (v of xs)`。配列と添字を隠し変数に置いて回す。
            Stmt::ForOf { name, declared: _, iterable, body } => {
                let array = self.hidden_slot("forof.array");
                let index = self.hidden_slot("forof.index");

                self.expression(iterable)?;
                self.code.push(array.store());
                self.code.push(Op::ConstInt(0));
                self.code.push(index.store());

                let start = self.code.len() as u32;
                self.code.push(index.load());
                self.code.push(array.load());
                self.code.push(Op::ArrayLen);
                self.code.push(Op::Lt);
                let jump_end = self.emit_jump(Op::JumpIfFalse(0));

                self.code.push(array.load());
                self.code.push(index.load());
                self.code.push(Op::GetIndex);
                // `let` で作られた名前ならローカルへ書く。store() が見分ける。
                self.store(name);
                self.code.push(Op::Pop);

                self.loops.push(LoopCtx::loop_body());
                self.statement(body)?;
                let ctx = self.loops.pop().expect("直前に積んだ");

                let update_at = self.code.len() as u32;
                for at in ctx.continues {
                    self.patch_to(at, update_at);
                }
                self.code.push(index.load());
                self.code.push(Op::ConstInt(1));
                self.code.push(Op::Add);
                self.code.push(index.store());
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
        let outer_slots = self.local_slots;
        let outer_in_function = self.in_function;
        self.in_function = true;

        for (slot, param) in params.iter().enumerate() {
            self.params.insert(param.name.clone(), slot as u16);
        }

        self.local_slots = params.len() as u16;
        for local in self.locals_of(body) {
            self.params.insert(local, self.local_slots);
            self.local_slots += 1;
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
        let slots = std::mem::replace(&mut self.local_slots, outer_slots);
        self.in_function = outer_in_function;
        Ok(self.push_function_with_locals(name, params.len() as u8, slots, code))
    }

    /// この関数の中で閉じている宣言を選ぶ。
    ///
    /// `let` で作られ、かつプログラムのどこからも外から触られない名前だけを
    /// ローカルにする。判定はとても素朴で、名前の現れる回数が「この関数が
    /// 自分で書いている数」と「プログラム全体の数」で一致するかどうかを見る。
    /// 入れ子の関数から見えている名前は数が合わないので、グローバルに残る。
    fn locals_of(&self, body: &[Stmt]) -> Vec<String> {
        let mut names = Names::new(false);
        names.statements(body);

        let mut locals = Vec::new();
        for name in &names.declared {
            // 引数と同じ名前は引数のまま。JavaScript でも同じ変数になる。
            if self.params.contains_key(name) || locals.contains(name) {
                continue;
            }
            if names.counts.get(name) == self.uses.get(name) {
                locals.push(name.clone());
            }
        }
        locals
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
                // `$.map(p5.Vector.random3D)` のように、呼ばずに値として
                // 渡す書き方もある。
                if let Some(native) = self.vector_static(object, name) {
                    self.code.push(Op::ConstNativeFn(native));
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

            // `...xs` は引数の並びの中にしか書けない。
            Expr::Spread(_) => {
                return Err(CompileError::new(0, 0, "... はここには書けません".to_string()));
            }
        }
        Ok(())
    }

    /// 代入。JavaScript の代入は式なので、書いた値をスタックに残す。
    fn assign(&mut self, target: &Target, op: AssignOp, value: &Expr) -> Result<(), CompileError> {
        match target {
            // `[a, b] = xs`。右辺を隠し変数へ置いてから、順に取り出す。
            // `[a, b] = [b, a]` の入れ替えが期待どおり動くのはこのため。
            Target::Destructure(targets) => {
                let temp = self.hidden_slot("destructure");
                self.expression(value)?;
                self.code.push(temp.store());

                for (index, slot) in targets.iter().enumerate() {
                    let Some(slot) = slot else { continue };
                    self.code.push(temp.load());
                    self.code.push(Op::ConstInt(index as i32));
                    self.code.push(Op::GetIndex);
                    self.store_top(slot)?;
                    self.code.push(Op::Pop);
                }
                // 代入は式なので、右辺そのものを残す。
                self.code.push(temp.load());
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
            AssignOp::Pow => BinaryOp::Pow,
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

    /// `p5.Vector.random3D` のような静的メソッドを組み込みへ読み替える。
    ///
    /// `p5` も `p5.Vector` も値としては持っていない。名前の並びだけを見て、
    /// 対応する組み込みへ差し替える。`Math.sin` と同じやり方。
    fn vector_static(&self, object: &Expr, name: &str) -> Option<natives::Native> {
        let Expr::Member { object, name: class } = object else { return None };
        if class != "Vector" || !is_namespace(object, "p5") || self.globals.contains_key("p5") {
            return None;
        }
        natives::resolve_any(&format!("p5.Vector.{name}"))
    }

    /// スタックトップの値を的へ書き込む。書いた値はスタックに残る。
    fn store_top(&mut self, target: &Target) -> Result<(), CompileError> {
        match target {
            Target::Var(name) => self.store(name),
            Target::Member(object, name) => {
                let key = self.key(name);
                // `[obj, value]` の順に積む必要があるので、値を退避してから
                // オブジェクトを積み直す。
                let temp = self.hidden_slot("store");
                self.code.push(temp.store());
                self.expression(object)?;
                self.code.push(temp.load());
                self.code.push(Op::SetProp(key));
            }
            Target::Index(object, index) => {
                let temp = self.hidden_slot("store");
                self.code.push(temp.store());
                self.expression(object)?;
                self.expression(index)?;
                self.code.push(temp.load());
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

    /// `f(...xs)` のような、引数の個数が実行時まで決まらない呼び出し。
    ///
    /// 引数をひとつの配列に組み立ててから渡す。組み立て方は配列リテラルの
    /// 展開と同じ。
    fn spread_call(&mut self, callee: &Expr, args: &[Expr]) -> Result<(), CompileError> {
        // `Math.max(...xs)` は組み込みへ読み替える。`Math` は実体を持たない
        // ので、メソッド呼び出しにすると受け手が無くて落ちる。
        if let Expr::Member { object, name } = callee
            && (is_math(object) || is_namespace(object, "String"))
            && let Some(native) = natives::resolve_by_name(math_name(name))
        {
            self.spread_args(args)?;
            self.code.push(Op::CallNativeSpread(native));
            return Ok(());
        }

        // 受け手や呼ぶ相手は引数の配列より先に積む。
        let target = match callee {
            Expr::Member { object, name } => {
                let key = self.key(name);
                self.expression(object)?;
                Some(key)
            }
            Expr::Ident(name)
                if !self.globals.contains_key(name)
                    && !self.params.contains_key(name)
                    && natives::is_native(name) =>
            {
                None
            }
            _ => {
                self.expression(callee)?;
                Some(u16::MAX)
            }
        };

        self.spread_args(args)?;

        match (target, callee) {
            (None, Expr::Ident(name)) => {
                let native = natives::resolve_by_name(name)
                    .ok_or_else(|| CompileError::new(0, 0, format!("{name} は呼べません")))?;
                self.code.push(Op::CallNativeSpread(native));
            }
            (Some(key), Expr::Member { .. }) => self.code.push(Op::CallMethodSpread(key)),
            _ => self.code.push(Op::CallValueSpread),
        }
        Ok(())
    }

    /// 引数の並びを 1 本の配列に組み立てる。
    fn spread_args(&mut self, args: &[Expr]) -> Result<(), CompileError> {
        self.code.push(Op::NewArray(0));
        for arg in args {
            match arg {
                Expr::Spread(inner) => {
                    self.expression(inner)?;
                    self.code.push(Op::ArrayExtend);
                }
                other => {
                    self.expression(other)?;
                    self.code.push(Op::ArrayPush);
                }
            }
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
        // `f(...xs)`。個数が実行時まで決まらないので、引数を配列にまとめて渡す。
        if args.iter().any(|a| matches!(a, Expr::Spread(_))) {
            return self.spread_call(callee, args);
        }
        let argc = args.len() as u8;

        // `Math.sin(x)` と `String.fromCodePoint(n)` は組み込みへ読み替える。
        if let Expr::Member { object, name } = callee
            && (is_math(object) || is_namespace(object, "String"))
            && let Some(native) = natives::resolve(math_name(name), argc)
        {
            for arg in args {
                self.expression(arg)?;
            }
            self.code.push(Op::CallNative(native, argc));
            return Ok(());
        }

        // `p5.Vector.add(a, b)` のような静的メソッド。`p5.Vector` という値は
        // 持っていないので、メソッド呼び出しへ回す前にここで受ける。
        if let Expr::Member { object, name } = callee
            && let Some(native) = self.vector_static(object, name)
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
            // 多すぎる引数は捨てる。JavaScript は余った引数を無視するので、
            // `noFill(H = W / 2)` のように、呼び出しを代入の置き場にした
            // つぶやき作品がある。評価だけはしないと代入が起きない。
            if let Some(&max) = natives::accepted_arities(name).iter().max()
                && argc > max
                && let Some(native) = natives::resolve(name, max)
            {
                for arg in args {
                    self.expression(arg)?;
                }
                for _ in max..argc {
                    self.code.push(Op::Pop);
                }
                self.code.push(Op::CallNative(native, max));
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

        // それ以外は値として呼ぶ。名前で呼んでいるなら、その名前も覚えておく。
        // 中身が空だったときに「何が無いのか」を言えるのはここだけ。
        self.expression(callee)?;
        for arg in args {
            self.expression(arg)?;
        }
        match callee {
            Expr::Ident(name) => {
                let index = self.intern(name);
                self.code.push(Op::CallNamed(index, argc));
            }
            _ => self.code.push(Op::CallValue(argc)),
        }
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
    is_namespace(expr, "Math")
}

/// `Math` や `String` のような、まとめ役の名前を指しているか。
fn is_namespace(expr: &Expr, name: &str) -> bool {
    matches!(expr, Expr::Ident(n) if n == name)
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
        BinaryOp::Pow => Op::Pow,
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


/// 構文木に現れる名前を数える。
///
/// 同じ数え方で「関数が自分で書いている数」と「プログラム全体の数」を取り、
/// 一致すればその名前はその関数の外へ出ていない、と判断する
/// ([`Compiler::locals_of`])。両者で数え方がずれると判断も狂うので、
/// 歩き方の違いは [`Self::into_functions`] だけにしてある。
struct Names {
    /// 入れ子の関数の中まで数えるか。
    into_functions: bool,
    counts: HashMap<String, usize>,
    /// `let` / `const` / `var` と `for (let v of ...)` が作った名前。
    declared: Vec<String>,
}

impl Names {
    fn new(into_functions: bool) -> Self {
        Self { into_functions, counts: HashMap::new(), declared: Vec::new() }
    }

    fn see(&mut self, name: &str) {
        *self.counts.entry(name.to_string()).or_default() += 1;
    }

    fn declare(&mut self, name: &str) {
        self.see(name);
        self.declared.push(name.to_string());
    }

    fn statements(&mut self, statements: &[Stmt]) {
        for stmt in statements {
            self.statement(stmt);
        }
    }

    fn statement(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::Expr(e) => self.expression(e),
            Stmt::Declare(bindings) => {
                for (name, value) in bindings {
                    self.declare(name);
                    if let Some(value) = value {
                        self.expression(value);
                    }
                }
            }
            Stmt::If { cond, then, otherwise } => {
                self.expression(cond);
                self.statement(then);
                if let Some(otherwise) = otherwise {
                    self.statement(otherwise);
                }
            }
            Stmt::For { init, cond, update, body } => {
                if let Some(init) = init {
                    self.statement(init);
                }
                if let Some(cond) = cond {
                    self.expression(cond);
                }
                if let Some(update) = update {
                    self.expression(update);
                }
                self.statement(body);
            }
            Stmt::While { cond, body } => {
                self.expression(cond);
                self.statement(body);
            }
            Stmt::ForOf { name, declared, iterable, body } => {
                // `for (v of xs)` は宣言ではない。JavaScript でもグローバルを書く。
                if *declared {
                    self.declare(name);
                } else {
                    self.see(name);
                }
                self.expression(iterable);
                self.statement(body);
            }
            Stmt::Switch { value, cases, .. } => {
                self.expression(value);
                for case in cases {
                    if let Some(label) = &case.label {
                        self.expression(label);
                    }
                    self.statements(&case.body);
                }
            }
            Stmt::Return(value) => {
                if let Some(value) = value {
                    self.expression(value);
                }
            }
            Stmt::Break { .. } | Stmt::Continue { .. } => {}
            Stmt::Block(statements) => self.statements(statements),
            Stmt::Function { name, params, body, .. } => {
                // 名前はこの場所へ書かれる。中身は入れ子の関数のもの。
                self.see(name);
                if self.into_functions {
                    self.function(params, body);
                }
            }
        }
    }

    fn function(&mut self, params: &[Param], body: &[Stmt]) {
        for param in params {
            if let Some(default) = &param.default {
                self.expression(default);
            }
        }
        self.statements(body);
    }

    fn expression(&mut self, expr: &Expr) {
        match expr {
            Expr::Str(_) | Expr::Number(_) | Expr::Bool(_) | Expr::Undefined => {}
            Expr::Ident(name) => self.see(name),
            Expr::Array(items) => {
                for item in items {
                    match item {
                        ArrayElem::Item(e) | ArrayElem::Spread(e) => self.expression(e),
                    }
                }
            }
            Expr::Object(fields) => {
                for (_, value) in fields {
                    self.expression(value);
                }
            }
            Expr::Arrow { params, body } => {
                if self.into_functions {
                    match body.as_ref() {
                        ArrowBody::Expr(e) => {
                            for param in params {
                                if let Some(default) = &param.default {
                                    self.expression(default);
                                }
                            }
                            self.expression(e);
                        }
                        ArrowBody::Block(statements) => self.function(params, statements),
                    }
                }
            }
            Expr::Unary { operand, .. } => self.expression(operand),
            Expr::Binary { lhs, rhs, .. } | Expr::Logical { lhs, rhs, .. } => {
                self.expression(lhs);
                self.expression(rhs);
            }
            Expr::Ternary { cond, then, other } => {
                self.expression(cond);
                self.expression(then);
                self.expression(other);
            }
            Expr::Assign { target, value, .. } => {
                self.target(target);
                self.expression(value);
            }
            Expr::Update { target, .. } => self.target(target),
            Expr::Spread(e) => self.expression(e),
            Expr::Call { callee, args, .. } => {
                self.expression(callee);
                for arg in args {
                    self.expression(arg);
                }
            }
            Expr::Member { object, .. } => self.expression(object),
            Expr::Index { object, index } => {
                self.expression(object);
                self.expression(index);
            }
            Expr::Sequence(a, b) => {
                self.expression(a);
                self.expression(b);
            }
        }
    }

    fn target(&mut self, target: &Target) {
        match target {
            Target::Var(name) => self.see(name),
            Target::Member(object, _) => self.expression(object),
            Target::Index(object, index) => {
                self.expression(object);
                self.expression(index);
            }
            Target::Destructure(parts) => {
                for part in parts.iter().flatten() {
                    self.target(part);
                }
            }
        }
    }
}
