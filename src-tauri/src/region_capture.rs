//! 撮影範囲を矩形で選ばせるオーバーレイ (Windows の撮影で使う。CYBERNEURA-DEV-882)
//!
//! macOS は `screencapture -i` が範囲選択まで面倒を見るが、Windows にはそれに当たる
//! コマンドが無い。そこで「先にモニター全体を撮り、その静止画をモニターいっぱいの
//! 枠なしウインドウに映して、その上で矩形を選ばせる」方式にしている (Greenshot /
//! ShareX と同じ作り)。選ばれた範囲は撮ってあった画像から切り出すので、選んでいる
//! 間に画面が動いても、写るのは撮影ボタンを押した瞬間の画面になる。
//!
//! Windows の切り取りツール (`ms-screenclip:`) を呼ぶ方式は採らなかった。結果が
//! クリップボード経由でしか返らず (ユーザーのクリップボードを上書きする)、Esc で
//! やめたことをアプリから知る手段が無く (待ち続けるかタイムアウトしかない)、
//! 切り取りツール自体がアンインストールできるうえ、Windows 10 / 11 やツールの版で
//! 挙動が変わるため。
//!
//! 流れ:
//! 1. `select_region` が撮影済みの画像を PNG の data URL にして `RegionCaptureState` に
//!    置き、非表示のオーバーレイ (`capture-region-<n>` ウインドウ) を開いて待つ
//! 2. オーバーレイ (`src/routes/capture-region/+page.svelte`) が
//!    `capture_region_preview` で画像を受け取り、描き終えたら `capture_region_ready`
//!    で表示してもらう (先に出すと WebView の白い下地が一瞬モニター全体に出る)
//! 3. 選び終えたら `capture_region_finish` に画像ピクセル単位の矩形 (やめたら null) を渡す
//! 4. `select_region` が矩形を画像の内側に丸めて切り出す
//!
//! ウインドウが Alt+F4 などで閉じられた時も、待っている側が取り残されないよう
//! `Destroyed` で送信側を捨てて「やめた」扱いにする。
//!
//! このモジュール自体はどの OS でもコンパイルする (Linux の cargo check / test で
//! 検査できるように)。呼んでいるのは Windows の `capture_screen_to` だけ。
#![cfg_attr(not(windows), allow(dead_code))]

use base64::{engine::general_purpose::STANDARD, Engine as _};
use serde::Deserialize;
use std::sync::Mutex;
use tauri::{Manager, WebviewUrl, WebviewWindowBuilder};
use tokio::sync::oneshot;

/// オーバーレイのウインドウラベルの接頭辞。実際のラベルは撮影ごとに番号を付ける
/// (`capture-region-<n>`)。閉じかけの前回のウインドウと取り違えないため
const OVERLAY_LABEL_PREFIX: &str = "capture-region-";

static NEXT_OVERLAY_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// オーバーレイが描画の完了を知らせてこない時に、自分で表示するまでの時間
///
/// 表示されないまま待ち続けると、メインウインドウも隠れているので画面に何も出ず、
/// Esc で抜けることもできない。出してしまえば Esc / Alt+F4 でやめられる
const READY_FALLBACK: std::time::Duration = std::time::Duration::from_secs(3);

