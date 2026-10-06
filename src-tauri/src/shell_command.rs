//! 画像に対してユーザー定義のシェルコマンドを実行する (macOS のみ)
//!
//! コマンドは設定ファイルの `shell_commands` (`[{id, name, shell, command}]`) に
//! Preferences から登録する。フロントは id と画像のパスだけを渡し、実行する中身は
//! ここで設定ファイルから引き直す。
//!
//! **画像のパスは文字列置換せず、環境変数 `IMAGE_PATH` で渡す。** コマンド中の
//! `${IMAGE_PATH}` はシェル自身が展開する。置換で埋め込むと、Finder から開いた
//! `a;rm -rf ~.png` のようなファイル名がそのままコマンドとして実行される。
//!
//! シェルはログインシェル (`-l -c`) で起動する。GUI アプリから起動したプロセスの
//! PATH は /usr/bin:/bin:/usr/sbin:/sbin だけで、Homebrew のコマンドが見つからないため
//! (~/.zprofile などの PATH 設定をここで読ませる)。
//!
//! 出力 (stdout + stderr) は `$TMPDIR/flashcap/shell-logs/` に 1 実行 1 ファイルで残す。

#[cfg(unix)]
use std::path::Path;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize)]
#[cfg_attr(not(unix), allow(dead_code))]
pub(crate) struct ShellCommandConfig {
    pub id: String,
    #[serde(default)]
    pub name: String,
    pub shell: String,
    pub command: String,
}

#[derive(Debug, Serialize)]
#[cfg_attr(not(unix), allow(dead_code))]
pub(crate) struct ShellRunResult {
    pub success: bool,
    /// シグナルで終了した・起動できなかった時は None
    pub exit_code: Option<i32>,
    pub log_path: String,
}

const CONFIG_KEY: &str = "shell_commands";

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn find_command(id: &str) -> Result<ShellCommandConfig, String> {
    let list = match crate::config::get(CONFIG_KEY) {
        Some(serde_json::Value::Array(list)) => list,
        _ => vec![],
    };
    // 形の合わない要素は飛ばす (フロントの parseShellCommands と同じ扱い)。
    // 1 件の書き損じで、メニューに出ている他のコマンドまで実行できなくしない
    list.into_iter()
        .filter_map(|item| serde_json::from_value::<ShellCommandConfig>(item).ok())
        .find(|c| c.id == id)
        .ok_or_else(|| "The shell command was not found. It may have been removed in Preferences.".to_string())
}

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn log_dir() -> Result<PathBuf, String> {
    let dir = crate::ensure_private_flashcap_dir()
        .map_err(|e| format!("Failed to prepare the temporary directory: {}", e))?
        .join("shell-logs");
    crate::create_private_dir(&dir)
        .map_err(|e| format!("Failed to create {}: {}", dir.display(), e))?;
    Ok(dir)
}

/// ログのファイル名に使えるよう、名前を英数字 (日本語も含む) と `-` `_` に絞る
#[cfg_attr(not(unix), allow(dead_code))]
fn log_file_stem(name: &str) -> String {
    let slug: String = name
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '-' })
        .take(40)
        .collect();
    let slug = slug.trim_matches('-');
    if slug.is_empty() { "command".to_string() } else { slug.to_string() }
}

#[cfg(unix)]
fn create_log_file(dir: &Path, name: &str) -> Result<(PathBuf, std::fs::File), String> {
    use std::os::unix::fs::OpenOptionsExt;
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S%.3f");
    let stem = log_file_stem(name);
    for attempt in 0..100 {
        let file_name = if attempt == 0 {
            format!("{}-{}.log", stamp, stem)
        } else {
            format!("{}-{}-{}.log", stamp, stem, attempt)
        };
        let path = dir.join(file_name);
        // create_new は既存のファイルにも symlink にも書かない
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&path)
        {
            Ok(file) => return Ok((path, file)),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(format!("Failed to create {}: {}", path.display(), e)),
        }
    }
    Err("Failed to create a log file".to_string())
}

