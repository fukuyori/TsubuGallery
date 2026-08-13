//! サムネイルの保存 (設計書 §7.3)。
//!
//! 画像は DB へ入れずファイルとして持つ。Phase 7 で SQLite を入れたあとも、
//! テーブルに載るのはこのパスだけ。

use std::path::{Path, PathBuf};
use std::sync::mpsc;

use tsubu_renderer::CapturedImage;

/// サムネイルの既定の横幅。縦はビューアのアスペクト比から決める。
///
/// 実際の幅は画質設定 ([`tsubu_core::settings::ImageQuality`]) が決めるので、
/// これはテストと説明のための基準値。
#[cfg(test)]
const WIDTH: u32 = 640;

/// ビューポートのアスペクト比に合わせたサムネイル解像度。
///
/// 横幅は画質設定から来る (設計書 §24 の Image Quality)。
pub fn size_for_width(viewport: (u32, u32), width: u32) -> (u32, u32) {
    let width = width.max(1);
    let (vw, vh) = viewport;
    if vw == 0 || vh == 0 {
        return (width, width);
    }
    let height = (width as f32 * vh as f32 / vw as f32).round();
    (width, (height as u32).clamp(64, 2048))
}

pub fn save_png(image: &CapturedImage, path: &Path) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("{} を作成できませんでした: {e}", parent.display()))?;
    }

    let buffer = image::RgbaImage::from_raw(image.width, image.height, image.rgba.clone())
        .ok_or_else(|| "キャプチャした画素数が解像度と一致しません".to_string())?;

    buffer
        .save_with_format(path, image::ImageFormat::Png)
        .map_err(|e| format!("{} を保存できませんでした: {e}", path.display()))
}

/// ディスクから読み込んだ画素。
pub struct Decoded {
    pub id: String,
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// ディスク上のサムネイルをワーカースレッドで読む。
///
/// Gallery のスクロールを止めないため、PNG のデコードはメインスレッドで行わない
/// (設計書 §22)。
pub struct ThumbnailLoader {
    requests: mpsc::Sender<(String, PathBuf)>,
    results: mpsc::Receiver<Result<Decoded, (String, String)>>,
}

impl ThumbnailLoader {
    pub fn new() -> Self {
        let (requests, request_rx) = mpsc::channel::<(String, PathBuf)>();
        let (result_tx, results) = mpsc::channel();

        std::thread::Builder::new()
            .name("tsubu-thumbnail-loader".into())
            .spawn(move || {
                // 送信側が落ちたらループを抜けてスレッドが終わる。
                for (id, path) in request_rx {
                    let decoded = image::open(&path)
                        .map(|img| {
                            let rgba = img.to_rgba8();
                            Decoded {
                                id: id.clone(),
                                width: rgba.width(),
                                height: rgba.height(),
                                rgba: rgba.into_raw(),
                            }
                        })
                        .map_err(|e| (id.clone(), format!("{} を読めません: {e}", path.display())));
                    if result_tx.send(decoded).is_err() {
                        break;
                    }
                }
            })
            .expect("サムネイル読み込みスレッドを起動できませんでした");

        Self { requests, results }
    }

    pub fn request(&self, id: &str, path: &Path) {
        let _ = self.requests.send((id.to_string(), path.to_path_buf()));
    }

    /// 完了した読み込みを 1 件取り出す。無ければ `None`。
    pub fn poll(&self) -> Option<Result<Decoded, (String, String)>> {
        self.results.try_recv().ok()
    }
}

impl Default for ThumbnailLoader {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thumbnail_keeps_viewport_aspect() {
        assert_eq!(size_for_width((1600, 1000), WIDTH), (640, 400));
        assert_eq!(size_for_width((1000, 1000), WIDTH), (640, 640));
    }

    #[test]
    fn degenerate_viewport_falls_back_to_square() {
        assert_eq!(size_for_width((0, 0), WIDTH), (640, 640));
    }

    #[test]
    fn image_quality_changes_the_width_but_not_the_aspect() {
        assert_eq!(size_for_width((1600, 1000), 320), (320, 200));
        assert_eq!(size_for_width((1600, 1000), 1280), (1280, 800));
    }
}
