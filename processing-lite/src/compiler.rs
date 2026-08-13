//! Bytecode Compiler (設計書 §13 / §15.1)。
//!
//! 保存時に一度だけ走らせ、以降は Bytecode だけを実行する。名前解決もここで
//! 済ませるので、VM は文字列を一切扱わない。

use std::collections::HashMap;

use crate::ast::*;
use crate::bytecode::{CompiledFunction, Op, Program};
use crate::lexer::CompileError;
use crate::natives::{self, BuiltinVar};

pub fn compile(ast: &Ast) -> Result<Program, CompileError> {
    Compiler::new(ast)?.run()
}

/// スコープ内の変数 1 つ。
#[derive(Clone, Copy)]
struct VarSlot {
    slot: u16,
    ty: Type,
}

struct FunctionSig {
    index: u16,
    arity: u8,
    line: u32,
    column: u32,
}

/// 生成に必要なクラスの情報。
struct ClassInfo {
    fields: Vec<(Type, String)>,
    /// メソッド名と、それを実装する関数の位置。
    methods: Vec<(String, u16)>,
    constructor: Option<u16>,
}

struct Compiler<'a> {
    ast: &'a Ast,
    functions: HashMap<String, FunctionSig>,
    globals: HashMap<String, VarSlot>,

    /// 現在コンパイル中の関数のローカル。内側ほど後ろ。
    scopes: Vec<HashMap<String, VarSlot>>,
    next_local: u16,
    max_locals: u16,
    code: Vec<Op>,
    /// プロパティ名の表。`v.x` のような参照で使う。
    keys: Vec<String>,
    /// 文字列リテラルの表。
    strings: Vec<String>,
    /// いまコンパイル中のクラスのフィールド名。裸の `x` を `this.x` と読む。
    fields: Vec<String>,
    /// クラスごとの情報。`new` で使う。
    classes: HashMap<String, ClassInfo>,
    /// 入れ子になったループ。`break` と `continue` の飛び先を集める。
    loops: Vec<LoopCtx>,
}

/// `break` を受け止める入れ物。ループと `switch` が積む。
///
/// `break` は末尾へ、`continue` は「次の回」へ飛ぶ。どちらも飛び先が確定する
/// のは組み終えてからなので、位置だけ覚えておいて後から埋める。
#[derive(Default)]
struct LoopCtx {
    breaks: Vec<usize>,
    continues: Vec<usize>,
    /// `continue` を受けるか。`switch` は受けず、外側のループへ通す。
    takes_continue: bool,
}

impl LoopCtx {
    fn loop_body() -> Self {
        Self { takes_continue: true, ..Self::default() }
    }
}

impl<'a> Compiler<'a> {
    fn new(ast: &'a Ast) -> Result<Self, CompileError> {
        let mut functions = HashMap::new();
        for (index, f) in ast.functions.iter().enumerate() {
            if natives::is_native(&f.name) || BuiltinVar::resolve(&f.name).is_some() {
                return Err(CompileError::new(
                    f.line,
                    f.column,
                    format!("{} は Processing Lite の組み込み名なので再定義できません", f.name),
                ));
            }
            let sig = FunctionSig {
                index: index as u16,
                arity: f.params.len() as u8,
                line: f.line,
                column: f.column,
            };
            if let Some(previous) = functions.insert(f.name.clone(), sig) {
                return Err(CompileError::new(
                    f.line,
                    f.column,
                    format!("関数 {} は {} 行目で既に定義されています", f.name, previous.line),
                ));
            }
        }

        Ok(Self {
            ast,
            functions,
            globals: HashMap::new(),
            scopes: Vec::new(),
            next_local: 0,
            max_locals: 0,
            code: Vec::new(),
            keys: Vec::new(),
            strings: Vec::new(),
            fields: Vec::new(),
            classes: HashMap::new(),
            loops: Vec::new(),
        })
    }

