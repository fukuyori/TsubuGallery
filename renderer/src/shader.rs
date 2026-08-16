//! つぶやき GLSL のフラグメントシェーダーを WGSL へ翻訳する。
//!
//! `#つぶやきGLSL` は twigl.app の「geekest」を前提に書かれる。`main()` も
//! `precision` も uniform 宣言も省き、`r` (解像度) `t` (経過秒) `FC`
//! (`gl_FragCoord`) `o` (出力色) と、`rotate2D` / `hsv` / `snoise3D` などの
//! 補助関数がある状態から書き始める。ここではその前提を [`PREAMBLE`] として
//! 与え、本文を `main()` で包んでから naga に食わせる。
//!
//! ```text
//! つぶやき GLSL → PREAMBLE + 本文 + 後書き → naga (glsl-in) → 検証 → WGSL
//! ```
//!
//! WGSL を先に作って検証まで済ませるのは、[`wgpu`] の既定のエラー処理が
//! 検証エラーでプロセスを落とすため。ここで転んだものは読み込みの時点で
//! エラーとして扱い、GPU へは渡さない。
//!
//! # 元の GLSL との違い
//!
//! - naga の GLSL フロントエンドは `#version 440/450/460` しか受けないので
//!   `#version 450` を名乗る。ES 300 相当の書き方はそのまま通る。
//! - `gl_FragCoord` は GLSL では左下原点、wgpu では左上原点。`r.y` から引いて
//!   twigl と同じ向きに直す。
//! - 本文末尾の `#つぶやきGLSL` のようなタグ行は、行数を保ったまま空行にする。
//!   プリプロセッサ指令ではないので、そのままでは通らない。

use std::ops::Range;

/// 本文の前に置く、twigl 互換の前置き。
///
/// 補助関数の名前と中身は twigl に合わせてある。`snoise*` は webgl-noise の
/// 実装で、GLSL には無いオーバーロード解決を避けるため補助関数だけ改名した。
const PREAMBLE: &str = r#"#version 450

layout(std140, set = 0, binding = 0) uniform Tsubu {
    vec2 r;
    vec2 m;
    float t;
    float f;
};

layout(location = 0) out vec4 tsubu_color;

const float PI = 3.141592653589793;
const float TAU = 6.283185307179586;

mat2 rotate2D(float a) {
    return mat2(cos(a), sin(a), -sin(a), cos(a));
}

mat3 rotate3D(float angle, vec3 axis) {
    vec3 a = normalize(axis);
    float s = sin(angle);
    float c = cos(angle);
    float k = 1.0 - c;
    return mat3(
        a.x * a.x * k + c,       a.y * a.x * k + a.z * s, a.z * a.x * k - a.y * s,
        a.x * a.y * k - a.z * s, a.y * a.y * k + c,       a.z * a.y * k + a.x * s,
        a.x * a.z * k + a.y * s, a.y * a.z * k - a.x * s, a.z * a.z * k + c
    );
}

vec3 hsv(float h, float s, float v) {
    vec4 k = vec4(1.0, 2.0 / 3.0, 1.0 / 3.0, 3.0);
    vec3 p = abs(fract(vec3(h) + k.xyz) * 6.0 - vec3(k.w));
    return v * mix(vec3(k.x), clamp(p - vec3(k.x), 0.0, 1.0), s);
}

vec2 tsubu_mod289_2(vec2 x) { return x - floor(x * (1.0 / 289.0)) * 289.0; }
vec3 tsubu_mod289_3(vec3 x) { return x - floor(x * (1.0 / 289.0)) * 289.0; }
vec4 tsubu_mod289_4(vec4 x) { return x - floor(x * (1.0 / 289.0)) * 289.0; }
vec3 tsubu_permute_3(vec3 x) { return tsubu_mod289_3(((x * 34.0) + 1.0) * x); }
vec4 tsubu_permute_4(vec4 x) { return tsubu_mod289_4(((x * 34.0) + 1.0) * x); }
vec4 tsubu_taylor_inv_sqrt(vec4 x) { return 1.79284291400159 - 0.85373472095314 * x; }

