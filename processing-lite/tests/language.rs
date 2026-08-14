
//! 言語機能が「動く」だけでなく「正しい」ことを確かめる。
//!
//! 落ちないことと結果が合っていることは別。描かれた図形の数や、色に載せて
//! 運んだ計算結果まで見る。
use tsubu_processing_lite::{Sketch, VmSketch};
use tsubu_renderer::Graphics;

/// 1 フレーム描いて、円が描かれた個数を数える。
fn shapes(src: &str) -> usize {
    let mut s = VmSketch::compile(src, 1).expect("コンパイルできる");
    let mut g = Graphics::new();
    g.begin_frame(400.0, 400.0);
    s.setup(&mut g);
    g.begin_frame(400.0, 400.0);
    g.frame_count = 1;
    s.draw(&mut g);
    assert!(s.error().is_none(), "{:?}", s.error());
    g.draw_list().indices.len()
}

/// 円 1 個ぶんの三角形の数を基準に、何個描かれたかを返す。
fn count(src: &str, one: &str) -> usize {
    let unit = shapes(one);
    assert!(unit > 0);
    shapes(src) / unit
}

#[test]
fn values_are_right() {
    let one_java = "void draw(){circle(0,0,10);}";
    let one_p5 = "draw=_=>{createCanvas(400,400);circle(0,0,10)}";

    // break: 10 個で止まる (i=0..9)
    assert_eq!(count("void draw(){for(int i=0;i<99;i++){if(i>9)break;circle(0,0,10);}}", one_java), 10);
    // continue: 偶数を飛ばして 5 個 (i=1,3,5,7,9)
    assert_eq!(count("void draw(){for(int i=0;i<10;i++){if(i%2==0)continue;circle(0,0,10);}}", one_java), 5);
    // 拡張 for: 3 要素
    assert_eq!(count("int[] a={1,2,3};\nvoid draw(){for(int v:a)circle(0,0,10);}", one_java), 3);
    // new float[n]: n 個
    assert_eq!(count("void draw(){float[] a=new float[7];for(int i=0;i<a.length;i++)circle(0,0,10);}", one_java), 7);
    // p5 の break / continue
    assert_eq!(count("draw=_=>{createCanvas(400,400);for(i=0;i<99;i++){if(i>4)break;circle(0,0,10)}}", one_p5), 5);
    assert_eq!(count("draw=_=>{createCanvas(400,400);for(i=0;i<10;i++){if(i%2)continue;circle(0,0,10)}}", one_p5), 5);
    // 拡張 for の中の continue / break
    assert_eq!(count("int[] a={1,2,3,4};\nvoid draw(){for(int v:a){if(v==2)continue;circle(0,0,10);}}", one_java), 3);
    assert_eq!(count("int[] a={1,2,3,4};\nvoid draw(){for(int v:a){if(v==3)break;circle(0,0,10);}}", one_java), 2);
    // 入れ子のループで break が内側だけを抜ける
    assert_eq!(
        count("void draw(){for(int i=0;i<3;i++)for(int j=0;j<9;j++){if(j>1)break;circle(0,0,10);}}", one_java),
        6
    );

    println!("  すべて期待どおり");
}

/// 計算結果そのものを確かめる。
#[test]
fn arithmetic_is_right() {
    // 画面幅いっぱいの矩形を描き、その色で値を運ぶ。
    let probe = |expr: &str| -> i32 {
        let src = format!("void draw(){{ int v = {expr}; fill(v,0,0); rect(0,0,10,10); }}");
        let mut s = VmSketch::compile(&src, 1).expect("コンパイルできる");
        let mut g = Graphics::new();
        g.begin_frame(400.0, 400.0);
        s.setup(&mut g);
        g.begin_frame(400.0, 400.0);
        s.draw(&mut g);
        assert!(s.error().is_none(), "{:?} / {src}", s.error());
        (g.draw_list().vertices[0].color[0] * 255.0).round() as i32
    };

    assert_eq!(probe("13 & 6"), 4);
    assert_eq!(probe("9 | 6"), 15);
    assert_eq!(probe("12 ^ 10"), 6);
    assert_eq!(probe("3 << 4"), 48);
    assert_eq!(probe("200 >> 2"), 50);
    assert_eq!(probe("~200 & 255"), 55);
    assert_eq!(probe("(int)7.9"), 7);
    // 16 進。詰めた色を取り出す書き方がそのまま動くこと。
    assert_eq!(probe("0xFF"), 255);
    assert_eq!(probe("(0xFF6B35 >> 16) & 255"), 255);
    assert_eq!(probe("(0xFF6B35 >> 8) & 255"), 107);
    assert_eq!(probe("0xFF6B35 & 255"), 53);
    assert_eq!(probe("(int)(15 / 2.0)"), 7);
    // シフト量は下位 5bit だけ (Java と同じ)
    assert_eq!(probe("1 << 33"), 2);
    // 優先順位: `|` より `^` より `&` が強い。
    // 正しければ 1|(2^(4&4)) = 1|6 = 7。左から順だと ((1|2)^4)&4 = 4 になる。
    assert_eq!(probe("1 | 2 ^ 4 & 4"), 7);
    // 比較よりシフトが強い。1<<3=8 なので 8>4 は真。
    assert_eq!(probe("1 << 3 > 4 ? 5 : 6"), 5);
    // 代入演算子
    assert_eq!(probe("0"), 0);
    let src = "void draw(){int v=12; v&=10; v|=1; v^=2; v<<=1; fill(v,0,0); rect(0,0,10,10);}";
    let mut s = VmSketch::compile(src, 1).unwrap();
    let mut g = Graphics::new();
    g.begin_frame(400.0, 400.0);
    s.setup(&mut g);
    g.begin_frame(400.0, 400.0);
    s.draw(&mut g);
    assert!(s.error().is_none());
    // 12&10=8, 8|1=9, 9^2=11, 11<<1=22
    assert_eq!((g.draw_list().vertices[0].color[0] * 255.0).round() as i32, 22);

    println!("  計算も期待どおり");
}

/// p5 側でも同じことができるか。
#[test]
fn the_p5_side_matches() {
    let probe = |expr: &str| -> i32 {
        let src = format!("draw=_=>{{createCanvas(400,400);fill({expr},0,0);rect(0,0,10,10)}}");
        let mut s = VmSketch::compile(&src, 1).expect("コンパイルできる");
        let mut g = Graphics::new();
        g.begin_frame(400.0, 400.0);
        s.setup(&mut g);
        g.begin_frame(400.0, 400.0);
        s.draw(&mut g);
        assert!(s.error().is_none(), "{:?} / {src}", s.error());
        (g.draw_list().vertices[0].color[0] * 255.0).round() as i32
    };

    assert_eq!(probe("13 & 6"), 4);
    assert_eq!(probe("3 << 4"), 48);
    assert_eq!(probe("0xFF & 200"), 200);
    // JavaScript の符号なし右シフト。
    assert_eq!(probe("(-8 >>> 28)"), 15);
    assert_eq!(probe("~200 & 255"), 55);
}

/// ループの外の break と continue は、動く前に止める。
#[test]
fn break_outside_a_loop_is_refused() {
    for src in [
        "void draw(){break;}",
        "void draw(){continue;}",
        "draw=_=>{createCanvas(400,400);break}",
    ] {
        match VmSketch::compile(src, 1) {
            Ok(_) => panic!("{src} は弾かれるはず"),
            Err(e) => assert!(e.to_string().contains("ループ"), "{src} → {e}"),
        }
    }
}


// ---- 2 周目 --------------------------------------------------------------

/// `switch`。Java と同じく `break` が無ければ次の case へ落ちる。
#[test]
fn switch_falls_through_like_java() {
    let one = "void draw(){circle(0,0,10);}";
    let c = |src: &str| count(src, one);

    assert_eq!(c("void draw(){switch(1){case 0:circle(0,0,10);break;case 1:circle(0,0,10);circle(0,0,10);break;}}"), 2);
    assert_eq!(c("void draw(){switch(9){case 0:circle(0,0,10);break;default:circle(0,0,10);circle(0,0,10);}}"), 2);
    // break が無ければ次の case の中身も実行する。
    assert_eq!(c("void draw(){switch(0){case 0:circle(0,0,10);case 1:circle(0,0,10);break;case 2:circle(0,0,10);}}"), 2);
    // 一致も default も無ければ何もしない。
    assert_eq!(c("void draw(){switch(9){case 0:circle(0,0,10);break;}}"), 0);
    // default が先に書かれていても、一致するラベルが優先される。
    assert_eq!(c("void draw(){switch(1){default:circle(0,0,10);break;case 1:circle(0,0,10);circle(0,0,10);break;}}"), 2);
}

/// switch の中の break は switch だけを抜け、continue は外側のループへ届く。
#[test]
fn break_and_continue_find_the_right_target() {
    let one = "void draw(){circle(0,0,10);}";
    // i=1 のときだけ switch を抜けるが、ループは続く。
    assert_eq!(
        count("void draw(){for(int i=0;i<3;i++){switch(i){case 1:break;default:circle(0,0,10);}circle(0,0,10);}}", one),
        5
    );
    // continue は switch を飛び越えてループの次の回へ。
    assert_eq!(
        count("void draw(){for(int i=0;i<4;i++){switch(i){case 1:continue;}circle(0,0,10);}}", one),
        3
    );
}

