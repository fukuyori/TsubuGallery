//! グリッドのレイアウト計算 (設計書 §6.3)。
//!
//! 列数は固定値ではなくカードの最小幅から決める。結果として画面幅に応じて
//! 設計書の想定どおりの列数になる。
//!
//! ```text
//! Phone       2 columns
//! Tablet      3〜4 columns
//! Desktop     4〜8 columns
//! Wide Screen 6〜10 columns
//! ```

/// カードの最小幅 (論理ピクセル)。画面クラスごとに変える。
///
/// 単一の値では「Phone 2 列」と「Desktop でカードが小さくなりすぎない」を
/// 両立できないため、幅の帯ごとに基準を切り替える。
pub const MIN_CARD_WIDTH_PHONE: f32 = 168.0;
pub const MIN_CARD_WIDTH_TABLET: f32 = 200.0;
pub const MIN_CARD_WIDTH_DESKTOP: f32 = 240.0;

/// 画面幅から最小カード幅を選ぶ。
pub fn min_card_width_for(available_width: f32) -> f32 {
    if available_width < 600.0 {
        MIN_CARD_WIDTH_PHONE
    } else if available_width < 1000.0 {
        MIN_CARD_WIDTH_TABLET
    } else {
        MIN_CARD_WIDTH_DESKTOP
    }
}

/// カード間の余白。
pub const SPACING: f32 = 14.0;
/// 大画面でカードが巨大化しないための上限 (設計書の Wide Screen 6〜10 列)。
pub const MAX_COLUMNS: usize = 10;
/// サムネイルの縦横比 (16:10)。
pub const THUMBNAIL_ASPECT: f32 = 16.0 / 10.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GridMetrics {
    pub columns: usize,
    pub card_width: f32,
    /// サムネイル部分の高さ。タイトル行はこれに加算される。
    pub thumbnail_height: f32,
}

/// 使える幅から列数とカード幅を決める。
pub fn layout(available_width: f32) -> GridMetrics {
    layout_scaled(available_width, 1.0)
}

/// カードの大きさ設定を効かせた版 (設計書 §24 の Card Size)。
///
/// `scale` は最小カード幅への倍率。大きくすると列が減り、1 枚が大きくなる。
pub fn layout_scaled(available_width: f32, scale: f32) -> GridMetrics {
    // 画面クラスの判定は倍率をかける前の幅で行う。Phone を「大」にしても
    // 1 列に潰れてしまわないよう、下限も置く。
    let min_width = (min_card_width_for(available_width) * scale.max(0.1)).max(80.0);
    layout_with(available_width, min_width, SPACING, MAX_COLUMNS)
}

/// 大型カードの最小幅。1 作品を大きく見せるのが目的なので画面クラスで変えない。
///
/// 大きくしすぎると 1 列になって 1 作品で画面が埋まり、眺めるどころか
/// スクロールしないと次が見えない。ノート PC 幅で 2 列に収まる値にしてある。
pub const MIN_CARD_WIDTH_LARGE: f32 = 420.0;
/// 大型カードの最大列数 (設計書 §6.2 の「大型カード」)。
pub const MAX_COLUMNS_LARGE: usize = 3;

/// 大型カード表示のレイアウト。
///
/// グリッドと違って画面幅から段階的に列を増やさず、広いときだけ 2〜3 列にする。
pub fn layout_large(available_width: f32, scale: f32) -> GridMetrics {
    let min_width = (MIN_CARD_WIDTH_LARGE * scale.max(0.1)).max(240.0);
    layout_with(available_width, min_width, SPACING, MAX_COLUMNS_LARGE)
}

