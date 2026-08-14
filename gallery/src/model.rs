//! Gallery が並べる作品の表示モデル。
//!
//! Phase 7 で SQLite の `Sketch` / `Tag` テーブルへ移すまでの、UI 側から見た形。
//! 画像そのものはここに持たず、読み込み状態だけを持つ (設計書 §7.3)。

use std::collections::BTreeSet;

/// 作品が実行可能かどうか (設計書 §6.1 の「実行可能／エラー状態」)。
///
/// 組み込みスケッチは必ず動くので今は常に [`SketchStatus::Ready`] だが、
/// Phase 6 で保存時コンパイルが入るとコンパイルエラーがここへ入る。
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub enum SketchStatus {
    #[default]
    Ready,
    Error(String),
}

impl SketchStatus {
    pub fn is_error(&self) -> bool {
        matches!(self, SketchStatus::Error(_))
    }
}

/// サムネイルの読み込み状態 (設計書 §22 の段階ロード)。
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub enum ThumbnailState {
    /// まだ画像が無い。ディスクにも無ければ生成する。
    #[default]
    Missing,
    /// 読み込み中または生成中。
    Loading,
    /// 表示できる。テクスチャ本体は UI 層が持つ。
    Ready,
    Failed(String),
}

#[derive(Clone, Debug)]
pub struct GalleryItem {
    pub id: String,
    pub title: String,
    /// 作者。空なら出さない。
    pub author: String,
    /// 元の投稿などへのリンク。
    pub link: String,
    /// どちらの方言として読まれたか (`Processing` / `p5.js`)。
    ///
    /// 一覧に出すためだけに持つ。読めなかった作品は `None`。
    pub dialect: Option<String>,
    pub favorite: bool,
    pub status: SketchStatus,
    pub thumbnail: ThumbnailState,
    /// 設計書 §19.2 のタグ。
    pub tags: BTreeSet<String>,
    /// 所属するコレクション (設計書 §27)。
    pub collections: BTreeSet<String>,
    /// 追加した時刻 (UNIX 秒)。「最近追加」の並び替えに使う。
    pub created_at: i64,
    /// 最後に Viewer で開いた時刻。「最近表示」の並び替えに使う。
    pub last_opened_at: Option<i64>,
}

impl GalleryItem {
    /// 作成日時は必ず渡す。
    ///
    /// 既定値のまま作れるようにしておくと、入れ忘れて 0 のまま並ぶ。
    /// 「最近追加」でいちばん古い扱いになり、足したばかりの作品が
    /// 末尾へ落ちる。忘れられないよう引数にしてある。
    pub fn new(id: impl Into<String>, title: impl Into<String>, created_at: i64) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            author: String::new(),
            link: String::new(),
            dialect: None,
            favorite: false,
            status: SketchStatus::default(),
            thumbnail: ThumbnailState::default(),
            tags: BTreeSet::new(),
            collections: BTreeSet::new(),
            created_at,
            last_opened_at: None,
        }
    }
}

/// Gallery の絞り込み条件 (設計書 §20)。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Filter {
    /// タイトルの部分一致。大文字小文字は区別しない。
    pub text: String,
    pub favorites_only: bool,
    /// コンパイルできない作品だけ。
    pub errors_only: bool,
    pub tag: Option<String>,
    /// このコレクションに入っている作品だけ (設計書 §27)。
    pub collection: Option<String>,
}

impl Filter {
    pub fn is_empty(&self) -> bool {
        *self == Filter::default()
    }

    /// この作品を表示するか。
    pub fn matches(&self, item: &GalleryItem) -> bool {
        if self.favorites_only && !item.favorite {
            return false;
        }
        if self.errors_only && !item.status.is_error() {
            return false;
        }
        if let Some(tag) = &self.tag
            && !item.tags.contains(tag)
        {
            return false;
        }
        if let Some(collection) = &self.collection
            && !item.collections.contains(collection)
        {
            return false;
        }
        if !self.text.is_empty() {
            let needle = self.text.to_lowercase();
            // id も見る。タイトルは id から作られるが、改名すると離れるため。
            // 作者でも探せる。誰の作品かで思い出すことが多い。
            if !item.title.to_lowercase().contains(&needle)
                && !item.id.to_lowercase().contains(&needle)
                && !item.author.to_lowercase().contains(&needle)
            {
                return false;
            }
        }
        true
    }
}

/// 並び順 (設計書 §20)。
///
/// 設定として保存するので実体は [`tsubu_core::settings`] に置いてある。二重に
/// 持つと、片方だけ選択肢が増えたときに食い違う。
pub use tsubu_core::settings::{Choice, SortOrder};

#[cfg(test)]
mod collection_tests {
    use super::*;

    fn item_in(collections: &[&str]) -> GalleryItem {
        let mut item = GalleryItem::new("a", "A", 0);
        item.collections = collections.iter().map(|c| c.to_string()).collect();
        item
    }

    #[test]
    fn a_collection_filter_keeps_only_its_members() {
        let filter = Filter { collection: Some("夜".into()), ..Filter::default() };
        assert!(filter.matches(&item_in(&["夜"])));
        assert!(filter.matches(&item_in(&["夜", "線"])));
        assert!(!filter.matches(&item_in(&["線"])));
        assert!(!filter.matches(&item_in(&[])));
    }

    #[test]
    fn no_collection_filter_keeps_everything() {
        assert!(Filter::default().matches(&item_in(&[])));
    }

    /// 絞り込みは重ねられる。コレクション かつ お気に入り。
    #[test]
    fn a_collection_filter_stacks_with_the_others() {
        let filter =
            Filter { collection: Some("夜".into()), favorites_only: true, ..Filter::default() };
        let mut item = item_in(&["夜"]);
        assert!(!filter.matches(&item), "お気に入りでないので外れる");
        item.favorite = true;
        assert!(filter.matches(&item));
    }
}