/// オーバーレイから返る矩形 (撮影画像のピクセル単位。左上が原点)
#[derive(Debug, Clone, Copy, Deserialize, PartialEq)]
pub struct Selection {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// オーバーレイを置くモニターの位置と大きさ (物理ピクセル。仮想デスクトップ座標)
#[derive(Debug, Clone, Copy)]
pub(crate) struct OverlayGeometry {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

struct Pending {
    /// この選択に使っているオーバーレイのラベル。コマンドはこのウインドウからの
    /// 呼び出しだけを受け付ける
    label: String,
    /// オーバーレイに映す画像 (data:image/png;base64,...)
    preview: String,
    tx: oneshot::Sender<Option<Selection>>,
}

/// 進行中の範囲選択。同時に 1 つだけ
#[derive(Default)]
pub(crate) struct RegionCaptureState(Mutex<Option<Pending>>);

/// 選ばれた矩形を、画像の内側に収まる整数の範囲 (x, y, width, height) に直す
///
/// 端数は外側へ丸める (選んだ範囲が 1px でも欠けないように)。画像からはみ出した部分は
/// 削る。幅か高さが 0 になる、または NaN / 無限大を含む矩形は None (撮らない)。
pub(crate) fn crop_bounds(
    sel: Selection,
    image_width: u32,
    image_height: u32,
) -> Option<(u32, u32, u32, u32)> {
    let values = [sel.x, sel.y, sel.width, sel.height];
    if values.iter().any(|v| !v.is_finite()) || sel.width <= 0.0 || sel.height <= 0.0 {
        return None;
    }
    let clamp_x = |v: f64| v.clamp(0.0, f64::from(image_width));
    let clamp_y = |v: f64| v.clamp(0.0, f64::from(image_height));
    let left = clamp_x(sel.x.floor());
    let top = clamp_y(sel.y.floor());
    let right = clamp_x((sel.x + sel.width).ceil());
    let bottom = clamp_y((sel.y + sel.height).ceil());
    if right <= left || bottom <= top {
        return None;
    }
    Some((
        left as u32,
        top as u32,
        (right - left) as u32,
        (bottom - top) as u32,
    ))
}

/// 撮影済みの画像の上で範囲を選ばせ、選ばれた部分を切り出して返す
///
/// やめた場合 (Esc / 右クリック / ウインドウを閉じた) は "cancelled" を含むエラーを返す
/// (フロントはこの文字列でキャンセルとエラーを見分ける。macOS の screencapture と同じ)。
pub(crate) async fn select_region(
    app: &tauri::AppHandle,
    image: image::RgbaImage,
    geometry: OverlayGeometry,
) -> Result<image::RgbaImage, String> {
    let (image, preview) = tauri::async_runtime::spawn_blocking(move || {
        let mut png = std::io::Cursor::new(Vec::new());
        image.write_to(&mut png, image::ImageFormat::Png).map(|_| {
            let url = format!(
                "data:image/png;base64,{}",
                STANDARD.encode(png.into_inner())
            );
            (image, url)
        })
    })
    .await
    .map_err(|e| format!("Screen capture task failed: {}", e))?
    .map_err(|e| format!("Failed to encode the screenshot: {}", e))?;

    let label = format!(
        "{}{}",
        OVERLAY_LABEL_PREFIX,
        NEXT_OVERLAY_ID.fetch_add(1, std::sync::atomic::Ordering::SeqCst)
    );
    let rx = {
        let state = app.state::<RegionCaptureState>();
        let mut pending = state.0.lock().unwrap_or_else(|e| e.into_inner());
        if pending.is_some() {
            return Err("An area selection is already in progress".to_string());
        }
        let (tx, rx) = oneshot::channel();
        *pending = Some(Pending {
            label: label.clone(),
            preview,
            tx,
        });
        rx
    };

    let window = match open_overlay(app, &label, geometry) {
        Ok(window) => window,
        Err(e) => {
            take_pending(app, &label);
            return Err(e);
        }
    };

    // 描画の完了が届かなくても、一定時間で出す (READY_FALLBACK の説明)
    let fallback = window.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(READY_FALLBACK).await;
        if matches!(fallback.is_visible(), Ok(false)) {
            show_overlay(&fallback);
        }
    });

    // 送信側が捨てられた (ウインドウが閉じられた) 時は Err になる。どちらも「やめた」
    let selection = rx.await.ok().flatten();
    take_pending(app, &label);
    let _ = window.close();

    let selection = selection.ok_or_else(|| "Screenshot was cancelled".to_string())?;
    let (x, y, width, height) = crop_bounds(selection, image.width(), image.height())
        .ok_or_else(|| "Screenshot was cancelled: the selected area is empty".to_string())?;
    Ok(image::imageops::crop_imm(&image, x, y, width, height).to_image())
}

/// label のオーバーレイの選択が進行中なら取り出す (別のオーバーレイの選択には触らない)
fn take_pending(app: &tauri::AppHandle, label: &str) -> Option<Pending> {
    let state = app.state::<RegionCaptureState>();
    let mut pending = state.0.lock().unwrap_or_else(|e| e.into_inner());
    if pending.as_ref().is_some_and(|p| p.label == label) {
        pending.take()
    } else {
        None
    }
}

