//! TsubuGallery — Prototype A〜D。
//!
//! - A: 固定スケッチを全画面 60fps で描画する
//! - B: 複数スケッチをメモリに保持し、再起動せず瞬時に切り替える
//! - C: 同一 Renderer からフレームを取得し、サムネイル画像として保存する
//! - D: スクリーンショットをグリッド表示し、選択した作品を Viewer へ渡す
//! - E: 外部の Processing Lite コードを Parser / AST / Bytecode 経由で実行する

mod alert;
mod editing;
mod editor;
mod editor_ui;
mod fonts;
mod gallery_ui;
mod gfx;
mod headless;
mod loader;
mod settings_ui;
mod theme;
mod thumbnail;
mod ui;
mod viewer;
mod viewer_ui;

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use tsubu_core::repository::{self, CompileStatus, Repository, SketchMeta};
use tsubu_core::settings::{Choice, PlaybackSpeed, Settings, StartScreen, ViewMode};
use tsubu_core::{DataPaths, InstanceLock, LanguagePreference, Locales, LockError};
use tsubu_gallery::model::{GalleryItem, SketchStatus, ThumbnailState};
use tsubu_gallery::{GalleryView, Move};
use winit::application::ApplicationHandler;
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Fullscreen, Icon, Window, WindowId};

use editor::Editor;
use editor_ui::EditorAction;
use gallery_ui::{GalleryAction, GalleryOutput, GalleryUi};
use gfx::Gfx;
use settings_ui::{SettingsAction, SettingsUi};
use thumbnail::ThumbnailLoader;
use tsubu_renderer::{Capturer, Color, SAMPLE_COUNT};
use ui::{UiFrame, UiLayer};
use viewer::Viewer;
use viewer_ui::ViewerOverlay;
use wgpu::CurrentSurfaceTexture;

/// Gallery の地。egui のパネルが上に乗る。
const GALLERY_BACKGROUND: Color = Color::rgba(0.07, 0.07, 0.08, 1.0);
/// 1 フレームに GPU で作るサムネイルの上限。Gallery の応答性を優先する (§22)。
const GPU_THUMBNAILS_PER_FRAME: usize = 1;
/// 1 フレームに投げるディスク読み込みの上限。
const DISK_THUMBNAILS_PER_FRAME: usize = 4;

/// 使い方。`--help` で出す。
const USAGE: &str = "\
TsubuGallery — 短い Processing / p5.js / GLSL 作品のギャラリー

  tsubugallery                       ギャラリーを開く
  tsubugallery --capture-all [DIR]   全作品のサムネイルを作って終了
  tsubugallery --version             版を表示
  tsubugallery --help                この説明

同じデータ領域を 2 つ同時には開けません。並べて動かすには TSUBU_DATA_DIR を
分けてください。

ログ
  <データ領域>/logs/tsubu.log に、直す先のある出来事だけを 1 行 1 件で残します。
  動かない作品は `ERROR sketch` の行に、ファイルと行・列まで入ります。

環境変数
  TSUBU_DATA_DIR       データ領域の場所
  TSUBU_START_SCREEN   起動画面を上書き: gallery / viewer / editor / settings
  RUST_LOG             ログの詳しさ (既定 warn。info / debug で増やす)
";

fn main() {
    let paths = DataPaths::resolve();
    // ログの置き場は作っておく。ここで失敗しても、標準エラーには出せる。
    let _ = std::fs::create_dir_all(paths.logs());
    let log_file = tsubu_core::logging::init(&paths);
    tsubu_core::logging::install_panic_hook();

    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--help" || a == "-h") {
        print!("{USAGE}");
        return;
    }
    if args.iter().any(|a| a == "--version" || a == "-V") {
        println!("tsubugallery {}", env!("CARGO_PKG_VERSION"));
        match log_file {
            Some(path) => println!("ログ: {}", path.display()),
            None => println!("ログ: ファイルへは書けません"),
        }
        return;
    }

    // 同じデータ領域を 2 つ動かさない。SQLite の書き込みが競り、サムネイルの
    // 生成も二重になる。ロックは終了時に OS が外すので、落ちても残らない。
    //
    // このガードは main が終わるまで持ち続ける。途中で捨てるとロックが外れる。
    let _instance = match InstanceLock::acquire(paths.root()) {
        Ok(lock) => lock,
        Err(LockError::AlreadyRunning { pid }) => {
            let locales = Locales::builtin();
            let mut message = locales.t("app.already_running").to_string();
            if let Some(pid) = pid {
                message.push_str(&format!(" (pid {pid})"));
            }
            eprintln!("{message}");
            eprintln!("{}", locales.t("app.already_running.hint"));
            log::error!("{} は使用中なので起動できません", paths.root().display());
            std::process::exit(1);
        }
        Err(e) => {
            // ロックが取れない環境でも起動はさせる。読み取り専用の場所に
            // データ領域を置いた、といった事情が考えられる。
            log::warn!("多重起動を防げません: {e}");
            eprintln!("{}", Locales::builtin().t("app.lock_failed"));
            // ガードの型を合わせられないので、ここから先はロック無しで進む。
            return run(paths);
        }
    };

    run(paths)
}

/// ロックを取ったあとの本体。
fn run(paths: DataPaths) {
    if let Some(dir) = capture_all_target() {
        // 保存済みの設定に従う。ウィンドウ版で撮ったサムネイルと解像度を
        // 揃えたいので、画質だけはここでも読む。
        let settings = Repository::open(&paths.database())
            .and_then(|r| r.settings())
            .unwrap_or_else(|e| {
                log::warn!("設定を読めないので既定値を使います: {e}");
                Settings::default()
            });
        let width = settings.image_quality.width();
        match headless::capture_all(
            &paths,
            &dir,
            width,
            width * 10 / 16,
            settings.capture_frame,
            crate::viewer::Viewer::to_fit(settings.canvas_fit),
        ) {
            Ok(paths) => {
                println!("{} 件のサムネイルを生成しました\n{}", paths.len(), paths.join("\n"))
            }
            Err(e) => {
                eprintln!("{e}");
                std::process::exit(1);
            }
        }
        return;
    }

    let event_loop = EventLoop::new().expect("イベントループを作成できませんでした");
    // アニメーションを回し続けるので、待機せず次のフレームへ進む。
    event_loop.set_control_flow(ControlFlow::Poll);

    let mut app = App::new(paths);
    if let Err(e) = event_loop.run_app(&mut app) {
        log::error!("イベントループが異常終了しました: {e}");
    }
}

/// `--capture-all [DIR]` が指定されていれば、その出力先を返す。
fn capture_all_target() -> Option<std::path::PathBuf> {
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--capture-all" {
            let dir = args
                .next()
                .map(std::path::PathBuf::from)
                .unwrap_or_else(|| DataPaths::resolve().thumbnails());
            return Some(dir);
        }
    }
    None
}

/// ウィンドウとタスクバーに出すアイコン。
///
/// Windows では exe に埋めた資源が使われるのでこれが無くても出るが、Linux では
/// 渡さないと環境ごとの既定の絵になる。同じ 1 枚を両方に使う。
///
/// 読めなくても起動は続ける。絵が出ないだけで、できることは変わらない。
fn window_icon() -> Option<Icon> {
    let png = include_bytes!("../assets/icon.png");
    let image = match image::load_from_memory_with_format(png, image::ImageFormat::Png) {
        Ok(image) => image.into_rgba8(),
        Err(e) => {
            log::warn!("アイコンを読めませんでした: {e}");
            return None;
        }
    };
    let (width, height) = image.dimensions();
    Icon::from_rgba(image.into_raw(), width, height)
        .inspect_err(|e| log::warn!("アイコンを作れませんでした: {e}"))
        .ok()
}

/// 表示中の画面。ホームは Gallery (設計書 §6.1)。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Screen {
    Gallery,
    Viewer,
    Editor,
    Settings,
}

/// 起動画面を上書きする環境変数。
///
/// 通常は設定 (§24 の `General / Start Screen`) で決めるが、開発中に一発で
/// 別の画面を出したいことがあるので残してある。指定があればそちらが勝つ。
const START_SCREEN_ENV: &str = "TSUBU_START_SCREEN";

impl Screen {
    /// 設定と環境変数から起動画面を決める。環境変数が優先。
    fn initial(settings: &Settings) -> Self {
        let from_settings = match settings.start_screen {
            StartScreen::Gallery => Screen::Gallery,
            StartScreen::Viewer => Screen::Viewer,
        };
        match std::env::var(START_SCREEN_ENV).as_deref() {
            Ok("gallery") => Screen::Gallery,
            Ok("viewer") => Screen::Viewer,
            Ok("editor") => Screen::Editor,
            Ok("settings") => Screen::Settings,
            Ok(other) if !other.is_empty() => {
                log::warn!("{START_SCREEN_ENV}={other} は不明な値です。設定に従います。");
                from_settings
            }
            _ => from_settings,
        }
    }
}

/// スクリーンセーバーに入る前の状態。抜けたらここへ戻す。
#[derive(Clone, Copy, Debug)]
struct ScreensaverReturn {
    screen: Screen,
    fullscreen: bool,
}

/// これから作るサムネイル 1 件。
struct ThumbnailJob {
    index: usize,
    frame: u64,
    /// ディスクに画像があっても作り直す (手動更新, §7.2)。
    force: bool,
}

struct App {
    window: Option<Arc<Window>>,
    gfx: Option<Gfx>,
    ui: Option<UiLayer>,

