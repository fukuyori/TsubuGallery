//! 多重起動の防止。
//!
//! 同じデータ領域を 2 つのプロセスが同時に触ると、SQLite の書き込みが競り、
//! サムネイルの生成も二重になる。データ領域ごとにファイルロックを 1 本取り、
//! 2 本目が取れなければ起動をやめる。
//!
//! ロックは OS が持つので、異常終了しても残らない。PID ファイルのように
//! 「前回の残骸かどうか」を判定する必要がない。
//!
//! 逆に、データ領域が違えば同時に動く。開発中に `TSUBU_DATA_DIR` を分けて
//! 並べて確かめられる。

use std::fs::{File, TryLockError};
use std::path::{Path, PathBuf};

/// ロックファイルの名前。データ領域の直下に置く。中身は使わない。
pub const LOCK_FILE: &str = "instance.lock";

/// 先に起動しているプロセスの PID を書いておくファイル。
///
/// ロックファイル自身には書けない。Windows のバイト範囲ロックは強制なので、
/// ロックを持っているあいだ、その範囲は別のハンドルから読もうとしても
/// `ERROR_LOCK_VIOLATION` で断られる。POSIX の勧告的ロックとはそこが違い、
/// 同じファイルへ書くと Windows でだけ PID を読み出せない。
///
/// ロックの判定には使わない。異常終了すればこのファイルは残るが、起動できる
/// かどうかは OS が持つロックだけで決まるので、古い PID が見えるだけで済む。
pub const PID_FILE: &str = "instance.pid";

#[derive(Debug, thiserror::Error)]
pub enum LockError {
    /// すでに同じデータ領域を使っているプロセスがある。
    #[error("すでに起動しています")]
    AlreadyRunning {
        /// 先に起動しているプロセスの PID。読めなければ `None`。
        pid: Option<u32>,
    },
    #[error("ロックファイルを扱えませんでした: {0}")]
    Io(#[from] std::io::Error),
}

/// 起動中であることの印。落とすとロックが外れる。
///
/// アプリが動いているあいだ持ち続ける必要がある。受け取ってすぐ捨てると、
/// その場でロックが外れて意味がなくなる。
#[derive(Debug)]
pub struct InstanceLock {
    // ファイルを開いたままにしておくことがロックの実体。
    file: File,
    path: PathBuf,
    /// [`PID_FILE`] の場所。外すときに消す。
    pid_path: PathBuf,
}

impl InstanceLock {
    /// `dir` のロックを取る。すでに誰かが持っていれば
    /// [`LockError::AlreadyRunning`]。
    pub fn acquire(dir: &Path) -> Result<Self, LockError> {
        std::fs::create_dir_all(dir)?;
        let path = dir.join(LOCK_FILE);
        let pid_path = dir.join(PID_FILE);

        let file = File::options()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&path)?;

        match file.try_lock() {
            Ok(()) => {}
            Err(TryLockError::WouldBlock) => {
                return Err(LockError::AlreadyRunning { pid: read_pid(&pid_path) });
            }
            Err(TryLockError::Error(e)) => return Err(LockError::Io(e)),
        }

        // ロックファイルの中身は使わない。昔の版が書いた PID が残っていても
        // 紛らわしいだけなので空にしておく。
        let _ = file.set_len(0);

        // 取れてから自分の PID を隣へ書く。困ったときに誰が握っているか分かる。
        // 目安でしかないので、書けなくても起動は続ける。
        let _ = std::fs::write(&pid_path, std::process::id().to_string());

