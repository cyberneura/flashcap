//! メニューバーへの常駐 (CYBERNEURA-DEV-761)
//!
//! Preferences の「Keep FlashCap in the menu bar」を ON にすると、メニューバーに
//! FlashCap のアイコンを置き、クリックで撮影・録画・OCR のメニューを開く。
//! 設定は `settings.json` の `show_in_menu_bar` (bool、未設定は OFF)。
//!
//! Preferences はフロントから store に書いた後に `sync_menu_bar` を呼ぶ。起動時は
//! setup から `sync` を呼ぶ。どちらも「保存値を読んで、アイコンの有無とメニューの
//! 文言をそれに合わせる」だけなので、何度呼んでもよい。
//!
//! 常駐中はメインウインドウを閉じても終了せず隠す (lib.rs の `on_window_event`)。
//! 閉じるとフロントの WebView ごと破棄され、メニューからの撮影 (do-capture) を
//! 受け取る相手がいなくなるため。

use tauri::image::Image;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::Manager;
use tauri_plugin_store::StoreExt;

use crate::CaptureKind;

/// settings.json のキー。Preferences (`src/routes/preferences/+page.svelte`) と揃えること
const STORE_KEY: &str = "show_in_menu_bar";

const TRAY_ID: &str = "menu-bar";

/// メニューバーのアイコン (36x36 のテンプレート画像。scripts/make-tray-icons.py で生成)。
/// tray-icon crate が高さ 18pt に揃えるので、Retina で等倍になる
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
const ICON_PNG: &[u8] = include_bytes!("../icons/tray/menu-bar.png");

/// メニューの項目
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MenuAction {
    OpenMainWindow,
    Capture,
    CaptureWithTimer,
    RecordVideo,
    RecordVideoWithTimer,
    CopyTextOnScreen,
    Quit,
}

impl MenuAction {
    const ALL: [MenuAction; 7] = [
        MenuAction::OpenMainWindow,
        MenuAction::Capture,
        MenuAction::CaptureWithTimer,
        MenuAction::RecordVideo,
        MenuAction::RecordVideoWithTimer,
        MenuAction::CopyTextOnScreen,
        MenuAction::Quit,
    ];

    fn menu_id(self) -> &'static str {
        match self {
            MenuAction::OpenMainWindow => "menu-bar-open",
            MenuAction::Capture => "menu-bar-capture",
            MenuAction::CaptureWithTimer => "menu-bar-capture-timer",
            MenuAction::RecordVideo => "menu-bar-record",
            MenuAction::RecordVideoWithTimer => "menu-bar-record-timer",
            MenuAction::CopyTextOnScreen => "menu-bar-ocr",
            MenuAction::Quit => "menu-bar-quit",
        }
    }

    fn from_menu_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|action| action.menu_id() == id)
    }

    /// タイマーの秒数は Preferences の Timer delay に従う
    fn label(self, timer_delay: u32) -> String {
        match self {
            MenuAction::OpenMainWindow => "Open FlashCap".to_string(),
            MenuAction::Capture => "Capture Screenshot".to_string(),
            MenuAction::CaptureWithTimer => {
                format!("Capture Screenshot with Timer ({}s)", timer_delay)
            }
            MenuAction::RecordVideo => "Record Video".to_string(),
            MenuAction::RecordVideoWithTimer => {
                format!("Record Video with Timer ({}s)", timer_delay)
            }
            MenuAction::CopyTextOnScreen => "Copy Text on Screen (OCR)".to_string(),
            MenuAction::Quit => "Quit FlashCap".to_string(),
        }
    }

    /// この OS で使える項目か。録画 (screencapture -v) と OCR (Vision Framework) は
    /// macOS にしか無いので、他の OS ではメニューに出さない
    fn is_available(self) -> bool {
        cfg!(target_os = "macos")
            || !matches!(
                self,
                MenuAction::RecordVideo
                    | MenuAction::RecordVideoWithTimer
                    | MenuAction::CopyTextOnScreen
            )
    }

    /// この項目の後に区切り線を入れるか
    fn separator_after(self) -> bool {
        matches!(
            self,
            MenuAction::OpenMainWindow
                | MenuAction::CaptureWithTimer
                | MenuAction::RecordVideoWithTimer
                | MenuAction::CopyTextOnScreen
        )
    }
}

fn is_enabled(app: &tauri::AppHandle) -> bool {
    app.store("settings.json")
        .ok()
        .and_then(|store| store.get(STORE_KEY))
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
}

/// メニューバーにアイコンが出ているか
pub(crate) fn is_shown(app: &tauri::AppHandle) -> bool {
    app.tray_by_id(TRAY_ID).is_some()
}