/// コマンドを実行し、終わるまで待つ
///
/// シェルを起動できなかった場合もログにその旨を書いて失敗として返す
/// (フロントは成否とログの場所だけを見て、スナックバーからログを開ける)
#[cfg(unix)]
pub(crate) async fn run(
    command: &ShellCommandConfig,
    image_path: &str,
    log_dir: &Path,
    cwd: &Path,
) -> Result<ShellRunResult, String> {
    use std::io::Write;

    let (log_path, mut log) = create_log_file(log_dir, &command.name)?;
    let log_path_string = log_path.to_string_lossy().to_string();
    let started = std::time::Instant::now();

    let header = format!(
        "# name:       {}\n# shell:      {} -l -c\n# started:    {}\n# IMAGE_PATH: {}\n# command:\n{}\n# ---- output ----\n",
        command.name,
        command.shell,
        chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
        image_path,
        command.command,
    );
    log.write_all(header.as_bytes())
        .map_err(|e| format!("Failed to write {}: {}", log_path.display(), e))?;

    let fail = |mut log: std::fs::File, message: String| -> Result<ShellRunResult, String> {
        let _ = writeln!(log, "# ---- failed to start: {} ----", message);
        Ok(ShellRunResult { success: false, exit_code: None, log_path: log_path_string.clone() })
    };

    if !Path::new(&command.shell).is_absolute() {
        return fail(log, format!("the shell must be an absolute path (got `{}`)", command.shell));
    }
    let (stdout, stderr) = match (log.try_clone(), log.try_clone()) {
        (Ok(out), Ok(err)) => (out, err),
        (Err(e), _) | (_, Err(e)) => return fail(log, e.to_string()),
    };

    let child = tokio::process::Command::new(&command.shell)
        .arg("-l")
        .arg("-c")
        .arg(&command.command)
        .env("IMAGE_PATH", image_path)
        .current_dir(cwd)
        .stdin(std::process::Stdio::null())
        .stdout(stdout)
        .stderr(stderr)
        .spawn();
    let mut child = match child {
        Ok(child) => child,
        Err(e) => return fail(log, format!("{}: {}", command.shell, e)),
    };
    let status = match child.wait().await {
        Ok(status) => status,
        Err(e) => return fail(log, e.to_string()),
    };

    let elapsed = started.elapsed().as_secs_f64();
    let _ = writeln!(log, "\n# ---- {} ({:.2}s) ----", status, elapsed);
    Ok(ShellRunResult {
        success: status.success(),
        exit_code: status.code(),
        log_path: log_path_string,
    })
}

#[cfg(target_os = "macos")]
#[tauri::command]
pub(crate) async fn run_shell_command(id: String, image_path: String) -> Result<ShellRunResult, String> {
    let command = find_command(&id)?;
    let log_dir = log_dir()?;
    let cwd = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/"));
    run(&command, &image_path, &log_dir, &cwd).await
}

#[cfg(not(target_os = "macos"))]
#[tauri::command]
pub(crate) async fn run_shell_command(_id: String, _image_path: String) -> Result<ShellRunResult, String> {
    Err("Shell commands are only supported on macOS".to_string())
}

/// ログを既定のアプリで開く。開けるのはログの置き場の中のファイルだけ
#[tauri::command]
pub(crate) fn open_shell_log(app: tauri::AppHandle, path: String) -> Result<(), String> {
    let dir = log_dir()?;
    let dir = dunce::canonicalize(&dir).map_err(|e| e.to_string())?;
    let file = dunce::canonicalize(&path).map_err(|e| format!("{}: {}", path, e))?;
    if file.parent() != Some(dir.as_path()) {
        return Err(format!("{} is not a shell command log", path));
    }
    crate::open_directory(&app, &file.to_string_lossy())
}

