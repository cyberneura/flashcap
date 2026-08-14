use chrono::Local;
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tauri::{Emitter, Manager, WebviewUrl, WebviewWindowBuilder};

#[derive(Debug, Serialize)]
pub struct VideoResult {
    /// 録画した生 .mov の絶対パス (temp ディレクトリ内)
    pub file_path: String,
}

/// 進行中の録画プロセスを保持する Tauri 管理ステート
#[derive(Default)]
pub struct RecordingState(pub Mutex<Option<Recording>>);

pub struct Recording {
    /// バックグラウンドの screencapture プロセス
    pub child: std::process::Child,
    /// 出力先 (temp の生 .mov)
    pub path: String,
}

/// 録画の生データを置く作業ディレクトリ ($TEMP/flashcap) を、
/// 所有者専用にしたうえで返す
/// asset protocol の scope ($TEMP/**) に含まれるためプレビュー可能
///
/// 以前は作成失敗を握り潰していたが、この呼び出しは「録画の生データを他ユーザーから
/// 読まれない場所に置く」という保証を兼ねるようになったので、失敗したら録画を始めない。
fn video_temp_dir() -> Result<PathBuf, String> {
    crate::ensure_private_flashcap_dir()
        .map_err(|e| format!("Failed to prepare the working directory: {}", e))
}

/// 過去の録画生データ (rec-*.mov) を削除する (best-effort)
/// 書き出し結果のみ保存する方針のため、生データは溜めない。
/// keep は削除しない (今回の録画ファイル)。プレビュー中のファイルを
/// 消さないよう、録画成功後に呼ぶこと
fn cleanup_old_recordings(dir: &Path, keep: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path == keep {
            continue;
        }
        let is_recording = path.extension().and_then(|s| s.to_str()) == Some("mov")
            && path
                .file_name()
                .and_then(|s| s.to_str())
                .map_or(false, |n| n.starts_with("rec-"));
        if is_recording {
            let _ = std::fs::remove_file(&path);
        }
    }
}

/// システムにインストールされた実行ファイルを探す
/// GUI 起動時は PATH が最小限 (/usr/bin:/bin 等) なため、
/// Homebrew / MacPorts の定番パスを優先的に直接探索する
fn find_binary(name: &str) -> Option<String> {
    let candidates = [
        format!("/opt/homebrew/bin/{}", name),
        format!("/usr/local/bin/{}", name),
        format!("/opt/local/bin/{}", name),
    ];
    for c in candidates.iter() {
        if Path::new(c).is_file() {
            return Some(c.clone());
        }
    }
    // フォールバック: PATH を which で探索
    if let Ok(out) = std::process::Command::new("/usr/bin/which").arg(name).output() {
        if out.status.success() {
            let p = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !p.is_empty() && Path::new(&p).is_file() {
                return Some(p);
            }
        }
    }
    None
}

fn find_ffmpeg() -> Option<String> {
    find_binary("ffmpeg")
}

/// 書き出し UI の有効/無効判定用: システム ffmpeg があるか
#[tauri::command]
pub fn check_ffmpeg_available() -> bool {
    find_ffmpeg().is_some()
}

/// 指定 PID に SIGINT を送る。screencapture -v は SIGINT で録画を finalize して終了する
fn send_sigint(pid: u32) {
    let _ = std::process::Command::new("/bin/kill")
        .args(["-INT", &pid.to_string()])
        .status();
}

/// アプリ終了時などに進行中の録画を停止する (プロセスの孤立を防ぐ)
pub fn abort_recording(app: &tauri::AppHandle) {
    let state = app.state::<RecordingState>();
    let Ok(mut guard) = state.0.lock() else {
        return;
    };
    if let Some(mut rec) = guard.take() {
        send_sigint(rec.child.id());
        let _ = rec.child.wait();
    }
}

/// 画面上のウィンドウ情報 (ウィンドウスナップ用)
/// 座標はグローバル Quartz ポイント (screencapture -R / オーバーレイと同じ座標系)
#[derive(Debug, Serialize)]
pub struct CaptureWindow {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub app: String,
    pub title: String,
}