    viewer: Viewer,
    gallery: GalleryView,
    /// サムネイル生成用に作品を作り直すための元ソース。
    sources: Vec<loader::Source>,
    /// サムネイル生成専用の描画コンテキスト。表示中の絵とは混ぜない。
    thumb_graphics: tsubu_renderer::Graphics,
    textures: HashMap<String, egui::TextureHandle>,
    loader: ThumbnailLoader,

    locales: Locales,
    paths: DataPaths,
    capturer: Capturer,
    /// メタデータの保存先 (設計書 §19)。開けなければ None で動き続ける。
    repository: Option<Repository>,
    /// 絞り込みに出すタグ一覧。
    tags: Vec<String>,
    /// 既知のコレクション名 (設計書 §27)。
    collections: Vec<String>,
    /// コレクションの割り当て中なら、その作品。
    assigning: Option<usize>,

    screen: Screen,
    show_info: bool,
    fullscreen: bool,
    mouse: (f32, f32),
    mouse_pressed: bool,
    /// カーソルがこの窓の上にあるか。全画面でも、別のモニタへ出ていれば false。
    cursor_inside: bool,
    /// キーボードで選択が動いた直後だけ true。
    scroll_to_selected: bool,
    /// 手動更新など、優先して処理するサムネイル。
    requested_thumbnail: Option<ThumbnailJob>,
    /// 編集中の作品。
    editor: Option<Editor>,
    /// 削除の確認待ち。
    /// 最後にキャンバスを消したときの Viewer の世代。
    canvas_epoch: u64,
    pending_delete: Option<usize>,
    /// 編集を閉じたときに戻る画面。
    return_screen: Screen,
    /// スライドショー中なら、次に送る時刻 (設計書 §27)。
    slideshow: Option<Instant>,
    /// 最後にユーザーが何かした時刻。スクリーンセーバーの起点。
    last_input: Instant,
    /// スクリーンセーバーとして自動で入ったか。抜けるときに元へ戻す。
    screensaver: Option<ScreensaverReturn>,
    /// 設定 (設計書 §24)。変えた瞬間に効かせて DB へ書く。
    settings: Settings,
    /// 設定画面を閉じたときに戻る画面。
    settings_return: Screen,
}

impl App {
    /// `paths` は main が解決したもの。ロックを取った領域と必ず同じにする。
    fn new(paths: DataPaths) -> Self {
        if let Err(e) = paths.ensure() {
            log::error!("データディレクトリを準備できませんでした: {e}");
        }
        log::info!("データ領域: {}", paths.root().display());

        let mut locales = Locales::builtin();
        // 追加言語は locales/ に JSON を置くだけで増やせる (設計書 §11)。
        locales.load_dir(std::path::Path::new("locales"));
        locales.load_dir(&paths.root().join("locales"));

        let mut repository = match Repository::open(&paths.database()) {
            Ok(r) => {
                log::info!(
                    "メタデータ DB: {} (schema {})",
                    paths.database().display(),
                    r.schema_version().unwrap_or(-1)
                );
                Some(r)
            }
            Err(e) => {
                // メタデータが無くても作品は動く。お気に入りとタグを諦めるだけ。
                log::error!("メタデータ DB を開けませんでした: {e}");
                None
            }
        };

        let settings = match repository.as_ref().map(|r| r.settings()) {
            Some(Ok(s)) => s,
            Some(Err(e)) => {
                log::error!("設定を読めませんでした: {e}");
                Settings::default()
            }
            None => Settings::default(),
        };
        locales.set_preference(settings.language.clone());
        log::info!("UI 言語: {}", locales.active_tag());

        let outcomes = loader::load_library(&paths);
        let mut items = Vec::with_capacity(outcomes.len());
        let mut sketches = Vec::with_capacity(outcomes.len());
        let mut sources = Vec::with_capacity(outcomes.len());
        for outcome in outcomes {
            // 作成日時はこのあと sync_metadata が DB から入れ直す。
            let mut item = GalleryItem::new(&outcome.sketch.info.id, &outcome.sketch.info.title, 0);
            item.dialect = outcome.sketch.dialect().map(|d| d.label().to_string());
            if let Some(error) = outcome.error {
                item.status = SketchStatus::Error(error);
            }
            items.push(item);
            sketches.push(outcome.sketch);
            sources.push(outcome.source);
        }
        log::info!("{} 件の作品を読み込みました", sketches.len());

        let tags = Self::sync_metadata(repository.as_mut(), &mut items, &sources);
        let collections =
            repository.as_ref().and_then(|r| r.collections().ok()).unwrap_or_default();

        let mut viewer = Viewer::new(sketches);
        viewer.apply_settings(&settings);

        // 作品の `text()` は OS のフォントを借りる。無ければ文字は出ない。
        let mut thumb_graphics = tsubu_renderer::Graphics::new();
        thumb_graphics.set_fit(Viewer::to_fit(settings.canvas_fit));
        let sketch_fonts = fonts::load_sketch_fonts();
        if !sketch_fonts.is_empty() {
            viewer.set_fonts(sketch_fonts.clone());
            thumb_graphics.font.set_fonts(sketch_fonts);
        }
        let mut gallery = GalleryView::new(items);
        gallery.set_sort(settings.sort_order);

        Self {
            window: None,
            gfx: None,
            ui: None,
            viewer,
            gallery,
            sources,
            thumb_graphics,
            textures: HashMap::new(),
            loader: ThumbnailLoader::new(),
            locales,
            paths,
            capturer: Capturer::new(),
            repository,
            tags,
            screen: Screen::initial(&settings),
            show_info: false,
            fullscreen: settings.fullscreen,
            mouse: (0.0, 0.0),
            mouse_pressed: false,
            cursor_inside: false,
            scroll_to_selected: false,
            requested_thumbnail: None,
            editor: None,
            canvas_epoch: 0,
            pending_delete: None,
            collections,
            assigning: None,
            return_screen: Screen::Gallery,
            settings_return: Screen::Gallery,
            slideshow: None,
            last_input: Instant::now(),
            screensaver: None,
            settings,
        }
    }

    /// 設定を保存し、動いている各所へ反映する。
    fn apply_settings(&mut self) {
        self.locales.set_preference(self.settings.language.clone());
        self.gallery.set_sort(self.settings.sort_order);
        self.viewer.apply_settings(&self.settings);

        if let Some(ui) = self.ui.as_mut() {
            ui.set_theme(self.settings.theme);
        }
        if self.fullscreen != self.settings.fullscreen {
            self.toggle_fullscreen();
        }

        if let Some(repo) = self.repository.as_mut()
            && let Err(e) = repo.save_settings(&self.settings)
        {
            log::error!("設定を保存できませんでした: {e}");
        }
    }

    /// 再生速度を 1 段変える。設定画面で選ぶのと同じもので、保存もされる。
    ///
    /// 見ている最中に「もっとゆっくり」と思ったときに設定画面まで行かせない。
    fn change_speed(&mut self, step: fn(PlaybackSpeed) -> PlaybackSpeed) {
        let next = step(self.settings.playback_speed);
        if next == self.settings.playback_speed {
            return;
        }
        self.settings.playback_speed = next;
        log::debug!("再生速度を {next}× にしました");
        self.apply_settings();
    }

    /// 起動を諦めるときに、その理由をダイアログで出す。
    ///
    /// 翻訳した一文の下に、生のエラーをそのまま添える。上は利用者が読むもの、
    /// 下は問い合わせを受けたときにこちらが見る材料。ログにも同じものが残るが、
    /// ログの場所を知らない相手には、この画面が唯一の手がかりになる。
    fn report_fatal(&self, key: &str, detail: &str) {
        alert::fatal(&format!("{}\n\n{detail}", self.locales.t(key)));
    }

    /// 表示方式を順に切り替える (設計書 §6.2)。
    ///
    /// 設定画面からも変えられるが、見比べたいものなので一覧から直接切り替える。
    fn cycle_view_mode(&mut self) {
        let all = <ViewMode as Choice>::ALL;
        let next = all
            .iter()
            .position(|m| *m == self.settings.view_mode)
            .map_or(0, |i| (i + 1) % all.len());
        self.settings.view_mode = all[next];
        log::debug!("表示方式を {} にしました", self.settings.view_mode);
        self.apply_settings();
    }

    // ---- コレクション (設計書 §27) --------------------------------------

    /// 作品をコレクションへ出し入れする。
    fn set_collection(&mut self, index: usize, name: &str, member: bool) {
        let Some(id) = self.gallery.items().get(index).map(|i| i.id.clone()) else { return };

        if let Some(repo) = self.repository.as_mut() {
            let now = repository::now();
            let result = if member {
                repo.add_to_collection(name, &id, now)
            } else {
                repo.remove_from_collection(name, &id)
            };
            if let Err(e) = result {
                log::error!("コレクションを更新できませんでした: {e}");
                return;
            }
        }

        self.gallery.set_collection(index, name, member);
        self.refresh_collections();
    }

    fn delete_collection(&mut self, name: &str) {
        if let Some(repo) = self.repository.as_mut()
            && let Err(e) = repo.delete_collection(name)
        {
            log::error!("コレクションを消せませんでした: {e}");
            return;
        }
        self.gallery.remove_collection(name);
        self.refresh_collections();
    }

