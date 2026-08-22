//! ログの記録 (設計書 §21 の運用面)。
//!
//! ログは 2 つの読み手を想定している。
//!
//! - **人**。うまく動かないときに、何が起きたかを追う
//! - **道具**。エディタや外部のエージェントが読んで、作品やアプリの直しどころを
//!   見つける
//!
//! 後者があるので、出来事は 1 行 1 件で、後ろの部分だけが自由文になるように
//! 書く。行の頭は必ず `時刻 レベル 種別` で、作品にまつわる行は続けて
//! `id=... line=... column=...` と `鍵=値` を並べる。値に空白か引用符が
//! 混じるときだけ `"..."` で囲む ([`fields`])。
//!
//! ```text
//! 2026-08-14T12:14:52.317Z ERROR sketch id=sketch-12 phase=compile dialect=p5 line=23 column=9 file=".../sketch-12.pde" message="rotate は引数 1 か 4 個で呼びます (2 個渡されています)"
//! ```
//!
//! 置き場所は `<データ領域>/logs/tsubu.log`。大きくなったら `tsubu.log.1` …
//! へ送って捨てる。消えても動作には影響しない。
//!
//! 標準エラーへの出力は今までどおり `env_logger` に任せ、絞り込みの指定
//! (`RUST_LOG`) もそのまま効く。ここはその手前に割り込んで、同じ記録を
//! ファイルへも書く。
//!
//! # レベルの決め方
//!
//! | レベル | 意味 |
//! |---|---|
//! | `error` | 求められたことができなかった。**直す先がある** — 作品のコンパイル失敗、実行の打ち切り、保存できなかった |
//! | `warn` | 続けられるが本来の姿ではない。フォントが無い、設定を読めず既定値にした |
//! | `info` | 順調な進行。既定では出さない |
//! | `debug` | 追跡用の細かい記録 |
//!
//! 「動かない作品がある」は利用者にとってエラーなので `warn` ではなく
//! `error` で書く。ログを読む道具は、まず `ERROR` だけを拾えばよい。

use std::fmt::Write as _;
use std::fs::{File, OpenOptions};
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use log::{Level, Log, Metadata, Record};

use crate::paths::DataPaths;

/// ログファイルの名前。
pub const LOG_FILE: &str = "tsubu.log";

/// 1 本のログファイルの上限。超えたら送り出す。
const MAX_BYTES: u64 = 1 << 20;

/// 取っておく古いログの本数。`tsubu.log.1` … `tsubu.log.3`。
const KEEP: usize = 3;

/// 何も指定が無いときのレベル。
///
/// 既定では **手を入れる先がある出来事だけ** を残す。順調に進んでいることの
/// 記録 (`info`) は、量のわりに読み手の役に立たない。追いたいときは
/// `RUST_LOG=info` や `RUST_LOG=tsubugallery=debug` で上げる。
const DEFAULT_LEVEL: &str = "warn";

/// ログを標準エラーとファイルの両方へ出すようにする。
///
/// 書けない場所 (読み取り専用のデータ領域など) では、標準エラーだけにして
/// 先へ進む。ログが取れないことでアプリが起動できなくなるほうが困る。
///
/// 戻り値は書き込み先。ファイルへ出せなかったときは `None`。
pub fn init(paths: &DataPaths) -> Option<PathBuf> {
    let stderr =
        env_logger::Builder::from_env(env_logger::Env::default().default_filter_or(DEFAULT_LEVEL))
            .build();
    let level = stderr.filter();

    let path = paths.logs().join(LOG_FILE);
    let sink = Sink::open(&path);
    let opened = sink.is_some();

    let logger = Logger {
        stderr,
        sink: sink.map(Mutex::new),
    };
    if log::set_boxed_logger(Box::new(logger)).is_err() {
        // 既に誰かが入れている。テストから二度呼ばれた場合など。
        return None;
    }
    log::set_max_level(level);

    opened.then_some(path)
}

