//! スクリーンショット作成時の自動コピー (CYBERNEURA-DEV-761)
//!
//! 「コピーしない / パスをコピー / 画像をコピー」の選択は、メインウインドウの
//! ツールバーのボタン (`src/lib/Toolbar.svelte`) で行う。選んだ値はフロントが
//! `settings.json` (tauri-plugin-store) の `auto_copy_on_capture` に書き、
//! ここでは撮影のたびにそれを読むだけ。
//!
//! 自動コピーの対象は「撮影」(take_screenshot_interactive / take_screenshot_timer)
//! だけ。貼り付け・ファイルを開く・OCR・動画は撮影ではないのでコピーしない。

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

use base64::{engine::general_purpose::STANDARD, Engine as _};
use tauri::image::Image;
use tauri_plugin_clipboard_manager::ClipboardExt;
use tauri_plugin_store::StoreExt;

use crate::ScreenshotResult;

/// settings.json のキー。値は AutoCopyMode::as_setting() の文字列。
/// フロント (`src/routes/+page.svelte` の AUTO_COPY_KEY) と揃えること
const STORE_KEY: &str = "auto_copy_on_capture";

/// 最後に撮影した回の通し番号
///
/// コピーは撮影ごとに別スレッドで走るので、完了順は撮影順と一致しない (画像の
/// デコードは大きい画像ほど遅い)。書き込む直前にこれと照合し、後の撮影に
/// 追い越されたコピーは捨てる
static LATEST_CAPTURE: AtomicU64 = AtomicU64::new(0);

/// 「最新の撮影か」の照合とクリップボードへの書き込みをまとめて直列にする
///
/// 照合だけでは足りない。照合を通った先の撮影の書き込みが遅いと、その間に後の撮影が
/// 照合と書き込みを終え、先の撮影が最後にクリップボードを上書きしてしまう。
/// デコードはこのロックの外で行うので、待たされるのは書き込みの間だけ
static CLIPBOARD_WRITE: Mutex<()> = Mutex::new(());

/// 撮影後に何をクリップボードへ入れるか
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AutoCopyMode {
    None,
    Path,
    Image,
}

// 値を書くのはフロントなので、文字列への変換はフロントとの突き合わせ (テスト) でしか使わない
impl AutoCopyMode {
    #[cfg(test)]
    const ALL: [AutoCopyMode; 3] = [AutoCopyMode::None, AutoCopyMode::Path, AutoCopyMode::Image];

    #[cfg(test)]
    fn as_setting(self) -> &'static str {
        match self {
            AutoCopyMode::None => "none",
            AutoCopyMode::Path => "path",
            AutoCopyMode::Image => "image",
        }
    }

    /// 保存値から読む。**未設定や知らない値は None (コピーしない) に倒す。**
    /// この機能より前から使っている人の撮影で、黙ってクリップボードを
    /// 上書きし始めないため
    fn from_setting(value: Option<&str>) -> Self {
        match value {
            Some("path") => AutoCopyMode::Path,
            Some("image") => AutoCopyMode::Image,
            _ => AutoCopyMode::None,
        }
    }
}

fn current_mode(app: &tauri::AppHandle) -> AutoCopyMode {
    let value = app
        .store("settings.json")
        .ok()
        .and_then(|store| store.get(STORE_KEY))
        .and_then(|v| v.as_str().map(String::from));
    AutoCopyMode::from_setting(value.as_deref())
}

/// 撮影が終わった画像を、設定に従ってクリップボードへ入れて通知する
///
/// **撮影結果を返すのを待たせない。** 通知の osascript は起動に数百 ms かかり、
/// 画像のコピーは PNG を RGBA に展開する (Retina の全画面で数十 MB)。撮影コマンドの
/// 中で待つと、その分だけ撮影後にウインドウが出てくるのが遅れる。
pub(crate) fn copy_after_capture(app: &tauri::AppHandle, result: &ScreenshotResult) {
    // コピーしない設定の撮影でも番号は進める。先の撮影のコピーが後から終わって、
    // 後の撮影の直後にクリップボードを書き換えるのを防ぐため
    let generation = next_capture_generation();
    let mode = current_mode(app);
    if mode == AutoCopyMode::None {
        return;
    }

    let app = app.clone();
    let file_path = result.file_path.clone();
    // 画像をコピーしない時まで数 MB の base64 を複製しない
    let data = (mode == AutoCopyMode::Image).then(|| result.data.clone());

    tauri::async_runtime::spawn_blocking(move || {
        let image = match data.as_deref().map(decode_image).transpose() {
            Ok(image) => image,
            Err(e) => {
                crate::ocr::notify("FlashCap", &e);
                return;
            }
        };

        // 中身は無いので、他のスレッドが書き込み中に panic していても続行してよい
        let guard = CLIPBOARD_WRITE.lock().unwrap_or_else(|e| e.into_inner());
        let outcome = if !is_latest_capture(generation) {
            Ok(None)
        } else if let Some(image) = image {
            app.clipboard()
                .write_image(&image)
                .map(|_| Some("Copied the image to the clipboard"))
                .map_err(|e| format!("Failed to copy the image: {}", e))
        } else {
            app.clipboard()
                .write_text(file_path)
                .map(|_| Some("Copied the image path to the clipboard"))
                .map_err(|e| format!("Failed to copy the image path: {}", e))
        };
        // 通知 (osascript の起動) の間まで次の撮影の書き込みを待たせない
        drop(guard);

        match outcome {
            Ok(Some(message)) => crate::ocr::notify("FlashCap", message),
            // 後の撮影に追い越された。クリップボードは後の撮影の分が入る
            Ok(None) => {}
            Err(e) => crate::ocr::notify("FlashCap", &e),
        }
    });
}

/// 撮影ごとの通し番号を 1 つ進め、今回の番号を返す
fn next_capture_generation() -> u64 {
    LATEST_CAPTURE.fetch_add(1, Ordering::SeqCst) + 1
}

fn is_latest_capture(generation: u64) -> bool {
    LATEST_CAPTURE.load(Ordering::SeqCst) == generation
}

fn decode_image(data_base64: &str) -> Result<Image<'static>, String> {
    let png = STANDARD
        .decode(data_base64)
        .map_err(|e| format!("Failed to copy the image: {}", e))?;
    Image::from_bytes(&png).map_err(|e| format!("Failed to copy the image: {}", e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_round_trip() {
        for mode in AutoCopyMode::ALL {
            assert_eq!(AutoCopyMode::from_setting(Some(mode.as_setting())), mode);
        }
    }

    #[test]
    fn an_overtaken_capture_is_no_longer_the_latest() {
        let first = next_capture_generation();
        assert!(is_latest_capture(first));
        let second = next_capture_generation();
        assert!(!is_latest_capture(first), "先の撮影のコピーは捨てる");
        assert!(is_latest_capture(second));
    }

    #[test]
    fn missing_or_unknown_settings_do_not_copy() {
        // 既存ユーザーの撮影で、いきなりクリップボードを書き換え始めないこと
        assert_eq!(AutoCopyMode::from_setting(None), AutoCopyMode::None);
        assert_eq!(
            AutoCopyMode::from_setting(Some("clipboard")),
            AutoCopyMode::None
        );
        assert_eq!(AutoCopyMode::from_setting(Some("")), AutoCopyMode::None);
    }
}
