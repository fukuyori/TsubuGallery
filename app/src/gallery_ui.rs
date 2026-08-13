//! Gallery 画面の描画 (設計書 §6 / §20)。
//!
//! ホーム画面はコード一覧ではなくスクリーンショットのグリッド。列数は
//! [`tsubu_gallery::grid`] が画面幅から決めるので、ここは決まった矩形へ描くだけ。
//! 絞り込みと並び替えの判断は [`GalleryView`] が持ち、ここは操作を渡すだけ。

use std::collections::HashMap;

use tsubu_core::Locales;
use crate::theme::Palette;
use tsubu_core::settings::{CardSize, ViewMode};
use tsubu_gallery::model::{Choice, Filter, SortOrder, ThumbnailState};
use tsubu_gallery::{GalleryView, grid};

/// Gallery 上でユーザーが起こしたこと。main 側が処理する。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GalleryAction {
    Open(usize),
    Select(usize),
    ToggleFavorite(usize),
    /// 削除の確認に答えた。
    ConfirmDelete,
    CancelDelete,
    /// 設定画面を開く。
    OpenSettings,
    /// コレクションへの出し入れ。`true` なら入れる。
    SetCollection(usize, String, bool),
    /// コレクションごと消す。
    DeleteCollection(String),
    /// コレクションの割り当て画面を閉じる。
    CloseCollections,
}

/// 1 フレーム描いた結果。
#[derive(Default)]
pub struct GalleryOutput {
    pub actions: Vec<GalleryAction>,
    /// 実際に使った列数。上下移動の幅になるので view-model へ返す必要がある。
    pub columns: usize,
}

pub struct GalleryUi<'a> {
    pub view: &'a mut GalleryView,
    pub textures: &'a HashMap<String, egui::TextureHandle>,
    pub locales: &'a Locales,
    /// 絞り込みに出す既知のタグ。
    pub tags: &'a [String],
    /// 既知のコレクション名 (設計書 §27)。
    pub collections: &'a [String],
    /// コレクションの割り当て中なら、その作品。
    pub assigning: Option<usize>,
    /// キーボード操作の直後だけ true。選択中のカードを画面内へ送る。
    pub scroll_to_selected: bool,
    /// 削除の確認中なら、その作品名。
    pub pending_delete: Option<String>,
    /// 作品の並べ方 (設計書 §6.2)。
    pub view_mode: ViewMode,
    /// カードの大きさ (設計書 §24)。
    pub card_size: CardSize,
    /// カードに作品名を出すか。
    pub show_titles: bool,
}

const TITLE_ROW_HEIGHT: f32 = 34.0;
/// 大型カードのタイトル行。文字を大きくする分だけ高くとる。
const TITLE_ROW_HEIGHT_LARGE: f32 = 48.0;
/// リスト 1 行の高さ。サムネイルの縦がこれから決まる。
const LIST_ROW_HEIGHT: f32 = 64.0;
const LIST_ROW_GAP: f32 = 6.0;
const LIST_PADDING: f32 = 6.0;

/// Gallery を描き、発生したアクションと使った列数を返す。
pub fn build(root: &mut egui::Ui, state: &mut GalleryUi<'_>) -> GalleryOutput {
    let mut out = GalleryOutput::default();

    egui::Panel::top("tsubu.gallery.top").exact_size(92.0).show(root, |ui| {
        ui.add_space(6.0);
        header(ui, state, &mut out.actions);
        ui.add_space(8.0);
        filter_bar(ui, state);
    });

    egui::CentralPanel::default().show(root, |ui| {
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            out.columns = grid_body(ui, state, &mut out.actions);
        });
    });

    if let Some(title) = state.pending_delete.clone() {
        delete_confirm(root.ctx(), &title, state.locales, &mut out.actions);
    } else if let Some(index) = state.assigning {
        collection_dialog(root.ctx(), state, index, &mut out.actions);
    }

    out
}

fn header(ui: &mut egui::Ui, state: &GalleryUi<'_>, actions: &mut Vec<GalleryAction>) {
    ui.horizontal(|ui| {
        ui.add_space(8.0);
        // アプリ名はローカライズしない (設計書 §2)。
        ui.label(egui::RichText::new("TsubuGallery").size(20.0).strong());
        ui.add_space(12.0);

        let total = state.view.len();
        let shown = state.view.visible_len();
        let count = if shown == total { total.to_string() } else { format!("{shown} / {total}") };
        ui.label(egui::RichText::new(count).size(13.0).color(Palette::of(ui).dim));

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.add_space(8.0);
            if ui
                .button(egui::RichText::new(state.locales.t("gallery.settings")).size(12.0))
                .clicked()
            {
                actions.push(GalleryAction::OpenSettings);
            }
            ui.add_space(12.0);
            hints(ui, state.locales);
        });
    });
}

