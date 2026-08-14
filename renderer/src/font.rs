//! 文字の字形をアトラスへ焼く (設計書 §14.2 の `text()`)。
//!
//! 字形は輪郭線として取り出し、自前で塗り分けてから 1 枚のテクスチャへ並べる。
//! 使った字だけを焼き、同じ字は使い回す。
//!
//! アトラスの左上には不透明な白い点を置いてある。文字以外の図形はそこを指す
//! ので、描画のパイプラインを 1 本にまとめられる。

use std::collections::HashMap;
use std::rc::Rc;

use skrifa::instance::{LocationRef, Size};
use skrifa::outline::{DrawSettings, OutlinePen};
use skrifa::raw::FileRef;
use skrifa::{FontRef, MetadataProvider};

/// アトラスの一辺。CJK を数百字使う作品でも収まる大きさ。
pub const ATLAS_SIZE: usize = 1024;
/// 字形を焼くときの大きさ (px)。表示はここから拡大縮小する。
const RASTER_SIZE: f32 = 48.0;
/// 字形どうしの余白。にじみ出しを防ぐ。
const PADDING: usize = 1;
/// 縦方向の細分数。多いほど滑らかになるが遅くなる。
const SUBSAMPLES: usize = 4;

/// アトラス上の 1 文字。
#[derive(Clone, Copy, Debug)]
pub struct Glyph {
    /// テクスチャ上の位置 (0..1)。
    pub uv: [f32; 4],
    /// 基準点からのずれ。`RASTER_SIZE` を 1 とした比率。
    pub offset: [f32; 2],
    /// 大きさ。`RASTER_SIZE` を 1 とした比率。
    pub size: [f32; 2],
    /// 次の字までの送り幅。`RASTER_SIZE` を 1 とした比率。
    pub advance: f32,
}

/// 焼いた字形を溜めておくアトラス。
pub struct FontAtlas {
    /// 使うフォント。前から順に試し、字形を持っているものを使う。
    ///
    /// 1 本では足りない。日本語のフォントに麻雀牌や記号は入っていないことが
    /// 多く、記号のフォントに日本語は入っていない。
    fonts: Vec<Rc<Vec<u8>>>,
    pixels: Vec<u8>,
    glyphs: HashMap<char, Option<Glyph>>,
    /// 次に字形を置く位置。
    cursor: (usize, usize),
    /// いま並べている行の高さ。
    row_height: usize,
    /// 中身が変わるたびに増える。GPU 側はこれを見て送り直す。
    version: u64,
}

impl Default for FontAtlas {
    fn default() -> Self {
        Self::new()
    }
}

impl FontAtlas {
    pub fn new() -> Self {
        let mut pixels = vec![0u8; ATLAS_SIZE * ATLAS_SIZE];
        // 左上を不透明にしておく。文字以外の図形はここを指す。
        for y in 0..2 {
            for x in 0..2 {
                pixels[y * ATLAS_SIZE + x] = 255;
            }
        }
        Self {
            fonts: Vec::new(),
            pixels,
            glyphs: HashMap::new(),
            // 白い点を避けて始める。
            cursor: (4, 0),
            row_height: 4,
            version: 1,
        }
    }

    /// 図形が指す「白い点」の位置。
    pub fn white_uv() -> [f32; 2] {
        let half = 0.5 / ATLAS_SIZE as f32;
        [half, half]
    }

    /// 使うフォントを差し替える。焼いた字形は捨てる。
    pub fn set_font(&mut self, bytes: Vec<u8>) {
        self.set_fonts(vec![bytes]);
    }

    /// 予備も含めて差し替える。前のものから順に字形を探す。
    pub fn set_fonts(&mut self, fonts: Vec<Vec<u8>>) {
        self.fonts = fonts
            .into_iter()
            .filter(|bytes| {
                let ok = font_of(bytes).is_some();
                if !ok {
                    log::warn!("フォントとして読めませんでした");
                }
                ok
            })
            .map(Rc::new)
            .collect();
        self.glyphs.clear();
        self.pixels[4..].fill(0);
        self.cursor = (4, 0);
        self.row_height = 4;
        self.version += 1;
    }

    pub fn has_font(&self) -> bool {
        !self.fonts.is_empty()
    }

