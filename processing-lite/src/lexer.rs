//! 字句解析 (設計書 §13 の Lexer)。

use std::fmt;

/// コンパイル時のエラー。位置つきでユーザーへ返す (設計書 §25 の「構文エラー表示」)。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompileError {
    pub line: u32,
    pub column: u32,
    pub message: String,
}

impl CompileError {
    pub fn new(line: u32, column: u32, message: impl Into<String>) -> Self {
        Self { line, column, message: message.into() }
    }
}

impl fmt::Display for CompileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}行{}列: {}", self.line, self.column, self.message)
    }
}

impl std::error::Error for CompileError {}

#[derive(Clone, Debug, PartialEq)]
pub enum TokenKind {
    /// `int` などの整数リテラル。
    Int(i32),
    /// 小数点や指数を含むリテラル。
    Float(f32),
    Ident(String),
    /// `"..."` の文字列。
    Str(String),
    Keyword(Keyword),

    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    Assign,
    PlusAssign,
    MinusAssign,
    StarAssign,
    SlashAssign,
    Increment,
    Decrement,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    AndAnd,
    OrOr,
    Bang,
    /// ビット演算 (設計書 §14.1 の演算子)。
    Amp,
    Pipe,
    Caret,
    Tilde,
    Shl,
    Shr,
    /// 符号なし右シフト `>>>`。
    UShr,
    AmpAssign,
    PipeAssign,
    CaretAssign,
    ShlAssign,
    ShrAssign,
    PercentAssign,
    Question,
    Colon,
    Comma,
    Semicolon,
    LParen,
    RParen,
    LBrace,
    RBrace,
    LBracket,
    RBracket,
    /// `a.length` のための `.`。数値リテラルの中の `.` はここへ来ない。
    Dot,

    Eof,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Keyword {
    Void,
    Int,
    Float,
    Boolean,
    If,
    Else,
    For,
    While,
    Return,
    Break,
    Continue,
    New,
    Class,
    This,
    Switch,
    Case,
    Default,
    True,
    False,
}

impl Keyword {
    /// 語がキーワードなら返す。エディタの色分けからも使う。
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "void" => Keyword::Void,
            "int" => Keyword::Int,
            "float" => Keyword::Float,
            "boolean" => Keyword::Boolean,
            "if" => Keyword::If,
            "else" => Keyword::Else,
            "for" => Keyword::For,
            "while" => Keyword::While,
            "return" => Keyword::Return,
            "break" => Keyword::Break,
            "continue" => Keyword::Continue,
            "new" => Keyword::New,
            "class" => Keyword::Class,
            "this" => Keyword::This,
            "switch" => Keyword::Switch,
            "case" => Keyword::Case,
            "default" => Keyword::Default,
            "true" => Keyword::True,
            "false" => Keyword::False,
            _ => return None,
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    pub line: u32,
    pub column: u32,
}

pub fn tokenize(source: &str) -> Result<Vec<Token>, CompileError> {
    Lexer::new(source).run()
}

struct Lexer<'a> {
    chars: Vec<char>,
    pos: usize,
    line: u32,
    column: u32,
    source: std::marker::PhantomData<&'a str>,
}

impl<'a> Lexer<'a> {
    fn new(source: &'a str) -> Self {
        Self {
            chars: source.chars().collect(),
            pos: 0,
            line: 1,
            column: 1,
            source: std::marker::PhantomData,
        }
    }

    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn peek_at(&self, offset: usize) -> Option<char> {
        self.chars.get(self.pos + offset).copied()
    }

    fn bump(&mut self) -> Option<char> {
        let c = self.chars.get(self.pos).copied()?;
        self.pos += 1;
        if c == '\n' {
            self.line += 1;
            self.column = 1;
        } else {
            self.column += 1;
        }
        Some(c)
    }

    /// 次が `expected` なら消費する。
    fn eat(&mut self, expected: char) -> bool {
        if self.peek() == Some(expected) {
            self.bump();
            true
        } else {
            false
        }
    }

