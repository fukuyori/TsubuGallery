//! スタック型 VM (設計書 §16)。
//!
//! 1 フレームの実行には命令数の予算を設ける。暴走した作品があっても、そのフレーム
//! を打ち切って Viewer へ制御を返すので、Gallery 全体は止まらない (設計書 §21.1)。

use std::fmt;

use tsubu_renderer::{Color, Graphics, Shadow};

use crate::ast::Type;
use crate::bytecode::{Op, Program, Value, VectorRef};
use crate::math::Rng;
use crate::natives::{self, BuiltinVar};

/// 1 フレームで実行してよい命令数の既定値。
///
/// 60fps を保てる範囲より十分大きく取り、無限ループだけを止める大きさにする。
pub const DEFAULT_FRAME_BUDGET: u64 = 20_000_000;

/// 関数呼び出しのネスト上限。
const MAX_CALL_DEPTH: usize = 256;

/// 実行を打ち切った理由。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Trap {
    /// フレームの実行予算を使い切った (設計書 §21.1)。
    BudgetExceeded,
    /// 再帰が深すぎる。
    CallDepthExceeded,
    DivideByZero,
    /// 関数でないものを呼ぼうとした。
    NotCallable(String),
    /// 配列に無いメソッドを呼ぼうとした。
    NoSuchMethod(String),
    /// 配列が大きくなりすぎた。
    ArrayTooLong,
    /// VM 内部の不整合。コンパイラのバグ。
    Internal(String),
}

impl fmt::Display for Trap {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Trap::BudgetExceeded => write!(f, "1 フレームの実行量が上限を超えました"),
            Trap::CallDepthExceeded => write!(f, "関数の呼び出しが深すぎます"),
            Trap::DivideByZero => write!(f, "整数を 0 で割りました"),
            Trap::NotCallable(what) => write!(f, "{what} は関数として呼べません"),
            Trap::NoSuchMethod(name) => write!(f, "{name}() というメソッドはありません"),
            Trap::ArrayTooLong => write!(f, "配列が大きくなりすぎました"),
            Trap::Internal(m) => write!(f, "内部エラー: {m}"),
        }
    }
}

impl std::error::Error for Trap {}

struct Frame {
    function: u16,
    ip: usize,
    /// このフレームのローカルが始まる位置。
    locals_base: usize,
}

pub struct Vm {
    stack: Vec<Value>,
    locals: Vec<Value>,
    frames: Vec<Frame>,
    globals: Vec<Value>,
    rng: Rng,
    /// 直近フレームで実行した命令数。
    pub last_frame_ops: u64,
    /// 引数の受け渡し用。毎回確保しないため使い回す。
    args: Vec<Value>,
    /// `drawingContext`。触った作品にだけ作る。
    ///
    /// 書き込みが消えては影の指定を受け取れないので、同じ実体を返し続ける。
    drawing_context: Option<Value>,
    /// `push()` のたびに積む `drawingContext` の控え。
    ///
    /// p5.js の `push()` は canvas の `save()` も呼ぶので、影の指定は
    /// `pop()` で元へ戻る。
    saved_contexts: Vec<Vec<(u16, Value)>>,
}

impl Vm {
    /// `seed` は `random()` の再現性のために作品ごとに固定する。
    pub fn new(program: &Program, seed: u64) -> Self {
        Self {
            stack: Vec::with_capacity(256),
            locals: Vec::with_capacity(256),
            frames: Vec::with_capacity(16),
            globals: vec![Value::Void; program.global_count as usize],
            rng: Rng::new(seed),
            last_frame_ops: 0,
            args: Vec::with_capacity(8),
            drawing_context: None,
            saved_contexts: Vec::new(),
        }
    }

    /// グローバル変数を名前で読む。
    ///
    /// 名前はコンパイル時に消えるので、`Program` の側から引き直す。診断とテスト用。
    pub fn global_by_name(&self, program: &Program, name: &str) -> Option<Value> {
        let slot = program.global_slot(name)?;
        self.globals.get(slot as usize).cloned()
    }

    /// グローバル変数の初期化。`setup()` の前に一度だけ呼ぶ。
    pub fn init_globals(
        &mut self,
        program: &Program,
        g: &mut Graphics,
        budget: u64,
    ) -> Result<(), Trap> {
        self.call(program, program.globals_init, g, budget)
    }

    /// 引数なしの関数を最後まで実行する。`setup()` / `draw()` の入口。
    ///
    /// 呼び出しは VM の内側でフレームを積むだけなので、ホストのスタックは
    /// 再帰の深さに関係なく一定に保たれる。
    pub fn call(
        &mut self,
        program: &Program,
        function: u16,
        g: &mut Graphics,
        budget: u64,
    ) -> Result<(), Trap> {
        self.stack.clear();
        self.locals.clear();
        self.frames.clear();
        self.last_frame_ops = 0;

        self.enter(program, function, 0)?;
        let result = self.execute(program, g, budget);
        // 打ち切られた場合も、次のフレームへ状態を持ち越さない。
        self.stack.clear();
        self.locals.clear();
        self.frames.clear();
        self.last_frame_ops = match &result {
            Ok(ops) => *ops,
            Err(_) => budget,
        };
        result.map(|_| ())
    }

    fn enter(&mut self, program: &Program, function: u16, argc: u8) -> Result<(), Trap> {
        if self.frames.len() >= MAX_CALL_DEPTH {
            return Err(Trap::CallDepthExceeded);
        }
        let f = program
            .functions
            .get(function as usize)
            .ok_or_else(|| Trap::Internal(format!("関数 {function} がありません")))?;

        let locals_base = self.locals.len();
        let slots = f.local_count as usize;
        // 足りない引数は undefined。JavaScript と同じ。
        self.locals.resize(locals_base + slots, Value::Undefined);

        // 引数はスタック上に左から積まれている。
        let split = self
            .stack
            .len()
            .checked_sub(argc as usize)
            .ok_or_else(|| Trap::Internal("引数がスタックに足りません".into()))?;
        for (i, value) in self.stack.drain(split..).enumerate() {
            // 多すぎる引数は捨てる。`map` は要素と添字を渡すが、
            // `p => ...` のように受け取らない書き方が普通にある。
            if i < slots {
                self.locals[locals_base + i] = value;
            }
        }

        self.frames.push(Frame { function, ip: 0, locals_base });
        Ok(())
    }

    /// フレームスタックが空になるまで実行し、消費した命令数を返す。
    fn execute(&mut self, program: &Program, g: &mut Graphics, budget: u64) -> Result<u64, Trap> {
        let mut ops = 0u64;

        while !self.frames.is_empty() {
            ops += 1;
            if ops > budget {
                return Err(Trap::BudgetExceeded);
            }
            self.step(program, g)?;
        }

        Ok(ops)
    }