fn hints(ui: &mut egui::Ui, locales: &Locales) {
    let palette = Palette::of(ui);
    let (dim, key) = (palette.dim, palette.strong);
    for (k, label) in [
        ("Esc", locales.t("gallery.quit")),
        ("Del", locales.t("gallery.delete")),
        ("E", locales.t("gallery.edit")),
        ("N", locales.t("gallery.new")),
        ("C", locales.t("gallery.collection")),
        ("P", locales.t("gallery.slideshow")),
        ("V", locales.t("gallery.view_mode")),
        ("S", locales.t("gallery.favorite")),
        (locales.t("gallery.double_click"), locales.t("gallery.open")),
        ("↑↓←→", locales.t("gallery.navigate")),
    ] {
        ui.label(egui::RichText::new(label).size(11.0).color(dim));
        ui.add_space(4.0);
        ui.label(egui::RichText::new(k).size(11.0).strong().color(key));
        ui.add_space(12.0);
    }
}

/// 絞り込みと並び替え (設計書 §20)。
fn filter_bar(ui: &mut egui::Ui, state: &mut GalleryUi<'_>) {
    let mut filter = state.view.filter().clone();
    let mut sort = state.view.sort();

    ui.horizontal(|ui| {
        ui.add_space(8.0);
        ui.add(
            egui::TextEdit::singleline(&mut filter.text)
                .desired_width(200.0)
                .hint_text(state.locales.t("gallery.search")),
        );

        ui.toggle_value(&mut filter.favorites_only, state.locales.t("gallery.favorites_only"));
        ui.toggle_value(&mut filter.errors_only, state.locales.t("gallery.errors_only"));

        // タグ (設計書 §19.2 / §20)。付いているタグが無ければ出さない。
        if !state.tags.is_empty() {
            let current =
                filter.tag.clone().unwrap_or_else(|| state.locales.t("gallery.all_tags").into());
            egui::ComboBox::from_id_salt("tsubu.gallery.tag").selected_text(current).show_ui(
                ui,
                |ui| {
                    ui.selectable_value(&mut filter.tag, None, state.locales.t("gallery.all_tags"));
                    for tag in state.tags {
                        ui.selectable_value(&mut filter.tag, Some(tag.clone()), tag);
                    }
                },
            );
        }

        // コレクション (設計書 §27)。1 つも無ければ出さない。
        if !state.collections.is_empty() {
            let current = filter
                .collection
                .clone()
                .unwrap_or_else(|| state.locales.t("gallery.all_collections").into());
            egui::ComboBox::from_id_salt("tsubu.gallery.collection")
                .selected_text(current)
                .show_ui(ui, |ui| {
                    ui.selectable_value(
                        &mut filter.collection,
                        None,
                        state.locales.t("gallery.all_collections"),
                    );
                    for name in state.collections {
                        ui.selectable_value(&mut filter.collection, Some(name.clone()), name);
                    }
                });
        }

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.add_space(8.0);
            egui::ComboBox::from_id_salt("tsubu.gallery.sort")
                .selected_text(state.locales.t(&sort.locale_key()).to_string())
                .show_ui(ui, |ui| {
                    for &order in SortOrder::ALL {
                        let label = state.locales.t(&order.locale_key()).to_string();
                        ui.selectable_value(&mut sort, order, label);
                    }
                });

            if !filter.is_empty() && ui.button(state.locales.t("gallery.clear_filter")).clicked() {
                filter = Filter::default();
            }
        });
    });

    state.view.set_filter(filter);
    state.view.set_sort(sort);
}

/// 一覧本体を描き、使った列数を返す。
///
/// 列数は上下キーの移動幅になるので、どの表示方式でも正しく返す必要がある
/// (リストは 1 列)。
fn grid_body(ui: &mut egui::Ui, state: &GalleryUi<'_>, actions: &mut Vec<GalleryAction>) -> usize {
    let visible = state.view.visible().to_vec();

    if visible.is_empty() {
        // 作品が無いのか、絞り込みで消えているのかを区別する。
        let message = if state.view.is_empty() {
            state.locales.t("gallery.empty")
        } else {
            state.locales.t("gallery.no_matches")
        };
        ui.centered_and_justified(|ui| {
            ui.label(egui::RichText::new(message).size(14.0).color(Palette::of(ui).dim));
        });
        return 1;
    }

    match state.view_mode {
        ViewMode::Grid => cards(ui, state, actions, &visible, false),
        ViewMode::LargeCard => cards(ui, state, actions, &visible, true),
        ViewMode::List => {
            list(ui, state, actions, &visible);
            1
        }
    }
}