    fn run(mut self) -> Result<Vec<Token>, CompileError> {
        let mut tokens = Vec::new();

        loop {
            self.skip_trivia()?;
            let (line, column) = (self.line, self.column);
            let Some(c) = self.peek() else {
                tokens.push(Token { kind: TokenKind::Eof, line, column });
                return Ok(tokens);
            };

            let kind = if c.is_ascii_digit() || (c == '.' && self.peek_at(1).is_some_and(|d| d.is_ascii_digit())) {
                self.number()?
            } else if c.is_alphabetic() || c == '_' {
                self.word()
            } else if c == '\'' {
                self.char_literal()?
            } else if c == '"' {
                self.string_literal()?
            } else {
                self.operator()?
            };

            tokens.push(Token { kind, line, column });
        }
    }

    /// 空白とコメントを読み飛ばす。
    fn skip_trivia(&mut self) -> Result<(), CompileError> {
        loop {
            match self.peek() {
                Some(c) if c.is_whitespace() => {
                    self.bump();
                }
                Some('/') if self.peek_at(1) == Some('/') => {
                    while let Some(c) = self.peek() {
                        if c == '\n' {
                            break;
                        }
                        self.bump();
                    }
                }
                Some('/') if self.peek_at(1) == Some('*') => {
                    let (line, column) = (self.line, self.column);
                    self.bump();
                    self.bump();
                    loop {
                        match self.peek() {
                            None => {
                                return Err(CompileError::new(
                                    line,
                                    column,
                                    "コメントが閉じられていません",
                                ));
                            }
                            Some('*') if self.peek_at(1) == Some('/') => {
                                self.bump();
                                self.bump();
                                break;
                            }
                            _ => {
                                self.bump();
                            }
                        }
                    }
                }
                _ => return Ok(()),
            }
        }
    }

    fn number(&mut self) -> Result<TokenKind, CompileError> {
        let (line, column) = (self.line, self.column);
        let start = self.pos;
        let mut is_float = false;

        // `0xFF6B35` の 16 進。詰めた色を書くのに使う。
        if self.peek() == Some('0') && matches!(self.peek_at(1), Some('x' | 'X')) {
            self.bump();
            self.bump();
            let digits = self.pos;
            while self.peek().is_some_and(|c| c.is_ascii_hexdigit()) {
                self.bump();
            }
            if self.pos == digits {
                return Err(CompileError::new(line, column, "16 進数の桁がありません"));
            }
            let text: String = self.chars[digits..self.pos].iter().collect();
            // Java の int は 32bit。`0xFFFFFFFF` は -1 になる。
            let value = u32::from_str_radix(&text, 16)
                .map_err(|_| CompileError::new(line, column, "16 進数が大きすぎます"))?;
            return Ok(TokenKind::Int(value as i32));
        }

        while self.peek().is_some_and(|c| c.is_ascii_digit()) {
            self.bump();
        }
        if self.peek() == Some('.') && self.peek_at(1) != Some('.') {
            is_float = true;
            self.bump();
            while self.peek().is_some_and(|c| c.is_ascii_digit()) {
                self.bump();
            }
        }
        if matches!(self.peek(), Some('e' | 'E')) {
            is_float = true;
            self.bump();
            if matches!(self.peek(), Some('+' | '-')) {
                self.bump();
            }
            if !self.peek().is_some_and(|c| c.is_ascii_digit()) {
                return Err(CompileError::new(line, column, "指数部に数字がありません"));
            }
            while self.peek().is_some_and(|c| c.is_ascii_digit()) {
                self.bump();
            }
        }
        // Java の `1.0f` 表記も受ける。
        if matches!(self.peek(), Some('f' | 'F')) {
            is_float = true;
            self.bump();
        }

        let text: String = self.chars[start..self.pos].iter().filter(|c| !matches!(c, 'f' | 'F')).collect();

        if is_float {
            text.parse::<f32>()
                .map(TokenKind::Float)
                .map_err(|_| CompileError::new(line, column, format!("数値として読めません: {text}")))
        } else {
            match text.parse::<i32>() {
                Ok(v) => Ok(TokenKind::Int(v)),
                // int に収まらない整数リテラルは float として扱う。
                Err(_) => text.parse::<f32>().map(TokenKind::Float).map_err(|_| {
                    CompileError::new(line, column, format!("数値として読めません: {text}"))
                }),
            }
        }
    }

