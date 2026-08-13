//! Processing Lite ランタイム。
//!
//! 設計書 §13 の言語処理系一式。ソースはここでコンパイルされ、Bytecode として
//! VM が実行する。Viewer は [`sketch::Sketch`] しか知らないので、将来
//! p5.js subset などのフロントエンドを足しても Viewer は変わらない (§23.2)。
//!
//! ```text
//! source → lexer → parser → ast → compiler → bytecode → vm ─→ dyn Sketch → Graphics
//! ```

pub mod ast;
pub mod bytecode;
pub mod compiler;
pub mod dialect;
pub mod examples;
pub mod format;
pub mod highlight;
pub mod js;
pub mod lexer;
pub mod math;
pub mod natives;
pub mod parser;
pub mod sketch;
pub mod vm;
pub mod vm_sketch;

pub use lexer::CompileError;
pub use sketch::{BrokenSketch, LoadedSketch, Sketch, SketchInfo};
pub use vm_sketch::VmSketch;
