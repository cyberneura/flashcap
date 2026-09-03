# FlashCap

![](./src-tauri/icons/128x128@2x.png)

A macOS screenshot capture & annotation app.

![](./documents/images/flashcap-20260130-104529.png)

## Download

### Homebrew (macOS)

```shell
brew install --cask cyberneura/tap/flashcap
```

### Manual download

Get the latest `flashcap_x.y.z_universal.dmg` from the
[Releases](https://github.com/cyberneura/flashcap/releases) page and drag the app into
`/Applications`.

## First launch

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

- Screenshot capture (interactive area selection)
- Timer capture (configurable delay: 3/5/10 seconds)
- Arrow annotation tool (color, thickness, white stroke, drop shadow)
- Mask tool (mosaic, blur, fill) with resize/move handles
- Crop tool (drag to select, Enter to apply, annotations move with the image)
- OCR text recognition (macOS Vision Framework, Japanese/English)
- Clipboard integration (copy path or image)
- Drag & drop to external apps (e.g. Slack)
- Configurable save location (tmp / macOS default / custom folder)
- Keyboard shortcuts (ESC to quit, Delete to remove selected annotation)
- Preferences window (save location, timer delay)

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

## Tech Stack

- **Frontend**: SvelteKit 2, Svelte 5, TypeScript
- **Backend**: Rust (Tauri 2.x)
- **Build**: Vite, pnpm

## Prerequisites

- Rust (stable)
- Node.js
- pnpm
- macOS

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

Bumps the version, pushes it to `main`, and runs the GitHub Actions release build
(signed + notarized universal dmg) until the Release is published.

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
