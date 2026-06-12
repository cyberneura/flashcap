# FlashCap - Project Guide

macOS screenshot capture & annotation app.

## Commands

- `pnpm install` - Install dependencies
- `pnpm tauri dev` - Start development server
- `pnpm tauri build` - Production build
- `pnpm check` - TypeScript type check

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

- Screenshots are saved to `/tmp/flashcap/` (configurable in Preferences)
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
- `write_image_to_file` - Save annotated image (path restricted to save directory)
- Common result loading: `load_screenshot_result()` shared by both capture commands

## Build & Check

- `cargo check` in `src-tauri/` for Rust type check
- `pnpm check` for Svelte/TypeScript check
- Run both before committing
- Production build: `cargo build --release` in `src-tauri/` (run before push)

## Framework Note

SvelteKit 2 (Svelte 5 runes) and Tauri 2.x are new frameworks. Use **context7 MCP** to look up current API docs before making changes.
