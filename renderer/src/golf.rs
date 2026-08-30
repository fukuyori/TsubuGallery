//! FragCoord.xyz の GOLF 記法を、つぶやき GLSL へ展開する。
//!
//! GOLF は <https://fragcoord.xyz/docs#golf> で定義されている、文字数を詰める
//! ためのフラグメントシェーダー記法。XorDev 氏の X への投稿はこの形で書かれる。
//! 中身は GLSL ES 300 へのテキスト変換で、FragCoord のトランスパイラの手順を
//! 追って同じ順に展開する。
//!
//! ```text
//! GOLF → (この module) → つぶやき GLSL → shader::compile → WGSL
//! ```
//!
//! 出力は [`crate::shader::compile`] が受ける つぶやき GLSL (twigl geekest 相当)
//! なので、`R` `T` `C` `O` のような 1 文字の uniform は twigl の `r` `t` `FC`
//! `o` へ寄せる。FragCoord の `u_resolution` は `vec3`、`u_mouse` は px 単位の
//! `vec4` なので、そこは組み立てて合わせる。
//!
//! 各変換は行の中で完結させ、行数を変えない。naga のエラー位置がそのまま
//! 元のソースの行を指すようにするため。例外はトップレベルの関数定義で、
//! GLSL では `main` の中に置けないので前へ持ち上げる。
//!
//! # 対応していないもの
//!
//! - `fX` (f2/f3/f4 へ 3 通り展開する総称関数)
//! - `%` の float 版 (`mod` へ書き換えない。GLSL のままなので int 同士だけ)
//! - `P1`〜`P4` `B` `A` `K` `W` などのテクスチャ系 uniform。名前は
//!   `u_pass1` のような FragCoord の名前へ展開し、naga が「無い」と言う。
//! - `D` (フレーム間隔) `Y` (日付) `S` (スクロール) `G` (ドラッグ) なども同様。
//!
//! 空引数 `f4(,33,11,)` の `0.0` 埋め、`@(N)` ループ、行末 `;` の省略、
//! `**` (pow)、`~x` (1-x)、`#D` などの短縮指令、1 行関数定義、暗黙の `main`
//! は扱う。整数リテラルの float 化は naga が暗黙変換を受け付けるので行わない。

/// GOLF の型名。展開後の GLSL 名と対にする。
const TYPES: &[(&str, &str)] = &[
    ("s2", "sampler2D"),
    ("f2", "vec2"),
    ("f3", "vec3"),
    ("f4", "vec4"),
    ("f", "float"),
    ("i2", "ivec2"),
    ("i3", "ivec3"),
    ("i4", "ivec4"),
    ("u2", "uvec2"),
    ("u3", "uvec3"),
    ("u4", "uvec4"),
    ("b2", "bvec2"),
    ("b3", "bvec3"),
    ("b4", "bvec4"),
    ("m2", "mat2"),
    ("m3", "mat3"),
    ("m4", "mat4"),
];

/// 関数名の別名。FragCoord の表のとおり。古い 2 文字の別名も受ける。
const FUNCTIONS: &[(&str, &str)] = &[
    ("txF", "texelFetch"),
    ("txS", "textureSize"),
    ("tex", "texture"),
    ("nor", "normalize"),
    ("len", "length"),
    ("crs", "cross"),
    ("clm", "clamp"),
    ("sms", "smoothstep"),
    ("stp", "step"),
    ("flr", "floor"),
    ("frc", "fract"),
    ("sgn", "sign"),
    ("sqt", "sqrt"),
    ("isq", "inversesqrt"),
    ("rfl", "reflect"),
    ("rfr", "refract"),
    ("dst", "distance"),
    ("fwd", "fwidth"),
    ("asn", "asin"),
    ("acs", "acos"),
    // GLSL の atan は 2 引数版が atan2 を兼ねる。
    ("at2", "atan"),
    ("atn", "atan"),
    ("ex2", "exp2"),
    ("lg2", "log2"),
    ("cel", "ceil"),
    ("rnd", "round"),
    ("rad", "radians"),
    ("deg", "degrees"),
    ("ddx", "dFdx"),
    ("ddy", "dFdy"),
    ("det", "determinant"),
    ("trp", "transpose"),
    ("inv", "inverse"),
    ("mcm", "matrixCompMult"),
    ("ab", "abs"),
    ("mn", "min"),
    ("mx", "max"),
    ("ler", "mix"),
    ("dpt", "dot"),
    ("pw", "pow"),
    ("md", "mod"),
    ("sn", "sin"),
    ("cs", "cos"),
    ("tn", "tan"),
    ("xp", "exp"),
    ("lg", "log"),
    ("x2", "exp2"),
    ("l2", "log2"),
];

/// 1 文字 (と少しの 2 文字) の uniform。twigl の名前で持っているものはそれへ。
///
/// `R` は FragCoord では `vec3` なので `r` から組み立てる。`M` は px 単位の
/// `vec4` (xy がカーソル、zw がクリック) なので、0..1 の `m` に `r` を掛けて
/// 位置だけ合わせる。クリックは取っていないので 0。
const UNIFORMS: &[(&str, &str)] = &[
    ("P1", "u_pass1"),
    ("P2", "u_pass2"),
    ("P3", "u_pass3"),
    ("P4", "u_pass4"),
    ("RR", "u_refresh_rate"),
    ("CP", "u_camera_pos"),
    ("CD", "u_camera_dir"),
    ("R", "vec3(r, 1.0)"),
    ("T", "t"),
    ("D", "u_time_delta"),
    ("F", "f"),
    ("M", "vec4(m * r, 0.0, 0.0)"),
    ("C", "FC"),
    ("O", "o"),
    ("G", "u_drag"),
    ("Y", "u_date"),
    ("A", "u_audio"),
    ("K", "u_keyboard"),
    ("W", "u_webcam"),
    ("B", "u_main"),
    ("N", "u_recursion"),
    ("S", "u_scroll"),
];

