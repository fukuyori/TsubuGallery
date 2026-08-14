//! 全画面 Viewer の再生状態 (設計書 §8 / §18)。
//!
//! 全作品のインスタンスを常駐させ、切り替えではプロセスもランタイムも作り直さない。
//! 各作品はそれぞれの `frameCount` を保持するので、戻ってきたときも続きから動く。

use tsubu_core::settings::{FrameRate, Navigation, Settings};
use std::time::Instant;

use tsubu_processing_lite::LoadedSketch;
use tsubu_processing_lite::math::Rng;
use tsubu_renderer::Graphics;

/// 直近フレームの計測値。HUD が表示する。
#[derive(Clone, Copy, Debug, Default)]
pub struct Stats {
    pub fps: f32,
    pub frame_count: u64,
    /// 最後の作品切り替えに要した時間 (ms)。
    pub last_switch_ms: f32,
}

pub struct Viewer {
    sketches: Vec<LoadedSketch>,
    /// 作品ごとの `frameCount`。切り替えで巻き戻さないため個別に持つ。
    frame_counts: Vec<u64>,
    current: usize,
    /// 表示中の絵が別物になるたびに増える。キャンバスの蓄積を捨てる合図。
    epoch: u64,

    graphics: Graphics,
    /// 先読み用。表示中の描画内容を壊さないよう別インスタンスを使う。
    warmup: Graphics,

    paused: bool,
    started: Instant,
    last_frame: Instant,
    fps: f32,
    last_switch_ms: f32,
    rng: Rng,

    width: f32,
    height: f32,

    /// 設計書 §24 の Viewer / Runtime 設定。
    frame_rate: FrameRate,
    navigation: Navigation,
    preload: bool,
    /// 目標フレームレートに合わせるため、次に進めてよくなる時刻。
    next_step: Instant,
}

impl Viewer {
    /// 作品 0 本でも作れる。すべて削除された Gallery でも成立させるため。
    pub fn new(sketches: Vec<LoadedSketch>) -> Self {
        let frame_counts = vec![0; sketches.len()];
        Self {
            sketches,
            frame_counts,
            current: 0,
            epoch: 0,
            graphics: Graphics::new(),
            warmup: Graphics::new(),
            paused: false,
            started: Instant::now(),
            last_frame: Instant::now(),
            fps: 0.0,
            last_switch_ms: 0.0,
            rng: Rng::new(0x0073_556E_6275_u64),
            width: 1.0,
            height: 1.0,
            frame_rate: FrameRate::default(),
            navigation: Navigation::default(),
            preload: true,
            next_step: Instant::now(),
        }
    }

    /// 作品の `text()` に使うフォントを渡す。前のものから順に字形を探す。
    pub fn set_fonts(&mut self, fonts: Vec<Vec<u8>>) {
        self.graphics.font.set_fonts(fonts.clone());
        self.warmup.font.set_fonts(fonts);
    }

    /// 設定を反映する (設計書 §24)。
    pub fn apply_settings(&mut self, settings: &Settings) {
        self.frame_rate = settings.frame_rate;
        self.navigation = settings.navigation;
        self.preload = settings.preload;

        let budget = settings.execution_budget.instructions();
        for sketch in &mut self.sketches {
            sketch.set_budget(budget);
        }
    }

    pub fn len(&self) -> usize {
        self.sketches.len()
    }

    pub fn is_empty(&self) -> bool {
        self.sketches.is_empty()
    }

    pub fn current_index(&self) -> usize {
        self.current
    }

    /// 表示中の絵の世代。変わったらキャンバスを消して描き直す。
    pub fn epoch(&self) -> u64 {
        self.epoch
    }

    pub fn current_title(&self) -> Option<&str> {
        self.sketches.get(self.current).map(|s| s.info.title.as_str())
    }

    /// 表示中の作品が動かない理由。コンパイルエラーや実行の打ち切り。
    pub fn current_error(&self) -> Option<&str> {
        self.sketches.get(self.current)?.error()
    }

    /// 表示中の作品が、どちらの方言として読まれたか。
    pub fn current_dialect(&self) -> Option<tsubu_processing_lite::dialect::Dialect> {
        self.sketches.get(self.current)?.dialect()
    }

    /// 作品を差し替える。編集を保存したときに使う。
    pub fn replace(&mut self, index: usize, sketch: LoadedSketch) {
        if let Some(slot) = self.sketches.get_mut(index) {
            *slot = sketch;
            self.frame_counts[index] = 0;
            self.graphics.reset_state();
            self.epoch += 1;
        }
    }

    /// 作品を差し込む。Gallery と同じ並びを保つため位置を指定する。
    pub fn insert(&mut self, index: usize, sketch: LoadedSketch) {
        let index = index.min(self.sketches.len());
        self.sketches.insert(index, sketch);
        self.frame_counts.insert(index, 0);
        if self.current >= index && self.sketches.len() > 1 {
            self.current += 1;
        }
    }