/// `for...of`。
#[test]
fn for_of_walks_an_array() {
    let one = "draw=_=>{createCanvas(400,400);circle(0,0,10)}";
    let c = |src: &str| count(src, one);

    assert_eq!(c("draw=_=>{createCanvas(400,400);for(v of [1,2,3])circle(v,0,10)}"), 3);
    assert_eq!(c("draw=_=>{createCanvas(400,400);for(const v of [1,2,3,4]){if(v==2)continue;circle(v,0,10)}}"), 3);
    assert_eq!(c("draw=_=>{createCanvas(400,400);for(let v of [1,2,3,4]){if(v==3)break;circle(v,0,10)}}"), 2);
    // 入れ子でも走査どうしが混ざらない。
    assert_eq!(c("draw=_=>{createCanvas(400,400);for(a of [1,2])for(b of [1,2,3])circle(a,b,10)}"), 6);
}

/// スプレッド。`[...Array(n)]` はつぶやき p5 の定番。
#[test]
fn spread_expands_arrays() {
    let one = "draw=_=>{createCanvas(400,400);circle(0,0,10)}";
    let c = |src: &str| count(src, one);

    assert_eq!(c("draw=_=>{createCanvas(400,400);[...Array(9)].map((_,i)=>circle(i*40,200,20))}"), 9);
    assert_eq!(c("draw=_=>{createCanvas(400,400);[...Array(5).keys()].map(i=>circle(i*40,200,20))}"), 5);
    // 展開と要素を混ぜられる。
    assert_eq!(c("draw=_=>{createCanvas(400,400);a=[1,2];for(v of [...a,3,...a])circle(v,0,10)}"), 5);
}

/// 分割代入。
#[test]
fn destructuring_assigns_in_one_go() {
    let color = |src: &str| -> (i32, i32, i32) {
        let mut s = VmSketch::compile(src, 1).expect("コンパイルできる");
        let mut g = Graphics::new();
        g.begin_frame(400.0, 400.0);
        s.setup(&mut g);
        g.begin_frame(400.0, 400.0);
        s.draw(&mut g);
        assert!(s.error().is_none(), "{:?} / {src}", s.error());
        let c = g.draw_list().vertices[0].color;
        (
            (c[0] * 255.0).round() as i32,
            (c[1] * 255.0).round() as i32,
            (c[2] * 255.0).round() as i32,
        )
    };

    assert_eq!(
        color("draw=_=>{createCanvas(400,400);[a,b]=[10,20];fill(a,b,0);rect(0,0,9,9)}"),
        (10, 20, 0)
    );
    // 入れ替え。右辺を先に作るので、両方とも元の値から取れる。
    assert_eq!(
        color("draw=_=>{createCanvas(400,400);a=1;b=2;[a,b]=[b,a];fill(a*10,b*10,0);rect(0,0,9,9)}"),
        (20, 10, 0)
    );
    // 配列の要素やプロパティへも書ける。
    assert_eq!(
        color("draw=_=>{createCanvas(400,400);o={};v=[0,0];[o.x,v[1]]=[30,40];fill(o.x,v[1],0);rect(0,0,9,9)}"),
        (30, 40, 0)
    );
}


// ---- 3 周目: 足りていなかった API ----------------------------------------

/// 描いた三角形の数。
fn triangles(src: &str) -> usize {
    let mut s = VmSketch::compile(src, 1).expect("コンパイルできる");
    let mut g = Graphics::new();
    g.begin_frame(400.0, 400.0);
    s.setup(&mut g);
    g.begin_frame(400.0, 400.0);
    g.frame_count = 1;
    s.draw(&mut g);
    assert!(s.error().is_none(), "{:?} / {src}", s.error());
    g.draw_list().indices.len() / 3
}

/// `beginShape()` / `vertex()` / `endShape()`。
#[test]
fn shapes_are_drawn() {
    // 四角形を塗ると三角形 2 つ。
    assert_eq!(
        triangles("draw=_=>{createCanvas(400,400);noStroke();beginShape();vertex(0,0);vertex(10,0);vertex(10,10);vertex(0,10);endShape()}"),
        2
    );
    // 頂点が 2 つでは塗れない。
    assert_eq!(
        triangles("draw=_=>{createCanvas(400,400);noStroke();beginShape();vertex(0,0);vertex(10,0);endShape()}"),
        0
    );
    // TRIANGLES は 3 つずつ独立した三角形。
    assert_eq!(
        triangles("draw=_=>{createCanvas(400,400);noStroke();beginShape(TRIANGLES);vertex(0,0);vertex(9,0);vertex(0,9);vertex(20,20);vertex(29,20);vertex(20,29);endShape()}"),
        2
    );
    // beginShape() を呼ばずに vertex() だけ書いても落ちない。
    assert_eq!(triangles("draw=_=>{createCanvas(400,400);vertex(0,0);vertex(9,9)}"), 0);
}

/// `arc()` は塗ると扇形になる。角度の幅に応じて分割数が変わる。
#[test]
fn arcs_scale_with_their_sweep() {
    let quarter = triangles("draw=_=>{createCanvas(400,400);noStroke();arc(200,200,100,100,0,HALF_PI)}");
    let half = triangles("draw=_=>{createCanvas(400,400);noStroke();arc(200,200,100,100,0,PI)}");
    assert!(quarter > 0);
    assert!(half > quarter, "半周 {half} が 4 分の 1 周 {quarter} より粗い");
    // 幅が 0 なら何も描かない。
    assert_eq!(triangles("draw=_=>{createCanvas(400,400);noStroke();arc(200,200,100,100,1,1)}"), 0);
}

/// `color()` と `lerpColor()`。
#[test]
fn colors_are_values() {
    let color = |src: &str| -> (i32, i32, i32) {
        let mut s = VmSketch::compile(src, 1).expect("コンパイルできる");
        let mut g = Graphics::new();
        g.begin_frame(400.0, 400.0);
        s.setup(&mut g);
        g.begin_frame(400.0, 400.0);
        s.draw(&mut g);
        assert!(s.error().is_none(), "{:?} / {src}", s.error());
        let c = g.draw_list().vertices[0].color;
        (
            (c[0] * 255.0).round() as i32,
            (c[1] * 255.0).round() as i32,
            (c[2] * 255.0).round() as i32,
        )
    };

    assert_eq!(
        color("draw=_=>{createCanvas(400,400);fill(color(255,128,0));rect(0,0,9,9)}"),
        (255, 128, 0)
    );
    // 真ん中で混ぜる。
    assert_eq!(
        color("draw=_=>{createCanvas(400,400);fill(lerpColor(color(0,0,0),color(200,100,50),.5));rect(0,0,9,9)}"),
        (100, 50, 25)
    );
    // colorMode(HSB) でも二重変換にならない。赤のまま。
    assert_eq!(
        color("draw=_=>{createCanvas(400,400);colorMode(HSB);c=color(0,100,100);fill(c);rect(0,0,9,9)}"),
        (255, 0, 0)
    );
}

/// `Math.*` は組み込みへ読み替える。
#[test]
fn the_math_namespace_works() {
    let value = |expr: &str| -> i32 {
        let src = format!("draw=_=>{{createCanvas(400,400);fill({expr},0,0);rect(0,0,9,9)}}");
        let mut s = VmSketch::compile(&src, 1).expect("コンパイルできる");
        let mut g = Graphics::new();
        g.begin_frame(400.0, 400.0);
        s.setup(&mut g);
        g.begin_frame(400.0, 400.0);
        s.draw(&mut g);
        assert!(s.error().is_none(), "{:?} / {src}", s.error());
        (g.draw_list().vertices[0].color[0] * 255.0).round() as i32
    };

    assert_eq!(value("Math.abs(-30)"), 30);
    assert_eq!(value("Math.hypot(3,4)*10"), 50);
    assert_eq!(value("Math.max(1,2,3)*10"), 30);
    assert_eq!(value("Math.round(Math.PI*10)"), 31);
    assert_eq!(value("Math.sign(-5)*-40"), 40);
    // 関数として持ち回る書き方。
    assert_eq!(value("(S=Math.sqrt)(100)*2"), 20);
}


// ---- 4 周目: 形の指定と角度の単位 ------------------------------------------

/// 描かれた図形を囲む枠を返す。位置が正しいかを見るのに使う。
fn bounds(src: &str) -> (f32, f32, f32, f32) {
    let mut s = VmSketch::compile(src, 1).expect("コンパイルできる");
    let mut g = Graphics::new();
    g.begin_frame(400.0, 400.0);
    s.setup(&mut g);
    g.begin_frame(400.0, 400.0);
    g.frame_count = 1;
    s.draw(&mut g);
    assert!(s.error().is_none(), "{:?} / {src}", s.error());

    let v = &g.draw_list().vertices;
    assert!(!v.is_empty(), "何も描かれていない: {src}");
    let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for p in v {
        x0 = x0.min(p.pos[0]);
        y0 = y0.min(p.pos[1]);
        x1 = x1.max(p.pos[0]);
        y1 = y1.max(p.pos[1]);
    }
    (x0, y0, x1, y1)
}