    fn run(mut self) -> Result<Program, CompileError> {
        // グローバルはトップレベルの宣言順に slot を振る。
        let mut globals_init = Vec::new();
        for stmt in &self.ast.globals {
            let Stmt::VarDecl { ty, name, init, line, column } = stmt else {
                unreachable!("パーサはトップレベルに宣言以外を作らない");
            };
            if self.globals.contains_key(name) || BuiltinVar::resolve(name).is_some() {
                return Err(CompileError::new(
                    *line,
                    *column,
                    format!("変数 {name} が重複しています"),
                ));
            }
            let slot = self.globals.len() as u16;
            self.globals.insert(name.clone(), VarSlot { slot, ty: *ty });

            std::mem::swap(&mut self.code, &mut globals_init);
            match init {
                Some(expr) => self.expression(expr)?,
                None => self.emit_default(*ty),
            }
            self.code.push(Op::Coerce(*ty));
            self.code.push(Op::StoreGlobal(slot));
            std::mem::swap(&mut self.code, &mut globals_init);
        }

        let global_count = self.globals.len() as u16;

        // クラスのメソッドは、`this` を第 1 引数に足した普通の関数にする。
        //
        // 位置決めを本体作りより先に済ませる。`draw()` の中で `new P()` と書ける
        // ようにするには、関数を組む前にクラスを知っている必要がある。
        let mut pending = Vec::new();
        let base = self.ast.functions.len() as u16;
        for class in &self.ast.classes {
            let mut methods = Vec::new();
            let mut constructor = None;
            if let Some(f) = &class.constructor {
                constructor = Some(base + pending.len() as u16);
                pending.push((class.clone(), f.clone()));
            }
            for f in &class.methods {
                methods.push((f.name.clone(), base + pending.len() as u16));
                pending.push((class.clone(), f.clone()));
            }
            let info = ClassInfo { fields: class.fields.clone(), methods, constructor };
            if self.classes.insert(class.name.clone(), info).is_some() {
                return Err(CompileError::new(
                    class.line,
                    class.column,
                    format!("クラス {} が重複しています", class.name),
                ));
            }
        }

        let mut functions = Vec::with_capacity(self.ast.functions.len() + pending.len() + 1);
        for f in &self.ast.functions {
            functions.push(self.function(f)?);
        }
        for (class, f) in &pending {
            functions.push(self.method(class, f)?);
        }

        // グローバル初期化も関数として持たせる。名前に使えない文字を入れて、
        // ユーザーの関数と衝突しないようにする。
        globals_init.push(Op::Return);
        let globals_init_index = functions.len() as u16;
        functions.push(CompiledFunction {
            name: "<globals>".into(),
            arity: 0,
            local_count: 0,
            return_type: Type::Void,
            code: globals_init,
        });

        let entry = |name: &str| self.functions.get(name).map(|s| s.index);
        if let Some(setup) = self.functions.get("setup")
            && setup.arity != 0
        {
            return Err(CompileError::new(setup.line, setup.column, "setup() に引数は書けません"));
        }
        if let Some(draw) = self.functions.get("draw")
            && draw.arity != 0
        {
            return Err(CompileError::new(draw.line, draw.column, "draw() に引数は書けません"));
        }

        let program = Program {
            setup: entry("setup"),
            draw: entry("draw"),
            functions,
            keys: self.keys.clone(),
            strings: self.strings.clone(),
            global_names: self.globals.iter().map(|(n, v)| (n.clone(), v.slot)).collect(),
            globals_init: globals_init_index,
            global_count,
        };

        if program.setup.is_none() && program.draw.is_none() {
            return Err(CompileError::new(1, 1, "setup() か draw() のどちらかが必要です"));
        }

        Ok(program)
    }

    /// クラスのメソッドを関数として組む。第 1 引数は `this`。
    fn method(&mut self, class: &Class, f: &Function) -> Result<CompiledFunction, CompileError> {
        let mut with_this = f.clone();
        with_this.params.insert(0, (Type::Instance, "this".to_string()));
        // 名前を分けておく。診断や重複検査で普通の関数と混ざらないようにする。
        with_this.name = format!("{}.{}", class.name, f.name);

        self.fields = class.fields.iter().map(|(_, n)| n.clone()).collect();
        let compiled = self.function(&with_this);
        self.fields.clear();
        compiled
    }

