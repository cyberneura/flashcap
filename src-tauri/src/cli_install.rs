//! PATH に置く `flashcap` コマンド (macOS のみ)。
//!
//! アプリメニューの Install Command Line Tools… が、`.app` の中の実行ファイルへの symlink を
//! [`CLI_LINK_PATH`] に置く。その symlink から起動された時は、GUI をターミナルで動かさず
//! LaunchServices に起動させ直す ([`launch_app`])。
#![cfg_attr(not(target_os = "macos"), allow(dead_code))]

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

pub const CLI_LINK_PATH: &str = "/usr/local/bin/flashcap";

/// [`CLI_LINK_PATH`] のような symlink 経由 = コマンドラインから起動された。
/// `.app` の実行ファイルをそのまま動かすと GUI がターミナルにぶら下がる (閉じると落ちる) ので、
/// その時は LaunchServices に起動させる
pub fn invoked_via_symlink() -> bool {
    std::env::current_exe()
        .and_then(|path| path.symlink_metadata())
        .map(|metadata| metadata.file_type().is_symlink())
        .unwrap_or(false)
}

/// LaunchServices への渡し方
#[derive(Debug, PartialEq)]
enum Launch {
    /// 引数なし。起動済みなら前面に出すだけ
    Plain,
    /// 引数が全部実在するファイル。`open -a` で渡すと、起動中でもコールド起動でも
    /// `RunEvent::Opened` で届く (Finder の「このアプリケーションで開く」と同じ経路)
    Files(Vec<String>),
    /// `--capture` などを含む。`open -n --args` で新しいプロセスに渡し、
    /// 起動中なら single-instance がそのインスタンスへ転送する
    Args(Vec<String>),
}

/// 起動し直したプロセスの cwd は `/` になるので、実在するパスはここで絶対パスにする。
/// `-` で始まる引数はオプションとして扱い、パスとして見ない (作業フォルダに `--capture` という
/// ファイルがあっても撮影の指示のまま渡す。`-` で始まる画像は `./-a.png` と書けば開ける)
fn plan_launch(args: &[String], cwd: Option<&Path>, exists: impl Fn(&Path) -> bool) -> Launch {
    if args.is_empty() {
        return Launch::Plain;
    }
    let absolute = |arg: &String| -> Option<String> {
        if arg.starts_with('-') {
            return None;
        }
        let path = Path::new(arg);
        let full = match cwd {
            Some(cwd) if path.is_relative() => cwd.join(path),
            _ => path.to_path_buf(),
        };
        exists(&full).then(|| full.to_string_lossy().into_owned())
    };
    let files: Vec<Option<String>> = args.iter().map(absolute).collect();
    if files.iter().all(Option::is_some) {
        return Launch::Files(files.into_iter().flatten().collect());
    }
    Launch::Args(
        args.iter()
            .zip(files)
            .map(|(arg, file)| file.unwrap_or_else(|| arg.clone()))
            .collect(),
    )
}

/// 終了コードを返す
pub fn run_client() -> i32 {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let cwd = std::env::current_dir().ok();
    let launch = plan_launch(&args, cwd.as_deref(), Path::exists);
    match launch_app(launch) {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("flashcap: {e}");
            1
        }
    }
}

fn launch_app(launch: Launch) -> Result<(), String> {
    let executable = std::env::current_exe()
        .and_then(|path| path.canonicalize())
        .map_err(|e| format!("Failed to locate the application: {e}"))?;

    let Some(bundle) = app_bundle(&executable) else {
        // 開発中のビルド (.app の外)。起動済みなら single-instance が引数を渡して終わる
        let args = match launch {
            Launch::Plain => Vec::new(),
            Launch::Files(args) | Launch::Args(args) => args,
        };
        Command::new(&executable)
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("Failed to start {}: {e}", executable.display()))?;
        return Ok(());
    };

    let mut command = Command::new("/usr/bin/open");
    match launch {
        Launch::Plain => {
            command.arg(&bundle);
        }
        Launch::Files(files) => {
            command.arg("-a").arg(&bundle).args(files);
        }
        // 起動済みのアプリに open は引数を渡さず前面に出すだけなので、-n で新しいプロセスにする
        Launch::Args(args) => {
            command.arg("-n").arg(&bundle).arg("--args").args(args);
        }
    }
    let status = command
        .status()
        .map_err(|e| format!("Failed to run open: {e}"))?;
    if !status.success() {
        return Err(format!("open exited with {status}"));
    }
    Ok(())
}

/// `<名前>.app/Contents/MacOS/<実行ファイル>` の `<名前>.app`
fn app_bundle(executable: &Path) -> Option<PathBuf> {
    let macos = executable.parent()?;
    let contents = macos.parent()?;
    let bundle = contents.parent()?;
    let is_bundle = macos.file_name()? == "MacOS"
        && contents.file_name()? == "Contents"
        && bundle.extension()? == "app";
    is_bundle.then(|| bundle.to_path_buf())
}

