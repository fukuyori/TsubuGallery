//! GPU デバイスとサーフェスの管理 (設計書 §9.2 の Platform Adapter 相当)。
//!
//! ウィンドウ 1 枚 / サーフェス 1 枚を Viewer と egui で共有する。フレーム内で
//! パスは 1 回しか開かず、スケッチを描いた上に UI を重ねる。

use std::sync::Arc;

use tsubu_renderer::{BatchRenderer, Canvas, MsaaTarget, SAMPLE_COUNT};
use winit::window::Window;

pub struct Gfx {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub surface: wgpu::Surface<'static>,
    pub config: wgpu::SurfaceConfiguration,
    pub batch: BatchRenderer,
    pub msaa: MsaaTarget,
    /// スケッチの絵。`background()` を呼ばないフレームのために残しておく。
    pub canvas: Canvas,
    /// 選ばれた GPU の名前。`I` の情報に出す。
    ///
    /// 毎フレーム組み立て直さないよう、ここで作っておく。起動中に変わらない。
    pub gpu: String,
    /// どの経路で描いているか。`Vulkan · DiscreteGpu` のような形。
    ///
    /// 内蔵と外付けの 2 枚が載っている機械では、どちらが選ばれたかで速さが
    /// 何倍も変わる。動きが遅いという報告を受けたとき、最初に見る場所。
    pub backend: String,
}

impl Gfx {
    pub fn new(window: Arc<Window>) -> Result<Self, String> {
        let size = window.inner_size();
        let instance = wgpu::Instance::default();

        let surface = instance
            .create_surface(window)
            .map_err(|e| format!("サーフェスを作成できませんでした: {e}"))?;

        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
            apply_limit_buckets: false,
        }))
        .map_err(|e| format!("GPU アダプタが見つかりません: {e}"))?;

        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("tsubu.device"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::downlevel_defaults().using_resolution(adapter.limits()),
            ..Default::default()
        }))
        .map_err(|e| format!("GPU デバイスを取得できませんでした: {e}"))?;

        let caps = surface.get_capabilities(&adapter);
        // 頂点色は sRGB のまま渡すので、自動変換のかからない非 sRGB を優先する。
        // egui も同じフレームバッファへ描くため、こちらのほうが色が合う。
        let format = caps
            .formats
            .iter()
            .copied()
            .find(|f| !f.is_srgb())
            .unwrap_or(caps.formats[0]);

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            color_space: wgpu::SurfaceColorSpace::Auto,
            width: size.width.max(1),
            height: size.height.max(1),
            // 60fps を安定させたいので v-sync を使う (設計書 §22)。
            present_mode: wgpu::PresentMode::AutoVsync,
            alpha_mode: caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);

        let info = adapter.get_info();
        log::info!(
            "GPU: {} ({:?} / {:?}) driver {} {} / surface format {:?}",
            info.name,
            info.backend,
            info.device_type,
            info.driver,
            info.driver_info,
            format
        );

        let batch = BatchRenderer::new(&device);
        let canvas = Canvas::new(&device, format);

        Ok(Self {
            device,
            queue,
            surface,
            config,
            batch,
            msaa: MsaaTarget::new(SAMPLE_COUNT),
            canvas,
            gpu: info.name,
            // 種類まで出す。Backend も DeviceType も固有名詞なので翻訳しない。
            backend: format!("{:?} · {:?}", info.backend, info.device_type),
        })
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        self.config.width = width;
        self.config.height = height;
        self.surface.configure(&self.device, &self.config);
    }

    pub fn format(&self) -> wgpu::TextureFormat {
        self.config.format
    }

    pub fn size(&self) -> (u32, u32) {
        (self.config.width, self.config.height)
    }
}
