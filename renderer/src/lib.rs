//! TsubuGallery の描画エンジン。
//!
//! Processing Lite の API 呼び出しを共通の描画コマンドへ変換し、バッチ化して GPU
//! へ渡す (設計書 §17)。Viewer 用のウィンドウ描画とサムネイル生成は同じ
//! [`BatchRenderer`] を共有する。

pub mod batch;
pub mod canvas;
pub mod capture;
pub mod draw;
pub mod font;
pub mod texture;

pub use batch::{BatchRenderer, SAMPLE_COUNT};
pub use canvas::Canvas;
pub use capture::{CaptureError, CapturedImage, Capturer};
pub use draw::{
    Affine, AngleMode, Batch, BlendMode, Color, ColorMode, DrawList, Graphics, ShapeKind,
    ShapeMode, TextAlign, Vertex,
};
pub use texture::MsaaTarget;