/// twigl の前置きが持つ名前のうち、GOLF では自由に使える小文字のもの。
///
/// GOLF が予約するのは大文字の `R` `T` `M` `O` だけなので、作品は `t` や `r`
/// をローカル変数に使う (`f3 p = ..., t = p`)。そのまま `T` を `t` へ写すと、
/// 作品の `t` が時間を隠して絵が止まる。先に作品側を `_t` へ退避してから
/// [`UNIFORMS`] を写す。`f` は型名なので作品側には出てこない。
const SHADOWED: &[(&str, &str)] =
    &[("r", "_r"), ("t", "_t"), ("m", "_m"), ("o", "_o"), ("FC", "_FC")];

/// 短縮指令。行頭だけ。
const DIRECTIVES: &[(&str, &str)] =
    &[("#D", "#define"), ("#I", "#ifdef"), ("#E", "#endif"), ("#L", "#else"), ("#U", "#undef")];

/// 1 行関数定義や題名の判定で「識別子ではあるが名前には使えない」語。
const RESERVED: &[&str] = &[
    "void", "if", "else", "for", "while", "do", "return", "break", "continue", "switch", "case",
    "default", "struct", "layout", "discard", "uniform", "const", "in", "out", "inout",
    "precision", "main", "flat", "highp", "mediump", "lowp", "float", "int", "uint", "bool",
    "vec2", "vec3", "vec4", "ivec2", "ivec3", "ivec4", "uvec2", "uvec3", "uvec4", "bvec2",
    "bvec3", "bvec4", "mat2", "mat3", "mat4", "sampler2D", "sampler3D", "samplerCube", "f",
    "f2", "f3", "f4", "fX", "i2", "i3", "i4", "u2", "u3", "u4", "b2", "b3", "b4", "m2", "m3",
    "m4", "s2", "fragColor", "R", "T", "D", "F", "M", "C", "O", "G", "Y", "RR", "CP", "CD",
    "A", "K", "W", "B", "N", "S", "P1", "P2", "P3", "P4", "true", "false",
];

/// GLSL 側の型名。トップレベルの関数定義を見つけるのに使う。
const GLSL_TYPES: &[&str] = &[
    "void", "float", "int", "uint", "bool", "vec2", "vec3", "vec4", "ivec2", "ivec3", "ivec4",
    "uvec2", "uvec3", "uvec4", "bvec2", "bvec3", "bvec4", "mat2", "mat3", "mat4", "sampler2D",
    "sampler3D", "samplerCube",
];

/// GOLF を つぶやき GLSL へ展開する。
///
/// 通らないコードでも panic せず、それなりの文字列を返す。正しさの検証は
/// この後の naga に任せる。
pub fn to_glsl(source: &str) -> String {
    let mut text = blank_comments(source);
    text = blank_title(&text);
    text = expand_directives(&text);
    text = expand_loops(&text);
    text = expand_one_line_functions(&text);
    text = insert_semicolons(&text);
    text = expand_pow(&text);
    text = expand_complement(&text);
    text = replace_words(&text, TYPES);
    text = split_declarations(&text);
    text = fill_empty_arguments(&text);
    text = wrap_vector_initializers(&text);
    text = replace_calls(&text, FUNCTIONS);
    text = replace_words(&text, SHADOWED);
    text = replace_words(&text, UNIFORMS);
    text = widen_scalar_output(&text);
    text = define_hpi(&text);
    hoist_functions(&text)
}

// ---- 文字の種類 ------------------------------------------------------------

fn is_word(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

fn is_ident_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_'
}

/// `open` にある `(` `[` `{` と対になる閉じ括弧の位置。
fn matching(bytes: &[u8], open: usize) -> Option<usize> {
    let (o, c) = match bytes[open] {
        b'(' => (b'(', b')'),
        b'[' => (b'[', b']'),
        b'{' => (b'{', b'}'),
        _ => return None,
    };
    let mut depth = 0usize;
    for (i, &b) in bytes.iter().enumerate().skip(open) {
        if b == o {
            depth += 1;
        } else if b == c {
            depth -= 1;
            if depth == 0 {
                return Some(i);
            }
        }
    }
    None
}

