//! コード編集画面の描画 (設計書 §25 の Editor)。
//!
//! 入力欄にフォーカスがあるあいだ、キー入力は egui が食べる。保存や終了の
//! ショートカットもここで拾う。
//!
//! コード欄は行番号つきで、Processing Lite の文法に沿って色を付ける。折り返しは
//! しない。行番号と行が 1 対 1 でなくなると、エラー行を指せなくなるため。

use crate::theme::Palette;
use tsubu_core::Locales;
use tsubu_processing_lite::highlight;

use crate::editing::{self, CharRange};
use crate::editor::Editor;

/// コード欄の id。カーソル位置を読み書きするのに要る。
const CODE_ID: &str = "tsubu.editor.code";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditorAction {
    /// リンクをブラウザで開く。
    OpenLink,
    /// 保存してコンパイルし直す。
    Save,
    /// 編集をやめて元の画面へ戻る。未保存なら確認を挟む。
    Close,
    /// 確認に答えて、変更を捨てて閉じる。
    DiscardAndClose,
    /// 確認をやめて編集へ戻る。
    CancelClose,
    /// 保存してから Viewer で実行する。
    Run,
    /// 改行とインデントを入れて読みやすくする。
    Expand,
    /// 空白とコメントを削って詰める。
    Compress,
}

const CODE_SIZE: f32 = 14.0;

pub fn build(root: &mut egui::Ui, editor: &mut Editor, locales: &Locales) -> Vec<EditorAction> {
    let mut actions = Vec::new();

    // ショートカット。テキスト入力中でも効くよう、ウィジェットより先に見る。
    let confirming = editor.confirming_close;
    root.input(|i| {
        if confirming {
            // 確認中は答えだけを受ける。
            if i.key_pressed(egui::Key::Escape) {
                actions.push(EditorAction::CancelClose);
            }
            return;
        }
        if i.modifiers.command && i.key_pressed(egui::Key::S) {
            actions.push(EditorAction::Save);
        }
        if i.modifiers.command && i.key_pressed(egui::Key::Enter) {
            actions.push(EditorAction::Run);
        }
        if i.modifiers.command && i.key_pressed(egui::Key::F) {
            actions.push(EditorAction::Expand);
        }
        if i.modifiers.command && i.key_pressed(egui::Key::K) {
            actions.push(EditorAction::Compress);
        }
        if i.key_pressed(egui::Key::Escape) {
            actions.push(EditorAction::Close);
        }
    });

    top_bar(root, editor, locales, &mut actions);
    status_bar(root, editor, locales, &mut actions);
    diagnosis_panel(root, editor, locales);
    code_area(root, editor);

    actions
}

fn top_bar(
    root: &mut egui::Ui,
    editor: &mut Editor,
    locales: &Locales,
    actions: &mut Vec<EditorAction>,
) {
    egui::Panel::top("tsubu.editor.top").exact_size(52.0).show(root, |ui| {
        ui.horizontal_centered(|ui| {
            ui.add_space(8.0);

            // 入力欄は残った幅を分け合う。決め打ちの幅にすると、窓が狭いときに
            // 右のボタンへ食い込む。
            const BUTTON_AREA: f32 = 330.0;
            const LABELS: f32 = 210.0;
            let each =
                ((ui.available_width() - BUTTON_AREA - LABELS) / 4.0).clamp(60.0, 220.0);

            let field = |ui: &mut egui::Ui, label: &str, hint: &str, value: &mut String| {
                ui.label(egui::RichText::new(label).size(12.0));
                ui.add(
                    egui::TextEdit::singleline(value).desired_width(each).hint_text(hint),
                );
                ui.add_space(8.0);
            };

            field(
                ui,
                locales.t("editor.name"),
                locales.t("editor.name"),
                &mut editor.name,
            );
            field(
                ui,
                locales.t("editor.author"),
                locales.t("editor.author_hint"),
                &mut editor.author,
            );
            field(
                ui,
                locales.t("editor.link"),
                locales.t("editor.link_hint"),
                &mut editor.link,
            );
            field(
                ui,
                locales.t("editor.tags"),
                locales.t("editor.tags_hint"),
                &mut editor.tags,
            );

            // 開けるリンクのときだけボタンを出す。押せないボタンより分かりやすい。
            if tsubu_core::open::check(&editor.link).is_ok()
                && ui
                    .button(locales.t("editor.open_link"))
                    .on_hover_text(editor.link.trim())
                    .clicked()
            {
                actions.push(EditorAction::OpenLink);
            }

            if editor.is_dirty() {
                ui.label(
                    egui::RichText::new(locales.t("editor.unsaved"))
                        .size(11.0)
                        .color(Palette::of(ui).strong),
                );
            }

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_space(8.0);
                if ui.button(locales.t("editor.close")).clicked() {
                    actions.push(EditorAction::Close);
                }
                if ui.button(locales.t("editor.run")).clicked() {
                    actions.push(EditorAction::Run);
                }
                if ui.button(locales.t("editor.save")).clicked() {
                    actions.push(EditorAction::Save);
                }
                ui.add_space(10.0);
                if ui.button(locales.t("editor.compress")).clicked() {
                    actions.push(EditorAction::Compress);
                }
                if ui.button(locales.t("editor.expand")).clicked() {
                    actions.push(EditorAction::Expand);
                }
            });
        });
    });
}

