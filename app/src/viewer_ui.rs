//! Viewer に重ねる操作オーバーレイ (設計書 §8.2)。
//!
//! 常時は何も出さず、操作があったときだけ薄く表示して一定時間で自動的に消える。
//! 情報表示 (`I`) だけは明示トグルなので自動非表示の対象外。

use tsubu_core::Locales;

use crate::ui::panel;
use crate::viewer::Stats;

pub struct ViewerOverlay<'a> {
    pub title: &'a str,
    pub index: usize,
    pub total: usize,
    pub paused: bool,
    pub stats: Stats,
    /// 0.0 なら操作系オーバーレイを描かない。
    pub alpha: f32,
    pub show_info: bool,
    /// 作品が動かない理由。コンパイルエラーや実行の打ち切り。
    pub error: Option<&'a str>,
    /// Processing か p5.js か。
    pub dialect: Option<&'static str>,
    /// 作者。空なら出さない。
    pub author: &'a str,
    /// 元の投稿などへのリンク。
    pub link: &'a str,
    /// スライドショー中か (設計書 §27)。
    pub slideshow: bool,
    /// スクリーンセーバーとして動いているか。
    pub screensaver: bool,
}

pub fn build(ui: &mut egui::Ui, info: &ViewerOverlay<'_>, locales: &Locales) {
    let ctx = ui.ctx();
    // スクリーンセーバー中は何も重ねない。作品だけを見せるための機能なので、
    // 操作の説明が出ていては台無しになる。
    if info.alpha > 0.0 && !info.screensaver {
        title_area(ctx, info, locales);
        hint_area(ctx, info, locales);
    }

    if let Some(error) = info.error {
        error_area(ctx, error, locales);
    } else if info.paused {
        egui::Area::new("tsubu.viewer.paused".into())
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .interactable(false)
            .show(ctx, |ui| {
                panel(ui, |ui| {
                    ui.label(
                        egui::RichText::new(locales.t("viewer.paused"))
                            .size(18.0)
                            .color(egui::Color32::from_white_alpha(220)),
                    );
                });
            });
    }

    if info.show_info {
        info_area(ctx, info, locales);
    }
}

/// 動かない作品の理由を画面中央に出す。Gallery のエラーバッジと対になる。
fn error_area(ctx: &egui::Context, error: &str, locales: &Locales) {
    egui::Area::new("tsubu.viewer.error".into())
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .interactable(false)
        .show(ctx, |ui| {
            ui.set_max_width(520.0);
            panel(ui, |ui| {
                ui.label(
                    egui::RichText::new(locales.t("viewer.error"))
                        .size(16.0)
                        .strong()
                        .color(egui::Color32::from_rgb(255, 150, 150)),
                );
                ui.add_space(6.0);
                ui.label(egui::RichText::new(error).size(13.0).monospace());
            });
        });
}

fn title_area(ctx: &egui::Context, info: &ViewerOverlay<'_>, locales: &Locales) {
    let fade = egui::Color32::from_white_alpha((info.alpha * 255.0) as u8);
    let dim = egui::Color32::from_white_alpha((info.alpha * 160.0) as u8);

    egui::Area::new("tsubu.viewer.title".into())
        .anchor(egui::Align2::LEFT_TOP, [20.0, 20.0])
        .interactable(false)
        .show(ctx, |ui| {
            ui.set_opacity(info.alpha);
            panel(ui, |ui| {
                ui.label(egui::RichText::new(info.title).size(22.0).strong().color(fade));
                let mut sub = format!("{} / {}", info.index + 1, info.total);
                if info.slideshow {
                    // 自動で絵が変わる理由が分かるようにする。
                    sub.push_str("  ·  ");
                    sub.push_str(locales.t("viewer.slideshow"));
                }
                ui.label(egui::RichText::new(sub).size(13.0).color(dim));
            });
        });
}

