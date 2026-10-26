//! "Check for Updates…" (CYBERNEURA-DEV-1127。queryfolio の CYBERNEURA-DEV-1079 の移植)
//!
//! ユーザーが選んだ時だけ動く。起動時や定期のチェックはしないので、頼まれない限り
//! GitHub には何も送らない。
//!
//! - GitHub の releases API で最新の公開 Release (draft / prerelease は `releases/latest` に
//!   出てこない) を取り、そのタグを自分の version と比べる。
//! - Homebrew の cask で入れたアプリなら `brew upgrade --cask flashcap` と再起動を勧める。
//!   それ以外は Release ページを開いて手で入れてもらう。
//!
//! tauri-plugin-updater は使わない。更新署名の鍵を CI の secret に登録する必要があり、
//! それは人間の作業になる。Homebrew は cask の sha256 でダウンロードを検証するので、
//! brew 経由ならその鍵なしで更新できる。
//!
//! 入口は macOS のアプリメニュー (About / Third-Party Licenses の直下) と Preferences の
//! About 節のボタン。Windows にはアプリメニューを置いていない (lib.rs の set_app_menu) ので、
//! Windows で使えるのは後者だけ。

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use tauri::AppHandle;
use tauri_plugin_dialog::{
    DialogExt, MessageDialogButtons, MessageDialogKind, MessageDialogResult,
};

const LATEST_RELEASE_API: &str = "https://api.github.com/repos/cyberneura/flashcap/releases/latest";
/// API の `html_url` ではなくこちらを開く。アプリが開くのはこの固定のページだけになる
const RELEASES_PAGE: &str = "https://github.com/cyberneura/flashcap/releases/latest";
const CASK: &str = "flashcap";

pub(crate) const MENU_ID: &str = "check-for-updates";

const UPDATE_WITH_BREW: &str = "Update with Homebrew";
const OPEN_RELEASE_PAGE: &str = "Open Release Page";
const LATER: &str = "Later";
const RESTART_NOW: &str = "Restart Now";

/// チェック・brew upgrade・それらのダイアログのどれかが進行中の間だけ立てる
static IN_PROGRESS: AtomicBool = AtomicBool::new(false);

#[derive(Debug, serde::Deserialize)]
struct LatestRelease {
    tag_name: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
}

/// `v1.2.3` / `1.2.3` を (major, minor, patch) にする。それ以外 (pre-release の接尾辞、
/// 欠けた要素) は受け付けない。flashcap の Release は常に素の `v<major>.<minor>.<patch>`。
fn parse_version(s: &str) -> Option<(u64, u64, u64)> {
    let s = s.trim();
    let s = s.strip_prefix('v').unwrap_or(s);
    let mut parts = s.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    let patch = parts.next()?.parse().ok()?;
    if parts.next().is_some() {
        return None;
    }
    Some((major, minor, patch))
}

/// 最新版が `current` より新しい時だけ、その version (`v` 抜き) を返す。
fn newer_version(current: &str, latest: &LatestRelease) -> Option<String> {
    if latest.draft || latest.prerelease {
        return None;
    }
    let latest_v = parse_version(&latest.tag_name)?;
    let current_v = parse_version(current)?;
    (latest_v > current_v).then(|| format!("{}.{}.{}", latest_v.0, latest_v.1, latest_v.2))
}

async fn fetch_latest() -> Result<LatestRelease, String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|e| e.to_string())?;
    let res = client
        .get(LATEST_RELEASE_API)
        // GitHub は User-Agent の無い API リクエストを拒否する
        .header("User-Agent", "Flashcap update check")
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let status = res.status();
    if !status.is_success() {
        return Err(format!("GitHub returned HTTP {}", status.as_u16()));
    }
    res.json::<LatestRelease>().await.map_err(|e| e.to_string())
}

/// Caskroom に cask がある Homebrew prefix の `brew` (GUI アプリはシェルの PATH を
/// 引き継がないので、`brew` はフルパスで呼ぶ)。
fn brew_candidates(prefixes: &[&Path]) -> Vec<PathBuf> {
    prefixes
        .iter()
        .filter(|prefix| prefix.join("Caskroom").join(CASK).is_dir())
        .map(|prefix| prefix.join("bin/brew"))
        .filter(|brew| brew.is_file())
        .collect()
}

