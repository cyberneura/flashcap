# FlashCap - Project Guide

macOS screenshot capture & annotation app.

## Commands

- `pnpm install` - Install dependencies
- `pnpm tauri dev` - Start development server
- `pnpm tauri build` - Production build
- `pnpm check` - TypeScript type check
- `pnpm release [patch|minor|major]` - Bump version and run the GitHub Actions release build

## Architecture

- **Frontend** (`src/`): SvelteKit 2 + Svelte 5 (runes syntax), TypeScript
- **Backend** (`src-tauri/`): Rust, Tauri 2.x
- **Pages**: `src/routes/+page.svelte` - Main capture UI
- **Components**:
  - `src/lib/ArrowOverlay.svelte` - Arrow annotation overlay
  - `src/lib/MaskOverlay.svelte` - Mask (mosaic/blur/fill) overlay
- **Types**: `src/lib/types.ts`
- **Preferences**: `src/routes/preferences/+page.svelte`

## Key Details

- Screenshots are saved to `$TMPDIR/flashcap/` (configurable in Preferences).
  **Not `/tmp`** — that is mode 1777 and readable by every account on the Mac, and
  what this app writes is whatever was on screen. `flashcap_temp_dir()` /
  `create_private_dir()` in `src-tauri/src/lib.rs` are the only way to build and
  create these directories; `create_dir_all` alone is umask-dependent (0755).
- ESC key exits the app
- Arrow tool for annotation (with white stroke, drop shadow options)
- Mask tool: mosaic, blur, fill modes with 8-direction resize handles
- Timer capture: `screencapture -i -T <delay>` via async Rust command (delay configurable in Preferences)
- Clipboard copy support (image-png feature enabled)
- Settings stored via `tauri-plugin-store` (`settings.json`)

## Capture Start Handshake (src-tauri/src/lib.rs)

キャプチャー開始は3経路 (`--capture` コールド起動 / single-instance 再起動 /
`flashcap://capture` URL スキーム) あり、すべて `CaptureHandshake` に統一されている。

- **経路は `show()` しない**。`request_capture()` で `do-capture` を emit するだけ。
  ウィンドウ表示はフロント `captureScreen()` が撮影完了後の `show()` + `setFocus()` で行う。
  → 経路側で `show()` を足すと show→hide の点滅が起きるので追加しないこと。
- コールド起動は WebView 未ロードのため `do-capture` を取りこぼす。`CaptureHandshake`
  (`Mutex<{frontend_ready, capture_pending}>`) が frontend-ready を待って emit する。
  フロントは `do-capture` リスナー登録後に `frontend-ready` を一度だけ emit する。
- `captureScreen()` の `finally` ではガードフラグ `isCapturing` を `show()`/`setFocus()` の
  await が**全部終わった後**に false へ戻す。先に戻すと復元中の `do-capture` が新キャプチャーを
  開始してインターリーブする。

## Rust Commands (src-tauri/src/lib.rs)

- `take_screenshot_interactive` - Standard interactive capture (`screencapture -i`)
- `take_screenshot_timer` - Timer capture (`screencapture -i -T <N>`, async to avoid UI freeze)
- `write_image_to_file` - Save annotated image (path restricted to the save directory, plus the
  files the user opened themselves in this session). `load_image_file` records each opened file's
  canonical path in `OpenedImages`, which is what allows Cmd+S to overwrite an image that lives
  outside the save directory. 許可されるのは実体が一致するそのファイルだけで、
  そのフォルダは開放しない。書き込み直前に `encode_for_target_format` が上書き先の拡張子へ
  合わせて詰め直す (JPEG などは image crate、HEIC は sips)。変換先が無い拡張子は
  **書かずにエラー** — PNG のまま書くと拡張子と中身が食い違い、上書きなので原本も戻せない
- Common result loading: `load_screenshot_result()` shared by both capture commands

## Build & Check

- `cargo check` in `src-tauri/` for Rust type check
- `pnpm check` for Svelte/TypeScript check
- Run both before committing
- Production build: `cargo build --release` in `src-tauri/` (run before push)

## Release (.github/workflows/release.yml + scripts/release.sh)

配布は GitHub Release。`pnpm release [patch|minor|major]` で version 採番 → main へ push →
`workflow_dispatch` で Actions を起動 → 署名+公証済み universal dmg が公開される。

- **`workflow_dispatch` のみ**。push では自動ビルドしない (無駄な CI を避ける)。
- **macOS のみビルドする**。screencapture / Vision Framework 依存の macOS 専用アプリなので
  Windows ビルドは作らない。成果物は `--target universal-apple-darwin --bundles dmg` の
  `flashcap_<version>_universal.dmg` (x86_64 + arm64)。