    fn function(&mut self, f: &Function) -> Result<CompiledFunction, CompileError> {
        self.scopes.clear();
        self.scopes.push(HashMap::new());
        self.next_local = 0;
        self.max_locals = 0;
        self.code = Vec::new();

        for (ty, name) in &f.params {
            self.declare_local(name, *ty, f.line, f.column)?;
        }

        for stmt in &f.body {
            self.statement(stmt)?;
        }

        // 最後まで到達したときのための暗黙の return。
        if f.return_type == Type::Void {
            self.code.push(Op::Return);
        } else {
            self.emit_default(f.return_type);
            self.code.push(Op::ReturnValue);
        }

        Ok(CompiledFunction {
            name: f.name.clone(),
            arity: f.params.len() as u8,
            local_count: self.max_locals,
            return_type: f.return_type,
            code: std::mem::take(&mut self.code),
        })
    }

    // ---- スコープ -------------------------------------------------------

    fn declare_local(
        &mut self,
        name: &str,
        ty: Type,
        line: u32,
        column: u32,
    ) -> Result<u16, CompileError> {
        if BuiltinVar::resolve(name).is_some() {
            return Err(CompileError::new(
                line,
                column,
                format!("{name} は組み込み変数なので上書きできません"),
            ));
        }
        let scope = self.scopes.last_mut().expect("スコープは必ず 1 つ以上ある");
        if scope.contains_key(name) {
            return Err(CompileError::new(line, column, format!("変数 {name} が重複しています")));
        }
        let slot = self.next_local;
        self.next_local += 1;
        self.max_locals = self.max_locals.max(self.next_local);
        scope.insert(name.to_string(), VarSlot { slot, ty });
        Ok(slot)
    }

    fn lookup(&self, name: &str) -> Option<(VarSlot, bool)> {
        for scope in self.scopes.iter().rev() {
            if let Some(v) = scope.get(name) {
                return Some((*v, true));
            }
        }
        self.globals.get(name).map(|v| (*v, false))
    }

    fn push_scope(&mut self) {
        self.scopes.push(HashMap::new());
    }

    fn pop_scope(&mut self) {
        let scope = self.scopes.pop().expect("push と対で呼ぶ");
        // 抜けたブロックの slot は再利用する。
        self.next_local -= scope.len() as u16;
    }

    // ---- 文 -------------------------------------------------------------

