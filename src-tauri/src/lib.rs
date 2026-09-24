// objc 0.2 の msg_send! マクロ展開時に出る unexpected_cfgs(cargo-clippy)警告を抑制
#![allow(unexpected_cfgs)]

mod auto_copy;
mod menu_bar;
mod ocr;
mod video;

use base64::{engine::general_purpose::STANDARD, Engine as _};
use chrono::Local;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
#[cfg(target_os = "macos")]
use std::process::Command;
#[cfg(target_os = "macos")]
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::{Emitter, Listener, Manager, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_store::StoreExt;

const SUPPORTED_IMAGE_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "gif", "bmp", "webp", "tiff", "tif", "heic", "heif"];

#[derive(Debug, Serialize, Deserialize)]
pub struct ScreenshotResult {
    pub width: usize,
    pub height: usize,
    pub data: String,      // base64 encoded PNG
    pub file_path: String, // saved file path
}

/// flashcap が一時ファイルと既定の保存先に使うディレクトリ ($TMPDIR/flashcap)
///
/// **/tmp を直接使わないこと。** macOS の /tmp (= /private/tmp) は mode 1777 の
/// 共有領域で、同じ Mac の別ユーザーアカウントから中身を読める。このアプリが扱うのは
/// 画面に映っていたものそのもの (パスワード入力画面・社内資料・メール) なので、
/// 既定の置き場が他人から読めてよいものではない。
///
/// std::env::temp_dir() は macOS では通常 $TMPDIR (/var/folders/.../T/) を返し、
/// これはユーザー専用。ただし **TMPDIR が未設定の実行環境では /tmp に落ちる**ので、
/// 「temp_dir() だから安全」と考えてはいけない。実際の権限は
/// ensure_private_flashcap_dir() が確かめて締める。
pub(crate) fn flashcap_temp_dir() -> PathBuf {
    std::env::temp_dir().join("flashcap")
}

/// 所有者だけが読み書きできるディレクトリを作る (無ければ、親ごと)
///
/// std::fs::create_dir_all は mode を指定しないため umask 依存で 0755 になりうる。
/// スクリーンショットの置き場としてはそれでは広すぎるので、mode を明示して作る。
/// mkdir(2) の mode は umask で削られるだけなので、0700 より広くなることはない。
///
/// **既に在るディレクトリの mode は変えない。** ユーザーが custom: で指定した
/// 既存フォルダの権限を、こちらの都合で締めてしまわないため。flashcap 自身の
/// 作業ディレクトリを締め直したい場合は ensure_private_flashcap_dir() を使う。
#[cfg(unix)]
pub(crate) fn create_private_dir<P: AsRef<Path>>(dir: P) -> std::io::Result<()> {
    use std::os::unix::fs::DirBuilderExt;
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(dir)
}

/// Windows には Unix の mode が無い。作ったディレクトリは親の ACL を継承するので、
/// 既定の置き場 (%TEMP%\flashcap。%TEMP% は %LOCALAPPDATA%\Temp でユーザー専用) は
/// 他のアカウントから読めない。custom: の任意フォルダは、そのフォルダの親の ACL 次第
#[cfg(not(unix))]
pub(crate) fn create_private_dir<P: AsRef<Path>>(dir: P) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)
}

/// dir とその祖先に、自分以外のアカウントが手を出せる要素が無いことを確かめる
///
/// **dir 自身の mode だけでは足りない。** 0700 のディレクトリでも、その**エントリ**が
/// 誰でも書ける親の下にあれば、中を辿らずに丸ごと rename して差し替えられる。
/// パス名で辿り直す以上、道中のどの要素にも割り込まれてはいけない。
///
/// 見るのは group と other の書き込みビット。**sticky を例外にはしない。**
/// sticky なら他人のエントリを消せないので rename による差し替えは防げるが、
/// それを許すと /tmp (1777) が通ってしまう。macOS の $TMPDIR の祖先
/// (/var/folders/... と /private/var、/) はどれも他人に書けないので、例外を
/// 設ける実利が無いまま /tmp を招き入れることになる。
///
/// **所有者も見る。** mode だけでは足りない: 他人が所有する 0755 のディレクトリは
/// group/other の書き込みビットが立っていないので mode の検査は通るが、**その所有者
/// 自身は書ける**ので、検査から書き込みまでの間に次の要素を rename して差し替えられる。
/// 最後の chmod が所有を証明するのは `flashcap` ディレクトリだけなので、道中は
/// ここで見るしかない。許すのは root と自分だけ (root を許さないと `/` や `/var` で
/// 落ちる)。
///
/// 限界: 見ているのは Unix の mode と uid だけ。macOS の拡張 ACL は検査していないので、
/// ACL で他ユーザーに開かれた要素はここを通る。また、この検査と実際の書き込みの間で
/// パスを完全に固定しているわけではない (それには openat/O_NOFOLLOW でハンドルを
/// 握り続ける必要がある)。いずれも残存リスクであって、この関数が塞いだ範囲ではない。
#[cfg(unix)]
fn reject_if_others_can_meddle(dir: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::MetadataExt;
    use std::os::unix::fs::PermissionsExt;

    // SAFETY: getuid は常に成功し、シグナル安全で副作用を持たない
    let self_uid = unsafe { libc::getuid() };

    for ancestor in dir.ancestors() {
        let meta = std::fs::metadata(ancestor)?;
        let mode = meta.permissions().mode();
        if mode & 0o022 != 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                format!(
                    "{} is writable by other accounts, so {} cannot be trusted; refusing to \
                     store screen captures there (is TMPDIR set?)",
                    ancestor.display(),
                    dir.display()
                ),
            ));
        }
        let owner = meta.uid();
        if owner != 0 && owner != self_uid {
            return Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                format!(
                    "{} is owned by uid {} (neither root nor the current user), so {} cannot \
                     be trusted; refusing to store screen captures there (is TMPDIR set?)",
                    ancestor.display(),
                    owner,
                    dir.display()
                ),
            ));
        }
    }
    Ok(())
}

/// flashcap 専用の作業ディレクトリを、所有者だけが触れる状態にして返す
///
/// **既に在るディレクトリを締め直すのがこの関数の要点。** $TMPDIR/flashcap は
/// 以前のバージョンの video_temp_dir() が create_dir_all で作っており、umask 次第で
/// 0755 のまま残っている。新規作成時に mode を渡すだけでは、既に一度でも録画した
/// ユーザーの環境が直らない (そして直らないまま、そこへスクリーンショットを
/// 置き始めることになる)。
///
/// 所有者の確認に uid を引く必要はない。**chmod は所有者にしか通らない**ので、
/// 他人が作ったディレクトリならここが EPERM で落ちる。その場合は奪い返さず、
/// エラーを返して書き込みをやめる。だから chmod は**現在の mode に関わらず毎回**
/// 呼ぶ。既に 0700 の時に省くと、他人所有の 0700 ディレクトリが素通りしてしまう
/// (所有者確認がこの呼び出しそのものなので、省いた瞬間に確認も消える)。
///
/// **保証しているのは Unix の mode だけ。** macOS の拡張 ACL は見ていないので、
/// ACL で他ユーザーに開かれたディレクトリはここを通る。これは残存リスクであって
/// この関数が塞いだ範囲ではない。塞ぐなら chmod -N まで踏み込むこと。
pub(crate) fn ensure_private_flashcap_dir() -> std::io::Result<PathBuf> {
    // 置き場に至る道のどこかが自分以外にも書けるなら、この先の検査は意味を持たない。
    // 「stat して chmod して使う」はパス名を辿り直す 3 手なので、その間にどこかの
    // 要素を差し替えられると、検査した対象と実際に書く先が別物になる (TOCTOU)。
    // 誰も割り込めないことを先に確かめる。
    //
    // macOS の GUI プロセスは launchd がユーザー専用の TMPDIR (0700) を必ず渡すので、
    // ここに引っかかるのは TMPDIR を落とした異常な起動か、/var 以下が壊れている
    // マシンだけ。その場合は /tmp へ黙って落ちるより、撮らずに止まる方が正しい。
    //
    // Windows は mode と uid の代わりに ACL で守られていて、この検査は当てはまらない。
    // %TEMP% (%LOCALAPPDATA%\Temp) はユーザー専用の ACL を持つので、そこに頼る。
    #[cfg(unix)]
    reject_if_others_can_meddle(&std::env::temp_dir())?;

    let dir = flashcap_temp_dir();
    create_private_dir(&dir)?;

    // symlink 越しに書かせない。作業ディレクトリの実体が別の場所を指していると、
    // 以降の書き込みも削除もそちらへ向く。symlink_metadata はリンクを辿らないので、
    // 「リンクではなく本物のディレクトリか」をここで判定できる。
    let meta = std::fs::symlink_metadata(&dir)?;
    if !meta.file_type().is_dir() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("{} is not a real directory", dir.display()),
        ));
    }

    // **先に締めてから片付ける。順序が逆だと意味が無い。**
    //
    // 片付けを先にすると、片付け終わってから chmod が効くまでの隙に、まだ
    // 書ける状態のディレクトリへ新しい symlink を置かれる。それは「掃除済みの
    // 信頼できるディレクトリ」の中に残り、以降の書き込みがそのリンク先へ抜ける。
    // 先に 0700 にしてしまえば他人はもうエントリを追加できないので、その後の
    // 片付けが取りこぼしなく終わる。
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700))?;
    }

    // 締める前に他人が置いていったものを片付ける。
    //
    // **mode を直すだけでは足りない。** 以前のバージョンが 0755 / 0775 で作った
    // ディレクトリには、締める前に他人が作れたエントリが残っている。特に
    // `flashcap-paste-<timestamp>.png` は名前が予測できるので、symlink を
    // 先置きされていると、締め直した後の書き込みがそのリンク先へ抜ける。
    let had_foreign_entries = purge_foreign_entries(&dir)?;
    if had_foreign_entries {
        eprintln!(
            "[flashcap] {} contained entries owned by other accounts or symlinks; removed them",
            dir.display()
        );
    }

    Ok(dir)
}

/// 作業ディレクトリから「自分が所有する通常のファイル / ディレクトリ」以外を消す
///
/// 消すのは symlink と、自分以外が所有するエントリ。**symlink は中身を見ずに消す**
/// (辿ると判定そのものがリンク先の情報になる)。1 件でも消したら true を返す。
///
/// ここは flashcap が自分で作る一時ファイルしか置かない場所なので、消して困る
/// ものは無い。逆に残すと、締め直した後の書き込みが他人の仕掛けを踏む。
fn purge_foreign_entries(dir: &Path) -> std::io::Result<bool> {
    let mut removed_any = false;

    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        // symlink_metadata はリンクを辿らない
        let meta = std::fs::symlink_metadata(&path)?;
        let is_symlink = meta.file_type().is_symlink();
        if !is_symlink && is_owned_by_self(&meta) {
            continue;
        }
        let removed = if !is_symlink && meta.file_type().is_dir() {
            std::fs::remove_dir_all(&path)
        } else {
            std::fs::remove_file(&path)
        };
        match removed {
            Ok(()) => removed_any = true,
            Err(e) => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::PermissionDenied,
                    format!(
                        "{} could not be removed ({e}); refusing to use {} for screen captures",
                        path.display(),
                        dir.display()
                    ),
                ))
            }
        }
    }
    Ok(removed_any)
}

/// エントリの所有者が自分か
#[cfg(unix)]
fn is_owned_by_self(meta: &std::fs::Metadata) -> bool {
    use std::os::unix::fs::MetadataExt;
    // SAFETY: getuid は常に成功し、シグナル安全で副作用を持たない
    meta.uid() == unsafe { libc::getuid() }
}