/// `rectMode()` で引数の意味が変わる。
#[test]
fn rect_mode_changes_where_the_rectangle_lands() {
    let corner = bounds("draw=_=>{createCanvas(400,400);noStroke();rect(100,100,40,20)}");
    assert!((corner.0 - 100.0).abs() < 0.01 && (corner.2 - 140.0).abs() < 0.01, "{corner:?}");

    // CENTER なら同じ引数で中心が (100,100) になる。
    let center = bounds("draw=_=>{createCanvas(400,400);noStroke();rectMode(CENTER);rect(100,100,40,20)}");
    assert!((center.0 - 80.0).abs() < 0.01 && (center.2 - 120.0).abs() < 0.01, "{center:?}");

    // CORNERS は 2 つの角。
    let corners = bounds("draw=_=>{createCanvas(400,400);noStroke();rectMode(CORNERS);rect(100,100,140,120)}");
    assert!((corners.0 - 100.0).abs() < 0.01 && (corners.2 - 140.0).abs() < 0.01, "{corners:?}");

    // RADIUS は中心と半径。
    let radius = bounds("draw=_=>{createCanvas(400,400);noStroke();rectMode(RADIUS);rect(100,100,40,20)}");
    assert!((radius.0 - 60.0).abs() < 0.01 && (radius.2 - 140.0).abs() < 0.01, "{radius:?}");

    // square() も rectMode に従う。
    let square = bounds("draw=_=>{createCanvas(400,400);noStroke();rectMode(CENTER);square(100,100,40)}");
    assert!((square.0 - 80.0).abs() < 0.01 && (square.2 - 120.0).abs() < 0.01, "{square:?}");
}

/// `ellipseMode()` も同じく効く。`circle()` は影響を受けない。
#[test]
fn ellipse_mode_changes_where_the_ellipse_lands() {
    let center = bounds("draw=_=>{createCanvas(400,400);noStroke();ellipse(100,100,40,40)}");
    assert!((center.0 - 80.0).abs() < 0.5, "{center:?}");

    let corner = bounds("draw=_=>{createCanvas(400,400);noStroke();ellipseMode(CORNER);ellipse(100,100,40,40)}");
    assert!((corner.0 - 100.0).abs() < 0.5, "{corner:?}");

    // circle() は常に中心指定。
    let circle = bounds("draw=_=>{createCanvas(400,400);noStroke();ellipseMode(CORNER);circle(100,100,40)}");
    assert!((circle.0 - 80.0).abs() < 0.5, "circle は ellipseMode に従わない: {circle:?}");
}

/// `angleMode(DEGREES)` は三角関数・逆関数・`rotate()` に効く。
#[test]
fn angle_mode_switches_the_unit() {
    let value = |src: &str| -> i32 {
        let mut s = VmSketch::compile(src, 1).expect("コンパイルできる");
        let mut g = Graphics::new();
        g.begin_frame(400.0, 400.0);
        s.setup(&mut g);
        g.begin_frame(400.0, 400.0);
        s.draw(&mut g);
        assert!(s.error().is_none(), "{:?} / {src}", s.error());
        (g.draw_list().vertices[0].color[0] * 255.0).round() as i32
    };

    // 度なら sin(90) がちょうど 1。
    assert_eq!(
        value("draw=_=>{createCanvas(400,400);angleMode(DEGREES);fill(sin(90)*200,0,0);rect(0,0,9,9)}"),
        200
    );
    // 逆関数の戻り値も度で返る。asin(1) = 90。
    assert_eq!(
        value("draw=_=>{createCanvas(400,400);angleMode(DEGREES);fill(asin(1)*2,0,0);rect(0,0,9,9)}"),
        180
    );
    // RADIANS へ戻せる。
    assert_eq!(
        value("draw=_=>{createCanvas(400,400);angleMode(DEGREES);angleMode(RADIANS);fill(sin(90)*200,0,0);rect(0,0,9,9)}"),
        179
    );

    // rotate() も度で受ける。90 度回すと右向きが下向きになる。
    let turned = bounds("draw=_=>{createCanvas(400,400);noStroke();angleMode(DEGREES);rotate(90);rect(100,0,20,10)}");
    assert!(turned.0 < 0.01 && turned.1 > 99.0, "回っていない: {turned:?}");
}

/// `quad()` は凹んだ四角形も正しく塗る。
#[test]
fn quads_handle_a_notch() {
    // 塗ると三角形 2 つ。凹んでいても数は変わらない。
    let src = "draw=_=>{createCanvas(400,400);noStroke();quad(0,0,100,20,40,40,100,80)}";
    assert_eq!(triangles(src), 2);
    let b = bounds(src);
    assert!(b.2 <= 100.01 && b.3 <= 80.01, "枠からはみ出しています: {b:?}");
}

/// `noLoop()` の状態を Viewer が読めること。
#[test]
fn no_loop_is_visible_to_the_host() {
    let mut s = VmSketch::compile("draw=_=>{createCanvas(400,400);noLoop();circle(0,0,10)}", 1)
        .expect("コンパイルできる");
    let mut g = Graphics::new();
    g.begin_frame(400.0, 400.0);
    assert!(g.is_looping(), "最初は回っている");
    s.setup(&mut g);
    g.begin_frame(400.0, 400.0);
    s.draw(&mut g);
    assert!(!g.is_looping(), "noLoop() が伝わっていない");
}


// ---- 5 周目: PVector ------------------------------------------------------

/// 色に載せた値を取り出す。
fn probe(src: &str) -> i32 {
    let mut s = VmSketch::compile(src, 1).expect("コンパイルできる");
    let mut g = Graphics::new();
    g.begin_frame(400.0, 400.0);
    s.setup(&mut g);
    g.begin_frame(400.0, 400.0);
    s.draw(&mut g);
    assert!(s.error().is_none(), "{:?} / {src}", s.error());
    (g.draw_list().vertices[0].color[0] * 255.0).round() as i32
}

/// p5 の `createVector()`。
#[test]
fn p5_vectors_work() {
    let v = |body: &str| probe(&format!("draw=_=>{{createCanvas(400,400);{body};rect(0,0,9,9)}}"));

    assert_eq!(v("p=createVector(30,40);fill(p.x,0,0)"), 30);
    assert_eq!(v("p=createVector(30,40);fill(p.mag(),0,0)"), 50);
    assert_eq!(v("p=createVector(10,20);p.add(createVector(5,0));fill(p.x*10,0,0)"), 150);
    // 数珠つなぎ。自分を書き換えて自分を返す。
    assert_eq!(v("p=createVector(1,2);fill(p.mult(3).add(2,0).x*20,0,0)"), 100);
    assert_eq!(v("p=createVector(1,2);p.x=7;fill(p.x*10,0,0)"), 70);
    assert_eq!(v("a=createVector(0,0);b=createVector(30,40);fill(a.dist(b),0,0)"), 50);
    assert_eq!(v("p=createVector(3,4);p.normalize();fill(p.mag()*200,0,0)"), 200);
    assert_eq!(v("p=createVector(9,9);p.limit(5);fill(p.mag()*40,0,0)"), 200);
}

/// `copy()` は別の実体。写しを変えても元は変わらない。
#[test]
fn copies_are_independent() {
    assert_eq!(
        probe("draw=_=>{createCanvas(400,400);a=createVector(1,2);b=a.copy();b.x=100;fill(a.x*30,0,0);rect(0,0,9,9)}"),
        30
    );
    // 代入しただけなら同じ実体を指す。
    assert_eq!(
        probe("draw=_=>{createCanvas(400,400);a=createVector(1,2);b=a;b.x=6;fill(a.x*30,0,0);rect(0,0,9,9)}"),
        180
    );
}

/// Java Mode の `PVector`。
#[test]
fn java_vectors_work() {
    let v = |body: &str| probe(&format!("void draw(){{ {body} rect(0,0,9,9); }}"));

    assert_eq!(v("PVector p = new PVector(30, 40); fill(p.x, 0, 0);"), 30);
    assert_eq!(v("PVector p = new PVector(30, 40); fill(p.mag(), 0, 0);"), 50);
    assert_eq!(
        v("PVector a = new PVector(10, 20); a.add(new PVector(5, 0)); fill(a.x * 10, 0, 0);"),
        150
    );
    assert_eq!(v("PVector p = new PVector(1, 2); p.x += 5; fill(p.x * 20, 0, 0);"), 120);
    // 宣言だけなら 0 ベクトル。
    assert_eq!(v("PVector p; fill(p.mag() + 42, 0, 0);"), 42);
    // 関数の引数と戻り値。
    assert_eq!(
        probe("float len(PVector v) { return v.mag(); }\nvoid draw(){ fill(len(new PVector(30,40)), 0, 0); rect(0,0,9,9); }"),
        50
    );
}