    /// コレクション一覧を DB から読み直す。
    ///
    /// 絞り込みに使っているコレクションが消えたら、絞り込みも外す。そうしないと
    /// 一覧が空のまま戻せなくなる。
    fn refresh_collections(&mut self) {
        if let Some(repo) = self.repository.as_ref() {
            self.collections = repo.collections().unwrap_or_default();
        }
        let mut filter = self.gallery.filter().clone();
        if let Some(current) = &filter.collection
            && !self.collections.contains(current)
        {
            filter.collection = None;
            self.gallery.set_filter(filter);
        }
    }

    // ---- 再生 (設計書 §27) ---------------------------------------------

    /// 再生する順番。Gallery で絞り込んだ結果をそのまま使う。
    ///
    /// 「お気に入りだけ」「このタグだけ」で絞ってから再生すれば、それが
    /// そのままプレイリストになる。
    fn playlist(&self) -> Vec<usize> {
        self.gallery.visible().to_vec()
    }

    /// 再生順の中で前後に動き、Gallery の選択も合わせる。
    fn advance(&mut self, delta: i32) {
        let order = self.playlist();
        if delta >= 0 {
            self.viewer.next(&order);
        } else {
            self.viewer.previous(&order);
        }
        self.gallery.select(self.viewer.current_index());
        self.restart_slideshow_timer();
    }

    fn slideshow_interval(&self) -> Duration {
        Duration::from_secs(self.settings.slideshow_interval.max(1) as u64)
    }

    /// 手で送ったあとは、そこからまた間隔いっぱい見せる。
    fn restart_slideshow_timer(&mut self) {
        if self.slideshow.is_some() {
            self.slideshow = Some(Instant::now() + self.slideshow_interval());
        }
    }

    fn toggle_slideshow(&mut self) {
        if self.slideshow.take().is_some() {
            log::debug!("スライドショーを止めました");
            return;
        }
        if self.viewer.is_empty() {
            return;
        }
        if self.screen != Screen::Viewer {
            self.open_viewer(self.gallery.selected().unwrap_or(0));
        }
        self.slideshow = Some(Instant::now() + self.slideshow_interval());
        log::debug!("スライドショーを始めました ({} 秒ごと)", self.settings.slideshow_interval);
    }

    /// スライドショーの時計を見て、頃合いなら次へ送る。
    fn pump_slideshow(&mut self) {
        let Some(at) = self.slideshow else { return };
        // Viewer から離れたら止める。編集中に絵が変わっては困る。
        if self.screen != Screen::Viewer {
            self.slideshow = None;
            return;
        }
        if Instant::now() < at {
            return;
        }
        let order = self.playlist();
        self.viewer.next(&order);
        self.gallery.select(self.viewer.current_index());
        self.slideshow = Some(Instant::now() + self.slideshow_interval());
    }

    /// 無操作が続いたらスクリーンセーバーを始める (設計書 §27)。
    fn pump_screensaver(&mut self) {
        if self.screensaver.is_some() {
            return;
        }
        // 自分でスライドショーを始めているなら、もう見せるものは出ている。
        if self.slideshow.is_some() {
            return;
        }
        let Some(idle) = self.settings.screensaver.idle() else { return };
        // 編集中と設定中は入らない。手を止めて画面を読んでいるだけのことがある。
        if !matches!(self.screen, Screen::Gallery | Screen::Viewer) {
            return;
        }
        if self.viewer.is_empty() || self.last_input.elapsed() < idle {
            return;
        }

        log::info!("スクリーンセーバーを始めます");
        self.screensaver =
            Some(ScreensaverReturn { screen: self.screen, fullscreen: self.fullscreen });
        if !self.fullscreen {
            self.toggle_fullscreen();
        }
        if self.screen != Screen::Viewer {
            self.open_viewer(self.gallery.selected().unwrap_or(0));
        }
        self.slideshow = Some(Instant::now() + self.slideshow_interval());
    }

    /// 何か操作されたときに呼ぶ。スクリーンセーバー中なら元の画面へ戻す。
    ///
    /// 戻り値が `true` なら、その操作は「セーバーを解除する」ためだけに使い、
    /// 画面へは渡さない。解除の一打で作品が消えたりしては困る。
    fn note_input(&mut self) -> bool {
        self.last_input = Instant::now();
        let Some(back) = self.screensaver.take() else { return false };

        log::info!("スクリーンセーバーを抜けます");
        self.slideshow = None;
        if self.fullscreen != back.fullscreen {
            self.toggle_fullscreen();
        }
        self.screen = match back.screen {
            Screen::Viewer if self.viewer.is_empty() => Screen::Gallery,
            other => other,
        };
        true
    }

    fn open_settings(&mut self) {
        self.settings_return = self.screen;
        self.screen = Screen::Settings;
    }

    fn close_settings(&mut self) {
        self.screen = match self.settings_return {
            Screen::Viewer if !self.viewer.is_empty() => Screen::Viewer,
            Screen::Settings => Screen::Gallery,
            other => other,
        };
    }

    /// ファイル側の一覧と DB を突き合わせ、保存済みのメタデータを項目へ載せる。
    ///
    /// 初めて見る作品は行を作り、ファイルが消えた作品は行を消す。返すのは
    /// 絞り込みに使うタグ一覧。
    fn sync_metadata(
        repository: Option<&mut Repository>,
        items: &mut [GalleryItem],
        sources: &[loader::Source],
    ) -> Vec<String> {
        let Some(repo) = repository else { return Vec::new() };
        let now = repository::now();

        let known: HashMap<String, SketchMeta> = match repo.all() {
            Ok(all) => all.into_iter().map(|m| (m.id.clone(), m)).collect(),
            Err(e) => {
                log::error!("メタデータを読めませんでした: {e}");
                return Vec::new();
            }
        };

        for (item, source) in items.iter_mut().zip(sources) {
            let hash = repository::source_hash(&source.text);
            let status = match &item.status {
                SketchStatus::Error(e) => CompileStatus::Error(e.clone()),
                SketchStatus::Ready => CompileStatus::Ok,
            };

            let meta = match known.get(&item.id) {
                Some(saved) => {
                    // 保存済みの好みを画面へ戻す。
                    item.favorite = saved.favorite;
                    item.tags = saved.tags.clone();
                    item.collections = saved.collections.clone();
                    item.author = saved.author.clone();
                    item.link = saved.link.clone();
                    item.created_at = saved.created_at;
                    item.last_opened_at = saved.last_opened_at;

                    SketchMeta {
                        title: item.title.clone(),
                        compile_hash: hash,
                        compile_status: status,
                        updated_at: now,
                        ..saved.clone()
                    }
                }
                None => {
                    item.created_at = now;
                    SketchMeta {
                        compile_hash: hash,
                        compile_status: status,
                        ..SketchMeta::new(&item.id, &item.title, now)
                    }
                }
            };

            if let Err(e) = repo.upsert(&meta) {
                log::error!("{} のメタデータを保存できませんでした: {e}", item.id);
            }
        }

        // アプリの外で消された作品の行を落とす。
        let ids: Vec<String> = items.iter().map(|i| i.id.clone()).collect();
        match repo.retain(&ids) {
            Ok(n) if n > 0 => log::info!("ファイルの無いメタデータを {n} 件片付けました"),
            Ok(_) => {}
            Err(e) => log::error!("メタデータを整理できませんでした: {e}"),
        }

        repo.tags().unwrap_or_default()
    }

    /// DB へ 1 件書き戻す。画面側の状態が正。
    fn persist(&mut self, index: usize) {
        let Some(repo) = self.repository.as_mut() else { return };
        let Some(item) = self.gallery.items().get(index) else { return };
        let now = repository::now();

        let mut meta = match repo.get(&item.id) {
            Ok(Some(meta)) => meta,
            Ok(None) => SketchMeta::new(&item.id, &item.title, now),
            Err(e) => {
                log::error!("{} のメタデータを読めませんでした: {e}", item.id);
                return;
            }
        };

        meta.title = item.title.clone();
        meta.favorite = item.favorite;
        meta.tags = item.tags.clone();
        meta.author = item.author.clone();
        meta.link = item.link.clone();
        meta.last_opened_at = item.last_opened_at;
        meta.compile_status = match &item.status {
            SketchStatus::Error(e) => CompileStatus::Error(e.clone()),
            SketchStatus::Ready => CompileStatus::Ok,
        };
        meta.updated_at = now;
        if let Some(source) = self.sources.get(index) {
            meta.compile_hash = repository::source_hash(&source.text);
        }

        if let Err(e) = repo.upsert(&meta) {
            log::error!("{} のメタデータを保存できませんでした: {e}", item.id);
        }
        self.refresh_tags();
    }

    fn refresh_tags(&mut self) {
        if let Some(repo) = self.repository.as_ref() {
            self.tags = repo.tags().unwrap_or_default();
        }
    }

    // ---- 画面遷移 -------------------------------------------------------

    fn open_viewer(&mut self, index: usize) {
        self.gallery.select(index);
        self.viewer.switch_to(index);
        self.screen = Screen::Viewer;
        if let Some(ui) = self.ui.as_mut() {
            ui.note_activity();
        }

        // 「最近表示」で並べ替えられるように記録する (設計書 §20)。
        let now = repository::now();
        if let Some(item) = self.gallery.items_mut_at(index) {
            item.last_opened_at = Some(now);
        }
        if let Some(repo) = self.repository.as_mut()
            && let Some(item) = self.gallery.items().get(index)
            && let Err(e) = repo.touch_opened(&item.id, now)
        {
            log::error!("表示時刻を記録できませんでした: {e}");
        }
    }