    pub fn version(&self) -> u64 {
        self.version
    }

    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }

    /// 1 文字ぶんの字形。まだ焼いていなければここで焼く。
    ///
    /// 字形を持たない文字や、アトラスが埋まったときは `None`。
    pub fn glyph(&mut self, ch: char) -> Option<Glyph> {
        if let Some(cached) = self.glyphs.get(&ch) {
            return *cached;
        }
        let baked = self.bake(ch);
        self.glyphs.insert(ch, baked);
        baked
    }

    fn bake(&mut self, ch: char) -> Option<Glyph> {
        // 字形を持っているフォントを前から探す。
        let (bytes, glyph_id) = self.fonts.iter().find_map(|bytes| {
            let font = font_of(bytes)?;
            let id = font.charmap().map(ch)?;
            Some((bytes.clone(), id))
        })?;
        let font = font_of(&bytes)?;

        let size = Size::new(RASTER_SIZE);
        let metrics = font.glyph_metrics(size, LocationRef::default());
        let advance = metrics.advance_width(glyph_id).unwrap_or(0.0) / RASTER_SIZE;

        let outlines = font.outline_glyphs();
        let outline = outlines.get(glyph_id)?;

        let mut pen = Outline::default();
        outline
            .draw(DrawSettings::unhinted(skrifa::instance::Size::new(RASTER_SIZE), LocationRef::default()), &mut pen)
            .ok()?;

        // 空白のように輪郭を持たない字。送り幅だけ返す。
        if pen.segments.is_empty() {
            return Some(Glyph { uv: [0.0; 4], offset: [0.0; 2], size: [0.0; 2], advance });
        }

        let coverage = pen.rasterize()?;
        let (w, h) = (coverage.width, coverage.height);
        let (x, y) = self.allocate(w, h)?;

        for row in 0..h {
            let dst = (y + row) * ATLAS_SIZE + x;
            let src = row * w;
            self.pixels[dst..dst + w].copy_from_slice(&coverage.pixels[src..src + w]);
        }
        self.version += 1;

        let scale = 1.0 / ATLAS_SIZE as f32;
        Some(Glyph {
            uv: [
                x as f32 * scale,
                y as f32 * scale,
                (x + w) as f32 * scale,
                (y + h) as f32 * scale,
            ],
            // フォントの座標系は上が正。画面は下が正なので符号を入れ替える。
            offset: [coverage.left / RASTER_SIZE, -coverage.top / RASTER_SIZE],
            size: [w as f32 / RASTER_SIZE, h as f32 / RASTER_SIZE],
            advance,
        })
    }

    /// アトラスに `w` x `h` の場所を取る。埋まっていれば `None`。
    fn allocate(&mut self, w: usize, h: usize) -> Option<(usize, usize)> {
        if w == 0 || h == 0 || w > ATLAS_SIZE {
            return None;
        }
        if self.cursor.0 + w + PADDING > ATLAS_SIZE {
            // 次の行へ折り返す。
            self.cursor = (0, self.cursor.1 + self.row_height + PADDING);
            self.row_height = 0;
        }
        if self.cursor.1 + h + PADDING > ATLAS_SIZE {
            log::warn!("字形のアトラスが埋まりました");
            return None;
        }
        let at = self.cursor;
        self.cursor.0 += w + PADDING;
        self.row_height = self.row_height.max(h);
        Some(at)
    }
}

/// バイト列からフォントを取り出す。
///
/// macOS の `.ttc` のように複数のフォントが束ねられたファイルもあるので、
/// その場合は先頭の 1 本を使う。
fn font_of(bytes: &[u8]) -> Option<FontRef<'_>> {
    match FileRef::new(bytes).ok()? {
        FileRef::Font(font) => Some(font),
        FileRef::Collection(collection) => collection.get(0).ok(),
    }
}

/// 焼き上がった 1 文字の濃さ。
struct Coverage {
    pixels: Vec<u8>,
    width: usize,
    height: usize,
    /// 基準点から見た左端と上端 (px)。
    left: f32,
    top: f32,
}

/// 輪郭線を直線の集まりとして受け取る。
#[derive(Default)]
struct Outline {
    segments: Vec<[f32; 4]>,
    start: [f32; 2],
    current: [f32; 2],
}

impl Outline {
    fn line(&mut self, to: [f32; 2]) {
        self.segments.push([self.current[0], self.current[1], to[0], to[1]]);
        self.current = to;
    }

    /// 曲線は折れ線へ均す。焼く大きさが決まっているので分割数も決め打ちでよい。
    fn flatten(&mut self, points: &[[f32; 2]], to: [f32; 2]) {
        const STEPS: usize = 8;
        let from = self.current;
        for i in 1..=STEPS {
            let t = i as f32 / STEPS as f32;
            let p = match points.len() {
                1 => {
                    let u = 1.0 - t;
                    [
                        u * u * from[0] + 2.0 * u * t * points[0][0] + t * t * to[0],
                        u * u * from[1] + 2.0 * u * t * points[0][1] + t * t * to[1],
                    ]
                }
                _ => {
                    let u = 1.0 - t;
                    let (a, b, c, d) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
                    [
                        a * from[0] + b * points[0][0] + c * points[1][0] + d * to[0],
                        a * from[1] + b * points[0][1] + c * points[1][1] + d * to[1],
                    ]
                }
            };
            self.line(p);
        }
    }

