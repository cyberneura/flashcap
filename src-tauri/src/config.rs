//! 設定ファイル `~/.config/flashcap/config.json`
//!
//! 中身はキーと値のフラットな JSON オブジェクト。フロントは `config_get_all` /
//! `config_set` で読み書きし (`src/lib/config.ts`)、Rust の読み手は撮影のたびに
//! `get` でファイルから読み直す。キャッシュしないのは、手で書き換えた内容や、
//! 別のマシンから同期されてきた内容をそのまま拾うため。
//!
//! **ホームフォルダの実パスを値に書かない。** マシン間で共有する前提なので、
//! パスはフロントが `~` に畳んでから保存し、使う側 (`expand_home`) で展開する。

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde_json::{Map, Value};
use tauri::Emitter;

/// 書き込みの直列化。読む → 1 キー差し替える → 書く、の間に別の書き込みが
/// 割り込むと、片方の変更が消える (Preferences とメインウインドウが同時に書く)
static WRITE_LOCK: Mutex<()> = Mutex::new(());

pub(crate) fn config_path() -> Option<PathBuf> {
    dirs::home_dir().map(|home| home.join(".config").join("flashcap").join("config.json"))
}

fn read_from(path: &Path) -> Result<Map<String, Value>, String> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Map::new()),
        Err(e) => return Err(format!("Failed to read {}: {}", path.display(), e)),
    };
    if text.trim().is_empty() {
        return Ok(Map::new());
    }
    match serde_json::from_str::<Value>(&text) {
        Ok(Value::Object(map)) => Ok(map),
        Ok(_) => Err(format!("{} is not a JSON object", path.display())),
        Err(e) => Err(format!("{} is not valid JSON: {}", path.display(), e)),
    }
}

/// 1 キーを書き換えて保存する
///
/// **読めないファイルには書かない。** 手で編集して JSON を壊した状態で上書きすると、
/// 他の設定がすべて消える。
fn set_in(path: &Path, key: &str, value: Value) -> Result<(), String> {
    let _guard = WRITE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut map = read_from(path)?;
    map.insert(key.to_string(), value);
    write_atomically(path, &map)
}

/// 一時ファイルに書いてから rename する (書きかけのファイルを読ませない)
///
/// config.json が dotfiles の管理下への symlink であっても、rename で symlink 自体を
/// 普通のファイルに置き換えないよう、リンク先の実体の隣で差し替える。
/// リンク先がまだ無い (dotfiles 側にファイルを作っていない) 時もリンク先に作る。
fn write_atomically(path: &Path, map: &Map<String, Value>) -> Result<(), String> {
    let target = resolve_write_target(path)?;
    let dir = target.parent().ok_or("The config path has no parent directory")?;
    let tmp = dir.join(format!(".config.json.{}.tmp", std::process::id()));

    let mut text = serde_json::to_string_pretty(map).map_err(|e| e.to_string())?;
    text.push('\n');

    let result = (|| -> std::io::Result<()> {
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create(true).truncate(true);
        // シェルコマンドを持つファイルなので、他のアカウントから読ませない
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&tmp)?;
        file.write_all(text.as_bytes())?;
        file.sync_all()?;
        std::fs::rename(&tmp, &target)
    })();
    if let Err(e) = result {
        let _ = std::fs::remove_file(&tmp);
        return Err(format!("Failed to write {}: {}", target.display(), e));
    }
    Ok(())
}

fn resolve_write_target(path: &Path) -> Result<PathBuf, String> {
    if let Ok(real) = std::fs::canonicalize(path) {
        return Ok(real);
    }
    let target = match std::fs::read_link(path) {
        Ok(link) if link.is_absolute() => link,
        Ok(link) => path.parent().map(|p| p.join(&link)).unwrap_or(link),
        Err(_) => path.to_path_buf(),
    };
    let dir = target.parent().ok_or("The config path has no parent directory")?;
    std::fs::create_dir_all(dir).map_err(|e| format!("Failed to create {}: {}", dir.display(), e))?;
    Ok(target)
}

/// 設定値を 1 つ読む。ファイルが無い・壊れている時は None (各読み手の既定値に倒れる)
pub(crate) fn get(key: &str) -> Option<Value> {
    let path = config_path()?;
    match read_from(&path) {
        Ok(mut map) => map.remove(key),
        Err(e) => {
            eprintln!("[config] {}", e);
            None
        }
    }
}

/// 先頭の `~` をホームフォルダに展開する (`~/foo`、Windows の `~\foo`、`~` 単体)
pub(crate) fn expand_home(path: &str) -> PathBuf {
    expand_home_with(path, dirs::home_dir().as_deref())
}

fn expand_home_with(path: &str, home: Option<&Path>) -> PathBuf {
    let Some(home) = home else {
        return PathBuf::from(path);
    };
    if path == "~" {
        return home.to_path_buf();
    }
    match path.strip_prefix("~/").or_else(|| path.strip_prefix("~\\")) {
        Some(rest) => home.join(rest),
        None => PathBuf::from(path),
    }
}