/// アプリメニューの Install Command Line Tools…。/usr/local/bin は root の持ち物のことが
/// 多いので、書けなければ管理者のパスワードを尋ねて作る
pub fn install() -> Result<String, String> {
    let executable = std::env::current_exe()
        .and_then(|path| path.canonicalize())
        .map_err(|e| format!("Failed to locate the application: {e}"))?;
    check_install_source(&executable)?;
    let link = Path::new(CLI_LINK_PATH);
    // 別のものを上書きしない (symlink なら、以前に作ったものとして張り替える)
    if let Ok(metadata) = link.symlink_metadata() {
        if !metadata.file_type().is_symlink() {
            return Err(format!(
                "{CLI_LINK_PATH} already exists and is not a symlink."
            ));
        }
    }

    let script = install_script(&executable, link);
    let as_user = Command::new("/bin/sh")
        .arg("-c")
        .arg(&script)
        .stderr(Stdio::null())
        .status()
        .map_err(|e| format!("Failed to run sh: {e}"))?;
    // 上の確認の後に別のものに置き換わった。パスワードを訊いても結果は変わらない
    if as_user.code() == Some(EXISTS_EXIT_CODE) {
        return Err(format!(
            "{CLI_LINK_PATH} already exists and is not a symlink."
        ));
    }
    if !as_user.success() {
        let output = Command::new("/usr/bin/osascript")
            .arg("-e")
            .arg(format!(
                "do shell script \"{}\" with administrator privileges",
                applescript_escape(&script)
            ))
            .output()
            .map_err(|e| format!("Failed to run osascript: {e}"))?;
        if !output.status.success() {
            return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
        }
    }
    Ok(format!(
        "{CLI_LINK_PATH} -> {}\n\nRun `flashcap <image>` to open an image, or `flashcap --capture` to start a capture.",
        executable.display()
    ))
}

/// 隔離属性が付いたまま起動されたアプリ (App Translocation。ダウンロードした場所や dmg から
/// 直接開いたもの) は、起動のたびにランダムな読み取り専用の場所から動くので、リンクが次回切れる
fn check_install_source(executable: &Path) -> Result<(), String> {
    if executable.to_string_lossy().contains("/AppTranslocation/") {
        return Err(
            "Move Flashcap to the Applications folder, open it from there, and try again."
                .to_string(),
        );
    }
    Ok(())
}

const EXISTS_EXIT_CODE: i32 = 3;

/// symlink でないものが無いことをスクリプトの中でも確かめる。`install` の確認はスクリプトより前
/// (パスワードの入力を挟むとずっと前) なので、その間にできたファイルを root の `ln -sf` が
/// 消してしまう (ディレクトリならその中にリンクを作る)。空いているパスには `-f` 無しで張るので、
/// 確認の後に何かが現れても消さずに `ln` が失敗する。`-f` で置き換えるのは既存の symlink だけ。
/// その確認と置き換えの間 (マイクロ秒) にファイルへすり替えられるのは root だけで
/// (スクリプトを root で動かすのは /usr/local/bin にユーザーが書けない時だけ)、POSIX には
/// 「symlink の時だけ置き換える」操作が無いのでこの隙間は閉じられない
fn install_script(executable: &Path, link: &Path) -> String {
    let link = link.to_string_lossy();
    let directory = Path::new(link.as_ref())
        .parent()
        .unwrap()
        .to_string_lossy()
        .to_string();
    format!(
        "if [ -L {link_q} ]; then ln -sfn {target_q} {link_q}; elif [ -e {link_q} ]; then echo {message} >&2; exit {EXISTS_EXIT_CODE}; else mkdir -p {dir_q} && ln -s {target_q} {link_q}; fi",
        dir_q = sh_quote(&directory),
        target_q = sh_quote(&executable.to_string_lossy()),
        link_q = sh_quote(&link),
        message = sh_quote(&format!("{link} already exists and is not a symlink.")),
    )
}

