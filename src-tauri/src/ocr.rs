use base64::{engine::general_purpose::STANDARD, Engine as _};
use serde::Deserialize;
use std::process::Stdio;
use tauri::Manager;

/// macOS 通知センターに通知を表示
fn notify(title: &str, body: &str) {
    let _ = std::process::Command::new("osascript")
        .args([
            "-e",
            &format!(
                "display notification \"{}\" with title \"{}\"",
                body.replace('\\', "\\\\").replace('"', "\\\""),
                title.replace('\\', "\\\\").replace('"', "\\\""),
            ),
        ])
        .output();
}

/// テキストをクリップボードにコピー (pbcopy)
fn copy_to_clipboard(text: &str) -> Result<(), String> {
    use std::io::Write;
    let mut child = std::process::Command::new("pbcopy")
        .stdin(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to spawn pbcopy: {}", e))?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(text.as_bytes())
            .map_err(|e| format!("Failed to write to pbcopy: {}", e))?;
    }
    let status = child
        .wait()
        .map_err(|e| format!("pbcopy failed to wait: {}", e))?;
    if !status.success() {
        return Err(format!("pbcopy exited with non-zero status: {:?}", status));
    }
    Ok(())
}

/// macOS 通知センターに通知を表示する Tauri コマンド
#[tauri::command]
pub fn show_notification(title: String, body: String) {
    notify(&title, &body);
}