/// 結果表示は下に固定して、コード欄の高さが揺れないようにする。
fn status_bar(
    root: &mut egui::Ui,
    editor: &mut Editor,
    locales: &Locales,
    actions: &mut Vec<EditorAction>,
) {
    let mut jump = None;

    egui::Panel::bottom("tsubu.editor.status").exact_size(46.0).show(root, |ui| {
        ui.horizontal_centered(|ui| {
            ui.add_space(8.0);

            // 投稿の文字数が効くので、常に出しておく。
            ui.label(
                egui::RichText::new(format!(
                    "{} {}",
                    editor.source.chars().count(),
                    locales.t("editor.chars")
                ))
                .size(11.0)
                .monospace()
                .color(Palette::of(ui).strong),
            );
            ui.add_space(12.0);

            if editor.confirming_close {
                ui.label(
                    egui::RichText::new(locales.t("editor.discard_confirm"))
                        .size(12.0)
                        .color(Palette::of(ui).error),
                );
                ui.add_space(10.0);
                if ui.button(locales.t("common.cancel")).clicked() {
                    actions.push(EditorAction::CancelClose);
                }
                if ui.button(locales.t("editor.discard")).clicked() {
                    actions.push(EditorAction::DiscardAndClose);
                }
                if ui.button(locales.t("editor.save")).clicked() {
                    actions.push(EditorAction::Save);
                }
            } else if let Some(message) = &editor.io_error {
                ui.label(egui::RichText::new(message).size(12.0).color(Palette::of(ui).error));
            } else if let Some(error) = &editor.error {
                ui.label(
                    egui::RichText::new(locales.t("editor.compile_error"))
                        .size(12.0)
                        .strong()
                        .color(Palette::of(ui).error),
                );
                // 押すとその行へ飛ぶ。長いコードでは探すのが手間になる。
                let label = egui::Label::new(
                    egui::RichText::new(error.to_string())
                        .size(12.0)
                        .monospace()
                        .color(Palette::of(ui).error)
                        .underline(),
                )
                .sense(egui::Sense::click());
                if ui.add(label).on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                    jump = Some(error.line);
                }
            } else if !editor.is_dirty() && !editor.is_new() {
                ui.label(egui::RichText::new(locales.t("editor.saved")).size(12.0).color(Palette::of(ui).ok));
            } else if editor.is_checked_ok() {
                // 保存前でも、いま書いてあるコードが通ることは分かる。
                // どちらの方言として読めたかも出す。取り違えに気付ける。
                let message = match editor.dialect {
                    Some(dialect) => {
                        format!("{} · {}", dialect.label(), locales.t("editor.no_errors"))
                    }
                    None => locales.t("editor.no_errors").to_string(),
                };
                ui.label(egui::RichText::new(message).size(12.0).color(Palette::of(ui).ok));
            } else {
                ui.label(
                    egui::RichText::new(locales.shortcut("editor.hint"))
                        .size(11.0)
                        .color(Palette::of(ui).dim),
                );
            }

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_space(8.0);
                ui.label(
                    egui::RichText::new(locales.shortcut("editor.keys"))
                        .size(11.0)
                        .color(Palette::of(ui).dim),
                );
            });
        });
    });

    if jump.is_some() {
        editor.jump_to_line = jump;
    }
}

