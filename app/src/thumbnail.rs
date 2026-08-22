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
    let expected = image.width as usize * image.height as usize * 4;
    if image.rgba.len() != expected {
        return Err("キャプチャした画素数が解像度と一致しません".to_string());
    }

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("{} を作成できませんでした: {e}", parent.display()))?;
    }

    image::save_buffer_with_format(
        path,
        &image.rgba,
        image.width,
        image.height,
        image::ColorType::Rgba8,
        image::ImageFormat::Png,
    )
    .map_err(|e| format!("{} を保存できませんでした: {e}", path.display()))
}

/// ディスクから読み込んだ画素。
pub struct Decoded {
    pub id: String,
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

pub enum Completed {
    Loaded(Decoded),
    Saved {
        id: String,
        force: bool,
    },
    Failed {
        id: String,
        path: PathBuf,
        message: String,
        force: bool,
    },
}

enum Request {
    Load {
        id: String,
        path: PathBuf,
    },
    Save {
        id: String,
        path: PathBuf,
        image: CapturedImage,
        force: bool,
    },
}

/// ディスク上のサムネイルをワーカースレッドで読む。
///
/// Gallery のスクロールを止めないため、PNG のデコードはメインスレッドで行わない
/// (設計書 §22)。
pub struct ThumbnailLoader {
    requests: mpsc::Sender<Request>,
    results: mpsc::Receiver<Completed>,
}

impl ThumbnailLoader {
    pub fn new() -> Result<Self, String> {
        let (requests, request_rx) = mpsc::channel::<Request>();
        let (result_tx, results) = mpsc::channel();

        std::thread::Builder::new()
            .name("tsubu-thumbnail-loader".into())
            .spawn(move || {
                // 送信側が落ちたらループを抜けてスレッドが終わる。
                for request in request_rx {
                    let completed = match request {
                        Request::Load { id, path } => match image::open(&path) {
                            Ok(img) => {
                                let rgba = img.to_rgba8();
                                Completed::Loaded(Decoded {
                                    id,
                                    width: rgba.width(),
                                    height: rgba.height(),
                                    rgba: rgba.into_raw(),
                                })
                            }
                            Err(e) => Completed::Failed {
                                id,
                                message: format!("{} を読めません: {e}", path.display()),
                                path,
                                force: false,
                            },
                        },
                        Request::Save {
                            id,
                            path,
                            image,
                            force,
                        } => match save_png(&image, &path) {
                            Ok(()) => Completed::Saved { id, force },
                            Err(message) => Completed::Failed {
                                id,
                                path,
                                message,
                                force,
                            },
                        },
                    };
                    if result_tx.send(completed).is_err() {
                        break;
                    }
                }
            })
            .map_err(|e| format!("サムネイル処理スレッドを起動できませんでした: {e}"))?;

        Ok(Self { requests, results })
    }

    pub fn request_load(&self, id: &str, path: &Path) -> Result<(), String> {
        self.requests
            .send(Request::Load {
                id: id.to_string(),
                path: path.to_path_buf(),
            })
            .map_err(|_| "サムネイル処理スレッドが停止しています".to_string())
    }

    pub fn request_save(
        &self,
        id: &str,
        path: &Path,
        image: CapturedImage,
        force: bool,
    ) -> Result<(), String> {
        self.requests
            .send(Request::Save {
                id: id.to_string(),
                path: path.to_path_buf(),
                image,
                force,
            })
            .map_err(|_| "サムネイル処理スレッドが停止しています".to_string())
    }

    /// 完了した処理を 1 件取り出す。無ければ `Ok(None)`。
    pub fn poll(&self) -> Result<Option<Completed>, String> {
        match self.results.try_recv() {
            Ok(result) => Ok(Some(result)),
            Err(mpsc::TryRecvError::Empty) => Ok(None),
            Err(mpsc::TryRecvError::Disconnected) => {
                Err("サムネイル処理スレッドが停止しています".to_string())
            }
        }
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

    #[test]
    fn png_save_rejects_an_invalid_pixel_count_before_touching_the_file_system() {
        let image = CapturedImage {
            width: 2,
            height: 2,
            rgba: vec![0; 15],
        };
        let error = save_png(&image, Path::new("missing-parent/thumbnail.png"))
            .expect_err("invalid image must fail");
        assert!(error.contains("画素数"));
    }

    #[test]
    fn worker_saves_and_loads_a_thumbnail() {
        let root = std::env::temp_dir().join(format!(
            "tsubu-thumbnail-worker-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("時計が正しい")
                .as_nanos()
        ));
        let path = root.join("demo.png");
        let loader = ThumbnailLoader::new().expect("ワーカーを作れる");
        loader
            .request_save(
                "demo",
                &path,
                CapturedImage {
                    width: 1,
                    height: 1,
                    rgba: vec![10, 20, 30, 255],
                },
                true,
            )
            .expect("保存を頼める");
        match loader
            .results
            .recv_timeout(std::time::Duration::from_secs(2))
        {
            Ok(Completed::Saved { id, force }) => {
                assert_eq!(id, "demo");
                assert!(force);
            }
            _ => panic!("保存完了が返りませんでした"),
        }

        loader
            .request_load("demo", &path)
            .expect("読み込みを頼める");
        match loader
            .results
            .recv_timeout(std::time::Duration::from_secs(2))
        {
            Ok(Completed::Loaded(image)) => {
                assert_eq!(image.id, "demo");
                assert_eq!((image.width, image.height), (1, 1));
                assert_eq!(image.rgba, [10, 20, 30, 255]);
            }
            _ => panic!("読み込み完了が返りませんでした"),
        }

        std::fs::remove_dir_all(root).expect("テスト用ファイルを片付けられる");
    }
}