    fn back_to_gallery(&mut self) {
        self.gallery.select(self.viewer.current_index());
        self.screen = Screen::Gallery;
        self.scroll_to_selected = true;
    }

    /// 編集画面を開く。
    fn open_editor(&mut self, index: usize) {
        let Some(source) = self.sources.get(index) else { return };
        let Some(item) = self.gallery.items().get(index) else { return };
        let tags = item.tags.iter().cloned().collect::<Vec<_>>().join(", ");
        self.editor = Some(Editor::edit(
            index,
            item.id.clone(),
            source.text.clone(),
            tags,
            item.author.clone(),
            item.link.clone(),
        ));
        self.return_screen = self.screen;
        self.screen = Screen::Editor;
    }

    /// 新規作成。名前だけ決めて、保存するまでファイルは作らない。
    fn new_sketch(&mut self) {
        let name =
            tsubu_core::library::unique_id(&self.paths.sketches(), editor::DEFAULT_NAME);
        self.editor = Some(Editor::new_sketch(name));
        self.return_screen = Screen::Gallery;
        self.screen = Screen::Editor;
    }

    fn close_editor(&mut self) {
        self.editor = None;
        // 作品が 1 本も無ければ Viewer へは戻れない。
        self.screen = match self.return_screen {
            Screen::Viewer if !self.viewer.is_empty() => Screen::Viewer,
            _ => Screen::Gallery,
        };
        self.scroll_to_selected = true;
    }

    /// 編集内容を保存し、コンパイルし直す (設計書 §15.1)。
    ///
    /// コンパイルに失敗してもファイルは書く。ユーザーの入力を失わせないため。
    /// 実行中のインスタンスは直前の正常なものを保ったままにする。
    fn save_editor(&mut self) {
        let Some(mut ed) = self.editor.take() else { return };
        let dir = self.paths.sketches();
        let name = ed.name.trim().to_string();

        if !tsubu_core::library::is_valid_id(&name) {
            ed.io_error = Some(format!("{}: {name}", self.locales.t("editor.invalid_name")));
            self.editor = Some(ed);
            return;
        }

        // 名前が変わっていたら、他の作品と衝突しないか見る。
        let previous_id = ed.index.and_then(|i| self.gallery.items().get(i)).map(|i| i.id.clone());
        let renaming = previous_id.as_deref() != Some(name.as_str());
        if renaming && tsubu_core::library::exists(&dir, &name) {
            ed.io_error = Some(format!("{}: {name}", self.locales.t("editor.name_taken")));
            self.editor = Some(ed);
            return;
        }

        if let Err(e) = tsubu_core::library::save(&dir, &name, &ed.source) {
            ed.io_error = Some(e.to_string());
            self.editor = Some(ed);
            return;
        }
        if renaming && let Some(old) = &previous_id {
            let _ = tsubu_core::library::delete(&dir, old);
            let _ = std::fs::remove_file(self.paths.thumbnail_for(old));
            self.textures.remove(old);
        }

        let source = loader::Source::from_id_and_text(&name, ed.source.clone());
        let compiled = source.instantiate();
        ed.set_check_result(
            ed.source.clone(),
            compiled.as_ref().err().cloned(),
            compiled.as_ref().ok().map(|c| c.dialect),
        );

        let index = match ed.index {
            Some(index) => {
                self.update_sketch(index, name.clone(), source, compiled);
                index
            }
            None => self.insert_sketch(name.clone(), source, compiled),
        };

        self.gallery.set_tags(index, ed.parsed_tags());
        self.gallery.set_credit(index, ed.author.trim(), ed.link.trim());
        ed.mark_saved(index);
        self.gallery.select(index);
        self.invalidate_thumbnail(index, &name);

        // 改名したら DB の行も付け替える。タグと作成日時は引き継ぐ。
        if renaming
            && let Some(old) = &previous_id
            && let Some(repo) = self.repository.as_mut()
            && let Err(e) =
                repo.rename(old, &name, &tsubu_core::library::title_from_id(&name), repository::now())
        {
            log::error!("{old} の改名を記録できませんでした: {e}");
        }
        self.persist(index);

        log::info!("{name} を保存しました");
        self.editor = Some(ed);
    }

    /// 既存の作品を差し替える。
    fn update_sketch(
        &mut self,
        index: usize,
        id: String,
        source: loader::Source,
        compiled: Result<tsubu_processing_lite::Compiled, tsubu_processing_lite::CompileError>,
    ) {
        let title = tsubu_core::library::title_from_id(&id);
        self.sources[index] = source;

        match compiled {
            Ok(compiled) => {
                let info = tsubu_processing_lite::SketchInfo {
                    id: id.clone(),
                    title: title.clone(),
                    thumbnail_frame: self.settings.capture_frame,
                };
                // 書き換えで Processing から p5.js へ変わることがある。
                self.gallery.set_dialect(index, Some(compiled.dialect.label().to_string()));
                self.viewer
                    .replace(index, tsubu_processing_lite::LoadedSketch::new(info, compiled.sketch));
                self.gallery.set_status(index, SketchStatus::Ready);
            }
            // 直前の正常なコードで動かし続ける (設計書 §15.1)。
            Err(e) => self.gallery.set_status(index, SketchStatus::Error(e.to_string())),
        }
        self.gallery.rename(index, id, title);
    }

    /// 新しい作品を並びの正しい位置へ足す。
    fn insert_sketch(
        &mut self,
        id: String,
        source: loader::Source,
        compiled: Result<tsubu_processing_lite::Compiled, tsubu_processing_lite::CompileError>,
    ) -> usize {
        // Gallery はファイル名順なので、同じ順序を保つ位置に差し込む。
        let index = self
            .gallery
            .items()
            .iter()
            .position(|item| item.id.as_str() > id.as_str())
            .unwrap_or(self.gallery.len());

        let title = tsubu_core::library::title_from_id(&id);
        let info = tsubu_processing_lite::SketchInfo {
            id: id.clone(),
            title: title.clone(),
            thumbnail_frame: self.settings.capture_frame,
        };

        // 「最近追加」で先頭に来るように、その場で作成日時を入れる。DB へ
        // 書くのは後だが、画面はいま並べ替える。同じ名前の行が残っていれば
        // その日時を引き継ぐ (消して作り直したとき)。
        let created_at = self
            .repository
            .as_ref()
            .and_then(|repo| repo.get(&id).ok().flatten())
            .map_or_else(repository::now, |meta| meta.created_at);
        let mut item = GalleryItem::new(&id, &title, created_at);
        let sketch: Box<dyn tsubu_processing_lite::Sketch> = match compiled {
            Ok(compiled) => {
                // 通ったものは必ずどれかの方言で読めている。
                item.dialect = Some(compiled.dialect.label().to_string());
                compiled.sketch
            }
            Err(e) => {
                item.status = SketchStatus::Error(e.to_string());
                Box::new(tsubu_processing_lite::BrokenSketch::new(e.to_string()))
            }
        };

        self.sources.insert(index, source);
        self.viewer.insert(index, tsubu_processing_lite::LoadedSketch::new(info, sketch));
        self.gallery.insert(index, item);
        index
    }

    /// 削除を実行する。確認済みの前提。
    fn delete_sketch(&mut self, index: usize) {
        let Some(item) = self.gallery.items().get(index) else { return };
        let id = item.id.clone();

        if let Err(e) = tsubu_core::library::delete(&self.paths.sketches(), &id) {
            log::error!("{id} を削除できませんでした: {e}");
            if let Some(ui) = self.ui.as_mut() {
                ui.toast(e.to_string());
            }
            return;
        }
        let _ = std::fs::remove_file(self.paths.thumbnail_for(&id));
        self.textures.remove(&id);
        if let Some(repo) = self.repository.as_mut()
            && let Err(e) = repo.delete(&id)
        {
            log::error!("{id} のメタデータを消せませんでした: {e}");
        }

        self.sources.remove(index);
        self.viewer.remove(index);
        self.gallery.remove(index);
        self.refresh_tags();
        log::info!("{id} を削除しました");

        if self.viewer.is_empty() {
            self.screen = Screen::Gallery;
        }
    }

    /// サムネイルを作り直させる。
    fn invalidate_thumbnail(&mut self, index: usize, id: &str) {
        let _ = std::fs::remove_file(self.paths.thumbnail_for(id));
        self.textures.remove(id);
        self.gallery.set_thumbnail_state(index, ThumbnailState::Missing);
    }

    /// キー操作で全画面を切り替え、設定にも残す。
    fn toggle_fullscreen_and_remember(&mut self) {
        self.toggle_fullscreen();
        if self.settings.fullscreen != self.fullscreen {
            self.settings.fullscreen = self.fullscreen;
            if let Some(repo) = self.repository.as_mut()
                && let Err(e) = repo.save_settings(&self.settings)
            {
                log::error!("設定を保存できませんでした: {e}");
            }
        }
    }

    fn toggle_fullscreen(&mut self) {
        let Some(window) = &self.window else { return };
        self.fullscreen = !self.fullscreen;
        window.set_fullscreen(self.fullscreen.then(|| Fullscreen::Borderless(None)));
    }

    fn cycle_language(&mut self) {
        let tags: Vec<String> =
            self.locales.available().iter().map(|t| t.tag().to_string()).collect();
        if tags.is_empty() {
            return;
        }
        let current = self.locales.active_tag().to_string();
        let next = tags.iter().position(|t| *t == current).map_or(0, |i| (i + 1) % tags.len());
        self.locales.set_preference(LanguagePreference::Explicit(tags[next].clone()));
        log::info!("UI 言語を {} に切り替えました", self.locales.active_tag());
    }