/// 方言が違うときに、何が足りないのかを並べる。
///
/// `使えない文字です: '$'` とだけ言われても直しようがないので、原因をまとめて出す。
fn diagnosis_panel(root: &mut egui::Ui, editor: &Editor, locales: &Locales) {
    let Some(diagnosis) = &editor.diagnosis else { return };
    if editor.confirming_close {
        return;
    }

    egui::Panel::bottom("tsubu.editor.diagnosis").resizable(false).show(root, |ui| {
        ui.add_space(6.0);
        ui.label(
            egui::RichText::new(locales.t(diagnosis.dialect.locale_key()))
                .size(12.0)
                .strong()
                .color(Palette::of(ui).accent),
        );
        ui.add_space(4.0);

        for finding in &diagnosis.findings {
            ui.horizontal(|ui| {
                ui.add_space(8.0);
                ui.label(
                    egui::RichText::new(format!("{} {}", finding.line, locales.t("dialect.line")))
                        .size(11.0)
                        .monospace()
                        .color(Palette::of(ui).dim),
                );
                ui.label(
                    egui::RichText::new(locales.t(finding.key))
                        .size(11.0)
                        .color(Palette::of(ui).strong),
                );
            });
        }
        ui.add_space(6.0);
    });
}

/// 行番号つきのコード欄。
fn code_area(root: &mut egui::Ui, editor: &mut Editor) {
    let error_line = editor.error_line();
    let font = egui::FontId::monospace(CODE_SIZE);
    let id = egui::Id::new(CODE_ID);

    apply_editing_keys(root.ctx(), id, editor);

    egui::CentralPanel::default().show(root, |ui| {
        let gutter = gutter_width(ui, &font, editor.source.lines().count().max(1));

        // 折り返さないので横にもスクロールする。
        egui::ScrollArea::both().auto_shrink([false, false]).show(ui, |ui| {
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                // 行番号は本文を描いたあとに位置が分かるので、場所だけ先に取る。
                let (gutter_rect, _) = ui.allocate_exact_size(
                    egui::vec2(gutter, ui.available_height()),
                    egui::Sense::hover(),
                );

                let layout_font = font.clone();
                let output = egui::TextEdit::multiline(&mut editor.source)
                    .id(id)
                    .font(font.clone())
                    .desired_width(f32::INFINITY)
                    .desired_rows(30)
                    .layouter(&mut |ui: &egui::Ui, buffer: &dyn egui::TextBuffer, _wrap| {
                        // 折り返しは行番号とずれるので使わない。
                        let palette = Palette::of(ui);
                        ui.fonts_mut(|fonts| {
                            fonts.layout_job(layout(
                                buffer.as_str(),
                                &layout_font,
                                error_line,
                                &palette,
                            ))
                        })
                    })
                    .show(ui);

                draw_gutter(ui, gutter_rect, &output, &font, error_line);
            });
        });
    });
}

/// 編集用のキーを先に拾って、本文とカーソルを書き換える。
///
/// egui の [`egui::TextEdit`] は素の入力欄なので、字下げや行の入れ替えは
/// こちらで用意する。キーは `consume_key` で取り上げ、入力欄へは渡さない。
fn apply_editing_keys(ctx: &egui::Context, id: egui::Id, editor: &mut Editor) {
    // エラー表示を押されていたら、その行の先頭へ飛ぶ。
    if let Some(line) = editor.jump_to_line.take() {
        let at = editing::start_of_line(&editor.source, line);
        let mut state = egui::TextEdit::load_state(ctx, id).unwrap_or_default();
        state.cursor.set_char_range(Some(egui::text::CCursorRange::one(
            egui::text::CCursor::new(at),
        )));
        state.store(ctx, id);
        ctx.memory_mut(|m| m.request_focus(id));
        return;
    }

    // 入力欄を触っていないときは何もしない。
    if ctx.memory(|m| m.focused()) != Some(id) {
        return;
    }
    let Some(mut state) = egui::TextEdit::load_state(ctx, id) else { return };
    let Some(range) = state.cursor.char_range() else { return };
    let (a, b) = (range.primary.index.0, range.secondary.index.0);
    let selection: CharRange = a.min(b)..a.max(b);

    use egui::{Key, Modifiers};
    let edit = ctx.input_mut(|i| {
        let source = &editor.source;
        if i.consume_key(Modifiers::NONE, Key::Enter) {
            return Some(editing::newline_with_indent(source, selection.clone()));
        }
        if i.consume_key(Modifiers::NONE, Key::Tab) {
            return Some(editing::indent(source, selection.clone()));
        }
        if i.consume_key(Modifiers::SHIFT, Key::Tab) {
            return Some(editing::outdent(source, selection.clone()));
        }
        if i.consume_key(Modifiers::COMMAND, Key::Slash) {
            return Some(editing::toggle_comment(source, selection.clone()));
        }
        if i.consume_key(Modifiers::COMMAND, Key::D) {
            return Some(editing::duplicate_lines(source, selection.clone()));
        }
        if i.consume_key(Modifiers::ALT, Key::ArrowUp) {
            return Some(editing::move_lines(source, selection.clone(), -1));
        }
        if i.consume_key(Modifiers::ALT, Key::ArrowDown) {
            return Some(editing::move_lines(source, selection.clone(), 1));
        }
        None
    });

    let Some(edit) = edit else { return };
    editor.source = edit.text;
    state.cursor.set_char_range(Some(egui::text::CCursorRange::two(
        egui::text::CCursor::new(edit.selection.start),
        egui::text::CCursor::new(edit.selection.end),
    )));
    state.store(ctx, id);
}