/// 画面上の通常ウィンドウを前面→背面の順で列挙する (CGWindowList)。
/// 自前のオーバーレイは level=1000 (layer != 0) のため自動的に除外される。
/// メインウィンドウは選択中 hide されているため on-screen に含まれない
#[cfg(target_os = "macos")]
#[tauri::command]
pub fn list_capture_windows() -> Vec<CaptureWindow> {
    use core_foundation::array::{CFArray, CFArrayRef};
    use core_foundation::base::{CFType, TCFType};
    use core_foundation::dictionary::CFDictionary;
    use core_foundation::number::CFNumber;
    use core_foundation::string::CFString;

    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        fn CGWindowListCopyWindowInfo(option: u32, relative_to_window: u32) -> CFArrayRef;
    }
    const ON_SCREEN_ONLY: u32 = 1 << 0; // kCGWindowListOptionOnScreenOnly
    const EXCLUDE_DESKTOP: u32 = 1 << 4; // kCGWindowListExcludeDesktopElements
    const NULL_WINDOW_ID: u32 = 0; // kCGNullWindowID

    let raw = unsafe { CGWindowListCopyWindowInfo(ON_SCREEN_ONLY | EXCLUDE_DESKTOP, NULL_WINDOW_ID) };
    if raw.is_null() {
        return vec![];
    }
    let list: CFArray<CFDictionary<CFString, CFType>> =
        unsafe { CFArray::wrap_under_create_rule(std::mem::transmute(raw)) };

    let key = |s: &'static str| CFString::from_static_string(s);
    let num_f64 = |d: &CFDictionary<CFString, CFType>, k: &'static str| -> Option<f64> {
        d.find(key(k))?.downcast::<CFNumber>()?.to_f64()
    };
    let str_val = |d: &CFDictionary<CFString, CFType>, k: &'static str| -> String {
        d.find(key(k))
            .and_then(|v| v.downcast::<CFString>())
            .map(|s| s.to_string())
            .unwrap_or_default()
    };

    let mut out = Vec::new();
    for d in list.iter() {
        // 通常ウィンドウのみ (メニューバー・Dock・自前オーバーレイ等は layer != 0)
        let layer = num_f64(&d, "kCGWindowLayer").unwrap_or(-1.0);
        if layer != 0.0 {
            continue;
        }
        // 不可視・極小ウィンドウを除外
        let alpha = num_f64(&d, "kCGWindowAlpha").unwrap_or(1.0);
        if alpha <= 0.0 {
            continue;
        }
        let Some(bounds) = d
            .find(key("kCGWindowBounds"))
            .and_then(|v| v.downcast::<CFDictionary>())
        else {
            continue;
        };
        // bounds は型なし CFDictionary なので CFString/CFType として読み直す
        let bounds: CFDictionary<CFString, CFType> =
            unsafe { CFDictionary::wrap_under_get_rule(bounds.as_concrete_TypeRef()) };
        let (Some(x), Some(y), Some(w), Some(h)) = (
            num_f64(&bounds, "X"),
            num_f64(&bounds, "Y"),
            num_f64(&bounds, "Width"),
            num_f64(&bounds, "Height"),
        ) else {
            continue;
        };
        if w < 50.0 || h < 50.0 {
            continue;
        }
        out.push(CaptureWindow {
            x,
            y,
            width: w,
            height: h,
            app: str_val(&d, "kCGWindowOwnerName"),
            title: str_val(&d, "kCGWindowName"),
        });
    }
    out
}

#[cfg(not(target_os = "macos"))]
#[tauri::command]
pub fn list_capture_windows() -> Vec<CaptureWindow> {
    vec![]
}

