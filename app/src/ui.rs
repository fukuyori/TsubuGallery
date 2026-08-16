//! egui のフレーム処理をまとめた層。
//!
//! Gallery と Viewer で描く内容は違うが、入力の受け渡し・テッセレーション・
//! GPU への転送は同じなので、ここに一本化して画面ごとの UI はクロージャで渡す。
//! 画面共通のクロム (トースト、操作からの経過時間) もここが持つ。

use tsubu_core::settings::Theme;
use std::time::{Duration, Instant};

use egui_wgpu::ScreenDescriptor;
use winit::window::Window;

/// 操作がないときに Viewer のオーバーレイを消すまでの時間 (設計書 §8.2)。
pub const AUTO_HIDE: Duration = Duration::from_millis(2600);
/// 操作がないときにマウスカーソルを消すまでの時間。
///
/// オーバーレイ ([`AUTO_HIDE`]) より少しあとにする。同時に消すと画面から 2 つの
/// ものが一度に消えて、何が起きたのか読み取りにくい。
pub const CURSOR_HIDE: Duration = Duration::from_millis(3000);
/// トーストの表示時間。
const TOAST_DURATION: Duration = Duration::from_millis(2200);

/// 1 フレーム分の GPU ハンドル。引数を束ねるためだけの入れ物。
pub struct UiFrame<'a> {
    pub window: &'a Window,
    pub device: &'a wgpu::Device,
    pub queue: &'a wgpu::Queue,
    pub encoder: &'a mut wgpu::CommandEncoder,
    pub size_in_pixels: [u32; 2],
}

pub struct UiLayer {
    ctx: egui::Context,
    state: egui_winit::State,
    renderer: egui_wgpu::Renderer,
    jobs: Vec<egui::ClippedPrimitive>,
    to_free: Vec<egui::TextureId>,
    screen: ScreenDescriptor,

    last_activity: Instant,
    /// このフレームでカーソルを消すか。呼び出し側が毎フレーム決める。
    cursor_hidden: bool,
    toast: Option<(String, Instant)>,
    /// CJK フォントを用意できたか。できていなければ日本語 UI は使わない。
    pub has_cjk_font: bool,
}

impl UiLayer {
    pub fn new(
        window: &Window,
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        msaa_samples: u32,
    ) -> Self {
        let ctx = egui::Context::default();
        let has_cjk_font = crate::fonts::install_cjk_font(&ctx);
        ctx.set_visuals(egui::Visuals::dark());

        let state = egui_winit::State::new(
            ctx.clone(),
            egui::ViewportId::ROOT,
            window,
            Some(window.scale_factor() as f32),
            None,
            None,
        );

        let renderer = egui_wgpu::Renderer::new(
            device,
            format,
            egui_wgpu::RendererOptions { msaa_samples, ..Default::default() },
        );

        Self {
            ctx,
            state,
            renderer,
            jobs: Vec::new(),
            to_free: Vec::new(),
            screen: ScreenDescriptor { size_in_pixels: [1, 1], pixels_per_point: 1.0 },
            last_activity: Instant::now(),
            cursor_hidden: false,
            toast: None,
            has_cjk_font,
        }
    }

    /// テクスチャ登録などで UI 層の [`egui::Context`] が要るとき用。
    /// 配色を切り替える (設計書 §24 の Theme)。
    pub fn set_theme(&mut self, theme: Theme) {
        self.ctx.set_visuals(match theme {
            Theme::Dark => egui::Visuals::dark(),
            Theme::Light => egui::Visuals::light(),
        });
    }

    pub fn ctx(&self) -> &egui::Context {
        &self.ctx
    }

    /// egui にイベントを渡す。`true` なら UI が消費したのでスケッチへは流さない。
    pub fn on_window_event(&mut self, window: &Window, event: &winit::event::WindowEvent) -> bool {
        self.state.on_window_event(window, event).consumed
    }

    /// 何か操作があったことを記録する。
    pub fn note_activity(&mut self) {
        self.last_activity = Instant::now();
    }

    /// カーソルを消してよいだけの時間、操作が途切れているか。
    pub fn cursor_idle(&self) -> bool {
        self.last_activity.elapsed() >= CURSOR_HIDE
    }