/// 桁数から行番号欄の幅を決める。
fn gutter_width(ui: &egui::Ui, font: &egui::FontId, lines: usize) -> f32 {
    let digits = lines.to_string().len().max(2);
    let glyph = ui.fonts_mut(|f| f.glyph_width(font, '0'));
    glyph * digits as f32 + 18.0
}

/// 本文の行位置に合わせて番号を描く。
fn draw_gutter(
    ui: &egui::Ui,
    rect: egui::Rect,
    output: &egui::text_edit::TextEditOutput,
    font: &egui::FontId,
    error_line: Option<u32>,
) {
    let painter = ui.painter();
    let palette = Palette::of(ui);
    let mut line = 1u32;

    for row in output.galley.rows.iter() {
        let top = output.galley_pos.y + row.min_y();
        let height = row.height();

        if error_line == Some(line) {
            // 番号側にも敷いて、目で行を追えるようにする。
            painter.rect_filled(
                egui::Rect::from_min_size(
                    egui::pos2(rect.min.x, top),
                    egui::vec2(rect.width(), height),
                ),
                0.0,
                palette.error_row,
            );
        }

        painter.text(
            egui::pos2(rect.max.x - 8.0, top),
            egui::Align2::RIGHT_TOP,
            line.to_string(),
            font.clone(),
            if error_line == Some(line) { palette.error } else { palette.gutter },
        );

        // 折り返した続きの行には番号を振らない。
        if row.ends_with_newline {
            line += 1;
        }
    }
}

/// ソースを色分けした [`egui::text::LayoutJob`] にする。
fn layout(
    source: &str,
    font: &egui::FontId,
    error_line: Option<u32>,
    palette: &Palette,
) -> egui::text::LayoutJob {
    let mut job = egui::text::LayoutJob {
        // 折り返すと行番号と対応しなくなる。
        wrap: egui::text::TextWrapping { max_width: f32::INFINITY, ..Default::default() },
        ..Default::default()
    };

    let error_range = error_line.and_then(|line| line_range(source, line));

    for span in highlight::spans(source) {
        // エラー行だけ背景を敷いて、どこで転んだかを目で追えるようにする。
        let background = match error_range {
            Some((start, end)) if span.start >= start && span.end <= end => palette.error_row,
            _ => egui::Color32::TRANSPARENT,
        };

        job.append(
            &source[span.start..span.end],
            0.0,
            egui::TextFormat {
                font_id: font.clone(),
                color: palette.syntax(span.class),
                background,
                ..Default::default()
            },
        );
    }

    job
}

