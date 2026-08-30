//! コードの整形と短縮 (設計書 §25 の Editor)。
//!
//! `#つぶやきProcessing` は文字数を詰めるために 1 行へ畳んであることが多い。
//! 読むときは [`expand`] で改行とインデントを入れ、投稿するときは [`compress`]
//! で詰め直す。
//!
//! どちらも **意味を変えない** ことが唯一の要件なので、[`crate::highlight::tokens`]
//! が返すトークン列を並べ替えずに、あいだの空白だけを作り直す。変換の前後で
//! Bytecode が一致することはテストで固定してある。
//!
//! つぶやき GLSL も同じ扱いで通す。文の区切りも括弧の対応も C 系で共通なので、
//! 空白を作り直すぶんには方言を見分ける必要がない。例外は `#` で始まる行で、
//! `#version` のようなプリプロセッサ指令は 1 行で完結していなければ通らず、
//! `#つぶやきGLSL` のようなタグは書いたとおりに残したい。どちらも行ごと
//! そのまま通す。

use crate::highlight::{TokenClass, tokens};

/// インデント 1 段分。
const INDENT: &str = "  ";

/// 1 行の目安の長さ。これを超えたら括弧の中で折り返す。
///
/// 縮めて書かれた作品は 1 文が数百文字になることがあり、開いただけでは読めない。
const MAX_WIDTH: usize = 96;

/// 折り返しの入れ子の上限。壊れた入力で回り続けないための歯止め。
const MAX_WRAP_DEPTH: u32 = 8;

/// 1 行に畳まれたコードへ改行とインデントを入れる。
///
/// 文の区切りで改行して字下げしたあと、まだ長すぎる行は括弧の中で折り返す。
pub fn expand(source: &str) -> String {
    wrap_long_lines(&lay_out(source))
}

/// 文と括弧の対応から、改行と字下げを決める。折り返しはまだしない。
fn lay_out(source: &str) -> String {
    let pieces = Pieces::new(source);
    let mut out = Writer::new();

    // `if (...)` の本体が `{` でないときは、その 1 文だけ字下げする。
    let mut single_statement_depth: Vec<usize> = Vec::new();
    // 制御構文のヘッダ `(...)` を読んでいるあいだの括弧の深さ。
    let mut header_paren: Option<usize> = None;
    let mut paren_depth = 0usize;
    // 元のコードにあった改行。セミコロンを書かない p5.js では文の区切りなので、
    // 落とすとコードが壊れる。
    let mut pending_newline = false;
    // 空行があったか。区切りとして書かれたものなので 1 行だけ残す。
    let mut pending_blank = false;

    for (index, piece) in pieces.iter().enumerate() {
        let text = piece.text;

        // 括弧の外の改行だけを引き継ぐ。式の途中の折り返しは畳んでよい。
        if pending_newline && !matches!(piece.class, TokenClass::Plain) {
            pending_newline = false;
            let pending_blank = std::mem::take(&mut pending_blank);
            if paren_depth == 0 {
                // 何か書いたあとの改行だけが文の終わり。既に行頭にいるなら、
                // 直前の `)` が入れた改行なので、まだ本体は始まっていない。
                let ended_a_line = !out.at_line_start;
                out.newline();
                if pending_blank {
                    out.blank_line();
                }
                if ended_a_line {
                    // セミコロンを書かない作品では、改行が文の終わり。
                    // `if (…)\n  x = 1\n}` の字下げをここで閉じる。
                    close_single_statements(&mut out, &mut single_statement_depth);
                }
            }
        }

        // プリプロセッサ行。1 行で完結していないと通らないので、字下げも
        // 折り返しも入れずにそのまま置く。
        if piece.directive {
            out.newline();
            let indent = std::mem::take(&mut out.indent);
            out.push(text);
            out.indent = indent;
            out.newline();
            continue;
        }

        match piece.class {
            TokenClass::Comment => {
                // 行の途中にあったコメントは、その行のまま残す。
                if piece.same_line_as_previous_code {
                    out.space();
                } else {
                    out.newline();
                }
                out.push(text);
                if text.starts_with("//") {
                    out.newline();
                }
            }

            TokenClass::Punct => match text {
                "{" => {
                    out.space_unless_line_start();
                    out.push("{");
                    out.indent += 1;
                    out.newline();
                }
                "}" => {
                    out.indent = out.indent.saturating_sub(1);
                    out.newline();
                    out.push("}");
                    out.newline();
                    close_single_statements(&mut out, &mut single_statement_depth);
                    if out.indent == 0 {
                        // トップレベルの区切りは 1 行空ける。
                        out.blank_line();
                    }
                }
                ";" => {
                    out.push(";");
                    if paren_depth == 0 {
                        // `for (a; b; c)` の `;` では改行しない。
                        out.newline();
                        close_single_statements(&mut out, &mut single_statement_depth);
                    } else {
                        out.space();
                    }
                }
                "," => {
                    out.push(",");
                    out.space();
                }
                "(" => {
                    // `if (`, `for (` は空ける。`circle(` は空けない。
                    if pieces.previous_is_control_keyword(index) {
                        out.space();
                    }
                    out.push_tight("(");
                    paren_depth += 1;
                }
                ")" => {
                    // `for (…; i--; )` のような末尾の空白を残さない。
                    out.trim_trailing_space();
                    out.push(")");
                    paren_depth = paren_depth.saturating_sub(1);

                    if header_paren == Some(paren_depth) {
                        header_paren = None;
                        if pieces.next_code(index) == Some("{") {
                            // `{` 側が空白を入れる。
                        } else {
                            // 本体が 1 文だけ。次の行へ字下げして書く。
                            out.indent += 1;
                            single_statement_depth.push(out.indent);
                            out.newline();
                        }
                    }
                }
                other => out.push(other),
            },

            TokenClass::Keyword | TokenClass::Type => {
                match text {
                    "else" => {
                        if out.ends_with_close_brace() {
                            // `}` が入れた改行を戻して `} else` にする。
                            out.join_with_previous_line();
                            out.space();
                        } else {
                            // 括弧なしの本体のあとなので、行頭から始める。
                            out.newline();
                        }
                        out.push("else");

                        match pieces.next_code(index) {
                            // `else if` と `else {` は同じ行に続ける。
                            Some("{" | "if") => out.space(),
                            _ => {
                                // 本体が 1 文だけ。次の行へ字下げして書く。
                                out.indent += 1;
                                single_statement_depth.push(out.indent);
                                out.newline();
                            }
                        }
                    }
                    "if" | "for" | "while" => {
                        out.space_unless_line_start();
                        out.push(text);
                        header_paren = Some(paren_depth);
                    }
                    _ => {
                        out.space_unless_line_start();
                        out.push(text);
                    }
                }
                if matches!(text, "return") {
                    out.space();
                }
            }

            TokenClass::Operator => {
                if text == "." {
                    // プロパティの区切り。両側とも空けない。
                    out.push_tight(".");
                } else if pieces.is_unary(index) {
                    // 単項は右にくっつける。`-1`, `!ok`
                    out.space_unless_line_start();
                    out.push_tight(text);
                } else if matches!(text, "++" | "--") {
                    // 前置 (`++i`) は右に、後置 (`i++`) は左にくっつく。
                    // 後置のあとは `i++ < 9` のように空ける。
                    if pieces.follows_value(index) {
                        out.push(text);
                    } else {
                        out.space_unless_line_start();
                        out.push_tight(text);
                    }
                } else {
                    out.space_unless_line_start();
                    out.push(text);
                    out.space();
                }
            }

            TokenClass::Plain => {
                // 空白は捨てるが、改行があったことは覚えておく。
                let newlines = text.matches('\n').count();
                if newlines > 0 {
                    pending_newline = true;
                }
                if newlines > 1 {
                    pending_blank = true;
                }
            }

            _ => {
                out.space_unless_line_start();
                out.push(text);
            }
        }
    }

    out.finish()
}

