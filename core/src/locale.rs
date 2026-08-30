//! UI の多言語化 (設計書 §11)。
//!
//! 表示文字列はソースへ埋め込まず、必ず翻訳キー経由で取得する。同梱の
//! `ja-JP` / `en-US` に加えて、`locales/` へ JSON を置くだけで言語を追加できる。

use std::collections::BTreeMap;
use std::path::Path;

/// 未翻訳キーの最終フォールバック先。
pub const FALLBACK_LANGUAGE: &str = "en-US";

/// 修飾キーの表記。キーボードの刻印が OS で違うので、翻訳ではなく実行環境で決める。
///
/// `⌥` と `⇥` は同梱フォントに字形が無く豆腐になるので、記号ではなく名前で出す。
pub mod modifier {
    pub const COMMAND: &str = if cfg!(target_os = "macos") { "⌘" } else { "Ctrl+" };
    pub const ALT: &str = if cfg!(target_os = "macos") { "Opt+" } else { "Alt+" };
}

const BUILTIN: &[(&str, &str)] = &[
    ("ja-JP", include_str!("../../locales/ja-JP.json")),
    ("en-US", include_str!("../../locales/en-US.json")),
];

/// 言語の選択方法 (設計書 §11.2)。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LanguagePreference {
    /// OS の設定に従う。
    System,
    /// ユーザーが明示的に選んだ言語タグ。
    Explicit(String),
}

/// 1 言語分の翻訳表。
#[derive(Clone, Debug)]
pub struct Translation {
    tag: String,
    /// 設定画面に出す自称表記 (`日本語` / `English`)。
    native_name: String,
    strings: BTreeMap<String, String>,
}

impl Translation {
    fn parse(tag: &str, json: &str) -> Result<Self, serde_json::Error> {
        let strings: BTreeMap<String, String> = serde_json::from_str(json)?;
        let native_name = strings
            .get("language.native_name")
            .cloned()
            .unwrap_or_else(|| tag.to_string());
        Ok(Self { tag: tag.to_string(), native_name, strings })
    }

    pub fn tag(&self) -> &str {
        &self.tag
    }

    pub fn native_name(&self) -> &str {
        &self.native_name
    }
}

/// 読み込み済みの全言語と、現在の選択状態。
pub struct Locales {
    available: Vec<Translation>,
    preference: LanguagePreference,
    active: usize,
    fallback: Option<usize>,
}

impl Locales {
    /// 同梱の翻訳だけを読み込む。
    pub fn builtin() -> Self {
        let available = BUILTIN
            .iter()
            .filter_map(|(tag, json)| match Translation::parse(tag, json) {
                Ok(t) => Some(t),
                Err(e) => {
                    // 同梱ファイルが壊れているのはビルド時の事故なので、
                    // 起動は続けつつ記録だけ残す。
                    log::error!("内蔵翻訳 {tag} の解析に失敗しました: {e}");
                    None
                }
            })
            .collect();

        let mut locales =
            Self { available, preference: LanguagePreference::System, active: 0, fallback: None };
        locales.reindex();
        locales
    }

    /// `dir` 内の `*.json` を追加で読み込む。同じタグがあれば上書きする。
    pub fn load_dir(&mut self, dir: &Path) {
        let Ok(entries) = std::fs::read_dir(dir) else { return };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            let Some(tag) = path.file_stem().and_then(|s| s.to_str()) else { continue };
            let Ok(json) = std::fs::read_to_string(&path) else { continue };
            match Translation::parse(tag, &json) {
                Ok(t) => {
                    if let Some(slot) = self.available.iter_mut().find(|x| x.tag == t.tag) {
                        *slot = t;
                    } else {
                        self.available.push(t);
                    }
                }
                Err(e) => log::warn!("翻訳ファイル {} を読めませんでした: {e}", path.display()),
            }
        }
        self.reindex();
    }

    pub fn available(&self) -> &[Translation] {
        &self.available
    }

    pub fn preference(&self) -> &LanguagePreference {
        &self.preference
    }

    /// 実際に使われている言語タグ。
    pub fn active_tag(&self) -> &str {
        self.available.get(self.active).map_or(FALLBACK_LANGUAGE, |t| t.tag.as_str())
    }

    pub fn set_preference(&mut self, preference: LanguagePreference) {
        self.preference = preference;
        self.reindex();
    }

    /// ショートカットの説明を翻訳する。
    ///
    /// `{cmd}` と `{alt}` は動いている OS の刻印に置き換える。macOS では `⌘` `⌥`、
    /// Windows と Linux では `Ctrl+` `Alt+`。
    pub fn shortcut(&self, key: &str) -> String {
        self.t(key).replace("{cmd}", modifier::COMMAND).replace("{alt}", modifier::ALT)
    }

    /// キーを翻訳する。未定義なら `en-US`、それも無ければキー自身を返す。
    pub fn t<'a>(&'a self, key: &'a str) -> &'a str {
        if let Some(v) = self.available.get(self.active).and_then(|t| t.strings.get(key)) {
            return v;
        }
        if let Some(v) = self.fallback.and_then(|i| self.available[i].strings.get(key)) {
            return v;
        }
        key
    }

    /// 選択状態を現在の [`LanguagePreference`] に合わせ直す。
    fn reindex(&mut self) {
        self.available.sort_by(|a, b| a.tag.cmp(&b.tag));
        self.fallback = self.available.iter().position(|t| t.tag == FALLBACK_LANGUAGE);

        let wanted = match &self.preference {
            LanguagePreference::Explicit(tag) => tag.clone(),
            LanguagePreference::System => detect_system_language(),
        };

        self.active = match_language(&self.available, &wanted)
            .or(self.fallback)
            .unwrap_or(0);
    }
}