/// パニックもログに残す。
///
/// 既定のフックは標準エラーにしか出さないので、落ちた理由がファイルに残らない。
/// 元のフックはそのあと呼ぶので、端末での見え方は変わらない。
pub fn install_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let message = info
            .payload()
            .downcast_ref::<&str>()
            .map(|s| (*s).to_string())
            .or_else(|| info.payload().downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "詳細不明".to_string());
        let at = match info.location() {
            Some(l) => format!("{}:{}:{}", l.file(), l.line(), l.column()),
            None => "場所不明".to_string(),
        };
        log::error!(target: "panic", "{}", fields(&[("at", &at), ("message", &message)]));
        previous(info);
    }));
}

/// 作品を処理したときの出来事。
///
/// ログを読む道具が、どのファイルの何行目を直せばよいかまで分かるように、
/// 位置と元ファイルまで持つ。
pub struct SketchRecord<'a> {
    /// 作品の識別子。`.pde` のファイル名でもある。
    pub id: &'a str,
    /// どこで起きたか。`compile` / `run` / `thumbnail`。
    pub phase: &'a str,
    /// どちらの方言として読まれたか。分からなければ `None`。
    pub dialect: Option<&'a str>,
    pub line: Option<u32>,
    pub column: Option<u32>,
    /// 元のソースファイル。
    pub source: Option<&'a Path>,
    pub message: &'a str,
}

impl SketchRecord<'_> {
    fn line_text(&self) -> String {
        let mut pairs: Vec<(&str, String)> = vec![
            ("id", self.id.to_string()),
            ("phase", self.phase.to_string()),
        ];
        if let Some(dialect) = self.dialect {
            pairs.push(("dialect", dialect.to_string()));
        }
        if let Some(line) = self.line {
            pairs.push(("line", line.to_string()));
        }
        if let Some(column) = self.column {
            pairs.push(("column", column.to_string()));
        }
        if let Some(source) = self.source {
            // 区切りは `/` に寄せる。Windows の `\` は引用符の中でエスケープが
            // 要るので、そのまま書くと読みにくい。`/` でも開ける。
            pairs.push(("file", source.display().to_string().replace('\\', "/")));
        }
        pairs.push(("message", self.message.to_string()));

        let borrowed: Vec<(&str, &str)> = pairs.iter().map(|(k, v)| (*k, v.as_str())).collect();
        fields(&borrowed)
    }
}

/// 作品が動かなかったことを記録する。
pub fn sketch_failed(record: &SketchRecord<'_>) {
    log::error!(target: "sketch", "{}", record.line_text());
}

/// 作品を読めたことを記録する。数が多いので `debug` で。
pub fn sketch_loaded(id: &str, dialect: Option<&str>, instructions: usize) {
    let instructions = instructions.to_string();
    let mut pairs: Vec<(&str, &str)> = vec![("id", id), ("phase", "compile"), ("result", "ok")];
    if let Some(dialect) = dialect {
        pairs.push(("dialect", dialect));
    }
    pairs.push(("instructions", &instructions));
    log::debug!(target: "sketch", "{}", fields(&pairs));
}

/// `鍵=値` を空白区切りで並べる。
///
/// 値に空白・引用符・改行が混じるときだけ `"..."` で囲み、中の `"` と `\` を
/// エスケープする。囲まないほうが読みやすく、読む側も `key=([^"\s]+|"...")`
/// だけを見ればよくなる。
pub fn fields(pairs: &[(&str, &str)]) -> String {
    let mut out = String::new();
    for (key, value) in pairs {
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(key);
        out.push('=');
        if needs_quotes(value) {
            out.push('"');
            for c in value.chars() {
                match c {
                    '"' => out.push_str("\\\""),
                    '\\' => out.push_str("\\\\"),
                    '\n' => out.push_str("\\n"),
                    '\r' => out.push_str("\\r"),
                    '\t' => out.push_str("\\t"),
                    other => out.push(other),
                }
            }
            out.push('"');
        } else {
            out.push_str(value);
        }
    }
    out
}

