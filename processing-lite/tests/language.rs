
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
