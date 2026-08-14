//! 作品メタデータの永続化 (設計書 §19)。
//!
//! # ソースはファイル、メタデータは DB
//!
//! 設計書 §19.1 は `source` も `Sketch` テーブルに置いているが、ここでは
//! `.pde` ファイルを正とし、DB にはそれ以外のメタデータだけを持たせている。
//! 理由は次の 3 つ。
//!
//! - 好きなエディタで書ける。作品は短いテキストなので、アプリを起動しないと
//!   触れないほうが不便になる。
//! - DB が壊れてもユーザーの作品は失われない。作り直せるのは付随情報だけ。
//! - サムネイルを DB へ入れない判断 (§7.3) と同じ理由が、ソースにも当てはまる。
//!
//! テーブルの列は §19.1 に合わせてある。`source` だけが `.pde` の側にある。

use std::collections::{BTreeSet, HashMap};
use std::path::Path;

use rusqlite::{Connection, params};

use crate::settings::Settings;

pub type Result<T> = std::result::Result<T, rusqlite::Error>;

/// 作品 1 件のメタデータ。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SketchMeta {
    pub id: String,
    pub title: String,
    /// 作者。設計書 §19.1 には無いが、つぶやきの作品は出どころが大事なので足した。
    pub author: String,
    /// 元の投稿などへのリンク。`http://` か `https://` だけを受ける。
    pub link: String,
    pub favorite: bool,
    /// ソースのハッシュ。変わっていなければコンパイル結果を信用してよい。
    pub compile_hash: String,
    pub compile_status: CompileStatus,
    /// サムネイルを取得するフレーム (設計書 §7.1)。
    pub thumbnail_frame: u64,
    /// 追加した時刻 (UNIX 秒)。
    pub created_at: i64,
    pub updated_at: i64,
    /// 最後に Viewer で開いた時刻。未表示なら `None`。
    pub last_opened_at: Option<i64>,
    pub tags: BTreeSet<String>,
    /// 所属するコレクション (設計書 §27)。
    ///
    /// 読み取り専用。[`Repository::upsert`] はここを見ない。書き換えるには
    /// [`Repository::add_to_collection`] と [`Repository::remove_from_collection`]
    /// を使う。タグと違って順序を持つので、まとめて入れ替える形にしていない。
    pub collections: BTreeSet<String>,
}

impl SketchMeta {
    pub fn new(id: impl Into<String>, title: impl Into<String>, now: i64) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            author: String::new(),
            link: String::new(),
            favorite: false,
            compile_hash: String::new(),
            compile_status: CompileStatus::Unknown,
            thumbnail_frame: 90,
            created_at: now,
            updated_at: now,
            last_opened_at: None,
            tags: BTreeSet::new(),
            collections: BTreeSet::new(),
        }
    }
}

/// 保存時コンパイルの結果 (設計書 §19.1 の `compile_status`)。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CompileStatus {
    /// まだ一度もコンパイルしていない。
    Unknown,
    Ok,
    Error(String),
}

impl CompileStatus {
    fn to_row(&self) -> (i64, Option<&str>) {
        match self {
            CompileStatus::Unknown => (0, None),
            CompileStatus::Ok => (1, None),
            CompileStatus::Error(m) => (2, Some(m.as_str())),
        }
    }

    fn from_row(code: i64, message: Option<String>) -> Self {
        match code {
            1 => CompileStatus::Ok,
            2 => CompileStatus::Error(message.unwrap_or_default()),
            _ => CompileStatus::Unknown,
        }
    }
}

/// 作品メタデータの入れ物。
pub struct Repository {
    conn: Connection,
}