fn sh_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn applescript_escape(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

// macOS 専用のコード。テストは Unix のパス・symlink・/bin/sh を使うので Windows では走らせない
#[cfg(all(test, unix))]
mod tests {
    use super::*;

    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|v| v.to_string()).collect()
    }

    // a.png と /tmp/b.png と、作業フォルダの --capture という名前のファイルが実在する
    fn exists(path: &Path) -> bool {
        path == Path::new("/work/a.png")
            || path == Path::new("/tmp/b.png")
            || path == Path::new("/work/--capture")
    }

    #[test]
    fn plan_launch_opens_existing_files_through_launch_services() {
        // Arrange
        let cwd = Path::new("/work");

        // Act
        let launch = plan_launch(&args(&["a.png", "/tmp/b.png"]), Some(cwd), exists);

        // Assert: 相対パスは呼び出し元の cwd で絶対パスになる
        assert_eq!(launch, Launch::Files(args(&["/work/a.png", "/tmp/b.png"])));
    }

    #[test]
    fn plan_launch_passes_options_as_arguments_with_absolute_paths() {
        // Arrange
        let cwd = Path::new("/work");

        // Act
        let launch = plan_launch(&args(&["--capture", "a.png"]), Some(cwd), exists);

        // Assert: 同じ名前のファイルがあっても --capture はオプションのまま
        assert_eq!(launch, Launch::Args(args(&["--capture", "/work/a.png"])));
        assert_eq!(
            plan_launch(&args(&["--capture"]), Some(cwd), exists),
            Launch::Args(args(&["--capture"]))
        );
    }

    #[test]
    fn plan_launch_without_arguments_just_launches() {
        assert_eq!(
            plan_launch(&[], Some(Path::new("/work")), exists),
            Launch::Plain
        );
        // 実在しないファイルは --args で渡す (起動後に collect_image_args が落とす)
        assert_eq!(
            plan_launch(&args(&["missing.png"]), Some(Path::new("/work")), exists),
            Launch::Args(args(&["missing.png"]))
        );
    }

    #[test]
    fn app_bundle_finds_the_enclosing_app() {
        assert_eq!(
            app_bundle(Path::new(
                "/Applications/Flashcap.app/Contents/MacOS/flashcap"
            )),
            Some(PathBuf::from("/Applications/Flashcap.app"))
        );
        assert_eq!(
            app_bundle(Path::new("/repo/src-tauri/target/debug/flashcap")),
            None
        );
    }

    #[test]
    fn check_install_source_rejects_a_translocated_app() {
        let translocated = Path::new(
            "/private/var/folders/x/AppTranslocation/1234/d/Flashcap.app/Contents/MacOS/flashcap",
        );
        let installed = Path::new("/Applications/Flashcap.app/Contents/MacOS/flashcap");

        assert!(check_install_source(translocated).is_err());
        assert!(check_install_source(installed).is_ok());
    }

    #[test]
    fn install_script_quotes_paths_for_applescript_and_sh() {
        // Arrange: パスに空白と引用符がある
        let executable =
            Path::new("/Applications/My \"Cap\" Tools's/Flashcap.app/Contents/MacOS/flashcap");

        // Act
        let script = install_script(executable, Path::new(CLI_LINK_PATH));
        let escaped = applescript_escape(&script);

        // Assert: AppleScript の文字列の中で、" と \ がそのまま終端や逃がしとして読まれない
        assert!(script.ends_with(
            "else mkdir -p '/usr/local/bin' && ln -s '/Applications/My \"Cap\" Tools'\\''s/Flashcap.app/Contents/MacOS/flashcap' '/usr/local/bin/flashcap'; fi"
        ));
        assert!(!escaped
            .replace("\\\\", "")
            .replace("\\\"", "")
            .contains(['"', '\\']));
    }

    fn run_install_script(link: &Path) -> std::process::ExitStatus {
        Command::new("/bin/sh")
            .arg("-c")
            .arg(install_script(Path::new("/target/flashcap"), link))
            .stderr(Stdio::null())
            .status()
            .unwrap()
    }

    /// テストごとの空のディレクトリ (dev-dependency を足さないため tempfile は使わない)
    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "flashcap-cli-install-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn read_link(path: &Path) -> String {
        std::fs::read_link(path)
            .unwrap()
            .to_string_lossy()
            .into_owned()
    }

    #[test]
    fn install_script_creates_or_replaces_a_symlink() {
        // Arrange: 片方は何も無く、もう片方には以前のインストールのリンクがある
        let dir = temp_dir("replace");
        let fresh = dir.join("bin").join("flashcap");
        let earlier = dir.join("earlier");
        std::os::unix::fs::symlink("/old/flashcap", &earlier).unwrap();

        // Act
        let fresh_status = run_install_script(&fresh);
        let earlier_status = run_install_script(&earlier);

        // Assert
        assert!(fresh_status.success());
        assert!(earlier_status.success());
        assert_eq!(read_link(&fresh), "/target/flashcap");
        assert_eq!(read_link(&earlier), "/target/flashcap");
    }

    #[test]
    fn install_script_leaves_a_file_or_directory_alone() {
        // Arrange: 普通のファイルと本物のディレクトリがある
        let dir = temp_dir("keep");
        let file = dir.join("file");
        std::fs::write(&file, "keep").unwrap();
        let directory = dir.join("directory");
        std::fs::create_dir(&directory).unwrap();

        // Act
        let file_status = run_install_script(&file);
        let directory_status = run_install_script(&directory);

        // Assert: 置き換えず、ディレクトリの中にもリンクを作らない
        assert_eq!(file_status.code(), Some(EXISTS_EXIT_CODE));
        assert_eq!(directory_status.code(), Some(EXISTS_EXIT_CODE));
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "keep");
        assert_eq!(std::fs::read_dir(&directory).unwrap().count(), 0);
    }
}