/// カードを敷き詰める。グリッドと大型カードの違いは 1 枚の大きさだけ。
fn cards(
    ui: &mut egui::Ui,
    state: &GalleryUi<'_>,
    actions: &mut Vec<GalleryAction>,
    visible: &[usize],
    large: bool,
) -> usize {
    let scale = state.card_size.scale();
    let metrics = if large {
        grid::layout_large(ui.available_width(), scale)
    } else {
        grid::layout_scaled(ui.available_width(), scale)
    };

    // 作品名を出さないときは行そのものを畳む。空白だけ残ると間延びする。
    // 大型カードでは文字も大きくするので、行も高くとる。
    let title_row = match (state.show_titles, large) {
        (false, _) => 0.0,
        (true, false) => TITLE_ROW_HEIGHT,
        (true, true) => TITLE_ROW_HEIGHT_LARGE,
    };
    let card_height = metrics.thumbnail_height + title_row;

    ui.add_space(grid::SPACING);

    for row in visible.chunks(metrics.columns) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = grid::SPACING;
            for &index in row {
                let size = egui::vec2(metrics.card_width, card_height);
                let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());

                if state.scroll_to_selected && Some(index) == state.view.selected() {
                    ui.scroll_to_rect(rect, None);
                }
                // 選ぶのは意図した操作にする。マウスを通り過ぎただけで
                // 選択が動くと、編集や削除の対象が定まらない。
                if response.clicked() {
                    actions.push(GalleryAction::Select(index));
                }
                if response.double_clicked() {
                    actions.push(GalleryAction::Open(index));
                }

                if ui.is_rect_visible(rect)
                    && draw_card(
                        ui,
                        rect,
                        state,
                        index,
                        &response,
                        CardLayout {
                            thumbnail_height: metrics.thumbnail_height,
                            title_row,
                            large,
                        },
                    )
                {
                    actions.push(GalleryAction::ToggleFavorite(index));
                }
            }
        });
        ui.add_space(grid::SPACING);
    }

    metrics.columns
}

/// 1 行 1 作品。作品が増えたときに名前で探せるのが狙い (設計書 §6.2)。
fn list(
    ui: &mut egui::Ui,
    state: &GalleryUi<'_>,
    actions: &mut Vec<GalleryAction>,
    visible: &[usize],
) {
    ui.add_space(grid::SPACING * 0.5);

    for &index in visible {
        let width = ui.available_width();
        let (rect, response) =
            ui.allocate_exact_size(egui::vec2(width, LIST_ROW_HEIGHT), egui::Sense::click());

        if state.scroll_to_selected && Some(index) == state.view.selected() {
            ui.scroll_to_rect(rect, None);
        }
        if response.clicked() {
            actions.push(GalleryAction::Select(index));
        }
        if response.double_clicked() {
            actions.push(GalleryAction::Open(index));
        }

        if ui.is_rect_visible(rect) && draw_list_row(ui, rect, state, index, &response) {
            actions.push(GalleryAction::ToggleFavorite(index));
        }
        ui.add_space(LIST_ROW_GAP);
    }

    ui.add_space(grid::SPACING);
}

/// 一覧 1 行を描く。お気に入り星が押されたら `true`。
fn draw_list_row(
    ui: &mut egui::Ui,
    rect: egui::Rect,
    state: &GalleryUi<'_>,
    index: usize,
    response: &egui::Response,
) -> bool {
    let Some(item) = state.view.item(index) else { return false };
    let selected = Some(index) == state.view.selected();
    let palette = Palette::of(ui);
    let painter = ui.painter();

    let bg = if response.hovered() { palette.card_bg_hover } else { palette.card_bg };
    painter.rect_filled(rect, 8.0, bg);

    // 左端にサムネイル。縦横比はグリッドと同じにして、同じ絵に見えるようにする。
    let thumb_height = rect.height() - LIST_PADDING * 2.0;
    let thumb_rect = egui::Rect::from_min_size(
        egui::pos2(rect.min.x + LIST_PADDING, rect.min.y + LIST_PADDING),
        egui::vec2(thumb_height * grid::THUMBNAIL_ASPECT, thumb_height),
    );
    draw_thumbnail(painter, thumb_rect, state, item, &palette, 6.0);

    // 星の分だけ右を空ける。
    let text_left = thumb_rect.max.x + 14.0;
    let text_right = rect.max.x - 44.0;
    let text_width = (text_right - text_left).max(1.0);

    // 2 行目に方言・状態・タグ。グリッドではサムネイルへ重ねている情報を、
    // ここでは重ねずに置ける。作品名と同じになるだけの id は出さない。
    let mut detail: Vec<String> = Vec::new();
    if item.status.is_error() {
        detail.push(state.locales.t("gallery.compile_error").to_string());
    } else if let Some(dialect) = &item.dialect {
        detail.push(dialect.clone());
    }
    if !item.tags.is_empty() {
        detail.push(item.tags.iter().cloned().collect::<Vec<_>>().join(", "));
    }
    if item.id != item.title {
        detail.push(item.id.clone());
    }
    let detail = detail.join("  ·  ");

    // 出すものが無ければ作品名を行の中央に置く。空行の分だけ上に寄ると落ち着かない。
    let title_y = if detail.is_empty() { rect.center().y } else { rect.center().y - 9.0 };
    painter.text(
        egui::pos2(text_left, title_y),
        egui::Align2::LEFT_CENTER,
        elide(&item.title, text_width),
        egui::FontId::proportional(14.0),
        palette.card_text,
    );

    if !detail.is_empty() {
        painter.text(
            egui::pos2(text_left, rect.center().y + 11.0),
            egui::Align2::LEFT_CENTER,
            elide(&detail, text_width),
            egui::FontId::proportional(11.0),
            if item.status.is_error() { palette.error } else { palette.dim },
        );
    }

    let star = draw_star(ui, egui::pos2(rect.max.x - 22.0, rect.center().y), index, item, &palette);

    if selected {
        ui.painter().rect_stroke(
            rect,
            8.0,
            egui::Stroke::new(2.0, palette.accent),
            egui::StrokeKind::Inside,
        );
    }

    star
}

