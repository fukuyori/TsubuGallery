//! p5.js subset のフロントエンド (設計書 §23.2)。
//!
//! ```text
//! source → js::lexer → js::parser → js::ast → js::compiler → bytecode → vm
//! ```
//!
//! Bytecode から先は Java Mode の Processing Lite と共通。値の表現だけ
//! [`crate::bytecode::Value`] を配列・オブジェクト・関数まで広げてある。

pub mod ast;
pub mod compiler;
pub mod lexer;
pub mod parser;

pub use compiler::compile;
pub use parser::parse;

#[cfg(test)]
mod tests {
    use crate::sketch::Sketch;
    use crate::vm_sketch::VmSketch;
    use tsubu_renderer::Graphics;

    /// 画面に貼られた、実際の p5.js のつぶやき作品。
    const PASTED: &str = r#"t=0
$=[]
draw=_⇒{t?colorMode(HSB):createCanvas(W=720,W)
B=blendMode
B(BLEND)
background(0,.03)
B(ADD)
for(i=2;i--;)$[t++%W]={x:t*1.5%W,y:t*4%W,s:25,c:t%360}
$.map(p⇒fill(p.c,90,W,.1)+circle(p.x+=cos(A=noise(p.x/180,p.y/180,t/W/W)*99),p.y+=sin(A),p.s*=.99))}"#;

    fn run(source: &str, frames: u64) -> Graphics {
        let mut sketch = VmSketch::compile(source, 1).expect("コンパイルに成功する");
        let mut g = Graphics::new();

        g.begin_frame(720.0, 720.0);
        sketch.setup(&mut g);
        for frame in 1..=frames {
            g.begin_frame(720.0, 720.0);
            g.frame_count = frame;
            sketch.draw(&mut g);
        }
        assert!(sketch.error().is_none(), "実行が止まった: {:?}", sketch.error());
        g
    }

    /// 式を 1 つ評価して数値で返す。`out` グローバルへ書かせて読み出す。
    fn eval(expr: &str) -> f32 {
        eval_after("", expr)
    }

    /// トップレベルの宣言を先に置いてから、式を 1 つ評価する。
    fn eval_after(prelude: &str, expr: &str) -> f32 {
        let source = format!("{prelude}\nout=0\ndraw=_=>{{out={expr}}}");
        let program = crate::js::compile(&crate::js::parse(&source).expect("パース"))
            .expect("コンパイル");
        let mut vm = crate::vm::Vm::new(&program, 1);
        let mut g = Graphics::new();
        g.begin_frame(100.0, 100.0);
        vm.init_globals(&program, &mut g, crate::vm::DEFAULT_FRAME_BUDGET).expect("初期化");
        vm.call(&program, program.draw.expect("draw"), &mut g, crate::vm::DEFAULT_FRAME_BUDGET)
            .expect("実行");
        vm.global_by_name(&program, "out").expect("out がある").as_f32()
    }

    #[test]
    fn the_pasted_sketch_compiles_and_draws() {
        let g = run(PASTED, 60);
        assert!(!g.draw_list().is_empty(), "何も描いていない");
    }

    #[test]
    fn semicolons_can_be_omitted() {
        let g = run("draw=_=>{\n  background(0)\n  circle(1,2,3)\n}", 1);
        assert!(!g.draw_list().is_empty());
    }

    #[test]
    fn numbers_divide_like_javascript() {
        // JavaScript の数値は 1 種類しかないので、整数どうしでも小数になる。
        assert_eq!(eval("7/2"), 3.5);
    }

    #[test]
    fn arrays_index_length_and_grow() {
        assert_eq!(eval("[10,20,30][1]"), 20.0);
        assert_eq!(eval("[1,2,3].length"), 3.0);
        // 飛ばした添字は undefined で埋まる。
        assert_eq!(eval("(a=[],a[3]=9,a.length)"), 4.0);
        // 範囲外は undefined。数にすると NaN。
        assert!(eval("[1][5]").is_nan());
    }

    #[test]
    fn objects_carry_named_fields() {
        assert_eq!(eval("({x:1,y:2}).y"), 2.0);
        assert_eq!(eval("(p={x:1},p.x+=5,p.x)"), 6.0);
        // 略記 `{x}` は `{x: x}`。
        assert_eq!(eval("(x=7,({x}).x)"), 7.0);
    }

