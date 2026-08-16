//! 貼られたコードの方言を見分け、対応していない書き方を挙げる。
//!
//! Processing (Java Mode) と p5.js のどちらで書かれているかを当てて、その方言で
//! **まだ対応していないもの**だけを並べる。エラー位置だけを見せられても直し
//! ようがないので、原因をまとめて示すのが目的。
//!
//! コンパイルに失敗したときだけ呼ぶ。判定は当て推量なので、通ったコードに
//! 口を出さない。

use crate::highlight::{TokenClass, tokens};

/// 何で書かれたコードに見えるか。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dialect {
    /// p5.js。アロー関数や `$`、型なしの代入などが手がかり。
    P5,
    /// Processing (Java Mode)。ただし未対応の構文を使っている。
    Processing,
    /// つぶやき GLSL。フラグメントシェーダー 1 本。
    Glsl,
}

impl Dialect {
    /// 画面に出す名前。固有名詞なので翻訳しない。
    pub fn label(self) -> &'static str {
        match self {
            Dialect::P5 => "p5.js",
            Dialect::Processing => "Processing",
            Dialect::Glsl => "GLSL",
        }
    }

    /// 見出しの翻訳キー。
    pub fn locale_key(self) -> &'static str {
        match self {
            Dialect::P5 => "dialect.p5",
            Dialect::Processing => "dialect.processing",
            Dialect::Glsl => "dialect.glsl",
        }
    }
}

/// GLSL にしか出てこない語。1 つでもあれば GLSL と見てよい。
///
/// Processing にも p5.js にも同じ名前の型や変数は無い。`float` や `void` の
/// ように両方にある語は入れない。
const GLSL_ONLY: &[&str] = &[
    "vec2",
    "vec3",
    "vec4",
    "ivec2",
    "ivec3",
    "ivec4",
    "bvec2",
    "bvec3",
    "bvec4",
    "mat2",
    "mat3",
    "mat4",
    "sampler2D",
    "gl_FragCoord",
    "gl_FragColor",
    "gl_Position",
    "rotate2D",
    "rotate3D",
    "snoise2D",
    "snoise3D",
    "fwidth",
];

/// つぶやき GLSL として読むべきコードか。
///
/// コンパイルを試す前に呼ぶ。GLSL は Processing のパーサに掛けても 1 行目で
/// 転ぶだけで、どこが悪いのかを言えない。先に見分けて別の道へ送る。
pub fn looks_like_glsl(source: &str) -> bool {
    tokens(source)
        .iter()
        .filter(|s| !matches!(s.class, TokenClass::Plain | TokenClass::Comment))
        .any(|s| GLSL_ONLY.contains(&&source[s.start..s.end]))
}

/// p5.js で、まだ持っていない API。
const UNSUPPORTED_P5_API: &[&str] = &[
    "get",
    "set",
    "image",
    "loadImage",
    "createImage",
    "pixels",
    "loadPixels",
    "updatePixels",
    "strokeCap",
    "strokeJoin",
    "redraw",
    "frameRate",
    "save",
    "saveFrame",
    "shader",
    "createShader",
];

/// 3D のうち、まだ持っていないもの。
///
/// `box()` / `sphere()` と既定のカメラは動く。視点を自分で組む API と、
/// 立方体と球以外の立体はまだ。
const UNSUPPORTED_3D: &[&str] = &[
    "camera",
    "perspective",
    "ortho",
    "frustum",
    "cylinder",
    "cone",
    "torus",
    "plane",
    "texture",
    "createShape",
    "shininess",
    "specular",
];

/// 受け付けるが、まだ効かない API。
const IGNORED_API: &[&str] = &["blendMode", "drawingContext"];

/// 見つかった未対応の構文 1 件。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Finding {
    /// 翻訳キー。表示文字列は UI 層が引く。
    pub key: &'static str,
    /// 1 始まりの行。
    pub line: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnosis {
    pub dialect: Dialect,
    /// 見つかった順。同じ種類は最初の 1 件だけ。
    pub findings: Vec<Finding>,
}

/// p5.js らしさの手がかり。これ自体は対応済みなので、指摘はしない。
const P5_HINT_API: &[&str] = &["createCanvas", "colorMode", "blendMode", "noLoop", "frameRate"];