/// `PVector` の配列。要素は 1 本ずつ別の実体でなければならない。
#[test]
fn a_vector_array_holds_separate_vectors() {
    // 1 つ動かしても他は動かない。使い回していると全部動いてしまう。
    assert_eq!(
        probe("PVector[] p = new PVector[3];\nvoid draw(){ p[0].x = 9; fill(p[1].x * 10 + 20, 0, 0); rect(0,0,9,9); }"),
        20
    );
    // 添字の要素へメソッドを呼べる。
    assert_eq!(
        probe("PVector[] p = new PVector[2];\nvoid draw(){ p[0].add(new PVector(3,4)); fill(p[0].mag() * 10, 0, 0); rect(0,0,9,9); }"),
        50
    );
    // 要素そのものを差し替えられる。
    assert_eq!(
        probe("PVector[] p = new PVector[2];\nvoid draw(){ p[1] = new PVector(6,8); fill(p[1].mag() * 10, 0, 0); rect(0,0,9,9); }"),
        100
    );
}


// ---- 6 周目: 文字列と text() ---------------------------------------------

/// 文字列の値。
#[test]
fn strings_behave_like_processing() {
    let v = |body: &str| probe(&format!("draw=_=>{{createCanvas(400,400);{body};rect(0,0,9,9)}}"));

    // 長さは文字数。バイト数ではない。
    assert_eq!(v("fill('あいう'.length*80,0,0)"), 240);
    // `+` は連結。数を混ぜてもよい。
    assert_eq!(v("s='n='+3+'!';fill(s.length*60,0,0)"), 240);
    // 数として読める文字列は数になる。
    assert_eq!(v("fill('12'*10,0,0)"), 120);
    // 空文字列は偽。
    assert_eq!(v("fill(''?30:200,0,0)"), 200);
    // 比較は中身で見る。
    assert_eq!(v("fill('a'=='a'?200:0,0,0)"), 200);
    assert_eq!(v("fill('a'<'b'?200:0,0,0)"), 200);
}

/// Java Mode の `String`。
#[test]
fn java_strings_work() {
    let v = |body: &str| probe(&format!("void draw(){{ {body} rect(0,0,9,9); }}"));

    assert_eq!(v("String s = \"あい\"; fill(s.length * 100, 0, 0);"), 200);
    assert_eq!(v("String s = \"n=\" + 3; fill(s.length * 60, 0, 0);"), 180);
    // 配列も持てる。
    assert_eq!(
        probe("String[] m = {\"a\", \"bb\"};\nvoid draw(){ fill(m[1].length * 100, 0, 0); rect(0,0,9,9); }"),
        200
    );
    // `int()` は型名と同じ綴りだが関数として呼べる。
    assert_eq!(v("fill(int(7.9) * 20, 0, 0);"), 140);
}

/// フォントが無いときは何も描かない。落ちてはいけない。
#[test]
fn text_without_a_font_draws_nothing() {
    let mut s = VmSketch::compile("draw=_=>{createCanvas(400,400);textSize(40);text('あ',10,50)}", 1)
        .expect("コンパイルできる");
    let mut g = Graphics::new();
    g.begin_frame(400.0, 400.0);
    s.setup(&mut g);
    g.begin_frame(400.0, 400.0);
    s.draw(&mut g);
    assert!(s.error().is_none(), "{:?}", s.error());
    assert!(g.draw_list().indices.is_empty());
}

/// `nf()` は桁を 0 で埋める。
#[test]
fn nf_pads_with_zeros() {
    let v = |body: &str| probe(&format!("draw=_=>{{createCanvas(400,400);{body};rect(0,0,9,9)}}"));
    assert_eq!(v("fill(nf(5,3,0).length*60,0,0)"), 180);
    assert_eq!(v("fill(nf(1.5,3,2).length*40,0,0)"), 240);
    // str() は文字にするだけ。
    assert_eq!(v("fill(str(12).length*100,0,0)"), 200);
}


// ---- 7 周目: テンプレート・多次元配列・クラス ------------------------------

/// `` `a${b}c` `` の展開。
#[test]
fn template_literals_interpolate() {
    let v = |body: &str| probe(&format!("draw=_=>{{createCanvas(400,400);{body};rect(0,0,9,9)}}"));

    // 式が無ければただの文字列。
    assert_eq!(v("fill(`abc`.length*60,0,0)"), 180);
    // 数を挟むと文字列になる。
    assert_eq!(v("s=`n=${1+2}!`;fill(s.length*40,0,0)"), 160);
    // 先頭が式でも文字列に寄る。
    assert_eq!(v("s=`${1}${2}${3}`;fill(s.length*60,0,0)"), 180);
    // 中に `{}` があっても閉じ位置を間違えない。
    assert_eq!(v("f=x=>{return x*2};s=`${f(3)}`;fill(s.length*200,0,0)"), 200);
}

/// 2 次元配列。
#[test]
fn two_dimensional_arrays_work() {
    let v = |body: &str| probe(&format!("void draw(){{ {body} rect(0,0,9,9); }}"));

    assert_eq!(v("float[][] a = new float[3][4]; fill(a.length*30 + a[0].length*20, 0, 0);"), 170);
    assert_eq!(v("float[][] a = new float[2][2]; a[1][0] = 7; fill(a[1][0]*20, 0, 0);"), 140);
    // 行ごとに別の配列。使い回していると 1 行の書き換えが全部へ及ぶ。
    assert_eq!(v("float[][] a = new float[3][3]; a[0][0] = 9; fill(a[1][0]*10 + 40, 0, 0);"), 40);
    assert_eq!(
        v("float[][] a = new float[4][4]; for(int y=0;y<4;y++) for(int x=0;x<4;x++) a[y][x] = y*4+x; fill(a[3][3]*10, 0, 0);"),
        150
    );
}

/// クラス。
#[test]
fn classes_hold_state_and_behaviour() {
    let v = |src: &str| probe(src);

    // 生成とフィールドの読み。
    assert_eq!(
        v("class P{float x;P(float a){x=a;}}\nvoid draw(){P p=new P(7);fill(p.x*20,0,0);rect(0,0,9,9);}"),
        140
    );
    // メソッドが自分の状態を書き換える。
    assert_eq!(
        v("class P{float x;P(float a){x=a;}void step(){x+=3;}}\nvoid draw(){P p=new P(1);p.step();p.step();fill(p.x*20,0,0);rect(0,0,9,9);}"),
        140
    );
    // 戻り値のあるメソッド。
    assert_eq!(
        v("class P{float x;P(float a){x=a;}float twice(){return x*2;}}\nvoid draw(){P p=new P(5);fill(p.twice()*20,0,0);rect(0,0,9,9);}"),
        200
    );
    // `this` で明示しても同じ。
    assert_eq!(
        v("class P{float x;P(float a){this.x=a;}}\nvoid draw(){P p=new P(6);fill(p.x*30,0,0);rect(0,0,9,9);}"),
        180
    );
    // コンストラクタを書かなくても作れる。フィールドは既定値。
    assert_eq!(v("class P{float x;}\nvoid draw(){P p=new P();fill(p.x+80,0,0);rect(0,0,9,9);}"), 80);
    // 生成してすぐメソッドを呼べる。
    assert_eq!(
        v("class P{float x;P(float a){x=a;}void show(){fill(x*10,0,0);rect(0,0,9,9);}}\nvoid draw(){new P(9).show();}"),
        90
    );
}

/// 実体どうしは独立している。
#[test]
fn instances_do_not_share_state() {
    assert_eq!(
        probe("class P{float x;P(float a){x=a;}}\nvoid draw(){P a=new P(1);P b=new P(9);fill(a.x*30,0,0);rect(0,0,9,9);}"),
        30
    );
    // 配列に入れても同じ。
    assert_eq!(
        probe("class P{float x;P(float a){x=a;}}\nvoid draw(){P[] ps=new P[2];ps[0]=new P(4);ps[1]=new P(1);fill(ps[0].x*50,0,0);rect(0,0,9,9);}"),
        200
    );
    // 拡張 for で回せる。
    assert_eq!(
        probe("class P{float x;P(float a){x=a;}}\nvoid draw(){P[] ps=new P[2];ps[0]=new P(1);ps[1]=new P(2);float s=0;for(P p:ps)s+=p.x;fill(s*60,0,0);rect(0,0,9,9);}"),
        180
    );
}

/// 定義していないクラスは使えない。動く前に止める。
#[test]
fn an_unknown_class_is_refused() {
    assert!(
        VmSketch::compile("void draw(){ Thing t = new Thing(); }", 1).is_err(),
        "知らないクラスが通ってしまいました"
    );
    // 定義があれば通る。
    assert!(
        VmSketch::compile("class Thing{}\nvoid draw(){ Thing t = new Thing(); }", 1).is_ok()
    );
}


// ---- 8 周目: 静的モードと複数宣言 ------------------------------------------