/// オーバーレイ窓を「全 Space + フルスクリーンアプリの上」にも表示させる。
/// Tauri の always_on_top だけでは macOS のフルスクリーン Space を覆えないため、
/// NSWindow の collectionBehavior と level を直接設定する
#[cfg(target_os = "macos")]
fn elevate_overlay_window(win: &tauri::WebviewWindow) {
    use objc::runtime::Object;
    use objc::{msg_send, sel, sel_impl};

    let Ok(ns_ptr) = win.ns_window() else {
        eprintln!("[flashcap] elevate: ns_window() failed for {}", win.label());
        return;
    };
    let ns = ns_ptr as *mut Object;
    // NSWindowCollectionBehavior のビット:
    //   CanJoinAllSpaces(1<<0) | Stationary(1<<4) | FullScreenAuxiliary(1<<8)
    const COLLECTION_BEHAVIOR: u64 = (1 << 0) | (1 << 4) | (1 << 8);
    // フルスクリーンアプリやメニューバーより前面に出す高い window level
    const OVERLAY_LEVEL: i64 = 1000; // NSScreenSaverWindowLevel 相当
    unsafe {
        let _: () = msg_send![ns, setCollectionBehavior: COLLECTION_BEHAVIOR];
        let _: () = msg_send![ns, setLevel: OVERLAY_LEVEL];
        // 実際に適用されたかを読み戻して確認 (tao 側の上書き検出用)
        let behavior: u64 = msg_send![ns, collectionBehavior];
        let level: i64 = msg_send![ns, level];
        eprintln!(
            "[flashcap] elevate {}: applied behavior={} (want {}), level={} (want {})",
            win.label(),
            behavior,
            COLLECTION_BEHAVIOR,
            level,
            OVERLAY_LEVEL
        );
    }
}

#[cfg(not(target_os = "macos"))]
fn elevate_overlay_window(_win: &tauri::WebviewWindow) {}

/// アプリの activation policy を切り替える。
/// macOS では Dock に出る通常アプリ (Regular) のウィンドウは他アプリの
/// フルスクリーン Space に重ねられない (Electron の app.dock.hide() と同じ既知制約)。
/// 範囲選択中だけ Accessory にして、フルスクリーンアプリ上にオーバーレイを出す
#[cfg(target_os = "macos")]
fn set_accessory_activation_policy(app: &tauri::AppHandle, accessory: bool) {
    use objc::runtime::{Class, Object, BOOL, YES};
    use objc::{msg_send, sel, sel_impl};

    let _ = app.run_on_main_thread(move || unsafe {
        let Some(cls) = Class::get("NSApplication") else {
            return;
        };
        let ns_app: *mut Object = msg_send![cls, sharedApplication];
        // NSApplicationActivationPolicy: Regular=0, Accessory=1
        let policy: i64 = if accessory { 1 } else { 0 };
        let ok: BOOL = msg_send![ns_app, setActivationPolicy: policy];
        if accessory {
            // policy 切替後に再アクティブ化しないとキー入力が外れる
            let _: () = msg_send![ns_app, activateIgnoringOtherApps: YES];
        }
        eprintln!("[flashcap] activation policy -> {} (ok={})", policy, ok as i32);
    });
}

#[cfg(not(target_os = "macos"))]
fn set_accessory_activation_policy(_app: &tauri::AppHandle, _accessory: bool) {}

/// 開いている範囲選択オーバーレイをすべて閉じる
fn close_region_selectors(app: &tauri::AppHandle) {
    for (label, window) in app.webview_windows() {
        if label.starts_with("region-selector") {
            let _ = window.close();
        }
    }
}

/// 範囲選択用の透明オーバーレイを各ディスプレイに 1 枚ずつ開く (マルチモニタ対応)
/// 各オーバーレイには自身のディスプレイの Quartz ポイント原点 (qx, qy) を渡す。
/// screencapture -R はグローバル Quartz 座標 (main 左上=原点, y下, ポイント単位) を取り、
/// 各モニタの Quartz 原点は Tauri の position()/scale_factor() で求まる
#[tauri::command]
pub fn open_region_selector(app: tauri::AppHandle) -> Result<(), String> {
    let result = open_region_selector_impl(&app);
    if result.is_err() {
        // 失敗時: 既に作成済みのオーバーレイを閉じ、メインウィンドウと
        // activation policy を確実に復帰する (中途半端な状態を残さない)
        close_region_selectors(&app);
        set_accessory_activation_policy(&app, false);
        if let Some(main) = app.get_webview_window("main") {
            let _ = main.show();
            let _ = main.set_focus();
        }
    }
    result
}