/// 実行中の実行ファイルが入っている `.app`
/// (`/Applications/Flashcap.app/Contents/MacOS/flashcap` -> `/Applications/Flashcap.app`)。
fn running_app_bundle() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?.canonicalize().ok()?;
    exe.ancestors()
        .find(|p| p.extension().is_some_and(|ext| ext == "app"))
        .map(Path::to_path_buf)
}

/// cask の appdir (アプリを置く場所)。`Caskroom/flashcap/.metadata/config.json` は Homebrew が
/// インストール時に書く設定で、`explicit` (`--appdir`) > `env` (`HOMEBREW_CASK_OPTS`) >
/// `default` の順に効く (Homebrew の `Cask::Config`)。読めなければ Homebrew の既定の
/// `/Applications`。
fn cask_appdir(config_json: Option<&str>, home: Option<&Path>) -> PathBuf {
    let config = config_json.and_then(|json| serde_json::from_str::<serde_json::Value>(json).ok());
    let appdir = config.as_ref().and_then(|config| {
        ["explicit", "env", "default"].iter().find_map(|level| {
            config
                .get(level)?
                .get("appdir")?
                .as_str()
                .filter(|dir| !dir.is_empty())
        })
    });
    match appdir {
        Some(dir) => match (dir.strip_prefix("~/"), home) {
            (Some(rest), Some(home)) => home.join(rest),
            _ => PathBuf::from(dir),
        },
        None => PathBuf::from("/Applications"),
    }
}

/// appdir が `HOMEBREW_CASK_OPTS` (`env`) だけで決まっていたか。
///
/// `brew upgrade` は保存された設定のうち `explicit` だけを引き継ぎ、`env` は upgrade を
/// 実行した時の環境から取り直す (Homebrew の `Cask::Upgrade` / `Config#merge`)。GUI アプリは
/// シェルの環境変数を引き継がないので、ここから upgrade すると既定の `/Applications` に
/// 入れ直され、動いている場所に新しい版が来ない (再起動しても古い版のまま)。
/// この場合は brew を勧めず Release ページに回す。
fn appdir_set_only_by_env(config_json: Option<&str>) -> bool {
    let Some(config) =
        config_json.and_then(|json| serde_json::from_str::<serde_json::Value>(json).ok())
    else {
        return false;
    };
    let appdir = |level: &str| {
        config
            .get(level)
            .and_then(|c| c.get("appdir"))
            .and_then(|v| v.as_str())
            .filter(|dir| !dir.is_empty())
    };
    appdir("explicit").is_none() && appdir("env").is_some_and(|env| Some(env) != appdir("default"))
}

/// `brew info --cask --json=v2 flashcap` から、cask が置いたアプリの場所を取る。
///
/// 現行の Homebrew は `app` の成果物に、インストール時の設定で解決済みの絶対パスを
/// `target` として付ける (`{"app":["Flashcap.app"],"target":"/Applications/Flashcap.app"}`)。
/// それが無い古い brew では appdir と成果物の名前から組み立てる
/// (`app "Foo.app", target: "Bar.app"` の形なら置かれる名前は `Bar.app`)。
///
/// `brew list --cask flashcap` の成果物の一覧 (`/Applications/Flashcap.app (...)`) は
/// stdout が端末の時しか出ない (パイプだと Caskroom の中を find した一覧になる) ので使わない。
fn cask_app_paths(info_json: &str, appdir: &Path, home: Option<&Path>) -> Vec<PathBuf> {
    let Ok(info) = serde_json::from_str::<serde_json::Value>(info_json) else {
        return Vec::new();
    };
    let artifacts = info["casks"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|cask| cask["token"] == CASK)
        .filter_map(|cask| cask["artifacts"].as_array())
        .flatten();
    let mut paths = Vec::new();
    for artifact in artifacts {
        let Some(args) = artifact["app"].as_array() else {
            continue;
        };
        if let Some(target) = artifact["target"].as_str().map(Path::new) {
            if target.is_absolute() {
                paths.push(target.to_path_buf());
                continue;
            }
        }
        // Homebrew の Relocated と同じ解決: target があればそれ (`~/` は展開、相対なら
        // appdir の下)、無ければ source の basename を appdir の下に置く
        let source = args.first().and_then(|v| v.as_str()).map(Path::new);
        let renamed = args
            .iter()
            .find_map(|v| v.get("target").and_then(|t| t.as_str()));
        let path = match (renamed, source) {
            (Some(target), _) => match (target.strip_prefix("~/"), home) {
                (Some(rest), Some(home)) => home.join(rest),
                (Some(_), None) => continue,
                _ => appdir.join(target),
            },
            (None, Some(source)) => match source.file_name() {
                Some(name) => appdir.join(name),
                None => continue,
            },
            (None, None) => continue,
        };
        paths.push(path);
    }
    paths
}

