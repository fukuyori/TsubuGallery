//! 起動しても黒い窓を出さない。
//!
//! この実行ファイルは GUI (`windows_subsystem = "windows"`) として作ってある。
//! そうしないと、ショートカットやスタートメニュー、インストーラの「完了」から
//! 開くたびに、使いもしないコンソールが一緒に開いてしまう。作品を見るための
//! アプリなので、最初に目へ入るのが黒い窓では具合が悪い。
//!
//! その代わり、コマンドラインから叩いたときも標準出力の行き先が無くなる。
//! `--help` や `--version`、`--capture-all` の進み具合が誰にも見えない。
//! そこで、呼んだ側にコンソールがあるときだけ、そこへ繋ぎ直す。自分で開くので
//! はなく借りるので、エクスプローラーから開いたときは何も起きない。

/// 呼んだ側のコンソールがあれば、標準出力と標準エラーをそこへ向ける。
///
/// 標準ライブラリはハンドルを最初に使うときまで取りに行かないので、何かを
/// 書き出す前に呼ぶ。借りられなければ黙って何もしない。コンソールが無いのは
/// エクスプローラーから開いたときの普通の姿で、困ったことではない。
#[cfg(windows)]
pub fn attach_to_parent() {
    use windows_sys::Win32::Foundation::{HANDLE, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::System::Console::{
        ATTACH_PARENT_PROCESS, AttachConsole, GetStdHandle, STD_ERROR_HANDLE, STD_HANDLE,
        STD_INPUT_HANDLE, STD_OUTPUT_HANDLE, SetStdHandle,
    };

    /// その枠にいま行き先が入っているか。入っていればその値。
    fn current(slot: STD_HANDLE) -> Option<HANDLE> {
        // SAFETY: 決まった枠の番号を渡して、いま入っている値を見るだけ。
        let handle = unsafe { GetStdHandle(slot) };
        (!handle.is_null() && handle != INVALID_HANDLE_VALUE).then_some(handle)
    }

    // どの枠をどの予約名へ繋ぐか。`CONOUT$` / `CONIN$` は、いま繋いだ
    // コンソールを指す予約名。
    let slots = [
        (STD_OUTPUT_HANDLE, "CONOUT$"),
        (STD_ERROR_HANDLE, "CONOUT$"),
        (STD_INPUT_HANDLE, "CONIN$"),
    ];

    // AttachConsole は、行き先の決まっている枠まで、繋いだコンソールのものへ
    // 差し替えてしまう。`--version > file` や `| more` を壊さないよう、呼ぶ前
    // の値を控えておいて、あとで戻す。
    let saved = slots.map(|(slot, _)| current(slot));

    // SAFETY: 引数は決まった定数ひとつ。呼んだ側にコンソールが無ければ 0 が
    // 返るだけで、そのときは下の繋ぎ直しもしない。
    if unsafe { AttachConsole(ATTACH_PARENT_PROCESS) } == 0 {
        return;
    }

    for ((slot, name), before) in slots.into_iter().zip(saved) {
        match before {
            // もともとリダイレクトされていた枠。元の行き先へ戻す。
            // SAFETY: この関数へ入る前に有効だったハンドルを、同じ枠へ返すだけ。
            Some(handle) => unsafe {
                SetStdHandle(slot, handle);
            },
            // 空だった枠。GUI として起動したぶんの埋め合わせに、借りた
            // コンソールを据える。
            None => point_at(name, slot),
        }
    }
}

/// 予約名を開いて、標準ハンドルの枠へ据える。
///
/// 呼ばれるのは空いている枠だけだが、AttachConsole がすでに埋めていることも
/// ある。そのときは開き直さずそのまま使う。
#[cfg(windows)]
fn point_at(name: &str, slot: windows_sys::Win32::System::Console::STD_HANDLE) {
    use std::os::windows::io::IntoRawHandle;

    use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;
    use windows_sys::Win32::System::Console::{GetStdHandle, SetStdHandle};

    // SAFETY: 決まった枠の番号を渡して、いま入っている値を見るだけ。
    let current = unsafe { GetStdHandle(slot) };
    if !current.is_null() && current != INVALID_HANDLE_VALUE {
        return;
    }

    let Ok(file) = std::fs::OpenOptions::new().read(true).write(true).open(name) else {
        return;
    };
    // 据えたハンドルはプロセスが終わるまで使う。File のまま持つと、この関数を
    // 出たところで閉じてしまうので、所有権を手放して生かしておく。
    let handle = file.into_raw_handle();

    // SAFETY: いま開いたばかりの有効なハンドルを、決まった枠へ据えるだけ。
    // 枠は呼び出し側が渡す 3 つの定数のどれかに限られる。
    unsafe {
        SetStdHandle(slot, handle);
    }
}

/// Windows 以外は端末から起動するのがふつうで、標準出力は最初から繋がっている。
#[cfg(not(windows))]
pub fn attach_to_parent() {}