    /// `'a'` のような文字リテラル。Java と同じく文字コード (整数) として扱う。
    ///
    /// 文字列型は無いが、`key == 'a'` が書けないと `key` が使い物にならないので
    /// リテラルだけ用意する。
    fn char_literal(&mut self) -> Result<TokenKind, CompileError> {
        let (line, column) = (self.line, self.column);
        self.bump();

        let c = match self.bump() {
            Some('\\') => escape(self.bump(), line, column)?,
            Some('\'') => return Err(CompileError::new(line, column, "空の文字リテラルです")),
            Some(c) => c,
            None => return Err(CompileError::new(line, column, "文字リテラルが閉じていません")),
        };

        if !self.eat('\'') {
            return Err(CompileError::new(line, column, "文字リテラルが閉じていません"));
        }
        Ok(TokenKind::Int(c as i32))
    }

    /// `"..."` の文字列。
    fn string_literal(&mut self) -> Result<TokenKind, CompileError> {
        let (line, column) = (self.line, self.column);
        self.bump();

        let mut out = String::new();
        loop {
            match self.bump() {
                Some('"') => return Ok(TokenKind::Str(out)),
                Some('\\') => out.push(escape(self.bump(), line, column)?),
                // 途中で行が終わったら閉じ忘れとみなす。次の行まで飲み込むと
                // 誤りの場所が分からなくなる。
                Some('\n') | None => {
                    return Err(CompileError::new(line, column, "文字列が閉じていません"));
                }
                Some(c) => out.push(c),
            }
        }
    }

    fn word(&mut self) -> TokenKind {
        let start = self.pos;
        while self.peek().is_some_and(|c| c.is_alphanumeric() || c == '_') {
            self.bump();
        }
        let text: String = self.chars[start..self.pos].iter().collect();
        match Keyword::parse(&text) {
            Some(k) => TokenKind::Keyword(k),
            None => TokenKind::Ident(text),
        }
    }

    fn operator(&mut self) -> Result<TokenKind, CompileError> {
        let (line, column) = (self.line, self.column);
        let c = self.bump().expect("呼び出し側が存在を確認済み");

        Ok(match c {
            '+' if self.eat('+') => TokenKind::Increment,
            '+' if self.eat('=') => TokenKind::PlusAssign,
            '+' => TokenKind::Plus,
            '-' if self.eat('-') => TokenKind::Decrement,
            '-' if self.eat('=') => TokenKind::MinusAssign,
            '-' => TokenKind::Minus,
            '*' if self.eat('=') => TokenKind::StarAssign,
            '*' => TokenKind::Star,
            '/' if self.eat('=') => TokenKind::SlashAssign,
            '/' => TokenKind::Slash,
            '%' if self.eat('=') => TokenKind::PercentAssign,
            '%' => TokenKind::Percent,
            '=' if self.eat('=') => TokenKind::Eq,
            '=' => TokenKind::Assign,
            '!' if self.eat('=') => TokenKind::Ne,
            '!' => TokenKind::Bang,
            // 長いものから見る。`<<=` を `<` `<=` に割ってしまわないため。
            '<' if self.peek() == Some('<') && self.peek_at(1) == Some('=') => {
                self.bump();
                self.bump();
                TokenKind::ShlAssign
            }
            '<' if self.eat('<') => TokenKind::Shl,
            '<' if self.eat('=') => TokenKind::Le,
            '<' => TokenKind::Lt,
            '>' if self.peek() == Some('>') && self.peek_at(1) == Some('>') => {
                self.bump();
                self.bump();
                TokenKind::UShr
            }
            '>' if self.peek() == Some('>') && self.peek_at(1) == Some('=') => {
                self.bump();
                self.bump();
                TokenKind::ShrAssign
            }
            '>' if self.eat('>') => TokenKind::Shr,
            '>' if self.eat('=') => TokenKind::Ge,
            '>' => TokenKind::Gt,
            '&' if self.eat('&') => TokenKind::AndAnd,
            '&' if self.eat('=') => TokenKind::AmpAssign,
            '&' => TokenKind::Amp,
            '|' if self.eat('|') => TokenKind::OrOr,
            '|' if self.eat('=') => TokenKind::PipeAssign,
            '|' => TokenKind::Pipe,
            '^' if self.eat('=') => TokenKind::CaretAssign,
            '^' => TokenKind::Caret,
            '~' => TokenKind::Tilde,
            '?' => TokenKind::Question,
            ':' => TokenKind::Colon,
            ',' => TokenKind::Comma,
            ';' => TokenKind::Semicolon,
            '(' => TokenKind::LParen,
            ')' => TokenKind::RParen,
            '{' => TokenKind::LBrace,
            '}' => TokenKind::RBrace,
            '[' => TokenKind::LBracket,
            ']' => TokenKind::RBracket,
            '.' => TokenKind::Dot,
            other => {
                return Err(CompileError::new(
                    line,
                    column,
                    format!("使えない文字です: {other:?}"),
                ));
            }
        })
    }
}

