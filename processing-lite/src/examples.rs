//! 同梱の Processing Lite 作品。
//!
//! 初回起動時にデータ領域へ書き出され、以降はふつうのユーザー作品として扱われる。
//! バイナリに埋め込んであるので、インストール先の構成に依存しない。

/// サムネイルを取得するフレーム (設計書 §7.1 の「一定フレーム後に自動取得」)。
///
/// 作品ごとの指定は Phase 7 で保存できるようにする。それまでは全作品共通。
pub const DEFAULT_THUMBNAIL_FRAME: u64 = 90;

pub struct Example {
    pub id: &'static str,
    pub source: &'static str,
}

pub const EXAMPLES: &[Example] = &[
    Example { id: "spiral", source: include_str!("../sketches/spiral.pde") },
    Example { id: "pulse-grid", source: include_str!("../sketches/pulse-grid.pde") },
    Example { id: "lissajous", source: include_str!("../sketches/lissajous.pde") },
    Example { id: "noise-field", source: include_str!("../sketches/noise-field.pde") },
    Example { id: "moire", source: include_str!("../sketches/moire.pde") },
    Example { id: "orbit", source: include_str!("../sketches/orbit.pde") },
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sketch::Sketch;
    use crate::vm_sketch::VmSketch;
    use tsubu_renderer::Graphics;

    #[test]
    fn every_example_compiles() {
        for example in EXAMPLES {
            if let Err(e) = VmSketch::compile(example.source, 1) {
                panic!("{} がコンパイルできません: {e}", example.id);
            }
        }
    }

    #[test]
    fn every_example_draws_something_at_its_thumbnail_frame() {
        for example in EXAMPLES {
            let mut sketch =
                VmSketch::compile(example.source, 1).expect("コンパイルに成功する");
            let mut g = Graphics::new();

            g.begin_frame(640.0, 400.0);
            sketch.setup(&mut g);

            // フレーム番号に依存する作品があるので、目標フレームまで進める。
            for frame in 1..=DEFAULT_THUMBNAIL_FRAME {
                g.begin_frame(640.0, 400.0);
                g.frame_count = frame;
                sketch.draw(&mut g);
            }

            assert!(sketch.error().is_none(), "{} が失敗: {:?}", example.id, sketch.error());
            assert!(
                !g.draw_list().is_empty(),
                "{} が何も描いていません",
                example.id
            );
        }
    }

    #[test]
    fn examples_have_unique_ids() {
        let mut ids: Vec<&str> = EXAMPLES.iter().map(|e| e.id).collect();
        ids.sort_unstable();
        let count = ids.len();
        ids.dedup();
        assert_eq!(ids.len(), count, "id が重複しています");
    }

    #[test]
    fn examples_stay_within_a_sensible_instruction_budget() {
        // 実行予算 (§21.1) に対して、同梱作品が桁違いに重くないことを確かめる。
        for example in EXAMPLES {
            let mut sketch = VmSketch::compile(example.source, 1).expect("コンパイル");
            let mut g = Graphics::new();
            g.begin_frame(640.0, 400.0);
            sketch.setup(&mut g);
            g.begin_frame(640.0, 400.0);
            g.frame_count = 60;
            sketch.draw(&mut g);

            assert!(
                sketch.last_frame_ops() < 2_000_000,
                "{} が 1 フレームで {} 命令使っています",
                example.id,
                sketch.last_frame_ops()
            );
        }
    }
}
