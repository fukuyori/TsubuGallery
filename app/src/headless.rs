//! ウィンドウを開かずに全作品のサムネイルを生成する。
//!
//! Viewer と同じ Renderer / Capturer / VM を通るので、これが通れば
//! 「コンパイル → 実行 → フレーム取得」の縦串が成立している。サムネイルの一括
//! 再生成や CI での描画スモークテストにも使う。

use std::path::Path;

use tsubu_core::DataPaths;
use tsubu_renderer::{BatchRenderer, Capturer, Graphics};

use crate::loader;
use crate::thumbnail;

/// 全作品のサムネイルを作る。`default_frame` は作品側に指定が無いときに使う。
pub fn capture_all(
    paths: &DataPaths,
    out_dir: &Path,
    width: u32,
    height: u32,
    default_frame: u64,
) -> Result<Vec<String>, String> {
    let instance = wgpu::Instance::default();

    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        // サーフェスが無くてよいのがヘッドレスの利点。
        compatible_surface: None,
        force_fallback_adapter: false,
        apply_limit_buckets: false,
    }))
    .map_err(|e| format!("GPU アダプタが見つかりません: {e}"))?;

    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("tsubu.headless.device"),
        required_features: wgpu::Features::empty(),
        required_limits: wgpu::Limits::downlevel_defaults().using_resolution(adapter.limits()),
        ..Default::default()
    }))
    .map_err(|e| format!("GPU デバイスを取得できませんでした: {e}"))?;

    let mut batch = BatchRenderer::new(&device);
    let mut capturer = Capturer::new();
    let mut graphics = Graphics::new();
    graphics.font.set_fonts(crate::fonts::load_sketch_fonts());
    let mut written = Vec::new();

    for outcome in loader::load_library(paths) {
        let mut loaded = outcome.sketch;
        let frame = if loaded.info.thumbnail_frame > 0 {
            loaded.info.thumbnail_frame
        } else {
            default_frame
        };

        capturer.begin();
        graphics.reset_state();
        graphics.begin_frame(width as f32, height as f32);
        loaded.step(&mut graphics);
        capturer.draw(&device, &queue, &mut batch, &graphics, width, height);

        // 作品は frameCount から絵を決めるので、目標フレームまで進めてから撮る。
        // 1 枚ずつ積むのは、残像を使う作品を本物と同じ絵にするため。
        for f in 1..=frame {
            graphics.begin_frame(width as f32, height as f32);
            graphics.frame_count = f;
            graphics.time = f as f32 / 60.0;
            loaded.step(&mut graphics);
            capturer.draw(&device, &queue, &mut batch, &graphics, width, height);
        }

        if let Some(error) = loaded.error() {
            log::error!("{} は実行できません: {error}", loaded.info.id);
            continue;
        }

        let image = capturer
            .read(&device, &queue, width, height)
            .map_err(|e| format!("{} のキャプチャに失敗しました: {e}", loaded.info.id))?;

        let path = out_dir.join(format!("{}.png", loaded.info.id));
        thumbnail::save_png(&image, &path)?;
        log::info!("{} (frame {frame}) → {}", loaded.info.title, path.display());
        written.push(path.display().to_string());
    }

    Ok(written)
}
