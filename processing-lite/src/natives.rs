//! Processing Lite API のネイティブ関数と組み込み変数 (設計書 §14.2)。
//!
//! ユーザーコードから触れるのはここに並んだものだけ。任意のファイルアクセスや
//! ネットワークへの入り口は最初から存在しない (設計書 §21)。

use tsubu_renderer::{AngleMode, Color, Graphics, Origin, ShapeKind, ShapeMode, TextAlign};

use crate::bytecode::Value;
use crate::math::{self, Rng};

/// 読み取り専用の組み込み変数。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BuiltinVar {
    Width,
    Height,
    /// `drawingContext`。ブラウザのキャンバスそのもの。ここには無いので、
    /// 書き込みを受け取るだけの入れ物を渡す。
    DrawingContext,
    FrameCount,
    MouseX,
    MouseY,
    MousePressed,
    KeyPressed,
    Pi,
    TwoPi,
    Tau,
    HalfPi,
    QuarterPi,

    // p5.js の定数。
    Rgb,
    Hsb,
    Blend,
    Add,
    Multiply,
    Screen,

    // beginShape() / endShape() の指定。
    Close,
    Points,
    Lines,
    Triangles,
    TriangleStrip,
    TriangleFan,

    // rectMode() / ellipseMode() / angleMode() の指定。
    Corner,
    Corners,
    Center,
    Radius,
    Degrees,
    Radians,

    // textAlign() の指定。
    Left,
    Right,
    Top,
    Bottom,
    Baseline,

    // size() / createCanvas() の 3 つめ。
    P2d,
    P3d,
    WebGl,

    // arc() の閉じ方。
    Open,
    Chord,
    Pie,

    // blendMode() の追加ぶん。
    Difference,
    Exclusion,
    Darkest,
    Lightest,
    Subtract,
    Replace,
}

impl BuiltinVar {
    pub fn resolve(name: &str) -> Option<Self> {
        Some(match name {
            "width" => BuiltinVar::Width,
            "drawingContext" => BuiltinVar::DrawingContext,
            "height" => BuiltinVar::Height,
            "frameCount" => BuiltinVar::FrameCount,
            "mouseX" => BuiltinVar::MouseX,
            "mouseY" => BuiltinVar::MouseY,
            "mousePressed" => BuiltinVar::MousePressed,
            "keyPressed" => BuiltinVar::KeyPressed,
            "PI" => BuiltinVar::Pi,
            "TWO_PI" => BuiltinVar::TwoPi,
            "TAU" => BuiltinVar::Tau,
            "HALF_PI" => BuiltinVar::HalfPi,
            "QUARTER_PI" => BuiltinVar::QuarterPi,
            "RGB" => BuiltinVar::Rgb,
            "HSB" | "HSV" => BuiltinVar::Hsb,
            "BLEND" => BuiltinVar::Blend,
            "ADD" => BuiltinVar::Add,
            "MULTIPLY" => BuiltinVar::Multiply,
            "SCREEN" => BuiltinVar::Screen,
            "CLOSE" => BuiltinVar::Close,
            "POINTS" => BuiltinVar::Points,
            "LINES" => BuiltinVar::Lines,
            "TRIANGLES" => BuiltinVar::Triangles,
            "TRIANGLE_STRIP" => BuiltinVar::TriangleStrip,
            "TRIANGLE_FAN" => BuiltinVar::TriangleFan,
            "CORNER" => BuiltinVar::Corner,
            "CORNERS" => BuiltinVar::Corners,
            "CENTER" => BuiltinVar::Center,
            "RADIUS" => BuiltinVar::Radius,
            "DEGREES" => BuiltinVar::Degrees,
            "RADIANS" => BuiltinVar::Radians,
            "LEFT" => BuiltinVar::Left,
            "RIGHT" => BuiltinVar::Right,
            "TOP" => BuiltinVar::Top,
            "BOTTOM" => BuiltinVar::Bottom,
            "BASELINE" => BuiltinVar::Baseline,
            // P2D と OPENGL は Processing の別名。中身は同じ。
            "P2D" | "JAVA2D" => BuiltinVar::P2d,
            "P3D" | "OPENGL" => BuiltinVar::P3d,
            "WEBGL" => BuiltinVar::WebGl,
            "OPEN" => BuiltinVar::Open,
            "CHORD" => BuiltinVar::Chord,
            "PIE" => BuiltinVar::Pie,
            "DIFFERENCE" => BuiltinVar::Difference,
            "EXCLUSION" => BuiltinVar::Exclusion,
            "DARKEST" => BuiltinVar::Darkest,
            "LIGHTEST" => BuiltinVar::Lightest,
            "SUBTRACT" => BuiltinVar::Subtract,
            "REPLACE" => BuiltinVar::Replace,
            _ => return None,
        })
    }

