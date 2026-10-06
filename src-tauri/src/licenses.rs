//! 依存ライブラリのライセンス表示 (Third-Party Licenses)
//!
//! リポジトリ直下の THIRD-PARTY-NOTICES.txt (`pnpm notices` =
//! scripts/generate-third-party-notices.sh の生成物) をビルド時に埋め込み、
//! `licenses` ウインドウ (`src/routes/licenses/+page.svelte`) に出す。
//! 開く入口は macOS のアプリメニュー (About の直下) と、Preferences の About 節のボタン
//! (Windows にはアプリメニューが無いため)。

use tauri::{Manager, WebviewUrl, WebviewWindowBuilder};

/// アプリメニューの項目 ID (lib.rs の `app.on_menu_event` で振り分ける)
pub(crate) const MENU_ID: &str = "third-party-licenses";

/// 配布物に含まれる依存ライブラリのライセンス一覧
const THIRD_PARTY_NOTICES: &str = include_str!("../../THIRD-PARTY-NOTICES.txt");

/// ウインドウに流し込む本文
#[tauri::command]
pub fn third_party_notices() -> &'static str {
    THIRD_PARTY_NOTICES
}

/// Preferences のボタンから開く。
/// async にしておく: 同期コマンドの中でウインドウを作ると Windows でデッドロックする
/// (WebviewWindowBuilder のドキュメント)
#[tauri::command]
pub async fn open_third_party_licenses(app: tauri::AppHandle) -> Result<(), String> {
    open_window(&app).map_err(|e| format!("Failed to open Third-Party Licenses: {}", e))
}

