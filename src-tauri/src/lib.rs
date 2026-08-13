// objc 0.2 の msg_send! マクロ展開時に出る unexpected_cfgs(cargo-clippy)警告を抑制
#![allow(unexpected_cfgs)]

mod ocr;
mod video;

use base64::{engine::general_purpose::STANDARD, Engine as _};
use chrono::Local;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Command;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::{Emitter, Listener, Manager, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_store::StoreExt;

const SUPPORTED_IMAGE_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "gif", "bmp", "webp", "tiff", "tif", "heic", "heif"];

#[derive(Debug, Serialize, Deserialize)]
pub struct ScreenshotResult {
    pub width: usize,
    pub height: usize,
    pub data: String,      // base64 encoded PNG
    pub file_path: String, // saved file path
}

/// flashcap が一時ファイルと既定の保存先に使うディレクトリ ($TMPDIR/flashcap)
///
/// **/tmp を直接使わないこと。** macOS の /tmp (= /private/tmp) は mode 1777 の
/// 共有領域で、同じ Mac の別ユーザーアカウントから中身を読める。このアプリが扱うのは
/// 画面に映っていたものそのもの (パスワード入力画面・社内資料・メール) なので、
/// 既定の置き場が他人から読めてよいものではない。
///
/// std::env::temp_dir() は macOS では通常 $TMPDIR (/var/folders/.../T/) を返し、
/// これはユーザー専用。ただし **TMPDIR が未設定の実行環境では /tmp に落ちる**ので、
/// 「temp_dir() だから安全」と考えてはいけない。実際の権限は
/// ensure_private_flashcap_dir() が確かめて締める。
pub(crate) fn flashcap_temp_dir() -> PathBuf {
    std::env::temp_dir().join("flashcap")
}

/// 所有者だけが読み書きできるディレクトリを作る (無ければ、親ごと)
///
/// std::fs::create_dir_all は mode を指定しないため umask 依存で 0755 になりうる。
/// スクリーンショットの置き場としてはそれでは広すぎるので、mode を明示して作る。
/// mkdir(2) の mode は umask で削られるだけなので、0700 より広くなることはない。
///
/// **既に在るディレクトリの mode は変えない。** ユーザーが custom: で指定した
/// 既存フォルダの権限を、こちらの都合で締めてしまわないため。flashcap 自身の
/// 作業ディレクトリを締め直したい場合は ensure_private_flashcap_dir() を使う。
pub(crate) fn create_private_dir<P: AsRef<Path>>(dir: P) -> std::io::Result<()> {
    use std::os::unix::fs::DirBuilderExt;
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(dir)
}

/// flashcap 専用の作業ディレクトリを、所有者だけが触れる状態にして返す
///
/// **既に在るディレクトリを締め直すのがこの関数の要点。** $TMPDIR/flashcap は
/// 以前のバージョンの video_temp_dir() が create_dir_all で作っており、umask 次第で
/// 0755 のまま残っている。新規作成時に mode を渡すだけでは、既に一度でも録画した
/// ユーザーの環境が直らない (そして直らないまま、そこへスクリーンショットを
/// 置き始めることになる)。
///
/// 所有者の確認に uid を引く必要はない。**chmod は所有者にしか通らない**ので、
/// 他人が作ったディレクトリならここが EPERM で落ちる。その場合は奪い返さず、
/// エラーを返して書き込みをやめる。だから chmod は**現在の mode に関わらず毎回**
/// 呼ぶ。既に 0700 の時に省くと、他人所有の 0700 ディレクトリが素通りしてしまう
/// (所有者確認がこの呼び出しそのものなので、省いた瞬間に確認も消える)。
///
/// **保証しているのは Unix の mode だけ。** macOS の拡張 ACL は見ていないので、
/// ACL で他ユーザーに開かれたディレクトリはここを通る。これは残存リスクであって
/// この関数が塞いだ範囲ではない。塞ぐなら chmod -N まで踏み込むこと。
pub(crate) fn ensure_private_flashcap_dir() -> std::io::Result<PathBuf> {
    use std::os::unix::fs::PermissionsExt;

    // 親が自分以外にも書ける場所なら、この先の検査は意味を持たない。
    // 「stat して chmod して使う」はパス名を辿り直す 3 手なので、その間に
    // ディレクトリを symlink と差し替えられると、検査した対象と実際に書く先が
    // 別物になる (TOCTOU)。誰も割り込めない場所であることを先に確かめる。
    //
    // **group とother の両方を見る。** /tmp の 1777 だけでなく、0770 のように
    // 同じグループの別アカウントが書ける場所でも、エントリを差し替える隙は同じだけある。
    //
    // macOS の GUI プロセスは launchd がユーザー専用の TMPDIR (0700) を必ず渡すので、
    // ここに引っかかるのは TMPDIR を落とした異常な起動だけ。その時は /tmp へ
    // 黙って落ちるより、撮らずに止まる方がこのアプリには正しい。
    let parent = std::env::temp_dir();
    let parent_meta = std::fs::metadata(&parent)?;
    if parent_meta.permissions().mode() & 0o022 != 0 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            format!(
                "temp directory {} is writable by other accounts; refusing to store screen \
                 captures there (is TMPDIR set?)",
                parent.display()
            ),
        ));
    }

    let dir = flashcap_temp_dir();
    create_private_dir(&dir)?;

    // symlink 越しに書かせない。作業ディレクトリの実体が別の場所を指していると、
    // 以降の書き込みも削除もそちらへ向く。symlink_metadata はリンクを辿らないので、
    // 「リンクではなく本物のディレクトリか」をここで判定できる。
    let meta = std::fs::symlink_metadata(&dir)?;
    if !meta.file_type().is_dir() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("{} is not a real directory", dir.display()),
        ));
    }

    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700))?;
    Ok(dir)
}