/// 完全一致 → 言語サブタグ一致 の順で探す (`ja_JP.UTF-8` → `ja-JP` → `ja`)。
fn match_language(available: &[Translation], wanted: &str) -> Option<usize> {
    let wanted = normalize_tag(wanted);
    if let Some(i) = available.iter().position(|t| t.tag.eq_ignore_ascii_case(&wanted)) {
        return Some(i);
    }
    let primary = wanted.split('-').next().unwrap_or(&wanted).to_ascii_lowercase();
    available
        .iter()
        .position(|t| t.tag.split('-').next().unwrap_or("").eq_ignore_ascii_case(&primary))
}

/// `ja_JP.UTF-8` のような POSIX 表記を BCP 47 風へ寄せる。
fn normalize_tag(raw: &str) -> String {
    raw.split(['.', '@']).next().unwrap_or(raw).replace('_', "-")
}

/// OS の言語設定を推定する。
///
/// Windows ではユーザーの表示言語 (`GetUserDefaultLocaleName`、`ja-JP` のような
/// BCP 47) を引く。`LANG` などの環境変数は Windows には普通無いので、それだけを
/// 見ると常に英語になってしまう。ほかの OS と、Windows で取れなかったときは
/// 環境変数を見る。
pub fn detect_system_language() -> String {
    if let Some(tag) = windows_user_language() {
        return tag;
    }
    for key in ["LC_ALL", "LC_MESSAGES", "LANG"] {
        if let Ok(v) = std::env::var(key)
            && !v.is_empty()
            && v != "C"
            && v != "POSIX"
        {
            return normalize_tag(&v);
        }
    }
    FALLBACK_LANGUAGE.to_string()
}

/// Windows のユーザー表示言語。取れなければ `None`。
#[cfg(windows)]
fn windows_user_language() -> Option<String> {
    use windows_sys::Win32::Globalization::GetUserDefaultLocaleName;

    // LOCALE_NAME_MAX_LENGTH (85)。ロケール名はこれより長くならない。
    let mut buffer = [0u16; 85];
    // SAFETY: バッファは 85 要素あり、長さもそのまま渡す。
    // 戻り値は終端の NUL を含む文字数で、失敗なら 0。
    let written = unsafe { GetUserDefaultLocaleName(buffer.as_mut_ptr(), buffer.len() as i32) };
    if written <= 1 {
        return None;
    }
    let tag = String::from_utf16_lossy(&buffer[..written as usize - 1]);
    (!tag.is_empty()).then_some(tag)
}

#[cfg(not(windows))]
fn windows_user_language() -> Option<String> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Windows ではユーザーの表示言語が BCP 47 の形で取れる。
    #[cfg(windows)]
    #[test]
    fn windows_reports_the_user_language() {
        let tag = windows_user_language().expect("表示言語が取れる");
        assert!(tag.contains('-'), "BCP 47 の形ではない: {tag}");
        assert_eq!(detect_system_language(), tag, "環境変数より表示言語が先");
    }

    #[test]
    fn builtin_languages_load() {
        let l = Locales::builtin();
        let tags: Vec<_> = l.available().iter().map(|t| t.tag()).collect();
        assert!(tags.contains(&"ja-JP"), "tags = {tags:?}");
        assert!(tags.contains(&"en-US"), "tags = {tags:?}");
    }

    #[test]
    fn explicit_preference_wins() {
        let mut l = Locales::builtin();
        l.set_preference(LanguagePreference::Explicit("ja-JP".into()));
        assert_eq!(l.active_tag(), "ja-JP");
        assert_eq!(l.t("viewer.next"), "次の作品");

        l.set_preference(LanguagePreference::Explicit("en-US".into()));
        assert_eq!(l.t("viewer.next"), "Next Sketch");
    }

    #[test]
    fn posix_locale_matches_language_subtag() {
        let mut l = Locales::builtin();
        l.set_preference(LanguagePreference::Explicit("ja_JP.UTF-8".into()));
        assert_eq!(l.active_tag(), "ja-JP");
    }

    #[test]
    fn unknown_language_falls_back_to_english() {
        let mut l = Locales::builtin();
        l.set_preference(LanguagePreference::Explicit("xx-YY".into()));
        assert_eq!(l.active_tag(), FALLBACK_LANGUAGE);
    }

    #[test]
    fn shortcuts_use_the_keyboard_of_this_platform() {
        let l = Locales::builtin();
        let text = l.shortcut("editor.hint");
        assert!(!text.contains("{cmd}"), "置き換え漏れ: {text}");
        assert!(!text.contains("{alt}"), "置き換え漏れ: {text}");
        assert!(text.contains(modifier::COMMAND), "{text}");

        if cfg!(target_os = "macos") {
            assert!(text.contains('⌘'), "{text}");
        } else {
            assert!(text.contains("Ctrl+"), "{text}");
        }
    }

    #[test]
    fn unknown_key_returns_the_key_itself() {
        let l = Locales::builtin();
        assert_eq!(l.t("no.such.key"), "no.such.key");
    }

    #[test]
    fn every_key_exists_in_every_language() {
        let l = Locales::builtin();
        let reference: Vec<&String> = l
            .available()
            .iter()
            .find(|t| t.tag() == FALLBACK_LANGUAGE)
            .expect("en-US present")
            .strings
            .keys()
            .collect();

        for t in l.available() {
            for key in &reference {
                assert!(
                    t.strings.contains_key(key.as_str()),
                    "{} に {key} がありません",
                    t.tag()
                );
            }
            assert_eq!(
                t.strings.len(),
                reference.len(),
                "{} のキー数が en-US と一致しません",
                t.tag()
            );
        }
    }
}