/// カード 1 枚の寸法。
#[derive(Clone, Copy)]
struct CardLayout {
    thumbnail_height: f32,
    /// タイトル行の高さ。0 なら行ごと出さない。
    title_row: f32,
    /// 大型カードか。文字の大きさが変わる。
    large: bool,
}

/// カード 1 枚を描く。お気に入り星が押されたら `true`。
fn draw_card(
    ui: &mut egui::Ui,
    rect: egui::Rect,
    state: &GalleryUi<'_>,
    index: usize,
    response: &egui::Response,
    layout: CardLayout,
) -> bool {
    let CardLayout { thumbnail_height, title_row, large } = layout;
    let Some(item) = state.view.item(index) else { return false };
    let selected = Some(index) == state.view.selected();
    let painter = ui.painter();

    let palette = Palette::of(ui);
    let bg = if response.hovered() { palette.card_bg_hover } else { palette.card_bg };
    painter.rect_filled(rect, 10.0, bg);

    let thumb_rect =
        egui::Rect::from_min_size(rect.min, egui::vec2(rect.width(), thumbnail_height));
    draw_thumbnail(painter, thumb_rect, state, item, &palette, 10.0);

    // 大型カードは文字も大きくする。小さい字のまま枠だけ広げても意味がない。
    let (title_size, sub_size) = if large { (18.0, 13.0) } else { (13.0, 11.0) };

    // タイトル行。
    if title_row > 0.0 {
        let title_rect = egui::Rect::from_min_max(
            egui::pos2(rect.min.x + 10.0, thumb_rect.max.y),
            egui::pos2(rect.max.x - 34.0, rect.max.y),
        );
        painter.text(
            title_rect.left_center(),
            egui::Align2::LEFT_CENTER,
            elide(&item.title, title_rect.width()),
            egui::FontId::proportional(title_size),
            palette.card_text,
        );
    }

    if item.status.is_error() {
        painter.text(
            egui::pos2(thumb_rect.min.x + 10.0, thumb_rect.min.y + 10.0),
            egui::Align2::LEFT_TOP,
            state.locales.t("gallery.compile_error"),
            egui::FontId::proportional(sub_size),
            palette.error,
        );
    }

    // タグは右下に小さく出す。多いときは先頭だけ (設計書 §6.1)。
    if let Some(tag) = item.tags.iter().next() {
        let more = item.tags.len() - 1;
        let label = if more > 0 { format!("{tag} +{more}") } else { tag.clone() };
        painter.text(
            egui::pos2(thumb_rect.max.x - 10.0, thumb_rect.max.y - 8.0),
            egui::Align2::RIGHT_BOTTOM,
            label,
            egui::FontId::proportional(sub_size),
            palette.dim,
        );
    }

    // お気に入り。カード本体とは別に当たり判定を持つ。
    // タイトル行があればその中、無ければサムネイルの右下へ重ねる。
    let star_y =
        if title_row > 0.0 { rect.max.y - title_row * 0.5 } else { rect.max.y - 20.0 };
    let star = draw_star(ui, egui::pos2(rect.max.x - 20.0, star_y), index, item, &palette);

    if selected {
        ui.painter().rect_stroke(
            rect,
            10.0,
            egui::Stroke::new(2.0, palette.accent),
            egui::StrokeKind::Inside,
        );
    }

    star
}