/// 長すぎる行を、括弧の中で折り返す。
///
/// 既に整形された行に対して働くので、桁数をそのまま数えられる。トークンの並びは
/// 変えず、空白を改行に替えるだけなので意味は変わらない。
fn wrap_long_lines(text: &str) -> String {
    let mut out = String::new();
    for line in text.lines() {
        wrap_line(line, 0, &mut out);
    }
    out
}

fn wrap_line(line: &str, depth: u32, out: &mut String) {
    // `#` の行は割れない。`#define f(x) (x*2)` の括弧に手を出さない。
    if line.starts_with('#') {
        out.push_str(line);
        out.push('\n');
        return;
    }
    if line.chars().count() <= MAX_WIDTH || depth >= MAX_WRAP_DEPTH {
        out.push_str(line);
        out.push('\n');
        return;
    }

    let Some(group) = widest_group(line) else {
        out.push_str(line);
        out.push('\n');
        return;
    };

    let chars: Vec<char> = line.chars().collect();
    let indent: String = chars.iter().take_while(|c| **c == ' ').collect();
    let inner_indent = format!("{indent}{INDENT}");

    // `foo(` までを 1 行目にする。
    let head: String = chars[..=group.open].iter().collect();
    wrap_line(&head, depth + 1, out);

    // 中身をカンマで割って、1 つずつ行にする。
    for item in split_top_level(&chars[group.open + 1..group.close]) {
        let text = item.trim();
        if text.is_empty() {
            continue;
        }
        wrap_line(&format!("{inner_indent}{text}"), depth + 1, out);
    }

    // 閉じ括弧から後ろは、元の字下げに戻して続ける。
    let tail: String = chars[group.close..].iter().collect();
    wrap_line(&format!("{indent}{tail}"), depth + 1, out);
}

/// 折り返しに使う括弧の位置。
struct Group {
    open: usize,
    close: usize,
}

/// 行の中で、いちばん外側にあって中身の長い括弧を選ぶ。
///
/// コメントの中の括弧は数えない。
fn widest_group(line: &str) -> Option<Group> {
    let chars: Vec<char> = line.chars().collect();
    let code_end = find_comment(&chars).unwrap_or(chars.len());

    let mut stack: Vec<usize> = Vec::new();
    let mut best: Option<Group> = None;

    for (index, c) in chars[..code_end].iter().enumerate() {
        match c {
            '(' | '[' => stack.push(index),
            ')' | ']' => {
                if let Some(open) = stack.pop() {
                    // いちばん外側 = 閉じたときにスタックが空になるもの。
                    if stack.is_empty() && index > open + 1 {
                        let width = index - open;
                        let better = best.as_ref().is_none_or(|g| width > g.close - g.open);
                        if better {
                            best = Some(Group { open, close: index });
                        }
                    }
                }
            }
            _ => {}
        }
    }

    best
}

/// 行コメントが始まる位置。
fn find_comment(chars: &[char]) -> Option<usize> {
    chars.windows(2).position(|w| w == ['/', '/'])
}

/// 括弧の外にあるカンマで割る。
fn split_top_level(chars: &[char]) -> Vec<String> {
    let mut items = Vec::new();
    let mut depth = 0i32;
    let mut current = String::new();

    for c in chars {
        match c {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth -= 1,
            ',' if depth == 0 => {
                current.push(',');
                items.push(std::mem::take(&mut current));
                continue;
            }
            _ => {}
        }
        current.push(*c);
    }
    if !current.trim().is_empty() {
        items.push(current);
    }
    items
}

