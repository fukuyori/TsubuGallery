//! キャンバスがフレームをまたいで残ることを、実際に GPU へ描いて確かめる。
//!
//! CPU 側の [`DrawList`] を見るだけでは足りない。「消さない」と決めた結果が
//! テクスチャに残っているかは、読み戻さないと分からない。

use std::sync::OnceLock;
use tsubu_renderer::{BatchRenderer, Capturer, Graphics};

const W: u32 = 64;
const H: u32 = 64;

/// テストを動かす GPU。無い環境ではテストごと飛ばす。
fn gpu() -> Option<&'static (wgpu::Device, wgpu::Queue)> {
    // libtest は各テストを並列に動かす。テストごとに Instance と Device を作ると、
    // 一部の Linux GPU ドライバが初期化競合でプロセスごと落ちるため共有する。
    static GPU: OnceLock<Option<(wgpu::Device, wgpu::Queue)>> = OnceLock::new();
    GPU.get_or_init(|| {
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
    })
    .as_ref()
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
    let mut batch = BatchRenderer::new(device);
    let mut capturer = Capturer::new();
    let mut g = Graphics::new();

    capturer.begin();

    // 1 枚目: 黒で塗って、左半分に赤い四角を置く。
    g.begin_frame(W as f32, H as f32);
    g.background(0.0);
    g.no_stroke();
    g.fill_rgb(255.0, 0.0, 0.0);
    g.rect(0.0, 0.0, 32.0, 64.0);
    capturer.draw(device, queue, &mut batch, &g, W, H);

    // 2 枚目: background を呼ばず、右半分に緑の四角を置く。
    g.begin_frame(W as f32, H as f32);
    g.no_stroke();
    g.fill_rgb(0.0, 255.0, 0.0);
    g.rect(32.0, 0.0, 32.0, 64.0);
    capturer.draw(device, queue, &mut batch, &g, W, H);

    let image = capturer.read(device, queue, W, H).expect("読み戻せる");

    let (r, _, _) = pixel(&image, 16, 32);
    assert!(
        r > 200,
        "1 枚目の赤が消えています: {:?}",
        pixel(&image, 16, 32)
    );
    let (_, green, _) = pixel(&image, 48, 32);
    assert!(
        green > 200,
        "2 枚目の緑が出ていません: {:?}",
        pixel(&image, 48, 32)
    );
}

#[test]
fn calling_background_wipes_the_previous_frame() {
    let Some((device, queue)) = gpu() else {
        eprintln!("GPU が無いので飛ばします");
        return;
    };
    let mut batch = BatchRenderer::new(device);
    let mut capturer = Capturer::new();
    let mut g = Graphics::new();

    capturer.begin();

    g.begin_frame(W as f32, H as f32);
    g.background(0.0);
    g.no_stroke();
    g.fill_rgb(255.0, 0.0, 0.0);
    g.rect(0.0, 0.0, 32.0, 64.0);
    capturer.draw(device, queue, &mut batch, &g, W, H);

    // 今度は塗り直す。前のフレームの赤は残ってはいけない。
    g.begin_frame(W as f32, H as f32);
    g.background(0.0);
    capturer.draw(device, queue, &mut batch, &g, W, H);

    let image = capturer.read(device, queue, W, H).expect("読み戻せる");
    assert_eq!(
        pixel(&image, 16, 32),
        (0, 0, 0),
        "background() で消えていません"
    );
}

#[test]
fn blur_affects_only_the_geometry_before_its_call() {
    let Some((device, queue)) = gpu() else {
        eprintln!("GPU が無いので飛ばします");
        return;
    };
    let mut batch = BatchRenderer::new(device);
    let mut capturer = Capturer::new();
    let mut g = Graphics::new();

    capturer.begin();
    g.begin_frame(W as f32, H as f32);
    g.background(0.0);
    g.no_stroke();
    g.fill_rgb(255.0, 255.0, 255.0);
    g.rect(16.0, 24.0, 4.0, 16.0);
    g.blur(3.0);
    // filter() の後ろなので、この赤はぼけてはいけない。
    g.fill_rgb(255.0, 0.0, 0.0);
    g.rect(48.0, 24.0, 4.0, 16.0);
    capturer.draw(device, queue, &mut batch, &g, W, H);

    let image = capturer.read(device, queue, W, H).expect("読み戻せる");
    let spread = pixel(&image, 13, 32);
    assert!(
        spread.0 > 10,
        "白い四角の外へぼけが広がっていません: {spread:?}"
    );

    let red = pixel(&image, 49, 32);
    assert!(
        red.0 > 200 && red.1 < 20 && red.2 < 20,
        "後段の赤が描けていません: {red:?}"
    );
    let before_red = pixel(&image, 45, 32);
    assert!(
        before_red.0 < 10,
        "filter() 後の赤までぼけています: {before_red:?}"
    );
}