float snoise2D(vec2 v) {
    const vec4 C = vec4(0.211324865405187, 0.366025403784439, -0.577350269189626, 0.024390243902439);
    vec2 i = floor(v + dot(v, C.yy));
    vec2 x0 = v - i + dot(i, C.xx);
    vec2 i1 = (x0.x > x0.y) ? vec2(1.0, 0.0) : vec2(0.0, 1.0);
    vec4 x12 = x0.xyxy + C.xxzz;
    x12.xy -= i1;
    i = tsubu_mod289_2(i);
    vec3 p = tsubu_permute_3(tsubu_permute_3(i.y + vec3(0.0, i1.y, 1.0)) + i.x + vec3(0.0, i1.x, 1.0));
    vec3 m = max(0.5 - vec3(dot(x0, x0), dot(x12.xy, x12.xy), dot(x12.zw, x12.zw)), 0.0);
    m = m * m;
    m = m * m;
    vec3 x = 2.0 * fract(p * C.www) - 1.0;
    vec3 h = abs(x) - 0.5;
    vec3 ox = floor(x + 0.5);
    vec3 a0 = x - ox;
    m *= 1.79284291400159 - 0.85373472095314 * (a0 * a0 + h * h);
    vec3 g;
    g.x = a0.x * x0.x + h.x * x0.y;
    g.yz = a0.yz * x12.xz + h.yz * x12.yw;
    return 130.0 * dot(m, g);
}

float snoise3D(vec3 v) {
    const vec2 C = vec2(1.0 / 6.0, 1.0 / 3.0);
    const vec4 D = vec4(0.0, 0.5, 1.0, 2.0);
    vec3 i = floor(v + dot(v, C.yyy));
    vec3 x0 = v - i + dot(i, C.xxx);
    vec3 g = step(x0.yzx, x0.xyz);
    vec3 l = 1.0 - g;
    vec3 i1 = min(g.xyz, l.zxy);
    vec3 i2 = max(g.xyz, l.zxy);
    vec3 x1 = x0 - i1 + C.xxx;
    vec3 x2 = x0 - i2 + C.yyy;
    vec3 x3 = x0 - D.yyy;
    i = tsubu_mod289_3(i);
    vec4 p = tsubu_permute_4(tsubu_permute_4(tsubu_permute_4(
        i.z + vec4(0.0, i1.z, i2.z, 1.0)) +
        i.y + vec4(0.0, i1.y, i2.y, 1.0)) +
        i.x + vec4(0.0, i1.x, i2.x, 1.0));
    float n_ = 0.142857142857;
    vec3 ns = n_ * D.wyz - D.xzx;
    vec4 j = p - 49.0 * floor(p * ns.z * ns.z);
    vec4 x_ = floor(j * ns.z);
    vec4 y_ = floor(j - 7.0 * x_);
    vec4 x = x_ * ns.x + ns.yyyy;
    vec4 y = y_ * ns.x + ns.yyyy;
    vec4 h = 1.0 - abs(x) - abs(y);
    vec4 b0 = vec4(x.xy, y.xy);
    vec4 b1 = vec4(x.zw, y.zw);
    vec4 s0 = floor(b0) * 2.0 + 1.0;
    vec4 s1 = floor(b1) * 2.0 + 1.0;
    vec4 sh = -step(h, vec4(0.0));
    vec4 a0 = b0.xzyw + s0.xzyw * sh.xxyy;
    vec4 a1 = b1.xzyw + s1.xzyw * sh.zzww;
    vec3 p0 = vec3(a0.xy, h.x);
    vec3 p1 = vec3(a0.zw, h.y);
    vec3 p2 = vec3(a1.xy, h.z);
    vec3 p3 = vec3(a1.zw, h.w);
    vec4 norm = tsubu_taylor_inv_sqrt(vec4(dot(p0, p0), dot(p1, p1), dot(p2, p2), dot(p3, p3)));
    p0 *= norm.x;
    p1 *= norm.y;
    p2 *= norm.z;
    p3 *= norm.w;
    vec4 mm = max(0.6 - vec4(dot(x0, x0), dot(x1, x1), dot(x2, x2), dot(x3, x3)), 0.0);
    mm = mm * mm;
    return 42.0 * dot(mm * mm, vec4(dot(p0, x0), dot(p1, x1), dot(p2, x2), dot(p3, x3)));
}

vec4 o;
#define gl_FragCoord vec4(gl_FragCoord.x, r.y - gl_FragCoord.y, gl_FragCoord.z, gl_FragCoord.w)
#define FC gl_FragCoord
#define gl_FragColor o
"#;

/// 画面を覆う三角形 1 枚を頂点番号から組み立てる頂点シェーダー。
///
/// naga が吐くのはフラグメントシェーダーだけなので、対になる頂点側をここで
/// 足す。GLSL 側からは触れないので WGSL で直接書く。
///
/// 奥行きを 0.5 に置くのは `gl_FragCoord.z` を OpenGL と揃えるため。OpenGL は
/// クリップ空間の -1..1 を 0..1 へ写すので、平面を手前に貼ると `z` は 0.5 に
/// なる。wgpu は 0..1 をそのまま使うので、同じ 0.5 を書けば揃う。`FC.z` を
/// 使う作品があるので、ここがずれると絵が変わる。
const FULLSCREEN_VS: &str = r#"
@vertex
fn tsubu_fullscreen(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    let x = f32((i << 1u) & 2u) * 2.0 - 1.0;
    let y = f32(i & 2u) * 2.0 - 1.0;
    return vec4<f32>(x, y, 0.5, 1.0);
}
"#;

