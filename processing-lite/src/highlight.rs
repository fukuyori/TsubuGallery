//! エディタ向けの色分け (設計書 §25 の Editor)。
//!
//! 実行用の [`crate::lexer`] とは目的が違うので別に持つ。編集中のコードはたいてい
//! 途中で壊れているため、ここでは決してエラーにせず、読めない文字も含めて必ず
//! ソース全体を覆う。コメントも色を付けるので落とさない。
//!
//! キーワードと API の一覧は [`crate::lexer`] と [`crate::natives`] から引く。
//! 語彙が二重管理にならないよう、突き合わせはテストで固定してある。
//!
//! [`tokens`] は 1 トークンずつ、[`spans`] は色が同じ区間をまとめて返す。
//! 整形 ([`crate::format`]) は前者を使う。

use crate::lexer::Keyword;
use crate::natives::{self, BuiltinVar};

/// 色分けの種類。実際の色は UI 層が決める。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TokenClass {
    /// 空白や、分類しないもの。
    Plain,
    Comment,
    Keyword,
    /// `int` `float` `boolean` `void`。
    Type,
    Number,
    /// `'a'` のような文字リテラル。
    Char,
    /// Processing Lite の API 関数。
    Api,
    /// `width` や `PI` のような組み込み変数。
    Builtin,
    /// ユーザーが定義した名前。
    Ident,
    Operator,
    Punct,
    /// この言語では使えない文字。`$` や `⇒` など。
    ///
    /// 空白と同じ [`TokenClass::Plain`] にしてしまうと、方言の判定
    /// ([`crate::dialect`]) が手がかりを取りこぼす。
    Unknown,
}

/// ソース中の 1 区間。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Span {
    /// バイト位置。
    pub start: usize,
    pub end: usize,
    pub class: TokenClass,
}

/// 色分け用に、同じ種類が続く区間をまとめて返す。
///
/// 返る区間は開始位置の昇順で、連結すると元のソースに戻る。
pub fn spans(source: &str) -> Vec<Span> {
    let mut merged: Vec<Span> = Vec::new();
    for token in tokens(source) {
        // 同じ種類が続くならまとめる。区間の数を減らすと描画も軽い。
        if let Some(last) = merged.last_mut()
            && last.class == token.class
            && last.end == token.start
        {
            last.end = token.end;
            continue;
        }
        merged.push(token);
    }
    merged
}

/// ソース全体を 1 トークンずつに分ける。
///
/// 演算子は Lexer と同じ最長一致で切る (`+=` は 1 つ、`=-` は 2 つ)。空白も
/// [`TokenClass::Plain`] の区間として残るので、連結すると元のソースに戻る。
pub fn tokens(source: &str) -> Vec<Span> {
    let chars: Vec<(usize, char)> = source.char_indices().collect();
    let mut spans: Vec<Span> = Vec::new();
    let mut i = 0;

    while i < chars.len() {
        let (start, c) = chars[i];

        let (end, class) = if c == '/' && next_is(&chars, i, '/') {
            let end = scan_while(&chars, i, |c| c != '\n');
            (end, TokenClass::Comment)
        } else if c == '/' && next_is(&chars, i, '*') {
            (scan_block_comment(&chars, i), TokenClass::Comment)
        } else if c == '\'' {
            (scan_char_literal(&chars, i), TokenClass::Char)
        } else if c.is_ascii_digit()
            || (c == '.' && chars.get(i + 1).is_some_and(|(_, d)| d.is_ascii_digit()))
        {
            (scan_number(&chars, i), TokenClass::Number)
        } else if c.is_alphabetic() || c == '_' {
            let end = scan_while(&chars, i, |c| c.is_alphanumeric() || c == '_');
            let word = &source[start..end];
            (end, classify_word(word))
        } else if c.is_whitespace() {
            (scan_while(&chars, i, char::is_whitespace), TokenClass::Plain)
        } else if is_operator_char(c) {
            (scan_operator(&chars, i), TokenClass::Operator)
        } else if matches!(c, '(' | ')' | '{' | '}' | '[' | ']' | ',' | ';') {
            (start + c.len_utf8(), TokenClass::Punct)
        } else {
            // この言語では使えない文字。必ず区間として残す。
            (start + c.len_utf8(), TokenClass::Unknown)
        };

        spans.push(Span { start, end, class });
        i = index_of(&chars, end).unwrap_or(chars.len());
    }

    spans
}

/// 演算子。最長一致で切る。
///
/// ここで `=-` を 1 つにまとめてしまうと、整形が `= -` を作れない。逆に `=>` を
/// `=` と `>` に割ってしまうと、整形が `= >` を作ってコードを壊す。どちらの方言の
/// 演算子もここに揃えておく必要がある。
const OPERATORS: &[&str] = &[
    // 3 文字
    "===", "!==", "**=", "%=",
    // 2 文字
    "=>", "++", "--", "+=", "-=", "*=", "/=", "==", "!=", "<=", ">=", "&&", "||", "**",
    // 1 文字
    "+", "-", "*", "/", "%", "=", "!", "<", ">", "?", ":", ".",
];