/// 空白とコメントを削って詰める。
///
/// 名前の付け替えはしない。意味を変えずに減らせるぶんだけを減らす。
pub fn compress(source: &str) -> String {
    let mut out = String::new();
    let mut previous = String::new();
    let mut pending_newline = false;
    let mut group_depth = 0i32;
    // 改行を文の区切りとして残すか。`;` を省ける p5.js と GOLF の都合で、GLSL
    // は必ず `;` で終わるので全部詰めてよい。
    let keeps_newlines =
        crate::dialect::looks_like_golf(source) || !crate::dialect::looks_like_glsl(source);

    for piece in Pieces::new(source).iter() {
        // プリプロセッサ行は詰められない。前後の改行ごと残す。
        if piece.directive {
            if !out.is_empty() && !out.ends_with('\n') {
                out.push('\n');
            }
            out.push_str(piece.text.trim());
            out.push('\n');
            previous.clear();
            pending_newline = false;
            continue;
        }

        let text = match piece.class {
            TokenClass::Comment => continue,
            TokenClass::Plain => {
                // 空白は落とすが、改行があったことは覚えておく。
                if piece.text.contains('\n') {
                    pending_newline = true;
                }
                continue;
            }
            TokenClass::Number => shorten_number(piece.text),
            _ => piece.text.to_string(),
        };
        if text.is_empty() {
            continue;
        }

        if matches!(text.as_str(), "(" | "[") {
            group_depth += 1;
        }

        // セミコロンを書かない p5.js では改行が文の区切り。消すと繋がってしまう。
        if keeps_newlines && pending_newline && group_depth == 0 && ends_statement(&previous) {
            out.push('\n');
        } else if needs_space(&previous, &text) {
            out.push(' ');
        }
        pending_newline = false;

        // 閉じ括弧を数えるのは、改行を決めたあと。先に減らすと、括弧の中に
        // あった改行が文の区切りに見えて `)` の手前で行が割れる。
        if matches!(text.as_str(), ")" | "]") {
            group_depth -= 1;
        }

        out.push_str(&text);
        previous = text;
    }

    out
}

/// そのトークンで文が終わりうるか。改行を残すかの判断に使う。
fn ends_statement(text: &str) -> bool {
    text.ends_with(|c: char| c.is_alphanumeric() || c == '_') || matches!(text, ")" | "]")
}

/// 詰めたときに 2 つのトークンがくっついて別の意味にならないか。
fn needs_space(previous: &str, next: &str) -> bool {
    let Some(a) = previous.chars().last() else { return false };
    let Some(b) = next.chars().next() else { return false };

    let wordish = |c: char| c.is_alphanumeric() || c == '_';

    // `int x` を `intx` にしてはいけない。
    if wordish(a) && wordish(b) {
        return true;
    }

    // 点は数の一部にも、プロパティの区切りにもなる。`1` と `.5` を詰めると
    // `1.5` になって値が変わるが、`p` と `.x` は詰めてよい。
    if (a == '.' && b.is_ascii_digit()) || (b == '.' && a.is_ascii_digit()) {
        return true;
    }

    // `a+ +b` が `a++b` に、`a/ /*c*/ b` が `a//b` になるのを防ぐ。
    let joined = format!("{a}{b}");
    matches!(
        joined.as_str(),
        "++" | "--" | "+=" | "-=" | "*=" | "/=" | "==" | "!=" | "<=" | ">=" | "&&" | "||" | "//"
            | "/*"
    )
}

/// 数値リテラルを短くする。型は変えない。
///
/// `0.5` → `.5`、`1.0` → `1.`、`2.0f` → `2.`。整数はそのまま。
pub fn shorten_number(text: &str) -> String {
    // 指数表記は触らない。桁を落とすと値が変わりうる。
    if text.contains(['e', 'E']) {
        return text.trim_end_matches(['f', 'F']).to_string();
    }

    let body = text.trim_end_matches(['f', 'F']);
    let Some((integer, fraction)) = body.split_once('.') else {
        // 整数。`f` が付いていたら小数なので、点を足して型を保つ。
        return if body == text { body.to_string() } else { format!("{body}.") };
    };

    let fraction = fraction.trim_end_matches('0');
    // 小数点は必ず残す。落とすと int になってしまう。
    let integer = if fraction.is_empty() { integer } else { integer.trim_start_matches('0') };

    let shortened = format!("{integer}.{fraction}");
    // `.` だけになったら元が `0.0` などなので、`0.` に戻す。
    if shortened == "." { "0.".to_string() } else { shortened }
}

// ---------------------------------------------------------------------------

/// トークンに、整形で要る前後関係を添えたもの。
struct Piece<'a> {
    text: &'a str,
    class: TokenClass,
    /// 直前のコードと同じ行にあるか。行末コメントの判定に使う。
    same_line_as_previous_code: bool,
    /// `#` で始まる行 1 本ぶん。中は割らない。
    directive: bool,
}

impl Piece<'_> {
    /// 前後関係を見るときに数えるか。空白・コメント・プリプロセッサ行は数えない。
    fn is_code(&self) -> bool {
        !self.directive && !matches!(self.class, TokenClass::Plain | TokenClass::Comment)
    }
}

struct Pieces<'a> {
    pieces: Vec<Piece<'a>>,
}