/// 保存先を書き込める状態にして返す
///
/// **設定の種別ではなく、解決後のパスで振り分ける。** 解決先が flashcap の作業
/// ディレクトリそのものなら、既存でも所有者専用まで締め直す — custom: でそこを
/// 名指ししていた場合も同じ扱いになるが、flashcap が管理する場所であることに
/// 変わりはないので、それでよい。
///
/// それ以外 (ユーザーが選んだ任意のフォルダ) は無ければ 0700 で作るだけで、
/// **既存フォルダの権限には触らない** — 意図して共有しているフォルダを、
/// こちらの都合で締めないため。
pub(crate) fn prepare_save_directory(app: &tauri::AppHandle) -> Result<String, String> {
    let dir = get_save_directory(app);
    if Path::new(&dir) == flashcap_temp_dir() {
        return ensure_private_flashcap_dir()
            .map(|p| p.to_string_lossy().to_string())
            .map_err(|e| format!("Failed to prepare save directory '{}': {}", dir, e));
    }
    create_private_dir(&dir)
        .map_err(|e| format!("Failed to create save directory '{}': {}", dir, e))?;
    Ok(dir)
}

/// 設定から保存先ディレクトリを取得する
/// "tmp" -> $TMPDIR/flashcap/
/// "macos_default" -> macOS のスクリーンショット保存先
/// "custom:<path>" -> カスタムパス
fn get_save_directory(app: &tauri::AppHandle) -> String {
    let setting = app
        .store("settings.json")
        .ok()
        .and_then(|store| store.get("save_directory"))
        .and_then(|v| v.as_str().map(String::from))
        .unwrap_or_else(|| "tmp".to_string());

    match setting.as_str() {
        "tmp" => default_save_directory(),
        "macos_default" => get_macos_screenshot_dir(),
        s if s.starts_with("custom:") => s.strip_prefix("custom:").unwrap().to_string(),
        _ => default_save_directory(),
    }
}

/// 設定値 "tmp" が指す実際のパス。設定値の文字列は互換のため "tmp" のまま残している
fn default_save_directory() -> String {
    flashcap_temp_dir().to_string_lossy().to_string()
}

/// 既定の保存先の実パスを返す (Preferences の表示用)
/// 環境によって変わるパスなので、画面にハードコードせずここから引く
#[tauri::command]
fn get_default_save_directory() -> String {
    default_save_directory()
}

