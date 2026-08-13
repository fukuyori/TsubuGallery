//! 設定画面 (設計書 §24)。
//!
//! 項目は設計書の 5 グループをそのままの並びで出す。値を変えた瞬間に効かせて
//! 保存もするので、「適用」ボタンは置かない。取り消したいときは既定値へ戻す。

use tsubu_core::Locales;
use tsubu_core::locale::LanguagePreference;
use tsubu_core::settings::{
    CAPTURE_FRAME_RANGE, CardSize, Choice, ExecutionBudget, FrameRate, ImageQuality, Navigation,
    SLIDESHOW_INTERVAL_RANGE, ScreenSaver, Settings, SortOrder, StartScreen, Theme, ViewMode,
};

/// 設定画面で起きたこと。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SettingsAction {
    /// 変更された。保存して各所へ反映する。
    Changed,
    Close,
}

pub struct SettingsUi<'a> {
    pub settings: &'a mut Settings,
    pub locales: &'a Locales,
}

/// 言語欄に出す「システムに合わせる」の選択肢。
const SYSTEM_LANGUAGE: &str = "settings.language.system";

const LABEL_WIDTH: f32 = 200.0;
const CONTROL_WIDTH: f32 = 220.0;

pub fn build(root: &mut egui::Ui, state: &mut SettingsUi<'_>) -> Vec<SettingsAction> {
    let mut actions = Vec::new();
    let t = |key: &str| state.locales.t(key).to_string();

    egui::Panel::top("tsubu.settings.top").exact_size(56.0).show(root, |ui| {
        ui.add_space(12.0);
        ui.horizontal(|ui| {
            ui.add_space(16.0);
            ui.label(egui::RichText::new(t("settings.title")).size(20.0).strong());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_space(16.0);
                if ui.button(t("settings.close")).clicked() {
                    actions.push(SettingsAction::Close);
                }
                if ui.button(t("settings.reset")).clicked() {
                    *state.settings = Settings::default();
                    actions.push(SettingsAction::Changed);
                }
            });
        });
    });

    egui::CentralPanel::default().show(root, |ui| {
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            ui.add_space(8.0);
            let before = state.settings.clone();

            group(ui, &t("settings.group.general"), |ui| {
                language_row(ui, state);
                choice_row::<Theme>(ui, state.locales, &t("settings.theme"), &mut state.settings.theme);
                choice_row::<StartScreen>(
                    ui,
                    state.locales,
                    &t("settings.start_screen"),
                    &mut state.settings.start_screen,
                );
            });

            group(ui, &t("settings.group.gallery"), |ui| {
                choice_row::<ViewMode>(
                    ui,
                    state.locales,
                    &t("settings.view_mode"),
                    &mut state.settings.view_mode,
                );
                // リストはカードを並べないので、大きさの出番が無い。
                if state.settings.view_mode.uses_card_size() {
                    choice_row::<CardSize>(
                        ui,
                        state.locales,
                        &t("settings.card_size"),
                        &mut state.settings.card_size,
                    );
                }
                choice_row::<SortOrder>(
                    ui,
                    state.locales,
                    &t("settings.sort_order"),
                    &mut state.settings.sort_order,
                );
                switch_row(
                    ui,
                    state.locales,
                    &t("settings.show_titles"),
                    &mut state.settings.show_titles,
                );
            });

            group(ui, &t("settings.group.viewer"), |ui| {
                switch_row(
                    ui,
                    state.locales,
                    &t("settings.fullscreen"),
                    &mut state.settings.fullscreen,
                );
                choice_row::<FrameRate>(
                    ui,
                    state.locales,
                    &t("settings.frame_rate"),
                    &mut state.settings.frame_rate,
                );
                choice_row::<Navigation>(
                    ui,
                    state.locales,
                    &t("settings.navigation"),
                    &mut state.settings.navigation,
                );
                switch_row(ui, state.locales, &t("settings.preload"), &mut state.settings.preload);
                note(ui, &t("settings.preload.note"));

                row(ui, &t("settings.slideshow_interval"), |ui| {
                    ui.spacing_mut().slider_width = CONTROL_WIDTH - 70.0;
                    ui.add(
                        egui::Slider::new(
                            &mut state.settings.slideshow_interval,
                            SLIDESHOW_INTERVAL_RANGE,
                        )
                        .suffix(" s")
                        .clamping(egui::SliderClamping::Always),
                    );
                });
                note(ui, &t("settings.slideshow_interval.note"));

                choice_row::<ScreenSaver>(
                    ui,
                    state.locales,
                    &t("settings.screensaver"),
                    &mut state.settings.screensaver,
                );
                note(ui, &t("settings.screensaver.note"));
            });

            group(ui, &t("settings.group.thumbnail"), |ui| {
                row(ui, &t("settings.capture_frame"), |ui| {
                    ui.spacing_mut().slider_width = CONTROL_WIDTH - 70.0;
                    ui.add(
                        egui::Slider::new(&mut state.settings.capture_frame, CAPTURE_FRAME_RANGE)
                            .clamping(egui::SliderClamping::Always),
                    );
                });
                note(ui, &t("settings.capture_frame.note"));
                choice_row::<ImageQuality>(
                    ui,
                    state.locales,
                    &t("settings.image_quality"),
                    &mut state.settings.image_quality,
                );
                note(ui, &format!("{} px", state.settings.image_quality.width()));
            });

            group(ui, &t("settings.group.runtime"), |ui| {
                choice_row::<ExecutionBudget>(
                    ui,
                    state.locales,
                    &t("settings.execution_budget"),
                    &mut state.settings.execution_budget,
                );
                note(ui, &t("settings.execution_budget.note"));
            });

            ui.add_space(24.0);

            if *state.settings != before {
                actions.push(SettingsAction::Changed);
            }
        });
    });

    actions
}

