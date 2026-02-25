use base64::{engine::general_purpose::STANDARD, Engine as _};
use chrono::Local;
use serde::{Deserialize, Serialize};
use std::process::Command;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::{Emitter, Manager, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_store::StoreExt;

const SUPPORTED_IMAGE_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "gif", "bmp", "webp", "tiff", "tif"];

#[derive(Debug, Serialize, Deserialize)]
pub struct ScreenshotResult {
    pub width: usize,
    pub height: usize,
    pub data: String,      // base64 encoded PNG
    pub file_path: String, // saved file path
}

/// 設定から保存先ディレクトリを取得する
/// "tmp" -> /tmp/flashcap/
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
        "tmp" => "/tmp/flashcap".to_string(),
        "macos_default" => get_macos_screenshot_dir(),
        s if s.starts_with("custom:") => s.strip_prefix("custom:").unwrap().to_string(),
        _ => "/tmp/flashcap".to_string(),
    }
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
                .unwrap_or_else(|| "/tmp/flashcap".to_string())
        })
}

fn get_screenshot_path(app: &tauri::AppHandle) -> String {
    let dir = std::path::PathBuf::from(get_save_directory(app));
    let _ = std::fs::create_dir_all(&dir);
    let timestamp = Local::now().format("%Y%m%d-%H%M%S");
    let filename = format!("flashcap-{}.png", timestamp);
    dir.join(filename).to_string_lossy().to_string()
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

/// 画像ファイルを読み込んで ScreenshotResult を生成
fn load_image_result(file_path: String) -> Result<ScreenshotResult, String> {
    if !std::path::Path::new(&file_path).exists() {
        return Err("Image file does not exist".to_string());
    }

    let absolute_path = std::fs::canonicalize(&file_path)
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or(file_path);

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
    let file_path = get_screenshot_path(&app);

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
    let file_path = get_screenshot_path(&app);
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

/// base64 PNG データをファイルに書き出す
/// パスは保存先ディレクトリ内に制限する
#[tauri::command]
fn write_image_to_file(app: tauri::AppHandle, path: String, data_base64: String) -> Result<(), String> {
    let save_dir = std::fs::canonicalize(get_save_directory(&app))
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
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_drag::init())
        .plugin(tauri_plugin_store::Builder::new().build())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
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

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![take_screenshot_interactive, take_screenshot_timer, write_image_to_file, load_image_file])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| {
            match event {
                tauri::RunEvent::Reopen { .. } => {
                    // Dock アイコンクリック時: ウインドウを表示してフォーカスするだけ
                    if let Some(w) = app.get_webview_window("main") {
                        let _ = w.show();
                        let _ = w.set_focus();
                    }
                }
                tauri::RunEvent::Opened { urls } => {
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