    /// 入力中のコードをコンパイルしてみて、エラーがあれば知らせる。
    ///
    /// 保存はしない。ファイルにも Viewer にも触れないので、打ち間違いのたびに
    /// 動いている作品が止まることはない。
    fn check_editor(&mut self) {
        let Some(editor) = self.editor.as_mut() else { return };
        editor.note_source_changes();

        let Some(source) = editor.source_to_check() else { return };
        let source = source.to_string();
        let compiled = tsubu_processing_lite::compile_sketch(&source, 0);
        let dialect = compiled.as_ref().ok().map(|c| c.dialect);
        editor.set_check_result(source, compiled.err(), dialect);
    }

    /// サムネイルを作り直す (設計書 §7.2)。
    fn request_thumbnail_refresh(&mut self, index: usize, frame: u64) {
        self.requested_thumbnail = Some(ThumbnailJob { index, frame, force: true });
    }

    // ---- 入力 -----------------------------------------------------------

    fn handle_key(&mut self, key: KeyCode, event_loop: &ActiveEventLoop) {
        match self.screen {
            Screen::Gallery => self.handle_gallery_key(key, event_loop),
            Screen::Viewer => self.handle_viewer_key(key),
            // 編集画面のキーは egui 側 (editor_ui) が拾う。
            Screen::Editor => {}
            Screen::Settings => {
                if key == KeyCode::Escape {
                    self.close_settings();
                }
            }
        }
    }

    fn handle_gallery_key(&mut self, key: KeyCode, event_loop: &ActiveEventLoop) {
        // 確認中は、答えるまで他の操作を受けない。
        if self.pending_delete.is_some() {
            match key {
                KeyCode::Enter | KeyCode::NumpadEnter => {
                    if let Some(index) = self.pending_delete.take() {
                        self.delete_sketch(index);
                    }
                }
                KeyCode::Escape => self.pending_delete = None,
                _ => {}
            }
            return;
        }

        let nav = match key {
            KeyCode::ArrowLeft => Some(Move::Left),
            KeyCode::ArrowRight => Some(Move::Right),
            KeyCode::ArrowUp => Some(Move::Up),
            KeyCode::ArrowDown => Some(Move::Down),
            KeyCode::Home => Some(Move::First),
            KeyCode::End => Some(Move::Last),
            _ => None,
        };
        if let Some(direction) = nav {
            self.gallery.move_selection(direction);
            self.scroll_to_selected = true;
            return;
        }

        let selected = self.gallery.selected_index();
        match key {
            KeyCode::Enter | KeyCode::NumpadEnter | KeyCode::Space => {
                if !self.viewer.is_empty() {
                    self.open_viewer(selected);
                }
            }
            KeyCode::KeyR => {
                self.viewer.random(&self.playlist());
                self.open_viewer(self.viewer.current_index());
            }
            KeyCode::KeyS => {
                self.gallery.toggle_favorite(selected);
                self.persist(selected);
            }
            KeyCode::KeyN => self.new_sketch(),
            KeyCode::KeyE => self.open_editor(selected),
            KeyCode::Delete | KeyCode::Backspace => {
                if !self.gallery.is_empty() {
                    self.pending_delete = Some(selected);
                }
            }
            KeyCode::KeyT => {
                if let Some(frame) = self.viewer.thumbnail_frame_at(selected) {
                    self.request_thumbnail_refresh(selected, frame);
                }
            }
            KeyCode::KeyL => self.cycle_language(),
            KeyCode::KeyF | KeyCode::F11 => self.toggle_fullscreen_and_remember(),
            KeyCode::Comma => self.open_settings(),
            KeyCode::KeyV => self.cycle_view_mode(),
            KeyCode::KeyP => self.toggle_slideshow(),
            KeyCode::KeyC => {
                if !self.gallery.is_empty() {
                    self.assigning = Some(selected);
                }
            }
            KeyCode::KeyO => self.open_selected_link(selected),
            KeyCode::Escape => {
                if self.assigning.take().is_some() {
                    // まず割り当て画面を閉じる。
                } else if self.fullscreen {
                    self.toggle_fullscreen();
                } else {
                    // Gallery がホーム画面なので、ここが終了点。
                    event_loop.exit();
                }
            }
            _ => {}
        }
    }

    fn handle_viewer_key(&mut self, key: KeyCode) {
        match key {
            KeyCode::ArrowRight | KeyCode::PageDown => {
                self.advance(1);
            }
            KeyCode::ArrowLeft | KeyCode::PageUp => {
                self.advance(-1);
            }
            KeyCode::Space => self.viewer.toggle_pause(),
            // ↑↓ にしているのは配列に依らないから。`[` `]` は JIS 配列だと
            // 物理位置がずれ、刻印と逆のキーを押すことになる。
            KeyCode::ArrowUp => self.change_speed(PlaybackSpeed::faster),
            KeyCode::ArrowDown => self.change_speed(PlaybackSpeed::slower),
            KeyCode::KeyR => {
                self.viewer.random(&self.playlist());
                self.gallery.select(self.viewer.current_index());
                self.restart_slideshow_timer();
            }
            KeyCode::KeyT => {
                // 手動更新は「いま見えている絵」をそのままサムネイルにする (§7.2)。
                let index = self.viewer.current_index();
                let frame = self.viewer.stats().frame_count;
                self.request_thumbnail_refresh(index, frame);
            }
            KeyCode::KeyI => self.show_info = !self.show_info,
            KeyCode::KeyO => self.open_selected_link(self.viewer.current_index()),
            KeyCode::KeyP => self.toggle_slideshow(),
            KeyCode::KeyE => self.open_editor(self.viewer.current_index()),
            KeyCode::KeyL => self.cycle_language(),
            KeyCode::Comma => self.open_settings(),
            KeyCode::KeyF | KeyCode::F11 => self.toggle_fullscreen_and_remember(),
            KeyCode::Escape => {
                if self.fullscreen {
                    self.toggle_fullscreen();
                } else {
                    self.back_to_gallery();
                }
            }
            _ => {}
        }
    }

    // ---- サムネイル ------------------------------------------------------

    /// 読み込み完了の取り込みと、次のサムネイル取得の着手。
    ///
    /// 1 フレームあたりの仕事量を絞ってあるので、作品数が増えても Gallery の
    /// 操作は止まらない (設計書 §22)。
    fn pump_thumbnails(&mut self) {
        while let Some(result) = self.loader.poll() {
            match result {
                Ok(decoded) => {
                    if let Some(index) = self.gallery.index_of(&decoded.id) {
                        self.upload_texture(
                            &decoded.id,
                            decoded.width,
                            decoded.height,
                            &decoded.rgba,
                        );
                        self.gallery.set_thumbnail_state(index, ThumbnailState::Ready);
                    }
                }
                Err((id, message)) => {
                    tsubu_core::logging::sketch_failed(&tsubu_core::logging::SketchRecord {
                        id: &id,
                        phase: "thumbnail",
                        dialect: None,
                        line: None,
                        column: None,
                        source: Some(&self.paths.thumbnail_for(&id)),
                        message: &message,
                    });
                    if let Some(index) = self.gallery.index_of(&id) {
                        self.gallery.set_thumbnail_state(index, ThumbnailState::Failed(message));
                    }
                }
            }
        }

        let mut gpu_budget = GPU_THUMBNAILS_PER_FRAME;
        let mut disk_budget = DISK_THUMBNAILS_PER_FRAME;

        loop {
            let job = match self.requested_thumbnail.take() {
                Some(job) => job,
                None => match self.gallery.next_missing_thumbnail() {
                    Some(index) => {
                        let frame = self.viewer.thumbnail_frame_at(index).unwrap_or(60);
                        ThumbnailJob { index, frame, force: false }
                    }
                    None => return,
                },
            };

            let Some(item) = self.gallery.items().get(job.index) else { return };
            let id = item.id.clone();
            let path = self.paths.thumbnail_for(&id);

            if !job.force && path.exists() {
                if disk_budget == 0 {
                    return;
                }
                disk_budget -= 1;
                self.gallery.set_thumbnail_state(job.index, ThumbnailState::Loading);
                self.loader.request(&id, &path);
                continue;
            }

            if gpu_budget == 0 {
                // 予算切れ。次のフレームで拾い直せるよう未取得のままにしておく。
                self.requested_thumbnail = Some(job);
                return;
            }
            gpu_budget -= 1;
            self.gallery.set_thumbnail_state(job.index, ThumbnailState::Loading);

            let result = self.generate_thumbnail(job.index, job.frame, &id, &path);
            let message = match result {
                Ok(()) => {
                    self.gallery.set_thumbnail_state(job.index, ThumbnailState::Ready);
                    self.locales.t("viewer.thumbnail_saved").to_string()
                }
                Err(e) => {
                    log::error!("サムネイル生成に失敗しました: {e}");
                    let message = format!("{}: {e}", self.locales.t("viewer.thumbnail_error"));
                    self.gallery.set_thumbnail_state(job.index, ThumbnailState::Failed(e));
                    message
                }
            };
            // 起動時の一括生成で通知が流れ続けないよう、手動更新のときだけ知らせる。
            if job.force && let Some(ui) = self.ui.as_mut() {
                ui.toast(message);
            }
        }
    }