fn scan_operator(chars: &[(usize, char)], i: usize) -> usize {
    let text: String = chars[i..].iter().take(3).map(|(_, c)| *c).collect();
    for op in OPERATORS {
        if op.len() > 1 && text.starts_with(op) {
            return end_byte(chars, i + op.chars().count());
        }
    }
    end_byte(chars, i + 1)
}

/// 語を分類する。
fn classify_word(word: &str) -> TokenClass {
    match Keyword::parse(word) {
        Some(Keyword::Void | Keyword::Int | Keyword::Float | Keyword::Boolean) => TokenClass::Type,
        Some(_) => TokenClass::Keyword,
        None if natives::is_native(word) => TokenClass::Api,
        None if BuiltinVar::resolve(word).is_some() => TokenClass::Builtin,
        None => TokenClass::Ident,
    }
}

fn next_is(chars: &[(usize, char)], i: usize, expected: char) -> bool {
    chars.get(i + 1).is_some_and(|(_, c)| *c == expected)
}

fn index_of(chars: &[(usize, char)], byte: usize) -> Option<usize> {
    chars.iter().position(|(b, _)| *b >= byte)
}

/// `i` から条件を満たすあいだ進み、終端のバイト位置を返す。
fn scan_while(chars: &[(usize, char)], i: usize, mut keep: impl FnMut(char) -> bool) -> usize {
    let mut j = i;
    while let Some((_, c)) = chars.get(j) {
        if !keep(*c) {
            break;
        }
        j += 1;
    }
    end_byte(chars, j)
}

fn end_byte(chars: &[(usize, char)], j: usize) -> usize {
    match chars.get(j) {
        Some((b, _)) => *b,
        None => chars.last().map_or(0, |(b, c)| b + c.len_utf8()),
    }
}

/// `/* ... */`。閉じていなければ末尾まで。
fn scan_block_comment(chars: &[(usize, char)], i: usize) -> usize {
    let mut j = i + 2;
    while j < chars.len() {
        if chars[j].1 == '*' && next_is(chars, j, '/') {
            return end_byte(chars, j + 2);
        }
        j += 1;
    }
    end_byte(chars, chars.len())
}

/// `'a'`。閉じていなければ 1 文字だけ食べて諦める。
fn scan_char_literal(chars: &[(usize, char)], i: usize) -> usize {
    let mut j = i + 1;
    while j < chars.len() {
        match chars[j].1 {
            '\\' => j += 2,
            '\'' => return end_byte(chars, j + 1),
            '\n' => break,
            _ => j += 1,
        }
    }
    end_byte(chars, i + 1)
}

fn scan_number(chars: &[(usize, char)], i: usize) -> usize {
    let mut j = i;
    let mut seen_dot = false;
    while let Some((_, c)) = chars.get(j) {
        match c {
            c if c.is_ascii_digit() => j += 1,
            '.' if !seen_dot => {
                seen_dot = true;
                j += 1;
            }
            'e' | 'E' if chars.get(j + 1).is_some_and(|(_, d)| d.is_ascii_digit() || *d == '+' || *d == '-') => {
                j += 2;
            }
            'f' | 'F' => {
                j += 1;
                break;
            }
            _ => break,
        }
    }
    end_byte(chars, j)
}