    /// 命令を 1 つ実行する。
    fn step(&mut self, program: &Program, g: &mut Graphics) -> Result<(), Trap> {
        {
            let frame = self.frames.last_mut().expect("呼び出し側が存在を確認済み");
            let code = &program.functions[frame.function as usize].code;
            let Some(&op) = code.get(frame.ip) else {
                return Err(Trap::Internal("関数の終端を越えました".into()));
            };
            frame.ip += 1;
            let locals_base = frame.locals_base;

            match op {
                Op::ConstInt(v) => self.stack.push(Value::Int(v)),
                Op::ConstFloat(v) => self.stack.push(Value::Float(v)),
                Op::ConstBool(v) => self.stack.push(Value::Bool(v)),

                Op::LoadLocal(slot) => {
                    let v = self.locals[locals_base + slot as usize].clone();
                    self.stack.push(v);
                }
                Op::StoreLocal(slot) => {
                    let v = self.pop()?;
                    self.locals[locals_base + slot as usize] = v;
                }
                Op::LoadGlobal(slot) => self.stack.push(self.globals[slot as usize].clone()),
                Op::StoreGlobal(slot) => {
                    let v = self.pop()?;
                    self.globals[slot as usize] = v;
                }
                Op::LoadBuiltin(var) => {
                    let v = match var {
                        BuiltinVar::DrawingContext => {
                            self.drawing_context.get_or_insert_with(Value::new_object).clone()
                        }
                        _ => var.read(g),
                    };
                    self.stack.push(v);
                }

                Op::Add | Op::Sub | Op::Mul | Op::Div | Op::Rem => {
                    let rhs = self.pop()?;
                    let lhs = self.pop()?;
                    // どちらかが文字列なら `+` は連結。JavaScript と Java の
                    // どちらもそう決めている。
                    if op == Op::Add
                        && (matches!(lhs, Value::Str(_)) || matches!(rhs, Value::Str(_)))
                    {
                        let joined = format!("{}{}", lhs.to_display(), rhs.to_display());
                        if joined.len() > MAX_STRING_LENGTH {
                            return Err(Trap::ArrayTooLong);
                        }
                        self.stack.push(Value::new_str(joined));
                    } else {
                        self.stack.push(arithmetic(op, lhs, rhs)?);
                    }
                }
                Op::Neg => {
                    let v = self.pop()?;
                    self.stack.push(match v {
                        Value::Int(i) => Value::Int(i.wrapping_neg()),
                        other => Value::Float(-other.as_f32()),
                    });
                }
                Op::Not => {
                    let v = self.pop()?;
                    self.stack.push(Value::Bool(!v.truthy()));
                }
                Op::BitNot => {
                    let v = self.pop()?;
                    self.stack.push(Value::Int(!to_i32(&v)));
                }
                Op::BitAnd | Op::BitOr | Op::BitXor | Op::Shl | Op::Shr | Op::UShr => {
                    let rhs = self.pop()?;
                    let lhs = self.pop()?;
                    self.stack.push(Value::Int(bitwise(op, to_i32(&lhs), to_i32(&rhs))?));
                }

                Op::Eq | Op::Ne | Op::Lt | Op::Le | Op::Gt | Op::Ge => {
                    let rhs = self.pop()?;
                    let lhs = self.pop()?;
                    self.stack.push(Value::Bool(compare(op, lhs, rhs)));
                }

                Op::Dup => {
                    let v =
                        self.stack.last().ok_or_else(|| Trap::Internal("スタックが空".into()))?.clone();
                    self.stack.push(v);
                }
                Op::Pop => {
                    self.pop()?;
                }

                Op::Jump(target) => self.set_ip(target),
                Op::JumpIfFalse(target) => {
                    if !self.pop()?.truthy() {
                        self.set_ip(target);
                    }
                }
                Op::JumpIfTrue(target) => {
                    if self.pop()?.truthy() {
                        self.set_ip(target);
                    }
                }

                Op::Coerce(ty) => {
                    let v = self.pop()?;
                    self.stack.push(v.coerce(ty));
                }

                Op::CallNative(native, argc) => {
                    let split = self
                        .stack
                        .len()
                        .checked_sub(argc as usize)
                        .ok_or_else(|| Trap::Internal("引数がスタックに足りません".into()))?;
                    self.args.clear();
                    self.args.extend(self.stack.drain(split..));
                    let result = self.call_native(native, program, g);
                    self.stack.push(result);
                }

                Op::Call(function, argc) => self.enter(program, function, argc)?,

                Op::ConstStr(index) => {
                    self.stack.push(Value::new_str(program.string(index)));
                }
                Op::ConstUndefined => self.stack.push(Value::Undefined),
                Op::ConstFunction(index) => self.stack.push(Value::Function(index)),
                Op::ConstNativeFn(native) => self.stack.push(Value::NativeFn(native)),

                Op::IsUndefined => {
                    let value = self.pop()?;
                    self.stack.push(Value::Bool(matches!(value, Value::Undefined)));
                }

                Op::Dup2 => {
                    let split = self.split_at(2)?;
                    let (a, b) = (self.stack[split].clone(), self.stack[split + 1].clone());
                    self.stack.push(a);
                    self.stack.push(b);
                }

                Op::NewArray(count) => {
                    let split = self.split_at(count as usize)?;
                    let values: Vec<Value> = self.stack.drain(split..).collect();
                    self.stack.push(Value::new_array(values));
                }
                Op::NewArrayOf(ty) => {
                    let size = self.pop()?.as_i32();
                    if size < 0 {
                        return Err(Trap::ArrayTooLong);
                    }
                    // 1 フレームでメモリを食い潰さないよう、他の配列操作と
                    // 同じ上限を掛ける。
                    const MAX_LENGTH: i32 = 1 << 20;
                    if size > MAX_LENGTH {
                        return Err(Trap::ArrayTooLong);
                    }
                    let element = ty.element().unwrap_or(Type::Float);
                    let items = if element == Type::Instance {
                        // Java の配列は null で始まる。ここでは undefined。
                        // どのクラスの実体を入れるかは書く側が決める。
                        vec![Value::Undefined; size as usize]
                    } else if element == Type::Str {
                        vec![Value::new_str(""); size as usize]
                    } else if element == Type::Vector {
                        // ベクトルは参照なので、写しではなく 1 本ずつ作る。
                        // 使い回すと、1 つ動かしただけで全部が動いてしまう。
                        (0..size).map(|_| Value::new_vector(0.0, 0.0, 0.0)).collect()
                    } else {
                        vec![Value::Int(0).coerce(element); size as usize]
                    };
                    self.stack.push(Value::new_array(items));
                }

                Op::NewArray2Of(ty) => {
                    // 積んだ順は [行数, 列数]。
                    let cols = self.pop()?.as_i32();
                    let rows = self.pop()?.as_i32();
                    if rows < 0 || cols < 0 {
                        return Err(Trap::ArrayTooLong);
                    }
                    let (rows, cols) = (rows as usize, cols as usize);
                    if rows.saturating_mul(cols) > MAX_ARRAY_LENGTH {
                        return Err(Trap::ArrayTooLong);
                    }
                    let element = ty.element().and_then(|t| t.element()).unwrap_or(Type::Float);
                    let fill = Value::Int(0).coerce(element);
                    // 行ごとに別の配列を作る。使い回すと 1 行の書き換えが
                    // 全部の行へ及んでしまう。
                    let grid = (0..rows)
                        .map(|_| Value::new_array(vec![fill.clone(); cols]))
                        .collect();
                    self.stack.push(Value::new_array(grid));
                }

                Op::ArrayLen => {
                    let target = self.pop()?;
                    let len = match &target {
                        Value::Array(items) => items.borrow().len() as i32,
                        // 文字列の長さは文字数。バイト数ではない。
                        Value::Str(text) => text.chars().count() as i32,
                        _ => return Err(Trap::NoSuchMethod("length".into())),
                    };
                    self.stack.push(Value::Int(len));
                }

                Op::ArrayPush => {
                    let value = self.pop()?;
                    let array = self.peek()?;
                    match &array {
                        Value::Array(items) => {
                            if items.borrow().len() >= MAX_ARRAY_LENGTH {
                                return Err(Trap::ArrayTooLong);
                            }
                            items.borrow_mut().push(value);
                        }
                        _ => return Err(Trap::NoSuchMethod("push".into())),
                    }
                }

                Op::ArrayExtend => {
                    let other = self.pop()?;
                    let array = self.peek()?;
                    let (Value::Array(target), Value::Array(source)) = (&array, &other) else {
                        // 展開できるのは配列だけ。文字列はまだ無い。
                        return Err(Trap::NoSuchMethod("...".into()));
                    };
                    // 自分自身を展開しても止まるよう、先に写しを取る。
                    let items: Vec<Value> = source.borrow().clone();
                    if target.borrow().len() + items.len() > MAX_ARRAY_LENGTH {
                        return Err(Trap::ArrayTooLong);
                    }
                    target.borrow_mut().extend(items);
                }

                Op::NewObject => self.stack.push(Value::new_object()),

                Op::InitProp(key) => {
                    let value = self.pop()?;
                    let object = self.peek()?;
                    set_property(&object, key, value, program)?;
                }
                Op::GetProp(key) => {
                    let object = self.pop()?;
                    self.stack.push(get_property(&object, key, program));
                }
                Op::SetProp(key) => {
                    let value = self.pop()?;
                    let object = self.pop()?;
                    set_property(&object, key, value.clone(), program)?;
                    // 代入は式。書いた値を残す。
                    self.stack.push(value);
                }

                Op::GetIndex => {
                    let index = self.pop()?;
                    let target = self.pop()?;
                    self.stack.push(get_index(&target, &index));
                }
                Op::SetIndex => {
                    let value = self.pop()?;
                    let index = self.pop()?;
                    let target = self.pop()?;
                    set_index(&target, &index, value.clone())?;
                    self.stack.push(value);
                }

                // 引数を配列で受ける呼び出し。`f(...xs)` から来る。
                Op::CallNativeSpread(native) => {
                    let args = self.pop()?;
                    self.args.clear();
                    if let Value::Array(items) = &args {
                        self.args.extend(items.borrow().iter().cloned());
                    }
                    let result = self.call_native(native, program, g);
                    self.stack.push(result);
                }
                Op::CallMethodSpread(key) => {
                    let argc = self.unpack_args()?;
                    let split = self.split_at(argc as usize + 1)?;
                    let receiver = self.stack[split].clone();
                    if let Value::Function(index) = get_property(&receiver, key, program) {
                        self.enter(program, index, argc + 1)?;
                    } else {
                        self.stack.remove(split);
                        self.call_method(program, receiver, key, argc, g)?;
                    }
                }
                Op::CallValueSpread => {
                    let argc = self.unpack_args()?;
                    let split = self.split_at(argc as usize + 1)?;
                    let callee = self.stack.remove(split);
                    self.invoke(program, callee, argc, g)?;
                }

                Op::CallValue(argc) => {
                    let split = self.split_at(argc as usize + 1)?;
                    let callee = self.stack.remove(split);
                    self.invoke(program, callee, argc, g)?;
                }
                Op::CallMethod(key, argc) => {
                    let split = self.split_at(argc as usize + 1)?;
                    // 受け手は引数の下に積まれている。ユーザー定義のメソッドは
                    // それを第 1 引数 (`this`) として受け取るので、覗くだけで
                    // 取り除かない。
                    let receiver = self.stack[split].clone();
                    if let Value::Function(index) = get_property(&receiver, key, program) {
                        self.enter(program, index, argc + 1)?;
                    } else {
                        self.stack.remove(split);
                        self.call_method(program, receiver, key, argc, g)?;
                    }
                }

                Op::Return => {
                    self.leave()?;
                    self.stack.push(Value::Void);
                }
                Op::ReturnValue => {
                    let v = self.pop()?;
                    self.leave()?;
                    self.stack.push(v);
                }
            }
        }

        Ok(())
    }