    fn statement(&mut self, stmt: &Stmt) -> Result<(), CompileError> {
        match stmt {
            Stmt::VarDecl { ty, name, init, line, column } => {
                match init {
                    Some(expr) => self.expression(expr)?,
                    None => self.emit_default(*ty),
                }
                self.code.push(Op::Coerce(*ty));
                let slot = self.declare_local(name, *ty, *line, *column)?;
                self.code.push(Op::StoreLocal(slot));
            }

            // メソッドの中では、フィールドへの代入は `this.x = ...` の意味。
            Stmt::Assign { name, op, value, .. }
                if self.fields.contains(name) && self.lookup(name).is_none() =>
            {
                let key = self.key(name);
                self.code.push(Op::LoadLocal(0));
                if let Some(binary) = op.binary() {
                    self.code.push(Op::Dup);
                    self.code.push(Op::GetProp(key));
                    self.expression(value)?;
                    self.code.push(binary_op(binary));
                } else {
                    self.expression(value)?;
                }
                self.code.push(Op::SetProp(key));
                self.code.push(Op::Pop);
            }

            Stmt::IncDec { name, delta, .. }
                if self.fields.contains(name) && self.lookup(name).is_none() =>
            {
                let key = self.key(name);
                self.code.push(Op::LoadLocal(0));
                self.code.push(Op::Dup);
                self.code.push(Op::GetProp(key));
                self.code.push(Op::ConstInt(*delta));
                self.code.push(Op::Add);
                self.code.push(Op::SetProp(key));
                self.code.push(Op::Pop);
            }

            Stmt::Assign { name, op, value, line, column } => {
                let (var, is_local) = self.lookup(name).ok_or_else(|| {
                    CompileError::new(*line, *column, format!("変数 {name} が見つかりません"))
                })?;

                if *op != AssignOp::Set {
                    self.code.push(if is_local {
                        Op::LoadLocal(var.slot)
                    } else {
                        Op::LoadGlobal(var.slot)
                    });
                }
                self.expression(value)?;
                if let Some(binary) = op.binary() {
                    self.code.push(binary_op(binary));
                }
                self.code.push(Op::Coerce(var.ty));
                self.code.push(if is_local {
                    Op::StoreLocal(var.slot)
                } else {
                    Op::StoreGlobal(var.slot)
                });
            }

            Stmt::IncDec { name, delta, line, column } => {
                let (var, is_local) = self.lookup(name).ok_or_else(|| {
                    CompileError::new(*line, *column, format!("変数 {name} が見つかりません"))
                })?;
                self.code.push(if is_local {
                    Op::LoadLocal(var.slot)
                } else {
                    Op::LoadGlobal(var.slot)
                });
                self.code.push(Op::ConstInt(*delta));
                self.code.push(Op::Add);
                self.code.push(Op::Coerce(var.ty));
                self.code.push(if is_local {
                    Op::StoreLocal(var.slot)
                } else {
                    Op::StoreGlobal(var.slot)
                });
            }

            Stmt::Block(stmts) => {
                self.push_scope();
                for s in stmts {
                    self.statement(s)?;
                }
                self.pop_scope();
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
                // 初期化した変数は for の中だけで生きる。
                self.push_scope();
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

                // continue は更新式へ飛ぶ。飛ばしてしまうと `i++` が実行されず
                // 無限ループになる。
                let update_at = self.code.len() as u32;
                for at in ctx.continues {
                    self.patch_to(at, update_at);
                }
                if let Some(update) = update {
                    self.statement(update)?;
                }
                self.code.push(Op::Jump(start));
                if let Some(jump_end) = jump_end {
                    self.patch(jump_end);
                }
                for at in ctx.breaks {
                    self.patch(at);
                }
                self.pop_scope();
            }

            // 判定した値を隠し変数に置き、ラベルと順に比べて飛ぶ。
            // 一致が無ければ default へ、それも無ければ switch の外へ。
            Stmt::Switch { value, cases, line, column } => {
                self.push_scope();
                let slot = self.declare_local("$switch.value", Type::Int, *line, *column)?;
                self.expression(value)?;
                self.code.push(Op::StoreLocal(slot));

                // 先に振り分けだけを並べる。中身はそのあとに続けて置き、
                // break が無ければ次の case へ落ちるようにする。
                let mut entries = Vec::new();
                for case in cases {
                    let Some(label) = &case.label else { continue };
                    self.code.push(Op::LoadLocal(slot));
                    self.expression(label)?;
                    self.code.push(Op::Eq);
                    entries.push(self.emit_jump(Op::JumpIfTrue(0)));
                }
                let to_default = self.emit_jump(Op::Jump(0));

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
                // switch の中の continue は外側のループのもの。ここでは触らない。
                if let Some(outer) = self.loops.last_mut() {
                    outer.continues.extend(ctx.continues);
                }
                self.pop_scope();
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
                // switch は continue を受けない。Java と同じく外側のループへ抜ける。
                let Some(target) = self.loops.iter().rposition(|c| c.takes_continue) else {
                    return Err(CompileError::new(
                        *line,
                        *column,
                        "continue はループの中でしか使えません".to_string(),
                    ));
                };
                let at = self.emit_jump(Op::Jump(0));
                self.loops[target].continues.push(at);
            }

            // `a[i] = v` / `a[i] += v`。
            Stmt::AssignIndex { target, index, op, value, .. } => {
                self.expression(target)?;
                self.expression(index)?;
                if let Some(binary) = op.binary() {
                    // 読んでから書くので、対象と添字を残しておく。
                    self.code.push(Op::Dup2);
                    self.code.push(Op::GetIndex);
                    self.expression(value)?;
                    self.code.push(binary_op(binary));
                } else {
                    self.expression(value)?;
                }
                self.code.push(Op::SetIndex);
                // 代入は文なので結果は捨てる。
                self.code.push(Op::Pop);
            }

            // `v.x = 1` / `v.x += 1`。
            Stmt::AssignField { target, name, op, value, .. } => {
                let key = self.key(name);
                self.expression(target)?;
                if let Some(binary) = op.binary() {
                    self.code.push(Op::Dup);
                    self.code.push(Op::GetProp(key));
                    self.expression(value)?;
                    self.code.push(binary_op(binary));
                } else {
                    self.expression(value)?;
                }
                self.code.push(Op::SetProp(key));
                self.code.push(Op::Pop);
            }

            Stmt::IncDecIndex { target, index, delta, .. } => {
                self.expression(target)?;
                self.expression(index)?;
                self.code.push(Op::Dup2);
                self.code.push(Op::GetIndex);
                self.code.push(Op::ConstInt(*delta));
                self.code.push(Op::Add);
                self.code.push(Op::SetIndex);
                self.code.push(Op::Pop);
            }

            // `for (int v : a)`。添字を隠し変数に持って回す。
            Stmt::ForEach { ty, name, iterable, body, line, column } => {
                self.push_scope();

                // 配列そのものと添字を、ユーザーからは見えない名前で置く。
                // 途中で `a = ...` と入れ替えられても走査が壊れないようにする。
                let array_slot = self.declare_local("$foreach.array", Type::IntArray, *line, *column)?;
                self.expression(iterable)?;
                self.code.push(Op::StoreLocal(array_slot));

                let index_slot = self.declare_local("$foreach.index", Type::Int, *line, *column)?;
                self.code.push(Op::ConstInt(0));
                self.code.push(Op::StoreLocal(index_slot));

                let value_slot = self.declare_local(name, *ty, *line, *column)?;

                let start = self.code.len() as u32;
                self.code.push(Op::LoadLocal(index_slot));
                self.code.push(Op::LoadLocal(array_slot));
                self.code.push(Op::ArrayLen);
                self.code.push(Op::Lt);
                let jump_end = self.emit_jump(Op::JumpIfFalse(0));

                // 今回の要素を取り出して変数へ入れる。
                self.code.push(Op::LoadLocal(array_slot));
                self.code.push(Op::LoadLocal(index_slot));
                self.code.push(Op::GetIndex);
                self.code.push(Op::Coerce(*ty));
                self.code.push(Op::StoreLocal(value_slot));

                self.loops.push(LoopCtx::loop_body());
                self.statement(body)?;
                let ctx = self.loops.pop().expect("直前に積んだ");

                let update_at = self.code.len() as u32;
                for at in ctx.continues {
                    self.patch_to(at, update_at);
                }
                self.code.push(Op::LoadLocal(index_slot));
                self.code.push(Op::ConstInt(1));
                self.code.push(Op::Add);
                self.code.push(Op::StoreLocal(index_slot));
                self.code.push(Op::Jump(start));

                self.patch(jump_end);
                for at in ctx.breaks {
                    self.patch(at);
                }
                self.pop_scope();
            }

            Stmt::Expr(expr) => {
                self.expression(expr)?;
                // 式文の結果は捨てる。
                self.code.push(Op::Pop);
            }

            Stmt::Return { value, .. } => match value {
                Some(expr) => {
                    self.expression(expr)?;
                    self.code.push(Op::ReturnValue);
                }
                None => self.code.push(Op::Return),
            },
        }
        Ok(())
    }