impl<'a> Pieces<'a> {
    fn new(source: &'a str) -> Self {
        let mut pieces = Vec::new();
        let mut saw_code_on_this_line = false;
        // プリプロセッサ行を 1 つにまとめたときの、その行の終わり。
        let mut directive_end = 0usize;

        for span in tokens(source) {
            // まとめた行の中身は読み飛ばす。
            if span.start < directive_end {
                continue;
            }

            let text = &source[span.start..span.end];
            if span.class == TokenClass::Plain {
                if text.contains('\n') {
                    saw_code_on_this_line = false;
                }
                pieces.push(Piece {
                    text,
                    class: span.class,
                    same_line_as_previous_code: false,
                    directive: false,
                });
                continue;
            }

            // `#version 450` や `#つぶやきGLSL`。`#` は演算子でも括弧でもない
            // ので 1 文字の Unknown として出てくる。行の頭にあるものだけを
            // 見るのは、行の途中に書かれた `#` を巻き込まないため。
            if text == "#" && !saw_code_on_this_line {
                let end = source[span.start..]
                    .find('\n')
                    .map_or(source.len(), |at| span.start + at);
                directive_end = end;
                pieces.push(Piece {
                    text: source[span.start..end].trim_end(),
                    class: span.class,
                    same_line_as_previous_code: false,
                    directive: true,
                });
                saw_code_on_this_line = true;
                continue;
            }

            pieces.push(Piece {
                text,
                class: span.class,
                same_line_as_previous_code: saw_code_on_this_line,
                directive: false,
            });
            saw_code_on_this_line = true;
        }

        Self { pieces }
    }

    fn iter(&self) -> impl Iterator<Item = &Piece<'a>> {
        self.pieces.iter()
    }

    /// 空白・コメント・プリプロセッサ行を飛ばして 1 つ前のコード。
    fn previous_code(&self, index: usize) -> Option<&Piece<'a>> {
        self.pieces[..index].iter().rev().find(|p| p.is_code())
    }

    fn next_code(&self, index: usize) -> Option<&str> {
        self.pieces[index + 1..].iter().find(|p| p.is_code()).map(|p| p.text)
    }

    fn previous_is_control_keyword(&self, index: usize) -> bool {
        matches!(self.previous_code(index).map(|p| p.text), Some("if" | "for" | "while"))
    }

    /// その演算子が単項か。直前に値が無ければ単項。
    fn is_unary(&self, index: usize) -> bool {
        matches!(self.pieces[index].text, "-" | "+" | "!") && !self.follows_value(index)
    }

    /// 直前が値で終わっているか。単項と後置の見分けに使う。
    fn follows_value(&self, index: usize) -> bool {
        match self.previous_code(index) {
            None => false,
            Some(prev) => {
                // 数はそれだけで値。末尾の字だけで見ると `18.` を取り逃がす。
                // GLSL は `2.-r` のようにここを踏む書き方が多い。
                prev.class == TokenClass::Number
                    || prev.text.ends_with(|c: char| c.is_alphanumeric() || c == '_')
                    || matches!(prev.text, ")" | "]")
            }
        }
    }
}

/// 行の組み立て。インデントと空白の面倒をここに閉じ込める。
struct Writer {
    out: String,
    indent: usize,
    /// 行頭 (まだ何も書いていない)。
    at_line_start: bool,
    /// 直前に書いたものが、次にくっつくか。`(`・単項・`.`・前置の `++`。
    ///
    /// 書いた文字から見分けようとすると `9.` の小数点と `p.` の区切り、
    /// 前置の `++i` と後置の `i++` が区別できない。書いた側が言う。
    tight: bool,
}

impl Writer {
    fn new() -> Self {
        Self { out: String::new(), indent: 0, at_line_start: true, tight: false }
    }

    fn push(&mut self, text: &str) {
        if self.at_line_start {
            for _ in 0..self.indent {
                self.out.push_str(INDENT);
            }
            self.at_line_start = false;
        }
        self.out.push_str(text);
        self.tight = false;
    }

    /// 次のトークンをくっつけて書く。
    fn push_tight(&mut self, text: &str) {
        self.push(text);
        self.tight = true;
    }

    /// 直前の空白を取り消す。
    fn trim_trailing_space(&mut self) {
        while self.out.ends_with(' ') {
            self.out.pop();
        }
    }

    fn space(&mut self) {
        if !self.at_line_start && !self.out.ends_with(' ') {
            self.out.push(' ');
        }
    }

    /// 行頭では字下げが空白の代わりになるので、何もしない。
    fn space_unless_line_start(&mut self) {
        if !self.at_line_start && !self.tight {
            self.space();
        }
    }

    /// 直前が `}` で終わっているか。`} else` にできるかの判断に使う。
    fn ends_with_close_brace(&self) -> bool {
        self.out.trim_end().ends_with('}')
    }

    /// 1 行空ける。行頭でなければ改行してから。
    fn blank_line(&mut self) {
        if self.out.is_empty() {
            return;
        }
        self.newline();
        if !self.out.ends_with("\n\n") {
            self.out.push('\n');
        }
    }

    fn newline(&mut self) {
        if self.at_line_start {
            return;
        }
        while self.out.ends_with(' ') {
            self.out.pop();
        }
        self.out.push('\n');
        self.at_line_start = true;
    }

    /// 直前の改行を取り消して、同じ行に続ける。`} else` を作るのに使う。
    fn join_with_previous_line(&mut self) {
        while self.out.ends_with(['\n', ' ']) {
            self.out.pop();
        }
        self.at_line_start = false;
    }

    fn finish(mut self) -> String {
        while self.out.ends_with(['\n', ' ']) {
            self.out.pop();
        }
        self.out.push('\n');
        self.out
    }
}