fn group(ui: &mut egui::Ui, title: &str, body: impl FnOnce(&mut egui::Ui)) {
    ui.add_space(10.0);
    ui.horizontal(|ui| {
        ui.add_space(16.0);
        ui.label(
            egui::RichText::new(title)
                .size(13.0)
                .strong()
                .color(egui::Color32::from_rgb(120, 170, 255)),
        );
    });
    ui.add_space(4.0);
    body(ui);
    ui.add_space(6.0);
}

/// ラベル 1 列 + コントロール 1 列の行。
///
/// ラベルの領域は文字の長さによらず幅を固定する。`allocate_ui` に大きさを渡す
/// だけでは中身の分しか使われず、項目ごとにコントロールの左端がずれる。
fn row(ui: &mut egui::Ui, label: &str, control: impl FnOnce(&mut egui::Ui)) {
    ui.horizontal(|ui| {
        ui.add_space(24.0);
        let height = ui.spacing().interact_size.y;
        let (rect, _) = ui.allocate_exact_size(egui::vec2(LABEL_WIDTH, height), egui::Sense::hover());
        ui.painter().text(
            rect.left_center(),
            egui::Align2::LEFT_CENTER,
            label,
            egui::TextStyle::Body.resolve(ui.style()),
            ui.visuals().text_color(),
        );
        control(ui);
    });
}

/// 説明文。値の意味が名前だけでは分からない項目に添える。
fn note(ui: &mut egui::Ui, text: &str) {
    ui.horizontal(|ui| {
        ui.add_space(24.0 + LABEL_WIDTH);
        ui.label(
            egui::RichText::new(text).size(11.0).color(egui::Color32::from_white_alpha(110)),
        );
    });
    ui.add_space(2.0);
}

fn choice_row<T: Choice>(ui: &mut egui::Ui, locales: &Locales, label: &str, value: &mut T) {
    row(ui, label, |ui| {
        egui::ComboBox::from_id_salt(format!("tsubu.settings.{label}"))
            .selected_text(locales.t(&value.locale_key()).to_string())
            .width(CONTROL_WIDTH - 8.0)
            .show_ui(ui, |ui| {
                for option in T::ALL {
                    let text = locales.t(&option.locale_key()).to_string();
                    ui.selectable_value(value, *option, text);
                }
            });
    });
}

fn switch_row(ui: &mut egui::Ui, locales: &Locales, label: &str, value: &mut bool) {
    row(ui, label, |ui| {
        let text = locales.t(if *value { "settings.on" } else { "settings.off" }).to_string();
        // 幅を揃え、OFF でも枠を描く。枠が無いと文字が並んでいるだけに見えて、
        // 押せることが分からない。
        let size = egui::vec2(72.0, ui.spacing().interact_size.y);
        if ui.add_sized(size, egui::Button::new(text).selected(*value)).clicked() {
            *value = !*value;
        }
    });
}

