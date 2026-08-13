//! スケッチの実行インタフェース。
//!
//! Viewer は「何が絵を描いているか」を知らない。将来 p5.js subset などを足すときも
//! [`Sketch`] を実装した型を増やすだけで、Viewer 側は変わらない (設計書 §23.2)。

use tsubu_renderer::Graphics;

/// 1 作品分の実行単位。
///
/// スレッドをまたがない。作品の値 (配列やオブジェクト) は `Rc` で持つので、
/// 生成から破棄まで同じスレッドに留める。
pub trait Sketch {
    /// `setup()`。最初の [`Sketch::draw`] の直前に一度だけ呼ばれる。
    fn setup(&mut self, g: &mut Graphics) {
        let _ = g;
    }

    /// `draw()`。毎フレーム呼ばれる。
    fn draw(&mut self, g: &mut Graphics);

    /// どちらの方言として読まれたか。分からなければ `None`。
    fn dialect(&self) -> Option<crate::dialect::Dialect> {
        None
    }

    /// 1 フレームに使ってよい命令数を変える (設計書 §24 の Execution Budget)。
    ///
    /// 実装しない型では何も起きない。予算という考え方を持たない作品もある。
    fn set_budget(&mut self, budget: u64) {
        let _ = budget;
    }

    /// 実行を続けられない状態になっていればその理由。
    ///
    /// コンパイルエラーや、実行予算を使い切り続けている作品がここに出る。
    fn error(&self) -> Option<&str> {
        None
    }
}

/// ギャラリーに並ぶ作品のメタ情報。
///
/// Phase 7 で SQLite の `Sketch` テーブルへ移すまでの、コード上の暫定表現。
#[derive(Clone, Debug)]
pub struct SketchInfo {
    /// ファイル名やキャッシュキーに使う安定した識別子。
    pub id: String,
    /// 表示名。ユーザーデータなので UI 言語からは独立している (設計書 §11.3)。
    pub title: String,
    /// サムネイルを取得するフレーム (設計書 §7.1)。
    pub thumbnail_frame: u64,
}

/// メモリ上に読み込み済みの作品。
///
/// Prototype B の「再起動せずに切り替える」を成立させるため、Viewer は全作品の
/// インスタンスを保持したままにする。
pub struct LoadedSketch {
    pub info: SketchInfo,
    pub sketch: Box<dyn Sketch>,
    /// `setup()` を実行済みか。
    pub initialized: bool,
}

impl LoadedSketch {
    pub fn set_budget(&mut self, budget: u64) {
        self.sketch.set_budget(budget);
    }

    pub fn new(info: SketchInfo, sketch: Box<dyn Sketch>) -> Self {
        Self { info, sketch, initialized: false }
    }

    /// 1 フレーム進める。初回だけ `setup()` を挟む。
    pub fn step(&mut self, g: &mut Graphics) {
        if !self.initialized {
            self.sketch.setup(g);
            self.initialized = true;
        }
        self.sketch.draw(g);
    }

    pub fn error(&self) -> Option<&str> {
        self.sketch.error()
    }

    pub fn dialect(&self) -> Option<crate::dialect::Dialect> {
        self.sketch.dialect()
    }
}

/// コンパイルに失敗した作品の代わりに置くスケッチ。
///
/// Gallery からは開けてしまうので、Viewer で「動かない理由」を示せるように
/// 何かしら描く必要がある。
pub struct BrokenSketch {
    message: String,
}

impl BrokenSketch {
    pub fn new(message: impl Into<String>) -> Self {
        Self { message: message.into() }
    }
}

impl Sketch for BrokenSketch {
    fn draw(&mut self, g: &mut Graphics) {
        g.background_rgb(40.0, 14.0, 18.0);

        // 文字は描けないので、斜線だけで「壊れている」ことを示す。
        let s = g.width.min(g.height);
        g.stroke_rgba(220.0, 90.0, 100.0, 200.0);
        g.stroke_weight(s * 0.006);
        let (cx, cy, r) = (g.width * 0.5, g.height * 0.5, s * 0.12);
        g.line(cx - r, cy - r, cx + r, cy + r);
        g.line(cx + r, cy - r, cx - r, cy + r);
    }

    fn error(&self) -> Option<&str> {
        Some(&self.message)
    }
}
