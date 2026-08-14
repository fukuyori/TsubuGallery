//! リンクをブラウザで開く。
//!
//! 開くのはユーザーが押したときだけで、作品のコードからは触れない
//! (設計書 §21 の砂場はそのまま)。
//!
//! URL は作品のメタデータから来る。外部のプログラムへ渡す値なので、
//! `http` と `https` 以外は開かない。`file:` や `javascript:` を開けてしまうと、
//! 作品を配った人が受け取った人の環境で何かを起こせることになる。

use std::process::Command;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum OpenError {
    #[error("http:// か https:// のリンクだけ開けます")]
    NotWebLink,
    #[error("リンクに使えない文字が入っています")]
    BadCharacter,
    #[error("ブラウザを開けませんでした: {0}")]
    Failed(String),
}

/// 開いてよいリンクか調べ、そのまま返す。
///
/// 画面側でボタンを出すかどうかの判断にも使う。
pub fn check(url: &str) -> Result<&str, OpenError> {
    let url = url.trim();
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return Err(OpenError::NotWebLink);
    }
    // 空白や制御文字は URL に現れない。混ざっていたら引数の切れ目を
    // 作られる恐れがあるので開かない。
    if url.len() <= "https://".len()
        || url.chars().any(|c| c.is_whitespace() || c.is_control())
    {
        return Err(OpenError::BadCharacter);
    }
    Ok(url)
}

/// 既定のブラウザで開く。
pub fn open(url: &str) -> Result<(), OpenError> {
    let url = check(url)?;
    let result = if cfg!(target_os = "macos") {
        Command::new("open").arg(url).spawn()
    } else if cfg!(target_os = "windows") {
        // `cmd /C start` は URL の `&` を自分で解釈してしまうので使わない。
        Command::new("rundll32")
            .args(["url.dll,FileProtocolHandler", url])
            .spawn()
    } else {
        Command::new("xdg-open").arg(url).spawn()
    };

    match result {
        Ok(_) => Ok(()),
        Err(e) => Err(OpenError::Failed(e.to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn web_links_are_allowed() {
        assert!(check("https://example.com/a").is_ok());
        assert!(check("http://example.com").is_ok());
        // 前後の空白は落とす。
        assert_eq!(check("  https://example.com  ").unwrap(), "https://example.com");
        // クエリの `&` は URL の一部。開けなければならない。
        assert!(check("https://example.com/?a=1&b=2").is_ok());
    }

    /// ブラウザ以外の入り口は開かない。
    ///
    /// 作品と一緒に配られた URL がそのまま外部プログラムへ渡るので、
    /// ここを緩めると受け取った人の環境で何かを起こせてしまう。
    #[test]
    fn anything_that_is_not_a_web_link_is_refused() {
        for bad in [
            "file:///etc/passwd",
            "javascript:alert(1)",
            "vbscript:x",
            "data:text/html,<script>",
            "smb://host/share",
            "ftp://example.com",
            "/etc/passwd",
            "example.com",
            "",
            "HTTPS://EXAMPLE.COM",
        ] {
            assert_eq!(check(bad), Err(OpenError::NotWebLink), "{bad} が通ってしまいました");
        }
    }

    /// 引数の切れ目を作られないようにする。
    #[test]
    fn whitespace_and_control_characters_are_refused() {
        for bad in [
            "https://example.com a",
            "https://example.com\nid",
            "https://exa\tmple.com",
            "https://",
        ] {
            assert!(check(bad).is_err(), "{bad:?} が通ってしまいました");
        }
    }
}