/// 保存先フォルダを Finder で開く（未キャプチャ時用。無ければ作成する）
#[tauri::command]
fn open_save_directory(app: tauri::AppHandle) -> Result<(), String> {
    let dir = prepare_save_directory(&app)?;
    let status = Command::new("open")
        .arg(&dir)
        .status()
        .map_err(|e| format!("Failed to open save directory '{}': {}", dir, e))?;
    if !status.success() {
        return Err(format!("`open` exited with {} for '{}'", status, dir));
    }
    Ok(())
}

/// macOS の screencapture デフォルト保存先を取得
fn get_macos_screenshot_dir() -> String {
    Command::new("defaults")
        .args(["read", "com.apple.screencapture", "location"])
        .output()
        .ok()
        .and_then(|out| {
            if out.status.success() {
                String::from_utf8(out.stdout)
                    .ok()
                    .map(|s| s.trim().to_string())
            } else {
                None
            }
        })
        .unwrap_or_else(|| {
            dirs::desktop_dir()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(default_save_directory)
        })
}

/// 撮影結果の保存先パスを組み立てる
///
/// 保存先を用意できなかった場合はエラーを返して撮影を始めない。以前はここで
/// 作成失敗を握り潰していたが、この関数は「所有者しか読めない場所に置く」という
/// 保証を兼ねるようになったので、握り潰すと保証のないまま撮り続けることになる。
fn get_screenshot_path(app: &tauri::AppHandle) -> Result<String, String> {
    let dir = PathBuf::from(prepare_save_directory(app)?);
    let timestamp = Local::now().format("%Y%m%d-%H%M%S");
    let filename = format!("flashcap-{}.png", timestamp);
    Ok(dir.join(filename).to_string_lossy().to_string())
}

/// スクリーンショットの画像サイズに合わせてメインウインドウを拡大する
/// 天地左右 +20px の余白を確保し、モニターの作業領域を上限とする
/// ウインドウが画像より大きい場合は縮小しない
fn resize_window_for_image(app: &tauri::AppHandle, width: usize, height: usize) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    let Ok(Some(monitor)) = window.current_monitor() else {
        return;
    };

    let scale = monitor.scale_factor();
    let padding = 20.0;
    let toolbar_h = 41.0; // ツールバー 40px + ボーダー 1px

    // 画像の論理サイズ（screen points）
    let img_w = width as f64 / scale;
    let img_h = height as f64 / scale;

    // 画像を等倍表示するのに必要なウインドウ内部サイズ
    let desired_w = img_w + padding * 2.0;
    let desired_h = img_h + padding * 2.0 + toolbar_h;

    // 作業領域（メニューバー・Dock を除いた領域）の論理サイズ
    let work = monitor.work_area();
    let max_w = work.size.width as f64 / scale;
    let max_h = work.size.height as f64 / scale;

    // 現在のウインドウ内部サイズ（論理ピクセル）
    let Ok(cur) = window.inner_size() else { return };
    let cur_w = cur.width as f64 / scale;
    let cur_h = cur.height as f64 / scale;

    // 現在より大きい場合のみ拡大、作業領域で上限
    let new_w = desired_w.max(cur_w).min(max_w);
    let new_h = desired_h.max(cur_h).min(max_h);

    if (new_w - cur_w).abs() > 1.0 || (new_h - cur_h).abs() > 1.0 {
        let _ = window.set_size(tauri::LogicalSize::new(new_w, new_h));
        let _ = window.center();
    }
}

/// HEIC/HEIF ファイルを macOS sips コマンドで PNG に変換してバイト列と寸法を返す
fn convert_heic_to_png(source_path: &str) -> Result<(Vec<u8>, u32, u32), String> {
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let temp_png = std::env::temp_dir().join(format!("flashcap-heic-{}-{}.png", std::process::id(), ts));

    let output = Command::new("sips")
        .args(["-s", "format", "png", source_path, "--out"])
        .arg(&temp_png)
        .output()
        .map_err(|e| format!("Failed to run sips: {}", e))?;

    if !output.status.success() {
        let _ = std::fs::remove_file(&temp_png);
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("sips conversion failed: {}", stderr));
    }

    let png_data = std::fs::read(&temp_png);
    let _ = std::fs::remove_file(&temp_png);
    let png_data = png_data.map_err(|e| format!("Failed to read converted PNG: {}", e))?;

    let img = image::load_from_memory(&png_data)
        .map_err(|e| format!("Failed to decode converted PNG: {}", e))?;

    Ok((png_data, img.width(), img.height()))
}