/// cask が置いたアプリの場所のどれかが `bundle` か。
///
/// パスは大文字小文字まで完全一致で比べる。0.8.3 で .app 名が `flashcap.app` から
/// `Flashcap.app` に変わったが、APFS の既定は大文字小文字を区別しないので、緩く比べると
/// 旧名で動いている古い版にも brew を走らせうる (brew info は tap の最新の cask を読むので、
/// 旧名の bundle は一致しない = 旧版からの更新は Release ページに回る)。
/// 実行中のパスは canonicalize 済み (= ディスク上の綴り)。
fn cask_installs_bundle(app_paths: &[PathBuf], bundle: &Path) -> bool {
    app_paths.iter().any(|path| path == bundle)
}

/// `brew list --cask --versions flashcap` の出力 (`flashcap 0.8.3`) のうち一番新しい version
fn parse_cask_version(output: &str) -> Option<(u64, u64, u64)> {
    output
        .lines()
        .filter_map(|line| {
            let mut words = line.split_whitespace();
            (words.next()? == CASK).then_some(words)
        })
        .flat_map(|words| words.filter_map(parse_version))
        .max()
}

/// 端末なしで brew を動かす (入力待ちにさせない)。成功なら stdout、失敗なら出力の末尾を返す。
fn run_brew(brew: &Path, args: &[&str], envs: &[(&str, &str)]) -> Result<String, String> {
    let output = std::process::Command::new(brew)
        .args(args)
        .envs(envs.iter().copied())
        .env("HOMEBREW_NO_ENV_HINTS", "1")
        // 最近の Homebrew は既定で ask モード (確認を求める) になり、stdin が閉じていると
        // 中断する。`--no-ask` ではなく環境変数にしているのは、古い brew は未知のフラグを
        // エラーにするが未知の環境変数は無視するため
        .env("HOMEBREW_NO_ASK", "1")
        .stdin(std::process::Stdio::null())
        .output()
        .map_err(|e| format!("could not run {}: {e}", brew.display()))?;
    if output.status.success() {
        return Ok(String::from_utf8_lossy(&output.stdout).into_owned());
    }
    let mut text = String::from_utf8_lossy(&output.stderr).into_owned();
    if text.trim().is_empty() {
        text = String::from_utf8_lossy(&output.stdout).into_owned();
    }
    Err(tail_lines(&text, 10))
}

/// 実行中のアプリを管理している `brew` (あれば)。Caskroom にあるだけでは足りない。
/// 動いているのが別の場所に手で置いたコピーなら、cask を上げても別のアプリが更新される
/// だけになる。だから cask が置いた場所が自分自身の bundle であることを条件にする。
/// ブロックする (`brew info` を実行する)。
fn homebrew_brew_for_running_app() -> Option<PathBuf> {
    if !cfg!(target_os = "macos") {
        return None;
    }
    let bundle = running_app_bundle()?;
    let home = dirs::home_dir();
    brew_candidates(&[Path::new("/opt/homebrew"), Path::new("/usr/local")])
        .into_iter()
        .find(|brew| {
            // brew_candidates は <prefix>/bin/brew を返す
            let Some(prefix) = brew.parent().and_then(Path::parent) else {
                return false;
            };
            let config = std::fs::read_to_string(
                prefix
                    .join("Caskroom")
                    .join(CASK)
                    .join(".metadata/config.json"),
            )
            .ok();
            if appdir_set_only_by_env(config.as_deref()) {
                return false;
            }
            let appdir = cask_appdir(config.as_deref(), home.as_deref());
            run_brew(brew, &["info", "--cask", "--json=v2", CASK], &[]).is_ok_and(|out| {
                cask_installs_bundle(&cask_app_paths(&out, &appdir, home.as_deref()), &bundle)
            })
        })
}

