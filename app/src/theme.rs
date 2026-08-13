//! 配色 (設計書 §24 の Theme)。
//!
//! 明るい配色でも文字が読めるように、色は 1 か所で決める。egui の
//! [`egui::Visuals`] が持っている色はウィジェット用なので、カードやコード表示の
//! ように自前で描く部分はここから引く。

use tsubu_processing_lite::highlight::TokenClass;

/// 自前で描く部分の色。
#[derive(Clone, Copy, Debug)]
pub struct Palette {
    pub dark: bool,
    /// カードの地。
    pub card_bg: egui::Color32,
    pub card_bg_hover: egui::Color32,
    /// カードの上に載る文字。
    pub card_text: egui::Color32,
    /// 補助的な文字 (件数、説明、キーの説明)。
    pub dim: egui::Color32,
    /// 補助文字のうち目立たせたいもの (キー名)。
    pub strong: egui::Color32,
    pub accent: egui::Color32,
    pub error: egui::Color32,
    pub ok: egui::Color32,
    /// 行番号。
    pub gutter: egui::Color32,
    /// エラー行の帯。
    pub error_row: egui::Color32,
}

impl Palette {
    pub fn of(ui: &egui::Ui) -> Self {
        Self::for_mode(ui.visuals().dark_mode)
    }

    pub fn for_mode(dark: bool) -> Self {
        if dark {
            Self {
                dark,
                card_bg: egui::Color32::from_rgb(28, 28, 32),
                card_bg_hover: egui::Color32::from_rgb(40, 40, 46),
                card_text: egui::Color32::from_rgb(222, 224, 230),
                dim: egui::Color32::from_rgb(128, 131, 140),
                strong: egui::Color32::from_rgb(196, 199, 208),
                accent: egui::Color32::from_rgb(120, 170, 255),
                error: egui::Color32::from_rgb(255, 140, 140),
                ok: egui::Color32::from_rgb(140, 220, 160),
                gutter: egui::Color32::from_rgb(96, 100, 112),
                error_row: egui::Color32::from_rgb(70, 22, 26),
            }
        } else {
            Self {
                dark,
                card_bg: egui::Color32::from_rgb(226, 228, 233),
                card_bg_hover: egui::Color32::from_rgb(213, 216, 223),
                card_text: egui::Color32::from_rgb(28, 30, 36),
                dim: egui::Color32::from_rgb(104, 108, 118),
                strong: egui::Color32::from_rgb(48, 52, 60),
                accent: egui::Color32::from_rgb(30, 100, 210),
                error: egui::Color32::from_rgb(186, 40, 40),
                ok: egui::Color32::from_rgb(28, 126, 60),
                gutter: egui::Color32::from_rgb(150, 155, 166),
                error_row: egui::Color32::from_rgb(252, 226, 226),
            }
        }
    }

    /// 構文の色分け。
    pub fn syntax(&self, class: TokenClass) -> egui::Color32 {
        if self.dark {
            match class {
                TokenClass::Comment => egui::Color32::from_rgb(110, 122, 110),
                TokenClass::Keyword => egui::Color32::from_rgb(226, 140, 200),
                TokenClass::Type => egui::Color32::from_rgb(120, 180, 255),
                TokenClass::Number => egui::Color32::from_rgb(230, 180, 120),
                TokenClass::Char => egui::Color32::from_rgb(180, 210, 130),
                TokenClass::Api => egui::Color32::from_rgb(120, 210, 200),
                TokenClass::Builtin => egui::Color32::from_rgb(200, 170, 255),
                TokenClass::Ident | TokenClass::Plain => egui::Color32::from_rgb(222, 224, 230),
                TokenClass::Operator => egui::Color32::from_rgb(190, 195, 205),
                TokenClass::Punct => egui::Color32::from_rgb(150, 155, 165),
                // 使えない文字。ここで気付けるように目立たせる。
                TokenClass::Unknown => egui::Color32::from_rgb(255, 120, 120),
            }
        } else {
            match class {
                TokenClass::Comment => egui::Color32::from_rgb(96, 128, 96),
                TokenClass::Keyword => egui::Color32::from_rgb(168, 36, 128),
                TokenClass::Type => egui::Color32::from_rgb(26, 88, 196),
                TokenClass::Number => egui::Color32::from_rgb(158, 88, 18),
                TokenClass::Char => egui::Color32::from_rgb(86, 128, 36),
                TokenClass::Api => egui::Color32::from_rgb(16, 116, 116),
                TokenClass::Builtin => egui::Color32::from_rgb(106, 66, 186),
                TokenClass::Ident | TokenClass::Plain => egui::Color32::from_rgb(30, 32, 38),
                TokenClass::Operator => egui::Color32::from_rgb(68, 73, 84),
                TokenClass::Punct => egui::Color32::from_rgb(104, 109, 120),
                TokenClass::Unknown => egui::Color32::from_rgb(200, 36, 36),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 文字が地の色に埋もれていないか。
    ///
    /// 明るい配色で白い文字を使ってしまう取りこぼしを、ここで止める。
    #[test]
    fn text_stands_out_from_the_background() {
        fn luminance(c: egui::Color32) -> f32 {
            0.2126 * c.r() as f32 + 0.7152 * c.g() as f32 + 0.0722 * c.b() as f32
        }

        for dark in [true, false] {
            let p = Palette::for_mode(dark);
            // 地の色は egui 側が決めるので、その代表値と比べる。
            let bg = if dark { 27.0 } else { 248.0 };

            for (name, color) in [
                ("dim", p.dim),
                ("strong", p.strong),
                ("accent", p.accent),
                ("error", p.error),
                ("ok", p.ok),
                ("gutter", p.gutter),
            ] {
                let diff = (luminance(color) - bg).abs();
                assert!(diff > 40.0, "dark={dark} の {name} が地に近すぎます (差 {diff:.0})");
            }

            // カードの文字はカードの地と比べる。
            let diff = (luminance(p.card_text) - luminance(p.card_bg)).abs();
            assert!(diff > 80.0, "dark={dark} のカード文字が読めません (差 {diff:.0})");

            for class in [
                TokenClass::Comment,
                TokenClass::Keyword,
                TokenClass::Type,
                TokenClass::Number,
                TokenClass::Char,
                TokenClass::Api,
                TokenClass::Builtin,
                TokenClass::Ident,
                TokenClass::Operator,
                TokenClass::Punct,
                TokenClass::Unknown,
            ] {
                let diff = (luminance(p.syntax(class)) - bg).abs();
                assert!(diff > 40.0, "dark={dark} の {class:?} が地に近すぎます (差 {diff:.0})");
            }
        }
    }
}
