//! 値と命令セット (設計書 §16)。
//!
//! スタック型 VM 向けの素朴な命令列。表示時にコンパイルしないための中間形式で
//! あり、将来 p5.js subset などを足すときの共通 IR でもある (設計書 §23.2)。

use std::cell::RefCell;
use std::rc::Rc;

use crate::ast::Type;
use crate::natives::{BuiltinVar, Native};

/// 配列の中身。`Rc` で共有し、参照が消えたら解放される。
pub type ArrayRef = Rc<RefCell<Vec<Value>>>;
/// オブジェクトの中身。鍵はコンパイル時に採番した番号 (`Program::keys`)。
///
/// つぶやき作品のオブジェクトは数個の要素しか持たないので、線形探索で足りる。
pub type ObjectRef = Rc<RefCell<Vec<(u16, Value)>>>;
/// ベクトルの中身。`[x, y, z]`。
pub type VectorRef = Rc<RefCell<[f32; 3]>>;

/// 実行時の値。
///
/// Processing (Java Mode) は前 4 つしか作らない。残りは p5.js フロントエンド用で、
/// どちらも同じ VM を通る (設計書 §23.2)。
#[derive(Clone, Debug)]
pub enum Value {
    Int(i32),
    Float(f32),
    Bool(bool),
    /// `void` 関数の戻り値。
    Void,

    /// JavaScript の `undefined`。
    Undefined,
    Array(ArrayRef),
    Object(ObjectRef),
    /// 文字列。書き換えないので `Rc` で共有する。
    Str(Rc<str>),
    /// `createVector()` / `new PVector()` が作るベクトル。
    ///
    /// オブジェクトで代用せず専用にしたのは、成分の名前をキー表に頼らずに
    /// 済ませるため。組み込み関数はキー表を見られない。
    Vector(VectorRef),
    /// ユーザー定義関数への参照。
    Function(u16),
    /// 組み込み関数への参照。`B = blendMode` のように値として持てる。
    NativeFn(Native),
}

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Value::Int(a), Value::Int(b)) => a == b,
            (Value::Float(a), Value::Float(b)) => a == b,
            (Value::Bool(a), Value::Bool(b)) => a == b,
            (Value::Void, Value::Void) | (Value::Undefined, Value::Undefined) => true,
            // 参照は同一性で比べる。JavaScript と同じ。
            (Value::Array(a), Value::Array(b)) => Rc::ptr_eq(a, b),
            (Value::Object(a), Value::Object(b)) => Rc::ptr_eq(a, b),
            (Value::Vector(a), Value::Vector(b)) => Rc::ptr_eq(a, b),
            // 文字列は中身で比べる。参照の同一性ではない。
            (Value::Str(a), Value::Str(b)) => a == b,
            (Value::Function(a), Value::Function(b)) => a == b,
            (Value::NativeFn(a), Value::NativeFn(b)) => a == b,
            _ => false,
        }
    }
}

impl Value {
    pub fn as_f32(&self) -> f32 {
        match self {
            Value::Int(v) => *v as f32,
            Value::Float(v) => *v,
            Value::Bool(v) => {
                if *v {
                    1.0
                } else {
                    0.0
                }
            }
            // JavaScript の `undefined` は数値にすると NaN。
            Value::Undefined => f32::NAN,
            Value::Void => 0.0,
            // 数として読める文字列は数にする。`"12" * 2` が 24 になる。
            Value::Str(s) => s.trim().parse().unwrap_or(f32::NAN),
            _ => f32::NAN,
        }
    }

    pub fn as_i32(&self) -> i32 {
        match self {
            Value::Int(v) => *v,
            // Java と同じくゼロ方向へ切り捨てる。
            Value::Float(v) => {
                if v.is_finite() {
                    *v as i32
                } else {
                    0
                }
            }
            Value::Bool(v) => *v as i32,
            _ => 0,
        }
    }

    pub fn truthy(&self) -> bool {
        match self {
            Value::Bool(v) => *v,
            Value::Int(v) => *v != 0,
            // NaN は偽。JavaScript と同じ。
            Value::Float(v) => *v != 0.0 && !v.is_nan(),
            Value::Void | Value::Undefined => false,
            // 空文字列は偽。JavaScript と同じ。
            Value::Str(s) => !s.is_empty(),
            // 配列・オブジェクト・関数は常に真。
            _ => true,
        }
    }

