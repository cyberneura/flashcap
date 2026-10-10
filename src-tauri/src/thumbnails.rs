//! 左サイドバーのサムネイルブラウザ (CYBERNEURA-DEV-995)
//!
//! 保存先フォルダ (Preferences の保存先) の直下にある画像を新しい順に並べ、
//! サムネイルを作って返す。表示・クリック・ドラッグはフロント
//! (`src/lib/ThumbnailSidebar.svelte`) の仕事で、ここは読むだけ。
//!
//! - **サムネイルは保存先フォルダの直下のファイルにしか作らない。** フロントから任意の
//!   パスを渡されても、他の場所の画像を読んで返す口にしない (一覧と同じ範囲に揃える)。
//! - **symlink は一覧にも載せず、サムネイルも作らない。** 保存先に置かれたリンク越しに
//!   フォルダの外のファイルを読ませないため。
//! - asset プロトコルで原寸の画像を直接見せる形にはしていない。scope を保存先 (任意の
//!   フォルダになりうる) へ広げることになり、Retina のスクリーンショットを原寸のまま
//!   何十枚も WebView に読ませることにもなるため。

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use base64::{engine::general_purpose::STANDARD, Engine as _};
use serde::Serialize;

/// 一覧に載せる最大件数。保存先がスクリーンショットを何千枚も溜めた
/// フォルダ (macOS の既定の保存先 = デスクトップ等) でも、一覧を作る手間を抑える
const LIST_LIMIT: usize = 300;

/// サムネイルの長辺 (px)。サイドバーの幅 (論理 224px) を Retina で等倍に近く描ける大きさ
const THUMBNAIL_MAX: u32 = 400;

#[derive(Debug, Serialize, PartialEq)]
pub(crate) struct SavedImage {
    pub path: String,
    pub name: String,
    /// 作成日時 (UNIX ミリ秒)。作成日時を取れないファイルシステムでは更新日時
    pub created_ms: u64,
    /// 更新日時 (UNIX ミリ秒)。注釈を書き戻すと変わるので、フロントはサムネイルの作り直しに使う
    pub modified_ms: u64,
    pub size: u64,
}

fn to_millis(time: SystemTime) -> u64 {
    time.duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// dir の直下にある画像ファイルを、作成日時の新しい順に最大 limit 件返す
///
/// 読めないエントリは飛ばす (1 件のために一覧全体を失敗させない)。
fn list_images_in(dir: &Path, limit: usize) -> Result<Vec<SavedImage>, String> {
    let entries =
        std::fs::read_dir(dir).map_err(|e| format!("Failed to read '{}': {}", dir.display(), e))?;

    let mut images: Vec<SavedImage> = entries
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| {
            let path = entry.path();
            // DirEntry::metadata は symlink を辿らない。リンクはここで落ちる
            let meta = entry.metadata().ok()?;
            if !meta.is_file() || !crate::is_supported_image_path(&path) {
                return None;
            }
            let modified = meta.modified().ok()?;
            let created = meta.created().unwrap_or(modified);
            Some(SavedImage {
                name: entry.file_name().to_string_lossy().to_string(),
                path: path.to_string_lossy().to_string(),
                created_ms: to_millis(created),
                modified_ms: to_millis(modified),
                size: meta.len(),
            })
        })
        .collect();

    // 同じ秒に撮ったもの同士でも順序がぶれないよう、名前の逆順を第 2 キーにする
    // (flashcap-<日時>.png は名前の順 = 撮った順)
    images.sort_by(|a, b| {
        b.created_ms
            .cmp(&a.created_ms)
            .then_with(|| b.name.cmp(&a.name))
    });
    images.truncate(limit);
    Ok(images)
}