/// `if (a) x = 1;` のように括弧を書かなかった本体を閉じる。
fn close_single_statements(out: &mut Writer, depths: &mut Vec<usize>) {
    while let Some(depth) = depths.last() {
        if *depth != out.indent {
            break;
        }
        depths.pop();
        out.indent = out.indent.saturating_sub(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compiler::compile;
    use crate::parser::parse;

    /// 実際に貼られたつぶやき GLSL。1 行のままでは読めない。
    const SHADER: &str = "for(float i,g,e,s;++i<18.;){vec3 p=vec3((FC.xy*2.-r)/r.y*(9.+cos(t*.5)*3.),g+.2)*rotate3D(t*.5,vec3(-4,sin(t)+7.,0));s=1.;for(int i;i++<9;p=vec3(1.5,4,3)-abs(abs(p)*e-vec3(1,1.2,3)))s*=e=max(.95,9./dot(p,p));g+=mod(length(p.yy),p.y)/s*.5;o.rgb+=hsv(.59,.4-g,s/4e3);}";

    #[test]
    fn a_shader_gets_laid_out() {
        let expanded = expand(SHADER);
        assert!(expanded.lines().count() > 5, "1 行のままです:\n{expanded}");
        assert!(expanded.contains("\n  vec3 p = vec3("), "字下げが入っていません:\n{expanded}");
    }

    /// 整形しても同じシェーダーであること。トークンの比較では
    /// 「くっつけてはいけないものをくっつけた」を見つけられない。
    #[test]
    fn a_formatted_shader_still_compiles() {
        for source in [SHADER, &expand(SHADER), &compress(&expand(SHADER))] {
            crate::glsl_sketch::GlslSketch::compile(source)
                .unwrap_or_else(|e| panic!("{}行{}列 {}\n{source}", e.line, e.column, e.message));
        }
    }

    #[test]
    fn a_shader_goes_back_to_one_line() {
        // GLSL は必ず `;` で終わるので、詰めるときに改行を残す理由が無い。
        assert_eq!(compress(&expand(SHADER)), SHADER);
    }

    /// `#` の行は 1 行で完結していなければ通らない。字下げも折り返しもしない。
    #[test]
    fn a_hash_line_is_left_alone() {
        let source = "#define S(a) smoothstep(0.,1.,a)\nvoid main(){o=vec4(S(FC.x/r.x));}\n#つぶやきGLSL";
        for text in [expand(source), compress(&expand(source))] {
            assert!(
                text.contains("\n#つぶやきGLSL") || text.starts_with("#つぶやきGLSL"),
                "タグが崩れました:\n{text}"
            );
            assert!(text.contains("#define S(a) smoothstep(0.,1.,a)\n"), "{text}");
        }
    }

    /// 小数点はプロパティの区切りではない。
    ///
    /// `2.` は「値の途中」に見えるので、次の `-` が単項に、`/` の前の空白が
    /// 落ちる。GLSL は `2.-r` や `9./d` と書くので、ここを踏み続ける。
    #[test]
    fn a_decimal_point_does_not_swallow_the_next_operator() {
        assert_eq!(expand("o=vec4(2.-r);").trim(), "o = vec4(2. - r);");
        assert_eq!(expand("s=9./dot(p,p);").trim(), "s = 9. / dot(p, p);");
        // プロパティの区切りのほうは、これまでどおり詰める。
        assert_eq!(expand("o.rgb=p.yy;").trim(), "o.rgb = p.yy;");
    }

    /// 後置の `++` は値の終わり。前置の `++` は次にくっつく。
    #[test]
    fn an_increment_knows_which_side_it_belongs_to() {
        assert_eq!(expand("for(int i;i++<9;)f();").trim(), "for (int i; i++ < 9;)\n  f();");
        assert_eq!(expand("for(float i;++i<18.;)f();").trim(), "for (float i; ++i < 18.;)\n  f();");
    }

    /// どちらの方言でもよいので Bytecode まで通す。
    ///
    /// トークン列の比較だけでは、トークナイザ自身の取りこぼし
    /// (`=>` を `=` と `>` に割るなど) を見つけられない。実際にコンパイルして
    /// 突き合わせる。
    fn program_of(source: &str) -> Result<String, String> {
        let program = match parse(source).ok().and_then(|ast| compile(&ast).ok()) {
            Some(program) => program,
            None => {
                let script = crate::js::parse(source).map_err(|e| e.to_string())?;
                crate::js::compile(&script).map_err(|e| e.to_string())?
            }
        };

        // `global_names` は HashMap なので並びが安定しない。命令列と鍵だけ見る。
        Ok(format!(
            "{:?}|{:?}|{:?}|{:?}|{}",
            program.functions, program.keys, program.setup, program.draw, program.global_count
        ))
    }

    /// 変換の前後で Bytecode が変わらないことを確かめる。
    ///
    /// 整形は見た目だけを変えるものなので、これが崩れたらバグ。
    fn assert_same_program(original: &str, transformed: &str) {
        let before = program_of(original).expect("元がコンパイルできる");
        let after = match program_of(transformed) {
            Ok(after) => after,
            Err(e) => panic!("変換後がコンパイルできない: {e}\n---\n{transformed}\n---"),
        };
        assert_eq!(before, after, "Bytecode が変わった\n--- 変換後 ---\n{transformed}\n---");
    }

    /// 空白とコメントを除いたトークン列。整形で並びが変わらないことを見る。
    fn code_tokens(source: &str) -> Vec<String> {
        crate::highlight::tokens(source)
            .into_iter()
            .filter(|s| !matches!(s.class, TokenClass::Plain | TokenClass::Comment))
            .map(|s| source[s.start..s.end].to_string())
            .collect()
    }

    /// 数値は短縮で表記が変わるので、値で比べる。
    fn same_token(before: &str, after: &str) -> bool {
        if before == after {
            return true;
        }
        match (
            before.trim_end_matches(['f', 'F']).parse::<f64>(),
            after.trim_end_matches(['f', 'F']).parse::<f64>(),
        ) {
            (Ok(a), Ok(b)) => a == b,
            _ => false,
        }
    }

    /// 実際に書かれうるコード。1 行に畳んだものも含む。
    const SAMPLES: &[&str] = &[
        "void draw(){background(0);}",
        "int t;void draw(){t++;background(t%255);}",
        "void draw(){for(int i=0;i<10;i++){point(i,i);}}",
        "void draw(){for(int i=0;i<10;i++)point(i,i);}",
        "void draw(){if(frameCount>10)background(0);else background(255);}",
        "void draw(){if(frameCount>10){background(0);}else{background(255);}}",
        "void draw(){float x=-1.0;float y=+2;x=x*-1;point(x,y);}",
        "float f(float a,float b){return a*b;}void draw(){point(f(1,2),0);}",
        "void draw(){int i=0;while(i<5){i++;}point(i,0);}",
        "void draw(){background(a()?0:255);}int a(){return 1;}",
    ];

    /// パースできないコードも含めた、より広い題材。
    ///
    /// 整形はトークンを並べ替えないので、コンパイルが通らなくても
    /// 「トークン列が変わらない」ことは常に成り立つ。
    const RAW_SAMPLES: &[&str] = &[
        "void draw(){float a=1.0,b;}",
        "int[] xs;",
        "void draw(){x=(int)y;}",
        "void draw(){switch(a){case 1:break;}}",
        "// コメントだけ",
        "void draw(){a=b/ /* わる */ c;}",
    ];

    #[test]
    fn expanding_keeps_the_token_sequence() {
        for source in SAMPLES.iter().chain(RAW_SAMPLES) {
            let before = code_tokens(source);
            let out = expand(source);
            let after = code_tokens(&out);
            assert_eq!(before, after, "トークンが変わった\n--- {source}\n--- {out}");
        }
    }

    #[test]
    fn compressing_keeps_the_token_sequence() {
        for source in SAMPLES.iter().chain(RAW_SAMPLES) {
            let before = code_tokens(source);
            let out = compress(source);
            let after = code_tokens(&out);
            assert_eq!(before.len(), after.len(), "トークン数が変わった\n--- {source}\n--- {out}");
            for (b, a) in before.iter().zip(&after) {
                assert!(same_token(b, a), "{b} が {a} になった\n--- {source}\n--- {out}");
            }
        }
    }

    /// p5.js で書かれた題材。整形が JavaScript の書き方を壊さないことを見る。
    const P5_SAMPLES: &[&str] = &[
        "draw=_=>{background(0);circle(1,2,3)}",
        "t=0\ndraw=_=>{t++;background(t%255)}",
        "draw=_=>{[1,2,3].map(v=>circle(v,v,v))}",
        "draw=_=>{p={x:1,y:2};p.x+=1;circle(p.x,p.y,3)}",
        "a=(y,d)=>point(y,d)\ndraw=_=>{a(1,2)}",
        "draw=_=>{for(i=1e4;i--;)point(i%99,i%77)}",
        "draw=_=>{x=1;if(x===1)background(0);else background(255)}",
        "draw=_=>{c=[];c.push(1);circle(c.length,0,1)}",
    ];

    #[test]
    fn expanding_never_breaks_p5_code() {
        for source in P5_SAMPLES {
            assert_same_program(source, &expand(source));
        }
    }

    #[test]
    fn compressing_never_breaks_p5_code() {
        for source in P5_SAMPLES {
            assert_same_program(source, &compress(source));
            assert_same_program(source, &expand(&compress(source)));
            assert_same_program(source, &compress(&expand(source)));
        }
    }

    /// 実際に貼られた形に近い、縮めて書かれた p5.js。
    ///
    /// アロー関数・既定値つき引数・カンマ演算子・セミコロン省略・括弧なしの
    /// for 本体が一度に出てくる。整形がここを壊さないことを固定する。
    const GOLFED: &str = "a=(y,d=mag(k=(5+sin(y*2-t/2)*2)*cos(i/29),e=y/7-13)-6)=>point((q=3*sin(k*2)+cos(y))*d+w,(cos(e)+sin(k))*d+w)\nt=0\ndraw=_=>{\nt||createCanvas(w=400,w)\nbackground(9),stroke(w,116)\nfor(t+=PI/240,i=1e4;i--;)a(i/295)\n}";

    #[test]
    fn a_golfed_sketch_survives_both_directions() {
        assert_same_program(GOLFED, &expand(GOLFED));
        assert_same_program(GOLFED, &compress(GOLFED));
        assert_same_program(GOLFED, &compress(&expand(GOLFED)));
        assert_same_program(GOLFED, &expand(&compress(GOLFED)));
    }

    #[test]
    fn expanding_a_golfed_sketch_closes_its_braces() {
        let out = expand(GOLFED);
        assert!(out.ends_with("}\n"), "\n{out}");
        assert!(!out.contains("\n  }"), "括弧なし本体の字下げが残っている\n{out}");
    }

    // ---- 折り返し ------------------------------------------------------

    #[test]
    fn a_long_line_is_wrapped_at_its_widest_group() {
        let out = expand(GOLFED);
        for line in out.lines() {
            assert!(
                line.chars().count() <= MAX_WIDTH,
                "{} 文字の行が残っている: {line}",
                line.chars().count()
            );
        }
    }

    #[test]
    fn wrapping_breaks_at_top_level_commas() {
        let out = expand(GOLFED);
        assert!(out.contains("a = (\n"), "\n{out}");
        assert!(out.contains("\n  y,\n"), "引数ごとに行を分ける\n{out}");
        assert!(out.contains("\n) => point("), "閉じ括弧は元の字下げへ戻す\n{out}");
    }

    #[test]
    fn short_lines_are_left_alone() {
        let out = expand("void draw() {\n  circle(1, 2, 3);\n}");
        assert_eq!(out, "void draw() {\n  circle(1, 2, 3);\n}\n", "\n{out}");
    }

    #[test]
    fn wrapping_is_idempotent() {
        let once = expand(GOLFED);
        assert_eq!(expand(&once), once, "2 回目で変わってはいけない\n{once}");
    }

    #[test]
    fn brackets_inside_a_comment_do_not_confuse_the_wrapper() {
        let long = format!("void draw() {{\n  point(1, 2); // {}\n}}", "あ".repeat(80));
        let out = expand(&long);
        assert!(out.contains("point(1, 2);"), "\n{out}");
    }

    #[test]
    fn a_blank_line_is_kept_but_not_multiplied() {
        let out = expand("int a = 1;\n\n\n\nint b = 2;\nvoid draw() {}");
        assert!(out.starts_with("int a = 1;\n\nint b = 2;"), "\n{out}");
    }

    #[test]
    fn arrows_survive_expanding() {
        // `=>` を `= >` に割ると代入とみなされて壊れる。
        let out = expand("draw=_=>{circle(1,2,3)}");
        assert!(out.contains("=>"), "\n{out}");
        assert!(!out.contains("= >"), "\n{out}");
    }

    #[test]
    fn member_access_is_not_spaced() {
        let out = expand("draw=_=>{p={x:1};circle(p.x,0,1)}");
        assert!(out.contains("p.x"), "\n{out}");
        assert!(!out.contains("p . x"), "\n{out}");
    }

    /// 詰めるときも同じ。`.` を数の一部と同じに扱うと、逆に伸びる。
    #[test]
    fn member_access_is_not_spaced_when_compressing() {
        let out = compress("draw=_=>{circle(p.x, 0, 1)}");
        assert!(out.contains("p.x"), "\n{out}");
        // `1` と `.5` を詰めると `1.5` になってしまう。ここは空けたまま。
        assert!(needs_space("1", ".5"), "数どうしはくっつけられない");
        assert!(!needs_space("p", "."), "プロパティの区切りは詰めてよい");
    }

    #[test]
    fn a_for_header_has_no_space_before_the_paren() {
        let out = expand("draw=_=>{for(i=3;i--;)point(i,i)}");
        assert!(!out.contains("; )"), "\n{out}");
    }

    #[test]
    fn newlines_between_statements_are_kept() {
        // セミコロンを書かない p5.js では、改行が文の区切り。畳むと壊れる。
        let out = expand("t=0\ndraw=_=>{t++}");
        assert!(out.starts_with("t = 0\n"), "\n{out}");
    }

    #[test]
    fn compressing_keeps_a_separator_where_a_newline_was_one() {
        // `t=0` と `draw=…` が繋がると別の意味になる。
        let out = compress("t=0\ndraw=_=>{t++}");
        assert!(out.starts_with("t=0\n"), "{out}");
    }

    #[test]
    fn compressing_drops_newlines_that_are_not_separators() {
        // セミコロンで終わっていれば改行は要らない。
        let out = compress("void draw() {\n  background(0);\n  point(1, 2);\n}");
        assert!(!out.contains('\n'), "{out}");
    }

    #[test]
    fn newlines_inside_parentheses_are_folded() {
        let out = expand("void draw() {\n  circle(1,\n    2, 3);\n}");
        assert!(out.contains("circle(1, 2, 3);"), "\n{out}");
    }

    #[test]
    fn expanding_never_changes_the_program() {
        for source in SAMPLES {
            assert_same_program(source, &expand(source));
        }
    }

    #[test]
    fn compressing_never_changes_the_program() {
        for source in SAMPLES {
            assert_same_program(source, &compress(source));
        }
    }

    #[test]
    fn the_bundled_sketches_survive_a_round_trip() {
        for source in [
            include_str!("../sketches/spiral.pde"),
            include_str!("../sketches/pulse-grid.pde"),
            include_str!("../sketches/lissajous.pde"),
            include_str!("../sketches/noise-field.pde"),
            include_str!("../sketches/moire.pde"),
            include_str!("../sketches/orbit.pde"),
        ] {
            assert_same_program(source, &expand(source));
            assert_same_program(source, &compress(source));
            // 詰めてから開いても、開いてから詰めても同じ意味。
            assert_same_program(source, &expand(&compress(source)));
            assert_same_program(source, &compress(&expand(source)));
        }
    }

    #[test]
    fn expanding_is_idempotent() {
        for source in SAMPLES {
            let once = expand(source);
            assert_eq!(expand(&once), once, "2 回目で変わってはいけない\n{once}");
        }
    }

    #[test]
    fn compressing_is_idempotent() {
        for source in SAMPLES {
            let once = compress(source);
            assert_eq!(compress(&once), once);
        }
    }

    #[test]
    fn a_one_liner_becomes_several_lines() {
        let out = expand("void draw(){background(0);circle(1,2,3);}");
        assert_eq!(
            out,
            "void draw() {\n  background(0);\n  circle(1, 2, 3);\n}\n",
            "\n{out}"
        );
    }

    #[test]
    fn nested_blocks_are_indented() {
        let out = expand("void draw(){for(int i=0;i<3;i++){point(i,i);}}");
        assert_eq!(
            out,
            "void draw() {\n  for (int i = 0; i < 3; i++) {\n    point(i, i);\n  }\n}\n",
            "\n{out}"
        );
    }

    #[test]
    fn a_body_without_braces_is_indented_for_one_statement_only() {
        let out = expand("void draw(){for(int i=0;i<3;i++)point(i,i);background(0);}");
        assert_eq!(
            out,
            "void draw() {\n  for (int i = 0; i < 3; i++)\n    point(i, i);\n  background(0);\n}\n",
            "\n{out}"
        );
    }

    #[test]
    fn else_stays_on_the_closing_brace() {
        let out = expand("void draw(){if(a()>0){background(0);}else{background(255);}}int a(){return 1;}");
        assert!(out.contains("} else {"), "\n{out}");
    }

    #[test]
    fn else_after_a_braceless_body_starts_its_own_line() {
        let out = expand("void draw(){if(frameCount>10)background(0);else background(255);}");
        assert_eq!(
            out,
            "void draw() {\n  if (frameCount > 10)\n    background(0);\n  else\n    background(255);\n}\n",
            "\n{out}"
        );
    }

    #[test]
    fn else_if_stays_on_one_line() {
        let out = expand("void draw(){if(a()>0){point(0,0);}else if(a()<0){point(1,1);}else{point(2,2);}}int a(){return 1;}");
        assert!(out.contains("} else if ("), "\n{out}");
        assert!(out.contains("} else {"), "\n{out}");
    }

    #[test]
    fn top_level_definitions_are_separated_by_a_blank_line() {
        let out = expand("int t;void setup(){size(1,1);}void draw(){background(0);}");
        assert_eq!(
            out,
            "int t;\nvoid setup() {\n  size(1, 1);\n}\n\nvoid draw() {\n  background(0);\n}\n",
            "\n{out}"
        );
    }

    #[test]
    fn unary_minus_is_not_spaced_but_subtraction_is() {
        let out = expand("void draw(){float x=-1;x=x-1;}");
        assert!(out.contains("float x = -1;"), "\n{out}");
        assert!(out.contains("x = x - 1;"), "\n{out}");
    }

    #[test]
    fn comments_survive_expanding() {
        let out = expand("// 先頭\nvoid draw(){background(0);// 行末\n}");
        assert!(out.contains("// 先頭"), "\n{out}");
        assert!(out.contains("// 行末"), "\n{out}");
    }

    #[test]
    fn comments_are_dropped_when_compressing() {
        let out = compress("// 消える\nvoid draw(){/* これも */background(0);}");
        assert_eq!(out, "void draw(){background(0);}");
    }

    #[test]
    fn compressing_actually_shrinks_a_formatted_sketch() {
        let source = include_str!("../sketches/spiral.pde");
        let compressed = compress(source);
        // コメントと字下げが落ちるぶん、2/3 以下にはなる。
        assert!(
            compressed.len() * 3 < source.len() * 2,
            "{} → {} 文字にしかならない",
            source.len(),
            compressed.len()
        );
        assert!(!compressed.contains('\n'), "改行が残っている");
    }

    #[test]
    fn compressing_keeps_tokens_apart_when_needed() {
        assert_eq!(compress("void draw(){int a=1;float b=a;}"), "void draw(){int a=1;float b=a;}");
        // `+ +` を `++` にしない。
        assert_eq!(compress("void draw(){int a=1;a=a+ +1;}"), "void draw(){int a=1;a=a+ +1;}");
        // `- -` も同じ。
        assert_eq!(compress("void draw(){int a=1;a=a- -1;}"), "void draw(){int a=1;a=a- -1;}");
    }

    #[test]
    fn a_dropped_comment_never_creates_a_new_one() {
        // `a / /*c*/ b` を詰めて `a//b` にしてしまうと、以降が全部コメントになる。
        let out = compress("void draw(){float a=4;float b=2;float c=a/ /* わる */ b;point(c,0);}");
        assert!(!out.contains("//"), "コメントができてしまった: {out}");
        assert_same_program("void draw(){float a=4;float b=2;float c=a/b;point(c,0);}", &out);
    }

    // ---- 数値の短縮 -----------------------------------------------------

    #[test]
    fn numbers_get_shorter_without_changing_type() {
        assert_eq!(shorten_number("0.5"), ".5");
        assert_eq!(shorten_number("1.0"), "1.");
        assert_eq!(shorten_number("1.50"), "1.5");
        assert_eq!(shorten_number("2.0f"), "2.");
        assert_eq!(shorten_number("0.0"), "0.");
        assert_eq!(shorten_number("10.00"), "10.");
        assert_eq!(shorten_number("0.250"), ".25");
    }

    #[test]
    fn integers_stay_integers() {
        assert_eq!(shorten_number("0"), "0");
        assert_eq!(shorten_number("42"), "42");
        // `1f` は小数なので、点を足して型を保つ。
        assert_eq!(shorten_number("1f"), "1.");
    }

    #[test]
    fn exponents_are_left_alone() {
        assert_eq!(shorten_number("1e3"), "1e3");
        assert_eq!(shorten_number("1.5e-3"), "1.5e-3");
    }

    #[test]
    fn shortened_numbers_still_parse_to_the_same_value() {
        for text in ["0.5", "1.0", "1.50", "2.0f", "0.0", "10.00", "0.250", "42", "1e3"] {
            let short = shorten_number(text);
            let original: f64 = text.trim_end_matches(['f', 'F']).parse().expect("元が読める");
            let after: f64 = short.parse().expect("短縮後が読める");
            assert_eq!(original, after, "{text} → {short}");
        }
    }

    #[test]
    fn broken_code_does_not_panic() {
        for source in ["", "void draw(){", "int x =", "@", "/* 閉じない"] {
            let _ = expand(source);
            let _ = compress(source);
        }
    }
}



