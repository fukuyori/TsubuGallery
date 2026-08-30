//! 作品の Export / Import (設計書 §27)。
//!
//! 複数の作品をソースとメタデータごと 1 つの JSON にまとめる。サムネイルは
//! 入れない。受け取った側で作り直せるものなので、ファイルを小さく保つ。
//!
//! ```json
//! {
//!   "app": "TsubuGallery",
//!   "format": 1,
//!   "exported_at": 1780000000,
//!   "sketches": [
//!     { "id": "spiral", "title": "Spiral", "author": "", "link": "",
//!       "tags": ["abstract"], "favorite": false, "source": "..." }
//!   ]
//! }
//! ```
//!
//! `id` はファイル名 (`<id>.pde`) であり、受け取った側で既にあれば
//! [`crate::library::unique_id`] で `spiral-2` のように別名になる。

use std::collections::BTreeSet;
use std::path::Path;

use serde::{Deserialize, Serialize};

/// 拡張子。`.json` の前に付けて、ただの JSON と見分ける。
pub const EXTENSION: &str = "tsubu.json";

/// この版が書く形式番号。読むときは同じか古いものだけ受ける。
pub const FORMAT: u32 = 1;

const APP: &str = "TsubuGallery";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExportFile {
    pub app: String,
    pub format: u32,
    /// UNIX 秒。
    pub exported_at: i64,
    pub sketches: Vec<ExportedSketch>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExportedSketch {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub link: String,
    #[serde(default)]
    pub tags: BTreeSet<String>,
    #[serde(default)]
    pub favorite: bool,
    pub source: String,
}

impl ExportFile {
    pub fn new(exported_at: i64, sketches: Vec<ExportedSketch>) -> Self {
        Self { app: APP.to_string(), format: FORMAT, exported_at, sketches }
    }

    /// 書き出す。
    pub fn write(&self, path: &Path) -> std::io::Result<()> {
        let text = serde_json::to_string_pretty(self).map_err(std::io::Error::other)?;
        std::fs::write(path, text)
    }

    /// 読み込む。このアプリのファイルでないもの、新しすぎる形式は断る。
    pub fn read(path: &Path) -> Result<Self, ReadError> {
        let text = std::fs::read_to_string(path)?;
        let file: Self = serde_json::from_str(&text)?;
        if file.app != APP {
            return Err(ReadError::NotOurs);
        }
        if file.format > FORMAT {
            return Err(ReadError::TooNew(file.format));
        }
        Ok(file)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ReadError {
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("JSON として読めません: {0}")]
    Json(#[from] serde_json::Error),
    #[error("TsubuGallery のエクスポートファイルではありません")]
    NotOurs,
    #[error("新しい版で作られたファイルです (format {0})")]
    TooNew(u32),
}

/// 既定のファイル名。`tsubugallery-2026-08-30.tsubu.json`
pub fn default_file_name(exported_at: i64) -> String {
    let (y, m, d) = civil_from_unix(exported_at);
    format!("tsubugallery-{y:04}-{m:02}-{d:02}.{EXTENSION}")
}

/// UNIX 秒から UTC の年月日。Howard Hinnant の days_from_civil の逆。
fn civil_from_unix(seconds: i64) -> (i64, u32, u32) {
    let days = seconds.div_euclid(86_400);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> ExportFile {
        ExportFile::new(
            1_780_000_000,
            vec![ExportedSketch {
                id: "spiral".into(),
                title: "Spiral".into(),
                author: "someone".into(),
                link: "https://example.com/1".into(),
                tags: ["abstract".to_string(), "loop".to_string()].into_iter().collect(),
                favorite: true,
                source: "void draw() {}".into(),
            }],
        )
    }

    #[test]
    fn a_file_round_trips() {
        let path = std::env::temp_dir()
            .join(format!("tsubu-exchange-{}.{EXTENSION}", std::process::id()));
        let file = sample();
        file.write(&path).unwrap();
        assert_eq!(ExportFile::read(&path).unwrap(), file);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn missing_optional_fields_are_filled_in() {
        let text = r#"{"app":"TsubuGallery","format":1,"exported_at":0,
            "sketches":[{"id":"a","title":"A","source":"x"}]}"#;
        let file: ExportFile = serde_json::from_str(text).unwrap();
        assert_eq!(file.sketches[0].author, "");
        assert!(file.sketches[0].tags.is_empty());
        assert!(!file.sketches[0].favorite);
    }

    #[test]
    fn foreign_and_future_files_are_refused() {
        let dir = std::env::temp_dir();
        let foreign = dir.join(format!("tsubu-foreign-{}.json", std::process::id()));
        std::fs::write(&foreign, r#"{"app":"Other","format":1,"exported_at":0,"sketches":[]}"#)
            .unwrap();
        assert!(matches!(ExportFile::read(&foreign), Err(ReadError::NotOurs)));

        let future = dir.join(format!("tsubu-future-{}.json", std::process::id()));
        std::fs::write(&future, r#"{"app":"TsubuGallery","format":99,"exported_at":0,"sketches":[]}"#)
            .unwrap();
        assert!(matches!(ExportFile::read(&future), Err(ReadError::TooNew(99))));

        let _ = std::fs::remove_file(foreign);
        let _ = std::fs::remove_file(future);
    }

    #[test]
    fn the_default_name_carries_the_date() {
        // 2026-08-30T00:00:00Z
        assert_eq!(default_file_name(1_788_048_000), "tsubugallery-2026-08-30.tsubu.json");
        assert_eq!(civil_from_unix(0), (1970, 1, 1));
        assert_eq!(civil_from_unix(951_782_400), (2000, 2, 29));
    }
}