- **draft → publish の 2 ジョブ構成**。tauri-action はビルド前に Release を作るため、
  `releaseDraft: false` だとビルド失敗時に空の Release が公開されてしまう。draft で作り、
  build 成功後に publish ジョブが `gh release edit --draft=false --latest` で公開する。
  失敗時は draft のまま残る。
- **version は毎回インクリメント必須**。公開済みと同じ version で再実行すると tauri-action が
  draft 状態の不一致でエラーになる。`scripts/release.sh` が採番を自動化して bump 忘れを構造的に消す。
- **`tauriScript: pnpm exec tauri` は消さない**。省略すると tauri-action は pnpm プロジェクトに
  対して `pnpm tauri build` を実行し、`package.json` の `tauri` スクリプトが持つインラインの
  `APPLE_SIGNING_IDENTITY=...` が workflow の env を上書きしてしまう (シェルのインライン代入は
  継承 env より強い)。その結果 CI が Secret ではなくローカル用にハードコードした identity で
  署名しようとする。`pnpm exec tauri` はスクリプトを経由しないので Secret が効く。
- **`tauri.conf.json` の `signingIdentity: "-"` は消さない**。tauri-cli は
  `APPLE_SIGNING_IDENTITY` env があればそれを優先する (env > config)。CI は Secret の
  Developer ID で署名、env の無い素のローカルビルドは ad-hoc 署名、という両立のための設定。
  `pnpm tauri` スクリプトは env を渡しているのでローカルも Developer ID で署名される。
- **`uses:` はすべて commit SHA 固定**。Apple の秘密鍵入り証明書を keychain に置くジョブなので、
  可変タグ (`@v0` / `@v4` / `@stable`) だと差し替え1つで証明書を抜かれうる。tauri-action だけ
  固定しても、先行ステップの action が改変されれば同じことなので全部固定する。更新時は行末の
  `# v4` コメントを頼りに、新しい SHA を調べて置き換えること。
- **checkout は `persist-credentials: false`**。write 権限の `GITHUB_TOKEN` を `.git/config` に
  残さない。Release 操作に必要な token は各ステップに env で明示的に渡している。
- **証明書は一時 keychain に import し、`list-keychains` で検索リストにも入れる**。codesign は
  default keychain ではなく検索リストから identity を引く。直後の `find-identity | grep` は
  「identity 0 件でも exit 0」という仕様を潰すためのアサーションで、証明書が引けない状態を
  ビルドの奥ではなくこのステップで落とす。
- **`cancel-in-progress: false` + `queue: max` の両方を書く**。1 dispatch = 1 version なので、
  キャンセルされた run の version は (bump コミットは main に残ったまま) 永久に公開されない。
  走行中を守る `cancel-in-progress: false` だけでは不十分で、既定の `queue: single` は pending を
  1 件しか保持せず、新しい dispatch が既存 pending を置き換える (走行中 1 + dispatch 2 回で
  真ん中の version が消える)。`queue: max` は pending を 100 件まで積む。CI 分数より取りこぼし防止。
- **`dtolnay/rust-toolchain` の SHA は master 履歴から選ぶ**。`@stable` の指す SHA は生成ブランチ
  stable の先端で、それを pin すると stable が進んだ時に commit が GC され、以降の run が Rust
  セットアップ前に落ちる。master 履歴の SHA を pin し、ref から toolchain を推測できなくなる分
  `toolchain: stable` を明示する。
- **`pnpm publish` は使えない** (pnpm 組み込みコマンドで scripts から上書き不可)。
  コマンド名は必ず `release`。
- **`package.json` の version は飾り**だが、見た目の一貫性のため release.sh が
  `tauri.conf.json` と同期させている。tauri-action が読むのは `tauri.conf.json` の方。
- **弱点**: `pnpm release` は main へ直接 push する。ブランチ保護 (PR 必須) を掛けると破綻する。
  掛ける運用にするなら tag 駆動 (CI で version 注入) へ切り替えること。
- 必要な GitHub Secrets (登録済み): `APPLE_CERTIFICATE` / `APPLE_CERTIFICATE_PASSWORD` /
  `APPLE_SIGNING_IDENTITY` / `APPLE_ID` / `APPLE_PASSWORD` / `APPLE_TEAM_ID`。
  `APPLE_PASSWORD` は App 用パスワード (通常の Apple ID パスワードでは公証が通らない)。

## Framework Note

SvelteKit 2 (Svelte 5 runes) and Tauri 2.x are new frameworks. Use **context7 MCP** to look up current API docs before making changes.