/// Windows の所有者は ACL 側の概念で、std からは引けない。%TEMP% はユーザー専用なので
/// 他のアカウントがエントリを置くことはできず、ここでは symlink だけを片付ける
#[cfg(not(unix))]
fn is_owned_by_self(_meta: &std::fs::Metadata) -> bool {
    true
}

/// 外部コマンドの出力先に使う、使い捨ての専用サブディレクトリ
///
/// ここで確保するのは**名前**の方。ファイルではなくディレクトリを mkdir で作る。
/// mkdir は既存の名前に対して必ず失敗する (= O_EXCL 相当) ので、通れば
/// 「この名前は自分が取った」ことが確定し、しかも**中のファイル名を使い終わるまで
/// 予約したままにできる**。ファイルを O_EXCL で作って消す方式だと、消した瞬間から
/// 外部コマンドが作るまでの間が空いてしまい、予約になっていなかった。
/// 出力ファイル自体はまだ存在しないので、外部コマンドが既存ファイルを上書き
/// できるかどうかにも依存しない。
///
/// 画面に映っていたものを残さないための後始末は、途中で抜ける経路が多く手で書くと
/// 必ず漏れる (特に **Future がキャンセルされた場合は、以降の行が 1 行も動かない**)。
/// drop に載せておけば、キャンセルでも早期 return でも同じように消える。
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) struct PrivateWorkdir {
    dir: PathBuf,
    /// 外部コマンドに渡す出力先 (dir の中の固定名)
    ///
    /// **String に落とさない。** Unix のパスはバイト列で UTF-8 とは限らず、
    /// to_string_lossy() は不正なバイトを U+FFFD に置換する。TMPDIR に非 UTF-8 の
    /// バイトが含まれていると、実際に作ったディレクトリとは別の (存在しない)
    /// パスを外部コマンドに渡すことになる。
    pub(crate) path: PathBuf,
}

impl Drop for PrivateWorkdir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// flashcap の作業ディレクトリの下に PrivateWorkdir を 1 つ確保する
///
/// 基底には ensure_private_flashcap_dir() を使う。**std::env::temp_dir() を直接
/// 使わないこと。** TMPDIR が未設定の実行環境では /tmp (mode 1777) に落ち、
/// 画面に映っていたものが同じ Mac の別アカウントから読める状態になる。
/// 基底の用意に失敗したら、書き込まずにエラーを返す (撮らずに止まる方が正しい)。
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) fn create_private_workdir(
    prefix: &str,
    file_name: &str,
) -> Result<PrivateWorkdir, String> {
    let base = ensure_private_flashcap_dir()
        .map_err(|e| format!("Failed to prepare the temp directory: {}", e))?;
    reserve_private_workdir(&base, prefix, file_name)
}

/// base の下に mode 0700 のサブディレクトリを 1 つ確保する
///
/// base を引数に取るのは、この予約ロジックだけを基底の用意から切り離して
/// テストできるようにするため。実際の呼び出しでは create_private_workdir を使う。
///
/// 名前は PID + ナノ秒。PID だけだと同一プロセス内の同時実行が衝突する。
/// 衝突しても mkdir が弾くので、取り違えではなく取り直しになる。
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn reserve_private_workdir(
    base: &Path,
    prefix: &str,
    file_name: &str,
) -> Result<PrivateWorkdir, String> {
    for _ in 0..16 {
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let dir = base.join(format!("{}-{}-{}", prefix, std::process::id(), ts));
        match create_exclusive_private_dir(&dir) {
            Ok(()) => {
                let path = dir.join(file_name);
                return Ok(PrivateWorkdir { dir, path });
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(format!("Failed to create the temp directory: {}", e)),
        }
    }
    Err(format!("Failed to reserve a temp directory for {}", prefix))
}

/// 1 階層だけ作る (既存なら AlreadyExists)。Unix では所有者専用の 0700 で作る
#[cfg(unix)]
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn create_exclusive_private_dir(dir: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::DirBuilderExt;
    std::fs::DirBuilder::new().mode(0o700).create(dir)
}

/// Windows は親 (ユーザー専用の %TEMP%\flashcap) の ACL を継承する
#[cfg(not(unix))]
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn create_exclusive_private_dir(dir: &Path) -> std::io::Result<()> {
    std::fs::create_dir(dir)
}

/// 保存先を書き込める状態にして返す
///
/// **設定の種別ではなく、解決後のパスで振り分ける。** 解決先が flashcap の作業
/// ディレクトリそのものなら、既存でも所有者専用まで締め直す — custom: でそこを
/// 名指ししていた場合も同じ扱いになるが、flashcap が管理する場所であることに
/// 変わりはないので、それでよい。
///
/// それ以外 (ユーザーが選んだ任意のフォルダ) は無ければ 0700 で作るだけで、
/// **既存フォルダの権限には触らない** — 意図して共有しているフォルダを、
/// こちらの都合で締めないため。
pub(crate) fn prepare_save_directory(app: &tauri::AppHandle) -> Result<String, String> {
    let managed = |e: std::io::Error, dir: &str| format!("Failed to prepare save directory '{}': {}", dir, e);

    let dir = get_save_directory(app);
    if Path::new(&dir) == flashcap_temp_dir() {
        return ensure_private_flashcap_dir()
            .map(|p| p.to_string_lossy().to_string())
            .map_err(|e| managed(e, &dir));
    }

    create_private_dir(&dir)
        .map_err(|e| format!("Failed to create save directory '{}': {}", dir, e))?;

    // 上の比較は字句比較なので、custom: が symlink や .. 越しに作業ディレクトリを
    // 指している場合をすり抜ける。すり抜けると「既存の権限に触らない」側へ回り、
    // 旧版が残した 0755 のディレクトリにスクリーンショットを置き続けることになる。
    // 実体で突き合わせて拾い直す (canonicalize は実在しないパスに失敗するので、
    // 作った後に呼ぶ)。解決できなければ字句比較の結果をそのまま採る。
    if let (Ok(resolved), Ok(temp)) = (
        dunce::canonicalize(&dir),
        dunce::canonicalize(flashcap_temp_dir()),
    ) {
        if resolved == temp {
            return ensure_private_flashcap_dir()
                .map(|p| p.to_string_lossy().to_string())
                .map_err(|e| managed(e, &dir));
        }
    }
    Ok(dir)
}

/// 設定から保存先ディレクトリを取得する
/// "tmp" -> $TMPDIR/flashcap/
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
        "tmp" => default_save_directory(),
        // 設定値の文字列は互換のため "macos_default" のまま。Windows では
        // Win+PrintScreen の保存先 (ピクチャ\スクリーンショット) を指す
        "macos_default" => get_os_screenshot_dir(),
        s if s.starts_with("custom:") => s.strip_prefix("custom:").unwrap().to_string(),
        _ => default_save_directory(),
    }
}

/// 設定値 "tmp" が指す実際のパス。設定値の文字列は互換のため "tmp" のまま残している
fn default_save_directory() -> String {
    flashcap_temp_dir().to_string_lossy().to_string()
}

/// 既定の保存先の実パスを返す (Preferences の表示用)
/// 環境によって変わるパスなので、画面にハードコードせずここから引く
#[tauri::command]
fn get_default_save_directory() -> String {
    default_save_directory()
}

/// 保存先フォルダを Finder で開く（未キャプチャ時用。無ければ作成する）
#[tauri::command]
fn open_save_directory(app: tauri::AppHandle) -> Result<(), String> {
    let dir = prepare_save_directory(&app)?;
    open_directory(&app, &dir)
}

#[cfg(target_os = "macos")]
fn open_directory(_app: &tauri::AppHandle, dir: &str) -> Result<(), String> {
    let status = Command::new("open")
        .arg(dir)
        .status()
        .map_err(|e| format!("Failed to open save directory '{}': {}", dir, e))?;
    if !status.success() {
        return Err(format!("`open` exited with {} for '{}'", status, dir));
    }
    Ok(())
}

/// macOS 以外は opener プラグインに任せる。Windows の explorer.exe は成功しても
/// 終了コード 1 を返すので、`open` と同じように終了コードで判定できない
#[cfg(not(target_os = "macos"))]
fn open_directory(app: &tauri::AppHandle, dir: &str) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    app.opener()
        .open_path(dir, None::<&str>)
        .map_err(|e| format!("Failed to open save directory '{}': {}", dir, e))
}

/// macOS の screencapture デフォルト保存先を取得
#[cfg(target_os = "macos")]
fn get_os_screenshot_dir() -> String {
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
                .unwrap_or_else(default_save_directory)
        })
}

/// Windows の Win+PrintScreen の保存先 (ピクチャ\スクリーンショット) を取得
///
/// フォルダの実体は既知フォルダ (FOLDERID_Screenshots) で移動できるが、それを引くには
/// Shell API (SHGetKnownFolderPath) が要る。既定の場所で足りるので、ピクチャ直下の
/// Screenshots を返す (表示名は「スクリーンショット」でも、実際のフォルダ名は Screenshots)。
/// ユーザーがフォルダを移動していた場合は追従しない。
#[cfg(not(target_os = "macos"))]
fn get_os_screenshot_dir() -> String {
    dirs::picture_dir()
        .map(|p| p.join("Screenshots"))
        .or_else(dirs::desktop_dir)
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(default_save_directory)
}