fn open_region_selector_impl(app: &tauri::AppHandle) -> Result<(), String> {
    let main = app
        .get_webview_window("main")
        .ok_or_else(|| "main window not found".to_string())?;
    let _ = main.hide();

    // 既に開いていれば作り直さない
    let already_open = app
        .webview_windows()
        .keys()
        .any(|l| l.starts_with("region-selector"));
    if already_open {
        return Ok(());
    }

    let monitors = main
        .available_monitors()
        .map_err(|e| format!("Failed to enumerate monitors: {}", e))?;
    if monitors.is_empty() {
        return Err("No monitors found".to_string());
    }

    // 選択中だけ Accessory にしてフルスクリーン Space 上にもオーバーレイを出せるようにする
    set_accessory_activation_policy(app, true);

    for (i, m) in monitors.iter().enumerate() {
        let pos = m.position();
        let size = m.size();
        let scale = m.scale_factor();
        // 論理ポイント (= Quartz ポイント原点 / サイズ)。
        // Retina 混在では物理ピクセルで配置すると窓のスケールで誤変換されるため、
        // スケール非依存な論理ポイントで配置する
        let lx = pos.x as f64 / scale;
        let ly = pos.y as f64 / scale;
        let lw = size.width as f64 / scale;
        let lh = size.height as f64 / scale;
        eprintln!(
            "[flashcap] monitor {}: phys_pos=({},{}) phys_size=({}x{}) scale={} -> logical pos=({},{}) size=({}x{})",
            i, pos.x, pos.y, size.width, size.height, scale, lx, ly, lw, lh
        );
        let label = format!("region-selector-{}", i);
        let url = format!("/region-select?qx={}&qy={}", lx, ly);

        // visible_on_all_workspaces は使わない (tao が collectionBehavior を
        // 上書きするため)。必要なフラグは elevate_overlay_window で直接設定する
        let win = WebviewWindowBuilder::new(app, &label, WebviewUrl::App(url.into()))
            .transparent(true)
            .decorations(false)
            .always_on_top(true)
            .skip_taskbar(true)
            .shadow(false)
            .visible(false)
            .build()
            .map_err(|e| format!("Failed to open region selector: {}", e))?;

        // 配置・サイズ・NSWindow 設定はすべてメインスレッドで行う。
        // AppKit (setLevel/setCollectionBehavior) はメインスレッド必須で、
        // ワーカースレッドから呼ぶと効かないため
        let win_main = win.clone();
        let _ = win.run_on_main_thread(move || {
            // 論理座標で配置 (NSWindow はポイント単位なのでスケール非依存)
            let _ = win_main.set_position(tauri::LogicalPosition::new(lx, ly));
            let _ = win_main.set_size(tauri::LogicalSize::new(lw, lh));
            let _ = win_main.show();
            // フルスクリーンアプリの上にも出すための NSWindow 設定。
            // show() が tao 側の level/collectionBehavior を再適用する可能性があるため
            // show の後に設定する
            elevate_overlay_window(&win_main);
        });
    }

    // 最初のオーバーレイにフォーカス (Esc 受付用)
    if let Some(w) = app.get_webview_window("region-selector-0") {
        let _ = w.set_focus();
    }
    Ok(())
}

/// あるオーバーレイが選択を開始したことを他のオーバーレイに伝え、選択をクリアさせる。
/// Tauri のイベント配信 (emit / emit_to) はオーバーレイ webview に届かなかったため、
/// 各 webview への eval (evaluateJavaScript 直叩き) で確実に通知する
#[tauri::command]
pub fn broadcast_region_selecting(app: tauri::AppHandle, origin: String) {
    for (label, window) in app.webview_windows() {
        if !label.starts_with("region-selector") || label == origin {
            continue;
        }
        // ラベルは英数とハイフンのみなので {:?} の文字列リテラルで JS に安全に埋め込める
        let js = format!("window.__regionClear && window.__regionClear({:?});", origin);
        let result = window.eval(&js);
        eprintln!(
            "[flashcap] region-clear eval -> {} (from {}): {:?}",
            label, origin, result
        );
    }
}