/// 画像ファイルを読み込んで ScreenshotResult を生成
fn load_image_result(file_path: String) -> Result<ScreenshotResult, String> {
    if !std::path::Path::new(&file_path).exists() {
        return Err("Image file does not exist".to_string());
    }

    let absolute_path = std::fs::canonicalize(&file_path)
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or(file_path);

    // HEIC/HEIF は image crate が非対応のため macOS sips で PNG に変換
    let ext = std::path::Path::new(&absolute_path)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase())
        .unwrap_or_default();

    if ext == "heic" || ext == "heif" {
        let (png_data, width, height) = convert_heic_to_png(&absolute_path)?;
        return Ok(ScreenshotResult {
            width: width as usize,
            height: height as usize,
            data: STANDARD.encode(&png_data),
            file_path: absolute_path,
        });
    }

    let img_data = std::fs::read(&absolute_path)
        .map_err(|e| format!("Failed to read image: {}", e))?;

    let img = image::load_from_memory(&img_data)
        .map_err(|e| format!("Failed to decode image: {}", e))?;

    // PNG 以外の形式は PNG にエンコードし直す
    let png_data = if image::guess_format(&img_data).map(|f| f == image::ImageFormat::Png).unwrap_or(false) {
        img_data
    } else {
        let mut buf = std::io::Cursor::new(Vec::new());
        img.write_to(&mut buf, image::ImageFormat::Png)
            .map_err(|e| format!("Failed to re-encode as PNG: {}", e))?;
        buf.into_inner()
    };

    Ok(ScreenshotResult {
        width: img.width() as usize,
        height: img.height() as usize,
        data: STANDARD.encode(&png_data),
        file_path: absolute_path,
    })
}

#[tauri::command]
async fn take_screenshot_interactive(
    app: tauri::AppHandle,
) -> Result<ScreenshotResult, String> {
    let file_path = get_screenshot_path(&app)?;

    let mut args = vec!["-i".to_string()];
    if get_exclude_shadow(&app) {
        args.push("-o".to_string());
    }
    args.push(file_path.clone());

    let status = tokio::process::Command::new("screencapture")
        .args(&args)
        .status()
        .await
        .map_err(|e| format!("Failed to run screencapture: {}", e))?;

    if !status.success() {
        return Err("Screenshot was cancelled".to_string());
    }

    let result = load_image_result(file_path)?;
    resize_window_for_image(&app, result.width, result.height);
    Ok(result)
}

/// ウィンドウキャプチャー時のドロップシャドウを除外するか（デフォルト true）
fn get_exclude_shadow(app: &tauri::AppHandle) -> bool {
    app.store("settings.json")
        .ok()
        .and_then(|store| store.get("exclude_shadow"))
        .and_then(|v| v.as_bool())
        .unwrap_or(true)
}

/// 設定からタイマー秒数を取得（デフォルト5秒）
fn get_timer_delay(app: &tauri::AppHandle) -> u32 {
    app.store("settings.json")
        .ok()
        .and_then(|store| store.get("timer_delay"))
        .and_then(|v| v.as_u64())
        .map(|v| v as u32)
        .unwrap_or(5)
}

/// タイマー付きスクリーンショット
/// async にすることで -T 待機中にアプリがフリーズしない
#[tauri::command]
async fn take_screenshot_timer(
    app: tauri::AppHandle,
) -> Result<ScreenshotResult, String> {
    let file_path = get_screenshot_path(&app)?;
    let delay = get_timer_delay(&app).to_string();

    let mut args = vec!["-i".to_string()];
    if get_exclude_shadow(&app) {
        args.push("-o".to_string());
    }
    args.extend(["-T".to_string(), delay, file_path.clone()]);

    let status = tokio::process::Command::new("screencapture")
        .args(&args)
        .status()
        .await
        .map_err(|e| format!("Failed to run screencapture: {}", e))?;

    if !status.success() {
        return Err("Screenshot was cancelled".to_string());
    }

    let result = load_image_result(file_path)?;
    resize_window_for_image(&app, result.width, result.height);
    Ok(result)
}

