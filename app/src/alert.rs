//! 起動に失敗したことを画面へ知らせる。
//!
//! GPU が用意できない場面では egui も描けないので、アプリ内の UI では伝えようが
//! ない。頼れるのは OS のダイアログだけになる。
//!
//! ここを黙って終わらせると、ショートカットから起動した人には空のウィンドウが
//! 一瞬光って消えるだけに見える。標準エラーへ出しても、コンソールがプロセスと
//! 一緒に閉じてしまうので読めない。ログファイルを開くまで理由が分からない、と
//! いう状態を避けるための最後の一手。

/// 致命的な失敗を伝え、確認されるまで待つ。
///
/// 待つのが要点。押されるまで止めないと、結局ウィンドウが消えるだけになる。
#[cfg(windows)]
pub fn fatal(message: &str) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        MB_ICONERROR, MB_OK, MB_SETFOREGROUND, MessageBoxW,
    };

    // アプリ名はローカライズしない (設計書 §2)。
    let caption = wide("TsubuGallery");
    let text = wide(message);
    // SAFETY: どちらも NUL で終わる UTF-16 で、呼び出しのあいだは `wide` が返した
    // Vec が生かしている。親ウィンドウ無し (null) は MessageBoxW が許す指定で、
    // このとき現れるダイアログはどのウィンドウにも属さない。
    unsafe {
        MessageBoxW(
            std::ptr::null_mut(),
            text.as_ptr(),
            caption.as_ptr(),
            MB_OK | MB_ICONERROR | MB_SETFOREGROUND,
        );
    }
}

/// Win32 が受け取る、NUL で終わる UTF-16 にする。
#[cfg(windows)]
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Windows 以外は端末から起動するのがふつうなので、標準エラーで足りる。
#[cfg(not(windows))]
pub fn fatal(message: &str) {
    eprintln!("{message}");
}

#[cfg(test)]
mod tests {
    #[cfg(windows)]
    #[test]
    fn a_wide_string_ends_with_a_nul() {
        // 終端を落とすと MessageBoxW がバッファの先まで読み続ける。
        assert_eq!(super::wide("あ"), vec![0x3042, 0]);
        assert_eq!(super::wide(""), vec![0]);
    }
}