    // ---- 式 -------------------------------------------------------------

    fn expression(&mut self, expr: &Expr) -> Result<(), CompileError> {
        match expr {
            Expr::Int(v) => self.code.push(Op::ConstInt(*v)),
            Expr::Float(v) => self.code.push(Op::ConstFloat(*v)),
            Expr::Bool(v) => self.code.push(Op::ConstBool(*v)),

            // メソッドの中では、フィールド名は `this.x` の意味になる。
            Expr::Var(name) if self.fields.contains(name) && self.lookup(name).is_none() => {
                let key = self.key(name);
                self.code.push(Op::LoadLocal(0));
                self.code.push(Op::GetProp(key));
            }

            Expr::Var(name) => {
                if let Some(builtin) = BuiltinVar::resolve(name) {
                    self.code.push(Op::LoadBuiltin(builtin));
                } else {
                    let (var, is_local) = self.lookup(name).ok_or_else(|| {
                        CompileError::new(0, 0, format!("変数 {name} が見つかりません"))
                    })?;
                    self.code.push(if is_local {
                        Op::LoadLocal(var.slot)
                    } else {
                        Op::LoadGlobal(var.slot)
                    });
                }
            }

            Expr::Unary { op, operand } => {
                self.expression(operand)?;
                self.code.push(match op {
                    UnaryOp::Neg => Op::Neg,
                    UnaryOp::Not => Op::Not,
                    UnaryOp::BitNot => Op::BitNot,
                });
            }

            Expr::Str(text) => {
                let index = self.intern(text);
                self.code.push(Op::ConstStr(index));
            }

            // `new P(a)`。器を作り、既定値とメソッドを載せてから初期化を呼ぶ。
            Expr::New { class, args, line, column } => {
                let Some(info) = self.classes.get(class) else {
                    return Err(CompileError::new(
                        *line,
                        *column,
                        format!("クラス {class} がありません"),
                    ));
                };
                let fields = info.fields.clone();
                let methods = info.methods.clone();
                let constructor = info.constructor;

                self.code.push(Op::NewObject);
                for (ty, name) in &fields {
                    let key = self.key(name);
                    self.emit_default(*ty);
                    self.code.push(Op::InitProp(key));
                }
                // メソッドは実体ごとに載せる。`p.step()` の引き先になる。
                for (name, index) in &methods {
                    let key = self.key(name);
                    self.code.push(Op::ConstFunction(*index));
                    self.code.push(Op::InitProp(key));
                }

                if let Some(constructor) = constructor {
                    if args.len() > u8::MAX as usize - 1 {
                        return Err(CompileError::new(*line, *column, "引数が多すぎます".to_string()));
                    }
                    // 初期化は `this` を第 1 引数として受け取る。器はスタックに
                    // 残したいので、呼ぶ前に複製しておく。
                    self.code.push(Op::Dup);
                    for arg in args {
                        self.expression(arg)?;
                    }
                    self.code.push(Op::Call(constructor, args.len() as u8 + 1));
                    self.code.push(Op::Pop);
                }
            }

            Expr::This => self.code.push(Op::LoadLocal(0)),

            Expr::Cast { ty, operand } => {
                self.expression(operand)?;
                self.code.push(Op::Coerce(*ty));
            }

            Expr::Index { target, index, .. } => {
                self.expression(target)?;
                self.expression(index)?;
                self.code.push(Op::GetIndex);
            }

            Expr::NewArray { ty, sizes } => {
                for size in sizes {
                    self.expression(size)?;
                }
                self.code.push(if sizes.len() == 2 {
                    Op::NewArray2Of(*ty)
                } else {
                    Op::NewArrayOf(*ty)
                });
            }

            Expr::ArrayLit { ty, items } => {
                for item in items {
                    self.expression(item)?;
                    // 宣言した型に合わせる。`float[] a = {1,2}` を int の配列に
                    // してしまうと、あとの割り算が整数演算になる。
                    if let Some(element) = ty.element() {
                        self.code.push(Op::Coerce(element));
                    }
                }
                self.code.push(Op::NewArray(items.len() as u16));
            }

            Expr::Field { target, name } => {
                let key = self.key(name);
                self.expression(target)?;
                self.code.push(Op::GetProp(key));
            }

            Expr::MethodCall { target, name, args, line, column } => {
                if args.len() > u8::MAX as usize {
                    return Err(CompileError::new(*line, *column, "引数が多すぎます".to_string()));
                }
                let key = self.key(name);
                self.expression(target)?;
                for arg in args {
                    self.expression(arg)?;
                }
                self.code.push(Op::CallMethod(key, args.len() as u8));
            }

            Expr::NewVector { args } => {
                for arg in args.iter().take(3) {
                    self.expression(arg)?;
                }
                // 足りない成分は 0。`new PVector(1, 2)` の z がこれ。
                for _ in args.len()..3 {
                    self.code.push(Op::ConstFloat(0.0));
                }
                self.code.push(Op::CallNative(natives::Native::CreateVector, 3));
            }

            Expr::ArrayLen { target } => {
                self.expression(target)?;
                self.code.push(Op::ArrayLen);
            }

            Expr::Binary { op, lhs, rhs } => {
                self.expression(lhs)?;
                self.expression(rhs)?;
                self.code.push(binary_op(*op));
            }

            // 短絡評価。左辺で結果が決まるなら右辺を実行しない。
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

            Expr::Call { name, args, line, column } => {
                for arg in args {
                    self.expression(arg)?;
                }
                let argc = args.len() as u8;

                if let Some(sig) = self.functions.get(name) {
                    if sig.arity != argc {
                        return Err(CompileError::new(
                            *line,
                            *column,
                            format!("{name} は引数 {} 個ですが {argc} 個渡されています", sig.arity),
                        ));
                    }
                    let index = sig.index;
                    self.code.push(Op::Call(index, argc));
                } else if let Some(native) = natives::resolve(name, argc) {
                    self.code.push(Op::CallNative(native, argc));
                } else if natives::is_native(name) {
                    let arities = natives::accepted_arities(name)
                        .iter()
                        .map(u8::to_string)
                        .collect::<Vec<_>>()
                        .join(" か ");
                    return Err(CompileError::new(
                        *line,
                        *column,
                        format!("{name} は引数 {arities} 個で呼びます ({argc} 個渡されています)"),
                    ));
                } else {
                    return Err(CompileError::new(
                        *line,
                        *column,
                        format!("{name} という関数はありません"),
                    ));
                }
            }
        }
        Ok(())
    }