    /// 引数 `n` 個ぶん手前の位置。
    fn split_at(&self, n: usize) -> Result<usize, Trap> {
        self.stack
            .len()
            .checked_sub(n)
            .ok_or_else(|| Trap::Internal("引数がスタックに足りません".into()))
    }

    fn peek(&self) -> Result<Value, Trap> {
        self.stack.last().cloned().ok_or_else(|| Trap::Internal("スタックが空".into()))
    }

    /// 積んである引数の配列をばらしてスタックへ戻し、個数を返す。
    ///
    /// 引数の個数は 255 まで。それ以上は溢れるので切る。
    fn unpack_args(&mut self) -> Result<u8, Trap> {
        let args = self.pop()?;
        let Value::Array(items) = args else { return Ok(0) };
        let items = items.borrow();
        let argc = items.len().min(u8::MAX as usize);
        self.stack.extend(items.iter().take(argc).cloned());
        Ok(argc as u8)
    }

    /// 値として持っている関数を呼ぶ。引数はスタックに積まれている。
    fn invoke(
        &mut self,
        program: &Program,
        callee: Value,
        argc: u8,
        g: &mut Graphics,
    ) -> Result<(), Trap> {
        match callee {
            Value::Function(index) => self.enter(program, index, argc),
            Value::NativeFn(native) => {
                let split = self.split_at(argc as usize)?;
                self.args.clear();
                self.args.extend(self.stack.drain(split..));
                let result = self.call_native(native, program, g);
                self.stack.push(result);
                Ok(())
            }
            other => Err(Trap::NotCallable(other.type_name().to_string())),
        }
    }

