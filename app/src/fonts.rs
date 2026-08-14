//! UI フォントの用意。
//!
//! egui の既定フォントに CJK は含まれないので、日本語 UI を出すには OS のフォント
//! を借りる必要がある (設計書 §11.3 の CJK 要件)。見つからない環境では日本語が
//! 豆腐になるため、その場合は呼び出し側が英語へ退避する。

use std::sync::Arc;

/// OS ごとの CJK フォント候補。上から順に試す。
const CANDIDATES: &[&str] = &[
    // macOS
    "/System/Library/Fonts/ヒラギノ角ゴシック W3.ttc",
    "/System/Library/Fonts/ヒラギノ角ゴシック W4.ttc",
    "/System/Library/Fonts/Hiragino Sans GB.ttc",
    "/Library/Fonts/Arial Unicode.ttf",
    // Windows
    "C:/Windows/Fonts/YuGothM.ttc",
    "C:/Windows/Fonts/meiryo.ttc",
    "C:/Windows/Fonts/msgothic.ttc",
    // Linux
    "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
    "/usr/share/fonts/opentype/noto/NotoSansCJKjp-Regular.otf",
    "/usr/share/fonts/truetype/fonts-japanese-gothic.ttf",
    "/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc",
];

/// 記号のフォント。日本語のフォントに入っていない字を補う。
///
/// 麻雀牌 (U+1F000〜) やトランプのような記号は、CJK フォントには無いことが
/// 多い。1 本だけだと、そういう字を使う作品が空白になる。
///
/// 上ほど優先。作品はたいてい Windows か Web ブラウザで作られているので、
/// そこで使われる字形に近いものを先に置く。同じ字でも、フォントによって
/// 字の大きさもベースラインからの位置も違う。作品はそれを前提に座標を
/// 決めていることがあり、ここが変わると図形と字がずれる。
///
/// 照合はファイル名を小文字の英数字だけに均してから。配布元によって
/// `seguisym.ttf` だったり `Segoe-UI-Symbol.ttf` だったりするので、
/// 完全一致で探すと入れてあるのに見つけられない。
const SYMBOL_NAMES: &[&str] = &[
    // Windows。麻雀牌や将棋の駒はこちらが本家。
    "seguisym",
    "segoeuisymbol",
    "seguiemj",
    "segoeuiemoji",
    // Linux
    "notosanssymbols2",
    "notosanssymbols",
    // macOS
    "applesymbols",
];

/// 記号のフォントを何本まで積むか。
///
/// 1 本に無い字が別の 1 本にあることがある。増やすほど字形を焼くときの
/// 探索が伸びるので、ほどほどで止める。
const MAX_SYMBOL_FONTS: usize = 3;

/// フォントを探す場所。ユーザーが自分で入れたものも拾う。
///
/// `~/Library/Fonts` に置いたフォントは、OS のフォント帳から入れたもの。
/// ここを見ないと「入れたのに使われない」ことになる。
fn font_dirs() -> Vec<std::path::PathBuf> {
    let mut dirs: Vec<std::path::PathBuf> = Vec::new();
    if let Some(home) = std::env::var_os("HOME").map(std::path::PathBuf::from) {
        dirs.push(home.join("Library/Fonts"));
        dirs.push(home.join(".fonts"));
        dirs.push(home.join(".local/share/fonts"));
    }
    if let Some(local) = std::env::var_os("LOCALAPPDATA").map(std::path::PathBuf::from) {
        dirs.push(local.join("Microsoft/Windows/Fonts"));
    }
    dirs.extend(
        [
            "/Library/Fonts",
            "/System/Library/Fonts",
            "/System/Library/Fonts/Supplemental",
            "C:/Windows/Fonts",
            "/usr/share/fonts/truetype/noto",
            "/usr/share/fonts/opentype/noto",
            "/usr/share/fonts/truetype",
        ]
        .iter()
        .map(std::path::PathBuf::from),
    );
    dirs
}

