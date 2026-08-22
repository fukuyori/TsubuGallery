//! Gallery の状態。UI フレームワークには依存しない。
//!
//! egui 側はこの構造体を読んで描くだけなので、Gallery のふるまい (選択の動き、
//! 絞り込み、サムネイル取得の順番) を UI なしでテストできる。

use crate::model::{Filter, GalleryItem, SortOrder, ThumbnailState};

/// キーボードによる選択移動。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Move {
    Left,
    Right,
    Up,
    Down,
    First,
    Last,
}

pub struct GalleryView {
    /// 全作品。位置は Viewer やソースの配列と一致する。
    items: Vec<GalleryItem>,
    /// 絞り込みと並び替えを通したあとの表示順。中身は `items` の添字。
    visible: Vec<usize>,
    /// `visible` の中での選択位置。
    cursor: usize,
    /// 直近のレイアウトで使われた列数。上下移動の幅になる。
    columns: usize,

    filter: Filter,
    sort: SortOrder,
}

impl GalleryView {
    pub fn new(items: Vec<GalleryItem>) -> Self {
        let mut view = Self {
            items,
            visible: Vec::new(),
            cursor: 0,
            columns: 1,
            filter: Filter::default(),
            sort: SortOrder::default(),
        };
        view.refresh();
        view
    }

    // ---- 参照 -----------------------------------------------------------

    /// 全作品。絞り込みの影響を受けない。
    pub fn items(&self) -> &[GalleryItem] {
        &self.items
    }

    /// 表示する作品の並び。中身は [`GalleryView::items`] の添字。
    pub fn visible(&self) -> &[usize] {
        &self.visible
    }

    pub fn item(&self, index: usize) -> Option<&GalleryItem> {
        self.items.get(index)
    }

    /// 絞り込みに関わらない値 (表示時刻など) を直接書き換えるとき用。
    ///
    /// 絞り込みの対象になる値を変えるときは、専用のメソッドを使うこと。
    /// そうしないと表示順が作り直されない。
    pub fn items_mut_at(&mut self, index: usize) -> Option<&mut GalleryItem> {
        self.items.get_mut(index)
    }

    /// タグを差し替える。絞り込みに関わるので表示順を作り直す。
    pub fn set_tags(&mut self, index: usize, tags: std::collections::BTreeSet<String>) {
        if let Some(item) = self.items.get_mut(index) {
            item.tags = tags;
        }
        self.refresh();
    }

    /// 全作品数。
    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// 絞り込み後の件数。
    pub fn visible_len(&self) -> usize {
        self.visible.len()
    }

    /// 選択中の作品の位置。絞り込みで何も残っていなければ `None`。
    pub fn selected(&self) -> Option<usize> {
        self.visible.get(self.cursor).copied()
    }

    /// 選択中の位置。何も無ければ 0 を返す。
    pub fn selected_index(&self) -> usize {
        self.selected().unwrap_or(0)
    }

    pub fn selected_item(&self) -> Option<&GalleryItem> {
        self.items.get(self.selected()?)
    }

    pub fn index_of(&self, id: &str) -> Option<usize> {
        self.items.iter().position(|i| i.id == id)
    }

    pub fn filter(&self) -> &Filter {
        &self.filter
    }

    pub fn sort(&self) -> SortOrder {
        self.sort
    }

    /// 上下移動に使っている列数。
    pub fn columns(&self) -> usize {
        self.columns
    }

    // ---- 更新 -----------------------------------------------------------

    /// 描画側が決めた列数を教える。上下移動はこの値を使う。
    pub fn set_columns(&mut self, columns: usize) {
        self.columns = columns.max(1);
    }

    pub fn set_filter(&mut self, filter: Filter) {
        if self.filter != filter {
            self.filter = filter;
            self.refresh();
        }
    }

    pub fn set_sort(&mut self, sort: SortOrder) {
        if self.sort != sort {
            self.sort = sort;
            self.refresh();
        }
    }

    /// 作品を選ぶ。絞り込みで隠れている作品は選べない。
    pub fn select(&mut self, index: usize) {
        if let Some(pos) = self.visible.iter().position(|&i| i == index) {
            self.cursor = pos;
        }
    }