#[test]
fn a_translucent_background_fades_the_previous_frame() {
    let Some((device, queue)) = gpu() else {
        eprintln!("GPU が無いので飛ばします");
        return;
    };
    let mut batch = BatchRenderer::new(device);
    let mut capturer = Capturer::new();
    let mut g = Graphics::new();

    capturer.begin();

    g.begin_frame(W as f32, H as f32);
    g.background(0.0);
    g.no_stroke();
    g.fill_rgb(255.0, 255.0, 255.0);
    g.rect(0.0, 0.0, 64.0, 64.0);
    capturer.draw(device, queue, &mut batch, &g, W, H);

    let full = pixel(
        &capturer.read(device, queue, W, H).expect("読み戻せる"),
        32,
        32,
    )
    .0;
    assert!(full > 250, "白が出ていません: {full}");

    // 半透明の黒を重ねる。消えはせず、暗くなるだけ。
    for _ in 0..4 {
        g.begin_frame(W as f32, H as f32);
        g.background_color(tsubu_renderer::Color::rgba(0.0, 0.0, 0.0, 0.2));
        capturer.draw(device, queue, &mut batch, &g, W, H);
    }

    let faded = pixel(
        &capturer.read(device, queue, W, H).expect("読み戻せる"),
        32,
        32,
    )
    .0;
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
    let mut batch = BatchRenderer::new(device);
    let mut capturer = Capturer::new();
    let mut g = Graphics::new();

    g.begin_frame(W as f32, H as f32);
    g.background(0.0);
    g.no_stroke();
    g.fill_rgb(255.0, 0.0, 0.0);
    g.rect(0.0, 0.0, 64.0, 64.0);

    let image = capturer
        .capture(device, queue, &mut batch, &g, W, H)
        .expect("撮れる");
    assert_eq!(pixel(&image, 32, 32), (255, 0, 0), "図形が消えています");
}

/// 文字はフォントが無ければ何も描かない。落ちてはいけない。
#[test]
fn text_without_a_font_is_silent() {
    let Some((device, queue)) = gpu() else {
        eprintln!("GPU が無いので飛ばします");
        return;
    };
    let mut batch = BatchRenderer::new(device);
    let mut capturer = Capturer::new();
    let mut g = Graphics::new();

    g.begin_frame(W as f32, H as f32);
    g.background(0.0);
    g.set_text_size(40.0);
    g.text("あ", 5.0, 50.0);

    let image = capturer
        .capture(device, queue, &mut batch, &g, W, H)
        .expect("撮れる");
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
    let mut batch = BatchRenderer::new(device);
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

    let image = capturer
        .capture(device, queue, &mut batch, &g, W, H)
        .expect("撮れる");
    let (r, _, b) = pixel(&image, W / 2, H / 2);
    assert!(
        r > 200 && b < 60,
        "奥のものが手前を隠しています: {:?}",
        pixel(&image, W / 2, H / 2)
    );
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
    let mut batch = BatchRenderer::new(device);
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

    let image = capturer
        .capture(device, queue, &mut batch, &g, W, H)
        .expect("撮れる");
    assert_eq!(
        pixel(&image, W / 2, H / 2),
        (0, 0, 255),
        "最後に描いた色になりません"
    );
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
    let mut batch = BatchRenderer::new(device);
    let mut capturer = Capturer::new();
    let mut g = Graphics::new();

    g.begin_frame(W as f32, H as f32);
    g.set_canvas(W as f32, H as f32);
    g.enable_3d(tsubu_renderer::Origin::Center);
    g.no_stroke();
    g.background_color(tsubu_renderer::Color::rgba(0.0, 0.0, 0.0, 0.5));
    g.fill_rgb(255.0, 0.0, 0.0);
    g.draw_box(40.0, 40.0, 40.0);

    let image = capturer
        .capture(device, queue, &mut batch, &g, W, H)
        .expect("撮れる");
    let (r, ..) = pixel(&image, W / 2, H / 2);
    assert!(
        r > 100,
        "立体が背景に隠されました: {:?}",
        pixel(&image, W / 2, H / 2)
    );
}