/// サムネイルか、その代わりの枠を描く。
fn draw_thumbnail(
    painter: &egui::Painter,
    rect: egui::Rect,
    state: &GalleryUi<'_>,
    item: &tsubu_gallery::GalleryItem,
    palette: &Palette,
    rounding: f32,
) {
    match (&item.thumbnail, state.textures.get(&item.id)) {
        (ThumbnailState::Ready, Some(texture)) => {
            painter.image(
                texture.id(),
                rect,
                egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                egui::Color32::WHITE,
            );
        }
        (ThumbnailState::Failed(_), _) => placeholder(painter, rect, "!", palette.error, rounding),
        // 生成待ちのあいだも枠だけ見せて、レイアウトが飛ばないようにする (§22)。
        _ => placeholder(painter, rect, "…", palette.dim, rounding),
    }
}

/// お気に入りの星。押されたら `true`。
///
/// カード本体とは別に当たり判定を持たせて、星だけを押せるようにする。
fn draw_star(
    ui: &mut egui::Ui,
    center: egui::Pos2,
    index: usize,
    item: &tsubu_gallery::GalleryItem,
    palette: &Palette,
) -> bool {
    let rect = egui::Rect::from_center_size(center, egui::vec2(26.0, 26.0));
    let id = ui.id().with(("tsubu.gallery.star", index));
    let response = ui.interact(rect, id, egui::Sense::click());
    let color = if item.favorite {
        egui::Color32::from_rgb(255, 200, 90)
    } else if response.hovered() {
        palette.strong
    } else {
        palette.dim
    };
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        if item.favorite { "★" } else { "☆" },
        egui::FontId::proportional(16.0),
        color,
    );
    response.clicked()
}

/// 作品の絵が置かれる場所。絵が来るまでの間も同じ枠を出しておく。
fn placeholder(
    painter: &egui::Painter,
    rect: egui::Rect,
    glyph: &str,
    color: egui::Color32,
    rounding: f32,
) {
    painter.rect_filled(rect, rounding, egui::Color32::from_rgb(18, 18, 21));
    painter.text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        glyph,
        egui::FontId::proportional(20.0),
        color,
    );
}

/// コレクションへの出し入れ (設計書 §27)。
///
/// 作品を選んで `C`。チェックを付け外しするだけで、その場で効かせる。
fn collection_dialog(
    ctx: &egui::Context,
    state: &GalleryUi<'_>,
    index: usize,
    actions: &mut Vec<GalleryAction>,
) {
    let Some(item) = state.view.item(index) else { return };
    let locales = state.locales;

    egui::Area::new("tsubu.gallery.collections.shade".into())
        .order(egui::Order::Foreground)
        .fixed_pos(egui::pos2(0.0, 0.0))
        .show(ctx, |ui| {
            let screen = ctx.viewport_rect();
            ui.allocate_response(screen.size(), egui::Sense::click());
            ui.painter().rect_filled(screen, 0.0, egui::Color32::from_black_alpha(160));
        });

    egui::Area::new("tsubu.gallery.collections".into())
        .order(egui::Order::Foreground)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            egui::Frame::popup(ui.style()).inner_margin(20.0).show(ui, |ui| {
                ui.set_min_width(320.0);
                ui.label(egui::RichText::new(locales.t("gallery.collections")).size(15.0).strong());
                ui.label(
                    egui::RichText::new(&item.title).size(12.0).color(Palette::of(ui).dim),
                );
                ui.add_space(10.0);

                for name in state.collections {
                    ui.horizontal(|ui| {
                        let mut member = item.collections.contains(name);
                        if ui.checkbox(&mut member, name).changed() {
                            actions.push(GalleryAction::SetCollection(
                                index,
                                name.clone(),
                                member,
                            ));
                        }
                        ui.with_layout(
                            egui::Layout::right_to_left(egui::Align::Center),
                            |ui| {
                                if ui
                                    .button(locales.t("gallery.delete_collection"))
                                    .on_hover_text(locales.t("gallery.delete_collection.hint"))
                                    .clicked()
                                {
                                    actions
                                        .push(GalleryAction::DeleteCollection(name.clone()));
                                }
                            },
                        );
                    });
                }

                if state.collections.is_empty() {
                    ui.label(
                        egui::RichText::new(locales.t("gallery.no_collections"))
                            .size(12.0)
                            .color(Palette::of(ui).dim),
                    );
                }

                ui.add_space(10.0);
                ui.separator();
                ui.add_space(6.0);

                // 新しいコレクションを作る。名前は 1 フレームだけ持てばよいので
                // egui の一時記憶に置く。
                let id = egui::Id::new("tsubu.gallery.collections.new");
                let mut name: String = ui.data(|d| d.get_temp(id).unwrap_or_default());
                ui.horizontal(|ui| {
                    let entry = ui.add(
                        egui::TextEdit::singleline(&mut name)
                            .desired_width(200.0)
                            .hint_text(locales.t("gallery.new_collection")),
                    );
                    let submitted =
                        entry.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                    let clicked = ui.button(locales.t("gallery.add")).clicked();
                    if (submitted || clicked) && !name.trim().is_empty() {
                        actions.push(GalleryAction::SetCollection(
                            index,
                            name.trim().to_string(),
                            true,
                        ));
                        name.clear();
                    }
                });
                ui.data_mut(|d| d.insert_temp(id, name));

                ui.add_space(12.0);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button(locales.t("gallery.close")).clicked() {
                        actions.push(GalleryAction::CloseCollections);
                    }
                });
            });
        });
}