    #[test]
    fn arrow_functions_are_values() {
        assert_eq!(eval("(f=x=>x*2,f(21))"), 42.0);
        assert_eq!(eval("(f=(a,b)=>a+b,f(1,2))"), 3.0);
        assert_eq!(eval("(f=x=>{return x+1},f(1))"), 2.0);
    }

    #[test]
    fn native_functions_can_be_stored_in_a_variable() {
        // `B = blendMode` のような書き方。
        let g = run("draw=_=>{C=circle;C(1,2,3)}", 1);
        assert!(!g.draw_list().is_empty(), "変数経由でも描ける");
    }

    #[test]
    fn map_and_push_work_on_arrays() {
        assert_eq!(eval("[1,2,3].map(x=>x*2)[2]"), 6.0);
        assert_eq!(eval("(a=[],a.push(1),a.push(2),a.length)"), 2.0);
        assert_eq!(eval("[1,2,3,4].filter(x=>x>2).length"), 2.0);
    }

    #[test]
    fn map_passes_the_index_but_one_parameter_is_fine() {
        assert_eq!(eval("[9,9,9].map((v,i)=>i)[2]"), 2.0);
        assert_eq!(eval("[9,9,9].map(v=>v)[0]"), 9.0);
    }

    #[test]
    fn numbers_are_truthy_like_javascript() {
        assert_eq!(eval("0?1:2"), 2.0);
        assert_eq!(eval("3?1:2"), 1.0);
        // `for(i=2;i--;)` の書き方が成り立つ。
        assert_eq!(eval("(n=0,(()=>{for(i=3;i--;)n++})(),n)"), 3.0);
    }

    #[test]
    fn assignment_is_an_expression() {
        assert_eq!(eval("(a=1)+(b=2)"), 3.0);
        assert_eq!(eval("(a=1,a+=2)"), 3.0);
        // 後置は元の値を返す。
        assert_eq!(eval("(i=5,i++)"), 5.0);
        assert_eq!(eval("(i=5,++i)"), 6.0);
    }

    #[test]
    fn a_function_declaration_also_works() {
        let g = run("function draw(){circle(1,2,3)}", 1);
        assert!(!g.draw_list().is_empty());
    }

    #[test]
    fn setup_runs_once_before_draw() {
        let source = "n=0\nsetup=_=>{n=10}\ndraw=_=>{n++;circle(n,0,1)}";
        let mut sketch = VmSketch::compile(source, 1).expect("コンパイル");
        let mut g = Graphics::new();
        g.begin_frame(100.0, 100.0);
        sketch.setup(&mut g);
        for frame in 1..=3 {
            g.begin_frame(100.0, 100.0);
            g.frame_count = frame;
            sketch.draw(&mut g);
        }
        assert!(sketch.error().is_none());
    }

    #[test]
    fn a_processing_sketch_still_picks_the_java_frontend() {
        // どちらの方言も同じ入口から入る。取り違えないこと。
        let g = run(include_str!("../../sketches/spiral.pde"), 3);
        assert!(!g.draw_list().is_empty());
    }

    #[test]
    fn a_runaway_p5_loop_is_stopped() {
        let mut sketch = VmSketch::compile("draw=_=>{for(;;)circle(0,0,1)}", 1)
            .expect("コンパイル")
            .with_budget(2_000);
        let mut g = Graphics::new();
        for frame in 1..=4 {
            g.begin_frame(100.0, 100.0);
            g.frame_count = frame;
            if frame == 1 {
                sketch.setup(&mut g);
            }
            sketch.draw(&mut g);
        }
        assert!(sketch.error().is_some(), "止まらなかった");
    }

    #[test]
    fn switch_picks_a_case_and_falls_through() {
        // break で止まる。
        assert_eq!(eval("(f=n=>{switch(n){case 1:return 10;case 2:return 20}return 0},f(2))"), 20.0);
        // break が無ければ次の case へ落ちる。Java Mode と同じ。
        assert_eq!(eval("(n=0,(()=>{switch(1){case 1:n++;case 2:n++;break;case 3:n++}})(),n)"), 2.0);
        // どれにも当たらなければ default。
        assert_eq!(eval("(f=n=>{switch(n){case 1:return 1;default:return 9}},f(7))"), 9.0);
        // default が無く、当たらなければ何もしない。
        assert_eq!(eval("(n=5,(()=>{switch(0){case 1:n=1}})(),n)"), 5.0);
    }