    // ---- 補助 -----------------------------------------------------------

    /// 文字列を定数表へ入れ、その番号を返す。
    fn intern(&mut self, text: &str) -> u16 {
        if let Some(index) = self.strings.iter().position(|s| s == text) {
            return index as u16;
        }
        self.strings.push(text.to_string());
        (self.strings.len() - 1) as u16
    }

    /// プロパティ名を表へ入れ、その番号を返す。
    fn key(&mut self, name: &str) -> u16 {
        if let Some(index) = self.keys.iter().position(|k| k == name) {
            return index as u16;
        }
        self.keys.push(name.to_string());
        (self.keys.len() - 1) as u16
    }

    fn emit_default(&mut self, ty: Type) {
        let empty = if ty == Type::Str { self.intern("") } else { 0 };
        self.code.push(match ty {
            Type::Int => Op::ConstInt(0),
            Type::Float => Op::ConstFloat(0.0),
            Type::Boolean => Op::ConstBool(false),
            Type::Void => Op::ConstInt(0),
            // 宣言だけした PVector は 0 ベクトル。Java の null より扱いやすい。
            Type::Vector => Op::CallNative(natives::Native::CreateVector, 0),
            Type::Str => Op::ConstStr(empty),
            Type::VectorArray
            | Type::InstanceArray
            | Type::StrArray
            | Type::IntArray2
            | Type::FloatArray2
            | Type::BooleanArray2 => Op::NewArray(0),
            // Java の配列の初期値は null。ここでは要素 0 個の配列にしておく。
            // null を足すより、長さ 0 として扱えるほうが事故が少ない。
            ty if ty.is_array() => Op::NewArray(0),
            _ => Op::ConstInt(0),
        });
    }

