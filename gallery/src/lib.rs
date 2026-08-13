//! Gallery — 作品をサムネイルのグリッドとして並べる層 (設計書 §6)。
//!
//! ここには UI フレームワークへの依存を入れない。レイアウト計算・選択の動き・
//! サムネイル取得の順序といった「ふるまい」だけを持ち、実際の描画は app 側の
//! egui コードが担当する。

pub mod grid;
pub mod model;
pub mod view_model;

pub use grid::{GridMetrics, layout};
pub use model::{Filter, GalleryItem, SketchStatus, SortOrder, ThumbnailState};
pub use view_model::{GalleryView, Move};
