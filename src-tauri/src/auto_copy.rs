//! スクリーンショット作成時の自動コピー (CYBERNEURA-DEV-761)
//!
//! メニューバーのアイコン 1 つで「コピーしない / パスをコピー / 画像をコピー」を選ぶ。
//! 選んだ値は `settings.json` (tauri-plugin-store) の `auto_copy_on_capture` に
//! 永続化し、アイコンの図柄も選択に合わせて差し替える。
//!
//! **メニューバーにはウインドウではなくネイティブのメニューを出す。** 幅を取らない
//! ことが要件なので、アイコン 1 つを置いてクリックでメニューを開く。自前の
//! ポップアップウインドウは、位置合わせ・フォーカスを失った時の閉じ方・Space の
//! 扱いを全部こちらで持つことになるうえ、見た目も macOS のメニューバーアプリの
//! 流儀から外れる。
//!
//! 自動コピーの対象は「撮影」(take_screenshot_interactive / take_screenshot_timer)
//! だけ。貼り付け・ファイルを開く・OCR・動画は撮影ではないのでコピーしない。

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;

use base64::{engine::general_purpose::STANDARD, Engine as _};
use tauri::image::Image;
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri_plugin_clipboard_manager::ClipboardExt;
use tauri_plugin_store::StoreExt;

use crate::ScreenshotResult;

/// settings.json のキー。値は AutoCopyMode::as_setting() の文字列
const STORE_KEY: &str = "auto_copy_on_capture";

const TRAY_ID: &str = "auto-copy";

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

/// メニューバーのアイコンを置けたか
///
/// **置けなかったセッションでは保存値に関わらずコピーしない。** 切り替えの UI は
/// このアイコンのメニューしか無いので、保存値が path / image のままだと、
/// 撮影のたびにクリップボードが書き換わるのに止める手段が無くなる。
/// 保存値そのものは書き換えない (次にアイコンを置けた起動では選択が戻る)
static TRAY_AVAILABLE: AtomicBool = AtomicBool::new(false);

/// 撮影後に何をクリップボードへ入れるか
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AutoCopyMode {
    None,
    Path,
    Image,
}

impl AutoCopyMode {
    /// メニューに並べる順
    const ALL: [AutoCopyMode; 3] = [AutoCopyMode::None, AutoCopyMode::Path, AutoCopyMode::Image];

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