/// 括弧の外にあるカンマで切る。
fn split_top_level(text: &str) -> Vec<&str> {
    let bytes = text.as_bytes();
    let mut parts = Vec::new();
    let mut depth = 0i32;
    let mut start = 0;
    for (i, &b) in bytes.iter().enumerate() {
        match b {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth -= 1,
            b',' if depth == 0 => {
                parts.push(&text[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    parts.push(&text[start..]);
    parts
}

/// 識別子を単位に置き換える。`.` の後 (メンバー) と数値の中は触らない。
fn replace_words(text: &str, table: &[(&str, &str)]) -> String {
    let bytes = text.as_bytes();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b.is_ascii_digit() || (b == b'.' && bytes.get(i + 1).is_some_and(u8::is_ascii_digit)) {
            // 数値。`1e5f` の f や `2.f` を語と見ない。
            let start = i;
            i += 1;
            while i < bytes.len() && (is_word(bytes[i]) || bytes[i] == b'.') {
                i += 1;
            }
            out.push_str(&text[start..i]);
            continue;
        }
        if is_ident_start(b) {
            let start = i;
            while i < bytes.len() && is_word(bytes[i]) {
                i += 1;
            }
            let word = &text[start..i];
            let after_dot = start > 0 && bytes[start - 1] == b'.';
            match table.iter().find(|(from, _)| *from == word) {
                Some((_, to)) if !after_dot => out.push_str(to),
                _ => out.push_str(word),
            }
            continue;
        }
        out.push(b as char);
        i += 1;
    }
    out
}

/// `name(` の形だけ置き換える。呼び出しでない同名の変数はそのまま。
fn replace_calls(text: &str, table: &[(&str, &str)]) -> String {
    let bytes = text.as_bytes();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if is_word(b) || b == b'.' {
            let start = i;
            while i < bytes.len() && (is_word(bytes[i]) || bytes[i] == b'.') {
                i += 1;
            }
            let word = &text[start..i];
            let mut k = i;
            while k < bytes.len() && (bytes[k] == b' ' || bytes[k] == b'\t') {
                k += 1;
            }
            let called = bytes.get(k) == Some(&b'(');
            match table.iter().find(|(from, _)| *from == word) {
                Some((_, to)) if called => out.push_str(to),
                _ => out.push_str(word),
            }
            continue;
        }
        out.push(b as char);
        i += 1;
    }
    out
}

// ---- 前処理 ----------------------------------------------------------------

/// コメントを空白にする。行数は変えない。
fn blank_comments(source: &str) -> String {
    let bytes = source.as_bytes();
    let mut out = String::with_capacity(source.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'/' && bytes.get(i + 1) == Some(&b'/') {
            while i < bytes.len() && bytes[i] != b'\n' {
                out.push(' ');
                i += 1;
            }
            continue;
        }
        if bytes[i] == b'/' && bytes.get(i + 1) == Some(&b'*') {
            i += 2;
            out.push_str("  ");
            while i < bytes.len() && !(bytes[i] == b'*' && bytes.get(i + 1) == Some(&b'/')) {
                out.push(if bytes[i] == b'\n' { '\n' } else { ' ' });
                i += 1;
            }
            if i < bytes.len() {
                out.push_str("  ");
                i += 2;
            }
            continue;
        }
        // 文字単位で写す。マルチバイトは UTF-8 の続きをまとめて。
        let ch = source[i..].chars().next().unwrap_or(' ');
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

/// 投稿の 1 行目に付く題名を空にする。
///
/// XorDev 氏の投稿は `Fever` のように題名から始まる。コードではないので
/// そのままでは `Unknown variable: Fever` で転ぶ。語だけが並び、演算子も
/// 括弧も無く、先頭が型名や予約語でない最初の行を題名と見る。
fn blank_title(text: &str) -> String {
    let mut lines: Vec<String> = text.split('\n').map(str::to_string).collect();
    let first = lines.iter().position(|l| !l.trim().is_empty());
    if let Some(index) = first
        && looks_like_title(lines[index].trim())
    {
        lines[index].clear();
    }
    lines.join("\n")
}

fn looks_like_title(line: &str) -> bool {
    if line.starts_with('#') {
        return false;
    }
    let plain = line.chars().all(|c| {
        c.is_alphanumeric() || matches!(c, ' ' | '\t' | '_' | '\'' | '!' | '?' | '.' | ':' | '-')
    });
    if !plain {
        return false;
    }
    let Some(first) = line.split_whitespace().next() else { return false };
    if !first.bytes().next().is_some_and(is_ident_start) {
        return false;
    }
    !RESERVED.contains(&first) && !GLSL_TYPES.contains(&first)
}

/// `#D` などの短縮指令を伸ばす。
fn expand_directives(text: &str) -> String {
    text.split('\n')
        .map(|line| {
            let trimmed = line.trim_start();
            let indent = &line[..line.len() - trimmed.len()];
            for (short, long) in DIRECTIVES {
                if let Some(rest) = trimmed.strip_prefix(short)
                    && !rest.bytes().next().is_some_and(is_word)
                {
                    return format!("{indent}{long}{rest}");
                }
            }
            line.to_string()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// `@(N)` を `for(int _fc=0;_fc<N;_fc++)` へ。
///
/// `@(i, N)` は変数名を、`@(i, from, N)` は開始値も指定する。名前を省いた
/// ときは、入れ子で衝突しないよう `_fc` `_fc1` … を順に使う。
fn expand_loops(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = String::with_capacity(text.len());
    let mut used = 0usize;
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'@' {
            let mut k = i + 1;
            while k < bytes.len() && (bytes[k] == b' ' || bytes[k] == b'\t') {
                k += 1;
            }
            if bytes.get(k) == Some(&b'(')
                && let Some(close) = matching(bytes, k)
            {
                let args: Vec<&str> = split_top_level(&text[k + 1..close]).iter().map(|a| a.trim()).collect();
                let header = match args.as_slice() {
                    [count] => {
                        let name = loop_name(text, &mut used);
                        Some(format!("for(int {name}=0;{name}<{count};{name}++)"))
                    }
                    [name, count] => Some(format!("for(int {name}=0;{name}<{count};{name}++)")),
                    [name, from, count] => {
                        Some(format!("for(int {name}={from};{name}<{count};{name}++)"))
                    }
                    _ => None,
                };
                if let Some(header) = header {
                    out.push_str(&header);
                    i = close + 1;
                    continue;
                }
            }
        }
        let ch = text[i..].chars().next().unwrap_or(' ');
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

/// まだ使っていないループ変数名。ソースに同名があれば飛ばす。
fn loop_name(text: &str, used: &mut usize) -> String {
    loop {
        let name = if *used == 0 { "_fc".to_string() } else { format!("_fc{used}") };
        *used += 1;
        if !contains_word(text, &name) {
            return name;
        }
    }
}

fn contains_word(text: &str, word: &str) -> bool {
    let bytes = text.as_bytes();
    let mut from = 0;
    while let Some(at) = text[from..].find(word) {
        let start = from + at;
        let end = start + word.len();
        let before = start == 0 || !is_word(bytes[start - 1]);
        let after = end >= bytes.len() || !is_word(bytes[end]);
        if before && after {
            return true;
        }
        from = start + 1;
    }
    false
}

/// `name a b = expr` の 1 行関数定義を伸ばす。
///
/// 戻り値も引数も float。式が `true`/`false` なら bool、`3u` なら uint。
/// ブロックの外にある行だけ見る。
fn expand_one_line_functions(text: &str) -> String {
    let mut depth = 0i32;
    let mut out = Vec::new();
    for line in text.split('\n') {
        let mut replaced = None;
        if depth == 0 {
            replaced = one_line_function(line);
        }
        let emitted = replaced.unwrap_or_else(|| line.to_string());
        for b in emitted.bytes() {
            match b {
                b'{' => depth += 1,
                b'}' => depth = (depth - 1).max(0),
                _ => {}
            }
        }
        out.push(emitted);
    }
    out.join("\n")
}

fn one_line_function(line: &str) -> Option<String> {
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.starts_with('#') {
        return None;
    }
    let indent = &line[..line.len() - line.trim_start().len()];
    let eq = plain_assignment(trimmed)?;
    let head = trimmed[..eq].trim();
    let body = trimmed[eq + 1..].trim().trim_end_matches(';').trim();
    if body.is_empty() {
        return None;
    }
    let words: Vec<&str> = head.split_whitespace().collect();
    if words.len() < 2 || !words.iter().all(|w| is_identifier(w)) {
        return None;
    }
    let name = words[0];
    if RESERVED.contains(&name) {
        return None;
    }
    let (ret, param) = if body == "true" || body == "false" {
        ("bool", "bool")
    } else if body.ends_with('u') && body[..body.len() - 1].bytes().all(|b| b.is_ascii_digit()) {
        ("uint", "uint")
    } else {
        ("float", "float")
    };
    let params: Vec<String> = words[1..].iter().map(|p| format!("{param} {p}")).collect();
    Some(format!("{indent}{ret} {name}({}) {{ return {body}; }}", params.join(", ")))
}

fn is_identifier(word: &str) -> bool {
    let bytes = word.as_bytes();
    bytes.first().is_some_and(|b| is_ident_start(*b)) && bytes.iter().all(|b| is_word(*b))
}

/// `==` `!=` `<=` `>=` `+=` などではない、単独の `=` の位置。
fn plain_assignment(text: &str) -> Option<usize> {
    let bytes = text.as_bytes();
    for i in 0..bytes.len() {
        if bytes[i] != b'=' {
            continue;
        }
        let before = if i > 0 { bytes[i - 1] } else { b' ' };
        let after = bytes.get(i + 1).copied().unwrap_or(b' ');
        if !matches!(before, b'=' | b'!' | b'<' | b'>') && after != b'=' {
            return Some(i);
        }
    }
    None
}

// ---- 行末の `;` ------------------------------------------------------------

/// 省かれた行末の `;` を補う。
///
/// 行末が `,` `;` `{` `}` の行、`#` で始まる行、`if (...)` `for (...)` のような
/// 制御文の頭、`else` / `do` だけの行には足さない。`}` で終わる行は、その手前の
/// 文へ足す。括弧が閉じていない行は、閉じる行まで足さない。
fn insert_semicolons(text: &str) -> String {
    let mut out = Vec::new();
    let mut open = 0i32;
    for line in text.split('\n') {
        for b in line.bytes() {
            match b {
                b'(' | b'[' => open += 1,
                b')' | b']' => open = (open - 1).max(0),
                _ => {}
            }
        }
        if open > 0 {
            out.push(line.to_string());
            continue;
        }
        out.push(terminate(line));
    }
    out.join("\n")
}

fn terminate(line: &str) -> String {
    let trimmed = line.trim_end();
    if trimmed.trim().is_empty() || trimmed.trim_start().starts_with('#') || trimmed.ends_with(',') {
        return line.to_string();
    }
    // 末尾の `}` の並びを外して、その手前の文を見る。
    let body_end = trimmed.trim_end_matches(['}', ' ', '\t']).len();
    let (body, braces) = trimmed.split_at(body_end);
    if !braces.is_empty() {
        let body = body.trim_end();
        return if body.is_empty() || needs_no_semicolon(body) {
            line.to_string()
        } else {
            format!("{body};{braces}")
        };
    }
    if needs_no_semicolon(trimmed) {
        return line.to_string();
    }
    format!("{trimmed};")
}

fn needs_no_semicolon(statement: &str) -> bool {
    let s = statement.trim_end();
    if s.ends_with(';') || s.ends_with('{') || s.ends_with('}') || s.ends_with(',') {
        return true;
    }
    let last_word = s.rsplit(|c: char| !(c.is_alphanumeric() || c == '_')).next().unwrap_or("");
    if s.ends_with(last_word) && matches!(last_word, "else" | "do") {
        return true;
    }
    is_control_header(s)
}

/// `if (...)` `for (...)` `while (...)` `switch (...)` で終わっているか。
fn is_control_header(s: &str) -> bool {
    if !s.ends_with(')') {
        return false;
    }
    let bytes = s.as_bytes();
    let mut depth = 0i32;
    let mut open = None;
    for i in (0..bytes.len()).rev() {
        match bytes[i] {
            b')' => depth += 1,
            b'(' => {
                depth -= 1;
                if depth == 0 {
                    open = Some(i);
                    break;
                }
            }
            _ => {}
        }
    }
    let Some(open) = open else { return false };
    let head = s[..open].trim_end();
    let word = head.rsplit(|c: char| !(c.is_alphanumeric() || c == '_')).next().unwrap_or("");
    head.ends_with(word) && matches!(word, "if" | "for" | "while" | "switch")
}

// ---- 演算子 ----------------------------------------------------------------

/// `a ** b` を `pow(a, b)` へ。右から畳むので `a ** b ** c` は `pow(a, pow(b, c))`。
fn expand_pow(text: &str) -> String {
    let mut text = text.to_string();
    let mut guard = 0;
    while let Some(at) = text.rfind("**") {
        guard += 1;
        if guard > 200 {
            break;
        }
        let bytes = text.as_bytes();
        let left = operand_before(bytes, at);
        let right = operand_after(bytes, at + 2);
        match (left, right) {
            (Some(l), Some(r)) if l.start < l.end && r.start < r.end => {
                let base = text[l.clone()].trim().to_string();
                let exponent = text[r.clone()].trim().to_string();
                text.replace_range(l.start..r.end, &format!("pow({base}, {exponent})"));
            }
            // 取れないときは壊さずに `*` へ落とす。naga が文句を言う。
            _ => text.replace_range(at..at + 2, "*"),
        }
    }
    text
}

/// `~x` を `(1.0-(x))` へ。
fn expand_complement(text: &str) -> String {
    let mut text = text.to_string();
    let mut guard = 0;
    while let Some(at) = text.rfind('~') {
        guard += 1;
        if guard > 200 {
            break;
        }
        let bytes = text.as_bytes();
        match operand_after(bytes, at + 1) {
            Some(r) if r.start < r.end => {
                let operand = text[r.clone()].trim().to_string();
                text.replace_range(at..r.end, &format!("(1.0-({operand}))"));
            }
            _ => text.replace_range(at..at + 1, " "),
        }
    }
    text
}

/// `at` の手前にある被演算子の範囲。`f(x).y` `p.xy` `2.` `(a+b)` を 1 つと見る。
fn operand_before(bytes: &[u8], at: usize) -> Option<std::ops::Range<usize>> {
    let mut i = at;
    while i > 0 && bytes[i - 1].is_ascii_whitespace() {
        i -= 1;
    }
    if i == 0 {
        return None;
    }
    let end = i;
    // `.xy` のような後置を含め、`)` なら括弧ごと戻る。`f(x).y` は
    // メンバー → 括弧 → 関数名と、つながる限り戻る。
    loop {
        if i > 0 && bytes[i - 1] == b')' {
            let mut depth = 0i32;
            let mut k = i;
            while k > 0 {
                k -= 1;
                match bytes[k] {
                    b')' => depth += 1,
                    b'(' => {
                        depth -= 1;
                        if depth == 0 {
                            break;
                        }
                    }
                    _ => {}
                }
            }
            if depth != 0 {
                return None;
            }
            i = k;
            // 関数名。
            while i > 0 && is_word(bytes[i - 1]) {
                i -= 1;
            }
        } else if i > 0 && (is_word(bytes[i - 1]) || bytes[i - 1] == b'.') {
            while i > 0 && (is_word(bytes[i - 1]) || bytes[i - 1] == b'.') {
                i -= 1;
            }
        } else {
            break;
        }
        if !(i > 0 && (bytes[i - 1] == b'.' || bytes[i - 1] == b')')) {
            break;
        }
    }
    (i < end).then_some(i..end)
}

/// `at` から始まる被演算子の範囲。符号、括弧、関数呼び出し、後置の `.xy` を含む。
fn operand_after(bytes: &[u8], at: usize) -> Option<std::ops::Range<usize>> {
    let mut i = at;
    while i < bytes.len() && bytes[i].is_ascii_whitespace() {
        i += 1;
    }
    let start = i;
    if i < bytes.len() && (bytes[i] == b'-' || bytes[i] == b'+') {
        i += 1;
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
    }
    if i >= bytes.len() {
        return None;
    }
    if bytes[i] == b'(' {
        i = matching(bytes, i)? + 1;
    } else if is_word(bytes[i]) || bytes[i] == b'.' {
        while i < bytes.len() && (is_word(bytes[i]) || bytes[i] == b'.') {
            i += 1;
        }
        if i < bytes.len() && bytes[i] == b'(' {
            i = matching(bytes, i)? + 1;
        }
    } else {
        return None;
    }
    // 後置のメンバー。`f(x).y`
    while i < bytes.len() && bytes[i] == b'.' {
        i += 1;
        while i < bytes.len() && is_word(bytes[i]) {
            i += 1;
        }
    }
    Some(start..i)
}

// ---- 宣言と構築子 ----------------------------------------------------------

/// `float a = 1., b;` を `float a = 1.; float b;` に分ける。
///
/// `vec3 p = x, q = y;` を [`wrap_vector_initializers`] が `vec3(x, q = y)` と
/// 包んでしまわないための下ごしらえ。括弧の中 (`for` の頭) は触らない。
fn split_declarations(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < bytes.len() {
        if is_ident_start(bytes[i]) && (i == 0 || !is_word(bytes[i - 1])) {
            let start = i;
            while i < bytes.len() && is_word(bytes[i]) {
                i += 1;
            }
            let word = &text[start..i];
            let is_type = GLSL_TYPES.contains(&word) && word != "void";
            let at_statement_start = statement_start(bytes, start);
            if is_type
                && at_statement_start
                && bytes.get(i).is_some_and(|b| b.is_ascii_whitespace())
                && let Some(end) = statement_end(bytes, i)
            {
                let rest = &text[i..end];
                let parts = split_top_level(rest);
                if parts.len() > 1 && all_declarators(&parts) {
                    let joined = parts
                        .iter()
                        .map(|p| format!("{word} {}", p.trim()))
                        .collect::<Vec<_>>()
                        .join("; ");
                    out.push_str(&joined);
                    i = end;
                    continue;
                }
            }
            out.push_str(word);
            continue;
        }
        let ch = text[i..].chars().next().unwrap_or(' ');
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

/// 各部分が `name` か `name = ...` の形か。関数定義の引数並びを除けるように。
fn all_declarators(parts: &[&str]) -> bool {
    parts.iter().all(|p| {
        let p = p.trim();
        let name_end = p.bytes().position(|b| !is_word(b)).unwrap_or(p.len());
        let name = &p[..name_end];
        let rest = p[name_end..].trim_start();
        is_identifier(name) && (rest.is_empty() || rest.starts_with('=') && !rest.starts_with("=="))
    })
}

/// `at` が文の先頭か。手前が `;` `{` `}` か行頭で、括弧の中でない。
fn statement_start(bytes: &[u8], at: usize) -> bool {
    let mut i = at;
    while i > 0 && (bytes[i - 1] == b' ' || bytes[i - 1] == b'\t') {
        i -= 1;
    }
    if i == 0 {
        return true;
    }
    match bytes[i - 1] {
        b';' | b'{' | b'}' | b'\n' => {
            // `for (` の中ではないか。手前の未閉の `(` を探す。
            let mut depth = 0i32;
            let mut k = i;
            while k > 0 {
                k -= 1;
                match bytes[k] {
                    b')' => depth += 1,
                    b'(' => {
                        if depth == 0 {
                            return false;
                        }
                        depth -= 1;
                    }
                    _ => {}
                }
            }
            true
        }
        _ => false,
    }
}

/// `from` から、括弧の外にある最初の `;` の位置。
fn statement_end(bytes: &[u8], from: usize) -> Option<usize> {
    let mut depth = 0i32;
    for (i, &b) in bytes.iter().enumerate().skip(from) {
        match b {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => {
                if depth == 0 {
                    return None;
                }
                depth -= 1;
            }
            b';' if depth == 0 => return Some(i),
            _ => {}
        }
    }
    None
}

/// `vec4(,33,11,)` の空いた引数を 0 で埋める。
fn fill_empty_arguments(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < bytes.len() {
        if is_ident_start(bytes[i]) && (i == 0 || !is_word(bytes[i - 1]) && bytes[i - 1] != b'.') {
            let start = i;
            while i < bytes.len() && is_word(bytes[i]) {
                i += 1;
            }
            let word = &text[start..i];
            let zero = match word {
                "vec2" | "vec3" | "vec4" => Some("0.0"),
                "ivec2" | "ivec3" | "ivec4" | "uvec2" | "uvec3" | "uvec4" => Some("0"),
                "bvec2" | "bvec3" | "bvec4" => Some("false"),
                _ => None,
            };
            if let Some(zero) = zero
                && bytes.get(i) == Some(&b'(')
                && let Some(close) = matching(bytes, i)
            {
                let inner = &text[i + 1..close];
                let parts = split_top_level(inner);
                if parts.len() > 1 && parts.iter().any(|p| p.trim().is_empty()) {
                    let filled: Vec<String> = parts
                        .iter()
                        .map(|p| if p.trim().is_empty() { zero.to_string() } else { p.trim().to_string() })
                        .collect();
                    out.push_str(word);
                    out.push('(');
                    out.push_str(&fill_empty_arguments(&filled.join(", ")));
                    out.push(')');
                    i = close + 1;
                    continue;
                }
            }
            out.push_str(word);
            continue;
        }
        let ch = text[i..].chars().next().unwrap_or(' ');
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

/// `vec3 p = expr;` の右辺を `vec3(expr)` で包む。`f3 p = 0` を通すため。
///
/// 右辺が既にその型の構築子なら触らない。ベクトルをベクトルで包むのは GLSL
/// では無害なので、それ以外は型を見ずに包む。
fn wrap_vector_initializers(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < bytes.len() {
        if is_ident_start(bytes[i]) && (i == 0 || !is_word(bytes[i - 1]) && bytes[i - 1] != b'.') {
            let start = i;
            while i < bytes.len() && is_word(bytes[i]) {
                i += 1;
            }
            let word = &text[start..i];
            let vector = matches!(
                word,
                "vec2" | "vec3" | "vec4" | "ivec2" | "ivec3" | "ivec4" | "uvec2" | "uvec3" | "uvec4"
                    | "bvec2" | "bvec3" | "bvec4" | "mat2" | "mat3" | "mat4"
            );
            if vector && let Some(decl) = declaration_with_initializer(bytes, i) {
                let init = text[decl.init_start..decl.end].trim();
                let already = init.starts_with(word)
                    && init[word.len()..].trim_start().starts_with('(')
                    && init.ends_with(')')
                    && matching(init.as_bytes(), init.find('(').unwrap_or(0)) == Some(init.len() - 1);
                if already {
                    out.push_str(&text[start..decl.end]);
                } else {
                    out.push_str(&text[start..decl.init_start]);
                    out.push(' ');
                    out.push_str(word);
                    out.push('(');
                    out.push_str(init);
                    out.push(')');
                }
                i = decl.end;
                continue;
            }
            out.push_str(word);
            continue;
        }
        let ch = text[i..].chars().next().unwrap_or(' ');
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

struct Declaration {
    /// `=` の直後。
    init_start: usize,
    /// 文を閉じる `;` の位置。
    end: usize,
}

/// `at` (型名の直後) から ` name = init;` の形を読む。
fn declaration_with_initializer(bytes: &[u8], at: usize) -> Option<Declaration> {
    let mut i = at;
    if !bytes.get(i).is_some_and(u8::is_ascii_whitespace) {
        return None;
    }
    while i < bytes.len() && bytes[i].is_ascii_whitespace() {
        i += 1;
    }
    if !bytes.get(i).is_some_and(|b| is_ident_start(*b)) {
        return None;
    }
    while i < bytes.len() && is_word(bytes[i]) {
        i += 1;
    }
    while i < bytes.len() && (bytes[i] == b' ' || bytes[i] == b'\t') {
        i += 1;
    }
    if bytes.get(i) != Some(&b'=') || bytes.get(i + 1) == Some(&b'=') {
        return None;
    }
    let init_start = i + 1;
    let end = statement_end(bytes, init_start)?;
    if bytes[init_start..end].iter().all(u8::is_ascii_whitespace) {
        return None;
    }
    Some(Declaration { init_start, end })
}

/// `o = 0.5;` のようなスカラー代入を `vec4` にする。
fn widen_scalar_output(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'o'
            && (i == 0 || !is_word(bytes[i - 1]) && bytes[i - 1] != b'.')
            && !bytes.get(i + 1).is_some_and(|b| is_word(*b))
        {
            let mut k = i + 1;
            while k < bytes.len() && (bytes[k] == b' ' || bytes[k] == b'\t') {
                k += 1;
            }
            if bytes.get(k) == Some(&b'=') && bytes.get(k + 1) != Some(&b'=') {
                let mut v = k + 1;
                while v < bytes.len() && (bytes[v] == b' ' || bytes[v] == b'\t') {
                    v += 1;
                }
                let value_start = v;
                if v < bytes.len() && (bytes[v] == b'-' || bytes[v] == b'+') {
                    v += 1;
                }
                let digits_start = v;
                while v < bytes.len() && (bytes[v].is_ascii_digit() || bytes[v] == b'.') {
                    v += 1;
                }
                if v < bytes.len() && (bytes[v] == b'e' || bytes[v] == b'E') {
                    v += 1;
                    if v < bytes.len() && (bytes[v] == b'-' || bytes[v] == b'+') {
                        v += 1;
                    }
                    while v < bytes.len() && bytes[v].is_ascii_digit() {
                        v += 1;
                    }
                }
                let mut s = v;
                while s < bytes.len() && (bytes[s] == b' ' || bytes[s] == b'\t') {
                    s += 1;
                }
                if v > digits_start
                    && bytes[digits_start..v].iter().any(u8::is_ascii_digit)
                    && bytes.get(s) == Some(&b';')
                {
                    out.push_str("o = vec4(");
                    out.push_str(&text[value_start..v]);
                    out.push(')');
                    i = v;
                    continue;
                }
            }
        }
        let ch = text[i..].chars().next().unwrap_or(' ');
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

/// `HPI` を使っていれば定義を足す。`PI` と `TAU` は前置きにある。
///
/// 行数を変えないよう 1 行目の頭に置く。
fn define_hpi(text: &str) -> String {
    if !contains_word(text, "HPI") || text.contains("#define HPI") {
        return text.to_string();
    }
    format!("const float HPI = 1.57079632679; {text}")
}

/// トップレベルの関数定義を前へ出す。
///
/// [`crate::shader::compile`] は `void main` が無いソース全体を `main` で包む。
/// GOLF は関数定義と文を並べて書けるので、関数だけ先に出し、残りを `main` に
/// する。関数が無ければ何もしない (行数を保つ)。
fn hoist_functions(text: &str) -> String {
    let functions = top_level_functions(text);
    if functions.is_empty() {
        return text.to_string();
    }
    let mut hoisted = String::new();
    let mut body = String::new();
    let mut cursor = 0;
    for range in &functions {
        body.push_str(&text[cursor..range.start]);
        hoisted.push_str(text[range.clone()].trim());
        hoisted.push('\n');
        cursor = range.end;
    }
    body.push_str(&text[cursor..]);
    let body = body.trim();
    if body.is_empty() {
        return hoisted;
    }
    format!("{hoisted}void main() {{\n{body}\n}}\n")
}

/// `type name(...) { ... }` の範囲。ブロックの外にあるものだけ。
fn top_level_functions(text: &str) -> Vec<std::ops::Range<usize>> {
    let bytes = text.as_bytes();
    let mut found = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'{' {
            i = matching(bytes, i).map_or(bytes.len(), |c| c + 1);
            continue;
        }
        if is_ident_start(bytes[i]) && (i == 0 || !is_word(bytes[i - 1]) && bytes[i - 1] != b'.') {
            let start = i;
            while i < bytes.len() && is_word(bytes[i]) {
                i += 1;
            }
            let word = &text[start..i];
            if GLSL_TYPES.contains(&word)
                && let Some(range) = function_definition(bytes, start, i)
            {
                i = range.end;
                found.push(range);
                continue;
            }
            continue;
        }
        i += 1;
    }
    found
}

/// `type` の直後から ` name ( ... ) { ... }` を読む。`main` は除く。
fn function_definition(bytes: &[u8], start: usize, after_type: usize) -> Option<std::ops::Range<usize>> {
    let mut i = after_type;
    if !bytes.get(i).is_some_and(u8::is_ascii_whitespace) {
        return None;
    }
    while i < bytes.len() && bytes[i].is_ascii_whitespace() {
        i += 1;
    }
    let name_start = i;
    while i < bytes.len() && is_word(bytes[i]) {
        i += 1;
    }
    if i == name_start || &bytes[name_start..i] == b"main" {
        return None;
    }
    while i < bytes.len() && bytes[i].is_ascii_whitespace() {
        i += 1;
    }
    if bytes.get(i) != Some(&b'(') {
        return None;
    }
    i = matching(bytes, i)? + 1;
    while i < bytes.len() && bytes[i].is_ascii_whitespace() {
        i += 1;
    }
    if bytes.get(i) != Some(&b'{') {
        return None;
    }
    let close = matching(bytes, i)?;
    Some(start..close + 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 2026-08 の XorDev 氏の投稿 "Fever"。題名の行から始まる。
    const FEVER: &str = "Fever\nf z,d\n@(70)\n{\nf3 p = z * nor(2*C.rgb - R.xyy)\np.xy *= mat2(cos(z*.5+f4(,33,11,)))\np.z-=T;\nd=2; @(5) d+=d,\np += sin(p.yzx*d+z) / d\nz += min(abs(cos(p.y)),d=len(1/tan(p.xz)))/4;\nO += f4(1.1+sin(p),)/d\n}\nO = tanh(O / 2e2)\n";

    #[test]
    fn fever_expands_to_glsl_line_for_line() {
        let glsl = to_glsl(FEVER);
        let lines: Vec<&str> = glsl.lines().collect();
        assert_eq!(lines.len(), FEVER.lines().count(), "行数を変えない\n{glsl}");
        assert_eq!(lines[0].trim(), "", "題名は消える");
        assert_eq!(lines[1], "float z; float d;");
        assert_eq!(lines[2], "for(int _fc=0;_fc<70;_fc++)");
        assert_eq!(lines[4], "vec3 p = vec3(z * normalize(2*FC.rgb - vec3(r, 1.0).xyy));");
        assert_eq!(lines[5], "p.xy *= mat2(cos(z*.5+vec4(0.0, 33, 11, 0.0)));");
        assert_eq!(lines[6], "p.z-=t;");
        assert_eq!(lines[7], "d=2; for(int _fc1=0;_fc1<5;_fc1++) d+=d,");
        assert_eq!(lines[8], "p += sin(p.yzx*d+z) / d;");
        assert_eq!(lines[9], "z += min(abs(cos(p.y)),d=length(1/tan(p.xz)))/4;");
        assert_eq!(lines[10], "o += vec4(1.1+sin(p), 0.0)/d;");
        assert_eq!(lines[12], "o = tanh(o / 2e2);");
    }

    #[test]
    fn fever_compiles_to_wgsl() {
        let glsl = to_glsl(FEVER);
        let wgsl = crate::shader::compile(&glsl).unwrap_or_else(|e| panic!("{e:?}\n{glsl}"));
        assert!(wgsl.contains("@fragment"));
    }

    #[test]
    fn the_documented_example_compiles() {
        let source = "f2 uv = C.xy / R.xy\nf3 col = f3(uv, sin(T * 2.))\nO = f4(mix(f3(0.), col, 0.5), 1.)";
        let glsl = to_glsl(source);
        crate::shader::compile(&glsl).unwrap_or_else(|e| panic!("{e:?}\n{glsl}"));
    }

    #[test]
    fn loops_take_a_name_and_a_start() {
        assert_eq!(to_glsl("@(i, 9) {}"), "for(int i=0;i<9;i++) {}");
        assert_eq!(to_glsl("@(i, 1, 9) {}"), "for(int i=1;i<9;i++) {}");
        // 入れ子は別の名前になる。
        let glsl = to_glsl("@(3) { @(4) { O += 1. } }");
        assert!(glsl.contains("_fc=0") && glsl.contains("_fc1=0"), "{glsl}");
    }

    #[test]
    fn a_taken_loop_name_is_skipped() {
        let glsl = to_glsl("f _fc = 1.\n@(3) O += _fc");
        assert!(glsl.contains("for(int _fc1=0;_fc1<3;_fc1++)"), "{glsl}");
    }

    #[test]
    fn power_and_complement() {
        assert_eq!(to_glsl("O = a ** 2."), "o = pow(a, 2.);");
        assert_eq!(to_glsl("O = g(x).y ** (b + 1.)"), "o = pow(g(x).y, (b + 1.));");
        assert_eq!(to_glsl("O = ~x"), "o = (1.0-(x));");
        assert_eq!(to_glsl("O = ~len(p).x"), "o = (1.0-(length(p).x));");
    }

    #[test]
    fn semicolons_respect_control_flow_and_braces() {
        let glsl = to_glsl("if (a > 1.)\n{\nO = 1.\n}\nelse\nO = 2.");
        assert_eq!(glsl, "if (a > 1.)\n{\no = vec4(1.);\n}\nelse\no = vec4(2.);");
        assert_eq!(to_glsl("{ O = 1. }"), "{ o = vec4(1.); }");
        assert_eq!(to_glsl("O = f4(1.,\n2., 3., 4.)"), "o = vec4(1.,\n2., 3., 4.);");
    }

    #[test]
    fn directives_and_one_line_functions() {
        assert_eq!(to_glsl("#D Z 9"), "#define Z 9");
        let glsl = to_glsl("sq x = x * x\nO = f4(sq(2.))");
        assert!(glsl.starts_with("float sq(float x) { return x * x; }"), "{glsl}");
        assert!(glsl.contains("void main()"), "関数があれば main で包む\n{glsl}");
        crate::shader::compile(&glsl).unwrap_or_else(|e| panic!("{e:?}\n{glsl}"));
    }

    #[test]
    fn a_function_body_is_hoisted_ahead_of_the_statements() {
        let source = "f sdf(f3 p) { return len(p) - 1. }\nO = f4(sdf(f3(C.xy / R.xy, 0.)))";
        let glsl = to_glsl(source);
        assert!(glsl.starts_with("float sdf(vec3 p) { return length(p) - 1.; }"), "{glsl}");
        crate::shader::compile(&glsl).unwrap_or_else(|e| panic!("{e:?}\n{glsl}"));
    }

    #[test]
    fn a_scalar_initializer_becomes_a_vector() {
        let glsl = to_glsl("f3 p = 0\nf2 q = C.xy, w = 1.\nO = f4(p, 1.) + q.x + w.x");
        assert!(glsl.contains("vec3 p = vec3(0);"), "{glsl}");
        assert!(glsl.contains("vec2 q = vec2(FC.xy); vec2 w = vec2(1.);"), "{glsl}");
        crate::shader::compile(&glsl).unwrap_or_else(|e| panic!("{e:?}\n{glsl}"));
    }

    #[test]
    fn a_title_only_goes_when_it_is_not_code() {
        assert_eq!(to_glsl("Neon Rain\nO = C"), "\no = FC;");
        assert_eq!(to_glsl("discard\nO = C"), "discard;\no = FC;");
        assert_eq!(to_glsl("f z\nO = C"), "float z;\no = FC;");
    }

    /// XorDev 氏の作品は `t` や `r` をローカルに使う。時間の `t` と衝突させない。
    #[test]
    fn a_local_named_like_a_twigl_uniform_is_moved_aside() {
        let glsl = to_glsl("f3 t = C.rgb, r = t\nO = f4(t + r, T) * len(t.xy)");
        assert_eq!(glsl, "vec3 _t = vec3(FC.rgb); vec3 _r = vec3(_t);\no = vec4(_t + _r, t) * length(_t.xy);");
        crate::shader::compile(&glsl).unwrap_or_else(|e| panic!("{e:?}\n{glsl}"));
    }

    #[test]
    fn member_names_and_numbers_are_left_alone() {
        assert_eq!(to_glsl("O = p.f + 1e5"), "o = p.f + 1e5;");
        assert_eq!(to_glsl("O.T = 1."), "o.T = 1.;");
    }

    #[test]
    fn hpi_is_defined_when_used() {
        let glsl = to_glsl("O = f4(HPI)");
        assert!(glsl.starts_with("const float HPI"), "{glsl}");
        crate::shader::compile(&glsl).unwrap_or_else(|e| panic!("{e:?}\n{glsl}"));
    }

    #[test]
    fn broken_input_does_not_panic() {
        for source in ["", "@(", "**", "~", "f4(", "{", "日本語だけ", "#", "@(1,2,3,4)", "a ** ", "x ="] {
            let _ = to_glsl(source);
        }
    }
}