    pub fn move_selection(&mut self, direction: Move) {
        if self.visible.is_empty() {
            return;
        }
        let last = self.visible.len() - 1;
        let cols = self.columns.max(1);

        self.cursor = match direction {
            Move::Left => self.cursor.saturating_sub(1),
            Move::Right => (self.cursor + 1).min(last),
            // 1 行目での Up は動かさない。saturating_sub だと同じ行の別の
            // カードへ横飛びしてしまう。
            Move::Up => {
                if self.cursor >= cols {
                    self.cursor - cols
                } else {
                    self.cursor
                }
            }
            // 最終行が埋まっていないときは末尾へ寄せる。
            Move::Down => (self.cursor + cols).min(last),
            Move::First => 0,
            Move::Last => last,
        };
    }

    pub fn toggle_favorite(&mut self, index: usize) {
        if let Some(item) = self.items.get_mut(index) {
            item.favorite = !item.favorite;
        }
        // お気に入りのみ表示中なら、外した作品はその場で消える。
        self.refresh_keeping_selection(index);
    }

    pub fn set_status(&mut self, index: usize, status: crate::model::SketchStatus) {
        if let Some(item) = self.items.get_mut(index) {
            item.status = status;
        }
        // 状態の変化は選択と関係ないので、選んでいる作品は動かさない。
        self.refresh();
    }

    /// コレクションへの所属を書き換える (設計書 §27)。
    pub fn set_collection(&mut self, index: usize, name: &str, member: bool) {
        if let Some(item) = self.items.get_mut(index) {
            if member {
                item.collections.insert(name.to_string());
            } else {
                item.collections.remove(name);
            }
        }
        // 絞り込み中なら、外した作品はその場で消える。
        self.refresh_keeping_selection(index);
    }

    /// コレクションを全作品から外す。コレクションごと消したとき。
    pub fn remove_collection(&mut self, name: &str) {
        for item in &mut self.items {
            item.collections.remove(name);
        }
        self.refresh();
    }

    /// 作者とリンクを記録する。
    pub fn set_credit(&mut self, index: usize, author: &str, link: &str) {
        if let Some(item) = self.items.get_mut(index) {
            item.author = author.to_string();
            item.link = link.to_string();
        }
        // 作者でも検索できるので、絞り込み中なら見え方が変わる。
        self.refresh_keeping_selection(index);
    }

    /// どちらの方言として読まれたかを記録する。リスト表示に出す。
    pub fn set_dialect(&mut self, index: usize, dialect: Option<String>) {
        if let Some(item) = self.items.get_mut(index) {
            item.dialect = dialect;
        }
    }

    pub fn set_thumbnail_state(&mut self, index: usize, state: ThumbnailState) {
        if let Some(item) = self.items.get_mut(index) {
            item.thumbnail = state;
        }
    }

    /// ワーカーが停止したとき、完了通知を待ち続ける項目を再試行可能に戻す。
    pub fn reset_loading_thumbnails(&mut self) {
        for item in &mut self.items {
            if matches!(item.thumbnail, ThumbnailState::Loading) {
                item.thumbnail = ThumbnailState::Missing;
            }
        }
    }

    /// 作品を差し込む。ファイル名順を保つため位置を指定する。
    ///
    /// 選んでいた作品はそのまま選ばれ続ける。新しい作品を選びたい呼び出し側は
    /// あとから [`GalleryView::select`] する。
    pub fn insert(&mut self, index: usize, item: GalleryItem) {
        let index = index.min(self.items.len());
        let selected = self.selected();
        self.items.insert(index, item);
        self.refresh();

        if let Some(previous) = selected {
            // 手前に差し込まれた分だけ添字がずれる。
            let moved = if index <= previous {
                previous + 1
            } else {
                previous
            };
            self.select(moved);
        }
    }

    /// 作品を取り除く。選択は近くへ寄せる。
    pub fn remove(&mut self, index: usize) {
        if index >= self.items.len() {
            return;
        }
        let selected = self.selected();
        let cursor = self.cursor;
        self.items.remove(index);
        self.refresh();

        match selected {
            Some(previous) if previous != index => {
                let moved = if index < previous {
                    previous - 1
                } else {
                    previous
                };
                self.select(moved);
            }
            // 選んでいた作品が消えたときは、同じ位置の隣へ寄せる。
            _ => self.cursor = cursor.min(self.visible.len().saturating_sub(1)),
        }
    }

    /// 名前を変える。
    pub fn rename(&mut self, index: usize, id: String, title: String) {
        if let Some(item) = self.items.get_mut(index) {
            item.id = id;
            item.title = title;
        }
        self.refresh();
    }