    /// 配列のメソッドを呼ぶ。
    fn call_method(
        &mut self,
        program: &Program,
        receiver: Value,
        key: u16,
        argc: u8,
        g: &mut Graphics,
    ) -> Result<(), Trap> {
        let name = program.key(key);

        // 文字列のメソッド。
        if let Value::Str(text) = &receiver {
            let text = text.clone();
            let split = self.split_at(argc as usize)?;
            let args: Vec<Value> = self.stack.drain(split..).collect();
            let result = string_method(name, &text, &args)?;
            self.stack.push(result);
            return Ok(());
        }

        // ベクトルのメソッド。
        if let Value::Vector(v) = &receiver {
            let v = v.clone();
            let split = self.split_at(argc as usize)?;
            let args: Vec<Value> = self.stack.drain(split..).collect();
            let result = vector_method(name, &receiver, &v, &args)?;
            self.stack.push(result);
            return Ok(());
        }

        // p5 の描画関数は自分自身を返すので `background(9).stroke(w, 116)` と
        // 数珠つなぎに書ける。こちらは void を返すので、void に対する呼び出しを
        // 組み込みへ回すことで同じ書き方を通す。
        if matches!(receiver, Value::Void)
            && let Some(native) = natives::resolve(name, argc)
        {
            let split = self.split_at(argc as usize)?;
            self.args.clear();
            self.args.extend(self.stack.drain(split..));
            let result = self.call_native(native, program, g);
            self.stack.push(result);
            return Ok(());
        }

        let Value::Array(array) = &receiver else {
            // 配列以外へのメソッド呼び出しは、プロパティに入っている関数として扱う。
            let callee = get_property(&receiver, key, program);
            if matches!(callee, Value::Undefined) {
                // 何が呼べなかったのかを伝える。型名だけでは直しようがない。
                return Err(Trap::NotCallable(format!("{}.{name}", receiver.type_name())));
            }
            return self.invoke(program, callee, argc, g);
        };

        let split = self.split_at(argc as usize)?;
        let args: Vec<Value> = self.stack.drain(split..).collect();

        match (name, args.len()) {
            ("push", _) => {
                array.borrow_mut().extend(args);
                let length = array.borrow().len();
                self.stack.push(Value::Float(length as f32));
                Ok(())
            }
            // `[...Array(9).keys()]` の形で使う。添字を並べた配列を返す。
            ("keys" | "entries", 0) => {
                let len = array.borrow().len();
                let items = (0..len)
                    .map(|i| {
                        if name == "keys" {
                            Value::Float(i as f32)
                        } else {
                            Value::new_array(vec![
                                Value::Float(i as f32),
                                array.borrow()[i].clone(),
                            ])
                        }
                    })
                    .collect();
                self.stack.push(Value::new_array(items));
                Ok(())
            }
            // 端から出し入れする。`shift()` は待ち行列を進める作品でよく出る。
            ("pop" | "shift", 0) => {
                let mut items = array.borrow_mut();
                let taken = if name == "pop" {
                    items.pop()
                } else if items.is_empty() {
                    None
                } else {
                    Some(items.remove(0))
                };
                self.stack.push(taken.unwrap_or(Value::Undefined));
                Ok(())
            }
            ("unshift", _) => {
                let mut items = array.borrow_mut();
                for (i, v) in args.into_iter().enumerate() {
                    items.insert(i, v);
                }
                let length = items.len();
                self.stack.push(Value::Float(length as f32));
                Ok(())
            }
            ("at", 1) => {
                let items = array.borrow();
                let at = args[0].as_i32();
                let index = if at < 0 { items.len() as i32 + at } else { at };
                let found = usize::try_from(index).ok().and_then(|i| items.get(i).cloned());
                self.stack.push(found.unwrap_or(Value::Undefined));
                Ok(())
            }
            ("slice", _) => {
                let items = array.borrow();
                let (from, to) = slice_range(items.len(), args.first(), args.get(1));
                self.stack.push(Value::new_array(items[from..to].to_vec()));
                Ok(())
            }
            // `splice(start, count, ...items)`。取り除いたぶんを返す。
            ("splice", _) => {
                let mut items = array.borrow_mut();
                let len = items.len();
                let start = clamp_index(len, args.first().map(Value::as_i32).unwrap_or(0));
                let count = match args.get(1) {
                    Some(v) => (v.as_i32().max(0) as usize).min(len - start),
                    None => len - start,
                };
                let removed: Vec<Value> = items.splice(start..start + count, args.iter().skip(2).cloned()).collect();
                drop(items);
                self.stack.push(Value::new_array(removed));
                Ok(())
            }
            ("concat", _) => {
                let mut out = array.borrow().clone();
                for arg in args {
                    match arg {
                        Value::Array(other) => out.extend(other.borrow().iter().cloned()),
                        single => out.push(single),
                    }
                }
                self.stack.push(Value::new_array(out));
                Ok(())
            }
            ("reverse", 0) => {
                array.borrow_mut().reverse();
                self.stack.push(receiver.clone());
                Ok(())
            }
            ("fill", _) => {
                let value = args.first().cloned().unwrap_or(Value::Undefined);
                let mut items = array.borrow_mut();
                let (from, to) = slice_range(items.len(), args.get(1), args.get(2));
                for slot in &mut items[from..to] {
                    *slot = value.clone();
                }
                drop(items);
                self.stack.push(receiver.clone());
                Ok(())
            }
            // 1 段だけ平らにする。`flat(n)` の n は見ない。
            ("flat", _) => {
                let mut out = Vec::new();
                for item in array.borrow().iter() {
                    match item {
                        Value::Array(inner) => out.extend(inner.borrow().iter().cloned()),
                        single => out.push(single.clone()),
                    }
                }
                self.stack.push(Value::new_array(out));
                Ok(())
            }
            ("join", _) => {
                let sep = match args.first() {
                    Some(Value::Str(s)) => s.to_string(),
                    Some(other) => other.to_display(),
                    None => ",".to_string(),
                };
                let text = array
                    .borrow()
                    .iter()
                    .map(|v| match v {
                        Value::Undefined | Value::Void => String::new(),
                        other => other.to_display(),
                    })
                    .collect::<Vec<_>>()
                    .join(&sep);
                self.stack.push(Value::new_str(text.as_str()));
                Ok(())
            }
            ("indexOf" | "lastIndexOf" | "includes", _) => {
                let wanted = args.first().cloned().unwrap_or(Value::Undefined);
                let items = array.borrow();
                let hit = if name == "lastIndexOf" {
                    items.iter().rposition(|v| compare(Op::Eq, v.clone(), wanted.clone()))
                } else {
                    items.iter().position(|v| compare(Op::Eq, v.clone(), wanted.clone()))
                };
                drop(items);
                self.stack.push(match name {
                    "includes" => Value::Bool(hit.is_some()),
                    _ => Value::Float(hit.map_or(-1.0, |i| i as f32)),
                });
                Ok(())
            }
            // 比べ方を渡さないと、JavaScript と同じく文字として並べる。
            ("sort", _) => {
                let items: Vec<Value> = array.borrow().clone();
                let sorted = match args.first() {
                    Some(cmp) => self.sort_with(program, items, cmp.clone(), g)?,
                    None => {
                        let mut items = items;
                        items.sort_by_key(|v| v.to_display());
                        items
                    }
                };
                *array.borrow_mut() = sorted;
                self.stack.push(receiver.clone());
                Ok(())
            }
            ("reduce", _) => {
                let callback = args.first().cloned().unwrap_or(Value::Undefined);
                let items: Vec<Value> = array.borrow().clone();
                let mut iter = items.into_iter().enumerate();
                let mut acc = match args.get(1) {
                    Some(init) => init.clone(),
                    None => match iter.next() {
                        Some((_, first)) => first,
                        None => {
                            self.stack.push(Value::Undefined);
                            return Ok(());
                        }
                    },
                };
                for (index, item) in iter {
                    self.stack.push(acc);
                    self.stack.push(item);
                    self.stack.push(Value::Float(index as f32));
                    self.invoke(program, callback.clone(), 3, g)?;
                    self.run_until(program, g)?;
                    acc = self.pop()?;
                }
                self.stack.push(acc);
                Ok(())
            }
            ("map" | "forEach" | "filter" | "find" | "findIndex" | "findLast" | "some" | "every"
                | "flatMap", 1) => {
                let callback = args.into_iter().next().expect("1 個ある");
                let items: Vec<Value> = array.borrow().clone();
                let mut results = Vec::with_capacity(items.len());

                // 見つけた時点で答えが決まるものは、そこで止める。
                let mut answer: Option<Value> = None;
                for (index, item) in items.into_iter().enumerate() {
                    // コールバックは 1 要素ずつ、その場で最後まで走らせる。
                    self.stack.push(item.clone());
                    self.stack.push(Value::Float(index as f32));
                    self.invoke(program, callback.clone(), 2, g)?;
                    self.run_until(program, g)?;
                    let produced = self.pop()?;
                    let hit = produced.truthy();

                    match name {
                        "map" => results.push(produced),
                        "filter" if hit => results.push(item),
                        "flatMap" => match produced {
                            Value::Array(inner) => results.extend(inner.borrow().iter().cloned()),
                            single => results.push(single),
                        },
                        "find" if hit => {
                            answer = Some(item);
                            break;
                        }
                        "findLast" if hit => answer = Some(item),
                        "findIndex" if hit => {
                            answer = Some(Value::Float(index as f32));
                            break;
                        }
                        "some" if hit => {
                            answer = Some(Value::Bool(true));
                            break;
                        }
                        "every" if !hit => {
                            answer = Some(Value::Bool(false));
                            break;
                        }
                        _ => {}
                    }
                }

                self.stack.push(match (name, answer) {
                    (_, Some(found)) => found,
                    ("forEach", None) => Value::Undefined,
                    ("find" | "findLast", None) => Value::Undefined,
                    ("findIndex", None) => Value::Float(-1.0),
                    ("some", None) => Value::Bool(false),
                    ("every", None) => Value::Bool(true),
                    _ => Value::new_array(results),
                });
                Ok(())
            }
            _ => Err(Trap::NoSuchMethod(name.to_string())),
        }
    }