/// 言語だけは「システムに合わせる」があるので専用に組む。
fn language_row(ui: &mut egui::Ui, state: &mut SettingsUi<'_>) {
    let label = state.locales.t("settings.language").to_string();
    let system = state.locales.t(SYSTEM_LANGUAGE).to_string();

    let selected = match &state.settings.language {
        LanguagePreference::System => system.clone(),
        LanguagePreference::Explicit(tag) => state
            .locales
            .available()
            .iter()
            .find(|t| t.tag() == tag)
            .map_or_else(|| tag.clone(), |t| t.native_name().to_string()),
    };

    row(ui, &label, |ui| {
        egui::ComboBox::from_id_salt("tsubu.settings.language")
            .selected_text(selected)
            .width(CONTROL_WIDTH - 8.0)
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut state.settings.language, LanguagePreference::System, system);
                for translation in state.locales.available() {
                    ui.selectable_value(
                        &mut state.settings.language,
                        LanguagePreference::Explicit(translation.tag().to_string()),
                        translation.native_name(),
                    );
                }
            });
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// ウィンドウを開かずに設定画面を描き、頂点数と操作を返す。
    fn run(settings: &mut Settings) -> (Vec<SettingsAction>, usize) {
        let ctx = egui::Context::default();
        let locales = Locales::builtin();
        let mut actions = Vec::new();
        let mut vertices = 0;

        // Area の配置は 1 フレーム目には確定しないので 2 フレーム回す。
        for _ in 0..2 {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::pos2(0.0, 0.0),
                    egui::vec2(900.0, 900.0),
                )),
                ..Default::default()
            };
            let mut output = ctx.run_ui(input, |ui| {
                actions = build(ui, &mut SettingsUi { settings, locales: &locales });
            });
            output.textures_delta.clear();
            vertices = ctx
                .tessellate(output.shapes, 1.0)
                .iter()
                .map(|p| match &p.primitive {
                    egui::epaint::Primitive::Mesh(mesh) => mesh.vertices.len(),
                    _ => 0,
                })
                .sum();
        }

        (actions, vertices)
    }

    #[test]
    fn the_settings_screen_draws() {
        let mut settings = Settings::default();
        let (actions, vertices) = run(&mut settings);
        assert!(vertices > 500, "何も描けていません: {vertices} 頂点");
        assert!(actions.is_empty(), "触っていないのに操作が出ています: {actions:?}");
    }

    #[test]
    fn drawing_does_not_change_the_settings() {
        let mut settings = Settings::default();
        let before = settings.clone();
        run(&mut settings);
        assert_eq!(settings, before);
    }

    /// 画面に出す文字列がすべて翻訳されているか。
    ///
    /// 翻訳が無いと [`Locales::t`] はキーをそのまま返すので、生のキーが画面へ
    /// 出てしまう。追加した項目の翻訳忘れはこれで気づける。
    #[test]
    fn every_label_is_translated() {
        let locales = Locales::builtin();
        let mut missing = Vec::new();

        let mut check = |key: String| {
            if locales.t(&key) == key {
                missing.push(key);
            }
        };

        for key in [
            "settings.title",
            "settings.close",
            "settings.reset",
            "settings.on",
            "settings.off",
            "settings.group.general",
            "settings.group.gallery",
            "settings.group.viewer",
            "settings.group.thumbnail",
            "settings.group.runtime",
            "settings.language",
            SYSTEM_LANGUAGE,
            "settings.theme",
            "settings.start_screen",
            "settings.view_mode",
            "settings.card_size",
            "settings.sort_order",
            "settings.show_titles",
            "settings.fullscreen",
            "settings.frame_rate",
            "settings.navigation",
            "settings.preload",
            "settings.preload.note",
            "settings.slideshow_interval",
            "settings.slideshow_interval.note",
            "settings.screensaver",
            "settings.screensaver.note",
            "settings.capture_frame",
            "settings.capture_frame.note",
            "settings.image_quality",
            "settings.execution_budget",
            "settings.execution_budget.note",
        ] {
            check(key.to_string());
        }

        fn variants<T: Choice>(check: &mut impl FnMut(String)) {
            for v in T::ALL {
                check(v.locale_key());
            }
        }
        variants::<Theme>(&mut check);
        variants::<StartScreen>(&mut check);
        variants::<ViewMode>(&mut check);
        variants::<CardSize>(&mut check);
        variants::<Navigation>(&mut check);
        variants::<ScreenSaver>(&mut check);
        variants::<ImageQuality>(&mut check);
        variants::<ExecutionBudget>(&mut check);
        variants::<FrameRate>(&mut check);
        variants::<SortOrder>(&mut check);

        assert!(missing.is_empty(), "翻訳が足りません: {missing:?}");
    }
}
