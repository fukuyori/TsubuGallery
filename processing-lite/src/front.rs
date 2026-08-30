//! 貼られたコードを、方言を問わず 1 本の [`Sketch`] にする入口。
//!
//! 呼ぶ側 (読み込み・エディタの検査・サムネイル生成) は、そのコードが
//! Processing なのか p5.js なのか、それとも GLSL なのかを気にしない。
//! ここで見分けて、それぞれのフロントエンドへ送る。
//!
//! ```text
//! source ─┬─ GOLF      → GlslSketch (GOLF → GLSL → naga → WGSL → GPU)
//!         ├─ GLSL      → GlslSketch (naga → WGSL → GPU)
//!         └─ それ以外   → VmSketch   (parser → bytecode → VM)
//! ```

use crate::dialect::{self, Dialect};
use crate::glsl_sketch::GlslSketch;
use crate::lexer::CompileError;
use crate::sketch::Sketch;
use crate::vm_sketch::VmSketch;

/// コンパイルの通ったスケッチと、どの方言として読まれたか。
pub struct Compiled {
    pub sketch: Box<dyn Sketch>,
    pub dialect: Dialect,
    /// バイトコードの命令数。GLSL には無いので 0。
    pub instructions: usize,
}

/// ソース 1 本をコンパイルする。
///
/// `seed` は `random()` の再現性のために作品ごとへ固定した値。GLSL には
/// 乱数が無いので効かない。
pub fn compile(source: &str, seed: u64) -> Result<Compiled, CompileError> {
    // GOLF は GLSL の語も含むので、GLSL より先に見る。
    if dialect::looks_like_golf(source) {
        let sketch = GlslSketch::compile_golf(source)?;
        return Ok(Compiled { sketch: Box::new(sketch), dialect: Dialect::Golf, instructions: 0 });
    }
    if dialect::looks_like_glsl(source) {
        let sketch = GlslSketch::compile(source)?;
        return Ok(Compiled { sketch: Box::new(sketch), dialect: Dialect::Glsl, instructions: 0 });
    }

    let sketch = VmSketch::compile(source, seed)?;
    let dialect = sketch.dialect();
    let instructions = sketch.instruction_count();
    Ok(Compiled { sketch: Box::new(sketch), dialect, instructions })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_p5_sketch_goes_to_the_vm() {
        let compiled = compile("draw=_=>{circle(1,2,3)}", 0).expect("通る");
        assert_eq!(compiled.dialect, Dialect::P5);
        assert!(compiled.instructions > 0);
    }

    #[test]
    fn a_processing_sketch_goes_to_the_vm() {
        let compiled = compile("void draw() { circle(1, 2, 3); }", 0).expect("通る");
        assert_eq!(compiled.dialect, Dialect::Processing);
    }

    #[test]
    fn a_tweet_sized_shader_goes_to_the_gpu() {
        let compiled = compile("o = vec4(FC.xy / r, 0, 1);", 0).expect("通る");
        assert_eq!(compiled.dialect, Dialect::Glsl);
        assert_eq!(compiled.instructions, 0, "GLSL に命令数は無い");
    }

    #[test]
    fn a_golf_shader_goes_to_the_gpu_too() {
        let compiled = compile("f2 uv = C.xy / R.xy\nO = f4(uv, 0, 1)", 0).expect("通る");
        assert_eq!(compiled.dialect, Dialect::Golf);
        assert_eq!(compiled.instructions, 0);
    }

    /// GLSL は Processing のパーサへ回さない。
    ///
    /// 回すと 1 行目の `float e, i` で転び、「文の区切りがありません」としか
    /// 言えない。何が起きているのかが分からないエラーになる。
    #[test]
    fn a_shader_is_not_reported_as_broken_processing() {
        let source = "float e, i, g;\nfor (; i++< 1e2;) {\n  vec3 p = vec3(FC.xy / r, g);\n  g += length(p);\n}\no += g;";
        let compiled = compile(source, 0).expect("通る");
        assert_eq!(compiled.dialect, Dialect::Glsl);
    }
}