    /// 飛び先未定のジャンプを置き、その位置を返す。
    fn emit_jump(&mut self, op: Op) -> usize {
        self.code.push(op);
        self.code.len() - 1
    }

    /// `emit_jump` で置いた命令の飛び先を現在位置にする。
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

/// 二項演算子から命令へ。代入演算子の展開からも使う。
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse;

    fn compile_source(source: &str) -> Result<Program, CompileError> {
        compile(&parse(source).expect("パースに成功する"))
    }

    #[test]
    fn a_minimal_sketch_compiles() {
        let p = compile_source("void draw() { background(0); }").expect("コンパイルに成功する");
        assert!(p.draw.is_some());
        assert!(p.setup.is_none());
        assert_eq!(p.global_count, 0);
    }

    #[test]
    fn globals_are_initialised_in_declaration_order() {
        let p = compile_source("int a = 1;\nfloat b = 2.0;\nvoid draw() {}")
            .expect("コンパイルに成功する");
        assert_eq!(p.global_count, 2);
        let init = &p.functions[p.globals_init as usize].code;
        let stores: Vec<_> =
            init.iter().filter(|op| matches!(op, Op::StoreGlobal(_))).copied().collect();
        assert_eq!(stores, vec![Op::StoreGlobal(0), Op::StoreGlobal(1)]);
    }

