//! キャンバスがフレームをまたいで残ることを、実際に GPU へ描いて確かめる。
//!
//! CPU 側の [`DrawList`] を見るだけでは足りない。「消さない」と決めた結果が
//! テクスチャに残っているかは、読み戻さないと分からない。

use tsubu_renderer::{BatchRenderer, Capturer, Graphics};

const W: u32 = 64;
const H: u32 = 64;

/// テストを動かす GPU。無い環境ではテストごと飛ばす。
fn gpu() -> Option<(wgpu::Device, wgpu::Queue)> {
    let instance = wgpu::Instance::default();
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        compatible_surface: None,
        force_fallback_adapter: false,
        apply_limit_buckets: false,
    }))
    .ok()?;
    pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("tsubu.test"),
        required_features: wgpu::Features::empty(),
        required_limits: wgpu::Limits::downlevel_defaults().using_resolution(adapter.limits()),
        ..Default::default()
    }))
    .ok()
}

/// `(x, y)` の画素を `(r, g, b)` で返す。
fn pixel(image: &tsubu_renderer::CapturedImage, x: u32, y: u32) -> (u8, u8, u8) {
    let i = ((y * image.width + x) * 4) as usize;
    (image.rgba[i], image.rgba[i + 1], image.rgba[i + 2])
}

#[test]
fn a_frame_without_background_keeps_what_came_before() {
    let Some((device, queue)) = gpu() else {
        eprintln!("GPU が無いので飛ばします");
        return;
    };
    let mut batch = BatchRenderer::new(&device);
    let mut capturer = Capturer::new();
    let mut g = Graphics::new();

    capturer.begin();

    // 1 枚目: 黒で塗って、左半分に赤い四角を置く。
    g.begin_frame(W as f32, H as f32);
    g.background(0.0);
    g.no_stroke();
    g.fill_rgb(255.0, 0.0, 0.0);
    g.rect(0.0, 0.0, 32.0, 64.0);
    capturer.draw(&device, &queue, &mut batch, &g, W, H);

    // 2 枚目: background を呼ばず、右半分に緑の四角を置く。
    g.begin_frame(W as f32, H as f32);
    g.no_stroke();
    g.fill_rgb(0.0, 255.0, 0.0);
    g.rect(32.0, 0.0, 32.0, 64.0);
    capturer.draw(&device, &queue, &mut batch, &g, W, H);

    let image = capturer.read(&device, &queue, W, H).expect("読み戻せる");

    let (r, _, _) = pixel(&image, 16, 32);
    assert!(r > 200, "1 枚目の赤が消えています: {:?}", pixel(&image, 16, 32));
    let (_, green, _) = pixel(&image, 48, 32);
    assert!(green > 200, "2 枚目の緑が出ていません: {:?}", pixel(&image, 48, 32));
}

#[test]
fn calling_background_wipes_the_previous_frame() {
    let Some((device, queue)) = gpu() else {
        eprintln!("GPU が無いので飛ばします");
        return;
    };
    let mut batch = BatchRenderer::new(&device);
    let mut capturer = Capturer::new();
    let mut g = Graphics::new();

    capturer.begin();

    g.begin_frame(W as f32, H as f32);
    g.background(0.0);
    g.no_stroke();
    g.fill_rgb(255.0, 0.0, 0.0);
    g.rect(0.0, 0.0, 32.0, 64.0);
    capturer.draw(&device, &queue, &mut batch, &g, W, H);

    // 今度は塗り直す。前のフレームの赤は残ってはいけない。
    g.begin_frame(W as f32, H as f32);
    g.background(0.0);
    capturer.draw(&device, &queue, &mut batch, &g, W, H);

    let image = capturer.read(&device, &queue, W, H).expect("読み戻せる");
    assert_eq!(pixel(&image, 16, 32), (0, 0, 0), "background() で消えていません");
}

#[test]
fn a_translucent_background_fades_the_previous_frame() {
    let Some((device, queue)) = gpu() else {
        eprintln!("GPU が無いので飛ばします");
        return;
    };
    let mut batch = BatchRenderer::new(&device);
    let mut capturer = Capturer::new();
    let mut g = Graphics::new();

    capturer.begin();

    g.begin_frame(W as f32, H as f32);
    g.background(0.0);
    g.no_stroke();
    g.fill_rgb(255.0, 255.0, 255.0);
    g.rect(0.0, 0.0, 64.0, 64.0);
    capturer.draw(&device, &queue, &mut batch, &g, W, H);

    let full = pixel(&capturer.read(&device, &queue, W, H).expect("読み戻せる"), 32, 32).0;
    assert!(full > 250, "白が出ていません: {full}");

    // 半透明の黒を重ねる。消えはせず、暗くなるだけ。
    for _ in 0..4 {
        g.begin_frame(W as f32, H as f32);
        g.background_color(tsubu_renderer::Color::rgba(0.0, 0.0, 0.0, 0.2));
        capturer.draw(&device, &queue, &mut batch, &g, W, H);
    }

    let faded = pixel(&capturer.read(&device, &queue, W, H).expect("読み戻せる"), 32, 32).0;
    assert!(faded < full, "暗くなっていません: {faded} (元 {full})");
    assert!(faded > 0, "消えてしまいました: {faded}");
}


/// 図形はフォントを読み込んでいなくても見える。
///
/// 文字と図形は同じ経路で描く。字形のアトラスを GPU へ送り忘れると、図形まで
/// 透明になって画面が真っ黒になる。一度それで詰まったので番人を置く。
#[test]
fn shapes_are_visible_without_any_font() {
    let Some((device, queue)) = gpu() else {
        eprintln!("GPU が無いので飛ばします");
        return;
    };
    let mut batch = BatchRenderer::new(&device);
    let mut capturer = Capturer::new();
    let mut g = Graphics::new();

    g.begin_frame(W as f32, H as f32);
    g.background(0.0);
    g.no_stroke();
    g.fill_rgb(255.0, 0.0, 0.0);
    g.rect(0.0, 0.0, 64.0, 64.0);

    let image = capturer.capture(&device, &queue, &mut batch, &g, W, H).expect("撮れる");
    assert_eq!(pixel(&image, 32, 32), (255, 0, 0), "図形が消えています");
}

/// 文字はフォントが無ければ何も描かない。落ちてはいけない。
#[test]
fn text_without_a_font_is_silent() {
    let Some((device, queue)) = gpu() else {
        eprintln!("GPU が無いので飛ばします");
        return;
    };
    let mut batch = BatchRenderer::new(&device);
    let mut capturer = Capturer::new();
    let mut g = Graphics::new();

    g.begin_frame(W as f32, H as f32);
    g.background(0.0);
    g.set_text_size(40.0);
    g.text("あ", 5.0, 50.0);

    let image = capturer.capture(&device, &queue, &mut batch, &g, W, H).expect("撮れる");
    assert_eq!(pixel(&image, 32, 32), (0, 0, 0));
}