/// path が save_dir の直下にある通常ファイル (symlink でない) なら、その実体のパスを返す
///
/// save_dir は canonicalize 済みであること。
fn resolve_within(save_dir: &Path, path: &str) -> Result<PathBuf, String> {
    let given = Path::new(path);
    let meta = std::fs::symlink_metadata(given)
        .map_err(|e| format!("Failed to read '{}': {}", path, e))?;
    if !meta.is_file() {
        return Err(format!("'{}' is not a regular file", path));
    }
    let target =
        dunce::canonicalize(given).map_err(|e| format!("Failed to resolve '{}': {}", path, e))?;
    if target.parent() != Some(save_dir) {
        return Err(format!(
            "'{}' is not in the save directory '{}'",
            target.display(),
            save_dir.display()
        ));
    }
    Ok(target)
}

/// 画像を長辺 THUMBNAIL_MAX 以内に縮めた PNG を、data URL で返す
fn make_thumbnail(path: &Path) -> Result<String, String> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase())
        .unwrap_or_default();

    let img = if ext == "heic" || ext == "heif" {
        let (png, _, _) = crate::convert_heic_to_png(&path.to_string_lossy())?;
        image::load_from_memory(&png)
    } else {
        image::ImageReader::open(path)
            .map_err(|e| format!("Failed to open image: {}", e))?
            .with_guessed_format()
            .map_err(|e| format!("Failed to read image: {}", e))?
            .decode()
    }
    .map_err(|e| format!("Failed to decode image: {}", e))?;

    // 元から小さい画像は拡大しない
    let thumb = if img.width() > THUMBNAIL_MAX || img.height() > THUMBNAIL_MAX {
        img.thumbnail(THUMBNAIL_MAX, THUMBNAIL_MAX)
    } else {
        img
    };

    let mut buf = std::io::Cursor::new(Vec::new());
    thumb
        .write_to(&mut buf, image::ImageFormat::Png)
        .map_err(|e| format!("Failed to encode thumbnail: {}", e))?;
    Ok(format!(
        "data:image/png;base64,{}",
        STANDARD.encode(buf.into_inner())
    ))
}

/// 保存先フォルダの画像を新しい順に返す
#[tauri::command]
pub(crate) async fn list_saved_images(app: tauri::AppHandle) -> Result<Vec<SavedImage>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        // 実体のパスで返す。load_image_file が開いた画像のパスを canonicalize するので、
        // 保存先が symlink や .. を含んでいると字句のままのパスでは「今開いている画像」と
        // 突き合わなくなる (ドラッグ前の書き戻しが飛ばされ、注釈前のファイルが渡る)
        let dir = dunce::canonicalize(crate::prepare_save_directory(&app)?)
            .map_err(|e| format!("Failed to resolve save directory: {}", e))?;
        list_images_in(&dir, LIST_LIMIT)
    })
    .await
    .map_err(|e| format!("Failed to list saved images: {}", e))?
}