    fn menu_id(self) -> &'static str {
        match self {
            AutoCopyMode::None => "auto-copy-none",
            AutoCopyMode::Path => "auto-copy-path",
            AutoCopyMode::Image => "auto-copy-image",
        }
    }

    fn from_menu_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|mode| mode.menu_id() == id)
    }

    fn label(self) -> &'static str {
        match self {
            AutoCopyMode::None => "Don't copy",
            AutoCopyMode::Path => "Copy the image file path",
            AutoCopyMode::Image => "Copy the image data",
        }
    }

    /// メニューバーのアイコン (36x36 のテンプレート画像。scripts/make-tray-icons.py で生成)
    fn icon_png(self) -> &'static [u8] {
        match self {
            AutoCopyMode::None => include_bytes!("../icons/tray/auto-copy-none.png"),
            AutoCopyMode::Path => include_bytes!("../icons/tray/auto-copy-path.png"),
            AutoCopyMode::Image => include_bytes!("../icons/tray/auto-copy-image.png"),
        }
    }

    fn icon(self) -> tauri::Result<Image<'static>> {
        Image::from_bytes(self.icon_png())
    }

    fn tooltip(self) -> String {
        format!("FlashCap: {}", self.label())
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

/// 選択を保存する。ディスクへの書き込みに失敗しても、メモリ上の store には
/// 入っているので、このセッションの撮影には効く (次の起動で戻るだけ)
fn persist_mode(app: &tauri::AppHandle, mode: AutoCopyMode) -> Result<(), String> {
    let store = app
        .store("settings.json")
        .map_err(|e| format!("Failed to open settings: {}", e))?;
    store.set(STORE_KEY, mode.as_setting());
    store
        .save()
        .map_err(|e| format!("Failed to save settings: {}", e))
}

/// メニューバーにアイコンを置く
pub(crate) fn setup_tray(app: &tauri::AppHandle) -> tauri::Result<()> {
    let mode = current_mode(app);

    // 見出しは押せない項目で表す (macOS のメニューでは灰色の見出しになる)
    let header = MenuItem::with_id(
        app,
        "auto-copy-header",
        "Auto-copy after capture",
        false,
        None::<&str>,
    )?;
    let items = AutoCopyMode::ALL
        .into_iter()
        .map(|m| CheckMenuItem::with_id(app, m.menu_id(), m.label(), true, m == mode, None::<&str>))
        .collect::<tauri::Result<Vec<_>>>()?;

    let menu = Menu::new(app)?;
    menu.append(&header)?;
    menu.append(&PredefinedMenuItem::separator(app)?)?;
    for item in &items {
        menu.append(item)?;
    }

    TrayIconBuilder::with_id(TRAY_ID)
        .icon(mode.icon()?)
        .icon_as_template(true)
        .tooltip(mode.tooltip())
        .menu(&menu)
        .show_menu_on_left_click(true)
        // このハンドラはトレイ以外のメニュー (アプリメニューの Preferences...) の
        // イベントでも呼ばれるので、自分の id だけを拾う
        .on_menu_event(move |app, event| {
            if let Some(selected) = AutoCopyMode::from_menu_id(event.id().as_ref()) {
                select_mode(app, &items, selected);
            }
        })
        .build(app)?;

    TRAY_AVAILABLE.store(true, Ordering::SeqCst);
    Ok(())
}

fn select_mode(app: &tauri::AppHandle, items: &[CheckMenuItem<tauri::Wry>], mode: AutoCopyMode) {
    if let Err(e) = persist_mode(app, mode) {
        eprintln!("[flashcap] {}", e);
    }

    // チェックは項目を押した時点で OS 側が反転させる。選択中の項目をもう一度押すと
    // 外れてしまうので、ラジオボタンとして振る舞うよう全項目を付け直す
    for (item, m) in items.iter().zip(AutoCopyMode::ALL) {
        let _ = item.set_checked(m == mode);
    }

    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        match mode.icon() {
            // set_icon の後に set_icon_as_template を呼ぶと macOS で 2 回描かれて
            // ちらつくので、両方を一度に設定する
            Ok(icon) => {
                let _ = tray.set_icon_with_as_template(Some(icon), true);
            }
            Err(e) => eprintln!("[flashcap] Failed to load the tray icon: {}", e),
        }
        let _ = tray.set_tooltip(Some(mode.tooltip()));
    }
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
    let mode = effective_mode(TRAY_AVAILABLE.load(Ordering::SeqCst), || current_mode(app));
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

/// 撮影後に実際に使うモード。アイコンを置けていなければ保存値を読まずにコピーしない
/// (理由は TRAY_AVAILABLE を参照)
fn effective_mode(tray_available: bool, stored: impl FnOnce() -> AutoCopyMode) -> AutoCopyMode {
    if tray_available {
        stored()
    } else {
        AutoCopyMode::None
    }
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
    fn without_the_menubar_icon_nothing_is_copied() {
        // 切り替えの UI が無いのに保存値どおりコピーし続けないこと
        for stored in AutoCopyMode::ALL {
            assert_eq!(effective_mode(false, || stored), AutoCopyMode::None);
            assert_eq!(effective_mode(true, || stored), stored);
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

    #[test]
    fn menu_ids_map_back_to_their_mode() {
        for mode in AutoCopyMode::ALL {
            assert_eq!(AutoCopyMode::from_menu_id(mode.menu_id()), Some(mode));
        }
        // 見出しや他のメニュー (Preferences...) の id は拾わない
        assert_eq!(AutoCopyMode::from_menu_id("auto-copy-header"), None);
        assert_eq!(AutoCopyMode::from_menu_id("preferences"), None);
    }

    #[test]
    fn every_mode_has_a_distinct_menubar_sized_icon() {
        // tray-icon crate はアイコンを高さ 18pt に揃える。36px なら Retina で等倍になる
        let mut seen = Vec::new();
        for mode in AutoCopyMode::ALL {
            let icon = mode
                .icon()
                .unwrap_or_else(|e| panic!("{:?} のアイコンを読めない: {}", mode, e));
            assert_eq!((icon.width(), icon.height()), (36, 36), "{:?}", mode);
            // テンプレート画像は形をアルファだけで表す。何も描かれていなければ見えない
            assert!(
                icon.rgba().chunks(4).any(|px| px[3] > 0),
                "{:?} のアイコンが透明",
                mode
            );
            assert!(
                !seen.contains(&icon.rgba().to_vec()),
                "{:?} の図柄が他と同じ",
                mode
            );
            seen.push(icon.rgba().to_vec());
        }
    }
}