/// 範囲選択をキャンセルする (オーバーレイを閉じてメインを戻す)
#[tauri::command]
pub fn cancel_region_selection(app: tauri::AppHandle) {
    close_region_selectors(&app);
    set_accessory_activation_policy(&app, false);
    if let Some(main) = app.get_webview_window("main") {
        let _ = main.show();
        let _ = main.set_focus();
    }
}

/// 指定領域の録画をバックグラウンドで開始する
/// x,y,width,height はグローバル Quartz 座標 (ポイント単位、main 左上=原点、y下)。
/// オーバーレイ側で各モニタの Quartz 原点 + ローカル選択座標として算出済み。
/// screencapture -R にそのまま渡す (外部ディスプレイは負座標になりうる)
#[tauri::command]
pub fn start_video_recording(
    app: tauri::AppHandle,
    state: tauri::State<'_, RecordingState>,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
) -> Result<String, String> {
    // 上限は最大級のディスプレイ構成でも届かない防御値
    if !(1..=16384).contains(&width) || !(1..=16384).contains(&height) {
        return Err("Invalid recording region".to_string());
    }

    let dir = video_temp_dir()?;
    let timestamp = Local::now().format("%Y%m%d-%H%M%S");
    let path = dir
        .join(format!("rec-{}.mov", timestamp))
        .to_string_lossy()
        .to_string();

    let region = format!("{},{},{},{}", x, y, width, height);
    // -r: DPI メタデータを付けない (プレイヤーが表示時に勝手にスケールするのを防ぐ)
    let child = std::process::Command::new("screencapture")
        .args(["-v", "-x", "-r", "-R", &region, &path])
        .spawn()
        .map_err(|e| format!("Failed to start recording: {}", e))?;

    {
        let mut guard = state.0.lock().map_err(|e| e.to_string())?;
        // 既存の録画があれば停止しておく (二重起動防止)
        if let Some(mut prev) = guard.take() {
            send_sigint(prev.child.id());
            let _ = prev.child.wait();
        }
        *guard = Some(Recording {
            child,
            path: path.clone(),
        });
    }

    // オーバーレイを閉じてメインを表示、録画開始を通知
    close_region_selectors(&app);
    set_accessory_activation_policy(&app, false);
    if let Some(main) = app.get_webview_window("main") {
        let _ = main.show();
        let _ = main.set_focus();
    }
    let _ = app.emit("recording-started", ());

    Ok(path)
}

/// 録画を停止し、finalize された .mov のパスを返す。
/// screencapture の finalize は数百ms〜数秒かかるため、wait は blocking タスクで行い
/// コマンドスレッドを塞がない
#[tauri::command]
pub async fn stop_video_recording(
    state: tauri::State<'_, RecordingState>,
) -> Result<VideoResult, String> {
    let rec = state.0.lock().map_err(|e| e.to_string())?.take();
    let Some(mut rec) = rec else {
        return Err("No active recording".to_string());
    };
    let path = rec.path.clone();

    tokio::task::spawn_blocking(move || {
        send_sigint(rec.child.id());
        // SIGINT 後、screencapture が finalize して終了するのを待つ
        let _ = rec.child.wait();
    })
    .await
    .map_err(|e| format!("Failed to wait for recording process: {}", e))?;

    if !Path::new(&path).exists() {
        return Err("Recording file was not created".to_string());
    }

    // 片付けは best-effort。ここで失敗しても録画自体は成功しているので、
    // 結果を捨てずに掃除だけ諦める
    if let Ok(dir) = video_temp_dir() {
        cleanup_old_recordings(&dir, Path::new(&path));
    }

    Ok(VideoResult { file_path: path })
}