/// 保存値に合わせてアイコンを置く / 外す。置いてあればメニューを作り直す
/// (タイマーの秒数が変わった時に文言を追従させるため)
pub(crate) fn sync(app: &tauri::AppHandle) -> Result<(), String> {
    if !is_enabled(app) {
        // 外したアイコンは、戻り値の TrayIcon を drop した時点でメニューバーから消える
        if app.remove_tray_by_id(TRAY_ID).is_some() {
            // 常駐中に閉じて隠していたメインウインドウを戻す。Preferences だけ開いた状態で
            // OFF にすると、それを閉じた後に見えるウインドウが 1 枚も無いまま動き続ける
            if let Some(w) = app.get_webview_window("main") {
                if !w.is_visible().unwrap_or(true) {
                    let _ = w.show();
                }
            }
        }
        return Ok(());
    }

    let menu = build_menu(app).map_err(|e| format!("Failed to build the menu bar menu: {}", e))?;
    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        return tray
            .set_menu(Some(menu))
            .map_err(|e| format!("Failed to update the menu bar menu: {}", e));
    }

    let icon = tray_icon(app)?;
    TrayIconBuilder::with_id(TRAY_ID)
        .icon(icon)
        // テンプレート画像 (OS が色を塗る) は macOS だけ。Windows はアプリのアイコンをそのまま出す
        .icon_as_template(cfg!(target_os = "macos"))
        .tooltip("FlashCap")
        .menu(&menu)
        .show_menu_on_left_click(true)
        // **ここで on_menu_event を登録しない。** トレイのハンドラは app 全体の
        // リストに積まれ、トレイを外しても消えない。設定を OFF → ON するたびに
        // 1 つずつ増え、1 回のクリックで撮影が 2 回走るようになる。
        // イベントは lib.rs の app.on_menu_event から handle_menu_event に回す
        .build(app)
        .map_err(|e| format!("Failed to set up the menu bar icon: {}", e))?;
    Ok(())
}

/// メニューバーのアイコン。macOS は黒一色のテンプレート画像 (色は OS が付ける)
#[cfg(target_os = "macos")]
fn tray_icon(_app: &tauri::AppHandle) -> Result<Image<'static>, String> {
    Image::from_bytes(ICON_PNG).map_err(|e| format!("Failed to load the menu bar icon: {}", e))
}

/// Windows の通知領域はテンプレート画像を塗り替えないので、黒一色のままだと暗い
/// タスクバーで見えなくなる。アプリのアイコンをそのまま使う
#[cfg(not(target_os = "macos"))]
fn tray_icon(app: &tauri::AppHandle) -> Result<Image<'static>, String> {
    app.default_window_icon()
        .map(|icon| icon.clone().to_owned())
        .ok_or_else(|| "The app has no icon to show in the notification area".to_string())
}

/// Preferences から、設定を書き換えた後に呼ぶ
#[tauri::command]
pub fn sync_menu_bar(app: tauri::AppHandle) -> Result<(), String> {
    sync(&app)
}

fn build_menu(app: &tauri::AppHandle) -> tauri::Result<Menu<tauri::Wry>> {
    let timer_delay = crate::get_timer_delay(app);
    let menu = Menu::new(app)?;
    for action in MenuAction::ALL.into_iter().filter(|a| a.is_available()) {
        let item = MenuItem::with_id(
            app,
            action.menu_id(),
            action.label(timer_delay),
            true,
            None::<&str>,
        )?;
        menu.append(&item)?;
        if action.separator_after() {
            menu.append(&PredefinedMenuItem::separator(app)?)?;
        }
    }
    Ok(menu)
}

fn show_main_window(app: &tauri::AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.set_focus();
    }
}

/// アプリ全体のメニューイベントのうち、このメニューの項目だけを処理する
/// (lib.rs の `app.on_menu_event` から、アプリメニューの Preferences... と一緒に届く)
pub(crate) fn handle_menu_event(app: &tauri::AppHandle, id: &str) {
    if let Some(action) = MenuAction::from_menu_id(id).filter(|a| a.is_available()) {
        run(app, action);
    }
}

