//! コード編集画面の状態 (設計書 §25 の Editor)。
//!
//! 通常の利用ではここを開かない。編集が必要なときだけ Gallery / Viewer から入る
//! (設計書 §4)。
//!
//! 入力中も裏でコンパイルして、エラーを保存前に知らせる。打っている途中のコードは
//! たいてい壊れているので、手が止まってから少し待って確かめる。

/// 新規作品の既定名。ここから `sketch-2`, `sketch-3` と重複を避ける。
pub const DEFAULT_NAME: &str = "sketch";

pub struct Editor {
    /// 編集中の作品の位置。新規作成中は `None`。
    pub index: Option<usize>,
    pub name: String,
    pub source: String,
    /// タグ。カンマ区切りで編集する (設計書 §19.2)。
    pub tags: String,
    /// 作者。
    pub author: String,
    /// 元の投稿などへのリンク。
    pub link: String,
    /// 保存済みの状態。変更の有無を見るために持つ。
    saved_name: String,
    saved_source: String,
    saved_tags: String,
    saved_author: String,
    saved_link: String,
    /// 直近の保存で出たコンパイルエラー。行を強調するので位置ごと持つ。
    pub error: Option<tsubu_processing_lite::CompileError>,
    /// 保存に失敗した理由 (ファイル書き込みなど)。
    pub io_error: Option<String>,
    /// 未保存のまま閉じようとして、確認を待っている。
    pub confirming_close: bool,
    /// この行へカーソルを飛ばす。エラー表示を押したときに入る。
    pub jump_to_line: Option<u32>,

    /// エラーの原因が方言の違いらしいときの内訳。
    pub diagnosis: Option<tsubu_processing_lite::dialect::Diagnosis>,
    /// 直近のチェックで、どちらの方言として読めたか。
    pub dialect: Option<tsubu_processing_lite::dialect::Dialect>,
    /// 最後にエラーチェックしたときのソース。
    checked_source: String,
    /// 最後にソースが変わった時刻。手が止まったかの判定に使う。
    changed_at: Option<std::time::Instant>,
}

/// 入力が止まってからエラーチェックするまでの待ち時間。
///
/// 打っている最中は毎文字が構文エラーになるので、少し置いてから確かめる。
pub const CHECK_DELAY: std::time::Duration = std::time::Duration::from_millis(400);

impl Editor {
    /// 新規作成。まだファイルは作らない。
    ///
    /// 本文は空で始める。ひな形を入れておくと、貼り付ける前に消す手間がかかる。
    pub fn new_sketch(name: String) -> Self {
        Self {
            index: None,
            name,
            source: String::new(),
            tags: String::new(),
            author: String::new(),
            link: String::new(),
            // 未保存であることを示すため、保存済みの状態は空にしておく。
            saved_name: String::new(),
            saved_source: String::new(),
            saved_tags: String::new(),
            saved_author: String::new(),
            saved_link: String::new(),
            error: None,
            io_error: None,
            confirming_close: false,
            jump_to_line: None,
            diagnosis: None,
            dialect: None,
            checked_source: String::new(),
            changed_at: None,
        }
    }

    /// 既存の作品を開く。
    pub fn edit(
        index: usize,
        name: String,
        source: String,
        tags: String,
        author: String,
        link: String,
    ) -> Self {
        Self {
            index: Some(index),
            saved_name: name.clone(),
            saved_source: source.clone(),
            saved_tags: tags.clone(),
            saved_author: author.clone(),
            saved_link: link.clone(),
            name,
            source,
            tags,
            author,
            link,
            error: None,
            io_error: None,
            confirming_close: false,
            jump_to_line: None,
            diagnosis: None,
            dialect: None,
            checked_source: String::new(),
            changed_at: None,
        }
    }

    pub fn is_dirty(&self) -> bool {
        self.name != self.saved_name
            || self.source != self.saved_source
            || self.tags != self.saved_tags
            || self.author != self.saved_author
            || self.link != self.saved_link
    }