#[tauri::command]
pub(crate) fn config_get_all() -> Result<Map<String, Value>, String> {
    let path = config_path().ok_or("Could not find the home directory")?;
    read_from(&path)
}

/// 書いた後、全ウインドウに `config-changed` (payload はキー) を送る。
/// Preferences での変更をメインウインドウに即時反映するため
#[tauri::command]
pub(crate) fn config_set(app: tauri::AppHandle, key: String, value: Value) -> Result<(), String> {
    let path = config_path().ok_or("Could not find the home directory")?;
    set_in(&path, &key, value)?;
    let _ = app.emit("config-changed", key);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "flashcap-config-test-{}-{}",
            name,
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn set_creates_the_directory_and_keeps_other_keys() {
        // Arrange
        let dir = temp_dir("keeps-keys");
        let path = dir.join("flashcap").join("config.json");

        // Act
        set_in(&path, "timer_delay", json!(10)).unwrap();
        set_in(&path, "exclude_shadow", json!(false)).unwrap();
        set_in(&path, "timer_delay", json!(3)).unwrap();

        // Assert
        let map = read_from(&path).unwrap();
        assert_eq!(map.get("timer_delay"), Some(&json!(3)));
        assert_eq!(map.get("exclude_shadow"), Some(&json!(false)));
        assert_eq!(map.len(), 2);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_or_empty_file_reads_as_empty() {
        // Arrange
        let dir = temp_dir("empty");
        std::fs::create_dir_all(&dir).unwrap();
        let empty = dir.join("empty.json");
        std::fs::write(&empty, "  \n").unwrap();

        // Act / Assert
        assert!(read_from(&dir.join("missing.json")).unwrap().is_empty());
        assert!(read_from(&empty).unwrap().is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn set_refuses_to_overwrite_a_broken_file() {
        // Arrange
        let dir = temp_dir("broken");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.json");
        let broken = "{\"timer_delay\": 10,";
        std::fs::write(&path, broken).unwrap();

        // Act
        let result = set_in(&path, "exclude_shadow", json!(true));

        // Assert
        assert!(result.is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), broken);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn non_object_json_is_rejected() {
        // Arrange
        let dir = temp_dir("array");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.json");
        std::fs::write(&path, "[1, 2]").unwrap();

        // Act / Assert
        assert!(read_from(&path).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn writing_through_a_symlink_updates_the_target_and_keeps_the_link() {
        // Arrange
        let dir = temp_dir("symlink");
        let dotfiles = dir.join("dotfiles");
        std::fs::create_dir_all(&dotfiles).unwrap();
        let real = dotfiles.join("config.json");
        std::fs::write(&real, "{\"timer_delay\": 10}").unwrap();
        let link_dir = dir.join("home").join(".config").join("flashcap");
        std::fs::create_dir_all(&link_dir).unwrap();
        let link = link_dir.join("config.json");
        std::os::unix::fs::symlink(&real, &link).unwrap();

        // Act
        set_in(&link, "exclude_shadow", json!(false)).unwrap();

        // Assert
        assert!(std::fs::symlink_metadata(&link).unwrap().file_type().is_symlink());
        let map = read_from(&real).unwrap();
        assert_eq!(map.get("timer_delay"), Some(&json!(10)));
        assert_eq!(map.get("exclude_shadow"), Some(&json!(false)));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn writing_through_a_dangling_symlink_creates_the_target() {
        // Arrange
        let dir = temp_dir("dangling");
        let real = dir.join("dotfiles").join("flashcap").join("config.json");
        let link_dir = dir.join("home").join(".config").join("flashcap");
        std::fs::create_dir_all(&link_dir).unwrap();
        let link = link_dir.join("config.json");
        std::os::unix::fs::symlink("../../../dotfiles/flashcap/config.json", &link).unwrap();

        // Act
        set_in(&link, "timer_delay", json!(3)).unwrap();

        // Assert
        assert!(std::fs::symlink_metadata(&link).unwrap().file_type().is_symlink());
        assert_eq!(read_from(&real).unwrap().get("timer_delay"), Some(&json!(3)));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn written_file_is_private() {
        use std::os::unix::fs::PermissionsExt;
        // Arrange
        let dir = temp_dir("mode");
        let path = dir.join("config.json");

        // Act
        set_in(&path, "timer_delay", json!(5)).unwrap();

        // Assert
        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn expand_home_only_touches_a_leading_tilde() {
        // Arrange
        let home = Path::new("/Users/someone");

        // Act / Assert
        assert_eq!(expand_home_with("~", Some(home)), PathBuf::from("/Users/someone"));
        assert_eq!(
            expand_home_with("~/Pictures/shots", Some(home)),
            PathBuf::from("/Users/someone/Pictures/shots")
        );
        assert_eq!(
            expand_home_with("/Volumes/~/x", Some(home)),
            PathBuf::from("/Volumes/~/x")
        );
        assert_eq!(expand_home_with("~other/x", Some(home)), PathBuf::from("~other/x"));
        assert_eq!(expand_home_with("~/x", None), PathBuf::from("~/x"));
    }
}
