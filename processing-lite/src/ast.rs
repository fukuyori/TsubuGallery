//! 構文木 (設計書 §13 の AST)。
//!
//! Lexer と Bytecode Compiler の間の唯一の受け渡し形式。将来 p5.js subset などの
//! フロントエンドを足すときは、ここへ落とし込めば以降を共有できる (設計書 §23.2)。

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Type {
    Void,
    Int,
    Float,
    Boolean,
    /// `String`。
    Str,
    /// 1 次元配列。要素の型を持つ。
    ///
    /// `Box` を使わず種類を並べているのは [`Type`] を `Copy` のままにするため。
    /// つぶやきの長さでは多次元配列はまず出てこないので、これで足りる。
    IntArray,
    FloatArray,
    BooleanArray,
    /// `PVector`。中身は p5 の `createVector()` と同じもの。
    Vector,
    VectorArray,
    /// ユーザー定義クラスの実体。中身はオブジェクト。
    Instance,
    InstanceArray,
    StrArray,
    /// 2 次元配列。つぶやきの長さでは 3 次元まではまず出てこない。
    IntArray2,
    FloatArray2,
    BooleanArray2,
}

impl Type {
    pub fn name(self) -> &'static str {
        match self {
            Type::Void => "void",
            Type::Int => "int",
            Type::Float => "float",
            Type::Boolean => "boolean",
            Type::Str => "String",
            Type::IntArray => "int[]",
            Type::FloatArray => "float[]",
            Type::BooleanArray => "boolean[]",
            Type::Vector => "PVector",
            Type::VectorArray => "PVector[]",
            Type::Instance => "object",
            Type::InstanceArray => "object[]",
            Type::StrArray => "String[]",
            Type::IntArray2 => "int[][]",
            Type::FloatArray2 => "float[][]",
            Type::BooleanArray2 => "boolean[][]",
        }
    }

    /// 配列型か。
    pub fn is_array(self) -> bool {
        matches!(
            self,
            Type::IntArray
                | Type::FloatArray
                | Type::BooleanArray
                | Type::VectorArray
                | Type::StrArray
                | Type::IntArray2
                | Type::FloatArray2
                | Type::BooleanArray2
                | Type::InstanceArray
        )
    }

    /// 配列にした型。配列の配列は作れないので `None`。
    pub fn to_array(self) -> Option<Type> {
        Some(match self {
            Type::Int => Type::IntArray,
            Type::Float => Type::FloatArray,
            Type::Boolean => Type::BooleanArray,
            Type::Vector => Type::VectorArray,
            Type::Str => Type::StrArray,
            Type::IntArray => Type::IntArray2,
            Type::FloatArray => Type::FloatArray2,
            Type::BooleanArray => Type::BooleanArray2,
            Type::Instance => Type::InstanceArray,
            _ => return None,
        })
    }

    /// 配列の要素の型。配列でなければ `None`。
    pub fn element(self) -> Option<Type> {
        Some(match self {
            Type::IntArray => Type::Int,
            Type::FloatArray => Type::Float,
            Type::BooleanArray => Type::Boolean,
            Type::VectorArray => Type::Vector,
            Type::StrArray => Type::Str,
            Type::IntArray2 => Type::IntArray,
            Type::FloatArray2 => Type::FloatArray,
            Type::BooleanArray2 => Type::BooleanArray,
            Type::InstanceArray => Type::Instance,
            _ => return None,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnaryOp {
    Neg,
    Not,
    /// `~x`。整数として反転する。
    BitNot,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,

    /// ビット演算。両辺を `int` として扱う (設計書 §14.1)。
    BitAnd,
    BitOr,
    BitXor,
    Shl,
    Shr,
    /// 符号なし右シフト。
    UShr,
}

/// 短絡評価するので、通常の二項演算とは分けて持つ。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LogicalOp {
    And,
    Or,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AssignOp {
    Set,
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    BitAnd,
    BitOr,
    BitXor,
    Shl,
    Shr,
}

impl AssignOp {
    /// `x op= v` を `x = x op v` に展開するための演算子。`=` なら `None`。
    pub fn binary(self) -> Option<BinaryOp> {
        Some(match self {
            AssignOp::Set => return None,
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
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Expr {
    Int(i32),
    Float(f32),
    Bool(bool),
    Str(String),
    Var(String),
    Unary { op: UnaryOp, operand: Box<Expr> },
    Binary { op: BinaryOp, lhs: Box<Expr>, rhs: Box<Expr> },
    Logical { op: LogicalOp, lhs: Box<Expr>, rhs: Box<Expr> },
    Ternary { cond: Box<Expr>, then: Box<Expr>, other: Box<Expr> },
    Call { name: String, args: Vec<Expr>, line: u32, column: u32 },
    /// `(int)x` のようなキャスト。
    Cast { ty: Type, operand: Box<Expr> },
    /// `a[i]`
    Index { target: Box<Expr>, index: Box<Expr>, line: u32, column: u32 },
    /// `new float[n]` と `new float[r][c]`。要素は 0 で埋める。
    NewArray { ty: Type, sizes: Vec<Expr> },
    /// `{1, 2, 3}`
    ArrayLit { ty: Type, items: Vec<Expr> },
    /// `a.length`
    ArrayLen { target: Box<Expr> },
    /// `v.x`
    Field { target: Box<Expr>, name: String },
    /// `v.add(u)`
    MethodCall { target: Box<Expr>, name: String, args: Vec<Expr>, line: u32, column: u32 },
    /// `new PVector(x, y)`
    NewVector { args: Vec<Expr> },
    /// `new Thing(...)`。ユーザー定義クラスの生成。
    New { class: String, args: Vec<Expr>, line: u32, column: u32 },
    /// メソッドの中の `this`。
    This,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Stmt {
    VarDecl { ty: Type, name: String, init: Option<Expr>, line: u32, column: u32 },
    Assign { name: String, op: AssignOp, value: Expr, line: u32, column: u32 },
    /// `a[i] = v` / `a[i] += v`
    AssignIndex { target: Expr, index: Expr, op: AssignOp, value: Expr, line: u32, column: u32 },
    /// `v.x = 1` / `v.x += 1`
    AssignField { target: Expr, name: String, op: AssignOp, value: Expr, line: u32, column: u32 },
    /// `a[i]++`
    IncDecIndex { target: Expr, index: Expr, delta: i32, line: u32, column: u32 },
    /// `i++` / `i--`。
    IncDec { name: String, delta: i32, line: u32, column: u32 },
    If { cond: Expr, then: Box<Stmt>, otherwise: Option<Box<Stmt>> },
    While { cond: Expr, body: Box<Stmt> },
    For { init: Option<Box<Stmt>>, cond: Option<Expr>, update: Option<Box<Stmt>>, body: Box<Stmt> },
    Block(Vec<Stmt>),
    Expr(Expr),
    /// 一番内側のループを抜ける。
    /// `for (int v : a)`。
    ForEach {
        ty: Type,
        name: String,
        iterable: Expr,
        body: Box<Stmt>,
        line: u32,
        column: u32,
    },
    /// `switch`。Java と同じく、`break` が無ければ次の case へ落ちる。
    Switch { value: Expr, cases: Vec<SwitchCase>, line: u32, column: u32 },
    Break { line: u32, column: u32 },
    /// 一番内側のループの次の回へ進む。
    Continue { line: u32, column: u32 },
    Return { value: Option<Expr>, line: u32, column: u32 },
}

/// `switch` の 1 ラベル分。
#[derive(Clone, Debug, PartialEq)]
pub struct SwitchCase {
    /// `case 1:` の値。`default:` なら `None`。
    pub label: Option<Expr>,
    pub body: Vec<Stmt>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Function {
    pub name: String,
    pub return_type: Type,
    pub params: Vec<(Type, String)>,
    pub body: Vec<Stmt>,
    pub line: u32,
    pub column: u32,
}

/// ユーザー定義クラス (設計書 §14.1 の「Java 固有の高度な機能」)。
#[derive(Clone, Debug, PartialEq)]
pub struct Class {
    pub name: String,
    /// 名前と型。生成時に既定値で埋める。
    pub fields: Vec<(Type, String)>,
    /// クラス名と同じ名前の関数。省略できる。
    pub constructor: Option<Function>,
    pub methods: Vec<Function>,
    pub line: u32,
    pub column: u32,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Ast {
    /// トップレベルの変数宣言。`setup()` の前に一度だけ実行される。
    pub globals: Vec<Stmt>,
    pub functions: Vec<Function>,
    pub classes: Vec<Class>,
}

impl Ast {
    pub fn function(&self, name: &str) -> Option<&Function> {
        self.functions.iter().find(|f| f.name == name)
    }
}