/// `lights()` を呼ぶと面ごとに明るさが変わる。
#[test]
fn lights_shade_the_faces_differently() {
    let Some((device, queue)) = gpu() else {
        eprintln!("GPU が無いので飛ばします");
        return;
    };
    let mut batch = BatchRenderer::new(device);
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
        let image = capturer
            .capture(device, queue, &mut batch, &g, W, H)
            .expect("撮れる");
        (
            pixel(&image, W / 2 + 8, H / 2).0,
            pixel(&image, W / 2 - 10, H / 2).0,
        )
    };

    // 明かりが無ければ、どの面も塗った色そのまま。
    let (front, side) = brightness(false);
    assert_eq!((front, side), (255, 255), "明かり無しで陰影がつきました");
    // 明かりを点けると、視点を向いた面ほど明るくなる。
    let (front, side) = brightness(true);
    assert!(
        front < 250 && side < 250,
        "明かりを点けても素の色のままです: {front} と {side}"
    );
    assert!(
        front.abs_diff(side) > 20,
        "面の向きで明るさが変わりません: {front} と {side}"
    );
}

/// つぶやき GLSL が本当に GPU で塗られていることを、読み戻して確かめる。
///
/// 翻訳が通っただけでは分からない。パイプラインを組んで描くところまで通し、
/// uniform の `r` と `t` が届いているかを画素で見る。
#[test]
fn a_tweet_sized_shader_paints_the_whole_frame() {
    let Some((device, queue)) = gpu() else {
        eprintln!("GPU が無いので飛ばします");
        return;
    };
    let mut batch = BatchRenderer::new(device);
    let mut capturer = Capturer::new();
    let mut g = Graphics::new();

    // 左下を原点に、x は横位置、y は縦位置、青は t で決まる。
    let wgsl = tsubu_renderer::shader::compile("o = vec4(FC.xy / r, t, 1.);").expect("通る");
    let paint = tsubu_renderer::ShaderPaint {
        wgsl: wgsl.into(),
        key: 1,
        time: 0.5,
        frame: 0.0,
        mouse: [0.0, 0.0],
    };

    capturer.begin();
    g.begin_frame(W as f32, H as f32);
    g.paint_with_shader(paint);
    capturer.draw(device, queue, &mut batch, &g, W, H);
    let image = capturer.read(device, queue, W, H).expect("読み戻せる");

    // 図形を 1 つも積んでいないのに、四隅まで塗られている。
    let (left, _, _) = pixel(&image, 2, 32);
    let (right, _, _) = pixel(&image, 61, 32);
    assert!(
        left < 40 && right > 215,
        "FC.x が横に伸びていない: {left} {right}"
    );

    // GLSL の gl_FragCoord は左下原点。上のほうが明るくなる。
    let (_, top, _) = pixel(&image, 32, 2);
    let (_, bottom, _) = pixel(&image, 32, 61);
    assert!(
        top > 215 && bottom < 40,
        "上下が逆になっています: {top} {bottom}"
    );

    // t は uniform で届く。0.5 なので青は中くらい。
    let (_, _, blue) = pixel(&image, 32, 32);
    assert!((100..=155).contains(&blue), "t が届いていません: {blue}");
}