impl Repository {
    /// ファイルを開く。無ければ作る。
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        Self::from_connection(Connection::open(path)?)
    }

    /// テスト用のメモリ DB。
    pub fn in_memory() -> Result<Self> {
        Self::from_connection(Connection::open_in_memory()?)
    }

    fn from_connection(conn: Connection) -> Result<Self> {
        // 予期しない終了でメタデータを壊さないため WAL にする。
        // メモリ DB では効かないので、失敗しても無視してよい。
        let _ = conn.pragma_update(None, "journal_mode", "WAL");
        conn.pragma_update(None, "foreign_keys", "ON")?;

        let repo = Self { conn };
        repo.migrate()?;
        Ok(repo)
    }

    /// スキーマを最新にする。`user_version` を版として使う。
    fn migrate(&self) -> Result<()> {
        let version: i64 =
            self.conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;

        if version < 1 {
            self.conn.execute_batch(
                "
                CREATE TABLE sketch (
                    id              TEXT PRIMARY KEY,
                    title           TEXT NOT NULL,
                    favorite        INTEGER NOT NULL DEFAULT 0,
                    compile_hash    TEXT NOT NULL DEFAULT '',
                    compile_status  INTEGER NOT NULL DEFAULT 0,
                    compile_message TEXT,
                    thumbnail_frame INTEGER NOT NULL DEFAULT 90,
                    created_at      INTEGER NOT NULL,
                    updated_at      INTEGER NOT NULL,
                    last_opened_at  INTEGER
                );

                -- タグは多対多 (設計書 §19.2)。
                CREATE TABLE tag (
                    name TEXT PRIMARY KEY
                );

                -- 改名で id が変わるので ON UPDATE CASCADE が要る。
                CREATE TABLE sketch_tag (
                    sketch_id TEXT NOT NULL
                        REFERENCES sketch(id) ON DELETE CASCADE ON UPDATE CASCADE,
                    tag_name  TEXT NOT NULL
                        REFERENCES tag(name) ON DELETE CASCADE ON UPDATE CASCADE,
                    PRIMARY KEY (sketch_id, tag_name)
                );

                CREATE INDEX idx_sketch_tag_tag ON sketch_tag(tag_name);

                PRAGMA user_version = 1;
                ",
            )?;
        }

        if version < 2 {
            // 設定は値も文字列で持つ。項目を足すたびに移行を書かずに済む
            // (設計書 §24)。
            self.conn.execute_batch(
                "
                CREATE TABLE setting (
                    key   TEXT PRIMARY KEY,
                    value TEXT NOT NULL
                );

                PRAGMA user_version = 2;
                ",
            )?;
        }

        if version < 3 {
            // コレクション (設計書 §27 のプレイリスト)。タグと同じく多対多だが、
            // 並べ替えたいので位置を持つ。
            self.conn.execute_batch(
                "
                CREATE TABLE collection (
                    name       TEXT PRIMARY KEY,
                    created_at INTEGER NOT NULL
                );

                CREATE TABLE collection_sketch (
                    collection_name TEXT NOT NULL
                        REFERENCES collection(name) ON DELETE CASCADE ON UPDATE CASCADE,
                    sketch_id       TEXT NOT NULL
                        REFERENCES sketch(id) ON DELETE CASCADE ON UPDATE CASCADE,
                    position        INTEGER NOT NULL,
                    PRIMARY KEY (collection_name, sketch_id)
                );

                CREATE INDEX idx_collection_sketch_sketch ON collection_sketch(sketch_id);

                PRAGMA user_version = 3;
                ",
            )?;
        }

        if version < 4 {
            // 作者とリンク。既存の行は空文字列で始まる。
            self.conn.execute_batch(
                "
                ALTER TABLE sketch ADD COLUMN author TEXT NOT NULL DEFAULT '';
                ALTER TABLE sketch ADD COLUMN link   TEXT NOT NULL DEFAULT '';

                PRAGMA user_version = 4;
                ",
            )?;
        }

        Ok(())
    }

    // ---- コレクション (設計書 §27) --------------------------------------

    /// 全コレクション名。作った順ではなく名前順で返す。
    pub fn collections(&self) -> Result<Vec<String>> {
        let mut stmt = self.conn.prepare("SELECT name FROM collection ORDER BY name")?;
        let rows = stmt.query_map([], |row| row.get(0))?;
        rows.collect()
    }

    /// コレクションを作る。すでにあれば何もしない。
    pub fn create_collection(&mut self, name: &str, now: i64) -> Result<()> {
        self.conn.execute(
            "INSERT INTO collection (name, created_at) VALUES (?1, ?2)
             ON CONFLICT(name) DO NOTHING",
            params![name, now],
        )?;
        Ok(())
    }

    pub fn delete_collection(&mut self, name: &str) -> Result<()> {
        self.conn.execute("DELETE FROM collection WHERE name = ?1", params![name])?;
        Ok(())
    }

    /// 作品をコレクションへ入れる。すでに入っていれば位置も変えない。
    pub fn add_to_collection(&mut self, name: &str, sketch_id: &str, now: i64) -> Result<()> {
        let tx = self.conn.transaction()?;
        tx.execute(
            "INSERT INTO collection (name, created_at) VALUES (?1, ?2)
             ON CONFLICT(name) DO NOTHING",
            params![name, now],
        )?;
        // 末尾に足す。並べ替えの UI はまだ無いが、順序は持てるようにしておく。
        tx.execute(
            "INSERT INTO collection_sketch (collection_name, sketch_id, position)
             VALUES (?1, ?2, (SELECT coalesce(max(position), -1) + 1
                              FROM collection_sketch WHERE collection_name = ?1))
             ON CONFLICT(collection_name, sketch_id) DO NOTHING",
            params![name, sketch_id],
        )?;
        tx.commit()
    }

    pub fn remove_from_collection(&mut self, name: &str, sketch_id: &str) -> Result<()> {
        self.conn.execute(
            "DELETE FROM collection_sketch WHERE collection_name = ?1 AND sketch_id = ?2",
            params![name, sketch_id],
        )?;
        Ok(())
    }

    /// 作品 id ごとの所属コレクション。
    fn collection_memberships(&self) -> Result<HashMap<String, BTreeSet<String>>> {
        let mut stmt = self
            .conn
            .prepare("SELECT sketch_id, collection_name FROM collection_sketch")?;
        let rows = stmt
            .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))?;
        let mut out: HashMap<String, BTreeSet<String>> = HashMap::new();
        for row in rows {
            let (id, name) = row?;
            out.entry(id).or_default().insert(name);
        }
        Ok(out)
    }

    /// 保存されている設定を読む。1 件も無ければ既定値。
    pub fn settings(&self) -> Result<Settings> {
        let mut stmt = self.conn.prepare("SELECT key, value FROM setting")?;
        let rows = stmt
            .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))?
            .collect::<Result<Vec<_>>>()?;
        Ok(Settings::from_pairs(rows.iter().map(|(k, v)| (k.as_str(), v.as_str()))))
    }

    /// 設定を丸ごと書く。触っていない項目も書き直すが、13 行なので気にしない。
    pub fn save_settings(&mut self, settings: &Settings) -> Result<()> {
        let tx = self.conn.transaction()?;
        {
            let mut stmt = tx.prepare(
                "INSERT INTO setting (key, value) VALUES (?1, ?2)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            )?;
            for (key, value) in settings.to_pairs() {
                stmt.execute(rusqlite::params![key, value])?;
            }
        }
        tx.commit()
    }

    /// すべての作品メタデータを id 順で返す。
    pub fn all(&self) -> Result<Vec<SketchMeta>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, title, favorite, compile_hash, compile_status, compile_message,
                    thumbnail_frame, created_at, updated_at, last_opened_at, author, link
             FROM sketch ORDER BY id",
        )?;

        let mut sketches: Vec<SketchMeta> = stmt
            .query_map([], |row| {
                Ok(SketchMeta {
                    id: row.get(0)?,
                    title: row.get(1)?,
                    favorite: row.get::<_, i64>(2)? != 0,
                    compile_hash: row.get(3)?,
                    compile_status: CompileStatus::from_row(row.get(4)?, row.get(5)?),
                    thumbnail_frame: row.get::<_, i64>(6)?.max(0) as u64,
                    created_at: row.get(7)?,
                    updated_at: row.get(8)?,
                    last_opened_at: row.get(9)?,
                    author: row.get(10)?,
                    link: row.get(11)?,
                    tags: BTreeSet::new(),
                    collections: BTreeSet::new(),
                })
            })?
            .collect::<Result<_>>()?;

        // タグは 1 回のクエリでまとめて引き、件数分の往復を避ける。
        let mut tags: HashMap<String, BTreeSet<String>> = HashMap::new();
        let mut stmt = self.conn.prepare("SELECT sketch_id, tag_name FROM sketch_tag")?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        for row in rows {
            let (id, tag) = row?;
            tags.entry(id).or_default().insert(tag);
        }
        let mut memberships = self.collection_memberships()?;
        for sketch in &mut sketches {
            if let Some(t) = tags.remove(&sketch.id) {
                sketch.tags = t;
            }
            if let Some(c) = memberships.remove(&sketch.id) {
                sketch.collections = c;
            }
        }

        Ok(sketches)
    }

    /// 追加または更新する。タグも入れ替える。
    pub fn upsert(&mut self, meta: &SketchMeta) -> Result<()> {
        let tx = self.conn.transaction()?;
        let (status, message) = meta.compile_status.to_row();

        tx.execute(
            "INSERT INTO sketch
                (id, title, favorite, compile_hash, compile_status, compile_message,
                 thumbnail_frame, created_at, updated_at, last_opened_at, author, link)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
             ON CONFLICT(id) DO UPDATE SET
                title = excluded.title,
                favorite = excluded.favorite,
                compile_hash = excluded.compile_hash,
                compile_status = excluded.compile_status,
                compile_message = excluded.compile_message,
                thumbnail_frame = excluded.thumbnail_frame,
                updated_at = excluded.updated_at,
                last_opened_at = excluded.last_opened_at,
                author = excluded.author,
                link = excluded.link",
            params![
                meta.id,
                meta.title,
                meta.favorite as i64,
                meta.compile_hash,
                status,
                message,
                meta.thumbnail_frame as i64,
                meta.created_at,
                meta.updated_at,
                meta.last_opened_at,
                meta.author,
                meta.link,
            ],
        )?;

        tx.execute("DELETE FROM sketch_tag WHERE sketch_id = ?1", params![meta.id])?;
        for tag in &meta.tags {
            tx.execute("INSERT OR IGNORE INTO tag(name) VALUES (?1)", params![tag])?;
            tx.execute(
                "INSERT OR IGNORE INTO sketch_tag(sketch_id, tag_name) VALUES (?1, ?2)",
                params![meta.id, tag],
            )?;
        }

        tx.commit()
    }

    /// 1 件取り出す。
    pub fn get(&self, id: &str) -> Result<Option<SketchMeta>> {
        Ok(self.all()?.into_iter().find(|m| m.id == id))
    }

    pub fn delete(&mut self, id: &str) -> Result<()> {
        self.conn.execute("DELETE FROM sketch WHERE id = ?1", params![id])?;
        self.prune_tags()
    }

    /// 名前を変える。タグや日時は引き継ぐ。
    ///
    /// `sketch_tag` の参照は `ON UPDATE CASCADE` が付いて回るので、ここでは
    /// `sketch` の行だけ触ればよい。
    pub fn rename(&mut self, from: &str, to: &str, title: &str, now: i64) -> Result<()> {
        self.conn.execute(
            "UPDATE sketch SET id = ?2, title = ?3, updated_at = ?4 WHERE id = ?1",
            params![from, to, title, now],
        )?;
        Ok(())
    }

    /// 表示した時刻を記録する (設計書 §20 の「最近表示」)。
    pub fn touch_opened(&mut self, id: &str, now: i64) -> Result<()> {
        self.conn
            .execute("UPDATE sketch SET last_opened_at = ?2 WHERE id = ?1", params![id, now])?;
        Ok(())
    }

    pub fn set_favorite(&mut self, id: &str, favorite: bool, now: i64) -> Result<()> {
        self.conn.execute(
            "UPDATE sketch SET favorite = ?2, updated_at = ?3 WHERE id = ?1",
            params![id, favorite as i64, now],
        )?;
        Ok(())
    }

    /// 使われている全タグ。
    pub fn tags(&self) -> Result<Vec<String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT DISTINCT tag_name FROM sketch_tag ORDER BY tag_name")?;
        let tags = stmt.query_map([], |row| row.get(0))?.collect::<Result<_>>()?;
        Ok(tags)
    }

    /// DB にあってファイルに無い作品を消す。
    ///
    /// アプリの外で `.pde` を消したときに、メタデータだけが残らないようにする。
    pub fn retain(&mut self, existing_ids: &[String]) -> Result<usize> {
        let known: Vec<String> = self.all()?.into_iter().map(|m| m.id).collect();
        let mut removed = 0;
        for id in known {
            if !existing_ids.contains(&id) {
                self.conn.execute("DELETE FROM sketch WHERE id = ?1", params![id])?;
                removed += 1;
            }
        }
        if removed > 0 {
            self.prune_tags()?;
        }
        Ok(removed)
    }

    /// どの作品にも付いていないタグを消す。
    fn prune_tags(&self) -> Result<()> {
        self.conn.execute(
            "DELETE FROM tag WHERE name NOT IN (SELECT tag_name FROM sketch_tag)",
            [],
        )?;
        Ok(())
    }

    /// 行数。テストと診断用。
    pub fn count(&self) -> Result<usize> {
        let n: i64 = self.conn.query_row("SELECT COUNT(*) FROM sketch", [], |r| r.get(0))?;
        Ok(n as usize)
    }

    /// スキーマの版。不具合の切り分け用に外へ出しておく。
    pub fn schema_version(&self) -> Result<i64> {
        self.conn.query_row("PRAGMA user_version", [], |row| row.get(0))
    }
}