    /// 輪郭を塗り分けて濃さの並びにする。
    ///
    /// 上下に細分した走査線と輪郭の交点を求め、巻き数が 0 でない区間を塗る。
    /// 巻き数で見るので、`o` や `あ` のような穴のある字も正しく抜ける。
    fn rasterize(&self) -> Option<Coverage> {
        let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
        for s in &self.segments {
            x0 = x0.min(s[0]).min(s[2]);
            y0 = y0.min(s[1]).min(s[3]);
            x1 = x1.max(s[0]).max(s[2]);
            y1 = y1.max(s[1]).max(s[3]);
        }

        let left = x0.floor();
        let top = y1.ceil();
        let width = (x1.ceil() - left).max(1.0) as usize + 1;
        let height = (top - y0.floor()).max(1.0) as usize + 1;
        if width > ATLAS_SIZE || height > ATLAS_SIZE {
            return None;
        }

        let mut pixels = vec![0f32; width * height];
        let mut crossings: Vec<(f32, i32)> = Vec::new();

        for row in 0..height {
            for sub in 0..SUBSAMPLES {
                // フォントの座標系は上が正なので、行番号を y へ戻す。
                let y = top - row as f32 - (sub as f32 + 0.5) / SUBSAMPLES as f32;

                crossings.clear();
                for s in &self.segments {
                    let (ax, ay, bx, by) = (s[0], s[1], s[2], s[3]);
                    if (ay <= y) == (by <= y) {
                        continue;
                    }
                    let t = (y - ay) / (by - ay);
                    crossings.push((ax + (bx - ax) * t, if by > ay { 1 } else { -1 }));
                }
                if crossings.len() < 2 {
                    continue;
                }
                crossings.sort_by(|a, b| a.0.total_cmp(&b.0));

                let mut winding = 0;
                for pair in crossings.windows(2) {
                    winding += pair[0].1;
                    if winding == 0 {
                        continue;
                    }
                    // 塗る区間。端の画素は掛かった分だけ濃くする。
                    let (from, to) = (pair[0].0 - left, pair[1].0 - left);
                    let first = from.floor().max(0.0) as usize;
                    let last = (to.ceil() as usize).min(width);
                    for px in first..last {
                        let l = (px as f32).max(from);
                        let r = ((px + 1) as f32).min(to);
                        if r > l {
                            pixels[row * width + px] += (r - l) / SUBSAMPLES as f32;
                        }
                    }
                }
            }
        }

        Some(Coverage {
            pixels: pixels.iter().map(|v| (v.clamp(0.0, 1.0) * 255.0) as u8).collect(),
            width,
            height,
            left,
            top,
        })
    }
}

impl OutlinePen for Outline {
    fn move_to(&mut self, x: f32, y: f32) {
        self.close();
        self.start = [x, y];
        self.current = [x, y];
    }

    fn line_to(&mut self, x: f32, y: f32) {
        self.line([x, y]);
    }

    fn quad_to(&mut self, cx: f32, cy: f32, x: f32, y: f32) {
        self.flatten(&[[cx, cy]], [x, y]);
    }

    fn curve_to(&mut self, cx1: f32, cy1: f32, cx2: f32, cy2: f32, x: f32, y: f32) {
        self.flatten(&[[cx1, cy1], [cx2, cy2]], [x, y]);
    }

    fn close(&mut self) {
        // 閉じていない輪郭は塗り分けが破綻するので、必ず始点へ戻す。
        if self.current != self.start {
            self.line(self.start);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_white_pixel_is_opaque() {
        let atlas = FontAtlas::new();
        assert_eq!(atlas.pixels()[0], 255);
        let [u, v] = FontAtlas::white_uv();
        assert!(u > 0.0 && u < 0.01 && v > 0.0 && v < 0.01);
    }

    #[test]
    fn without_a_font_there_are_no_glyphs() {
        let mut atlas = FontAtlas::new();
        assert!(!atlas.has_font());
        assert!(atlas.glyph('A').is_none());
    }

    /// 四角い輪郭を塗ると中が埋まる。
    #[test]
    fn a_square_outline_is_filled() {
        let mut o = Outline::default();
        o.move_to(0.0, 0.0);
        o.line_to(10.0, 0.0);
        o.line_to(10.0, 10.0);
        o.line_to(0.0, 10.0);
        o.close();

        let c = o.rasterize().expect("焼ける");
        let center = c.pixels[(c.height / 2) * c.width + c.width / 2];
        assert!(center > 200, "中が塗られていません: {center}");
    }

    /// 内側にもう 1 周ある字は、中が抜ける。
    ///
    /// 巻き数で見ないと `o` や `あ` の穴が埋まってしまう。
    #[test]
    fn an_inner_contour_leaves_a_hole() {
        let mut o = Outline::default();
        // 外側は反時計回り。
        o.move_to(0.0, 0.0);
        o.line_to(20.0, 0.0);
        o.line_to(20.0, 20.0);
        o.line_to(0.0, 20.0);
        o.close();
        // 内側は時計回り。
        o.move_to(5.0, 5.0);
        o.line_to(5.0, 15.0);
        o.line_to(15.0, 15.0);
        o.line_to(15.0, 5.0);
        o.close();

        let c = o.rasterize().expect("焼ける");
        let at = |x: usize, y: usize| c.pixels[y * c.width + x];
        assert!(at(c.width / 2, c.height / 2) < 40, "穴が埋まっています");
        assert!(at(2, c.height / 2) > 200, "外側が塗られていません");
    }
}