fn open_overlay(
    app: &tauri::AppHandle,
    label: &str,
    geometry: OverlayGeometry,
) -> Result<tauri::WebviewWindow, String> {
    let window = WebviewWindowBuilder::new(app, label, WebviewUrl::App("/capture-region".into()))
        .title("FlashCap")
        .decorations(false)
        .resizable(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .shadow(false)
        .visible(false)
        .build()
        .map_err(|e| format!("Failed to open the area selection: {}", e))?;

    // モニターの物理座標で重ねる (論理座標だと、倍率の違うモニターが並んだ時にずれる)。
    // 大きさは位置の後に決める。別の倍率のモニターへ移ると Windows が大きさを直すため
    let _ = window.set_position(tauri::PhysicalPosition::new(geometry.x, geometry.y));
    let _ = window.set_size(tauri::PhysicalSize::new(geometry.width, geometry.height));

    let handle = app.clone();
    let label = label.to_string();
    window.on_window_event(move |event| {
        if let tauri::WindowEvent::Destroyed = event {
            // 送信側を捨てると、待っている select_region が「やめた」として戻る
            take_pending(&handle, &label);
        }
    });
    Ok(window)
}

fn show_overlay(window: &tauri::WebviewWindow) {
    let _ = window.show();
    let _ = window.set_focus();
}

const NOT_THE_OVERLAY: &str = "No area selection is in progress for this window";

fn is_current_overlay(state: &RegionCaptureState, window: &tauri::WebviewWindow) -> bool {
    let pending = state.0.lock().unwrap_or_else(|e| e.into_inner());
    pending.as_ref().is_some_and(|p| p.label == window.label())
}

/// オーバーレイに映す画像 (data URL) を返す
#[tauri::command]
pub fn capture_region_preview(
    window: tauri::WebviewWindow,
    state: tauri::State<'_, RegionCaptureState>,
) -> Result<String, String> {
    let pending = state.0.lock().unwrap_or_else(|e| e.into_inner());
    pending
        .as_ref()
        .filter(|p| p.label == window.label())
        .map(|p| p.preview.clone())
        .ok_or_else(|| NOT_THE_OVERLAY.to_string())
}

/// 画像を描き終えたオーバーレイを表示して前に出す
#[tauri::command]
pub fn capture_region_ready(
    window: tauri::WebviewWindow,
    state: tauri::State<'_, RegionCaptureState>,
) -> Result<(), String> {
    if !is_current_overlay(&state, &window) {
        return Err(NOT_THE_OVERLAY.to_string());
    }
    show_overlay(&window);
    Ok(())
}

/// 選んだ矩形 (画像ピクセル単位) を返す。null ならやめる
#[tauri::command]
pub fn capture_region_finish(
    window: tauri::WebviewWindow,
    state: tauri::State<'_, RegionCaptureState>,
    selection: Option<Selection>,
) -> Result<(), String> {
    let pending = {
        let mut pending = state.0.lock().unwrap_or_else(|e| e.into_inner());
        if !pending.as_ref().is_some_and(|p| p.label == window.label()) {
            return Err(NOT_THE_OVERLAY.to_string());
        }
        pending.take()
    };
    if let Some(pending) = pending {
        // 受信側が既に居ない (待つのをやめた) なら、何もしなくてよい
        let _ = pending.tx.send(selection);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sel(x: f64, y: f64, width: f64, height: f64) -> Selection {
        Selection {
            x,
            y,
            width,
            height,
        }
    }

    #[test]
    fn whole_pixels_are_kept_as_is() {
        assert_eq!(
            crop_bounds(sel(10.0, 20.0, 30.0, 40.0), 100, 100),
            Some((10, 20, 30, 40))
        );
    }

    #[test]
    fn fractions_are_rounded_outwards() {
        // 10.4..40.6 を選んだら 10..41 を撮る (選んだ範囲の端を欠かさない)
        assert_eq!(
            crop_bounds(sel(10.4, 20.5, 30.2, 10.1), 100, 100),
            Some((10, 20, 31, 11))
        );
    }

    #[test]
    fn the_selection_is_clamped_to_the_image() {
        assert_eq!(
            crop_bounds(sel(-5.0, -5.0, 20.0, 20.0), 100, 100),
            Some((0, 0, 15, 15))
        );
        assert_eq!(
            crop_bounds(sel(90.0, 95.0, 50.0, 50.0), 100, 100),
            Some((90, 95, 10, 5))
        );
        assert_eq!(
            crop_bounds(sel(0.0, 0.0, 100.0, 100.0), 100, 100),
            Some((0, 0, 100, 100))
        );
    }

    #[test]
    fn empty_or_outside_selections_are_refused() {
        assert_eq!(crop_bounds(sel(10.0, 10.0, 0.0, 10.0), 100, 100), None);
        assert_eq!(crop_bounds(sel(10.0, 10.0, 10.0, -1.0), 100, 100), None);
        assert_eq!(crop_bounds(sel(150.0, 10.0, 10.0, 10.0), 100, 100), None);
        assert_eq!(crop_bounds(sel(-20.0, 10.0, 10.0, 10.0), 100, 100), None);
    }

    #[test]
    fn non_finite_values_are_refused() {
        assert_eq!(crop_bounds(sel(f64::NAN, 0.0, 10.0, 10.0), 100, 100), None);
        assert_eq!(
            crop_bounds(sel(0.0, 0.0, f64::INFINITY, 10.0), 100, 100),
            None
        );
    }
}
