# FlashCap

![](./src-tauri/icons/128x128@2x.png)

A screenshot capture & annotation app for macOS and Windows.

![](./documents/images/flashcap-20260130-104529.png)

## Download

### Homebrew (macOS)

```shell
brew install --cask cyberneura/tap/flashcap
```

### Manual download (macOS)

Get the latest `flashcap_x.y.z_universal.dmg` from the
[Releases](https://github.com/cyberneura/flashcap/releases) page and drag the app into
`/Applications`.

### Windows

Get the latest `flashcap_x.y.z_x64-setup.exe` from the
[Releases](https://github.com/cyberneura/flashcap/releases) page and run it.
The installer is not code-signed, so SmartScreen warns on first run
("Windows protected your PC" → More info → Run anyway).

The Windows version differs from the macOS one:

- A capture takes the whole monitor under the mouse cursor (Windows has no
  equivalent of `screencapture -i`). Use the crop tool to cut out the area you need.
- Screen recording and OCR are macOS-only and are not shown.
- No notifications (e.g. after auto-copy).
- HEIC / HEIF images cannot be opened.
- Shortcuts use Ctrl instead of ⌘ (Ctrl+C / Ctrl+Shift+C / Ctrl+V / Ctrl+S / Ctrl+Z,
  Ctrl+, for Preferences).

## First launch (macOS)

Both methods install the same app from the same dmg. It is signed with a
Developer ID certificate and notarized by Apple, so it passes Gatekeeper — no
"unidentified developer" warning, and no `xattr -dr com.apple.quarantine`
workaround. macOS still shows the usual "downloaded from the Internet, are you
sure you want to open it?" confirmation on first launch. The binary is universal
(Intel + Apple Silicon).

macOS grants the app Screen Recording permission on first capture
(System Settings → Privacy & Security → Screen Recording).

OCR uses `/usr/bin/swift`, which ships with the Xcode Command Line Tools. If OCR
fails, install them with `xcode-select --install`.

## Features

- Screenshot capture (interactive area selection on macOS, the monitor under the cursor on Windows)
- Timer capture (configurable delay: 3/5/10 seconds)
- Arrow annotation tool (color, thickness, white stroke, drop shadow)
- Mask tool (mosaic, blur, fill) with resize/move handles
- Crop tool (drag to select, Enter to apply, annotations move with the image)
- OCR text recognition (macOS only: Vision Framework, Japanese/English)
- Clipboard integration (copy path or image)
- Auto-copy each capture (off / file path / image data) from the toolbar, with a notification
- Optional menu bar icon (Preferences) to capture, record, and copy text on screen without opening the window
- Drag & drop to external apps (e.g. Slack)
- Screen recording (macOS only)
- Configurable save location (tmp / OS default / custom folder)
- Keyboard shortcuts (ESC to quit, Delete to remove selected annotation)
- Preferences window (save location, timer delay, menu bar icon)

## CLI Options

```bash
# Headless OCR: capture region → OCR → copy text → notify → exit
flashcap --capture-screen-text

# Start capture directly on relaunch (instead of blinking the capture button)
flashcap --capture
```

## URL Scheme

```bash
# Headless OCR via URL scheme (for Alfred, Raycast, etc.)
open "flashcap://ocr"

# Start capture via URL scheme
open "flashcap://capture"
```

On Windows only `flashcap://capture` is supported, and it does not take a
screenshot by itself: it brings the window to the front and highlights the capture
button. Windows captures the whole monitor without any selection step, so a link on
a web page must not be able to trigger it. Use `flashcap --capture` from a local
shortcut or launcher to start a capture directly.

## Tech Stack

- **Frontend**: SvelteKit 2, Svelte 5, TypeScript
- **Backend**: Rust (Tauri 2.x)
- **Build**: Vite, pnpm

## Prerequisites

- Rust (stable)
- Node.js
- pnpm
- macOS or Windows

## Development

```bash
pnpm install
pnpm tauri dev
```

## Build

```bash
pnpm tauri build
```

## Type Check

```bash
pnpm check
```

## Release

A push to `main` whose `src-tauri/tauri.conf.json` version has no published GitHub Release yet,
and is newer than the latest one, builds and publishes it (signed + notarized universal dmg,
and an unsigned Windows NSIS installer). `pnpm release` bumps the
version, pushes it to `main`, and watches that build until the Release is published.
Bumping the version in a pull request and merging it releases the same way.

```bash
pnpm release           # 0.1.0 -> 0.1.1 (patch, default)
pnpm release minor     # 0.1.0 -> 0.2.0
pnpm release major     # 0.1.0 -> 1.0.0
```

Requires an authenticated `gh` CLI, and `main` must be clean and in sync with
`origin/main`. See [AGENTS.md](./AGENTS.md) for how the workflow is put together.

## Project Structure

```
src/                          # SvelteKit frontend
  routes/
    +page.svelte              # Main capture UI
    preferences/+page.svelte  # Preferences page
  lib/
    ArrowOverlay.svelte       # Arrow annotation overlay
    MaskOverlay.svelte        # Mask (mosaic/blur/fill) overlay
    CropOverlay.svelte        # Crop selection overlay
    types.ts                  # Shared types
src-tauri/                    # Rust backend (Tauri)
  src/lib.rs                  # Tauri commands & app setup
```

## Note for AI Assistants

SvelteKit 2 (with Svelte 5 runes) and Tauri 2.x are relatively new frameworks. When working on this project, use **context7 MCP** to look up the latest API documentation.
