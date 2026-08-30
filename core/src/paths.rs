//! データ保存場所の解決 (設計書 §7.3 / §9.2)。
//!
//! 保存先は OS ごとに違うので、Core は「どこに置くか」だけを決め、実際の
//! ディレクトリ取得は Platform Adapter 相当の [`dirs`] に任せる。

use std::path::{Path, PathBuf};

/// 開発時にデータ位置を差し替えるための環境変数。
pub const DATA_DIR_ENV: &str = "TSUBU_DATA_DIR";

/// アプリのデータ領域。
#[derive(Clone, Debug)]
pub struct DataPaths {
    root: PathBuf,
}

impl DataPaths {
    /// データ領域を決める。
    ///
    /// 優先順は `TSUBU_DATA_DIR` → [`crate::config::Config`] の `data_dir` →
    /// OS 標準のデータディレクトリ配下の `TsubuGallery/`。
    pub fn resolve() -> Self {
        if let Some(custom) = std::env::var_os(DATA_DIR_ENV) {
            return Self { root: PathBuf::from(custom) };
        }
        let default = Self::default_root();
        let config = crate::config::Config::load(&default);
        Self { root: config.data_dir.unwrap_or(default) }
    }

    /// 何も指定しないときの場所。`config.json` もここに置く。
    pub fn default_root() -> PathBuf {
        dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("TsubuGallery")
    }

    /// 環境変数で差し替えられているか。そのときは設定画面で変えても効かない。
    pub fn overridden_by_env() -> bool {
        std::env::var_os(DATA_DIR_ENV).is_some()
    }

    pub fn with_root(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// ソースやメタデータ。
    pub fn sketches(&self) -> PathBuf {
        self.root.join("sketches")
    }

    /// サムネイル画像。DB へは入れずファイルとして持つ (設計書 §7.3)。
    pub fn thumbnails(&self) -> PathBuf {
        self.root.join("thumbnails")
    }

    /// Bytecode キャッシュ。消えても再生成できるものだけを置く。
    pub fn cache(&self) -> PathBuf {
        self.root.join("cache")
    }

    /// 実行ログ。消しても動作に影響しない。
    pub fn logs(&self) -> PathBuf {
        self.root.join("logs")
    }

    /// メタデータ DB (設計書 §19)。
    pub fn database(&self) -> PathBuf {
        self.root.join("library.sqlite3")
    }

    pub fn thumbnail_for(&self, sketch_id: &str) -> PathBuf {
        self.thumbnails().join(format!("{sketch_id}.png"))
    }

    /// 必要なサブディレクトリをまとめて作る。
    pub fn ensure(&self) -> std::io::Result<()> {
        for dir in [self.sketches(), self.thumbnails(), self.cache(), self.logs()] {
            std::fs::create_dir_all(dir)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_matches_design_document() {
        let p = DataPaths::with_root("/tmp/tsubu-test");
        assert!(p.sketches().ends_with("sketches"));
        assert!(p.thumbnails().ends_with("thumbnails"));
        assert!(p.cache().ends_with("cache"));
        assert!(p.logs().ends_with("logs"));
        assert_eq!(p.thumbnail_for("spiral").file_name().unwrap(), "spiral.png");
        assert_eq!(p.database().file_name().unwrap(), "library.sqlite3");
    }
}
