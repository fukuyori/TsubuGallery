//! CPU 側の描画コマンド生成層。
//!
//! Processing Lite の描画 API はここで幾何形状へ展開され、単一の三角形リスト
//! ([`DrawList`]) になる。GPU バックエンドはこのリストしか知らないため、
//! Viewer 描画とサムネイル生成が同一経路を共有できる (設計書 §17)。

use crate::mat4::{Camera, Mat4, Origin};

/// sRGB 空間の色。各成分は `0.0..=1.0`。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Color {
    pub const BLACK: Color = Color::rgba(0.0, 0.0, 0.0, 1.0);
    pub const WHITE: Color = Color::rgba(1.0, 1.0, 1.0, 1.0);

    pub const fn rgba(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self { r, g, b, a }
    }

    /// Processing と同じ `0..=255` 表記から生成する。
    pub fn rgba255(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self::rgba(r / 255.0, g / 255.0, b / 255.0, a / 255.0)
    }

    /// `background()` を呼ばない作品の地の色。
    ///
    /// Processing の既定と同じ灰色 204。黒にすると、既定の黒い線で描く作品が
    /// 何も見えなくなる。
    pub const DEFAULT_BACKGROUND: Color = Color { r: 0.8, g: 0.8, b: 0.8, a: 1.0 };

    pub fn gray255(v: f32) -> Self {
        Self::rgba(v / 255.0, v / 255.0, v / 255.0, 1.0)
    }

    /// 頂点バッファへ載せる値。
    ///
    /// レンダーターゲットは非 sRGB フォーマットなので、sRGB の値をそのまま書く。
    /// つまりアルファ合成も sRGB 空間で起きるが、これは Processing の挙動と同じで、
    /// egui が期待するフレームバッファとも一致する。
    fn to_array(self) -> [f32; 4] {
        [self.r, self.g, self.b, self.a]
    }
}

/// 図形をどう重ねるか (p5.js の `blendMode`)。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum BlendMode {
    /// 通常のアルファ合成。
    #[default]
    Blend,
    /// 加算。光が重なって明るくなる。
    Add,
    /// 乗算。重なるほど暗くなる。
    Multiply,
    /// スクリーン。加算より穏やかに明るくなる。
    Screen,
    /// 差分。重なると反転する。白の上では色が抜ける。
    ///
    /// 本来は `|下 - 上|` だが、GPU の合成は引き算の符号を選べない。
    /// ここでは除外 (`上 + 下 - 2*上*下`) で近似する。どちらかが 0 か 1 の
    /// ときは完全に一致し、白い図形を黒地に重ねる使い方では差が出ない。
    Difference,
    /// 除外。差分より中間調がやわらかい。
    Exclusion,
    /// 暗いほうを採る。
    Darkest,
    /// 明るいほうを採る。
    Lightest,
    /// 下から上を引く。
    Subtract,
    /// 混ぜずに置き換える。
    Replace,
}

/// 同じ合成方法で続けて描ける区間。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Batch {
    pub blend: BlendMode,
    /// 深度バッファへ書くか。3D の作品だけ真になる。
    pub depth: bool,
    /// [`DrawList::indices`] の範囲。
    pub start: u32,
    pub end: u32,
}

/// 色の指定のしかた (p5.js の `colorMode`)。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ColorMode {
    #[default]
    Rgb,
    Hsb,
}

/// 2D アフィン変換。`x' = a*x + c*y + e`, `y' = b*x + d*y + f`。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Affine {
    pub a: f32,
    pub b: f32,
    pub c: f32,
    pub d: f32,
    pub e: f32,
    pub f: f32,
}

impl Affine {
    pub const IDENTITY: Affine = Affine { a: 1.0, b: 0.0, c: 0.0, d: 1.0, e: 0.0, f: 0.0 };

    /// `self` のあとに `rhs` をローカル適用する (Processing の `translate` 等と同じ順序)。
    pub fn then_local(self, rhs: Affine) -> Affine {
        Affine {
            a: self.a * rhs.a + self.c * rhs.b,
            b: self.b * rhs.a + self.d * rhs.b,
            c: self.a * rhs.c + self.c * rhs.d,
            d: self.b * rhs.c + self.d * rhs.d,
            e: self.a * rhs.e + self.c * rhs.f + self.e,
            f: self.b * rhs.e + self.d * rhs.f + self.f,
        }
    }

    pub fn apply(self, x: f32, y: f32) -> [f32; 2] {
        [self.a * x + self.c * y + self.e, self.b * x + self.d * y + self.f]
    }

    /// 拡大率の代表値。円の分割数や線幅の見積もりに使う。
    pub fn scale_hint(self) -> f32 {
        (self.a * self.d - self.b * self.c).abs().sqrt().max(1e-4)
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Vertex {
    /// キャンバス上の位置と深さ。深さは 0 が手前、1 が奥。
    ///
    /// 3D も CPU 側で画面座標まで落としてからここへ入れる。
    /// GPU に渡るのは 2D と同じ三角形の列。
    pub pos: [f32; 3],
    pub color: [f32; 4],
    /// 字形アトラス上の位置。文字以外の図形は白い点を指す。
    pub uv: [f32; 2],
}

/// `rectMode()` / `ellipseMode()` の指定。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ShapeMode {
    /// `rect()` の既定。左上と幅・高さ。
    #[default]
    Corner,
    /// 2 つの角。
    Corners,
    /// `ellipse()` の既定。中心と幅・高さ。
    Center,
    /// 中心と半径。
    Radius,
}

impl ShapeMode {
    /// 与えられた 4 つの数から、左上と幅・高さを求める。
    pub fn to_corner(self, a: f32, b: f32, c: f32, d: f32) -> (f32, f32, f32, f32) {
        match self {
            ShapeMode::Corner => (a, b, c, d),
            ShapeMode::Corners => (a.min(c), b.min(d), (c - a).abs(), (d - b).abs()),
            ShapeMode::Center => (a - c * 0.5, b - d * 0.5, c, d),
            ShapeMode::Radius => (a - c, b - d, c * 2.0, d * 2.0),
        }
    }

    /// 与えられた 4 つの数から、中心と幅・高さを求める。
    pub fn to_center(self, a: f32, b: f32, c: f32, d: f32) -> (f32, f32, f32, f32) {
        let (x, y, w, h) = self.to_corner(a, b, c, d);
        (x + w * 0.5, y + h * 0.5, w, h)
    }
}

/// `arc()` の閉じ方。
///
/// 塗りの形と縁取りの引き方が変わる。既定は `Open`。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ArcMode {
    /// 弦で閉じて塗り、縁は弧だけ。Processing と p5.js の既定。
    #[default]
    Open,
    /// 弦で閉じて塗り、縁も弦まで引く。
    Chord,
    /// 中心まで閉じて扇形に塗り、縁も中心まで引く。
    Pie,
}

/// `textAlign()` の指定。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TextAlign {
    /// 横なら左、縦なら上。
    #[default]
    Start,
    Center,
    /// 横なら右、縦なら下。
    End,
    /// 縦だけ。文字の基準線。
    Baseline,
}

/// `angleMode()`。角度の単位。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AngleMode {
    #[default]
    Radians,
    Degrees,
}

/// `beginShape()` の種類 (設計書 §14.2)。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ShapeKind {
    /// 既定。頂点を順に結んだ多角形。
    #[default]
    Polygon,
    Points,
    Lines,
    Triangles,
    TriangleStrip,
    TriangleFan,
}

/// 組み立て中の形。
#[derive(Clone, Debug)]
struct Shape {
    kind: ShapeKind,
    points: Vec<[f32; 2]>,
    /// `curveVertex()` で与えられた制御点。
    curves: Vec<[f32; 2]>,
}

/// `drawingContext` の影。canvas の `shadowBlur` / `shadowColor` 相当。
///
/// 本物はガウスぼかしだが、ここでは同じ形をずらして何枚か重ねて似せる。
/// 図形ごとの専用処理が要らないので、丸い角の矩形でも文字でも同じように効く。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shadow {
    /// canvas の `shadowBlur`。ぼけの広がり (ピクセル)。
    pub blur: f32,
    /// `shadowOffsetX` / `shadowOffsetY`。
    pub offset: [f32; 2],
    pub color: Color,
}

impl Shadow {
    /// 影を落とすか。色が透明、またはぼけも位置ずれも無ければ落とさない。
    fn is_visible(&self) -> bool {
        self.color.a > 0.0 && (self.blur > 0.0 || self.offset != [0.0, 0.0])
    }

    /// 重ねる位置と、その 1 枚の濃さ。
    ///
    /// 中心を濃く、外の輪ほど薄くする。同じ濃さで重ねると縁が硬い帯になって、
    /// canvas のなだらかなぼけにならない。
    fn samples(&self) -> Vec<([f32; 2], f32)> {
        let [dx, dy] = self.offset;
        if self.blur <= 0.0 {
            return vec![([dx, dy], 1.0)];
        }
        // ぼけが大きくても枚数は増やさない。重い作品で効きすぎないように。
        let radius = self.blur.min(64.0) * 0.7;
        let mut out = vec![([dx, dy], 1.0)];
        for (scale, weight, count) in [(0.35, 0.40, 8), (0.70, 0.18, 10), (1.0, 0.07, 12)] {
            for i in 0..count {
                // 輪ごとに少し回して、放射状の筋が出ないようにする。
                let a = std::f32::consts::TAU * (i as f32 + scale) / count as f32;
                let r = radius * scale;
                out.push(([dx + a.cos() * r, dy + a.sin() * r], weight));
            }
        }
        out
    }
}

/// `size()` で宣言されたキャンバスを、表示領域へどう当てはめるか。
///
/// つぶやき系の作品は正方形が多く、横長の画面に出すと左右が余る。
/// 埋めれば大きく見えるが、上下 (または左右) がはみ出して切れる。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CanvasFit {
    /// 全体が入るように収める。余った側に余白が出る。
    #[default]
    Contain,
    /// 表示領域を埋める。収まらない側は切れる。
    Cover,
}

/// 作品 1 つぶんの描画状態。
///
/// `setup()` で決まって以降ずっと効くもの — 塗り、線、キャンバスの大きさ、
/// 角度の単位、3D かどうか — をひとまとめにする。作品を切り替えるとき、
/// これを預けておいて戻ってきたら復す。
#[derive(Clone, Debug)]
pub struct GraphicsState {
    color_mode: ColorMode,
    color_max: [f32; 4],
    blend: BlendMode,
    canvas: Option<(f32, f32)>,
    fill: Option<Color>,
    stroke: Option<Color>,
    stroke_weight: f32,
    space: Option<Space>,
    shadow: Option<Shadow>,
    styles: Vec<Style>,
    text_stroked: bool,
    default_background: Color,
    text_size: f32,
    text_align: (TextAlign, TextAlign),
    rect_mode: ShapeMode,
    ellipse_mode: ShapeMode,
    angle_mode: AngleMode,
    looping: bool,
}

impl Default for GraphicsState {
    fn default() -> Self {
        Self {
            color_mode: ColorMode::Rgb,
            color_max: [255.0; 4],
            blend: BlendMode::Blend,
            canvas: None,
            fill: Some(Color::WHITE),
            stroke: Some(Color::BLACK),
            stroke_weight: 1.0,
            space: None,
            shadow: None,
            styles: Vec::new(),
            text_stroked: false,
            default_background: Color::DEFAULT_BACKGROUND,
            text_size: 12.0,
            text_align: (TextAlign::Start, TextAlign::Baseline),
            rect_mode: ShapeMode::Corner,
            ellipse_mode: ShapeMode::Center,
            angle_mode: AngleMode::Radians,
            looping: true,
        }
    }
}