/// 外部からの画像ファイルを開く
#[tauri::command]
fn load_image_file(
    app: tauri::AppHandle,
    path: String,
) -> Result<ScreenshotResult, String> {
    let result = load_image_result(path)?;
    resize_window_for_image(&app, result.width, result.height);
    Ok(result)
}

/// クリップボードから貼り付けた画像を保存する
#[tauri::command]
fn save_pasted_image(
    app: tauri::AppHandle,
    data_base64: String,
    width: usize,
    height: usize,
) -> Result<ScreenshotResult, String> {
    let dir = PathBuf::from(prepare_save_directory(&app)?);
    let timestamp = Local::now().format("%Y%m%d-%H%M%S");
    let filename = format!("flashcap-paste-{}.png", timestamp);
    let file_path = dir.join(&filename).to_string_lossy().to_string();

    let bytes = STANDARD
        .decode(&data_base64)
        .map_err(|e| format!("Failed to decode base64: {}", e))?;
    std::fs::write(&file_path, &bytes)
        .map_err(|e| format!("Failed to write file: {}", e))?;

    resize_window_for_image(&app, width, height);

    Ok(ScreenshotResult {
        width,
        height,
        data: data_base64,
        file_path,
    })
}

/// base64 PNG データをファイルに書き出す
/// パスは保存先ディレクトリ内に制限する
#[tauri::command]
fn write_image_to_file(app: tauri::AppHandle, path: String, data_base64: String) -> Result<(), String> {
    // 注釈済み画像もスクリーンショットと同じ中身なので、撮影と同じ経路で用意する。
    // 併せて、保存先がまだ無い場合に canonicalize が失敗して書き出せない問題も消える
    let save_dir = std::fs::canonicalize(prepare_save_directory(&app)?)
        .map_err(|e| format!("Failed to resolve save directory: {}", e))?;
    let target = std::fs::canonicalize(&path)
        .or_else(|_| {
            // ファイルが未作成の場合、親ディレクトリで検証
            std::path::Path::new(&path)
                .parent()
                .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidInput, "no parent"))
                .and_then(std::fs::canonicalize)
                .map(|p| p.join(std::path::Path::new(&path).file_name().unwrap()))
        })
        .map_err(|e| format!("Failed to resolve path: {}", e))?;

    if !target.starts_with(&save_dir) {
        return Err(format!(
            "Path '{}' is outside the save directory '{}'",
            target.display(),
            save_dir.display()
        ));
    }

    let bytes = STANDARD
        .decode(&data_base64)
        .map_err(|e| format!("Failed to decode base64: {}", e))?;
    std::fs::write(&target, &bytes).map_err(|e| format!("Failed to write file: {}", e))?;
    Ok(())
}

/// flashcap://ocr でヘッドレス OCR が要求されたか。
///
/// URL は setup の後に `RunEvent::Opened` で届くため、コールド起動では「フロントの
/// 描画完了を待ってウィンドウを表示する」通常の初期化と OCR が並走する。撮影中に
/// ウィンドウが出てくると、それがそのまま撮影範囲に写り込む。
///
/// 起動中に届いた場合の隠す処理 (Opened 側の hide) と対になっていて、こちらは
/// 「これから出てくるのを止める」役割。CaptureHandshake と違って読むだけなので、
/// 状態管理を足さず static で持つ。
static HEADLESS_OCR_REQUESTED: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

fn headless_ocr_requested() -> bool {
    HEADLESS_OCR_REQUESTED.load(std::sync::atomic::Ordering::SeqCst)
}

/// キャプチャー開始のハンドシェイク状態
///
/// キャプチャー開始経路 (--capture コールド / single-instance 再起動 /
/// flashcap://capture) は、いずれも「ウィンドウは show せず do-capture のみ送り、
/// 表示は captureScreen() の撮影完了後 show に一任する」設計に統一している。
/// ただしコールド起動 (WebView 未ロード) では do-capture が登録前のリスナーに届かず
/// 取りこぼすため、frontend-ready 受信を待ってから emit する必要がある。
///
/// frontend_ready と capture_pending を別々の AtomicBool で持つと
/// 「request 側が ready=false を見る → mark_ready 側が ready=true にし pending=false を見る
///  → request 側が pending=true を立てる」の順で do-capture が永久に飛ばない
/// lost-wakeup が起きうるため、Mutex で原子的に判定する。
#[derive(Default)]
struct CaptureHandshake {
    inner: std::sync::Mutex<HandshakeInner>,
}

