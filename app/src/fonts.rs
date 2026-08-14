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
const SYMBOL_CANDIDATES: &[&str] = &[
    // macOS
    "/System/Library/Fonts/Apple Symbols.ttf",
    "/System/Library/Fonts/Supplemental/Apple Symbols.ttf",
    // Windows
    "C:/Windows/Fonts/seguisym.ttf",
    // Linux
    "/usr/share/fonts/truetype/noto/NotoSansSymbols2-Regular.ttf",
    "/usr/share/fonts/opentype/noto/NotoSansSymbols2-Regular.ttf",
];

/// 作品の `text()` に使うフォントを、前から試す順に読む。
///
/// UI と同じ候補に記号のフォントを足す。`.ttc` のように束ねられたファイルでも、
/// 字形を取り出す側が先頭の 1 本を使う。
pub fn load_sketch_fonts() -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    if let Some(main) = CANDIDATES.iter().find_map(|p| std::fs::read(p).ok()) {
        out.push(main);
    }
    if let Some(symbols) = SYMBOL_CANDIDATES.iter().find_map(|p| std::fs::read(p).ok()) {
        out.push(symbols);
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