fn tail_lines(text: &str, n: usize) -> String {
    let lines: Vec<&str> = text.trim_end().lines().collect();
    lines[lines.len().saturating_sub(n)..].join("\n")
}

/// ダイアログを出し、閉じられるまで待つ。待ちはブロッキング用のスレッドで行い、
/// ユーザーがダイアログを読んでいる間 async ランタイムのワーカーを塞がない。
async fn ask(
    app: &AppHandle,
    kind: MessageDialogKind,
    text: String,
    buttons: MessageDialogButtons,
) -> MessageDialogResult {
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        app.dialog()
            .message(text)
            .title("Check for Updates")
            .kind(kind)
            .buttons(buttons)
            .blocking_show_with_result()
    })
    .await
    .unwrap_or_default()
}

async fn message(app: &AppHandle, kind: MessageDialogKind, text: String) {
    ask(app, kind, text, MessageDialogButtons::Ok).await;
}

fn open_release_page() {
    if let Err(e) = tauri_plugin_opener::open_url(RELEASES_PAGE, None::<&str>) {
        eprintln!("[update] failed to open the release page: {e}");
    }
}

/// Preferences の Check for Updates… ボタン (Windows ではここが唯一の入口)
#[tauri::command]
pub fn check_for_updates(app: AppHandle) {
    start(&app);
}

/// メニュー / Preferences からの入口。すぐ戻り、チェックとダイアログは裏のタスクで動く。
pub(crate) fn start(app: &AppHandle) {
    // 最後のダイアログが閉じるまで立てたままにし、連打でダイアログが積み重ならないようにする
    if IN_PROGRESS.swap(true, Ordering::SeqCst) {
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        run(&app).await;
        IN_PROGRESS.store(false, Ordering::SeqCst);
    });
}

async fn run(app: &AppHandle) {
    let current = app.package_info().version.to_string();
    let latest = match fetch_latest().await {
        Ok(latest) => latest,
        Err(e) => {
            let text = format!("Could not check for updates.\n\n{e}");
            message(app, MessageDialogKind::Error, text).await;
            return;
        }
    };
    let Some(newer) = newer_version(&current, &latest) else {
        let text = format!("Flashcap {current} is the latest version.");
        message(app, MessageDialogKind::Info, text).await;
        return;
    };

    let brew = tauri::async_runtime::spawn_blocking(homebrew_brew_for_running_app)
        .await
        .ok()
        .flatten();
    let buttons = if brew.is_some() {
        MessageDialogButtons::YesNoCancelCustom(
            UPDATE_WITH_BREW.into(),
            OPEN_RELEASE_PAGE.into(),
            LATER.into(),
        )
    } else {
        MessageDialogButtons::OkCancelCustom(OPEN_RELEASE_PAGE.into(), LATER.into())
    };
    let text = format!("Flashcap {newer} is available. You have {current}.");
    let choice = ask(app, MessageDialogKind::Info, text, buttons).await;

    match (choice, brew) {
        (MessageDialogResult::Custom(label), Some(brew)) if label == UPDATE_WITH_BREW => {
            upgrade_with_brew(app, brew, newer).await;
        }
        (MessageDialogResult::Custom(label), _) if label == OPEN_RELEASE_PAGE => {
            open_release_page();
        }
        _ => {}
    }
}