        Ok(Self { file, path, pid_path })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for InstanceLock {
    fn drop(&mut self) {
        // ロックはファイルを閉じた時点でも外れるが、PID を消す前に外して
        // おく。「ロックは無いのに PID がある」という隙間を作らないため。
        let _ = self.file.unlock();
        let _ = std::fs::remove_file(&self.pid_path);
    }
}

/// [`PID_FILE`] に書かれた PID。読めなければ `None`。
fn read_pid(pid_path: &Path) -> Option<u32> {
    std::fs::read_to_string(pid_path).ok()?.trim().parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("tsubu-lock-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn the_second_instance_is_refused() {
        let dir = temp_dir("second");
        let first = InstanceLock::acquire(&dir).expect("1 本目は取れる");

        match InstanceLock::acquire(&dir) {
            Err(LockError::AlreadyRunning { pid }) => {
                assert_eq!(pid, Some(std::process::id()), "誰が握っているか分かるはず");
            }
            Err(e) => panic!("想定と違う失敗: {e}"),
            Ok(_) => panic!("2 本目が取れてしまいました"),
        }

        drop(first);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// PID はロックファイルとは別に置く。Windows のバイト範囲ロックは強制
    /// なので、ロック中のファイルは別のハンドルから読めない。同じファイルへ
    /// 書くと、Windows でだけ PID が読み出せなくなる。
    #[test]
    fn the_pid_is_readable_while_the_lock_is_held() {
        let dir = temp_dir("pid-readable");
        let _first = InstanceLock::acquire(&dir).expect("1 本目は取れる");

        let text = std::fs::read_to_string(dir.join(PID_FILE)).expect("握ったままでも読める");
        assert_eq!(text.trim().parse::<u32>().ok(), Some(std::process::id()));

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 外したら PID は残さない。止まっているのに残っていると紛らわしい。
    #[test]
    fn the_pid_is_cleared_when_the_lock_is_dropped() {
        let dir = temp_dir("pid-cleared");
        {
            let _guard = InstanceLock::acquire(&dir).expect("1 本目");
        }
        assert!(!dir.join(PID_FILE).exists(), "外したのに PID が残っている");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 昔の版はロックファイルへ PID を書いていた。残っていても読まないが、
    /// 紛らわしいので空にしてから使う。
    #[test]
    fn an_old_pid_left_in_the_lock_file_is_wiped() {
        let dir = temp_dir("old-pid");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(LOCK_FILE), "99999").unwrap();

        let _first = InstanceLock::acquire(&dir).expect("取れる");
        assert_eq!(
            std::fs::metadata(dir.join(LOCK_FILE)).unwrap().len(),
            0,
            "ロックファイルは空にしておく"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 前のプロセスが終われば、次は起動できる。
    #[test]
    fn the_lock_is_released_when_dropped() {
        let dir = temp_dir("release");
        {
            let _guard = InstanceLock::acquire(&dir).expect("1 本目");
        }
        let _second = InstanceLock::acquire(&dir).expect("外れているはず");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// データ領域が違えば同時に動く。
    #[test]
    fn different_data_directories_do_not_collide() {
        let a = temp_dir("dir-a");
        let b = temp_dir("dir-b");
        let _first = InstanceLock::acquire(&a).expect("A");
        let _second = InstanceLock::acquire(&b).expect("B は別領域なので取れる");
        let _ = std::fs::remove_dir_all(&a);
        let _ = std::fs::remove_dir_all(&b);
    }

    /// 無いディレクトリでも作ってから取る。初回起動がこれ。
    #[test]
    fn a_missing_directory_is_created() {
        let dir = temp_dir("missing").join("deep").join("er");
        let guard = InstanceLock::acquire(&dir).expect("作られるはず");
        assert!(guard.path().exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 中身が壊れていてもロックとしては働く。PID は目安でしかない。
    #[test]
    fn a_garbled_lock_file_still_locks() {
        let dir = temp_dir("garbled");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(LOCK_FILE), "これは PID ではない").unwrap();

        let _first = InstanceLock::acquire(&dir).expect("取れる");
        match InstanceLock::acquire(&dir) {
            Err(LockError::AlreadyRunning { pid }) => assert_eq!(pid, Some(std::process::id())),
            other => panic!("{other:?}"),
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
