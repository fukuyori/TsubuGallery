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


// ---- 3D (設計書 §14.2) --------------------------------------------------

/// 手前の立体が奥の立体を隠す。
///
/// 深度バッファが効いていないと、描いた順にそのまま塗り重なる。奥のものを
/// あとに描いても隠れたままでなければならない。
#[test]
fn a_near_box_hides_a_far_one() {
    let Some((device, queue)) = gpu() else {
        eprintln!("GPU が無いので飛ばします");
        return;
    };
    let mut batch = BatchRenderer::new(&device);
    let mut capturer = Capturer::new();
    let mut g = Graphics::new();

    g.begin_frame(W as f32, H as f32);
    g.set_canvas(W as f32, H as f32);
    g.enable_3d(tsubu_renderer::Origin::Center);
    g.background(0.0);
    g.no_stroke();

    // 手前に赤、そのあと奥に青。順番どおりなら青が勝ってしまう。
    g.fill_rgb(255.0, 0.0, 0.0);
    g.translate_3d(0.0, 0.0, 20.0);
    g.draw_box(40.0, 40.0, 4.0);
    g.translate_3d(0.0, 0.0, -60.0);
    g.fill_rgb(0.0, 0.0, 255.0);
    g.draw_box(40.0, 40.0, 4.0);

    let image = capturer.capture(&device, &queue, &mut batch, &g, W, H).expect("撮れる");
    let (r, _, b) = pixel(&image, W / 2, H / 2);
    assert!(r > 200 && b < 60, "奥のものが手前を隠しています: {:?}", pixel(&image, W / 2, H / 2));
}

/// 2D だけの作品は、これまでどおり描いた順に重なる。
///
/// 深度バッファを足したせいで、あとから描いたものが隠れるようになっては
/// 困る。作品のほとんどは 2D なので、ここが本丸。
#[test]
fn flat_shapes_still_paint_in_order() {
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
    for _ in 0..8 {
        g.fill_rgb(255.0, 0.0, 0.0);
        g.rect(0.0, 0.0, W as f32, H as f32);
        g.fill_rgb(0.0, 0.0, 255.0);
        g.rect(0.0, 0.0, W as f32, H as f32);
    }

    let image = capturer.capture(&device, &queue, &mut batch, &g, W, H).expect("撮れる");
    assert_eq!(pixel(&image, W / 2, H / 2), (0, 0, 255), "最後に描いた色になりません");
}

/// 半透明の background() は 3D の作品でも効く。
///
/// 画面いっぱいの 1 枚が深さを書き込むと、そのあとの立体がぜんぶ隠れる。
/// 一度そこで詰まったので番人を置く。
#[test]
fn a_translucent_background_does_not_swallow_the_solids() {
    let Some((device, queue)) = gpu() else {
        eprintln!("GPU が無いので飛ばします");
        return;
    };
    let mut batch = BatchRenderer::new(&device);
    let mut capturer = Capturer::new();
    let mut g = Graphics::new();

    g.begin_frame(W as f32, H as f32);
    g.set_canvas(W as f32, H as f32);
    g.enable_3d(tsubu_renderer::Origin::Center);
    g.no_stroke();
    g.background_color(tsubu_renderer::Color::rgba(0.0, 0.0, 0.0, 0.5));
    g.fill_rgb(255.0, 0.0, 0.0);
    g.draw_box(40.0, 40.0, 40.0);

    let image = capturer.capture(&device, &queue, &mut batch, &g, W, H).expect("撮れる");
    let (r, ..) = pixel(&image, W / 2, H / 2);
    assert!(r > 100, "立体が背景に隠されました: {:?}", pixel(&image, W / 2, H / 2));
}

/// `lights()` を呼ぶと面ごとに明るさが変わる。
#[test]
fn lights_shade_the_faces_differently() {
    let Some((device, queue)) = gpu() else {
        eprintln!("GPU が無いので飛ばします");
        return;
    };
    let mut batch = BatchRenderer::new(&device);
    let mut capturer = Capturer::new();

    let mut brightness = |lit: bool| {
        let mut g = Graphics::new();
        g.begin_frame(W as f32, H as f32);
        g.set_canvas(W as f32, H as f32);
        g.enable_3d(tsubu_renderer::Origin::Center);
        g.background(0.0);
        g.no_stroke();
        g.lights(lit);
        g.fill_rgb(255.0, 255.0, 255.0);
        // 斜めに向けて、正面と側面の両方が見えるようにする。
        g.rotate_axis(1.0, [0.0, 1.0, 0.0]);
        g.draw_box(30.0, 30.0, 30.0);
        let image = capturer.capture(&device, &queue, &mut batch, &g, W, H).expect("撮れる");
        (pixel(&image, W / 2 + 8, H / 2).0, pixel(&image, W / 2 - 10, H / 2).0)
    };

    // 明かりが無ければ、どの面も塗った色そのまま。
    let (front, side) = brightness(false);
    assert_eq!((front, side), (255, 255), "明かり無しで陰影がつきました");
    // 明かりを点けると、視点を向いた面ほど明るくなる。
    let (front, side) = brightness(true);
    assert!(front < 250 && side < 250, "明かりを点けても素の色のままです: {front} と {side}");
    assert!(
        front.abs_diff(side) > 20,
        "面の向きで明るさが変わりません: {front} と {side}"
    );
}