async fn upgrade_with_brew(app: &AppHandle, brew: PathBuf, newer: String) {
    let result = tauri::async_runtime::spawn_blocking(move || {
        // brew は前回の更新から HOMEBREW_AUTO_UPDATE_SECS (既定 1 日) 経っていないと tap を
        // 読み直さない。0 にして今の tap を見させる。
        // brew は upgrade の前に動いている cask のアプリを終了させるので、結果を確かめて
        // 再起動を勧める前にこのプロセスが殺される。HOMEBREW_NO_UPGRADE_QUIT_CASKS
        // (= --no-quit。HOMEBREW_NO_ASK と同じ理由で環境変数) でそれを止める
        run_brew(
            &brew,
            &["upgrade", "--cask", CASK],
            &[
                ("HOMEBREW_AUTO_UPDATE_SECS", "0"),
                ("HOMEBREW_NO_UPGRADE_QUIT_CASKS", "1"),
            ],
        )?;
        // upgrade が成功しても新しい版が入ったとは限らない。tap がまだ Release を拾って
        // いなければ、brew は最新扱いで cask を飛ばす
        run_brew(&brew, &["list", "--cask", "--versions", CASK], &[])
            .map(|out| parse_cask_version(&out))
    })
    .await
    .unwrap_or_else(|e| Err(e.to_string()));

    let installed = match result {
        Ok(installed) => installed,
        Err(e) => {
            let text = format!(
                "brew upgrade failed.\n\n{e}\n\nRun `brew upgrade --cask {CASK}` in a terminal to see the details."
            );
            message(app, MessageDialogKind::Error, text).await;
            return;
        }
    };
    if installed.is_none() || installed < parse_version(&newer) {
        let text = format!(
            "Homebrew does not offer Flashcap {newer} yet. The tap follows new releases within an hour; try again later, or download it from the Release page."
        );
        let buttons = MessageDialogButtons::OkCancelCustom(OPEN_RELEASE_PAGE.into(), LATER.into());
        let choice = ask(app, MessageDialogKind::Info, text, buttons).await;
        if matches!(&choice, MessageDialogResult::Custom(label) if label == OPEN_RELEASE_PAGE) {
            open_release_page();
        }
        return;
    }
    let text = format!("Flashcap {newer} was installed. Restart Flashcap to use it.");
    let buttons = MessageDialogButtons::OkCancelCustom(RESTART_NOW.into(), LATER.into());
    let choice = ask(app, MessageDialogKind::Info, text, buttons).await;
    if matches!(&choice, MessageDialogResult::Custom(label) if label == RESTART_NOW) {
        app.restart();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn release(tag: &str) -> LatestRelease {
        LatestRelease {
            tag_name: tag.into(),
            draft: false,
            prerelease: false,
        }
    }

    #[test]
    fn test_parse_version() {
        assert_eq!(parse_version("v0.9.0"), Some((0, 9, 0)));
        assert_eq!(parse_version("1.2.3"), Some((1, 2, 3)));
        assert_eq!(parse_version(" v0.10.1 "), Some((0, 10, 1)));
        assert_eq!(parse_version("v1.2"), None);
        assert_eq!(parse_version("v1.2.3.4"), None);
        assert_eq!(parse_version("v1.2.3-beta.1"), None);
        assert_eq!(parse_version("latest"), None);
    }

    #[test]
    fn test_newer_version_compares_numerically() {
        assert_eq!(
            newer_version("0.9.0", &release("v0.10.0")),
            Some("0.10.0".into())
        );
        assert_eq!(
            newer_version("0.9.0", &release("v0.9.1")),
            Some("0.9.1".into())
        );
        assert_eq!(
            newer_version("0.9.9", &release("v1.0.0")),
            Some("1.0.0".into())
        );
    }

    #[test]
    fn test_newer_version_none_when_same_or_older() {
        assert_eq!(newer_version("0.9.0", &release("v0.9.0")), None);
        assert_eq!(newer_version("0.10.0", &release("v0.9.5")), None);
    }

    #[test]
    fn test_newer_version_ignores_drafts_prereleases_and_bad_tags() {
        let mut r = release("v9.0.0");
        r.draft = true;
        assert_eq!(newer_version("0.9.0", &r), None);
        let mut r = release("v9.0.0");
        r.prerelease = true;
        assert_eq!(newer_version("0.9.0", &r), None);
        assert_eq!(newer_version("0.9.0", &release("nightly")), None);
    }

    #[test]
    fn test_latest_release_deserializes_github_payload() {
        let json = r#"{"tag_name":"v0.9.0","name":"Flashcap v0.9.0","draft":false,"prerelease":false,"html_url":"https://github.com/cyberneura/flashcap/releases/tag/v0.9.0"}"#;
        let r: LatestRelease = serde_json::from_str(json).unwrap();
        assert_eq!(r.tag_name, "v0.9.0");
        assert!(!r.draft && !r.prerelease);
    }

    #[test]
    fn test_brew_candidates_require_the_cask_and_brew() {
        let dir = std::env::temp_dir().join(format!("flashcap-brew-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let with_cask = dir.join("with-cask");
        let without_cask = dir.join("without-cask");
        let without_brew = dir.join("without-brew");
        for prefix in [&with_cask, &without_cask] {
            std::fs::create_dir_all(prefix.join("bin")).unwrap();
            std::fs::write(prefix.join("bin/brew"), "").unwrap();
        }
        std::fs::create_dir_all(with_cask.join("Caskroom/flashcap")).unwrap();
        std::fs::create_dir_all(without_brew.join("Caskroom/flashcap")).unwrap();

        assert!(brew_candidates(&[&without_cask, &without_brew]).is_empty());
        assert_eq!(
            brew_candidates(&[&without_cask, &with_cask, &without_brew]),
            vec![with_cask.join("bin/brew")]
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn test_cask_appdir() {
        let home = Path::new("/Users/me");
        // explicit > env > default
        let json = r#"{"default":{"appdir":"/Applications"},"env":{"appdir":"/Env"},"explicit":{"appdir":"/Explicit"}}"#;
        assert_eq!(
            cask_appdir(Some(json), Some(home)),
            PathBuf::from("/Explicit")
        );
        let json = r#"{"default":{"appdir":"/Applications"},"env":{"appdir":"~/Applications"},"explicit":{}}"#;
        assert_eq!(
            cask_appdir(Some(json), Some(home)),
            PathBuf::from("/Users/me/Applications")
        );
        let json = r#"{"default":{"appdir":"/Applications"},"env":{},"explicit":{}}"#;
        assert_eq!(
            cask_appdir(Some(json), Some(home)),
            PathBuf::from("/Applications")
        );
        // 読めない・壊れている時は Homebrew の既定
        assert_eq!(
            cask_appdir(None, Some(home)),
            PathBuf::from("/Applications")
        );
        assert_eq!(
            cask_appdir(Some("{"), Some(home)),
            PathBuf::from("/Applications")
        );
    }

    // Homebrew は macOS 専用で、ここの /Users/... は Windows では絶対パスにならない
    #[cfg(unix)]
    #[test]
    fn test_cask_app_paths() {
        let appdir = Path::new("/Applications");
        // 現行の brew: 解決済みの target を使う
        let json = r#"{"formulae":[],"casks":[{"token":"flashcap","artifacts":[{"uninstall":[{"quit":"com.cyberneura.flashcap"}]},{"app":["Flashcap.app"],"target":"/Users/me/Applications/Flashcap.app"},{"zap":[{"trash":["~/.config/flashcap"]}]}]}]}"#;
        assert_eq!(
            cask_app_paths(json, appdir, Some(Path::new("/Users/me"))),
            vec![PathBuf::from("/Users/me/Applications/Flashcap.app")]
        );
        // target の無い古い brew: appdir + 名前
        let json = r#"{"casks":[{"token":"flashcap","artifacts":[{"app":["Flashcap.app"]}]}]}"#;
        assert_eq!(
            cask_app_paths(json, appdir, Some(Path::new("/Users/me"))),
            vec![PathBuf::from("/Applications/Flashcap.app")]
        );
        let json = r#"{"casks":[{"token":"flashcap","artifacts":[{"app":["flashcap.app",{"target":"Renamed.app"}]}]}]}"#;
        assert_eq!(
            cask_app_paths(json, appdir, Some(Path::new("/Users/me"))),
            vec![PathBuf::from("/Applications/Renamed.app")]
        );
        // source のディレクトリは置き場所に入らない (basename だけ)
        let json =
            r#"{"casks":[{"token":"flashcap","artifacts":[{"app":["dist/Flashcap.app"]}]}]}"#;
        assert_eq!(
            cask_app_paths(json, appdir, Some(Path::new("/Users/me"))),
            vec![PathBuf::from("/Applications/Flashcap.app")]
        );
        // target の ~/ はホームに展開する
        let json = r#"{"casks":[{"token":"flashcap","artifacts":[{"app":["Flashcap.app",{"target":"~/Apps/Flashcap.app"}]}]}]}"#;
        assert_eq!(
            cask_app_paths(json, appdir, Some(Path::new("/Users/me"))),
            vec![PathBuf::from("/Users/me/Apps/Flashcap.app")]
        );
        // 別の cask の成果物は見ない
        let json = r#"{"casks":[{"token":"other","artifacts":[{"app":["Flashcap.app"]}]}]}"#;
        assert!(cask_app_paths(json, appdir, Some(Path::new("/Users/me"))).is_empty());
        assert!(cask_app_paths("not json", appdir, None).is_empty());
    }

    #[test]
    fn test_appdir_set_only_by_env() {
        let env_only = r#"{"default":{"appdir":"/Applications"},"env":{"appdir":"/Users/me/Applications"},"explicit":{}}"#;
        assert!(appdir_set_only_by_env(Some(env_only)));
        // explicit (--appdir) は upgrade に引き継がれる
        let explicit = r#"{"default":{"appdir":"/Applications"},"env":{"appdir":"/Users/me/Applications"},"explicit":{"appdir":"/Users/me/Applications"}}"#;
        assert!(!appdir_set_only_by_env(Some(explicit)));
        // env が既定と同じなら upgrade でも同じ場所に入る
        let same = r#"{"default":{"appdir":"/Applications"},"env":{"appdir":"/Applications"},"explicit":{}}"#;
        assert!(!appdir_set_only_by_env(Some(same)));
        let plain = r#"{"default":{"appdir":"/Applications"},"env":{},"explicit":{}}"#;
        assert!(!appdir_set_only_by_env(Some(plain)));
        assert!(!appdir_set_only_by_env(None));
    }

    #[test]
    fn test_cask_installs_bundle() {
        let paths = vec![PathBuf::from("/Applications/Flashcap.app")];
        assert!(cask_installs_bundle(
            &paths,
            Path::new("/Applications/Flashcap.app")
        ));
        // 別の場所に手で置いたコピーは cask のものではない
        assert!(!cask_installs_bundle(
            &paths,
            Path::new("/Users/me/Apps/Flashcap.app")
        ));
        assert!(!cask_installs_bundle(
            &paths,
            Path::new("/Applications/Flashcap.app.old")
        ));
        // 0.8.2 以前の旧名 (flashcap.app) で動いている時は、tap の cask (Flashcap.app) と
        // 綴りが違うので一致させない (brew を走らせない)
        assert!(!cask_installs_bundle(
            &paths,
            Path::new("/Applications/flashcap.app")
        ));
        assert!(!cask_installs_bundle(
            &[],
            Path::new("/Applications/Flashcap.app")
        ));
    }

    #[test]
    fn test_parse_cask_version() {
        assert_eq!(parse_cask_version("flashcap 0.10.0\n"), Some((0, 10, 0)));
        assert_eq!(
            parse_cask_version("flashcap 0.9.0 0.10.0\n"),
            Some((0, 10, 0))
        );
        assert_eq!(parse_cask_version("other 1.0.0\n"), None);
        assert_eq!(parse_cask_version(""), None);
    }

    #[test]
    fn test_tail_lines() {
        assert_eq!(tail_lines("a\nb\nc\n", 2), "b\nc");
        assert_eq!(tail_lines("a", 5), "a");
    }
}