#[derive(Default)]
struct HandshakeInner {
    frontend_ready: bool,
    capture_pending: bool,
}

impl CaptureHandshake {
    /// ロックを取得する。poison (ロック保持中の panic) しても回復して継続する。
    /// GUI アプリではここで panic 連鎖させてプロセスを落とすより、状態を読めるだけ
    /// 読んで進む方が被害が小さい。
    fn lock(&self) -> std::sync::MutexGuard<'_, HandshakeInner> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// frontend-ready 受信を記録する。受信前にキャプチャーが予約済みだった場合 true を返す
    /// (= 呼び出し側が do-capture を emit すべき)。予約は consume して false に戻す。
    fn mark_ready(&self) -> bool {
        let mut s = self.lock();
        s.frontend_ready = true;
        let pending = s.capture_pending;
        s.capture_pending = false;
        pending
    }

    /// キャプチャーを要求する。frontend が ready 済みなら true (即 emit すべき)、
    /// 未 ready なら予約だけして false を返す (frontend-ready 受信時に emit される)。
    fn request(&self) -> bool {
        let mut s = self.lock();
        if s.frontend_ready {
            true
        } else {
            s.capture_pending = true;
            false
        }
    }

    /// frontend-ready より前にキャプチャーを予約する (--capture コールド起動用)。
    fn set_pending(&self) {
        self.lock().capture_pending = true;
    }

    fn is_ready(&self) -> bool {
        self.lock().frontend_ready
    }

    fn is_pending(&self) -> bool {
        self.lock().capture_pending
    }
}

/// フロントエンドにキャプチャー開始 (do-capture) を通知する。
/// ウィンドウは show しない (撮影完了後に captureScreen() が show する)。
/// frontend が未 ready の場合 (コールド起動) は予約だけ行い、frontend-ready 受信時に emit する。
fn request_capture(app: &tauri::AppHandle) {
    if app.state::<CaptureHandshake>().request() {
        let _ = app.emit("do-capture", ());
    }
}

