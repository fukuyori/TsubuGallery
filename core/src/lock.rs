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
use std::io::{Read, Seek, Write};
use std::path::{Path, PathBuf};

/// ロックファイルの名前。データ領域の直下に置く。
pub const LOCK_FILE: &str = "instance.lock";

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
}

impl InstanceLock {
    /// `dir` のロックを取る。すでに誰かが持っていれば
    /// [`LockError::AlreadyRunning`]。
    pub fn acquire(dir: &Path) -> Result<Self, LockError> {
        std::fs::create_dir_all(dir)?;
        let path = dir.join(LOCK_FILE);

        // 先に起動しているプロセスの PID を読めるよう、truncate はしない。
        let mut file = File::options()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&path)?;

        match file.try_lock() {
            Ok(()) => {}
            Err(TryLockError::WouldBlock) => {
                return Err(LockError::AlreadyRunning { pid: read_pid(&path) });
            }
            Err(TryLockError::Error(e)) => return Err(LockError::Io(e)),
        }

        // 取れてから自分の PID を書く。困ったときに誰が握っているか分かる。
        // 中身は目安で、ロックの判定には使わない。
        file.set_len(0)?;
        file.rewind()?;
        let _ = write!(file, "{}", std::process::id());
        let _ = file.flush();

        Ok(Self { file, path })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for InstanceLock {
    fn drop(&mut self) {
        // ロックはファイルを閉じた時点で外れる。中身だけ消しておくと、
        // 止まっているのに PID が残っていて紛らわしい、という状態を避けられる。
        let _ = self.file.set_len(0);
    }
}

/// ロックファイルに書かれた PID。読めなければ `None`。
fn read_pid(path: &Path) -> Option<u32> {
    let mut text = String::new();
    File::open(path).ok()?.read_to_string(&mut text).ok()?;
    text.trim().parse().ok()
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