/// 保存先フォルダの直下にある画像のサムネイルを data URL で返す
#[tauri::command]
pub(crate) async fn saved_image_thumbnail(
    app: tauri::AppHandle,
    path: String,
) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let save_dir = dunce::canonicalize(crate::prepare_save_directory(&app)?)
            .map_err(|e| format!("Failed to resolve save directory: {}", e))?;
        let target = resolve_within(&save_dir, &path)?;
        make_thumbnail(&target)
    })
    .await
    .map_err(|e| format!("Failed to make a thumbnail: {}", e))?
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(tag: &str) -> Self {
            let dir = std::env::temp_dir().join(format!(
                "flashcap-thumbs-{}-{}-{}",
                tag,
                std::process::id(),
                to_millis(SystemTime::now())
            ));
            std::fs::create_dir_all(&dir).unwrap();
            TempDir(dunce::canonicalize(&dir).unwrap())
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn write_png(path: &Path, width: u32, height: u32) {
        image::RgbaImage::from_pixel(width, height, image::Rgba([255, 0, 0, 255]))
            .save(path)
            .unwrap();
    }

    fn set_mtime(path: &Path, secs: u64) {
        let file = std::fs::File::options().write(true).open(path).unwrap();
        file.set_modified(UNIX_EPOCH + std::time::Duration::from_secs(secs))
            .unwrap();
    }

    #[test]
    fn lists_only_images_directly_inside_newest_first() {
        let dir = TempDir::new("list");
        write_png(&dir.0.join("a.png"), 2, 2);
        write_png(&dir.0.join("b.PNG"), 2, 2);
        std::fs::write(dir.0.join("notes.txt"), "x").unwrap();
        std::fs::create_dir(dir.0.join("sub.png")).unwrap();
        write_png(&dir.0.join("sub.png").join("nested.png"), 2, 2);
        std::os::unix::fs::symlink(dir.0.join("a.png"), dir.0.join("link.png")).unwrap();

        let images = list_images_in(&dir.0, 10).unwrap();
        let names: Vec<_> = images.iter().map(|i| i.name.as_str()).collect();
        assert_eq!(names.len(), 2, "{:?}", names);
        assert!(names.contains(&"a.png") && names.contains(&"b.PNG"));
        assert!(images.iter().all(|i| i.size > 0));
        // 新しい順
        assert!(images[0].created_ms >= images[1].created_ms);
    }

    #[test]
    fn falls_back_to_name_order_for_the_same_timestamp_and_respects_the_limit() {
        // 作成日時 (birth time) を取れない環境では更新日時で並ぶ。どちらでも
        // 同じ時刻のものは名前の逆順になることだけを確かめる
        let dir = TempDir::new("order");
        for name in ["flashcap-1.png", "flashcap-2.png", "flashcap-3.png"] {
            let path = dir.0.join(name);
            write_png(&path, 1, 1);
            set_mtime(&path, 1_000_000);
        }
        let images = list_images_in(&dir.0, 2).unwrap();
        assert_eq!(images.len(), 2);
        if images[0].created_ms == images[1].created_ms {
            assert_eq!(images[0].name, "flashcap-3.png");
            assert_eq!(images[1].name, "flashcap-2.png");
        }
    }

    #[test]
    fn thumbnails_are_limited_to_regular_files_directly_inside_the_save_dir() {
        let dir = TempDir::new("resolve");
        let outside = TempDir::new("outside");
        write_png(&dir.0.join("in.png"), 2, 2);
        write_png(&outside.0.join("secret.png"), 2, 2);
        std::fs::create_dir(dir.0.join("sub")).unwrap();
        write_png(&dir.0.join("sub").join("nested.png"), 2, 2);
        std::os::unix::fs::symlink(outside.0.join("secret.png"), dir.0.join("link.png")).unwrap();

        let ok = resolve_within(&dir.0, &dir.0.join("in.png").to_string_lossy()).unwrap();
        assert_eq!(ok, dir.0.join("in.png"));

        for bad in [
            outside.0.join("secret.png"),
            dir.0.join("link.png"),
            dir.0.join("sub").join("nested.png"),
            dir.0
                .join("sub")
                .join("..")
                .join("..")
                .join(outside.0.file_name().unwrap())
                .join("secret.png"),
            dir.0.join("missing.png"),
            dir.0.join("sub"),
        ] {
            assert!(
                resolve_within(&dir.0, &bad.to_string_lossy()).is_err(),
                "{} should be refused",
                bad.display()
            );
        }
    }

    #[test]
    fn thumbnails_shrink_large_images_and_keep_small_ones() {
        let dir = TempDir::new("thumb");
        let big = dir.0.join("big.png");
        let small = dir.0.join("small.png");
        write_png(&big, 1600, 800);
        write_png(&small, 40, 30);

        let decode = |url: String| {
            let b64 = url.strip_prefix("data:image/png;base64,").unwrap();
            image::load_from_memory(&STANDARD.decode(b64).unwrap()).unwrap()
        };
        let t = decode(make_thumbnail(&big).unwrap());
        assert_eq!((t.width(), t.height()), (400, 200));
        let t = decode(make_thumbnail(&small).unwrap());
        assert_eq!((t.width(), t.height()), (40, 30));
    }
}
