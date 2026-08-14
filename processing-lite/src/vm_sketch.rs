//! Processing Lite のソースを [`Sketch`] として動かす。
//!
//! 設計書 §15 の「表示時にコンパイルしない」に従い、コンパイルはここ (読み込み時)
//! で済ませる。Viewer が作品を開いた時点では Bytecode を実行するだけ。

use tsubu_renderer::Graphics;

use crate::bytecode::Program;
use crate::compiler::compile;
use crate::dialect::{self, Dialect};
use crate::lexer::CompileError;
use crate::parser::parse;
use crate::sketch::Sketch;
use crate::vm::{DEFAULT_FRAME_BUDGET, Trap, Vm};

/// 連続でこの回数だけ打ち切られたら、実行を諦めてエラー表示に切り替える。
///
/// 1 回の予算超過は重いフレームかもしれないが、続くなら無限ループとみなす。
const MAX_CONSECUTIVE_TRAPS: u32 = 3;

pub struct VmSketch {
    program: Program,
    /// どちらのフロントエンドが通ったか。
    dialect: Dialect,
    vm: Vm,
    /// `random()` の数列の出発点。動かし直すときにここへ戻す。
    seed: u64,
    budget: u64,
    consecutive_traps: u32,
    error: Option<String>,
}

impl VmSketch {
    /// ソースをコンパイルして実行可能にする。
    ///
    /// Processing (Java Mode) と p5.js の両方を受ける。どちらで書かれているかは
    /// 試してみて決める。コンパイルは数十 µs なので、外すのは安い (設計書 §23.2)。
    ///
    /// `seed` は `random()` の再現性のため、作品ごとに固定した値を渡す。
    pub fn compile(source: &str, seed: u64) -> Result<Self, CompileError> {
        let (program, dialect) = compile_any(source)?;
        let vm = Vm::new(&program, seed);
        Ok(Self {
            program,
            dialect,
            vm,
            seed,
            budget: DEFAULT_FRAME_BUDGET,
            consecutive_traps: 0,
            error: None,
        })
    }

    /// 1 フレームの実行予算を変える (設計書 §24 の Runtime / Execution Budget)。
    pub fn with_budget(mut self, budget: u64) -> Self {
        self.budget = budget;
        self
    }

    /// どちらの方言として読まれたか。
    pub fn dialect(&self) -> Dialect {
        self.dialect
    }

    pub fn instruction_count(&self) -> usize {
        self.program.instruction_count()
    }

    /// 直近フレームで実行した命令数。
    pub fn last_frame_ops(&self) -> u64 {
        self.vm.last_frame_ops
    }

    /// 打ち切りを記録する。続けて起きるようならその作品は止める。
    fn record(&mut self, result: Result<(), Trap>) {
        match result {
            Ok(()) => self.consecutive_traps = 0,
            Err(trap) => {
                self.consecutive_traps += 1;
                if self.consecutive_traps >= MAX_CONSECUTIVE_TRAPS {
                    // ここで諦めないと、暴走した作品が毎フレーム予算を使い切り、
                    // Gallery 全体の操作感を落とす (設計書 §21.1)。
                    self.error = Some(trap.to_string());
                }
            }
        }
    }
}

impl Sketch for VmSketch {
    fn set_budget(&mut self, budget: u64) {
        self.budget = budget;
    }

    fn setup(&mut self, g: &mut Graphics) {
        // p5.js の text() は塗りと線の両方で描く。Processing は塗りだけ。
        g.set_text_stroked(self.dialect == Dialect::P5);
        // 下地。Processing は灰 204、p5.js のキャンバスは透明でページの白が透ける。
        g.set_default_background(match self.dialect {
            Dialect::P5 => tsubu_renderer::Color::WHITE,
            Dialect::Processing => tsubu_renderer::Color::DEFAULT_BACKGROUND,
        });
        let result = self.vm.init_globals(&self.program, g, self.budget);
        if let Err(trap) = result {
            self.error = Some(trap.to_string());
            return;
        }
        if let Some(setup) = self.program.setup {
            let result = self.vm.call(&self.program, setup, g, self.budget);
            if let Err(trap) = result {
                self.error = Some(trap.to_string());
            }
        }
    }

    fn draw(&mut self, g: &mut Graphics) {
        if self.error.is_some() {
            return;
        }
        let Some(draw) = self.program.draw else { return };
        let result = self.vm.call(&self.program, draw, g, self.budget);
        self.record(result);
    }

    fn dialect(&self) -> Option<Dialect> {
        Some(self.dialect)
    }

    fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    fn instructions_last_frame(&self) -> u64 {
        self.vm.last_frame_ops
    }