/// 撮影結果の保存先パスを組み立てる
///
/// 保存先を用意できなかった場合はエラーを返して撮影を始めない。以前はここで
/// 作成失敗を握り潰していたが、この関数は「所有者しか読めない場所に置く」という
/// 保証を兼ねるようになったので、握り潰すと保証のないまま撮り続けることになる。
fn get_screenshot_path(app: &tauri::AppHandle) -> Result<String, String> {
    let dir = PathBuf::from(prepare_save_directory(app)?);
    let timestamp = Local::now().format("%Y%m%d-%H%M%S");
    let filename = format!("flashcap-{}.png", timestamp);
    Ok(dir.join(filename).to_string_lossy().to_string())
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
    // **この 2 つは CSS 側と暗黙に結合している。** 片方だけ動かすと画像が等倍で表示されなくなる:
    // - padding: viewport (`+page.svelte`) の `p-5`
    // - toolbar_h: `Toolbar.svelte` の root `py-2` (8+8) + 最も高い子 `.tool-btn` の `h-8` (32)
    //   + `border-b` (1) = 49。root の `min-h-[40px]` は下限にすぎず効いていない
    //   (`.tool-settings` は `-my-2` で親の padding に食い込むので、ツールを開いても高さは変わらない)
    let padding = 20.0;
    let toolbar_h = 49.0;

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

/// HEIC/HEIF ファイルを macOS sips コマンドで PNG に変換してバイト列と寸法を返す
///
/// 中間 PNG は変換元の画像そのものなので、他アカウントから読める場所に置かない。
/// work が生きている間だけ作業ディレクトリが存在し、以降どこで抜けても drop が
/// 中間 PNG ごと消すので、明示的な後始末は書かない。
#[cfg(target_os = "macos")]
fn convert_heic_to_png(source_path: &str) -> Result<(Vec<u8>, u32, u32), String> {
    let work = create_private_workdir("heic", "converted.png")?;

    let output = Command::new("sips")
        .args(["-s", "format", "png", source_path, "--out"])
        .arg(&work.path)
        .output()
        .map_err(|e| format!("Failed to run sips: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("sips conversion failed: {}", stderr));
    }

    let png_data =
        std::fs::read(&work.path).map_err(|e| format!("Failed to read converted PNG: {}", e))?;

    let img = image::load_from_memory(&png_data)
        .map_err(|e| format!("Failed to decode converted PNG: {}", e))?;

    Ok((png_data, img.width(), img.height()))
}

/// sips は macOS にしか無く、image crate も HEIC を読めない
#[cfg(not(target_os = "macos"))]
fn convert_heic_to_png(_source_path: &str) -> Result<(Vec<u8>, u32, u32), String> {
    Err("HEIC / HEIF images can only be opened on macOS".to_string())
}

/// 画像ファイルを読み込んで ScreenshotResult を生成
fn load_image_result(file_path: String) -> Result<ScreenshotResult, String> {
    if !std::path::Path::new(&file_path).exists() {
        return Err("Image file does not exist".to_string());
    }

    // std::fs::canonicalize は Windows で \\?\C:\... (verbatim パス) を返し、それがパス欄や
    // コピーしたパスにそのまま出る。dunce は通常のパスで表せる時はその形で返す
    // (Unix では std と同じ)。突き合わせる側もすべて dunce で揃えること
    let absolute_path = dunce::canonicalize(&file_path)
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or(file_path);

    // HEIC/HEIF は image crate が非対応のため macOS sips で PNG に変換
    let ext = std::path::Path::new(&absolute_path)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase())
        .unwrap_or_default();

    if ext == "heic" || ext == "heif" {
        let (png_data, width, height) = convert_heic_to_png(&absolute_path)?;
        return Ok(ScreenshotResult {
            width: width as usize,
            height: height as usize,
            data: STANDARD.encode(&png_data),
            file_path: absolute_path,
        });
    }

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

/// 撮影 (screencapture の対話選択) が進行中の数
///
/// メニューバーのメニューはフロントの `isCapturing` を見られないので、撮影コマンドの側で
/// 数える。撮影中に別の撮影・録画・OCR を始めたりメインウインドウを出したりすると、
/// 進行中の撮影に写り込むか、screencapture が 2 つ並ぶ (menu_bar.rs)
static CAPTURES_IN_PROGRESS: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);

/// 撮影中の印。生きている間だけ CAPTURES_IN_PROGRESS に数える (エラーやキャンセルで抜けても戻る)
pub(crate) struct CaptureInProgress;

impl CaptureInProgress {
    pub(crate) fn start() -> Self {
        CAPTURES_IN_PROGRESS.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        CaptureInProgress
    }
}

impl Drop for CaptureInProgress {
    fn drop(&mut self) {
        CAPTURES_IN_PROGRESS.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
    }
}

pub(crate) fn is_capture_in_progress() -> bool {
    CAPTURES_IN_PROGRESS.load(std::sync::atomic::Ordering::SeqCst) > 0
}

#[tauri::command]
async fn take_screenshot_interactive(
    app: tauri::AppHandle,
) -> Result<ScreenshotResult, String> {
    let _capturing = CaptureInProgress::start();
    let file_path = get_screenshot_path(&app)?;

    capture_screen_to(&app, &file_path, None).await?;

    let result = load_image_result(file_path)?;
    auto_copy::copy_after_capture(&app, &result);
    resize_window_for_image(&app, result.width, result.height);
    Ok(result)
}

/// 画面を撮って file_path に PNG で書く (macOS: screencapture の対話選択)
///
/// delay_seconds があればタイマー付き (-T)。ユーザーが範囲選択を Esc で取り消すと
/// screencapture は非 0 で終わるので、"cancelled" を含むエラーにする
/// (フロントはこの文字列でキャンセルとエラーを見分ける)。
#[cfg(target_os = "macos")]
async fn capture_screen_to(
    app: &tauri::AppHandle,
    file_path: &str,
    delay_seconds: Option<u32>,
) -> Result<(), String> {
    let mut args = vec!["-i".to_string()];
    if get_exclude_shadow(app) {
        args.push("-o".to_string());
    }
    if let Some(delay) = delay_seconds {
        args.extend(["-T".to_string(), delay.to_string()]);
    }
    args.push(file_path.to_string());

    let status = tokio::process::Command::new("screencapture")
        .args(&args)
        .status()
        .await
        .map_err(|e| format!("Failed to run screencapture: {}", e))?;

    if !status.success() {
        return Err("Screenshot was cancelled".to_string());
    }
    Ok(())
}

/// 画面を撮って file_path に PNG で書く (Windows: マウスカーソルのあるモニター全体)
///
/// Windows には screencapture -i に当たる「範囲を選ばせて撮る」コマンドが無いので、
/// モニター 1 枚を丸ごと撮る。範囲はメインウインドウのトリミング (crop) で切り出す。
///
/// 撮る前に少し待つ。フロントの captureScreen() は hide() の完了を待ってから
/// このコマンドを呼ぶが、Windows はウインドウを消すアニメーションが hide() の
/// 戻りより後まで残り、すぐ撮ると消えかけの自分が写り込む。
#[cfg(windows)]
async fn capture_screen_to(
    app: &tauri::AppHandle,
    file_path: &str,
    delay_seconds: Option<u32>,
) -> Result<(), String> {
    const HIDE_SETTLE: std::time::Duration = std::time::Duration::from_millis(300);
    let delay = delay_seconds
        .map(|s| std::time::Duration::from_secs(u64::from(s)))
        .unwrap_or_default()
        .max(HIDE_SETTLE);
    tokio::time::sleep(delay).await;

    // タイマーの間にカーソルを動かしたモニターを撮れるよう、待った後で位置を取る
    let cursor = app
        .cursor_position()
        .map_err(|e| format!("Failed to get the cursor position: {}", e))?;
    let (x, y) = (cursor.x.round() as i32, cursor.y.round() as i32);
    let file_path = PathBuf::from(file_path);

    tauri::async_runtime::spawn_blocking(move || -> Result<(), String> {
        let monitor = xcap::Monitor::from_point(x, y)
            .or_else(|_| {
                // カーソルの位置がどのモニターにも入らない (取り外した直後等) 時は
                // プライマリを撮る
                xcap::Monitor::all()?
                    .into_iter()
                    .find(|m| m.is_primary().unwrap_or(false))
                    .ok_or_else(|| xcap::XCapError::new("No primary monitor"))
            })
            .map_err(|e| format!("Failed to find a monitor to capture: {}", e))?;
        let image = monitor
            .capture_image()
            .map_err(|e| format!("Failed to capture the screen: {}", e))?;

        let mut png = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(image)
            .write_to(&mut png, image::ImageFormat::Png)
            .map_err(|e| format!("Failed to encode the screenshot: {}", e))?;
        // 名前 (flashcap-<秒>.png) が予測できるので、撮影結果も symlink を辿らずに書く
        write_without_following_symlinks(&file_path, &png.into_inner())
            .map_err(|e| format!("Failed to write the screenshot: {}", e))
    })
    .await
    .map_err(|e| format!("Screen capture task failed: {}", e))?
}

/// macOS と Windows 以外は配布していない。コンパイルを通すためだけの実装
#[cfg(not(any(target_os = "macos", windows)))]
async fn capture_screen_to(
    _app: &tauri::AppHandle,
    _file_path: &str,
    _delay_seconds: Option<u32>,
) -> Result<(), String> {
    Err("Screen capture is not supported on this platform".to_string())
}

/// ウィンドウキャプチャー時のドロップシャドウを除外するか（デフォルト true）
#[cfg(target_os = "macos")]
fn get_exclude_shadow(app: &tauri::AppHandle) -> bool {
    app.store("settings.json")
        .ok()
        .and_then(|store| store.get("exclude_shadow"))
        .and_then(|v| v.as_bool())
        .unwrap_or(true)
}

/// 設定からタイマー秒数を取得（デフォルト5秒）
pub(crate) fn get_timer_delay(app: &tauri::AppHandle) -> u32 {
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
    let _capturing = CaptureInProgress::start();
    let file_path = get_screenshot_path(&app)?;

    capture_screen_to(&app, &file_path, Some(get_timer_delay(&app))).await?;

    let result = load_image_result(file_path)?;
    auto_copy::copy_after_capture(&app, &result);
    resize_window_for_image(&app, result.width, result.height);
    Ok(result)
}

/// ユーザーが自分で開いた画像ファイルの実パス
///
/// 保存先ディレクトリの外にある画像でも、**ユーザー自身が開いたもの**は Cmd+S で
/// 元ファイルを上書きできる必要がある。保存先の封じ込め (write_image_within) は
/// 「予測できる名前で書き出す撮影画像」を守るための仕組みで、ユーザーがパスを
/// 指定して開いたファイルまで書けなくする意図は無い。
///
/// 記録するのは canonicalize 済みのパスだけ。symlink やリダイレクトを解決した後の
/// 実体で突き合わせるので、後から同名の symlink を置かれても許可対象は増えない。
#[derive(Default)]
struct OpenedImages {
    inner: std::sync::Mutex<std::collections::VecDeque<PathBuf>>,
}

/// 覚えておく件数の上限。上書き許可を無制限に溜め込まないための蓋で、
/// 「開いた画像を保存する」という使い方には十分な数。
const OPENED_IMAGES_LIMIT: usize = 64;

impl OpenedImages {
    fn remember(&self, path: &Path) {
        // load_image_result は canonicalize に失敗した場合だけ元のパスを返すので、
        // ここでも解決を試みる。実体で突き合わせないと write 側の canonicalize 済み
        // パスと一致せず、上書きが許可されない
        let path = &dunce::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        let Ok(mut paths) = self.inner.lock() else {
            return;
        };
        if paths.iter().any(|p| p == path) {
            return;
        }
        if paths.len() >= OPENED_IMAGES_LIMIT {
            paths.pop_front();
        }
        paths.push_back(path.to_path_buf());
    }

    fn snapshot(&self) -> Vec<PathBuf> {
        self.inner
            .lock()
            .map(|paths| paths.iter().cloned().collect())
            .unwrap_or_default()
    }
}

/// 外部からの画像ファイルを開く
#[tauri::command]
fn load_image_file(
    app: tauri::AppHandle,
    opened: tauri::State<'_, OpenedImages>,
    path: String,
) -> Result<ScreenshotResult, String> {
    let result = load_image_result(path)?;
    // load_image_result が返す file_path は canonicalize 済み。これを覚えておくと
    // 保存先の外にあるファイルでも Cmd+S で上書きできる
    opened.remember(Path::new(&result.file_path));
    resize_window_for_image(&app, result.width, result.height);
    Ok(result)
}

/// symlink を辿らずにファイルを書く
///
/// `std::fs::write` は symlink を辿るので、書き込み先の名前が予測できる場所では
/// 使えない。`O_NOFOLLOW` を付けると、対象が symlink だった時点で ELOOP になる。
#[cfg(unix)]
fn write_without_following_symlinks(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;

    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)?;
    file.write_all(bytes)
}

/// Windows 版。O_NOFOLLOW の代わりに FILE_FLAG_OPEN_REPARSE_POINT で開く
///
/// このフラグで開くと、対象が symlink (reparse point) でもリンク先へは辿らない。
/// ただし「リンク自体を開いて書く」ことになり、Unix のように失敗はしないので、
/// 先に symlink_metadata で見て symlink なら書かずにエラーにする (エラーの形を
/// Unix 版と揃えるため)。検査と open の間に最終要素を差し替えられても、open が
/// リンクを辿らないのでリンク先には書かない。
///
/// **守っているのは最終要素だけ** (Unix の O_NOFOLLOW と同じ範囲)。途中のディレクトリを
/// junction / mount point に差し替えられると、その先へ書くことになる。既定の置き場
/// (%TEMP%\flashcap) はユーザー専用の ACL で他人が差し替えられないことに頼っており、
/// custom: の任意フォルダはそのフォルダの ACL 次第 (残存リスク)。
#[cfg(windows)]
fn write_without_following_symlinks(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    use std::os::windows::fs::OpenOptionsExt;
    const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;

    if let Ok(meta) = std::fs::symlink_metadata(path) {
        if meta.file_type().is_symlink() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!(
                    "{} is a symbolic link; refusing to write through it",
                    path.display()
                ),
            ));
        }
    }
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)?;
    file.write_all(bytes)
}