    /// カンマ区切りのタグを整える。空白だけの要素と重複は落とす。
    pub fn parsed_tags(&self) -> std::collections::BTreeSet<String> {
        self.tags
            .split(',')
            .map(str::trim)
            .filter(|t| !t.is_empty())
            .map(str::to_owned)
            .collect()
    }

    pub fn is_new(&self) -> bool {
        self.index.is_none()
    }

    /// 前回のチェック以降にソースが変わっていれば、時刻を控える。
    ///
    /// 毎フレーム呼ぶ。
    pub fn note_source_changes(&mut self) {
        if self.source != self.checked_source && self.changed_at.is_none() {
            self.changed_at = Some(std::time::Instant::now());
        }
    }

    /// 何も書かれていない。
    pub fn is_blank(&self) -> bool {
        self.source.trim().is_empty()
    }

    /// いまエラーチェックすべきなら、対象のソースを返す。
    ///
    /// 空のときは調べない。書き始める前から「draw() がありません」と言われても
    /// 困るだけなので。
    pub fn source_to_check(&self) -> Option<&str> {
        if self.is_blank() {
            return None;
        }
        let changed_at = self.changed_at?;
        (changed_at.elapsed() >= CHECK_DELAY).then_some(self.source.as_str())
    }

    /// 入力が止まったあと、次に構文チェックを試す時刻。
    pub fn check_deadline(&self) -> Option<std::time::Instant> {
        (!self.is_blank())
            .then_some(self.changed_at?)
            .map(|changed| changed + CHECK_DELAY)
    }

    /// チェック結果を受け取る。
    ///
    /// エラーが出たときだけ方言を見る。通ったコードに口を出さない。
    pub fn set_check_result(
        &mut self,
        checked: String,
        error: Option<tsubu_processing_lite::CompileError>,
        dialect: Option<tsubu_processing_lite::dialect::Dialect>,
    ) {
        self.dialect = dialect;
        // 待っているあいだに書き換わっていたら、次のフレームでやり直す。
        self.changed_at = (self.source != checked).then(std::time::Instant::now);
        self.diagnosis = error.is_some().then(|| {
            tsubu_processing_lite::dialect::diagnose(&checked)
        }).filter(|d| !d.findings.is_empty());
        self.checked_source = checked;
        self.error = error;
    }

    /// いまの本文がコンパイルを通ることを確認済みか。
    pub fn is_checked_ok(&self) -> bool {
        self.error.is_none() && self.checked_source == self.source
    }

    /// エラーが指している行 (1 始まり)。
    pub fn error_line(&self) -> Option<u32> {
        self.error.as_ref().map(|e| e.line)
    }