    /// 作品を取り除く。表示中だったときは近くの作品へ寄せる。
    pub fn remove(&mut self, index: usize) {
        if index >= self.sketches.len() {
            return;
        }
        self.sketches.remove(index);
        self.frame_counts.remove(index);
        self.current = self.current.min(self.sketches.len().saturating_sub(1));
        self.graphics.reset_state();
        self.epoch += 1;
    }

    pub fn is_paused(&self) -> bool {
        self.paused
    }

    pub fn toggle_pause(&mut self) {
        self.paused = !self.paused;
    }

    pub fn stats(&self) -> Stats {
        Stats {
            fps: self.fps,
            frame_count: self.frame_counts[self.current],
            last_switch_ms: self.last_switch_ms,
        }
    }

    pub fn set_mouse(&mut self, x: f32, y: f32, pressed: bool) {
        self.graphics.mouse_x = x;
        self.graphics.mouse_y = y;
        self.graphics.mouse_pressed = pressed;
    }

    /// 1 フレーム進めて描画コマンドを作る。
    ///
    /// 一時停止中もこの関数は呼ぶ (ウィンドウ再描画のたびに同じ絵が必要なため)
    /// が、`frameCount` は進めない。
    pub fn render_frame(&mut self, width: f32, height: f32) -> Option<&Graphics> {
        self.width = width;
        self.height = height;
        if self.sketches.is_empty() {
            return None;
        }

        let now = Instant::now();
        let dt = now.duration_since(self.last_frame).as_secs_f32();
        self.last_frame = now;
        if dt > 0.0 {
            // 表示がちらつかない程度に平滑化する。
            let instant_fps = 1.0 / dt;
            self.fps = if self.fps == 0.0 { instant_fps } else { self.fps * 0.9 + instant_fps * 0.1 };
        }

        // 目標フレームレートより速く回さない。表示の更新はディスプレイに任せ、
        // 作品を進めるかどうかだけをここで決める (設計書 §24 の Frame Rate)。
        let due = now >= self.next_step;
        if due {
            // 遅れを引きずらないよう、次の締め切りは今から数える。
            self.next_step = now + self.frame_rate.interval();
        }
        // `noLoop()` を呼んだ作品はフレームを進めない。進めると、乱数を使う
        // 作品が毎フレーム違う絵になってちらつく。
        let looping = self.graphics.is_looping();
        if !self.paused && due && looping {
            self.frame_counts[self.current] += 1;
        }

        self.graphics.begin_frame(width, height);
        self.graphics.frame_count = self.frame_counts[self.current];
        self.graphics.time = self.started.elapsed().as_secs_f32();
        self.sketches[self.current].step(&mut self.graphics);
        Some(&self.graphics)
    }

    pub fn thumbnail_frame_at(&self, index: usize) -> Option<u64> {
        self.sketches.get(index).map(|s| s.info.thumbnail_frame)
    }

    /// 表示中の作品を最初から動かし直す。編集の保存後に使う。
    pub fn restart_current(&mut self) {
        if let Some(sketch) = self.sketches.get_mut(self.current) {
            sketch.initialized = false;
            self.frame_counts[self.current] = 0;
            self.graphics.reset_state();
            self.epoch += 1;
        }
    }

    /// 設定に従って次の作品へ移る (設計書 §24 の Navigation)。
    ///
    /// `order` は再生する順番。Gallery で絞り込んだ結果をそのまま渡すので、
    /// 「お気に入りだけ」「このタグだけ」がそのまま再生範囲になる (設計書 §27)。
    pub fn next(&mut self, order: &[usize]) {
        match self.navigation {
            Navigation::Sequential => self.step(order, 1),
            Navigation::Random => self.random(order),
        }
    }

    pub fn previous(&mut self, order: &[usize]) {
        self.step(order, -1);
    }

    /// 再生順の中で `delta` 個ぶん動く。
    fn step(&mut self, order: &[usize], delta: isize) {
        let order = self.usable_order(order);
        if order.is_empty() {
            return;
        }
        let position = order.iter().position(|i| *i == self.current);
        let next = match position {
            Some(p) => {
                let n = order.len() as isize;
                ((p as isize + delta).rem_euclid(n)) as usize
            }
            // 表示中の作品が再生順に無い (絞り込みで外れた) なら先頭から。
            None => 0,
        };
        self.switch_to(order[next]);
    }

    pub fn random(&mut self, order: &[usize]) {
        let order = self.usable_order(order);
        if order.len() < 2 {
            return;
        }
        // 同じ作品を引き直したときは隣へずらし、必ず絵が変わるようにする。
        let mut target = (self.rng.random(order.len() as f32)) as usize;
        target = target.min(order.len() - 1);
        if order[target] == self.current {
            target = (target + 1) % order.len();
        }
        self.switch_to(order[target]);
    }

