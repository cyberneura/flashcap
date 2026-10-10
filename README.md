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

- A capture freezes the monitor under the mouse cursor and lets you drag a rectangle
  on it (Windows has no equivalent of `screencapture -i`, so FlashCap draws its own
  selection overlay). Press Enter to take the whole monitor, Esc or right-click to
  cancel. Only the monitor under the cursor can be selected from.
- Screen recording, OCR and shell commands are macOS-only and are not shown.
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

- Screenshot capture with interactive area selection (on Windows, within the monitor under the cursor)
- Timer capture (configurable delay: 3/5/10 seconds)
- Arrow annotation tool (color, thickness, white stroke, drop shadow)
- Mask tool (mosaic, blur, fill) with resize/move handles
- Crop tool (drag to select, Enter to apply, annotations move with the image)
- OCR text recognition (macOS only: Vision Framework, Japanese/English)
- Clipboard integration (copy path or image)
- Auto-copy each capture (off / file path / image data) from the toolbar, with a notification
- Optional menu bar icon (Preferences) to capture, record, and copy text on screen without opening the window
- Drag & drop to external apps (e.g. Slack)
- Saved-images sidebar (toggle at the left end of the toolbar): thumbnails of the save folder,
  newest first. Click one to save the current image and open it; drag one to another app like a
  file from Finder
- Screen recording (macOS only)
- Configurable save location (tmp / OS default / custom folder)
- Keyboard shortcuts (ESC to quit, Delete to remove selected annotation)
- Shell commands (macOS only): register commands in Preferences and run them on the open image
  from the toolbar. The image path is passed as `$IMAGE_PATH` (write `"${IMAGE_PATH}"` with the
  quotes). Output is logged to `$TMPDIR/flashcap/shell-logs/`
- Preferences window (save location, timer delay, menu bar icon, shell commands)

## Configuration

All settings live in `~/.config/flashcap/config.json` (macOS and Windows). Paths under your
home folder are stored as `~/...`, so the file can be shared between machines (a symlink into
a dotfiles repository works).

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
button, so a link on a web page cannot put the capture overlay over your screen
(where a single Enter would take the whole monitor). Use `flashcap --capture` from a local
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
    licenses/+page.svelte     # Third-Party Licenses window
  lib/
    ArrowOverlay.svelte       # Arrow annotation overlay
    MaskOverlay.svelte        # Mask (mosaic/blur/fill) overlay
    CropOverlay.svelte        # Crop selection overlay
    ThumbnailSidebar.svelte   # Saved-images sidebar (thumbnail browser)
    types.ts                  # Shared types
src-tauri/                    # Rust backend (Tauri)
  src/lib.rs                  # Tauri commands & app setup
  src/licenses.rs             # Embeds THIRD-PARTY-NOTICES.txt
  src/thumbnails.rs           # Lists the save folder and makes thumbnails for the sidebar
```

## License

FlashCap is released under the [MIT License](./LICENSE).

## Third-party licenses

`THIRD-PARTY-NOTICES.txt` lists the licenses of the libraries bundled into the app: every
Rust crate compiled into the macOS and Windows binaries, the npm packages named in
`dependencies` in `package.json`, and the parts of the dev toolchain that end up in the
web view bundle (the Svelte / SvelteKit runtime and Tailwind CSS). It is compiled into
the app and shown by **FlashCap > Third-Party Licenses...** in the macOS menu bar, or by
the **Third-Party Licenses** button in Preferences (macOS and Windows). Regenerate it
after adding or updating a dependency; the Rust tests fail if a direct dependency is
missing from it.

```bash
cargo install cargo-about --locked --features cli   # once
pnpm notices
```

## Note for AI Assistants

SvelteKit 2 (with Svelte 5 runes) and Tauri 2.x are relatively new frameworks. When working on this project, use **context7 MCP** to look up the latest API documentation.
