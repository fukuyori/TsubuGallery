//! データ領域の外に置く、起動前に要る設定。
//!
//! 設定 (設計書 §24) は `library.sqlite3` の中にあるが、「データ領域をどこに
//! 置くか」だけは DB を開く前に決まっていなければならない。そこでこの 1 項目
//! だけを、OS 標準のデータディレクトリ配下の `config.json` に持つ。
//!
//! ```text
//! <OS のデータ領域>/TsubuGallery/config.json   ← ここは動かない
//!   { "data_dir": "D:/Dropbox/TsubuGallery" }   ← 作品・サムネイル・DB はこちら
//! ```
//!
//! 環境変数 `TSUBU_DATA_DIR` はこのファイルより優先する (開発時の差し替え用)。

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// ファイル名。
pub const FILE_NAME: &str = "config.json";

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Config {
    /// データ領域の置き場。`None` なら既定の場所。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data_dir: Option<PathBuf>,
}

impl Config {
    fn path(default_root: &Path) -> PathBuf {
        default_root.join(FILE_NAME)
    }

    /// 読む。無ければ既定値。壊れていても既定値で起動させ、理由はログに残す。
    pub fn load(default_root: &Path) -> Self {
        let path = Self::path(default_root);
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Self::default(),
            Err(e) => {
                log::warn!("{} を読めません: {e}", path.display());
                return Self::default();
            }
        };
        match serde_json::from_str(&text) {
            Ok(config) => config,
            Err(e) => {
                log::warn!("{} が壊れているので既定値を使います: {e}", path.display());
                Self::default()
            }
        }
    }

    /// 書く。既定値なら消す (無いことが既定を表す)。
    pub fn save(&self, default_root: &Path) -> std::io::Result<()> {
        let path = Self::path(default_root);
        if *self == Self::default() {
            return match std::fs::remove_file(&path) {
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
                _ => Ok(()),
            };
        }
        std::fs::create_dir_all(default_root)?;
        let text = serde_json::to_string_pretty(self).map_err(std::io::Error::other)?;
        std::fs::write(&path, text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("tsubu-config-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_missing_file_means_the_default() {
        let dir = temp_dir("missing");
        assert_eq!(Config::load(&dir), Config::default());
    }

    #[test]
    fn the_data_dir_round_trips() {
        let dir = temp_dir("roundtrip");
        let config = Config { data_dir: Some(PathBuf::from("D:/somewhere/else")) };
        config.save(&dir).unwrap();
        assert_eq!(Config::load(&dir), config);

        // 既定へ戻すとファイルは消える。
        Config::default().save(&dir).unwrap();
        assert!(!dir.join(FILE_NAME).exists());
        assert_eq!(Config::load(&dir), Config::default());
    }

    #[test]
    fn a_broken_file_falls_back_to_the_default() {
        let dir = temp_dir("broken");
        std::fs::write(dir.join(FILE_NAME), "{ not json").unwrap();
        assert_eq!(Config::load(&dir), Config::default());
    }
}