    /// 比べ方を渡された `sort()`。
    ///
    /// 比較のたびに作品のコードを呼ぶので、標準の `sort_by` は使えない
    /// (途中で失敗しうる)。素直にマージソートで回数を抑える。
    fn sort_with(
        &mut self,
        program: &Program,
        mut items: Vec<Value>,
        cmp: Value,
        g: &mut Graphics,
    ) -> Result<Vec<Value>, Trap> {
        if items.len() <= 1 {
            return Ok(items);
        }
        let right = items.split_off(items.len() / 2);
        let left = self.sort_with(program, items, cmp.clone(), g)?;
        let right = self.sort_with(program, right, cmp.clone(), g)?;

        let mut out = Vec::with_capacity(left.len() + right.len());
        let (mut i, mut j) = (0, 0);
        while i < left.len() && j < right.len() {
            self.stack.push(left[i].clone());
            self.stack.push(right[j].clone());
            self.invoke(program, cmp.clone(), 2, g)?;
            self.run_until(program, g)?;
            // 0 以下なら左が先。JavaScript の決まり。
            if self.pop()?.as_f32() <= 0.0 {
                out.push(left[i].clone());
                i += 1;
            } else {
                out.push(right[j].clone());
                j += 1;
            }
        }
        out.extend_from_slice(&left[i..]);
        out.extend_from_slice(&right[j..]);
        Ok(out)
    }

    /// いま積んだフレームが終わるまで回す。コールバックの実行に使う。
    fn run_until(&mut self, program: &Program, g: &mut Graphics) -> Result<(), Trap> {
        let floor = self.frames.len();
        if floor == 0 {
            return Ok(());
        }
        let mut guard = 0u64;
        while self.frames.len() >= floor {
            guard += 1;
            if guard > 5_000_000 {
                return Err(Trap::BudgetExceeded);
            }
            self.step(program, g)?;
        }
        Ok(())
    }

    fn set_ip(&mut self, target: u32) {
        self.frames.last_mut().expect("実行中はフレームがある").ip = target as usize;
    }

    fn leave(&mut self) -> Result<(), Trap> {
        let frame = self.frames.pop().ok_or_else(|| Trap::Internal("フレームがない".into()))?;
        self.locals.truncate(frame.locals_base);
        Ok(())
    }

    fn pop(&mut self) -> Result<Value, Trap> {
        self.stack.pop().ok_or_else(|| Trap::Internal("スタックが空".into()))
    }
}

/// `obj.key` を読む。無ければ `undefined`。
///
/// 配列の `length` だけは特別扱いする。
fn get_property(target: &Value, key: u16, program: &Program) -> Value {
    match target {
        Value::Object(fields) => fields
            .borrow()
            .iter()
            .find(|(k, _)| *k == key)
            .map(|(_, v)| v.clone())
            .unwrap_or(Value::Undefined),
        Value::Array(items) if program.key(key) == "length" => {
            Value::Float(items.borrow().len() as f32)
        }
        Value::Str(text) if program.key(key) == "length" => {
            Value::Float(text.chars().count() as f32)
        }
        Value::Vector(v) => match program.key(key) {
            "x" => Value::Float(v.borrow()[0]),
            "y" => Value::Float(v.borrow()[1]),
            "z" => Value::Float(v.borrow()[2]),
            _ => Value::Undefined,
        },
        _ => Value::Undefined,
    }
}

impl Vm {
    /// `drawingContext` へ書かれた影の指定を Graphics へ移す。
    ///
    /// `drawingContext` を触っていない作品では何もしない。触った作品でも
    /// 見るのは 4 つの項目だけなので、図形ごとに呼んでも安い。
    /// ネイティブ関数をひとつ呼ぶ。`drawingContext` の面倒もここで見る。
    fn call_native(&mut self, native: natives::Native, program: &Program, g: &mut Graphics) -> Value {
        self.sync_drawing_context(program, g);
        let result = natives::call(native, &self.args, g, &mut self.rng);
        // まだ `drawingContext` を触っていなくても控えは取る。触るのが
        // push() のあとなら、pop() でその指定ごと消えるのが正しい。
        match native {
            natives::Native::Push => {
                let now = match &self.drawing_context {
                    Some(Value::Object(fields)) => fields.borrow().clone(),
                    _ => Vec::new(),
                };
                self.saved_contexts.push(now);
            }
            natives::Native::Pop => {
                if let (Some(saved), Some(Value::Object(fields))) =
                    (self.saved_contexts.pop(), &self.drawing_context)
                {
                    *fields.borrow_mut() = saved;
                }
            }
            _ => {}
        }
        result
    }

    fn sync_drawing_context(&self, program: &Program, g: &mut Graphics) {
        let Some(Value::Object(fields)) = &self.drawing_context else { return };
        let mut shadow = Shadow { blur: 0.0, offset: [0.0; 2], color: Color::BLACK };
        let mut touched = false;
        for (key, value) in fields.borrow().iter() {
            match program.key(*key) {
                "shadowBlur" => {
                    shadow.blur = value.as_f32();
                    touched = true;
                }
                "shadowOffsetX" => {
                    shadow.offset[0] = value.as_f32();
                    touched = true;
                }
                "shadowOffsetY" => {
                    shadow.offset[1] = value.as_f32();
                    touched = true;
                }
                // 色は `color(0)` の戻り値。詰めた整数で来る。
                "shadowColor" => {
                    shadow.color = crate::natives::color_from_value(value, g);
                    touched = true;
                }
                _ => {}
            }
        }
        g.set_shadow(touched.then_some(shadow));
    }
}

fn set_property(target: &Value, key: u16, value: Value, program: &Program) -> Result<(), Trap> {
    if let Value::Vector(v) = target {
        let index = match program.key(key) {
            "x" => 0,
            "y" => 1,
            "z" => 2,
            other => return Err(Trap::NoSuchMethod(other.to_string())),
        };
        v.borrow_mut()[index] = value.as_f32();
        return Ok(());
    }
    let Value::Object(fields) = target else {
        return Err(Trap::Internal(format!(
            "{} にはプロパティを書けません",
            target.type_name()
        )));
    };
    let mut fields = fields.borrow_mut();
    match fields.iter_mut().find(|(k, _)| *k == key) {
        Some(slot) => slot.1 = value,
        None => fields.push((key, value)),
    }
    Ok(())
}

/// `a[i]`。範囲外は `undefined`。
fn get_index(target: &Value, index: &Value) -> Value {
    let Value::Array(items) = target else { return Value::Undefined };
    let at = index.as_i32();
    if at < 0 {
        return Value::Undefined;
    }
    items.borrow().get(at as usize).cloned().unwrap_or(Value::Undefined)
}

/// `a[i] = v`。JavaScript と同じく、飛ばした範囲は `undefined` で埋める。
fn set_index(target: &Value, index: &Value, value: Value) -> Result<(), Trap> {
    let Value::Array(items) = target else {
        return Err(Trap::Internal(format!("{} には添字で書けません", target.type_name())));
    };
    let at = index.as_i32();
    if at < 0 {
        return Ok(());
    }
    let at = at as usize;

    // 際限なく伸ばすと 1 フレームでメモリを食い潰すので、上限を設ける。
    const MAX_LENGTH: usize = 1 << 20;
    if at >= MAX_LENGTH {
        return Err(Trap::ArrayTooLong);
    }

    let mut items = items.borrow_mut();
    if at >= items.len() {
        items.resize(at + 1, Value::Undefined);
    }
    items[at] = value;
    Ok(())
}