/// p5.js で書かれていることを示すキーワード。対応済み。
const JS_KEYWORDS: &[&str] = &["let", "const", "var", "function"];

/// JavaScript にはあるが、まだ対応していないキーワード。
/// p5 側にしか出てこない語。見つかったら p5 と判定してよい。
///
/// `class` と `new` はここに入れていない。Java Mode では使えるので、挙げると
/// 動くコードに「使えません」と言うことになる。p5 で使ったときは、コンパイル
/// エラーが場所を教える。
const UNSUPPORTED_JS_KEYWORDS: &[&str] = &["in", "await", "async", "yield"];


/// Java Mode にはあるが、このランタイムがまだ持っていない語。
///
/// 対応が進むたびにここから外す。挙げ続けると、動くコードを「使えません」と
/// 言ってしまう。
const UNSUPPORTED_JAVA_KEYWORDS: &[&str] = &["static", "import", "char"];

/// コンパイルできなかったコードを見て、方言と未対応の構文を挙げる。
pub fn diagnose(source: &str) -> Diagnosis {
    // GLSL は Processing でも p5.js でもない。未対応の構文を挙げても
    // 「Processing にこの書き方は無い」ばかりになるので、方言だけ返す。
    if looks_like_glsl(source) {
        return Diagnosis { dialect: Dialect::Glsl, findings: Vec::new() };
    }

    let spans = tokens(source);
    let code: Vec<(usize, &str, TokenClass)> = spans
        .iter()
        .enumerate()
        .filter(|(_, s)| !matches!(s.class, TokenClass::Plain | TokenClass::Comment))
        .map(|(i, s)| (i, &source[s.start..s.end], s.class))
        .collect();

    let line_of = LineIndex::new(source);
    let mut found: Vec<Finding> = Vec::new();
    let mut add = |key: &'static str, byte: usize| {
        if !found.iter().any(|f| f.key == key) {
            found.push(Finding { key, line: line_of.line(byte) });
        }
    };

    // 開いている `{` の種類。ブロックかオブジェクトリテラルか。
    let mut braces: Vec<Brace> = Vec::new();
    // p5.js らしさの手がかりを見つけたか。findings とは別に数える。
    let mut looks_like_p5 = false;

    for (position, &(span_index, text, class)) in code.iter().enumerate() {
        let byte = spans[span_index].start;
        let previous = position.checked_sub(1).map(|p| code[p].1);
        let next = code.get(position + 1).map(|c| c.1);

        match text {
            "{" => {
                // `= {` や `, {` はオブジェクトリテラル。Java Mode の `{` は
                // ブロックだけなので、p5 らしさの手がかりになる。
                let kind = if matches!(previous, Some("=" | "," | "(" | ":" | "?")) {
                    looks_like_p5 = true;
                    Brace::ObjectLiteral
                } else {
                    Brace::Block
                };
                braces.push(kind);
            }
            "}" => {
                braces.pop();
            }
            _ => {}
        }
        let brace_depth = braces.len() as i32;

        // ---- どちらの方言かの手がかり (対応済みなので指摘しない) ----
        if (text == "=" && next == Some(">")) || text == "⇒" || text == "→" {
            looks_like_p5 = true;
        }
        if text == "$" || (class == TokenClass::Ident && JS_KEYWORDS.contains(&text)) {
            looks_like_p5 = true;
        }
        if P5_HINT_API.contains(&text) {
            looks_like_p5 = true;
        }
        // `p.x` や `$.map`。数値の小数点は 1 トークンなので混ざらない。
        if text == "." && previous.is_some_and(ends_with_value) {
            looks_like_p5 = true;
        }
        // トップレベルで型を書かずに代入している。
        if brace_depth == 0
            && class == TokenClass::Ident
            && next == Some("=")
            && matches!(previous, None | Some(";" | "}"))
        {
            looks_like_p5 = true;
        }

        // ---- 対応していないもの ----
        // 語の種類は問わない。`new` のように Java 側だけキーワードとして
        // 色分けされる語もあり、Ident に限ると取りこぼす。
        let word = matches!(class, TokenClass::Ident | TokenClass::Keyword | TokenClass::Type);
        if word && UNSUPPORTED_JS_KEYWORDS.contains(&text) {
            add("dialect.unsupported_js_keyword", byte);
            looks_like_p5 = true;
        }
        if word && UNSUPPORTED_JAVA_KEYWORDS.contains(&text) {
            add("dialect.unsupported_keyword", byte);
        }
        if UNSUPPORTED_P5_API.contains(&text) {
            add("dialect.unsupported_api", byte);
        }
        if word && UNSUPPORTED_3D.contains(&text) {
            add("dialect.no_3d", byte);
        }
        if IGNORED_API.contains(&text) {
            add("dialect.ignored_api", byte);
            looks_like_p5 = true;
        }
    }

    let dialect = if looks_like_p5 { Dialect::P5 } else { Dialect::Processing };

    // Java Mode でしか問題にならないものは、p5 と見たときには挙げない。
    if dialect == Dialect::P5 {
        found.retain(|f| f.key != "dialect.unsupported_keyword");
    } else {
        found.sort_by_key(|f| f.line);
    }

    Diagnosis { dialect, findings: found }
}