/// FragCoord / ShaderToy 流の `mainImage` と、vec4 1 本から作る mat2 を使う作品を
/// GPU まで通す。sketch 59 が使う互換経路の縮小版。
#[test]
fn a_main_image_shader_with_a_golfed_mat2_renders() {
    let Some((device, queue)) = gpu() else {
        eprintln!("GPU が無いので飛ばします");
        return;
    };
    let mut batch = BatchRenderer::new(device);
    let mut capturer = Capturer::new();
    let mut g = Graphics::new();
    let source = r#"
#define R(a) mat2(cos(a + vec4(0, 33, 11, 0)))
void mainImage(out vec4 fragColor, in vec2 fragCoord) {
    vec2 p = (fragCoord - 0.5 * iResolution.xy) / iResolution.y;
    p *= R(iTime);
    fragColor = vec4(abs(p), 0.5 + 0.5 * sin(iTime), 1.0);
}
"#;
    let wgsl = tsubu_renderer::shader::compile(source).expect("sketch 59 互換 GLSL が通る");

    capturer.begin();
    g.begin_frame(W as f32, H as f32);
    g.paint_with_shader(tsubu_renderer::ShaderPaint {
        wgsl: wgsl.into(),
        key: 59,
        time: 0.5,
        frame: 30.0,
        mouse: [0.0, 0.0],
    });
    capturer.draw(device, queue, &mut batch, &g, W, H);
    let image = capturer.read(device, queue, W, H).expect("読み戻せる");

    let left = pixel(&image, 4, H / 2);
    let right = pixel(&image, W - 5, H / 2);
    assert_ne!(left, right, "iResolution を使った座標変化が出ていません");
    let blue = pixel(&image, W / 2, H / 2).2;
    assert!(
        (175..=205).contains(&blue),
        "iTime が届いていません: {blue}"
    );
}

/// naga は GLSL のブロック変数を WGSL の関数先頭へ持ち上げる。明示的な初期化を
/// 補わないと、外側ループへ入り直したときに内側のループ変数が前回値を引き継ぐ。
#[test]
fn a_golfed_inner_loop_restarts_from_zero() {
    let Some((device, queue)) = gpu() else {
        eprintln!("GPU が無いので飛ばします");
        return;
    };
    let mut batch = BatchRenderer::new(device);
    let mut capturer = Capturer::new();
    let mut g = Graphics::new();

    let wgsl = tsubu_renderer::shader::compile(
        "for(int outer;outer++<2;)for(int inner;inner++<2;)o.r+=.1;",
    )
    .expect("通る");
    capturer.begin();
    g.begin_frame(W as f32, H as f32);
    g.paint_with_shader(tsubu_renderer::ShaderPaint {
        wgsl: wgsl.into(),
        key: 61,
        time: 0.0,
        frame: 0.0,
        mouse: [0.0, 0.0],
    });
    capturer.draw(device, queue, &mut batch, &g, W, H);
    let image = capturer.read(device, queue, W, H).expect("読み戻せる");

    let (red, _, _) = pixel(&image, W / 2, H / 2);
    assert!(
        (90..=115).contains(&red),
        "内側ループが 2 回目にリセットされていません: {red}"
    );
}

/// 同じキャンバスで GLSL と図形を行き来しても壊れない。
///
/// Viewer は 1 つのキャンバスを全作品で使い回す。GLSL の作品から Processing の
/// 作品へ切り替えたときに、前のシェーダーが残っていては困る。
#[test]
fn switching_from_a_shader_back_to_shapes_works() {
    let Some((device, queue)) = gpu() else {
        eprintln!("GPU が無いので飛ばします");
        return;
    };
    let mut batch = BatchRenderer::new(device);
    let mut capturer = Capturer::new();
    let mut g = Graphics::new();

    let wgsl = tsubu_renderer::shader::compile("o = vec4(1, 0, 0, 1);").expect("通る");
    capturer.begin();
    g.begin_frame(W as f32, H as f32);
    g.paint_with_shader(tsubu_renderer::ShaderPaint {
        wgsl: wgsl.into(),
        key: 2,
        time: 0.0,
        frame: 0.0,
        mouse: [0.0, 0.0],
    });
    capturer.draw(device, queue, &mut batch, &g, W, H);
    let image = capturer.read(device, queue, W, H).expect("読み戻せる");
    assert!(
        pixel(&image, 32, 32).0 > 200,
        "シェーダーが赤で塗っていない"
    );

    // 次のフレームはシェーダー無し。begin_frame で消えている。
    g.begin_frame(W as f32, H as f32);
    g.background(0.0);
    g.no_stroke();
    g.fill_rgb(0.0, 255.0, 0.0);
    g.rect(0.0, 0.0, 64.0, 64.0);
    capturer.draw(device, queue, &mut batch, &g, W, H);
    let image = capturer.read(device, queue, W, H).expect("読み戻せる");
    assert!(
        pixel(&image, 32, 32).1 > 200,
        "図形へ戻れていない: {:?}",
        pixel(&image, 32, 32)
    );
}