    /// このフレームでカーソルを消すか。[`Self::prepare`] の前に決める。
    pub fn set_cursor_hidden(&mut self, hidden: bool) {
        self.cursor_hidden = hidden;
    }

    pub fn toast(&mut self, message: impl Into<String>) {
        self.toast = Some((message.into(), Instant::now()));
        self.note_activity();
    }

    /// Viewer オーバーレイの不透明度。0 なら描かない (設計書 §8.2)。
    pub fn overlay_alpha(&self) -> f32 {
        let elapsed = self.last_activity.elapsed();
        if elapsed >= AUTO_HIDE {
            return 0.0;
        }
        // 最後の 600ms でフェードアウトさせる。
        let remaining = (AUTO_HIDE - elapsed).as_secs_f32();
        (remaining / 0.6).min(1.0)
    }

    fn active_toast(&self) -> Option<&str> {
        self.toast
            .as_ref()
            .filter(|(_, at)| at.elapsed() < TOAST_DURATION)
            .map(|(m, _)| m.as_str())
    }

    /// UI を組み立て、GPU バッファへ転送する。レンダーパス開始前に呼ぶ。
    pub fn prepare(&mut self, frame: UiFrame<'_>, mut build: impl FnMut(&mut egui::Ui)) {
        let UiFrame { window, device, queue, encoder, size_in_pixels } = frame;

        self.screen = ScreenDescriptor {
            size_in_pixels,
            pixels_per_point: window.scale_factor() as f32,
        };

        let raw_input = self.state.take_egui_input(window);
        let toast = self.active_toast().map(str::to_owned);
        let hide_cursor = self.cursor_hidden;

        let mut output = self.ctx.run_ui(raw_input, |ui| {
            build(ui);
            if let Some(message) = &toast {
                toast_area(ui.ctx(), message);
            }
            // カーソルを消すのは egui へ頼む。ここで直接
            // `window.set_cursor_visible(false)` を呼ぶと、egui-winit が毎フレーム
            // `set_cursor_visible(true)` を呼び返すので取り合いになる。
            // `CursorIcon::None` なら egui-winit 側が消してくれる。
            //
            // 組み立てのあとで指定する。カーソルがウィジェットの上に載っていると
            // そのウィジェットが自分の形を書き込むので、先に指定すると消される。
            if hide_cursor {
                ui.ctx().set_cursor_icon(egui::CursorIcon::None);
            }
        });

        self.state.handle_platform_output(window, output.platform_output);
        self.jobs = self.ctx.tessellate(output.shapes, output.pixels_per_point);

        // 取り出して空にする。egui 0.36 の TexturesDelta は、中身が残ったまま
        // 落とされると debug_assert! で落ちる。借りて回すだけでは空にならない。
        let mut delta = std::mem::take(&mut output.textures_delta);
        for (id, deltas) in delta.set.drain() {
            for image in deltas {
                self.renderer.update_texture(device, queue, id, &image);
            }
        }
        self.renderer.update_buffers(device, queue, encoder, &self.jobs, &self.screen);
        // 解放は submit のあと。まだ描画に使われているかもしれない。
        self.to_free = std::mem::take(&mut delta.free).into_iter().collect();
    }

    pub fn render(&self, pass: &mut wgpu::RenderPass<'static>) {
        self.renderer.render(pass, &self.jobs, &self.screen);
    }

    /// submit 後に呼ぶ。使い終わったテクスチャを解放する。
    pub fn after_submit(&mut self) {
        for id in self.to_free.drain(..) {
            self.renderer.free_texture(&id);
        }
    }
}

/// 半透明の黒地に載せる小さなパネル。画面をまたいで見た目を揃える。
pub fn panel(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::new()
        .fill(egui::Color32::from_black_alpha(150))
        .inner_margin(egui::Margin::symmetric(14, 10))
        .corner_radius(8.0)
        .show(ui, add);
}

fn toast_area(ctx: &egui::Context, message: &str) {
    egui::Area::new("tsubu.toast".into())
        .anchor(egui::Align2::LEFT_BOTTOM, [20.0, -24.0])
        .interactable(false)
        .show(ctx, |ui| {
            panel(ui, |ui| {
                ui.label(egui::RichText::new(message).size(13.0));
            });
        });
}