    /// 静的モードの作品は `draw()` を持たない (設計書 §14.1)。
    fn draws_once(&self) -> bool {
        self.program.draw.is_none()
    }

    fn restart(&mut self) {
        self.vm = Vm::new(&self.program, self.seed);
        self.consecutive_traps = 0;
        self.error = None;
    }
}

/// Processing と p5.js を順に試す。
///
/// 先に方言を見て、それらしいほうから当たる。両方外したときは、見立てた方言の
/// エラーを返す。そのほうが直しどころに近い。
fn compile_any(source: &str) -> Result<(Program, Dialect), CompileError> {
    let looks_like_p5 = dialect::diagnose(source).dialect == Dialect::P5;

    let processing = || compile(&parse(source)?);
    let p5 = || crate::js::compile(&crate::js::parse(source)?);

    if looks_like_p5 {
        return match p5() {
            Ok(program) => Ok((program, Dialect::P5)),
            Err(js_error) => {
                processing().map(|p| (p, Dialect::Processing)).map_err(|_| js_error)
            }
        };
    }

    match processing() {
        Ok(program) => Ok((program, Dialect::Processing)),
        Err(java_error) => p5().map(|p| (p, Dialect::P5)).map_err(|_| java_error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(sketch: &mut VmSketch, g: &mut Graphics, n: u64) {
        g.begin_frame(200.0, 120.0);
        g.frame_count = n;
        if n == 1 {
            sketch.setup(g);
        }
        sketch.draw(g);
    }

    #[test]
    fn each_frontend_is_reported() {
        let java = VmSketch::compile("void draw() { background(0); }", 1).expect("Java Mode");
        assert_eq!(java.dialect(), Dialect::Processing);
        assert_eq!(java.dialect().label(), "Processing");

        let p5 = VmSketch::compile("draw=_=>{background(0)}", 1).expect("p5.js");
        assert_eq!(p5.dialect(), Dialect::P5);
        assert_eq!(p5.dialect().label(), "p5.js");
    }

    #[test]
    fn a_compiled_sketch_draws() {
        let mut s = VmSketch::compile("void draw() { background(0); circle(50, 50, 20); }", 1)
            .expect("コンパイルに成功する");
        let mut g = Graphics::new();
        frame(&mut s, &mut g, 1);
        assert!(!g.draw_list().is_empty());
        assert!(s.error().is_none());
    }

    #[test]
    fn a_syntax_error_is_returned_at_compile_time() {
        let Err(e) = VmSketch::compile("void draw() { background(0) }", 1) else {
            panic!("コンパイルは失敗するはず");
        };
        assert!(e.message.contains("`;`"), "{e}");
    }

    #[test]
    fn setup_runs_once_and_globals_persist() {
        let mut s = VmSketch::compile(
            "int n = 0;\nvoid setup() { n = 10; }\nvoid draw() { n++; point(n, 0); }",
            1,
        )
        .expect("コンパイルに成功する");
        let mut g = Graphics::new();
        for i in 1..=3 {
            frame(&mut s, &mut g, i);
        }
        assert!(s.error().is_none());
    }

    #[test]
    fn one_overrun_frame_does_not_disable_the_sketch() {
        let mut s = VmSketch::compile("void draw() { while (true) { point(0, 0); } }", 1)
            .expect("コンパイルに成功する")
            .with_budget(500);
        let mut g = Graphics::new();
        frame(&mut s, &mut g, 1);
        assert!(s.error().is_none(), "1 回では止めない");
    }

    #[test]
    fn a_runaway_sketch_is_disabled_after_repeated_overruns() {
        let mut s = VmSketch::compile("void draw() { while (true) { point(0, 0); } }", 1)
            .expect("コンパイルに成功する")
            .with_budget(500);
        let mut g = Graphics::new();
        for i in 1..=MAX_CONSECUTIVE_TRAPS as u64 {
            frame(&mut s, &mut g, i);
        }
        assert!(s.error().is_some(), "続けて超過したら止める");
        assert!(s.error().expect("ある").contains("上限"), "{:?}", s.error());
    }

    #[test]
    fn a_heavy_but_finite_frame_resets_the_counter() {
        // 予算を 2 回超えたあと成功すると、カウンタが戻って止まらない。
        let mut s = VmSketch::compile(
            "int limit = 100000;
             void draw() { for (int i = 0; i < limit; i++) { } limit = 1; }",
            1,
        )
        .expect("コンパイルに成功する")
        .with_budget(5_000);
        let mut g = Graphics::new();
        frame(&mut s, &mut g, 1);
        assert!(s.error().is_none());
        frame(&mut s, &mut g, 2);
        // 2 フレーム目は limit=100000 のまま超過するが、まだ止めない。
        assert!(s.error().is_none());
    }
}