/// `pushStyle()` で退避する描画の見た目。座標変換は含まない。
#[derive(Clone, Copy, Debug)]
struct Style {
    fill: Option<Color>,
    stroke: Option<Color>,
    stroke_weight: f32,
    text_size: f32,
    text_align: (TextAlign, TextAlign),
    rect_mode: ShapeMode,
    ellipse_mode: ShapeMode,
    angle_mode: AngleMode,
    color_mode: ColorMode,
    color_max: [f32; 4],
    blend: BlendMode,
    shadow: Option<Shadow>,
}

/// 影を描いているあいだ、退避しておく描画状態。
struct ShadowPass {
    fill: Option<Color>,
    stroke: Option<Color>,
    matrix: Affine,
    model: Option<Mat4>,
}

/// 3D の状態。`size(w, h, P3D)` を書いた作品だけが持つ。
#[derive(Clone, Debug)]
struct Space {
    camera: Camera,
    /// モデルビュー行列。カメラの分もここに入っている。
    ///
    /// Processing の `resetMatrix()` はこれを単位行列へ戻す。カメラごと
    /// 消える、というのがそのまま「原点が画面の中央に来る」書き方になる。
    model: Mat4,
    stack: Vec<Mat4>,
    /// `lights()` が呼ばれたか。フレームごとに消える。
    lights: bool,
}

/// 視点より手前にある点の行き先。
///
/// 深さが 1 を超えるので、この頂点を含む面はラスタライザが落とす。
const BEHIND_THE_EYE: [f32; 3] = [0.0, 0.0, 2.0];

/// 1 つの形に貯められる頂点の上限。
const MAX_SHAPE_POINTS: usize = 20_000;

/// 1 フレーム分の描画コマンドを三角形へ展開したもの。
#[derive(Default, Debug)]
pub struct DrawList {
    /// このフレームで塗りつぶす色。`None` なら前のフレームを残す。
    ///
    /// Processing も p5.js も、`background()` を呼ばなければ前の絵が残る。
    /// 残像を使う作品はこれを当てにしている。
    pub clear: Option<Color>,
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
    /// 合成方法ごとの区間。ふつうは 1 つだけ。
    pub batches: Vec<Batch>,
}

impl DrawList {
    pub fn is_empty(&self) -> bool {
        self.indices.is_empty()
    }

    /// 確保済みバッファを保持したまま内容だけ捨てる。
    fn reset(&mut self) {
        self.clear = None;
        self.vertices.clear();
        self.indices.clear();
        self.batches.clear();
    }
}

impl Default for Color {
    fn default() -> Self {
        Color::BLACK
    }
}

/// Processing Lite の描画コンテキスト。
///
/// 図形 API と、スケッチから参照される環境変数 (`width` / `frameCount` など) の
/// 両方を持つ。将来 VM のネイティブ関数はこの構造体を受け取る。
pub struct Graphics {
    list: DrawList,
    fill: Option<Color>,
    stroke: Option<Color>,
    stroke_weight: f32,
    matrix: Affine,
    stack: Vec<Affine>,
    /// 3D。`size(w, h, P3D)` を書いた作品だけが持つ。
    space: Option<Space>,
    /// `drawingContext` で指定された影。
    shadow: Option<Shadow>,
    /// 影を描いている最中。入れ子にならないようにする。
    shadow_pass: Option<ShadowPass>,
    /// `pushStyle()` の退避先。
    styles: Vec<Style>,
    /// `text()` に線を付けるか。p5.js の作品だけ真。
    text_stroked: bool,
    /// `background()` を一度も呼ばない作品の下地。
    default_background: Color,
    /// いま積んでいる三角形が深度バッファへ書くか。
    ///
    /// 2D だけの作品では一度も書かない。深さは全部 0 のままなので、
    /// 描いた順にそのまま重なる。
    depth_write: bool,
    /// `beginShape()` から `endShape()` までのあいだの頂点。
    shape: Option<Shape>,
    /// 字形のアトラス。使った字だけを溜める。
    pub font: crate::font::FontAtlas,
    text_size: f32,
    text_align: (TextAlign, TextAlign),
    rect_mode: ShapeMode,
    ellipse_mode: ShapeMode,
    angle_mode: AngleMode,
    /// `noLoop()` が呼ばれたら false。Viewer がこれを見てフレームを止める。
    looping: bool,

    /// `width`
    pub width: f32,
    /// `height`
    pub height: f32,
    /// `frameCount`
    pub frame_count: u64,
    /// 実行開始からの経過秒。
    pub time: f32,
    /// `mouseX`
    pub mouse_x: f32,
    /// `mouseY`
    pub mouse_y: f32,
    /// `mousePressed`
    pub mouse_pressed: bool,
    /// `keyPressed`
    pub key_pressed: bool,

    color_mode: ColorMode,
    /// 各成分の最大値。`colorMode(HSB, 360, 100, 100, 1)` のような指定を受ける。
    color_max: [f32; 4],
    blend: BlendMode,

    /// 実際の表示サイズ。`width` / `height` とは別物。
    viewport: (f32, f32),
    /// 作品が `size()` / `createCanvas()` で宣言したキャンバス。
    canvas: Option<(f32, f32)>,
    /// キャンバスを表示領域へどう当てはめるか。
    fit: CanvasFit,
    /// キャンバスを表示領域へ収める変換。毎フレームここから始める。
    base: Affine,
}

impl Graphics {
    pub fn new() -> Self {
        Self {
            list: DrawList::default(),
            fill: Some(Color::WHITE),
            stroke: Some(Color::BLACK),
            stroke_weight: 1.0,
            matrix: Affine::IDENTITY,
            stack: Vec::with_capacity(16),
            space: None,
            shadow: None,
            shadow_pass: None,
            styles: Vec::new(),
            text_stroked: false,
            default_background: Color::DEFAULT_BACKGROUND,
            depth_write: false,
            shape: None,
            font: crate::font::FontAtlas::new(),
            text_size: 12.0,
            text_align: (TextAlign::Start, TextAlign::Baseline),
            rect_mode: ShapeMode::Corner,
            ellipse_mode: ShapeMode::Center,
            angle_mode: AngleMode::Radians,
            looping: true,
            width: 0.0,
            height: 0.0,
            frame_count: 0,
            time: 0.0,
            mouse_x: 0.0,
            mouse_y: 0.0,
            mouse_pressed: false,
            key_pressed: false,
            color_mode: ColorMode::Rgb,
            color_max: [255.0; 4],
            blend: BlendMode::Blend,
            viewport: (0.0, 0.0),
            canvas: None,
            fit: CanvasFit::default(),
            base: Affine::IDENTITY,
        }
    }

    /// フレーム開始。Processing と同様、行列だけリセットし描画スタイルは保持する。
    pub fn begin_frame(&mut self, width: f32, height: f32) {
        // 作りかけの形はフレームをまたがせない。endShape() を書き忘れた作品が、
        // 次のフレームの頂点まで巻き込んで巨大な形になるのを防ぐ。
        self.shape = None;
        self.list.reset();
        self.stack.clear();
        self.viewport = (width, height);
        if let Some(space) = &mut self.space {
            space.lights = false;
        }
        self.apply_canvas();
    }

    /// `size()` / `createCanvas()`。宣言されたキャンバスを表示領域へ収める。
    ///
    /// 作品が `createCanvas(400, 400)` と書いて座標をそのまま使うことは多い。
    /// 無視すると画面の一部にしか描かれないので、縦横比を保ったまま拡大し、
    /// 中央へ寄せる。
    pub fn set_canvas(&mut self, width: f32, height: f32) {
        if width <= 0.0 || height <= 0.0 {
            return;
        }
        self.canvas = Some((width, height));
        self.apply_canvas();
    }

    /// p5.js の `createCanvas()` をもう一度呼んだときの後始末。
    ///
    /// p5 はキャンバスの要素を作り直す。描いてあったものは消え、キャンバスの
    /// 文脈も初期化されるので、塗りと線の色、線の太さ、座標変換が既定へ戻る。
    /// `noFill()` / `noStroke()` は p5 側の旗なので残る。
    ///
    /// `draw()` の頭で毎フレーム呼んで画面を消す書き方がある。Processing の
    /// `size()` にこの働きは無いので、`createCanvas()` からだけ呼ぶ。
    pub fn recreate_canvas(&mut self) {
        self.list.vertices.clear();
        self.list.indices.clear();
        self.list.batches.clear();
        self.list.clear = Some(self.default_background);
        self.fill = self.fill.map(|_| Color::WHITE);
        self.stroke = self.stroke.map(|_| Color::BLACK);
        self.stroke_weight = 1.0;
        self.shape = None;
        self.stack.clear();
        self.matrix = self.base;
        if let Some(space) = &mut self.space {
            space.model = space.camera.modelview();
            space.stack.clear();
        }
    }

    /// キャンバス指定から `width` / `height` と基準の変換を決め直す。
    fn apply_canvas(&mut self) {
        let (view_w, view_h) = self.viewport;
        match self.canvas {
            Some((canvas_w, canvas_h)) => {
                let (wide, tall) = (view_w / canvas_w, view_h / canvas_h);
                let scale = match self.fit {
                    CanvasFit::Contain => wide.min(tall),
                    CanvasFit::Cover => wide.max(tall),
                };
                self.base = Affine {
                    a: scale,
                    b: 0.0,
                    c: 0.0,
                    d: scale,
                    e: (view_w - canvas_w * scale) * 0.5,
                    f: (view_h - canvas_h * scale) * 0.5,
                };
                self.width = canvas_w;
                self.height = canvas_h;
            }
            None => {
                self.base = Affine::IDENTITY;
                self.width = view_w;
                self.height = view_h;
            }
        }
        self.matrix = self.base;
        // キャンバスの大きさが変わればカメラも変わる。行列もフレームごとに
        // ここで初期状態へ戻る。
        if let Some(space) = &mut self.space {
            space.camera = Camera::new(self.width, self.height, space.camera.origin);
            space.model = space.camera.modelview();
            space.stack.clear();
        }
    }

    /// キャンバスの当てはめ方を決める。設定から渡す。
    ///
    /// 作品を切り替えても保つ。見る人の好みであって作品の性質ではない。
    pub fn set_fit(&mut self, fit: CanvasFit) {
        if self.fit != fit {
            self.fit = fit;
            self.apply_canvas();
        }
    }

    pub fn fit(&self) -> CanvasFit {
        self.fit
    }

    /// 実際の表示サイズ。作品から見える `width` / `height` とは違うことがある。
    pub fn viewport(&self) -> (f32, f32) {
        self.viewport
    }

    pub fn draw_list(&self) -> &DrawList {
        &self.list
    }

    /// スケッチを切り替えるときに呼ぶ。スタイルまで含めて初期状態へ戻す。
    pub fn reset_state(&mut self) {
        self.set_state(GraphicsState::default());
        // 経過は作品ごとのもの。持ち越さない。
        self.frame_count = 0;
        self.time = 0.0;
        self.list.reset();
    }

