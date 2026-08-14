//! p5.js subset の構文木。

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    /// `**`。右結合で、単項演算より強く結びつく。
    Pow,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,

    /// ビット演算。JavaScript と同じく 32bit 整数として扱う。
    BitAnd,
    BitOr,
    BitXor,
    Shl,
    Shr,
    UShr,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LogicalOp {
    And,
    Or,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnaryOp {
    Neg,
    Not,
    BitNot,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AssignOp {
    Set,
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    Pow,
    BitAnd,
    BitOr,
    BitXor,
    Shl,
    Shr,
}

/// 配列リテラルの要素。`[...a, b]` のように展開を混ぜられる。
#[derive(Clone, Debug, PartialEq)]
pub enum ArrayElem {
    Item(Expr),
    /// `...xs`
    Spread(Expr),
}

/// 関数の引数。既定値を書けるのは JavaScript の書き方。
///
/// 縮めたコードでは、既定値を「途中の計算を置く場所」として使うことが多い。
#[derive(Clone, Debug, PartialEq)]
pub struct Param {
    pub name: String,
    pub default: Option<Expr>,
}

/// 代入や `++` の書き込み先。
#[derive(Clone, Debug, PartialEq)]
pub enum Target {
    Var(String),
    /// `obj.name`
    Member(Box<Expr>, String),
    /// `obj[index]`
    Index(Box<Expr>, Box<Expr>),
    /// `[a, b] = ...` の分割代入。`None` は読み飛ばす位置 (`[, b]`)。
    Destructure(Vec<Option<Target>>),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Expr {
    /// 文字列。
    Str(String),
    Number(f32),
    Bool(bool),
    Undefined,
    Ident(String),
    Array(Vec<ArrayElem>),
    Object(Vec<(String, Expr)>),
    Arrow {
        params: Vec<Param>,
        body: Box<ArrowBody>,
    },
    Unary {
        op: UnaryOp,
        operand: Box<Expr>,
    },
    Binary {
        op: BinaryOp,
        lhs: Box<Expr>,
        rhs: Box<Expr>,
    },
    Logical {
        op: LogicalOp,
        lhs: Box<Expr>,
        rhs: Box<Expr>,
    },
    Ternary {
        cond: Box<Expr>,
        then: Box<Expr>,
        other: Box<Expr>,
    },
    Assign {
        target: Target,
        op: AssignOp,
        value: Box<Expr>,
    },
    /// `i++` / `--i`。`prefix` が false なら後置。
    Update {
        target: Target,
        delta: f32,
        prefix: bool,
    },
    /// `f(...xs)` の `...xs`。引数の並びの中にだけ現れる。
    Spread(Box<Expr>),
    Call {
        callee: Box<Expr>,
        args: Vec<Expr>,
        line: u32,
        column: u32,
    },
    Member {
        object: Box<Expr>,
        name: String,
    },
    Index {
        object: Box<Expr>,
        index: Box<Expr>,
    },
    /// カンマ演算子 `(a, b)`。左を捨てて右を返す。
    ///
    /// 縮めて書かれたコードでは、文の代わりによく使われる。
    Sequence(Box<Expr>, Box<Expr>),
}

#[derive(Clone, Debug, PartialEq)]
pub enum ArrowBody {
    /// `p => expr`
    Expr(Expr),
    /// `p => { ... }`
    Block(Vec<Stmt>),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Stmt {
    Expr(Expr),
    /// `let a = 1, b`
    Declare(Vec<(String, Option<Expr>)>),
    If {
        cond: Expr,
        then: Box<Stmt>,
        otherwise: Option<Box<Stmt>>,
    },
    For {
        init: Option<Box<Stmt>>,
        cond: Option<Expr>,
        update: Option<Expr>,
        body: Box<Stmt>,
    },
    While {
        cond: Expr,
        body: Box<Stmt>,
    },
    /// `for (v of xs)`。配列を順に回す。
    ForOf {
        name: String,
        /// `let` / `const` / `var` が書いてあったか。書いてなければ、
        /// JavaScript と同じくグローバルへの代入になる。
        declared: bool,
        iterable: Expr,
        body: Box<Stmt>,
    },
    Return(Option<Expr>),
    /// 一番内側のループを抜ける。
    Break { line: u32, column: u32 },
    /// 一番内側のループの次の回へ進む。
    Continue { line: u32, column: u32 },
    Block(Vec<Stmt>),
    /// `function f(a, b) { ... }`
    Function {
        name: String,
        params: Vec<Param>,
        body: Vec<Stmt>,
        line: u32,
        column: u32,
    },
}

/// プログラム全体。トップレベルの文の並び。
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Script {
    pub statements: Vec<Stmt>,
}