/// `setup()` も `draw()` も書かない書き方 (静的モード)。
///
/// Processing は関数の外に直接書いた文を `setup()` の中身として扱い、1 回だけ
/// 描く。つぶやきの短いコードはこの形が多い。
#[test]
fn a_sketch_without_setup_or_draw_runs_once() {
    // 静的モードは setup で描くので、setup を走らせた直後の枚数を数える。
    let in_setup = |src: &str| -> usize {
        let mut s = VmSketch::compile(src, 1).expect("コンパイルできる");
        let mut g = Graphics::new();
        g.begin_frame(400.0, 400.0);
        s.setup(&mut g);
        assert!(s.error().is_none(), "{:?} / {src}", s.error());
        g.draw_list().indices.len()
    };
    let one = in_setup("circle(0,0,10);");
    assert!(one > 0, "静的モードで何も描けていない");

    assert_eq!(in_setup("size(400,400);\ncircle(0,0,10);"), one);
    // ループも書ける。
    assert_eq!(in_setup("for(int i=0;i<5;i++)circle(0,0,10);") / one, 5);
    // 変数も宣言できる。
    assert_eq!(
        in_setup("float r = 3;\nfor(int i=0;i<3;i++){circle(0,0,10);r+=1;}") / one,
        3
    );
    // ユーザーが setup を書いていれば、そちらが優先される。
    assert_eq!(
        in_setup("void setup(){ circle(0,0,10); circle(0,0,10); }\nvoid draw(){}") / one,
        2
    );
}

/// `float r, i, d;` のように 1 文で複数を宣言できる。
#[test]
fn several_variables_can_share_one_declaration() {
    let v = |body: &str| probe(&format!("void draw(){{ {body} rect(0,0,9,9); }}"));

    assert_eq!(v("float a, b; a = 3; b = 4; fill(a*b*10, 0, 0);"), 120);
    // 初期値を混ぜられる。
    assert_eq!(v("float a = 2, b, c = 5; b = 3; fill(a*b*c*4, 0, 0);"), 120);
    // グローバルでも書ける。
    assert_eq!(
        probe("float a = 2, b = 3;\nvoid draw(){ fill(a*b*20, 0, 0); rect(0,0,9,9); }"),
        120
    );
    // 配列型も並べられる。
    assert_eq!(
        probe("int[] a = {1,2}, b = {3};\nvoid draw(){ fill(a[1]*b[0]*20, 0, 0); rect(0,0,9,9); }"),
        120
    );
}

/// `TAU` と `resetMatrix()`。
#[test]
fn tau_and_reset_matrix_exist() {
    let v = |body: &str| probe(&format!("void draw(){{ {body} rect(0,0,9,9); }}"));
    // TAU は TWO_PI と同じ。
    assert_eq!(v("fill(TAU*40, 0, 0);"), 251);
    // resetMatrix() は積んだ変換を捨てる。
    let b = bounds("void draw(){ noStroke(); translate(100,100); resetMatrix(); rect(0,0,20,20); }");
    assert!(b.0 < 0.01 && b.1 < 0.01, "変換が残っています: {b:?}");
}


// ---- 9 周目: 式としての代入・clear()・詰めた色 -----------------------------

/// 引数の中でも代入できる。`line(x, y, x += dx, y += dy)` の形。
#[test]
fn assignment_works_inside_an_expression() {
    let v = |body: &str| probe(&format!("void draw(){{ {body} rect(0,0,9,9); }}"));

    // 代入した値がそのまま式の値になる。
    assert_eq!(v("float x = 1; fill((x += 2) * 40, 0, 0);"), 120);
    // 書き込みも起きている。
    assert_eq!(v("float x = 1; x += 2; fill(x * 40, 0, 0);"), 120);
    // 単純代入。
    assert_eq!(v("float x; fill((x = 5) * 30, 0, 0);"), 150);
    // 引数の途中で書き換えても、あとの引数は新しい値を見る。
    assert_eq!(v("float x = 1; fill((x += 1) * 20, (x += 1) * 20, 0);"), 40);
    // 配列の要素へも書ける。
    assert_eq!(v("float[] a = new float[2]; fill((a[0] += 3) * 40, 0, 0);"), 120);

    // 実際の使い方。線を引きながら位置を進める。
    let mut s = VmSketch::compile(
        "float x,y;\nvoid draw(){ for(int i=0;i<4;i++) line(x,y,x+=10,y+=10); }",
        1,
    )
    .expect("コンパイルできる");
    let mut g = Graphics::new();
    g.begin_frame(400.0, 400.0);
    s.setup(&mut g);
    g.begin_frame(400.0, 400.0);
    s.draw(&mut g);
    assert!(s.error().is_none(), "{:?}", s.error());
    // 線 1 本が四角 1 つ。4 本引く。
    assert_eq!(g.draw_list().indices.len(), 4 * 6);
}

/// `clear()` は積んだ絵を捨てる。
#[test]
fn clear_wipes_the_canvas() {
    let mut s = VmSketch::compile("void draw(){ circle(0,0,10); clear(); }", 1)
        .expect("コンパイルできる");
    let mut g = Graphics::new();
    g.begin_frame(400.0, 400.0);
    s.setup(&mut g);
    g.begin_frame(400.0, 400.0);
    s.draw(&mut g);
    assert!(s.error().is_none(), "{:?}", s.error());
    assert!(g.draw_list().indices.is_empty(), "clear() で消えていません");
    // 透明ではなく黒で塗る。透明だとサムネイルが透けてしまう。
    assert_eq!(g.draw_list().clear, Some(tsubu_renderer::Color::BLACK));
}

/// `stroke(-1)` のように int をひとつ渡す書き方は、詰めた色 (0xAARRGGBB)。
#[test]
fn a_single_negative_int_is_a_packed_colour() {
    let v = |body: &str| probe(&format!("void draw(){{ {body} rect(0,0,9,9); }}"));

    // -1 は 0xFFFFFFFF、つまり不透明な白。
    assert_eq!(v("fill(-1);"), 255);
    // 0..255 はこれまでどおり明度。
    assert_eq!(v("fill(128);"), 128);
    assert_eq!(v("fill(0);"), 0);
    // 詰めた色から赤を取り出せる。0xFFFF6B35 の R は 255。
    assert_eq!(v("fill(0xFFFF6B35);"), 255);

    // p5 の数値は 1 種類なので、この読み替えは起きない。
    assert_eq!(
        probe("draw=_=>{createCanvas(400,400);fill(128);rect(0,0,9,9)}"),
        128
    );
}


// ---- 10 周目: 前置増減・配列初期化子・色の成分 ------------------------------

/// `++t` は増やしたあとの値、`t++` は増やす前の値。
#[test]
fn prefix_and_postfix_increment_differ() {
    let v = |body: &str| probe(&format!("void draw(){{ {body} rect(0,0,9,9); }}"));

    assert_eq!(v("int t = 1; fill(++t * 50, 0, 0);"), 100);
    assert_eq!(v("int t = 1; fill(t++ * 50, 0, 0);"), 50);
    // どちらも書き込みは起きる。
    assert_eq!(v("int t = 1; t++; fill(t * 50, 0, 0);"), 100);
    assert_eq!(v("int t = 3; fill(--t * 50, 0, 0);"), 100);
    // 添字の中でも使える。
    assert_eq!(
        v("int[] a = new int[4]; int t = 0; a[++t] = 3; fill(a[1] * 50, 0, 0);"),
        150
    );
}

/// `new int[]{1, 2}` は中身の並びで大きさが決まる。
#[test]
fn an_array_can_be_created_from_a_list() {
    let v = |body: &str| probe(&format!("void draw(){{ {body} rect(0,0,9,9); }}"));

    assert_eq!(v("int[] a = new int[]{3, 4}; fill(a[0] * a[1] * 10, 0, 0);"), 120);
    // 既存の書き方も残る。
    assert_eq!(v("int[] a = {3, 4}; fill(a[0] * a[1] * 10, 0, 0);"), 120);
    // 2 次元配列の行として入れられる。
    assert_eq!(
        v("int[][] p = new int[2][2]; p[1] = new int[]{6, 0}; fill(p[1][0] * 20, 0, 0);"),
        120
    );
}

/// 色の成分を取り出す。
#[test]
fn colour_components_can_be_read() {
    let v = |body: &str| probe(&format!("void draw(){{ {body} rect(0,0,9,9); }}"));

    assert_eq!(v("fill(red(color(200, 100, 50)), 0, 0);"), 200);
    assert_eq!(v("fill(green(color(200, 100, 50)), 0, 0);"), 100);
    assert_eq!(v("fill(blue(color(200, 100, 50)), 0, 0);"), 50);
    assert_eq!(v("fill(alpha(color(200, 100, 50)), 0, 0);"), 255);
    // 明度は最大の成分。既定では 0..255 で返る。
    assert_eq!(v("fill(brightness(color(200, 100, 50)), 0, 0);"), 200);
    assert_eq!(v("fill(brightness(color(0, 0, 0)) + 30, 0, 0);"), 30);
    // 彩度。灰色は 0。
    assert_eq!(v("fill(saturation(color(100, 100, 100)) + 40, 0, 0);"), 40);
    assert_eq!(v("fill(saturation(color(255, 0, 0)), 0, 0);"), 255);
    // 詰めた色からも読める。
    assert_eq!(v("fill(red(0xFFC86432), 0, 0);"), 200);
}

