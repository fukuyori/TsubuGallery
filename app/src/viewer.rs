//! 全画面 Viewer の再生状態 (設計書 §8 / §18)。
//!
//! 全作品のインスタンスを常駐させ、切り替えではプロセスもランタイムも作り直さない。
//! 各作品はそれぞれの `frameCount` を保持するので、戻ってきたときも続きから動く。

use tsubu_core::settings::{FrameRate, Navigation, PlaybackSpeed, Settings};
use std::time::Instant;

use tsubu_processing_lite::LoadedSketch;
use tsubu_processing_lite::math::Rng;
use tsubu_renderer::{Graphics, GraphicsState};

/// 1 フレームに積める図形の量を超えた作品に出す理由。
///
/// 予算超過 (§21.1) と同じく、作品を止めたうえで Gallery と Viewer に出す。
pub const TOO_MUCH_GEOMETRY: &str = "1 フレームの図形が多すぎて描けません";

/// 直近フレームの計測値。HUD が表示する。
/// 表示用にならす。跳ねた値をそのまま出すと読めない。
fn smooth(previous: f32, now: f32) -> f32 {
    if previous <= 0.0 { now } else { previous * 0.9 + now * 0.1 }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Stats {
    pub fps: f32,
    pub frame_count: u64,
    /// 作品にとっての経過秒。再生速度をかけて積んだもので、GLSL の `t`。
    pub sketch_time: f32,
    /// 最後の作品切り替えに要した時間 (ms)。
    pub last_switch_ms: f32,
    /// 作品を 1 フレーム進めるのにかかった時間 (ms)。VM と図形の組み立て。
    pub sketch_ms: f32,
    /// 1 フレームぶんの仕事の時間 (ms)。作品、UI、GPU への積み込みまで。
    pub frame_ms: f32,
    /// フレームの間隔 (ms)。実測。
    pub interval_ms: f32,
    /// 仕事の時間 ÷ フレームの間隔。この主スレッドが CPU を使っている割合。
    pub load: f32,
    /// 作品が 1 フレームに実行した命令数。
    pub instructions: u64,
    /// 1 フレームぶんの三角形。
    pub triangles: usize,
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
    /// 作品ごとの描画状態。切り替えても `setup()` の結果を失わないため。
    states: Vec<Option<GraphicsState>>,
    /// 直前のフレームで画面を消さなかった作品。
    ///
    /// 消さない作品は、すでにキャンバスに載っているものを当てにしている。
    /// 切り替えや大きさ変更でキャンバスを捨てたら、頭から動かし直さないと
    /// 地の色ごと失われる。`background()` を最初の 1 回だけ呼ぶ書き方が
    /// あり、そういう作品は白い画面のままになる。
    leans_on_the_canvas: Vec<bool>,
    /// 1 フレームに積める量を超えた作品。描き切れないので止める。
    ///
    /// 作品ごとの状態なので、他の並びと同じ添字で持つ。
    overflowed: Vec<bool>,

    paused: bool,
    /// 作品ごとの経過秒。壁時計ではなく、再生速度をかけて積んだもの。
    ///
    /// GLSL 作品の `t` はここから来る。壁時計をそのまま渡すと、一時停止しても
    /// 動き続け、作り直しても続きから始まってしまう。frameCount と同じく
    /// 作品ごとに持ち、同じ場面で止めたり戻したりできるようにする。
    clocks: Vec<f32>,
    last_frame: Instant,
    fps: f32,
    last_switch_ms: f32,
    /// 直近の実測。急に跳ねると読めないので、ならしてから見せる。
    sketch_ms: f32,
    frame_ms: f32,
    interval_ms: f32,
    rng: Rng,

    width: f32,
    height: f32,

    /// 設計書 §24 の Viewer / Runtime 設定。
    frame_rate: FrameRate,
    /// 作品の時計にかける倍率。フレームレートとは別物 (設計書 §24)。
    speed: PlaybackSpeed,
    navigation: Navigation,
    preload: bool,
    /// 目標フレームレートに合わせるため、次に進めてよくなる時刻。
    next_step: Instant,
}

impl Viewer {
    /// 作品 0 本でも作れる。すべて削除された Gallery でも成立させるため。
    pub fn new(sketches: Vec<LoadedSketch>) -> Self {
        let frame_counts = vec![0; sketches.len()];
        let clocks = vec![0.0; sketches.len()];
        let states = vec![None; sketches.len()];
        let leans_on_the_canvas = vec![false; sketches.len()];
        let overflowed = vec![false; sketches.len()];
        Self {
            states,
            leans_on_the_canvas,
            overflowed,
            sketches,
            frame_counts,
            current: 0,
            epoch: 0,
            graphics: Graphics::new(),
            warmup: Graphics::new(),
            paused: false,
            clocks,
            last_frame: Instant::now(),
            fps: 0.0,
            last_switch_ms: 0.0,
            sketch_ms: 0.0,
            frame_ms: 0.0,
            interval_ms: 0.0,
            rng: Rng::new(0x0073_556E_6275_u64),
            width: 1.0,
            height: 1.0,
            frame_rate: FrameRate::default(),
            speed: PlaybackSpeed::default(),
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

    /// キャンバスを捨てたら、この作品は動かし直す必要があるか。
    ///
    /// 毎フレーム `background()` で塗り直す作品は要らない。溜めた絵や
    /// 一度きりの地塗りを当てにしている作品だけが対象。
    fn leans_on_the_canvas(&self, index: usize) -> bool {
        self.sketches.get(index).is_some_and(LoadedSketch::draws_once)
            || self.leans_on_the_canvas.get(index).copied().unwrap_or(false)
    }

    /// 設定の当てはめ方を描画側の型へ。
    ///
    /// 設定は core、描画は renderer にあり、互いを知らない。
    pub fn to_fit(fit: tsubu_core::settings::CanvasFit) -> tsubu_renderer::CanvasFit {
        match fit {
            tsubu_core::settings::CanvasFit::Contain => tsubu_renderer::CanvasFit::Contain,
            tsubu_core::settings::CanvasFit::Cover => tsubu_renderer::CanvasFit::Cover,
        }
    }

    /// 設定を反映する (設計書 §24)。
    pub fn apply_settings(&mut self, settings: &Settings) {
        self.frame_rate = settings.frame_rate;
        self.speed = settings.playback_speed;
        self.navigation = settings.navigation;
        self.preload = settings.preload;
        // 正方形の作品を横長の画面へ出すときの当てはめ方。
        let fit = Self::to_fit(settings.canvas_fit);
        self.graphics.set_fit(fit);
        self.warmup.set_fit(fit);

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
        if self.overflowed.get(self.current).copied().unwrap_or(false) {
            return Some(TOO_MUCH_GEOMETRY);
        }
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
            self.clocks[index] = 0.0;
            self.states[index] = None;
            self.leans_on_the_canvas[index] = false;
            self.overflowed[index] = false;
            self.graphics.reset_state();
            self.epoch += 1;
        }
    }

    /// 作品を差し込む。Gallery と同じ並びを保つため位置を指定する。
    pub fn insert(&mut self, index: usize, sketch: LoadedSketch) {
        let index = index.min(self.sketches.len());
        self.sketches.insert(index, sketch);
        self.frame_counts.insert(index, 0);
        self.clocks.insert(index, 0.0);
        self.states.insert(index, None);
        self.leans_on_the_canvas.insert(index, false);
        self.overflowed.insert(index, false);
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
        self.clocks.remove(index);
        self.states.remove(index);
        self.leans_on_the_canvas.remove(index);
        self.overflowed.remove(index);
        // 手前が消えたぶん、見ている位置も繰り上がる。そうしないと
        // 別の作品を指したままになる。
        if self.current > index {
            self.current -= 1;
        }
        self.current = self.current.min(self.sketches.len().saturating_sub(1));
        self.graphics.reset_state();
        self.epoch += 1;
    }

    pub fn is_paused(&self) -> bool {
        self.paused
    }

    /// 表示中の作品にとっての経過秒。GLSL の `t` に渡っている値。
    pub fn sketch_time(&self) -> f32 {
        self.clocks.get(self.current).copied().unwrap_or(0.0)
    }

    pub fn toggle_pause(&mut self) {
        self.paused = !self.paused;
    }

    pub fn stats(&self) -> Stats {
        Stats {
            fps: self.fps,
            frame_count: self.frame_counts.get(self.current).copied().unwrap_or(0),
            sketch_time: self.sketch_time(),
            last_switch_ms: self.last_switch_ms,
            sketch_ms: self.sketch_ms,
            frame_ms: self.frame_ms,
            // 間隔より仕事が長ければ、目標のフレームレートには追いつけない。
            interval_ms: self.interval_ms,
            load: if self.interval_ms > 0.0 {
                (self.frame_ms / self.interval_ms).min(1.0)
            } else {
                0.0
            },
            instructions: self
                .sketches
                .get(self.current)
                .map_or(0, LoadedSketch::instructions_last_frame),
            triangles: self.graphics.draw_list().indices.len() / 3,
        }
    }

    /// 1 フレームぶんの仕事にかかった時間を受け取る。
    ///
    /// 作品だけでなく UI と GPU への積み込みまで含めた実測は、呼ぶ側にしか
    /// 分からない。ここでならして負荷にする。
    pub fn note_frame_work(&mut self, elapsed: std::time::Duration) {
        self.frame_ms = smooth(self.frame_ms, elapsed.as_secs_f32() * 1000.0);
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
        // 大きさが変わるとキャンバスは作り直され、溜めた絵は消える。
        // `setup()` の中だけで描く作品は、ここでも動かし直さないと白紙になる。
        let resized = (self.width - width).abs() > 0.5 || (self.height - height).abs() > 0.5;
        self.width = width;
        self.height = height;
        if self.sketches.is_empty() {
            return None;
        }
        if resized && self.leans_on_the_canvas(self.current) {
            self.sketches[self.current].restart();
            self.graphics.reset_state();
        }

        let now = Instant::now();
        let dt = now.duration_since(self.last_frame).as_secs_f32();
        self.last_frame = now;
        if dt > 0.0 {
            // 表示がちらつかない程度に平滑化する。
            let instant_fps = 1.0 / dt;
            self.fps = if self.fps == 0.0 { instant_fps } else { self.fps * 0.9 + instant_fps * 0.1 };
            self.interval_ms = smooth(self.interval_ms, dt * 1000.0);
        }

        // 目標フレームレートより速く回さない。表示の更新はディスプレイに任せ、
        // 作品を進めるかどうかだけをここで決める (設計書 §24 の Frame Rate)。
        let due = now >= self.next_step;
        if due {
            // 遅れを引きずらないよう、次の締め切りは今から数える。倍率で割るのは
            // frameCount 基準の作品も同じ速さで動かすため。2× なら締め切りが
            // 半分になり、1 秒あたり倍のフレームを進める。
            self.next_step = now + self.frame_rate.interval().div_f32(self.speed.multiplier());
        }
        // `noLoop()` を呼んだ作品はフレームを進めない。進めると、乱数を使う
        // 作品が毎フレーム違う絵になってちらつく。
        // 描き切れなかった作品はもう進めない。同じ絵をもう一度組み立てても
        // 同じところで溢れるだけで、そのあいだ画面は止まったままになる。
        let stopped = self.overflowed[self.current];
        let looping = self.graphics.is_looping();
        if !self.paused && looping && !stopped {
            // 時計は毎フレーム進める。frameCount は目標フレームレートの
            // 刻みだが、GLSL の `t` は連続なので、刻むとかくつく。
            self.clocks[self.current] += dt * self.speed.multiplier();
            if due {
                self.frame_counts[self.current] += 1;
            }
        }
        if stopped {
            return Some(&self.graphics);
        }

        self.graphics.begin_frame(width, height);
        self.graphics.frame_count = self.frame_counts[self.current];
        self.graphics.time = self.clocks[self.current];
        let t0 = Instant::now();
        self.sketches[self.current].step(&mut self.graphics);
        self.sketch_ms = smooth(self.sketch_ms, t0.elapsed().as_secs_f32() * 1000.0);
        if self.graphics.overflowed() {
            self.overflowed[self.current] = true;
        }
        // このフレームで画面を消したか。消していなければ、次に開くときは
        // 頭から動かし直す。
        self.leans_on_the_canvas[self.current] = self.graphics.draw_list().clear.is_none();
        Some(&self.graphics)
    }

    pub fn thumbnail_frame_at(&self, index: usize) -> Option<u64> {
        self.sketches.get(index).map(|s| s.info.thumbnail_frame)
    }

    /// 表示中の作品を最初から動かし直す。編集の保存後に使う。
    pub fn restart_current(&mut self) {
        if let Some(sketch) = self.sketches.get_mut(self.current) {
            sketch.restart();
            self.frame_counts[self.current] = 0;
            self.clocks[self.current] = 0.0;
            self.states[self.current] = None;
            self.leans_on_the_canvas[self.current] = false;
            // 動かし直せば、また最初のフレームから試させる。作品を直したあとの
            // 保存でここへ来るので、直っていれば動く。
            self.overflowed[self.current] = false;
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
        // いまの作品の状態を預ける。`setup()` はもう二度と走らないので、
        // ここで取っておかないと塗りや線の色が既定へ戻ってしまう。
        self.states[self.current] = Some(self.graphics.state());
        self.current = index;
        match self.states[index].clone() {
            Some(state) => self.graphics.set_state(state),
            None => self.graphics.reset_state(),
        }
        // 切り替えると溜めた絵は捨てられる。それを当てにしている作品は
        // 描き直す者がいないので、ここで最初から動かし直す。
        if self.leans_on_the_canvas(index) {
            self.sketches[index].restart();
            self.states[index] = None;
            self.graphics.reset_state();
        }
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
            // `setup()` の中だけで描く作品を温めても、絵は warmup 側の
            // キャンバスへ行って消える。切り替えのときに動かし直す。
            if self.sketches[index].initialized || self.leans_on_the_canvas(index) {
                continue;
            }
            self.warmup.reset_state();
            self.warmup.begin_frame(self.width, self.height);
            self.warmup.frame_count = self.frame_counts[index];
            self.sketches[index].step(&mut self.warmup);
            // 温めた時点で溢れたなら、開いても描けない。ここで印を付けておくと、
            // 切り替えた瞬間に理由が出る。
            if self.warmup.overflowed() {
                self.overflowed[index] = true;
            }
            // `setup()` はこの warmup 側で走ってしまった。結果を預けておかないと、
            // 切り替えたときに `setup()` で決めた色や大きさが失われる。
            self.states[index] = Some(self.warmup.state());
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

    /// 何フレームか進めて、作品の時計を動かす。
    fn run(v: &mut Viewer, frames: usize) {
        for _ in 0..frames {
            v.render_frame(64.0, 64.0);
        }
    }

    /// 一時停止したら作品の時計も止まる。
    ///
    /// GLSL 作品の `t` はここから来る。壁時計のままだと、Space を押しても
    /// 絵が動き続ける。
    #[test]
    fn pausing_freezes_the_clock() {
        let mut v = viewer_of(2);
        run(&mut v, 8);
        assert!(v.sketch_time() > 0.0, "動いていない");

        v.toggle_pause();
        run(&mut v, 2);
        let stopped_at = v.sketch_time();
        run(&mut v, 8);
        assert_eq!(v.sketch_time(), stopped_at, "止めたのに進んでいる");

        v.toggle_pause();
        run(&mut v, 8);
        assert!(v.sketch_time() > stopped_at, "再開しても止まったまま");
    }

    /// 動かし直したら時計も 0 から。
    #[test]
    fn restarting_puts_the_clock_back_to_zero() {
        let mut v = viewer_of(2);
        run(&mut v, 8);
        assert!(v.sketch_time() > 0.0);
        v.restart_current();
        assert_eq!(v.sketch_time(), 0.0);
    }

    /// 時計は作品ごと。別の作品を見ているあいだは進まない。
    #[test]
    fn each_sketch_keeps_its_own_clock() {
        let mut v = viewer_of(2);
        run(&mut v, 8);
        let first = v.sketch_time();
        assert!(first > 0.0);

        v.switch_to(1);
        assert_eq!(v.sketch_time(), 0.0, "2 本目は 0 から始まる");
        run(&mut v, 8);

        v.switch_to(0);
        assert_eq!(v.sketch_time(), first, "見ていないあいだに進んでいる");
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

    /// `setup()` で決めた色が、作品を切り替えても失われない。
    ///
    /// Viewer は 1 つの Graphics を全作品で使い回す。切り替えのたびに初期状態へ
    /// 戻していたので、`stroke(-1)` だけを `setup()` に書いた作品は、
    /// 一度離れて戻ると線が黒になった。`clear()` の黒地と重なって画面が
    /// 真っ黒のままになる。`setup()` は作品ごとに一度きりなので直らない。
    #[test]
    fn what_setup_decided_survives_switching_away_and_back() {
        use tsubu_processing_lite::VmSketch;

        let sketch = |id: &str, src: &str| {
            LoadedSketch::new(
                SketchInfo { id: id.into(), title: id.into(), thumbnail_frame: 1 },
                Box::new(VmSketch::compile(src, 1).expect("コンパイルできる")),
            )
        };
        let mut viewer = Viewer::new(vec![
            // setup() でだけ白い線とキャンバスを決める作品。
            sketch(
                "white",
                "void setup(){ size(720, 720); stroke(-1); }\n\
                 void draw(){ clear(); line(0, 0, 100, 100); }",
            ),
            sketch("other", "void draw(){ background(0); circle(50, 50, 10); }"),
        ]);

        fn line_color(v: &mut Viewer) -> (f32, f32, f32) {
            let g = v.render_frame(200.0, 100.0).expect("描かれる");
            let c = g.draw_list().vertices.first().expect("線がある").color;
            (c[0], c[1], c[2])
        }

        assert_eq!(line_color(&mut viewer), (1.0, 1.0, 1.0), "最初から白くない");
        viewer.switch_to(1);
        viewer.render_frame(200.0, 100.0);
        viewer.switch_to(0);
        assert_eq!(line_color(&mut viewer), (1.0, 1.0, 1.0), "戻ったら線が黒くなりました");
    }

    /// 先読みした作品も、切り替えたときに `setup()` の結果を持っている。
    ///
    /// 先読みは別の Graphics で `setup()` を走らせる。そちらの結果を
    /// 預けておかないと、先読みした作品にかぎって色が既定へ戻る。
    #[test]
    fn a_preloaded_sketch_keeps_what_its_setup_decided() {
        use tsubu_processing_lite::VmSketch;

        let sketch = |id: &str, src: &str| {
            LoadedSketch::new(
                SketchInfo { id: id.into(), title: id.into(), thumbnail_frame: 1 },
                Box::new(VmSketch::compile(src, 1).expect("コンパイルできる")),
            )
        };
        let mut viewer = Viewer::new(vec![
            sketch("first", "void draw(){ background(0); circle(50, 50, 10); }"),
            sketch(
                "white",
                "void setup(){ size(720, 720); stroke(-1); }\n\
                 void draw(){ clear(); line(0, 0, 100, 100); }",
            ),
        ]);
        // 隣を先読みすると、1 の `setup()` は warmup 側の Graphics で走る。
        viewer.width = 200.0;
        viewer.height = 100.0;
        viewer.preload_neighbours();
        assert!(viewer.sketches[1].initialized, "先読みが走っていません");
        viewer.switch_to(1);
        let g = viewer.render_frame(200.0, 100.0).expect("描かれる");
        let c = g.draw_list().vertices.first().expect("線がある").color;
        assert_eq!((c[0], c[1], c[2]), (1.0, 1.0, 1.0), "先読みした作品の線が黒です");
    }

    /// `setup()` の中だけで描く作品は、戻ってきたら描き直す。
    ///
    /// 静的モード (設計書 §14.1) の作品は `draw()` を持たない。切り替えると
    /// 溜めた絵は捨てられるので、描き直す者がいないと白紙のままになる。
    /// 同じ乱数の数列から始めるので、絵はサムネイルとも一致する。
    #[test]
    fn a_sketch_that_draws_only_in_setup_is_run_again_when_shown_again() {
        use tsubu_processing_lite::VmSketch;

        let sketch = |id: &str, src: &str| {
            LoadedSketch::new(
                SketchInfo { id: id.into(), title: id.into(), thumbnail_frame: 1 },
                Box::new(VmSketch::compile(src, 1).expect("コンパイルできる")),
            )
        };
        // 静的モード。`draw()` が無く、絵は 1 度きり。
        let mut viewer = Viewer::new(vec![
            sketch("static", "size(400, 400);\nline(0, 0, random(100), 50);"),
            sketch("other", "void draw(){ background(0); circle(50, 50, 10); }"),
        ]);

        let first = {
            let g = viewer.render_frame(200.0, 100.0).expect("描かれる");
            let v = &g.draw_list().vertices;
            assert!(!v.is_empty(), "静的モードの作品が描かれていません");
            v.iter().map(|p| p.pos[0]).fold(f32::MIN, f32::max)
        };

        viewer.switch_to(1);
        viewer.render_frame(200.0, 100.0);
        viewer.switch_to(0);

        let g = viewer.render_frame(200.0, 100.0).expect("描かれる");
        let v = &g.draw_list().vertices;
        assert!(!v.is_empty(), "戻ったら白紙になりました");
        // 乱数も戻すので、同じ絵が出る。
        let again = v.iter().map(|p| p.pos[0]).fold(f32::MIN, f32::max);
        assert!((first - again).abs() < 0.01, "違う絵になりました: {first} と {again}");
    }

    /// 窓の大きさが変わっても、`setup()` の中だけで描く作品は消えない。
    #[test]
    fn resizing_does_not_wipe_a_sketch_that_draws_only_in_setup() {
        use tsubu_processing_lite::VmSketch;

        let mut viewer = Viewer::new(vec![LoadedSketch::new(
            SketchInfo { id: "s".into(), title: "s".into(), thumbnail_frame: 1 },
            Box::new(
                VmSketch::compile("size(400, 400);\nline(0, 0, 100, 50);", 1)
                    .expect("コンパイルできる"),
            ),
        )]);

        assert!(!viewer.render_frame(200.0, 100.0).expect("描かれる").draw_list().is_empty());
        // 大きさが変わるとキャンバスは作り直される。
        let g = viewer.render_frame(400.0, 300.0).expect("描かれる");
        assert!(!g.draw_list().is_empty(), "大きさを変えたら白紙になりました");
    }

    /// 負荷は仕事の時間をフレームの間隔で割ったもの。
    ///
    /// 画面の空きを待つ時間は仕事に数えない。数えると、何を映していても
    /// いつも 100% に見えて役に立たなくなる。
    #[test]
    fn the_load_is_work_over_wall_time() {
        let mut viewer = viewer_of(1);
        // 16ms 間隔で 4ms 働いた、という状態を作る。
        for _ in 0..40 {
            viewer.interval_ms = smooth(viewer.interval_ms, 16.0);
            viewer.note_frame_work(std::time::Duration::from_micros(4000));
        }
        let stats = viewer.stats();
        assert!((stats.load - 0.25).abs() < 0.02, "負荷が合いません: {}", stats.load);

        // 間隔を超えて働いても 100% で止まる。1 本の糸はそれ以上使えない。
        for _ in 0..80 {
            viewer.note_frame_work(std::time::Duration::from_micros(40_000));
        }
        assert!((viewer.stats().load - 1.0).abs() < 0.001, "{}", viewer.stats().load);
    }

    /// 手前の作品を消したら、見ている位置も繰り上がる。
    ///
    /// 繰り上げないと別の作品を指したままになる。そのあと新しい作品を
    /// 足して開こうとしたとき、位置がたまたま一致すると
    /// 「もう見ている」と判断されて切り替わらない。
    #[test]
    fn removing_an_earlier_sketch_keeps_pointing_at_the_same_one() {
        let mut viewer = viewer_of(5);
        viewer.switch_to(3);
        viewer.remove(1);
        assert_eq!(viewer.current_index(), 2, "指す先がずれました");

        // 後ろを消しても動かない。
        viewer.remove(3);
        assert_eq!(viewer.current_index(), 2);

        // 見ているものを消したら、その位置に繰り上がってきたものを見る。
        viewer.remove(2);
        assert_eq!(viewer.current_index(), 2.min(viewer.len().saturating_sub(1)));
    }

    /// 地を一度しか塗らない作品も、戻ってきたら描き直す。
    ///
    /// `f++ || background(0)` のように最初の 1 フレームだけ塗る書き方がある。
    /// 切り替えでキャンバスを捨てたあと、そのまま続きを描くと、白い地に
    /// 白い線を引くことになって何も見えない。
    #[test]
    fn a_sketch_that_paints_its_ground_once_is_run_again_when_shown_again() {
        use tsubu_processing_lite::VmSketch;

        let sketch = |id: &str, src: &str| {
            LoadedSketch::new(
                SketchInfo { id: id.into(), title: id.into(), thumbnail_frame: 1 },
                Box::new(VmSketch::compile(src, 1).expect("コンパイルできる")),
            )
        };
        let mut viewer = Viewer::new(vec![
            sketch(
                "once",
                "f=0\ndraw=_=>{f++||background(0);stroke(255);line(0,0,50,50)}",
            ),
            sketch("other", "void draw(){ background(0); circle(50, 50, 10); }"),
        ]);

        // 1 フレーム目は地を塗る。2 フレーム目からは塗らない。
        assert!(viewer.render_frame(200.0, 100.0).expect("描かれる").draw_list().clear.is_some());
        assert!(viewer.render_frame(200.0, 100.0).expect("描かれる").draw_list().clear.is_none());

        viewer.switch_to(1);
        viewer.render_frame(200.0, 100.0);
        viewer.switch_to(0);

        // 戻ってきたら、また地から塗り直す。
        let g = viewer.render_frame(200.0, 100.0).expect("描かれる");
        assert!(g.draw_list().clear.is_some(), "地を塗り直していません。白いままになります");
    }

    /// 毎フレーム塗り直す作品は、切り替えても動かし直さない。
    ///
    /// 続きから見せるのが本来 (設計書 §18)。動かし直す必要があるのは、
    /// すでにキャンバスに載っているものを当てにしている作品だけ。
    #[test]
    fn a_sketch_that_clears_every_frame_keeps_running() {
        use tsubu_processing_lite::VmSketch;

        let sketch = |id: &str, src: &str| {
            LoadedSketch::new(
                SketchInfo { id: id.into(), title: id.into(), thumbnail_frame: 1 },
                Box::new(VmSketch::compile(src, 1).expect("コンパイルできる")),
            )
        };
        let mut viewer = Viewer::new(vec![
            sketch("clears", "c=0\ndraw=_=>{background(0);c++;circle(c,50,10)}"),
            sketch("other", "void draw(){ background(0); circle(50, 50, 10); }"),
        ]);
        viewer.render_frame(200.0, 100.0);
        viewer.switch_to(1);
        viewer.render_frame(200.0, 100.0);
        viewer.switch_to(0);
        // 動かし直されていない = `setup()` から走り直さない。
        assert!(viewer.sketches[0].initialized, "続きから見せるはずが、頭へ戻りました");
    }
}