    pub fn read(self, g: &Graphics) -> Value {
        use std::f32::consts;
        match self {
            // 影の設定などを書き込む先。効きはしないが、書けないと止まってしまう。
            BuiltinVar::DrawingContext => Value::new_object(),
            // size() を呼ばない作品も動くよう、width/height は実表示サイズを返す。
            BuiltinVar::Width => Value::Int(g.width as i32),
            BuiltinVar::Height => Value::Int(g.height as i32),
            BuiltinVar::FrameCount => Value::Int(g.frame_count as i32),
            BuiltinVar::MouseX => Value::Int(g.mouse_x as i32),
            BuiltinVar::MouseY => Value::Int(g.mouse_y as i32),
            BuiltinVar::MousePressed => Value::Bool(g.mouse_pressed),
            BuiltinVar::KeyPressed => Value::Bool(g.key_pressed),
            BuiltinVar::Pi => Value::Float(consts::PI),
            BuiltinVar::TwoPi => Value::Float(consts::TAU),
            // TAU は TWO_PI と同じ。p5 の書き方。
            BuiltinVar::Tau => Value::Float(consts::TAU),
            BuiltinVar::HalfPi => Value::Float(consts::FRAC_PI_2),
            BuiltinVar::QuarterPi => Value::Float(consts::FRAC_PI_4),
            // 定数は番号でしかないので、そのまま数値にする。
            BuiltinVar::Rgb => Value::Float(1.0),
            BuiltinVar::Hsb => Value::Float(3.0),
            BuiltinVar::Blend => Value::Float(0.0),
            BuiltinVar::Add => Value::Float(1.0),
            BuiltinVar::Multiply => Value::Float(2.0),
            BuiltinVar::Screen => Value::Float(3.0),
            // 形の指定も番号。beginShape / endShape の側で見分ける。
            BuiltinVar::Close => Value::Float(10.0),
            BuiltinVar::Points => Value::Float(11.0),
            BuiltinVar::Lines => Value::Float(12.0),
            BuiltinVar::Triangles => Value::Float(13.0),
            BuiltinVar::TriangleStrip => Value::Float(14.0),
            BuiltinVar::TriangleFan => Value::Float(15.0),
            BuiltinVar::Corner => Value::Float(20.0),
            BuiltinVar::Corners => Value::Float(21.0),
            BuiltinVar::Center => Value::Float(22.0),
            BuiltinVar::Radius => Value::Float(23.0),
            BuiltinVar::Radians => Value::Float(24.0),
            BuiltinVar::Degrees => Value::Float(25.0),
            BuiltinVar::Left => Value::Float(26.0),
            BuiltinVar::Right => Value::Float(27.0),
            BuiltinVar::Top => Value::Float(28.0),
            BuiltinVar::Bottom => Value::Float(29.0),
            BuiltinVar::Baseline => Value::Float(30.0),
            // 描画方式。size() が見分ける。
            BuiltinVar::P2d => Value::Float(40.0),
            BuiltinVar::P3d => Value::Float(41.0),
            BuiltinVar::WebGl => Value::Float(42.0),
            BuiltinVar::Open => Value::Float(50.0),
            BuiltinVar::Chord => Value::Float(51.0),
            BuiltinVar::Pie => Value::Float(52.0),
            BuiltinVar::Difference => Value::Float(4.0),
            BuiltinVar::Exclusion => Value::Float(5.0),
            BuiltinVar::Darkest => Value::Float(6.0),
            BuiltinVar::Lightest => Value::Float(7.0),
            BuiltinVar::Subtract => Value::Float(8.0),
            BuiltinVar::Replace => Value::Float(9.0),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Native {
    Size,
    /// `Array(n)`。長さ n の配列を作る。`[...Array(n)]` の形でよく出る。
    ArrayOf,
    /// `createVector()` / `new PVector()`。
    CreateVector,
    FromCodePoint,
    Text,
    TextSize,
    TextAlign,
    TextWidth,
    Str,
    Nf,
    BeginShape,
    Vertex,
    EndShape,
    Arc,
    Bezier,
    /// `color()`。値としての色を作る。
    MakeColor,
    LerpColor,
    Red,
    Green,
    Blue,
    Alpha,
    Hue,
    Saturation,
    Brightness,
    RandomGaussian,
    Quad,
    /// `square(x, y, size)`。数の 2 乗 (`Native::Square`) とは別物。
    SquareShape,
    Curve,
    CurveVertex,
    BezierVertex,
    ResetMatrix,
    Clear,
    RectMode,
    EllipseMode,
    AngleMode,
    NoLoop,
    Loop,
    Hypot,
    Sign,
    Cbrt,
    Log2,
    Log10,
    Background,
    Fill,
    Stroke,
    NoFill,
    NoStroke,
    StrokeWeight,

    Point,
    Line,
    Rect,
    Ellipse,
    Circle,
    Triangle,

    Translate,
    Rotate,
    Scale,
    PushMatrix,
    Push,
    Pop,
    PushStyle,
    PopStyle,

    // 3D (設計書 §14.2)。
    Box,
    Sphere,
    RotateX,
    RotateY,
    RotateZ,
    SphereDetail,
    Lights,
    NoLights,
    PopMatrix,

    Sin,
    Cos,
    Tan,
    Atan2,
    Abs,
    Min,
    Max,
    Map,
    Constrain,
    Sqrt,
    Pow,
    Floor,
    Ceil,
    Round,
    Dist,
    Lerp,
    Radians,
    Degrees,

    Random,
    Noise,

    // p5.js 側の名前。
    CreateCanvas,
    ColorMode,
    BlendMode,
    Square,
    Magnitude,
    Atan,
    Asin,
    Acos,
    Exp,
    Log,
    Norm,
    ToInt,
    ToFloat,
    RandomSeed,
    NoiseSeed,
    Millis,
}

/// 1 つの名前に対して受け付ける引数の数。Processing の多重定義を数で見分ける。
struct Signature {
    name: &'static str,
    native: Native,
    arities: &'static [u8],
}

const SIGNATURES: &[Signature] = &[
    Signature { name: "size", native: Native::Size, arities: &[2, 3] },
    Signature { name: "createCanvas", native: Native::CreateCanvas, arities: &[1, 2, 3] },
    Signature { name: "colorMode", native: Native::ColorMode, arities: &[1, 2, 4, 5] },
    Signature { name: "blendMode", native: Native::BlendMode, arities: &[1] },
    Signature { name: "sq", native: Native::Square, arities: &[1] },
    Signature { name: "mag", native: Native::Magnitude, arities: &[2] },
    Signature { name: "atan", native: Native::Atan, arities: &[1] },
    Signature { name: "asin", native: Native::Asin, arities: &[1] },
    Signature { name: "acos", native: Native::Acos, arities: &[1] },
    Signature { name: "exp", native: Native::Exp, arities: &[1] },
    Signature { name: "log", native: Native::Log, arities: &[1] },
    // JavaScript の Math にあって p5 に無いもの。`Math.hypot` などから使う。
    Signature { name: "hypot", native: Native::Hypot, arities: &[2] },
    Signature { name: "sign", native: Native::Sign, arities: &[1] },
    Signature { name: "cbrt", native: Native::Cbrt, arities: &[1] },
    Signature { name: "log2", native: Native::Log2, arities: &[1] },
    Signature { name: "log10", native: Native::Log10, arities: &[1] },
    Signature { name: "norm", native: Native::Norm, arities: &[3] },
    Signature { name: "int", native: Native::ToInt, arities: &[1] },
    Signature { name: "float", native: Native::ToFloat, arities: &[1] },
    Signature { name: "randomSeed", native: Native::RandomSeed, arities: &[1] },
    Signature { name: "noiseSeed", native: Native::NoiseSeed, arities: &[1] },
    Signature { name: "millis", native: Native::Millis, arities: &[0] },
    // p5 では pushMatrix / popMatrix をこう呼ぶ。
    // p5.js の push()/pop() は見た目まで戻す。Processing の pushMatrix() とは違う。
    Signature { name: "push", native: Native::Push, arities: &[0] },
    Signature { name: "pop", native: Native::Pop, arities: &[0] },
    Signature { name: "pushStyle", native: Native::PushStyle, arities: &[0] },
    Signature { name: "popStyle", native: Native::PopStyle, arities: &[0] },
    Signature { name: "background", native: Native::Background, arities: &[1, 2, 3, 4] },
    Signature { name: "fill", native: Native::Fill, arities: &[1, 2, 3, 4] },
    Signature { name: "stroke", native: Native::Stroke, arities: &[1, 2, 3, 4] },
    Signature { name: "noFill", native: Native::NoFill, arities: &[0] },
    Signature { name: "noStroke", native: Native::NoStroke, arities: &[0] },
    Signature { name: "strokeWeight", native: Native::StrokeWeight, arities: &[1] },
    Signature { name: "point", native: Native::Point, arities: &[2] },
    Signature { name: "line", native: Native::Line, arities: &[4] },
    Signature { name: "rect", native: Native::Rect, arities: &[4, 5, 6, 7, 8] },
    Signature { name: "ellipse", native: Native::Ellipse, arities: &[4] },
    Signature { name: "circle", native: Native::Circle, arities: &[3] },
    Signature { name: "triangle", native: Native::Triangle, arities: &[6] },
    Signature { name: "beginShape", native: Native::BeginShape, arities: &[0, 1] },
    Signature { name: "vertex", native: Native::Vertex, arities: &[2] },
    Signature { name: "endShape", native: Native::EndShape, arities: &[0, 1] },
    Signature { name: "arc", native: Native::Arc, arities: &[6, 7] },
    Signature { name: "quad", native: Native::Quad, arities: &[8] },
    Signature { name: "square", native: Native::SquareShape, arities: &[3] },
    Signature { name: "resetMatrix", native: Native::ResetMatrix, arities: &[0] },
    Signature { name: "clear", native: Native::Clear, arities: &[0] },
    Signature { name: "curve", native: Native::Curve, arities: &[8] },
    Signature { name: "curveVertex", native: Native::CurveVertex, arities: &[2] },
    Signature { name: "bezierVertex", native: Native::BezierVertex, arities: &[6] },
    Signature { name: "rectMode", native: Native::RectMode, arities: &[1] },
    Signature { name: "ellipseMode", native: Native::EllipseMode, arities: &[1] },
    Signature { name: "angleMode", native: Native::AngleMode, arities: &[1] },
    Signature { name: "noLoop", native: Native::NoLoop, arities: &[0] },
    Signature { name: "loop", native: Native::Loop, arities: &[0] },
    Signature { name: "bezier", native: Native::Bezier, arities: &[8] },
    Signature { name: "color", native: Native::MakeColor, arities: &[1, 2, 3, 4] },
    Signature { name: "lerpColor", native: Native::LerpColor, arities: &[3] },
    Signature { name: "red", native: Native::Red, arities: &[1] },
    Signature { name: "green", native: Native::Green, arities: &[1] },
    Signature { name: "blue", native: Native::Blue, arities: &[1] },
    Signature { name: "alpha", native: Native::Alpha, arities: &[1] },
    Signature { name: "hue", native: Native::Hue, arities: &[1] },
    Signature { name: "saturation", native: Native::Saturation, arities: &[1] },
    Signature { name: "brightness", native: Native::Brightness, arities: &[1] },
    Signature { name: "randomGaussian", native: Native::RandomGaussian, arities: &[0] },
    Signature { name: "translate", native: Native::Translate, arities: &[2, 3] },
    Signature { name: "rotate", native: Native::Rotate, arities: &[1, 4] },
    Signature { name: "scale", native: Native::Scale, arities: &[1, 2, 3] },
    Signature { name: "box", native: Native::Box, arities: &[1, 3] },
    Signature { name: "sphere", native: Native::Sphere, arities: &[1] },
    Signature { name: "sphereDetail", native: Native::SphereDetail, arities: &[1, 2] },
    Signature { name: "rotateX", native: Native::RotateX, arities: &[1] },
    Signature { name: "rotateY", native: Native::RotateY, arities: &[1] },
    Signature { name: "rotateZ", native: Native::RotateZ, arities: &[1] },
    Signature { name: "lights", native: Native::Lights, arities: &[0] },
    Signature { name: "noLights", native: Native::NoLights, arities: &[0] },
    // 光源を細かく置く API。向きや色までは再現しないので、既定の明かりを
    // 点けるだけにする。真っ黒よりは作品の姿が伝わる。
    Signature { name: "ambientLight", native: Native::Lights, arities: &[1, 2, 3, 4] },
    Signature { name: "directionalLight", native: Native::Lights, arities: &[6] },
    Signature { name: "pointLight", native: Native::Lights, arities: &[6] },
    Signature { name: "pushMatrix", native: Native::PushMatrix, arities: &[0] },
    Signature { name: "popMatrix", native: Native::PopMatrix, arities: &[0] },
    Signature { name: "sin", native: Native::Sin, arities: &[1] },
    Signature { name: "cos", native: Native::Cos, arities: &[1] },
    Signature { name: "tan", native: Native::Tan, arities: &[1] },
    Signature { name: "atan2", native: Native::Atan2, arities: &[2] },
    Signature { name: "abs", native: Native::Abs, arities: &[1] },
    Signature { name: "min", native: Native::Min, arities: &[2] },
    Signature { name: "max", native: Native::Max, arities: &[2] },
    Signature { name: "Array", native: Native::ArrayOf, arities: &[1] },
    Signature { name: "createVector", native: Native::CreateVector, arities: &[0, 1, 2, 3] },
    Signature { name: "fromCodePoint", native: Native::FromCodePoint, arities: &[1] },
    Signature { name: "fromCharCode", native: Native::FromCodePoint, arities: &[1] },
    Signature { name: "text", native: Native::Text, arities: &[3] },
    Signature { name: "textSize", native: Native::TextSize, arities: &[1] },
    Signature { name: "textAlign", native: Native::TextAlign, arities: &[1, 2] },
    Signature { name: "textWidth", native: Native::TextWidth, arities: &[1] },
    Signature { name: "str", native: Native::Str, arities: &[1] },
    Signature { name: "nf", native: Native::Nf, arities: &[2, 3] },
    Signature { name: "map", native: Native::Map, arities: &[5] },
    Signature { name: "constrain", native: Native::Constrain, arities: &[3] },
    Signature { name: "sqrt", native: Native::Sqrt, arities: &[1] },
    Signature { name: "pow", native: Native::Pow, arities: &[2] },
    Signature { name: "floor", native: Native::Floor, arities: &[1] },
    Signature { name: "ceil", native: Native::Ceil, arities: &[1] },
    Signature { name: "round", native: Native::Round, arities: &[1] },
    Signature { name: "dist", native: Native::Dist, arities: &[4] },
    Signature { name: "lerp", native: Native::Lerp, arities: &[3] },
    Signature { name: "radians", native: Native::Radians, arities: &[1] },
    Signature { name: "degrees", native: Native::Degrees, arities: &[1] },
    Signature { name: "random", native: Native::Random, arities: &[0, 1, 2] },
    Signature { name: "noise", native: Native::Noise, arities: &[1, 2, 3] },
];

/// 引数の数を選ばない関数。p5.js の `min` / `max` は好きなだけ渡せる。
const VARIADIC: &[&str] = &["min", "max"];

/// 名前が Processing Lite API かどうか。
pub fn is_native(name: &str) -> bool {
    SIGNATURES.iter().any(|s| s.name == name)
}

/// 名前と引数の数からネイティブ関数を決める。
pub fn resolve(name: &str, argc: u8) -> Option<Native> {
    SIGNATURES
        .iter()
        .find(|s| {
            s.name == name
                && (s.arities.contains(&argc) || (VARIADIC.contains(&name) && argc >= 1))
        })
        .map(|s| s.native)
}

/// 引数の数を問わず、名前だけからネイティブ関数を決める。
///
/// `B = blendMode` のように、関数を値として持つときに使う。
pub fn resolve_any(name: &str) -> Option<Native> {
    SIGNATURES.iter().find(|s| s.name == name).map(|s| s.native)
}

/// エラーメッセージ用に、その名前が受け付ける引数の数を並べる。
pub fn is_variadic(name: &str) -> bool {
    VARIADIC.contains(&name)
}

/// 引数の個数を問わず、名前だけでネイティブを引く。
///
/// `f(...xs)` のように個数が実行時まで決まらない呼び出しで使う。
pub fn resolve_by_name(name: &str) -> Option<Native> {
    SIGNATURES.iter().find(|s| s.name == name).map(|s| s.native)
}

pub fn accepted_arities(name: &str) -> Vec<u8> {
    SIGNATURES
        .iter()
        .find(|s| s.name == name)
        .map(|s| s.arities.to_vec())
        .unwrap_or_default()
}

/// `drawingContext` の影が乗る図形。
///
/// 影は同じ形をずらして重ねて作るので、呼び直しても副作用の無いものに
/// 限る。`random()` を使う関数を入れると乱数の数列がずれる。
fn casts_shadow(native: Native) -> bool {
    matches!(
        native,
        Native::Rect
            | Native::Ellipse
            | Native::Circle
            | Native::SquareShape
            | Native::Triangle
            | Native::Quad
            | Native::Line
            | Native::Point
            | Native::Arc
            | Native::Bezier
            | Native::Curve
            | Native::EndShape
            | Native::Text
            | Native::Box
            | Native::Sphere
    )
}

/// ネイティブ関数を実行する。引数の数は解決時に検証済み。
pub fn call(native: Native, args: &[Value], g: &mut Graphics, rng: &mut Rng) -> Value {
    // 影があれば、同じ形を先にぼかし色で置く。
    if casts_shadow(native) {
        let samples = g.shadow_samples();
        let any = !samples.is_empty();
        for (offset, weight) in samples {
            g.begin_shadow(offset, weight);
            run(native, args, g, rng);
        }
        if any {
            g.end_shadow();
        }
    }
    run(native, args, g, rng)
}

fn run(native: Native, args: &[Value], g: &mut Graphics, rng: &mut Rng) -> Value {
    // 値として呼ばれた場合 (`B = blendMode; B(ADD)`) は引数の数を検証していない。
    // 足りない分は 0 として扱い、範囲外アクセスで落ちないようにする。
    let f = |i: usize| args.get(i).map_or(0.0, Value::as_f32);

    match native {
        // 宣言されたキャンバスは、縦横比を保ったまま表示領域へ収める。
        Native::Size | Native::CreateCanvas => {
            g.set_canvas(f(0), f(1));
            // 3 つめは描画方式。P3D と WEBGL で原点の置き方が違う。
            match args.get(2).map(|v| v.as_f32() as i32) {
                Some(41) => g.enable_3d(Origin::TopLeft),
                Some(42) => g.enable_3d(Origin::Center),
                _ => {}
            }
            // p5.js の createCanvas() は呼ぶたびにキャンバスを作り直す。
            // 中身は消え、塗りと線も既定へ戻る。`draw()` の頭で毎フレーム
            // 呼んで画面を消す書き方があり、そこが違うと絵が積もり続ける。
            // Processing の size() にこの働きは無い。
            if native == Native::CreateCanvas {
                g.recreate_canvas();
            }
            Value::Void
        }

        // 色の解釈は colorMode に従う。引数の数え方は p5 と同じ。
        Native::Background => {
            let color = resolve_color(args, g);
            g.background_color(color);
            Value::Void
        }
        Native::Fill => {
            let color = resolve_color(args, g);
            g.fill_color(color);
            Value::Void
        }
        Native::Stroke => {
            let color = resolve_color(args, g);
            g.stroke_color(color);
            Value::Void
        }
        Native::NoFill => {
            g.no_fill();
            Value::Void
        }
        Native::NoStroke => {
            g.no_stroke();
            Value::Void
        }
        Native::StrokeWeight => {
            g.stroke_weight(f(0));
            Value::Void
        }

        Native::Point => {
            g.point(f(0), f(1));
            Value::Void
        }
        Native::Line => {
            g.line(f(0), f(1), f(2), f(3));
            Value::Void
        }
        Native::Rect => {
            // 5 個目からは角の丸み。1 個なら 4 隅とも同じ、4 個なら左上から
            // 時計回りに指定する (Processing と同じ)。
            match args.len() {
                0..=4 => g.rect(f(0), f(1), f(2), f(3)),
                5 => g.rect_rounded(f(0), f(1), f(2), f(3), [f(4); 4]),
                6 => g.rect_rounded(f(0), f(1), f(2), f(3), [f(4), f(5), f(5), f(5)]),
                7 => g.rect_rounded(f(0), f(1), f(2), f(3), [f(4), f(5), f(6), f(6)]),
                _ => g.rect_rounded(f(0), f(1), f(2), f(3), [f(4), f(5), f(6), f(7)]),
            }
            Value::Void
        }
        Native::Ellipse => {
            g.ellipse(f(0), f(1), f(2), f(3));
            Value::Void
        }
        Native::Circle => {
            g.circle(f(0), f(1), f(2));
            Value::Void
        }
        // ---- 自由な形 ----
        Native::BeginShape => {
            g.begin_shape(shape_kind(args.first()));
            Value::Void
        }
        Native::Vertex => {
            g.vertex(f(0), f(1));
            Value::Void
        }
        Native::EndShape => {
            // `endShape(CLOSE)` だけが閉じる指示。
            let close = args.first().is_some_and(|v| v.as_f32() == 10.0);
            g.end_shape(close);
            Value::Void
        }
        Native::Arc => {
            // 7 つめは閉じ方。角度は angleMode() の単位。
            let mode = match args.get(6).map(|v| v.as_f32() as i32) {
                Some(51) => tsubu_renderer::ArcMode::Chord,
                Some(52) => tsubu_renderer::ArcMode::Pie,
                _ => tsubu_renderer::ArcMode::Open,
            };
            g.arc_mode(f(0), f(1), f(2), f(3), g.to_radians(f(4)), g.to_radians(f(5)), mode);
            Value::Void
        }
        Native::Quad => {
            g.quad_points(f(0), f(1), f(2), f(3), f(4), f(5), f(6), f(7));
            Value::Void
        }
        Native::SquareShape => {
            g.square(f(0), f(1), f(2));
            Value::Void
        }
        Native::Curve => {
            g.curve(f(0), f(1), f(2), f(3), f(4), f(5), f(6), f(7));
            Value::Void
        }
        Native::CurveVertex => {
            g.curve_vertex(f(0), f(1));
            Value::Void
        }
        Native::BezierVertex => {
            g.bezier_vertex(f(0), f(1), f(2), f(3), f(4), f(5));
            Value::Void
        }
        Native::ResetMatrix => {
            g.reset_matrix();
            Value::Void
        }
        Native::Clear => {
            g.clear();
            Value::Void
        }
        Native::RectMode => {
            g.set_rect_mode(shape_mode(args.first()));
            Value::Void
        }
        Native::EllipseMode => {
            g.set_ellipse_mode(shape_mode(args.first()));
            Value::Void
        }
        Native::AngleMode => {
            // 25.0 が DEGREES。それ以外はラジアン。
            let degrees = args.first().is_some_and(|v| v.as_f32() == 25.0);
            g.set_angle_mode(if degrees { AngleMode::Degrees } else { AngleMode::Radians });
            Value::Void
        }
        Native::NoLoop => {
            g.set_looping(false);
            Value::Void
        }
        Native::Loop => {
            g.set_looping(true);
            Value::Void
        }
        Native::Bezier => {
            g.bezier(f(0), f(1), f(2), f(3), f(4), f(5), f(6), f(7));
            Value::Void
        }

        // ---- 値としての色 ----
        //
        // 中身は `[r, g, b, a]` の配列。専用の型を足さずに済ませている。
        // `fill()` や `stroke()` は配列を受け取ったらそのまま色として使う。
        Native::MakeColor => {
            let numbers: Vec<f32> = args.iter().map(Value::as_f32).collect();
            let c = g.color_from(&numbers);
            Value::new_array(vec![
                Value::Float(c.r * 255.0),
                Value::Float(c.g * 255.0),
                Value::Float(c.b * 255.0),
                Value::Float(c.a * 255.0),
            ])
        }
        Native::LerpColor => {
            let a = color_components(args.first());
            let b = color_components(args.get(1));
            let t = f(2).clamp(0.0, 1.0);
            Value::new_array(
                (0..4)
                    .map(|i| Value::Float(a[i] + (b[i] - a[i]) * t))
                    .collect(),
            )
        }

        Native::Triangle => {
            g.triangle(f(0), f(1), f(2), f(3), f(4), f(5));
            Value::Void
        }

        Native::Translate => {
            if args.len() >= 3 {
                g.translate_3d(f(0), f(1), f(2));
            } else {
                g.translate(f(0), f(1));
            }
            Value::Void
        }
        Native::Rotate => {
            // 4 引数は軸まわりの回転。Processing の 3D の書き方。
            if args.len() >= 4 {
                g.rotate_axis(g.to_radians(f(0)), [f(1), f(2), f(3)]);
            } else {
                g.rotate(g.to_radians(f(0)));
            }
            Value::Void
        }
        Native::Scale => match args.len() {
            0 | 1 => {
                g.scale(f(0), f(0));
                Value::Void
            }
            2 => {
                g.scale(f(0), f(1));
                Value::Void
            }
            _ => {
                g.scale_3d(f(0), f(1), f(2));
                Value::Void
            }
        },

        // 3D。奥行きのある図形は、呼ばれた時点で 3D へ切り替える。
        // size() に P3D を書き忘れた作品も、そのまま動く。
        Native::Box => {
            if args.len() >= 3 {
                g.draw_box(f(0), f(1), f(2));
            } else {
                g.draw_box(f(0), f(0), f(0));
            }
            Value::Void
        }
        Native::Sphere => {
            g.sphere(f(0));
            Value::Void
        }
        Native::SphereDetail => {
            // 1 つだけなら経度・緯度とも同じ。Processing と同じ扱い。
            let longitude = f(0) as usize;
            let latitude = if args.len() >= 2 { f(1) as usize } else { longitude };
            g.set_sphere_detail(longitude, latitude);
            Value::Void
        }
        Native::RotateX => {
            g.rotate_axis(g.to_radians(f(0)), [1.0, 0.0, 0.0]);
            Value::Void
        }
        Native::RotateY => {
            g.rotate_axis(g.to_radians(f(0)), [0.0, 1.0, 0.0]);
            Value::Void
        }
        Native::RotateZ => {
            g.rotate_axis(g.to_radians(f(0)), [0.0, 0.0, 1.0]);
            Value::Void
        }
        Native::Lights => {
            g.lights(true);
            Value::Void
        }
        Native::NoLights => {
            g.lights(false);
            Value::Void
        }
        Native::PushMatrix => {
            g.push_matrix();
            Value::Void
        }
        Native::Push => {
            g.push_all();
            Value::Void
        }
        Native::Pop => {
            g.pop_all();
            Value::Void
        }
        Native::PushStyle => {
            g.push_style();
            Value::Void
        }
        Native::PopStyle => {
            g.pop_style();
            Value::Void
        }
        Native::PopMatrix => {
            g.pop_matrix();
            Value::Void
        }

        // 三角関数の引数と、逆関数の戻り値は angleMode() の単位で扱う。
        Native::Sin => Value::Float(g.to_radians(f(0)).sin()),
        Native::Cos => Value::Float(g.to_radians(f(0)).cos()),
        Native::Tan => Value::Float(g.to_radians(f(0)).tan()),
        Native::Atan2 => Value::Float(g.from_radians(f(0).atan2(f(1)))),
        Native::Sqrt => Value::Float(f(0).max(0.0).sqrt()),
        Native::Pow => Value::Float(f(0).powf(f(1))),
        Native::Radians => Value::Float(f(0).to_radians()),
        Native::Degrees => Value::Float(f(0).to_degrees()),
        Native::Map => Value::Float(math::map(f(0), f(1), f(2), f(3), f(4))),
        Native::Lerp => Value::Float(f(0) + (f(1) - f(0)) * f(2)),
        Native::Dist => Value::Float(((f(2) - f(0)).powi(2) + (f(3) - f(1)).powi(2)).sqrt()),

        // Processing の floor/ceil/round は int を返す。
        Native::Floor => Value::Int(f(0).floor() as i32),
        Native::Ceil => Value::Int(f(0).ceil() as i32),
        Native::Round => Value::Int(f(0).round() as i32),

        // abs/min/max は int を渡せば int が返る。
        Native::Abs => match args.first().unwrap_or(&Value::Float(0.0)) {
            Value::Int(v) => Value::Int(v.saturating_abs()),
            _ => Value::Float(f(0).abs()),
        },
        // p5.js の min / max は引数をいくつでも取る。
        // `Array(9)` は長さ 9 の配列。中身は undefined で、`map` の添字を
        // 使うための足場として使われる。
        Native::ArrayOf => {
            let n = args.first().map_or(0.0, Value::as_f32);
            let n = if n.is_finite() && n >= 0.0 { n as usize } else { 0 };
            Value::new_array(vec![Value::Undefined; n.min(1 << 20)])
        }
        Native::Hypot => Value::Float(f(0).hypot(f(1))),
        Native::Sign => Value::Float(match f(0) {
            v if v > 0.0 => 1.0,
            v if v < 0.0 => -1.0,
            // 0 と NaN はそのまま返す。JavaScript の Math.sign と同じ。
            v => v,
        }),
        Native::Cbrt => Value::Float(f(0).cbrt()),
        Native::Log2 => Value::Float(f(0).log2()),
        Native::Log10 => Value::Float(f(0).log10()),
        Native::CreateVector => Value::new_vector(f(0), f(1), f(2)),

        // `String.fromCodePoint(n)`。番号から 1 文字を作る。
        Native::FromCodePoint => {
            let code = f(0);
            let ch = if code.is_finite() && code >= 0.0 {
                char::from_u32(code as u32).unwrap_or('\u{fffd}')
            } else {
                '\u{fffd}'
            };
            Value::new_str(ch.to_string())
        }

        // ---- 文字 ----
        Native::Text => {
            let text = args.first().map(Value::to_display).unwrap_or_default();
            g.text(&text, f(1), f(2));
            Value::Void
        }
        Native::TextSize => {
            g.set_text_size(f(0));
            Value::Void
        }
        Native::TextAlign => {
            g.set_text_align(text_align(args.first()), text_align(args.get(1)));
            Value::Void
        }
        Native::TextWidth => {
            let text = args.first().map(Value::to_display).unwrap_or_default();
            Value::Float(g.measure(&text))
        }
        Native::Str => Value::new_str(args.first().map(Value::to_display).unwrap_or_default()),
        Native::Nf => {
            // `nf(v, 桁, 小数桁)`。Processing は左を 0 で埋める。
            let digits = args.get(1).map_or(0, |v| v.as_f32().max(0.0) as usize);
            let decimals = args.get(2).map_or(0, |v| v.as_f32().max(0.0) as usize);
            let value = f(0);
            let body = format!("{:.*}", decimals, value.abs());
            let (whole, rest) = body.split_once('.').unwrap_or((body.as_str(), ""));
            let padded = format!("{whole:0>digits$}");
            let sign = if value < 0.0 { "-" } else { "" };
            Value::new_str(if rest.is_empty() {
                format!("{sign}{padded}")
            } else {
                format!("{sign}{padded}.{rest}")
            })
        }
        // 色の成分。`color()` の作った値と、詰めた int のどちらも受ける。
        Native::Red => Value::Float(color_components(args.first())[0]),
        Native::Green => Value::Float(color_components(args.first())[1]),
        Native::Blue => Value::Float(color_components(args.first())[2]),
        Native::Alpha => Value::Float(color_components(args.first())[3]),
        Native::Hue | Native::Saturation | Native::Brightness => {
            let c = color_components(args.first());
            let (h, s, v) = rgb_to_hsb(c[0] / 255.0, c[1] / 255.0, c[2] / 255.0);
            // 返す範囲は colorMode に合わせる。既定では 0..255。
            let max = g.color_max();
            Value::Float(match native {
                Native::Hue => h * max[0],
                Native::Saturation => s * max[1],
                _ => v * max[2],
            })
        }
        // 平均 0、標準偏差 1 の正規乱数 (Box-Muller)。
        Native::RandomGaussian => {
            let u1 = rng.random(1.0).max(f32::MIN_POSITIVE);
            let u2 = rng.random(1.0);
            Value::Float((-2.0 * u1.ln()).sqrt() * (std::f32::consts::TAU * u2).cos())
        }
        Native::Min => extreme(args, false),
        Native::Max => extreme(args, true),
        Native::Constrain => match (
            args.first().unwrap_or(&Value::Float(0.0)),
            args.get(1).unwrap_or(&Value::Float(0.0)),
            args.get(2).unwrap_or(&Value::Float(0.0)),
        ) {
            (Value::Int(v), Value::Int(lo), Value::Int(hi)) => {
                Value::Int((*v).clamp(*lo.min(hi), *hi.max(lo)))
            }
            _ => Value::Float(math::constrain(f(0), f(1), f(2))),
        },

        Native::Random => {
            if args.len() == 1 {
                Value::Float(rng.random(f(0)))
            } else {
                Value::Float(rng.random_between(f(0), f(1)))
            }
        }
        Native::Noise => match args.len() {
            0 | 1 => Value::Float(math::noise(f(0), 0.0)),
            2 => Value::Float(math::noise(f(0), f(1))),
            _ => Value::Float(math::noise3(f(0), f(1), f(2))),
        },

        Native::ColorMode => {
            let mode = if f(0) as i32 == 3 {
                tsubu_renderer::ColorMode::Hsb
            } else {
                tsubu_renderer::ColorMode::Rgb
            };
            // p5 の既定値。`colorMode(HSB)` だけなら 360, 100, 100, 1。
            let default_max = match mode {
                tsubu_renderer::ColorMode::Hsb => [360.0, 100.0, 100.0, 1.0],
                tsubu_renderer::ColorMode::Rgb => [255.0; 4],
            };
            let max = match args.len() {
                0 | 1 => default_max,
                2 => [f(1), f(1), f(1), f(1)],
                4 => [f(1), f(2), f(3), default_max[3]],
                _ => [f(1), f(2), f(3), f(4)],
            };
            g.color_mode(mode, max);
            Value::Void
        }

        Native::BlendMode => {
            // 定数の値は BuiltinVar 側で決めている。
            let mode = match f(0) as i32 {
                1 => tsubu_renderer::BlendMode::Add,
                2 => tsubu_renderer::BlendMode::Multiply,
                3 => tsubu_renderer::BlendMode::Screen,
                4 => tsubu_renderer::BlendMode::Difference,
                5 => tsubu_renderer::BlendMode::Exclusion,
                6 => tsubu_renderer::BlendMode::Darkest,
                7 => tsubu_renderer::BlendMode::Lightest,
                8 => tsubu_renderer::BlendMode::Subtract,
                9 => tsubu_renderer::BlendMode::Replace,
                _ => tsubu_renderer::BlendMode::Blend,
            };
            g.blend_mode(mode);
            Value::Void
        }

        Native::Square => Value::Float(f(0) * f(0)),
        Native::Magnitude => Value::Float((f(0) * f(0) + f(1) * f(1)).sqrt()),
        Native::Atan => Value::Float(g.from_radians(f(0).atan())),
        Native::Asin => Value::Float(g.from_radians(f(0).clamp(-1.0, 1.0).asin())),
        Native::Acos => Value::Float(g.from_radians(f(0).clamp(-1.0, 1.0).acos())),
        Native::Exp => Value::Float(f(0).exp()),
        Native::Log => Value::Float(f(0).max(f32::MIN_POSITIVE).ln()),
        Native::Norm => Value::Float(math::map(f(0), f(1), f(2), 0.0, 1.0)),
        Native::ToInt => Value::Int(args.first().map_or(0, Value::as_i32)),
        Native::ToFloat => Value::Float(f(0)),

        // 乱数もノイズもシード固定で動かしているので、種の付け替えだけ受ける。
        Native::RandomSeed => {
            *rng = Rng::new(f(0) as i64 as u64);
            Value::Void
        }
        // ノイズは位置だけで決まる実装なので、種は持てない。受けるだけ。
        Native::NoiseSeed => Value::Void,

        // frameCount から求める。実時間ではないので、再生が止まれば進まない。
        Native::Millis => Value::Float(g.frame_count as f32 * 1000.0 / 60.0),
    }
}

/// 引数のうち最小か最大を返す。int どうしなら int のまま。
fn extreme(args: &[Value], want_max: bool) -> Value {
    let all_int = !args.is_empty() && args.iter().all(|v| matches!(v, Value::Int(_)));
    if all_int {
        let mut best = args[0].as_i32();
        for value in &args[1..] {
            let v = value.as_i32();
            best = if want_max { best.max(v) } else { best.min(v) };
        }
        return Value::Int(best);
    }

    let mut best = args.first().map_or(0.0, Value::as_f32);
    for value in args.iter().skip(1) {
        let v = value.as_f32();
        best = if want_max { best.max(v) } else { best.min(v) };
    }
    Value::Float(best)
}

/// 引数を数値の並びにする。色の指定に使う。
fn numbers(args: &[Value]) -> Vec<f32> {
    args.iter().map(Value::as_f32).collect()
}

/// 色を決める。`color()` の作った値と、数値の並びの両方を受ける。
///
/// `color()` の中身は RGB の 0〜255 で持っているので、[`Graphics::color_from`]
/// は通さない。通すと `colorMode(HSB)` のときに二重変換になる。
/// 値ひとつを色として読む。`color()` の戻り値や詰めた整数を受ける。
pub fn color_from_value(value: &Value, g: &Graphics) -> Color {
    resolve_color(std::slice::from_ref(value), g)
}

fn resolve_color(args: &[Value], g: &Graphics) -> Color {
    // `stroke(-1)` のように int をひとつ渡す書き方は、詰めた色 (0xAARRGGBB)。
    // Processing は型で見分けるので、こちらも int のときだけそう読む。
    // float の `fill(128.0)` は今までどおり明度。
    //
    // p5.js にこの解釈は無い。`stroke(500)` はただの明度で、255 へ丸まって
    // 白になる。詰めた色として読むと alpha が 0 になり、何も描かれない。
    if let (1, Some(Value::Int(packed))) = (args.len(), args.first())
        && !(0..=255).contains(packed)
        && g.flavour().packs_ints_into_colors()
    {
        let v = *packed as u32;
        return Color::rgba(
            ((v >> 16) & 255) as f32 / 255.0,
            ((v >> 8) & 255) as f32 / 255.0,
            (v & 255) as f32 / 255.0,
            ((v >> 24) & 255) as f32 / 255.0,
        );
    }
    let Some(Value::Array(_)) = args.first() else {
        return g.color_from(&numbers(args));
    };
    let c = color_components(args.first());
    // `fill(c, 50)` の 50 は colorMode の不透明度の最大値で測る。
    let alpha = match args.get(1) {
        Some(v) => (v.as_f32() / g.color_max()[3]).clamp(0.0, 1.0),
        None => (c[3] / 255.0).clamp(0.0, 1.0),
    };
    Color::rgba(
        (c[0] / 255.0).clamp(0.0, 1.0),
        (c[1] / 255.0).clamp(0.0, 1.0),
        (c[2] / 255.0).clamp(0.0, 1.0),
        alpha,
    )
}

/// `beginShape()` の引数から形の種類を決める。
fn shape_kind(arg: Option<&Value>) -> ShapeKind {
    match arg.map(Value::as_f32) {
        Some(11.0) => ShapeKind::Points,
        Some(12.0) => ShapeKind::Lines,
        Some(13.0) => ShapeKind::Triangles,
        Some(14.0) => ShapeKind::TriangleStrip,
        Some(15.0) => ShapeKind::TriangleFan,
        _ => ShapeKind::Polygon,
    }
}

/// `textAlign()` の引数から揃え方を決める。
fn text_align(arg: Option<&Value>) -> TextAlign {
    match arg.map(Value::as_f32) {
        Some(22.0) => TextAlign::Center,
        Some(26.0) => TextAlign::Start,
        Some(27.0) => TextAlign::End,
        Some(28.0) => TextAlign::Start,
        Some(29.0) => TextAlign::End,
        Some(30.0) => TextAlign::Baseline,
        _ => TextAlign::Start,
    }
}

/// `rectMode()` / `ellipseMode()` の引数から指定を決める。
fn shape_mode(arg: Option<&Value>) -> ShapeMode {
    match arg.map(Value::as_f32) {
        Some(21.0) => ShapeMode::Corners,
        Some(22.0) => ShapeMode::Center,
        Some(23.0) => ShapeMode::Radius,
        _ => ShapeMode::Corner,
    }
}

/// RGB (0..1) を HSB (0..1) へ。色の成分を返す関数で使う。
fn rgb_to_hsb(r: f32, g: f32, b: f32) -> (f32, f32, f32) {
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let span = max - min;

    let hue = if span < 1e-6 {
        0.0
    } else if max == r {
        ((g - b) / span / 6.0).rem_euclid(1.0)
    } else if max == g {
        ((b - r) / span + 2.0) / 6.0
    } else {
        ((r - g) / span + 4.0) / 6.0
    };
    let saturation = if max < 1e-6 { 0.0 } else { span / max };
    (hue, saturation, max)
}

/// 色の値から `[r, g, b, a]` を取り出す。
///
/// `color()` が返す配列のほか、単なる数値も灰色として受ける。
fn color_components(value: Option<&Value>) -> [f32; 4] {
    match value {
        Some(Value::Array(items)) => {
            let items = items.borrow();
            let get = |i: usize| items.get(i).map_or(0.0, Value::as_f32);
            [get(0), get(1), get(2), items.get(3).map_or(255.0, Value::as_f32)]
        }
        // 詰めた色 (0xAARRGGBB)。`color()` の戻り値以外にも、int を直に
        // 渡す書き方がある。
        Some(Value::Int(packed)) if !(0..=255).contains(packed) => {
            let v = *packed as u32;
            [
                ((v >> 16) & 255) as f32,
                ((v >> 8) & 255) as f32,
                (v & 255) as f32,
                ((v >> 24) & 255) as f32,
            ]
        }
        Some(other) => {
            let v = other.as_f32();
            [v, v, v, 255.0]
        }
        None => [0.0, 0.0, 0.0, 255.0],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overloads_are_resolved_by_argument_count() {
        assert_eq!(resolve("fill", 1), Some(Native::Fill));
        assert_eq!(resolve("fill", 3), Some(Native::Fill));
        assert_eq!(resolve("fill", 5), None);
        assert_eq!(resolve("rect", 3), None);
        assert_eq!(resolve("rect", 4), Some(Native::Rect));
    }

    #[test]
    fn unknown_names_do_not_resolve() {
        assert!(!is_native("loadImage"));
        assert_eq!(resolve("loadImage", 1), None);
    }

    #[test]
    fn known_name_with_wrong_arity_can_be_reported() {
        assert!(is_native("rect"));
        assert_eq!(accepted_arities("rect"), vec![4, 5, 6, 7, 8]);
        assert_eq!(accepted_arities("fill"), vec![1, 2, 3, 4]);
    }

    #[test]
    fn integer_math_stays_integer() {
        let mut g = Graphics::new();
        let mut rng = Rng::new(1);
        assert_eq!(call(Native::Abs, &[Value::Int(-3)], &mut g, &mut rng), Value::Int(3));
        assert_eq!(
            call(Native::Min, &[Value::Int(2), Value::Int(5)], &mut g, &mut rng),
            Value::Int(2)
        );
        assert_eq!(call(Native::Abs, &[Value::Float(-3.5)], &mut g, &mut rng), Value::Float(3.5));
    }

    #[test]
    fn floor_and_round_return_int_like_processing() {
        let mut g = Graphics::new();
        let mut rng = Rng::new(1);
        assert_eq!(call(Native::Floor, &[Value::Float(2.9)], &mut g, &mut rng), Value::Int(2));
        assert_eq!(call(Native::Round, &[Value::Float(2.5)], &mut g, &mut rng), Value::Int(3));
    }

    #[test]
    fn drawing_natives_emit_geometry() {
        let mut g = Graphics::new();
        let mut rng = Rng::new(1);
        g.begin_frame(100.0, 100.0);
        assert!(g.draw_list().is_empty());
        call(
            Native::Rect,
            &[Value::Int(0), Value::Int(0), Value::Int(10), Value::Int(10)],
            &mut g,
            &mut rng,
        );
        assert!(!g.draw_list().is_empty());
    }
}