fn needs_quotes(value: &str) -> bool {
    value.is_empty()
        || value
            .chars()
            .any(|c| c.is_whitespace() || c == '"' || c == '\\' || c.is_control())
}

/// 標準エラーへの出力の手前に割り込んで、同じ記録をファイルへも書くロガー。
struct Logger {
    stderr: env_logger::Logger,
    /// 開けなかったときは `None`。そのまま標準エラーだけで動く。
    sink: Option<Mutex<Sink>>,
}

impl Log for Logger {
    fn enabled(&self, metadata: &Metadata<'_>) -> bool {
        self.stderr.enabled(metadata)
    }

    fn log(&self, record: &Record<'_>) {
        // GNOME 系の button-layout には `menu` が入ることがあるが、winit が
        // Wayland のクライアント側装飾に使う sctk-adwaita はそのボタンを描かず、
        // 無視したことを warn にする。ウィンドウの動作には影響せず、こちらで
        // 直す先もないので、標準エラーと永続ログの両方からこの 1 件だけ除く。
        if is_known_platform_noise(record) {
            return;
        }
        // 絞り込みの判定は env_logger のものをそのまま使う。ファイルと画面で
        // 出るものが食い違わないようにするため。
        if !self.stderr.matches(record) {
            return;
        }
        if let Some(sink) = &self.sink {
            // ロックが毒されていても、ログのために落ちる必要はない。
            if let Ok(mut sink) = sink.lock() {
                sink.write(&format_line(record));
            }
        }
        self.stderr.log(record);
    }

    fn flush(&self) {
        if let Some(sink) = &self.sink
            && let Ok(mut sink) = sink.lock()
        {
            sink.flush();
        }
        self.stderr.flush();
    }
}

fn is_known_platform_noise(record: &Record<'_>) -> bool {
    record.target() == "sctk_adwaita::buttons"
        && record.args().to_string() == "Ignoring unknown button type: menu"
}

/// ファイルに書く 1 行を作る。改行は含めない。
fn format_line(record: &Record<'_>) -> String {
    let mut line = String::with_capacity(128);
    let _ = write!(
        line,
        "{} {:5} {} {}",
        timestamp(SystemTime::now()),
        level_name(record.level()),
        record.target(),
        record.args()
    );
    // 1 行 1 件を守る。複数行のメッセージも 1 行に畳む。
    line.replace(['\n', '\r'], " ")
}

fn level_name(level: Level) -> &'static str {
    match level {
        Level::Error => "ERROR",
        Level::Warn => "WARN",
        Level::Info => "INFO",
        Level::Debug => "DEBUG",
        Level::Trace => "TRACE",
    }
}

/// 書き込み先のログファイル。大きくなったら送り出す。
struct Sink {
    path: PathBuf,
    file: File,
    /// いま開いているファイルの大きさ。毎回問い合わせない。
    written: u64,
}

impl Sink {
    fn open(path: &Path) -> Option<Self> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).ok()?;
        }
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .ok()?;
        let written = file.metadata().map(|m| m.len()).unwrap_or(0);
        Some(Self {
            path: path.to_path_buf(),
            file,
            written,
        })
    }

    fn write(&mut self, line: &str) {
        if self.written + line.len() as u64 + 1 > MAX_BYTES {
            self.rotate();
        }
        if writeln!(self.file, "{line}").is_ok() {
            self.written += line.len() as u64 + 1;
        }
    }

    /// `tsubu.log` → `tsubu.log.1` → … と送り、一番古いものを捨てる。
    ///
    /// 送れなかったときは、いまのファイルに書き続ける。ログが増え続けるのは
    /// 困るが、書けなくなるほうがもっと困る。
    fn rotate(&mut self) {
        let _ = self.file.flush();
        for n in (1..KEEP).rev() {
            let from = rotated(&self.path, n);
            let to = rotated(&self.path, n + 1);
            if from.exists() {
                let _ = std::fs::rename(&from, &to);
            }
        }
        if std::fs::rename(&self.path, rotated(&self.path, 1)).is_err() {
            return;
        }
        match OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
        {
            Ok(file) => {
                self.file = file;
                self.written = 0;
            }
            // 開き直せないなら、古いほうの手を離さずに書き続ける。
            Err(_) => {
                let _ = std::fs::rename(rotated(&self.path, 1), &self.path);
            }
        }
    }

    fn flush(&mut self) {
        let _ = self.file.flush();
    }
}

