//! つぶやき GLSL を [`Sketch`] として動かす。
//!
//! 中身は [`tsubu_renderer::shader`] が作った WGSL 1 本きり。VM も
//! バイトコードも通らないので、`draw()` は毎フレーム「このシェーダーで塗って」
//! と [`Graphics`] へ伝えるだけになる。実際に絵を作るのは GPU 側。
//!
//! 命令数の予算 ([`crate::vm::DEFAULT_FRAME_BUDGET`]) はここでは効かない。
//! ループは GPU の中で回るので、CPU 側から数えようが無い。

use std::sync::Arc;

use tsubu_renderer::{Graphics, ShaderPaint, shader};

use crate::dialect::Dialect;
use crate::lexer::CompileError;
use crate::sketch::Sketch;

pub struct GlslSketch {
    /// 翻訳済みの WGSL。フレームをまたいで使い回す。
    wgsl: Arc<str>,
    /// パイプラインを引くための鍵。WGSL から作るので同じ作品なら同じ値。
    key: u64,
}

impl GlslSketch {
    /// つぶやき GLSL をコンパイルして実行可能にする。
    ///
    /// GPU へ渡す前に naga で検証まで済ませる。wgpu の既定の扱いでは、
    /// 検証エラーはプロセスごと落ちるため、通らないものはここで弾く。
    pub fn compile(source: &str) -> Result<Self, CompileError> {
        let wgsl = shader::compile(source)
            .map_err(|e| CompileError::new(e.line, e.column, e.message))?;
        let key = fingerprint(&wgsl);
        Ok(Self { wgsl: wgsl.into(), key })
    }

    /// 翻訳結果の WGSL。中身を見たいときのために出しておく。
    pub fn wgsl(&self) -> &str {
        &self.wgsl
    }
}

impl Sketch for GlslSketch {
    fn draw(&mut self, g: &mut Graphics) {
        // マウスは 0..1 に正規化して渡す。twigl の `m` と同じ。
        let (view_w, view_h) = g.viewport();
        let mouse = [
            if view_w > 0.0 { g.mouse_x / view_w } else { 0.0 },
            if view_h > 0.0 { g.mouse_y / view_h } else { 0.0 },
        ];
        g.paint_with_shader(ShaderPaint {
            wgsl: self.wgsl.clone(),
            key: self.key,
            time: g.time,
            frame: g.frame_count as f32,
            mouse,
        });
    }

    fn dialect(&self) -> Option<Dialect> {
        Some(Dialect::Glsl)
    }
}

/// WGSL からパイプラインの鍵を作る。FNV-1a。
fn fingerprint(wgsl: &str) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in wgsl.as_bytes() {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tweet_sized_shader_compiles() {
        let sketch = GlslSketch::compile("o = vec4(FC.xy / r, sin(t) * .5 + .5, 1.);")
            .expect("通る");
        assert!(sketch.wgsl().contains("@fragment"));
        assert_eq!(sketch.dialect(), Some(Dialect::Glsl));
    }

    #[test]
    fn drawing_hands_the_shader_to_the_graphics() {
        let mut sketch = GlslSketch::compile("o = vec4(1);").expect("通る");
        let mut g = Graphics::new();
        g.begin_frame(320.0, 240.0);
        g.time = 1.5;
        g.frame_count = 90;
        sketch.draw(&mut g);

        let paint = g.draw_list().shader.as_ref().expect("シェーダーを渡している");
        assert_eq!(paint.time, 1.5);
        assert_eq!(paint.frame, 90.0);
    }

    #[test]
    fn the_key_follows_the_shader_not_the_instance() {
        let a = GlslSketch::compile("o = vec4(1);").expect("通る");
        let b = GlslSketch::compile("o = vec4(1);").expect("通る");
        let c = GlslSketch::compile("o = vec4(0);").expect("通る");
        assert_eq!(a.key, b.key, "同じソースは同じパイプラインを使う");
        assert_ne!(a.key, c.key);
    }

    #[test]
    fn a_broken_shader_reports_where() {
        let error = GlslSketch::compile("float a = 1.;\no = vec4(nosuch);")
            .err()
            .expect("通らない");
        assert_eq!(error.line, 2);
    }
}