/// pad_color を ffmpeg 用の安全な色指定 (0xRRGGBB) に正規化する
/// フィルタ文字列インジェクションを防ぐため hex 6 桁のみ許可、不正値は黒
fn sanitize_color(hex: &str) -> String {
    let h = hex.trim().trim_start_matches('#');
    if h.len() == 6 && h.chars().all(|c| c.is_ascii_hexdigit()) {
        format!("0x{}", h)
    } else {
        "0x000000".to_string()
    }
}

/// サイズプリセット + リサイズモードから ffmpeg の scale フィルタ文字列を組み立てる
/// すべて偶数寸法になる (h264 の要件)
fn build_scale_filter(
    size_preset: &str,
    resize_mode: &str,
    pad: bool,
    pad_color: &str,
) -> Result<String, String> {
    match size_preset {
        // 等倍はドットバイドット: scale だと奇数寸法時に全体が補間リサンプルされるため、
        // crop で偶数化のみ行う (補間ゼロ)
        "original" => Ok("crop=trunc(iw/2)*2:trunc(ih/2)*2:0:0".to_string()),
        "half" => Ok("scale=trunc(iw/4)*2:trunc(ih/4)*2".to_string()),
        "hd" | "720p" | "square512" => {
            let (w, h) = match size_preset {
                "hd" => (1920, 1080),
                "720p" => (1280, 720),
                _ => (512, 512),
            };
            match resize_mode {
                "fill" => Ok(format!(
                    "scale={w}:{h}:force_original_aspect_ratio=increase,crop={w}:{h}"
                )),
                "fit" => {
                    if pad {
                        let color = sanitize_color(pad_color);
                        Ok(format!(
                            "scale={w}:{h}:force_original_aspect_ratio=decrease,pad={w}:{h}:(ow-iw)/2:(oh-ih)/2:{color}"
                        ))
                    } else {
                        Ok(format!(
                            "scale={w}:{h}:force_original_aspect_ratio=decrease:force_divisible_by=2"
                        ))
                    }
                }
                other => Err(format!("Invalid resize_mode: {}", other)),
            }
        }
        other => Err(format!("Invalid size_preset: {}", other)),
    }
}

/// ffmpeg を実行し、失敗時は stderr 末尾を返す。
/// デバッグ用にコマンドと失敗時の stderr をターミナルにも出す
async fn run_ffmpeg(ffmpeg: &str, args: &[String]) -> Result<(), String> {
    eprintln!("[flashcap] ffmpeg {}", args.join(" "));
    let output = tokio::process::Command::new(ffmpeg)
        .args(args)
        .output()
        .await
        .map_err(|e| format!("Failed to run ffmpeg: {}", e))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let tail: Vec<&str> = stderr.lines().rev().take(12).collect();
        let tail = tail.into_iter().rev().collect::<Vec<_>>().join("\n");
        eprintln!("[flashcap] ffmpeg FAILED ({}):\n{}", output.status, tail);
        return Err(format!("ffmpeg failed:\n{}", tail));
    }
    Ok(())
}