    /// 宣言された型へ寄せる。
    pub fn coerce(self, ty: Type) -> Value {
        match ty {
            Type::Int => Value::Int(self.as_i32()),
            Type::Float => Value::Float(self.as_f32()),
            Type::Boolean => Value::Bool(self.truthy()),
            Type::Void => Value::Void,
            // 文字列は数へ寄せない。表示用の文字へ直す。
            Type::Str => Value::new_str(self.to_display()),
            // 配列とベクトルは参照なので値を作り直さない。宣言した型と中身が
            // 食い違っていても、ここで直せることはない。
            Type::IntArray
            | Type::FloatArray
            | Type::BooleanArray
            | Type::Vector
            | Type::Instance
            | Type::InstanceArray
            | Type::VectorArray
            | Type::StrArray
            | Type::IntArray2
            | Type::FloatArray2
            | Type::BooleanArray2 => self,
        }
    }

    pub fn type_name(&self) -> &'static str {
        match self {
            Value::Int(_) => "int",
            Value::Float(_) => "float",
            Value::Bool(_) => "boolean",
            Value::Void => "void",
            Value::Undefined => "undefined",
            Value::Array(_) => "array",
            Value::Object(_) => "object",
            Value::Vector(_) => "vector",
            Value::Str(_) => "string",
            Value::Function(_) | Value::NativeFn(_) => "function",
        }
    }

    pub fn new_array(values: Vec<Value>) -> Value {
        Value::Array(Rc::new(RefCell::new(values)))
    }

    /// 文字列を作る。
    pub fn new_str(text: impl Into<Rc<str>>) -> Value {
        Value::Str(text.into())
    }

    /// 文字列としての中身。数値なども表示用の文字へ直す。
    pub fn to_display(&self) -> String {
        match self {
            Value::Str(s) => s.to_string(),
            Value::Int(v) => v.to_string(),
            // 整数なら小数点を出さない。Processing の `str()` と同じ。
            Value::Float(v) if v.fract() == 0.0 && v.is_finite() => format!("{}", *v as i64),
            Value::Float(v) => format!("{v}"),
            Value::Bool(v) => v.to_string(),
            Value::Void => String::new(),
            Value::Undefined => "undefined".to_string(),
            Value::Array(items) => items
                .borrow()
                .iter()
                .map(Value::to_display)
                .collect::<Vec<_>>()
                .join(","),
            Value::Vector(v) => {
                let v = v.borrow();
                format!("[{}, {}, {}]", v[0], v[1], v[2])
            }
            Value::Object(_) => "[object]".to_string(),
            Value::Function(_) | Value::NativeFn(_) => "[function]".to_string(),
        }
    }

    /// ベクトルを作る。
    pub fn new_vector(x: f32, y: f32, z: f32) -> Value {
        Value::Vector(Rc::new(RefCell::new([x, y, z])))
    }

    pub fn new_object() -> Value {
        Value::Object(Rc::new(RefCell::new(Vec::new())))
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Op {
    ConstInt(i32),
    ConstFloat(f32),
    ConstBool(bool),
    ConstUndefined,
    /// 文字列の定数表からひとつ積む。
    ConstStr(u16),
    /// ユーザー定義関数を値として積む。
    ConstFunction(u16),
    /// 組み込み関数を値として積む。`B = blendMode` のように使う。
    ConstNativeFn(Native),

    LoadLocal(u16),
    StoreLocal(u16),
    LoadGlobal(u16),
    StoreGlobal(u16),
    LoadBuiltin(BuiltinVar),

    Add,
    Sub,
    Mul,
    Div,
    Rem,
    Neg,
    Not,

    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,

    /// ビット演算。両辺を `int` へ寄せてから計算する。
    BitAnd,
    BitOr,
    BitXor,
    Shl,
    Shr,
    UShr,
    BitNot,

    /// スタックトップを複製する。短絡評価に使う。
    Dup,
    /// 上 2 つをまとめて複製する。`a[i] += v` の読み書きに使う。
    Dup2,
    /// `undefined` かどうか。引数の既定値を埋めるのに使う。
    IsUndefined,
    Pop,
    Jump(u32),
    JumpIfFalse(u32),
    JumpIfTrue(u32),

    /// 宣言された型へ寄せる。
    Coerce(Type),

    CallNative(Native, u8),
    /// ユーザー定義関数。`(関数インデックス, 引数の数)`。
    Call(u16, u8),

    Return,
    ReturnValue,

    // ---- p5.js フロントエンド向け -------------------------------------
    /// スタック上の n 個から配列を作る。
    NewArray(u16),
    /// 長さを取り出し、その数だけ既定値を並べた配列を作る。`new float[n]`。
    NewArrayOf(Type),
    /// `[行数, 列数]` を取り出して 2 次元配列を作る。`new float[r][c]`。
    NewArray2Of(Type),
    /// 配列の長さ。`a.length`。
    ArrayLen,
    /// `[array, value]` → `[array]`。配列の末尾へ 1 つ足す。
    ArrayPush,
    /// `[array, other]` → `[array]`。`other` の中身をすべて足す。展開に使う。
    ArrayExtend,
    /// 空のオブジェクトを積む。
    NewObject,
    /// `[obj, value]` → `[obj]`。オブジェクトリテラルの組み立て。
    InitProp(u16),
    /// `[obj]` → `[value]`。
    GetProp(u16),
    /// `[obj, value]` → `[value]`。代入は式なので値を残す。
    SetProp(u16),

    /// 引数を配列にまとめて渡す呼び出し。`f(...xs)` のように、個数が
    /// 実行時まで決まらないときに使う。積んである配列の中身が引数になる。
    CallNativeSpread(crate::natives::Native),
    CallMethodSpread(u16),
    CallValueSpread,
    /// `[target, index]` → `[value]`。
    GetIndex,
    /// `[target, index, value]` → `[value]`。
    SetIndex,
    /// スタックに積まれた値を関数として呼ぶ。
    CallValue(u8),
    /// `[obj, args...]` → `[result]`。配列のメソッド呼び出し。
    CallMethod(u16, u8),
}

#[derive(Clone, Debug)]
pub struct CompiledFunction {
    pub name: String,
    pub arity: u8,
    /// 引数を含むローカル変数の総数。
    pub local_count: u16,
    pub return_type: Type,
    pub code: Vec<Op>,
}

/// コンパイル済みスケッチ 1 本。
#[derive(Clone, Debug)]
pub struct Program {
    pub functions: Vec<CompiledFunction>,
    /// プロパティ名。命令は番号で参照する。
    pub keys: Vec<String>,
    /// 文字列リテラル。命令は番号で参照する。
    pub strings: Vec<String>,
    /// グローバル変数の名前と位置。実行には要らないが、診断で使う。
    pub global_names: std::collections::HashMap<String, u16>,
    /// グローバル変数を初期化する合成関数。`setup()` の前に一度だけ呼ぶ。
    ///
    /// ふつうの関数として持たせることで、VM の実行経路が 1 本で済む。
    pub globals_init: u16,
    pub global_count: u16,
    pub setup: Option<u16>,
    pub draw: Option<u16>,
}

impl Program {
    /// 番号から文字列リテラルを引く。
    pub fn string(&self, index: u16) -> &str {
        self.strings.get(index as usize).map_or("", String::as_str)
    }

    /// 名前からグローバルの位置を引く。診断とテスト用。
    pub fn global_slot(&self, name: &str) -> Option<u16> {
        self.global_names.get(name).copied()
    }

    pub fn key(&self, index: u16) -> &str {
        self.keys.get(index as usize).map_or("?", String::as_str)
    }
}

impl Program {
    /// 命令数。キャッシュ規模の目安に使う。
    pub fn instruction_count(&self) -> usize {
        self.functions.iter().map(|f| f.code.len()).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn float_to_int_truncates_toward_zero() {
        assert_eq!(Value::Float(1.9).as_i32(), 1);
        assert_eq!(Value::Float(-1.9).as_i32(), -1);
    }

    #[test]
    fn coerce_follows_the_declared_type() {
        assert_eq!(Value::Float(3.7).coerce(Type::Int), Value::Int(3));
        assert_eq!(Value::Int(3).coerce(Type::Float), Value::Float(3.0));
        assert_eq!(Value::Int(0).coerce(Type::Boolean), Value::Bool(false));
    }

    #[test]
    fn truthiness_matches_the_value() {
        assert!(Value::Bool(true).truthy());
        assert!(!Value::Int(0).truthy());
        assert!(Value::Float(0.5).truthy());
        assert!(!Value::Void.truthy());
    }
}