    /// 保存が通ったことを記録する。
    pub fn mark_saved(&mut self, index: usize) {
        self.index = Some(index);
        self.saved_name = self.name.clone();
        self.saved_source = self.source.clone();
        self.saved_tags = self.tags.clone();
        self.saved_author = self.author.clone();
        self.saved_link = self.link.clone();
        self.io_error = None;
        self.confirming_close = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_sketch_starts_empty() {
        let e = Editor::new_sketch("demo".into());
        assert!(e.is_new());
        assert!(e.is_blank(), "ひな形を入れない");
        assert!(e.is_dirty(), "名前は決まっているので保存対象");
    }

    #[test]
    fn an_empty_buffer_is_not_checked() {
        let mut e = Editor::new_sketch("demo".into());
        e.note_source_changes();
        e.changed_at = Some(std::time::Instant::now() - CHECK_DELAY);
        assert!(e.source_to_check().is_none(), "書き始める前に文句を言わない");

        e.source.push_str("void draw() {}");
        e.note_source_changes();
        e.changed_at = Some(std::time::Instant::now() - CHECK_DELAY);
        assert!(e.source_to_check().is_some());
    }

    #[test]
    fn an_opened_sketch_starts_clean() {
        let e = Editor::edit(0, "demo".into(), "void draw() {}".into(), String::new(), String::new(), String::new());
        assert!(!e.is_new());
        assert!(!e.is_dirty());
    }

    #[test]
    fn editing_the_name_code_or_tags_marks_it_dirty() {
        let mut e = Editor::edit(0, "demo".into(), "void draw() {}".into(), String::new(), String::new(), String::new());
        e.source.push(' ');
        assert!(e.is_dirty());

        let mut e = Editor::edit(0, "demo".into(), "void draw() {}".into(), String::new(), String::new(), String::new());
        e.name = "renamed".into();
        assert!(e.is_dirty());

        let mut e = Editor::edit(0, "demo".into(), "void draw() {}".into(), String::new(), String::new(), String::new());
        e.tags = "circles".into();
        assert!(e.is_dirty());
    }

    #[test]
    fn tags_are_split_trimmed_and_deduplicated() {
        let mut e = Editor::new_sketch("demo".into());
        e.tags = " circles , monochrome ,, circles ".into();
        let tags: Vec<String> = e.parsed_tags().into_iter().collect();
        assert_eq!(tags, vec!["circles", "monochrome"]);
    }

    // ---- 入力中のエラーチェック -----------------------------------------

    #[test]
    fn an_untouched_editor_has_nothing_to_check() {
        let e = Editor::edit(0, "demo".into(), "void draw() {}".into(), String::new(), String::new(), String::new());
        assert!(e.source_to_check().is_none(), "開いただけでは走らせない");
    }

    #[test]
    fn a_change_is_not_checked_immediately() {
        let mut e = Editor::edit(0, "demo".into(), "void draw() {}".into(), String::new(), String::new(), String::new());
        e.source.push('x');
        e.note_source_changes();
        // 打った直後は待つ。毎文字が構文エラーになるので。
        assert!(e.source_to_check().is_none());
        assert!(e.check_deadline().is_some(), "待機後の検査が予約されていない");
    }

    #[test]
    fn a_change_is_checked_once_the_hand_stops() {
        let mut e = Editor::edit(0, "demo".into(), "void draw() {}".into(), String::new(), String::new(), String::new());
        e.source.push('x');
        e.note_source_changes();
        e.changed_at = Some(std::time::Instant::now() - CHECK_DELAY);

        assert_eq!(e.source_to_check(), Some(e.source.as_str()));
    }

    #[test]
    fn the_result_sticks_until_the_source_changes_again() {
        let mut e = Editor::edit(0, "demo".into(), "void draw() {}".into(), String::new(), String::new(), String::new());
        e.source.push('x');
        e.note_source_changes();
        e.changed_at = Some(std::time::Instant::now() - CHECK_DELAY);

        let checked = e.source.clone();
        e.set_check_result(checked, None, None);
        assert!(e.source_to_check().is_none(), "同じソースを何度も調べない");

        e.note_source_changes();
        assert!(e.source_to_check().is_none(), "変わっていないので走らない");
    }

    #[test]
    fn editing_while_the_check_is_pending_schedules_another_one() {
        let mut e = Editor::edit(0, "demo".into(), "void draw() {}".into(), String::new(), String::new(), String::new());
        e.source.push('x');
        e.note_source_changes();

        // 待っているあいだに続きを打った、という状況。
        let stale = e.source.clone();
        e.source.push('y');
        e.set_check_result(stale, None, None);

        e.changed_at = Some(std::time::Instant::now() - CHECK_DELAY);
        assert_eq!(e.source_to_check(), Some(e.source.as_str()), "新しいほうを調べ直す");
    }

    #[test]
    fn saving_clears_the_dirty_flag() {
        let mut e = Editor::new_sketch("demo".into());
        e.mark_saved(3);
        assert!(!e.is_dirty());
        assert_eq!(e.index, Some(3));
        assert!(!e.is_new());
    }


}