/// 削除の確認。取り消せない操作なので必ず一度止める。
fn delete_confirm(
    ctx: &egui::Context,
    title: &str,
    locales: &Locales,
    actions: &mut Vec<GalleryAction>,
) {
    // 背面のカードを触れないよう、画面全体を覆う。
    egui::Area::new("tsubu.gallery.delete.shade".into())
        .anchor(egui::Align2::LEFT_TOP, [0.0, 0.0])
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            let screen = ui.ctx().viewport_rect();
            ui.allocate_response(screen.size(), egui::Sense::click());
            ui.painter().rect_filled(screen, 0.0, egui::Color32::from_black_alpha(160));
        });

    egui::Area::new("tsubu.gallery.delete".into())
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            egui::Frame::new()
                .fill(egui::Color32::from_rgb(32, 32, 38))
                .inner_margin(egui::Margin::symmetric(22, 18))
                .corner_radius(10.0)
                .show(ui, |ui| {
                    ui.set_max_width(420.0);
                    ui.label(
                        egui::RichText::new(locales.t("gallery.delete_confirm"))
                            .size(15.0)
                            .strong(),
                    );
                    ui.add_space(6.0);
                    ui.label(egui::RichText::new(title).size(14.0).monospace());
                    ui.add_space(4.0);
                    ui.label(
                        egui::RichText::new(locales.t("gallery.delete_note"))
                            .size(12.0)
                            .color(egui::Color32::from_white_alpha(140)),
                    );
                    ui.add_space(14.0);
                    ui.horizontal(|ui| {
                        if ui.button(locales.t("common.cancel")).clicked() {
                            actions.push(GalleryAction::CancelDelete);
                        }
                        let delete = egui::Button::new(
                            egui::RichText::new(locales.t("gallery.delete"))
                                .color(egui::Color32::WHITE),
                        )
                        .fill(egui::Color32::from_rgb(170, 50, 60));
                        if ui.add(delete).clicked() {
                            actions.push(GalleryAction::ConfirmDelete);
                        }
                    });
                });
        });
}