    /// いまの描画状態を取り出す。
    ///
    /// Viewer は 1 つの [`Graphics`] を全作品で使い回す。切り替えのたびに
    /// 初期状態へ戻すと、`setup()` で決めた色やキャンバスの大きさが消える。
    /// `setup()` は作品ごとに一度しか走らないので、二度と戻ってこない。
    /// 作品ごとにこれを持っておいて、戻ってきたときに復す。
    pub fn state(&self) -> GraphicsState {
        GraphicsState {
            color_mode: self.color_mode,
            color_max: self.color_max,
            blend: self.blend,
            canvas: self.canvas,
            fill: self.fill,
            stroke: self.stroke,
            stroke_weight: self.stroke_weight,
            space: self.space.clone(),
            shadow: self.shadow,
            styles: self.styles.clone(),
            text_stroked: self.text_stroked,
            default_background: self.default_background,
            text_size: self.text_size,
            text_align: self.text_align,
            rect_mode: self.rect_mode,
            ellipse_mode: self.ellipse_mode,
            angle_mode: self.angle_mode,
            looping: self.looping,
        }
    }

    /// [`Graphics::state`] で取ったものを戻す。
    pub fn set_state(&mut self, s: GraphicsState) {
        self.color_mode = s.color_mode;
        self.color_max = s.color_max;
        self.blend = s.blend;
        self.canvas = s.canvas;
        self.fill = s.fill;
        self.stroke = s.stroke;
        self.stroke_weight = s.stroke_weight;
        self.space = s.space;
        self.shadow = s.shadow;
        self.styles = s.styles;
        self.text_stroked = s.text_stroked;
        self.default_background = s.default_background;
        self.text_size = s.text_size;
        self.text_align = s.text_align;
        self.rect_mode = s.rect_mode;
        self.ellipse_mode = s.ellipse_mode;
        self.angle_mode = s.angle_mode;
        self.looping = s.looping;

        // フレームの途中でしか意味を持たないものは持ち越さない。
        self.stack.clear();
        self.shape = None;
        self.shadow_pass = None;
        self.depth_write = false;
        // キャンバスの指定から base と width/height を組み直す。
        self.apply_canvas();
    }

    /// `rectMode()`。
    pub fn set_rect_mode(&mut self, mode: ShapeMode) {
        self.rect_mode = mode;
    }

    /// `ellipseMode()`。
    pub fn set_ellipse_mode(&mut self, mode: ShapeMode) {
        self.ellipse_mode = mode;
    }

    /// `angleMode()`。
    pub fn set_angle_mode(&mut self, mode: AngleMode) {
        self.angle_mode = mode;
    }

    /// 与えられた角度をラジアンへ直す。`angleMode(DEGREES)` のときだけ変換する。
    pub fn to_radians(&self, angle: f32) -> f32 {
        match self.angle_mode {
            AngleMode::Radians => angle,
            AngleMode::Degrees => angle.to_radians(),
        }
    }

    /// ラジアンを、いまの単位へ直す。`atan2()` などの戻り値に使う。
    pub fn from_radians(&self, angle: f32) -> f32 {
        match self.angle_mode {
            AngleMode::Radians => angle,
            AngleMode::Degrees => angle.to_degrees(),
        }
    }

    /// `noLoop()` / `loop()`。
    pub fn set_looping(&mut self, looping: bool) {
        self.looping = looping;
    }

    /// フレームを進めてよいか。`noLoop()` のあいだは false。
    pub fn is_looping(&self) -> bool {
        self.looping
    }

    // ---- 色と線 ---------------------------------------------------------

    /// `colorMode()`。最大値を変えると、以降の色指定の意味が変わる。
    pub fn color_mode(&mut self, mode: ColorMode, max: [f32; 4]) {
        self.color_mode = mode;
        self.color_max = max;
    }

    pub fn current_color_mode(&self) -> ColorMode {
        self.color_mode
    }

    /// `colorMode()` で決めた各成分の最大値。
    pub fn color_max(&self) -> [f32; 4] {
        self.color_max
    }

    /// `blendMode()`。以降の図形の重ね方が変わる。
    pub fn blend_mode(&mut self, blend: BlendMode) {
        self.blend = blend;
    }

    pub fn current_blend_mode(&self) -> BlendMode {
        self.blend
    }

    /// p5 と同じ引数の数え方で色を作る。
    ///
    /// 1 個なら明度、2 個なら明度と不透明度、3 個で色、4 個で色と不透明度。
    pub fn color_from(&self, args: &[f32]) -> Color {
        let max = self.color_max;
        let alpha = |v: f32| (v / max[3]).clamp(0.0, 1.0);

        match (self.color_mode, args.len()) {
            (_, 0) => Color::BLACK,
            (ColorMode::Rgb, 1) => {
                let v = (args[0] / max[0]).clamp(0.0, 1.0);
                Color::rgba(v, v, v, 1.0)
            }
            (ColorMode::Rgb, 2) => {
                let v = (args[0] / max[0]).clamp(0.0, 1.0);
                Color::rgba(v, v, v, alpha(args[1]))
            }
            (ColorMode::Rgb, 3) => Color::rgba(
                (args[0] / max[0]).clamp(0.0, 1.0),
                (args[1] / max[1]).clamp(0.0, 1.0),
                (args[2] / max[2]).clamp(0.0, 1.0),
                1.0,
            ),
            (ColorMode::Rgb, _) => Color::rgba(
                (args[0] / max[0]).clamp(0.0, 1.0),
                (args[1] / max[1]).clamp(0.0, 1.0),
                (args[2] / max[2]).clamp(0.0, 1.0),
                alpha(args[3]),
            ),
            // HSB で引数が少ないときは、p5 と同じく明度だけの指定になる。
            (ColorMode::Hsb, 1) => {
                let v = (args[0] / max[2]).clamp(0.0, 1.0);
                Color::rgba(v, v, v, 1.0)
            }
            (ColorMode::Hsb, 2) => {
                let v = (args[0] / max[2]).clamp(0.0, 1.0);
                Color::rgba(v, v, v, alpha(args[1]))
            }
            (ColorMode::Hsb, n) => {
                let a = if n >= 4 { alpha(args[3]) } else { 1.0 };
                hsb_to_color(
                    args[0] / max[0],
                    (args[1] / max[1]).clamp(0.0, 1.0),
                    (args[2] / max[2]).clamp(0.0, 1.0),
                    a,
                )
            }
        }
    }

    pub fn fill_color(&mut self, color: Color) {
        self.fill = Some(color);
    }

    pub fn stroke_color(&mut self, color: Color) {
        self.stroke = Some(color);
    }

    /// `background(gray)`。Processing と同じく、それまでの描画を消去する。
    pub fn background(&mut self, gray: f32) {
        self.background_color(Color::gray255(gray));
    }

    pub fn background_rgb(&mut self, r: f32, g: f32, b: f32) {
        self.background_color(Color::rgba255(r, g, b, 255.0));
    }

    /// `background()`。
    ///
    /// 不透明なら塗りつぶし、それまでの描画を捨てる。半透明なら「上から薄く塗る」
    /// (p5.js の残像表現)。前のフレームは残らないので、地は黒のままになる。
    /// `clear()`。積んだ絵を捨てる。
    ///
    /// Processing では透明になるが、ここでは黒で塗る。透明のままだと、
    /// 書き出したサムネイルが透けて、白い線の作品が見えなくなる。画面では
    /// 黒地に重ねて表示するので、見た目は Processing と変わらない。
    pub fn clear(&mut self) {
        self.list.vertices.clear();
        self.list.indices.clear();
        self.list.batches.clear();
        self.list.clear = Some(Color::BLACK);
    }

    pub fn background_color(&mut self, c: Color) {
        if c.a >= 1.0 {
            self.list.vertices.clear();
            self.list.indices.clear();
            self.list.batches.clear();
            self.list.clear = Some(c);
            return;
        }

        // 表示領域ぜんぶを覆う 1 枚。座標変換もキャンバスの拡大も外して描く。
        // 3D でも画面に貼るだけなので、遠近と深さは外す。深さを書き込むと
        // このあとの立体がぜんぶ隠れてしまう。
        let matrix = std::mem::replace(&mut self.matrix, Affine::IDENTITY);
        let space = self.space.take();
        let depth = std::mem::replace(&mut self.depth_write, false);
        let (w, h) = self.viewport;
        self.quad([0.0, 0.0], [w, 0.0], [w, h], [0.0, h], c);
        self.depth_write = depth;
        self.space = space;
        self.matrix = matrix;
    }

    pub fn fill(&mut self, gray: f32) {
        self.fill = Some(Color::gray255(gray));
    }

    pub fn fill_rgb(&mut self, r: f32, g: f32, b: f32) {
        self.fill = Some(Color::rgba255(r, g, b, 255.0));
    }

    pub fn fill_rgba(&mut self, r: f32, g: f32, b: f32, a: f32) {
        self.fill = Some(Color::rgba255(r, g, b, a));
    }

    pub fn no_fill(&mut self) {
        self.fill = None;
    }

    pub fn stroke(&mut self, gray: f32) {
        self.stroke = Some(Color::gray255(gray));
    }

    pub fn stroke_rgb(&mut self, r: f32, g: f32, b: f32) {
        self.stroke = Some(Color::rgba255(r, g, b, 255.0));
    }

    pub fn stroke_rgba(&mut self, r: f32, g: f32, b: f32, a: f32) {
        self.stroke = Some(Color::rgba255(r, g, b, a));
    }

    pub fn no_stroke(&mut self) {
        self.stroke = None;
    }

    pub fn stroke_weight(&mut self, w: f32) {
        self.stroke_weight = w.max(0.0);
    }

    // ---- 影 (`drawingContext`) ------------------------------------------

    /// `drawingContext.shadowBlur` などを設定する。
    pub fn set_shadow(&mut self, shadow: Option<Shadow>) {
        self.shadow = shadow.filter(Shadow::is_visible);
    }

    pub fn shadow(&self) -> Option<Shadow> {
        self.shadow
    }

    /// 影を落とす形をこれから描く。重ねる位置を返す。
    ///
    /// 落とさないとき、または影そのものを描いている最中なら空。
    pub fn shadow_samples(&self) -> Vec<([f32; 2], f32)> {
        if self.shadow_pass.is_some() {
            return Vec::new();
        }
        self.shadow.map(|s| s.samples()).unwrap_or_default()
    }

    /// 影の 1 枚を描き始める。塗りと線をぼかし色へ差し替え、位置をずらす。
    pub fn begin_shadow(&mut self, offset: [f32; 2], weight: f32) {
        let Some(shadow) = self.shadow else { return };
        if self.shadow_pass.is_none() {
            self.shadow_pass = Some(ShadowPass {
                fill: self.fill,
                stroke: self.stroke,
                matrix: self.matrix,
                model: self.space.as_ref().map(|s| s.model),
            });
        }
        let alpha = (shadow.color.a * weight).clamp(0.0, 1.0);
        let color = Color { a: alpha, ..shadow.color };
        self.fill = self.fill.map(|_| color);
        self.stroke = self.stroke.map(|_| color);
        // ずらしは画面の向き。作品の回転には従わない (canvas と同じ)。
        let saved = self.shadow_pass.as_ref().expect("いま入れた");
        self.matrix = Affine { e: saved.matrix.e + offset[0], f: saved.matrix.f + offset[1], ..saved.matrix };
        if let (Some(space), Some(model)) = (self.space.as_mut(), saved.model) {
            space.model = model;
        }
    }

    /// 影を描き終える。差し替えた状態を戻す。
    pub fn end_shadow(&mut self) {
        let Some(saved) = self.shadow_pass.take() else { return };
        self.fill = saved.fill;
        self.stroke = saved.stroke;
        self.matrix = saved.matrix;
        if let (Some(space), Some(model)) = (self.space.as_mut(), saved.model) {
            space.model = model;
        }
    }