/// 頂点シェーダーの入口。
pub const VERTEX_ENTRY: &str = "tsubu_fullscreen";

/// naga が付けるフラグメントシェーダーの入口。GLSL の `main` から来る。
pub const FRAGMENT_ENTRY: &str = "main";

/// GLSL を受け付けられなかった理由。位置は元のソース上の行と列。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShaderError {
    /// 1 始まりの行。前置きの中で転んだときは 1。
    pub line: u32,
    /// 1 始まりの列。
    pub column: u32,
    pub message: String,
}

/// つぶやき GLSL 1 本を WGSL へ翻訳する。
///
/// 返る文字列には頂点シェーダー ([`VERTEX_ENTRY`]) とフラグメントシェーダー
/// ([`FRAGMENT_ENTRY`]) の両方が入っていて、そのまま
/// [`wgpu::ShaderSource::Wgsl`] に渡せる。検証済みなので GPU 側では転ばない。
pub fn compile(source: &str) -> Result<String, ShaderError> {
    let wrapped = wrap(source);

    let mut frontend = naga::front::glsl::Frontend::default();
    let options = naga::front::glsl::Options {
        stage: naga::ShaderStage::Fragment,
        defines: Default::default(),
    };
    let module = frontend.parse(&options, &wrapped.text).map_err(|errors| {
        // 出るのは最初の 1 件だけ。後続は前の失敗に引きずられたものが多い。
        let first = errors
            .errors
            .first()
            .map(|e| (e.kind.to_string(), e.meta.to_range()))
            .unwrap_or_else(|| ("GLSL を読めませんでした".into(), None));
        wrapped.locate(first.1, first.0)
    })?;

    let mut validator = naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        // 追加機能を要る書き方は受けない。GPU 側で通らないものを先に落とす。
        naga::valid::Capabilities::empty(),
    );
    let info = validator
        .validate(&module)
        .map_err(|e| wrapped.locate(e.spans().next().and_then(|(s, _)| s.to_range()), e.to_string()))?;

    let mut wgsl =
        naga::back::wgsl::write_string(&module, &info, naga::back::wgsl::WriterFlags::empty())
            .map_err(|e| wrapped.locate(None, e.to_string()))?;
    wgsl.push_str(FULLSCREEN_VS);
    Ok(wgsl)
}

/// 前置きと後書きで挟んだソース。行番号を元へ戻すための情報も持つ。
struct Wrapped {
    text: String,
    /// 本文が始まるまでの行数。エラー行からこれを引くと元のソースの行になる。
    lines_before: u32,
    /// 本文のバイト範囲。ここから外れた位置は元のソースを指していない。
    body: Range<usize>,
}

impl Wrapped {
    /// naga の位置を元のソースの行・列へ直す。
    fn locate(&self, span: Option<Range<usize>>, message: String) -> ShaderError {
        let Some(at) = span.map(|s| s.start).filter(|at| self.body.contains(at)) else {
            // 前置きや後書きで転んでいる。指せる場所が無いので先頭にする。
            return ShaderError { line: 1, column: 1, message };
        };
        let (line, column) = line_and_column(&self.text, at);
        ShaderError { line: line.saturating_sub(self.lines_before).max(1), column, message }
    }
}

/// つぶやき GLSL を、naga に渡せる 1 本の GLSL に組み立てる。
///
/// `void main` を自分で書く流儀 (twigl の geek / geeker) も混ざるので、
/// その場合は名前を変えて呼び出す側へ回す。どちらでも `o` は 0 から始まる。
fn wrap(source: &str) -> Wrapped {
    // 不透明で出す。つぶやき GLSL の `o.a` は色ではなく作業用の 4 本目で、
    // ループ回数を数えたり (`o.w++ < 9e2`) 明るさを溜めたりに使われる。
    // そのまま透明度として扱うと、絵が抜けたり真っ白になったりする。
    let body = strip_hashtags(source);
    let (prefix, suffix) = if declares_main(&body) {
        (
            PREAMBLE.to_string(),
            "\nvoid main() { o = vec4(0.0); tsubu_user_main(); tsubu_color = vec4(o.rgb, 1.0); }\n"
                .to_string(),
        )
    } else {
        (
            format!("{PREAMBLE}void main() {{\no = vec4(0.0);\n"),
            "\ntsubu_color = vec4(o.rgb, 1.0);\n}\n".to_string(),
        )
    };
    let body = if declares_main(&body) {
        body.replacen("void main", "void tsubu_user_main", 1)
    } else {
        body
    };

    let lines_before = prefix.bytes().filter(|b| *b == b'\n').count() as u32;
    let start = prefix.len();
    let mut text = prefix;
    text.push_str(&body);
    let end = text.len();
    text.push_str(&suffix);

    Wrapped { text, lines_before, body: start..end }
}