/// ソースのハッシュ。変更検知にだけ使うので、暗号強度は要らない。
pub fn source_hash(source: &str) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in source.as_bytes() {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}

/// 現在時刻 (UNIX 秒)。取れなければ 0。
pub fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo() -> Repository {
        Repository::in_memory().expect("メモリ DB を開ける")
    }

    fn meta(id: &str) -> SketchMeta {
        SketchMeta::new(id, super::super::library::title_from_id(id), 1000)
    }

    #[test]
    fn a_fresh_database_is_migrated() {
        let r = repo();
        assert_eq!(r.schema_version().expect("読める"), 4);
        assert_eq!(r.count().expect("数えられる"), 0);
    }

    #[test]
    fn author_and_link_survive_a_save_and_load() {
        let mut repo = repo();
        let mut meta = SketchMeta::new("a", "A", 1);
        meta.author = "だれか".into();
        meta.link = "https://example.com/status/1".into();
        repo.upsert(&meta).unwrap();

        let saved = repo.get("a").unwrap().expect("ある");
        assert_eq!(saved.author, "だれか");
        assert_eq!(saved.link, "https://example.com/status/1");

        // 改名しても付いて回る。
        repo.rename("a", "b", "B", 2).unwrap();
        let renamed = repo.get("b").unwrap().expect("ある");
        assert_eq!(renamed.author, "だれか");
    }

    #[test]
    fn a_collection_holds_sketches() {
        let mut repo = repo();
        repo.upsert(&SketchMeta::new("a", "A", 1)).unwrap();
        repo.upsert(&SketchMeta::new("b", "B", 1)).unwrap();

        repo.add_to_collection("お気に入り集", "a", 1).unwrap();
        repo.add_to_collection("お気に入り集", "b", 1).unwrap();
        assert_eq!(repo.collections().unwrap(), vec!["お気に入り集".to_string()]);

        let all = repo.all().unwrap();
        assert!(all[0].collections.contains("お気に入り集"));
        assert!(all[1].collections.contains("お気に入り集"));

        repo.remove_from_collection("お気に入り集", "b").unwrap();
        let all = repo.all().unwrap();
        assert!(all[0].collections.contains("お気に入り集"));
        assert!(all[1].collections.is_empty());
    }

    /// 同じ作品を二度入れても増えない。
    #[test]
    fn adding_twice_is_harmless() {
        let mut repo = repo();
        repo.upsert(&SketchMeta::new("a", "A", 1)).unwrap();
        repo.add_to_collection("c", "a", 1).unwrap();
        repo.add_to_collection("c", "a", 1).unwrap();

        let count: i64 = repo
            .conn
            .query_row("SELECT count(*) FROM collection_sketch", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1);
    }

    /// 作品を消したら、コレクションからも消える。
    #[test]
    fn deleting_a_sketch_takes_it_out_of_collections() {
        let mut repo = repo();
        repo.upsert(&SketchMeta::new("a", "A", 1)).unwrap();
        repo.add_to_collection("c", "a", 1).unwrap();
        repo.delete("a").unwrap();

        let count: i64 = repo
            .conn
            .query_row("SELECT count(*) FROM collection_sketch", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 0, "参照が残っています");
        assert_eq!(repo.collections().unwrap(), vec!["c".to_string()], "コレクション自体は残す");
    }

    /// 名前を変えても所属は付いて回る。
    #[test]
    fn renaming_a_sketch_keeps_its_collections() {
        let mut repo = repo();
        repo.upsert(&SketchMeta::new("a", "A", 1)).unwrap();
        repo.add_to_collection("c", "a", 1).unwrap();
        repo.rename("a", "z", "Z", 2).unwrap();

        let all = repo.all().unwrap();
        assert_eq!(all[0].id, "z");
        assert!(all[0].collections.contains("c"), "改名で所属が切れました");
    }

    /// コレクションを消しても作品は消えない。
    #[test]
    fn deleting_a_collection_keeps_the_sketches() {
        let mut repo = repo();
        repo.upsert(&SketchMeta::new("a", "A", 1)).unwrap();
        repo.add_to_collection("c", "a", 1).unwrap();
        repo.delete_collection("c").unwrap();

        assert_eq!(repo.count().unwrap(), 1);
        assert!(repo.all().unwrap()[0].collections.is_empty());
        assert!(repo.collections().unwrap().is_empty());
    }

    #[test]
    fn settings_survive_a_save_and_load() {
        use crate::settings::{CardSize, Settings, Theme};

        let mut repo = Repository::in_memory().unwrap();
        assert_eq!(repo.settings().unwrap(), Settings::default(), "初回は既定値");

        let settings = Settings {
            theme: Theme::Light,
            card_size: CardSize::Large,
            capture_frame: 42,
            ..Settings::default()
        };
        repo.save_settings(&settings).unwrap();

        assert_eq!(repo.settings().unwrap(), settings);

        // 2 回目の保存で行が増えないこと。
        repo.save_settings(&settings).unwrap();
        let count: i64 =
            repo.conn.query_row("SELECT count(*) FROM setting", [], |r| r.get(0)).unwrap();
        assert_eq!(count as usize, settings.to_pairs().len());
    }

    #[test]
    fn migrating_twice_is_harmless() {
        let r = repo();
        r.migrate().expect("2 回目も通る");
        assert_eq!(r.schema_version().expect("読める"), 4);
    }

    #[test]
    fn upsert_inserts_then_updates() {
        let mut r = repo();
        r.upsert(&meta("spiral")).expect("入る");
        assert_eq!(r.count().expect("数えられる"), 1);

        let mut m = meta("spiral");
        m.favorite = true;
        m.title = "変えた".into();
        r.upsert(&m).expect("更新できる");

        assert_eq!(r.count().expect("数えられる"), 1, "重複して増えない");
        let got = r.get("spiral").expect("引ける").expect("ある");
        assert!(got.favorite);
        assert_eq!(got.title, "変えた");
    }

    #[test]
    fn favorites_survive_a_reopen() {
        let dir = std::env::temp_dir().join("tsubu-repo-test-favorite");
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("library.sqlite3");

        {
            let mut r = Repository::open(&path).expect("開ける");
            r.upsert(&meta("spiral")).expect("入る");
            r.set_favorite("spiral", true, 2000).expect("付けられる");
        }
        {
            let r = Repository::open(&path).expect("開き直せる");
            assert!(r.get("spiral").expect("引ける").expect("ある").favorite);
        }

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn tags_round_trip_and_are_shared() {
        let mut r = repo();
        let mut a = meta("a");
        a.tags.insert("circles".into());
        a.tags.insert("monochrome".into());
        let mut b = meta("b");
        b.tags.insert("circles".into());

        r.upsert(&a).expect("入る");
        r.upsert(&b).expect("入る");

        assert_eq!(r.tags().expect("引ける"), vec!["circles", "monochrome"]);
        let got = r.get("a").expect("引ける").expect("ある");
        assert_eq!(got.tags.iter().cloned().collect::<Vec<_>>(), vec!["circles", "monochrome"]);
    }

    #[test]
    fn replacing_tags_removes_the_old_ones() {
        let mut r = repo();
        let mut m = meta("a");
        m.tags.insert("old".into());
        r.upsert(&m).expect("入る");

        m.tags.clear();
        m.tags.insert("new".into());
        r.upsert(&m).expect("更新できる");

        let got = r.get("a").expect("引ける").expect("ある");
        assert_eq!(got.tags.iter().cloned().collect::<Vec<_>>(), vec!["new"]);
    }

    #[test]
    fn deleting_a_sketch_takes_its_tags_with_it() {
        let mut r = repo();
        let mut m = meta("a");
        m.tags.insert("lonely".into());
        r.upsert(&m).expect("入る");

        r.delete("a").expect("消せる");
        assert_eq!(r.count().expect("数えられる"), 0);
        assert!(r.tags().expect("引ける").is_empty(), "誰も使っていないタグは残さない");
    }

    #[test]
    fn renaming_keeps_tags_and_timestamps() {
        let mut r = repo();
        let mut m = meta("before");
        m.tags.insert("keep".into());
        r.upsert(&m).expect("入る");

        r.rename("before", "after", "After", 3000).expect("改名できる");

        assert!(r.get("before").expect("引ける").is_none());
        let got = r.get("after").expect("引ける").expect("ある");
        assert_eq!(got.title, "After");
        assert_eq!(got.created_at, 1000, "作成日時は引き継ぐ");
        assert_eq!(got.updated_at, 3000);
        assert_eq!(got.tags.iter().cloned().collect::<Vec<_>>(), vec!["keep"]);
    }

    #[test]
    fn retain_drops_rows_whose_file_is_gone() {
        let mut r = repo();
        r.upsert(&meta("kept")).expect("入る");
        r.upsert(&meta("removed")).expect("入る");

        let removed = r.retain(&["kept".to_string()]).expect("整理できる");
        assert_eq!(removed, 1);
        assert!(r.get("removed").expect("引ける").is_none());
        assert!(r.get("kept").expect("引ける").is_some());
    }

    #[test]
    fn compile_status_round_trips() {
        let mut r = repo();
        let mut m = meta("a");
        m.compile_status = CompileStatus::Error("3行3列: `;` がありません".into());
        r.upsert(&m).expect("入る");

        let got = r.get("a").expect("引ける").expect("ある");
        assert_eq!(got.compile_status, CompileStatus::Error("3行3列: `;` がありません".into()));

        m.compile_status = CompileStatus::Ok;
        r.upsert(&m).expect("更新できる");
        assert_eq!(r.get("a").expect("引ける").expect("ある").compile_status, CompileStatus::Ok);
    }

    #[test]
    fn touch_opened_records_the_time() {
        let mut r = repo();
        r.upsert(&meta("a")).expect("入る");
        assert!(r.get("a").expect("引ける").expect("ある").last_opened_at.is_none());

        r.touch_opened("a", 4242).expect("記録できる");
        assert_eq!(r.get("a").expect("引ける").expect("ある").last_opened_at, Some(4242));
    }

    #[test]
    fn the_source_hash_detects_changes() {
        assert_eq!(source_hash("void draw() {}"), source_hash("void draw() {}"));
        assert_ne!(source_hash("void draw() {}"), source_hash("void draw() { }"));
    }
}