/// プリファレンスウィンドウを開く (既に開いていればフォーカス)
fn open_preferences_window(app: &tauri::AppHandle) -> tauri::Result<()> {
    if let Some(window) = app.get_webview_window("preferences") {
        let _ = window.set_focus();
        return Ok(());
    }

    WebviewWindowBuilder::new(app, "preferences", WebviewUrl::App("/preferences".into()))
        .title("Preferences")
        .inner_size(500.0, 350.0)
        .resizable(true)
        .center()
        .build()?;

    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(video::RecordingState::default())
        .manage(CaptureHandshake::default())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_drag::init())
        .plugin(tauri_plugin_store::Builder::new().build())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_deep_link::init())
        .plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
            // --capture-screen-text: 既存インスタンスでヘッドレス OCR 実行
            if args.iter().any(|a| a == "--capture-screen-text") {
                let handle = app.clone();
                tauri::async_runtime::spawn(async move {
                    ocr::run_headless_ocr(&handle, false).await;
                });
                return;
            }
            // --capture: 再起動時に点滅させず、そのままキャプチャーを開始する
            if args.iter().any(|a| a == "--capture") {
                request_capture(app);
                return;
            }
            // 既に起動中のインスタンスに対して再度起動コマンドが来た場合
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.show();
                let _ = w.set_focus();
            }
            // args[0] はバイナリパス。args[1..] がファイルパス
            let file_paths: Vec<String> = args.iter().skip(1)
                .filter(|a| {
                    let p = std::path::Path::new(a);
                    p.exists() && p.extension().map_or(false, |ext| {
                        ext.to_str().map_or(false, |e| SUPPORTED_IMAGE_EXTENSIONS.contains(&e.to_lowercase().as_str()))
                    })
                })
                .cloned()
                .collect();
            if !file_paths.is_empty() {
                let _ = app.emit("open-file", file_paths);
            } else {
                let _ = app.emit("reactivate", ());
            }
        }))
        .setup(|app| {
            let handle = app.handle();

            // macOS ネイティブメニュー
            let preferences =
                MenuItem::with_id(handle, "preferences", "Preferences...", true, Some("CmdOrCtrl+,"))?;

            let app_submenu = Submenu::with_items(
                handle,
                app.package_info().name.clone(),
                true,
                &[
                    &PredefinedMenuItem::about(handle, None, None)?,
                    &PredefinedMenuItem::separator(handle)?,
                    &preferences,
                    &PredefinedMenuItem::separator(handle)?,
                    &PredefinedMenuItem::services(handle, None)?,
                    &PredefinedMenuItem::separator(handle)?,
                    &PredefinedMenuItem::hide(handle, None)?,
                    &PredefinedMenuItem::hide_others(handle, None)?,
                    &PredefinedMenuItem::separator(handle)?,
                    &PredefinedMenuItem::quit(handle, None)?,
                ],
            )?;

            let edit_submenu = Submenu::with_items(
                handle,
                "Edit",
                true,
                &[
                    &PredefinedMenuItem::undo(handle, None)?,
                    &PredefinedMenuItem::redo(handle, None)?,
                    &PredefinedMenuItem::separator(handle)?,
                    &PredefinedMenuItem::cut(handle, None)?,
                    &PredefinedMenuItem::copy(handle, None)?,
                    &PredefinedMenuItem::paste(handle, None)?,
                    &PredefinedMenuItem::select_all(handle, None)?,
                ],
            )?;

            let window_submenu = Submenu::with_items(
                handle,
                "Window",
                true,
                &[
                    &PredefinedMenuItem::minimize(handle, None)?,
                    &PredefinedMenuItem::maximize(handle, None)?,
                    &PredefinedMenuItem::close_window(handle, None)?,
                ],
            )?;

            let menu = Menu::with_items(handle, &[&app_submenu, &edit_submenu, &window_submenu])?;
            app.set_menu(menu)?;

            app.on_menu_event(move |app, event| {
                if event.id() == "preferences" {
                    let _ = open_preferences_window(app);
                }
            });

            // --capture-screen-text: ヘッドレス OCR モード
            let headless_ocr = std::env::args().any(|a| a == "--capture-screen-text");
            if headless_ocr {
                let handle = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    ocr::run_headless_ocr(&handle, true).await;
                });
            }

            // 通常起動時はメインウィンドウを表示してアクティブにする
            // (ウィンドウは visible:false で生成される)
            // setup 段階で show() すると WebView 未描画の白いウィンドウが
            // 一瞬見えてしまうため、フロントの描画完了 (frontend-ready) を
            // 待ってから表示する。frontend-ready は WebView の JS ロード後に
            // 一度だけ emit される。
            // ヘッドレス OCR 時は表示しない
            if !headless_ocr {
                let handle = app.handle().clone();

                // --capture コールド起動: frontend-ready 受信前にキャプチャーを予約しておく。
                // (URL スキーム capture のコールド起動は RunEvent::Opened → request_capture が
                //  同じ予約を行う)
                if std::env::args().any(|a| a == "--capture") {
                    app.state::<CaptureHandshake>().set_pending();
                }

                let handle_cb = handle.clone();
                app.once_any("frontend-ready", move |_| {
                    if handle_cb.state::<CaptureHandshake>().mark_ready() {
                        // キャプチャーが予約済み: do-capture を送る。
                        // captureScreen 側は撮影前に hide を呼ぶが、ウィンドウは
                        // visible:false のままなので hide は no-op。撮影完了後に
                        // show される。ここではウィンドウを表示しない。
                        let _ = handle_cb.emit("do-capture", ());
                    } else if !headless_ocr_requested() {
                        // 通常起動: フロント描画完了を待って表示する。
                        // ヘッドレス OCR 中は出さない (撮影に写り込むため)。
                        if let Some(w) = handle_cb.get_webview_window("main") {
                            let _ = w.show();
                            let _ = w.set_focus();
                        }
                    }
                });

                // フェイルセーフ: frontend-ready が一定時間来ない場合
                // (WebView の JS ロード失敗・onMount 到達前の例外等) は
                // ウィンドウが永久に非表示のままになるため、強制的に表示する。
                // ただしキャプチャー予約中 (--capture / flashcap://capture のコールド起動)
                // は表示しない。低速環境で frontend-ready が2秒を超えてから届くと
                // show → captureScreen の hide で点滅するため。予約中に frontend が
                // 永久に来ない場合はそもそも captureScreen が動かずキャプチャー不能なので、
                // 空ウィンドウを出しても無意味。
                // ヘッドレス OCR 中も同様に表示しない (撮影に写り込むため。OCR は
                // フロントを使わないので、ウィンドウが出ないままでも困らない)。
                tauri::async_runtime::spawn(async move {
                    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                    let handshake = handle.state::<CaptureHandshake>();
                    if !handshake.is_ready()
                        && !handshake.is_pending()
                        && !headless_ocr_requested()
                    {
                        if let Some(w) = handle.get_webview_window("main") {
                            let _ = w.show();
                            let _ = w.set_focus();
                        }
                    }
                });
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![take_screenshot_interactive, take_screenshot_timer, write_image_to_file, load_image_file, open_save_directory, get_default_save_directory, save_pasted_image, ocr::ocr_image, ocr::ocr_capture_region, ocr::show_notification, video::open_region_selector, video::cancel_region_selection, video::broadcast_region_selecting, video::list_capture_windows, video::start_video_recording, video::stop_video_recording, video::export_video, video::check_ffmpeg_available])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| {
            match event {
                tauri::RunEvent::ExitRequested { .. } => {
                    // 録画中にアプリ終了する場合、録画プロセスを停止して孤立を防ぐ
                    video::abort_recording(app);
                }
                tauri::RunEvent::Reopen { .. } => {
                    // Dock アイコンクリック時: ウインドウを表示してフォーカス
                    if let Some(w) = app.get_webview_window("main") {
                        let _ = w.show();
                        let _ = w.set_focus();
                    }
                    // キャプチャーボタンを点滅させて目立たせる
                    let _ = app.emit("reactivate", ());
                }
                tauri::RunEvent::Opened { urls } => {
                    // flashcap:// URL scheme によるコマンド実行
                    for url in &urls {
                        if url.scheme() != "flashcap" {
                            continue;
                        }
                        match url.host_str() {
                            // flashcap://ocr: ヘッドレス OCR を実行
                            Some("ocr") => {
                                // ヘッドレス OCR は画面を撮るので、メインウィンドウが
                                // 出ていると撮影範囲に自分が写り込む。出さないための
                                // 経路が 2 つあり、両方要る:
                                // - 既に起動している場合、URL を開くと macOS がアプリを
                                //   前面に出すので、今出ているものを hide する
                                // - コールド起動の場合、ウィンドウはまだ visible:false
                                //   なので hide は no-op。代わりにフラグを立てて、
                                //   frontend-ready 側の show を止める
                                HEADLESS_OCR_REQUESTED
                                    .store(true, std::sync::atomic::Ordering::SeqCst);
                                if let Some(w) = app.get_webview_window("main") {
                                    let _ = w.hide();
                                }
                                let handle = app.clone();
                                tauri::async_runtime::spawn(async move {
                                    ocr::run_headless_ocr(&handle, false).await;
                                });
                                return;
                            }
                            // flashcap://capture: キャプチャーを開始する。
                            // コールド起動時は frontend-ready を待ってから emit される
                            // (request_capture 内のハンドシェイクで取りこぼしを防ぐ)。
                            Some("capture") => {
                                request_capture(app);
                                return;
                            }
                            _ => {}
                        }
                    }

                    // ファイル関連付けや Dock へのドロップで開かれた場合
                    let file_paths: Vec<String> = urls.iter()
                        .filter_map(|url| {
                            if url.scheme() != "file" {
                                return None;
                            }
                            let path = url.to_file_path().ok()?;
                            // 対応する画像拡張子のみ許可
                            let ext = path.extension()?.to_str()?.to_lowercase();
                            if SUPPORTED_IMAGE_EXTENSIONS.contains(&ext.as_str()) {
                                Some(path.to_string_lossy().to_string())
                            } else {
                                None
                            }
                        })
                        .collect();
                    if !file_paths.is_empty() {
                        if let Some(w) = app.get_webview_window("main") {
                            let _ = w.show();
                            let _ = w.set_focus();
                        }
                        let _ = app.emit("open-file", file_paths);
                    }
                }
                _ => {}
            }
        });
}