/// 開いている `{` が何を囲んでいるか。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Brace {
    Block,
    ObjectLiteral,
}

fn ends_with_value(text: &str) -> bool {
    text.ends_with(|c: char| c.is_alphanumeric() || c == '_') || matches!(text, ")" | "]")
}

/// バイト位置から行番号を引く。
struct LineIndex {
    /// 各行の開始バイト位置。
    starts: Vec<usize>,
}

impl LineIndex {
    fn new(source: &str) -> Self {
        let mut starts = vec![0];
        for (index, byte) in source.bytes().enumerate() {
            if byte == b'\n' {
                starts.push(index + 1);
            }
        }
        Self { starts }
    }

    fn line(&self, byte: usize) -> u32 {
        match self.starts.binary_search(&byte) {
            Ok(index) => index as u32 + 1,
            Err(index) => index as u32,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keys(source: &str) -> Vec<&'static str> {
        diagnose(source).findings.into_iter().map(|f| f.key).collect()
    }

    /// 画面に貼られた、実際の p5.js のつぶやき作品。いまは動く。
    const P5_SAMPLE: &str = r#"t=0
$=[]
draw=_⇒{t?colorMode(HSB):createCanvas(W=720,W)
B=blendMode
B(BLEND)
background(0,.03)
B(ADD)
for(i=2;i--;)$[t++%W]={x:t*1.5%W,y:t*4%W,s:25,c:t%360}
$.map(p⇒fill(p.c,90,W,.1)+circle(p.x+=cos(A=noise(p.x/180,p.y/180,t/W/W)*99),p.y+=sin(A),p.s*=.99))}"#;

    #[test]
    fn the_pasted_sketch_is_recognised_as_p5() {
        assert_eq!(diagnose(P5_SAMPLE).dialect, Dialect::P5);
    }

    #[test]
    fn supported_p5_syntax_is_not_reported() {
        // アロー関数・配列・オブジェクト・型なし変数はどれも動く。文句を言わない。
        let keys = keys(P5_SAMPLE);
        for gone in [
            "dialect.arrow_function",
            "dialect.object_literal",
            "dialect.member_access",
            "dialect.untyped_variable",
            "dialect.missing_semicolon",
            "dialect.dollar_name",
        ] {
            assert!(!keys.contains(&gone), "{gone} をまだ指摘している: {keys:?}");
        }
    }

    #[test]
    fn an_api_that_only_gets_accepted_is_flagged() {
        // blendMode は受けるが効かない。黙って無視すると絵が違う理由が分からない。
        assert!(keys(P5_SAMPLE).contains(&"dialect.ignored_api"), "{:?}", keys(P5_SAMPLE));
    }

    #[test]
    fn p5_apis_we_do_not_have_are_listed() {
        let keys = keys("draw=_=>{image(1,2,3)}");
        assert!(keys.contains(&"dialect.unsupported_api"), "{keys:?}");
    }

    #[test]
    fn javascript_features_we_do_not_have_are_listed() {
        assert!(keys("draw=_⇒{await f()}").contains(&"dialect.unsupported_js_keyword"));
        assert!(keys("draw=_⇒{for(k in o){}}").contains(&"dialect.unsupported_js_keyword"));
    }

    #[test]
    fn unsupported_java_syntax_is_reported_as_processing() {
        let diagnosis = diagnose("void draw() {\n  static int a = 1;\n}");
        assert_eq!(diagnosis.dialect, Dialect::Processing);
        let keys: Vec<&str> = diagnosis.findings.iter().map(|f| f.key).collect();
        assert!(keys.contains(&"dialect.unsupported_keyword"), "{keys:?}");
    }

    /// p5 側も、対応した書き方に文句を言わない。
    #[test]
    fn supported_p5_syntax_is_not_reported_either() {
        for (name, src) in [
            ("for...of", "draw=_=>{for(v of [1,2])circle(v,0,1)}"),
            ("スプレッド", "draw=_=>{[...Array(9)].map((_,i)=>circle(i,0,1))}"),
            ("分割代入", "draw=_=>{[a,b]=[1,2];circle(a,b,1)}"),
            ("break", "draw=_=>{for(i=0;i<9;i++){if(i>2)break;circle(i,0,1)}}"),
            ("ビット演算", "draw=_=>{circle(frameCount&255,0,1)}"),
            ("beginShape", "draw=_=>{beginShape();vertex(1,1);vertex(2,2);endShape(CLOSE)}"),
            ("arc / bezier", "draw=_=>{arc(1,1,2,2,0,3);bezier(0,0,1,1,2,0,3,1)}"),
            ("color", "draw=_=>{fill(lerpColor(color(255,0,0),color(0,0,255),.5))}"),
            ("Math", "draw=_=>{circle(Math.hypot(3,4),Math.PI,1)}"),
            ("quad / square", "draw=_=>{quad(0,0,1,0,1,1,0,1);square(0,0,1)}"),
            ("curve", "draw=_=>{curve(0,0,1,1,2,1,3,0)}"),
            ("モード指定", "draw=_=>{rectMode(CENTER);ellipseMode(CORNER);angleMode(DEGREES)}"),
            ("noLoop", "draw=_=>{noLoop()}"),
            ("createVector", "draw=_=>{v=createVector(1,2);circle(v.x,v.y,v.mag())}"),
            ("文字列と text", "draw=_=>{textSize(20);text('あ'+1,10,20)}"),
            ("テンプレート", "draw=_=>{text(`n=${1+2}`,10,20)}"),
        ] {
            let keys = keys(src);
            for stale in ["dialect.unsupported_js_keyword", "dialect.unsupported_api"] {
                assert!(!keys.contains(&stale), "{name}: {stale} が挙がっています ({keys:?})");
            }
        }
    }

    /// 対応した書き方に文句を言わない。
    ///
    /// 診断が実装より古いままだと、動くコードを「使えません」と言ってしまう。
    #[test]
    fn things_we_now_support_are_not_reported() {
        for (name, src) in [
            ("配列", "float[] x = new float[9];\nvoid draw() { circle(x[0], 0, 1); }"),
            ("キャスト", "void draw() { int i = (int) 1.5; }"),
            ("break", "void draw() { for (int i = 0; i < 9; i++) { break; } }"),
            ("continue", "void draw() { for (int i = 0; i < 9; i++) { continue; } }"),
            ("ビット演算", "void draw() { int i = 3 & 1 | 2 ^ 4; }"),
            ("拡張 for", "int[] a = {1};\nvoid draw() { for (int v : a) circle(v, 0, 1); }"),
            (
                "switch",
                "void draw() { switch (frameCount) { case 0: break; default: circle(0, 0, 1); } }",
            ),
            (
                "class",
                "class P { float x; P(float a) { x = a; } }\nvoid draw() { circle(new P(1).x, 0, 1); }",
            ),
            (
                "複数宣言",
                "float a = 1, b;\nvoid draw() { circle(a, b, 1); }",
            ),
            (
                "静的モード",
                "size(400, 400);\ncircle(1, 2, 3);",
            ),
            (
                "3D",
                "void setup() { size(400, 400, P3D); }\n\
                 void draw() { lights(); rotateX(1); box(9); sphere(4); }",
            ),
            (
                "WEBGL",
                "function setup() { createCanvas(400, 400, WEBGL); }\n\
                 function draw() { rotateY(1); box(9); }",
            ),
            (
                "2 次元配列",
                "float[][] g = new float[2][2];\nvoid draw() { circle(g[0][0], 0, 1); }",
            ),
            (
                "PVector",
                "PVector[] p = new PVector[2];\nvoid draw() { p[0].add(new PVector(1, 2)); circle(p[0].x, p[0].y, 1); }",
            ),
        ] {
            let keys = keys(src);
            for stale in [
                "dialect.array",
                "dialect.cast",
                "dialect.unsupported_keyword",
                "dialect.unsupported_js_keyword",
            ] {
                assert!(!keys.contains(&stale), "{name}: {stale} が挙がっています ({keys:?})");
            }
        }
    }

    #[test]
    fn java_only_complaints_do_not_appear_for_p5_code() {
        // p5 では `[]` も `,` も普通の書き方。
        let keys = keys("draw=_=>{a=[1,2];b={x:1,y:2}}");
        assert!(!keys.contains(&"dialect.untyped_variable"), "{keys:?}");
    }

    #[test]
    fn findings_point_at_the_right_line() {
        let diagnosis = diagnose("draw=_=>{\n  circle(1,2,3)\n  image(1,2,3)\n}");
        let finding = diagnosis
            .findings
            .iter()
            .find(|f| f.key == "dialect.unsupported_api")
            .expect("image() を見つけている");
        assert_eq!(finding.line, 3);
    }

    #[test]
    fn each_kind_is_reported_only_once() {
        let mut keys = keys("draw=_=>{text(1);textSize(2);text(3)}");
        let count = keys.len();
        keys.sort_unstable();
        keys.dedup();
        assert_eq!(keys.len(), count, "同じ種類を何度も出している");
    }

    #[test]
    fn a_working_sketch_reports_nothing() {
        for source in [
            include_str!("../sketches/spiral.pde"),
            include_str!("../sketches/noise-field.pde"),
            include_str!("../sketches/moire.pde"),
        ] {
            assert!(diagnose(source).findings.is_empty(), "動く作品に文句を付けている");
        }
    }

    /// 実際に貼られたつぶやき GLSL。Processing でも p5.js でもない。
    #[test]
    fn a_tweet_sized_shader_is_recognised_as_glsl() {
        for source in [
            "float e, i, a, w, x, g;\nfor (; i++< 1e2;) {\n  vec3 p = vec3((FC.xy - .5 * r) / r.y * g, g - 3.);\n  p.zy *= rotate2D(.6);\n}",
            "void main() {\n  vec3 d = vec3(gl_FragCoord.xy / r - .5, .8);\n  gl_FragColor = vec4(d, 1);\n}",
            "o += vec4(snoise3D(vec3(FC.xy, t)));",
        ] {
            assert!(looks_like_glsl(source), "GLSL と見ていない: {source}");
            assert_eq!(diagnose(source).dialect, Dialect::Glsl);
        }
    }

    /// Processing と p5.js を GLSL と取り違えない。
    ///
    /// `float` や `void` は両方にある語なので手がかりにしていない。
    #[test]
    fn processing_and_p5_are_not_mistaken_for_glsl() {
        for source in [
            P5_SAMPLE,
            "float x = 1;\nvoid draw() { circle(x, 0, 1); }",
            "void setup() { size(400, 400, P3D); }\nvoid draw() { box(9); }",
            include_str!("../sketches/spiral.pde"),
            include_str!("../sketches/noise-field.pde"),
        ] {
            assert!(!looks_like_glsl(source), "GLSL と間違えている: {source}");
        }
    }

    /// GLSL には未対応の構文を並べない。
    ///
    /// 挙げても「Processing にこの書き方は無い」ばかりになり、直す先を指せない。
    #[test]
    fn a_shader_gets_no_processing_findings() {
        let diagnosis = diagnose("vec3 p = vec3(FC.xy, 1);\nfor (int i = 0; i < 9; i++) p *= 2.;\no.rgb = p;");
        assert_eq!(diagnosis.dialect, Dialect::Glsl);
        assert!(diagnosis.findings.is_empty(), "{:?}", diagnosis.findings);
    }

    #[test]
    fn broken_input_does_not_panic() {
        for source in ["", "$", "⇒", "/* 閉じない", "日本語だけ"] {
            let _ = diagnose(source);
            let _ = looks_like_glsl(source);
        }
    }

    #[test]
    fn line_index_is_one_based() {
        let index = LineIndex::new("a\nbb\nccc");
        assert_eq!(index.line(0), 1);
        assert_eq!(index.line(2), 2);
        assert_eq!(index.line(5), 3);
    }
}