/// `randomGaussian()` は平均 0 のあたりに散る。
#[test]
fn random_gaussian_is_centred() {
    let mut s = VmSketch::compile(
        "float sum, n;\nvoid draw(){ for(int i=0;i<400;i++){ sum += randomGaussian(); n++; } }",
        1,
    )
    .expect("コンパイルできる");
    let mut g = Graphics::new();
    g.begin_frame(400.0, 400.0);
    s.setup(&mut g);
    g.begin_frame(400.0, 400.0);
    s.draw(&mut g);
    assert!(s.error().is_none(), "{:?}", s.error());

    // 400 個の平均。0 のまわりに集まる。
    let sum = g.draw_list();
    let _ = sum;
    // 値そのものは色に載せて確かめる。
    let mean = probe(
        "float sum;\nvoid draw(){ for(int i=0;i<400;i++) sum += randomGaussian(); fill(abs(sum/400)*255, 0, 0); rect(0,0,9,9); }",
    );
    assert!(mean < 60, "平均が 0 から離れすぎています: {mean}");
}


// ---- 11 周目: 角丸・String・drawingContext ---------------------------------

/// `rect()` は 5 個目以降の引数で角が丸くなる。
#[test]
fn rectangles_can_have_rounded_corners() {
    let square = triangles("draw=_=>{createCanvas(400,400);noStroke();rect(0,0,40,40)}");
    let round = triangles("draw=_=>{createCanvas(400,400);noStroke();rect(0,0,40,40,8)}");
    assert_eq!(square, 2, "角のない四角は三角形 2 つ");
    assert!(round > square, "角丸のほうが分割が多いはず: {round}");

    // 4 隅それぞれ指定できる。
    assert!(triangles("draw=_=>{createCanvas(400,400);noStroke();rect(0,0,40,40,2,4,6,8)}") > 2);
    // 半径 0 なら元の四角に戻る。
    assert_eq!(triangles("draw=_=>{createCanvas(400,400);noStroke();rect(0,0,40,40,0)}"), 2);

    // 枠からはみ出さない。
    let b = bounds("draw=_=>{createCanvas(400,400);noStroke();rect(10,10,40,40,8)}");
    assert!(b.0 >= 9.99 && b.2 <= 50.01, "はみ出しています: {b:?}");
}

/// `String.fromCodePoint()` は番号から 1 文字を作る。
#[test]
fn string_from_code_point_builds_a_character() {
    let v = |body: &str| probe(&format!("draw=_=>{{createCanvas(400,400);{body};rect(0,0,9,9)}}"));
    assert_eq!(v("fill(String.fromCodePoint(65).length*200,0,0)"), 200);
    // 麻雀牌のような範囲外の文字でも 1 文字。
    assert_eq!(v("s=String.fromCodePoint(126976);fill(s.length*200,0,0)"), 200);
    assert_eq!(v("fill((String.fromCodePoint(65)=='A')?200:0,0,0)"), 200);
}

/// `drawingContext` はブラウザのものなので効かないが、書いても止まらない。
#[test]
fn drawing_context_accepts_writes_without_failing() {
    let mut s = VmSketch::compile(
        "draw=_=>{createCanvas(400,400);D=drawingContext;D.shadowBlur=25;D.shadowColor=color(0);circle(1,1,1)}",
        1,
    )
    .expect("コンパイルできる");
    let mut g = Graphics::new();
    g.begin_frame(400.0, 400.0);
    s.setup(&mut g);
    g.begin_frame(400.0, 400.0);
    s.draw(&mut g);
    assert!(s.error().is_none(), "{:?}", s.error());
    assert!(!g.draw_list().indices.is_empty(), "円が描かれていません");
}


/// 見つからない変数は、場所を添えて知らせる。
///
/// 位置が `0行0列` では直しようがない。
#[test]
fn an_unknown_variable_is_reported_with_its_place() {
    for (src, line, column) in [
        ("void draw(){ circle(nosuch, 1, 1); }", 1, 21),
        ("void setup(){ size(720, 720, NOPE); }\nvoid draw(){}", 1, 30),
        ("void draw(){ float a = 1; a += missing; }", 1, 32),
    ] {
        match VmSketch::compile(src, 1) {
            Ok(_) => panic!("{src} は弾かれるはず"),
            Err(e) => {
                assert_eq!((e.line, e.column), (line, column), "{src} → {e}");
                assert!(e.to_string().contains("見つかりません"), "{e}");
            }
        }
    }
}


// ---- 3D (設計書 §14.2) --------------------------------------------------

/// `size(w, h, P3D)` の作品が、遠近のついた立体を描く。
#[test]
fn a_box_is_drawn_in_perspective() {
    // 6 面 × 2 枚の塗りと、12 本の稜線 × 2 枚。
    let filled = "void setup(){size(400,400,P3D);}\n\
                  void draw(){noStroke();translate(200,200,0);box(50);}";
    assert_eq!(triangles(filled), 12, "面が 6 枚ぶんない");

    // 手前の面は奥の面より大きく写る。それが遠近。
    let (x0, _, x1, _) = bounds(filled);
    let near = "void setup(){size(400,400,P3D);}\n\
                void draw(){noStroke();translate(200,200,120);box(50);}";
    let (nx0, _, nx1, _) = bounds(near);
    assert!(nx1 - nx0 > (x1 - x0) * 1.3, "近づけても大きくなりません: {} → {}", x1 - x0, nx1 - nx0);
}

/// `z = 0` の平面は 2D と同じ座標で写る。
///
/// P3D と書いただけの作品が、2D のつもりで置いた図形をそのままの場所に
/// 出せるということ。
#[test]
fn the_flat_plane_lands_where_2d_would_put_it() {
    let flat = bounds("void setup(){size(400,400);}\nvoid draw(){noStroke();rect(100,120,40,20);}");
    let deep =
        bounds("void setup(){size(400,400,P3D);}\nvoid draw(){noStroke();rect(100,120,40,20);}");
    for (a, b) in [(flat.0, deep.0), (flat.1, deep.1), (flat.2, deep.2), (flat.3, deep.3)] {
        assert!((a - b).abs() < 0.5, "2D と 3D で場所が違います: {flat:?} と {deep:?}");
    }
}

/// `resetMatrix()` は 3D ではカメラごと消える。
///
/// つぶやき系の作品が原点まわりに立体を並べるときの定石。これが効かないと
/// ぜんぶ画面の左上へ寄ってしまう。
#[test]
fn resetting_the_matrix_moves_the_eye_to_the_origin() {
    let (x0, y0, x1, y1) = bounds(
        "void setup(){size(400,400,P3D);}\n\
         void draw(){noStroke();resetMatrix();translate(0,0,-200);box(40);}",
    );
    let (cx, cy) = ((x0 + x1) * 0.5, (y0 + y1) * 0.5);
    assert!((cx - 200.0).abs() < 1.0 && (cy - 200.0).abs() < 1.0, "中央に来ません: {cx}, {cy}");
}

/// p5.js の `WEBGL` は原点が画面の中央。
#[test]
fn webgl_starts_from_the_centre() {
    let (x0, y0, x1, y1) =
        bounds("function setup(){createCanvas(400,400,WEBGL);}\n\
                function draw(){noStroke();box(40);}");
    let (cx, cy) = ((x0 + x1) * 0.5, (y0 + y1) * 0.5);
    assert!((cx - 200.0).abs() < 1.0 && (cy - 200.0).abs() < 1.0, "中央に来ません: {cx}, {cy}");
}

/// `box(0)` は何も描かない。
///
/// つぶやき系は `box(cond ? 6 : 0)` で立方体を間引く。0 で 1 枚でも
/// 描いてしまうと、画面が立方体で埋まる。
#[test]
fn a_box_of_zero_size_draws_nothing() {
    assert_eq!(
        triangles("void setup(){size(400,400,P3D);}\nvoid draw(){translate(200,200,0);box(0);}"),
        0
    );
}

/// 3 次元のノイズは、奥行きの向きにも滑らかにつながる。
///
/// 2D のノイズを z でずらすやり方だと、切る位置ごとに別の模様になり、
/// 立体の中身が砂嵐になる。
#[test]
fn noise_is_continuous_along_the_third_axis() {
    let at = |z: f32| {
        let src = format!(
            "float v;\nvoid setup(){{size(400,400);}}\n\
             void draw(){{ v = noise(1.5, 2.5, {z}); noStroke(); rect(0, 0, v * 100, 10); }}"
        );
        let (_, _, x1, _) = bounds(&src);
        x1
    };
    let base = at(3.0);
    // すぐ隣ならほとんど変わらない。
    assert!((at(3.01) - base).abs() < 2.0, "隣が飛んでいます: {base} と {}", at(3.01));
    // 遠ければ別の値になる。ずっと同じでは 3D になっていない。
    let far: Vec<f32> = (1..8).map(|i| at(3.0 + i as f32)).collect();
    assert!(far.iter().any(|v| (v - base).abs() > 2.0), "z を変えても同じです: {base}, {far:?}");
}


// ---- 影と push()/pop() --------------------------------------------------