/// ファイル名を小文字の英数字だけに均す。`Segoe-UI-Symbol.ttf` → `segoeuisymbol`。
fn normalize(name: &str) -> String {
    name.rsplit_once('.')
        .map_or(name, |(stem, _)| stem)
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

/// 記号のフォントを優先順に探す。
fn find_symbol_fonts() -> Vec<std::path::PathBuf> {
    let mut found: Vec<(usize, std::path::PathBuf)> = Vec::new();
    for dir in font_dirs() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else { continue };
            let name = normalize(name);
            if let Some(rank) = SYMBOL_NAMES.iter().position(|want| name == *want)
                && !found.iter().any(|(r, _)| *r == rank)
            {
                found.push((rank, path));
            }
        }
    }
    found.sort_by_key(|(rank, _)| *rank);
    found.into_iter().take(MAX_SYMBOL_FONTS).map(|(_, path)| path).collect()
}

/// 作品の `text()` に使うフォントを、前から試す順に読む。
///
/// UI と同じ候補に記号のフォントを足す。`.ttc` のように束ねられたファイルでも、
/// 字形を取り出す側が先頭の 1 本を使う。
pub fn load_sketch_fonts() -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    if let Some(main) = CANDIDATES.iter().find_map(|p| std::fs::read(p).ok()) {
        out.push(main);
    }
    let symbols = find_symbol_fonts();
    if symbols.is_empty() {
        log::info!("記号のフォントが見つかりません。麻雀牌などは空白になります。");
    }
    for path in symbols {
        if let Ok(bytes) = std::fs::read(&path) {
            log::info!("記号のフォント: {}", path.display());
            out.push(bytes);
        }
    }
    if out.is_empty() {
        log::warn!("フォントが見つかりませんでした。text() は何も描きません。");
    }
    out
}

/// CJK フォントを egui へ登録する。登録できたら `true`。
pub fn install_cjk_font(ctx: &egui::Context) -> bool {
    let Some((path, bytes)) = CANDIDATES
        .iter()
        .find_map(|p| std::fs::read(p).ok().map(|b| (*p, b)))
    else {
        log::warn!("CJK フォントが見つかりませんでした。UI を英語で表示します。");
        return false;
    };

    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert("system-cjk".to_owned(), Arc::new(egui::FontData::from_owned(bytes)));

    // 既定フォントの後ろに置く。欧文は元のまま、CJK だけこちらで補う。
    for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
        fonts.families.entry(family).or_default().push("system-cjk".to_owned());
    }

    ctx.set_fonts(fonts);
    log::info!("CJK フォントを読み込みました: {path}");
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 記号のフォントは、ユーザーが自分で入れた場所からも見つける。
    ///
    /// `~/Library/Fonts` に入れたフォントを見に行かないと、
    /// 「入れたのに使われない」ことになる。
    #[test]
    fn user_font_directories_are_searched() {
        let dirs = font_dirs();
        if let Some(home) = std::env::var_os("HOME").map(std::path::PathBuf::from) {
            assert!(dirs.contains(&home.join("Library/Fonts")), "{dirs:?}");
        }
        // OS が用意する場所も残す。
        assert!(dirs.iter().any(|d| d.starts_with("/System") || d.starts_with("C:/")), "{dirs:?}");
    }

    /// Windows の記号フォントを macOS より先に探す。
    ///
    /// 作品はたいてい Windows か Web で作られていて、字の大きさと
    /// ベースラインからの位置がそちらに合わせて書かれている。
    #[test]
    fn the_windows_symbol_font_comes_first() {
        let at = |name: &str| SYMBOL_NAMES.iter().position(|f| *f == name);
        assert!(at("seguisym") < at("applesymbols"), "{SYMBOL_NAMES:?}");
    }

    /// 配布元による名前の揺れを吸収する。
    ///
    /// 同じ Segoe UI Symbol でも `seguisym.ttf` だったり
    /// `Segoe-UI-Symbol.ttf` だったりする。完全一致で探すと、入れてあるのに
    /// 見つけられない。
    #[test]
    fn the_same_font_is_recognised_under_any_spelling() {
        for name in ["seguisym.ttf", "Segoe-UI-Symbol.ttf", "Segoe UI Symbol.TTF", "SegoeUISymbol.ttf"] {
            let n = normalize(name);
            assert!(
                SYMBOL_NAMES.contains(&n.as_str()),
                "{name} を記号のフォントとして拾えません ({n})"
            );
        }
        // 別のフォントまで拾ってしまわない。
        assert!(!SYMBOL_NAMES.contains(&normalize("segoeui.ttf").as_str()));
        assert!(!SYMBOL_NAMES.contains(&normalize("HackNerdFont-Regular.ttf").as_str()));
    }
}