fn is_operator_char(c: char) -> bool {
    matches!(c, '+' | '-' | '*' | '/' | '%' | '=' | '!' | '<' | '>' | '&' | '|' | '?' | ':' | '.')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn classes(source: &str) -> Vec<(&str, TokenClass)> {
        spans(source).into_iter().map(|s| (&source[s.start..s.end], s.class)).collect()
    }

    /// どんな入力でも、区間を連結すると元のソースに戻る。
    fn assert_covers(source: &str) {
        let spans = spans(source);
        let mut at = 0;
        let mut rebuilt = String::new();
        for span in &spans {
            assert_eq!(span.start, at, "隙間か重なりがある: {source:?}");
            assert!(span.end > span.start, "空の区間がある: {source:?}");
            rebuilt.push_str(&source[span.start..span.end]);
            at = span.end;
        }
        assert_eq!(at, source.len(), "末尾が欠けている: {source:?}");
        assert_eq!(rebuilt, source);
    }

    fn token_texts(source: &str) -> Vec<&str> {
        tokens(source).into_iter().map(|s| &source[s.start..s.end]).collect()
    }

    #[test]
    fn javascript_operators_stay_whole() {
        // 割ってしまうと、整形が `= >` を作ってコードが壊れる。
        assert_eq!(token_texts("a=>a"), vec!["a", "=>", "a"]);
        assert_eq!(token_texts("a===b"), vec!["a", "===", "b"]);
        assert_eq!(token_texts("a!==b"), vec!["a", "!==", "b"]);
        assert_eq!(token_texts("a**b"), vec!["a", "**", "b"]);
        assert_eq!(token_texts("a%=b"), vec!["a", "%=", "b"]);
    }

    #[test]
    fn operators_are_split_by_longest_match() {
        assert_eq!(token_texts("a=-1"), vec!["a", "=", "-", "1"]);
        assert_eq!(token_texts("a+=1"), vec!["a", "+=", "1"]);
        assert_eq!(token_texts("i++"), vec!["i", "++"]);
        assert_eq!(token_texts("a<=b"), vec!["a", "<=", "b"]);
        assert_eq!(token_texts("a&&!b"), vec!["a", "&&", "!", "b"]);
    }

    #[test]
    fn spans_merge_what_tokens_keep_apart() {
        // 描画では `=-` をまとめてよいが、トークンとしては別。
        assert_eq!(tokens("a=-1").len(), 4);
        assert_eq!(spans("a=-1").len(), 3, "= と - は同じ色なのでまとまる");
    }

    #[test]
    fn types_keywords_and_names_are_separated() {
        assert_eq!(
            classes("int x"),
            vec![("int", TokenClass::Type), (" ", TokenClass::Plain), ("x", TokenClass::Ident)]
        );
        assert_eq!(classes("if")[0].1, TokenClass::Keyword);
        assert_eq!(classes("true")[0].1, TokenClass::Keyword);
    }

    #[test]
    fn api_names_and_builtin_variables_get_their_own_class() {
        assert_eq!(classes("circle")[0].1, TokenClass::Api);
        assert_eq!(classes("width")[0].1, TokenClass::Builtin);
        assert_eq!(classes("PI")[0].1, TokenClass::Builtin);
        assert_eq!(classes("myHelper")[0].1, TokenClass::Ident);
    }

    #[test]
    fn comments_are_one_span() {
        assert_eq!(
            classes("x // あとは無視\ny"),
            vec![
                ("x", TokenClass::Ident),
                (" ", TokenClass::Plain),
                ("// あとは無視", TokenClass::Comment),
                ("\n", TokenClass::Plain),
                ("y", TokenClass::Ident),
            ]
        );
        assert_eq!(classes("/* a\nb */")[0].1, TokenClass::Comment);
    }

    #[test]
    fn numbers_include_their_suffix_and_exponent() {
        assert_eq!(classes("1"), vec![("1", TokenClass::Number)]);
        assert_eq!(classes("1.5"), vec![("1.5", TokenClass::Number)]);
        assert_eq!(classes(".5"), vec![(".5", TokenClass::Number)]);
        assert_eq!(classes("1e3"), vec![("1e3", TokenClass::Number)]);
        assert_eq!(classes("2.0f"), vec![("2.0f", TokenClass::Number)]);
    }

    #[test]
    fn char_literals_are_highlighted() {
        assert_eq!(classes("'a'"), vec![("'a'", TokenClass::Char)]);
        assert_eq!(classes("'\\n'"), vec![("'\\n'", TokenClass::Char)]);
    }

    #[test]
    fn characters_the_language_cannot_use_are_marked() {
        assert_eq!(classes("$"), vec![("$", TokenClass::Unknown)]);
        assert_eq!(classes("⇒"), vec![("⇒", TokenClass::Unknown)]);
    }

    #[test]
    fn broken_input_never_panics_and_still_covers_everything() {
        for source in [
            "",
            "@",
            "int x = ",
            "/* 閉じていない",
            "'",
            "'a",
            "void draw() { background(0)",
            "日本語のコメントだけ",
            "1..2",
            "x @@ y",
        ] {
            assert_covers(source);
        }
    }

    #[test]
    fn a_whole_sketch_is_covered() {
        assert_covers(include_str!("../sketches/noise-field.pde"));
        assert_covers(include_str!("../sketches/moire.pde"));
    }

    #[test]
    fn multibyte_text_is_split_on_character_boundaries() {
        let source = "// つぶやき\nint x = 1;";
        assert_covers(source);
        // 部分文字列として取り出せる = 文字境界で切れている。
        for span in spans(source) {
            let _ = &source[span.start..span.end];
        }
    }

    /// 実行用の Lexer が知っているキーワードは、必ず色が付く。
    ///
    /// 片方にだけ語を足したときに気付けるようにしておく。
    #[test]
    fn every_lexer_keyword_is_classified() {
        for word in [
            "void", "int", "float", "boolean", "if", "else", "for", "while", "return", "true",
            "false",
        ] {
            assert!(Keyword::parse(word).is_some(), "{word} は Lexer のキーワードではない");
            let class = classes(word)[0].1;
            assert!(
                matches!(class, TokenClass::Keyword | TokenClass::Type),
                "{word} が {class:?} になっている"
            );
        }
    }

    /// API 名を足したら、そのまま色が付く。
    #[test]
    fn native_names_are_classified_from_the_shared_table() {
        for name in ["circle", "translate", "noise", "strokeWeight"] {
            assert!(natives::is_native(name));
            assert_eq!(classes(name)[0].1, TokenClass::Api, "{name}");
        }
    }
}