/// Third-Party Licenses ウインドウを開く (既に開いていればフォーカス)。
/// 作り方は Preferences と同じ
pub(crate) fn open_window(app: &tauri::AppHandle) -> tauri::Result<()> {
    if let Some(window) = app.get_webview_window("licenses") {
        let _ = window.set_focus();
        return Ok(());
    }

    WebviewWindowBuilder::new(app, "licenses", WebviewUrl::App("/licenses".into()))
        .title("Third-Party Licenses")
        .inner_size(640.0, 560.0)
        .min_inner_size(400.0, 300.0)
        .resizable(true)
        .theme(Some(tauri::Theme::Dark))
        // ページ (bg-[#1a1a1a]) が描かれる前の白い画面を出さない
        .background_color(tauri::window::Color(0x1a, 0x1a, 0x1a, 0xff))
        .center()
        .build()?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 配布物に入る直接依存の crate 名を Cargo.toml から拾う。`[dependencies]` と
    /// `[target.'cfg(..)'.dependencies]` の `name = ...` の 1 行書式だけを見る
    /// (`[dependencies.foo]` 形式は拾えない)。build / dev 依存は配布物に入らないので除く。
    /// 配布しないターゲット (Linux) 専用の target 依存を足すと、about.toml の targets の
    /// 外なので notices に載らず、このテストが落ちる。その時はここで除外する。
    fn direct_rust_dependencies(manifest: &str) -> Vec<String> {
        let mut section = String::new();
        let mut names = Vec::new();
        for line in manifest.lines() {
            let line = line.trim();
            if line.starts_with('[') {
                section = line.to_string();
                continue;
            }
            let shipped = section == "[dependencies]"
                || (section.starts_with("[target.") && section.ends_with(".dependencies]"));
            if !shipped || line.is_empty() || line.starts_with('#') {
                continue;
            }
            if let Some((name, _)) = line.split_once('=') {
                names.push(name.trim().to_string());
            }
        }
        names
    }

    /// package.json の dependencies (vite が bundle する runtime 依存) の名前
    fn direct_npm_dependencies() -> Vec<String> {
        let package: serde_json::Value =
            serde_json::from_str(include_str!("../../package.json")).expect("package.json parses");
        package["dependencies"]
            .as_object()
            .expect("package.json has dependencies")
            .keys()
            .cloned()
            .collect()
    }

    /// Windows の checkout は autocrlf で CRLF になるので、行末を揃えてから読む
    /// (`"\nimporters:\n"` のような改行込みの検索が外れる)
    fn lf(text: &str) -> String {
        text.replace("\r\n", "\n")
    }

    /// THIRD-PARTY-NOTICES.txt の "Used by:" ブロックに並ぶ (package 名, version)。
    /// 生成物のエントリは区切り線 → `License: ...` → 空行 → "Used by:" の並びなので、
    /// その並びだけをブロックとして読む (ライセンス本文に同じ字面があっても数えない)。
    /// npm 側と Rust 側は "# Rust crates" の見出しで分かれている
    fn packages_in_notices(notices: &str, section: &str) -> Vec<(String, String)> {
        let text = match section {
            "npm" => notices.split("# Rust crates").next(),
            "rust" => notices.split("# Rust crates").nth(1),
            _ => None,
        }
        .expect("the notices file has a Rust crates heading");
        let separator = "=".repeat(80);
        let mut packages = Vec::new();
        let mut in_block = false;
        let mut after_separator = false;
        let mut after_license = false;
        for line in text.lines() {
            if in_block {
                let mut words = line.strip_prefix("  ").unwrap_or("").split(' ');
                match (words.next(), words.next()) {
                    (Some(name), Some(version)) if !name.is_empty() => {
                        packages.push((name.to_string(), version.to_string()));
                    }
                    _ => in_block = false,
                }
                continue;
            }
            in_block = after_license && line == "Used by:";
            after_license = (after_separator && line.starts_with("License: "))
                || (after_license && line.is_empty());
            after_separator = line == separator;
        }
        packages
    }

    /// Cargo.lock の [[package]] ブロック。(name, version, dependencies の行)
    fn locked_rust_packages(lock: &str) -> Vec<(String, String, Vec<String>)> {
        lock.split("[[package]]")
            .skip(1)
            .map(|block| {
                let field = |key: &str| {
                    block
                        .lines()
                        .find_map(|line| line.strip_prefix(key))
                        .map(|rest| rest.trim().trim_matches('"').to_string())
                        .unwrap_or_default()
                };
                let deps = block
                    .lines()
                    .filter_map(|line| line.strip_prefix(" \""))
                    .map(|line| line.trim_end_matches("\",").to_string())
                    .collect();
                (field("name = "), field("version = "), deps)
            })
            .collect()
    }

    /// Cargo.lock が flashcap の直接依存 `name` に選んだ version。同じ crate が 2 つ以上の
    /// version で入っている時は、ルートの dependencies に `name version` の形で書かれる
    fn resolved_rust_version(lock: &[(String, String, Vec<String>)], name: &str) -> String {
        let root = lock
            .iter()
            .find(|(crate_name, _, _)| crate_name == "flashcap")
            .expect("Cargo.lock has the flashcap package");
        let entry = root
            .2
            .iter()
            .find(|dep| *dep == name || dep.starts_with(&format!("{name} ")))
            .unwrap_or_else(|| panic!("{name} is not a dependency of flashcap in Cargo.lock"));
        // 同名同 version で source が違う時は `name version (source)` になるので 2 語目だけ
        match entry.split(' ').nth(1) {
            Some(version) => version.to_string(),
            None => {
                let mut versions = lock
                    .iter()
                    .filter(|(crate_name, _, _)| crate_name == name)
                    .map(|(_, version, _)| version.clone());
                let version = versions.next().expect("the crate is in Cargo.lock");
                assert!(
                    versions.next().is_none(),
                    "{name} has several versions in Cargo.lock"
                );
                version
            }
        }
    }

    /// pnpm-lock.yaml の importers の `.` (このプロジェクト) が `section`
    /// (`dependencies` / `devDependencies`) に選んだ (name, version)。
    /// `name:` → `specifier:` → `version:` の 3 行で並ぶ
    fn resolved_npm_versions(lock: &str, section: &str) -> Vec<(String, String)> {
        let heading = format!("    {section}:");
        let importer = lock
            .split("\nimporters:\n")
            .nth(1)
            .expect("pnpm-lock.yaml has an importers section")
            .split("\npackages:\n")
            .next()
            .expect("importers come before packages");
        let mut resolved = Vec::new();
        let mut name = String::new();
        let mut in_dependencies = false;
        for line in importer.lines() {
            if line.starts_with("    ") && !line.starts_with("     ") {
                in_dependencies = line == heading;
                continue;
            }
            if !in_dependencies {
                continue;
            }
            if let Some(key) = line
                .strip_prefix("      ")
                .filter(|rest| !rest.starts_with(' '))
            {
                name = key.trim_end_matches(':').trim_matches('\'').to_string();
            } else if let Some(version) = line.strip_prefix("        version: ") {
                // peer 依存の括弧は notices の version には無い
                let version = version.split('(').next().unwrap_or(version).trim();
                resolved.push((name.clone(), version.to_string()));
            }
        }
        resolved
    }

    /// pnpm-lock.yaml の packages 節にある (name, version)。`  name@version:` か
    /// `  'name@version':` の行。devDependencies や推移依存 (生成スクリプトの
    /// BUNDLED_RUNTIME) もここで突き合わせる
    fn locked_npm_packages(lock: &str) -> Vec<(String, String)> {
        let packages = lock
            .split("\npackages:\n")
            .nth(1)
            .expect("pnpm-lock.yaml has a packages section")
            .split("\nsnapshots:\n")
            .next()
            .expect("packages come before snapshots");
        packages
            .lines()
            .filter_map(|line| line.strip_prefix("  "))
            .filter(|rest| !rest.starts_with(' '))
            .filter_map(|key| {
                let key = key.trim_end_matches(':').trim_matches('\'');
                // スコープ付きの名前は先頭にも @ があるので、最後の @ で分ける
                let (name, version) = key.rsplit_once('@')?;
                (!name.is_empty()).then(|| (name.to_string(), version.to_string()))
            })
            .collect()
    }

    /// 直接依存が、Cargo.lock が選んだ version で載っているか。名前だけだと、上げた依存の
    /// 旧 version が推移依存として残っている時に通ってしまう
    #[test]
    fn third_party_notices_list_every_direct_rust_dependency() {
        // Arrange
        let deps = direct_rust_dependencies(&lf(include_str!("../Cargo.toml")));
        assert!(deps.contains(&"tauri".to_string()), "parsed deps: {deps:?}");
        assert!(deps.contains(&"xcap".to_string()), "parsed deps: {deps:?}");
        let lock = locked_rust_packages(&lf(include_str!("../Cargo.lock")));
        let listed = packages_in_notices(&lf(THIRD_PARTY_NOTICES), "rust");
        assert!(listed.len() > 100, "parsed notices: {listed:?}");

        // Act
        let missing: Vec<(String, String)> = deps
            .iter()
            .map(|name| (name.clone(), resolved_rust_version(&lock, name)))
            .filter(|entry| !listed.contains(entry))
            .collect();

        // Assert
        assert!(
            missing.is_empty(),
            "not in THIRD-PARTY-NOTICES.txt (run `pnpm notices`): {missing:?}"
        );
    }

    /// 載っている crate の version が Cargo.lock と食い違えば、依存を上げたのに
    /// `pnpm notices` を流していない
    #[test]
    fn third_party_notices_match_cargo_lock_versions() {
        // Arrange
        let lock = locked_rust_packages(&lf(include_str!("../Cargo.lock")));
        let listed = packages_in_notices(&lf(THIRD_PARTY_NOTICES), "rust");
        assert!(listed.len() > 100, "parsed notices: {listed:?}");

        // Act
        let stale: Vec<&(String, String)> = listed
            .iter()
            .filter(|(name, version)| !lock.iter().any(|(n, v, _)| n == name && v == version))
            .collect();

        // Assert
        assert!(
            stale.is_empty(),
            "not in Cargo.lock (run `pnpm notices`): {stale:?}"
        );
    }

    /// 直接依存が pnpm-lock.yaml の解決 version で載っているか、また載っている package が
    /// すべて lock にあるか。notices の npm 側は node_modules の package.json から書くので、
    /// lock を更新して install と再生成を忘れると古い version のまま残る
    #[test]
    fn third_party_notices_match_pnpm_lock() {
        // Arrange
        let deps = direct_npm_dependencies();
        assert!(
            deps.contains(&"@tauri-apps/api".to_string()),
            "parsed deps: {deps:?}"
        );
        let lock = lf(include_str!("../../pnpm-lock.yaml"));
        let resolved = resolved_npm_versions(&lock, "dependencies");
        assert_eq!(
            resolved.len(),
            deps.len(),
            "parsed lock importer: {resolved:?}"
        );
        let locked = locked_npm_packages(&lock);
        let listed = packages_in_notices(&lf(THIRD_PARTY_NOTICES), "npm");
        // 生成スクリプトの BUNDLED_RUNTIME (devDependencies / 推移依存だが bundle に入るもの)。
        // devDependencies は importer の解決 version と比べる。推移依存は lock に 1 つの
        // version しか無いことを前提に、それと比べる (2 つ以上になったらここで落ちるので、
        // どの version が bundle されるかを確かめてテストを直す)
        let dev_resolved = resolved_npm_versions(&lock, "devDependencies");
        let bundled: Vec<(String, String)> = ["svelte", "@sveltejs/kit", "tailwindcss", "esm-env"]
            .iter()
            .map(|name| {
                let version = match dev_resolved.iter().find(|(n, _)| n == name) {
                    Some((_, version)) => version.clone(),
                    None => {
                        let versions: Vec<&String> = locked
                            .iter()
                            .filter(|(n, _)| n == name)
                            .map(|(_, v)| v)
                            .collect();
                        assert_eq!(
                            versions.len(),
                            1,
                            "{name} has several versions in pnpm-lock.yaml: {versions:?}"
                        );
                        versions[0].clone()
                    }
                };
                (name.to_string(), version)
            })
            .collect();

        // Act
        let missing: Vec<&(String, String)> = resolved
            .iter()
            .chain(bundled.iter())
            .filter(|entry| !listed.contains(entry))
            .collect();
        let unknown: Vec<&(String, String)> = listed
            .iter()
            .filter(|entry| !locked.contains(entry))
            .collect();

        // Assert
        assert!(
            missing.is_empty(),
            "not in THIRD-PARTY-NOTICES.txt (run `pnpm notices`): {missing:?}"
        );
        assert!(
            unknown.is_empty(),
            "not in pnpm-lock.yaml (run `pnpm notices`): {unknown:?}"
        );
    }

    /// Windows の checkout (CRLF) でも同じ結果になること
    #[test]
    fn notices_parsers_accept_crlf() {
        // Arrange
        let notices = "x\r\n# Rust crates\r\n\r\n".to_string()
            + &"=".repeat(80)
            + "\r\nLicense: MIT\r\n\r\nUsed by:\r\n  serde 1.0.0 (u)\r\n\r\ntext\r\n";
        let lock = "lockfileVersion: '9.0'\r\nimporters:\r\n  .:\r\n    dependencies:\r\n      '@a/b':\r\n        specifier: ^5\r\n        version: 5.3.0\r\npackages:\r\n  '@a/b@5.3.0':\r\n    resolution: {}\r\n  c@1.0.0:\r\n    resolution: {}\r\nsnapshots:\r\n";

        // Act
        let listed = packages_in_notices(&lf(&notices), "rust");
        let resolved = resolved_npm_versions(&lf(lock), "dependencies");
        let locked = locked_npm_packages(&lf(lock));

        // Assert
        assert_eq!(listed, vec![("serde".to_string(), "1.0.0".to_string())]);
        assert_eq!(resolved, vec![("@a/b".to_string(), "5.3.0".to_string())]);
        assert_eq!(
            locked,
            vec![
                ("@a/b".to_string(), "5.3.0".to_string()),
                ("c".to_string(), "1.0.0".to_string())
            ]
        );
    }
}