/// 最初に描かれた三角形の色。
fn first_color(src: &str) -> [f32; 4] {
    let mut s = VmSketch::compile(src, 1).expect("コンパイルできる");
    let mut g = Graphics::new();
    g.begin_frame(400.0, 400.0);
    s.setup(&mut g);
    g.begin_frame(400.0, 400.0);
    g.frame_count = 1;
    s.draw(&mut g);
    assert!(s.error().is_none(), "{:?} / {src}", s.error());
    g.draw_list().vertices.first().expect("何か描かれている").color
}

const PLAIN_RECT: &str = "draw=_=>{createCanvas(400,400);noStroke();rect(150,150,100,100)}";

/// `drawingContext.shadowBlur` が形の外へ広がる。
///
/// 受け取るだけで捨てていたので、影だけで成り立っている作品が真っ白に
/// なっていた。
#[test]
fn a_shadow_spreads_around_the_shape() {
    let plain = bounds(PLAIN_RECT);
    let lit = bounds(
        "draw=_=>{createCanvas(400,400);noStroke();\
         drawingContext.shadowBlur=20;drawingContext.shadowColor=color(0);\
         rect(150,150,100,100)}",
    );
    assert!(lit.0 < plain.0 - 5.0, "左へ広がっていません: {lit:?} と {plain:?}");
    assert!(lit.2 > plain.2 + 5.0, "右へ広がっていません: {lit:?} と {plain:?}");
    // 影は形の下に敷く。最初に出る色は黒に近いはず。
    let c = first_color(
        "draw=_=>{createCanvas(400,400);noStroke();fill(255,0,0);\
         drawingContext.shadowBlur=20;drawingContext.shadowColor=color(0);\
         rect(150,150,100,100)}",
    );
    assert!(c[0] < 0.1 && c[1] < 0.1 && c[3] > 0.0, "影が黒く敷かれていません: {c:?}");
}

/// `shadowOffsetX` / `shadowOffsetY` でずれる。
#[test]
fn a_shadow_can_be_offset() {
    let at = bounds(
        "draw=_=>{createCanvas(400,400);noStroke();\
         drawingContext.shadowColor=color(0);drawingContext.shadowOffsetX=30;\
         rect(150,150,100,100)}",
    );
    let plain = bounds(PLAIN_RECT);
    assert!((at.2 - plain.2 - 30.0).abs() < 1.0, "右へずれていません: {at:?}");
}

/// p5.js の `pop()` は見た目まで戻す。
///
/// canvas の `save()` / `restore()` と同じ扱いなので、影の指定も戻る。
/// Processing の `popMatrix()` は座標変換だけで、こちらは戻さない。
#[test]
fn the_p5_pop_puts_back_the_style_and_the_shadow() {
    // push() の中で付けた影は pop() で消える。
    let after = bounds(
        "draw=_=>{createCanvas(400,400);noStroke();push();\
         drawingContext.shadowBlur=20;drawingContext.shadowColor=color(0);pop();\
         rect(150,150,100,100)}",
    );
    let plain = bounds(PLAIN_RECT);
    assert!((after.0 - plain.0).abs() < 0.5, "影が残っています: {after:?} と {plain:?}");

    // 塗りの色も戻る。
    let popped = first_color(
        "draw=_=>{createCanvas(400,400);noStroke();fill(255,255,255);\
         push();fill(255,0,0);pop();rect(150,150,100,100)}",
    );
    assert!(popped[1] > 0.9, "p5 の pop() が塗りを戻していません: {popped:?}");

    // Processing の popMatrix() は色を戻さない。
    let kept = first_color(
        "void setup(){size(400,400);}\n\
         void draw(){noStroke();fill(255,255,255);\
         pushMatrix();fill(255,0,0);popMatrix();rect(150,150,100,100);}",
    );
    assert!(kept[1] < 0.1, "popMatrix() が色まで戻しています: {kept:?}");
}


/// フォントを積んだうえで、最初に描かれた三角形の色を返す。
///
/// フォントの無い環境では `None`。
fn first_text_color(src: &str) -> Option<[f32; 4]> {
    const CANDIDATES: &[&str] = &[
        "/System/Library/Fonts/ヒラギノ角ゴシック W3.ttc",
        "C:/Windows/Fonts/meiryo.ttc",
        "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
    ];
    let font = CANDIDATES.iter().find_map(|p| std::fs::read(p).ok())?;

    let mut s = VmSketch::compile(src, 1).expect("コンパイルできる");
    let mut g = Graphics::new();
    g.font.set_fonts(vec![font]);
    g.begin_frame(400.0, 400.0);
    s.setup(&mut g);
    g.begin_frame(400.0, 400.0);
    g.frame_count = 1;
    s.draw(&mut g);
    assert!(s.error().is_none(), "{:?} / {src}", s.error());
    Some(g.draw_list().vertices.first().expect("字が描かれている").color)
}

/// p5.js の `text()` は塗りと線の両方で描く。
///
/// 白いカードに白い字を置く作品は、線が付かないと何も見えない。
/// Processing の `text()` は塗りだけなので、そちらは付けない。
#[test]
fn p5_text_is_stroked_but_processing_text_is_not() {
    let Some(p5) = first_text_color(
        "draw=_=>{createCanvas(400,400);textSize(60);fill(255,255,255);stroke(255,0,0);text('あ',50,200)}",
    ) else {
        eprintln!("フォントが無いので飛ばします");
        return;
    };
    assert!(p5[0] > 0.9 && p5[1] < 0.1, "p5 の字に線が付いていません: {p5:?}");

    let java = first_text_color(
        "void setup(){size(400,400);}\n\
         void draw(){textSize(60);fill(255,255,255);stroke(255,0,0);text(\"あ\",50,200);}",
    )
    .expect("フォントはある");
    assert!(java[1] > 0.9, "Processing の字に線が付いています: {java:?}");

    // 線を消せば p5 でも塗りだけになる。
    let bare = first_text_color(
        "draw=_=>{createCanvas(400,400);textSize(60);fill(255,255,255);noStroke();text('あ',50,200)}",
    )
    .expect("フォントはある");
    assert!(bare[1] > 0.9, "noStroke() が効いていません: {bare:?}");
}


/// `f(...xs)`。引数の並びに展開を書ける。
///
/// 配列リテラルの展開だけを見ていたので、`stroke(...B, 9)` のような
/// 書き方が通らなかった。引数の個数が実行時まで決まらないので、
/// ひとつの配列にまとめてから渡す。
#[test]
fn a_call_can_spread_its_arguments() {
    // 塗りつぶした四角は三角形 2 つ。展開して渡しても同じ。
    for src in [
        "draw=_=>{createCanvas(400,400);noStroke();let B=[10,20,30,40];rect(...B)}",
        "draw=_=>{createCanvas(400,400);noStroke();let B=[20,30,40];rect(10, ...B)}",
        "draw=_=>{createCanvas(400,400);noStroke();let B=[10,20];rect(...B, 30, 40)}",
        "draw=_=>{createCanvas(400,400);noStroke();let B=[20,30];rect(10, ...B, 40)}",
    ] {
        assert_eq!(triangles(src), 2, "{src}");
    }

    // 場所も引数を並べて書いたときと同じ。
    let spread = bounds("draw=_=>{createCanvas(400,400);noStroke();let B=[10,20,30,40];rect(...B)}");
    let plain = bounds("draw=_=>{createCanvas(400,400);noStroke();rect(10,20,30,40)}");
    assert_eq!(spread, plain);
}

/// 展開はユーザー定義の関数、メソッド、`Math.*` でも使える。
#[test]
fn spreading_works_for_every_kind_of_call() {
    assert_eq!(
        triangles(
            "f=(a,b,c,d)=>rect(a,b,c,d);\n\
             draw=_=>{createCanvas(400,400);noStroke();let B=[10,20,30,40];f(...B)}"
        ),
        2,
        "ユーザー定義の関数"
    );
    assert_eq!(
        triangles(
            "draw=_=>{createCanvas(400,400);noStroke();\
             let a=[10,20];let b=[30,40];a.push(...b);rect(a[0],a[1],a[2],a[3])}"
        ),
        2,
        "配列のメソッド"
    );
    // Math.max(...xs) は組み込みへ読み替える。`Math` は実体を持たない。
    let (_, _, x1, _) = bounds(
        "draw=_=>{createCanvas(400,400);noStroke();let B=[3,40,12];rect(0,0,Math.max(...B),10)}",
    );
    assert!((x1 - 40.0).abs() < 0.01, "Math.max の展開が効いていません: {x1}");
}

/// `...` を引数の並び以外へ書いたら弾く。
#[test]
fn a_stray_spread_is_refused() {
    assert!(VmSketch::compile("draw=_=>{let a=...[1,2]}", 1).is_err());
}