    #[test]
    fn block_scopes_reuse_local_slots() {
        let p = compile_source(
            "void draw() { { int a = 1; a = a; } { int b = 2; b = b; } }",
        )
        .expect("コンパイルに成功する");
        // 2 つのブロックは同時に生きていないので slot は 1 つで足りる。
        assert_eq!(p.functions[0].local_count, 1);
    }

    #[test]
    fn nested_blocks_need_separate_slots() {
        let p = compile_source("void draw() { int a = 1; { int b = 2; b = a; } }")
            .expect("コンパイルに成功する");
        assert_eq!(p.functions[0].local_count, 2);
    }

    #[test]
    fn undefined_variable_is_rejected() {
        let e = compile_source("void draw() { x = 1; }").unwrap_err();
        assert!(e.message.contains("見つかりません"), "{e}");
    }

    #[test]
    fn undefined_function_is_rejected() {
        let e = compile_source("void draw() { loadImage(1); }").unwrap_err();
        assert!(e.message.contains("という関数はありません"), "{e}");
    }

    #[test]
    fn wrong_arity_on_a_native_names_the_accepted_counts() {
        let e = compile_source("void draw() { rect(1, 2, 3); }").unwrap_err();
        assert!(e.message.contains("引数 4 個"), "{e}");
    }

    #[test]
    fn wrong_arity_on_a_user_function_is_rejected() {
        let e = compile_source("void draw() { f(1); }\nvoid f(int a, int b) {}").unwrap_err();
        assert!(e.message.contains("引数 2 個"), "{e}");
    }

    #[test]
    fn builtin_names_cannot_be_shadowed() {
        let e = compile_source("void draw() { int width = 1; }").unwrap_err();
        assert!(e.message.contains("組み込み変数"), "{e}");

        let e = compile_source("void rect() {}\nvoid draw() {}").unwrap_err();
        assert!(e.message.contains("組み込み名"), "{e}");
    }

    #[test]
    fn duplicate_definitions_are_rejected() {
        let e = compile_source("void f() {}\nvoid f() {}\nvoid draw() {}").unwrap_err();
        assert!(e.message.contains("既に定義"), "{e}");

        let e = compile_source("int a = 1;\nint a = 2;\nvoid draw() {}").unwrap_err();
        assert!(e.message.contains("重複"), "{e}");
    }

    #[test]
    fn a_sketch_without_setup_or_draw_is_rejected() {
        let e = compile_source("int a = 1;").unwrap_err();
        assert!(e.message.contains("setup() か draw()"), "{e}");
    }

    #[test]
    fn entry_points_may_not_take_arguments() {
        let e = compile_source("void draw(int a) {}").unwrap_err();
        assert!(e.message.contains("draw() に引数"), "{e}");
    }

    #[test]
    fn jumps_point_inside_the_code() {
        let p = compile_source("void draw() { for (int i = 0; i < 3; i++) { point(i, i); } }")
            .expect("コンパイルに成功する");
        let code = &p.functions[0].code;
        for op in code {
            if let Op::Jump(t) | Op::JumpIfFalse(t) | Op::JumpIfTrue(t) = op {
                assert!((*t as usize) <= code.len(), "飛び先が範囲外: {t}");
            }
        }
    }
}