    /// 実在する作品だけに絞った再生順。空なら全作品を順に回す。
    ///
    /// 絞り込みで 0 件になったまま再生すると何も起きなくなるので、そのときは
    /// 全作品へ戻す。
    fn usable_order(&self, order: &[usize]) -> Vec<usize> {
        let filtered: Vec<usize> =
            order.iter().copied().filter(|i| *i < self.sketches.len()).collect();
        if filtered.is_empty() { (0..self.sketches.len()).collect() } else { filtered }
    }

    pub fn switch_to(&mut self, index: usize) {
        if index >= self.sketches.len() || index == self.current {
            return;
        }
        let t0 = Instant::now();
        self.current = index;
        self.graphics.reset_state();
        self.epoch += 1;
        self.last_switch_ms = t0.elapsed().as_secs_f32() * 1000.0;
        self.preload_neighbours();
    }

    /// 前後の作品の `setup()` を先に済ませておく (設計書 §18)。
    ///
    /// これをやっておくと、切り替え直後のフレームで初期化コストを踏まない。
    fn preload_neighbours(&mut self) {
        if !self.preload {
            return;
        }
        let n = self.sketches.len();
        if n == 0 {
            return;
        }
        let neighbours = [(self.current + 1) % n, (self.current + n - 1) % n];
        for index in neighbours {
            if self.sketches[index].initialized {
                continue;
            }
            self.warmup.reset_state();
            self.warmup.begin_frame(self.width, self.height);
            self.warmup.frame_count = self.frame_counts[index];
            self.sketches[index].step(&mut self.warmup);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tsubu_processing_lite::{BrokenSketch, SketchInfo};

    fn viewer_of(n: usize) -> Viewer {
        Viewer::new(
            (0..n)
                .map(|i| {
                    let info = SketchInfo {
                        id: format!("sketch-{i}"),
                        title: format!("Sketch {i}"),
                        thumbnail_frame: 1,
                    };
                    // 実行はしないので中身は何でもよい。
                    LoadedSketch::new(info, Box::new(BrokenSketch::new("テスト用")))
                })
                .collect(),
        )
    }

    /// 絞り込んだ結果がそのまま再生範囲になる (設計書 §27)。
    #[test]
    fn navigation_stays_inside_the_playlist() {
        let mut v = viewer_of(6);
        let playlist = [1usize, 3, 5];

        v.switch_to(1);
        v.next(&playlist);
        assert_eq!(v.current_index(), 3);
        v.next(&playlist);
        assert_eq!(v.current_index(), 5);
        // 端で折り返す。
        v.next(&playlist);
        assert_eq!(v.current_index(), 1);

        v.previous(&playlist);
        assert_eq!(v.current_index(), 5);
    }

    /// 再生範囲の外を見ているときは、範囲の先頭から始める。
    ///
    /// 作品を見ている最中に絞り込みを変えると起きる。
    #[test]
    fn navigation_from_outside_the_playlist_starts_at_the_front() {
        let mut v = viewer_of(6);
        v.switch_to(4);
        v.next(&[1, 3, 5]);
        assert_eq!(v.current_index(), 1);
    }

    /// 絞り込みで 0 件になっても動けなくならないこと。
    #[test]
    fn an_empty_playlist_falls_back_to_every_sketch() {
        let mut v = viewer_of(4);
        v.switch_to(0);
        v.next(&[]);
        assert_eq!(v.current_index(), 1, "全作品を順に回るはず");
    }

    /// 消えた作品を指す再生順を渡されても落ちないこと。
    #[test]
    fn a_stale_playlist_is_ignored() {
        let mut v = viewer_of(2);
        v.switch_to(0);
        v.next(&[0, 1, 99]);
        assert_eq!(v.current_index(), 1);
        v.next(&[0, 1, 99]);
        assert_eq!(v.current_index(), 0, "存在しない 99 へは行かない");
    }

    /// 1 件だけの再生順では動かない。同じ作品へ切り替え直すと絵が巻き戻る。
    #[test]
    fn a_single_item_playlist_does_not_move() {
        let mut v = viewer_of(6);
        v.switch_to(2);
        let epoch = v.epoch();
        v.next(&[2]);
        assert_eq!(v.current_index(), 2);
        assert_eq!(v.epoch(), epoch, "描き直す必要はない");
    }

    /// ランダムでも再生範囲から出ない。
    #[test]
    fn random_stays_inside_the_playlist() {
        let mut v = viewer_of(8);
        let playlist = [0usize, 2, 4];
        v.switch_to(0);
        for _ in 0..30 {
            v.random(&playlist);
            assert!(playlist.contains(&v.current_index()), "外へ出ました: {}", v.current_index());
        }
    }

    /// ランダム送りは必ず絵を変える。同じ作品が出ると止まって見える。
    #[test]
    fn random_always_moves() {
        let mut v = viewer_of(3);
        v.switch_to(0);
        for _ in 0..20 {
            let before = v.current_index();
            v.random(&[0, 1, 2]);
            assert_ne!(v.current_index(), before);
        }
    }
}