/// 配列のメソッド。`push` / `map` の類だけでは足りない。
///
/// つぶやき系は配列を待ち行列のように使う。`shift()` が無いだけで
/// 作品が止まる。
#[test]
fn arrays_have_the_methods_javascript_gives_them() {
    /// 式の値を `rect` の幅で測る。
    fn value_of(expr: &str) -> f32 {
        let (_, _, x1, _) =
            bounds(&format!("draw=_=>{{createCanvas(400,400);noStroke();rect(0,0,({expr}),10)}}"));
        x1
    }

    for (expr, want) in [
        // 端から出し入れする。
        ("a=[7,8,9], a.shift(), a[0]*10+a.length", 82.0),
        ("a=[7,8,9], a.pop()*10+a.length", 92.0),
        ("a=[8], a.unshift(7), a[0]*10+a.length", 72.0),
        ("a=[7,8,9], a.at(-1)", 9.0),
        // 切り出す。splice は取り除いたぶんを返す。
        ("a=[1,2,3,4], a.slice(1,3).join(\"\")", 23.0),
        ("a=[1,2,3,4], b=a.splice(1,2), b[0]*10+a.length", 22.0),
        ("[1,2].concat([3],4).length", 4.0),
        ("[[1,2],[3]].flat().length", 3.0),
        // 並べ替えと探索。
        ("[1,2,3].reverse()[0]", 3.0),
        ("[0,0,0].fill(5)[2]", 5.0),
        ("[7,8,9].indexOf(9)", 2.0),
        ("[7,8].includes(8) ? 1 : 0", 1.0),
        ("[7,8].includes(3) ? 1 : 0", 0.0),
        ("[7,8,9].lastIndexOf(3)+10", 9.0),
        // 比べ方を渡さないと文字として並ぶ。JavaScript と同じ。
        ("[3,20,1].sort()[0]", 1.0),
        ("[3,20,1].sort()[1]", 20.0),
        ("[3,20,1].sort((a,b)=>a-b)[2]", 20.0),
        // たたみ込みと述語。
        ("[1,2,3].reduce((a,b)=>a+b,0)", 6.0),
        ("[4,5].reduce((a,b)=>a+b)", 9.0),
        ("[1,8,9].find(v=>v>5)", 8.0),
        ("[1,8,9].findLast(v=>v>5)", 9.0),
        ("[1,8,9].findIndex(v=>v>5)", 1.0),
        ("[1,8].findIndex(v=>v>90)+10", 9.0),
        ("[1,2].some(v=>v>1) ? 1 : 0", 1.0),
        ("[1,2].every(v=>v>0) ? 1 : 0", 1.0),
        ("[1,2].every(v=>v>1) ? 1 : 0", 0.0),
        ("[1,2].flatMap(v=>[v,v]).length", 4.0),
        ("[\"a\",\"b\"].join(\"-\").length", 3.0),
    ] {
        let got = value_of(expr);
        assert!((got - want).abs() < 0.01, "{expr} → {got} (期待 {want})");
    }
}

/// 空の配列にも安全に使える。
#[test]
fn the_array_methods_are_safe_on_an_empty_array() {
    for src in [
        "draw=_=>{let a=[];a.shift();a.pop();a.reverse();a.sort();circle(1,2,3)}",
        "draw=_=>{let a=[];a.slice(2,9);a.splice(1,5);a.fill(0);circle(1,2,3)}",
        "draw=_=>{let a=[];a.reduce((x,y)=>x+y);a.find(v=>v);circle(1,2,3)}",
    ] {
        assert!(triangles(src) > 0, "{src}");
    }
}


/// `point()` は丸。四角ではない。
///
/// Processing も p5.js も、点は線の端と同じ丸で描く。太い点をばらまく作品が
/// 角ばって見えていた。
#[test]
fn a_fat_point_is_round() {
    let src = "draw=_=>{createCanvas(400,400);strokeWeight(40);point(200,200)}";
    let (x0, y0, x1, y1) = bounds(src);
    // 太さぶんの直径に収まる。
    assert!((x1 - x0 - 40.0).abs() < 1.0, "大きさが違います: {:?}", (x0, y0, x1, y1));
    assert!((y1 - y0 - 40.0).abs() < 1.0, "大きさが違います: {:?}", (x0, y0, x1, y1));

    // 四角なら三角形 2 つ。丸は扇形に分かれるのでもっと多い。
    assert!(triangles(src) > 8, "四角のままです: {}", triangles(src));

    // 隅が空いている。四角ならここまで色が来る。
    let mut s = VmSketch::compile(src, 1).expect("コンパイルできる");
    let mut g = Graphics::new();
    g.begin_frame(400.0, 400.0);
    s.setup(&mut g);
    g.begin_frame(400.0, 400.0);
    g.frame_count = 1;
    s.draw(&mut g);
    let corner = g
        .draw_list()
        .vertices
        .iter()
        .any(|p| (p.pos[0] - 220.0).abs() < 0.5 && (p.pos[1] - 220.0).abs() < 0.5);
    assert!(!corner, "隅に頂点があります。まだ四角です");

    // 細い点は四角のままでよい。見た目が変わらないのに頂点だけ増える。
    assert_eq!(
        triangles("draw=_=>{createCanvas(400,400);strokeWeight(1);point(200,200)}"),
        2
    );
}


/// `background()` を呼ばない作品の下地は、方言で違う。
///
/// Processing のキャンバスは灰 204 で始まる。p5.js のキャンバスは透明で、
/// 後ろのページの白が透ける。半透明を塗り重ねる作品では、この下地が
/// そのまま画面全体の明るさになる。
#[test]
fn the_ground_under_a_sketch_depends_on_the_dialect() {
    let ground = |src: &str| {
        let mut s = VmSketch::compile(src, 1).expect("コンパイルできる");
        let mut g = Graphics::new();
        g.begin_frame(400.0, 400.0);
        s.setup(&mut g);
        g.default_background()
    };

    let p5 = ground("draw=_=>{createCanvas(400,400);circle(1,2,3)}");
    assert_eq!((p5.r, p5.g, p5.b), (1.0, 1.0, 1.0), "p5 の下地が白ではありません");

    let java = ground("void setup(){size(400,400);}\nvoid draw(){circle(1,2,3);}");
    assert!((java.r - 0.8).abs() < 0.01, "Processing の下地が灰 204 ではありません: {java:?}");
}


/// p5.js の `createCanvas()` は呼ぶたびにキャンバスを作り直す。
///
/// `draw()` の頭で毎フレーム呼んで画面を消す書き方がある。作り直さないと
/// 半透明を重ねる作品が積もり続け、数フレームで彩度が振り切れて
/// 別の絵になる。Processing の `size()` にこの働きは無い。
#[test]
fn calling_create_canvas_again_wipes_the_canvas() {
    let mut s = VmSketch::compile(
        "draw=_=>{createCanvas(400,400);noStroke();fill(255,0,0);rect(0,0,50,50)}",
        1,
    )
    .expect("コンパイルできる");
    let mut g = Graphics::new();
    g.begin_frame(400.0, 400.0);
    s.setup(&mut g);

    // 1 フレーム目に何か描いておく。
    g.begin_frame(400.0, 400.0);
    g.frame_count = 1;
    s.draw(&mut g);
    let first = g.draw_list().indices.len();
    assert!(first > 0);

    // 2 フレーム目。createCanvas() が消しに入るので、溜めた絵は残らない。
    g.begin_frame(400.0, 400.0);
    g.frame_count = 2;
    s.draw(&mut g);
    assert_eq!(g.draw_list().indices.len(), first, "描いた量が増えています");
    assert!(g.draw_list().clear.is_some(), "キャンバスを消していません");
}

/// Processing の `size()` は消さない。
#[test]
fn calling_size_again_leaves_the_canvas_alone() {
    let mut s = VmSketch::compile(
        "void draw(){ size(400,400); noStroke(); fill(255,0,0); rect(0,0,50,50); }",
        1,
    )
    .expect("コンパイルできる");
    let mut g = Graphics::new();
    g.begin_frame(400.0, 400.0);
    s.setup(&mut g);
    g.begin_frame(400.0, 400.0);
    g.frame_count = 1;
    s.draw(&mut g);
    assert!(g.draw_list().clear.is_none(), "size() がキャンバスを消しています");
}

/// `createCanvas()` は塗りと線も既定へ戻す。`noFill()` の類は残る。
///
/// キャンバスの文脈ごと作り直されるため。作品が毎フレーム `colorMode()` や
/// `noStroke()` を呼び直しているのは、これを見越してのこと。
#[test]
fn creating_the_canvas_again_puts_the_paint_back_to_default() {
    // 1 フレーム目で赤にしても、2 フレーム目の頭で白へ戻る。
    let mut s = VmSketch::compile(
        "c=0\ndraw=_=>{createCanvas(400,400);noStroke();c||fill(255,0,0);c=1;rect(0,0,50,50)}",
        1,
    )
    .expect("コンパイルできる");
    let mut g = Graphics::new();
    g.begin_frame(400.0, 400.0);
    s.setup(&mut g);
    for frame in 1..=2 {
        g.begin_frame(400.0, 400.0);
        g.frame_count = frame;
        s.draw(&mut g);
    }
    let c = g.draw_list().vertices.first().expect("描かれる").color;
    assert!(c[1] > 0.9, "塗りが既定へ戻っていません: {c:?}");

    // noStroke() は p5 側の旗なので残る。線は増えない。
    let quiet = triangles("draw=_=>{createCanvas(400,400);noStroke();rect(0,0,50,50)}");
    assert_eq!(quiet, 2, "線が復活しています");
}