/// クリップボードから貼り付けた画像を保存する
#[tauri::command]
fn save_pasted_image(
    app: tauri::AppHandle,
    data_base64: String,
    width: usize,
    height: usize,
) -> Result<ScreenshotResult, String> {
    let dir = PathBuf::from(prepare_save_directory(&app)?);
    let timestamp = Local::now().format("%Y%m%d-%H%M%S");
    let filename = format!("flashcap-paste-{}.png", timestamp);
    let file_path = dir.join(&filename).to_string_lossy().to_string();

    let bytes = STANDARD
        .decode(&data_base64)
        .map_err(|e| format!("Failed to decode base64: {}", e))?;
    // O_NOFOLLOW で書く。名前が予測できる (`flashcap-paste-<秒>.png`) ので、
    // 先置きされた symlink を辿ると別のファイルへ書かされる。
    // truncate は残す — 同じ秒に 2 回貼ると同名になるため、上書きは正常系。
    write_without_following_symlinks(Path::new(&file_path), &bytes)
        .map_err(|e| format!("Failed to write file: {}", e))?;

    resize_window_for_image(&app, width, height);

    Ok(ScreenshotResult {
        width,
        height,
        data: data_base64,
        file_path,
    })
}

/// base64 PNG データをファイルに書き出す
/// パスは保存先ディレクトリ内か、ユーザーが自分で開いた画像に制限する
#[tauri::command]
fn write_image_to_file(
    app: tauri::AppHandle,
    opened: tauri::State<'_, OpenedImages>,
    path: String,
    data_base64: String,
) -> Result<(), String> {
    // 注釈済み画像もスクリーンショットと同じ中身なので、撮影と同じ経路で用意する。
    // 併せて、保存先がまだ無い場合に canonicalize が失敗して書き出せない問題も消える
    let save_dir = dunce::canonicalize(prepare_save_directory(&app)?)
        .map_err(|e| format!("Failed to resolve save directory: {}", e))?;
    write_image_within(&save_dir, &opened.snapshot(), &path, &data_base64)
}

/// 保存先ディレクトリの中、またはユーザーが開いた画像に限って base64 の画像を書き出す
///
/// save_dir と opened を引数に取るのは、この判定と書き込みだけを AppHandle から
/// 切り離してテストできるようにするため。実際の呼び出しは write_image_to_file から。
/// save_dir と opened の各要素は canonicalize 済みであることが前提 (呼び出し側で解決する)。
fn write_image_within(
    save_dir: &Path,
    opened: &[PathBuf],
    path: &str,
    data_base64: &str,
) -> Result<(), String> {
    let target = dunce::canonicalize(path)
        .or_else(|_| {
            // ファイルが未作成の場合、親ディレクトリで検証
            std::path::Path::new(path)
                .parent()
                .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidInput, "no parent"))
                .and_then(dunce::canonicalize)
                .map(|p| p.join(std::path::Path::new(path).file_name().unwrap()))
        })
        .map_err(|e| format!("Failed to resolve path: {}", e))?;

    // 保存先の外でも、ユーザーが自分で開いた画像そのものへの上書きは許す
    // (「開いて注釈して Cmd+S で元ファイルを更新する」が使い方の 1 つのため)。
    // 許すのは実体が一致する 1 ファイルだけで、そのディレクトリは開放しない。
    if !target.starts_with(save_dir) && !opened.iter().any(|p| p == &target) {
        return Err(format!(
            "Path '{}' is outside the save directory '{}'",
            target.display(),
            save_dir.display()
        ));
    }

    let bytes = STANDARD
        .decode(data_base64)
        .map_err(|e| format!("Failed to decode base64: {}", e))?;
    // 元ファイルを上書きする場合、中身は PNG のままだと拡張子と食い違う。
    // 拡張子に合わせて詰め直してから書く
    let bytes = encode_for_target_format(&target, bytes)?;
    // O_NOFOLLOW で書く。上の封じ込め検査は **リンク先が存在しない symlink
    // (dangling symlink) をすり抜ける**: canonicalize は ENOENT で失敗し、
    // フォールバックの「親を canonicalize してファイル名を join」がリンクを
    // 解決しないまま save_dir 内のパスを組み立てるため。そこへ std::fs::write を
    // 使うと、リンクを辿って save_dir の外にファイルが作られる。
    // 名前は flashcap-<秒>.png で予測できるので、保存先に書ける相手なら先置きできる
    // (既定の作業ディレクトリは purge されるが、custom: の任意フォルダは締め直さない)。
    // truncate は残す — 注釈済み画像で元ファイルを上書きするのは正常系。
    write_without_following_symlinks(&target, &bytes)
        .map_err(|e| format!("Failed to write file: {}", e))?;
    Ok(())
}

/// JPEG で書き戻す時の品質 (image crate の既定は 75)
///
/// 上書き保存なので、開いて保存するたびに劣化が積み上がる。写真の上に注釈を載せる
/// 使い方では輪郭のリンギングが目に見えるため、既定より高めに取る。
const JPEG_QUALITY: u8 = 92;

/// PNG バイト列を、上書き先の拡張子に合わせた形式へ詰め直す
///
/// フロントが渡してくるのは常に PNG だが、上書き先はユーザーが開いた画像
/// (JPEG / HEIC など) でもありうる。中身と拡張子が食い違うファイルを作らないよう、
/// 書く直前にここで揃える。
///
/// **変換できない拡張子では書かずにエラーにする。** PNG のまま書くと、拡張子は
/// 元のままで中身だけ差し替わった開けないファイルが残り、しかも上書きなので原本が
/// 戻せない。FlashCap が開ける形式 (SUPPORTED_IMAGE_EXTENSIONS) はすべて変換先が
/// あるので、この分岐に来るのは想定外の拡張子だけ。
fn encode_for_target_format(target: &Path, png_bytes: Vec<u8>) -> Result<Vec<u8>, String> {
    let ext = target
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase())
        .unwrap_or_default();

    match ext.as_str() {
        "png" | "" => Ok(png_bytes),
        // image crate は HEIC を書けないので、読み込みと同じく sips に任せる
        // (sips のフォーマット名は .heif に対しても "heic")
        "heic" | "heif" => convert_png_to_heic(&png_bytes),
        _ => {
            let format = image::ImageFormat::from_extension(&ext)
                .filter(image::ImageFormat::writing_enabled)
                .ok_or_else(|| {
                    format!("Saving as '{}' is not supported (copy the image or save it as PNG instead)", ext)
                })?;
            let img = image::load_from_memory(&png_bytes)
                .map_err(|e| format!("Failed to decode image: {}", e))?;
            // 色深度・チャンネル数をエンコーダーが必ず扱える形に揃える。
            // 元画像がグレースケールや 16bit のまま来ることがあり、渡す形式によっては
            // Unsupported で落ちる。JPEG だけはアルファを扱えないので RGB8 にする
            let mut out = std::io::Cursor::new(Vec::new());
            if format == image::ImageFormat::Jpeg {
                // 既定の品質 (75) だと、上書きのたびに写真と注釈の輪郭が目に見えて劣化する
                image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, JPEG_QUALITY)
                    .encode_image(&image::DynamicImage::ImageRgb8(img.to_rgb8()))
                    .map_err(|e| format!("Failed to encode {}: {}", ext, e))?;
            } else {
                image::DynamicImage::ImageRgba8(img.to_rgba8())
                    .write_to(&mut out, format)
                    .map_err(|e| format!("Failed to encode {}: {}", ext, e))?;
            }
            Ok(out.into_inner())
        }
    }
}

/// sips で PNG を HEIC へ変換する (macOS 専用。convert_heic_to_png の逆方向)
#[cfg(target_os = "macos")]
fn convert_png_to_heic(png_bytes: &[u8]) -> Result<Vec<u8>, String> {
    let work = create_private_workdir("encode", "source.png")?;
    std::fs::write(&work.path, png_bytes)
        .map_err(|e| format!("Failed to write the temporary PNG: {}", e))?;

    let converted = work.path.with_file_name("converted.heic");
    let output = Command::new("sips")
        .args(["-s", "format", "heic"])
        .arg(&work.path)
        .arg("--out")
        .arg(&converted)
        .output()
        .map_err(|e| format!("Failed to run sips: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("sips conversion failed: {}", stderr));
    }

    std::fs::read(&converted).map_err(|e| format!("Failed to read the converted image: {}", e))
}

#[cfg(not(target_os = "macos"))]
fn convert_png_to_heic(_png_bytes: &[u8]) -> Result<Vec<u8>, String> {
    Err("Saving as HEIC / HEIF is only supported on macOS".to_string())
}

/// flashcap://ocr でヘッドレス OCR が要求されたか。
///
/// URL は setup の後に `RunEvent::Opened` で届くため、コールド起動では「フロントの
/// 描画完了を待ってウィンドウを表示する」通常の初期化と OCR が並走する。撮影中に
/// ウィンドウが出てくると、それがそのまま撮影範囲に写り込む。
///
/// 起動中に届いた場合の隠す処理 (Opened 側の hide) と対になっていて、こちらは
/// 「これから出てくるのを止める」役割。FrontendHandshake と違って読むだけなので、
/// 状態管理を足さず static で持つ。
static HEADLESS_OCR_REQUESTED: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

fn headless_ocr_requested() -> bool {
    HEADLESS_OCR_REQUESTED.load(std::sync::atomic::Ordering::SeqCst)
}

/// フロント (WebView) の準備待ちハンドシェイク状態
///
/// キャプチャー開始経路 (--capture コールド / single-instance 再起動 /
/// macOS の flashcap://capture) は、いずれも「ウィンドウは show せず do-capture のみ送り、
/// 表示は captureScreen() の撮影完了後 show に一任する」設計に統一している。
/// ただしコールド起動 (WebView 未ロード) では do-capture が登録前のリスナーに届かず
/// 取りこぼすため、frontend-ready 受信を待ってから emit する必要がある。
///
/// **ファイルを開く経路 (Finder の「このアプリケーションで開く」/ Dock へのドロップ) も
/// 同じ取りこぼしをする。** macOS は起動直後に application:openURLs: を送るため、
/// アプリが起動していない状態から開くと open-file が必ず捨てられる。だからここは
/// キャプチャー専用ではなく「フロントが用意できるまで仕事を預かる場所」として持つ。
///
/// frontend_ready と待ち行列を別々の Atomic / Mutex に分けると
/// 「request 側が ready=false を見る → mark_ready 側が ready=true にして待ち行列が空だと見る
///  → request 側が待ち行列に積む」の順で通知が永久に飛ばない lost-wakeup が起きうるため、
/// 1 つの Mutex で原子的に判定する。
#[derive(Default)]
struct FrontendHandshake {
    inner: std::sync::Mutex<HandshakeInner>,
}

#[derive(Default)]
struct HandshakeInner {
    frontend_ready: bool,
    /// frontend-ready より前に予約された撮影。後から来た予約が勝つ
    capture_pending: Option<CaptureKind>,
    /// frontend-ready より前に届いた「開くべき画像」のパス
    pending_files: Vec<String>,
    /// frontend-ready より前に届いた「撮影ボタンを点滅させよ」(Windows の flashcap://capture)
    reactivate_pending: bool,
}

/// 撮影の種類
///
/// do-capture の payload として、フロントの `captureScreen()` に実行させる
/// コマンド名を渡す。**予約 (capture_pending) にも種類ごと持たせる。** メニューバーの
/// 「タイマー付きで撮影」がコールド起動の直後に押されると、種類を持たない予約では
/// 通常の撮影に化けるため
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CaptureKind {
    Interactive,
    Timer,
}

impl CaptureKind {
    fn command(self) -> &'static str {
        match self {
            CaptureKind::Interactive => "take_screenshot_interactive",
            CaptureKind::Timer => "take_screenshot_timer",
        }
    }
}