/// 文字列のメソッドを 1 つ実行する。
///
/// 文字数で数える。日本語を並べる作品では、バイト数で数えると位置が狂う。
fn string_method(name: &str, text: &str, args: &[Value]) -> Result<Value, Trap> {
    let chars: Vec<char> = text.chars().collect();
    let index = |i: usize| args.get(i).map_or(0.0, Value::as_f32).max(0.0) as usize;

    Ok(match name {
        "length" => Value::Int(chars.len() as i32),
        "charAt" => Value::new_str(chars.get(index(0)).map(|c| c.to_string()).unwrap_or_default()),
        "substring" => {
            let from = index(0).min(chars.len());
            let to = match args.get(1) {
                Some(v) => (v.as_f32().max(0.0) as usize).clamp(from, chars.len()),
                None => chars.len(),
            };
            Value::new_str(chars[from..to].iter().collect::<String>())
        }
        "indexOf" => {
            let needle = args.first().map(Value::to_display).unwrap_or_default();
            // 文字単位の位置へ直す。バイト位置のままでは charAt と噛み合わない。
            Value::Int(match text.find(&needle) {
                Some(byte) => text[..byte].chars().count() as i32,
                None => -1,
            })
        }
        "split" => {
            let sep = args.first().map(Value::to_display).unwrap_or_default();
            let parts: Vec<Value> = if sep.is_empty() {
                chars.iter().map(|c| Value::new_str(c.to_string())).collect()
            } else {
                text.split(&sep as &str).map(Value::new_str).collect()
            };
            Value::new_array(parts)
        }
        "repeat" => {
            let times = index(0);
            if text.len().saturating_mul(times) > MAX_STRING_LENGTH {
                return Err(Trap::ArrayTooLong);
            }
            Value::new_str(text.repeat(times))
        }
        "toUpperCase" => Value::new_str(text.to_uppercase()),
        "toLowerCase" => Value::new_str(text.to_lowercase()),
        "trim" => Value::new_str(text.trim()),
        other => return Err(Trap::NoSuchMethod(other.to_string())),
    })
}

/// `{x, y, z}` を読む。
fn components(value: &Value) -> [f32; 3] {
    match value {
        Value::Vector(v) => *v.borrow(),
        // 数値を渡されたら全成分に同じ値。`v.mult(2)` の形。
        other => {
            let n = other.as_f32();
            [n, n, n]
        }
    }
}

