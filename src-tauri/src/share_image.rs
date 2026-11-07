//! 共有に回す画像 (画像のコピー / ドラッグ / 撮影後の自動コピー) だけを、Retina の 2 倍から
//! 等倍に縮める (CYBERNEURA-DEV-1211)。保存ファイルとサイドバーの画像は原寸のまま。
//!
//! 設定は `share_scale_down` (bool、既定 false)。判定と縮小の純粋な部分は `share_scale.rs`。
//! 縮めた PNG は `$TMPDIR/flashcap/share/<元の名前>.png` に書き (ドラッグで渡すファイル)、
//! share ディレクトリは起動時に掃除する (`video.rs` の古い録画の削除と同じ方針)。

use std::path::{Path, PathBuf};

use base64::{engine::general_purpose::STANDARD, Engine as _};
use tauri::Manager;

use crate::share_scale::{maybe_scale_down, should_scale_down, ScreenInfo};

/// 設定ファイルのキー。Preferences の "Share at 1x" トグルが書く
pub(crate) const SHARE_SCALE_DOWN_KEY: &str = "share_scale_down";

pub(crate) fn is_enabled() -> bool {
    crate::config::get(SHARE_SCALE_DOWN_KEY)
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
}

/// 撮影したモニターとして、メインウインドウのあるモニター (無ければメインモニター) を使う
/// (撮影時のモニターは記録していない。ウインドウは撮影の直前まで同じモニターにあるのが普通)
pub(crate) fn screen_info(app: &tauri::AppHandle) -> Option<ScreenInfo> {
    let monitor = app
        .get_webview_window("main")
        .and_then(|w| w.current_monitor().ok().flatten())
        .or_else(|| app.primary_monitor().ok().flatten())?;
    let scale = monitor.scale_factor();
    let size = monitor.size();
    Some(ScreenInfo {
        logical_w: (size.width as f64 / scale).round() as u32,
        logical_h: (size.height as f64 / scale).round() as u32,
        scale_factor: scale,
    })
}

/// 縮小が有効な時だけモニター情報を返す (自動コピーが撮影スレッドの外で使う)
pub(crate) fn screen_info_if_enabled(app: &tauri::AppHandle) -> Option<ScreenInfo> {
    if is_enabled() {
        screen_info(app)
    } else {
        None
    }
}

/// 縮めた PNG の置き場 ($TMPDIR/flashcap/share)。作業ディレクトリと同じく自分専用 (0700)
fn share_dir() -> Result<PathBuf, String> {
    let dir = crate::ensure_private_flashcap_dir()
        .map_err(|e| format!("Failed to prepare the working directory: {}", e))?
        .join("share");
    crate::create_private_dir(&dir)
        .map_err(|e| format!("Failed to prepare the share directory: {}", e))?;
    Ok(dir)
}

/// 起動時に前回の縮小画像を消す (best-effort)。共有に渡した後は要らないので溜めない。
/// **symlink は辿らない**: `share` 自体が symlink なら (別の場所を指しているなら) 中を列挙せず、
/// 中の項目も `symlink_metadata` で通常ファイルのものだけ消す (リンク先を消さない)
pub(crate) fn cleanup_share_dir() {
    let dir = crate::flashcap_temp_dir().join("share");
    let Ok(meta) = std::fs::symlink_metadata(&dir) else {
        return;
    };
    if !meta.is_dir() {
        return;
    }
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let is_regular = std::fs::symlink_metadata(&path)
            .map(|m| m.file_type().is_file())
            .unwrap_or(false);
        if is_regular {
            let _ = std::fs::remove_file(&path);
        }
    }
}

/// `make_share_image` の結果。縮めた時だけ `data` (base64 PNG) が入る (クリップボードへ
/// 入れるのはフロントなので、ファイルを読み直さずに済ませる)。縮めなければ元のパスのまま
#[derive(serde::Serialize)]
pub struct ShareImage {
    pub path: String,
    pub data: Option<String>,
}

/// 共有に回す画像を用意する。設定が ON、撮影で得た画像 (`captured`)、かつ Retina のモニターで
/// 論理サイズより大きい時だけ 1/2 に縮めた PNG を share ディレクトリに書いてそのパスを返す。
/// それ以外は元のパスをそのまま返す。元のファイルには触らない。
/// 縮める元は `data_base64` (今まさに共有する PNG。画像のコピーが渡す) があればそれで、無ければ
/// `path` のファイル (ドラッグ。渡すのはファイルそのものなので中身は書き戻し済み)。コピーで
/// ファイルを読み直すと、undo の後 (注釈が無くなり書き戻しが飛ばされた時) に古い注釈入りの
/// ファイルを縮めてしまう。
/// 画像の読み込み・縮小・エンコード・書き込みは `spawn_blocking` に逃がす (Retina の全画面は
/// 数十 MB になり、メインスレッドで行うとコピー / ドラッグのたびに UI が固まる)
#[tauri::command]
pub async fn make_share_image(
    app: tauri::AppHandle,
    path: String,
    captured: bool,
    data_base64: Option<String>,
) -> Result<ShareImage, String> {
    let original = ShareImage {
        path: path.clone(),
        data: None,
    };
    if !captured || !is_enabled() {
        return Ok(original);
    }
    // モニター情報だけはここ (コマンドのスレッド) で取る
    let Some(screen) = screen_info(&app) else {
        return Ok(original);
    };
    tauri::async_runtime::spawn_blocking(move || {
        build_share_image(&path, data_base64.as_deref(), screen)
    })
    .await
    .map_err(|e| format!("Failed to prepare the share image: {}", e))?
}

/// `make_share_image` の重い部分 (ブロッキングスレッドで走る)
fn build_share_image(
    path: &str,
    data_base64: Option<&str>,
    screen: ScreenInfo,
) -> Result<ShareImage, String> {
    let original = ShareImage {
        path: path.to_string(),
        data: None,
    };
    let source = Path::new(path);
    let img = match data_base64 {
        Some(data) => {
            let bytes = STANDARD
                .decode(data)
                .map_err(|e| format!("Failed to decode the image: {}", e))?;
            image::load_from_memory(&bytes).map_err(|e| format!("Failed to read the image: {}", e))?
        }
        None => image::open(source).map_err(|e| format!("Failed to read the image: {}", e))?,
    };
    if !should_scale_down(
        img.width(),
        img.height(),
        screen.logical_w,
        screen.logical_h,
        screen.scale_factor,
    ) {
        return Ok(original);
    }
    let small = maybe_scale_down(img, Some(screen));
    let mut png = Vec::new();
    small
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .map_err(|e| format!("Failed to encode the image: {}", e))?;
    // 中身は常に PNG なので、元が JPEG 等でも拡張子は .png にする
    let stem = source
        .file_stem()
        .and_then(|s| s.to_str())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| "Invalid image path".to_string())?;
    let out = share_dir()?.join(format!("{stem}.png"));
    // 名前が予測できる場所なので symlink を辿らない (save_pasted_image と同じ)
    crate::write_without_following_symlinks(&out, &png)
        .map_err(|e| format!("Failed to write the share image: {}", e))?;
    Ok(ShareImage {
        path: out.to_string_lossy().to_string(),
        data: Some(STANDARD.encode(&png)),
    })
}
