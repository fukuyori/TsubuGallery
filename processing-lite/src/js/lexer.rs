//! p5.js フロントエンドの字句解析。
//!
//! Java Mode 用の [`crate::lexer`] とは別に持つ。数値がすべて浮動小数であること、
//! `=>` があること、改行が文の区切りになりうること (ASI) が違う。

use crate::lexer::CompileError;

/// テンプレートリテラルの断片。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TemplatePart {
    Text(String),
    /// `${...}` の中身。まだ解析していない元のコード。
    Expr(String),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Tok {
    Number(f32),
    /// `"..."` / `'...'` / `` `...` `` の文字列。
    Str(String),
    /// `` `a${b}c` ``。文字と式が交互に並ぶ。式は元のまま持ち、構文解析で読む。
    Template(Vec<TemplatePart>),
    Ident(String),
    Keyword(Kw),

    Arrow,
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    /// べき乗 `**`。`d ** 2` のように、つぶやきでは `pow()` より短いのでよく出る。
    StarStar,
    Assign,
    PlusAssign,
    MinusAssign,
    StarAssign,
    StarStarAssign,
    SlashAssign,
    PercentAssign,
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
    /// ビット演算。つぶやきでは短く書くためによく出る。
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
    Question,
    Colon,
    Comma,
    Semicolon,
    Dot,
    /// `...`。配列を展開する。
    Spread,
    LParen,
    RParen,
    LBrace,
    RBrace,
    LBracket,
    RBracket,

    Eof,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kw {
    Let,
    Const,
    Var,
    Function,
    If,
    Else,
    For,
    While,
    Return,
    Break,
    Continue,
    True,
    False,
    Undefined,
    Null,
}

impl Kw {
    fn parse(word: &str) -> Option<Self> {
        Some(match word {
            "let" => Kw::Let,
            "const" => Kw::Const,
            "var" => Kw::Var,
            "function" => Kw::Function,
            "if" => Kw::If,
            "else" => Kw::Else,
            "for" => Kw::For,
            "break" => Kw::Break,
            "continue" => Kw::Continue,
            "while" => Kw::While,
            "return" => Kw::Return,
            "true" => Kw::True,
            "false" => Kw::False,
            "undefined" => Kw::Undefined,
            "null" => Kw::Null,
            _ => return None,
        })
    }
}

#[derive(Clone, Debug)]
pub struct Token {
    pub tok: Tok,
    pub line: u32,
    pub column: u32,
    /// 直前に改行があったか。セミコロン省略 (ASI) の判断に使う。
    pub newline_before: bool,
}

pub fn tokenize(source: &str) -> Result<Vec<Token>, CompileError> {
    let chars: Vec<char> = source.chars().collect();
    let mut tokens = Vec::new();
    let mut i = 0;
    let mut line = 1u32;
    let mut column = 1u32;
    let mut newline_before = false;

    macro_rules! advance {
        ($n:expr) => {{
            for _ in 0..$n {
                if chars.get(i) == Some(&'\n') {
                    line += 1;
                    column = 1;
                } else {
                    column += 1;
                }
                i += 1;
            }
        }};
    }

    while i < chars.len() {
        let c = chars[i];

        // 空白とコメント。
        if c.is_whitespace() {
            if c == '\n' {
                newline_before = true;
            }
            advance!(1);
            continue;
        }
        if c == '/' && chars.get(i + 1) == Some(&'/') {
            while i < chars.len() && chars[i] != '\n' {
                advance!(1);
            }
            continue;
        }
        if c == '/' && chars.get(i + 1) == Some(&'*') {
            let (start_line, start_column) = (line, column);
            advance!(2);
            loop {
                if i >= chars.len() {
                    return Err(CompileError::new(
                        start_line,
                        start_column,
                        "コメントが閉じられていません",
                    ));
                }
                if chars[i] == '*' && chars.get(i + 1) == Some(&'/') {
                    advance!(2);
                    break;
                }
                advance!(1);
            }
            continue;
        }

        let (start_line, start_column) = (line, column);
        let push = |tok: Tok, tokens: &mut Vec<Token>, newline_before: &mut bool| {
            tokens.push(Token { tok, line: start_line, column: start_column, newline_before: *newline_before });
            *newline_before = false;
        };

        // 文字列。`"` `'` `` ` `` のどれでも囲める。
        // `` `a${b}c` `` は式を挟めるので別に読む。
        if c == '`' {
            advance!(1);
            let mut parts: Vec<TemplatePart> = Vec::new();
            let mut text = String::new();
            loop {
                match chars.get(i).copied() {
                    Some('`') => {
                        advance!(1);
                        break;
                    }
                    Some('\\') => {
                        advance!(1);
                        let next = chars.get(i).copied();
                        advance!(1);
                        text.push(match next {
                            Some('n') => '\n',
                            Some('t') => '\t',
                            Some('r') => '\r',
                            Some(other) => other,
                            None => {
                                return Err(CompileError::new(
                                    line,
                                    column,
                                    "文字列が閉じていません".to_string(),
                                ));
                            }
                        });
                    }
                    // `${` から対応する `}` までが式。入れ子の `{}` も数える。
                    Some('$') if chars.get(i + 1) == Some(&'{') => {
                        advance!(2);
                        let mut depth = 1;
                        let mut inner = String::new();
                        loop {
                            match chars.get(i).copied() {
                                Some('{') => {
                                    depth += 1;
                                    inner.push('{');
                                    advance!(1);
                                }
                                Some('}') => {
                                    depth -= 1;
                                    advance!(1);
                                    if depth == 0 {
                                        break;
                                    }
                                    inner.push('}');
                                }
                                Some(ch) => {
                                    inner.push(ch);
                                    advance!(1);
                                }
                                None => {
                                    return Err(CompileError::new(
                                        line,
                                        column,
                                        "${ が閉じていません".to_string(),
                                    ));
                                }
                            }
                        }
                        if !text.is_empty() {
                            parts.push(TemplatePart::Text(std::mem::take(&mut text)));
                        }
                        parts.push(TemplatePart::Expr(inner));
                    }
                    Some(ch) => {
                        advance!(1);
                        text.push(ch);
                    }
                    None => {
                        return Err(CompileError::new(
                            line,
                            column,
                            "文字列が閉じていません".to_string(),
                        ));
                    }
                }
            }
            if !text.is_empty() || parts.is_empty() {
                parts.push(TemplatePart::Text(text));
            }
            push(Tok::Template(parts), &mut tokens, &mut newline_before);
            continue;
        }

        if matches!(c, '"' | '\'') {
            let quote = c;
            advance!(1);
            let mut out = String::new();
            loop {
                match chars.get(i).copied() {
                    Some(ch) if ch == quote => {
                        advance!(1);
                        break;
                    }
                    Some('\\') => {
                        advance!(1);
                        let next = chars.get(i).copied();
                        advance!(1);
                        out.push(match next {
                            Some('n') => '\n',
                            Some('t') => '\t',
                            Some('r') => '\r',
                            Some('0') => '\0',
                            Some(other) => other,
                            None => {
                                return Err(CompileError::new(
                                    line,
                                    column,
                                    "文字列が閉じていません".to_string(),
                                ));
                            }
                        });
                    }
                    Some(ch) => {
                        advance!(1);
                        out.push(ch);
                    }
                    None => {
                        return Err(CompileError::new(
                            line,
                            column,
                            "文字列が閉じていません".to_string(),
                        ));
                    }
                }
            }
            push(Tok::Str(out), &mut tokens, &mut newline_before);
            continue;
        }

        // `0xFF6B35` の 16 進。詰めた色を書くのに使う。
        if c == '0' && matches!(chars.get(i + 1), Some('x' | 'X')) {
            advance!(2);
            let start = i;
            while chars.get(i).is_some_and(|d| d.is_ascii_hexdigit()) {
                advance!(1);
            }
            let text: String = chars[start..i].iter().collect();
            let Ok(value) = u32::from_str_radix(&text, 16) else {
                return Err(CompileError::new(line, column, "16 進数として読めません".to_string()));
            };
            push(Tok::Number(value as f32), &mut tokens, &mut newline_before);
            continue;
        }

        // 数値。JavaScript の数値は 1 種類なので、すべて浮動小数にする。
        if c.is_ascii_digit() || (c == '.' && chars.get(i + 1).is_some_and(|d| d.is_ascii_digit())) {
            let start = i;
            while chars.get(i).is_some_and(|d| d.is_ascii_digit()) {
                advance!(1);
            }
            if chars.get(i) == Some(&'.') {
                advance!(1);
                while chars.get(i).is_some_and(|d| d.is_ascii_digit()) {
                    advance!(1);
                }
            }
            if matches!(chars.get(i), Some('e' | 'E')) {
                advance!(1);
                if matches!(chars.get(i), Some('+' | '-')) {
                    advance!(1);
                }
                while chars.get(i).is_some_and(|d| d.is_ascii_digit()) {
                    advance!(1);
                }
            }
            let text: String = chars[start..i].iter().collect();
            let value = text.parse::<f32>().map_err(|_| {
                CompileError::new(start_line, start_column, format!("数値として読めません: {text}"))
            })?;
            push(Tok::Number(value), &mut tokens, &mut newline_before);
            continue;
        }

        // 名前。JavaScript では `$` と `_` も使える。
        if c.is_alphabetic() || c == '_' || c == '$' {
            let start = i;
            while chars.get(i).is_some_and(|d| d.is_alphanumeric() || *d == '_' || *d == '$') {
                advance!(1);
            }
            let word: String = chars[start..i].iter().collect();
            let tok = match Kw::parse(&word) {
                Some(kw) => Tok::Keyword(kw),
                None => Tok::Ident(word),
            };
            push(tok, &mut tokens, &mut newline_before);
            continue;
        }

        // 記号。長いものから照合する。
        let two: String = chars[i..].iter().take(2).collect();
        let three: String = chars[i..].iter().take(3).collect();

        // `===` と `!==` は `==` `!=` と同じに扱う。
        if three == "===" || three == "!==" {
            let tok = if three == "===" { Tok::Eq } else { Tok::Ne };
            advance!(3);
            push(tok, &mut tokens, &mut newline_before);
            continue;
        }

        let three_char = match three.as_str() {
            "..." => Some(Tok::Spread),
            "**=" => Some(Tok::StarStarAssign),
            ">>>" => Some(Tok::UShr),
            "<<=" => Some(Tok::ShlAssign),
            ">>=" => Some(Tok::ShrAssign),
            _ => None,
        };
        if let Some(tok) = three_char {
            advance!(3);
            push(tok, &mut tokens, &mut newline_before);
            continue;
        }

        let two_char = match two.as_str() {
            "=>" => Some(Tok::Arrow),
            "==" => Some(Tok::Eq),
            "!=" => Some(Tok::Ne),
            "<=" => Some(Tok::Le),
            ">=" => Some(Tok::Ge),
            "&&" => Some(Tok::AndAnd),
            "||" => Some(Tok::OrOr),
            "**" => Some(Tok::StarStar),
            "++" => Some(Tok::Increment),
            "--" => Some(Tok::Decrement),
            "+=" => Some(Tok::PlusAssign),
            "-=" => Some(Tok::MinusAssign),
            "*=" => Some(Tok::StarAssign),
            "/=" => Some(Tok::SlashAssign),
            "%=" => Some(Tok::PercentAssign),
            "<<" => Some(Tok::Shl),
            ">>" => Some(Tok::Shr),
            "&=" => Some(Tok::AmpAssign),
            "|=" => Some(Tok::PipeAssign),
            "^=" => Some(Tok::CaretAssign),
            _ => None,
        };
        if let Some(tok) = two_char {
            advance!(2);
            push(tok, &mut tokens, &mut newline_before);
            continue;
        }

        let single = match c {
            // つぶやきでは 1 文字節約のため `⇒` を使うことがある。
            '⇒' | '→' => Tok::Arrow,
            '+' => Tok::Plus,
            '-' => Tok::Minus,
            '*' => Tok::Star,
            '/' => Tok::Slash,
            '%' => Tok::Percent,
            '&' => Tok::Amp,
            '|' => Tok::Pipe,
            '^' => Tok::Caret,
            '~' => Tok::Tilde,
            '=' => Tok::Assign,
            '!' => Tok::Bang,
            '<' => Tok::Lt,
            '>' => Tok::Gt,
            '?' => Tok::Question,
            ':' => Tok::Colon,
            ',' => Tok::Comma,
            ';' => Tok::Semicolon,
            '.' => Tok::Dot,
            '(' => Tok::LParen,
            ')' => Tok::RParen,
            '{' => Tok::LBrace,
            '}' => Tok::RBrace,
            '[' => Tok::LBracket,
            ']' => Tok::RBracket,
            other => {
                return Err(CompileError::new(
                    start_line,
                    start_column,
                    format!("使えない文字です: {other:?}"),
                ));
            }
        };
        advance!(1);
        push(single, &mut tokens, &mut newline_before);
    }

    tokens.push(Token { tok: Tok::Eof, line, column, newline_before });
    Ok(tokens)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn toks(source: &str) -> Vec<Tok> {
        tokenize(source).expect("字句解析に成功する").into_iter().map(|t| t.tok).collect()
    }

    #[test]
    fn dollar_and_underscore_are_ordinary_name_characters() {
        assert_eq!(
            toks("$ _ $x"),
            vec![
                Tok::Ident("$".into()),
                Tok::Ident("_".into()),
                Tok::Ident("$x".into()),
                Tok::Eof
            ]
        );
    }

    #[test]
    fn arrows_come_in_three_spellings() {
        assert_eq!(toks("=>"), vec![Tok::Arrow, Tok::Eof]);
        assert_eq!(toks("⇒"), vec![Tok::Arrow, Tok::Eof]);
        assert_eq!(toks("→"), vec![Tok::Arrow, Tok::Eof]);
    }

    #[test]
    fn strict_equality_folds_into_loose() {
        assert_eq!(toks("a===b"), vec![Tok::Ident("a".into()), Tok::Eq, Tok::Ident("b".into()), Tok::Eof]);
        assert_eq!(toks("a!==b"), vec![Tok::Ident("a".into()), Tok::Ne, Tok::Ident("b".into()), Tok::Eof]);
    }

    #[test]
    fn every_number_is_a_float() {
        assert_eq!(toks("1 .5 1e3"), vec![
            Tok::Number(1.0),
            Tok::Number(0.5),
            Tok::Number(1000.0),
            Tok::Eof
        ]);
    }

    #[test]
    fn newlines_are_recorded_for_semicolon_insertion() {
        let tokens = tokenize("a\nb c").expect("ok");
        assert!(!tokens[0].newline_before, "先頭");
        assert!(tokens[1].newline_before, "改行のあと");
        assert!(!tokens[2].newline_before, "同じ行");
    }

    #[test]
    fn comments_are_skipped() {
        assert_eq!(toks("1 // 無視\n/* これも */ 2"), vec![Tok::Number(1.0), Tok::Number(2.0), Tok::Eof]);
    }
}
