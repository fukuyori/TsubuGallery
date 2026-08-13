//! 作品ファイルの読み書き (設計書 §31 の core/repository の前段)。
//!
//! Phase 7 で SQLite が入るまで、作品は `<data>/sketches/*.pde` に置く。ここは
//! ファイルの並べ方だけを知っていて、中身が何語かは知らない。

use std::path::{Path, PathBuf};

/// 作品ファイルの拡張子。Processing と同じ。
pub const EXTENSION: &str = "pde";

/// 読み込んだ作品 1 本。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SketchFile {
    /// ファイル名から取った識別子。
    pub id: String,
    /// 表示名。
    pub title: String,
    pub source: String,
    pub path: PathBuf,
}

/// 識別子から表示名を作る。`pulse-grid` → `Pulse Grid`。
///
/// Phase 7 でタイトルを DB に持つまでの規約。ユーザーが付けた名前を勝手に
/// 変えてしまわないよう、区切り文字がある語だけを整形する。
pub fn title_from_id(id: &str) -> String {
    if !id.contains(['-', '_']) {
        return id.to_string();
    }
    id.split(['-', '_'])
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// ディレクトリ内の作品をファイル名順に読み込む。
pub fn load_all(dir: &Path) -> Vec<SketchFile> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };

    let mut files: Vec<SketchFile> = entries
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some(EXTENSION) {
                return None;
            }
            let id = path.file_stem()?.to_str()?.to_string();
            let source = match std::fs::read_to_string(&path) {
                Ok(s) => s,
                Err(e) => {
                    log::warn!("{} を読めませんでした: {e}", path.display());
                    return None;
                }
            };
            Some(SketchFile { title: title_from_id(&id), id, source, path })
        })
        .collect();

    files.sort_by(|a, b| a.id.cmp(&b.id));
    files
}

/// 識別子として使える文字列か。
///
/// そのままファイル名になるので、ディレクトリを抜け出せる文字を弾く。日本語は
/// そのまま使える (設計書 §11.3)。
pub fn is_valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.chars().count() <= 64
        && !id.starts_with('.')
        && !id.chars().any(|c| {
            c.is_control() || matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|')
        })
        && id.trim() == id
}

/// `base` から、まだ使われていない識別子を作る。`sketch`, `sketch-2`, ...
pub fn unique_id(dir: &Path, base: &str) -> String {
    if !exists(dir, base) {
        return base.to_string();
    }
    for n in 2.. {
        let candidate = format!("{base}-{n}");
        if !exists(dir, &candidate) {
            return candidate;
        }
    }
    unreachable!("空きが必ず見つかる")
}

pub fn exists(dir: &Path, id: &str) -> bool {
    path_for(dir, id).exists()
}

pub fn path_for(dir: &Path, id: &str) -> PathBuf {
    dir.join(format!("{id}.{EXTENSION}"))
}

/// 作品を書き出す。
pub fn save(dir: &Path, id: &str, source: &str) -> std::io::Result<PathBuf> {
    if !is_valid_id(id) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("作品名として使えません: {id}"),
        ));
    }
    std::fs::create_dir_all(dir)?;
    let path = path_for(dir, id);
    std::fs::write(&path, source)?;
    Ok(path)
}

/// 作品を消す。既に無い場合も成功とする。
pub fn delete(dir: &Path, id: &str) -> std::io::Result<()> {
    let path = path_for(dir, id);
    match std::fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e),
    }
}