/// frontend-ready 受信時に取り出す、溜まっていた仕事
struct PendingWork {
    capture: Option<CaptureKind>,
    files: Vec<String>,
    reactivate: bool,
}

impl FrontendHandshake {
    /// ロックを取得する。poison (ロック保持中の panic) しても回復して継続する。
    /// GUI アプリではここで panic 連鎖させてプロセスを落とすより、状態を読めるだけ
    /// 読んで進む方が被害が小さい。
    fn lock(&self) -> std::sync::MutexGuard<'_, HandshakeInner> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// frontend-ready 受信を記録し、溜まっていた仕事を取り出す (取り出したぶんは消す)。
    fn mark_ready(&self) -> PendingWork {
        let mut s = self.lock();
        s.frontend_ready = true;
        PendingWork {
            capture: s.capture_pending.take(),
            files: std::mem::take(&mut s.pending_files),
            reactivate: std::mem::take(&mut s.reactivate_pending),
        }
    }

    /// 撮影ボタンの点滅 (reactivate) を要求する。frontend が ready 済みなら true
    /// (即 show + emit すべき)、未 ready なら予約だけして false を返す
    /// (frontend-ready 受信時の表示に続けて emit される)。
    fn request_reactivate(&self) -> bool {
        let mut s = self.lock();
        if s.frontend_ready {
            true
        } else {
            s.reactivate_pending = true;
            false
        }
    }

    /// キャプチャーを要求する。frontend が ready 済みなら true (即 emit すべき)、
    /// 未 ready なら予約だけして false を返す (frontend-ready 受信時に emit される)。
    fn request_capture(&self, kind: CaptureKind) -> bool {
        let mut s = self.lock();
        if s.frontend_ready {
            true
        } else {
            s.capture_pending = Some(kind);
            false
        }
    }

    /// 画像を開くよう要求する。frontend が ready 済みなら渡されたパスをそのまま返し
    /// (= 呼び出し側が即 emit する)、未 ready なら預かって None を返す。
    fn request_open_files(&self, files: Vec<String>) -> Option<Vec<String>> {
        let mut s = self.lock();
        if s.frontend_ready {
            Some(files)
        } else {
            s.pending_files.extend(files);
            None
        }
    }

    /// frontend-ready より前にキャプチャーを予約する (--capture コールド起動用)。
    fn set_capture_pending(&self) {
        self.lock().capture_pending = Some(CaptureKind::Interactive);
    }

    fn is_ready(&self) -> bool {
        self.lock().frontend_ready
    }

    fn is_capture_pending(&self) -> bool {
        self.lock().capture_pending.is_some()
    }

    fn is_open_pending(&self) -> bool {
        !self.lock().pending_files.is_empty()
    }
}

/// フロントの描画が終わっているか (frontend-ready を受け取ったか)
pub(crate) fn is_frontend_ready(app: &tauri::AppHandle) -> bool {
    app.state::<FrontendHandshake>().is_ready()
}

/// フロントエンドにキャプチャー開始 (do-capture) を通知する。
/// ウィンドウは show しない (撮影完了後に captureScreen() が show する)。
/// frontend が未 ready の場合 (コールド起動) は予約だけ行い、frontend-ready 受信時に emit する。
pub(crate) fn request_capture(app: &tauri::AppHandle, kind: CaptureKind) {
    if app.state::<FrontendHandshake>().request_capture(kind) {
        let _ = app.emit("do-capture", kind.command());
    }
}

/// フロントエンドに画像を開かせる (open-file)。
///
/// **未 ready の時は show しない。** WebView が描画を始める前に表示しても白いウィンドウが
/// 見えるだけなので、表示は frontend-ready 側の show に一任する。預けたパスは
/// frontend-ready 受信時に emit される。
///
/// (キャプチャー経路が show しない理由は別で、あちらは撮影前の hide と重なって
///  show→hide の点滅になるのを避けている。open 経路に撮影前の hide は無い)
fn request_open_files(app: &tauri::AppHandle, files: Vec<String>) {
    if files.is_empty() {
        return;
    }
    if let Some(files) = app.state::<FrontendHandshake>().request_open_files(files) {
        if let Some(w) = app.get_webview_window("main") {
            let _ = w.show();
            let _ = w.set_focus();
        }
        let _ = app.emit("open-file", files);
    }
}

/// FlashCap が開ける画像ファイルの拡張子か
fn is_supported_image_path(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| SUPPORTED_IMAGE_EXTENSIONS.contains(&e.to_lowercase().as_str()))
        .unwrap_or(false)
}

/// コールド起動の argv から「開くべき画像」を決める
///
/// **`--capture` が入っていれば画像は無視する。** single-instance 経路は
/// `request_capture()` を呼んだ時点で return してファイル引数を見ないので、
/// こちらだけ両方処理すると `flashcap --capture foo.png` の結果が起動状態で変わる
/// (コールドだけ open-file と do-capture が同時に飛び、loadImageFile() と
/// captureScreen() が並走して、後に終わった方が表示を上書きする)。
fn image_args_for_startup<I: IntoIterator<Item = String>>(args: I) -> Vec<String> {
    let args: Vec<String> = args.into_iter().collect();
    if args.iter().any(|a| is_capture_arg(a) || is_capture_url(a)) {
        return Vec::new();
    }
    collect_image_args(args)
}

/// argv のこの要素が「撮影を始めよ」の指示か
///
/// **`--capture` だけ。** ローカルのショートカットやランチャーから渡すもので、
/// Web ページからは渡せない。`flashcap://capture` は `is_capture_url` で別に判定する。
fn is_capture_arg(arg: &str) -> bool {
    arg == "--capture"
}

/// argv のこの要素が `flashcap://capture` か
///
/// macOS は URL スキームを `RunEvent::Opened` で渡すので argv には来ないが、
/// Windows は URL を開くとその URL を引数にして flashcap.exe を起動する
/// (起動中なら single-instance が既存のインスタンスへ転送する)。
///
/// **これでは撮影を始めない (CYBERNEURA-DEV-852)。** Windows 版の撮影は
/// モニター全体の即時撮影で、macOS の `screencapture -i` のような
/// ユーザーの操作を挟まない。URL は任意の Web ページやメールのリンクから開けるので、
/// ここで撮影すると外部から非対話で画面を撮らせられる。ウインドウを前に出して
/// 撮影ボタンを点滅させるだけにし、撮影はユーザーのボタン押下に任せる。
fn is_capture_url(arg: &str) -> bool {
    tauri::Url::parse(arg)
        .map(|url| url.scheme() == "flashcap" && url.host_str() == Some("capture"))
        .unwrap_or(false)
}

/// コマンドライン引数から、実在する対応画像ファイルだけを取り出す
///
/// `--capture` のようなフラグや OS が足す引数 (`-psn_0_...` 等) は拡張子を持たないので
/// 拡張子の判定だけで落ちる。実在確認はその先の話で、`.png` で終わるが存在しないパスを
/// フロントへ渡さないためのもの (渡しても `load_image_file` が失敗するだけ)
fn collect_image_args<I: IntoIterator<Item = String>>(args: I) -> Vec<String> {
    args.into_iter()
        .filter(|a| {
            let path = Path::new(a);
            path.exists() && is_supported_image_path(path)
        })
        .collect()
}