    /// 次に取得すべきサムネイルを返す。
    ///
    /// 表示中の並びを、選択位置から後ろへ、続いて先頭から探す。画面に映っている
    /// 可能性が高いものを先に埋めることで、スクロールを妨げずに絵が揃っていく
    /// (設計書 §22)。隠れている作品も最後にまとめて拾う。
    pub fn next_missing_thumbnail(&self) -> Option<usize> {
        let n = self.visible.len();
        let from_visible = (0..n)
            .map(|offset| self.visible[(self.cursor + offset) % n.max(1)])
            .find(|&i| self.items[i].thumbnail == ThumbnailState::Missing);
        if from_visible.is_some() {
            return from_visible;
        }
        (0..self.items.len()).find(|&i| self.items[i].thumbnail == ThumbnailState::Missing)
    }

    // ---- 内部 -----------------------------------------------------------

    /// 表示順を作り直し、`index` の作品が見えていればそれを選び直す。
    fn refresh_keeping_selection(&mut self, index: usize) {
        self.refresh();
        self.select(index);
    }

    fn refresh(&mut self) {
        let selected_before = self.visible.get(self.cursor).copied();

        self.visible = (0..self.items.len())
            .filter(|&i| self.filter.matches(&self.items[i]))
            .collect();

        match self.sort {
            SortOrder::Name => {
                // items は既にファイル名順なので、添字の順がそのまま名前順。
            }
            SortOrder::RecentlyAdded => {
                self.visible.sort_by(|&a, &b| {
                    self.items[b]
                        .created_at
                        .cmp(&self.items[a].created_at)
                        .then(a.cmp(&b))
                });
            }
            SortOrder::RecentlyOpened => {
                self.visible.sort_by(|&a, &b| {
                    // 未表示は後ろへ。
                    let (x, y) = (self.items[a].last_opened_at, self.items[b].last_opened_at);
                    match (x, y) {
                        (Some(x), Some(y)) => y.cmp(&x).then(a.cmp(&b)),
                        (Some(_), None) => std::cmp::Ordering::Less,
                        (None, Some(_)) => std::cmp::Ordering::Greater,
                        (None, None) => a.cmp(&b),
                    }
                });
            }
        }

        // 同じ作品を選び続ける。消えていたら近い位置へ寄せる。
        self.cursor = match selected_before.and_then(|i| self.visible.iter().position(|&v| v == i))
        {
            Some(pos) => pos,
            None => self.cursor.min(self.visible.len().saturating_sub(1)),
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::SketchStatus;

    fn view(count: usize, columns: usize) -> GalleryView {
        let items = (0..count)
            .map(|i| GalleryItem::new(format!("id{i}"), format!("Title {i}"), 0))
            .collect();
        let mut v = GalleryView::new(items);
        v.set_columns(columns);
        v
    }

    #[test]
    fn horizontal_movement_clamps_at_both_ends() {
        let mut v = view(5, 3);
        v.move_selection(Move::Left);
        assert_eq!(v.selected_index(), 0);
        v.move_selection(Move::Last);
        v.move_selection(Move::Right);
        assert_eq!(v.selected_index(), 4);
    }

    #[test]
    fn vertical_movement_steps_by_column_count() {
        let mut v = view(9, 3);
        v.move_selection(Move::Down);
        assert_eq!(v.selected_index(), 3);
        v.move_selection(Move::Down);
        assert_eq!(v.selected_index(), 6);
        v.move_selection(Move::Up);
        assert_eq!(v.selected_index(), 3);
    }

    #[test]
    fn moving_down_into_a_partial_last_row_lands_on_the_last_item() {
        let mut v = view(7, 3);
        v.select(5);
        v.move_selection(Move::Down);
        assert_eq!(v.selected_index(), 6);
    }

    #[test]
    fn moving_up_from_the_first_row_stays_put() {
        let mut v = view(9, 3);
        v.select(1);
        v.move_selection(Move::Up);
        assert_eq!(v.selected_index(), 1);
    }

    #[test]
    fn navigation_on_an_empty_gallery_does_nothing() {
        let mut v = GalleryView::new(Vec::new());
        v.move_selection(Move::Down);
        assert_eq!(v.selected_index(), 0);
        assert!(v.selected_item().is_none());
    }

    #[test]
    fn missing_thumbnails_are_fetched_from_the_selection_outwards() {
        let mut v = view(5, 5);
        v.select(3);
        assert_eq!(v.next_missing_thumbnail(), Some(3));

        v.set_thumbnail_state(3, ThumbnailState::Ready);
        assert_eq!(v.next_missing_thumbnail(), Some(4));

        v.set_thumbnail_state(4, ThumbnailState::Ready);
        assert_eq!(v.next_missing_thumbnail(), Some(0));
    }

    #[test]
    fn hidden_sketches_still_get_their_thumbnails_eventually() {
        let mut v = view(3, 3);
        v.items[0].favorite = true;
        v.set_filter(Filter {
            favorites_only: true,
            ..Default::default()
        });
        assert_eq!(v.visible_len(), 1);

        v.set_thumbnail_state(0, ThumbnailState::Ready);
        // 表示中の分が終わったら、隠れている作品も拾う。
        assert_eq!(v.next_missing_thumbnail(), Some(1));
    }

    #[test]
    fn no_missing_thumbnails_returns_none() {
        let mut v = view(3, 3);
        for i in 0..3 {
            v.set_thumbnail_state(i, ThumbnailState::Ready);
        }
        assert_eq!(v.next_missing_thumbnail(), None);
    }

    #[test]
    fn stopped_worker_only_resets_loading_thumbnails() {
        let mut v = view(3, 3);
        v.set_thumbnail_state(0, ThumbnailState::Loading);
        v.set_thumbnail_state(1, ThumbnailState::Ready);
        v.set_thumbnail_state(2, ThumbnailState::Failed("broken".into()));

        v.reset_loading_thumbnails();

        assert_eq!(v.items()[0].thumbnail, ThumbnailState::Missing);
        assert_eq!(v.items()[1].thumbnail, ThumbnailState::Ready);
        assert_eq!(
            v.items()[2].thumbnail,
            ThumbnailState::Failed("broken".into())
        );
    }

    #[test]
    fn removing_the_selected_item_keeps_the_selection_in_range() {
        let mut v = view(3, 3);
        v.select(2);
        v.remove(2);
        assert_eq!(v.len(), 2);
        assert_eq!(v.selected_index(), 1);

        v.remove(0);
        v.remove(0);
        assert!(v.is_empty());
        assert!(v.selected_item().is_none());
    }

    #[test]
    fn inserting_before_the_selection_keeps_the_same_item_selected() {
        let mut v = view(3, 3);
        v.select(1);
        let selected_id = v.selected_item().expect("ある").id.clone();

        v.insert(0, GalleryItem::new("new", "New", 0));
        assert_eq!(v.selected_item().expect("ある").id, selected_id);
    }

    #[test]
    fn inserting_after_the_selection_leaves_it_alone() {
        let mut v = view(3, 3);
        v.select(0);
        v.insert(2, GalleryItem::new("new", "New", 0));
        assert_eq!(v.selected_index(), 0);
    }

    #[test]
    fn removing_an_earlier_item_keeps_the_same_sketch_selected() {
        let mut v = view(4, 4);
        v.select(2);
        let selected_id = v.selected_item().expect("ある").id.clone();

        v.remove(0);
        assert_eq!(v.selected_item().expect("ある").id, selected_id);
    }

    #[test]
    fn renaming_updates_both_id_and_title() {
        let mut v = view(2, 2);
        v.rename(0, "renamed".into(), "Renamed".into());
        assert_eq!(v.items()[0].id, "renamed");
        assert_eq!(v.items()[0].title, "Renamed");
        assert_eq!(v.index_of("renamed"), Some(0));
    }

    #[test]
    fn favorite_toggles_back_and_forth() {
        let mut v = view(2, 2);
        assert!(!v.items()[1].favorite);
        v.toggle_favorite(1);
        assert!(v.items()[1].favorite);
        v.toggle_favorite(1);
        assert!(!v.items()[1].favorite);
    }

    // ---- 絞り込みと並び替え (設計書 §20) --------------------------------

    #[test]
    fn text_filter_matches_title_and_id_case_insensitively() {
        let mut v = GalleryView::new(vec![
            GalleryItem::new("spiral", "Spiral", 0),
            GalleryItem::new("pulse-grid", "Pulse Grid", 0),
        ]);
        v.set_filter(Filter {
            text: "SPI".into(),
            ..Default::default()
        });
        assert_eq!(v.visible(), &[0]);

        v.set_filter(Filter {
            text: "grid".into(),
            ..Default::default()
        });
        assert_eq!(v.visible(), &[1]);

        v.set_filter(Filter {
            text: "見つからない".into(),
            ..Default::default()
        });
        assert!(v.visible().is_empty());
    }

    #[test]
    fn favorites_and_errors_can_be_filtered() {
        let mut v = view(3, 3);
        v.items[1].favorite = true;
        v.items[2].status = SketchStatus::Error("だめ".into());

        v.set_filter(Filter {
            favorites_only: true,
            ..Default::default()
        });
        assert_eq!(v.visible(), &[1]);

        v.set_filter(Filter {
            errors_only: true,
            ..Default::default()
        });
        assert_eq!(v.visible(), &[2]);
    }

    #[test]
    fn tag_filter_selects_only_tagged_sketches() {
        let mut v = view(3, 3);
        v.items[0].tags.insert("circles".into());
        v.items[2].tags.insert("circles".into());

        v.set_filter(Filter {
            tag: Some("circles".into()),
            ..Default::default()
        });
        assert_eq!(v.visible(), &[0, 2]);
    }

    #[test]
    fn filters_combine() {
        let mut v = view(4, 4);
        v.items[1].favorite = true;
        v.items[1].tags.insert("keep".into());
        v.items[3].favorite = true;

        v.set_filter(Filter {
            favorites_only: true,
            tag: Some("keep".into()),
            ..Default::default()
        });
        assert_eq!(v.visible(), &[1]);
    }

    #[test]
    fn sorting_by_recently_added_puts_the_newest_first() {
        let mut v = view(3, 3);
        v.items[0].created_at = 100;
        v.items[1].created_at = 300;
        v.items[2].created_at = 200;
        v.set_sort(SortOrder::RecentlyAdded);
        assert_eq!(v.visible(), &[1, 2, 0]);
    }

    #[test]
    fn sorting_by_recently_opened_pushes_unopened_to_the_end() {
        let mut v = view(3, 3);
        v.items[0].last_opened_at = Some(50);
        v.items[2].last_opened_at = Some(90);
        v.set_sort(SortOrder::RecentlyOpened);
        assert_eq!(v.visible(), &[2, 0, 1]);
    }

    #[test]
    fn navigation_stays_inside_the_filtered_set() {
        let mut v = view(5, 5);
        v.items[1].favorite = true;
        v.items[3].favorite = true;
        v.set_filter(Filter {
            favorites_only: true,
            ..Default::default()
        });

        v.move_selection(Move::First);
        assert_eq!(v.selected_index(), 1);
        v.move_selection(Move::Right);
        assert_eq!(v.selected_index(), 3, "隠れている 2 は飛ばす");
        v.move_selection(Move::Right);
        assert_eq!(v.selected_index(), 3, "末尾で止まる");
    }

    #[test]
    fn the_same_sketch_stays_selected_when_the_filter_changes() {
        let mut v = view(4, 4);
        v.items[2].favorite = true;
        v.select(2);

        v.set_filter(Filter {
            favorites_only: true,
            ..Default::default()
        });
        assert_eq!(v.selected_index(), 2, "残っているなら選択を維持する");
    }

    #[test]
    fn unfavoriting_while_filtered_moves_the_selection_somewhere_valid() {
        let mut v = view(3, 3);
        v.items[0].favorite = true;
        v.items[1].favorite = true;
        v.set_filter(Filter {
            favorites_only: true,
            ..Default::default()
        });
        v.select(1);

        v.toggle_favorite(1);
        assert_eq!(v.visible(), &[0]);
        assert_eq!(v.selected_index(), 0);
    }

    #[test]
    fn everything_filtered_out_leaves_no_selection() {
        let mut v = view(3, 3);
        v.set_filter(Filter {
            favorites_only: true,
            ..Default::default()
        });
        assert_eq!(v.visible_len(), 0);
        assert!(v.selected().is_none());
        assert!(v.selected_item().is_none());
    }

    /// 作成日時は作るときに必ず渡す。
    ///
    /// 既定値のまま作れると入れ忘れる。忘れると 0 になり、
    /// 「最近追加」でいちばん古い扱いで末尾へ落ちる。
    #[test]
    fn an_item_carries_the_time_it_was_made() {
        let item = GalleryItem::new("a", "A", 1_234);
        assert_eq!(item.created_at, 1_234);
    }

    /// 足したばかりの作品が「最近追加」の先頭に来る。
    ///
    /// 作成日時を入れずに差し込むと 0 になり、いちばん古い扱いで末尾へ
    /// 落ちる。作った直後に見当たらないのは分かりにくい。
    #[test]
    fn a_freshly_added_sketch_comes_first_under_recently_added() {
        let mut v = view(3, 3);
        for (i, item) in v.items.iter_mut().enumerate() {
            item.created_at = 100 + i as i64;
        }
        v.set_sort(SortOrder::RecentlyAdded);

        let mut fresh = GalleryItem::new("new", "New", 0);
        fresh.created_at = 999;
        v.insert(1, fresh);

        let first = v.visible().first().copied().expect("何かある");
        assert_eq!(
            v.item(first).map(|i| i.id.as_str()),
            Some("new"),
            "先頭に来ません"
        );
    }
}