/// 自分で `main()` を書いているか。
fn declares_main(source: &str) -> bool {
    source.contains("void main")
}

/// プリプロセッサ指令ではない `#` 行を空行にする。
///
/// つぶやき GLSL は本文に `#つぶやきGLSL` のタグを添えて投稿される。行を消すと
/// エラー位置がずれるので、中身だけ落として行は残す。
fn strip_hashtags(source: &str) -> String {
    const DIRECTIVES: &[&str] = &[
        "version", "define", "undef", "ifdef", "ifndef", "if", "else", "elif", "endif", "error",
        "pragma", "extension", "line",
    ];

    let kept: Vec<&str> = source
        .lines()
        .map(|line| {
            let rest = line.trim_start();
            let Some(rest) = rest.strip_prefix('#') else { return line };
            let word = rest.trim_start();
            let word_end = word.find(|c: char| !c.is_ascii_alphabetic()).unwrap_or(word.len());
            if DIRECTIVES.contains(&&word[..word_end]) { line } else { "" }
        })
        .collect();
    kept.join("\n")
}

/// バイト位置を 1 始まりの行・列にする。列は文字数で数える。
fn line_and_column(text: &str, at: usize) -> (u32, u32) {
    let head = &text[..at.min(text.len())];
    let line = head.bytes().filter(|b| *b == b'\n').count() as u32 + 1;
    let column = head.rsplit('\n').next().unwrap_or("").chars().count() as u32 + 1;
    (line, column)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_geekest_one_liner_becomes_a_shader() {
        let wgsl = compile("o = vec4(FC.xy / r, sin(t), 1.);").expect("通る");
        assert!(wgsl.contains("@fragment"), "{wgsl}");
        assert!(wgsl.contains("@vertex"), "{wgsl}");
        assert!(wgsl.contains(VERTEX_ENTRY), "{wgsl}");
    }

    #[test]
    fn writing_your_own_main_also_works() {
        // twigl の geek 流儀。gl_FragColor へ書く作品もここに入る。
        let wgsl = compile("void main() {\n  gl_FragColor = vec4(gl_FragCoord.xy / r, 0, 1);\n}")
            .expect("通る");
        assert!(wgsl.contains("@fragment"), "{wgsl}");
    }

    #[test]
    fn the_helpers_twigl_provides_are_available() {
        for source in [
            "o.rgb = hsv(t, 1., 1.);",
            "o.rg = FC.xy * rotate2D(t);",
            "o.rgb = vec3(FC.xy, 1.) * rotate3D(t, vec3(0, 1, 0));",
            "o = vec4(snoise2D(FC.xy));",
            "o = vec4(snoise3D(vec3(FC.xy, t)));",
            "o = vec4(PI, TAU, 0, 1);",
        ] {
            assert!(compile(source).is_ok(), "{source} が通らない: {:?}", compile(source));
        }
    }

    #[test]
    fn the_hashtag_at_the_end_is_not_a_directive() {
        let source = "o = vec4(1);\n\n# つぶやきGLSL\n";
        assert!(compile(source).is_ok(), "{:?}", compile(source));
    }

    #[test]
    fn an_error_points_at_the_line_in_the_original_source() {
        // 3 行目で宣言していない名前を使う。
        let source = "float a = 1.;\nfloat b = 2.;\no = vec4(nosuch);";
        let error = compile(source).expect_err("通らない");
        assert_eq!(error.line, 3, "{error:?}");
    }

    #[test]
    fn the_fragment_coordinate_is_flipped_to_the_glsl_convention() {
        // gl_FragCoord は GLSL が左下原点、wgpu が左上原点。r.y から引いて直す。
        let wgsl = compile("o = vec4(FC.y);").expect("通る");
        assert!(wgsl.contains("global.r"), "解像度を使って上下を直していない: {wgsl}");
    }

    #[test]
    fn the_hashtag_line_does_not_move_the_line_numbers() {
        let source = "#つぶやきGLSL\nfloat a = 1.;\no = vec4(nosuch);";
        let error = compile(source).expect_err("通らない");
        assert_eq!(error.line, 3, "{error:?}");
    }

    #[test]
    fn broken_input_does_not_panic() {
        for source in ["", "{", "/* 閉じない", "日本語だけ", "#", "#version 100"] {
            let _ = compile(source);
        }
    }
}