/// 幅に収まらないタイトルを末尾省略する。
///
/// 翻訳とユーザーデータで文字幅が大きく変わるため、文字数ではなく概算幅で切る
/// (設計書 §11.3)。
fn elide(title: &str, width: f32) -> String {
    // 比例フォント 13px のおおよその平均字幅。CJK は約 2 倍。
    let budget = width / 7.0;
    let mut used = 0.0;
    let mut out = String::new();
    for ch in title.chars() {
        let w = if ch.is_ascii() { 1.0 } else { 2.0 };
        if used + w > budget {
            out.push('…');
            return out;
        }
        used += w;
        out.push(ch);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use tsubu_gallery::GalleryItem;

    /// ウィンドウを開かずに Gallery を描く。
    ///
    /// Area の配置は 1 フレーム目には確定しないので、2 フレーム回して
    /// 落ち着いた状態を見る。
    fn run(view: &mut GalleryView, pending_delete: Option<&str>) -> (GalleryOutput, usize) {
        run_in(view, pending_delete, ViewMode::Grid)
    }

    /// 表示方式を指定して描く。
    pub(super) fn run_in(
        view: &mut GalleryView,
        pending_delete: Option<&str>,
        view_mode: ViewMode,
    ) -> (GalleryOutput, usize) {
        run_full(view, pending_delete, view_mode, &[], None)
    }

    /// コレクションと割り当て画面まで含めて描く。
    pub(super) fn run_full(
        view: &mut GalleryView,
        pending_delete: Option<&str>,
        view_mode: ViewMode,
        collections: &[String],
        assigning: Option<usize>,
    ) -> (GalleryOutput, usize) {
        let ctx = egui::Context::default();
        let locales = Locales::builtin();
        let textures = HashMap::new();
        let tags: Vec<String> = Vec::new();
        let mut out = GalleryOutput::default();
        let mut vertices = 0;

        for _ in 0..2 {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::pos2(0.0, 0.0),
                    egui::vec2(1100.0, 720.0),
                )),
                ..Default::default()
            };
            let mut output = ctx.run_ui(input, |ui| {
                out = build(
                    ui,
                    &mut GalleryUi {
                        view,
                        textures: &textures,
                        locales: &locales,
                        tags: &tags,
                        collections,
                        assigning,
                        scroll_to_selected: false,
                        pending_delete: pending_delete.map(str::to_owned),
                        view_mode,
                        card_size: CardSize::default(),
                        show_titles: true,
                    },
                );
            });
            // テストではテクスチャを GPU へ送らないので、明示的に捨てる。
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

        (out, vertices)
    }

    fn view(count: usize) -> GalleryView {
        GalleryView::new(
            (0..count).map(|i| GalleryItem::new(format!("id{i}"), format!("Title {i}"))).collect(),
        )
    }

    #[test]
    fn the_grid_reports_the_columns_it_used() {
        let (out, vertices) = run(&mut view(6), None);
        assert!(out.columns >= 4, "デスクトップ幅なら 4 列以上: {}", out.columns);
        assert!(vertices > 100, "描画された頂点が少なすぎます: {vertices}");
    }

    /// クリックとダブルクリックを合成して 1 フレーム流す。
    fn click_card(view: &mut GalleryView, position: egui::Pos2, count: usize) -> Vec<GalleryAction> {
        let ctx = egui::Context::default();
        let locales = Locales::builtin();
        let textures = HashMap::new();
        let tags: Vec<String> = Vec::new();
        let mut actions = Vec::new();

        for frame in 0..3 {
            let mut events = vec![egui::Event::PointerMoved(position)];
            if frame == 1 {
                for _ in 0..count {
                    events.push(egui::Event::PointerButton {
                        pos: position,
                        button: egui::PointerButton::Primary,
                        pressed: true,
                        modifiers: Default::default(),
                    });
                    events.push(egui::Event::PointerButton {
                        pos: position,
                        button: egui::PointerButton::Primary,
                        pressed: false,
                        modifiers: Default::default(),
                    });
                }
            }
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::pos2(0.0, 0.0),
                    egui::vec2(1100.0, 720.0),
                )),
                events,
                ..Default::default()
            };
            let mut output = ctx.run_ui(input, |ui| {
                let out = build(
                    ui,
                    &mut GalleryUi {
                        view,
                        textures: &textures,
                        locales: &locales,
                        tags: &tags,
                        collections: &[],
                        assigning: None,
                        scroll_to_selected: false,
                        pending_delete: None,
                        view_mode: ViewMode::default(),
                        card_size: CardSize::default(),
                        show_titles: true,
                    },
                );
                if frame == 1 {
                    actions = out.actions;
                }
            });
            output.textures_delta.clear();
            let _ = ctx.tessellate(output.shapes, 1.0);
        }
        actions
    }

    /// 1 枚目のカードの中心あたり。ヘッダと絞り込みバーの下。
    const FIRST_CARD: egui::Pos2 = egui::pos2(120.0, 200.0);

    #[test]
    fn a_single_click_selects_without_opening() {
        let mut v = view(6);
        let actions = click_card(&mut v, FIRST_CARD, 1);
        assert!(actions.contains(&GalleryAction::Select(0)), "{actions:?}");
        assert!(
            !actions.iter().any(|a| matches!(a, GalleryAction::Open(_))),
            "1 回のクリックで開いてはいけない: {actions:?}"
        );
    }

    #[test]
    fn hovering_does_not_change_the_selection() {
        let mut v = view(6);
        // 押さずに通り過ぎるだけ。
        let actions = click_card(&mut v, FIRST_CARD, 0);
        assert!(actions.is_empty(), "触っただけで何か起きている: {actions:?}");
    }

    #[test]
    fn a_double_click_opens() {
        let mut v = view(6);
        let actions = click_card(&mut v, FIRST_CARD, 2);
        assert!(actions.contains(&GalleryAction::Open(0)), "{actions:?}");
    }

    #[test]
    fn an_empty_gallery_still_renders() {
        let (out, vertices) = run(&mut view(0), None);
        assert!(vertices > 0);
        assert!(out.actions.is_empty());
    }

    #[test]
    fn a_gallery_hidden_by_the_filter_still_renders() {
        let mut v = view(3);
        v.set_filter(Filter { favorites_only: true, ..Default::default() });
        let (_, vertices) = run(&mut v, None);
        assert!(vertices > 0, "「該当なし」の表示が出る");
    }

    #[test]
    fn the_delete_confirmation_adds_an_overlay() {
        let (_, without) = run(&mut view(6), None);
        let (_, with) = run(&mut view(6), Some("Title 0"));
        assert!(with > without, "確認ダイアログの分だけ描画が増える");
    }

    #[test]
    fn short_titles_are_untouched() {
        assert_eq!(elide("Spiral", 200.0), "Spiral");
    }

    #[test]
    fn long_titles_get_an_ellipsis() {
        let out = elide("A very long sketch title that will not fit", 100.0);
        assert!(out.ends_with('…'));
        assert!(out.chars().count() < 42);
    }

    #[test]
    fn cjk_counts_as_double_width() {
        let narrow = elide("あいうえおかきくけこ", 70.0);
        assert!(narrow.chars().count() <= 6, "got {narrow:?}");
    }
}