#[tauri::command]
pub(crate) fn open_shell_log_dir(app: tauri::AppHandle) -> Result<(), String> {
    let dir = log_dir()?;
    crate::open_directory(&app, &dir.to_string_lossy())
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "flashcap-shell-test-{}-{}",
            name,
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn command(shell: &str, script: &str) -> ShellCommandConfig {
        ShellCommandConfig {
            id: "id".to_string(),
            name: "Upload to S3".to_string(),
            shell: shell.to_string(),
            command: script.to_string(),
        }
    }

    fn block_on<F: std::future::Future>(future: F) -> F::Output {
        tauri::async_runtime::block_on(future)
    }

    #[test]
    fn image_path_reaches_the_command_and_output_is_logged() {
        // Arrange
        let dir = temp_dir("logged");
        let cmd = command("/bin/sh", "echo \"got ${IMAGE_PATH}\"; echo oops >&2");

        // Act
        let result = block_on(run(&cmd, "/tmp/shot 1.png", &dir, &dir)).unwrap();

        // Assert
        assert!(result.success);
        assert_eq!(result.exit_code, Some(0));
        let log = std::fs::read_to_string(&result.log_path).unwrap();
        assert!(log.contains("got /tmp/shot 1.png\n"), "{}", log);
        assert!(log.contains("oops\n"), "{}", log);
        assert!(log.contains("# name:       Upload to S3"), "{}", log);
        assert!(Path::new(&result.log_path).starts_with(&dir));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn shell_syntax_in_the_image_path_is_not_executed() {
        // Arrange
        let dir = temp_dir("injection");
        let marker = dir.join("pwned");
        let evil = format!("a;touch {};$(touch {}).png", marker.display(), marker.display());
        let cmd = command("/bin/sh", "printf '%s\\n' \"${IMAGE_PATH}\"");

        // Act
        let result = block_on(run(&cmd, &evil, &dir, &dir)).unwrap();

        // Assert
        assert!(result.success);
        assert!(!marker.exists());
        let log = std::fs::read_to_string(&result.log_path).unwrap();
        assert!(log.contains(&format!("\n{}\n", evil)), "{}", log);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn non_zero_exit_is_a_failure_with_its_code() {
        // Arrange
        let dir = temp_dir("exit");
        let cmd = command("/bin/sh", "exit 3");

        // Act
        let result = block_on(run(&cmd, "/x.png", &dir, &dir)).unwrap();

        // Assert
        assert!(!result.success);
        assert_eq!(result.exit_code, Some(3));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_shell_fails_and_says_so_in_the_log() {
        // Arrange
        let dir = temp_dir("missing-shell");
        let cmd = command("/no/such/shell", "true");

        // Act
        let result = block_on(run(&cmd, "/x.png", &dir, &dir)).unwrap();

        // Assert
        assert!(!result.success);
        assert_eq!(result.exit_code, None);
        let log = std::fs::read_to_string(&result.log_path).unwrap();
        assert!(log.contains("failed to start: /no/such/shell"), "{}", log);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn relative_shell_is_rejected() {
        // Arrange
        let dir = temp_dir("relative-shell");
        let cmd = command("sh", "true");

        // Act
        let result = block_on(run(&cmd, "/x.png", &dir, &dir)).unwrap();

        // Assert
        assert!(!result.success);
        let log = std::fs::read_to_string(&result.log_path).unwrap();
        assert!(log.contains("must be an absolute path"), "{}", log);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn runs_in_the_given_working_directory() {
        // Arrange
        let dir = temp_dir("cwd");
        let work = dir.join("work");
        std::fs::create_dir_all(&work).unwrap();
        let cmd = command("/bin/sh", "touch here");

        // Act
        let result = block_on(run(&cmd, "/x.png", &dir, &work)).unwrap();

        // Assert
        assert!(result.success);
        assert!(work.join("here").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn log_file_names_are_safe_and_unique() {
        // Arrange
        let dir = temp_dir("names");

        // Act
        let (first, _) = create_log_file(&dir, "../../etc/スクショ up").unwrap();
        let (second, _) = create_log_file(&dir, "../../etc/スクショ up").unwrap();

        // Assert
        assert_ne!(first, second);
        assert_eq!(first.parent(), Some(dir.as_path()));
        let name = first.file_name().unwrap().to_string_lossy().to_string();
        assert!(name.ends_with("-etc-スクショ-up.log"), "{}", name);
        assert_eq!(log_file_stem("///"), "command");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