    // ---- 3D (設計書 §14.2) ----------------------------------------------

    /// `size(w, h, P3D)` / `createCanvas(w, h, WEBGL)`。
    ///
    /// 遠近のついたカメラに切り替える。`z = 0` の平面は 1 ピクセル 1 単位で
    /// 写るので、2D のつもりで書いた `rect()` も同じ場所に出る。
    pub fn enable_3d(&mut self, origin: Origin) {
        if self.space.as_ref().is_some_and(|s| s.camera.origin == origin) {
            return;
        }
        let camera = Camera::new(self.width, self.height, origin);
        self.space =
            Some(Space { model: camera.modelview(), camera, stack: Vec::new(), lights: false });
    }

    pub fn is_3d(&self) -> bool {
        self.space.is_some()
    }

    /// 3D の呼び出しが来たので、まだなら切り替える。
    ///
    /// すでに 3D なら何もしない。`createCanvas(w, h, WEBGL)` で原点を中央に
    /// している作品を、`box()` の一言で左上へ引き戻さないため。
    fn ensure_3d(&mut self) {
        if self.space.is_none() {
            self.enable_3d(Origin::TopLeft);
        }
    }

    /// `lights()`。既定の環境光と、視点から差す平行光を入れる。
    pub fn lights(&mut self, on: bool) {
        self.ensure_3d();
        if let Some(space) = &mut self.space {
            space.lights = on;
        }
    }

    /// ローカル座標をキャンバス上の位置と深さへ直す。
    fn pt(&self, x: f32, y: f32) -> [f32; 3] {
        match &self.space {
            Some(_) => self.pt3(x, y, 0.0).unwrap_or(BEHIND_THE_EYE),
            None => {
                let at = self.matrix.apply(x, y);
                [at[0], at[1], 0.0]
            }
        }
    }

    /// 奥行きつきの 1 点。視点より手前なら `None`。
    fn pt3(&self, x: f32, y: f32, z: f32) -> Option<[f32; 3]> {
        let space = self.space.as_ref()?;
        let (at, depth) = space.camera.project(space.model.point([x, y, z]))?;
        // キャンバスを表示領域へ収める分は 2D と共通。
        let at = self.base.apply(at[0], at[1]);
        Some([at[0], at[1], depth])
    }

    /// 円の分割数や線の細さを決めるための、おおよその拡大率。
    fn scale_hint(&self) -> f32 {
        match &self.space {
            // 遠近が入ると 1 つの値では表せない。等倍として扱う。
            Some(_) => 1.0,
            None => self.matrix.scale_hint(),
        }
    }

    /// 視点座標の 3 点で面を 1 枚。手前すぎるものは描かない。
    ///
    /// 面が視点をまたぐ場合は切らずに丸ごと落とす。立体の一部が消えるが、
    /// 変な形が画面いっぱいに伸びるよりは害が小さい。
    fn face(&mut self, points: [[f32; 3]; 3], color: Color) {
        let Some(space) = &self.space else { return };
        let mut screen = [[0.0f32; 3]; 3];
        for (out, p) in screen.iter_mut().zip(points) {
            let Some((at, depth)) = space.camera.project(p) else { return };
            let at = self.base.apply(at[0], at[1]);
            *out = [at[0], at[1], depth];
        }
        self.tri(screen[0], screen[1], screen[2], color);
    }