#[cfg(test)]
mod view_mode_tests {
    use super::tests::*;
    use super::*;
    use tsubu_gallery::GalleryItem;

    fn view_of(n: usize) -> GalleryView {
        GalleryView::new(
            (0..n)
                .map(|i| GalleryItem::new(format!("sketch-{i}"), format!("Sketch {i}")))
                .collect(),
        )
    }

    /// どの表示方式でも描けて、上下移動に使う列数が返ること。
    #[test]
    fn every_view_mode_draws() {
        for mode in <ViewMode as Choice>::ALL {
            let mut view = view_of(12);
            let (out, vertices) = run_in(&mut view, None, *mode);
            assert!(vertices > 500, "{mode:?} が描けていません: {vertices} 頂点");
            assert!(out.columns >= 1, "{mode:?} の列数が 0 です");
        }
    }

    /// リストは 1 行 1 作品。上下キーが 1 件ずつ動く前提になっている。
    #[test]
    fn the_list_is_one_column() {
        let mut view = view_of(12);
        let (out, _) = run_in(&mut view, None, ViewMode::List);
        assert_eq!(out.columns, 1);
    }

    /// 大型カードはグリッドより列が少ない。同じ列数なら切り替える意味がない。
    #[test]
    fn large_cards_use_fewer_columns_than_the_grid() {
        let mut view = view_of(12);
        let (grid, _) = run_in(&mut view, None, ViewMode::Grid);
        let (large, _) = run_in(&mut view, None, ViewMode::LargeCard);
        assert!(
            large.columns < grid.columns,
            "大型 {} 列 / グリッド {} 列",
            large.columns,
            grid.columns
        );
    }

    /// 表示方式を変えても選択は動かない。見え方を変えただけで対象が変わると困る。
    #[test]
    fn switching_the_view_keeps_the_selection() {
        let mut view = view_of(12);
        view.select(7);
        for mode in <ViewMode as Choice>::ALL {
            run_in(&mut view, None, *mode);
            assert_eq!(view.selected(), Some(7), "{mode:?} で選択が動きました");
        }
    }

    /// コレクションの割り当て画面が描けること (設計書 §27)。
    #[test]
    fn the_collection_dialog_draws() {
        let mut view = view_of(4);
        let collections = vec!["線もの".to_string(), "点もの".to_string()];

        let (plain, without) = run_full(&mut view, None, ViewMode::Grid, &collections, None);
        let (out, with) = run_full(&mut view, None, ViewMode::Grid, &collections, Some(0));

        assert!(with > without, "割り当て画面が出ていません: {without} → {with}");
        assert!(plain.actions.is_empty() && out.actions.is_empty(), "触っていないのに操作が出ています");
    }

    /// コレクションが 1 つも無くても割り当て画面は開ける。
    ///
    /// 最初の 1 つはここから作るので、開けないと詰む。
    #[test]
    fn the_dialog_opens_with_no_collections_yet() {
        let mut view = view_of(2);
        let (out, vertices) = run_full(&mut view, None, ViewMode::Grid, &[], Some(0));
        assert!(vertices > 500);
        assert!(out.actions.is_empty());
    }

    /// 消えた作品を指したまま開いても落ちないこと。
    #[test]
    fn the_dialog_survives_a_stale_index() {
        let mut view = view_of(2);
        let (out, _) = run_full(&mut view, None, ViewMode::Grid, &[], Some(99));
        assert!(out.actions.is_empty());
    }

    /// 作品が 0 件でも落ちないこと。
    #[test]
    fn an_empty_gallery_draws_in_every_view_mode() {
        for mode in <ViewMode as Choice>::ALL {
            let mut view = view_of(0);
            let (out, _) = run_in(&mut view, None, *mode);
            assert_eq!(out.columns, 1, "{mode:?}");
        }
    }
}