    /// `switch` の中の `continue` は、switch ではなく外のループに効く。
    #[test]
    fn continue_inside_a_switch_belongs_to_the_loop() {
        assert_eq!(
            eval("(n=0,(()=>{for(i=0;i<4;i++){switch(i%2){case 0:continue}n++}})(),n)"),
            2.0
        );
    }

    /// ループの外の `continue` は、switch の中にあっても通さない。
    #[test]
    fn continue_outside_a_loop_is_still_refused() {
        let source = "draw=_=>{switch(1){case 1:continue}}";
        let script = crate::js::parse(source).expect("パースは通る");
        assert!(crate::js::compile(&script).is_err(), "continue が通ってしまった");
    }

    /// JavaScript では予約語もプロパティ名に書ける。
    #[test]
    fn a_keyword_can_be_a_property_name() {
        assert_eq!(eval("({default:3}).default"), 3.0);
        assert_eq!(eval("({if:1,for:2}).for"), 2.0);
    }

    #[test]
    fn a_missing_function_is_named() {
        let mut sketch =
            VmSketch::compile("setup=_=>{createColorPicker('#fff')}\ndraw=_=>{}", 1).expect("通る");
        let mut g = Graphics::new();
        g.begin_frame(100.0, 100.0);
        sketch.setup(&mut g);
        let error = sketch.error().expect("止まるはず");
        assert!(error.contains("createColorPicker"), "{error}");
    }

    /// 文字列は 1 文字ずつに割れる。字数を数えるのにも使われる書き方。
    #[test]
    fn a_string_spreads_into_its_characters() {
        assert_eq!(eval("[...'#つぶやきProcessing'].length"), 15.0);
        assert_eq!(eval("[...'abc'][1]=='b'?7:0"), 7.0);
        // 展開した先を並べ直しても崩れない。
        assert_eq!(eval("[...[9],...'ab',...[8]].length"), 4.0);
    }

    /// `p5.Vector` の静的メソッド。`p5` という値は持っていないので、
    /// 名前の並びを見て組み込みへ読み替えている。
    #[test]
    fn p5_vector_has_static_methods() {
        // 単位球の上の点なので、長さは必ず 1。
        assert!((eval("p5.Vector.random3D().mag()") - 1.0).abs() < 1e-5);
        assert!((eval("p5.Vector.random2D().mag()") - 1.0).abs() < 1e-5);
        assert_eq!(eval("p5.Vector.add(createVector(1,2),createVector(3,4)).y"), 6.0);
        assert_eq!(eval("p5.Vector.dist(createVector(0,0),createVector(3,4))"), 5.0);
        assert_eq!(eval("p5.Vector.mult(createVector(1,2),3).y"), 6.0);
        assert_eq!(eval("p5.Vector.fromAngle(0,5).x"), 5.0);
    }

    /// 元のベクトルは変えない。ここがインスタンスのメソッドとの違い。
    #[test]
    fn a_static_vector_method_leaves_its_arguments_alone() {
        assert_eq!(eval("(a=createVector(1,2),p5.Vector.add(a,createVector(9,9)),a.x)"), 1.0);
        // インスタンスのほうは自分を書き換える。
        assert_eq!(eval("(a=createVector(1,2),a.add(createVector(9,9)),a.x)"), 10.0);
    }

    /// 作品が `p5` という名前を使っていたら、そちらを優先する。
    #[test]
    fn a_sketch_can_still_name_something_p5() {
        assert_eq!(eval_after("p5={Vector:{random3D:7}}", "p5.Vector.random3D"), 7.0);
    }

    /// コールバックに組み込みを渡せる。`$.map(p5.Vector.random3D)` の形。
    ///
    /// 組み込みはフレームを積まないので、作品の関数と同じに扱うと
    /// 呼び出し元の続きまで走ってしまう。
    #[test]
    fn a_builtin_can_be_a_callback() {
        assert_eq!(eval("[1,4,9].map(sqrt)[2]"), 3.0);
        assert_eq!(eval("[...'ab'].map(p5.Vector.random3D).length"), 2.0);
        assert_eq!(eval("[1,2,3].reduce(max)"), 3.0);
    }

    #[test]
    fn a_syntax_error_reports_a_position() {
        let Err(e) = crate::js::parse("draw=_=>{circle(1,2}") else {
            panic!("パースは失敗するはず");
        };
        assert_eq!(e.line, 1);
    }
}