/// ディレクトリに作品が 1 つも無ければ、同梱作品を書き出す。
///
/// 「無ければ 1 本ずつ補う」ではなく「空のときだけ」にしてあるので、ユーザーが
/// 削除した作品が次回起動で復活することはない。
pub fn seed_if_empty(dir: &Path, seeds: &[(&str, &str)]) -> std::io::Result<bool> {
    std::fs::create_dir_all(dir)?;
    if !load_all(dir).is_empty() {
        return Ok(false);
    }

    for (id, source) in seeds {
        let path = dir.join(format!("{id}.{EXTENSION}"));
        std::fs::write(&path, source)?;
    }
    Ok(!seeds.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// テスト用の一時ディレクトリ。`Drop` で片付ける。
    struct TempDir(PathBuf);

    impl TempDir {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir().join(format!("tsubu-library-test-{name}"));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).expect("一時ディレクトリを作れる");
            Self(dir)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn titles_are_derived_from_the_file_name() {
        assert_eq!(title_from_id("pulse-grid"), "Pulse Grid");
        assert_eq!(title_from_id("noise_field"), "Noise Field");
        // 区切りが無ければそのまま。ユーザーが付けた名前を壊さない。
        assert_eq!(title_from_id("spiral"), "spiral");
        assert_eq!(title_from_id("つぶやき"), "つぶやき");
    }

    #[test]
    fn only_pde_files_are_loaded_and_they_are_sorted() {
        let dir = TempDir::new("load");
        std::fs::write(dir.path().join("b.pde"), "void draw() {}").expect("書ける");
        std::fs::write(dir.path().join("a.pde"), "void draw() {}").expect("書ける");
        std::fs::write(dir.path().join("notes.txt"), "無視される").expect("書ける");

        let files = load_all(dir.path());
        assert_eq!(files.iter().map(|f| f.id.as_str()).collect::<Vec<_>>(), vec!["a", "b"]);
    }

    #[test]
    fn a_missing_directory_loads_as_empty() {
        assert!(load_all(Path::new("/tsubu/does/not/exist")).is_empty());
    }

    #[test]
    fn seeding_fills_an_empty_directory() {
        let dir = TempDir::new("seed-empty");
        let seeded =
            seed_if_empty(dir.path(), &[("demo", "void draw() {}")]).expect("書き出せる");
        assert!(seeded);
        assert_eq!(load_all(dir.path()).len(), 1);
    }

    #[test]
    fn ids_that_could_escape_the_directory_are_rejected() {
        assert!(is_valid_id("spiral"));
        assert!(is_valid_id("つぶやき 001"));
        assert!(!is_valid_id(""));
        assert!(!is_valid_id("../etc/passwd"));
        assert!(!is_valid_id("a/b"));
        assert!(!is_valid_id("a\\b"));
        assert!(!is_valid_id(".hidden"));
        assert!(!is_valid_id(" leading"));
        assert!(!is_valid_id("trailing "));
        assert!(!is_valid_id("with\u{0}null"));
    }

    #[test]
    fn unique_id_avoids_collisions() {
        let dir = TempDir::new("unique");
        assert_eq!(unique_id(dir.path(), "sketch"), "sketch");

        save(dir.path(), "sketch", "void draw() {}").expect("保存できる");
        assert_eq!(unique_id(dir.path(), "sketch"), "sketch-2");

        save(dir.path(), "sketch-2", "void draw() {}").expect("保存できる");
        assert_eq!(unique_id(dir.path(), "sketch"), "sketch-3");
    }

    #[test]
    fn save_and_delete_round_trip() {
        let dir = TempDir::new("save-delete");
        save(dir.path(), "demo", "void draw() { background(0); }").expect("保存できる");

        let files = load_all(dir.path());
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].source, "void draw() { background(0); }");

        delete(dir.path(), "demo").expect("消せる");
        assert!(load_all(dir.path()).is_empty());
        // 二度消しても失敗しない。
        delete(dir.path(), "demo").expect("消せる");
    }

    #[test]
    fn save_rejects_an_unusable_id() {
        let dir = TempDir::new("save-invalid");
        assert!(save(dir.path(), "../escape", "void draw() {}").is_err());
        assert!(save(dir.path(), "", "void draw() {}").is_err());
    }

    #[test]
    fn seeding_leaves_an_existing_library_alone() {
        let dir = TempDir::new("seed-existing");
        std::fs::write(dir.path().join("mine.pde"), "void draw() {}").expect("書ける");

        let seeded =
            seed_if_empty(dir.path(), &[("demo", "void draw() {}")]).expect("処理できる");
        assert!(!seeded, "既存の作品があるなら何も書かない");

        let ids: Vec<String> = load_all(dir.path()).into_iter().map(|f| f.id).collect();
        assert_eq!(ids, vec!["mine"], "削除した同梱作品が復活してはいけない");
    }
}
