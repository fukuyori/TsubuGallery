//! TsubuGallery Core。
//!
//! UI からも Processing Lite ランタイムからも独立した、アプリ共通のロジック層
//! (設計書 §13)。保存場所の解決、UI の多言語化、作品ファイルの読み書き、
//! メタデータの永続化 (SQLite) を持つ。

pub mod config;
pub mod exchange;
pub mod library;
pub mod locale;
pub mod lock;
pub mod logging;
pub mod open;
pub mod paths;
pub mod repository;
pub mod settings;

pub use library::SketchFile;
pub use lock::{InstanceLock, LockError};
pub use locale::{LanguagePreference, Locales};
pub use paths::DataPaths;
pub use repository::{CompileStatus, Repository, SketchMeta};