/// プリファレンスウィンドウを開く (フロントの Ctrl+, から)
///
/// macOS はアプリメニューの Preferences... (⌘,) から開くが、macOS 以外にはアプリメニューを
/// 置かないので (setup 参照)、ショートカットをフロントで拾ってここを呼ぶ
#[tauri::command]
fn open_preferences(app: tauri::AppHandle) -> Result<(), String> {
    open_preferences_window(&app).map_err(|e| format!("Failed to open Preferences: {}", e))
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

/// macOS のアプリメニュー (FlashCap / Edit / Window)
///
/// macOS 以外には置かない。Windows ではメニューがウインドウの中にメニューバーとして
/// 付き、Edit の Ctrl+C / Ctrl+V / Ctrl+Z がアクセラレーターとして WebView より先に
/// 奪われる (画像のコピーや undo をフロントの keydown で処理しているため、効かなくなる)。
#[cfg(target_os = "macos")]
fn set_app_menu(handle: &tauri::AppHandle) -> tauri::Result<()> {
    let preferences = MenuItem::with_id(
        handle,
        "preferences",
        "Preferences...",
        true,
        Some("CmdOrCtrl+,"),
    )?;

    let app_submenu = Submenu::with_items(
        handle,
        handle.package_info().name.clone(),
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
    handle.set_menu(menu)?;
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(video::RecordingState::default())
        .manage(FrontendHandshake::default())
        .manage(OpenedImages::default())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_drag::init())
        .plugin(tauri_plugin_store::Builder::new().build())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_deep_link::init())
        .plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
            // --capture-screen-text: 既存インスタンスでヘッドレス OCR 実行
            if args.iter().any(|a| a == "--capture-screen-text") {
                let handle = app.clone();
                tauri::async_runtime::spawn(async move {
                    ocr::run_headless_ocr(&handle, false).await;
                });
                return;
            }
            // --capture: 再起動時に点滅させず、そのままキャプチャーを開始する
            if args.iter().any(|a| is_capture_arg(a)) {
                request_capture(app, CaptureKind::Interactive);
                return;
            }
            // flashcap://capture (Windows): 撮影はせず、前に出して撮影ボタンを点滅させる。
            // 理由は is_capture_url のコメント。
            // **撮影中 (タイマーのカウントダウンや hide 後の待ちを含む) は何もしない。**
            // ここで show すると撮影中のモニター全体に自分が写り込む。URL は外から
            // いつでも開けるので、メニューバーの経路 (menu_bar.rs) と同じく弾く。
            // コールド起動の途中 (frontend-ready 前) に届いた時は、未描画のウインドウを出さず
            // リスナーも無いので、予約して frontend-ready 側の表示に任せる
            if args.iter().any(|a| is_capture_url(a)) {
                if is_capture_in_progress() {
                    return;
                }
                if app.state::<FrontendHandshake>().request_reactivate() {
                    if let Some(w) = app.get_webview_window("main") {
                        let _ = w.show();
                        let _ = w.set_focus();
                    }
                    let _ = app.emit("reactivate", ());
                }
                return;
            }
            // 既に起動中のインスタンスに対して再度起動コマンドが来た場合
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.show();
                let _ = w.set_focus();
            }
            // args[0] はバイナリパス。args[1..] がファイルパス
            let file_paths = collect_image_args(args.iter().skip(1).cloned());
            if !file_paths.is_empty() {
                request_open_files(app, file_paths);
            } else {
                let _ = app.emit("reactivate", ());
            }
        }))
        .setup(|app| {
            // macOS ネイティブメニュー
            #[cfg(target_os = "macos")]
            set_app_menu(app.handle())?;

            app.on_menu_event(move |app, event| {
                if event.id() == "preferences" {
                    let _ = open_preferences_window(app);
                } else {
                    menu_bar::handle_menu_event(app, event.id().as_ref());
                }
            });

            // --capture-screen-text: ヘッドレス OCR モード
            let headless_ocr = std::env::args().any(|a| a == "--capture-screen-text");
            if headless_ocr {
                let handle = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    ocr::run_headless_ocr(&handle, true).await;
                });
            } else if let Err(e) = menu_bar::sync(app.handle()) {
                // メニューバーに常駐する設定なら、ここでアイコンを置く。
                // ヘッドレス OCR は終わり次第プロセスごと終了するので、置いても一瞬で消えるだけ。
                // 置けなくてもウインドウからは全部操作できるので、起動は止めない
                eprintln!("[flashcap] {}", e);
            }

            // 通常起動時はメインウィンドウを表示してアクティブにする
            // (ウィンドウは visible:false で生成される)
            // setup 段階で show() すると WebView 未描画の白いウィンドウが
            // 一瞬見えてしまうため、フロントの描画完了 (frontend-ready) を
            // 待ってから表示する。frontend-ready は WebView の JS ロード後に
            // 一度だけ emit される。
            // ヘッドレス OCR 時は表示しない
            if !headless_ocr {
                let handle = app.handle().clone();

                // --capture コールド起動: frontend-ready 受信前にキャプチャーを予約しておく。
                // (macOS の URL スキーム capture のコールド起動は RunEvent::Opened →
                //  request_capture が同じ予約を行う。Windows の flashcap://capture は argv で
                //  来るが、is_capture_arg に当たらないので予約せず、通常の起動として表示し、
                //  起動中に受けた時と同じく撮影ボタンを点滅させる)
                if std::env::args().any(|a| is_capture_arg(&a)) {
                    app.state::<FrontendHandshake>().set_capture_pending();
                }
                if std::env::args().any(|a| is_capture_url(&a)) {
                    // まだ ready ではないので必ず予約になる
                    app.state::<FrontendHandshake>().request_reactivate();
                }

                // コールド起動の引数で渡された画像 (ターミナルからの `flashcap foo.png`)。
                // 起動中に同じコマンドを叩くと single-instance 側が argv で受けて開けるのに、
                // コールド起動だけ無視していたので揃える (--capture との優先順位も
                // image_args_for_startup が single-instance 経路に合わせている)。
                //
                // Finder の「このアプリケーションで開く」はここには来ない。起動中でも
                // コールド起動でも LaunchServices は openURLs を送るので (起動中のアプリに
                // 2 個目のプロセスは立たない)、あちらは RunEvent::Opened 側で受ける
                request_open_files(&handle, image_args_for_startup(std::env::args().skip(1)));

                let handle_cb = handle.clone();
                app.once_any("frontend-ready", move |_| {
                    let work = handle_cb.state::<FrontendHandshake>().mark_ready();
                    // コールド起動中に「このアプリケーションで開く」で預かった画像。
                    // ウィンドウの表示は下のキャプチャー判定と同じ経路に任せる
                    if !work.files.is_empty() {
                        let _ = handle_cb.emit("open-file", work.files);
                    }
                    if let Some(kind) = work.capture {
                        // キャプチャーが予約済み: do-capture を送る。
                        // captureScreen 側は撮影前に hide を呼ぶが、ウィンドウは
                        // visible:false のままなので hide は no-op。撮影完了後に
                        // show される。ここではウィンドウを表示しない。
                        let _ = handle_cb.emit("do-capture", kind.command());
                    } else if !headless_ocr_requested() {
                        // 通常起動: フロント描画完了を待って表示する。
                        // ヘッドレス OCR 中は出さない (撮影に写り込むため)。
                        if let Some(w) = handle_cb.get_webview_window("main") {
                            let _ = w.show();
                            let _ = w.set_focus();
                        }
                        // flashcap://capture のコールド起動 (Windows) か、起動途中に
                        // single-instance で届いた場合: 撮影はせず撮影ボタンを点滅させる。
                        // reactivate のリスナーは frontend-ready より先に登録される
                        if work.reactivate {
                            let _ = handle_cb.emit("reactivate", ());
                        }
                    }
                });

                // フェイルセーフ: frontend-ready が一定時間来ない場合
                // (WebView の JS ロード失敗・onMount 到達前の例外等) は
                // ウィンドウが永久に非表示のままになるため、強制的に表示する。
                // ただしキャプチャー予約中 (--capture / macOS の flashcap://capture のコールド起動)
                // は表示しない。低速環境で frontend-ready が2秒を超えてから届くと
                // show → captureScreen の hide で点滅するため。予約中に frontend が
                // 永久に来ない場合はそもそも captureScreen が動かずキャプチャー不能なので、
                // 空ウィンドウを出しても無意味。
                // **画像の預かり中も同じ理由で表示しない。** request_open_files() が
                // 「未 ready なら show しない」としているのに、ここで出してしまうと
                // 低速環境で白いウィンドウが 2 秒時点で出てから画像が入ることになり、
                // ハンドシェイクで避けたかった見え方をこちらが作ってしまう。
                // frontend が永久に来なければ open-file も処理できないので、
                // 空ウィンドウを出す意味が無いのもキャプチャーと同じ。
                // ヘッドレス OCR 中も同様に表示しない (撮影に写り込むため。OCR は
                // フロントを使わないので、ウィンドウが出ないままでも困らない)。
                tauri::async_runtime::spawn(async move {
                    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                    let handshake = handle.state::<FrontendHandshake>();
                    if !handshake.is_ready()
                        && !handshake.is_capture_pending()
                        && !handshake.is_open_pending()
                        && !headless_ocr_requested()
                    {
                        if let Some(w) = handle.get_webview_window("main") {
                            let _ = w.show();
                            let _ = w.set_focus();
                        }
                    }
                });
            }

            Ok(())
        })
        .on_window_event(|window, event| {
            // メニューバーに常駐している間は、メインウインドウを閉じても終了せず隠すだけにする。
            // 閉じるとウインドウ (= フロントの WebView) ごと破棄され、メニューバーからの
            // 撮影を受け取る相手がいなくなる
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == "main" && menu_bar::is_shown(window.app_handle()) {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![menu_bar::sync_menu_bar, open_preferences, take_screenshot_interactive, take_screenshot_timer, write_image_to_file, load_image_file, open_save_directory, get_default_save_directory, save_pasted_image, ocr::ocr_image, ocr::ocr_capture_region, ocr::show_notification, video::open_region_selector, video::cancel_region_selection, video::release_region_selector_for_countdown, video::broadcast_region_selecting, video::list_capture_windows, video::start_video_recording, video::stop_video_recording, video::export_video, video::check_ffmpeg_available])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| {
            match event {
                tauri::RunEvent::ExitRequested { .. } => {
                    // 録画中にアプリ終了する場合、録画プロセスを停止して孤立を防ぐ
                    video::abort_recording(app);
                }
                #[cfg(target_os = "macos")]
                tauri::RunEvent::Reopen { .. } => {
                    // Dock アイコンクリック時: ウインドウを表示してフォーカス
                    if let Some(w) = app.get_webview_window("main") {
                        let _ = w.show();
                        let _ = w.set_focus();
                    }
                    // キャプチャーボタンを点滅させて目立たせる
                    let _ = app.emit("reactivate", ());
                }
                #[cfg(target_os = "macos")]
                tauri::RunEvent::Opened { urls } => {
                    // flashcap:// URL scheme によるコマンド実行
                    for url in &urls {
                        if url.scheme() != "flashcap" {
                            continue;
                        }
                        match url.host_str() {
                            // flashcap://ocr: ヘッドレス OCR を実行
                            Some("ocr") => {
                                // ヘッドレス OCR は画面を撮るので、メインウィンドウが
                                // 出ていると撮影範囲に自分が写り込む。出さないための
                                // 経路が 2 つあり、両方要る:
                                // - 既に起動している場合、URL を開くと macOS がアプリを
                                //   前面に出すので、今出ているものを hide する
                                // - コールド起動の場合、ウィンドウはまだ visible:false
                                //   なので hide は no-op。代わりにフラグを立てて、
                                //   frontend-ready 側の show を止める
                                HEADLESS_OCR_REQUESTED
                                    .store(true, std::sync::atomic::Ordering::SeqCst);
                                if let Some(w) = app.get_webview_window("main") {
                                    let _ = w.hide();
                                }
                                let handle = app.clone();
                                tauri::async_runtime::spawn(async move {
                                    ocr::run_headless_ocr(&handle, false).await;
                                });
                                return;
                            }
                            // flashcap://capture: キャプチャーを開始する。
                            // コールド起動時は frontend-ready を待ってから emit される
                            // (request_capture 内のハンドシェイクで取りこぼしを防ぐ)。
                            Some("capture") => {
                                request_capture(app, CaptureKind::Interactive);
                                return;
                            }
                            _ => {}
                        }
                    }

                    // ファイル関連付け (Finder の「このアプリケーションで開く」) や
                    // Dock へのドロップで開かれた場合。
                    //
                    // **コールド起動では、ここはまだ WebView が JS をロードする前に来る。**
                    // 直接 emit すると届け先が無く捨てられるので、request_open_files に
                    // 預けて frontend-ready 受信時に送らせる
                    let file_paths: Vec<String> = urls.iter()
                        .filter_map(|url| {
                            if url.scheme() != "file" {
                                return None;
                            }
                            let path = url.to_file_path().ok()?;
                            // 対応する画像拡張子のみ許可
                            if is_supported_image_path(&path) {
                                Some(path.to_string_lossy().to_string())
                            } else {
                                None
                            }
                        })
                        .collect();
                    request_open_files(app, file_paths);
                }
                _ => {}
            }
        });
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    /// テスト用の一時ディレクトリ。Drop で消す。
    struct TempDir(std::path::PathBuf);

    impl TempDir {
        fn new(tag: &str) -> Self {
            // 同時実行しても衝突しないよう、プロセス id とタグで分ける
            let base = std::env::temp_dir().join(format!(
                "flashcap-test-{}-{}-{tag}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            create_private_dir(&base).expect("failed to create test dir");
            Self(base)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn chmod(path: &Path, mode: u32) {
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode))
            .expect("chmod failed");
    }

    #[test]
    fn purge_removes_symlinks_and_foreign_entries() {
        let tmp = TempDir::new("purge");
        // 自分が置いた通常ファイルは残す
        std::fs::write(tmp.path().join("mine.png"), b"x").unwrap();
        // 先置きされた symlink は、リンク先を見ずに消す
        std::os::unix::fs::symlink("/etc/passwd", tmp.path().join("evil.png")).unwrap();

        let removed = purge_foreign_entries(tmp.path()).unwrap();
        assert!(removed, "symlink があるのに消していない");
        assert!(tmp.path().join("mine.png").exists());
        assert!(
            std::fs::symlink_metadata(tmp.path().join("evil.png")).is_err(),
            "symlink が残っている"
        );
        // リンク先は消していない
        assert!(Path::new("/etc/passwd").exists());
    }

    #[test]
    fn purge_reports_false_when_nothing_to_remove() {
        let tmp = TempDir::new("purge-clean");
        std::fs::write(tmp.path().join("mine.png"), b"x").unwrap();
        assert!(!purge_foreign_entries(tmp.path()).unwrap());
        assert!(tmp.path().join("mine.png").exists());
    }

    #[test]
    fn write_refuses_to_follow_a_symlink() {
        let tmp = TempDir::new("nofollow");
        let victim = tmp.path().join("victim.txt");
        std::fs::write(&victim, b"original").unwrap();
        let link = tmp.path().join("link.png");
        std::os::unix::fs::symlink(&victim, &link).unwrap();

        let err = write_without_following_symlinks(&link, b"attacker").unwrap_err();
        // O_NOFOLLOW は symlink に当たると ELOOP を返す
        assert!(
            matches!(err.raw_os_error(), Some(libc::ELOOP)),
            "unexpected error: {err:?}"
        );
        // リンク先は書き換わっていない
        assert_eq!(std::fs::read(&victim).unwrap(), b"original");
    }

    #[test]
    fn write_creates_and_overwrites_regular_files() {
        // 同じ秒に 2 回貼ると同名になるので、上書きは正常系として通す必要がある
        let tmp = TempDir::new("overwrite");
        let path = tmp.path().join("shot.png");
        write_without_following_symlinks(&path, b"first").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"first");
        write_without_following_symlinks(&path, b"second").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"second");
    }

    #[test]
    fn open_files_are_held_until_the_frontend_is_ready() {
        // Arrange: コールド起動直後 (WebView 未ロード) の状態
        let handshake = FrontendHandshake::default();

        // Act: Finder の「このアプリケーションで開く」が届く
        let immediate = handshake.request_open_files(vec!["/tmp/a.png".to_string()]);

        // Assert: この場では emit させず、frontend-ready で受け取れる
        assert!(immediate.is_none(), "未 ready なのに即 emit しようとしている");
        let work = handshake.mark_ready();
        assert_eq!(work.files, vec!["/tmp/a.png".to_string()]);
        assert!(work.capture.is_none());
    }

    #[test]
    fn open_files_go_straight_through_once_the_frontend_is_ready() {
        // Arrange
        let handshake = FrontendHandshake::default();
        handshake.mark_ready();

        // Act
        let immediate = handshake.request_open_files(vec!["/tmp/a.png".to_string()]);

        // Assert: 即 emit させ、預かり分としては残さない
        assert_eq!(immediate, Some(vec!["/tmp/a.png".to_string()]));
        assert!(handshake.mark_ready().files.is_empty(), "二重に配送されている");
    }

    #[test]
    fn held_work_is_taken_exactly_once() {
        // Arrange: コールド起動で複数回開かれ、キャプチャーも予約された状態
        let handshake = FrontendHandshake::default();
        handshake.set_capture_pending();
        handshake.request_open_files(vec!["/tmp/a.png".to_string()]);
        handshake.request_open_files(vec!["/tmp/b.png".to_string()]);

        // Act
        let first = handshake.mark_ready();
        let second = handshake.mark_ready();

        // Assert: 溜めた順に 1 度だけ渡り、2 度目は空
        assert_eq!(
            first.files,
            vec!["/tmp/a.png".to_string(), "/tmp/b.png".to_string()]
        );
        assert_eq!(first.capture, Some(CaptureKind::Interactive));
        assert!(second.files.is_empty());
        assert!(second.capture.is_none());
    }

    #[test]
    fn a_reactivate_before_ready_is_held_and_delivered_once() {
        // Arrange: コールド起動の途中に flashcap://capture が届いた (Windows)
        let handshake = FrontendHandshake::default();

        // Act
        let immediate = handshake.request_reactivate();
        let first = handshake.mark_ready();
        let second = handshake.mark_ready();

        // Assert: その場では emit させず、frontend-ready で 1 度だけ渡る
        assert!(!immediate);
        assert!(first.reactivate);
        assert!(!second.reactivate);
        // ready 後は即 emit させ、預かり分としては残さない
        assert!(handshake.request_reactivate());
        assert!(!handshake.mark_ready().reactivate, "二重に配送されている");
    }

    #[test]
    fn a_held_capture_keeps_its_kind() {
        // Arrange: コールド起動の直後にメニューバーの「タイマー付きで撮影」が押された
        let handshake = FrontendHandshake::default();

        // Act
        let immediate = handshake.request_capture(CaptureKind::Timer);

        // Assert: 通常の撮影に化けずに、タイマー付きのまま frontend-ready で渡る
        assert!(!immediate, "未 ready なのに即 emit しようとしている");
        assert!(handshake.is_capture_pending());
        assert_eq!(handshake.mark_ready().capture, Some(CaptureKind::Timer));
        assert!(
            handshake.request_capture(CaptureKind::Interactive),
            "ready 後は即 emit する"
        );
        assert!(
            handshake.mark_ready().capture.is_none(),
            "ready 後の要求を預かっている"
        );
    }

    #[test]
    fn pending_work_is_visible_to_the_startup_fallback() {
        // Arrange: 2 秒フェイルセーフは「預かり中なら表示しない」を判定に使う
        let handshake = FrontendHandshake::default();
        assert!(!handshake.is_open_pending());

        // Act
        handshake.request_open_files(vec!["/tmp/a.png".to_string()]);

        // Assert: 預かっている間は true、取り出したら false に戻る
        assert!(handshake.is_open_pending(), "預かり中が見えていない");
        handshake.mark_ready();
        assert!(!handshake.is_open_pending(), "取り出した後も残っている");
    }

    #[test]
    fn capture_flag_wins_over_image_arguments_on_cold_start() {
        // Arrange: 実在する画像 + --capture を同時に渡す
        let tmp = TempDir::new("precedence");
        let image = tmp.path().join("shot.png");
        std::fs::write(&image, b"x").unwrap();
        let path = image.to_string_lossy().to_string();

        // Act
        let with_capture =
            image_args_for_startup(vec!["--capture".to_string(), path.clone()]);
        let with_capture_url =
            image_args_for_startup(vec!["flashcap://capture".to_string(), path.clone()]);
        let without_capture = image_args_for_startup(vec![path.clone()]);

        // Assert: single-instance 経路と同じ優先順位 (capture ならファイルは見ない)
        assert!(
            with_capture.is_empty(),
            "--capture と同時に画像を開こうとしている: {with_capture:?}"
        );
        assert!(
            with_capture_url.is_empty(),
            "flashcap://capture と同時に画像を開こうとしている: {with_capture_url:?}"
        );
        assert_eq!(without_capture, vec![path]);
    }

    #[test]
    fn the_capture_url_is_not_a_capture_request_like_the_flag() {
        // Windows は flashcap://capture を argv で渡してくる。Web から開ける URL なので、
        // --capture と違って撮影は始めない (CYBERNEURA-DEV-852)
        assert!(is_capture_arg("--capture"));
        assert!(!is_capture_arg("flashcap://capture"));
        assert!(!is_capture_arg("flashcap://capture/"));

        assert!(is_capture_url("flashcap://capture"));
        assert!(is_capture_url("flashcap://capture/"));
        assert!(!is_capture_url("--capture"));
        assert!(!is_capture_url("flashcap://ocr"));
        assert!(!is_capture_url("https://capture"));
        assert!(!is_capture_url("/tmp/capture.png"));

        // URL で起動された時も、argv の画像は開かない (従来どおり)
        assert!(image_args_for_startup(vec!["flashcap://capture".to_string()]).is_empty());
    }

    #[test]
    fn only_supported_image_extensions_are_opened() {
        // Arrange / Act / Assert
        assert!(is_supported_image_path(Path::new("/tmp/a.png")));
        assert!(is_supported_image_path(Path::new("/tmp/a.JPEG")));
        assert!(is_supported_image_path(Path::new("/tmp/a.heic")));
        assert!(!is_supported_image_path(Path::new("/tmp/a.pdf")));
        assert!(!is_supported_image_path(Path::new("/tmp/png")));
    }

    #[test]
    fn command_line_arguments_yield_only_existing_images() {
        // Arrange: フラグ・存在しないパス・非対応拡張子を混ぜる
        let tmp = TempDir::new("args");
        let image = tmp.path().join("shot.png");
        std::fs::write(&image, b"x").unwrap();
        let document = tmp.path().join("note.pdf");
        std::fs::write(&document, b"x").unwrap();
        let missing = tmp.path().join("gone.png");

        // Act
        let picked = collect_image_args(vec![
            "--capture".to_string(),
            image.to_string_lossy().to_string(),
            document.to_string_lossy().to_string(),
            missing.to_string_lossy().to_string(),
        ]);

        // Assert
        assert_eq!(picked, vec![image.to_string_lossy().to_string()]);
    }

    #[test]
    fn create_private_dir_is_0700_regardless_of_umask() {
        let tmp = TempDir::new("mode");
        let nested = tmp.path().join("a/b");
        create_private_dir(&nested).unwrap();
        for dir in [tmp.path().join("a"), nested] {
            let mode = std::fs::metadata(&dir).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o700, "{} は 0700 でない", dir.display());
        }
    }

    #[test]
    fn accepts_a_private_directory() {
        let tmp = TempDir::new("ok");
        // 祖先 (std::env::temp_dir() = 多くの環境で /tmp) が 1777 だと落ちるので、
        // ここでは自分が作った側だけを対象にする。
        if reject_if_others_can_meddle(tmp.path()).is_err() {
            // /tmp が誰でも書ける環境ではこの検査自体が正しく拒否する。
            // その場合は「拒否されること」が期待動作なので、ここで終わる。
            return;
        }
        assert!(reject_if_others_can_meddle(tmp.path()).is_ok());
    }

    #[test]
    fn rejects_a_group_or_other_writable_ancestor() {
        let tmp = TempDir::new("writable");
        let child = tmp.path().join("child");
        create_private_dir(&child).unwrap();

        chmod(tmp.path(), 0o777);
        let err = reject_if_others_can_meddle(&child).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::PermissionDenied);
        assert!(
            err.to_string().contains("writable by other accounts"),
            "unexpected message: {err}"
        );

        // 後始末できるように戻す
        chmod(tmp.path(), 0o700);
    }

    #[test]
    fn rejects_a_group_writable_ancestor_even_without_other_bit() {
        // 0o020 だけでも拒否する (0o022 のうち片方しか立っていないケース)
        let tmp = TempDir::new("group");
        let child = tmp.path().join("child");
        create_private_dir(&child).unwrap();

        chmod(tmp.path(), 0o770);
        let err = reject_if_others_can_meddle(&child).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::PermissionDenied);

        chmod(tmp.path(), 0o700);
    }

    #[test]
    fn accepts_root_owned_ancestors() {
        // `/` は root 所有で 0755。所有者検査で root を許していないと、
        // どんなパスでも必ず落ちる (= 機能が完全に死ぬ) ことの回帰テスト。
        let root = Path::new("/");
        let meta = std::fs::metadata(root).unwrap();
        use std::os::unix::fs::MetadataExt;
        assert_eq!(meta.uid(), 0, "前提: / は root 所有");
        assert_eq!(
            meta.permissions().mode() & 0o022,
            0,
            "前提: / は他人に書けない"
        );
        assert!(reject_if_others_can_meddle(root).is_ok());
    }

    #[test]
    fn reserved_workdir_is_0700_and_holds_the_output_name() {
        let tmp = TempDir::new("workdir");

        // umask を跨いでも 0700 であること。中の出力ファイルはまだ存在せず、
        // 名前だけが予約された状態になっていること。
        let work = reserve_private_workdir(tmp.path(), "heic", "converted.png")
            .expect("failed to reserve workdir");

        let dir = work.path.parent().unwrap().to_path_buf();
        assert_eq!(
            std::fs::metadata(&dir).unwrap().permissions().mode() & 0o777,
            0o700
        );
        assert!(dir.starts_with(tmp.path()));
        assert!(!work.path.exists(), "出力ファイルはまだ作らない");

        // 同じ base に 2 回確保しても衝突しない (別ディレクトリになる)
        let other = reserve_private_workdir(tmp.path(), "heic", "converted.png")
            .expect("failed to reserve second workdir");
        assert_ne!(work.path, other.path);

        drop(other);
        assert!(dir.exists(), "他方の drop で消えてはいけない");
    }

    #[test]
    fn reserved_workdir_is_removed_on_drop() {
        let tmp = TempDir::new("workdir-drop");

        let work = reserve_private_workdir(tmp.path(), "ocr", "capture.png")
            .expect("failed to reserve workdir");
        let dir = work.path.parent().unwrap().to_path_buf();

        // 外部コマンドが出力した後を模して、中身が残っていても消えること
        std::fs::write(&work.path, b"captured").expect("failed to write");
        assert!(dir.exists());

        drop(work);
        assert!(!dir.exists(), "drop で中身ごと消えていない");
    }

    #[test]
    fn write_image_refuses_a_dangling_symlink_planted_in_the_save_dir() {
        // 攻撃の形: 保存先に書ける相手が、予測できる出力名
        // (flashcap-<秒>.png) で外向きの symlink を先に置く。リンク先が
        // **存在しない**ので canonicalize は失敗し、封じ込め検査は
        // 「親を canonicalize してファイル名を join」した save_dir 内のパスを見る。
        // 検査だけでは止まらないので、書き込み側 (O_NOFOLLOW) が最後の砦になる。
        let tmp = TempDir::new("write-dangling");
        let save_dir = std::fs::canonicalize(tmp.path()).unwrap();

        let outside = save_dir.join("outside");
        create_private_dir(&outside).unwrap();
        let victim = outside.join("planted.png");

        let link = save_dir.join("flashcap-20260820-000000.png");
        std::os::unix::fs::symlink(&victim, &link).unwrap();

        let err =
            write_image_within(&save_dir, &[], link.to_str().unwrap(), &STANDARD.encode(b"png"))
                .unwrap_err();
        assert!(err.contains("Failed to write file"), "{}", err);
        assert!(
            !victim.exists(),
            "symlink を辿ってリンク先に書いてしまっている"
        );
    }

    #[test]
    fn write_image_rejects_paths_outside_the_save_dir() {
        let tmp = TempDir::new("write-outside");
        let save_dir = std::fs::canonicalize(tmp.path()).unwrap();

        let other = tmp.path().join("other");
        create_private_dir(&other).unwrap();
        let outside = std::fs::canonicalize(&other).unwrap().join("x.png");

        // save_dir を 1 階層深くして、outside がその外側になるようにする
        let inner = save_dir.join("inner");
        create_private_dir(&inner).unwrap();

        let err =
            write_image_within(&inner, &[], outside.to_str().unwrap(), &STANDARD.encode(b"png"))
                .unwrap_err();
        assert!(err.contains("outside the save directory"), "{}", err);
        assert!(!outside.exists());
    }

    #[test]
    fn write_image_writes_and_overwrites_inside_the_save_dir() {
        let tmp = TempDir::new("write-inside");
        let save_dir = std::fs::canonicalize(tmp.path()).unwrap();
        let target = save_dir.join("flashcap-20260820-000001.png");

        // 未作成のファイル (canonicalize が失敗し、親で検証される経路)
        write_image_within(
            &save_dir,
            &[],
            target.to_str().unwrap(),
            &STANDARD.encode(b"first"),
        )
        .unwrap();
        assert_eq!(std::fs::read(&target).unwrap(), b"first");

        // 注釈済み画像で元ファイルを上書きするのは正常系
        write_image_within(
            &save_dir,
            &[],
            target.to_str().unwrap(),
            &STANDARD.encode(b"second"),
        )
        .unwrap();
        assert_eq!(std::fs::read(&target).unwrap(), b"second");
    }

    #[test]
    fn write_image_overwrites_a_file_the_user_opened_outside_the_save_dir() {
        // 開いた画像に注釈して Cmd+S で元ファイルを上書きする経路。
        // 保存先の外にあっても、開いたファイルそのものなら書ける
        let tmp = TempDir::new("write-opened");
        let save_dir = std::fs::canonicalize(tmp.path()).unwrap().join("save");
        create_private_dir(&save_dir).unwrap();

        let elsewhere = std::fs::canonicalize(tmp.path()).unwrap().join("pictures");
        create_private_dir(&elsewhere).unwrap();
        let opened = elsewhere.join("photo.png");
        std::fs::write(&opened, b"original").unwrap();

        write_image_within(
            &save_dir,
            std::slice::from_ref(&opened),
            opened.to_str().unwrap(),
            &STANDARD.encode(b"annotated"),
        )
        .unwrap();
        assert_eq!(std::fs::read(&opened).unwrap(), b"annotated");
    }

    #[test]
    fn write_image_does_not_open_the_directory_of_an_opened_file() {
        // 許可されるのは開いたファイル 1 つだけ。同じフォルダの別ファイルは書けない
        let tmp = TempDir::new("write-opened-sibling");
        let save_dir = std::fs::canonicalize(tmp.path()).unwrap().join("save");
        create_private_dir(&save_dir).unwrap();

        let elsewhere = std::fs::canonicalize(tmp.path()).unwrap().join("pictures");
        create_private_dir(&elsewhere).unwrap();
        let opened = elsewhere.join("photo.png");
        std::fs::write(&opened, b"original").unwrap();
        let sibling = elsewhere.join("other.png");

        let err = write_image_within(
            &save_dir,
            &[opened],
            sibling.to_str().unwrap(),
            &STANDARD.encode(b"annotated"),
        )
        .unwrap_err();
        assert!(err.contains("outside the save directory"), "{}", err);
        assert!(!sibling.exists());
    }

    /// 1x1 の PNG を作る (エンコード経路のテスト用)
    fn tiny_png() -> Vec<u8> {
        let img = image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
            1,
            1,
            image::Rgba([1, 2, 3, 255]),
        ));
        let mut out = std::io::Cursor::new(Vec::new());
        img.write_to(&mut out, image::ImageFormat::Png).unwrap();
        out.into_inner()
    }

    #[test]
    fn saving_to_a_jpeg_path_writes_jpeg_bytes() {
        // JPEG を開いて上書きした時に、中身が PNG のままにならないこと
        // (注釈済み画像は RGBA なので、アルファを落とさないと encode に失敗する)
        let tmp = TempDir::new("encode-jpeg");
        let target = std::fs::canonicalize(tmp.path()).unwrap().join("photo.jpg");

        let bytes = encode_for_target_format(&target, tiny_png()).unwrap();
        assert_eq!(
            image::guess_format(&bytes).unwrap(),
            image::ImageFormat::Jpeg
        );
    }

    #[test]
    fn saving_to_a_png_path_keeps_the_bytes_untouched() {
        let tmp = TempDir::new("encode-png");
        let target = std::fs::canonicalize(tmp.path()).unwrap().join("shot.png");

        let png = tiny_png();
        assert_eq!(
            encode_for_target_format(&target, png.clone()).unwrap(),
            png,
            "PNG は詰め直さずそのまま書く"
        );
    }

    #[test]
    fn every_openable_extension_can_be_encoded() {
        // 開ける形式はすべて書き戻せること。ここが崩れると、その形式のファイルは
        // 「開けるが保存できない」になる (HEIC は sips 任せなので macOS でしか通らない)
        let tmp = TempDir::new("encode-openable");
        let dir = std::fs::canonicalize(tmp.path()).unwrap();

        for ext in SUPPORTED_IMAGE_EXTENSIONS {
            if *ext == "heic" || *ext == "heif" {
                continue;
            }
            let target = dir.join(format!("photo.{}", ext));
            let bytes = encode_for_target_format(&target, tiny_png())
                .unwrap_or_else(|e| panic!("{} を書き戻せない: {}", ext, e));
            let expected = image::ImageFormat::from_extension(ext).unwrap();
            assert_eq!(
                image::guess_format(&bytes).unwrap(),
                expected,
                "{} の中身が拡張子と食い違う",
                ext
            );
        }
    }

    #[test]
    fn saving_to_an_unknown_extension_is_refused() {
        // PNG のまま書くと、拡張子はそのままで開けないファイルに変わり、
        // 上書きなので原本も戻せない。書かずに失敗させる
        let tmp = TempDir::new("encode-unknown");
        let target = std::fs::canonicalize(tmp.path()).unwrap().join("image.xyz");

        let err = encode_for_target_format(&target, tiny_png()).unwrap_err();
        assert!(err.contains("not supported"), "{}", err);
    }

    #[test]
    fn saving_a_grayscale_image_to_a_lossless_format_succeeds() {
        // 注釈が無い保存では、開いた画像の色形式 (グレースケール等) のまま渡ってくる
        let tmp = TempDir::new("encode-gray");
        let target = std::fs::canonicalize(tmp.path())
            .unwrap()
            .join("photo.webp");

        let gray =
            image::DynamicImage::ImageLuma8(image::GrayImage::from_pixel(1, 1, image::Luma([128])));
        let mut png = std::io::Cursor::new(Vec::new());
        gray.write_to(&mut png, image::ImageFormat::Png).unwrap();

        let bytes = encode_for_target_format(&target, png.into_inner()).unwrap();
        assert_eq!(
            image::guess_format(&bytes).unwrap(),
            image::ImageFormat::WebP
        );
    }

    #[test]
    fn opened_images_dedupes_and_keeps_the_newest_within_the_limit() {
        let tmp = TempDir::new("opened-images");
        let dir = std::fs::canonicalize(tmp.path()).unwrap();

        let opened = OpenedImages::default();
        // canonicalize できないパス (未作成) はそのまま覚える
        let first = dir.join("first.png");
        opened.remember(&first);
        opened.remember(&first);
        assert_eq!(opened.snapshot(), vec![first.clone()]);

        for i in 0..OPENED_IMAGES_LIMIT {
            opened.remember(&dir.join(format!("f{}.png", i)));
        }
        let snapshot = opened.snapshot();
        assert_eq!(snapshot.len(), OPENED_IMAGES_LIMIT);
        assert!(
            !snapshot.contains(&first),
            "上限を超えた分は古い方から落ちる"
        );
        assert!(snapshot.contains(&dir.join(format!("f{}.png", OPENED_IMAGES_LIMIT - 1))));
    }
}