/// 録画した生データをトリム・形式変換・リサイズして保存ディレクトリに書き出す
#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub async fn export_video(
    app: tauri::AppHandle,
    input: String,
    start_sec: f64,
    end_sec: f64,
    format: String,
    size_preset: String,
    resize_mode: String,
    pad: bool,
    pad_color: String,
    fps: u32,
) -> Result<String, String> {
    let ffmpeg = find_ffmpeg()
        .ok_or_else(|| "ffmpeg not found. Install it with `brew install ffmpeg`.".to_string())?;

    // 形式の検証
    if format != "mp4" && format != "gif" {
        return Err(format!("Invalid format: {}", format));
    }

    // トリム範囲の検証
    if !start_sec.is_finite() || !end_sec.is_finite() || start_sec < 0.0 || end_sec <= start_sec {
        return Err(format!(
            "Invalid trim range: start={}, end={}",
            start_sec, end_sec
        ));
    }
    let duration = end_sec - start_sec;

    // 入力は temp 作業ディレクトリ内に制限する
    let temp_dir = std::fs::canonicalize(video_temp_dir()?)
        .map_err(|e| format!("Failed to resolve temp dir: {}", e))?;
    let input_canon = std::fs::canonicalize(&input)
        .map_err(|e| format!("Failed to resolve input '{}': {}", input, e))?;
    if !input_canon.starts_with(&temp_dir) {
        return Err("Input video is outside the working directory".to_string());
    }
    let input_str = input_canon.to_string_lossy().to_string();

    // scale フィルタ組み立て。setsar=1 で SAR を正規化し、
    // プレイヤーの表示スケーリング (非正方画素扱い) を防ぐ
    let vf_base = format!(
        "{},setsar=1",
        build_scale_filter(&size_preset, &resize_mode, pad, &pad_color)?
    );

    // 出力先 (保存ディレクトリ内)
    // 書き出し先も撮影と同じ経路で用意する。ここだけ get_save_directory() を直接
    // 使うと、旧版が 0755 で作った作業ディレクトリを締め直さないまま動画を置くことになる
    let save_dir = crate::prepare_save_directory(&app)?;
    let save_dir_canon = std::fs::canonicalize(&save_dir)
        .map_err(|e| format!("Failed to resolve save directory: {}", e))?;
    // ミリ秒まで含めて同一秒内の連続書き出しでも衝突しないようにする
    let timestamp = Local::now().format("%Y%m%d-%H%M%S-%3f");
    let ext = if format == "gif" { "gif" } else { "mp4" };
    let output_path = save_dir_canon
        .join(format!("flashcap-{}.{}", timestamp, ext))
        .to_string_lossy()
        .to_string();

    let start = format!("{:.3}", start_sec);
    let dur = format!("{:.3}", duration);

    // -ss/-t は入力オプションとして input の直前に置く。
    // 出力側に置くと複数入力 (gif の palette) で意図しない入力に適用される
    if format == "mp4" {
        let args: Vec<String> = vec![
            "-y".into(),
            "-ss".into(), start,
            "-t".into(), dur,
            "-i".into(), input_str,
            "-vf".into(), vf_base,
            "-c:v".into(), "libx264".into(),
            "-crf".into(), "20".into(),
            "-preset".into(), "veryfast".into(),
            "-pix_fmt".into(), "yuv420p".into(),
            "-movflags".into(), "+faststart".into(),
            "-an".into(),
            output_path.clone(),
        ];
        run_ffmpeg(&ffmpeg, &args).await?;
    } else {
        // gif: palettegen → paletteuse の 2 パス
        let fps = fps.clamp(1, 50);
        let palette_path = temp_dir
            .join(format!(
                "palette-{}-{}.png",
                std::process::id(),
                Local::now().format("%H%M%S%3f")
            ))
            .to_string_lossy()
            .to_string();

        // -update 1: 単一画像出力 (ffmpeg 8 の image2 警告対策)
        let pass1: Vec<String> = vec![
            "-y".into(),
            "-ss".into(), start.clone(),
            "-t".into(), dur.clone(),
            "-i".into(), input_str.clone(),
            "-vf".into(), format!("{},fps={},palettegen=stats_mode=diff", vf_base, fps),
            "-update".into(), "1".into(),
            "-frames:v".into(), "1".into(),
            palette_path.clone(),
        ];
        run_ffmpeg(&ffmpeg, &pass1).await?;

        let pass2: Vec<String> = vec![
            "-y".into(),
            "-ss".into(), start,
            "-t".into(), dur,
            "-i".into(), input_str,
            "-i".into(), palette_path.clone(),
            "-lavfi".into(),
            format!(
                "{},fps={}[x];[x][1:v]paletteuse=dither=bayer:bayer_scale=3",
                vf_base, fps
            ),
            output_path.clone(),
        ];
        let result = run_ffmpeg(&ffmpeg, &pass2).await;
        let _ = std::fs::remove_file(&palette_path);
        if result.is_err() {
            // 失敗時に 0 バイトの出力ゴミを保存フォルダに残さない
            let _ = std::fs::remove_file(&output_path);
        }
        result?;
    }

    Ok(output_path)
}
