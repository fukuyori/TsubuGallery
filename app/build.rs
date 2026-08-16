//! Windows の実行ファイルへアイコンと版情報を埋める。
//!
//! アイコンはエクスプローラ、タスクバー、インストール済みプログラムの一覧、
//! そして winit のウィンドウが使う。版情報はプロパティ画面に出るもので、電子
//! 署名したものを配るなら、署名と見え方を揃えるために入れておきたい。
//!
//! `assets/icon.ico` は `scripts/make-icon.py` が描く。
//!
//! Windows 以外では何もしない。判定を実行環境 (`cfg(windows)`) で行っているの
//! は、`winresource` が呼ぶ `rc.exe` が Windows SDK のものだから。Linux から
//! Windows 向けへクロスコンパイルした場合はアイコンが埋まらないが、配布物は
//! Windows 上で組む前提なので割り切る。

fn main() {
    #[cfg(windows)]
    {
        println!("cargo:rerun-if-changed=assets/icon.ico");

        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/icon.ico");
        // アプリ名はローカライズしない (設計書 §2)。
        res.set("ProductName", "TsubuGallery");
        res.set("FileDescription", "TsubuGallery");
        res.set("LegalCopyright", "MIT OR Apache-2.0");
        // FileVersion と ProductVersion は CARGO_PKG_VERSION から入る。

        if let Err(e) = res.compile() {
            // 埋められなくてもアプリは動く。ビルドごと止める理由はない。
            println!("cargo:warning=アイコンを埋め込めませんでした: {e}");
        }
    }
}