/// Windows 専用の実装 (Unix の mode / O_NOFOLLOW の代わり) のテスト。Windows の CI で走る
#[cfg(all(test, windows))]
mod windows_tests {
    use super::*;

    /// テスト用の一時ディレクトリ。Drop で消す。
    struct TempDir(PathBuf);

    impl TempDir {
        fn new(tag: &str) -> Self {
            let base = std::env::temp_dir().join(format!(
                "flashcap-test-{}-{}-{tag}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            create_private_dir(&base).expect("failed to create test dir");
            Self(base)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn write_creates_and_overwrites_regular_files() {
        let tmp = TempDir::new("overwrite");
        let path = tmp.0.join("shot.png");
        write_without_following_symlinks(&path, b"first").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"first");
        write_without_following_symlinks(&path, b"second").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"second");
    }

    #[test]
    fn write_refuses_to_follow_a_symlink() {
        let tmp = TempDir::new("nofollow");
        let victim = tmp.0.join("victim.txt");
        std::fs::write(&victim, b"original").unwrap();
        let link = tmp.0.join("link.png");
        // symlink の作成には権限 (管理者か開発者モード) が要る。作れない環境では
        // 攻撃側も作れないので、検査するものが無い
        if std::os::windows::fs::symlink_file(&victim, &link).is_err() {
            eprintln!("skipped: this account cannot create symbolic links");
            return;
        }

        assert!(write_without_following_symlinks(&link, b"attacker").is_err());
        assert_eq!(std::fs::read(&victim).unwrap(), b"original");
    }

    #[test]
    fn the_working_directory_can_be_prepared() {
        let dir = ensure_private_flashcap_dir().expect("failed to prepare the working directory");
        assert!(dir.is_dir());
        assert_eq!(dir, flashcap_temp_dir());
    }

    #[test]
    fn reserved_workdir_is_removed_on_drop() {
        let tmp = TempDir::new("workdir");
        let work = reserve_private_workdir(&tmp.0, "heic", "converted.png")
            .expect("failed to reserve workdir");
        let dir = work.path.parent().unwrap().to_path_buf();
        assert!(dir.is_dir());
        assert!(!work.path.exists(), "出力ファイルはまだ作らない");

        let other = reserve_private_workdir(&tmp.0, "heic", "converted.png")
            .expect("failed to reserve second workdir");
        assert_ne!(work.path, other.path);

        drop(work);
        assert!(!dir.exists(), "drop で消えていない");
    }

    #[test]
    fn the_os_screenshot_directory_is_an_absolute_path() {
        assert!(Path::new(&get_os_screenshot_dir()).is_absolute());
    }

    #[test]
    fn heic_is_refused_instead_of_being_written_as_png() {
        // 拡張子と中身が食い違うファイルを作らない (上書きなので原本が戻せなくなる)
        let err = encode_for_target_format(Path::new("C:\\photo.heic"), Vec::new()).unwrap_err();
        assert!(err.contains("only supported on macOS"), "{}", err);
    }
}