    /// 立体の稜線。面と同じ深さだと消えるので、ほんの少し手前へ寄せる。
    ///
    /// 視点座標を原点方向へ縮めると、画面上の位置は変わらないまま深さだけ
    /// 手前になる。画面の解像度にも遠近にも左右されない。
    fn edge(&mut self, a: [f32; 3], b: [f32; 3], color: Color) {
        const TOWARD_THE_EYE: f32 = 0.997;
        let Some(space) = &self.space else { return };
        let pull = |p: [f32; 3]| [p[0] * TOWARD_THE_EYE, p[1] * TOWARD_THE_EYE, p[2] * TOWARD_THE_EYE];
        let (Some((p0, d0)), Some((p1, d1))) =
            (space.camera.project(pull(a)), space.camera.project(pull(b)))
        else {
            return;
        };
        let p0 = self.base.apply(p0[0], p0[1]);
        let p1 = self.base.apply(p1[0], p1[1]);
        let (dx, dy) = (p1[0] - p0[0], p1[1] - p0[1]);
        let len = (dx * dx + dy * dy).sqrt();
        if len < 1e-6 {
            return;
        }
        // 太さは画面上のピクセル。細い線は Processing も画面基準で引く。
        let hw = (self.stroke_weight * 0.5).max(0.5);
        let (nx, ny) = (-dy / len * hw, dx / len * hw);
        let base = self.list.vertices.len() as u32;
        self.push_vertex([p0[0] + nx, p0[1] + ny, d0], color);
        self.push_vertex([p1[0] + nx, p1[1] + ny, d1], color);
        self.push_vertex([p1[0] - nx, p1[1] - ny, d1], color);
        self.push_vertex([p0[0] - nx, p0[1] - ny, d0], color);
        self.emit(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }

    /// 面の向きから明るさを決める。`lights()` を呼んでいなければ素の色。
    fn shade(&self, color: Color, normal: [f32; 3]) -> Color {
        let Some(space) = &self.space else { return color };
        if !space.lights {
            return color;
        }
        // 既定の lights() は環境光 128 と、視点から差す平行光 128。
        const LEVEL: f32 = 128.0 / 255.0;
        let n = crate::mat4::normalize(space.model.direction(normal));
        let level = (LEVEL + LEVEL * n[2].max(0.0)).min(1.0);
        Color { r: color.r * level, g: color.g * level, b: color.b * level, a: color.a }
    }

    /// `box()`。
    pub fn draw_box(&mut self, w: f32, h: f32, d: f32) {
        self.ensure_3d();
        if w == 0.0 && h == 0.0 && d == 0.0 {
            return;
        }
        let (x, y, z) = (w * 0.5, h * 0.5, d * 0.5);
        // 8 隅。下位ビットが x、次が y、その次が z の符号。
        let corner = |i: usize| {
            [
                if i & 1 == 0 { -x } else { x },
                if i & 2 == 0 { -y } else { y },
                if i & 4 == 0 { -z } else { z },
            ]
        };
        // 6 面。四隅の番号と外向きの法線。
        const FACES: [([usize; 4], [f32; 3]); 6] = [
            ([0, 2, 6, 4], [-1.0, 0.0, 0.0]),
            ([1, 5, 7, 3], [1.0, 0.0, 0.0]),
            ([0, 4, 5, 1], [0.0, -1.0, 0.0]),
            ([2, 3, 7, 6], [0.0, 1.0, 0.0]),
            ([0, 1, 3, 2], [0.0, 0.0, -1.0]),
            ([4, 6, 7, 5], [0.0, 0.0, 1.0]),
        ];
        self.solid(&FACES.map(|(idx, n)| (idx.map(corner), n)));
    }

    /// `sphere()`。緯度と経度で分ける。
    pub fn sphere(&mut self, radius: f32) {
        self.ensure_3d();
        if radius <= 0.0 {
            return;
        }
        // Processing の既定は 30 分割。小さい球にそこまでは要らない。
        let rings = ((radius * 0.5) as usize).clamp(6, 24);
        let segments = rings * 2;
        let point = |ring: usize, seg: usize| {
            let phi = std::f32::consts::PI * ring as f32 / rings as f32;
            let theta = std::f32::consts::TAU * seg as f32 / segments as f32;
            [phi.sin() * theta.cos(), phi.cos(), phi.sin() * theta.sin()]
        };
        let mut faces = Vec::with_capacity(rings * segments);
        for ring in 0..rings {
            for seg in 0..segments {
                let quad = [
                    point(ring, seg),
                    point(ring, seg + 1),
                    point(ring + 1, seg + 1),
                    point(ring + 1, seg),
                ];
                // 球では法線が位置とそのまま同じ向き。4 隅の平均で足りる。
                let mut n = [0.0f32; 3];
                for p in quad {
                    for (a, b) in n.iter_mut().zip(p) {
                        *a += b;
                    }
                }
                faces.push((quad.map(|p| [p[0] * radius, p[1] * radius, p[2] * radius]), n));
            }
        }
        self.solid(&faces);
    }

    /// 面の並びを塗って縁取る。ローカル座標で受け取る。
    fn solid(&mut self, faces: &[([[f32; 3]; 4], [f32; 3])]) {
        let Some(space) = &self.space else { return };
        let model = space.model;
        let was = std::mem::replace(&mut self.depth_write, true);

        if let Some(fill) = self.fill {
            for (quad, normal) in faces {
                let color = self.shade(fill, *normal);
                let eye = quad.map(|p| model.point(p));
                self.face([eye[0], eye[1], eye[2]], color);
                self.face([eye[0], eye[2], eye[3]], color);
            }
        }
        if let Some(stroke) = self.stroke {
            for (quad, _) in faces {
                let eye = quad.map(|p| model.point(p));
                for i in 0..4 {
                    self.edge(eye[i], eye[(i + 1) % 4], stroke);
                }
            }
        }
        self.depth_write = was;
    }

    // ---- 座標変換 -------------------------------------------------------

    /// `pushStyle()`。見た目だけを退避する。座標変換はそのまま。
    pub fn push_style(&mut self) {
        self.styles.push(Style {
            fill: self.fill,
            stroke: self.stroke,
            stroke_weight: self.stroke_weight,
            text_size: self.text_size,
            text_align: self.text_align,
            rect_mode: self.rect_mode,
            ellipse_mode: self.ellipse_mode,
            angle_mode: self.angle_mode,
            color_mode: self.color_mode,
            color_max: self.color_max,
            blend: self.blend,
            shadow: self.shadow,
        });
    }

    /// `popStyle()`。
    pub fn pop_style(&mut self) {
        let Some(s) = self.styles.pop() else { return };
        self.fill = s.fill;
        self.stroke = s.stroke;
        self.stroke_weight = s.stroke_weight;
        self.text_size = s.text_size;
        self.text_align = s.text_align;
        self.rect_mode = s.rect_mode;
        self.ellipse_mode = s.ellipse_mode;
        self.angle_mode = s.angle_mode;
        self.color_mode = s.color_mode;
        self.color_max = s.color_max;
        self.blend = s.blend;
        self.shadow = s.shadow;
    }

    /// p5.js の `push()`。座標変換と見た目の両方を退避する。
    ///
    /// Processing の `pushMatrix()` とは違い、塗りや線の色まで戻る。
    /// `drawingContext` の影も同じ扱い (p5 は canvas の `save()` を呼ぶ)。
    pub fn push_all(&mut self) {
        self.push_matrix();
        self.push_style();
    }

    /// p5.js の `pop()`。
    pub fn pop_all(&mut self) {
        self.pop_style();
        self.pop_matrix();
    }

    pub fn push_matrix(&mut self) {
        match &mut self.space {
            Some(space) => space.stack.push(space.model),
            None => self.stack.push(self.matrix),
        }
    }

    /// 座標変換を初期状態へ戻す。`resetMatrix()` 相当。
    pub fn reset_matrix(&mut self) {
        match &mut self.space {
            // 3D ではカメラごと消える。Processing と同じ。
            Some(space) => space.model = Mat4::IDENTITY,
            None => self.matrix = self.base,
        }
    }

    pub fn pop_matrix(&mut self) {
        match &mut self.space {
            Some(space) => {
                if let Some(m) = space.stack.pop() {
                    space.model = m;
                }
            }
            None => {
                if let Some(m) = self.stack.pop() {
                    self.matrix = m;
                }
            }
        }
    }

    pub fn translate(&mut self, x: f32, y: f32) {
        match &mut self.space {
            Some(space) => space.model = space.model.then_local(Mat4::translation(x, y, 0.0)),
            None => self.matrix = self.matrix.then_local(Affine { e: x, f: y, ..Affine::IDENTITY }),
        }
    }

    /// 3 引数の `translate()`。2D の作品では奥行きを捨てる。
    pub fn translate_3d(&mut self, x: f32, y: f32, z: f32) {
        self.ensure_3d();
        if let Some(space) = &mut self.space {
            space.model = space.model.then_local(Mat4::translation(x, y, z));
        }
    }

    pub fn rotate(&mut self, angle: f32) {
        match &mut self.space {
            Some(space) => space.model = space.model.then_local(Mat4::rotation_z(angle)),
            None => {
                let (s, c) = angle.sin_cos();
                self.matrix =
                    self.matrix.then_local(Affine { a: c, b: s, c: -s, d: c, e: 0.0, f: 0.0 });
            }
        }
    }

    /// `rotateX()` / `rotateY()` / `rotateZ()`。
    pub fn rotate_axis(&mut self, angle: f32, axis: [f32; 3]) {
        self.ensure_3d();
        if let Some(space) = &mut self.space {
            let m = Mat4::rotation_axis(angle, axis[0], axis[1], axis[2]);
            space.model = space.model.then_local(m);
        }
    }

    pub fn scale(&mut self, sx: f32, sy: f32) {
        match &mut self.space {
            Some(space) => space.model = space.model.then_local(Mat4::scaling(sx, sy, 1.0)),
            None => self.matrix = self.matrix.then_local(Affine { a: sx, d: sy, ..Affine::IDENTITY }),
        }
    }

    /// 3 引数の `scale()`。
    pub fn scale_3d(&mut self, sx: f32, sy: f32, sz: f32) {
        self.ensure_3d();
        if let Some(space) = &mut self.space {
            space.model = space.model.then_local(Mat4::scaling(sx, sy, sz));
        }
    }

    // ---- 図形 -----------------------------------------------------------

    /// `rect()`。引数の意味は `rectMode()` に従う。
    pub fn rect(&mut self, a: f32, b: f32, c1: f32, d: f32) {
        let (x, y, w, h) = self.rect_mode.to_corner(a, b, c1, d);
        self.rect_corner(x, y, w, h);
    }

    /// 角の丸い `rect()`。`radii` は左上から時計回り。
    pub fn rect_rounded(&mut self, a: f32, b: f32, c: f32, d: f32, radii: [f32; 4]) {
        let (x, y, w, h) = self.rect_mode.to_corner(a, b, c, d);
        // 半径は辺の半分まで。それ以上は意味を持たない。
        let limit = (w.abs().min(h.abs())) * 0.5;
        let r = radii.map(|v| v.clamp(0.0, limit));
        if r.iter().all(|v| *v <= 0.0) {
            self.rect_corner(x, y, w, h);
            return;
        }

        // 角ごとに円弧を刻んで、閉じた輪郭を作る。
        let mut points: Vec<[f32; 2]> = Vec::new();
        let corners = [
            // (中心, 開始角)。左上から時計回り。
            ([x + r[0], y + r[0]], std::f32::consts::PI),
            ([x + w - r[1], y + r[1]], -std::f32::consts::FRAC_PI_2),
            ([x + w - r[2], y + h - r[2]], 0.0),
            ([x + r[3], y + h - r[3]], std::f32::consts::FRAC_PI_2),
        ];
        for (i, (center, start)) in corners.iter().enumerate() {
            let radius = r[i];
            if radius <= 0.0 {
                // 角が丸くないなら頂点をひとつ置くだけ。
                let sharp = match i {
                    0 => [x, y],
                    1 => [x + w, y],
                    2 => [x + w, y + h],
                    _ => [x, y + h],
                };
                points.push(sharp);
                continue;
            }
            let steps = ((radius * self.scale_hint() * 0.5) as usize).clamp(3, 16);
            for step in 0..=steps {
                let t = start + std::f32::consts::FRAC_PI_2 * (step as f32 / steps as f32);
                points.push([center[0] + radius * t.cos(), center[1] + radius * t.sin()]);
            }
        }

        // 角丸の四角は凸なので、中心からの扇で塗れる。耳切りより速い。
        if let Some(color) = self.fill {
            let center = self.pt(x + w * 0.5, y + h * 0.5);
            let base = self.list.vertices.len() as u32;
            self.push_vertex(center, color);
            for p in &points {
                let at = self.pt(p[0], p[1]);
                self.push_vertex(at, color);
            }
            let n = points.len() as u32;
            for i in 0..n {
                self.emit(&[base, base + 1 + i, base + 1 + (i + 1) % n]);
            }
        }

        if self.stroke.is_some() && self.stroke_weight > 0.0 {
            for i in 0..points.len() {
                let from = points[i];
                let to = points[(i + 1) % points.len()];
                self.line(from[0], from[1], to[0], to[1]);
            }
        }
    }

    /// `ellipse()`。引数の意味は `ellipseMode()` に従う。
    pub fn ellipse(&mut self, a: f32, b: f32, c: f32, d: f32) {
        let (cx, cy, w, h) = self.ellipse_mode.to_center(a, b, c, d);
        self.ellipse_center(cx, cy, w, h);
    }

    /// 四角形を 4 点で描く。`quad()`。凹んでいてもよい。
    #[allow(clippy::too_many_arguments)]
    pub fn quad_points(
        &mut self,
        x1: f32,
        y1: f32,
        x2: f32,
        y2: f32,
        x3: f32,
        y3: f32,
        x4: f32,
        y4: f32,
    ) {
        let points = [[x1, y1], [x2, y2], [x3, y3], [x4, y4]];
        self.polygon(&points, true);
    }

    fn rect_corner(&mut self, x: f32, y: f32, w: f32, h: f32) {
        if let Some(c) = self.fill {
            self.quad([x, y], [x + w, y], [x + w, y + h], [x, y + h], c);
        }
        if self.stroke.is_some() && self.stroke_weight > 0.0 {
            self.line(x, y, x + w, y);
            self.line(x + w, y, x + w, y + h);
            self.line(x + w, y + h, x, y + h);
            self.line(x, y + h, x, y);
        }
    }

    fn ellipse_center(&mut self, cx: f32, cy: f32, w: f32, h: f32) {
        let rx = w * 0.5;
        let ry = h * 0.5;
        let segments = self.circle_segments(rx.abs().max(ry.abs()));

        if let Some(c) = self.fill {
            let center = self.pt(cx, cy);
            let base = self.list.vertices.len() as u32;
            self.push_vertex(center, c);
            for i in 0..=segments {
                let t = i as f32 / segments as f32 * std::f32::consts::TAU;
                let p = self.pt(cx + rx * t.cos(), cy + ry * t.sin());
                self.push_vertex(p, c);
            }
            for i in 0..segments {
                self.emit(&[base, base + 1 + i as u32, base + 2 + i as u32]);
            }
        }

        if self.stroke.is_some() && self.stroke_weight > 0.0 {
            let mut prev = (cx + rx, cy);
            for i in 1..=segments {
                let t = i as f32 / segments as f32 * std::f32::consts::TAU;
                let next = (cx + rx * t.cos(), cy + ry * t.sin());
                self.line(prev.0, prev.1, next.0, next.1);
                prev = next;
            }
        }
    }

    /// `circle()`。`ellipseMode()` の影響を受けない中心指定。
    pub fn circle(&mut self, cx: f32, cy: f32, diameter: f32) {
        self.ellipse_center(cx, cy, diameter, diameter);
    }

    /// `square()`。`rectMode()` に従う。
    pub fn square(&mut self, a: f32, b: f32, size: f32) {
        self.rect(a, b, size, size);
    }

    pub fn triangle(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x3: f32, y3: f32) {
        if let Some(c) = self.fill {
            let p1 = self.pt(x1, y1);
            let p2 = self.pt(x2, y2);
            let p3 = self.pt(x3, y3);
            self.tri(p1, p2, p3, c);
        }
        if self.stroke.is_some() && self.stroke_weight > 0.0 {
            self.line(x1, y1, x2, y2);
            self.line(x2, y2, x3, y3);
            self.line(x3, y3, x1, y1);
        }
    }

    /// 線分。太さはローカル空間で与えてから変換するため、`scale()` に追従する。
    pub fn line(&mut self, x1: f32, y1: f32, x2: f32, y2: f32) {
        let Some(c) = self.stroke else { return };
        if self.stroke_weight <= 0.0 {
            return;
        }
        let (dx, dy) = (x2 - x1, y2 - y1);
        let len = (dx * dx + dy * dy).sqrt();
        if len < 1e-6 {
            self.point(x1, y1);
            return;
        }
        // 画面上で 1px を下回る線も見えるように、変換後の太さで下限を設ける。
        let hw = (self.stroke_weight * 0.5).max(0.5 / self.scale_hint());
        let (nx, ny) = (-dy / len * hw, dx / len * hw);
        self.quad(
            [x1 + nx, y1 + ny],
            [x2 + nx, y2 + ny],
            [x2 - nx, y2 - ny],
            [x1 - nx, y1 - ny],
            c,
        );
        // 端は丸。Processing も p5.js も既定はこれで、太い線を折り返す作品では
        // 継ぎ目が角ばるかどうかがそのまま見た目に出る。
        //
        // 細い線には付けない。見た目は変わらないのに、三角形だけ 3 倍に増える。
        // 線を何万本も引く作品があるので、ここは効く。
        if hw * self.scale_hint() > 1.5 {
            self.round_cap(x1, y1, hw, c);
            self.round_cap(x2, y2, hw, c);
        }
    }

    // ---- 自由な形 (設計書 §14.2) ----------------------------------------

    /// `beginShape()`。以降の [`Graphics::vertex`] を貯め始める。
    pub fn begin_shape(&mut self, kind: ShapeKind) {
        self.shape = Some(Shape { kind, points: Vec::new(), curves: Vec::new() });
    }

    /// `vertex(x, y)`。`beginShape()` の外で呼ばれたら黙って捨てる。
    pub fn vertex(&mut self, x: f32, y: f32) {
        if let Some(shape) = &mut self.shape {
            // 際限なく貯めると 1 フレームでメモリを食うので上限を置く。
            if shape.points.len() < MAX_SHAPE_POINTS {
                shape.points.push([x, y]);
            }
        }
    }

    /// `curveVertex(x, y)`。制御点として貯め、`endShape()` で曲線に変える。
    ///
    /// Processing と同じく、最初と最後の 1 点は曲線の向きを決めるためだけに
    /// 使われ、線そのものには現れない。
    pub fn curve_vertex(&mut self, x: f32, y: f32) {
        if let Some(shape) = &mut self.shape
            && shape.curves.len() < MAX_SHAPE_POINTS
        {
            shape.curves.push([x, y]);
        }
    }

    /// `bezierVertex()`。直前の頂点から 3 次ベジェで繋ぐ。
    #[allow(clippy::too_many_arguments)]
    pub fn bezier_vertex(&mut self, cx1: f32, cy1: f32, cx2: f32, cy2: f32, x: f32, y: f32) {
        let Some(shape) = &self.shape else { return };
        let Some(&from) = shape.points.last() else { return };

        // 曲線を折れ線へ均して、そのまま頂点として足す。塗りにも縁取りにも
        // 同じ点列が使えるので、扱いが 1 本になる。
        let span = (from[0] - x).abs().max((from[1] - y).abs());
        let segments = ((span * 0.35) as usize).clamp(8, 64);
        let at = |t: f32| {
            let u = 1.0 - t;
            let (a, b, c, d) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
            [
                a * from[0] + b * cx1 + c * cx2 + d * x,
                a * from[1] + b * cy1 + c * cy2 + d * y,
            ]
        };
        for i in 1..=segments {
            let p = at(i as f32 / segments as f32);
            self.vertex(p[0], p[1]);
        }
    }

    /// `curve()`。4 つの制御点で Catmull-Rom 曲線を 1 本引く。
    #[allow(clippy::too_many_arguments)]
    pub fn curve(
        &mut self,
        x1: f32,
        y1: f32,
        x2: f32,
        y2: f32,
        x3: f32,
        y3: f32,
        x4: f32,
        y4: f32,
    ) {
        if self.stroke.is_none() || self.stroke_weight <= 0.0 {
            return;
        }
        let points = catmull_rom(&[[x1, y1], [x2, y2], [x3, y3], [x4, y4]]);
        for pair in points.windows(2) {
            self.line(pair[0][0], pair[0][1], pair[1][0], pair[1][1]);
        }
    }

    /// `endShape()`。貯めた頂点を実際に描く。`close` は `endShape(CLOSE)`。
    pub fn end_shape(&mut self, close: bool) {
        let Some(shape) = self.shape.take() else { return };

        // curveVertex() で作った点は、ここで曲線へ均してから通常の頂点と繋ぐ。
        let mut points = shape.points;
        if shape.curves.len() >= 4 {
            points.extend(catmull_rom(&shape.curves));
        }
        if points.is_empty() {
            return;
        }

        match shape.kind {
            ShapeKind::Points => {
                for p in &points {
                    self.point(p[0], p[1]);
                }
            }
            ShapeKind::Lines => {
                for pair in points.chunks_exact(2) {
                    self.line(pair[0][0], pair[0][1], pair[1][0], pair[1][1]);
                }
            }
            ShapeKind::Triangles => {
                for tri in points.chunks_exact(3) {
                    self.triangle(
                        tri[0][0], tri[0][1], tri[1][0], tri[1][1], tri[2][0], tri[2][1],
                    );
                }
            }
            ShapeKind::TriangleStrip => {
                for i in 2..points.len() {
                    let (a, b, c) = (points[i - 2], points[i - 1], points[i]);
                    self.triangle(a[0], a[1], b[0], b[1], c[0], c[1]);
                }
            }
            ShapeKind::TriangleFan => {
                for i in 2..points.len() {
                    let (a, b, c) = (points[0], points[i - 1], points[i]);
                    self.triangle(a[0], a[1], b[0], b[1], c[0], c[1]);
                }
            }
            ShapeKind::Polygon => self.polygon(&points, close),
        }
    }

    /// 多角形を塗って縁取る。凹んでいてもよい。
    fn polygon(&mut self, points: &[[f32; 2]], close: bool) {
        if let Some(c) = self.fill
            && points.len() >= 3
        {
            // 凹みのある形も塗れるよう、耳切り法で三角形へ分ける。
            // 扇状に分けると、凹んだところが外へはみ出す。
            for [a, b, cc] in triangulate(points) {
                let p1 = self.pt(points[a][0], points[a][1]);
                let p2 = self.pt(points[b][0], points[b][1]);
                let p3 = self.pt(points[cc][0], points[cc][1]);
                self.tri(p1, p2, p3, c);
            }
        }

        if self.stroke.is_some() && self.stroke_weight > 0.0 {
            for pair in points.windows(2) {
                self.line(pair[0][0], pair[0][1], pair[1][0], pair[1][1]);
            }
            // 塗るときは閉じた形として扱うが、線は指示があったときだけ閉じる。
            if close && points.len() >= 3 {
                let (first, last) = (points[0], points[points.len() - 1]);
                self.line(last[0], last[1], first[0], first[1]);
            }
        }
    }

    // ---- 文字 (設計書 §14.2) --------------------------------------------

    /// `textSize()`。
    /// `text()` に線を付けるか。p5.js は付け、Processing は付けない。
    ///
    /// 作品を読み込んだ側が方言に合わせて決める。
    pub fn set_text_stroked(&mut self, on: bool) {
        self.text_stroked = on;
    }

    /// `background()` を一度も呼ばない作品の下地。
    ///
    /// Processing は灰 204 で始まる。p5.js のキャンバスは透明で、後ろの
    /// ページの白が透ける。半透明を塗り重ねる作品では、この下地の色が
    /// そのまま画面全体の明るさになる。
    pub fn set_default_background(&mut self, color: Color) {
        self.default_background = color;
    }

    pub fn default_background(&self) -> Color {
        self.default_background
    }

    pub fn set_text_size(&mut self, size: f32) {
        self.text_size = size.max(0.0);
    }

    pub fn text_size(&self) -> f32 {
        self.text_size
    }

    /// `textAlign()`。
    pub fn set_text_align(&mut self, horizontal: TextAlign, vertical: TextAlign) {
        self.text_align = (horizontal, vertical);
    }

    /// 文字列を描いたときの幅。`textWidth()`。
    pub fn measure(&mut self, text: &str) -> f32 {
        let mut width = 0.0;
        for ch in text.chars() {
            if let Some(g) = self.font.glyph(ch) {
                width += g.advance;
            }
        }
        width * self.text_size
    }

    /// `text(str, x, y)`。フォントが無いときは何も描かない。
    pub fn text(&mut self, text: &str, x: f32, y: f32) {
        // p5.js の text() は塗りと線の両方を使う。線が付かないと、白い地に
        // 白い字を置く作品が消えてしまう。Processing の text() は塗りだけ。
        if self.text_stroked
            && let Some(edge) = self.stroke
            && self.stroke_weight > 0.0
        {
            // 字形は塗りつぶした形しか持っていないので、少しずつずらして
            // 重ね、はみ出したぶんを縁にする。
            let r = self.stroke_weight * 0.5;
            let fill = self.fill.take();
            self.fill = Some(edge);
            self.text_stroked = false;
            for i in 0..8 {
                let a = std::f32::consts::TAU * i as f32 / 8.0;
                self.text(text, x + a.cos() * r, y + a.sin() * r);
            }
            self.text_stroked = true;
            self.fill = fill;
        }

        let Some(color) = self.fill else { return };
        if !self.font.has_font() || self.text_size <= 0.0 {
            return;
        }

        // 揃え方に合わせて基準点をずらす。
        let width = self.measure(text);
        let x = match self.text_align.0 {
            TextAlign::Center => x - width * 0.5,
            TextAlign::End => x - width,
            _ => x,
        };
        // 縦は大まかな比率で寄せる。字形ごとの高さは使わない。
        let y = match self.text_align.1 {
            TextAlign::Start => y + self.text_size * 0.8,
            TextAlign::Center => y + self.text_size * 0.32,
            TextAlign::End => y,
            TextAlign::Baseline => y,
        };

        let mut pen = x;
        for ch in text.chars() {
            // 改行は次の行へ。行送りは大きさの 1.2 倍。
            if ch == '\n' {
                continue;
            }
            let Some(glyph) = self.font.glyph(ch) else { continue };
            if glyph.size[0] > 0.0 && glyph.size[1] > 0.0 {
                let gx = pen + glyph.offset[0] * self.text_size;
                let gy = y + glyph.offset[1] * self.text_size;
                let gw = glyph.size[0] * self.text_size;
                let gh = glyph.size[1] * self.text_size;
                let uv = glyph.uv;

                let p = [
                    self.pt(gx, gy),
                    self.pt(gx + gw, gy),
                    self.pt(gx + gw, gy + gh),
                    self.pt(gx, gy + gh),
                ];
                let base = self.list.vertices.len() as u32;
                self.push_textured(p[0], color, [uv[0], uv[1]]);
                self.push_textured(p[1], color, [uv[2], uv[1]]);
                self.push_textured(p[2], color, [uv[2], uv[3]]);
                self.push_textured(p[3], color, [uv[0], uv[3]]);
                self.emit(&[base, base + 1, base + 2, base, base + 2, base + 3]);
            }
            pen += glyph.advance * self.text_size;
        }
    }

    /// `arc(x, y, w, h, start, stop)`。
    ///
    /// 塗りは中心を含む扇形、線は弧そのもの。Processing の既定 (`OPEN`) と同じ。
    pub fn arc(&mut self, cx: f32, cy: f32, w: f32, h: f32, start: f32, stop: f32) {
        self.arc_mode(cx, cy, w, h, start, stop, ArcMode::default());
    }

    /// 閉じ方を指定する `arc()`。
    #[allow(clippy::too_many_arguments)]
    pub fn arc_mode(
        &mut self,
        cx: f32,
        cy: f32,
        w: f32,
        h: f32,
        start: f32,
        stop: f32,
        mode: ArcMode,
    ) {
        let (rx, ry) = (w * 0.5, h * 0.5);
        let sweep = stop - start;
        if sweep.abs() < 1e-6 {
            return;
        }
        // 全周のときと同じ密度になるよう、角度に比例した分割数にする。
        let full = self.circle_segments(rx.abs().max(ry.abs())) as f32;
        let segments =
            ((full * (sweep.abs() / std::f32::consts::TAU)).ceil() as usize).clamp(2, 512);

        let at = |i: usize| {
            let t = start + sweep * (i as f32 / segments as f32);
            (cx + rx * t.cos(), cy + ry * t.sin())
        };

        if let Some(c) = self.fill {
            // 扇形は中心から、弦で閉じるものは弧の始点から扇状に張る。
            let hub = if mode == ArcMode::Pie { (cx, cy) } else { at(0) };
            let hub = self.pt(hub.0, hub.1);
            let base = self.list.vertices.len() as u32;
            self.push_vertex(hub, c);
            for i in 0..=segments {
                let (x, y) = at(i);
                let p = self.pt(x, y);
                self.push_vertex(p, c);
            }
            for i in 0..segments {
                self.emit(&[base, base + 1 + i as u32, base + 2 + i as u32]);
            }
        }

        if self.stroke.is_some() && self.stroke_weight > 0.0 {
            let mut prev = at(0);
            for i in 1..=segments {
                let next = at(i);
                self.line(prev.0, prev.1, next.0, next.1);
                prev = next;
            }
            // 閉じ方によって、戻りの線が要る。
            let (first, last) = (at(0), at(segments));
            match mode {
                ArcMode::Open => {}
                ArcMode::Chord => self.line(last.0, last.1, first.0, first.1),
                ArcMode::Pie => {
                    self.line(last.0, last.1, cx, cy);
                    self.line(cx, cy, first.0, first.1);
                }
            }
        }
    }

    /// `bezier()`。3 次ベジェを折れ線として引く。
    #[allow(clippy::too_many_arguments)]
    pub fn bezier(
        &mut self,
        x1: f32,
        y1: f32,
        x2: f32,
        y2: f32,
        x3: f32,
        y3: f32,
        x4: f32,
        y4: f32,
    ) {
        if self.stroke.is_none() || self.stroke_weight <= 0.0 {
            return;
        }
        // 制御点の広がりから分割数を決める。小さい曲線に無駄な頂点を置かない。
        let span = (x1 - x4).abs().max((y1 - y4).abs()).max((x2 - x3).abs()).max((y2 - y3).abs());
        let segments = ((span * self.scale_hint() * 0.25) as usize).clamp(8, 128);

        let at = |t: f32| {
            let u = 1.0 - t;
            let (a, b, c, d) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
            (a * x1 + b * x2 + c * x3 + d * x4, a * y1 + b * y2 + c * y3 + d * y4)
        };

        let mut prev = at(0.0);
        for i in 1..=segments {
            let next = at(i as f32 / segments as f32);
            self.line(prev.0, prev.1, next.0, next.1);
            prev = next;
        }
    }

    /// `point()`。太さのぶんの丸い点。
    ///
    /// Processing も p5.js も、点は線の端と同じ丸で描く。四角で描くと、
    /// 太い点をばらまく作品が角ばって見える。
    pub fn point(&mut self, x: f32, y: f32) {
        let Some(c) = self.stroke else { return };
        let hw = (self.stroke_weight * 0.5).max(0.5 / self.scale_hint());
        self.round_cap(x, y, hw, c);
    }

    /// 中心と半径で丸を 1 つ。線の端と `point()` に使う。
    ///
    /// 半径が小さいうちは四角で済ませる。1px の点に扇形を張るのは無駄だし、
    /// 見た目も変わらない。
    fn round_cap(&mut self, x: f32, y: f32, r: f32, c: Color) {
        let screen = r * self.scale_hint();
        if screen <= 0.75 {
            self.quad([x - r, y - r], [x + r, y - r], [x + r, y + r], [x - r, y + r], c);
            return;
        }
        let steps = ((screen * 2.0) as usize).clamp(6, 32);
        let center = self.pt(x, y);
        let base = self.list.vertices.len() as u32;
        self.push_vertex(center, c);
        for i in 0..steps {
            let a = std::f32::consts::TAU * i as f32 / steps as f32;
            let p = self.pt(x + a.cos() * r, y + a.sin() * r);
            self.push_vertex(p, c);
        }
        for i in 0..steps as u32 {
            self.emit(&[base, base + 1 + i, base + 1 + (i + 1) % steps as u32]);
        }
    }

    // ---- 内部ヘルパ -----------------------------------------------------

    fn circle_segments(&self, radius: f32) -> usize {
        let screen_radius = radius * self.scale_hint();
        // 弧の 1 セグメントが数ピクセルに収まるようにする。細い輪郭線は継ぎ目が
        // 目立ちやすいので、塗りだけのときより多めに割る。
        ((screen_radius * 1.6) as usize).clamp(16, 256)
    }

    /// ローカル座標 4 点を変換して 2 三角形にする。
    fn quad(&mut self, p0: [f32; 2], p1: [f32; 2], p2: [f32; 2], p3: [f32; 2], c: Color) {
        let a = self.pt(p0[0], p0[1]);
        let b = self.pt(p1[0], p1[1]);
        let cc = self.pt(p2[0], p2[1]);
        let d = self.pt(p3[0], p3[1]);
        let base = self.list.vertices.len() as u32;
        self.push_vertex(a, c);
        self.push_vertex(b, c);
        self.push_vertex(cc, c);
        self.push_vertex(d, c);
        self.emit(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }

    /// 変換済みの 3 点。
    fn tri(&mut self, a: [f32; 3], b: [f32; 3], c: [f32; 3], color: Color) {
        let base = self.list.vertices.len() as u32;
        self.push_vertex(a, color);
        self.push_vertex(b, color);
        self.push_vertex(c, color);
        self.emit(&[base, base + 1, base + 2]);
    }

    /// 三角形を足す。いまの合成方法の区間へ入れる。
    fn emit(&mut self, indices: &[u32]) {
        let start = self.list.indices.len() as u32;
        self.list.indices.extend_from_slice(indices);
        let end = self.list.indices.len() as u32;

        match self.list.batches.last_mut() {
            // 合成方法が同じなら続きとして伸ばす。
            Some(last)
                if last.blend == self.blend
                    && last.depth == self.depth_write
                    && last.end == start =>
            {
                last.end = end
            }
            _ => self.list.batches.push(Batch {
                blend: self.blend,
                depth: self.depth_write,
                start,
                end,
            }),
        }
    }

    fn push_vertex(&mut self, pos: [f32; 3], color: Color) {
        // 図形はアトラスの白い点を指す。文字と同じ経路で描けるようにするため。
        self.list.vertices.push(Vertex {
            pos,
            color: color.to_array(),
            uv: crate::font::FontAtlas::white_uv(),
        });
    }

    fn push_textured(&mut self, pos: [f32; 3], color: Color, uv: [f32; 2]) {
        self.list.vertices.push(Vertex { pos, color: color.to_array(), uv });
    }
}

/// HSB (色相は 0..1 で巡回) から sRGB へ。
fn hsb_to_color(hue: f32, saturation: f32, brightness: f32, alpha: f32) -> Color {
    let h = (hue.fract() + 1.0).fract() * 6.0;
    let i = h.floor();
    let f = h - i;
    let p = brightness * (1.0 - saturation);
    let q = brightness * (1.0 - saturation * f);
    let t = brightness * (1.0 - saturation * (1.0 - f));

    let (r, g, b) = match i as i32 % 6 {
        0 => (brightness, t, p),
        1 => (q, brightness, p),
        2 => (p, brightness, t),
        3 => (p, q, brightness),
        4 => (t, p, brightness),
        _ => (brightness, p, q),
    };
    Color::rgba(r, g, b, alpha)
}

impl Default for Graphics {
    fn default() -> Self {
        Self::new()
    }
}

/// 制御点の並びを Catmull-Rom 曲線の折れ線へ均す。
///
/// 最初と最後の点は向きを決めるためだけに使い、線には現れない。Processing の
/// `curveVertex()` と同じ決まり。
fn catmull_rom(control: &[[f32; 2]]) -> Vec<[f32; 2]> {
    if control.len() < 4 {
        return Vec::new();
    }
    let mut out = Vec::new();
    for w in control.windows(4) {
        let (p0, p1, p2, p3) = (w[0], w[1], w[2], w[3]);
        // 区間の長さから分割数を決める。短い区間に無駄な点を置かない。
        let span = (p2[0] - p1[0]).abs().max((p2[1] - p1[1]).abs());
        let segments = ((span * 0.35) as usize).clamp(6, 48);
        for i in 0..=segments {
            // 端で重複しないよう、2 本目以降は始点を飛ばす。
            if i == 0 && !out.is_empty() {
                continue;
            }
            let t = i as f32 / segments as f32;
            let (t2, t3) = (t * t, t * t * t);
            let axis = |a: f32, b: f32, c: f32, d: f32| {
                0.5 * ((2.0 * b)
                    + (-a + c) * t
                    + (2.0 * a - 5.0 * b + 4.0 * c - d) * t2
                    + (-a + 3.0 * b - 3.0 * c + d) * t3)
            };
            out.push([
                axis(p0[0], p1[0], p2[0], p3[0]),
                axis(p0[1], p1[1], p2[1], p3[1]),
            ]);
        }
    }
    out
}

/// 多角形を三角形へ分ける (耳切り法)。返すのは `points` の添字。
///
/// 凹んだ形を扇状に分けると外へはみ出すので、耳を 1 つずつ落としていく。
/// 自己交差する形は正しく分けられないが、落ちはしない。
fn triangulate(points: &[[f32; 2]]) -> Vec<[usize; 3]> {
    let n = points.len();
    if n < 3 {
        return Vec::new();
    }

    let mut remaining: Vec<usize> = (0..n).collect();
    // 時計回りでも反時計回りでも同じ手順で扱えるよう、向きを揃える。
    if signed_area(points) < 0.0 {
        remaining.reverse();
    }

    let mut out = Vec::with_capacity(n.saturating_sub(2));
    // 耳が見つからない形 (自己交差など) で止まらないよう、空回りの回数を数える。
    let mut stuck = 0;
    while remaining.len() > 3 {
        if stuck > remaining.len() {
            break;
        }
        let mut cut = None;
        for i in 0..remaining.len() {
            let (a, b, c) = (
                remaining[(i + remaining.len() - 1) % remaining.len()],
                remaining[i],
                remaining[(i + 1) % remaining.len()],
            );
            if !is_convex(points[a], points[b], points[c]) {
                continue;
            }
            // 他の頂点を含んでいたら耳ではない。
            let contains = remaining.iter().any(|&j| {
                j != a
                    && j != b
                    && j != c
                    && point_in_triangle(points[j], points[a], points[b], points[c])
            });
            if !contains {
                cut = Some((i, [a, b, c]));
                break;
            }
        }
        match cut {
            Some((i, tri)) => {
                out.push(tri);
                remaining.remove(i);
                stuck = 0;
            }
            None => stuck += 1,
        }
    }
    if remaining.len() == 3 {
        out.push([remaining[0], remaining[1], remaining[2]]);
    }
    out
}

fn signed_area(points: &[[f32; 2]]) -> f32 {
    let mut sum = 0.0;
    for i in 0..points.len() {
        let a = points[i];
        let b = points[(i + 1) % points.len()];
        sum += a[0] * b[1] - b[0] * a[1];
    }
    sum * 0.5
}

fn cross(o: [f32; 2], a: [f32; 2], b: [f32; 2]) -> f32 {
    (a[0] - o[0]) * (b[1] - o[1]) - (a[1] - o[1]) * (b[0] - o[0])
}

fn is_convex(a: [f32; 2], b: [f32; 2], c: [f32; 2]) -> bool {
    cross(b, a, c) < 0.0
}

fn point_in_triangle(p: [f32; 2], a: [f32; 2], b: [f32; 2], c: [f32; 2]) -> bool {
    let d1 = cross(a, b, p);
    let d2 = cross(b, c, p);
    let d3 = cross(c, a, p);
    let has_neg = d1 < 0.0 || d2 < 0.0 || d3 < 0.0;
    let has_pos = d1 > 0.0 || d2 > 0.0 || d3 > 0.0;
    !(has_neg && has_pos)
}

#[cfg(test)]
mod tests {

    /// 三角形に分けた面積が、元の多角形と合うか。
    ///
    /// 凹んだ形を扇状に分けると、はみ出した分だけ面積が変わる。面積が合えば
    /// はみ出していない。
    fn area_of(points: &[[f32; 2]]) -> f32 {
        triangulate(points)
            .iter()
            .map(|[a, b, c]| {
                let (p, q, r) = (points[*a], points[*b], points[*c]);
                ((q[0] - p[0]) * (r[1] - p[1]) - (q[1] - p[1]) * (r[0] - p[0])).abs() * 0.5
            })
            .sum()
    }

    #[test]
    fn a_convex_polygon_is_split_into_n_minus_two() {
        let square = [[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]];
        assert_eq!(triangulate(&square).len(), 2);
        assert!((area_of(&square) - 100.0).abs() < 0.01, "{}", area_of(&square));
    }

    #[test]
    fn a_concave_polygon_does_not_spill_outside() {
        // 上辺の真ん中が内側へ凹んだ形。扇状に分けると凹みを塗ってしまう。
        let arrow = [
            [0.0, 0.0],
            [10.0, 0.0],
            [10.0, 10.0],
            [5.0, 4.0],
            [0.0, 10.0],
        ];
        let expected = signed_area(&arrow).abs();
        let got = area_of(&arrow);
        assert!((got - expected).abs() < 0.01, "面積 {got} ≠ {expected}");
        assert_eq!(triangulate(&arrow).len(), 3);
    }

    /// 星形。凹みが 5 つある。
    #[test]
    fn a_star_keeps_its_area() {
        let mut star = Vec::new();
        for i in 0..10 {
            let a = -std::f32::consts::FRAC_PI_2 + i as f32 * std::f32::consts::PI / 5.0;
            let r = if i % 2 == 0 { 100.0 } else { 40.0 };
            star.push([a.cos() * r, a.sin() * r]);
        }
        let expected = signed_area(&star).abs();
        let got = area_of(&star);
        assert!((got - expected).abs() / expected < 0.001, "面積 {got} ≠ {expected}");
    }

    /// 向きが逆でも同じように分けられる。
    #[test]
    fn winding_direction_does_not_matter() {
        let mut square = vec![[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]];
        let forward = area_of(&square);
        square.reverse();
        let backward = area_of(&square);
        assert!((forward - backward).abs() < 0.01);
    }

    /// 潰れた形でも止まらないこと。
    #[test]
    fn degenerate_shapes_do_not_hang() {
        assert!(triangulate(&[]).is_empty());
        assert!(triangulate(&[[0.0, 0.0], [1.0, 1.0]]).is_empty());
        // すべて同じ点。耳が見つからないが、無限には回らない。
        let same = [[1.0, 1.0]; 8];
        let _ = triangulate(&same);
        // 自己交差する形。正しくは分けられないが落ちない。
        let bowtie = [[0.0, 0.0], [10.0, 10.0], [10.0, 0.0], [0.0, 10.0]];
        let _ = triangulate(&bowtie);
    }

    #[test]
    fn a_shape_does_not_leak_into_the_next_frame() {
        let mut g = Graphics::new();
        g.begin_frame(100.0, 100.0);
        g.begin_shape(ShapeKind::Polygon);
        g.vertex(0.0, 0.0);
        g.vertex(50.0, 0.0);
        // endShape() を書き忘れたまま次のフレームへ。
        g.begin_frame(100.0, 100.0);
        g.vertex(50.0, 50.0);
        g.end_shape(true);
        assert!(g.draw_list().indices.is_empty(), "前のフレームの頂点が残っています");
    }
    use super::*;

    #[test]
    fn translate_then_rotate_matches_processing_order() {
        let mut g = Graphics::new();
        g.begin_frame(100.0, 100.0);
        g.translate(10.0, 20.0);
        g.rotate(std::f32::consts::FRAC_PI_2);
        // ローカル (1, 0) は 90 度回って (0, 1)、その後 (10, 20) 平行移動。
        let p = g.matrix.apply(1.0, 0.0);
        assert!((p[0] - 10.0).abs() < 1e-4, "x = {}", p[0]);
        assert!((p[1] - 21.0).abs() < 1e-4, "y = {}", p[1]);
    }

    #[test]
    fn push_pop_restores_matrix() {
        let mut g = Graphics::new();
        g.begin_frame(100.0, 100.0);
        g.push_matrix();
        g.translate(5.0, 5.0);
        g.pop_matrix();
        assert_eq!(g.matrix, Affine::IDENTITY);
    }

    #[test]
    fn background_discards_previous_geometry() {
        let mut g = Graphics::new();
        g.begin_frame(100.0, 100.0);
        g.rect(0.0, 0.0, 10.0, 10.0);
        assert!(!g.draw_list().is_empty());
        g.background(0.0);
        assert!(g.draw_list().is_empty());
    }

    #[test]
    fn a_frame_keeps_the_previous_one_unless_told_otherwise() {
        let mut g = Graphics::new();
        g.begin_frame(100.0, 100.0);
        assert_eq!(g.draw_list().clear, None, "background() を呼ばなければ残す");

        g.background(0.0);
        assert_eq!(g.draw_list().clear, Some(Color::BLACK));
    }

    #[test]
    fn a_translucent_background_paints_over_instead_of_clearing() {
        let mut g = Graphics::new();
        g.begin_frame(100.0, 50.0);
        g.rect(0.0, 0.0, 10.0, 10.0);
        let before = g.draw_list().indices.len();

        g.background_color(Color::rgba(0.0, 0.0, 0.0, 0.1));

        assert_eq!(g.draw_list().clear, None, "半透明では消さない");
        assert!(g.draw_list().indices.len() > before, "前の描画を消してはいけない");
        // 画面全体を覆う 1 枚が足される。
        assert_eq!(g.draw_list().indices.len(), before + 6);
    }

    #[test]
    fn a_declared_canvas_is_scaled_and_centred() {
        let mut g = Graphics::new();
        g.begin_frame(800.0, 400.0);
        g.set_canvas(200.0, 200.0);

        // 作品から見えるのは宣言したサイズ。
        assert_eq!((g.width, g.height), (200.0, 200.0));
        assert_eq!(g.viewport(), (800.0, 400.0));

        // 高さに合わせて 2 倍 (200 → 400)、横は中央へ寄せる。
        assert_eq!(g.matrix.apply(0.0, 0.0), [200.0, 0.0]);
        assert_eq!(g.matrix.apply(200.0, 200.0), [600.0, 400.0]);
    }

    #[test]
    fn without_a_declared_canvas_the_viewport_is_used_directly() {
        let mut g = Graphics::new();
        g.begin_frame(800.0, 400.0);
        assert_eq!((g.width, g.height), (800.0, 400.0));
        assert_eq!(g.matrix, Affine::IDENTITY);
    }

    #[test]
    fn the_canvas_transform_survives_the_next_frame() {
        let mut g = Graphics::new();
        g.begin_frame(800.0, 400.0);
        g.set_canvas(200.0, 200.0);
        g.begin_frame(800.0, 400.0);
        assert_eq!((g.width, g.height), (200.0, 200.0), "毎フレーム宣言し直さなくてよい");
        assert_eq!(g.matrix.apply(0.0, 0.0), [200.0, 0.0]);
    }

    #[test]
    fn push_and_pop_stay_above_the_canvas_transform() {
        let mut g = Graphics::new();
        g.begin_frame(400.0, 400.0);
        g.set_canvas(200.0, 200.0);
        let base = g.matrix;

        g.push_matrix();
        g.translate(10.0, 10.0);
        g.pop_matrix();
        assert_eq!(g.matrix, base, "キャンバスの拡大まで巻き戻してはいけない");
    }

    #[test]
    fn a_translucent_background_covers_the_whole_frame() {
        let mut g = Graphics::new();
        g.begin_frame(100.0, 50.0);
        // 座標変換の途中でも画面全体を覆う。
        g.translate(30.0, 30.0);
        g.rotate(1.0);
        g.background_color(Color::rgba(0.0, 0.0, 0.0, 0.5));

        let xs: Vec<f32> = g.draw_list().vertices.iter().map(|v| v.pos[0]).collect();
        let ys: Vec<f32> = g.draw_list().vertices.iter().map(|v| v.pos[1]).collect();
        assert_eq!(xs.iter().cloned().fold(f32::MAX, f32::min), 0.0);
        assert_eq!(xs.iter().cloned().fold(f32::MIN, f32::max), 100.0);
        assert_eq!(ys.iter().cloned().fold(f32::MIN, f32::max), 50.0);
    }

    #[test]
    fn no_fill_no_stroke_emits_nothing() {
        let mut g = Graphics::new();
        g.begin_frame(100.0, 100.0);
        g.no_fill();
        g.no_stroke();
        g.rect(0.0, 0.0, 10.0, 10.0);
        g.circle(5.0, 5.0, 4.0);
        g.line(0.0, 0.0, 9.0, 9.0);
        assert!(g.draw_list().is_empty());
    }

    #[test]
    fn shapes_are_grouped_by_blend_mode() {
        let mut g = Graphics::new();
        g.begin_frame(100.0, 100.0);
        g.no_stroke();

        g.rect(0.0, 0.0, 1.0, 1.0);
        g.rect(0.0, 0.0, 1.0, 1.0);
        assert_eq!(g.draw_list().batches.len(), 1, "同じ合成方法はまとまる");

        g.blend_mode(BlendMode::Add);
        g.rect(0.0, 0.0, 1.0, 1.0);
        assert_eq!(g.draw_list().batches.len(), 2);

        let batches = &g.draw_list().batches;
        assert_eq!(batches[0].blend, BlendMode::Blend);
        assert_eq!(batches[1].blend, BlendMode::Add);
        // 区間は隙間なく全部の三角形を覆う。
        assert_eq!(batches[0].start, 0);
        assert_eq!(batches[0].end, batches[1].start);
        assert_eq!(batches[1].end, g.draw_list().indices.len() as u32);
    }

    #[test]
    fn an_opaque_background_clears_the_batches_too() {
        let mut g = Graphics::new();
        g.begin_frame(100.0, 100.0);
        g.rect(0.0, 0.0, 1.0, 1.0);
        assert!(!g.draw_list().batches.is_empty());

        g.background(0.0);
        assert!(g.draw_list().batches.is_empty());
    }

    #[test]
    fn scale_hint_tracks_matrix() {
        let mut g = Graphics::new();
        g.begin_frame(100.0, 100.0);
        g.scale(4.0, 4.0);
        assert!((g.matrix.scale_hint() - 4.0).abs() < 1e-4);
    }

    /// キャンバスの当てはめ方。
    ///
    /// 正方形の作品を横長の画面へ出すとき、収めれば左右が余り、埋めれば
    /// 大きく映るかわりに上下が切れる。
    #[test]
    fn a_square_canvas_can_be_contained_or_cover_the_view() {
        let corners = |fit: CanvasFit| {
            let mut g = Graphics::new();
            g.set_fit(fit);
            g.begin_frame(200.0, 100.0);
            g.set_canvas(100.0, 100.0);
            g.no_stroke();
            g.fill(255.0);
            // キャンバスいっぱいの四角。表示領域のどこへ来るかを見る。
            g.rect(0.0, 0.0, 100.0, 100.0);
            let v = &g.draw_list().vertices;
            let x: Vec<f32> = v.iter().map(|p| p.pos[0]).collect();
            let y: Vec<f32> = v.iter().map(|p| p.pos[1]).collect();
            (
                x.iter().cloned().fold(f32::MAX, f32::min),
                y.iter().cloned().fold(f32::MAX, f32::min),
                x.iter().cloned().fold(f32::MIN, f32::max),
                y.iter().cloned().fold(f32::MIN, f32::max),
            )
        };

        // 収める: 高さに合わせて等倍。左右に 50px ずつ余白。
        assert_eq!(corners(CanvasFit::Contain), (50.0, 0.0, 150.0, 100.0));
        // 埋める: 幅に合わせて 2 倍。上下が 50px ずつはみ出す。
        assert_eq!(corners(CanvasFit::Cover), (0.0, -50.0, 200.0, 150.0));
    }

    /// 当てはめ方を変えたら、その場で効く。
    ///
    /// 設定画面で切り替えたときに次の `size()` まで待たされない。
    #[test]
    fn changing_the_fit_takes_effect_at_once() {
        let mut g = Graphics::new();
        g.begin_frame(200.0, 100.0);
        g.set_canvas(100.0, 100.0);
        g.no_stroke();
        g.set_fit(CanvasFit::Cover);
        g.fill(255.0);
        g.rect(0.0, 0.0, 100.0, 100.0);
        let right = g.draw_list().vertices.iter().map(|p| p.pos[0]).fold(f32::MIN, f32::max);
        assert_eq!(right, 200.0, "切り替えが次のフレームまで効いていません");
    }
}