fn hint_area(ctx: &egui::Context, info: &ViewerOverlay<'_>, locales: &Locales) {
    let fade = egui::Color32::from_white_alpha((info.alpha * 255.0) as u8);
    let dim = egui::Color32::from_white_alpha((info.alpha * 170.0) as u8);

    egui::Area::new("tsubu.viewer.hints".into())
        .anchor(egui::Align2::CENTER_BOTTOM, [0.0, -24.0])
        .interactable(false)
        .show(ctx, |ui| {
            ui.set_opacity(info.alpha);
            panel(ui, |ui| {
                ui.horizontal(|ui| {
                    let pause_label = if info.paused {
                        locales.t("viewer.resume")
                    } else {
                        locales.t("viewer.pause")
                    };
                    for (key, label) in [
                        ("←", locales.t("viewer.previous")),
                        ("→", locales.t("viewer.next")),
                        ("Space", pause_label),
                        ("P", locales.t("viewer.slideshow")),
                        ("R", locales.t("viewer.random")),
                        ("T", locales.t("viewer.update_thumbnail")),
                        ("I", locales.t("viewer.info")),
                        ("O", locales.t("gallery.open_link")),
                        ("F", locales.t("viewer.fullscreen")),
                        ("Esc", locales.t("viewer.back_to_gallery")),
                    ] {
                        ui.label(egui::RichText::new(key).size(12.0).strong().color(fade));
                        ui.label(egui::RichText::new(label).size(12.0).color(dim));
                        ui.add_space(10.0);
                    }
                });
            });
        });
}

/// 3 桁ごとに区切る。命令数は 100 万を超えるので、素の数字では読めない。
fn thousands(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

fn info_area(ctx: &egui::Context, info: &ViewerOverlay<'_>, locales: &Locales) {
    egui::Area::new("tsubu.viewer.info".into())
        .anchor(egui::Align2::RIGHT_TOP, [-20.0, 20.0])
        .interactable(false)
        .show(ctx, |ui| {
            panel(ui, |ui| {
                egui::Grid::new("tsubu.viewer.info.grid").num_columns(2).spacing([16.0, 4.0]).show(
                    ui,
                    |ui| {
                        let mut row = |k: &str, v: String| {
                            ui.label(
                                egui::RichText::new(k)
                                    .size(12.0)
                                    .color(egui::Color32::from_white_alpha(150)),
                            );
                            ui.label(egui::RichText::new(v).size(12.0).monospace());
                            ui.end_row();
                        };
                        row(locales.t("viewer.stat.sketch"), info.title.to_string());
                        if !info.author.is_empty() {
                            row(locales.t("viewer.author"), info.author.to_string());
                        }
                        if !info.link.is_empty() {
                            row(locales.t("viewer.link"), info.link.to_string());
                        }
                        if let Some(dialect) = info.dialect {
                            row(locales.t("viewer.stat.dialect"), dialect.to_string());
                        }
                        row(locales.t("viewer.stat.frame_rate"), format!("{:.1} fps", info.stats.fps));
                        row(locales.t("viewer.stat.frame"), info.stats.frame_count.to_string());
                        // 仕事の時間をフレームの間隔で割ったもの。1 に近いほど
                        // 余裕が無く、超えると目標のフレームレートに届かない。
                        row(
                            locales.t("viewer.stat.load"),
                            format!(
                                "{:>3.0}%   {:.1} / {:.1} ms",
                                info.stats.load * 100.0,
                                info.stats.frame_ms,
                                info.stats.interval_ms
                            ),
                        );
                        row(
                            locales.t("viewer.stat.sketch_time"),
                            format!("{:.2} ms", info.stats.sketch_ms),
                        );
                        row(
                            locales.t("viewer.stat.instructions"),
                            thousands(info.stats.instructions),
                        );
                        row(
                            locales.t("viewer.stat.triangles"),
                            thousands(info.stats.triangles as u64),
                        );
                        row(
                            locales.t("viewer.stat.switch_time"),
                            format!("{:.3} ms", info.stats.last_switch_ms),
                        );
                    },
                );
            });
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 大きい数は 3 桁ごとに区切る。
    ///
    /// 命令数は 100 万を超える。素の数字では桁が読めない。
    #[test]
    fn big_numbers_are_grouped() {
        for (n, want) in [
            (0u64, "0"),
            (7, "7"),
            (999, "999"),
            (1_000, "1,000"),
            (12_345, "12,345"),
            (1_234_567, "1,234,567"),
        ] {
            assert_eq!(thousands(n), want);
        }
    }
}