/// ベクトルのメソッドを 1 つ実行する。
///
/// `add` などは p5 と同じく自分を書き換え、自分を返す。数珠つなぎに書ける。
fn vector_method(
    name: &str,
    receiver: &Value,
    slot: &VectorRef,
    args: &[Value],
) -> Result<Value, Trap> {
    let me = *slot.borrow();
    let scalar = |i: usize| args.get(i).map_or(0.0, Value::as_f32);
    // 引数はベクトル 1 つのことも、成分を並べたこともある。
    let operand = || {
        if args.len() >= 2 {
            [scalar(0), scalar(1), scalar(2)]
        } else {
            components(args.first().unwrap_or(&Value::Float(0.0)))
        }
    };
    let mag = (me[0] * me[0] + me[1] * me[1] + me[2] * me[2]).sqrt();

    let mutate = |v: [f32; 3]| {
        *slot.borrow_mut() = v;
        receiver.clone()
    };

    Ok(match name {
        "add" => {
            let o = operand();
            mutate([me[0] + o[0], me[1] + o[1], me[2] + o[2]])
        }
        "sub" => {
            let o = operand();
            mutate([me[0] - o[0], me[1] - o[1], me[2] - o[2]])
        }
        "mult" => {
            let o = components(args.first().unwrap_or(&Value::Float(1.0)));
            mutate([me[0] * o[0], me[1] * o[1], me[2] * o[2]])
        }
        "div" => {
            // 0 で割ると無限大になる。p5 と同じく落としはしない。
            let o = components(args.first().unwrap_or(&Value::Float(1.0)));
            mutate([me[0] / o[0], me[1] / o[1], me[2] / o[2]])
        }
        "set" => mutate(operand()),
        "normalize" => {
            let d = if mag > 1e-9 { mag } else { 1.0 };
            mutate([me[0] / d, me[1] / d, me[2] / d])
        }
        "limit" => {
            let max = scalar(0);
            if mag > max && mag > 1e-9 {
                let k = max / mag;
                mutate([me[0] * k, me[1] * k, me[2] * k])
            } else {
                receiver.clone()
            }
        }
        "setMag" => {
            let d = if mag > 1e-9 { mag } else { 1.0 };
            let k = scalar(0) / d;
            mutate([me[0] * k, me[1] * k, me[2] * k])
        }
        "rotate" => {
            // 2 次元の回転。z はそのまま。
            let (sin, cos) = scalar(0).sin_cos();
            mutate([me[0] * cos - me[1] * sin, me[0] * sin + me[1] * cos, me[2]])
        }
        "lerp" => {
            let o = components(args.first().unwrap_or(&Value::Float(0.0)));
            let t = scalar(1);
            mutate([
                me[0] + (o[0] - me[0]) * t,
                me[1] + (o[1] - me[1]) * t,
                me[2] + (o[2] - me[2]) * t,
            ])
        }
        "copy" => Value::new_vector(me[0], me[1], me[2]),

        // 数を返すもの。自分は変えない。
        "mag" => Value::Float(mag),
        "magSq" => Value::Float(me[0] * me[0] + me[1] * me[1] + me[2] * me[2]),
        "heading" => Value::Float(me[1].atan2(me[0])),
        "dist" => {
            let o = components(args.first().unwrap_or(&Value::Float(0.0)));
            let d = [me[0] - o[0], me[1] - o[1], me[2] - o[2]];
            Value::Float((d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt())
        }
        "dot" => {
            let o = operand();
            Value::Float(me[0] * o[0] + me[1] * o[1] + me[2] * o[2])
        }
        "cross" => {
            let o = components(args.first().unwrap_or(&Value::Float(0.0)));
            Value::new_vector(
                me[1] * o[2] - me[2] * o[1],
                me[2] * o[0] - me[0] * o[2],
                me[0] * o[1] - me[1] * o[0],
            )
        }
        "angleBetween" => {
            let o = components(args.first().unwrap_or(&Value::Float(0.0)));
            let om = (o[0] * o[0] + o[1] * o[1] + o[2] * o[2]).sqrt();
            if mag < 1e-9 || om < 1e-9 {
                Value::Float(0.0)
            } else {
                let cos = (me[0] * o[0] + me[1] * o[1] + me[2] * o[2]) / (mag * om);
                Value::Float(cos.clamp(-1.0, 1.0).acos())
            }
        }
        other => return Err(Trap::NoSuchMethod(other.to_string())),
    })
}

/// 文字列の長さの上限 (バイト)。連結を繰り返す作品で青天井にしない。
const MAX_STRING_LENGTH: usize = 1 << 20;

/// 配列の長さの上限。1 フレームでメモリを食い潰さないための歯止め。
const MAX_ARRAY_LENGTH: usize = 1 << 20;

/// ビット演算のために整数へ寄せる。
///
/// JavaScript も Java も、ビット演算は 32bit 整数として行う。小数は切り捨てる。
fn to_i32(v: &Value) -> i32 {
    match v {
        Value::Int(i) => *i,
        Value::Bool(b) => *b as i32,
        other => {
            let f = other.as_f32();
            if f.is_finite() { f.trunc() as i32 } else { 0 }
        }
    }
}

fn bitwise(op: Op, a: i32, b: i32) -> Result<i32, Trap> {
    // シフト量は下位 5bit だけを見る。Java と JavaScript の両方がそう決めている。
    let shift = (b as u32) & 31;
    Ok(match op {
        Op::BitAnd => a & b,
        Op::BitOr => a | b,
        Op::BitXor => a ^ b,
        Op::Shl => a.wrapping_shl(shift),
        Op::Shr => a.wrapping_shr(shift),
        Op::UShr => ((a as u32).wrapping_shr(shift)) as i32,
        _ => return Err(Trap::Internal(format!("ビット演算ではない: {op:?}"))),
    })
}

fn arithmetic(op: Op, lhs: Value, rhs: Value) -> Result<Value, Trap> {
    // 両方 int なら int のまま計算する (Java と同じ整数演算)。
    if let (Value::Int(a), Value::Int(b)) = (&lhs, &rhs) {
        let (a, b) = (*a, *b);
        return Ok(Value::Int(match op {
            Op::Add => a.wrapping_add(b),
            Op::Sub => a.wrapping_sub(b),
            Op::Mul => a.wrapping_mul(b),
            Op::Div => {
                if b == 0 {
                    return Err(Trap::DivideByZero);
                }
                a.wrapping_div(b)
            }
            Op::Rem => {
                if b == 0 {
                    return Err(Trap::DivideByZero);
                }
                a.wrapping_rem(b)
            }
            _ => return Err(Trap::Internal(format!("算術命令ではない: {op:?}"))),
        }));
    }

    let (a, b) = (lhs.as_f32(), rhs.as_f32());
    Ok(Value::Float(match op {
        Op::Add => a + b,
        Op::Sub => a - b,
        Op::Mul => a * b,
        // 浮動小数の 0 除算は Java と同じく無限大にする。
        Op::Div => a / b,
        Op::Rem => a % b,
        _ => return Err(Trap::Internal(format!("算術命令ではない: {op:?}"))),
    }))
}

/// 負の添字は末尾から数える。範囲外は端で止める。JavaScript と同じ。
fn clamp_index(len: usize, at: i32) -> usize {
    if at < 0 { (len as i32 + at).max(0) as usize } else { (at as usize).min(len) }
}

/// `slice()` / `fill()` の範囲。省略すると全体。
fn slice_range(len: usize, from: Option<&Value>, to: Option<&Value>) -> (usize, usize) {
    let start = clamp_index(len, from.map(Value::as_i32).unwrap_or(0));
    let end = match to {
        Some(Value::Undefined) | None => len,
        Some(v) => clamp_index(len, v.as_i32()),
    };
    (start, end.max(start))
}

fn compare(op: Op, lhs: Value, rhs: Value) -> bool {
    // 文字列どうしは辞書順で比べる。
    if let (Value::Str(a), Value::Str(b)) = (&lhs, &rhs) {
        return match op {
            Op::Eq => a == b,
            Op::Ne => a != b,
            Op::Lt => a < b,
            Op::Le => a <= b,
            Op::Gt => a > b,
            Op::Ge => a >= b,
            _ => false,
        };
    }
    if let (Value::Bool(a), Value::Bool(b)) = (&lhs, &rhs) {
        let (a, b) = (*a, *b);
        return match op {
            Op::Eq => a == b,
            Op::Ne => a != b,
            // boolean の大小比較は Java にはないが、ここで弾くより順序を決めておく。
            _ => compare(op, Value::Int(a as i32), Value::Int(b as i32)),
        };
    }
    // 参照どうしの等値は同一性で見る。
    if matches!(op, Op::Eq | Op::Ne)
        && matches!(
            lhs,
            Value::Array(_)
                | Value::Object(_)
                | Value::Vector(_)
                | Value::Function(_)
                | Value::NativeFn(_)
        )
    {
        let same = lhs == rhs;
        return if op == Op::Eq { same } else { !same };
    }

    let (a, b) = (lhs.as_f32(), rhs.as_f32());
    match op {
        Op::Eq => a == b,
        Op::Ne => a != b,
        Op::Lt => a < b,
        Op::Le => a <= b,
        Op::Gt => a > b,
        Op::Ge => a >= b,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compiler::compile;
    use crate::parser::parse;

    /// スケッチを 1 フレーム実行し、`Graphics` を返す。
    fn run(source: &str) -> Result<Graphics, Trap> {
        run_frames(source, 1)
    }

    fn run_frames(source: &str, frames: u64) -> Result<Graphics, Trap> {
        let ast = parse(source).expect("パースに成功する");
        let program = compile(&ast).expect("コンパイルに成功する");
        let mut vm = Vm::new(&program, 12345);
        let mut g = Graphics::new();

        g.begin_frame(200.0, 100.0);
        vm.init_globals(&program, &mut g, DEFAULT_FRAME_BUDGET)?;
        if let Some(setup) = program.setup {
            vm.call(&program, setup, &mut g, DEFAULT_FRAME_BUDGET)?;
        }
        for frame in 1..=frames {
            g.begin_frame(200.0, 100.0);
            g.frame_count = frame;
            if let Some(draw) = program.draw {
                vm.call(&program, draw, &mut g, DEFAULT_FRAME_BUDGET)?;
            }
        }
        Ok(g)
    }

    /// 単一の式を評価して返す。`out` グローバルへ書かせて読み出す。
    fn eval(expr: &str) -> Value {
        let source = format!("float out = 0;\nvoid draw() {{ out = {expr}; }}");
        let ast = parse(&source).expect("パースに成功する");
        let program = compile(&ast).expect("コンパイルに成功する");
        let mut vm = Vm::new(&program, 1);
        let mut g = Graphics::new();
        g.begin_frame(200.0, 100.0);
        vm.init_globals(&program, &mut g, DEFAULT_FRAME_BUDGET).expect("初期化できる");
        vm.call(&program, program.draw.expect("draw がある"), &mut g, DEFAULT_FRAME_BUDGET)
            .expect("実行できる");
        vm.globals[0].clone()
    }

    fn eval_int(expr: &str) -> i32 {
        let source = format!("int out = 0;\nvoid draw() {{ out = {expr}; }}");
        let ast = parse(&source).expect("パースに成功する");
        let program = compile(&ast).expect("コンパイルに成功する");
        let mut vm = Vm::new(&program, 1);
        let mut g = Graphics::new();
        g.begin_frame(200.0, 100.0);
        vm.init_globals(&program, &mut g, DEFAULT_FRAME_BUDGET).expect("初期化できる");
        vm.call(&program, program.draw.expect("draw がある"), &mut g, DEFAULT_FRAME_BUDGET)
            .expect("実行できる");
        vm.globals[0].as_i32()
    }

    #[test]
    fn arithmetic_follows_precedence() {
        assert_eq!(eval("1 + 2 * 3"), Value::Float(7.0));
        assert_eq!(eval("(1 + 2) * 3"), Value::Float(9.0));
        assert_eq!(eval("10 - 3 - 2"), Value::Float(5.0));
    }

    #[test]
    fn integer_division_truncates_like_java() {
        assert_eq!(eval_int("7 / 2"), 3);
        assert_eq!(eval_int("-7 / 2"), -3);
        // 片方が float なら浮動小数の割り算になる。
        assert_eq!(eval("7 / 2.0"), Value::Float(3.5));
    }

    #[test]
    fn integer_modulo_works() {
        assert_eq!(eval_int("7 % 3"), 1);
        assert_eq!(eval_int("-7 % 3"), -1);
    }

    #[test]
    fn dividing_an_integer_by_zero_traps() {
        let source = "int out = 0;\nvoid draw() { out = 1 / 0; }";
        let program = compile(&parse(source).expect("パース")).expect("コンパイル");
        let mut vm = Vm::new(&program, 1);
        let mut g = Graphics::new();
        g.begin_frame(10.0, 10.0);
        vm.init_globals(&program, &mut g, DEFAULT_FRAME_BUDGET).expect("初期化");
        let err = vm
            .call(&program, program.draw.expect("draw"), &mut g, DEFAULT_FRAME_BUDGET)
            .unwrap_err();
        assert_eq!(err, Trap::DivideByZero);
    }

    #[test]
    fn comparisons_and_logic() {
        assert_eq!(eval("1 < 2 ? 10 : 20"), Value::Float(10.0));
        assert_eq!(eval("1 > 2 ? 10 : 20"), Value::Float(20.0));
        assert_eq!(eval("(1 < 2 && 3 < 4) ? 1 : 0"), Value::Float(1.0));
        assert_eq!(eval("(1 > 2 || 3 > 4) ? 1 : 0"), Value::Float(0.0));
    }

    #[test]
    fn logical_operators_short_circuit() {
        // 右辺を評価すると 0 除算になる。短絡すれば trap しない。
        assert_eq!(eval_int("(false && (1 / 0) > 0) ? 1 : 0"), 0);
        assert_eq!(eval_int("(true || (1 / 0) > 0) ? 1 : 0"), 1);
    }

    #[test]
    fn for_loop_runs_the_expected_number_of_times() {
        let source = "
            int total = 0;
            void draw() {
              for (int i = 0; i < 10; i++) {
                total = total + i;
              }
            }";
        let program = compile(&parse(source).expect("パース")).expect("コンパイル");
        let mut vm = Vm::new(&program, 1);
        let mut g = Graphics::new();
        g.begin_frame(10.0, 10.0);
        vm.init_globals(&program, &mut g, DEFAULT_FRAME_BUDGET).expect("初期化");
        vm.call(&program, program.draw.expect("draw"), &mut g, DEFAULT_FRAME_BUDGET)
            .expect("実行");
        assert_eq!(vm.globals[0], Value::Int(45));
    }

    #[test]
    fn while_loop_and_compound_assignment() {
        let source = "
            int total = 0;
            void draw() {
              int i = 0;
              while (i < 5) {
                total += i * 2;
                i++;
              }
            }";
        let program = compile(&parse(source).expect("パース")).expect("コンパイル");
        let mut vm = Vm::new(&program, 1);
        let mut g = Graphics::new();
        g.begin_frame(10.0, 10.0);
        vm.init_globals(&program, &mut g, DEFAULT_FRAME_BUDGET).expect("初期化");
        vm.call(&program, program.draw.expect("draw"), &mut g, DEFAULT_FRAME_BUDGET)
            .expect("実行");
        assert_eq!(vm.globals[0], Value::Int(20));
    }

    #[test]
    fn user_functions_receive_arguments_and_return_values() {
        let source = "
            float out = 0;
            void draw() { out = add(2, 3) + twice(8); }
            float add(float a, float b) { return a + b; }
            float twice(float x) { return x * 2; }";
        let program = compile(&parse(source).expect("パース")).expect("コンパイル");
        let mut vm = Vm::new(&program, 1);
        let mut g = Graphics::new();
        g.begin_frame(10.0, 10.0);
        vm.init_globals(&program, &mut g, DEFAULT_FRAME_BUDGET).expect("初期化");
        vm.call(&program, program.draw.expect("draw"), &mut g, DEFAULT_FRAME_BUDGET)
            .expect("実行");
        assert_eq!(vm.globals[0], Value::Float(21.0));
    }

    #[test]
    fn recursion_works() {
        let source = "
            int out = 0;
            void draw() { out = fib(10); }
            int fib(int n) { if (n < 2) { return n; } return fib(n - 1) + fib(n - 2); }";
        let program = compile(&parse(source).expect("パース")).expect("コンパイル");
        let mut vm = Vm::new(&program, 1);
        let mut g = Graphics::new();
        g.begin_frame(10.0, 10.0);
        vm.init_globals(&program, &mut g, DEFAULT_FRAME_BUDGET).expect("初期化");
        vm.call(&program, program.draw.expect("draw"), &mut g, DEFAULT_FRAME_BUDGET)
            .expect("実行");
        assert_eq!(vm.globals[0], Value::Int(55));
    }

    #[test]
    fn runaway_recursion_traps_instead_of_overflowing_the_host_stack() {
        let source = "
            int out = 0;
            void draw() { out = boom(1); }
            int boom(int n) { return boom(n + 1); }";
        let program = compile(&parse(source).expect("パース")).expect("コンパイル");
        let mut vm = Vm::new(&program, 1);
        let mut g = Graphics::new();
        g.begin_frame(10.0, 10.0);
        vm.init_globals(&program, &mut g, DEFAULT_FRAME_BUDGET).expect("初期化");
        let err = vm
            .call(&program, program.draw.expect("draw"), &mut g, DEFAULT_FRAME_BUDGET)
            .unwrap_err();
        assert_eq!(err, Trap::CallDepthExceeded);
    }

    #[test]
    fn an_infinite_loop_hits_the_frame_budget() {
        let source = "void draw() { while (true) { point(0, 0); } }";
        let program = compile(&parse(source).expect("パース")).expect("コンパイル");
        let mut vm = Vm::new(&program, 1);
        let mut g = Graphics::new();
        g.begin_frame(10.0, 10.0);
        let err = vm.call(&program, program.draw.expect("draw"), &mut g, 10_000).unwrap_err();
        assert_eq!(err, Trap::BudgetExceeded);
    }

    #[test]
    fn globals_persist_between_frames() {
        let source = "
            int ticks = 0;
            void draw() { ticks++; }";
        let program = compile(&parse(source).expect("パース")).expect("コンパイル");
        let mut vm = Vm::new(&program, 1);
        let mut g = Graphics::new();
        g.begin_frame(10.0, 10.0);
        vm.init_globals(&program, &mut g, DEFAULT_FRAME_BUDGET).expect("初期化");
        for _ in 0..5 {
            vm.call(&program, program.draw.expect("draw"), &mut g, DEFAULT_FRAME_BUDGET)
                .expect("実行");
        }
        assert_eq!(vm.globals[0], Value::Int(5));
    }

    #[test]
    fn builtin_variables_read_the_graphics_state() {
        let g = run("void draw() { rect(0, 0, width, height); }").expect("実行");
        assert!(!g.draw_list().is_empty());

        let source = "int w = 0;\nvoid draw() { w = width; }";
        let program = compile(&parse(source).expect("パース")).expect("コンパイル");
        let mut vm = Vm::new(&program, 1);
        let mut gg = Graphics::new();
        gg.begin_frame(640.0, 480.0);
        vm.init_globals(&program, &mut gg, DEFAULT_FRAME_BUDGET).expect("初期化");
        vm.call(&program, program.draw.expect("draw"), &mut gg, DEFAULT_FRAME_BUDGET)
            .expect("実行");
        assert_eq!(vm.globals[0], Value::Int(640));
    }

    #[test]
    fn drawing_calls_reach_the_graphics_layer() {
        let g = run("void draw() { background(0); fill(255, 0, 0); ellipse(50, 50, 20, 20); }")
            .expect("実行");
        assert!(!g.draw_list().is_empty());
        assert_eq!(g.draw_list().clear, Some(tsubu_renderer::Color::rgba(0.0, 0.0, 0.0, 1.0)));
    }

    #[test]
    fn setup_runs_before_the_first_frame() {
        let source = "
            int value = 0;
            void setup() { value = 7; }
            void draw() { value++; }";
        let program = compile(&parse(source).expect("パース")).expect("コンパイル");
        let mut vm = Vm::new(&program, 1);
        let mut g = Graphics::new();
        g.begin_frame(10.0, 10.0);
        vm.init_globals(&program, &mut g, DEFAULT_FRAME_BUDGET).expect("初期化");
        vm.call(&program, program.setup.expect("setup"), &mut g, DEFAULT_FRAME_BUDGET)
            .expect("setup");
        vm.call(&program, program.draw.expect("draw"), &mut g, DEFAULT_FRAME_BUDGET)
            .expect("draw");
        assert_eq!(vm.globals[0], Value::Int(8));
    }

    #[test]
    fn random_is_reproducible_for_a_given_seed() {
        let source = "float out = 0;\nvoid draw() { out = random(100); }";
        let program = compile(&parse(source).expect("パース")).expect("コンパイル");

        let sample = |seed| {
            let mut vm = Vm::new(&program, seed);
            let mut g = Graphics::new();
            g.begin_frame(10.0, 10.0);
            vm.init_globals(&program, &mut g, DEFAULT_FRAME_BUDGET).expect("初期化");
            vm.call(&program, program.draw.expect("draw"), &mut g, DEFAULT_FRAME_BUDGET)
                .expect("実行");
            vm.globals[0].as_f32()
        };

        assert_eq!(sample(42), sample(42));
        assert!((0.0..100.0).contains(&sample(42)));
    }

    #[test]
    fn many_frames_do_not_grow_the_stack() {
        let g = run_frames("void draw() { background(0); point(frameCount % 100, 10); }", 200)
            .expect("実行");
        assert!(!g.draw_list().is_empty());
    }
}