#[derive(Debug, Deserialize)]
pub struct OcrRegion {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// Swift スクリプトのパスを取得（開発時は resources/ 直下、ビルド時はバンドルリソース）
fn get_ocr_script_path(app: &tauri::AppHandle) -> Result<std::path::PathBuf, String> {
    // Tauri のリソースディレクトリから取得
    let resource_path = app
        .path()
        .resource_dir()
        .map_err(|e| format!("Failed to get resource dir: {}", e))?
        .join("resources")
        .join("ocr.swift");

    if resource_path.exists() {
        return Ok(resource_path);
    }

    // 開発時フォールバック: src-tauri/resources/ocr.swift
    let dev_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("resources")
        .join("ocr.swift");

    if dev_path.exists() {
        return Ok(dev_path);
    }

    Err(format!(
        "ocr.swift not found at {:?} or {:?}",
        resource_path, dev_path
    ))
}

/// OCR を実行して認識テキストを返す
async fn recognize_text(
    app: &tauri::AppHandle,
    png_data: &[u8],
    region: Option<OcrRegion>,
    img_width: u32,
    img_height: u32,
) -> Result<String, String> {
    let script_path = get_ocr_script_path(app)?;
    let base64_input = STANDARD.encode(png_data);

    let mut args = vec![script_path.to_string_lossy().to_string()];
    args.extend(["--size".to_string(), format!("{},{}", img_width, img_height)]);

    if let Some(ref r) = region {
        args.extend([
            "--region".to_string(),
            format!("{},{},{},{}", r.x, r.y, r.width, r.height),
        ]);
    }

    let mut child = tokio::process::Command::new("/usr/bin/swift")
        .args(&args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to spawn swift: {}", e))?;

    // stdin に base64 データを書き込む
    if let Some(mut stdin) = child.stdin.take() {
        use tokio::io::AsyncWriteExt;
        stdin
            .write_all(base64_input.as_bytes())
            .await
            .map_err(|e| format!("Failed to write to stdin: {}", e))?;
        stdin
            .write_all(b"\n")
            .await
            .map_err(|e| format!("Failed to write newline: {}", e))?;
        drop(stdin);
    }

    let output = child
        .wait_with_output()
        .await
        .map_err(|e| format!("Failed to wait for swift process: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("OCR failed: {}", stderr));
    }

    let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
    Ok(text)
}

/// 表示中の画像から OCR でテキスト抽出
#[tauri::command]
pub async fn ocr_image(
    app: tauri::AppHandle,
    data_base64: String,
    region: Option<OcrRegion>,
) -> Result<String, String> {
    let png_data = STANDARD
        .decode(&data_base64)
        .map_err(|e| format!("Failed to decode base64: {}", e))?;

    let img = image::load_from_memory(&png_data)
        .map_err(|e| format!("Failed to decode image: {}", e))?;

    recognize_text(&app, &png_data, region, img.width(), img.height()).await
}

/// OCR 1 回分の作業ディレクトリを排他的に作り、その中の出力先パスと共に返す
///
/// **予測可能なパスに撮らせてはいけない。** 以前は /tmp/flashcap-ocr-<pid>.png 固定で、
/// /tmp は全ユーザー共有だったため、先回りして同名のシンボリックリンクを置かれると
/// screencapture の出力先がリンク先に化け (任意ファイルの上書き)、逆に読ませたい
/// ファイルへ向けられればその内容が OCR されてクリップボードに入った。
///
/// 他ユーザーからの攻撃を実際に塞いでいるのは ensure_private_flashcap_dir() の方で、
/// 親が 0700 かつ自分の所有だと確かめられている以上、そこへリンクを仕込む余地は無い
/// (TMPDIR が無くて /tmp に落ちた場合も、そのディレクトリ自体を締めてから使う)。
///
/// ここで確保するのは**名前**の方。ファイルではなくディレクトリを mkdir で作る。
/// mkdir は既存の名前に対して必ず失敗する (= O_EXCL 相当) ので、通れば
/// 「この名前は自分が取った」ことが確定し、しかも**中のファイル名を使い終わるまで
/// 予約したままにできる**。ファイルを O_EXCL で作って消す方式だと、消した瞬間から
/// screencapture が作るまでの間が空いてしまい、予約になっていなかった。
/// 出力ファイル自体はまだ存在しないので、screencapture が既存ファイルを上書き
/// できるかどうかにも依存しない。
///
/// 戻り値の OcrWorkdir は drop 時に中身ごと消える。
///
/// 撮った画像を残さないための後始末は、途中で抜ける経路が多く手で書くと必ず漏れる
/// (特に **Future がキャンセルされた場合は、以降の行が 1 行も動かない**)。
/// drop に載せておけば、キャンセルでも早期 return でも同じように消える。
struct OcrWorkdir {
    dir: std::path::PathBuf,
    /// screencapture に渡す出力先 (dir の中の固定名)
    path: String,
}

impl Drop for OcrWorkdir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn create_ocr_workdir() -> Result<OcrWorkdir, String> {
    use std::os::unix::fs::DirBuilderExt;

    let base = crate::ensure_private_flashcap_dir()
        .map_err(|e| format!("Failed to prepare the temp directory: {}", e))?;

    // 名前は PID + ナノ秒。PID だけだと同一プロセス内の同時 OCR が衝突する。
    // 衝突しても mkdir が弾くので、取り違えではなく取り直しになる。
    for _ in 0..16 {
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let dir = base.join(format!("ocr-{}-{}", std::process::id(), ts));
        match std::fs::DirBuilder::new().mode(0o700).create(&dir) {
            Ok(()) => {
                let path = dir.join("capture.png").to_string_lossy().to_string();
                return Ok(OcrWorkdir { dir, path });
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(format!("Failed to create the temp directory: {}", e)),
        }
    }
    Err("Failed to reserve a temp directory for OCR".to_string())
}

/// screencapture -i → 一時ファイル → OCR の共通処理
async fn screencapture_and_ocr(app: &tauri::AppHandle) -> Result<String, String> {
    // work が生きている間だけ一時ディレクトリが存在する。以降どこで抜けても
    // (キャンセル含む) drop が撮影結果ごと消すので、明示的な後始末は書かない
    let work = create_ocr_workdir()?;

    let status = tokio::process::Command::new("screencapture")
        .args(["-i", &work.path])
        // キャンセルでこの Future が捨てられた時、screencapture を生かしたままにすると
        // 消した後のディレクトリへ書き込もうとし続ける。道連れに終了させる
        .kill_on_drop(true)
        .status()
        .await
        .map_err(|e| format!("Failed to run screencapture: {}", e))?;

    if !status.success() {
        return Err("Screenshot was cancelled".to_string());
    }

    let png_data =
        std::fs::read(&work.path).map_err(|e| format!("Failed to read screenshot: {}", e))?;
    // 以降はメモリ上のバイト列だけで足りる。OCR (数秒かかる) の間、画面の中身を
    // ディスクに置いたままにしない
    drop(work);

    let img = image::load_from_memory(&png_data)
        .map_err(|e| format!("Failed to decode image: {}", e))?;

    recognize_text(app, &png_data, None, img.width(), img.height()).await
}

/// screencapture -i で新規キャプチャ → OCR → テキストのみ返す
#[tauri::command]
pub async fn ocr_capture_region(app: tauri::AppHandle) -> Result<String, String> {
    screencapture_and_ocr(&app).await
}

/// ヘッドレスモード: screencapture → OCR → clipboard → 通知 (→ 終了)
pub async fn run_headless_ocr(app: &tauri::AppHandle, exit_after: bool) {
    match screencapture_and_ocr(app).await {
        Ok(text) if !text.is_empty() => {
            let char_count = text.chars().count();
            match copy_to_clipboard(&text) {
                Ok(_) => {
                    notify("FlashCap", &format!("Copied {} characters", char_count));
                }
                Err(e) => {
                    notify("FlashCap", &format!("Failed to copy: {}", e));
                }
            }
        }
        Ok(_) => {
            notify("FlashCap", "No text recognized");
        }
        Err(e) if e != "Screenshot was cancelled" => {
            notify("FlashCap", &format!("OCR failed: {}", e));
        }
        _ => {}
    }
    if exit_after {
        std::process::exit(0);
    }
}