    /// Viewer と同じ Renderer でオフスクリーン描画し、PNG 保存とテクスチャ登録まで行う。
    fn generate_thumbnail(
        &mut self,
        index: usize,
        frame: u64,
        id: &str,
        path: &std::path::Path,
    ) -> Result<(), String> {
        let Some(gfx) = self.gfx.as_mut() else {
            return Err("GPU がまだ初期化されていません".into());
        };
        let (width, height) =
            thumbnail::size_for_width(gfx.size(), self.settings.image_quality.width());

        let source = self.sources.get(index).ok_or_else(|| format!("作品 {index} がありません"))?;
        let mut sketch = source.instantiate().map_err(|e| e.to_string())?.sketch;

        // 表示中のインスタンスとは別に動かし、目標フレームまで進めてから撮る。
        // フレームをまたいで状態を持つ作品でも、実行結果と同じ絵になる。
        let g = &mut self.thumb_graphics;
        let capturer = &mut self.capturer;
        capturer.begin();

        g.reset_state();
        g.begin_frame(width as f32, height as f32);
        sketch.setup(g);
        capturer.draw(&gfx.device, &gfx.queue, &mut gfx.batch, g, width, height);

        for f in 1..=frame.max(1) {
            g.begin_frame(width as f32, height as f32);
            g.frame_count = f;
            g.time = f as f32 / 60.0;
            sketch.draw(g);
            // 1 枚ずつ GPU へ渡す。残像を使う作品は、目標フレームまで実際に
            // 積み上げないと本物と違う絵になる。
            capturer.draw(&gfx.device, &gfx.queue, &mut gfx.batch, g, width, height);
        }
        if let Some(error) = sketch.error() {
            return Err(error.to_string());
        }
        // 描き切れなかった絵は途中までしかない。撮っても本物と違う。
        if g.overflowed() {
            return Err(viewer::TOO_MUCH_GEOMETRY.to_string());
        }

        let image =
            capturer.read(&gfx.device, &gfx.queue, width, height).map_err(|e| e.to_string())?;

        thumbnail::save_png(&image, path)?;
        self.upload_texture(id, image.width, image.height, &image.rgba);
        log::info!("サムネイルを保存しました: {}", path.display());
        Ok(())
    }

    fn upload_texture(&mut self, id: &str, width: u32, height: u32, rgba: &[u8]) {
        let Some(ui) = self.ui.as_ref() else { return };
        let image =
            egui::ColorImage::from_rgba_unmultiplied([width as usize, height as usize], rgba);
        let handle =
            ui.ctx().load_texture(format!("tsubu.thumb.{id}"), image, egui::TextureOptions::LINEAR);
        self.textures.insert(id.to_string(), handle);
    }

    // ---- 描画 -----------------------------------------------------------