pub fn layout_with(
    available_width: f32,
    min_card_width: f32,
    spacing: f32,
    max_columns: usize,
) -> GridMetrics {
    let width = available_width.max(1.0);

    // n 列に必要な幅は n*card + (n-1)*spacing。これを解いて最大の n を求める。
    let columns = (((width + spacing) / (min_card_width + spacing)).floor() as usize)
        .clamp(1, max_columns.max(1));

    let card_width = ((width - spacing * (columns.saturating_sub(1)) as f32) / columns as f32)
        .max(1.0);

    GridMetrics {
        columns,
        card_width,
        thumbnail_height: (card_width / THUMBNAIL_ASPECT).max(1.0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn large_cards_stay_few_and_wide() {
        // 4K 相当の広い画面でも 3 列で頭打ちにする。
        let wide = layout_large(3000.0, 1.0);
        assert_eq!(wide.columns, MAX_COLUMNS_LARGE);
        assert!(wide.card_width > 900.0, "{wide:?}");

        // ノート PC のウィンドウ幅なら 2 列。1 列だと 1 作品で画面が埋まる。
        let laptop = layout_large(1100.0, 1.0);
        assert_eq!(laptop.columns, 2);
        assert!(laptop.card_width > 500.0, "{laptop:?}");

        // 同じ幅でグリッドより必ず少ない列になる。
        for width in [700.0, 1100.0, 1440.0, 2200.0, 3000.0] {
            let large = layout_large(width, 1.0);
            let grid = layout_scaled(width, 1.0);
            assert!(
                large.columns < grid.columns,
                "幅 {width}: 大型 {} 列 / グリッド {} 列",
                large.columns,
                grid.columns
            );
        }
    }

    #[test]
    fn a_narrow_window_falls_back_to_one_large_card() {
        assert_eq!(layout_large(400.0, 1.0).columns, 1);
    }

    #[test]
    fn card_size_changes_the_column_count() {
        let narrow = layout_scaled(1440.0, 1.4);
        let normal = layout_scaled(1440.0, 1.0);
        let wide = layout_scaled(1440.0, 0.72);
        assert!(
            narrow.columns < normal.columns && normal.columns < wide.columns,
            "大 {} / 中 {} / 小 {} の順に列が増えるはず",
            narrow.columns,
            normal.columns,
            wide.columns
        );
        assert!(narrow.card_width > normal.card_width);
    }

    #[test]
    fn a_tiny_scale_still_leaves_usable_cards() {
        let m = layout_scaled(320.0, 0.01);
        assert!(m.card_width >= 80.0 || m.columns == 1, "{m:?}");
    }

    #[test]
    fn column_counts_match_the_design_document() {
        // 設計書 §6.3 の想定を代表的な論理幅で確認する。
        for width in [360.0, 390.0, 430.0_f32] {
            assert_eq!(layout(width).columns, 2, "phone {width}");
        }
        for width in [768.0, 820.0, 960.0_f32] {
            assert!((3..=4).contains(&layout(width).columns), "tablet {width}");
        }
        for width in [1100.0, 1280.0, 1680.0, 1920.0_f32] {
            assert!((4..=8).contains(&layout(width).columns), "desktop {width}");
        }
        for width in [2200.0, 2560.0, 3440.0_f32] {
            assert!((6..=10).contains(&layout(width).columns), "wide {width}");
        }
    }

    #[test]
    fn desktop_cards_are_not_squeezed_to_phone_size() {
        assert!(layout(1280.0).card_width >= MIN_CARD_WIDTH_DESKTOP);
    }

    #[test]
    fn cards_and_spacing_fill_the_available_width() {
        for width in [320.0, 390.0, 768.0, 1100.0, 1920.0, 3840.0_f32] {
            let m = layout(width);
            let used = m.card_width * m.columns as f32 + SPACING * (m.columns - 1) as f32;
            assert!((used - width).abs() < 0.01, "width {width}: used {used}");
        }
    }

    #[test]
    fn columns_are_capped_on_very_wide_screens() {
        assert_eq!(layout(10_000.0).columns, MAX_COLUMNS);
    }

    #[test]
    fn narrow_or_degenerate_widths_still_give_one_column() {
        assert_eq!(layout(80.0).columns, 1);
        assert_eq!(layout(0.0).columns, 1);
        assert!(layout(0.0).card_width > 0.0);
    }

    #[test]
    fn thumbnail_keeps_the_card_aspect() {
        let m = layout(1100.0);
        assert!((m.card_width / m.thumbnail_height - THUMBNAIL_ASPECT).abs() < 1e-3);
    }
}