fn run(app: &tauri::AppHandle, action: MenuAction) {
    if action == MenuAction::Quit {
        app.exit(0);
        return;
    }

    // フロントの描画前 (コールド起動の直後) に受け付けるのは、撮影 (ハンドシェイクが
    // frontend-ready まで預かる) と Quit だけ。録画と OCR をここで始めると、途中で
    // frontend-ready 側がメインウインドウを出してしまい、範囲選択や撮影に写り込む。
    // Open も、描画前に出すと白いウインドウが見えるだけ (描画が終われば向こうが出す)
    let queued = matches!(action, MenuAction::Capture | MenuAction::CaptureWithTimer);
    if !queued && !crate::is_frontend_ready(app) {
        return;
    }

    // **録画中 (書き出し中を含む) は何も始めずにメインウインドウを出す。** 停止ボタンは
    // メインウインドウにしか無く、範囲選択を終えると start_video_recording が録画中の
    // ものを止めて差し替えてしまう。ツールバーも録画中は撮影ボタンを無効にしている
    if crate::video::is_recording(app) {
        show_main_window(app);
        return;
    }

    // 録画の範囲を選んでいる間 (タイマー付きのカウントダウン中を含む) は、Open だけを
    // 「範囲選択をやめてメインウインドウに戻る」として受け付ける。カウントダウン中の
    // オーバーレイはクリックを透過するので、他のアプリを触った後は Esc が届かず、
    // ここがキャンセルの手段になる
    if crate::video::is_selecting_region(app) {
        if action == MenuAction::OpenMainWindow {
            crate::video::cancel_region_selection(app.clone());
        }
        return;
    }

    // 撮影 / OCR の範囲を選んでいる間は何もしない。メインウインドウを出すと撮影に写り込み、
    // 撮影を始めると screencapture が 2 つ並ぶ
    if crate::is_capture_in_progress() {
        return;
    }

    match action {
        MenuAction::OpenMainWindow => show_main_window(app),
        // 撮影はウインドウ経由と同じくフロントの captureScreen() に任せる
        // (撮影前に隠し、撮影後に結果を出す。ウインドウを出すのもあちらの仕事)
        MenuAction::Capture => crate::request_capture(app, CaptureKind::Interactive),
        MenuAction::CaptureWithTimer => crate::request_capture(app, CaptureKind::Timer),
        MenuAction::RecordVideo => start_recording(app, None),
        MenuAction::RecordVideoWithTimer => start_recording(app, Some(crate::get_timer_delay(app))),
        MenuAction::CopyTextOnScreen => copy_text_on_screen(app),
        MenuAction::Quit => unreachable!("Quit は先頭で処理済み"),
    }
}

/// 録画の範囲選択を始める
fn start_recording(app: &tauri::AppHandle, delay_seconds: Option<u32>) {
    if let Err(e) = crate::video::open_region_selector(app.clone(), delay_seconds) {
        crate::ocr::notify("FlashCap", &format!("Failed to start recording: {}", e));
    }
}

/// 画面の範囲を選んで OCR し、テキストをクリップボードへ入れる
///
/// メインウインドウが出ていると撮影範囲に写り込むので、撮影の間だけ隠して元に戻す。
/// 戻す時にフォーカスは奪わない (ウインドウの ocrCaptureRegion() と同じ。テキストを
/// 貼りに行く先のアプリから前面を取り上げないため)
fn copy_text_on_screen(app: &tauri::AppHandle) {
    let main = app.get_webview_window("main");
    let was_visible = main
        .as_ref()
        .and_then(|w| w.is_visible().ok())
        .unwrap_or(false);
    if was_visible {
        if let Some(w) = &main {
            let _ = w.hide();
        }
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        crate::ocr::run_headless_ocr(&app, false).await;
        // OCR の間は CaptureInProgress がメニューを止めているが、終わった瞬間に始まった
        // 撮影や範囲選択があれば戻さない。戻すと新しい撮影や範囲選択にメインウインドウが
        // 写り込む (録画はメインウインドウを自分で出す)
        let busy = crate::is_capture_in_progress()
            || crate::video::is_selecting_region(&app)
            || crate::video::is_recording(&app);
        if was_visible && !busy {
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.show();
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn menu_ids_map_back_to_their_action() {
        for action in MenuAction::ALL {
            assert_eq!(MenuAction::from_menu_id(action.menu_id()), Some(action));
        }
        // アプリメニューの Preferences... の id は拾わない
        assert_eq!(MenuAction::from_menu_id("preferences"), None);
    }

    #[test]
    fn timer_labels_follow_the_timer_delay() {
        assert_eq!(
            MenuAction::CaptureWithTimer.label(10),
            "Capture Screenshot with Timer (10s)"
        );
        assert_eq!(
            MenuAction::RecordVideoWithTimer.label(3),
            "Record Video with Timer (3s)"
        );
        assert_eq!(MenuAction::Capture.label(10), "Capture Screenshot");
    }

    #[test]
    fn the_menu_bar_icon_is_a_visible_menubar_sized_template() {
        let icon = Image::from_bytes(ICON_PNG).expect("メニューバーのアイコンを読めない");
        assert_eq!((icon.width(), icon.height()), (36, 36));
        // テンプレート画像は形をアルファだけで表す。何も描かれていなければ見えない
        assert!(icon.rgba().chunks(4).any(|px| px[3] > 0), "アイコンが透明");
        // 色は OS が付けるので、RGB は黒で揃える
        assert!(
            icon.rgba().chunks(4).all(|px| px[..3] == [0, 0, 0]),
            "テンプレート画像に色が付いている"
        );
    }
}