/// 1 始まりの行番号から、その行のバイト範囲を求める。
fn line_range(source: &str, line: u32) -> Option<(usize, usize)> {
    let mut start = 0;
    for (index, text) in source.split_inclusive('\n').enumerate() {
        let end = start + text.len();
        if index as u32 + 1 == line {
            return Some((start, end));
        }
        start = end;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// ウィンドウを開かずに 1 フレーム分の UI を組み立てる。
    ///
    /// 画面を目で確かめられない環境でも、パネルが実際に描かれているか、
    /// ショートカットが繋がっているかをここで確認できる。
    fn run(editor: &mut Editor, events: Vec<egui::Event>) -> (Vec<EditorAction>, usize) {
        let ctx = egui::Context::default();
        let locales = Locales::builtin();
        // `i.modifiers` は ModifiersChanged で更新されるので、キーの前に入れる。
        // 実機では egui-winit が同じ順で送ってくる。
        let modifiers = events
            .iter()
            .find_map(|e| match e {
                egui::Event::Key { modifiers, .. } => Some(*modifiers),
                _ => None,
            })
            .unwrap_or_default();
        let mut all = vec![egui::Event::ModifiersChanged(modifiers)];
        all.extend(events);

        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::pos2(0.0, 0.0),
                egui::vec2(1100.0, 720.0),
            )),
            events: all,
            ..Default::default()
        };

        let mut actions = Vec::new();
        let mut output = ctx.run_ui(input, |ui| actions = build(ui, editor, &locales));
        // テストではテクスチャを GPU へ送らないので、明示的に捨てる。
        output.textures_delta.clear();
        let vertices: usize = ctx
            .tessellate(output.shapes, 1.0)
            .iter()
            .map(|p| match &p.primitive {
                egui::epaint::Primitive::Mesh(mesh) => mesh.vertices.len(),
                _ => 0,
            })
            .sum();
        (actions, vertices)
    }

    fn key(key: egui::Key, command: bool) -> egui::Event {
        egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers { command, ..Default::default() },
        }
    }

    #[test]
    fn the_editor_actually_draws_something() {
        let mut editor = Editor::new_sketch("demo".into());
        let (_, vertices) = run(&mut editor, Vec::new());
        assert!(vertices > 100, "描画された頂点が少なすぎます: {vertices}");
    }

    #[test]
    fn command_s_saves_and_command_enter_runs() {
        let mut editor = Editor::new_sketch("demo".into());
        let (actions, _) = run(&mut editor, vec![key(egui::Key::S, true)]);
        assert_eq!(actions, vec![EditorAction::Save]);

        let (actions, _) = run(&mut editor, vec![key(egui::Key::Enter, true)]);
        assert_eq!(actions, vec![EditorAction::Run]);
    }

    #[test]
    fn escape_asks_to_close() {
        let mut editor = Editor::new_sketch("demo".into());
        let (actions, _) = run(&mut editor, vec![key(egui::Key::Escape, false)]);
        assert_eq!(actions, vec![EditorAction::Close]);
    }

    #[test]
    fn while_confirming_only_the_answer_is_accepted() {
        let mut editor = Editor::new_sketch("demo".into());
        editor.confirming_close = true;

        // 確認中に ⌘S を押しても保存へは進まない。
        let (actions, _) = run(&mut editor, vec![key(egui::Key::S, true)]);
        assert!(actions.is_empty(), "{actions:?}");

        let (actions, _) = run(&mut editor, vec![key(egui::Key::Escape, false)]);
        assert_eq!(actions, vec![EditorAction::CancelClose]);
    }

    #[test]
    fn a_plain_p5_sketch_compiles_and_needs_no_diagnosis() {
        let p5 = "t=0\ndraw=_⇒{background(0);circle(t++,10,5)}";
        let mut editor = Editor::new_sketch("demo".into());
        editor.source = p5.to_string();

        let error = tsubu_processing_lite::VmSketch::compile(p5, 0).err();
        assert!(error.is_none(), "p5.js は動くはず: {error:?}");
        editor.set_check_result(p5.to_string(), error, Some(tsubu_processing_lite::dialect::Dialect::P5));
        assert!(editor.diagnosis.is_none());
        assert_eq!(editor.dialect.map(|d| d.label()), Some("p5.js"), "どちらで読めたか出せる");
    }

    #[test]
    fn p5_using_something_unsupported_gets_a_diagnosis_panel() {
        // `await` はまだ持っていない。
        let p5 = "draw=_⇒{await f()}";

        let mut editor = Editor::new_sketch("demo".into());
        editor.source = p5.to_string();
        let error = tsubu_processing_lite::VmSketch::compile(p5, 0).err();
        assert!(error.is_some(), "await はまだ通らない");
        editor.set_check_result(p5.to_string(), error, None);

        let diagnosis = editor.diagnosis.as_ref().expect("方言を判定している");
        assert!(!diagnosis.findings.is_empty(), "何が足りないかを挙げている");

        let (_, vertices) = run(&mut editor, Vec::new());
        assert!(vertices > 100, "内訳のパネルごと描けている: {vertices}");
    }

    #[test]
    fn a_working_sketch_gets_no_diagnosis() {
        let mut editor = Editor::new_sketch("demo".into());
        let source = editor.source.clone();
        editor.set_check_result(source, None, None);
        assert!(editor.diagnosis.is_none(), "通るコードに口を出さない");
    }

    #[test]
    fn an_editor_with_an_error_still_draws() {
        // 色分けとエラー表示を入れても組み立てが壊れないことだけを見る。
        // 「どの行に色が付くか」は layout() 側のテストで確かめている。
        let mut broken = Editor::new_sketch("demo".into());
        broken.error = Some(tsubu_processing_lite::CompileError::new(2, 3, "`;` がありません"));
        let (_, vertices) = run(&mut broken, Vec::new());
        assert!(vertices > 100, "描画された頂点が少なすぎます: {vertices}");
    }

    // ---- 行の対応 -------------------------------------------------------

    #[test]
    fn clicking_the_error_moves_the_cursor_to_that_line() {
        let mut editor = Editor::new_sketch("demo".into());
        editor.source = "void draw() {\n  background(0)\n}".into();
        editor.jump_to_line = Some(2);

        let ctx = egui::Context::default();
        let id = egui::Id::new(CODE_ID);
        apply_editing_keys(&ctx, id, &mut editor);

        assert!(editor.jump_to_line.is_none(), "一度で使い切る");
        let state = egui::TextEdit::load_state(&ctx, id).expect("状態が入っている");
        let range = state.cursor.char_range().expect("カーソルが入っている");
        assert_eq!(range.primary.index.0, editing::start_of_line(&editor.source, 2));
    }

    #[test]
    fn line_ranges_are_one_based_and_include_the_newline() {
        let source = "a\nbb\nccc";
        assert_eq!(line_range(source, 1), Some((0, 2)));
        assert_eq!(line_range(source, 2), Some((2, 5)));
        assert_eq!(line_range(source, 3), Some((5, 8)));
        assert_eq!(line_range(source, 4), None);
    }

    #[test]
    fn line_ranges_handle_multibyte_text() {
        let source = "// つぶやき\nint x = 1;";
        let (start, end) = line_range(source, 2).expect("2 行目がある");
        assert_eq!(&source[start..end], "int x = 1;");
    }

    #[test]
    fn the_error_row_is_the_only_one_with_a_background() {
        let source = "void draw() {\n  background(0)\n}\n";
        let font = egui::FontId::monospace(CODE_SIZE);
        let job = layout(source, &font, Some(2), &Palette::for_mode(true));

        let (start, end) = line_range(source, 2).expect("2 行目がある");
        for section in &job.sections {
            let range = section.byte_range.start.0..section.byte_range.end.0;
            let highlighted = section.format.background != egui::Color32::TRANSPARENT;
            let inside = range.start >= start && range.end <= end;
            assert_eq!(highlighted, inside, "{:?}", &source[range]);
        }
    }

    #[test]
    fn no_error_means_no_background_anywhere() {
        let source = "void draw() {\n  background(0);\n}\n";
        let font = egui::FontId::monospace(CODE_SIZE);
        let job = layout(source, &font, None, &Palette::for_mode(true));
        assert!(
            job.sections.iter().all(|s| s.format.background == egui::Color32::TRANSPARENT),
            "エラーが無いのに色が敷かれている"
        );
    }

    #[test]
    fn the_layout_job_reproduces_the_source() {
        let source = include_str!("../../processing-lite/sketches/spiral.pde");
        let font = egui::FontId::monospace(CODE_SIZE);
        let job = layout(source, &font, None, &Palette::for_mode(true));
        assert_eq!(job.text, source, "色分けで文字が落ちてはいけない");
    }

    #[test]
    fn keywords_and_api_names_get_different_colors() {
        let source = "int x = width; circle(1, 2, 3);";
        let font = egui::FontId::monospace(CODE_SIZE);
        let job = layout(source, &font, None, &Palette::for_mode(true));

        // 同じ書式が続く区間は egui がまとめるので、位置で引く。
        let color_at = |needle: &str| {
            let at = source.find(needle).unwrap_or_else(|| panic!("{needle} が無い"));
            job.sections
                .iter()
                .find(|s| s.byte_range.start.0 <= at && at < s.byte_range.end.0)
                .map(|s| s.format.color)
                .unwrap_or_else(|| panic!("{needle} を含む区間が無い"))
        };

        let ident = color_at("x");
        assert_ne!(color_at("int"), ident, "型");
        assert_ne!(color_at("width"), ident, "組み込み変数");
        assert_ne!(color_at("circle"), ident, "API");
        assert_ne!(color_at("int"), color_at("circle"), "型と API は別の色");
    }
}