/// `\n` のようなエスケープを 1 文字へ直す。文字リテラルと文字列で共有する。
fn escape(next: Option<char>, line: u32, column: u32) -> Result<char, CompileError> {
    Ok(match next {
        Some('n') => '\n',
        Some('t') => '\t',
        Some('r') => '\r',
        Some('0') => '\0',
        Some(other @ ('\\' | '\'' | '"')) => other,
        Some(other) => {
            return Err(CompileError::new(
                line,
                column,
                format!("使えないエスケープです: \\{other}"),
            ));
        }
        None => return Err(CompileError::new(line, column, "リテラルが閉じていません")),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(source: &str) -> Vec<TokenKind> {
        tokenize(source).expect("字句解析に成功する").into_iter().map(|t| t.kind).collect()
    }

    #[test]
    fn integers_and_floats_are_distinguished() {
        assert_eq!(
            kinds("1 1.5 .5 2. 1e3 2.0f"),
            vec![
                TokenKind::Int(1),
                TokenKind::Float(1.5),
                TokenKind::Float(0.5),
                TokenKind::Float(2.0),
                TokenKind::Float(1000.0),
                TokenKind::Float(2.0),
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn keywords_are_separated_from_identifiers() {
        assert_eq!(
            kinds("int x float"),
            vec![
                TokenKind::Keyword(Keyword::Int),
                TokenKind::Ident("x".into()),
                TokenKind::Keyword(Keyword::Float),
                TokenKind::Eof,
            ]
        );
        // 接頭辞が一致するだけの識別子はキーワードにしない。
        assert_eq!(kinds("integer"), vec![TokenKind::Ident("integer".into()), TokenKind::Eof]);
    }

    #[test]
    fn multi_character_operators_win_over_single() {
        assert_eq!(
            kinds("++ += == != <= >= && || -- -= *= /="),
            vec![
                TokenKind::Increment,
                TokenKind::PlusAssign,
                TokenKind::Eq,
                TokenKind::Ne,
                TokenKind::Le,
                TokenKind::Ge,
                TokenKind::AndAnd,
                TokenKind::OrOr,
                TokenKind::Decrement,
                TokenKind::MinusAssign,
                TokenKind::StarAssign,
                TokenKind::SlashAssign,
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn comments_are_skipped() {
        assert_eq!(
            kinds("1 // ここは無視\n/* 複数行\nコメント */ 2"),
            vec![TokenKind::Int(1), TokenKind::Int(2), TokenKind::Eof]
        );
    }

    #[test]
    fn unterminated_block_comment_is_an_error() {
        let e = tokenize("/* 開いたまま").unwrap_err();
        assert!(e.message.contains("閉じられていません"), "{e}");
    }

    #[test]
    fn char_literals_become_their_code_point() {
        assert_eq!(
            kinds("'a' '0' '\\n'"),
            vec![TokenKind::Int(97), TokenKind::Int(48), TokenKind::Int(10), TokenKind::Eof]
        );
    }

    #[test]
    fn unterminated_char_literal_is_an_error() {
        assert!(tokenize("'a").unwrap_err().message.contains("閉じていません"));
        assert!(tokenize("''").unwrap_err().message.contains("空の文字"));
    }

    #[test]
    fn positions_are_tracked_across_lines() {
        let tokens = tokenize("1\n  22").expect("ok");
        assert_eq!((tokens[0].line, tokens[0].column), (1, 1));
        assert_eq!((tokens[1].line, tokens[1].column), (2, 3));
    }

    #[test]
    fn unknown_character_reports_its_position() {
        let e = tokenize("int x = 1;\nfloat y = @;").unwrap_err();
        assert_eq!(e.line, 2);
        assert!(e.message.contains("使えない文字"), "{e}");
    }
}
