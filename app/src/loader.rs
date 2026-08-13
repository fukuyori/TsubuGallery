//! 作品の読み込みとコンパイル (設計書 §15.1)。
//!
//! 起動時にまとめてコンパイルするので、Gallery から作品を選んだ時点では Parser を
//! 動かさない (§15.2)。コンパイルに失敗した作品も一覧には残し、理由を持たせる。

use tsubu_core::DataPaths;
use tsubu_core::library;
use tsubu_processing_lite::examples::{DEFAULT_THUMBNAIL_FRAME, EXAMPLES};
use tsubu_processing_lite::{BrokenSketch, LoadedSketch, SketchInfo, VmSketch};

/// 読み込み結果 1 件。
pub struct LoadOutcome {
    pub sketch: LoadedSketch,
    /// サムネイル生成用に、実行中とは別のインスタンスを作り直すための元ソース。
    pub source: Source,
    /// コンパイルに失敗していればその理由。
    pub error: Option<String>,
}

/// 作品を作り直すのに要る情報。
#[derive(Clone)]
pub struct Source {
    pub text: String,
    pub seed: u64,
}

impl Source {
    /// 作品 id から決まるシードで束ねる。
    pub fn from_id_and_text(id: &str, text: String) -> Self {
        Self { text, seed: seed_for(id) }
    }

    /// 実行中のインスタンスから独立した、もう 1 本を組み立てる。
    ///
    /// サムネイルは目標フレームまで進めて撮るので、表示中の作品の状態を
    /// 進めてしまわないよう別インスタンスが要る。
    pub fn instantiate(&self) -> Result<VmSketch, tsubu_processing_lite::CompileError> {
        VmSketch::compile(&self.text, self.seed)
    }
}

/// データ領域の作品をすべて読み込む。空なら同梱作品を書き出してから読む。
pub fn load_library(paths: &DataPaths) -> Vec<LoadOutcome> {
    let dir = paths.sketches();

    let seeds: Vec<(&str, &str)> = EXAMPLES.iter().map(|e| (e.id, e.source)).collect();
    match library::seed_if_empty(&dir, &seeds) {
        Ok(true) => log::info!("同梱作品を {} へ書き出しました", dir.display()),
        Ok(false) => {}
        Err(e) => log::error!("同梱作品を書き出せませんでした: {e}"),
    }

    let files = library::load_all(&dir);
    if files.is_empty() {
        log::warn!("{} に作品がありません", dir.display());
    }

    files
        .into_iter()
        .map(|file| {
            let info = SketchInfo {
                title: file.title,
                thumbnail_frame: DEFAULT_THUMBNAIL_FRAME,
                // random() を作品ごとに再現可能にするため、シードは id から作る。
                id: file.id.clone(),
            };
            let source = Source { text: file.source, seed: seed_for(&file.id) };

            match source.instantiate() {
                Ok(sketch) => {
                    log::debug!(
                        "{} をコンパイルしました ({} 命令)",
                        info.id,
                        sketch.instruction_count()
                    );
                    LoadOutcome {
                        sketch: LoadedSketch::new(info, Box::new(sketch)),
                        source,
                        error: None,
                    }
                }
                Err(e) => {
                    let message = e.to_string();
                    log::warn!("{} をコンパイルできません: {message}", info.id);
                    LoadOutcome {
                        sketch: LoadedSketch::new(
                            info,
                            Box::new(BrokenSketch::new(message.clone())),
                        ),
                        source,
                        error: Some(message),
                    }
                }
            }
        })
        .collect()
}

/// 作品 id から決まるシード。同じ作品はいつ実行しても同じ乱数列になる。
fn seed_for(id: &str) -> u64 {
    // FNV-1a。分布の質より、安定していて桁が散ることが大事。
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in id.as_bytes() {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_seed_is_stable_and_differs_per_sketch() {
        assert_eq!(seed_for("spiral"), seed_for("spiral"));
        assert_ne!(seed_for("spiral"), seed_for("moire"));
    }
}