    fn redraw(&mut self) {
        // 1 フレームの仕事にかかる時間。これをフレームの間隔で割ると、
        // このアプリが CPU をどれだけ使い続けているかになる。
        let frame_started = std::time::Instant::now();
        self.pump_thumbnails();
        self.pump_screensaver();
        self.pump_slideshow();
        if self.screen == Screen::Editor {
            self.check_editor();
        }

        let Some(window) = self.window.clone() else { return };
        let background = self.background();
        let (Some(gfx), Some(ui)) = (self.gfx.as_mut(), self.ui.as_mut()) else { return };
        let (width, height) = gfx.size();

        // Gallery ではスケッチを回さない。地の色だけ塗って egui に任せる。
        // Viewer ではキャンバスが画面を覆うので、地の色は見えない。
        let clear = match self.screen {
            Screen::Gallery | Screen::Editor | Screen::Settings => background,
            Screen::Viewer => Color::BLACK,
        };

        // 画面の空きを待つ時間は仕事ではない。ここを数えると負荷が
        // いつも 100% に見えてしまう。
        let waiting = std::time::Instant::now();
        let frame = match gfx.surface.get_current_texture() {
            CurrentSurfaceTexture::Success(frame) | CurrentSurfaceTexture::Suboptimal(frame) => {
                frame
            }
            // 次のフレームで作り直せば回復する。
            CurrentSurfaceTexture::Outdated | CurrentSurfaceTexture::Lost => {
                gfx.resize(width, height);
                return;
            }
            // 最小化中やタイムアウトはこのフレームを捨てるだけでよい。
            CurrentSurfaceTexture::Timeout | CurrentSurfaceTexture::Occluded => return,
            CurrentSurfaceTexture::Validation => {
                log::error!("サーフェスの取得が検証エラーになりました");
                return;
            }
        };
        let vsync_wait = waiting.elapsed();
        let view = frame.texture.create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = gfx
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("tsubu.frame") });

        // スケッチは画面ではなくキャンバスへ描く。`background()` を呼ばない
        // フレームは前の絵の上に重なり、残像がそのまま残る。
        let mut draw_sketch = false;
        if self.screen == Screen::Viewer {
            if self.viewer.epoch() != self.canvas_epoch {
                gfx.canvas.reset();
                self.canvas_epoch = self.viewer.epoch();
            }
            self.viewer.set_mouse(self.mouse.0, self.mouse.1, self.mouse_pressed);
            let paused = self.viewer.is_paused();
            if let Some(graphics) = self.viewer.render_frame(width as f32, height as f32) {
                // 一時停止中は積み増さない。同じ図形を毎フレーム重ねてしまい、
                // 止めたはずの絵が濃くなっていく。ただしキャンバスが空 (作品を
                // 変えた直後やサイズ変更の直後) なら 1 枚は描く。
                if !paused || !gfx.canvas.has_content() {
                    gfx.canvas.render(
                        &gfx.device,
                        &gfx.queue,
                        &mut gfx.batch,
                        &mut encoder,
                        graphics,
                        width,
                        height,
                    );
                }
                draw_sketch = gfx.canvas.has_content();
            }
        }

        let mut gallery_output = GalleryOutput::default();
        let mut editor_actions: Vec<EditorAction> = Vec::new();
        let mut settings_actions: Vec<SettingsAction> = Vec::new();
        {
            let screen = self.screen;
            // 可変借用の前に、必要な文字列は取り出しておく。
            let pending_delete = self
                .pending_delete
                .and_then(|i| self.gallery.items().get(i))
                .map(|i| i.title.clone());
            let collections = &self.collections;
            let assigning = self.assigning;
            let view_mode = self.settings.view_mode;
            // 作者とリンクは Gallery 側の項目に載っている。可変借用の前に写す。
            let credit = self
                .gallery
                .items()
                .get(self.viewer.current_index())
                .map_or((String::new(), String::new()), |i| (i.author.clone(), i.link.clone()));
            let card_size = self.settings.card_size;
            let show_titles = self.settings.show_titles;
            let settings = &mut self.settings;
            let gallery = &mut self.gallery;
            let textures = &self.textures;
            let locales = &self.locales;
            let tags = &self.tags;
            let scroll_to_selected = self.scroll_to_selected;
            let mut editor = self.editor.as_mut();
            let hide_cursor =
                should_hide_cursor(self.fullscreen, self.cursor_inside, ui.cursor_idle());
            ui.set_cursor_hidden(hide_cursor);
            let overlay = ViewerOverlay {
                title: self.viewer.current_title().unwrap_or(""),
                index: self.viewer.current_index(),
                total: self.viewer.len(),
                paused: self.viewer.is_paused(),
                speed: settings.playback_speed,
                stats: self.viewer.stats(),
                alpha: ui.overlay_alpha(),
                show_info: self.show_info,
                error: self.viewer.current_error(),
                dialect: self.viewer.current_dialect().map(|d| d.label()),
                author: &credit.0,
                link: &credit.1,
                slideshow: self.slideshow.is_some(),
                screensaver: self.screensaver.is_some(),
                gpu: &gfx.gpu,
                backend: &gfx.backend,
            };

            ui.prepare(
                UiFrame {
                    window: &window,
                    device: &gfx.device,
                    queue: &gfx.queue,
                    encoder: &mut encoder,
                    size_in_pixels: [width, height],
                },
                |ui| match screen {
                    Screen::Gallery => {
                        gallery_output = gallery_ui::build(
                            ui,
                            &mut GalleryUi {
                                view: gallery,
                                textures,
                                locales,
                                tags,
                                scroll_to_selected,
                                pending_delete: pending_delete.clone(),
                                collections,
                                assigning,
                                view_mode,
                                card_size,
                                show_titles,
                            },
                        );
                    }
                    Screen::Viewer => viewer_ui::build(ui, &overlay, locales),
                    Screen::Settings => {
                        settings_actions =
                            settings_ui::build(ui, &mut SettingsUi { settings, locales });
                    }
                    Screen::Editor => {
                        if let Some(editor) = editor.as_deref_mut() {
                            editor_actions = editor_ui::build(ui, editor, locales);
                        }
                    }
                },
            );
        }
        self.scroll_to_selected = false;
        // 上下移動はここで得た列数を使う。返し忘れると 1 列扱いになり、
        // 上下が前後移動と同じ動きになってしまう。
        if gallery_output.columns > 0 && gallery_output.columns != self.gallery.columns() {
            log::debug!("Gallery のグリッドが {} 列になりました", gallery_output.columns);
            self.gallery.set_columns(gallery_output.columns);
        }

        {
            let msaa_view = gfx.msaa.view(&gfx.device, width, height, gfx.format());
            let mut pass = encoder
                .begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("tsubu.frame.pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: msaa_view,
                        depth_slice: None,
                        resolve_target: Some(&view),
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color {
                                r: clear.r as f64,
                                g: clear.g as f64,
                                b: clear.b as f64,
                                a: clear.a as f64,
                            }),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                })
                // egui は 'static なパスを要求する。
                .forget_lifetime();

            if draw_sketch {
                gfx.canvas.present(&gfx.device, &mut pass, gfx.format(), SAMPLE_COUNT);
            }
            ui.render(&mut pass);
        }

        gfx.queue.submit(Some(encoder.finish()));
        window.pre_present_notify();
        gfx.queue.present(frame);
        ui.after_submit();

        // 並び替えは絞り込みバーからも変えられる。設定画面で変えたときと
        // 同じように残さないと、次の起動で戻ってしまう。
        if self.gallery.sort() != self.settings.sort_order {
            self.settings.sort_order = self.gallery.sort();
            if let Some(repo) = self.repository.as_mut()
                && let Err(e) = repo.save_settings(&self.settings)
            {
                log::error!("設定を保存できませんでした: {e}");
            }
        }

        self.apply_gallery_actions(&gallery_output.actions);
        self.apply_editor_actions(&editor_actions);
        self.apply_settings_actions(&settings_actions);
        self.sync_runtime_errors();
        self.viewer.note_frame_work(frame_started.elapsed().saturating_sub(vsync_wait));
    }

    fn apply_settings_actions(&mut self, actions: &[SettingsAction]) {
        for action in actions {
            match action {
                SettingsAction::Changed => self.apply_settings(),
                SettingsAction::Close => self.close_settings(),
            }
        }
    }

    /// 画面の地の色。配色設定に合わせる。
    fn background(&self) -> Color {
        match self.settings.theme {
            tsubu_core::settings::Theme::Dark => GALLERY_BACKGROUND,
            tsubu_core::settings::Theme::Light => Color::rgba(0.96, 0.96, 0.97, 1.0),
        }
    }

    /// 選んだ作品のリンクを開く。無い作品では何もしない。
    fn open_selected_link(&mut self, index: usize) {
        let link = self.gallery.items().get(index).map(|i| i.link.clone()).unwrap_or_default();
        if link.trim().is_empty() {
            return;
        }
        self.open_link(&link);
    }

    /// リンクをブラウザで開く。開けないものは開かない。
    fn open_link(&mut self, url: &str) {
        match tsubu_core::open::open(url) {
            Ok(()) => log::info!("リンクを開きました: {url}"),
            Err(e) => {
                log::error!("リンクを開けませんでした: {e} ({url})");
                if let Some(ui) = self.ui.as_mut() {
                    ui.toast(format!("{}: {e}", self.locales.t("app.link_failed")));
                }
            }
        }
    }

    fn apply_editor_actions(&mut self, actions: &[EditorAction]) {
        for action in actions {
            match action {
                EditorAction::OpenLink => {
                    if let Some(link) = self.editor.as_ref().map(|e| e.link.trim().to_string()) {
                        self.open_link(&link);
                    }
                }
                EditorAction::Save => self.save_editor(),
                EditorAction::Run => {
                    self.save_editor();
                    // コンパイルが通ったときだけ実行へ移る。
                    let ok = self.editor.as_ref().is_some_and(|e| e.error.is_none() && e.io_error.is_none());
                    if let Some(index) = self.editor.as_ref().and_then(|e| e.index)
                        && ok
                    {
                        self.editor = None;
                        // 先に移ってから動かし直す。逆にすると、直前まで
                        // 見ていた別の作品を動かし直すだけになる。
                        self.open_viewer(index);
                        self.viewer.restart_current();
                    }
                }
                EditorAction::Expand => {
                    if let Some(ed) = self.editor.as_mut() {
                        ed.source = tsubu_processing_lite::format::expand(&ed.source);
                    }
                }
                EditorAction::Compress => {
                    if let Some(ed) = self.editor.as_mut() {
                        ed.source = tsubu_processing_lite::format::compress(&ed.source);
                    }
                }
                EditorAction::Close => {
                    // 未保存の作業を黙って捨てない。空なら失うものが無いので聞かない。
                    match self.editor.as_mut() {
                        Some(ed) if ed.is_dirty() && !ed.is_blank() => ed.confirming_close = true,
                        _ => self.close_editor(),
                    }
                }
                EditorAction::DiscardAndClose => self.close_editor(),
                EditorAction::CancelClose => {
                    if let Some(ed) = self.editor.as_mut() {
                        ed.confirming_close = false;
                    }
                }
            }
        }
    }

    /// 実行中に止まった作品を Gallery のエラー表示へ反映する (設計書 §6.1)。
    fn sync_runtime_errors(&mut self) {
        let index = self.viewer.current_index();
        let Some(error) = self.viewer.current_error() else { return };
        let error = error.to_string();

        let already_marked = self
            .gallery
            .items()
            .get(index)
            .is_some_and(|item| item.status == SketchStatus::Error(error.clone()));
        if !already_marked {
            let id = self
                .gallery
                .items()
                .get(index)
                .map(|item| item.id.clone())
                .unwrap_or_default();
            tsubu_core::logging::sketch_failed(&tsubu_core::logging::SketchRecord {
                id: &id,
                phase: "run",
                dialect: self.viewer.current_dialect().map(|d| d.label()),
                line: None,
                column: None,
                source: Some(&tsubu_core::library::path_for(&self.paths.sketches(), &id)),
                message: &error,
            });
            self.gallery.set_status(index, SketchStatus::Error(error));
        }
    }

    fn apply_gallery_actions(&mut self, actions: &[GalleryAction]) {
        for action in actions {
            // 確認中・割り当て中はカードへの操作を通さない。
            if self.pending_delete.is_some()
                && !matches!(action, GalleryAction::ConfirmDelete | GalleryAction::CancelDelete)
            {
                continue;
            }
            if self.assigning.is_some()
                && !matches!(
                    action,
                    GalleryAction::SetCollection(..)
                        | GalleryAction::DeleteCollection(_)
                        | GalleryAction::CloseCollections
                )
            {
                continue;
            }
            match action.clone() {
                GalleryAction::Open(index) => self.open_viewer(index),
                GalleryAction::Select(index) => self.gallery.select(index),
                GalleryAction::ToggleFavorite(index) => {
                    self.gallery.toggle_favorite(index);
                    self.persist(index);
                }
                GalleryAction::ConfirmDelete => {
                    if let Some(index) = self.pending_delete.take() {
                        self.delete_sketch(index);
                    }
                }
                GalleryAction::CancelDelete => self.pending_delete = None,
                GalleryAction::OpenSettings => self.open_settings(),
                GalleryAction::OpenLink(index) => {
                    if let Some(link) = self.gallery.items().get(index).map(|i| i.link.clone()) {
                        self.open_link(&link);
                    }
                }
                GalleryAction::SetCollection(index, name, member) => {
                    self.set_collection(index, &name, member);
                }
                GalleryAction::DeleteCollection(name) => self.delete_collection(&name),
                GalleryAction::CloseCollections => self.assigning = None,
            }
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }

        let attributes = Window::default_attributes()
            // アプリ名はローカライズしない (設計書 §2)。
            .with_title("TsubuGallery")
            .with_window_icon(window_icon())
            .with_inner_size(winit::dpi::LogicalSize::new(1100.0, 720.0));

        let window = match event_loop.create_window(attributes) {
            Ok(w) => Arc::new(w),
            Err(e) => {
                log::error!("ウィンドウを作成できませんでした: {e}");
                self.report_fatal("app.window_failed", &e.to_string());
                event_loop.exit();
                return;
            }
        };

        let gfx = match Gfx::new(window.clone()) {
            Ok(g) => g,
            Err(e) => {
                log::error!("{e}");
                self.report_fatal("app.gpu_failed", &e);
                event_loop.exit();
                return;
            }
        };

        let mut ui = UiLayer::new(&window, &gfx.device, gfx.format(), SAMPLE_COUNT);
        if !ui.has_cjk_font {
            // 豆腐を出すより英語のほうが読める。
            self.locales.set_preference(LanguagePreference::Explicit("en-US".into()));
        }

        // Viewer を一度も描かないうちに Gallery から作品を開くと、そこで隣の
        // 作品の `setup()` が走る。窓の大きさを先に教えておかないと、
        // `createCanvas(innerWidth, innerHeight)` が 1×1 を読んでしまう。
        let size = window.inner_size();
        self.viewer.set_display_size(size.width as f32, size.height as f32);

        self.window = Some(window);
        self.gfx = Some(gfx);
        ui.set_theme(self.settings.theme);
        self.ui = Some(ui);

        // 起動画面が Editor 指定なら、先頭の作品を開いておく。
        if self.screen == Screen::Editor && self.editor.is_none() {
            if self.gallery.is_empty() {
                self.new_sketch();
            } else {
                self.open_editor(0);
            }
            self.screen = Screen::Editor;
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        // 操作の記録は egui へ渡す前に行う。文字入力は egui が食べてしまうので、
        // あとで見てもスクリーンセーバーには気付けない。
        let touched = match &event {
            WindowEvent::CursorMoved { position, .. } => {
                // 全画面への切り替えでウィンドウの大きさが変わると、指を触れて
                // いなくても座標が届く。わずかな動きでは解除しない。
                let dx = position.x as f32 - self.mouse.0;
                let dy = position.y as f32 - self.mouse.1;
                dx * dx + dy * dy > CURSOR_NOISE * CURSOR_NOISE
            }
            other => is_user_input(other),
        };
        if touched && self.note_input() {
            // セーバーを解除した一打は、ここで止めて画面へ渡さない。
            return;
        }

        if let (Some(window), Some(ui)) = (self.window.as_ref(), self.ui.as_mut())
            && ui.on_window_event(window, &event)
        {
            return;
        }

        match event {
            WindowEvent::CloseRequested => event_loop.exit(),

            WindowEvent::Resized(size) => {
                if let Some(gfx) = self.gfx.as_mut() {
                    gfx.resize(size.width, size.height);
                }
                self.viewer.set_display_size(size.width as f32, size.height as f32);
            }

            WindowEvent::CursorMoved { position, .. } => {
                self.mouse = (position.x as f32, position.y as f32);
                // 座標が届いた時点で窓の上にいる。全画面へ入った直後など、
                // CursorEntered が来ないまま動き始めることがある。
                self.cursor_inside = true;
                if let Some(ui) = self.ui.as_mut() {
                    ui.note_activity();
                }
            }

            WindowEvent::CursorEntered { .. } => self.cursor_inside = true,
            WindowEvent::CursorLeft { .. } => self.cursor_inside = false,

            WindowEvent::MouseInput { state, button: MouseButton::Left, .. } => {
                self.mouse_pressed = state == ElementState::Pressed;
                if let Some(ui) = self.ui.as_mut() {
                    ui.note_activity();
                }
            }

            WindowEvent::KeyboardInput { event, .. } => {
                if event.state != ElementState::Pressed || event.repeat {
                    return;
                }
                if let Some(ui) = self.ui.as_mut() {
                    ui.note_activity();
                }
                if let PhysicalKey::Code(code) = event.physical_key {
                    self.handle_key(code, event_loop);
                }
            }

            WindowEvent::RedrawRequested => self.redraw(),

            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }
}

/// これ以下のカーソル移動は「触っていない」とみなす (論理ピクセル)。
const CURSOR_NOISE: f32 = 4.0;

/// マウスカーソルを消すか。
///
/// 全画面で見ている最中に矢印が居座ると、作品の一部のように見えてしまう。
/// オーバーレイが引いたあとに続けて消す。
///
/// 窓の外にいるときは消さない。全画面でも、モニタが 2 枚あればカーソルは別の
/// 画面へ出ていける。そこで消すと、こちらを見ていない — 他のアプリを触って
/// いる — 人のカーソルを奪うことになる。
fn should_hide_cursor(fullscreen: bool, inside: bool, idle: bool) -> bool {
    fullscreen && inside && idle
}

/// ユーザーが何かした、と数えてよいイベントか。
///
/// ウィンドウの移動やフォーカスの変化は数えない。他のアプリを使っている間も
/// スクリーンセーバーが始まらなくなってしまう。
fn is_user_input(event: &WindowEvent) -> bool {
    matches!(
        event,
        WindowEvent::KeyboardInput { .. }
            | WindowEvent::CursorMoved { .. }
            | WindowEvent::MouseInput { .. }
            | WindowEvent::MouseWheel { .. }
            | WindowEvent::TouchpadPressure { .. }
            | WindowEvent::PinchGesture { .. }
            | WindowEvent::Touch(_)
    )
}

#[cfg(test)]
mod input_tests {
    use super::*;

    /// 操作でないものを操作と数えないこと。
    ///
    /// ここを間違えると、席を立っていてもスクリーンセーバーが始まらなくなる。
    #[test]
    fn window_housekeeping_is_not_user_input() {
        assert!(!is_user_input(&WindowEvent::Focused(true)), "フォーカスは操作ではない");
        assert!(!is_user_input(&WindowEvent::RedrawRequested));
        assert!(!is_user_input(&WindowEvent::CloseRequested));
        assert!(!is_user_input(&WindowEvent::Moved(winit::dpi::PhysicalPosition::new(1, 2))));
        assert!(!is_user_input(&WindowEvent::Occluded(true)));
    }

    /// ごく小さなカーソル移動では解除しない。
    ///
    /// 窓の上にいないカーソルは消さない。
    ///
    /// ここを落とすと、モニタ 2 枚で全画面にしている人が、別の画面で作業して
    /// いる最中にカーソルを見失う。
    #[test]
    fn the_cursor_only_disappears_over_our_own_window() {
        assert!(should_hide_cursor(true, true, true), "全画面・窓の上・無操作なら消す");
        assert!(!should_hide_cursor(true, false, true), "別のモニタへ出ていれば消さない");
        assert!(!should_hide_cursor(false, true, true), "窓表示のときは消さない");
        assert!(!should_hide_cursor(true, true, false), "触っている間は消さない");
    }

    /// カーソルはオーバーレイより遅れて消える。
    ///
    /// 同時だと画面から 2 つのものが一度に消えて、何が起きたのか読み取れない。
    #[test]
    fn the_cursor_goes_after_the_overlay() {
        assert!(ui::CURSOR_HIDE > ui::AUTO_HIDE);
    }

    /// 全画面への切り替えでウィンドウの大きさが変わると、指を触れていなくても
    /// 座標が届くことがある。
    #[test]
    fn tiny_cursor_moves_are_ignored() {
        let moved = |dx: f32, dy: f32| dx * dx + dy * dy > CURSOR_NOISE * CURSOR_NOISE;
        assert!(!moved(0.0, 0.0));
        assert!(!moved(2.0, 2.0));
        assert!(moved(0.0, 10.0));
        assert!(moved(-30.0, 0.0));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(id: &str) -> GalleryItem {
        GalleryItem::new(id, tsubu_core::library::title_from_id(id), 0)
    }

    fn source(text: &str) -> loader::Source {
        loader::Source::from_id_and_text("x", text.to_string())
    }

    #[test]
    fn first_sync_creates_a_row_for_every_sketch() {
        let mut repo = Repository::in_memory().expect("メモリ DB");
        let mut items = vec![item("a"), item("b")];
        let sources = vec![source("void draw() {}"), source("void draw() {}")];

        App::sync_metadata(Some(&mut repo), &mut items, &sources);

        assert_eq!(repo.count().expect("数えられる"), 2);
        assert!(items[0].created_at > 0, "追加時刻が入る");
    }

    #[test]
    fn saved_favorites_and_tags_come_back_on_the_next_sync() {
        let mut repo = Repository::in_memory().expect("メモリ DB");
        let mut items = vec![item("a")];
        let sources = vec![source("void draw() {}")];

        App::sync_metadata(Some(&mut repo), &mut items, &sources);
        assert!(!items[0].favorite);

        // ユーザーが★を付けてタグを足した、という状態を作る。
        let mut meta = repo.get("a").expect("引ける").expect("ある");
        meta.favorite = true;
        meta.tags.insert("circles".into());
        repo.upsert(&meta).expect("保存できる");

        // 起動し直したつもりで、まっさらな項目に対して同期する。
        let mut reloaded = vec![item("a")];
        let tags = App::sync_metadata(Some(&mut repo), &mut reloaded, &sources);

        assert!(reloaded[0].favorite, "お気に入りが戻る");
        assert!(reloaded[0].tags.contains("circles"), "タグが戻る");
        assert_eq!(tags, vec!["circles"], "絞り込み用のタグ一覧も返る");
    }

    #[test]
    fn the_created_time_is_kept_across_syncs() {
        let mut repo = Repository::in_memory().expect("メモリ DB");
        let sources = vec![source("void draw() {}")];

        let mut items = vec![item("a")];
        App::sync_metadata(Some(&mut repo), &mut items, &sources);
        let created = items[0].created_at;

        let mut again = vec![item("a")];
        App::sync_metadata(Some(&mut repo), &mut again, &sources);
        assert_eq!(again[0].created_at, created, "追加時刻は上書きしない");
    }

    #[test]
    fn metadata_for_a_deleted_file_is_dropped() {
        let mut repo = Repository::in_memory().expect("メモリ DB");
        let sources = vec![source("void draw() {}"), source("void draw() {}")];

        let mut items = vec![item("a"), item("b")];
        App::sync_metadata(Some(&mut repo), &mut items, &sources);
        assert_eq!(repo.count().expect("数えられる"), 2);

        // b.pde をアプリの外で消した、という状態。
        let mut remaining = vec![item("a")];
        App::sync_metadata(Some(&mut repo), &mut remaining, &sources[..1]);

        assert_eq!(repo.count().expect("数えられる"), 1);
        assert!(repo.get("b").expect("引ける").is_none());
    }

    #[test]
    fn a_compile_error_is_recorded() {
        let mut repo = Repository::in_memory().expect("メモリ DB");
        let mut items = vec![item("a")];
        items[0].status = SketchStatus::Error("3行3列: `;` がありません".into());
        let sources = vec![source("void draw() { background(0) }")];

        App::sync_metadata(Some(&mut repo), &mut items, &sources);

        let meta = repo.get("a").expect("引ける").expect("ある");
        assert!(matches!(meta.compile_status, CompileStatus::Error(_)));
    }

    #[test]
    fn syncing_without_a_database_is_harmless() {
        let mut items = vec![item("a")];
        let sources = vec![source("void draw() {}")];
        // DB を開けなかった環境でも、作品は普通に並ぶ。
        let tags = App::sync_metadata(None, &mut items, &sources);
        assert!(tags.is_empty());
        assert!(!items[0].favorite);
    }
}