fn rotated(path: &Path, n: usize) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(format!(".{n}"));
    path.with_file_name(name)
}

/// `2026-08-14T12:14:52.317Z`。
///
/// 時差を持つ暦は標準ライブラリだけでは組み立てられないので、`env_logger` と
/// 同じく UTC で書く。ログを突き合わせるときは、どちらも同じ物差しになる。
fn timestamp(now: SystemTime) -> String {
    let since = now.duration_since(UNIX_EPOCH).unwrap_or_default();
    let secs = since.as_secs();
    let millis = since.subsec_millis();

    let days = (secs / 86_400) as i64;
    let time = secs % 86_400;
    let (year, month, day) = civil_from_days(days);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}.{millis:03}Z",
        time / 3600,
        (time % 3600) / 60,
        time % 60
    )
}

/// 1970-01-01 からの日数を年月日にする。
///
/// Howard Hinnant の `civil_from_days`。うるう年の規則をそのまま式にしたもので、
/// 表を持たずに済む。
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365; // [0, 399]
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32; // [1, 12]
    (if m <= 2 { y + 1 } else { y }, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn a_timestamp_is_iso_8601_in_utc() {
        assert_eq!(timestamp(UNIX_EPOCH), "1970-01-01T00:00:00.000Z");
        assert_eq!(
            timestamp(UNIX_EPOCH + Duration::from_millis(1_786_707_310_317)),
            "2026-08-14T11:35:10.317Z"
        );
        // うるう日をまたいでもずれない。
        assert_eq!(
            timestamp(UNIX_EPOCH + Duration::from_secs(1_709_164_800)),
            "2024-02-29T00:00:00.000Z"
        );
    }

    #[test]
    fn fields_quote_only_what_needs_it() {
        assert_eq!(fields(&[("id", "sketch-12")]), "id=sketch-12");
        assert_eq!(
            fields(&[("line", "23"), ("column", "9")]),
            "line=23 column=9"
        );
        // 空白が入る値は囲む。読む側は key="..." を 1 つの値として取れる。
        assert_eq!(fields(&[("message", "a b")]), r#"message="a b""#);
        assert_eq!(
            fields(&[("message", "say \"hi\"")]),
            r#"message="say \"hi\"""#
        );
        assert_eq!(
            fields(&[("message", "1 行目\n2 行目")]),
            "message=\"1 行目\\n2 行目\""
        );
        assert_eq!(fields(&[("message", "")]), r#"message="""#);
    }

    #[test]
    fn only_the_harmless_wayland_menu_warning_is_ignored() {
        let menu = Record::builder()
            .target("sctk_adwaita::buttons")
            .args(format_args!("Ignoring unknown button type: menu"))
            .build();
        assert!(is_known_platform_noise(&menu));

        let other_button = Record::builder()
            .target("sctk_adwaita::buttons")
            .args(format_args!("Ignoring unknown button type: future-button"))
            .build();
        assert!(!is_known_platform_noise(&other_button));

        let same_text_from_the_app = Record::builder()
            .target("tsubugallery")
            .args(format_args!("Ignoring unknown button type: menu"))
            .build();
        assert!(!is_known_platform_noise(&same_text_from_the_app));
    }

    #[test]
    fn a_sketch_failure_carries_the_place_to_fix() {
        let record = SketchRecord {
            id: "sketch-12",
            phase: "compile",
            dialect: Some("p5.js"),
            line: Some(23),
            column: Some(9),
            source: Some(Path::new("/data/sketches/sketch-12.pde")),
            message: "rotate は引数 1 か 4 個で呼びます",
        };
        let line = record.line_text();
        assert!(line.starts_with("id=sketch-12 phase=compile dialect=p5.js line=23 column=9"));
        assert!(line.contains("file=/data/sketches/sketch-12.pde"), "{line}");

        // Windows の区切りも `/` に寄せる。囲む必要が無くなり、そのまま開ける。
        let windows = SketchRecord {
            source: Some(Path::new(r"D:\data\sketches\sketch-12.pde")),
            ..record
        };
        assert!(
            windows
                .line_text()
                .contains("file=D:/data/sketches/sketch-12.pde"),
            "{}",
            windows.line_text()
        );
        assert!(
            line.contains(r#"message="rotate は引数 1 か 4 個で呼びます""#),
            "{line}"
        );
    }

    #[test]
    fn a_full_log_line_starts_with_time_level_and_target() {
        let line = format_line(
            &Record::builder()
                .args(format_args!("id=sketch-12"))
                .level(Level::Error)
                .target("sketch")
                .build(),
        );
        let mut parts = line.splitn(4, ' ');
        assert!(parts.next().expect("時刻").ends_with('Z'));
        assert_eq!(parts.next(), Some("ERROR"));
        assert_eq!(parts.next(), Some("sketch"));
        assert_eq!(parts.next(), Some("id=sketch-12"));
    }

    #[test]
    fn a_multi_line_message_stays_on_one_line() {
        let line = format_line(
            &Record::builder()
                .args(format_args!("1 行目\n2 行目"))
                .level(Level::Info)
                .target("app")
                .build(),
        );
        assert!(!line.contains('\n'), "{line}");
    }

    /// テスト用の一時ディレクトリ。`Drop` で片付ける。
    struct TempDir(PathBuf);

    impl TempDir {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir().join(format!("tsubu-log-test-{name}"));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).expect("作れる");
            Self(dir)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn the_log_file_is_appended_to() {
        let dir = TempDir::new("append");
        let path = dir.0.join(LOG_FILE);

        let mut sink = Sink::open(&path).expect("開ける");
        sink.write("1 件目");
        sink.flush();
        drop(sink);

        // 開き直しても上書きしない。前回の起動ぶんが残る。
        let mut sink = Sink::open(&path).expect("開ける");
        sink.write("2 件目");
        sink.flush();

        let text = std::fs::read_to_string(&path).expect("読める");
        assert_eq!(text.lines().collect::<Vec<_>>(), vec!["1 件目", "2 件目"]);
    }

    #[test]
    fn the_log_rotates_and_keeps_only_a_few() {
        let dir = TempDir::new("rotate");
        let path = dir.0.join(LOG_FILE);
        let mut sink = Sink::open(&path).expect("開ける");

        // 上限を超えるまで書く。1 行あたり 100 バイト強。
        let line = "x".repeat(100);
        let rounds = (MAX_BYTES / 100) * (KEEP as u64 + 2);
        for _ in 0..rounds {
            sink.write(&line);
        }
        sink.flush();

        assert!(path.exists(), "いまのログがある");
        assert!(rotated(&path, 1).exists(), "1 つ前のログがある");
        assert!(rotated(&path, KEEP).exists(), "{KEEP} 本目まで残る");
        assert!(
            !rotated(&path, KEEP + 1).exists(),
            "それより古いものは捨てる"
        );

        // 送り出した直後のファイルは上限より小さい。
        let len = std::fs::metadata(&path).expect("読める").len();
        assert!(len < MAX_BYTES, "{len}");
    }
}
